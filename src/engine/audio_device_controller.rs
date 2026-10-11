//! A CPAL stream never crosses threads: it is created, played and destroyed on
//! this controller. The GUI only owns bounded commands and plain-data mirrors.
//! Drivers cannot be forcibly cancelled safely; timeout is an observation, not
//! a license to launch another unbounded sequence of blocked driver threads.
use super::*;
use std::sync::atomic::AtomicUsize;

const TIMEOUT: Duration = Duration::from_secs(5);
const MAX_CONTROLLERS: usize = 2; // the old and replacement backend during reconnect
static CONTROLLERS: AtomicUsize = AtomicUsize::new(0);

struct Permit;
impl Permit {
    fn acquire() -> Result<Self> {
        CONTROLLERS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < MAX_CONTROLLERS).then_some(count + 1)
            })
            .map(|_| Self)
            .map_err(|_| anyhow::anyhow!("The existing audio drivers are still busy. Save your progress and restart the app before reconnecting again."))
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        CONTROLLERS.fetch_sub(1, Ordering::AcqRel);
    }
}

pub struct OpenRequest {
    rx: mpsc::Receiver<Result<AudioIO>>,
    started: Instant,
    cancelled: Arc<AtomicBool>,
    finished: bool,
    handed_off: bool,
}
impl OpenRequest {
    pub fn poll(&mut self) -> Option<Result<AudioIO>> {
        if self.finished {
            return None;
        }
        match self.rx.try_recv() {
            Ok(result) => {
                self.finished = true;
                self.handed_off = result.is_ok();
                Some(result)
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.finished = true;
                Some(Err(anyhow::anyhow!(
                    "Audio device controller stopped during initialization"
                )))
            }
            Err(mpsc::TryRecvError::Empty) if self.started.elapsed() >= TIMEOUT => {
                self.finished = true;
                self.cancelled.store(true, Ordering::Release);
                Some(Err(anyhow::anyhow!(
                    "Audio device initialization timed out. The driver is still busy; editing and saving remain available."
                )))
            }
            Err(mpsc::TryRecvError::Empty) => None,
        }
    }
    pub(super) fn wait(mut self) -> Result<AudioIO> {
        loop {
            if let Some(result) = self.poll() {
                return result;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}
impl Drop for OpenRequest {
    fn drop(&mut self) {
        if !self.handed_off {
            self.cancelled.store(true, Ordering::Release);
        }
    }
}

pub(super) enum Operation {
    Output { name: Option<String>, block: u32 },
    Park,
    Suspend,
    Resume,
}
pub(super) struct State {
    pub output_config: cpal::StreamConfig,
    pub input_name: String,
    pub output_name: String,
    pub endpoint_id: String,
    pub live_input: bool,
    pub online: bool,
    pub status: String,
    pub error: Option<String>,
}
struct Completion {
    serial: u64,
    state: State,
}
pub(super) struct Client {
    tx: mpsc::SyncSender<(u64, Operation)>,
    rx: mpsc::Receiver<Completion>,
    shutdown: Arc<AtomicBool>,
    diagnostics: Arc<Diagnostics>,
    serial: u64,
    pending: Option<(u64, Instant)>,
    disconnected: bool,
}
impl Client {
    pub(super) fn request(&mut self, operation: Operation) -> Result<()> {
        anyhow::ensure!(
            !self.shutdown.load(Ordering::Acquire),
            "This audio device controller has been retired"
        );
        anyhow::ensure!(
            !self.busy(),
            "An audio device operation is already in progress"
        );
        anyhow::ensure!(
            !self.disconnected,
            "The audio device controller has stopped"
        );
        let serial = self.serial.wrapping_add(1);
        self.tx
            .try_send((serial, operation))
            .map_err(|_| anyhow::anyhow!("Audio device controller is busy or unavailable"))?;
        self.serial = serial;
        self.pending = Some((serial, Instant::now()));
        Ok(())
    }
    pub(super) fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub(super) fn timed_out(&self) -> bool {
        self.disconnected || self.pending.is_some_and(|(_, at)| at.elapsed() >= TIMEOUT)
    }
    pub(super) fn shutdown(&self) {
        self.diagnostics.shutdown.store(true, Ordering::Release);
        self.shutdown.store(true, Ordering::Release);
    }
    pub(super) fn poll(&mut self) -> Option<State> {
        match self.rx.try_recv() {
            Ok(result) => {
                if self
                    .pending
                    .is_some_and(|(serial, _)| serial == result.serial)
                {
                    self.pending = None;
                }
                Some(result.state)
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.disconnected = true;
                None
            }
            Err(mpsc::TryRecvError::Empty) => None,
        }
    }
}

pub(super) fn open(input: String, output: Option<String>, block: u32) -> Result<OpenRequest> {
    let permit = Permit::acquire()?;
    let (tx, rx) = mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    let shutdown = cancelled.clone();
    std::thread::Builder::new()
        .name("audio-device-controller".into())
        .spawn(move || {
            let _permit = permit;
            if let Err(error) = run(input, output, block, shutdown, &tx) {
                let _ = tx.send(Err(error));
            }
        })
        .context("Cannot start audio device controller")?;
    Ok(OpenRequest {
        rx,
        started: Instant::now(),
        cancelled,
        finished: false,
        handed_off: false,
    })
}

struct Devices {
    input_stream: Option<cpal::Stream>,
    output_stream: Option<cpal::Stream>,
    output_return_tx: mpsc::Sender<Box<OutputState>>,
    output_return_rx: mpsc::Receiver<Box<OutputState>>,
    parked: Option<Box<OutputState>>,
    host: cpal::Host,
    config: cpal::StreamConfig,
    output_config: cpal::StreamConfig,
    input_name: String,
    output_name: String,
    endpoint_id: String,
    status: String,
    diagnostics: Arc<Diagnostics>,
    worker_stop: Arc<AtomicBool>,
}
impl Drop for Devices {
    fn drop(&mut self) {
        // All potentially blocking destruction stays on this same owner thread.
        self.diagnostics.shutdown.store(true, Ordering::Release);
        self.worker_stop.store(true, Ordering::Release);
        self.input_stream.take();
        self.output_stream.take();
        self.parked.take();
        while self.output_return_rx.try_recv().is_ok() {}
        self.worker_stop.store(true, Ordering::Release);
    }
}
impl Devices {
    fn state(&self, error: Option<String>) -> State {
        State {
            output_config: self.output_config.clone(),
            input_name: self.input_name.clone(),
            output_name: self.output_name.clone(),
            endpoint_id: self.endpoint_id.clone(),
            live_input: self.input_stream.is_some(),
            online: self.output_stream.is_some(),
            status: error.clone().unwrap_or_else(|| self.status.clone()),
            error,
        }
    }
    fn apply(&mut self, operation: Operation) -> Result<()> {
        match operation {
            Operation::Output { name, block } => {
                let (device, id) = if let Some(name) = name {
                    let device = self
                        .host
                        .output_devices()?
                        .find(|d| d.name().ok().as_deref() == Some(&name))
                        .context("Selected output device unavailable")?;
                    (device, format!("named:{name}"))
                } else {
                    let before = super::super::output_watch::default_id()?;
                    let device = cpal::default_host()
                        .default_output_device()
                        .context("No system output device")?;
                    let after = super::super::output_watch::default_id()?;
                    anyhow::ensure!(
                        before == after,
                        "System output changed while connecting; retrying is safe"
                    );
                    (device, after)
                };
                self.retarget(&device, id, block)
            }
            Operation::Park => self.park(),
            Operation::Suspend => {
                if let Some(stream) = &self.input_stream {
                    stream.pause()?;
                }
                if let Some(stream) = &self.output_stream {
                    stream.pause()?;
                }
                Ok(())
            }
            Operation::Resume => {
                if let Some(stream) = &self.input_stream {
                    stream.play()?;
                }
                if let Some(stream) = &self.output_stream {
                    stream.play()?;
                }
                Ok(())
            }
        }
    }
    fn retarget(&mut self, device: &cpal::Device, id: String, block: u32) -> Result<()> {
        let name = device.name()?;
        let output_config = super::super::device_config::endpoint_config(
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
        // The new endpoint starts silent. The old renderer is not replaced or
        // cloned: ownership transfers only once its old stream returns it.
        let stream = device.build_output_stream(
            &output_config,
            move |data: &mut [f32], info| pump.process(data, info, channels, rate, &diagnostics),
            move |_| {
                errors.stream_errors.fetch_add(1, Ordering::Relaxed);
                errors.output_errors.fetch_add(1, Ordering::Relaxed);
            },
            None,
        )?;
        anyhow::ensure!(
            !self.diagnostics.shutdown.load(Ordering::Acquire),
            "Audio backend was retired during output initialization"
        );
        stream.play()?;
        self.output_stream.take();
        if self.parked.is_none() {
            self.parked = Some(
                self.output_return_rx
                    .recv_timeout(Duration::from_secs(1))
                    .context("Output callback did not return its state")?,
            );
        }
        let mut state = self.parked.take().context("Missing output state")?;
        state.converter =
            super::super::output_resampler::OutputResampler::new(self.config.sample_rate.0, rate);
        state.drift.reset_target(block as usize);
        state.fade_total = (rate / 200).max(1) as usize;
        state.fade_remaining = state.fade_total;
        state.callback.calibration.take();
        self.diagnostics.calibrating.store(false, Ordering::Relaxed);
        if self.diagnostics.shutdown.load(Ordering::Acquire) {
            self.parked = Some(state);
            anyhow::bail!("Audio backend was retired during output handoff");
        }
        if let Some(input) = &self.input_stream {
            if let Err(error) = input.play() {
                self.parked = Some(state);
                return Err(error.into());
            }
        }
        self.diagnostics
            .output_generation
            .fetch_add(1, Ordering::Relaxed);
        if let Err(state) = tx.push(state) {
            self.parked = Some(state);
            anyhow::bail!("Output handoff queue unavailable");
        }
        self.output_stream = Some(stream);
        self.output_name = name;
        self.output_config = output_config;
        self.endpoint_id = id;
        self.status = format!(
            "{} Hz engine -> {} Hz / {} ch output",
            self.config.sample_rate.0, rate, channels
        );
        Ok(())
    }
    fn park(&mut self) -> Result<()> {
        if let Some(input) = &self.input_stream {
            let _ = input.pause();
        }
        self.output_stream.take();
        if self.parked.is_none() {
            self.parked = self.output_return_rx.try_recv().ok();
        }
        self.diagnostics
            .output_generation
            .fetch_add(1, Ordering::Relaxed);
        self.status = "No system output device; waiting to reconnect".into();
        self.endpoint_id.clear();
        if self.diagnostics.taking.load(Ordering::Relaxed) {
            self.diagnostics.take_failed.store(true, Ordering::Relaxed);
        }
        if let Some(state) = &mut self.parked {
            state.callback.calibration.take();
        }
        self.diagnostics.calibrating.store(false, Ordering::Relaxed);
        Ok(())
    }
    fn poll_parked(&mut self) {
        if self.output_stream.is_none() {
            if self.parked.is_none() {
                self.parked = self.output_return_rx.try_recv().ok();
            }
            if let Some(state) = &mut self.parked {
                // Device loss freezes musical time, but control/save commands
                // continue on the exact renderer as long as it was returned.
                state.callback.commands();
                state.callback.publish();
            }
        }
    }
}

fn run(
    input: String,
    output: Option<String>,
    block: u32,
    shutdown: Arc<AtomicBool>,
    ready: &mpsc::Sender<Result<AudioIO>>,
) -> Result<()> {
    let host = select_host(&input, output.as_deref())?;
    let input_device = if input.is_empty() {
        host.default_input_device()
            .context("No default input device")?
    } else {
        host.input_devices()?
            .find(|d| d.name().ok().as_deref() == Some(&input))
            .context("Selected input device unavailable; select it again when reconnected")?
    };
    let input_name = input_device.name()?;
    let output_device = if let Some(name) = output.as_ref() {
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
    let config = super::super::device_config::common_config(&inputs, &outputs, block)
        .or_else(|_| super::super::device_config::endpoint_config(&inputs, 48000, block, false))?;
    anyhow::ensure!(
        !shutdown.load(Ordering::Acquire),
        "Audio initialization cancelled"
    );
    let (mut audio, callback) = AudioIO::bridge(config.clone());
    let (mut input_tx, input_rx) = HeapRb::<Frame>::new(16_384).split();
    let (output_return_tx, output_return_rx) = mpsc::channel();
    let mut devices = Devices {
        input_stream: None,
        output_stream: None,
        output_return_tx,
        output_return_rx,
        parked: Some(Box::new(OutputState {
            callback,
            input_rx,
            drift: super::super::latency::InputAdapter::new(block as usize),
            converter: super::super::output_resampler::OutputResampler::new(
                config.sample_rate.0,
                config.sample_rate.0,
            ),
            fade_remaining: 0,
            fade_total: 1,
        })),
        host,
        config: config.clone(),
        output_config: config.clone(),
        input_name,
        output_name: String::new(),
        endpoint_id: String::new(),
        status: String::new(),
        diagnostics: audio.diagnostics.clone(),
        worker_stop: audio.stop.clone(),
    };
    let diagnostics = audio.diagnostics.clone();
    let input_errors = diagnostics.clone();
    let channels = config.channels as usize;
    devices.input_stream = Some(input_device.build_input_stream(
        &config,
        move |data: &[f32], info: &cpal::InputCallbackInfo| {
            if diagnostics.shutdown.load(Ordering::Acquire) {
                return;
            }
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
                if input_tx
                    .push([
                        clean(frame[0]),
                        clean(frame.get(1).copied().unwrap_or(frame[0])),
                    ])
                    .is_err()
                {
                    diagnostics.overflow.fetch_add(1, Ordering::Relaxed);
                }
            }
        },
        move |_| {
            input_errors.stream_errors.fetch_add(1, Ordering::Relaxed);
            input_errors.input_errors.fetch_add(1, Ordering::Relaxed);
            if input_errors.taking.load(Ordering::Relaxed) {
                input_errors.take_failed.store(true, Ordering::Relaxed);
            }
        },
        None,
    )?);
    anyhow::ensure!(
        !shutdown.load(Ordering::Acquire),
        "Audio initialization cancelled"
    );
    // Resolve again at the actual handoff, bracketing the default endpoint ID.
    // The system may have changed while formats and the input were prepared.
    let initial_error = devices
        .apply(Operation::Output {
            name: output,
            block,
        })
        .err()
        .map(|e| format!("Output unavailable; waiting to reconnect: {e}"));
    let state = devices.state(initial_error);
    audio.output_config = state.output_config;
    audio.input_name = state.input_name;
    audio.output_name = state.output_name;
    audio.live_input = state.live_input;
    audio.output_endpoint_id = state.endpoint_id;
    audio.online = state.online;
    audio.status = state.status;
    audio.device_error = state.error;
    let (tx, rx) = mpsc::sync_channel(1);
    let (completion_tx, completion_rx) = mpsc::channel();
    audio.controller = Some(Client {
        tx,
        rx: completion_rx,
        shutdown: shutdown.clone(),
        diagnostics: audio.diagnostics.clone(),
        serial: 0,
        pending: None,
        disconnected: false,
    });
    anyhow::ensure!(
        !shutdown.load(Ordering::Acquire),
        "Audio initialization cancelled"
    );
    if ready.send(Ok(audio)).is_err() {
        return Ok(());
    }
    while !shutdown.load(Ordering::Acquire) {
        devices.poll_parked();
        match rx.recv_timeout(Duration::from_millis(5)) {
            Ok((serial, operation)) => {
                let error = devices.apply(operation).err().map(|e| e.to_string());
                let _ = completion_tx.send(Completion {
                    serial,
                    state: devices.state(error),
                });
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    Ok(())
}

fn select_host(_input: &str, _output: Option<&str>) -> Result<cpal::Host> {
    #[cfg(all(target_os = "windows", feature = "asio"))]
    if let Some(output) = _output {
        if let Ok(host) = cpal::host_from_id(cpal::HostId::Asio) {
            if host
                .input_devices()?
                .any(|d| d.name().ok().as_deref() == Some(_input))
                && host
                    .output_devices()?
                    .any(|d| d.name().ok().as_deref() == Some(output))
            {
                return Ok(host);
            }
        }
    }
    Ok(cpal::default_host())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_timed_out_device_operation_stays_single_flight_and_can_complete_later() {
        let (tx, commands) = mpsc::sync_channel(1);
        let (complete, rx) = mpsc::channel();
        let mut client = Client {
            tx,
            rx,
            shutdown: Arc::new(AtomicBool::new(false)),
            serial: 0,
            diagnostics: Arc::new(Diagnostics::default()),
            pending: None,
            disconnected: false,
        };
        client.request(Operation::Park).unwrap();
        let serial = client.serial;
        client.pending = Some((serial, Instant::now() - TIMEOUT - Duration::from_secs(1)));
        assert!(client.timed_out());
        for _ in 0..100 {
            assert!(client.request(Operation::Resume).is_err());
        }
        assert!(matches!(commands.try_recv().unwrap().1, Operation::Park));
        assert!(commands.try_recv().is_err());
        assert!(
            complete
                .send(Completion {
                    serial,
                    state: State {
                        output_config: cpal::StreamConfig {
                            channels: 2,
                            sample_rate: cpal::SampleRate(48000),
                            buffer_size: cpal::BufferSize::Default
                        },
                        input_name: String::new(),
                        output_name: String::new(),
                        endpoint_id: String::new(),
                        live_input: true,
                        online: false,
                        status: "parked".into(),
                        error: None,
                    }
                })
                .is_ok()
        );
        assert_eq!(client.poll().unwrap().status, "parked");
        assert!(!client.busy());
        assert!(!client.timed_out());
        client.request(Operation::Resume).unwrap();
        client.shutdown();
        assert!(client.diagnostics.shutdown.load(Ordering::Acquire));
        assert!(client.request(Operation::Resume).is_err());
    }

    #[test]
    fn opening_timeout_cancels_once_without_waiting_or_starting_another_thread() {
        let (_tx, rx) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut request = OpenRequest {
            rx,
            started: Instant::now() - TIMEOUT - Duration::from_secs(1),
            cancelled: cancelled.clone(),
            finished: false,
            handed_off: false,
        };
        assert!(request.poll().unwrap().is_err());
        assert!(cancelled.load(Ordering::Acquire));
        assert!(request.poll().is_none());
    }

    #[test]
    fn controller_permits_bound_stuck_drivers_to_two_instances() {
        let first = Permit::acquire().unwrap();
        let second = Permit::acquire().unwrap();
        for _ in 0..100 {
            assert!(Permit::acquire().is_err());
        }
        drop(first);
        let replacement = Permit::acquire().unwrap();
        drop(replacement);
        drop(second);
    }

    #[test]
    fn dropping_the_gui_audio_handle_does_not_join_a_stalled_worker() {
        fn assert_send<T: Send>() {}
        assert_send::<AudioIO>();
        let (mut audio, callback) = AudioIO::bridge(cpal::StreamConfig {
            channels: 2,
            sample_rate: cpal::SampleRate(8000),
            buffer_size: cpal::BufferSize::Fixed(128),
        });
        audio.offline = Some(callback);
        let actual_worker = audio.worker_join.take().unwrap();
        let (release, stalled) = mpsc::channel();
        audio.worker_join = Some(std::thread::spawn(move || {
            let _ = stalled.recv();
        }));
        let (done, completion) = mpsc::channel();
        let ui = std::thread::spawn(move || {
            drop(audio);
            let _ = done.send(());
        });
        let responsive = completion.recv_timeout(Duration::from_millis(500)).is_ok();
        let _ = release.send(());
        ui.join().unwrap();
        actual_worker.join().unwrap();
        assert!(responsive, "GUI destruction waited for an unrelated worker");
    }

    #[test]
    fn retiring_a_stalled_controller_releases_the_replay_writer_without_driver_teardown() {
        let (mut audio, mut callback) = AudioIO::bridge(cpal::StreamConfig {
            channels: 2,
            sample_rate: cpal::SampleRate(8000),
            buffer_size: cpal::BufferSize::Fixed(128),
        });
        let root =
            PathBuf::from("var").join(format!("stalled-controller-take-{}", crate::session::id()));
        audio
            .send(Control::BeginTake {
                retired_audition: None,
                core: Box::new(RenderCore::new(8000)),
                snapshot: Box::new(AudioSnapshot::empty(8000)),
                root: root.clone(),
                project_id: "fixture.json".into(),
                data: crate::project::data_from_config(&AppConfig::new(120, 0, 5)),
            })
            .unwrap();
        callback.commands();
        assert!(audio.diagnostics.taking.load(Ordering::Relaxed));
        let deadline = Instant::now() + Duration::from_secs(5);
        while !root.join("input.wav").exists() {
            assert!(
                Instant::now() < deadline,
                "Replay writer did not open its fixture input"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        audio.offline = Some(callback);
        let (tx, _stalled_driver) = mpsc::sync_channel(1);
        let (_completion_tx, rx) = mpsc::channel();
        audio.controller = Some(Client {
            tx,
            rx,
            shutdown: Arc::new(AtomicBool::new(false)),
            diagnostics: audio.diagnostics.clone(),
            serial: 0,
            pending: None,
            disconnected: false,
        });
        audio.park_output();
        assert!(audio.device_busy());
        let worker = audio.worker_join.take().unwrap();
        audio.retire();
        assert!(audio.stop.load(Ordering::Acquire));
        assert!(audio.diagnostics.take_failed.load(Ordering::Acquire));
        let (done, completion) = mpsc::channel();
        let joiner = std::thread::spawn(move || {
            let _ = done.send(worker.join());
        });
        completion
            .recv_timeout(Duration::from_secs(5))
            .expect("Replay worker waited for stalled driver teardown")
            .unwrap();
        joiner.join().unwrap();
        assert!(!root.exists(), "Uncommitted replay was left behind");
        assert!(
            audio.device_busy(),
            "The fake driver is intentionally still unavailable"
        );
    }
}
