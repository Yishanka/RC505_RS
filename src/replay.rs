//! Replay stores dry input and ordered, sample-stamped commands, never a master
//! recording. Live playback and WAV export execute the same RenderCore.
use crate::{
    config::AppConfig,
    engine::{
        core::{Action, AudioSnapshot, Parameters, PdcApplied, RenderCore},
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

// v6 pins sample generations for held/releasing voices and records parameter lanes.
pub const RENDERER_VERSION: u32 = 6;
mod assets;
mod delta;
pub mod library;
pub mod streaming;
pub mod visuals;
pub use visuals::{ConfigPoint, ReplayVisuals, VisualFrame};
pub const MAX_TAKE_SECONDS: u64 = 1800;
const MAX_EVENTS: u64 = 100_000;
const MAX_LOG_BYTES: u64 = 512 * 1024 * 1024;
const MAX_EVENT_BYTES: usize = 64_000_000;
#[derive(Clone, Serialize, Deserialize)]
pub enum EventKind {
    Action(Action),
    /// Verification at the boundary following the sample that applied a graph.
    /// Payload timestamps are relative to the take, like Event::frame.
    PdcApplied(PdcApplied),
    Config(ProjectData),
    ConfigDelta(delta::ConfigDelta),
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Event {
    pub frame: u64,
    pub sequence: u64,
    pub kind: EventKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    sample_assets: Vec<assets::SampleReference>,
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
    event_bytes: u64,
    initial: Option<std::thread::JoinHandle<Result<()>>>,
    assets: assets::AssetWriter,
    config_state: delta::State,
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
            event_bytes: 0,
            initial: Some(initial),
            assets: assets::AssetWriter::default(),
            config_state: delta::State::default(),
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
        ensure!(
            self.next_sequence < MAX_EVENTS,
            "Replay exceeds the 100,000 operation limit"
        );
        let (kind, sample_assets) = match kind {
            EventKind::Config(mut data) => {
                let refs = self.assets.detach(&self.root, &mut data)?;
                self.config_state.encode(data, refs)?
            }
            EventKind::Action(action) => (EventKind::Action(action), Vec::new()),
            EventKind::PdcApplied(applied) => {
                ensure!(
                    applied.requested_at >= self.origin
                        && applied.requested_at <= applied.applied_at
                        && applied.applied_at.checked_add(1) == Some(frame),
                    "Invalid PDC application timestamp"
                );
                (
                    EventKind::PdcApplied(PdcApplied {
                        requested_at: applied.requested_at - self.origin,
                        applied_at: applied.applied_at - self.origin,
                    }),
                    Vec::new(),
                )
            }
            EventKind::ConfigDelta(_) => {
                anyhow::bail!("Only the replay writer may create configuration deltas")
            }
        };
        let bytes = serde_json::to_vec(&Event {
            frame: frame - self.origin,
            sequence: self.next_sequence,
            kind,
            sample_assets,
        })?;
        ensure!(bytes.len() <= MAX_EVENT_BYTES, "Replay event is too large");
        ensure!(
            self.event_bytes + bytes.len() as u64 + 1 <= MAX_LOG_BYTES,
            "Replay event log exceeds the 512 MiB limit"
        );
        self.events.write_all(&bytes)?;
        self.events.write_all(b"\n")?;
        self.event_bytes += bytes.len() as u64 + 1;
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
        value.version == 1 && (2..=RENDERER_VERSION).contains(&value.renderer),
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
pub struct RenderResult {
    pub visuals: std::sync::Arc<ReplayVisuals>,
    pub name: String,
    pub snapshot: AudioSnapshot,
    pub config: ProjectData,
    pub wav: PathBuf,
}
pub struct Exported {
    pub name: String,
    pub wav: PathBuf,
}

/// Runs on a background thread. Input checksums are validated before rendering.
/// WAV is published only after successful completion; the source remains immutable.
pub fn render(
    root: &Path,
    destination: &Path,
    progress: &std::sync::atomic::AtomicU64,
) -> Result<RenderResult> {
    render_impl(root, destination, progress, true)
}
pub fn export(
    root: &Path,
    destination: &Path,
    progress: &std::sync::atomic::AtomicU64,
) -> Result<Exported> {
    let rendered = render_impl(root, destination, progress, false)?;
    Ok(Exported {
        name: rendered.name,
        wav: rendered.wav,
    })
}
fn render_impl(
    root: &Path,
    destination: &Path,
    progress: &std::sync::atomic::AtomicU64,
    capture_visuals: bool,
) -> Result<RenderResult> {
    ensure!(
        !destination.exists(),
        "Export already exists; choose a new name"
    );
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
    core.set_renderer_version(metadata.renderer);
    core.configure(&mut Parameters::from_config(&config, metadata.sample_rate));
    core.restore(&mut initial);
    let mut visuals = ReplayVisuals {
        name: metadata.name.clone(),
        sample_rate: metadata.sample_rate,
        frames: metadata.frames,
        views: Vec::with_capacity(if capture_visuals {
            (metadata.frames / (metadata.sample_rate as u64 / 30).max(1) + 2) as usize
        } else {
            0
        }),
        initial: data.clone(),
        configs: Vec::new(),
    };
    let mut last_action = None;
    let mut last_pdc_applied = None;
    let mut input = hound::WavReader::open(root.join("input.wav"))?;
    ensure!(
        input.spec() == session::wav_spec(metadata.sample_rate)
            && input.duration() as u64 == metadata.frames,
        "Replay WAV length/format mismatch"
    );
    let mut samples = input.samples::<f32>();
    let mut lines = BufReader::new(fs::File::open(root.join("events.jsonl"))?).lines();
    let mut expected = 0;
    let mut assets = assets::AssetReader::new(root);
    let mut config_state = delta::State::default();
    let mut next = next_event(&mut lines, expected)?;
    let temporary = destination.with_extension(format!("{}.pending.wav", session::id()));
    struct PendingWav(PathBuf);
    impl Drop for PendingWav {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }
    let _pending_cleanup = PendingWav(temporary.clone());
    let mut output = hound::WavWriter::create(&temporary, session::wav_spec(metadata.sample_rate))?;
    for frame in 0..metadata.frames {
        while next.as_ref().is_some_and(|event| event.frame == frame) {
            let event = next.take().unwrap();
            match &event.kind {
                EventKind::PdcApplied(_) => verify_pdc_applied(&event, &mut last_pdc_applied)?,
                EventKind::Action(action) => {
                    ensure!(
                        event.sample_assets.is_empty(),
                        "Action events cannot reference samples"
                    );
                    core.action(*action, &mut OfflinePages);
                    last_action = Some((frame, *action));
                }
                EventKind::Config(_) | EventKind::ConfigDelta(_) => {
                    let value = config_state
                        .apply(&event, &mut assets)?
                        .context("Missing replay config")?;
                    if capture_visuals {
                        visuals.configs.push(ConfigPoint::new(frame, &data, &value));
                    }
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
        last_pdc_applied = core.take_pdc_applied_event();
        if capture_visuals && frame % (metadata.sample_rate as u64 / 30).max(1) == 0 {
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
        match &event.kind {
            EventKind::PdcApplied(_) => verify_pdc_applied(&event, &mut last_pdc_applied)?,
            EventKind::Action(action) => {
                ensure!(
                    event.sample_assets.is_empty(),
                    "Action events cannot reference samples"
                );
                core.action(*action, &mut OfflinePages);
                last_action = Some((metadata.frames, *action));
            }
            EventKind::Config(_) | EventKind::ConfigDelta(_) => {
                let value = config_state
                    .apply(&event, &mut assets)?
                    .context("Missing replay config")?;
                if capture_visuals {
                    visuals
                        .configs
                        .push(ConfigPoint::new(metadata.frames, &data, &value));
                }
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
    if capture_visuals {
        visuals.views.push(VisualFrame {
            frame: metadata.frames,
            view: core.view(),
            last_action,
        });
    }
    progress.store(metadata.frames, std::sync::atomic::Ordering::Relaxed);
    Ok(RenderResult {
        visuals: std::sync::Arc::new(visuals),
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
    ensure!(
        sequence < MAX_EVENTS && line.len() <= MAX_EVENT_BYTES,
        "Replay event count or size limit exceeded"
    );
    let event: Event = serde_json::from_str(&line)?;
    ensure!(
        event.sequence == sequence,
        "Replay command sequence is incomplete"
    );
    if let EventKind::PdcApplied(applied) = &event.kind {
        ensure!(
            event.sample_assets.is_empty()
                && applied.requested_at <= applied.applied_at
                && applied.applied_at.checked_add(1) == Some(event.frame),
            "Invalid replay PDC application marker"
        );
    }
    Ok(Some(event))
}
fn verify_pdc_applied(event: &Event, actual: &mut Option<PdcApplied>) -> Result<()> {
    let EventKind::PdcApplied(expected) = event.kind else {
        return Ok(());
    };
    ensure!(
        actual.take() == Some(expected),
        "Replay PDC application does not match the recorded audio boundary at sample {}",
        event.frame
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{FxKind, TrackFxKind, track_options::Quantize};
    #[test]
    fn live_input_and_operations_reproduce_bit_exact_output_and_final_loops() {
        for renderer in [2, 3, RENDERER_VERSION] {
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
            if renderer >= 3 {
                use crate::config::{
                    note_configs::NoteOct,
                    sequence_edit::{NoteEvent, PPQ},
                };
                osc.note.replace_events(
                    PPQ * 4,
                    &[48, 52, 55]
                        .map(|pitch| NoteEvent::new(0, PPQ * 4, NoteOct::from_pitch_index(pitch))),
                );
            }
        }
        config.track_fx.set_slot_kind(0, 0, TrackFxKind::Delay);
        config.track_fx.tracks[0].enabled[0][0] = true;
        config.track_fx.set_slot_kind(
            0,
            1,
            TrackFxKind::Audio(crate::config::audio_fx::AudioFxKind::Transpose),
        );
        config.track_fx.tracks[0].enabled[0][1] = true;
        if let Some(crate::config::TrackFx::Audio(fx)) = &mut config.track_fx.banks[0].slots[1].fx {
            fx.semitones = 7.0;
        }
        let mut core = RenderCore::new(8000);
        core.set_renderer_version(renderer);
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
            if matches!(frame, 900 | 3100 | 3601) {
                match frame {
                    900 => config.input_thru = false,
                    3100 => {
                        config.track_levels[0] = 0.37;
                        config.master_fx.compressor_enabled = true;
                        config.master_fx.reverb_enabled = true;
                        config.master_fx.compressor.threshold_db = -24.0;
                        config.master_fx.compressor.ratio = 4.0;
                        config.master_fx.reverb.mix = 0.19;
                        if renderer >= 3 {
                            if let Some(crate::config::InputFx::Oscillator(osc)) =
                                &mut config.input_fx.banks[0].slots[1].fx
                            {
                                use crate::config::osc_configs::{
                                    SampleAsset, SampleMode, Waveform,
                                };
                                osc.sample = Some(std::sync::Arc::new(SampleAsset::new(
                                    "Replay sample".into(),
                                    8000,
                                    (0..2048).map(|i| (i as f32 * 0.12).sin() * 0.4).collect(),
                                )));
                                osc.waveform.value = Waveform::Sample;
                                osc.sample_mode = SampleMode::Sampler;
                                osc.lfo.enabled = true;
                                osc.lfo.depth = 0.3;
                            }
                        }
                    }
                    _ => {
                        config.input_thru = true;
                        config.master_fx.compressor.threshold_db = -18.0;
                    }
                }
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
        if renderer >= 3 {
            assert_eq!(
                fs::read_dir(root.join("samples")).unwrap().count(),
                1,
                "The same accepted sample across Config events is stored once"
            );
        }
        if renderer != RENDERER_VERSION {
            let path = root.join("replay.json");
            let mut metadata: serde_json::Value =
                serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            metadata["renderer"] = renderer.into();
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
        if renderer >= 3 {
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
