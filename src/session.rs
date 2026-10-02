//! Versioned, lossless audio snapshots. A revision becomes visible only after
//! every WAV and its manifest have been flushed and the pointer is replaced.
use crate::{
    engine::{
        core::{AudioSnapshot, TRACKS},
        loop_audio::{LoopAudio, OfflinePages},
    },
    project::{self, ProjectData, ProjectEntry},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

pub const FORMAT_VERSION: u32 = 3;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioFile {
    pub file: String,
    pub frames: usize,
    pub sha256: String,
}
#[derive(Serialize, Deserialize)]
pub struct Manifest {
    #[serde(default)]
    pub histories: Vec<HistoryFiles>,
    pub version: u32,
    pub sample_rate: u32,
    pub frame: u64,
    pub config: ProjectData,
    pub tracks: Vec<AudioFile>,
    pub undo: Vec<AudioFile>,
    pub undo_valid: [bool; TRACKS],
    pub undone: [bool; TRACKS],
}
#[derive(Serialize, Deserialize)]
pub struct HistoryFiles {
    pub undo: Vec<AudioFile>,
    pub redo: Vec<AudioFile>,
}

pub fn id() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    format!(
        "{}-{}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}
pub fn safe_child(root: &Path, name: &str) -> Result<PathBuf> {
    ensure!(
        !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':']),
        "Invalid asset name"
    );
    Ok(root.join(name))
}
pub fn project_assets(entry: &ProjectEntry) -> Result<PathBuf> {
    safe_child(
        &crate::app_support::paths::projects_dir(),
        &format!("{}.assets", entry.file),
    )
}
pub fn checksum(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut block = [0u8; 65_536];
    loop {
        let read = file.read(&mut block)?;
        if read == 0 {
            break;
        }
        hash.update(&block[..read]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
pub fn wav_spec(sr: u32) -> hound::WavSpec {
    hound::WavSpec {
        channels: 2,
        sample_rate: sr,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    }
}
pub fn write_audio(path: &Path, sr: u32, audio: &LoopAudio) -> Result<AudioFile> {
    let mut wav = hound::WavWriter::create(path, wav_spec(sr))?;
    for i in 0..audio.len {
        for sample in audio.read(i) {
            wav.write_sample(sample)?;
        }
    }
    wav.finalize()?;
    fs::OpenOptions::new().write(true).open(path)?.sync_all()?;
    Ok(AudioFile {
        file: path
            .file_name()
            .context("Missing filename")?
            .to_string_lossy()
            .into_owned(),
        frames: audio.len,
        sha256: checksum(path)?,
    })
}
fn read_audio(root: &Path, item: &AudioFile, sr: u32) -> Result<LoopAudio> {
    let path = safe_child(root, &item.file)?;
    ensure!(
        checksum(&path)? == item.sha256,
        "Audio checksum mismatch: {}",
        item.file
    );
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    ensure!(spec == wav_spec(sr), "Unsupported snapshot audio format");
    ensure!(
        reader.duration() as usize == item.frames
            && item.frames <= sr as usize * crate::engine::loop_audio::MAX_LOOP_SECONDS,
        "Invalid loop length"
    );
    let mut audio = LoopAudio::new(sr);
    let mut samples = reader.samples::<f32>();
    for i in 0..item.frames {
        let frame = [
            samples.next().context("Truncated WAV")??,
            samples.next().context("Truncated WAV")??,
        ];
        ensure!(
            frame.iter().all(|s| s.is_finite()),
            "Non-finite audio sample"
        );
        ensure!(
            audio.write(i, frame, &mut OfflinePages),
            "Audio capacity exceeded"
        );
    }
    Ok(audio)
}

pub fn write_bundle(root: &Path, snapshot: &AudioSnapshot, config: ProjectData) -> Result<()> {
    fs::create_dir_all(root)?;
    let mut manifest = Manifest {
        histories: Vec::new(),
        version: FORMAT_VERSION,
        sample_rate: snapshot.sample_rate,
        frame: snapshot.at,
        config,
        tracks: Vec::new(),
        undo: Vec::new(),
        undo_valid: snapshot.undo_valid,
        undone: snapshot.undone,
    };
    // Shared pages are immutable while this worker writes. Reuse assets with the
    // same page identity instead of writing the legacy undo and history twice.
    let mut written: Vec<(usize, Vec<usize>, AudioFile)> = Vec::new();
    let mut write = |name: String, audio: &LoopAudio| -> Result<AudioFile> {
        let pages = audio
            .pages
            .iter()
            .map(|p| std::sync::Arc::as_ptr(p) as usize)
            .collect::<Vec<_>>();
        if let Some((_, _, file)) = written
            .iter()
            .find(|(len, old, _)| *len == audio.len && *old == pages)
        {
            return Ok(file.clone());
        }
        let file = write_audio(&root.join(name), snapshot.sample_rate, audio)?;
        written.push((audio.len, pages, file.clone()));
        Ok(file)
    };
    for i in 0..TRACKS {
        manifest
            .tracks
            .push(write(format!("track-{}.wav", i + 1), &snapshot.tracks[i])?);
        manifest
            .undo
            .push(write(format!("undo-{}.wav", i + 1), &snapshot.undo[i])?);
        let history = &snapshot.histories[i];
        let mut files = HistoryFiles {
            undo: Vec::new(),
            redo: Vec::new(),
        };
        for (kind, stack, output) in [
            ("undo", &history.undo, &mut files.undo),
            ("redo", &history.redo, &mut files.redo),
        ] {
            for (step, audio) in stack.slots[..stack.len].iter().enumerate() {
                output.push(write(
                    format!("history-{}-{kind}-{step}.wav", i + 1),
                    audio,
                )?);
            }
        }
        if !snapshot.has_histories && snapshot.undo_valid[i] {
            let stack = if snapshot.undone[i] {
                &mut files.redo
            } else {
                &mut files.undo
            };
            stack.push(manifest.undo[i].clone());
        }
        manifest.histories.push(files);
    }
    project::atomic_write(
        &root.join("manifest.json"),
        &serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(())
}
pub fn read_bundle(root: &Path) -> Result<(AudioSnapshot, ProjectData)> {
    let manifest: Manifest = serde_json::from_slice(&fs::read(root.join("manifest.json"))?)?;
    ensure!(
        manifest.version == 2 || manifest.version == FORMAT_VERSION,
        "Unsupported snapshot version {}",
        manifest.version
    );
    ensure!(
        (8_000..=192_000).contains(&manifest.sample_rate),
        "Invalid sample rate"
    );
    ensure!(
        manifest.tracks.len() == TRACKS && manifest.undo.len() == TRACKS,
        "Snapshot must contain five tracks"
    );
    let mut snapshot = AudioSnapshot::empty(manifest.sample_rate);
    snapshot.at = manifest.frame;
    let mut cache = std::collections::HashMap::<String, (AudioFile, LoopAudio)>::new();
    let mut read = |item: &AudioFile| -> Result<LoopAudio> {
        if let Some((previous, audio)) = cache.get(&item.file) {
            ensure!(previous == item, "Conflicting metadata for shared asset");
            let mut copy = LoopAudio::new(manifest.sample_rate);
            audio.share_into(&mut copy, &mut OfflinePages);
            return Ok(copy);
        }
        let audio = read_audio(root, item, manifest.sample_rate)?;
        let mut copy = LoopAudio::new(manifest.sample_rate);
        audio.share_into(&mut copy, &mut OfflinePages);
        cache.insert(item.file.clone(), (item.clone(), audio));
        Ok(copy)
    };
    for i in 0..TRACKS {
        snapshot.tracks[i] = read(&manifest.tracks[i])?;
        snapshot.undo[i] = read(&manifest.undo[i])?;
    }
    snapshot.undo_valid = manifest.undo_valid;
    snapshot.undone = manifest.undone;
    if manifest.version >= 3 {
        ensure!(
            manifest.histories.len() == TRACKS,
            "Invalid history track count"
        );
        for (history, files) in snapshot.histories.iter_mut().zip(&manifest.histories) {
            for (stack, items) in [
                (&mut history.undo, &files.undo),
                (&mut history.redo, &files.redo),
            ] {
                ensure!(
                    items.len() <= crate::engine::history::HISTORY_DEPTH,
                    "History exceeds supported depth"
                );
                for item in items {
                    let audio = read(item)?;
                    stack.push(&audio, &mut OfflinePages);
                }
            }
        }
    } else {
        for i in 0..TRACKS {
            if snapshot.undo_valid[i] {
                let stack = if snapshot.undone[i] {
                    &mut snapshot.histories[i].redo
                } else {
                    &mut snapshot.histories[i].undo
                };
                stack.push(&snapshot.undo[i], &mut OfflinePages);
            }
        }
    }
    snapshot.has_histories = true;
    Ok((snapshot, manifest.config))
}
pub fn save_snapshot(
    entry: &ProjectEntry,
    snapshot: &AudioSnapshot,
    mut data: ProjectData,
) -> Result<String> {
    let revision = id();
    let root = project_assets(entry)?.join("snapshots");
    fs::create_dir_all(&root)?;
    let temporary = root.join(format!("{revision}.pending"));
    write_bundle(&temporary, snapshot, data.clone())?;
    fs::rename(&temporary, root.join(&revision))?;
    data.snapshot = Some(revision.clone());
    project::save_project_data(entry, &data)?;
    Ok(revision)
}
pub fn load_snapshot(entry: &ProjectEntry, revision: &str) -> Result<AudioSnapshot> {
    let root = safe_child(&project_assets(entry)?.join("snapshots"), revision)?;
    Ok(read_bundle(&root)?.0)
}

/// Offline windowed-sinc conversion. Never reinterpret old frames at a new rate.
pub fn resample(snapshot: &mut AudioSnapshot, target_rate: u32) -> Result<()> {
    if snapshot.sample_rate == target_rate {
        return Ok(());
    }
    let source_rate = snapshot.sample_rate;
    for audio in snapshot
        .tracks
        .iter_mut()
        .chain(snapshot.undo.iter_mut())
        .chain(
            snapshot
                .histories
                .iter_mut()
                .flat_map(|h| h.undo.slots.iter_mut().chain(h.redo.slots.iter_mut())),
        )
    {
        let length = (audio.len as u64 * target_rate as u64).div_ceil(source_rate as u64) as usize;
        let mut converted = LoopAudio::new(target_rate);
        let ratio = source_rate as f64 / target_rate as f64;
        let cutoff = (1.0 / ratio).min(1.0) * 0.94;
        for i in 0..length {
            let position = i as f64 * ratio;
            let center = position.floor() as isize;
            let mut result = [0.0f64; 2];
            let mut sum = 0.0;
            for offset in -32..=32 {
                let x = (center + offset) as f64 - position;
                let z = std::f64::consts::PI * x * cutoff;
                let sinc = if z.abs() < 1e-12 { 1.0 } else { z.sin() / z };
                let window = 0.5 + 0.5 * (std::f64::consts::PI * x / 33.0).cos();
                let weight = sinc * window * cutoff;
                let index = (center + offset).rem_euclid(audio.len.max(1) as isize) as usize;
                let value = audio.read(index);
                sum += weight;
                for ch in 0..2 {
                    result[ch] += value[ch] as f64 * weight;
                }
            }
            ensure!(
                converted.write(
                    i,
                    [(result[0] / sum) as f32, (result[1] / sum) as f32],
                    &mut OfflinePages
                ),
                "Converted loop is too long"
            );
        }
        *audio = converted;
    }
    snapshot.sample_rate = target_rate;
    Ok(())
}

pub fn resample_frames(frames: &[[f32; 2]], source: u32, target: u32) -> Vec<[f32; 2]> {
    if source == target {
        return frames.to_vec();
    }
    let length = (frames.len() as u64 * target as u64).div_ceil(source as u64) as usize;
    let ratio = source as f64 / target as f64;
    let cutoff = (1.0 / ratio).min(1.0) * 0.94;
    (0..length)
        .map(|i| {
            let position = i as f64 * ratio;
            let center = position.floor() as isize;
            let mut out = [0.0f64; 2];
            let mut sum = 0.0;
            for offset in -32..=32 {
                let x = (center + offset) as f64 - position;
                let z = std::f64::consts::PI * x * cutoff;
                let weight = if z.abs() < 1e-12 { 1.0 } else { z.sin() / z }
                    * (0.5 + 0.5 * (std::f64::consts::PI * x / 33.0).cos())
                    * cutoff;
                if let Some(frame) = usize::try_from(center + offset)
                    .ok()
                    .and_then(|n| frames.get(n))
                {
                    for ch in 0..2 {
                        out[ch] += frame[ch] as f64 * weight;
                    }
                }
                sum += weight;
            }
            [(out[0] / sum) as f32, (out[1] / sum) as f32]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_roundtrip_shared_assets_and_v2_migration() {
        let root = std::path::Path::new("var").join(format!("history-snapshot-{}", id()));
        let mut snapshot = AudioSnapshot::empty(8000);
        snapshot.has_histories = true;
        for value in 1..=4 {
            snapshot.histories[0].checkpoint(&snapshot.tracks[0], &mut OfflinePages);
            snapshot.tracks[0].write(0, [value as f32 * 0.1; 2], &mut OfflinePages);
        }
        snapshot.histories[0].undo(&mut snapshot.tracks[0], &mut OfflinePages);
        snapshot.tracks[0].share_into(&mut snapshot.undo[0], &mut OfflinePages);
        snapshot.undo_valid[0] = true;
        write_bundle(
            &root,
            &snapshot,
            project::data_from_config(&crate::config::AppConfig::new(120, 0, 5)),
        )
        .unwrap();
        let mut manifest: Manifest =
            serde_json::from_slice(&fs::read(root.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(
            manifest.tracks[0].file, manifest.undo[0].file,
            "shared audio should be written once"
        );
        let (mut loaded, _) = read_bundle(&root).unwrap();
        assert_eq!(loaded.histories[0].undo.len, 3);
        assert_eq!(loaded.histories[0].redo.len, 1);
        assert!(std::sync::Arc::ptr_eq(
            &loaded.tracks[0].pages[0],
            &loaded.undo[0].pages[0]
        ));
        for value in (0..=2).rev() {
            loaded.histories[0].undo(&mut loaded.tracks[0], &mut OfflinePages);
            assert_eq!(loaded.tracks[0].read(0), [value as f32 * 0.1; 2]);
        }
        for value in 1..=4 {
            loaded.histories[0].redo(&mut loaded.tracks[0], &mut OfflinePages);
            assert_eq!(loaded.tracks[0].read(0), [value as f32 * 0.1; 2]);
        }
        manifest.version = 2;
        manifest.histories.clear();
        manifest.undone[0] = true;
        fs::write(
            root.join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        let (old, _) = read_bundle(&root).unwrap();
        assert_eq!(old.histories[0].undo.len, 0);
        assert_eq!(old.histories[0].redo.len, 1);
        assert_eq!(
            old.histories[0].redo.slots[0].read(0),
            snapshot.undo[0].read(0)
        );
    }
    #[test]
    fn snapshot_preserves_float_bits_undo_and_detects_corruption() {
        let root = std::env::temp_dir().join(format!("rc505-snapshot-test-{}", id()));
        let mut snapshot = AudioSnapshot::empty(8000);
        for i in 0..2051 {
            snapshot.tracks[2].write(
                i,
                [(i as f32 * 0.19).sin(), f32::from_bits(0x3a887766)],
                &mut OfflinePages,
            );
        }
        snapshot.tracks[2].share_into(&mut snapshot.undo[2], &mut OfflinePages);
        snapshot.undo_valid[2] = true;
        snapshot.tracks[2].write(1024, [0.3, 0.2], &mut OfflinePages);
        write_bundle(
            &root,
            &snapshot,
            project::data_from_config(&crate::config::AppConfig::new(117, 85, 5)),
        )
        .unwrap();
        let (loaded, data) = read_bundle(&root).unwrap();
        assert_eq!(data.beat.bpm, 117);
        assert!(loaded.undo_valid[2]);
        for i in 0..2051 {
            assert_eq!(
                loaded.tracks[2].read(i).map(f32::to_bits),
                snapshot.tracks[2].read(i).map(f32::to_bits)
            );
        }
        assert_ne!(loaded.tracks[2].read(1024), loaded.undo[2].read(1024));
        fs::write(root.join("track-3.wav"), b"corruption").unwrap();
        assert!(read_bundle(&root).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn rate_conversion_preserves_duration_and_rejects_path_traversal() {
        let frames: Vec<_> = (0..4410)
            .map(|i| [(i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5; 2])
            .collect();
        let converted = resample_frames(&frames, 44100, 48000);
        assert_eq!(converted.len(), 4800);
        let max_error = (64..4736)
            .map(|i| {
                (converted[i][0] - (i as f32 / 48000.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5)
                    .abs()
            })
            .fold(0.0f32, f32::max);
        assert!(max_error < 0.001, "resampling error {max_error}");
        for name in ["../outside", "..", "C:\\outside", "a/b", ""] {
            assert!(safe_child(Path::new("root"), name).is_err());
        }
    }
}
