//! Replay stores dry input and ordered, sample-stamped commands, never a master
//! recording. Live playback and WAV export execute the same RenderCore.
use crate::{
    config::AppConfig,
    engine::{
        core::{Action, AudioSnapshot, Parameters, RenderCore},
        loop_audio::OfflinePages,
    },
    project::ProjectData,
    session,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{BufRead, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};

pub const RENDERER_VERSION: u32 = 3;
pub mod visuals;
pub use visuals::{ConfigPoint, ReplayVisuals, VisualFrame};
pub const MAX_TAKE_SECONDS: u64 = 1800;
#[derive(Clone, Serialize, Deserialize)]
pub enum EventKind {
    Action(Action),
    Config(ProjectData),
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Event {
    pub frame: u64,
    pub sequence: u64,
    pub kind: EventKind,
}
#[derive(Serialize, Deserialize)]
pub struct ReplayInfo {
    pub version: u32,
    pub renderer: u32,
    pub project_id: String,
    pub name: String,
    pub sample_rate: u32,
    pub frames: u64,
    pub input_sha256: String,
    pub events_sha256: String,
    pub initial_sha256: String,
}

pub struct Writer {
    pub root: PathBuf,
    pub origin: u64,
    info: ReplayInfo,
    input: hound::WavWriter<BufWriter<fs::File>>,
    events: BufWriter<fs::File>,
    next_sequence: u64,
    initial: Option<std::thread::JoinHandle<Result<()>>>,
}
impl Writer {
    pub fn begin(
        root: PathBuf,
        project_id: String,
        origin: u64,
        snapshot: AudioSnapshot,
        config: ProjectData,
    ) -> Result<Self> {
        fs::create_dir_all(&root)?;
        let input = hound::WavWriter::create(
            root.join("input.wav"),
            session::wav_spec(snapshot.sample_rate),
        )?;
        let events = BufWriter::new(fs::File::create(root.join("events.jsonl"))?);
        let initial_root = root.join("initial");
        let sr = snapshot.sample_rate;
        let initial = std::thread::Builder::new()
            .name("replay-initial-state".into())
            .spawn(move || session::write_bundle(&initial_root, &snapshot, config))?;
        Ok(Self {
            root,
            origin,
            info: ReplayInfo {
                version: 1,
                renderer: RENDERER_VERSION,
                project_id,
                name: "Unsaved take".into(),
                sample_rate: sr,
                frames: 0,
                input_sha256: String::new(),
                events_sha256: String::new(),
                initial_sha256: String::new(),
            },
            input,
            events,
            next_sequence: 0,
            initial: Some(initial),
        })
    }
    pub fn audio(&mut self, at: u64, frames: &[[f32; 2]]) -> Result<()> {
        ensure!(
            at == self.origin + self.info.frames,
            "Replay input has a gap; this take cannot be exported accurately"
        );
        ensure!(
            self.info.frames + frames.len() as u64
                <= self.info.sample_rate as u64 * MAX_TAKE_SECONDS,
            "Take reached 30 minute limit"
        );
        for frame in frames {
            self.input.write_sample(frame[0])?;
            self.input.write_sample(frame[1])?;
        }
        self.info.frames += frames.len() as u64;
        Ok(())
    }
    pub fn event(&mut self, frame: u64, kind: EventKind) -> Result<()> {
        ensure!(frame >= self.origin, "Invalid replay timestamp");
        serde_json::to_writer(
            &mut self.events,
            &Event {
                frame: frame - self.origin,
                sequence: self.next_sequence,
                kind,
            },
        )?;
        self.events.write_all(b"\n")?;
        self.next_sequence += 1;
        Ok(())
    }
    pub fn finish(mut self, end: u64) -> Result<PathBuf> {
        ensure!(
            end == self.origin + self.info.frames,
            "Replay is incomplete"
        );
        self.input.finalize()?;
        self.events.flush()?;
        self.events.get_ref().sync_all()?;
        self.initial
            .take()
            .unwrap()
            .join()
            .map_err(|_| anyhow::anyhow!("Initial snapshot writer failed"))??;
        fs::OpenOptions::new()
            .write(true)
            .open(self.root.join("input.wav"))?
            .sync_all()?;
        self.info.input_sha256 = session::checksum(&self.root.join("input.wav"))?;
        self.info.events_sha256 = session::checksum(&self.root.join("events.jsonl"))?;
        self.info.initial_sha256 = session::checksum(&self.root.join("initial/manifest.json"))?;
        crate::project::atomic_write(
            &self.root.join("replay.json"),
            &serde_json::to_vec_pretty(&self.info)?,
        )?;
        Ok(self.root)
    }
}

pub fn info(root: &Path) -> Result<ReplayInfo> {
    let value: ReplayInfo = serde_json::from_slice(&fs::read(root.join("replay.json"))?)?;
    ensure!(
        value.version == 1 && (value.renderer == 2 || value.renderer == RENDERER_VERSION),
        "Replay requires renderer version {}",
        value.renderer
    );
    ensure!(
        (8_000..=192_000).contains(&value.sample_rate)
            && value.frames <= value.sample_rate as u64 * MAX_TAKE_SECONDS,
        "Invalid replay duration or sample rate"
    );
    Ok(value)
}
pub fn save_as(root: &Path, name: &str) -> Result<PathBuf> {
    ensure!(!name.trim().is_empty(), "Enter a replay name");
    let mut metadata = info(root)?;
    metadata.name = name.trim().chars().take(100).collect();
    let parent = root.parent().context("Missing replay directory")?;
    let destination = parent.join(format!("take-{}", session::id()));
    crate::project::atomic_write(
        &root.join("replay.json"),
        &serde_json::to_vec_pretty(&metadata)?,
    )?;
    fs::rename(root, &destination)?;
    Ok(destination)
}
pub fn list(entry: &crate::project::ProjectEntry) -> Vec<(PathBuf, String)> {
    let Ok(root) = session::project_assets(entry) else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(root.join("replays")) else {
        return Vec::new();
    };
    let mut result: Vec<_> = entries
        .flatten()
        .filter_map(|e| info(&e.path()).ok().map(|v| (e.path(), v.name)))
        .collect();
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}

/// Rename a managed replay into a recoverable trash directory. No recursive
/// deletion, arbitrary external-folder move or project-identity substitution.
pub fn trash(entry: &crate::project::ProjectEntry, source: &Path) -> Result<()> {
    let assets = session::project_assets(entry)?;
    trash_in(&assets, source, &entry.file)
}
fn trash_in(assets: &Path, source: &Path, project_id: &str) -> Result<()> {
    let assets = fs::canonicalize(assets)?;
    let parent = fs::canonicalize(assets.join("replays"))?;
    ensure!(
        parent.starts_with(&assets),
        "Replay directory escaped project assets"
    );
    ensure!(
        !fs::symlink_metadata(source)?.file_type().is_symlink(),
        "Linked replay folders cannot be deleted"
    );
    let source = fs::canonicalize(source)?;
    ensure!(
        source.parent() == Some(parent.as_path()),
        "Only this project's replay library can be deleted"
    );
    ensure!(
        info(&source)?.project_id == project_id,
        "Replay belongs to another project"
    );
    let trash = assets.join("replay-trash");
    fs::create_dir_all(&trash)?;
    let trash = fs::canonicalize(trash)?;
    ensure!(
        trash.starts_with(&assets),
        "Trash directory escaped project assets"
    );
    fs::rename(source, trash.join(session::id()))?;
    Ok(())
}
pub fn restore_last(entry: &crate::project::ProjectEntry) -> Result<bool> {
    restore_in(&session::project_assets(entry)?, &entry.file)
}
fn restore_in(assets: &Path, project_id: &str) -> Result<bool> {
    let assets = fs::canonicalize(assets)?;
    let trash = assets.join("replay-trash");
    if !trash.exists() {
        return Ok(false);
    }
    let trash = fs::canonicalize(trash)?;
    ensure!(
        trash.starts_with(&assets),
        "Trash directory escaped project assets"
    );
    let mut items = fs::read_dir(trash)?
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_type().is_ok_and(|t| t.is_dir() && !t.is_symlink())
                && info(&e.path()).is_ok_and(|m| m.project_id == project_id)
        })
        .collect::<Vec<_>>();
    items.sort_by_key(|e| e.file_name());
    let Some(item) = items.pop() else {
        return Ok(false);
    };
    let parent = assets.join("replays");
    fs::create_dir_all(&parent)?;
    let parent = fs::canonicalize(parent)?;
    ensure!(
        parent.starts_with(&assets),
        "Replay directory escaped project assets"
    );
    fs::rename(
        item.path(),
        parent.join(format!("take-restored-{}", session::id())),
    )?;
    Ok(true)
}

pub struct RenderResult {
    pub visuals: std::sync::Arc<ReplayVisuals>,
    pub project_id: String,
    pub name: String,
    pub snapshot: AudioSnapshot,
    pub config: ProjectData,
    pub wav: PathBuf,
}

/// Runs on a background thread. Input checksums are validated before rendering.
/// WAV is published only after successful completion; the source remains immutable.
pub fn render(
    root: &Path,
    destination: &Path,
    progress: &std::sync::atomic::AtomicU64,
) -> Result<RenderResult> {
    let metadata = info(root)?;
    ensure!(
        session::checksum(&root.join("input.wav"))? == metadata.input_sha256,
        "Replay input checksum mismatch"
    );
    ensure!(
        session::checksum(&root.join("events.jsonl"))? == metadata.events_sha256,
        "Replay event checksum mismatch"
    );
    ensure!(
        session::checksum(&root.join("initial/manifest.json"))? == metadata.initial_sha256,
        "Replay initial state checksum mismatch"
    );
    let (mut initial, mut data) = session::read_bundle(&root.join("initial"))?;
    ensure!(
        initial.sample_rate == metadata.sample_rate,
        "Replay sample rate mismatch"
    );
    let mut config = AppConfig::new(120, 0, 5);
    crate::project::apply_data_to_config(&mut config, data.clone());
    let mut core = RenderCore::new(metadata.sample_rate);
    core.legacy_renderer(metadata.renderer == 2);
    core.configure(&mut Parameters::from_config(&config, metadata.sample_rate));
    core.restore(&mut initial);
    let mut visuals = ReplayVisuals {
        name: metadata.name.clone(),
        sample_rate: metadata.sample_rate,
        frames: metadata.frames,
        views: Vec::with_capacity(
            (metadata.frames / (metadata.sample_rate as u64 / 30).max(1) + 2) as usize,
        ),
        initial: data.clone(),
        configs: Vec::new(),
    };
    let mut last_action = None;
    let mut input = hound::WavReader::open(root.join("input.wav"))?;
    ensure!(
        input.spec() == session::wav_spec(metadata.sample_rate)
            && input.duration() as u64 == metadata.frames,
        "Replay WAV length/format mismatch"
    );
    let mut samples = input.samples::<f32>();
    let mut lines = BufReader::new(fs::File::open(root.join("events.jsonl"))?).lines();
    let mut expected = 0;
    let mut next = next_event(&mut lines, expected)?;
    let temporary = destination.with_extension(format!("{}.pending.wav", session::id()));
    let mut output = hound::WavWriter::create(&temporary, session::wav_spec(metadata.sample_rate))?;
    for frame in 0..metadata.frames {
        while next.as_ref().is_some_and(|event| event.frame == frame) {
            let event = next.take().unwrap();
            match event.kind {
                EventKind::Action(action) => {
                    core.action(action, &mut OfflinePages);
                    last_action = Some((frame, action));
                }
                EventKind::Config(value) => {
                    visuals.configs.push(ConfigPoint::new(frame, &data, &value));
                    data = value;
                    crate::project::apply_data_to_config(&mut config, data.clone());
                    core.configure(&mut Parameters::from_config(&config, metadata.sample_rate));
                }
            }
            expected += 1;
            next = next_event(&mut lines, expected)?;
            ensure!(
                next.as_ref().is_none_or(|e| e.frame >= frame),
                "Replay commands are out of order"
            );
        }
        let dry = [
            samples.next().context("Truncated input")??,
            samples.next().context("Truncated input")??,
        ];
        ensure!(dry.iter().all(|s| s.is_finite()), "Invalid replay sample");
        let wet = core.process(dry, &mut OfflinePages);
        if frame % (metadata.sample_rate as u64 / 30).max(1) == 0 {
            visuals.views.push(VisualFrame {
                frame,
                view: core.view(),
                last_action,
            });
        }
        output.write_sample(wet[0])?;
        output.write_sample(wet[1])?;
        if frame % 4096 == 0 {
            progress.store(frame, std::sync::atomic::Ordering::Relaxed);
        }
    }
    // Events at the exclusive end have no audible frame, but belong to the
    // imported final state (e.g. Stop and End take in the same callback).
    while let Some(event) = next.take() {
        ensure!(
            event.frame == metadata.frames,
            "Replay command lies beyond its audio"
        );
        match event.kind {
            EventKind::Action(action) => {
                core.action(action, &mut OfflinePages);
                last_action = Some((metadata.frames, action));
            }
            EventKind::Config(value) => {
                visuals
                    .configs
                    .push(ConfigPoint::new(metadata.frames, &data, &value));
                data = value;
                crate::project::apply_data_to_config(&mut config, data.clone());
                core.configure(&mut Parameters::from_config(&config, metadata.sample_rate));
            }
        }
        expected += 1;
        next = next_event(&mut lines, expected)?;
    }
    output.finalize()?;
    fs::OpenOptions::new()
        .write(true)
        .open(&temporary)?
        .sync_all()?;
    ensure!(
        !destination.exists(),
        "Export already exists; choose a new name"
    );
    fs::rename(&temporary, destination)?;
    let mut snapshot = AudioSnapshot::empty(metadata.sample_rate);
    core.snapshot(&mut snapshot, &mut OfflinePages);
    visuals.views.push(VisualFrame {
        frame: metadata.frames,
        view: core.view(),
        last_action,
    });
    progress.store(metadata.frames, std::sync::atomic::Ordering::Relaxed);
    Ok(RenderResult {
        visuals: std::sync::Arc::new(visuals),
        project_id: metadata.project_id,
        name: metadata.name,
        snapshot,
        config: data,
        wav: destination.to_owned(),
    })
}
fn next_event(
    lines: &mut impl Iterator<Item = std::io::Result<String>>,
    sequence: u64,
) -> Result<Option<Event>> {
    let Some(line) = lines.next() else {
        return Ok(None);
    };
    let line = line?;
    ensure!(line.len() <= 2_000_000, "Replay event is too large");
    let event: Event = serde_json::from_str(&line)?;
    ensure!(
        event.sequence == sequence,
        "Replay command sequence is incomplete"
    );
    Ok(Some(event))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{FxKind, TrackFxKind, track_options::Quantize};
    #[test]
    fn replay_delete_restore_validates_ownership_and_preserves_exports() {
        let assets = Path::new("var").join(format!("replay-trash-test-{}", session::id()));
        let source = assets.join("replays/take-1");
        fs::create_dir_all(&source).unwrap();
        let info = ReplayInfo {
            version: 1,
            renderer: RENDERER_VERSION,
            project_id: "example.json".into(),
            name: "Bass".into(),
            sample_rate: 48000,
            frames: 0,
            input_sha256: String::new(),
            events_sha256: String::new(),
            initial_sha256: String::new(),
        };
        fs::write(
            source.join("replay.json"),
            serde_json::to_vec(&info).unwrap(),
        )
        .unwrap();
        fs::write(source.join("input.wav"), b"original input").unwrap();
        fs::create_dir_all(assets.join("replays/exports")).unwrap();
        fs::write(assets.join("replays/exports/export.wav"), b"export").unwrap();
        assert!(trash_in(&assets, &source, "foreign.json").is_err());
        assert!(source.exists());
        assert!(trash_in(&assets, &assets, "example.json").is_err());
        trash_in(&assets, &source, "example.json").unwrap();
        assert!(!source.exists());
        assert_eq!(
            fs::read(assets.join("replays/exports/export.wav")).unwrap(),
            b"export"
        );
        assert!(restore_in(&assets, "example.json").unwrap());
        assert!(!restore_in(&assets, "example.json").unwrap());
        let restored = fs::read_dir(assets.join("replays"))
            .unwrap()
            .flatten()
            .find(|e| e.file_name().to_string_lossy().starts_with("take-restored"))
            .unwrap();
        assert_eq!(
            fs::read(restored.path().join("input.wav")).unwrap(),
            b"original input"
        );
    }
    #[test]
    fn live_input_and_operations_reproduce_bit_exact_output_and_final_loops() {
        for renderer in [2, RENDERER_VERSION] {
            check_renderer(renderer);
        }
    }
    fn check_renderer(renderer: u32) {
        let root = std::env::temp_dir().join(format!("rc505-replay-test-{}", session::id()));
        let mut config = AppConfig::new(117, 3, 5);
        config.calibration = Some(crate::config::track_options::LatencyCalibration {
            frames: 27,
            sample_rate: 8000,
            input: config.system_config.input_device.value.clone(),
            output: config.system_config.output_device.value.clone(),
            buffer_frames: 128,
            displayed_ms: 3,
        });
        for options in &mut config.track_options {
            options.quantize = Quantize::Off;
        }
        config.input_fx.set_slot_kind(0, 0, FxKind::Filter);
        config.input_fx.banks[0].slots[0].is_enabled = true;
        config.input_fx.set_slot_kind(0, 1, FxKind::Oscillator);
        config.input_fx.banks[0].slots[1].is_enabled = true;
        if let Some(crate::config::InputFx::Oscillator(osc)) =
            &mut config.input_fx.banks[0].slots[1].fx
        {
            osc.threshold.value = 0;
        }
        config.track_fx.set_slot_kind(0, 0, TrackFxKind::Delay);
        config.track_fx.tracks[0].enabled[0][0] = true;
        let mut core = RenderCore::new(8000);
        core.legacy_renderer(renderer == 2);
        core.configure(&mut Parameters::from_config(&config, 8000));
        let mut initial = AudioSnapshot::empty(8000);
        core.snapshot(&mut initial, &mut OfflinePages);
        let mut writer = Writer::begin(
            root.clone(),
            "source-project.json".into(),
            0,
            initial,
            crate::project::data_from_config(&config),
        )
        .unwrap();
        let mut live = Vec::new();
        for frame in 0..6000u64 {
            let action = match frame {
                0 | 1000 | 1600 | 2800 => Some(Action::Trigger(0)),
                3400 | 3800 if renderer == 2 => Some(Action::Undo(0)),
                3400 => Some(Action::UndoStep(0)),
                3500 => Some(Action::UndoStep(0)),
                3600 | 3800 => Some(Action::RedoStep(0)),
                4400 => Some(Action::Stop(0)),
                4700 => Some(Action::Trigger(0)),
                _ => None,
            };
            if let Some(action) = action {
                core.action(action, &mut OfflinePages);
                writer.event(frame, EventKind::Action(action)).unwrap();
            }
            if frame == 3100 {
                config.track_levels[0] = 0.37;
                let data = crate::project::data_from_config(&config);
                core.configure(&mut Parameters::from_config(&config, 8000));
                writer.event(frame, EventKind::Config(data)).unwrap();
            }
            let dry = [
                (frame as f32 * 0.123).sin() * 0.1,
                (frame as f32 * 0.057).sin() * 0.04,
            ];
            writer.audio(frame, &[dry]).unwrap();
            live.push(core.process(dry, &mut OfflinePages));
        }
        writer.finish(6000).unwrap();
        if renderer == 2 {
            let path = root.join("replay.json");
            let mut metadata: serde_json::Value =
                serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            metadata["renderer"] = 2.into();
            fs::write(path, serde_json::to_vec(&metadata).unwrap()).unwrap();
        }
        let wav = root.join("rendered.wav");
        let result = render(&root, &wav, &std::sync::atomic::AtomicU64::new(0)).unwrap();
        assert_eq!(
            result.visuals.views.first().unwrap().view.tracks[0].mode,
            crate::engine::core::Mode::Recording
        );
        assert_eq!(result.visuals.views.last().unwrap().frame, 6000);
        assert!(
            result
                .visuals
                .configs
                .iter()
                .any(|point| point.frame == 3100)
        );
        let mut display = serde_json::to_value(&result.visuals.initial).unwrap();
        for point in &result.visuals.configs {
            visuals::apply(&mut display, &point.changes);
        }
        assert_eq!(display, serde_json::to_value(&result.config).unwrap());
        let values = hound::WavReader::open(&wav)
            .unwrap()
            .into_samples::<f32>()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        for (index, value) in values.iter().enumerate() {
            assert_eq!(
                value.to_bits(),
                live[index / 2][index % 2].to_bits(),
                "render frame {}",
                index / 2
            );
        }
        assert_eq!(result.snapshot.tracks[0].len, core.tracks[0].audio.len);
        if renderer == RENDERER_VERSION {
            assert_eq!(
                result.snapshot.histories[0].undo.len,
                core.tracks[0].history.undo.len
            );
            assert_eq!(
                result.snapshot.histories[0].redo.len,
                core.tracks[0].history.redo.len
            );
        } else {
            let stack = if core.tracks[0].undone {
                &result.snapshot.histories[0].redo
            } else {
                &result.snapshot.histories[0].undo
            };
            assert_eq!(stack.len, 1);
            assert_eq!(stack.slots[0].read(0), core.tracks[0].undo.read(0));
        }
        for i in 0..core.tracks[0].audio.len {
            assert_eq!(
                result.snapshot.tracks[0].read(i),
                core.tracks[0].audio.read(i)
            );
        }
        fs::write(root.join("events.jsonl"), "corrupt").unwrap();
        assert!(
            render(
                &root,
                &root.join("invalid.wav"),
                &std::sync::atomic::AtomicU64::new(0)
            )
            .is_err()
        );
        fs::remove_dir_all(&root).unwrap();
    }
}
