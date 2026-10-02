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
        Self {
            metronome_volume: config.metronome_volume,
            input: InputFxRuntime::from_config(&config.input_fx),
            track: TrackFxRuntime::from_config(&config.track_fx),
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
            history: super::history::AudioHistory::new(sr),
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

pub struct RenderCore {
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
        let mut track_fx = TrackFxEngine::new(sr as f32, TRACKS);
        track_fx.prepare();
        Self {
            transport: false,
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
        self.metronome_volume = p.metronome_volume;
        p.input = self
            .input
            .swap_runtime(std::mem::replace(&mut p.input, InputFxRuntime::empty()));
        // The empty runtime allocates no sequences; track empty would allocate,
        // so swap with the engine through the reference method instead.
        self.track_fx.exchange_runtime(&mut p.track);
        self.options = p.options;
        self.levels = p.levels;
        if self.idle() {
            self.bpm = p.bpm;
            self.latency = p.latency_frames;
        }
        self.input.set_routing(p.routing);
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
        for i in 0..TRACKS {
            if self.tracks[i]
                .pending
                .is_some_and(|(at, _)| at <= self.clock.frame)
            {
                let (_, action) = self.tracks[i].pending.take().unwrap();
                self.execute(i, action, pool);
            }
            let t = &mut self.tracks[i];
            if t.finish
                .is_some_and(|at| self.clock.frame >= at + self.latency as u64)
            {
                if t.mode == Mode::Recording {
                    t.cursor = self.latency % t.audio.len.max(1);
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
        if self.idle() {
            self.clock.origin = None;
        }
        self.input
            .set_clock(self.clock.origin.is_some(), self.bpm as usize);
        self.track_fx
            .set_clock(self.bpm as usize, self.clock.origin.is_some());
        let mut carriers = [None; TRACKS];
        let mut mixed = [0.0; 2];
        for i in 0..TRACKS {
            let t = &mut self.tracks[i];
            let audible = matches!(t.mode, Mode::Playing | Mode::Overdub);
            if t.audio.len == 0
                || t.mode == Mode::Recording
                || (!audible && self.clock.origin.is_none())
            {
                continue;
            }
            let position = if audible {
                t.cursor
            } else {
                self.clock.frame.saturating_sub(t.play_origin) as usize % t.audio.len
            };
            let read = if self.options[i].reverse {
                t.audio.len - 1 - position
            } else {
                position
            };
            let dry = t.audio.read(read);
            let wet = self.track_fx.process_frame(
                i,
                position as f64 / self.sample_rate as f64,
                dry[0],
                dry[1],
            );
            if audible || self.clock.origin.is_some() {
                carriers[i] = Some(wet);
            }
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
            if audible {
                mixed[0] += wet.0 * t.gain * fade;
                mixed[1] += wet.1 * t.gain * fade;
                self.track_peaks[i] =
                    self.track_peaks[i].max(wet.0.abs().max(wet.1.abs()) * t.gain * fade);
            }
        }
        let elapsed = self.clock.elapsed() as f64 / self.sample_rate as f64;
        let (left, right) = self
            .input
            .process_frame(elapsed, input[0], input[1], &carriers);
        for i in 0..TRACKS {
            let t = &mut self.tracks[i];
            let mut ok = true;
            if t.mode == Mode::Recording && self.clock.frame >= t.start + self.latency as u64 {
                ok = t.audio.write(t.audio.len, [left, right], pool);
            } else if t.mode == Mode::Overdub && self.clock.frame >= t.start + self.latency as u64 {
                let write = (t.cursor + t.audio.len - self.latency % t.audio.len) % t.audio.len;
                let old = t.audio.read(write);
                ok = t.audio.write(
                    write,
                    [
                        (old[0] + left).clamp(-1.0, 1.0),
                        (old[1] + right).clamp(-1.0, 1.0),
                    ],
                    pool,
                );
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
        mixed[0] += left;
        mixed[1] += right;
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
