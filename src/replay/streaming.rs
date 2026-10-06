//! Replay playback is a bounded producer/consumer, never a rendered WAV cache.
//! One worker owns the renderer and dry-input reader. Seeking reconstructs DSP
//! history from the original inputs; revision tags prevent stale queued audio.
use super::*;
use crate::engine::{core::EngineView, loop_audio::Frame, output_resampler::OutputResampler};
use ringbuf::{HeapConsumer, HeapRb};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc,
};

const QUEUE_FRAMES: usize = 4096;
pub struct Source {
    pub root: PathBuf,
    pub metadata: ReplayInfo,
    pub legacy_mydelay: bool,
    assets: super::assets::AssetReader,
    pub(super) events: Vec<Event>,
}
impl Source {
    pub fn open(root: &Path) -> Result<Arc<Self>> {
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
        ensure!(
            fs::metadata(root.join("events.jsonl"))?.len() <= MAX_LOG_BYTES,
            "Replay event log exceeds the 512 MiB playback limit"
        );
        let mut lines = BufReader::new(fs::File::open(root.join("events.jsonl"))?).lines();
        let mut events = Vec::new();
        let mut assets = super::assets::AssetReader::new(root);
        let mut config_state = super::delta::State::default();
        let mut legacy_mydelay = false;
        while let Some(event) = next_event(&mut lines, events.len() as u64)? {
            ensure!(
                (events.len() as u64) < MAX_EVENTS,
                "Replay exceeds the 100,000 operation limit"
            );
            ensure!(
                event.frame <= metadata.frames
                    && events
                        .last()
                        .is_none_or(|last: &Event| last.frame <= event.frame),
                "Replay commands are outside the audio or out of order"
            );
            ensure!(
                metadata.renderer >= 5 || !matches!(event.kind, EventKind::PdcApplied(_)),
                "PDC markers require renderer 5"
            );
            if let Some(data) = config_state.apply(&event, &mut assets)? {
                legacy_mydelay |= contains_mydelay(&data);
            }
            events.push(event);
        }
        let legacy_mydelay = if metadata.renderer < 4 {
            let initial: session::Manifest =
                serde_json::from_slice(&fs::read(root.join("initial/manifest.json"))?)?;
            contains_mydelay(&initial.config) || legacy_mydelay
        } else {
            false
        };
        Ok(Arc::new(Self {
            root: root.to_owned(),
            metadata,
            legacy_mydelay,
            assets,
            events,
        }))
    }
}
fn contains_mydelay(data: &ProjectData) -> bool {
    data.input_fx.banks.iter().any(|bank| {
        bank.slots
            .iter()
            .any(|slot| slot.kind == "MyDelay" || slot.my_delay.is_some())
    })
}

pub struct Machine {
    source: Arc<Source>,
    input: hound::WavReader<BufReader<fs::File>>,
    pub core: Box<RenderCore>,
    pub data: Arc<ProjectData>,
    config: AppConfig,
    next_event: usize,
    pub frame: u64,
    pub last_action: Option<(u64, Action)>,
    last_pdc_applied: Option<PdcApplied>,
    config_state: super::delta::State,
    assets: super::assets::AssetReader,
}
impl Machine {
    pub fn new(source: Arc<Source>) -> Result<Self> {
        let (mut initial, data) = session::read_bundle(&source.root.join("initial"))?;
        let sr = source.metadata.sample_rate;
        ensure!(initial.sample_rate == sr, "Replay sample rate mismatch");
        let input = hound::WavReader::open(source.root.join("input.wav"))?;
        ensure!(
            input.spec() == session::wav_spec(sr)
                && input.duration() as u64 == source.metadata.frames,
            "Replay input format or length mismatch"
        );
        let mut config = AppConfig::new(120, 0, 5);
        crate::project::apply_data_to_config(&mut config, data.clone());
        let mut core = Box::new(RenderCore::new(sr));
        core.set_renderer_version(source.metadata.renderer);
        core.configure(&mut Parameters::from_config(&config, sr));
        core.restore(&mut initial);
        let assets = source.assets.clone();
        let mut result = Self {
            source,
            input,
            core,
            data: Arc::new(data),
            config,
            next_event: 0,
            frame: 0,
            last_action: None,
            last_pdc_applied: None,
            config_state: super::delta::State::default(),
            assets,
        };
        result.apply_events()?;
        Ok(result)
    }
    fn apply_events(&mut self) -> Result<()> {
        while let Some(event) = self
            .source
            .events
            .get(self.next_event)
            .filter(|e| e.frame == self.frame)
        {
            match &event.kind {
                EventKind::PdcApplied(_) => verify_pdc_applied(event, &mut self.last_pdc_applied)?,
                EventKind::Action(action) => {
                    self.core.action(*action, &mut OfflinePages);
                    self.last_action = Some((self.frame, *action));
                }
                EventKind::Config(_) | EventKind::ConfigRaw(_) | EventKind::ConfigDelta(_) => {
                    let data = self
                        .config_state
                        .apply(event, &mut self.assets)?
                        .context("Missing replay configuration")?;
                    self.data = Arc::new(data.clone());
                    crate::project::apply_data_to_config(&mut self.config, data);
                    self.core.configure(&mut Parameters::from_config(
                        &self.config,
                        self.source.metadata.sample_rate,
                    ));
                }
            }
            self.next_event += 1;
        }
        Ok(())
    }
    pub fn next(&mut self) -> Result<Option<Frame>> {
        if self.frame >= self.source.metadata.frames {
            return Ok(None);
        }
        let mut samples = self.input.samples::<f32>();
        let input = [
            samples.next().context("Truncated replay input")??,
            samples.next().context("Truncated replay input")??,
        ];
        ensure!(
            input.iter().all(|v| v.is_finite()),
            "Invalid replay input sample"
        );
        let output = self.core.process(input, &mut OfflinePages);
        self.last_pdc_applied = self.core.take_pdc_applied_event();
        self.frame += 1;
        self.apply_events()?;
        Ok(Some(output))
    }
    pub fn advance_to(&mut self, target: u64, mut cancelled: impl FnMut() -> bool) -> Result<bool> {
        let target = target.min(self.source.metadata.frames);
        while self.frame < target {
            if self.frame % 2048 == 0 && cancelled() {
                return Ok(false);
            }
            self.next()?;
        }
        Ok(!cancelled())
    }
    pub fn snapshot(&self) -> AudioSnapshot {
        let mut snapshot = AudioSnapshot::empty(self.source.metadata.sample_rate);
        self.core.snapshot(&mut snapshot, &mut OfflinePages);
        snapshot
    }
    fn display(&mut self, revision: u64) -> DisplayFrame {
        DisplayFrame {
            revision,
            frame: self.frame,
            view: self.core.view(),
            data: self.data.clone(),
            last_action: self.last_action,
        }
    }
}

pub struct Shared {
    pub playing: AtomicBool,
    // A revision and its position/end bit are published together. An old
    // callback racing a seek cannot overwrite the new position with stale data.
    cursor: AtomicU64,
    pub underruns: AtomicU64,
    failed: AtomicBool,
    revision: AtomicU64,
    ready_revision: AtomicU64,
    target: AtomicU64,
    stop: AtomicBool,
    frames: u64,
}
impl Shared {
    pub fn seek(&self, frame: u64) {
        if self.failed.load(Ordering::Acquire) {
            return;
        }
        self.target.store(frame.min(self.frames), Ordering::Release);
        self.revision.fetch_add(1, Ordering::AcqRel);
    }
    pub fn toggle(&self) {
        if self.failed.load(Ordering::Acquire) {
            return;
        }
        if self.ended() {
            self.seek(0);
            self.playing.store(true, Ordering::Release);
        } else {
            self.playing.fetch_xor(true, Ordering::AcqRel);
        }
    }
    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }
    pub fn seeking(&self) -> bool {
        !self.failed.load(Ordering::Acquire)
            && self.ready_revision.load(Ordering::Acquire) != self.revision()
    }
    pub fn position(&self) -> u64 {
        let cursor = self.cursor.load(Ordering::Acquire);
        if cursor >> 32 == self.revision() as u32 as u64
            || self.ready_revision.load(Ordering::Acquire) != self.revision()
        {
            cursor & 0x7fff_ffff
        } else {
            self.target.load(Ordering::Acquire)
        }
    }
    pub fn ended(&self) -> bool {
        let cursor = self.cursor.load(Ordering::Acquire);
        cursor >> 32 == self.revision() as u32 as u64 && cursor & 0x8000_0000 != 0
    }
    pub fn playing(&self) -> bool {
        self.playing.load(Ordering::Acquire) && !self.ended()
    }
    fn publish_cursor(&self, revision: u64, position: u64, end: bool) {
        debug_assert!(position < 0x8000_0000);
        self.cursor.store(
            (revision << 32) | position | if end { 0x8000_0000 } else { 0 },
            Ordering::Release,
        );
    }
}

pub struct DisplayFrame {
    pub revision: u64,
    pub frame: u64,
    pub view: EngineView,
    pub data: Arc<ProjectData>,
    pub last_action: Option<(u64, Action)>,
}
pub enum DisplayEvent {
    Frame(DisplayFrame),
    Error(String),
}
#[derive(Clone, Copy)]
struct OutputFrame {
    revision: u64,
    position: u64,
    samples: Frame,
    end: bool,
}
pub struct Consumer {
    queue: HeapConsumer<OutputFrame>,
    pub shared: Arc<Shared>,
}
impl Consumer {
    /// Only bounded ring operations and atomics run on the audio callback.
    pub fn next(&mut self) -> Frame {
        let revision = self.shared.revision();
        // Drop old revisions even while paused. Otherwise a full old queue
        // could prevent the worker from producing the first sought sample.
        while self
            .queue
            .iter()
            .next()
            .is_some_and(|packet| packet.revision < revision)
        {
            let _ = self.queue.pop();
        }
        if self.shared.seeking() || self.shared.revision() != revision {
            return [0.0; 2];
        }
        if !self.shared.playing() {
            return [0.0; 2];
        }
        if self
            .queue
            .iter()
            .next()
            .is_some_and(|packet| packet.revision > revision)
        {
            return [0.0; 2];
        }
        if let Some(packet) = self.queue.pop() {
            if self.shared.revision() != revision {
                return [0.0; 2];
            }
            self.shared
                .publish_cursor(revision, packet.position, packet.end);
            return packet.samples;
        }
        self.shared.underruns.fetch_add(1, Ordering::Relaxed);
        [0.0; 2]
    }
}
impl Drop for Consumer {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
    }
}
pub struct Session {
    pub source: Arc<Source>,
    pub initial: Arc<ProjectData>,
    pub shared: Arc<Shared>,
    pub display: mpsc::Receiver<DisplayEvent>,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
    }
}
/// Construct an unsaved working copy; this never writes a project file.
pub fn prepare_import(
    source: Arc<Source>,
    frame: u64,
    sample_rate: u32,
) -> Result<(ProjectData, Box<RenderCore>)> {
    let mut machine = Machine::new(source)?;
    machine.advance_to(frame, || false)?;
    let mut snapshot = machine.snapshot();
    let mut data = (*machine.data).clone();
    data.snapshot = None;
    crate::presets::localize_replay_samples(&mut data);
    session::resample(&mut snapshot, sample_rate)?;
    let mut config = AppConfig::new(120, 0, 5);
    crate::project::apply_data_to_config(&mut config, data.clone());
    // The audio boundary can precede the UI's configuration acknowledgement by
    // one or more frames. Import the phrase actually accepted by this core.
    crate::phrases::commit_applied(&mut config, &machine.core.view().phrases);
    data = crate::project::data_from_config(&config);
    let mut core = Box::new(RenderCore::new(sample_rate));
    core.configure(&mut Parameters::from_config(&config, sample_rate));
    core.restore(&mut snapshot);
    Ok((data, core))
}
pub fn start(root: &Path, output_rate: u32) -> Result<(Consumer, Session)> {
    ensure!(
        (8_000..=192_000).contains(&output_rate),
        "Invalid playback sample rate"
    );
    let source = Source::open(root)?;
    let machine = Machine::new(source.clone())?;
    let initial = machine.data.clone();
    let (mut producer, queue) = HeapRb::new(QUEUE_FRAMES).split();
    let (tx, display) = mpsc::sync_channel(64);
    let shared = Arc::new(Shared {
        playing: AtomicBool::new(true),
        cursor: AtomicU64::new(0),
        underruns: AtomicU64::new(0),
        failed: AtomicBool::new(false),
        revision: AtomicU64::new(0),
        ready_revision: AtomicU64::new(0),
        target: AtomicU64::new(0),
        stop: AtomicBool::new(false),
        frames: source.metadata.frames,
    });
    let state = shared.clone();
    let source_thread = source.clone();
    std::thread::Builder::new()
        .name("replay-simulation".into())
        .spawn(move || {
            let result = (|| -> Result<()> {
                let sr = source_thread.metadata.sample_rate;
                let mut machine = machine;
                let mut revision = 0;
                let mut resampler = OutputResampler::new(sr, output_rate);
                let mut origin = 0;
                let mut output_cursor = 0u64;
                let mut total_output = source_thread
                    .metadata
                    .frames
                    .saturating_mul(output_rate as u64)
                    .div_ceil(sr as u64);
                let mut next_display = 0;
                let mut ended = false;
                let mut lookahead = std::collections::VecDeque::with_capacity(32);
                let mut displayed_data = machine.data.clone();
                let mut displayed_action = None;
                loop {
                    if state.stop.load(Ordering::Acquire) {
                        break;
                    }
                    let requested = state.revision();
                    if requested != revision {
                        revision = requested;
                        let target = state.target.load(Ordering::Acquire);
                        machine = Machine::new(source_thread.clone())?;
                        resampler = OutputResampler::new(sr, output_rate);
                        lookahead.clear();
                        let mut target_display = if target == 0 {
                            Some(machine.display(revision))
                        } else {
                            None
                        };
                        // Reconstruct the output rate converter too. Resetting it
                        // at the seek point would produce a cold-filter transient
                        // and different samples from uninterrupted playback.
                        origin = 0;
                        output_cursor = target
                            .saturating_mul(output_rate as u64)
                            .div_ceil(sr as u64);
                        if sr == output_rate {
                            if !machine.advance_to(target, || {
                                state.stop.load(Ordering::Acquire) || state.revision() != revision
                            })? {
                                continue;
                            }
                            target_display = Some(machine.display(revision));
                        } else {
                            for at in 0..output_cursor {
                                if at % 2048 == 0
                                    && (state.stop.load(Ordering::Acquire)
                                        || state.revision() != revision)
                                {
                                    break;
                                }
                                let mut error = None;
                                resampler.next(|| {
                                    let value = match machine.next() {
                                        Ok(Some(v)) => v,
                                        Ok(None) => [0.0; 2],
                                        Err(e) => {
                                            error = Some(e);
                                            [0.0; 2]
                                        }
                                    };
                                    if machine.frame == target {
                                        target_display = Some(machine.display(revision));
                                    }
                                    value
                                });
                                if let Some(error) = error {
                                    return Err(error);
                                }
                            }
                            if state.stop.load(Ordering::Acquire) || state.revision() != revision {
                                continue;
                            }
                            // A downsampling phase can leave the source one frame
                            // before the chosen point. Compute that state exactly,
                            // retaining its wet samples for the converter to consume.
                            while machine.frame < target {
                                if let Some(value) = machine.next()? {
                                    lookahead.push_back(value);
                                }
                            }
                            if target_display.is_none() {
                                target_display = Some(machine.display(revision));
                            }
                        }
                        total_output = source_thread
                            .metadata
                            .frames
                            .saturating_mul(output_rate as u64)
                            .div_ceil(sr as u64);
                        next_display = target;
                        ended = false;
                        if let Some(display) = target_display {
                            // Use a blocking send only on this worker: the explicit
                            // target view must not be lost behind obsolete frames.
                            if tx.send(DisplayEvent::Frame(display)).is_err() {
                                break;
                            }
                        }
                        displayed_data = machine.data.clone();
                        displayed_action = machine.last_action;
                        state.publish_cursor(
                            revision,
                            target,
                            target == source_thread.metadata.frames,
                        );
                        state.ready_revision.store(revision, Ordering::Release);
                    }
                    if machine.frame >= next_display
                        || !Arc::ptr_eq(&displayed_data, &machine.data)
                        || displayed_action != machine.last_action
                    {
                        let _ = tx.try_send(DisplayEvent::Frame(machine.display(revision)));
                        next_display = machine.frame + (sr as u64 / 30).max(1);
                        displayed_data = machine.data.clone();
                        displayed_action = machine.last_action;
                    }
                    if ended || producer.free_len() == 0 {
                        std::thread::sleep(std::time::Duration::from_millis(1));
                        continue;
                    }
                    if output_cursor >= total_output {
                        let _ = producer.push(OutputFrame {
                            revision,
                            position: source_thread.metadata.frames,
                            samples: [0.0; 2],
                            end: true,
                        });
                        let _ = tx.try_send(DisplayEvent::Frame(DisplayFrame {
                            revision,
                            frame: machine.frame,
                            view: machine.core.view(),
                            data: machine.data.clone(),
                            last_action: machine.last_action,
                        }));
                        ended = true;
                        continue;
                    }
                    let mut error = None;
                    let samples = resampler.next(|| {
                        if let Some(frame) = lookahead.pop_front() {
                            return frame;
                        }
                        match machine.next() {
                            Ok(Some(frame)) => frame,
                            Ok(None) => [0.0; 2],
                            Err(e) => {
                                error = Some(e);
                                [0.0; 2]
                            }
                        }
                    });
                    if let Some(error) = error {
                        return Err(error);
                    }
                    output_cursor += 1;
                    let position = (origin
                        + output_cursor.saturating_mul(sr as u64) / output_rate as u64)
                        .min(source_thread.metadata.frames);
                    let _ = producer.push(OutputFrame {
                        revision,
                        position,
                        samples,
                        end: false,
                    });
                }
                Ok(())
            })();
            if let Err(error) = result {
                state.playing.store(false, Ordering::Release);
                state.failed.store(true, Ordering::Release);
                let _ = tx.send(DisplayEvent::Error(error.to_string()));
            }
        })?;
    Ok((
        Consumer {
            queue,
            shared: shared.clone(),
        },
        Session {
            source,
            initial,
            shared,
            display,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (PathBuf, Vec<Frame>) {
        fixture_at(8000)
    }
    #[test]
    fn retired_filter_and_osc_fields_remain_patchable_in_legacy_wire_baselines() {
        use crate::config::{FxKind, InputFx, TrackFxKind};
        let root = PathBuf::from("var").join(format!("stream-replay-test-{}", session::id()));
        let mut config = AppConfig::new(120, 0, 5);
        config.input_fx.set_slot_kind(0, 0, FxKind::Filter);
        config.input_fx.banks[0].slots[0].is_enabled = true;
        config.input_fx.set_slot_kind(0, 1, FxKind::Oscillator);
        config.track_fx.set_slot_kind(0, 0, TrackFxKind::Filter);
        config.track_fx.tracks[0].enabled[0][0] = true;
        if let Some(InputFx::Filter(filter)) = &mut config.input_fx.banks[0].slots[0].fx {
            filter.cutoff_hz.value = 1200;
        }
        let data = crate::project::data_from_config(&config);
        let mut core = RenderCore::new(8000);
        core.set_renderer_version(9);
        core.configure(&mut Parameters::from_config(&config, 8000));
        let mut initial = AudioSnapshot::empty(8000);
        for n in 0..317 {
            initial.tracks[0].write(n, [(n as f32 * 0.13).sin() * 0.03; 2], &mut OfflinePages);
        }
        core.restore(&mut initial);
        core.snapshot(&mut initial, &mut OfflinePages);
        let mut writer = Writer::begin(
            root.clone(),
            "legacy-filter.json".into(),
            0,
            initial,
            data.clone(),
        )
        .unwrap();
        let mut live = Vec::new();
        for n in 0..1600u64 {
            if n == 0 {
                core.action(Action::All, &mut OfflinePages);
            }
            if n == 500 {
                config.track_levels[0] = 0.6;
                core.configure(&mut Parameters::from_config(&config, 8000));
            }
            if n == 700 {
                if let Some(InputFx::Filter(filter)) = &mut config.input_fx.banks[0].slots[0].fx {
                    filter.cutoff_hz.value = 2300;
                }
                core.configure(&mut Parameters::from_config(&config, 8000));
            }
            let dry = [(n as f32 * 0.193).sin() * 0.04; 2];
            writer.audio(n, &[dry]).unwrap();
            live.push(core.process(dry, &mut OfflinePages));
        }
        writer.finish(1600).unwrap();
        let mut old = serde_json::to_value(data).unwrap();
        old["input_fx"]["banks"][0]["slots"][0]["filter"]["sweep"] =
            serde_json::json!({"depth":0.7,"sync":true});
        old["track_fx"]["banks"][0]["slots"][0]["filter"]["env"] =
            serde_json::json!({"attack_ms":0.0,"sustain_pct":0});
        old["track_fx"]["banks"][0]["slots"][0]["filter"]["seq"] =
            serde_json::json!([false, true, false]);
        let osc = old["input_fx"]["banks"][0]["slots"][1]["osc"]
            .as_object_mut()
            .unwrap();
        osc.remove("gate_threshold");
        osc.remove("capture_threshold");
        osc.insert("threshold".into(), serde_json::json!(10));
        let manifest_path = root.join("initial/manifest.json");
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest["config"] = old.clone();
        fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let events = [
            serde_json::json!({"frame":0,"sequence":0,"kind":{"Config":old}}),
            serde_json::json!({"frame":0,"sequence":1,"kind":{"Action":"All"}}),
            serde_json::json!({"frame":300,"sequence":2,"kind":{"ConfigDelta":{"r":2,"ops":[
                {"p":["config","input_fx","banks",0,"slots",0,"filter","sweep","depth"],"v":1.0},
                {"p":["config","track_fx","banks",0,"slots",0,"filter","env","attack_ms"],"v":999.0},
                {"p":["config","track_fx","banks",0,"slots",0,"filter","seq",1],"v":false},
                {"p":["config","input_fx","banks",0,"slots",1,"osc","threshold"],"v":90}]}}}),
            serde_json::json!({"frame":500,"sequence":3,"kind":{"ConfigDelta":{"r":3,"ops":[{"p":["config","track_levels",0],"v":0.6}]}}}),
            serde_json::json!({"frame":700,"sequence":4,"kind":{"ConfigDelta":{"r":4,"ops":[
                {"p":["config","input_fx","banks",0,"slots",0,"filter","sweep"],"v":{"depth":0.25,"sync":false}},
                {"p":["config","track_fx","banks",0,"slots",0,"filter","env"],"v":{"attack_ms":17.0,"sustain_pct":20}},
                {"p":["config","track_fx","banks",0,"slots",0,"filter","seq"],"v":[true]},
                {"p":["config","input_fx","banks",0,"slots",0,"filter","cutoff_hz"],"v":2300}]}}}),
            serde_json::json!({"frame":1000,"sequence":5,"kind":{"ConfigDelta":{"r":5,"ops":[
                {"p":["config","input_fx","banks",0,"slots",0,"filter","sweep","depth"],"v":0.9},
                {"p":["config","input_fx","banks",0,"slots",1,"osc","threshold"],"v":77}]}}}),
        ];
        let log = events
            .iter()
            .map(|event| serde_json::to_string(event).unwrap())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        fs::write(root.join("events.jsonl"), log.as_bytes()).unwrap();
        let mut metadata = info(&root).unwrap();
        metadata.renderer = 9;
        metadata.initial_sha256 = session::checksum(&manifest_path).unwrap();
        metadata.events_sha256 = session::checksum(&root.join("events.jsonl")).unwrap();
        fs::write(
            root.join("replay.json"),
            serde_json::to_vec(&metadata).unwrap(),
        )
        .unwrap();
        let source = Source::open(&root).unwrap();
        assert!(matches!(source.events[0].kind, EventKind::ConfigRaw(_)));
        let mut machine = Machine::new(source.clone()).unwrap();
        for (n, expected) in live.iter().enumerate() {
            assert_eq!(
                machine.next().unwrap().unwrap().map(f32::to_bits),
                expected.map(f32::to_bits),
                "Retired fields changed modern DSP at {n}"
            );
        }
        for target in [0, 299, 300, 499, 500, 699, 700, 999, 1000] {
            let mut seek = Machine::new(source.clone()).unwrap();
            seek.advance_to(target, || false).unwrap();
            for expected in &live[target as usize..target as usize + 32] {
                assert_eq!(
                    seek.next().unwrap().unwrap().map(f32::to_bits),
                    expected.map(f32::to_bits)
                );
            }
        }
        let wav = root.join("explicit.wav");
        super::super::render(&root, &wav, &AtomicU64::new(0)).unwrap();
        let rendered: Vec<f32> = hound::WavReader::open(wav)
            .unwrap()
            .into_samples()
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        assert_eq!(
            rendered.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            live.iter()
                .flatten()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            fs::read_to_string(root.join("events.jsonl")).unwrap(),
            log,
            "Reading must not rewrite legacy logs"
        );
        drop(machine);
        drop(source);
        drop(core);
        cleanup(&root);
    }
    #[test]
    fn parameter_lanes_and_updated_distortion_dynamics_reproduce_live_seek_and_export() {
        use crate::config::{
            FxKind, InputFx, TrackFx, TrackFxKind,
            audio_fx::{AudioFxKind as K, DistortionType},
            automation::{Interpolation, ParameterLane, Target},
            dynamics_profiles::DynamicsProfile,
            track_options::InputRouting,
        };
        let root = PathBuf::from("var").join(format!("stream-replay-test-{}", session::id()));
        let origin = 81723;
        let mut config = AppConfig::new(120, 0, 5);
        config.input_routing = InputRouting::Serial;
        for (slot, kind) in [
            (0, FxKind::Audio(K::Distortion)),
            (1, FxKind::Filter),
            (2, FxKind::Audio(K::PanningDelay)),
            (3, FxKind::Reverb),
        ] {
            config.input_fx.set_slot_kind(0, slot, kind);
            config.input_fx.banks[0].slots[slot].is_enabled = true;
        }
        for (slot, kind) in [
            (0, TrackFxKind::Filter),
            (1, TrackFxKind::Delay),
            (2, TrackFxKind::Audio(K::Reverb)),
            (3, TrackFxKind::Audio(K::Dynamics)),
        ] {
            config.track_fx.set_slot_kind(0, slot, kind);
            config.track_fx.tracks[0].enabled[0][slot] = true;
        }
        let lane = |target| {
            let mut lane = ParameterLane::create(target);
            lane.enabled = true;
            lane.interpolation = Interpolation::Curve;
            lane.points[0].curve = 0.6;
            lane
        };
        for (slot, target) in [
            (1, Target::FilterCutoff),
            (2, Target::DelayTime),
            (3, Target::ReverbDecay),
        ] {
            config.input_fx.banks[0].slots[slot].parameter_lane = lane(target);
        }
        for (slot, target) in [
            (0, Target::FilterQ),
            (1, Target::DelayFeedback),
            (2, Target::ReverbAudioDecay),
        ] {
            config.track_fx.banks[0].slots[slot].parameter_lane = lane(target);
        }
        if let Some(InputFx::Audio(p)) = &mut config.input_fx.banks[0].slots[0].fx {
            p.distortion_type = DistortionType::Metal;
        }
        if let Some(TrackFx::Audio(p)) = &mut config.track_fx.banks[0].slots[3].fx {
            p.dynamics_profile = DynamicsProfile::PhoneVox;
            p.dynamics_amount = 7.0;
        }
        let mut core = RenderCore::new(8000);
        core.configure(&mut Parameters::from_config(&config, 8000));
        let mut initial = AudioSnapshot::empty(8000);
        for frame in 0..480 {
            initial.tracks[0].write(
                frame,
                [(frame as f32 * 0.23).sin() * 0.025; 2],
                &mut OfflinePages,
            );
        }
        core.restore(&mut initial);
        core.clock.frame = origin;
        core.snapshot(&mut initial, &mut OfflinePages);
        let mut writer = Writer::begin(
            root.clone(),
            "lanes.json".into(),
            origin,
            initial,
            crate::project::data_from_config(&config),
        )
        .unwrap();
        let mut live = Vec::new();
        for frame in 0..5200 {
            if matches!(frame, 0 | 4500) {
                core.action(Action::All, &mut OfflinePages);
                writer
                    .event(origin + frame, EventKind::Action(Action::All))
                    .unwrap();
            }
            if matches!(frame, 700 | 1300 | 2400 | 3200) {
                if frame == 700 {
                    if let Some(InputFx::Audio(p)) = &mut config.input_fx.banks[0].slots[0].fx {
                        p.distortion_type = DistortionType::Fuzz;
                    }
                    if let Some(TrackFx::Audio(p)) = &mut config.track_fx.banks[0].slots[3].fx {
                        p.dynamics_profile = DynamicsProfile::LowBoost;
                    }
                    config.input_fx.banks[0].slots[1]
                        .parameter_lane
                        .interpolation = Interpolation::Step;
                }
                if frame == 1300 {
                    config.input_fx.banks[0].slots[2].parameter_lane.target = Target::DelayWet;
                    config.input_fx.banks[0].slots[3].parameter_lane.target = Target::ReverbWet;
                    config.track_fx.banks[0].slots[2].parameter_lane.target = Target::ReverbWet;
                    config.track_fx.banks[0].slots[0]
                        .parameter_lane
                        .interpolation = Interpolation::Linear;
                }
                if frame == 2400 {
                    config
                        .input_fx
                        .set_slot_kind(0, 0, FxKind::Audio(K::Transpose));
                    if let Some(InputFx::Audio(p)) = &mut config.input_fx.banks[0].slots[0].fx {
                        p.semitones = 7.0;
                    }
                }
                if frame == 3200 {
                    config.input_fx.banks[0].slots[1].parameter_lane.enabled = false;
                    for point in &mut config.track_fx.banks[0].slots[1].parameter_lane.points {
                        point.value = 1.0;
                    }
                }
                core.configure(&mut Parameters::from_config(&config, 8000));
                writer
                    .event(
                        origin + frame,
                        EventKind::Config(crate::project::data_from_config(&config)),
                    )
                    .unwrap();
            }
            let dry = [
                (frame as f32 * 0.113).sin() * 0.025,
                (frame as f32 * 0.197).cos() * 0.02,
            ];
            writer.audio(origin + frame, &[dry]).unwrap();
            live.push(core.process(dry, &mut OfflinePages));
        }
        writer.finish(origin + 5200).unwrap();
        let source = Source::open(&root).unwrap();
        let mut replay = Machine::new(source.clone()).unwrap();
        for (frame, expected) in live.iter().enumerate() {
            assert_eq!(
                replay.next().unwrap().unwrap().map(f32::to_bits),
                expected.map(f32::to_bits),
                "parameter lane frame {frame}"
            );
        }
        for target in [0, 699, 700, 1300, 2399, 2400, 3200, 4500] {
            let mut seek = Machine::new(source.clone()).unwrap();
            seek.advance_to(target, || false).unwrap();
            for expected in &live[target as usize..target as usize + 60] {
                assert_eq!(
                    seek.next().unwrap().unwrap().map(f32::to_bits),
                    expected.map(f32::to_bits)
                );
            }
        }
        let (imported, _) = prepare_import(source.clone(), 3300, 8000).unwrap();
        assert_eq!(
            imported.track_fx.banks[0].slots[1].parameter_lane.points[0].value,
            1.0
        );
        assert!(!imported.input_fx.banks[0].slots[1].parameter_lane.enabled);
        let output = root.join("explicit.wav");
        super::super::render(&root, &output, &AtomicU64::new(0)).unwrap();
        let exported: Vec<f32> = hound::WavReader::open(output)
            .unwrap()
            .into_samples()
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        assert_eq!(
            exported.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            live.iter()
                .flatten()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>()
        );
        drop(replay);
        drop(source);
        drop(core);
        cleanup(&root);
    }
    #[test]
    fn filter_automation_and_mono_enhance_replay_through_transport_changes_and_seek() {
        use crate::config::{FxKind, InputFx, audio_fx::AudioFxKind, track_options::InputRouting};
        let root = PathBuf::from("var").join(format!("stream-replay-test-{}", session::id()));
        let origin = 123456;
        let mut config = AppConfig::new(120, 0, 5);
        config.input_routing = InputRouting::Serial;
        config.input_fx.set_slot_kind(0, 0, FxKind::Filter);
        config
            .input_fx
            .set_slot_kind(0, 1, FxKind::Audio(AudioFxKind::StereoEnhance));
        for slot in &mut config.input_fx.banks[0].slots[..2] {
            slot.is_enabled = true;
        }
        if let Some(InputFx::Filter(filter)) = &mut config.input_fx.banks[0].slots[0].fx {
            filter.cutoff_hz.value = 4000;
        }
        use crate::config::automation::{Interpolation, ParameterLane, Target};
        let mut lane = ParameterLane::create(Target::FilterCutoff);
        lane.enabled = true;
        lane.interpolation = Interpolation::Curve;
        lane.points[0].curve = -0.6;
        config.input_fx.banks[0].slots[0].parameter_lane = lane;
        let mut core = RenderCore::new(8000);
        core.configure(&mut Parameters::from_config(&config, 8000));
        core.clock.frame = origin;
        let mut initial = AudioSnapshot::empty(8000);
        core.snapshot(&mut initial, &mut OfflinePages);
        let mut writer = Writer::begin(
            root.clone(),
            "enhance-automation.json".into(),
            origin,
            initial,
            crate::project::data_from_config(&config),
        )
        .unwrap();
        let mut live = Vec::new();
        for frame in 0..6000 {
            let action = match frame {
                1000 | 4000 | 4500 => Some(Action::All),
                _ => None,
            };
            if let Some(action) = action {
                core.action(action, &mut OfflinePages);
                writer
                    .event(origin + frame, EventKind::Action(action))
                    .unwrap();
            }
            if matches!(frame, 2000 | 3500 | 4750) {
                let lane = &mut config.input_fx.banks[0].slots[0].parameter_lane;
                lane.points[1].value = 0.3;
                lane.interpolation = if frame >= 3500 {
                    Interpolation::Step
                } else {
                    Interpolation::Linear
                };
                if let Some(InputFx::Audio(enhance)) = &mut config.input_fx.banks[0].slots[1].fx {
                    enhance.enhance_amount = 0.8;
                    enhance.enhance_low_cut_hz = if frame == 3500 { 900.0 } else { 0.0 };
                    enhance.enhance_high_cut_hz = 2800.0;
                }
                core.configure(&mut Parameters::from_config(&config, 8000));
                writer
                    .event(
                        origin + frame,
                        EventKind::Config(crate::project::data_from_config(&config)),
                    )
                    .unwrap();
            }
            let dry = [((frame as f32 * 0.87).sin() + (frame as f32 * 0.043).sin()) * 0.1; 2];
            writer.audio(origin + frame, &[dry]).unwrap();
            live.push(core.process(dry, &mut OfflinePages));
        }
        writer.finish(origin + 6000).unwrap();
        assert!(
            live.iter().any(|out| (out[0] - out[1]).abs() > 0.001),
            "Mono input should acquire side energy"
        );
        let source = Source::open(&root).unwrap();
        let mut replay = Machine::new(source.clone()).unwrap();
        for (frame, expected) in live.iter().enumerate() {
            assert_eq!(
                replay.next().unwrap().unwrap().map(f32::to_bits),
                expected.map(f32::to_bits),
                "filter automation/enhance frame {frame}"
            );
        }
        for target in [0, 999, 1000, 2000, 3499, 3500, 4001, 4500, 4750] {
            let mut seek = Machine::new(source.clone()).unwrap();
            seek.advance_to(target, || false).unwrap();
            for expected in &live[target as usize..target as usize + 50] {
                assert_eq!(
                    seek.next().unwrap().unwrap().map(f32::to_bits),
                    expected.map(f32::to_bits)
                );
            }
        }
        let output = root.join("explicit.wav");
        super::super::render(&root, &output, &AtomicU64::new(0)).unwrap();
        let exported: Vec<f32> = hound::WavReader::open(output)
            .unwrap()
            .into_samples()
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        assert_eq!(
            exported.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            live.iter()
                .flatten()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>()
        );
        drop(replay);
        drop(source);
        drop(core);
        cleanup(&root);
    }
    #[test]
    fn deferred_pdc_markers_preserve_same_boundary_order_export_and_seek() {
        use crate::config::{FxKind, audio_fx::AudioFxKind, track_options::Quantize};
        let root = PathBuf::from("var").join(format!("stream-replay-test-{}", session::id()));
        let origin = 987654;
        let mut config = AppConfig::new(120, 0, 5);
        config.track_options[0].quantize = Quantize::Off;
        let mut core = RenderCore::new(8000);
        core.configure(&mut Parameters::from_config(&config, 8000));
        core.clock.frame = origin;
        let mut initial = AudioSnapshot::empty(8000);
        core.snapshot(&mut initial, &mut OfflinePages);
        let mut writer = Writer::begin(
            root.clone(),
            "pdc.json".into(),
            origin,
            initial,
            crate::project::data_from_config(&config),
        )
        .unwrap();
        let mut live = Vec::new();
        let mut applied_frames = Vec::new();
        for frame in 0..5000 {
            if matches!(frame, 0 | 400 | 500 | 900) {
                core.action(Action::Trigger(0), &mut OfflinePages);
                writer
                    .event(origin + frame, EventKind::Action(Action::Trigger(0)))
                    .unwrap();
            }
            if matches!(frame, 100 | 401 | 600) {
                if frame == 100 {
                    config
                        .input_fx
                        .set_slot_kind(0, 0, FxKind::Audio(AudioFxKind::Transpose));
                    config.input_fx.banks[0].slots[0].is_enabled = true;
                }
                if frame == 401 {
                    config.track_levels[0] = 0.7;
                }
                if frame == 600 {
                    config.input_fx.set_slot_kind(0, 0, FxKind::None);
                }
                core.configure(&mut Parameters::from_config(&config, 8000));
                writer
                    .event(
                        origin + frame,
                        EventKind::Config(crate::project::data_from_config(&config)),
                    )
                    .unwrap();
            }
            let dry = [
                (frame as f32 * 0.107).sin() * 0.2,
                (frame as f32 * 0.079).cos() * 0.15,
            ];
            writer.audio(origin + frame, &[dry]).unwrap();
            live.push(core.process(dry, &mut OfflinePages));
            if let Some(applied) = core.take_pdc_applied_event() {
                applied_frames.push(frame);
                writer
                    .event(origin + frame + 1, EventKind::PdcApplied(applied))
                    .unwrap();
            }
        }
        writer.finish(origin + 5000).unwrap();
        assert_eq!(applied_frames.len(), 2);
        assert_eq!(applied_frames[0], 400);
        let source = Source::open(&root).unwrap();
        let marker_index = source
            .events
            .iter()
            .position(|e| matches!(e.kind, EventKind::PdcApplied(_)))
            .unwrap();
        assert_eq!(source.events[marker_index].frame, 401);
        assert_eq!(
            source.events[marker_index + 1].frame,
            401,
            "The marker must remain before the next accepted config at this boundary"
        );
        let mut machine = Machine::new(source.clone()).unwrap();
        for expected in &live {
            assert_eq!(
                machine.next().unwrap().unwrap().map(f32::to_bits),
                expected.map(f32::to_bits)
            );
        }
        for target in [
            99,
            100,
            399,
            400,
            401,
            899,
            900,
            applied_frames[1],
            applied_frames[1] + 1,
        ] {
            let mut seek = Machine::new(source.clone()).unwrap();
            seek.advance_to(target, || false).unwrap();
            for expected in &live[target as usize..target as usize + 40] {
                assert_eq!(
                    seek.next().unwrap().unwrap().map(f32::to_bits),
                    expected.map(f32::to_bits)
                );
            }
        }
        let destination = root.join("explicit.wav");
        super::super::render(&root, &destination, &AtomicU64::new(0)).unwrap();
        let rendered: Vec<f32> = hound::WavReader::open(destination)
            .unwrap()
            .into_samples()
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        assert_eq!(
            rendered.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            live.iter()
                .flatten()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>()
        );
        let mut invalid = Source::open(&root).unwrap();
        if let EventKind::PdcApplied(marker) =
            &mut Arc::get_mut(&mut invalid).unwrap().events[marker_index].kind
        {
            marker.requested_at += 1;
        }
        let error = Machine::new(invalid)
            .unwrap()
            .advance_to(402, || false)
            .unwrap_err();
        assert!(error.to_string().contains("PDC application"));
        drop(machine);
        drop(source);
        drop(core);
        cleanup(&root);
    }
    #[test]
    fn queued_phrase_and_ui_ack_replay_and_seek_match_live_sample_for_sample() {
        use crate::{
            config::{
                FxKind,
                note_configs::{NoteConfigs, NoteOct},
                sequence_edit::NoteEvent,
            },
            presets::FxTarget,
        };
        let root = PathBuf::from("var").join(format!("stream-replay-test-{}", session::id()));
        let mut config = AppConfig::new(120, 0, 5);
        config.input_fx.set_slot_kind(0, 0, FxKind::Oscillator);
        config.input_fx.banks[0].slots[0].is_enabled = true;
        for track in &mut config.track_options {
            track.quantize = crate::config::track_options::Quantize::Off;
        }
        crate::presets::note_mut(&mut config, FxTarget::Input { bank: 0, slot: 0 })
            .unwrap()
            .replace_events(
                960,
                &[NoteEvent::new(0, 480, NoteOct::from_pitch_index(48))],
            );
        let mut next = NoteConfigs::new();
        next.replace_events(
            1440,
            &[NoteEvent::new(0, 720, NoteOct::from_pitch_index(55))],
        );
        let mut core = RenderCore::new(8000);
        core.configure(&mut Parameters::from_config(&config, 8000));
        let mut initial = AudioSnapshot::empty(8000);
        core.snapshot(&mut initial, &mut OfflinePages);
        let mut writer = Writer::begin(
            root.clone(),
            "phrase.json".into(),
            0,
            initial,
            crate::project::data_from_config(&config),
        )
        .unwrap();
        let mut live = Vec::new();
        for frame in 0..8000 {
            if frame == 0 || frame == 6000 {
                core.action(Action::Trigger(0), &mut OfflinePages);
                writer
                    .event(frame, EventKind::Action(Action::Trigger(0)))
                    .unwrap();
            }
            if frame == 1234 {
                crate::presets::note_mut(&mut config, FxTarget::Input { bank: 0, slot: 0 })
                    .unwrap()
                    .launch_clip(&next.clip(), true);
                core.configure(&mut Parameters::from_config(&config, 8000));
                writer
                    .event(
                        frame,
                        EventKind::Config(crate::project::data_from_config(&config)),
                    )
                    .unwrap();
            }
            writer.audio(frame, &[[0.0; 2]]).unwrap();
            live.push(core.process([0.0; 2], &mut OfflinePages));
            if frame == 4004 {
                let view = core.view();
                crate::phrases::commit_applied(&mut config, &view.phrases);
                assert!(
                    crate::presets::note_mut(&mut config, FxTarget::Input { bank: 0, slot: 0 })
                        .unwrap()
                        .pending
                        .is_none()
                );
                core.configure(&mut Parameters::from_config(&config, 8000));
                writer
                    .event(
                        frame + 1,
                        EventKind::Config(crate::project::data_from_config(&config)),
                    )
                    .unwrap();
            }
        }
        writer.finish(8000).unwrap();
        let source = Source::open(&root).unwrap();
        let mut machine = Machine::new(source.clone()).unwrap();
        for (frame, expected) in live.iter().enumerate() {
            assert_eq!(
                machine.next().unwrap().unwrap().map(f32::to_bits),
                expected.map(f32::to_bits),
                "queued replay frame {frame}"
            );
        }
        for target in [1234, 3999, 4000, 4001, 5900] {
            let mut seek = Machine::new(source.clone()).unwrap();
            seek.advance_to(target, || false).unwrap();
            for expected in &live[target as usize..target as usize + 100] {
                assert_eq!(
                    seek.next().unwrap().unwrap().map(f32::to_bits),
                    expected.map(f32::to_bits)
                );
            }
        }
        let (imported, _) = prepare_import(source.clone(), 2000, 8000).unwrap();
        assert!(
            imported.input_fx.banks[0].slots[0]
                .osc
                .as_ref()
                .unwrap()
                .pending_clip
                .is_some(),
            "Import before the switch must preserve the pending request"
        );
        let (imported, _) = prepare_import(source.clone(), 4001, 8000).unwrap();
        let imported = imported.input_fx.banks[0].slots[0].osc.as_ref().unwrap();
        assert!(
            imported.pending_clip.is_none(),
            "Import after the audio boundary must acknowledge the applied phrase even before the UI does"
        );
        assert_eq!(imported.clip.as_ref().unwrap().events, next.clip().events);
        drop(machine);
        drop(source);
        drop(core);
        cleanup(&root);
    }
    fn fixture_at(sr: u32) -> (PathBuf, Vec<Frame>) {
        let root = PathBuf::from("var").join(format!("stream-replay-test-{}", session::id()));
        let mut config = AppConfig::new(123, 0, 5);
        for track in &mut config.track_options {
            track.quantize = crate::config::track_options::Quantize::Off;
            // Synthetic input is generated against the engine clock, rather
            // than a performer responding to the delayed monitor output.
            track.record_reference = crate::config::track_options::RecordReference::Internal;
        }
        config
            .track_fx
            .set_slot_kind(0, 0, crate::config::TrackFxKind::Delay);
        config.track_fx.tracks[0].enabled[0][0] = true;
        config.track_fx.set_slot_kind(
            0,
            1,
            crate::config::TrackFxKind::Audio(crate::config::audio_fx::AudioFxKind::Transpose),
        );
        config.track_fx.tracks[0].enabled[0][1] = true;
        if let Some(crate::config::TrackFx::Audio(fx)) = &mut config.track_fx.banks[0].slots[1].fx {
            fx.semitones = 7.0;
        }
        let mut core = RenderCore::new(sr);
        core.configure(&mut Parameters::from_config(&config, sr));
        let mut initial = AudioSnapshot::empty(sr);
        core.snapshot(&mut initial, &mut OfflinePages);
        let mut writer = Writer::begin(
            root.clone(),
            "source.json".into(),
            0,
            initial,
            crate::project::data_from_config(&config),
        )
        .unwrap();
        let mut output = Vec::new();
        for frame in 0..6000 {
            let action = match frame {
                0 | 1000 | 1800 | 2300 => Some(Action::Trigger(0)),
                4400 => Some(Action::Stop(0)),
                4600 => Some(Action::Trigger(0)),
                _ => None,
            };
            if let Some(action) = action {
                core.action(action, &mut OfflinePages);
                writer.event(frame, EventKind::Action(action)).unwrap();
            }
            if frame == 1501 {
                config.track_levels[0] = 0.43;
                core.configure(&mut Parameters::from_config(&config, sr));
                writer
                    .event(
                        frame,
                        EventKind::Config(crate::project::data_from_config(&config)),
                    )
                    .unwrap();
            }
            if frame < 1500 && frame % 50 == 0 {
                config.track_levels[0] = 0.2 + (frame % 500) as f32 / 625.0;
                core.configure(&mut Parameters::from_config(&config, sr));
                writer
                    .event(
                        frame,
                        EventKind::Config(crate::project::data_from_config(&config)),
                    )
                    .unwrap();
            }
            let input = [
                (frame as f32 * 0.063).sin() * 0.2,
                (frame as f32 * 0.029).cos() * 0.1,
            ];
            writer.audio(frame, &[input]).unwrap();
            output.push(core.process(input, &mut OfflinePages));
        }
        writer.finish(6000).unwrap();
        (root, output)
    }
    fn wait_until(mut ready: impl FnMut() -> bool) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while !ready() {
            assert!(
                std::time::Instant::now() < deadline,
                "Replay worker timed out"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    fn consume(consumer: &mut Consumer) -> Frame {
        wait_until(|| !consumer.queue.is_empty());
        consumer.next()
    }
    fn cleanup(root: &Path) {
        let root = fs::canonicalize(root).unwrap();
        let workspace_var = fs::canonicalize("var").unwrap();
        assert!(
            root.starts_with(workspace_var)
                && root
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("stream-replay-test-")
        );
        wait_until(|| fs::remove_dir_all(&root).is_ok() || !root.exists());
    }
    #[test]
    fn future_revision_packets_and_stale_cursor_publications_cannot_corrupt_seek() {
        let shared = Arc::new(Shared {
            playing: AtomicBool::new(true),
            cursor: AtomicU64::new(0),
            underruns: AtomicU64::new(0),
            failed: AtomicBool::new(false),
            revision: AtomicU64::new(0),
            ready_revision: AtomicU64::new(0),
            target: AtomicU64::new(0),
            stop: AtomicBool::new(false),
            frames: 6000,
        });
        let (mut producer, queue) = HeapRb::new(2).split();
        producer
            .push(OutputFrame {
                revision: 1,
                position: 124,
                samples: [0.125, -0.25],
                end: false,
            })
            .ok()
            .unwrap();
        let mut consumer = Consumer {
            queue,
            shared: shared.clone(),
        };
        assert_eq!(consumer.next(), [0.0; 2]);
        assert_eq!(
            consumer.queue.len(),
            1,
            "A consumer holding an old revision must preserve future packets"
        );
        shared.seek(123);
        shared.publish_cursor(1, 123, false);
        shared.ready_revision.store(1, Ordering::Release);
        // A previous callback can finish between the worker publishing the
        // target and the next callback consuming its first sought sample.
        shared.publish_cursor(0, 5999, true);
        assert_eq!(shared.position(), 123);
        assert!(!shared.ended());
        assert_eq!(consumer.next(), [0.125, -0.25]);
        assert_eq!(shared.position(), 124);
    }
    #[test]
    fn legacy_mydelay_notice_checks_both_initial_state_and_later_config_events() {
        for initial in [false, true] {
            let root = PathBuf::from("var").join(format!("stream-replay-test-{}", session::id()));
            let mut config = AppConfig::new(120, 0, 5);
            if initial {
                config
                    .input_fx
                    .set_slot_kind(0, 0, crate::config::FxKind::MyDelay);
            }
            let mut writer = Writer::begin(
                root.clone(),
                "old.json".into(),
                0,
                AudioSnapshot::empty(8000),
                crate::project::data_from_config(&config),
            )
            .unwrap();
            if !initial {
                config
                    .input_fx
                    .set_slot_kind(0, 0, crate::config::FxKind::MyDelay);
                writer
                    .event(
                        0,
                        EventKind::Config(crate::project::data_from_config(&config)),
                    )
                    .unwrap();
            }
            writer.finish(0).unwrap();
            let mut metadata = info(&root).unwrap();
            metadata.renderer = 3;
            fs::write(
                root.join("replay.json"),
                serde_json::to_vec(&metadata).unwrap(),
            )
            .unwrap();
            assert!(Source::open(&root).unwrap().legacy_mydelay);
            metadata.renderer = RENDERER_VERSION;
            fs::write(
                root.join("replay.json"),
                serde_json::to_vec(&metadata).unwrap(),
            )
            .unwrap();
            assert!(!Source::open(&root).unwrap().legacy_mydelay);
            cleanup(&root);
        }
    }
    #[test]
    fn invalid_audio_reports_error_without_advancing_position_or_restart() {
        let (root, _) = fixture();
        let mut wav =
            hound::WavWriter::create(root.join("input.wav"), session::wav_spec(8000)).unwrap();
        for frame in 0..6000 {
            wav.write_sample(if frame == 0 { f32::NAN } else { 0.0 })
                .unwrap();
            wav.write_sample(0.0f32).unwrap();
        }
        wav.finalize().unwrap();
        let mut metadata = info(&root).unwrap();
        metadata.input_sha256 = session::checksum(&root.join("input.wav")).unwrap();
        fs::write(
            root.join("replay.json"),
            serde_json::to_vec(&metadata).unwrap(),
        )
        .unwrap();
        let (mut consumer, session) = start(&root, 8000).unwrap();
        wait_until(|| session.shared.failed.load(Ordering::Acquire));
        // The worker publishes the failure flag before sending its display
        // event. Wait for both rather than racing the channel send.
        wait_until(|| {
            session
                .display
                .try_iter()
                .any(|message| matches!(message, DisplayEvent::Error(_)))
        });
        assert_eq!(session.shared.position(), 0);
        session.shared.seek(5900);
        session.shared.toggle();
        assert!(!session.shared.playing());
        assert!(!session.shared.seeking());
        assert_eq!(consumer.next(), [0.0; 2]);
        assert_eq!(session.shared.position(), 0);
        drop(session);
        drop(consumer);
        cleanup(&root);
    }
    #[test]
    fn machine_matches_live_samples_and_imports_partial_recording_at_exact_frame() {
        let (root, expected) = fixture();
        let source = Source::open(&root).unwrap();
        let mut machine = Machine::new(source.clone()).unwrap();
        machine.advance_to(673, || false).unwrap();
        assert_eq!(machine.snapshot().tracks[0].len, 673);
        assert_eq!(
            machine.core.view().tracks[0].mode,
            crate::engine::core::Mode::Recording
        );
        assert_eq!(machine.next().unwrap().unwrap(), expected[673]);
        let mut machine = Machine::new(source).unwrap();
        for (i, expected) in expected.iter().enumerate() {
            assert_eq!(machine.next().unwrap().unwrap(), *expected, "frame {i}");
        }
        assert!(machine.next().unwrap().is_none());
        assert_eq!(machine.data.track_levels[0], 0.43);
        assert!(!root.join("exports").exists());
        drop(machine);
        cleanup(&root);
    }
    #[test]
    fn realtime_player_pause_seek_restart_are_bounded_and_do_not_create_wav() {
        let (root, expected) = fixture();
        let before = fs::read_dir(&root).unwrap().count();
        let (mut consumer, session) = start(&root, 8000).unwrap();
        for expected in &expected[..700] {
            assert_eq!(consume(&mut consumer), *expected);
        }
        session.shared.playing.store(false, Ordering::Release);
        let paused = session.shared.position();
        assert_eq!(
            crate::test_alloc::count(|| {
                for _ in 0..512 {
                    assert_eq!(consumer.next(), [0.0; 2]);
                }
            }),
            0
        );
        assert_eq!(session.shared.position(), paused);
        // Multiple requests supersede each other; a paused full queue must not
        // deadlock reconstruction or leak stale samples when resumed.
        session.shared.seek(4900);
        session.shared.seek(1234);
        wait_until(|| {
            consumer.next();
            !session.shared.seeking()
        });
        assert_eq!(session.shared.position(), 1234);
        session.shared.playing.store(true, Ordering::Release);
        for expected in &expected[1234..1450] {
            assert_eq!(consume(&mut consumer), *expected);
        }
        session.shared.seek(5990);
        session.shared.playing.store(false, Ordering::Release);
        wait_until(|| {
            consumer.next();
            !session.shared.seeking()
        });
        session.shared.playing.store(true, Ordering::Release);
        for expected in &expected[5990..] {
            assert_eq!(consume(&mut consumer), *expected);
        }
        consume(&mut consumer);
        assert!(session.shared.ended());
        assert!(!session.shared.playing());
        assert_eq!(session.shared.position(), 6000);
        assert_eq!(fs::read_dir(&root).unwrap().count(), before);
        drop(consumer);
        drop(session);
        cleanup(&root);
    }
    #[test]
    fn seeking_reconstructs_rate_conversion_history_not_a_cold_filter() {
        let (root, expected) = fixture();
        let mut resampler = OutputResampler::new(8000, 16000);
        let mut cursor = 0;
        let reference: Vec<_> = (0..12000)
            .map(|_| {
                resampler.next(|| {
                    let value = expected.get(cursor).copied().unwrap_or([0.0; 2]);
                    cursor += 1;
                    value
                })
            })
            .collect();
        let (mut consumer, session) = start(&root, 16000).unwrap();
        session.shared.playing.store(false, Ordering::Release);
        session.shared.seek(1234);
        wait_until(|| {
            consumer.next();
            !session.shared.seeking()
        });
        session.shared.playing.store(true, Ordering::Release);
        for (offset, expected) in reference[2468..3000].iter().enumerate() {
            assert_eq!(
                consume(&mut consumer),
                *expected,
                "converted seek frame {offset}"
            );
        }
        drop(consumer);
        drop(session);
        cleanup(&root);
    }
    #[test]
    fn paused_downsample_seek_publishes_exact_config_and_import_leaves_source_unchanged() {
        let (root, expected) = fixture_at(16000);
        let checksum = session::checksum(&root.join("initial/manifest.json")).unwrap();
        let source = Source::open(&root).unwrap();
        let (mut consumer, session) = start(&root, 8000).unwrap();
        session.shared.playing.store(false, Ordering::Release);
        session.shared.seek(1502);
        wait_until(|| {
            consumer.next();
            !session.shared.seeking()
        });
        let revision = session.shared.revision();
        let at_target = session
            .display
            .try_iter()
            .find_map(|event| match event {
                DisplayEvent::Frame(frame) if frame.revision == revision && frame.frame == 1502 => {
                    Some(frame)
                }
                _ => None,
            })
            .expect("exact target state was published before seek completed");
        assert_eq!(at_target.data.track_levels[0], 0.43);
        assert_eq!(at_target.view.frame, 1502);
        let mut converter = OutputResampler::new(16000, 8000);
        let mut at = 0;
        let reference: Vec<_> = (0..3000)
            .map(|_| {
                converter.next(|| {
                    let value = expected.get(at).copied().unwrap_or([0.0; 2]);
                    at += 1;
                    value
                })
            })
            .collect();
        session.shared.playing.store(true, Ordering::Release);
        for expected in &reference[751..850] {
            assert_eq!(consume(&mut consumer), *expected);
        }
        let (data, core) = prepare_import(source, 673, 16000).unwrap();
        assert!(data.snapshot.is_none());
        assert_eq!(core.tracks[0].audio.len, 673);
        assert_eq!(core.tracks[0].mode, crate::engine::core::Mode::Stopped);
        assert_eq!(
            session::checksum(&root.join("initial/manifest.json")).unwrap(),
            checksum
        );
        session.shared.seek(5900);
        let shared = session.shared.clone();
        drop(session);
        assert!(shared.stop.load(Ordering::Acquire));
        drop(consumer);
        drop(core);
        cleanup(&root);
    }
}
