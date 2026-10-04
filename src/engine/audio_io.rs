//! Input is a bounded SPSC stream. The output callback exclusively owns DSP;
//! prepared commands and deferred reclamation replace callback mutexes.
use super::{
    core::{Action, AudioSnapshot, EngineView, Parameters, RenderCore},
    loop_audio::{Frame, PAGE_FRAMES, Page, PageAllocator},
};
use crate::{
    config::AppConfig,
    project::{ProjectData, ProjectEntry},
    replay,
};
use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::{HeapConsumer, HeapProducer, HeapRb};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

const PACKET: usize = 256;
pub const DEFAULT_BUFFER: u32 = 128;
#[derive(Default)]
pub struct Diagnostics {
    pub calibration_hold: AtomicBool,
    pub auditioning: AtomicBool,
    pub audition_commands: AtomicU64,
    pub audition_frame: AtomicU64,
    pub underrun: AtomicU64,
    pub overflow: AtomicU64,
    pub stream_errors: AtomicU64,
    pub output_errors: AtomicU64,
    pub output_generation: AtomicU64,
    pub maximum_callback_ns: AtomicU64,
    pub callback_frames: AtomicU64,
    pub queue_frames: AtomicU64,
    pub input_latency_ns: AtomicU64,
    pub output_latency_ns: AtomicU64,
    pub take_failed: AtomicBool,
    pub taking: AtomicBool,
    pub calibrating: AtomicBool,
    pub player_frame: AtomicU64,
    pub player_playing: AtomicBool,
}
pub struct Player {
    pub stream: replay::streaming::Consumer,
}
pub enum Control {
    Spectrum(bool),
    CalibrationHold(bool),
    Audition(Option<Box<super::audition::Audition>>),
    AuditionUpdate(Box<super::audition::AuditionParameters>),
    Config {
        parameters: Box<Parameters>,
        data: Arc<ProjectData>,
    },
    Action(Action),
    Replace(Box<RenderCore>),
    Snapshot {
        snapshot: Box<AudioSnapshot>,
        entry: ProjectEntry,
        data: ProjectData,
    },
    Capture(Box<AudioSnapshot>),
    BeginTake {
        retired_audition: Option<Box<super::audition::Audition>>,
        core: Box<RenderCore>,
        snapshot: Box<AudioSnapshot>,
        root: PathBuf,
        project_id: String,
        data: ProjectData,
    },
    EndTake,
    Enable(bool),
    Player(Option<Box<Player>>),
    Calibrate(Option<Box<super::latency::Calibration>>),
}
enum WorkerMessage {
    PdcApplied(super::core::PdcApplied),
    Control {
        at: u64,
        command: Control,
        accepted: bool,
    },
    Audio {
        at: u64,
        len: usize,
        frames: [Frame; PACKET],
    },
}
pub enum Response {
    Saved(String),
    Take(PathBuf),
    Error(String),
    Calibrated(super::latency::Measurement),
    Captured(Box<AudioSnapshot>),
}
struct RealtimePages {
    available: HeapConsumer<Page>,
    retired: HeapProducer<Page>,
}
impl PageAllocator for RealtimePages {
    fn acquire(&mut self) -> Option<Page> {
        self.available.pop()
    }
    fn retire(&mut self, page: Page) {
        // Sized for tracks, snapshots and bounded history page references. Never
        // fall back to deallocation on the realtime thread if exhausted.
        if let Err(page) = self.retired.push(page) {
            std::mem::forget(page);
        }
    }
}
struct Callback {
    audition: Option<Box<super::audition::Audition>>,
    metronome: super::metronome::Metronome,
    visual: super::spectrum::SpectrumFeed,
    core: Box<RenderCore>,
    commands: HeapConsumer<Control>,
    worker: HeapProducer<WorkerMessage>,
    views: HeapProducer<EngineView>,
    pages: RealtimePages,
    diagnostics: Arc<Diagnostics>,
    enabled: bool,
    taking: bool,
    packet: [Frame; PACKET],
    packet_len: usize,
    packet_at: u64,
    player: Option<Box<Player>>,
    calibration: Option<Box<super::latency::Calibration>>,
    take_underrun: u64,
    take_overflow: u64,
}
impl Callback {
    fn flush(&mut self) {
        if self.packet_len == 0 {
            return;
        }
        if self
            .worker
            .push(WorkerMessage::Audio {
                at: self.packet_at,
                len: self.packet_len,
                frames: self.packet,
            })
            .is_err()
        {
            self.diagnostics.take_failed.store(true, Ordering::Relaxed);
        }
        self.packet_len = 0;
    }
    fn commands(&mut self) {
        for _ in 0..32 {
            if self.worker.free_len() < 3 {
                break;
            }
            let Some(mut command) = self.commands.pop() else {
                break;
            };
            self.flush();
            let at = self.core.clock.frame;
            let mut accepted = true;
            match &mut command {
                Control::Spectrum(value) => self.visual.set_enabled(*value),
                Control::CalibrationHold(value) => {
                    if *value
                        && (!self.core.tracks_stopped()
                            || self.taking
                            || self.player.is_some()
                            || self.audition.is_some())
                    {
                        accepted = false;
                    } else if !*value && self.calibration.is_some() {
                        accepted = false;
                    } else {
                        self.diagnostics
                            .calibration_hold
                            .store(*value, Ordering::Relaxed);
                        if *value {
                            self.core.action(Action::Panic, &mut self.pages);
                        }
                    }
                }
                Control::Audition(value) => {
                    if value.is_some()
                        && (self.taking
                            || self.diagnostics.calibration_hold.load(Ordering::Relaxed)
                            || self.player.is_some())
                    {
                        accepted = false;
                    } else {
                        std::mem::swap(&mut self.audition, value);
                        self.diagnostics
                            .auditioning
                            .store(self.audition.is_some(), Ordering::Relaxed);
                    }
                    self.diagnostics
                        .audition_commands
                        .fetch_add(1, Ordering::Release);
                }
                Control::AuditionUpdate(params) => {
                    if let Some(audition) = &mut self.audition {
                        audition.update(params);
                    }
                }

                Control::Config { parameters, .. } => self.core.configure(parameters),
                Control::Action(action) => {
                    if self.diagnostics.calibration_hold.load(Ordering::Relaxed)
                        || self.player.is_some()
                    {
                        accepted = false;
                    } else {
                        self.core.action(*action, &mut self.pages)
                    }
                }
                Control::Replace(core) => {
                    if self.taking {
                        accepted = false;
                    } else {
                        core.clock.frame = at;
                        std::mem::swap(&mut self.core, core);
                    }
                }
                Control::Snapshot { snapshot, .. } => self.core.snapshot(snapshot, &mut self.pages),
                Control::Capture(snapshot) => self.core.snapshot(snapshot, &mut self.pages),
                Control::BeginTake {
                    core,
                    snapshot,
                    retired_audition,
                    ..
                } => {
                    if !self.core.tracks_stopped()
                        || self.taking
                        || self.player.is_some()
                        || self.calibration.is_some()
                        || self.diagnostics.calibration_hold.load(Ordering::Relaxed)
                    {
                        accepted = false;
                    } else {
                        std::mem::swap(&mut self.audition, retired_audition);
                        self.diagnostics.auditioning.store(false, Ordering::Relaxed);
                        self.core.snapshot(snapshot, &mut self.pages);
                        core.restore(snapshot);
                        core.snapshot(snapshot, &mut self.pages);
                        core.clock.frame = at;
                        std::mem::swap(&mut self.core, core);
                        self.taking = true;
                        self.diagnostics.take_failed.store(false, Ordering::Relaxed);
                        self.diagnostics.taking.store(true, Ordering::Relaxed);
                        self.take_underrun = self.diagnostics.underrun.load(Ordering::Relaxed);
                        self.take_overflow = self.diagnostics.overflow.load(Ordering::Relaxed);
                    }
                }
                Control::EndTake => {
                    accepted = self.taking
                        && self.core.tracks.iter().all(|t| {
                            !matches!(
                                t.mode,
                                super::core::Mode::Recording | super::core::Mode::Overdub
                            )
                        });
                    if accepted {
                        self.taking = false;
                        self.diagnostics.taking.store(false, Ordering::Relaxed);
                    }
                }
                Control::Enable(value) => self.enabled = *value,
                Control::Player(value) => {
                    if self.taking
                        || !self.core.tracks_stopped()
                        || self.diagnostics.calibration_hold.load(Ordering::Relaxed)
                        || self.audition.is_some()
                    {
                        accepted = false;
                    } else {
                        if value.is_some() {
                            self.core.action(Action::Panic, &mut self.pages);
                        }
                        std::mem::swap(&mut self.player, value);
                    }
                }
                Control::Calibrate(value) => {
                    if !self.diagnostics.calibration_hold.load(Ordering::Relaxed)
                        || self.player.is_some()
                        || self.audition.is_some()
                        || self.taking
                        || !self.core.idle()
                        || self.calibration.is_some()
                        || value.as_ref().is_some_and(|probe| {
                            probe.output_generation
                                != self.diagnostics.output_generation.load(Ordering::Relaxed)
                        })
                    {
                        accepted = false;
                    } else {
                        std::mem::swap(&mut self.calibration, value);
                        self.diagnostics.calibrating.store(true, Ordering::Relaxed);
                    }
                }
            }
            let _ = self.worker.push(WorkerMessage::Control {
                at,
                command,
                accepted,
            });
        }
    }
    fn frame(&mut self, dry: Frame) -> Frame {
        let result = self.render_frame(dry);
        self.visual.push(result);
        result
    }
    fn render_frame(&mut self, dry: Frame) -> Frame {
        if let Some(calibration) = &mut self.calibration {
            if calibration.finished() {
                if self.worker.free_len() > 0 {
                    let command = Control::Calibrate(self.calibration.take());
                    self.diagnostics.calibrating.store(false, Ordering::Relaxed);
                    let _ = self.worker.push(WorkerMessage::Control {
                        at: self.core.clock.frame,
                        command,
                        accepted: true,
                    });
                }
                return [0.0; 2];
            }
            return calibration.process(dry);
        }
        if self.diagnostics.calibration_hold.load(Ordering::Relaxed) {
            return [0.0; 2];
        }
        if let Some(player) = &mut self.player {
            let frame = player.stream.next();
            self.diagnostics
                .player_frame
                .store(player.stream.shared.position(), Ordering::Relaxed);
            self.diagnostics
                .player_playing
                .store(player.stream.shared.playing(), Ordering::Relaxed);
            return frame;
        }
        if !self.enabled {
            return [0.0; 2];
        }
        if self.taking {
            if self.packet_len == 0 {
                self.packet_at = self.core.clock.frame;
            }
            self.packet[self.packet_len] = dry;
            self.packet_len += 1;
            if self.packet_len == PACKET {
                self.flush();
            }
        }
        let click = self.metronome.next(
            self.core.metronome && self.core.clock.origin.is_some(),
            self.core.clock.elapsed(),
            self.core.sample_rate,
            self.core.bpm,
            self.core.metronome_volume,
        );
        let mut result = self.core.process(dry, &mut self.pages);
        if let Some(applied) = self.core.take_pdc_applied_event() {
            if self.taking
                && self
                    .worker
                    .push(WorkerMessage::PdcApplied(applied))
                    .is_err()
            {
                self.diagnostics.take_failed.store(true, Ordering::Relaxed);
            }
        }
        let mut click = self.core.compensate_monitor_click(click);
        if let Some(audition) = &mut self.audition {
            let preview = audition.next(dry, &self.core);
            if audition.is_exclusive() {
                result = preview;
                click = 0.0;
            } else {
                result[0] += preview[0];
                result[1] += preview[1];
            }
        }
        if self.audition.as_ref().is_some_and(|a| a.finished()) && self.worker.free_len() > 0 {
            let retired = Control::Audition(self.audition.take());
            self.diagnostics.auditioning.store(false, Ordering::Relaxed);
            let _ = self.worker.push(WorkerMessage::Control {
                at: self.core.clock.frame,
                command: retired,
                accepted: true,
            });
        }
        result[0] = (result[0] + click).clamp(-1.0, 1.0);
        result[1] = (result[1] + click).clamp(-1.0, 1.0);
        if self.taking
            && (self.core.exhausted
                || self.diagnostics.underrun.load(Ordering::Relaxed) != self.take_underrun
                || self.diagnostics.overflow.load(Ordering::Relaxed) != self.take_overflow)
        {
            self.diagnostics.take_failed.store(true, Ordering::Relaxed);
        }
        result
    }
    fn publish(&mut self) {
        if let Some(audition) = &self.audition {
            self.diagnostics
                .audition_frame
                .store(audition.frame, Ordering::Relaxed);
        }
        if self.views.free_len() > 0 {
            let mut view = self.core.view();
            view.output_spectrum = self.visual.snapshot();
            let _ = self.views.push(view);
        }
    }
}
struct OutputState {
    callback: Callback,
    input_rx: HeapConsumer<Frame>,
    drift: super::latency::InputAdapter,
    converter: super::output_resampler::OutputResampler,
    fade_remaining: usize,
    fade_total: usize,
}
struct OutputPump {
    state: Option<Box<OutputState>>,
    pending: HeapConsumer<Box<OutputState>>,
    returned: mpsc::Sender<Box<OutputState>>,
}
impl Drop for OutputPump {
    fn drop(&mut self) {
        // Stream teardown only; no mutex/channel send occurs during rendering.
        if let Some(state) = self.state.take().or_else(|| self.pending.pop()) {
            let _ = self.returned.send(state);
        }
    }
}
impl OutputPump {
    fn process(
        &mut self,
        data: &mut [f32],
        info: &cpal::OutputCallbackInfo,
        channels: usize,
        rate: u32,
        diagnostics: &Diagnostics,
    ) {
        if self.state.is_none() {
            self.state = self.pending.pop();
        }
        let Some(state) = self.state.as_mut() else {
            data.fill(0.0);
            return;
        };
        let started = Instant::now();
        if let Some(duration) = info
            .timestamp()
            .playback
            .duration_since(&info.timestamp().callback)
        {
            diagnostics
                .output_latency_ns
                .store(duration.as_nanos() as u64, Ordering::Relaxed);
        }
        state.callback.commands();
        let frames = data.len() / channels;
        diagnostics
            .callback_frames
            .store(frames as u64, Ordering::Relaxed);
        diagnostics
            .queue_frames
            .store(state.input_rx.len() as u64, Ordering::Relaxed);
        let engine_frames =
            (frames as u64 * state.callback.core.sample_rate as u64).div_ceil(rate as u64) as usize;
        state.drift.begin_block(state.input_rx.len(), engine_frames);
        let callback = &mut state.callback;
        let input = &mut state.input_rx;
        let drift = &mut state.drift;
        for frame in data.chunks_exact_mut(channels) {
            let wet = state
                .converter
                .next(|| callback.frame(drift.next(input, diagnostics)));
            let gain = 1.0 - state.fade_remaining as f32 / state.fade_total as f32;
            state.fade_remaining = state.fade_remaining.saturating_sub(1);
            frame.fill(0.0);
            frame[0] = if channels == 1 {
                (wet[0] + wet[1]) * 0.5 * gain
            } else {
                wet[0] * gain
            };
            if channels > 1 {
                frame[1] = wet[1] * gain;
            }
        }
        callback.flush();
        callback.publish();
        diagnostics
            .maximum_callback_ns
            .fetch_max(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
    }
}
pub struct AudioIO {
    input_stream: Option<cpal::Stream>,
    output_stream: Option<cpal::Stream>,
    output_return_tx: mpsc::Sender<Box<OutputState>>,
    output_return_rx: mpsc::Receiver<Box<OutputState>>,
    parked_output: Option<Box<OutputState>>,
    pub output_config: cpal::StreamConfig,
    pub config: cpal::StreamConfig,
    input_name: String,
    output_name: String,
    commands: HeapProducer<Control>,
    views: HeapConsumer<EngineView>,
    pub diagnostics: Arc<Diagnostics>,
    responses: mpsc::Receiver<Response>,
    stop: Arc<AtomicBool>,
    offline: Option<Callback>,
    pub online: bool,
    pub status: String,
}
impl Drop for AudioIO {
    fn drop(&mut self) {
        self.input_stream.take();
        self.output_stream.take();
        self.offline.take();
        self.stop.store(true, Ordering::Release);
    }
}
impl AudioIO {
    pub fn new(input: &str, output: &str, _tracks: usize, _latency: usize) -> Result<Self> {
        Self::with_buffer(input, output, DEFAULT_BUFFER)
    }
    pub fn with_buffer(input: &str, output: &str, block: u32) -> Result<Self> {
        Self::open(input, Some(output), block)
    }
    pub fn with_system_output(input: &str, block: u32) -> Result<Self> {
        Self::open(input, None, block)
    }
    fn open(input: &str, output: Option<&str>, block: u32) -> Result<Self> {
        let host = if let Some(output) = output {
            select_host(input, output)?
        } else {
            cpal::default_host()
        };
        let input_device = host
            .input_devices()?
            .find(|d| d.name().ok().as_deref() == Some(input))
            .context("Input device unavailable")?;
        let output_device = if let Some(name) = output {
            host.output_devices()?
                .find(|d| d.name().ok().as_deref() == Some(name))
        } else {
            host.default_output_device()
        };
        let inputs = input_device.supported_input_configs()?.collect::<Vec<_>>();
        let outputs = output_device
            .as_ref()
            .and_then(|d| d.supported_output_configs().ok())
            .map(|v| v.collect::<Vec<_>>())
            .unwrap_or_default();
        let config = super::device_config::common_config(&inputs, &outputs, block)
            .or_else(|_| super::device_config::endpoint_config(&inputs, 48000, block, false))?;
        let (mut audio, callback) = Self::bridge(config.clone());
        let (mut input_tx, input_rx) = HeapRb::<Frame>::new(16_384).split();
        let diagnostics = audio.diagnostics.clone();
        let input_errors = audio.diagnostics.clone();
        let channels = config.channels as usize;
        let input_stream = input_device.build_input_stream(
            &config,
            move |data: &[f32], info: &cpal::InputCallbackInfo| {
                if let Some(duration) = info
                    .timestamp()
                    .callback
                    .duration_since(&info.timestamp().capture)
                {
                    diagnostics
                        .input_latency_ns
                        .store(duration.as_nanos() as u64, Ordering::Relaxed);
                }
                for frame in data.chunks_exact(channels) {
                    let clean = |s: f32| if s.is_finite() { s } else { 0.0 };
                    let value = [
                        clean(frame[0]),
                        clean(frame.get(1).copied().unwrap_or(frame[0])),
                    ];
                    if input_tx.push(value).is_err() {
                        diagnostics.overflow.fetch_add(1, Ordering::Relaxed);
                    }
                }
            },
            move |_| {
                input_errors.stream_errors.fetch_add(1, Ordering::Relaxed);
            },
            None,
        )?;
        audio.input_stream = Some(input_stream);
        audio.input_name = input.into();
        audio.parked_output = Some(Box::new(OutputState {
            callback,
            input_rx,
            drift: super::latency::InputAdapter::new(block as usize),
            converter: super::output_resampler::OutputResampler::new(
                config.sample_rate.0,
                config.sample_rate.0,
            ),
            fade_remaining: 0,
            fade_total: 1,
        }));
        if let Some(device) = output_device {
            if let Err(error) = audio.retarget_output(&device, block) {
                audio.status = format!("Output unavailable; waiting to reconnect: {error}");
            }
        } else {
            audio.status = "No system output device; waiting to reconnect".into();
        }
        Ok(audio)
    }
    /// Swap only the physical endpoint. The exact callback/renderer is moved,
    /// including transport, pending actions, undo, FX history and replay state.
    pub fn retarget_output(&mut self, device: &cpal::Device, block: u32) -> Result<()> {
        let name = device.name()?;
        let output_config = super::device_config::endpoint_config(
            &device.supported_output_configs()?.collect::<Vec<_>>(),
            self.config.sample_rate.0,
            block,
            true,
        )?;
        let (mut tx, rx) = HeapRb::<Box<OutputState>>::new(1).split();
        let mut pump = OutputPump {
            state: None,
            pending: rx,
            returned: self.output_return_tx.clone(),
        };
        let diagnostics = self.diagnostics.clone();
        let errors = diagnostics.clone();
        let channels = output_config.channels as usize;
        let rate = output_config.sample_rate.0;
        // Prepare/start the new endpoint with silence before releasing the old
        // one. A failed build never destroys the running renderer.
        let stream = device.build_output_stream(
            &output_config,
            move |data: &mut [f32], info| {
                pump.process(data, info, channels, rate, &diagnostics);
            },
            move |_| {
                errors.stream_errors.fetch_add(1, Ordering::Relaxed);
                errors.output_errors.fetch_add(1, Ordering::Relaxed);
            },
            None,
        )?;
        stream.play()?;
        self.output_stream.take(); // CPAL joins its callback; Drop returns ownership.
        if self.parked_output.is_none() {
            self.parked_output = Some(
                self.output_return_rx
                    .recv_timeout(Duration::from_secs(1))
                    .context("Output callback did not return its state")?,
            );
        }
        let mut state = self.parked_output.take().context("Missing output state")?;
        state.converter =
            super::output_resampler::OutputResampler::new(self.config.sample_rate.0, rate);
        state.drift.reset_target(block as usize);
        state.fade_total = (rate / 200).max(1) as usize;
        state.fade_remaining = state.fade_total;
        // A route change invalidates a loopback probe. Never apply its result.
        state.callback.calibration.take();
        self.diagnostics.calibrating.store(false, Ordering::Relaxed);
        if let Some(input) = &self.input_stream {
            if let Err(error) = input.play() {
                self.parked_output = Some(state);
                self.online = false;
                return Err(error.into());
            }
        }
        self.diagnostics
            .output_generation
            .fetch_add(1, Ordering::Relaxed);
        if let Err(state) = tx.push(state) {
            self.parked_output = Some(state);
            self.online = false;
            anyhow::bail!("Output handoff queue unavailable");
        }
        self.output_stream = Some(stream);
        self.output_name = name;
        self.output_config = output_config;
        self.online = true;
        self.status = format!(
            "{} Hz engine → {} Hz / {} ch output",
            self.config.sample_rate.0, rate, channels
        );
        Ok(())
    }
    pub fn park_output(&mut self) {
        if self.output_stream.is_none() {
            return;
        }
        if let Some(input) = &self.input_stream {
            let _ = input.pause();
        }
        self.output_stream.take();
        self.diagnostics
            .output_generation
            .fetch_add(1, Ordering::Relaxed);
        self.parked_output = self.output_return_rx.try_recv().ok();
        self.online = false;
        self.status = "No system output device; waiting to reconnect".into();
        // Input during physical device loss cannot be reconstructed.
        if self.diagnostics.taking.load(Ordering::Relaxed) {
            self.diagnostics.take_failed.store(true, Ordering::Relaxed);
        }
        if let Some(state) = &mut self.parked_output {
            state.callback.calibration.take();
        }
        self.diagnostics.calibrating.store(false, Ordering::Relaxed);
    }
    pub fn has_live_input(&self) -> bool {
        self.input_stream.is_some()
    }
    pub fn offline(message: String) -> Self {
        let config = cpal::StreamConfig {
            channels: 2,
            sample_rate: cpal::SampleRate(48_000),
            buffer_size: cpal::BufferSize::Fixed(DEFAULT_BUFFER),
        };
        let (mut audio, callback) = Self::bridge(config);
        audio.offline = Some(callback);
        audio.status = message;
        audio
    }
    fn bridge(config: cpal::StreamConfig) -> (Self, Callback) {
        let (output_return_tx, output_return_rx) = mpsc::channel();
        let sr = config.sample_rate.0;
        let (commands, command_rx) = HeapRb::new(128).split();
        let (view_tx, views) = HeapRb::new(8).split();
        let (worker_tx, mut worker_rx) = HeapRb::new(8192).split();
        let (mut page_tx, page_rx) = HeapRb::<Page>::new(256).split();
        for _ in 0..256 {
            let _ = page_tx.push(Arc::new([[0.0; 2]; PAGE_FRAMES]));
        }
        let (retire_tx, mut retire_rx) = HeapRb::<Page>::new(1_048_576).split();
        let stop = Arc::new(AtomicBool::new(false));
        let pool_stop = stop.clone();
        std::thread::Builder::new()
            .name("audio-page-pool".into())
            .spawn(move || {
                while !pool_stop.load(Ordering::Acquire) {
                    while let Some(page) = retire_rx.pop() {
                        if Arc::strong_count(&page) == 1 && page_tx.free_len() > 0 {
                            let _ = page_tx.push(page);
                        }
                    }
                    for _ in 0..8 {
                        if page_tx.free_len() == 0 {
                            break;
                        }
                        let _ = page_tx.push(Arc::new([[0.0; 2]; PAGE_FRAMES]));
                    }
                    std::thread::sleep(Duration::from_millis(2));
                }
            })
            .expect("page pool thread");
        let (response_tx, responses) = mpsc::channel();
        let diagnostics = Arc::new(Diagnostics::default());
        let worker_diag = diagnostics.clone();
        let worker_stop = stop.clone();
        std::thread::Builder::new()
            .name("audio-retire-and-replay".into())
            .spawn(move || {
                let mut writer: Option<replay::Writer> = None;
                while !worker_stop.load(Ordering::Acquire) || !worker_rx.is_empty() {
                    if let Some(message) = worker_rx.pop() {
                        if let Err(error) =
                            worker_message(message, &mut writer, &response_tx, &worker_diag)
                        {
                            worker_diag.take_failed.store(true, Ordering::Relaxed);
                            writer = None;
                            let _ = response_tx.send(Response::Error(error.to_string()));
                        }
                    } else {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                }
            })
            .expect("audio worker thread");
        let callback = Callback {
            audition: None,
            metronome: super::metronome::Metronome::new(sr),
            visual: super::spectrum::SpectrumFeed::new(sr, stop.clone()),
            core: Box::new(RenderCore::new(sr)),
            commands: command_rx,
            worker: worker_tx,
            views: view_tx,
            pages: RealtimePages {
                available: page_rx,
                retired: retire_tx,
            },
            diagnostics: diagnostics.clone(),
            enabled: false,
            taking: false,
            packet: [[0.0; 2]; PACKET],
            packet_len: 0,
            packet_at: 0,
            player: None,
            calibration: None,
            take_underrun: 0,
            take_overflow: 0,
        };
        (
            Self {
                input_stream: None,
                output_stream: None,
                output_return_tx,
                output_return_rx,
                parked_output: None,
                output_config: config.clone(),
                config,
                input_name: String::new(),
                output_name: String::new(),
                commands,
                views,
                diagnostics,
                responses,
                stop,
                offline: None,
                online: false,
                status: String::new(),
            },
            callback,
        )
    }
    pub fn send(&mut self, command: Control) -> Result<()> {
        self.commands
            .push(command)
            .map_err(|_| anyhow::anyhow!("Audio command queue is full; action was not applied"))
    }
    pub fn poll(&mut self) -> Option<EngineView> {
        if let Some(callback) = &mut self.offline {
            callback.commands();
            callback.publish();
        }
        if self.output_stream.is_none() {
            if self.parked_output.is_none() {
                self.parked_output = self.output_return_rx.try_recv().ok();
            }
            if let Some(state) = &mut self.parked_output {
                state.callback.commands();
                state.callback.publish();
            }
        }
        let mut view = None;
        while let Some(value) = self.views.pop() {
            view = Some(value);
        }
        view
    }
    pub fn responses(&self) -> impl Iterator<Item = Response> + '_ {
        self.responses.try_iter()
    }
    pub fn configure(&mut self, config: &AppConfig) -> Result<()> {
        self.send(Control::Config {
            parameters: Box::new(Parameters::from_config(config, self.config.sample_rate.0)),
            data: Arc::new(crate::project::data_from_config(config)),
        })
    }
    pub fn curr_input_name(&self) -> &str {
        &self.input_name
    }
    pub fn curr_output_name(&self) -> &str {
        if self.online { &self.output_name } else { "" }
    }
    pub fn suspend(&self) {
        if let Some(stream) = &self.input_stream {
            let _ = stream.pause();
        }
        if let Some(stream) = &self.output_stream {
            let _ = stream.pause();
        }
    }
    pub fn resume(&self) {
        if let Some(stream) = &self.input_stream {
            let _ = stream.play();
        }
        if let Some(stream) = &self.output_stream {
            let _ = stream.play();
        }
    }
}
fn worker_message(
    message: WorkerMessage,
    writer: &mut Option<replay::Writer>,
    tx: &mpsc::Sender<Response>,
    diagnostics: &Diagnostics,
) -> Result<()> {
    match message {
        WorkerMessage::PdcApplied(applied) => {
            if let Some(writer) = writer {
                writer.event(
                    applied.applied_at + 1,
                    replay::EventKind::PdcApplied(applied),
                )?;
            }
        }
        WorkerMessage::Audio { at, len, frames } => {
            if let Some(writer) = writer {
                writer.audio(at, &frames[..len])?;
            }
        }
        WorkerMessage::Control {
            at,
            command,
            accepted,
        } => {
            if !accepted {
                let _ = tx.send(Response::Error(
                    "Operation requires stopped tracks and no active replay capture".into(),
                ));
                return Ok(());
            }
            match command {
                Control::Config { data, .. } => {
                    if let Some(writer) = writer {
                        writer.event(at, replay::EventKind::Config((*data).clone()))?;
                    }
                }
                Control::Action(action) => {
                    if let Some(writer) = writer {
                        writer.event(at, replay::EventKind::Action(action))?;
                    }
                }
                Control::BeginTake {
                    snapshot,
                    root,
                    project_id,
                    data,
                    ..
                } => {
                    *writer = Some(replay::Writer::begin(
                        root, project_id, at, *snapshot, data,
                    )?);
                }
                Control::EndTake => {
                    let Some(value) = writer.take() else {
                        anyhow::bail!("No valid replay capture to finish");
                    };
                    anyhow::ensure!(
                        !diagnostics.take_failed.load(Ordering::Relaxed),
                        "Replay capture overflowed; incomplete input cannot be exported"
                    );
                    let root = value.finish(at)?;
                    let _ = tx.send(Response::Take(root));
                }
                Control::Snapshot {
                    snapshot,
                    entry,
                    data,
                } => {
                    let tx = tx.clone();
                    std::thread::spawn(move || {
                        let _ = tx.send(
                            match crate::session::save_snapshot(&entry, &snapshot, data) {
                                Ok(revision) => Response::Saved(revision),
                                Err(e) => Response::Error(format!("Snapshot save failed: {e}")),
                            },
                        );
                    });
                }
                Control::Capture(snapshot) => {
                    let _ = tx.send(Response::Captured(snapshot));
                }
                Control::Calibrate(Some(value)) if value.finished() => {
                    let tx = tx.clone();
                    std::thread::spawn(move || {
                        let _ = tx.send(match value.analyze() {
                            Ok(v) => Response::Calibrated(v),
                            Err(e) => Response::Error(e.to_string()),
                        });
                    });
                }
                _ => {}
            }
        }
    }
    Ok(())
}
fn select_host(_input: &str, _output: &str) -> Result<cpal::Host> {
    #[cfg(all(target_os = "windows", feature = "asio"))]
    if let Ok(host) = cpal::host_from_id(cpal::HostId::Asio) {
        if host
            .input_devices()?
            .any(|d| d.name().ok().as_deref() == Some(_input))
            && host
                .output_devices()?
                .any(|d| d.name().ok().as_deref() == Some(_output))
        {
            return Ok(host);
        }
    }
    Ok(cpal::default_host())
}

#[cfg(test)]
mod output_tests {
    use super::*;
    fn callback() -> (AudioIO, Callback) {
        AudioIO::bridge(cpal::StreamConfig {
            channels: 2,
            sample_rate: cpal::SampleRate(8000),
            buffer_size: cpal::BufferSize::Fixed(128),
        })
    }
    #[test]
    fn candidate_isolates_monitoring_without_changing_recording_or_live_parameters() {
        use crate::{
            config::{FxKind, InputFx, audio_fx::AudioFxKind},
            engine::audition::{Audition, AuditionParameters},
            presets::FxTarget,
        };
        let (mut audio, mut cb) = callback();
        cb.enabled = true;
        cb.core.options[0].quantize = crate::config::track_options::Quantize::Off;
        cb.core.action(Action::Trigger(0), &mut cb.pages);
        let mut staging = AppConfig::new(120, 0, 5);
        staging
            .input_fx
            .set_slot_kind(0, 0, FxKind::Audio(AudioFxKind::Pan));
        if let Some(InputFx::Audio(pan)) = &mut staging.input_fx.banks[0].slots[0].fx {
            pan.pan = 1.0;
        }
        let params =
            AuditionParameters::candidate(&staging, FxTarget::Input { bank: 0, slot: 0 }, 0)
                .unwrap();
        audio
            .send(Control::Audition(Some(Box::new(
                Audition::new(params, 8000).exclusive(),
            ))))
            .unwrap();
        cb.commands();
        for _ in 0..100 {
            let output = cb.frame([0.2; 2]);
            assert!(output[0].abs() < 1e-6);
            assert!(
                (output[1] - 0.2).abs() < 1e-6,
                "Candidate was added to the original monitor instead of isolated"
            );
        }
        assert_eq!(cb.core.tracks[0].audio.len, 100);
        for frame in 0..100 {
            assert_eq!(cb.core.tracks[0].audio.read(frame), [0.2; 2]);
        }
        audio.send(Control::Audition(None)).unwrap();
        cb.commands();
        assert_eq!(cb.frame([0.2; 2]), [0.2; 2]);
        assert_eq!(cb.core.clock.frame, 101);
    }
    #[test]
    fn audition_command_ack_survives_delayed_callback_and_full_stop_queue() {
        use crate::{
            config::{FxKind, InputFx, audio_fx::AudioFxKind},
            engine::audition::{Audition, AuditionParameters, Requests},
            presets::FxTarget,
        };
        let (mut audio, mut cb) = callback();
        cb.enabled = true;
        let mut requests = Requests::default();
        let mut config = AppConfig::new(120, 0, 5);
        config
            .input_fx
            .set_slot_kind(0, 0, FxKind::Audio(AudioFxKind::Pan));
        if let Some(InputFx::Audio(p)) = &mut config.input_fx.banks[0].slots[0].fx {
            p.pan = 1.0;
        }
        let params =
            AuditionParameters::candidate(&config, FxTarget::Input { bank: 0, slot: 0 }, 0)
                .unwrap();
        let serial = requests.next(0);
        audio
            .send(Control::Audition(Some(Box::new(
                Audition::new(params, 8000).exclusive(),
            ))))
            .unwrap();
        requests.sent(serial, true);
        assert_eq!(
            requests.observe(
                audio.diagnostics.audition_commands.load(Ordering::Acquire),
                audio.diagnostics.auditioning.load(Ordering::Relaxed)
            ),
            None,
            "Queued starts are not disproved by a stale false flag"
        );
        cb.commands();
        assert_eq!(
            requests.observe(
                audio.diagnostics.audition_commands.load(Ordering::Acquire),
                audio.diagnostics.auditioning.load(Ordering::Relaxed)
            ),
            Some(true)
        );
        while audio.send(Control::Spectrum(false)).is_ok() {}
        assert!(audio.send(Control::Audition(None)).is_err());
        assert_eq!(
            requests.observe(
                audio.diagnostics.audition_commands.load(Ordering::Acquire),
                audio.diagnostics.auditioning.load(Ordering::Relaxed)
            ),
            Some(true),
            "Failed Stop must retain the playing state"
        );
        assert!(cb.frame([0.2; 2])[0].abs() < 1e-6);
        cb.commands();
        let stop = requests.next(audio.diagnostics.audition_commands.load(Ordering::Acquire));
        audio.send(Control::Audition(None)).unwrap();
        requests.sent(stop, false);
        assert_eq!(requests.observe(serial, true), None);
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while audio.diagnostics.audition_commands.load(Ordering::Acquire) < stop {
            cb.commands();
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert_eq!(
            requests.observe(
                audio.diagnostics.audition_commands.load(Ordering::Acquire),
                audio.diagnostics.auditioning.load(Ordering::Relaxed)
            ),
            Some(false)
        );
        assert_eq!(cb.frame([0.2; 2]), [0.2; 2]);
    }
    #[test]
    fn failed_calibration_stays_muted_until_explicit_release() {
        let (mut audio, mut cb) = callback();
        cb.enabled = true;
        audio.send(Control::CalibrationHold(true)).unwrap();
        cb.commands();
        assert_eq!(cb.frame([0.5; 2]), [0.0; 2]);
        audio
            .send(Control::Calibrate(Some(Box::new(
                super::super::latency::Calibration::new(8000),
            ))))
            .unwrap();
        cb.commands();
        for _ in 0..24002 {
            cb.frame([0.0; 2]);
        }
        assert!(!cb.diagnostics.calibrating.load(Ordering::Relaxed));
        assert!(cb.diagnostics.calibration_hold.load(Ordering::Relaxed));
        assert_eq!(cb.frame([0.5; 2]), [0.0; 2]);
        audio.send(Control::Enable(false)).unwrap();
        audio.send(Control::Enable(true)).unwrap();
        cb.commands();
        assert_eq!(cb.frame([0.5; 2]), [0.0; 2]);
        audio.send(Control::CalibrationHold(false)).unwrap();
        cb.commands();
        assert!(cb.frame([0.5; 2])[0] > 0.1);
    }
    #[test]
    fn metronome_and_audition_are_monitor_only() {
        let (_audio, mut cb) = callback();
        cb.enabled = true;
        cb.core.options[0].quantize = crate::config::track_options::Quantize::Off;
        cb.core.action(Action::Metronome(true), &mut cb.pages);
        cb.core.action(Action::Trigger(0), &mut cb.pages);
        let mut heard = 0.0f32;
        for _ in 0..5000 {
            heard = heard.max(cb.frame([0.0; 2])[0].abs());
        }
        assert!(heard > 0.05);
        assert!(
            (0..cb.core.tracks[0].audio.len).all(|i| cb.core.tracks[0].audio.read(i) == [0.0; 2])
        );
        cb.core.action(Action::Panic, &mut cb.pages);
        let mut config = AppConfig::new(120, 0, 5);
        config
            .input_fx
            .set_slot_kind(0, 0, crate::config::FxKind::Oscillator);
        if let Some(crate::config::InputFx::Oscillator(osc)) =
            &mut config.input_fx.banks[0].slots[0].fx
        {
            osc.threshold.value = 100;
            osc.note.replace_events(
                3840,
                &[crate::config::sequence_edit::NoteEvent {
                    id: 0,
                    velocity: 100,
                    start: 0,
                    len: 3840,
                    pitch: crate::config::note_configs::NoteOct::from_pitch_index(48),
                }],
            );
        }
        cb.core
            .configure(&mut Parameters::from_config(&config, 8000));
        let params = super::super::audition::AuditionParameters::new(
            &config,
            crate::presets::FxTarget::Input { bank: 0, slot: 0 },
            0,
        )
        .unwrap();
        cb.audition = Some(Box::new(super::super::audition::Audition::new(
            params, 8000,
        )));
        let mut heard = 0.0f32;
        for _ in 0..4000 {
            heard = heard.max(cb.frame([0.0; 2])[0].abs());
        }
        assert!(heard > 0.01);
        assert!(
            cb.core.clock.origin.is_none(),
            "Audition cannot start performance"
        );
        cb.core.action(Action::Trigger(1), &mut cb.pages);
        for _ in 0..1000 {
            cb.frame([0.0; 2]);
        }
        assert!(
            (0..cb.core.tracks[1].audio.len).all(|i| cb.core.tracks[1].audio.read(i) == [0.0; 2])
        );
        assert!(!config.input_fx.banks[0].slots[0].is_enabled);
    }
    #[test]
    fn capture_accepts_stopped_tracks_with_a_running_transport() {
        let (mut audio, mut cb) = callback();
        cb.core.action(Action::Metronome(true), &mut cb.pages);
        assert!(!cb.core.idle() && cb.core.tracks_stopped());
        let config = AppConfig::new(120, 0, 5);
        let root = PathBuf::from("var").join(format!("capture-test-{}", crate::session::id()));
        audio
            .send(Control::BeginTake {
                core: Box::new(RenderCore::new(8000)),
                snapshot: Box::new(AudioSnapshot::empty(8000)),
                retired_audition: None,
                root: root.clone(),
                project_id: "test.json".into(),
                data: crate::project::data_from_config(&config),
            })
            .unwrap();
        cb.commands();
        assert!(cb.taking);
        assert!(!cb.core.metronome && cb.core.clock.origin.is_none());
        audio.send(Control::EndTake).unwrap();
        cb.commands();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let mut complete = false;
            for response in audio.responses() {
                match response {
                    Response::Take(path) => {
                        assert_eq!(path, root);
                        complete = true;
                    }
                    Response::Error(error) => panic!("Capture completion failed: {error}"),
                    _ => {}
                }
            }
            if complete {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "Capture completion timed out"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let root = std::fs::canonicalize(root).unwrap();
        assert_eq!(
            root.parent(),
            Some(std::fs::canonicalize("var").unwrap().as_path())
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn handoff_returns_exact_recording_renderer_even_before_first_callback() {
        let config = cpal::StreamConfig {
            channels: 2,
            sample_rate: cpal::SampleRate(8000),
            buffer_size: cpal::BufferSize::Fixed(128),
        };
        let (_audio, mut callback) = AudioIO::bridge(config);
        let mut pages = super::super::loop_audio::OfflinePages;
        callback.core.action(Action::Trigger(0), &mut pages);
        for _ in 0..200 {
            callback.core.process([0.1, -0.1], &mut pages);
        }
        let original = (&*callback.core) as *const RenderCore;
        let frames = callback.core.tracks[0].audio.len;
        let (_, input_rx) = HeapRb::new(128).split();
        let state = Box::new(OutputState {
            callback,
            input_rx,
            drift: super::super::latency::InputAdapter::new(128),
            converter: super::super::output_resampler::OutputResampler::new(8000, 8000),
            fade_remaining: 0,
            fade_total: 1,
        });
        let (mut tx, rx) = HeapRb::new(1).split();
        assert!(tx.push(state).is_ok());
        let (returned, receive) = mpsc::channel();
        drop(OutputPump {
            state: None,
            pending: rx,
            returned,
        });
        let mut state = receive.recv().unwrap();
        assert_eq!((&*state.callback.core) as *const RenderCore, original);
        assert_eq!(state.callback.core.clock.frame, 200);
        assert_eq!(
            state.callback.core.tracks[0].mode,
            super::super::core::Mode::Recording
        );
        assert_eq!(state.callback.core.tracks[0].audio.len, frames);
        state.callback.core.process([0.2, -0.2], &mut pages);
        assert_eq!(state.callback.core.clock.frame, 201);
        assert_eq!(state.callback.core.tracks[0].audio.len, frames + 1);
    }
    #[test]
    #[ignore = "explicit local hardware test; opens streams silently, writes no audio"]
    fn real_output_handoff_and_reconnect() {
        let host = cpal::default_host();
        let input = host
            .input_devices()
            .unwrap()
            .find(|d| {
                d.supported_input_configs().is_ok_and(|mut ranges| {
                    ranges.any(|r| {
                        r.channels() == 2
                            && r.sample_format() == cpal::SampleFormat::F32
                            && r.min_sample_rate().0 <= 48000
                            && r.max_sample_rate().0 >= 48000
                    })
                })
            })
            .expect("Stereo input");
        let mut audio = AudioIO::with_system_output(&input.name().unwrap(), 128).unwrap();
        assert!(audio.online);
        let original = audio.curr_output_name().to_owned();
        let watch = super::super::output_watch::OutputWatch::new();
        std::thread::sleep(Duration::from_millis(550));
        let watched = watch.poll().unwrap().unwrap();
        assert_eq!(watched.name, original);
        assert!(!watched.id.is_empty());
        let alternative = host
            .output_devices()
            .unwrap()
            .find(|d| d.name().is_ok_and(|n| n != original))
            .expect("Second output for handoff test");
        std::thread::sleep(Duration::from_millis(100));
        audio.retarget_output(&alternative, 128).unwrap();
        assert_eq!(audio.curr_output_name(), alternative.name().unwrap());
        audio.park_output();
        assert!(!audio.online);
        assert!(audio.parked_output.is_some());
        let default = host.default_output_device().unwrap();
        audio.retarget_output(&default, 128).unwrap();
        std::thread::sleep(Duration::from_millis(150));
        assert!(audio.online);
        assert_eq!(audio.curr_output_name(), original);
        assert!(audio.diagnostics.callback_frames.load(Ordering::Relaxed) > 0);
        assert_eq!(audio.diagnostics.output_errors.load(Ordering::Relaxed), 0);
        eprintln!(
            "Silent device handoff passed: {} -> {} -> {}",
            original,
            alternative.name().unwrap(),
            audio.curr_output_name()
        );
    }
}
