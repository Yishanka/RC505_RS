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
    pub underrun: AtomicU64,
    pub overflow: AtomicU64,
    pub stream_errors: AtomicU64,
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
    pub samples: Vec<Frame>,
    pub cursor: usize,
    pub playing: bool,
}
pub enum Control {
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
        core: Box<RenderCore>,
        snapshot: Box<AudioSnapshot>,
        root: PathBuf,
        project_id: String,
        data: ProjectData,
    },
    EndTake,
    Enable(bool),
    Player(Option<Box<Player>>),
    PlayerToggle,
    Calibrate(Option<Box<super::latency::Calibration>>),
}
enum WorkerMessage {
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
        // Larger than every page in ten maximum-length 192 kHz loops. Never
        // fall back to deallocation on the realtime thread if exhausted.
        if let Err(page) = self.retired.push(page) {
            std::mem::forget(page);
        }
    }
}
struct Callback {
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
                Control::Config { parameters, .. } => self.core.configure(parameters),
                Control::Action(action) => {
                    if self.calibration.is_some() || self.player.is_some() {
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
                Control::BeginTake { core, snapshot, .. } => {
                    if !self.core.idle()
                        || self.taking
                        || self.player.is_some()
                        || self.calibration.is_some()
                    {
                        accepted = false;
                    } else {
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
                    if self.taking || !self.core.idle() {
                        accepted = false;
                    } else {
                        std::mem::swap(&mut self.player, value);
                    }
                }
                Control::PlayerToggle => {
                    if let Some(player) = &mut self.player {
                        if player.cursor >= player.samples.len() {
                            player.cursor = 0;
                        }
                        player.playing = !player.playing;
                    }
                }
                Control::Calibrate(value) => {
                    if self.taking || !self.core.idle() || self.calibration.is_some() {
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
        if let Some(player) = &mut self.player {
            let frame = if player.playing && player.cursor < player.samples.len() {
                let frame = player.samples[player.cursor];
                player.cursor += 1;
                frame
            } else {
                [0.0; 2]
            };
            if player.cursor >= player.samples.len() {
                player.playing = false;
            }
            self.diagnostics
                .player_frame
                .store(player.cursor as u64, Ordering::Relaxed);
            self.diagnostics
                .player_playing
                .store(player.playing, Ordering::Relaxed);
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
        let result = self.core.process(dry, &mut self.pages);
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
        if self.views.free_len() > 0 {
            let _ = self.views.push(self.core.view());
        }
    }
}
pub struct AudioIO {
    input_stream: Option<cpal::Stream>,
    output_stream: Option<cpal::Stream>,
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
        let host = select_host(input, output)?;
        let input_device = host
            .input_devices()?
            .find(|d| d.name().ok().as_deref() == Some(input))
            .context("Input device unavailable")?;
        let output_device = host
            .output_devices()?
            .find(|d| d.name().ok().as_deref() == Some(output))
            .context("Output device unavailable")?;
        let config = super::device_config::common_config(
            &input_device.supported_input_configs()?.collect::<Vec<_>>(),
            &output_device
                .supported_output_configs()?
                .collect::<Vec<_>>(),
            block,
        )?;
        let (mut audio, mut callback) = Self::bridge(config.clone());
        let (mut input_tx, mut input_rx) = HeapRb::<Frame>::new(16_384).split();
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
        let diagnostics = audio.diagnostics.clone();
        let output_errors = diagnostics.clone();
        let mut drift = super::latency::InputAdapter::new(block as usize);
        let output_stream = output_device.build_output_stream(
            &config,
            move |data: &mut [f32], info: &cpal::OutputCallbackInfo| {
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
                callback.commands();
                let frames = data.len() / channels;
                diagnostics
                    .callback_frames
                    .store(frames as u64, Ordering::Relaxed);
                diagnostics
                    .queue_frames
                    .store(input_rx.len() as u64, Ordering::Relaxed);
                drift.begin_block(input_rx.len(), frames);
                for frame in data.chunks_exact_mut(channels) {
                    let dry = drift.next(&mut input_rx, &diagnostics);
                    let wet = callback.frame(dry);
                    frame[0] = if channels == 1 {
                        (wet[0] + wet[1]) * 0.5
                    } else {
                        wet[0]
                    };
                    if channels > 1 {
                        frame[1] = wet[1];
                    }
                }
                callback.flush();
                callback.publish();
                diagnostics
                    .maximum_callback_ns
                    .fetch_max(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
            },
            move |_| {
                output_errors.stream_errors.fetch_add(1, Ordering::Relaxed);
            },
            None,
        )?;
        input_stream.play()?;
        output_stream.play()?;
        audio.input_stream = Some(input_stream);
        audio.output_stream = Some(output_stream);
        audio.online = true;
        audio.input_name = input.into();
        audio.output_name = output.into();
        audio.status = format!(
            "{} Hz / {} ch / requested {} frames",
            config.sample_rate.0, channels, block
        );
        Ok(audio)
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
        let sr = config.sample_rate.0;
        let (commands, command_rx) = HeapRb::new(128).split();
        let (view_tx, views) = HeapRb::new(8).split();
        let (worker_tx, mut worker_rx) = HeapRb::new(8192).split();
        let (mut page_tx, page_rx) = HeapRb::<Page>::new(256).split();
        for _ in 0..256 {
            let _ = page_tx.push(Arc::new([[0.0; 2]; PAGE_FRAMES]));
        }
        let (retire_tx, mut retire_rx) = HeapRb::<Page>::new(131_072).split();
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
        &self.output_name
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
