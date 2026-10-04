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

pub const FORMAT_VERSION: u32 = 4;
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
    /// Version 4 uses JSON null for empty audio. Legacy objects deserialize as
    /// Some and keep their existing filename/checksum validation.
    pub tracks: Vec<Option<AudioFile>>,
    pub undo: Vec<Option<AudioFile>>,
    pub undo_valid: [bool; TRACKS],
    pub undone: [bool; TRACKS],
}
#[derive(Serialize, Deserialize)]
pub struct HistoryFiles {
    // A null entry is an actual history step back to an empty track, not an
    // absent step. Keep stack lengths/order even when no WAV is needed.
    pub undo: Vec<Option<AudioFile>>,
    pub redo: Vec<Option<AudioFile>>,
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
    let mut write = |name: String, audio: &LoopAudio| -> Result<Option<AudioFile>> {
        if audio.len == 0 {
            return Ok(None);
        }
        let pages = audio
            .pages
            .iter()
            .map(|p| std::sync::Arc::as_ptr(p) as usize)
            .collect::<Vec<_>>();
        if let Some((_, _, file)) = written
            .iter()
            .find(|(len, old, _)| *len == audio.len && *old == pages)
        {
            return Ok(Some(file.clone()));
        }
        let file = write_audio(&root.join(name), snapshot.sample_rate, audio)?;
        written.push((audio.len, pages, file.clone()));
        Ok(Some(file))
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
        (2..=FORMAT_VERSION).contains(&manifest.version),
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
    let mut read = |item: &Option<AudioFile>| -> Result<LoopAudio> {
        let Some(item) = item else {
            ensure!(
                manifest.version >= 4,
                "Empty references require snapshot version 4"
            );
            return Ok(LoopAudio::new(manifest.sample_rate));
        };
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
    data = project::persistable_data(&data)?;
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
    fn fixture_root(name: &str) -> PathBuf {
        Path::new("var").join(format!("{name}-{}", id()))
    }
    fn cleanup(root: &Path) {
        let root = root.canonicalize().unwrap();
        let workspace = Path::new("var").canonicalize().unwrap();
        assert!(root.starts_with(&workspace) && root != workspace);
        fs::remove_dir_all(root).unwrap();
    }
    fn wav_count(root: &Path) -> usize {
        fs::read_dir(root)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "wav"))
            .count()
    }
    #[test]
    fn empty_snapshot_and_replay_initial_use_null_references_without_wavs() {
        let root = fixture_root("empty-audio-bundle");
        let mut snapshot = AudioSnapshot::empty(8000);
        snapshot.has_histories = true;
        // Empty is a meaningful history step (undo to a blank track).
        snapshot.histories[0]
            .undo
            .push(&snapshot.tracks[0], &mut OfflinePages);
        snapshot.histories[0]
            .redo
            .push(&snapshot.tracks[0], &mut OfflinePages);
        let config = project::data_from_config(&crate::config::AppConfig::new(123, 0, 5));
        write_bundle(&root.join("snapshot"), &snapshot, config.clone()).unwrap();
        assert_eq!(wav_count(&root.join("snapshot")), 0);
        let data = fs::read(root.join("snapshot/manifest.json")).unwrap();
        let manifest: Manifest = serde_json::from_slice(&data).unwrap();
        assert_eq!(manifest.version, 4);
        assert!(
            manifest
                .tracks
                .iter()
                .chain(&manifest.undo)
                .all(Option::is_none)
        );
        assert!(manifest.histories[0].undo[0].is_none());
        assert!(manifest.histories[0].redo[0].is_none());
        let (loaded, config) = read_bundle(&root.join("snapshot")).unwrap();
        assert_eq!(
            (loaded.histories[0].undo.len, loaded.histories[0].redo.len),
            (1, 1)
        );
        assert!(
            loaded
                .tracks
                .iter()
                .chain(&loaded.undo)
                .all(|audio| audio.len == 0)
        );
        let replay = root.join("replay");
        let mut writer =
            crate::replay::Writer::begin(replay.clone(), "empty.json".into(), 900, loaded, config)
                .unwrap();
        // A real recording input belongs in input.wav; only empty initial-state
        // assets are omitted, not replay source PCM or its sample clock.
        writer.audio(900, &[[0.125, -0.25]]).unwrap();
        writer.finish(901).unwrap();
        assert_eq!(wav_count(&replay.join("initial")), 0);
        assert_eq!(wav_count(&replay), 1);
        assert_eq!(
            hound::WavReader::open(replay.join("input.wav"))
                .unwrap()
                .duration(),
            1
        );
        let (loaded, _) = read_bundle(&replay.join("initial")).unwrap();
        assert_eq!(
            (loaded.histories[0].undo.len, loaded.histories[0].redo.len),
            (1, 1)
        );
        let mut old: serde_json::Value = serde_json::from_slice(&data).unwrap();
        old["version"] = serde_json::json!(3);
        fs::write(
            root.join("snapshot/manifest.json"),
            serde_json::to_vec(&old).unwrap(),
        )
        .unwrap();
        assert!(
            read_bundle(&root.join("snapshot")).is_err(),
            "Null is not a legacy-format file reference"
        );
        cleanup(&root);
    }
    #[test]
    fn mixed_nonempty_and_empty_history_preserves_audio_sharing_and_silent_duration() {
        let root = fixture_root("mixed-audio-bundle");
        let mut snapshot = AudioSnapshot::empty(8000);
        snapshot.has_histories = true;
        snapshot.tracks[0] = LoopAudio::from_frames(8000, &[[0.125, -0.5], [1.25, -2.0]]).unwrap();
        snapshot.tracks[0].share_into(&mut snapshot.undo[0], &mut OfflinePages);
        snapshot.undo_valid[0] = true;
        snapshot.histories[0]
            .undo
            .push(&snapshot.tracks[1], &mut OfflinePages);
        snapshot.histories[0]
            .undo
            .push(&snapshot.tracks[0], &mut OfflinePages);
        let redo = LoopAudio::from_frames(8000, &[[0.25, 0.75]; 3]).unwrap();
        snapshot.histories[0].redo.push(&redo, &mut OfflinePages);
        // A nonempty silent loop still needs its duration and samples preserved.
        snapshot.tracks[4] = LoopAudio::from_frames(8000, &[[0.0; 2]; 17]).unwrap();
        write_bundle(
            &root,
            &snapshot,
            project::data_from_config(&crate::config::AppConfig::new(120, 0, 5)),
        )
        .unwrap();
        assert_eq!(wav_count(&root), 3);
        let manifest: Manifest =
            serde_json::from_slice(&fs::read(root.join("manifest.json")).unwrap()).unwrap();
        assert!(manifest.histories[0].undo[0].is_none());
        assert_eq!(
            manifest.tracks[0].as_ref().unwrap().file,
            manifest.undo[0].as_ref().unwrap().file
        );
        assert_eq!(
            manifest.tracks[0].as_ref().unwrap().file,
            manifest.histories[0].undo[1].as_ref().unwrap().file
        );
        let (mut loaded, _) = read_bundle(&root).unwrap();
        assert!(std::sync::Arc::ptr_eq(
            &loaded.tracks[0].pages[0],
            &loaded.histories[0].undo.slots[1].pages[0]
        ));
        assert_eq!(loaded.tracks[4].len, 17);
        assert!((0..17).all(|frame| loaded.tracks[4].read(frame) == [0.0; 2]));
        assert_eq!(
            loaded.tracks[0].read(1).map(f32::to_bits),
            [1.25f32, -2.0].map(f32::to_bits)
        );
        loaded.histories[0].undo(&mut loaded.tracks[0], &mut OfflinePages);
        loaded.histories[0].undo(&mut loaded.tracks[0], &mut OfflinePages);
        assert_eq!(loaded.tracks[0].len, 0);
        loaded.histories[0].redo(&mut loaded.tracks[0], &mut OfflinePages);
        assert_eq!(loaded.tracks[0].len, 2);
        assert_eq!(loaded.tracks[0].read(1), [1.25, -2.0]);
        let replay = root.join("replay");
        let mut writer = crate::replay::Writer::begin(
            replay.clone(),
            "mixed.json".into(),
            400,
            snapshot,
            project::data_from_config(&crate::config::AppConfig::new(120, 0, 5)),
        )
        .unwrap();
        writer.audio(400, &[[0.0; 2]]).unwrap();
        writer.finish(401).unwrap();
        assert_eq!(wav_count(&replay.join("initial")), 3);
        let (initial, _) = read_bundle(&replay.join("initial")).unwrap();
        assert_eq!(
            (initial.histories[0].undo.len, initial.histories[0].redo.len),
            (2, 1)
        );
        assert!(initial.undo_valid[0]);
        assert_eq!(initial.undo[0].read(1), [1.25, -2.0]);
        assert_eq!(initial.histories[0].redo.slots[0].read(2), [0.25, 0.75]);
        cleanup(&root);
    }
    #[test]
    fn valid_empty_legacy_undo_remains_a_real_step_without_an_empty_asset() {
        let root = fixture_root("empty-legacy-undo");
        let mut snapshot = AudioSnapshot::empty(8000);
        snapshot.tracks[0] = LoopAudio::from_frames(8000, &[[0.1, 0.2]]).unwrap();
        snapshot.undo_valid[0] = true;
        assert!(!snapshot.has_histories);
        write_bundle(
            &root,
            &snapshot,
            project::data_from_config(&crate::config::AppConfig::new(120, 0, 5)),
        )
        .unwrap();
        assert_eq!(wav_count(&root), 1);
        let (mut loaded, _) = read_bundle(&root).unwrap();
        assert_eq!(loaded.histories[0].undo.len, 1);
        loaded.histories[0].undo(&mut loaded.tracks[0], &mut OfflinePages);
        assert_eq!(loaded.tracks[0].len, 0);
        loaded.histories[0].redo(&mut loaded.tracks[0], &mut OfflinePages);
        assert_eq!(loaded.tracks[0].read(0), [0.1, 0.2]);
        cleanup(&root);
    }
    #[test]
    fn history_roundtrip_shared_assets_and_v2_v3_migration() {
        let root = fixture_root("history-snapshot");
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
            manifest.tracks[0].as_ref().unwrap().file,
            manifest.undo[0].as_ref().unwrap().file,
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
        // Reproduce actual v3/v2 bundles: even empty entries referred to a WAV.
        let empty =
            write_audio(&root.join("legacy-empty.wav"), 8000, &LoopAudio::new(8000)).unwrap();
        for item in manifest.tracks.iter_mut().chain(&mut manifest.undo).chain(
            manifest
                .histories
                .iter_mut()
                .flat_map(|history| history.undo.iter_mut().chain(&mut history.redo)),
        ) {
            if item.is_none() {
                *item = Some(empty.clone());
            }
        }
        manifest.version = 3;
        fs::write(
            root.join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        let (old_v3, _) = read_bundle(&root).unwrap();
        assert_eq!(
            (old_v3.histories[0].undo.len, old_v3.histories[0].redo.len),
            (3, 1)
        );
        assert_eq!(old_v3.histories[0].undo.slots[0].len, 0);
        assert_eq!(old_v3.histories[0].redo.slots[0].read(0), [0.4; 2]);
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
        cleanup(&root);
    }
    #[test]
    fn snapshot_preserves_float_bits_undo_and_detects_corruption() {
        let root = fixture_root("rc505-snapshot-test");
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
        cleanup(&root);
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
