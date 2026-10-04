//! Device-independent renderer shared by live audio and deterministic replay.
use super::{
    clock::SampleClock,
    input_fx::{InputFxEngine, InputFxRuntime},
    loop_audio::{Frame, LoopAudio, PageAllocator},
    track_fx::{TrackFxEngine, TrackFxRuntime},
};
use crate::config::{
    AppConfig,
    track_options::{InputRouting, Quantize, StopMode, TrackOptions},
};
use serde::{Deserialize, Serialize};

pub const TRACKS: usize = 5;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Empty,
    Recording,
    Playing,
    Overdub,
    Stopped,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Action {
    Trigger(usize),
    Stop(usize),
    Clear(usize),
    Undo(usize),
    UndoStep(usize),
    RedoStep(usize),
    Metronome(bool),
    All,
    Panic,
    Preview(bool),
}

/// Allocated on the control thread. Old runtimes are moved back into the same
/// envelope and retired on the worker, never freed by the callback.
pub struct Parameters {
    pub input_patch: InputFxRuntime,
    pub external_input: InputFxRuntime,
    pub external_patch: InputFxRuntime,
    pub track_patch: TrackFxRuntime,
    pub pdc: super::pdc::LatencyPlan,
    pub master: crate::dsp::master::MasterFxRuntime,
    pub input_thru: bool,
    pub metronome_volume: f32,
    pub input: InputFxRuntime,
    pub track: TrackFxRuntime,
    pub options: [TrackOptions; TRACKS],
    pub levels: [f32; TRACKS],
    pub bpm: u32,
    pub latency_frames: usize,
    pub routing: InputRouting,
}
impl Parameters {
    pub fn from_config(config: &AppConfig, sample_rate: u32) -> Self {
        let input = InputFxRuntime::from_config(&config.input_fx);
        let track = TrackFxRuntime::from_config(&config.track_fx);
        let pdc = super::pdc::LatencyPlan::new(
            &input,
            &track,
            sample_rate as f32,
            config.input_routing,
            config.pdc_enabled,
        );
        Self {
            input_patch: input.clone(),
            external_input: input.clone(),
            external_patch: input.clone(),
            track_patch: track.clone(),
            pdc,
            master: crate::dsp::master::MasterFxRuntime::from_config(&config.master_fx),
            input_thru: config.input_thru,
            metronome_volume: config.metronome_volume,
            input,
            track,
            options: std::array::from_fn(|i| {
                config.track_options.get(i).copied().unwrap_or_default()
            }),
            levels: std::array::from_fn(|i| config.track_levels.get(i).copied().unwrap_or(1.0)),
            bpm: config.beat_config.current_bpm().clamp(30, 300) as u32,
            latency_frames: config
                .calibration
                .as_ref()
                .filter(|v| {
                    v.sample_rate == sample_rate
                        && v.input == config.system_config.input_device.value
                        && v.output == config.system_config.output_device.value
                        && v.displayed_ms == config.beat_config.current_latency()
                })
                .map(|v| v.frames.min(sample_rate / 2) as usize)
                .unwrap_or(
                    (sample_rate as u64 * config.beat_config.current_latency() as u64 / 1000)
                        as usize,
                ),
            routing: config.input_routing,
        }
    }
}

pub struct CoreTrack {
    capture_delay: usize,
    generator_capture_delay: usize,
    last_generated_source_frame: Option<u64>,
    last_record_source_frame: Option<u64>,
    pub history: super::history::AudioHistory,
    pub audio: LoopAudio,
    pub undo: LoopAudio,
    pub undo_valid: bool,
    pub undone: bool,
    pub mode: Mode,
    pub cursor: usize,
    pub gain: f32,
    play_origin: u64,
    start: u64,
    finish: Option<u64>,
    finish_stopped: bool,
    pending: Option<(u64, Action)>,
    fade: Option<(u64, u64)>,
}
impl CoreTrack {
    fn new(sr: u32) -> Self {
        Self {
            capture_delay: 0,
            generator_capture_delay: 0,
            last_generated_source_frame: None,
            history: super::history::AudioHistory::new(sr),
            last_record_source_frame: None,
            audio: LoopAudio::new(sr),
            undo: LoopAudio::new(sr),
            undo_valid: false,
            undone: false,
            mode: Mode::Empty,
            cursor: 0,
            gain: 1.0,
            start: 0,
            play_origin: 0,
            finish: None,
            finish_stopped: false,
            pending: None,
            fade: None,
        }
    }
    /// Generated samples belong to n-Li, external samples to n-(H+Li+M).
    /// The earlier writer grows the recording so clocked OSC is playable at the
    /// musical loop boundary even while delayed microphone samples are pending.
    fn write_separate_sources(
        &mut self,
        now: u64,
        external: Frame,
        generated: Frame,
        pool: &mut impl PageAllocator,
    ) -> bool {
        let input_delay = self
            .capture_delay
            .saturating_sub(self.generator_capture_delay);
        let generated_frame = now.saturating_sub(input_delay as u64);
        let generated_ready = now >= self.start.saturating_add(input_delay as u64)
            && self
                .last_generated_source_frame
                .is_none_or(|last| generated_frame > last)
            && self.finish.is_none_or(|finish| generated_frame < finish);
        if generated_ready {
            let (target, value) = if self.mode == Mode::Recording {
                let target = generated_frame.saturating_sub(self.start) as usize;
                if self.audio.len != target {
                    return false;
                }
                (target, generated)
            } else if self.audio.len > 0 {
                let target =
                    (self.cursor + self.audio.len - input_delay % self.audio.len) % self.audio.len;
                let old = self.audio.read(target);
                (target, [old[0] + generated[0], old[1] + generated[1]])
            } else {
                return false;
            };
            // Do not clamp an incomplete X/G sum: opposite-sign components can
            // cancel later, including when overdubbing close to the bus ceiling.
            if !self.audio.write(target, value, pool) {
                return false;
            }
            self.last_generated_source_frame = Some(generated_frame);
        }
        let external_frame = now.saturating_sub(self.capture_delay as u64);
        let external_ready = now >= self.start.saturating_add(self.capture_delay as u64)
            && self
                .last_record_source_frame
                .is_none_or(|last| external_frame > last)
            && self.finish.is_none_or(|finish| external_frame < finish);
        if external_ready {
            let target = if self.mode == Mode::Recording {
                external_frame.saturating_sub(self.start) as usize
            } else {
                (self.cursor + self.audio.len - self.capture_delay % self.audio.len)
                    % self.audio.len
            };
            if target >= self.audio.len {
                return false;
            }
            let old = self.audio.read(target);
            let mut value = [old[0] + external[0], old[1] + external[1]];
            // In a loop shorter than H+M, later generated cycles can already be
            // present at this index. Clamp only once its last external partner
            // arrives, rather than clipping a not-yet-cancelled partial sum.
            let complete = self.mode == Mode::Recording
                || self.generator_capture_delay < self.audio.len
                || self.finish.is_some_and(|finish| {
                    external_frame.saturating_add(self.audio.len as u64) >= finish
                });
            if complete {
                value = value.map(crate::dsp::headroom);
            }
            if !self.audio.write(target, value, pool) {
                return false;
            }
            self.last_record_source_frame = Some(external_frame);
        }
        true
    }
}

pub struct AudioSnapshot {
    pub histories: [super::history::AudioHistory; TRACKS],
    pub has_histories: bool,
    pub sample_rate: u32,
    pub at: u64,
    pub tracks: [LoopAudio; TRACKS],
    pub undo: [LoopAudio; TRACKS],
    pub undo_valid: [bool; TRACKS],
    pub undone: [bool; TRACKS],
}
impl AudioSnapshot {
    pub fn empty(sr: u32) -> Self {
        Self {
            histories: std::array::from_fn(|_| super::history::AudioHistory::new(sr)),
            has_histories: false,
            sample_rate: sr,
            at: 0,
            tracks: std::array::from_fn(|_| LoopAudio::new(sr)),
            undo: std::array::from_fn(|_| LoopAudio::new(sr)),
            undo_valid: [false; TRACKS],
            undone: [false; TRACKS],
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct TrackView {
    pub undo_depth: usize,
    pub redo_depth: usize,
    pub mode: Mode,
    pub pending: bool,
    pub frames: usize,
    pub cursor: usize,
    pub undo: bool,
    pub redo: bool,
    pub peak: f32,
    pub wave: [f32; 24],
}
#[derive(Clone, Copy)]
pub struct EngineView {
    pub input_latency_pending: bool,
    pub graph_applied_at: Option<u64>,
    pub phrases: [[crate::dsp::oscillator::PhraseView; 4]; 4],
    pub pdc_frames: usize,
    pub input_fx_latency_frames: usize,
    pub bpm: u32,
    pub metronome: bool,
    pub output_spectrum: [f32; super::spectrum::BARS],
    pub frame: u64,
    pub elapsed: u64,
    pub running: bool,
    pub sample_rate: u32,
    pub tracks: [TrackView; TRACKS],
    pub input_peak: f32,
    pub output_peak: f32,
    pub clipped: u64,
    pub exhausted: bool,
}
impl Default for EngineView {
    fn default() -> Self {
        Self {
            phrases: [[crate::dsp::oscillator::PhraseView::default(); 4]; 4],
            pdc_frames: 0,
            input_fx_latency_frames: 0,
            input_latency_pending: false,
            graph_applied_at: None,
            bpm: 120,
            metronome: false,
            output_spectrum: [0.0; super::spectrum::BARS],
            frame: 0,
            elapsed: 0,
            running: false,
            sample_rate: 48_000,
            tracks: [TrackView::default(); TRACKS],
            input_peak: 0.0,
            output_peak: 0.0,
            clipped: 0,
            exhausted: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PdcApplied {
    pub requested_at: u64,
    pub applied_at: u64,
}
struct DeferredGraph {
    input: InputFxRuntime,
    external_input: InputFxRuntime,
    track: TrackFxRuntime,
    routing: InputRouting,
    plan: super::pdc::LatencyPlan,
    requested_at: u64,
    waiting: bool,
}
impl DeferredGraph {
    fn new() -> Self {
        Self {
            input: InputFxRuntime::empty(),
            external_input: InputFxRuntime::empty(),
            track: TrackFxRuntime::empty(TRACKS),
            routing: InputRouting::Serial,
            plan: super::pdc::LatencyPlan::default(),
            requested_at: 0,
            waiting: false,
        }
    }
}
pub struct RenderCore {
    separate_recording_sources: bool,
    external_input: Box<InputFxEngine>,
    deferred_graph: Box<DeferredGraph>,
    graph_applied_at: Option<u64>,
    pdc_applied_event: Option<PdcApplied>,
    pdc: Box<super::pdc::Compensation>,
    allow_pdc: bool,
    master: crate::dsp::master::MasterFxState,
    pub input_thru: bool,
    input_monitor_gain: f32,
    pub transport: bool,
    pub metronome: bool,
    pub metronome_volume: f32,
    legacy: bool,
    pub sample_rate: u32,
    pub clock: SampleClock,
    pub tracks: [CoreTrack; TRACKS],
    pub input: InputFxEngine,
    pub track_fx: TrackFxEngine,
    pub options: [TrackOptions; TRACKS],
    pub levels: [f32; TRACKS],
    pub bpm: u32,
    pub latency: usize,
    pub preview: bool,
    pub exhausted: bool,
    input_peak: f32,
    output_peak: f32,
    track_peaks: [f32; TRACKS],
    clipped: u64,
}

impl RenderCore {
    /// Called on a non-realtime thread. All FX buffers are prepared here.
    pub fn new(sr: u32) -> Self {
        let mut input = InputFxEngine::new(sr as f32);
        input.prepare(sr as f32);
        let mut external_input = Box::new(InputFxEngine::new(sr as f32));
        external_input.set_external_only(true);
        external_input.prepare(sr as f32);
        let mut track_fx = TrackFxEngine::new(sr as f32, TRACKS);
        track_fx.prepare();
        Self {
            separate_recording_sources: true,
            external_input,
            pdc: Box::new(super::pdc::Compensation::new(sr as f32)),
            allow_pdc: true,
            deferred_graph: Box::new(DeferredGraph::new()),
            graph_applied_at: None,
            pdc_applied_event: None,
            master: crate::dsp::master::MasterFxState::new(sr as f32),
            transport: false,
            input_thru: true,
            input_monitor_gain: 1.0,
            metronome: false,
            metronome_volume: 0.35,
            legacy: false,
            sample_rate: sr,
            clock: SampleClock::default(),
            tracks: std::array::from_fn(|_| CoreTrack::new(sr)),
            input,
            track_fx,
            options: [TrackOptions::default(); TRACKS],
            levels: [1.0; TRACKS],
            bpm: 120,
            latency: 0,
            preview: false,
            exhausted: false,
            input_peak: 0.0,
            output_peak: 0.0,
            track_peaks: [0.0; TRACKS],
            clipped: 0,
        }
    }
    pub fn configure(&mut self, p: &mut Parameters) {
        let plan = if self.allow_pdc {
            p.pdc
        } else {
            super::pdc::LatencyPlan::default()
        };
        let capture_active = self
            .tracks
            .iter()
            .any(|t| matches!(t.mode, Mode::Recording | Mode::Overdub));
        if capture_active
            && (plan.input_frames != self.pdc.plan.input_frames
                || plan.output_frames != self.pdc.plan.output_frames)
        {
            self.input.patch_compatible(&mut p.input_patch);
            self.external_input.patch_compatible(&mut p.external_patch);
            self.track_fx.patch_compatible(&mut p.track_patch);
            std::mem::swap(&mut self.deferred_graph.input, &mut p.input);
            std::mem::swap(
                &mut self.deferred_graph.external_input,
                &mut p.external_input,
            );
            std::mem::swap(&mut self.deferred_graph.track, &mut p.track);
            self.deferred_graph.routing = p.routing;
            self.deferred_graph.plan = plan;
            self.deferred_graph.requested_at = self.clock.frame;
            self.deferred_graph.waiting = true;
        } else {
            self.deferred_graph.waiting = false;
            self.pdc.plan = plan;
            self.input.set_pdc(plan.enabled, plan.track_frames);
            self.external_input.set_pdc(plan.enabled, plan.track_frames);
            self.track_fx.set_pdc(plan.enabled);
            p.input = self
                .input
                .swap_runtime(std::mem::replace(&mut p.input, InputFxRuntime::empty()));
            p.external_input = self.external_input.swap_runtime(std::mem::replace(
                &mut p.external_input,
                InputFxRuntime::empty(),
            ));
            self.track_fx.exchange_runtime(&mut p.track);
            self.input.set_routing(p.routing);
            self.external_input.set_routing(p.routing);
        }
        self.master.configure(p.master);
        self.input_thru = p.input_thru;
        if self.clock.frame == 0 {
            self.input_monitor_gain = if p.input_thru { 1.0 } else { 0.0 };
        }
        self.metronome_volume = p.metronome_volume;
        self.options = p.options;
        self.levels = p.levels;
        if self.separate_recording_sources {
            // Existing passes already own fixed offsets. New passes must see a
            // hardware estimate edited while transport/another track is active.
            self.latency = p.latency_frames.min(self.sample_rate as usize / 2);
        }
        if self.idle() {
            self.bpm = p.bpm;
            if !self.separate_recording_sources {
                self.latency = p.latency_frames;
            }
        }
        self.input
            .set_clock(self.clock.origin.is_some(), self.bpm as usize);
        self.track_fx
            .set_clock(self.bpm as usize, self.clock.origin.is_some());
    }
    pub fn idle(&self) -> bool {
        if self.legacy {
            self.legacy_idle()
        } else {
            !self.transport && !self.preview && self.tracks_stopped()
        }
    }
    pub fn tracks_stopped(&self) -> bool {
        self.tracks.iter().all(|t| {
            matches!(t.mode, Mode::Empty | Mode::Stopped)
                && t.pending.is_none()
                && t.finish.is_none()
                && t.fade.is_none()
        })
    }
    pub fn legacy_renderer(&mut self, enabled: bool) {
        self.legacy = enabled;
        self.input.set_legacy_fallback(enabled);
        self.external_input.set_legacy_fallback(enabled);
        if enabled {
            self.separate_recording_sources = false;
        }
    }
    pub fn set_renderer_version(&mut self, version: u32) {
        self.legacy_renderer(version == 2);
        self.allow_pdc = version >= 5;
        self.separate_recording_sources = version >= 7;
        if !self.allow_pdc {
            self.pdc.plan = super::pdc::LatencyPlan::default();
            self.pdc.reset();
            self.input.set_pdc(false, [0; 5]);
            self.external_input.set_pdc(false, [0; 5]);
            self.track_fx.set_pdc(false);
        }
    }
    pub fn pdc_frames(&self) -> usize {
        self.pdc.plan.output_frames
    }
    pub fn input_fx_latency_frames(&self) -> usize {
        self.pdc.plan.input_frames
    }
    pub fn compensate_monitor_click(&mut self, value: f32) -> f32 {
        self.pdc
            .click
            .process([value, value], self.pdc.plan.output_frames)[0]
    }
    pub fn take_pdc_applied_event(&mut self) -> Option<PdcApplied> {
        self.pdc_applied_event.take()
    }
    fn apply_deferred_graph_if_safe(&mut self) {
        if !self.deferred_graph.waiting
            || self
                .tracks
                .iter()
                .any(|t| matches!(t.mode, Mode::Recording | Mode::Overdub) || t.finish.is_some())
        {
            return;
        }
        self.pdc.plan = self.deferred_graph.plan;
        self.input
            .set_pdc(self.pdc.plan.enabled, self.pdc.plan.track_frames);
        self.track_fx.set_pdc(self.pdc.plan.enabled);
        self.external_input
            .set_pdc(self.pdc.plan.enabled, self.pdc.plan.track_frames);
        self.deferred_graph.external_input = self.external_input.swap_runtime(std::mem::replace(
            &mut self.deferred_graph.external_input,
            InputFxRuntime::empty(),
        ));
        self.external_input.set_routing(self.deferred_graph.routing);
        self.deferred_graph.input = self.input.swap_runtime(std::mem::replace(
            &mut self.deferred_graph.input,
            InputFxRuntime::empty(),
        ));
        self.track_fx
            .exchange_runtime(&mut self.deferred_graph.track);
        self.input.set_routing(self.deferred_graph.routing);
        self.deferred_graph.waiting = false;
        self.graph_applied_at = Some(self.clock.frame);
        self.pdc_applied_event = Some(PdcApplied {
            requested_at: self.deferred_graph.requested_at,
            applied_at: self.clock.frame,
        });
    }
    pub fn legacy_idle(&self) -> bool {
        !self.preview
            && self
                .tracks
                .iter()
                .all(|t| matches!(t.mode, Mode::Empty | Mode::Stopped) && t.pending.is_none())
    }
    pub fn snapshot(&self, snapshot: &mut AudioSnapshot, pool: &mut impl PageAllocator) {
        snapshot.has_histories = true;
        snapshot.at = self.clock.frame;
        for i in 0..TRACKS {
            let track = &self.tracks[i];
            if self.legacy {
                snapshot.histories[i].clear(pool);
                if track.undo_valid {
                    let stack = if track.undone {
                        &mut snapshot.histories[i].redo
                    } else {
                        &mut snapshot.histories[i].undo
                    };
                    stack.push(&track.undo, pool);
                }
            } else {
                track.history.copy_into(&mut snapshot.histories[i], pool);
            }
            self.tracks[i]
                .audio
                .share_into(&mut snapshot.tracks[i], pool);
            self.tracks[i].undo.share_into(&mut snapshot.undo[i], pool);
            snapshot.undo_valid[i] = self.tracks[i].undo_valid;
            snapshot.undone[i] = self.tracks[i].undone;
        }
    }
    /// Swaps a prepared snapshot; the caller retires the previous pages off-thread.
    pub fn restore(&mut self, snapshot: &mut AudioSnapshot) {
        self.pdc.reset();
        for i in 0..TRACKS {
            let t = &mut self.tracks[i];
            std::mem::swap(&mut t.history, &mut snapshot.histories[i]);
            std::mem::swap(&mut t.audio, &mut snapshot.tracks[i]);
            std::mem::swap(&mut t.undo, &mut snapshot.undo[i]);
            t.undo_valid = snapshot.undo_valid[i];
            t.undone = snapshot.undone[i];
            t.mode = if t.audio.len == 0 {
                Mode::Empty
            } else {
                Mode::Stopped
            };
            t.cursor = 0;
            t.pending = None;
            t.finish = None;
            t.fade = None;
            t.gain = self.levels[i];
        }
        self.clock.origin = None;
        self.preview = false;
        self.transport = false;
        self.metronome = false;
        self.exhausted = false;
    }
    fn boundary(&self, i: usize) -> u64 {
        match self.options[i].quantize {
            Quantize::Off => self.clock.frame,
            Quantize::Beat => self.clock.next_grid(self.sample_rate, self.bpm, 1),
            Quantize::Measure => self.clock.next_grid(self.sample_rate, self.bpm, 4),
            Quantize::Loop => {
                if let Some(t) = self
                    .tracks
                    .iter()
                    .find(|t| t.audio.len > 0 && matches!(t.mode, Mode::Playing | Mode::Overdub))
                {
                    self.clock.frame + ((t.audio.len - t.cursor) % t.audio.len) as u64
                } else {
                    self.clock.next_grid(self.sample_rate, self.bpm, 4)
                }
            }
        }
    }
    pub fn action(&mut self, action: Action, pool: &mut impl PageAllocator) {
        match action {
            Action::All => {
                let stop = !self.idle();
                self.preview = false;
                self.transport = !stop && !self.legacy;
                if stop {
                    self.metronome = false;
                } else if !self.legacy {
                    self.clock.start();
                }
                for i in 0..TRACKS {
                    if stop {
                        self.action(Action::Stop(i), pool);
                    } else if self.tracks[i].audio.len > 0 {
                        self.action(Action::Trigger(i), pool);
                    }
                }
            }
            Action::Panic => {
                self.pdc.reset();
                self.preview = false;
                self.transport = false;
                self.metronome = false;
                for t in &mut self.tracks {
                    t.pending = None;
                    t.finish = None;
                    t.fade = None;
                    t.mode = if t.audio.len == 0 {
                        Mode::Empty
                    } else {
                        Mode::Stopped
                    };
                }
                self.clock.origin = None;
            }
            Action::Preview(value) => {
                self.preview = value;
                if value {
                    if self.clock.origin.is_none() {
                        for track in &mut self.tracks {
                            track.play_origin = self.clock.frame;
                        }
                    }
                    self.clock.start();
                }
            }
            Action::Metronome(enabled) => {
                self.metronome = enabled;
                if enabled {
                    self.transport = true;
                    self.clock.start();
                }
            }
            Action::UndoStep(i) | Action::RedoStep(i) if i < TRACKS => {
                let t = &mut self.tracks[i];
                let available = if matches!(action, Action::UndoStep(_)) {
                    t.history.undo.len > 0
                } else {
                    t.history.redo.len > 0
                };
                if available
                    && !matches!(t.mode, Mode::Recording | Mode::Overdub)
                    && t.finish.is_none()
                {
                    if matches!(action, Action::UndoStep(_)) {
                        t.history.undo(&mut t.audio, pool);
                    } else {
                        t.history.redo(&mut t.audio, pool);
                    }
                    if t.audio.len == 0 {
                        t.mode = Mode::Empty;
                    } else if t.mode == Mode::Empty {
                        t.mode = Mode::Stopped;
                    }
                    t.cursor %= t.audio.len.max(1);
                    t.pending = None;
                    t.fade = None;
                }
            }
            Action::Clear(i) if i < TRACKS => {
                self.pdc.reset_track(i);
                let t = &mut self.tracks[i];
                if t.audio.len > 0 && !self.legacy {
                    t.history.checkpoint(&t.audio, pool);
                }
                t.audio.clear(pool);
                t.undo.clear(pool);
                t.undo_valid = false;
                t.undone = false;
                t.mode = Mode::Empty;
                t.pending = None;
                t.finish = None;
                t.fade = None;
                t.cursor = 0;
                self.track_fx.reset_track(i);
            }
            Action::Undo(i) if i < TRACKS => {
                let t = &mut self.tracks[i];
                if t.undo_valid
                    && !matches!(t.mode, Mode::Recording | Mode::Overdub)
                    && t.finish.is_none()
                {
                    std::mem::swap(&mut t.audio, &mut t.undo);
                    t.undone = !t.undone;
                    t.cursor %= t.audio.len.max(1);
                }
            }
            Action::Trigger(i) if i < TRACKS => {
                if self.tracks[i].pending.is_some()
                    || self.tracks[i].finish.is_some()
                    || self.tracks[i].fade.is_some()
                {
                    return;
                }
                if self.clock.origin.is_none() {
                    for track in &mut self.tracks {
                        track.play_origin = self.clock.frame;
                    }
                }
                self.clock.start();
                let at = self.boundary(i);
                self.tracks[i].pending = Some((at, action));
            }
            Action::Stop(i) if i < TRACKS => {
                let t = &mut self.tracks[i];
                if t.fade.is_some() || matches!(t.pending, Some((_, Action::Stop(_)))) {
                    t.pending = None;
                    t.fade = None;
                    t.finish = None;
                    t.mode = if t.audio.len == 0 {
                        Mode::Empty
                    } else {
                        Mode::Stopped
                    };
                    return;
                }
                if t.mode == Mode::Empty || t.mode == Mode::Stopped {
                    t.pending = None;
                    return;
                }
                if t.finish.is_some() {
                    t.finish_stopped = true;
                    if t.mode == Mode::Recording {
                        let at = self.boundary(i).max(self.tracks[i].start + 1);
                        self.tracks[i].finish = Some(self.tracks[i].finish.unwrap().min(at));
                    }
                    return;
                }
                let at = if t.mode == Mode::Recording {
                    self.boundary(i)
                } else if self.options[i].stop_mode == StopMode::LoopEnd {
                    self.clock.frame + (t.audio.len.saturating_sub(t.cursor)) as u64
                } else {
                    self.clock.frame
                };
                self.tracks[i].pending = Some((at, action));
            }
            _ => {}
        }
    }
    fn execute(&mut self, i: usize, action: Action, pool: &mut impl PageAllocator) {
        let capture_delay = if !self.allow_pdc {
            self.latency
        } else {
            match self.options[i].record_reference {
                crate::config::track_options::RecordReference::External => {
                    self.latency + self.pdc.plan.input_frames + self.pdc.plan.output_frames
                }
                crate::config::track_options::RecordReference::Internal => {
                    self.pdc.plan.input_frames
                }
            }
        };
        let generator_capture_delay = if self.separate_recording_sources
            && self.options[i].record_reference
                == crate::config::track_options::RecordReference::External
        {
            capture_delay.saturating_sub(self.pdc.plan.input_frames)
        } else {
            0
        };
        let t = &mut self.tracks[i];
        match action {
            Action::Trigger(_) => match t.mode {
                Mode::Empty => {
                    if !self.legacy {
                        t.history.checkpoint(&t.audio, pool);
                    }
                    t.audio.clear(pool);
                    t.undo.clear(pool);
                    t.undo_valid = false;
                    t.undone = false;
                    t.mode = Mode::Recording;
                    t.last_record_source_frame = None;
                    t.last_generated_source_frame = None;
                    t.capture_delay = capture_delay;
                    t.generator_capture_delay = generator_capture_delay;
                    t.start = self.clock.frame;
                    t.finish_stopped = false;
                    t.finish = if self.options[i].measures > 0 {
                        Some(
                            t.start
                                + SampleClock::beats_length(
                                    self.sample_rate,
                                    self.bpm,
                                    self.options[i].measures as u64 * 4,
                                ),
                        )
                    } else {
                        None
                    };
                }
                Mode::Recording => {
                    let minimum = if self.options[i].quantize == Quantize::Off {
                        1
                    } else {
                        SampleClock::beats_length(self.sample_rate, self.bpm, 1)
                    };
                    t.finish = Some(self.clock.frame.max(t.start + minimum));
                }
                Mode::Stopped => {
                    t.cursor =
                        if self.options[i].one_shot || self.options[i].quantize == Quantize::Off {
                            0
                        } else {
                            (self.clock.elapsed() as usize) % t.audio.len.max(1)
                        };
                    t.mode = Mode::Playing;
                    t.play_origin = self.clock.frame.saturating_sub(t.cursor as u64);
                    t.fade = None;
                }
                Mode::Playing if self.options[i].one_shot => {
                    t.cursor = 0;
                    t.play_origin = self.clock.frame;
                    t.fade = None;
                }
                Mode::Playing if !self.options[i].reverse => {
                    if !self.legacy {
                        t.history.checkpoint(&t.audio, pool);
                    }
                    t.audio.share_into(&mut t.undo, pool);
                    t.undo_valid = true;
                    t.undone = false;
                    t.mode = Mode::Overdub;
                    t.last_record_source_frame = None;
                    t.last_generated_source_frame = None;
                    t.capture_delay = capture_delay;
                    t.generator_capture_delay = generator_capture_delay;
                    t.start = self.clock.frame;
                }
                Mode::Overdub => {
                    t.finish = Some(self.clock.frame);
                    t.finish_stopped = false;
                }
                _ => {}
            },
            Action::Stop(_) => {
                if matches!(t.mode, Mode::Recording | Mode::Overdub) {
                    t.finish = Some(self.clock.frame.max(t.start + 1));
                    t.finish_stopped = true;
                } else if self.options[i].stop_mode == StopMode::Fade {
                    let length =
                        (self.options[i].fade_ms as u64 * self.sample_rate as u64 / 1000).max(1);
                    t.fade = Some((self.clock.frame, length));
                } else {
                    t.mode = Mode::Stopped;
                    t.fade = None;
                }
            }
            _ => {}
        }
    }
    pub fn process(&mut self, input: Frame, pool: &mut impl PageAllocator) -> Frame {
        let input = input.map(crate::dsp::headroom);
        for i in 0..TRACKS {
            if self.tracks[i]
                .pending
                .is_some_and(|(at, _)| at <= self.clock.frame)
            {
                let (_, action) = self.tracks[i].pending.take().unwrap();
                self.execute(i, action, pool);
            }
            let t = &mut self.tracks[i];
            let recording_delay = t.capture_delay;
            if t.finish
                .is_some_and(|at| self.clock.frame >= at + recording_delay as u64)
            {
                if self.separate_recording_sources
                    && t.generator_capture_delay > 0
                    && (t.last_record_source_frame != Some(t.finish.unwrap() - 1)
                        || t.last_generated_source_frame != Some(t.finish.unwrap() - 1))
                {
                    self.exhausted = true;
                    t.finish_stopped = true;
                }
                if t.mode == Mode::Recording {
                    let target = t.finish.unwrap().saturating_sub(t.start) as usize;
                    // Capture's reference/latency is fixed for the pass. A mismatch
                    // is an explicit failed take, never fabricated silent padding.
                    if t.audio.len != target {
                        self.exhausted = true;
                        t.finish_stopped = true;
                    }
                    t.cursor = recording_delay % t.audio.len.max(1);
                    t.play_origin = self.clock.frame.saturating_sub(t.cursor as u64);
                }
                t.mode = if t.audio.len == 0 {
                    Mode::Empty
                } else if t.finish_stopped {
                    Mode::Stopped
                } else {
                    Mode::Playing
                };
                t.finish = None;
            }
        }
        self.apply_deferred_graph_if_safe();
        if self.idle() {
            self.clock.origin = None;
        }
        self.input
            .set_clock(self.clock.origin.is_some(), self.bpm as usize);
        self.track_fx
            .set_clock(self.bpm as usize, self.clock.origin.is_some());
        let mut carriers = [None; TRACKS];
        // Track vocoders read a common pre-FX snapshot. This avoids processing-order
        // dependence or an instantaneous feedback graph between track effects.
        let raw_carriers = std::array::from_fn(|i| {
            let t = &self.tracks[i];
            let audible = matches!(t.mode, Mode::Playing | Mode::Overdub);
            if t.audio.len == 0
                || t.mode == Mode::Recording
                || (!audible && self.clock.origin.is_none())
            {
                return None;
            }
            let pos = if audible {
                t.cursor
            } else {
                self.clock.frame.saturating_sub(t.play_origin) as usize % t.audio.len
            };
            let read = if self.options[i].reverse {
                t.audio.len - 1 - pos
            } else {
                pos
            };
            let f = t.audio.read(read);
            Some((f[0], f[1]))
        });
        self.track_fx
            .set_vocoder_sources((input[0], input[1]), raw_carriers);
        self.track_fx
            .set_transport_elapsed(self.clock.elapsed() as f64 / self.sample_rate as f64);
        let mut mixed = [0.0; 2];
        for i in 0..TRACKS {
            let t = &mut self.tracks[i];
            // The recorded prefix can loop at the musical end boundary while the
            // final H+Li capture samples drain. Waiting to start playback until the
            // tail is stored would otherwise create an extra plugin-sized gap.
            let finishing = (self.pdc.plan.enabled || self.separate_recording_sources)
                && t.mode == Mode::Recording
                && !t.finish_stopped
                && !self.options[i].reverse
                && t.finish.is_some_and(|at| self.clock.frame >= at);
            let audible = matches!(t.mode, Mode::Playing | Mode::Overdub) || finishing;
            let available = t.audio.len > 0 && (t.mode != Mode::Recording || finishing);
            let loop_len = if finishing {
                t.finish.unwrap().saturating_sub(t.start) as usize
            } else {
                t.audio.len
            };
            let source_running = audible || self.clock.origin.is_some();
            let position = if finishing {
                self.clock.frame.saturating_sub(t.finish.unwrap()) as usize % loop_len.max(1)
            } else if audible && available {
                t.cursor
            } else if available {
                self.clock.frame.saturating_sub(t.play_origin) as usize % t.audio.len
            } else {
                0
            };
            let read = if available && self.options[i].reverse {
                loop_len - 1 - position
            } else {
                position
            };
            let dry = if available && source_running {
                t.audio.read(read)
            } else {
                [0.0; 2]
            };
            t.gain +=
                (self.levels[i] - t.gain) * (1.0 / (0.005 * self.sample_rate as f32)).min(1.0);
            let fade = t
                .fade
                .map(|(start, len)| 1.0 - ((self.clock.frame - start) as f32 / len as f32).min(1.0))
                .unwrap_or(1.0);
            if fade <= 0.0 {
                t.mode = Mode::Stopped;
                t.fade = None;
            }
            // Fader/stop controls belong to the same source frame as the audio.
            // Delay them by the rack latency before adding the remaining PDC tap.
            let gain = if audible { t.gain * fade } else { 0.0 };
            let gain = self.pdc.gains[i].process([gain, 0.0], self.pdc.plan.track_frames[i])[0];
            let wet = if available && (source_running || gain.abs() > 1e-8) {
                self.track_fx.process_frame(
                    i,
                    position as f64 / self.sample_rate as f64,
                    dry[0],
                    dry[1],
                )
            } else {
                (0.0, 0.0)
            };
            if available && source_running {
                carriers[i] = Some(wet);
            }
            let aligned = self.pdc.tracks[i].process(
                [wet.0 * gain, wet.1 * gain],
                self.pdc
                    .plan
                    .output_frames
                    .saturating_sub(self.pdc.plan.track_frames[i]),
            );
            mixed[0] += aligned[0];
            mixed[1] += aligned[1];
            if audible || gain.abs() > 1e-8 {
                self.track_peaks[i] = self.track_peaks[i].max(wet.0.abs().max(wet.1.abs()) * gain);
            }
        }
        let elapsed = self.clock.elapsed() as f64 / self.sample_rate as f64;
        let (left, right) = self
            .input
            .process_frame(elapsed, input[0], input[1], &carriers);
        // Keep this chain warm even at H=0: enabling compensation later must not
        // mistake an existing external effect tail for generated audio.
        let external = if self.separate_recording_sources {
            self.external_input
                .set_clock(self.clock.origin.is_some(), self.bpm as usize);
            let value = self
                .external_input
                .process_frame(elapsed, input[0], input[1], &carriers);
            value
        } else {
            (left, right)
        };
        for i in 0..TRACKS {
            let t = &mut self.tracks[i];
            let mut ok = true;
            let recording_delay = t.capture_delay;
            let recorded = [left, right];
            let split = self.separate_recording_sources && t.generator_capture_delay > 0;
            let source_frame = self.clock.frame.saturating_sub(recording_delay as u64);
            let ready = self.clock.frame >= t.start.saturating_add(recording_delay as u64)
                && t.last_record_source_frame
                    .is_none_or(|last| source_frame > last)
                && t.finish.is_none_or(|finish| source_frame < finish);
            if split && matches!(t.mode, Mode::Recording | Mode::Overdub) {
                ok = t.write_separate_sources(
                    self.clock.frame,
                    [external.0, external.1],
                    [left - external.0, right - external.1],
                    pool,
                );
            } else if t.mode == Mode::Recording && ready {
                let target = source_frame.saturating_sub(t.start) as usize;
                if t.audio.len != target {
                    ok = false;
                }
                if ok {
                    ok = t.audio.write(target, recorded, pool);
                    t.last_record_source_frame = Some(source_frame);
                }
            } else if t.mode == Mode::Overdub && ready && t.audio.len > 0 {
                let write = (t.cursor + t.audio.len - recording_delay % t.audio.len) % t.audio.len;
                let old = t.audio.read(write);
                ok = t.audio.write(
                    write,
                    [
                        if self.legacy {
                            (old[0] + recorded[0]).clamp(-1.0, 1.0)
                        } else {
                            crate::dsp::headroom(old[0] + recorded[0])
                        },
                        if self.legacy {
                            (old[1] + recorded[1]).clamp(-1.0, 1.0)
                        } else {
                            crate::dsp::headroom(old[1] + recorded[1])
                        },
                    ],
                    pool,
                );
                t.last_record_source_frame = Some(source_frame);
            }
            if !ok {
                self.exhausted = true;
                t.mode = if t.audio.len > 0 {
                    Mode::Stopped
                } else {
                    Mode::Empty
                };
                t.finish = None;
            }
            if matches!(t.mode, Mode::Playing | Mode::Overdub) && t.audio.len > 0 {
                t.cursor += 1;
                if t.cursor >= t.audio.len {
                    t.cursor = 0;
                    if self.options[i].one_shot {
                        t.mode = Mode::Stopped;
                    }
                }
            }
        }
        // INPUT THRU gates only the monitor branch, after the track writers.
        // A short linear ramp avoids clicks; input processing and record levels stay unchanged.
        let target = if self.input_thru { 1.0 } else { 0.0 };
        let step = 1.0 / (0.005 * self.sample_rate as f32).max(1.0);
        self.input_monitor_gain += (target - self.input_monitor_gain).clamp(-step, step);
        let input_gain = self
            .pdc
            .input_gain
            .process([self.input_monitor_gain, 0.0], self.pdc.plan.input_frames)[0];
        let monitored = self.pdc.input.process(
            [left * input_gain, right * input_gain],
            self.pdc
                .plan
                .output_frames
                .saturating_sub(self.pdc.plan.input_frames),
        );
        mixed[0] += monitored[0];
        mixed[1] += monitored[1];
        mixed = self.master.process(mixed);
        mixed = mixed.map(crate::dsp::headroom);
        self.input_peak = self.input_peak.max(input[0].abs().max(input[1].abs()));
        self.output_peak = self.output_peak.max(mixed[0].abs().max(mixed[1].abs()));
        if mixed.iter().any(|v| v.abs() > 1.0) {
            self.clipped += 1;
        }
        self.clock.frame += 1;
        [mixed[0].clamp(-1.0, 1.0), mixed[1].clamp(-1.0, 1.0)]
    }
    pub fn view(&mut self) -> EngineView {
        let tracks = std::array::from_fn(|i| {
            let t = &self.tracks[i];
            TrackView {
                mode: t.mode,
                pending: t.pending.is_some() || t.finish.is_some() || t.fade.is_some(),
                frames: t.audio.len,
                cursor: t.cursor,
                undo: !matches!(t.mode, Mode::Recording | Mode::Overdub)
                    && t.finish.is_none()
                    && if self.legacy {
                        t.undo_valid && !t.undone
                    } else {
                        t.history.undo.len > 0
                    },
                redo: !matches!(t.mode, Mode::Recording | Mode::Overdub)
                    && t.finish.is_none()
                    && if self.legacy {
                        t.undo_valid && t.undone
                    } else {
                        t.history.redo.len > 0
                    },
                undo_depth: t.history.undo.len,
                redo_depth: t.history.redo.len,
                peak: self.track_peaks[i],
                wave: std::array::from_fn(|bin| {
                    let start = bin * t.audio.len / 24;
                    (0..8)
                        .map(|j| t.audio.read(start + j * t.audio.len / (24 * 8)))
                        .fold(0.0f32, |v, s| v.max(s[0].abs()).max(s[1].abs()))
                }),
            }
        });
        let view = EngineView {
            input_latency_pending: self.deferred_graph.waiting,
            graph_applied_at: self.graph_applied_at,
            phrases: self.input.phrase_views(),
            pdc_frames: self.pdc.plan.output_frames,
            input_fx_latency_frames: self.pdc.plan.input_frames,
            bpm: self.bpm,
            metronome: self.metronome,
            output_spectrum: [0.0; super::spectrum::BARS],
            frame: self.clock.frame,
            elapsed: self.clock.elapsed(),
            running: self.clock.origin.is_some(),
            sample_rate: self.sample_rate,
            tracks,
            input_peak: self.input_peak,
            output_peak: self.output_peak,
            clipped: self.clipped,
            exhausted: self.exhausted,
        };
        self.input_peak = 0.0;
        self.output_peak = 0.0;
        self.track_peaks.fill(0.0);
        view
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pdc_aligns_serial_pitch_bypass_parallel_tracks_input_and_click() {
        use crate::config::{TrackFx, TrackFxKind, audio_fx::AudioFxKind};
        let sr = 8000;
        let window = crate::dsp::pitch_shift::latency_frames(sr as f32);
        let mut c = AppConfig::new(120, 0, 5);
        c.pdc_enabled = true;
        for slot in 0..2 {
            c.track_fx
                .set_slot_kind(0, slot, TrackFxKind::Audio(AudioFxKind::Transpose));
            if let Some(TrackFx::Audio(p)) = c.track_fx.slot_fx_mut(0, slot) {
                p.semitones = 0.0;
            }
            c.track_fx.tracks[0].enabled[0][slot] = true;
        }
        let mut engine = RenderCore::new(sr);
        engine.configure(&mut Parameters::from_config(&c, sr));
        for track in 0..2 {
            for n in 0..4000 {
                let value = if n == 0 { 0.1 } else { 0.0 };
                engine.tracks[track]
                    .audio
                    .write(n, [value, -value], &mut OfflinePages);
            }
            engine.tracks[track].mode = Mode::Playing;
        }
        engine.clock.start();
        assert_eq!(engine.pdc_frames(), window * 2);
        assert_eq!(engine.input_fx_latency_frames(), 0);
        for n in 0..window * 3 {
            let input = if n == 0 { [0.1, -0.1] } else { [0.0; 2] };
            let y = engine.process(input, &mut OfflinePages);
            let click = engine.compensate_monitor_click(if n == 0 { 0.2 } else { 0.0 });
            let expected = if n == window * 2 {
                [0.3, -0.3]
            } else {
                [0.0; 2]
            };
            assert!(
                (y[0] - expected[0]).abs() < 0.0001 && (y[1] - expected[1]).abs() < 0.0001,
                "frame{n}: {y:?}"
            );
            assert_eq!(click, if n == window * 2 { 0.2 } else { 0.0 });
        }
    }
    #[test]
    fn pdc_recording_removes_input_window_but_keeps_physical_calibration_independent() {
        use crate::config::{FxKind, InputFx, audio_fx::AudioFxKind};
        let sr = 8000;
        let hardware = 64;
        let window = crate::dsp::pitch_shift::latency_frames(sr as f32);
        let mut c = AppConfig::new(120, 8, 5);
        c.pdc_enabled = true;
        c.input_thru = false;
        c.track_options[0].quantize = Quantize::Off;
        c.input_fx
            .set_slot_kind(0, 0, FxKind::Audio(AudioFxKind::Transpose));
        c.input_fx.banks[0].slots[0].is_enabled = true;
        if let Some(InputFx::Audio(p)) = &mut c.input_fx.banks[0].slots[0].fx {
            p.semitones = 0.0;
        }
        let mut engine = RenderCore::new(sr);
        engine.configure(&mut Parameters::from_config(&c, sr));
        assert_eq!(engine.latency, hardware);
        assert_eq!(engine.input_fx_latency_frames(), window);
        engine.action(Action::Trigger(0), &mut OfflinePages);
        let signal = |n: usize| (n as f32 * 0.071).sin() * 0.2;
        let offset = hardware + window * 2;
        for n in 0..4000 + offset + 1 {
            if n == 1500 {
                c.input_fx.banks[0].slots[0].is_enabled = false;
                engine.configure(&mut Parameters::from_config(&c, sr));
            }
            if n == 4000 {
                engine.action(Action::Trigger(0), &mut OfflinePages);
            }
            let arrival = hardware + window;
            let x = if n >= arrival {
                signal(n - arrival)
            } else {
                0.0
            };
            engine.process([x, -x], &mut OfflinePages);
        }
        assert_eq!(engine.tracks[0].audio.len, 4000);
        assert_eq!(engine.tracks[0].mode, Mode::Playing);
        for n in 0..4000 {
            let y = engine.tracks[0].audio.read(n);
            assert!(
                (y[0] - signal(n)).abs() < 0.0005,
                "Record frame{n}: {} vs{}",
                y[0],
                signal(n)
            );
        }
    }
    #[test]
    fn pdc_old_renderer_and_legacy_json_stay_opt_out() {
        use crate::config::{FxKind, audio_fx::AudioFxKind};
        let mut c = AppConfig::new(120, 0, 5);
        c.input_fx
            .set_slot_kind(0, 0, FxKind::Audio(AudioFxKind::Transpose));
        let mut core = RenderCore::new(8000);
        core.set_renderer_version(4);
        core.configure(&mut Parameters::from_config(&c, 8000));
        assert_eq!(core.pdc_frames(), 0);
        let mut json = serde_json::to_value(crate::project::data_from_config(&c)).unwrap();
        json.as_object_mut().unwrap().remove("pdc_enabled");
        let data = serde_json::from_value(json).unwrap();
        crate::project::apply_data_to_config(&mut c, data);
        assert!(!c.pdc_enabled);
    }
    #[test]
    fn pdc_finishing_recording_starts_the_loop_on_its_audible_boundary() {
        use crate::config::{FxKind, audio_fx::AudioFxKind};
        let sr = 8000;
        let window = crate::dsp::pitch_shift::latency_frames(sr as f32);
        let length = 2000;
        let mut c = AppConfig::new(120, 8, 5);
        c.input_thru = false;
        c.track_options[0].quantize = Quantize::Off;
        c.track_options[0].record_reference =
            crate::config::track_options::RecordReference::Internal;
        c.input_fx
            .set_slot_kind(0, 0, FxKind::Audio(AudioFxKind::Transpose));
        c.input_fx.banks[0].slots[0].is_enabled = true;
        let mut core = RenderCore::new(sr);
        core.configure(&mut Parameters::from_config(&c, sr));
        core.action(Action::Trigger(0), &mut OfflinePages);
        for n in 0..length * 2 + window {
            if n == length {
                core.action(Action::Trigger(0), &mut OfflinePages);
            }
            let sample = if n == 0 || n == length - 1 { 0.2 } else { 0.0 };
            let y = core.process([sample; 2], &mut OfflinePages);
            if n >= length + window {
                let expected = if n == length + window || n == length * 2 + window - 1 {
                    0.2
                } else {
                    0.0
                };
                assert!(
                    (y[0] - expected).abs() < 0.0001,
                    "frame{n}:{} vs{expected}",
                    y[0]
                );
            }
        }
        assert_eq!(core.tracks[0].audio.len, length);
    }
    #[test]
    fn pdc_external_performer_on_audible_beat_records_at_zero_not_at_m() {
        use crate::config::{FxKind, audio_fx::AudioFxKind};
        let sr = 8000;
        let h = 64;
        let window = crate::dsp::pitch_shift::latency_frames(sr as f32);
        let start = 4000;
        let finish = 8000;
        let mut c = AppConfig::new(120, 8, 5);
        c.input_thru = false;
        c.input_fx
            .set_slot_kind(0, 0, FxKind::Audio(AudioFxKind::Transpose));
        c.input_fx.banks[0].slots[0].is_enabled = true;
        let mut core = RenderCore::new(sr);
        core.configure(&mut Parameters::from_config(&c, sr));
        core.action(Action::Metronome(true), &mut OfflinePages);
        let mut metronome = crate::engine::metronome::Metronome::new(sr);
        let mut click_peak = 0.0f32;
        for n in 0..finish + h + window * 2 + 1 {
            if n == 2000 {
                core.action(Action::Trigger(0), &mut OfflinePages);
            }
            if n == 6000 {
                core.action(Action::Trigger(0), &mut OfflinePages);
            }
            let click = metronome.next(core.metronome, core.clock.elapsed(), sr, 120, 1.0);
            let x = if n == start + window + h { 0.2 } else { 0.0 };
            core.process([x; 2], &mut OfflinePages);
            let click = core.compensate_monitor_click(click);
            if n >= start + window && n < start + window + 280 {
                click_peak = click_peak.max(click.abs());
            }
        }
        assert!(click_peak > 0.1);
        assert_eq!(core.tracks[0].audio.len, finish - start);
        assert!((core.tracks[0].audio.read(0)[0] - 0.2).abs() < 0.0001);
        assert!(
            core.tracks[0].audio.read(window)[0].abs() < 0.0001,
            "A listener's downbeat must not be stored M frames late"
        );
    }
    #[test]
    fn pdc_capture_defers_structural_latency_but_keeps_controls_live_without_holes() {
        use crate::config::{
            FxKind, InputFx, audio_fx::AudioFxKind, track_options::RecordReference,
        };
        let sr = 8000;
        let window = crate::dsp::pitch_shift::latency_frames(sr as f32);
        let mut c = AppConfig::new(120, 0, 5);
        c.input_thru = false;
        c.track_options[0].quantize = Quantize::Off;
        c.track_options[0].record_reference = RecordReference::Internal;
        c.input_fx
            .set_slot_kind(0, 0, FxKind::Audio(AudioFxKind::Transpose));
        c.input_fx.banks[0].slots[0].is_enabled = true;
        let mut core = RenderCore::new(sr);
        core.configure(&mut Parameters::from_config(&c, sr));
        core.action(Action::Trigger(0), &mut OfflinePages);
        for n in 0..4000 + window + 1 {
            if n == 1500 || n == 1800 || n == 2000 {
                c.input_fx.select_bank(1);
                if n == 1800 {
                    if let Some(InputFx::Audio(p)) = &mut c.input_fx.banks[0].slots[0].fx {
                        p.level_db = -6.0206;
                    }
                    c.track_levels[1] = 0.25;
                }
                if n == 2000 {
                    c.input_fx.banks[0].slots[0].is_enabled = false;
                }
                let mut parameters = Parameters::from_config(&c, sr);
                assert_eq!(
                    crate::test_alloc::count(|| core.configure(&mut parameters)),
                    0
                );
                assert!(core.view().input_latency_pending);
                assert_eq!(core.input_fx_latency_frames(), window);
                if n == 1800 {
                    assert_eq!(core.levels[1], 0.25);
                }
            }
            if n == 4000 {
                core.action(Action::Trigger(0), &mut OfflinePages);
            }
            core.process([0.2; 2], &mut OfflinePages);
        }
        assert!(!core.view().input_latency_pending);
        assert_eq!(core.input_fx_latency_frames(), 0);
        assert_eq!(
            core.take_pdc_applied_event(),
            Some(PdcApplied {
                requested_at: 2000,
                applied_at: (4000 + window) as u64
            })
        );
        assert_eq!(core.tracks[0].audio.len, 4000);
        for n in 0..4000 {
            let x = core.tracks[0].audio.read(n)[0];
            assert!(x >= 0.099 && x <= 0.201, "silent hole at{n}: {x}");
        }
        assert!(
            core.tracks[0].audio.read(1450)[0] < 0.13,
            "compatible trim change was frozen"
        );
        assert!(
            core.tracks[0].audio.read(1800)[0] > 0.19,
            "compatible bypass change was frozen"
        );
    }
    #[test]
    #[ignore = "manual process-memory measurement at prepared sample rates"]
    fn benchmark_prepared_core_memory() {
        for sr in [48000, 192000] {
            let start = std::time::Instant::now();
            let core = Box::new(RenderCore::new(sr));
            println!(
                "PREPARED_CORE {sr} {:.3} ms",
                start.elapsed().as_secs_f64() * 1000.0
            );
            std::thread::sleep(std::time::Duration::from_millis(800));
            std::hint::black_box(&core);
        }
    }
    #[test]
    #[ignore = "manual release callback stress; no soundcard deadline guarantee"]
    fn benchmark_pdc_formants_unity_feedback_bass_callbacks() {
        use crate::config::{
            FxKind, InputFx, TrackFx, TrackFxKind, audio_fx::AudioFxKind as K,
            track_options::RecordReference,
        };
        let sr = 48000;
        let mut c = AppConfig::new(120, 0, 5);
        c.pdc_enabled = true;
        c.input_thru = true;
        for (slot, kind) in [K::Delay, K::Transpose, K::PanningDelay, K::Equalizer]
            .into_iter()
            .enumerate()
        {
            c.input_fx.set_slot_kind(0, slot, FxKind::Audio(kind));
            c.input_fx.banks[0].slots[slot].is_enabled = true;
            if let Some(InputFx::Audio(p)) = &mut c.input_fx.banks[0].slots[slot].fx {
                p.time_ms = if slot == 2 { 1.01 } else { 1.0 };
                p.feedback = 1.0;
                p.feedback_repeats = 0;
                p.semitones = 3.0;
                p.high_cut_hz = 0.0;
                p.low_cut_hz = 0.0;
                p.effect_level = 0.4;
            }
        }
        for (slot, kind) in [K::Transpose, K::PanningDelay, K::Electric, K::Reverb]
            .into_iter()
            .enumerate()
        {
            c.track_fx.set_slot_kind(0, slot, TrackFxKind::Audio(kind));
            if let Some(TrackFx::Audio(p)) = c.track_fx.slot_fx_mut(0, slot) {
                p.semitones = 3.0;
                p.preserve_formants = matches!(kind, K::Transpose | K::Electric);
                p.time_ms = 1.0;
                p.feedback_repeats = 0;
                p.feedback = 1.0;
                p.high_cut_hz = 0.0;
                p.low_cut_hz = 0.0;
                p.mix = 0.7;
            }
            for track in 0..5 {
                c.track_fx.tracks[track].enabled[0][slot] = true;
                c.track_levels[track] = 0.1;
                c.track_options[track].quantize = Quantize::Off;
                c.track_options[track].record_reference = RecordReference::Internal;
            }
        }
        let mut core = RenderCore::new(sr);
        core.configure(&mut Parameters::from_config(&c, sr));
        for track in 0..5 {
            for n in 0..4800 {
                let x = (std::f32::consts::TAU * (60.0 + track as f32 * 20.0) * n as f32
                    / sr as f32)
                    .sin()
                    * 0.2;
                core.tracks[track]
                    .audio
                    .write(n, [x, x * 0.8], &mut OfflinePages);
            }
            core.tracks[track].mode = Mode::Playing;
        }
        core.clock.start();
        core.action(Action::Trigger(4), &mut OfflinePages);
        let input =
            |n: usize| [(std::f32::consts::TAU * 50.0 * n as f32 / sr as f32).sin() * 0.9; 2];
        for n in 0..24000 {
            core.process(input(n), &mut OfflinePages);
        }
        c.input_fx.select_bank(1);
        c.track_levels[2] = 0.07;
        let mut pending = Parameters::from_config(&c, sr);
        let block_frames = std::env::var("RC505_BENCH_BLOCK")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|n| matches!(n, 128 | 256 | 512))
            .unwrap_or(128);
        let blocks = 144000usize.div_ceil(block_frames);
        let mut timings = Vec::with_capacity(blocks);
        let mut checksum = 0.0f64;
        let allocations = crate::test_alloc::count(|| {
            for block in 0..blocks {
                let before = std::time::Instant::now();
                if block == 64000 / block_frames {
                    core.configure(&mut pending);
                }
                for offset in 0..block_frames {
                    let n = 24000 + block * block_frames + offset;
                    let output = core.process(input(n), &mut OfflinePages);
                    assert!(output.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
                    checksum += output[0] as f64;
                }
                timings.push(before.elapsed().as_secs_f64() * 1000.0);
            }
        });
        assert_eq!(allocations, 0);
        assert!(core.view().input_latency_pending);
        let total = timings.iter().sum::<f64>();
        timings.sort_by(f64::total_cmp);
        println!(
            "PDC + 5-track FX + 1ms/100% feedback bass + formants + overdub + pending graph: {:.4}s audio CPU {total:.2}ms; {block_frames}f p95 {:.3} p99 {:.3} max {:.3}ms; allocations {allocations}; checksum {checksum}",
            blocks as f64 * block_frames as f64 / sr as f64,
            timings[timings.len() * 95 / 100],
            timings[timings.len() * 99 / 100],
            timings[timings.len() - 1]
        );
    }
    use crate::engine::loop_audio::{OfflinePages, PAGE_FRAMES, Page};
    fn core() -> RenderCore {
        let mut core = RenderCore::new(8000);
        core.latency = 0;
        for options in &mut core.options {
            options.quantize = Quantize::Off;
        }
        core
    }
    #[test]
    fn input_thru_off_records_processed_input_without_monitoring_or_stopping_loops() {
        let mut config = AppConfig::new(120, 0, 5);
        config.input_thru = false;
        config.track_options[0].quantize = Quantize::Off;
        let mut core = core();
        core.configure(&mut Parameters::from_config(&config, 8000));
        core.action(Action::Trigger(0), &mut OfflinePages);
        for _ in 0..100 {
            assert_eq!(core.process([0.25, -0.125], &mut OfflinePages), [0.0; 2]);
        }
        assert_eq!(core.tracks[0].audio.read(30), [0.25, -0.125]);
        core.action(Action::Trigger(0), &mut OfflinePages);
        assert_eq!(core.process([0.8; 2], &mut OfflinePages), [0.25, -0.125]);
        assert_eq!(core.tracks[0].mode, Mode::Playing);
        core.action(Action::Trigger(0), &mut OfflinePages);
        assert_eq!(core.process([0.1; 2], &mut OfflinePages), [0.25, -0.125]);
        let overdub = core.tracks[0].audio.read(1);
        assert!((overdub[0] - 0.35).abs() < 1e-6 && (overdub[1] + 0.025).abs() < 1e-6);
        core.input_thru = true;
        for _ in 0..50 {
            core.process([0.0; 2], &mut OfflinePages);
        }
        assert_eq!(core.input_monitor_gain, 1.0);
    }
    #[test]
    fn float_recording_and_overdub_keep_headroom_until_fader_and_master() {
        use crate::config::{FxKind, InputFx, audio_fx::AudioFxKind};
        let mut config = AppConfig::new(120, 0, 5);
        config.input_thru = false;
        config.track_options[0].quantize = Quantize::Off;
        config
            .input_fx
            .set_slot_kind(0, 0, FxKind::Audio(AudioFxKind::Pan));
        config.input_fx.banks[0].slots[0].is_enabled = true;
        if let Some(InputFx::Audio(p)) = &mut config.input_fx.banks[0].slots[0].fx {
            p.level_db = 12.0;
        }
        let mut core = core();
        core.configure(&mut Parameters::from_config(&config, 8000));
        core.action(Action::Trigger(0), &mut OfflinePages);
        for _ in 0..1000 {
            core.process([0.4, -0.4], &mut OfflinePages);
        }
        let recorded = core.tracks[0].audio.read(900)[0];
        assert!(
            recorded > 1.5,
            "input rack clipped the recording: {recorded}"
        );
        core.action(Action::Trigger(0), &mut OfflinePages);
        config.track_levels[0] = 0.25;
        core.configure(&mut Parameters::from_config(&config, 8000));
        let mut out = [0.0; 2];
        for _ in 0..1900 {
            out = core.process([0.0; 2], &mut OfflinePages);
        }
        assert!(
            out[0] > 0.38 && out[0] < 0.41,
            "lower fader must recover the unclipped signal: {:?}",
            out
        );
        core.action(Action::Trigger(0), &mut OfflinePages);
        for _ in 0..1000 {
            core.process([0.4, -0.4], &mut OfflinePages);
        }
        let doubled = core.tracks[0].audio.read(900)[0];
        assert!(doubled > 3.0, "overdub clipped: {doubled}");
        core.action(Action::Trigger(0), &mut OfflinePages);
        config.track_levels[0] = 1.0;
        config.master_fx.compressor_enabled = true;
        config.master_fx.compressor.threshold_db = -20.0;
        config.master_fx.compressor.ratio = 20.0;
        config.master_fx.compressor.attack_ms = 0.1;
        core.configure(&mut Parameters::from_config(&config, 8000));
        for _ in 0..3000 {
            out = core.process([0.0; 2], &mut OfflinePages);
        }
        assert!(
            out[0] > 0.1 && out[0] < 0.14,
            "master receives unclipped floats: {:?}",
            out
        );
        assert!(
            core.process([f32::NAN, f32::INFINITY], &mut OfflinePages)
                .iter()
                .all(|v| v.is_finite())
        );
    }
    #[test]
    fn replay_start_frame_does_not_change_paused_vocoder_carrier_phase() {
        let mut config = AppConfig::new(120, 0, 5);
        config
            .input_fx
            .set_slot_kind(0, 0, crate::config::FxKind::Vocoder);
        config.input_fx.banks[0].slots[0].is_enabled = true;
        let mut initial = AudioSnapshot::empty(8000);
        for i in 0..317 {
            initial.tracks[0].write(i, [(i as f32 * 0.37).sin() * 0.2; 2], &mut OfflinePages);
        }
        let mut a = RenderCore::new(8000);
        let mut b = RenderCore::new(8000);
        a.configure(&mut Parameters::from_config(&config, 8000));
        b.configure(&mut Parameters::from_config(&config, 8000));
        a.restore(&mut initial);
        a.snapshot(&mut initial, &mut OfflinePages);
        b.restore(&mut initial);
        a.clock.frame = 987654;
        b.clock.frame = 0;
        a.action(Action::Preview(true), &mut OfflinePages);
        b.action(Action::Preview(true), &mut OfflinePages);
        for i in 0..2000 {
            let input = [(i as f32 * 0.15).sin() * 0.1; 2];
            assert_eq!(
                a.process(input, &mut OfflinePages).map(f32::to_bits),
                b.process(input, &mut OfflinePages).map(f32::to_bits)
            );
        }
    }
    fn record(core: &mut RenderCore, length: usize) {
        core.action(Action::Trigger(0), &mut OfflinePages);
        for i in 0..length {
            core.process([i as f32 / length as f32, 0.0], &mut OfflinePages);
        }
        core.action(Action::Trigger(0), &mut OfflinePages);
        core.process([0.0; 2], &mut OfflinePages);
    }
    #[test]
    fn exact_record_tail_overdub_undo_redo_and_snapshot() {
        let mut core = core();
        core.latency = 7;
        core.action(Action::Trigger(0), &mut OfflinePages);
        for i in 0..107 {
            core.process([i as f32 / 1000.0, 0.0], &mut OfflinePages);
        }
        core.action(Action::Trigger(0), &mut OfflinePages);
        for i in 107..115 {
            core.process([i as f32 / 1000.0, 0.0], &mut OfflinePages);
        }
        assert_eq!(core.tracks[0].audio.len, 107);
        assert_eq!(core.tracks[0].audio.read(0)[0], 0.007);
        assert_eq!(core.tracks[0].audio.read(106)[0], 0.113);
        let mut snapshot = AudioSnapshot::empty(8000);
        core.snapshot(&mut snapshot, &mut OfflinePages);
        core.action(Action::Trigger(0), &mut OfflinePages);
        for _ in 0..114 {
            core.process([0.1, 0.1], &mut OfflinePages);
        }
        core.action(Action::Trigger(0), &mut OfflinePages);
        for _ in 0..8 {
            core.process([0.1, 0.1], &mut OfflinePages);
        }
        assert!(core.tracks[0].audio.read(30)[1] > 0.0);
        core.action(Action::Undo(0), &mut OfflinePages);
        assert_eq!(core.tracks[0].audio.read(30), snapshot.tracks[0].read(30));
        core.action(Action::Undo(0), &mut OfflinePages);
        assert!(core.tracks[0].audio.read(30)[1] > 0.0);
        assert_eq!(snapshot.tracks[0].read(30)[1], 0.0);
    }
    #[test]
    fn consecutive_record_overdub_clear_history_survives_snapshot_restore() {
        let mut core = core();
        record(&mut core, 100);
        let original = core.tracks[0].audio.read(30);
        core.action(Action::Trigger(0), &mut OfflinePages);
        core.process([0.1; 2], &mut OfflinePages);
        core.action(Action::UndoStep(0), &mut OfflinePages);
        assert_eq!(
            core.tracks[0].mode,
            Mode::Overdub,
            "undo is disabled while writing"
        );
        for _ in 0..99 {
            core.process([0.1; 2], &mut OfflinePages);
        }
        core.action(Action::Trigger(0), &mut OfflinePages);
        core.process([0.0; 2], &mut OfflinePages);
        let overdub = core.tracks[0].audio.read(30);
        assert_ne!(original, overdub);
        core.action(Action::Clear(0), &mut OfflinePages);
        let mut saved = AudioSnapshot::empty(8000);
        core.snapshot(&mut saved, &mut OfflinePages);
        let mut restored = crate::engine::core::tests::core();
        restored.restore(&mut saved);
        restored.action(Action::UndoStep(0), &mut OfflinePages);
        assert_eq!(restored.tracks[0].audio.read(30), overdub);
        restored.action(Action::UndoStep(0), &mut OfflinePages);
        assert_eq!(restored.tracks[0].audio.read(30), original);
        restored.action(Action::UndoStep(0), &mut OfflinePages);
        assert_eq!(restored.tracks[0].mode, Mode::Empty);
        restored.action(Action::RedoStep(0), &mut OfflinePages);
        assert_eq!(restored.tracks[0].audio.read(30), original);
        restored.action(Action::RedoStep(0), &mut OfflinePages);
        assert_eq!(restored.tracks[0].audio.read(30), overdub);
        restored.action(Action::RedoStep(0), &mut OfflinePages);
        assert_eq!(restored.tracks[0].mode, Mode::Empty);
        assert_eq!(restored.tracks[1].history.undo.len, 0);
    }
    #[test]
    fn one_shot_reverse_stop_modes_and_fixed_length() {
        let mut core = core();
        record(&mut core, 100);
        core.action(Action::Panic, &mut OfflinePages);
        core.options[0].one_shot = true;
        core.options[0].reverse = true;
        core.action(Action::Trigger(0), &mut OfflinePages);
        let out = core.process([0.0; 2], &mut OfflinePages);
        assert!((out[0] - 0.99).abs() < 1e-5);
        for _ in 1..100 {
            core.process([0.0; 2], &mut OfflinePages);
        }
        assert_eq!(core.tracks[0].mode, Mode::Stopped);
        core.options[0].one_shot = false;
        core.options[0].reverse = false;
        core.options[0].stop_mode = StopMode::LoopEnd;
        core.action(Action::Trigger(0), &mut OfflinePages);
        core.process([0.0; 2], &mut OfflinePages);
        core.action(Action::Stop(0), &mut OfflinePages);
        assert!(core.tracks[0].pending.is_some());
        core.action(Action::Stop(0), &mut OfflinePages);
        assert_eq!(core.tracks[0].mode, Mode::Stopped);
        core.options[0].stop_mode = StopMode::Fade;
        core.options[0].fade_ms = 10;
        core.action(Action::Trigger(0), &mut OfflinePages);
        core.process([0.0; 2], &mut OfflinePages);
        core.action(Action::Stop(0), &mut OfflinePages);
        for _ in 0..81 {
            core.process([0.0; 2], &mut OfflinePages);
        }
        assert_eq!(core.tracks[0].mode, Mode::Stopped);
        core.action(Action::Clear(0), &mut OfflinePages);
        core.options[0].measures = 1;
        core.action(Action::Trigger(0), &mut OfflinePages);
        for _ in 0..16_001 {
            core.process([0.01; 2], &mut OfflinePages);
        }
        assert_eq!(core.tracks[0].audio.len, 16_000);
        assert_eq!(core.tracks[0].mode, Mode::Playing);
    }
    struct PreparedPages {
        available: Vec<Page>,
        retired: Vec<Page>,
    }
    impl PageAllocator for PreparedPages {
        fn acquire(&mut self) -> Option<Page> {
            self.available.pop()
        }
        fn retire(&mut self, page: Page) {
            self.retired.push(page);
        }
    }
    #[test]
    fn five_tracks_heavy_fx_commands_and_cow_do_not_allocate_or_free() {
        use crate::config::{FxKind, TrackFxKind};
        let mut config = AppConfig::new(120, 0, 5);
        for (slot, kind) in [
            FxKind::Oscillator,
            FxKind::MyDelay,
            FxKind::Vocoder,
            FxKind::Reverb,
        ]
        .into_iter()
        .enumerate()
        {
            config.input_fx.set_slot_kind(0, slot, kind);
            config.input_fx.banks[0].slots[slot].is_enabled = true;
        }
        for (slot, kind) in [TrackFxKind::Delay, TrackFxKind::Roll, TrackFxKind::Filter]
            .into_iter()
            .enumerate()
        {
            config.track_fx.set_slot_kind(0, slot, kind);
            for t in &mut config.track_fx.tracks {
                t.enabled[0][slot] = true;
            }
        }
        for options in &mut config.track_options {
            options.quantize = Quantize::Off;
        }
        let mut core = core();
        let mut parameters = Parameters::from_config(&config, 8000);
        let mut pages = PreparedPages {
            available: (0..256)
                .map(|_| std::sync::Arc::new([[0.0; 2]; PAGE_FRAMES]))
                .collect(),
            retired: Vec::with_capacity(1024),
        };
        let mut snapshot = AudioSnapshot::empty(8000);
        let count = crate::test_alloc::count(|| {
            core.configure(&mut parameters);
            for i in 0..5 {
                core.action(Action::Trigger(i), &mut pages);
            }
            for frame in 0..4000 {
                if frame == 1000 || frame == 1500 || frame == 3000 {
                    for i in 0..5 {
                        core.action(Action::Trigger(i), &mut pages);
                    }
                }
                if frame == 2000 {
                    core.snapshot(&mut snapshot, &mut pages);
                }
                core.process([(frame as f32 * 0.2).sin() * 0.05; 2], &mut pages);
            }
            for i in 0..5 {
                core.action(Action::Undo(i), &mut pages);
                core.action(Action::Clear(i), &mut pages);
            }
        });
        assert_eq!(count, 0, "Realtime renderer allocated or freed memory");
        assert!(!core.exhausted);
    }
}

#[cfg(test)]
#[path = "source_recording_tests.rs"]
mod source_recording_tests;
