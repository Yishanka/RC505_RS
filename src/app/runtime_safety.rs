//! Nonblocking device lifecycle, bounded checkpoint requests and recovery UI.
use super::*;
use crate::engine::{audio_io::OpenRequest, core::AudioSnapshot, device_scan::DeviceScan};
use crate::recovery::{Info, Store, WriteOutcome};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const ACK_TIMEOUT: Duration = Duration::from_secs(2);
const RAM_PERIOD: Duration = Duration::from_secs(2);
const DISK_PERIOD: Duration = Duration::from_secs(30);

pub struct Checkpoint {
    pub audio: Arc<AudioSnapshot>,
    pub data: project::ProjectData,
    pub captured_ms: u64,
    revision: u64,
}
impl Checkpoint {
    pub fn new(audio: AudioSnapshot, data: project::ProjectData) -> Arc<Self> {
        Arc::new(Self {
            audio: Arc::new(audio),
            data,
            captured_ms: now_ms(),
            revision: 0,
        })
    }
}
pub struct OpenedRecovery {
    pub store: Option<Store>,
    pub info: Option<Info>,
    pub baseline: Arc<Checkpoint>,
    pub warning: Option<String>,
}
impl OpenedRecovery {
    pub fn prepare(
        entry: ProjectEntry,
        core: &RenderCore,
        data: &project::ProjectData,
        writable: bool,
    ) -> Self {
        let mut audio = AudioSnapshot::empty(core.sample_rate);
        core.snapshot(&mut audio, &mut crate::engine::loop_audio::OfflinePages);
        let baseline = Checkpoint::new(audio, data.clone());
        let (store, info, warning) = if writable {
            match Store::new(entry) {
                Ok(store) => match store.begin() {
                    Ok(info) => (Some(store), info, None),
                    Err(e) => (None, None, Some(format!("Recovery unavailable: {e}"))),
                },
                Err(e) => (None, None, Some(e.to_string())),
            }
        } else {
            (None, None, None)
        };
        Self {
            store,
            info,
            baseline,
            warning,
        }
    }
}
#[derive(Clone, Copy, PartialEq)]
enum Purpose {
    Periodic,
    Save,
    Reconnect,
    InputLost,
}
struct Pending {
    token: u64,
    purpose: Purpose,
    started: Instant,
    revision: u64,
}
struct Opening {
    request: OpenRequest,
    input: String,
    output: Option<String>,
    block: u32,
}
pub struct Attached {
    epoch: u64,
    audio: AudioIO,
    point: Option<Arc<Checkpoint>>,
    failure: Option<String>,
    selection: (String, Option<String>, u32),
}
enum RecoveryResult {
    Cleared {
        exit: bool,
        revision: u64,
    },
    Restored {
        data: project::ProjectData,
        core: Box<RenderCore>,
        point: Arc<Checkpoint>,
    },
}
pub struct RuntimeSafety {
    pub scan: Option<DeviceScan>,
    pub scan_status: String,
    opening: Option<Opening>,
    pub attach: Option<mpsc::Receiver<anyhow::Result<Attached>>>,
    pub decision: Option<Info>,
    store: Option<Store>,
    last: Option<Arc<Checkpoint>>,
    recovery_job: Option<mpsc::Receiver<anyhow::Result<RecoveryResult>>>,
    write_job: Option<mpsc::Receiver<anyhow::Result<WriteOutcome>>>,
    recovery_job_epoch: u64,
    write_job_epoch: u64,
    pending: Option<Pending>,
    token: u64,
    epoch: u64,
    next_capture: Instant,
    next_disk: Instant,
    last_callback: Instant,
    callback_generation: u64,
    input_errors: u64,
    pub dirty: bool,
    revision: u64,
    saved_revision: u64,
    keep_audio_recovery: bool,
    closing: bool,
    faulted: bool,
    observed_config: Vec<u8>,
}
impl RuntimeSafety {
    pub fn new(offline: bool) -> Self {
        Self {
            scan: (!offline).then(DeviceScan::new),
            scan_status: String::new(),
            opening: None,
            attach: None,
            decision: None,
            store: None,
            last: None,
            recovery_job: None,
            write_job: None,
            pending: None,
            token: 0,
            epoch: 0,
            recovery_job_epoch: 0,
            write_job_epoch: 0,
            revision: 0,
            saved_revision: 0,
            keep_audio_recovery: false,
            closing: false,
            next_capture: Instant::now(),
            next_disk: Instant::now() + DISK_PERIOD,
            last_callback: Instant::now(),
            callback_generation: 0,
            input_errors: 0,
            dirty: false,
            faulted: false,
            observed_config: Vec::new(),
        }
    }
    pub fn recovery_busy(&self) -> bool {
        self.recovery_job.is_some() || self.attach.is_some()
    }
    pub fn open_pending(&self) -> bool {
        self.opening.is_some()
    }
    pub fn changed(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.dirty = true;
    }
    pub fn config_save_started(&mut self) {
        self.saved_revision = self.revision;
        self.keep_audio_recovery = true;
    }
    pub fn observe_config(&mut self, bytes: &[u8]) {
        if self.observed_config != bytes {
            self.observed_config = bytes.to_vec();
            self.changed();
        }
    }
    pub fn finish_departure(&mut self) {
        if let Some(store) = &self.store {
            store.invalidate();
        }
        self.epoch = self.epoch.wrapping_add(1);
        self.store = None;
        self.last = None;
        self.pending = None;
        self.decision = None;
        self.closing = false;
        self.dirty = false;
        self.opening = None;
    }
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
fn receive<T>(rx: &Option<mpsc::Receiver<anyhow::Result<T>>>) -> Option<anyhow::Result<T>> {
    rx.as_ref().and_then(|r| match r.try_recv() {
        Ok(value) => Some(value),
        Err(mpsc::TryRecvError::Empty) => None,
        Err(mpsc::TryRecvError::Disconnected) => Some(Err(anyhow::anyhow!(
            "Background operation ended unexpectedly"
        ))),
    })
}
impl MyApp {
    pub fn refresh_devices(&mut self) {
        if let Some(scan) = &mut self.safety.scan {
            if scan.request() {
                self.safety.scan_status.clear();
            }
        }
    }
    pub(super) fn start_audio_open(&mut self) {
        if self.safety.opening.is_some() || self.safety.attach.is_some() {
            return;
        }
        let output = (!self.config.system_config.follow_system_output)
            .then(|| self.config.system_config.output_device.value.clone());
        let input = self.config.system_config.input_device.value.clone();
        match AudioIO::open_async(input.clone(), output.clone(), self.buffer_frames) {
            Ok(request) => {
                self.safety.opening = Some(Opening {
                    request,
                    input,
                    output,
                    block: self.buffer_frames,
                });
                self.status = self
                    .language
                    .choose("Connecting audio…", "正在连接音频设备…")
                    .into();
            }
            Err(error) => {
                self.reconnecting = false;
                self.status = error.to_string();
            }
        }
    }
    pub(super) fn reset_audition_state(&mut self) {
        self.audition_requests = Default::default();
        self.previewing = false;
        self.note_audition = false;
        self.candidate_audition = false;
        self.audition_target = None;
    }
    pub(super) fn install_recovery_context(&mut self, opened: OpenedRecovery) -> Option<String> {
        if let Some(store) = &self.safety.store {
            store.invalidate();
        }
        self.safety.epoch = self.safety.epoch.wrapping_add(1);
        self.safety.recovery_job = None;
        self.safety.write_job = None;
        self.safety.faulted = false;
        self.safety.closing = false;
        self.safety.pending = None;
        self.safety.last = Some(opened.baseline);
        self.safety.store = opened.store;
        self.safety.decision = opened.info;
        self.safety.dirty = false;
        self.safety.next_capture = Instant::now();
        self.safety.revision = 0;
        self.safety.saved_revision = 0;
        self.safety.next_disk = Instant::now() + DISK_PERIOD;
        opened.warning
    }
    fn request_checkpoint(&mut self, purpose: Purpose) -> bool {
        if self.safety.pending.is_some() || self.active_project_idx.is_none() {
            return false;
        }
        self.safety.token = self.safety.token.wrapping_add(1);
        let token = self.safety.token;
        if !self.sync_config() {
            return false;
        }
        if !self.send(Control::Checkpoint {
            token,
            snapshot: Box::new(AudioSnapshot::empty(self.audio.config.sample_rate.0)),
            data: project::data_from_config(&self.config),
        }) {
            return false;
        }
        self.safety.pending = Some(Pending {
            token,
            purpose,
            started: Instant::now(),
            revision: self.safety.revision,
        });
        self.safety.next_capture = Instant::now() + RAM_PERIOD;
        true
    }
    pub(super) fn save_audio_checkpoint(&mut self) {
        if self.busy() || self.read_only {
            return;
        }
        // A periodic request can be superseded. Its late reply has no authority
        // to save a project or complete a different manual action.
        self.safety.pending = None;
        self.safety.keep_audio_recovery = false;
        self.saving = self.request_checkpoint(Purpose::Save);
        if self.saving {
            self.status = self
                .language
                .choose("Saving configuration and audio…", "正在保存配置与音频…")
                .into();
        }
    }
    pub(super) fn request_reconnect_checkpoint(&mut self) {
        if self.busy() || self.taking() || !self.tempo_edit_allowed() {
            return;
        }
        self.safety.pending = None;
        if self.active_project_idx.is_some() {
            self.reconnecting = self.request_checkpoint(Purpose::Reconnect);
        } else {
            self.start_audio_open();
        }
    }
    pub(super) fn accept_checkpoint(
        &mut self,
        token: u64,
        snapshot: Box<AudioSnapshot>,
        data: project::ProjectData,
    ) {
        if !self
            .safety
            .pending
            .as_ref()
            .is_some_and(|p| p.token == token)
        {
            return;
        }
        let request = self.safety.pending.take().unwrap();
        let point = Arc::new(Checkpoint {
            audio: Arc::from(snapshot),
            data,
            captured_ms: now_ms(),
            revision: request.revision,
        });
        self.safety.last = Some(point.clone());
        match request.purpose {
            Purpose::Periodic => {}
            Purpose::Save => {
                self.safety.saved_revision = request.revision;
                let Some(entry) = self
                    .active_project_idx
                    .and_then(|i| self.projects.get(i))
                    .cloned()
                else {
                    self.saving = false;
                    return;
                };
                let (tx, rx) = mpsc::channel();
                self.job = Some(rx);
                std::thread::spawn(move || {
                    let result =
                        crate::session::save_snapshot(&entry, &point.audio, point.data.clone());
                    let _ = tx.send(match result {
                        Ok(_) => JobResult::Saved,
                        Err(e) if e.is::<project::storage::SaveCleanupWarning>() => {
                            JobResult::SavedCleanupWarning(e.to_string())
                        }
                        Err(e) => JobResult::Error(format!("Save failed: {e}")),
                    });
                });
            }
            Purpose::Reconnect => {
                self.start_audio_open();
            }
            Purpose::InputLost => self.recover_audio_bridge(
                self.language
                    .choose("Input device disconnected", "输入设备已断开")
                    .into(),
            ),
        }
    }
    fn prepare_audio_bridge(&mut self, mut candidate: Option<AudioIO>, failure: Option<String>) {
        if self.safety.attach.is_some() {
            return;
        }
        let point = self.safety.last.clone();
        let data = project::data_from_config(&self.config);
        let active = self.active_project_idx.is_some();
        let epoch = self.safety.epoch;
        let selection = (
            self.config.system_config.input_device.value.clone(),
            (!self.config.system_config.follow_system_output)
                .then(|| self.config.system_config.output_device.value.clone()),
            self.buffer_frames,
        );
        let sr = self.audio.config.sample_rate.0;
        let (tx, rx) = mpsc::channel();
        self.safety.attach = Some(rx);
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<Attached> {
                let mut audio = candidate.take().unwrap_or_else(|| {
                    AudioIO::offline_at(sr, "Audio disconnected; offline editing".into())
                });
                let new_point = if active {
                    let mut snapshot = point
                        .as_ref()
                        .map(|p| p.audio.shared_clone())
                        .unwrap_or_else(|| AudioSnapshot::empty(sr));
                    crate::session::resample(&mut snapshot, audio.config.sample_rate.0)?;
                    let mut config = AppConfig::new(120, 0, 5);
                    project::apply_data_to_config(&mut config, data.clone());
                    let baseline = Arc::new(Checkpoint {
                        audio: Arc::new(snapshot.shared_clone()),
                        data: data.clone(),
                        captured_ms: point.as_ref().map_or_else(now_ms, |p| p.captured_ms),
                        revision: point.as_ref().map_or(0, |p| p.revision),
                    });
                    let mut core = Box::new(RenderCore::new(audio.config.sample_rate.0));
                    core.configure(&mut crate::engine::core::Parameters::from_config(
                        &config,
                        audio.config.sample_rate.0,
                    ));
                    core.restore(&mut snapshot);
                    audio.send(Control::Replace(core))?;
                    Some(baseline)
                } else {
                    None
                };
                Ok(Attached {
                    epoch,
                    audio,
                    point: new_point,
                    failure,
                    selection,
                })
            })();
            let _ = tx.send(result);
        });
    }
    fn recover_audio_bridge(&mut self, reason: String) {
        self.saving = false;
        self.reconnecting = false;
        self.safety.pending = None;
        if self.safety.closing || self.safety.faulted || self.safety.attach.is_some() {
            return;
        }
        self.safety.faulted = true;
        self.safety.pending = None;
        self.safety.opening = None;
        self.saving = false;
        self.reconnecting = false;
        self.take_pending = false;
        self.take_ending = false;
        self.exit_after_save = false;
        self.audio.retire();
        self.audio.online = false;
        self.reset_audition_state();
        self.player_open = false;
        self.replay_panel = None;
        self.prepare_audio_bridge(None, Some(reason));
    }
    fn persist_recovery_if_needed(&mut self) {
        if self.read_only
            || self.safety.closing
            || self.safety.decision.is_some()
            || self.safety.recovery_job.is_some()
            || self.safety.pending.is_some()
            || self.safety.write_job.is_some()
            || !self.safety.dirty
            || Instant::now() < self.safety.next_disk
        {
            return;
        }
        let (Some(store), Some(point)) = (self.safety.store.clone(), self.safety.last.clone())
        else {
            return;
        };
        let ticket = store.ticket_at(point.captured_ms);
        let (tx, rx) = mpsc::channel();
        self.safety.write_job = Some(rx);
        self.safety.write_job_epoch = self.safety.epoch;
        self.safety.dirty = self.safety.revision != point.revision;
        self.safety.next_disk = Instant::now() + DISK_PERIOD;
        std::thread::spawn(move || {
            let _ = tx.send(store.save(&ticket, &point.audio, &point.data));
        });
    }
    pub(super) fn clear_recovery_after_save(&mut self) {
        self.saving = false;
        if self.safety.keep_audio_recovery && !self.exit_after_save {
            self.safety.dirty = true;
            return;
        }
        self.clear_recovery(self.exit_after_save);
    }
    pub(super) fn discard_session(&mut self) {
        self.clear_recovery(true);
    }
    fn clear_recovery(&mut self, exit: bool) {
        self.safety.pending = None;
        self.safety.closing = exit;
        if exit {
            self.safety.opening = None;
            self.safety.attach = None;
            self.reconnecting = false;
        }
        let Some(store) = self.safety.store.clone() else {
            if exit {
                self.finish_exit();
            }
            return;
        };
        let ticket = store.invalidate();
        let revision = if self.safety.decision.is_some() {
            self.safety.revision
        } else {
            self.safety.saved_revision
        };
        let (tx, rx) = mpsc::channel();
        self.safety.recovery_job = Some(rx);
        self.safety.recovery_job_epoch = self.safety.epoch;
        std::thread::spawn(move || {
            let result = (|| {
                anyhow::ensure!(store.clear(&ticket)?, "Recovery cleanup was superseded");
                Ok(RecoveryResult::Cleared { exit, revision })
            })();
            let _ = tx.send(result);
        });
    }
    fn restore_recovery(&mut self) {
        let (Some(store), Some(info)) = (self.safety.store.clone(), self.safety.decision.clone())
        else {
            return;
        };
        let sr = self.audio.config.sample_rate.0;
        let (tx, rx) = mpsc::channel();
        self.safety.recovery_job = Some(rx);
        self.safety.recovery_job_epoch = self.safety.epoch;
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<RecoveryResult> {
                let (mut snapshot, data) = store.load(&info)?;
                crate::session::resample(&mut snapshot, sr)?;
                let point = Checkpoint::new(snapshot.shared_clone(), data.clone());
                let mut config = AppConfig::new(120, 0, 5);
                project::apply_data_to_config(&mut config, data.clone());
                let mut core = Box::new(RenderCore::new(sr));
                core.configure(&mut crate::engine::core::Parameters::from_config(
                    &config, sr,
                ));
                core.restore(&mut snapshot);
                Ok(RecoveryResult::Restored { data, core, point })
            })();
            let _ = tx.send(result);
        });
    }
    pub(super) fn poll_runtime_safety(&mut self) {
        if self.audio.online
            && !self.audio.device_busy()
            && self.config.system_config.follow_system_output
        {
            self.output_endpoint_id = self.audio.output_endpoint_id.clone();
            self.config.system_config.output_device.value =
                self.audio.curr_output_name().to_owned();
        }
        if let Some(result) = self.safety.scan.as_mut().and_then(DeviceScan::poll) {
            match result {
                Ok(devices) => {
                    self.config.system_config.apply_devices(&devices);
                    self.safety.scan_status = devices.warnings.join("\n");
                }
                Err(error) => {
                    self.safety.scan_status =
                        if self.safety.scan.as_ref().is_some_and(|s| s.timed_out()) {
                            self.language
                                .choose(
                                    "Device scan timed out; editing and saving remain available.",
                                    "设备扫描超时，仍可编辑和保存。",
                                )
                                .into()
                        } else {
                            error
                        }
                }
            }
        }
        if let Some(result) = self.safety.opening.as_mut().and_then(|o| o.request.poll()) {
            let request = self.safety.opening.take().unwrap();
            let selected_output = (!self.config.system_config.follow_system_output)
                .then(|| self.config.system_config.output_device.value.clone());
            match result {
                Ok(audio)
                    if self.buffer_frames == request.block
                        && selected_output == request.output
                        && (self.config.system_config.input_device.value == request.input
                            || (request.input.is_empty()
                                && self.config.system_config.input_device.value
                                    == audio.curr_input_name())) =>
                {
                    self.prepare_audio_bridge(Some(audio), None)
                }
                Ok(mut audio) => {
                    audio.retire();
                    std::thread::spawn(move || drop(audio));
                    self.reconnecting = false;
                    self.status = self
                        .language
                        .choose(
                            "Device selection changed; reconnect to apply it",
                            "设备选择已改变，请重新连接",
                        )
                        .into();
                }
                Err(error) => {
                    self.reconnecting = false;
                    self.status = format!(
                        "{}: {error}",
                        self.language.choose(
                            "Audio unavailable; editing and saving remain available",
                            "音频设备不可用，仍可编辑和保存"
                        )
                    );
                }
            }
        }
        if let Some(result) = receive(&self.safety.attach) {
            self.safety.attach = None;
            let selection = (
                self.config.system_config.input_device.value.clone(),
                (!self.config.system_config.follow_system_output)
                    .then(|| self.config.system_config.output_device.value.clone()),
                self.buffer_frames,
            );
            match result {
                Ok(mut attached)
                    if attached.epoch == self.safety.epoch
                        && (attached.failure.is_some() || attached.selection == selection) =>
                {
                    let calibration_hold = self.calibration_held();
                    self.audio.retire();
                    let old = std::mem::replace(&mut self.audio, attached.audio);
                    // Backend destruction is nonblocking; release large offline
                    // renderer allocations on a bounded, short-lived worker.
                    std::thread::spawn(move || drop(old));
                    self.reset_audition_state();
                    self.reconnecting = false;
                    self.last_config.clear();
                    self.safety.input_errors = 0;
                    self.safety.callback_generation = 0;
                    self.safety.last_callback = Instant::now();
                    self.output_endpoint_id = self.audio.output_endpoint_id.clone();
                    if self.audio.online {
                        self.config.system_config.output_device.value =
                            self.audio.curr_output_name().to_owned();
                        self.config.system_config.input_device.value =
                            self.audio.curr_input_name().to_owned();
                        self.safety.faulted = false;
                    }
                    self.loopback_connected = false;
                    self.measurement = None;
                    self.config.calibration = None;
                    if let Some(point) = attached.point.take() {
                        self.safety.last = Some(Arc::new(Checkpoint {
                            audio: point.audio.clone(),
                            data: project::data_from_config(&self.config),
                            captured_ms: point.captured_ms,
                            revision: self.safety.revision,
                        }));
                    }
                    let (dirty, revision) = (self.safety.dirty, self.safety.revision);
                    self.sync_config();
                    self.safety.dirty = dirty;
                    self.safety.revision = revision;
                    self.send(Control::Spectrum(self.visualizer_enabled));
                    self.send(Control::CalibrationHold(calibration_hold));
                    self.send(Control::Enable(
                        self.active_project_idx.is_some() && self.safety.decision.is_none(),
                    ));
                    if let Some(reason) = attached.failure {
                        self.status = if let Some(point) = &self.safety.last {
                            let age = now_ms().saturating_sub(point.captured_ms) / 1000;
                            format!("{} ({age}s): {reason}",self.language.choose("Audio interrupted; retained the latest available checkpoint. Save or reconnect","音频已中断，已保留最近可用检查点，可保存或重连"))
                        } else {
                            format!(
                                "{}: {reason}",
                                self.language.choose(
                                    "Audio disconnected; offline editing is available",
                                    "音频已断开，可以离线编辑"
                                )
                            )
                        };
                        self.safety.dirty = true;
                        self.safety.next_disk = Instant::now();
                    } else {
                        self.status = self
                            .language
                            .choose(
                                "Audio connected; tracks are stopped",
                                "音频已连接，轨道保持停止",
                            )
                            .into();
                    }
                }
                Ok(attached)
                    if attached.epoch != self.safety.epoch
                        && !self.safety.closing
                        && attached.selection == selection =>
                {
                    self.prepare_audio_bridge(Some(attached.audio), attached.failure);
                }
                Ok(attached) => {
                    self.reconnecting = false;
                    std::thread::spawn(move || drop(attached));
                }
                Err(error) => {
                    self.reconnecting = false;
                    self.saving = false;
                    self.safety.pending = None;
                    self.status = error.to_string();
                }
            }
        }
        if let Some(result) = receive(&self.safety.recovery_job) {
            self.safety.recovery_job = None;
            match result {
                _ if self.safety.recovery_job_epoch != self.safety.epoch => {}
                Ok(RecoveryResult::Cleared { exit, revision }) => {
                    self.safety.decision = None;
                    self.safety.dirty = self.safety.revision != revision;
                    if exit {
                        self.safety.epoch = self.safety.epoch.wrapping_add(1);
                        self.safety.store = None;
                        self.safety.last = None;
                        self.finish_exit();
                    } else {
                        self.send(Control::Enable(self.active_project_idx.is_some()));
                    }
                }
                Ok(RecoveryResult::Restored { data, core, point }) => {
                    let input = self.config.system_config.input_device.value.clone();
                    let output = self.config.system_config.output_device.value.clone();
                    project::apply_data_to_config(&mut self.config, data);
                    self.config.system_config.input_device.value = input;
                    self.config.system_config.output_device.value = output;
                    self.send(Control::Replace(core));
                    self.send(Control::Enable(true));
                    self.safety.last = Some(point);
                    self.safety.decision = None;
                    self.last_config.clear();
                    self.safety.dirty = true;
                    self.status = self
                        .language
                        .choose(
                            "Recovery loaded into the working session; save to keep it",
                            "恢复点已载入工作区，保存后保留",
                        )
                        .into();
                }
                Err(error) => {
                    self.exit_after_save = false;
                    self.safety.closing = false;
                    self.status = format!(
                        "{}: {error}",
                        self.language
                            .choose("Recovery operation failed", "恢复操作失败")
                    );
                }
            }
        }
        if let Some(result) = receive(&self.safety.write_job) {
            self.safety.write_job = None;
            match result {
                _ if self.safety.write_job_epoch != self.safety.epoch => {}
                Ok(WriteOutcome::Saved(_)) | Ok(WriteOutcome::Superseded) => {}
                Ok(WriteOutcome::SavedCleanupWarning(_, error)) => {
                    self.status = error;
                }
                Err(error) => {
                    self.status = format!(
                        "{}: {error}",
                        self.language.choose(
                            "Recovery checkpoint could not be completed",
                            "恢复检查点未能完成"
                        )
                    );
                    self.safety.dirty = true;
                }
            }
        }
        let now = Instant::now();
        let generation = self
            .audio
            .diagnostics
            .callback_generation
            .load(Ordering::Relaxed);
        if generation != self.safety.callback_generation {
            self.safety.callback_generation = generation;
            self.safety.last_callback = now;
        }
        let input_errors = self.audio.diagnostics.input_errors.load(Ordering::Relaxed);
        if input_errors != self.safety.input_errors && !self.safety.faulted && !self.safety.closing
        {
            self.safety.input_errors = input_errors;
            self.audio
                .diagnostics
                .take_failed
                .store(true, Ordering::Relaxed);
            self.send(Control::Action(crate::engine::core::Action::Panic));
            self.safety.pending = None;
            if !self.request_checkpoint(Purpose::InputLost) {
                self.recover_audio_bridge("Input device disconnected".into());
            }
        }
        let missing_callback = self.audio.online
            && self.audio.has_live_input()
            && now.duration_since(self.safety.last_callback) > ACK_TIMEOUT;
        let missing_ack = self
            .safety
            .pending
            .as_ref()
            .is_some_and(|p| now.duration_since(p.started) > ACK_TIMEOUT);
        let callback_alive = self.audio.online
            && self.audio.has_live_input()
            && now.duration_since(self.safety.last_callback) <= ACK_TIMEOUT;
        let input_lost_ack = missing_ack
            && self
                .safety
                .pending
                .as_ref()
                .is_some_and(|p| p.purpose == Purpose::InputLost);
        if missing_ack && callback_alive && !input_lost_ack {
            self.safety.pending = None;
            self.saving = false;
            self.reconnecting = false;
            self.exit_after_save = false;
            self.safety.next_capture = now + Duration::from_secs(5);
            self.status = self
                .language
                .choose(
                    "Audio is running, but the snapshot request timed out. Retry saving.",
                    "音频仍在运行，但快照请求超时，请重试保存。",
                )
                .into();
        }
        if !self.safety.closing
            && ((self.audio.device_timed_out() && !callback_alive)
                || missing_callback
                || (missing_ack && !callback_alive)
                || input_lost_ack)
        {
            self.recover_audio_bridge(
                self.language
                    .choose("Audio device stopped responding", "音频设备停止响应")
                    .into(),
            );
        }
        if self.active_project_idx.is_some()
            && !self.safety.closing
            && !self.reconnecting
            && !self.safety.recovery_busy()
            && !self.audio.diagnostics.calibrating.load(Ordering::Relaxed)
            && !self.engine_transition
            && self.safety.decision.is_none()
            && self.safety.pending.is_none()
            && now >= self.safety.next_capture
        {
            self.request_checkpoint(Purpose::Periodic);
        }
        if self.view.tracks.iter().any(|t| {
            matches!(
                t.mode,
                crate::engine::core::Mode::Recording | crate::engine::core::Mode::Overdub
            )
        }) {
            self.safety.changed();
        }
        self.persist_recovery_if_needed();
    }
    pub(super) fn draw_recovery_prompt(&mut self, ctx: &egui::Context) {
        let Some(info) = self.safety.decision.clone() else {
            return;
        };
        let lang = self.language;
        let mut restore = false;
        let mut saved = false;
        let mut cancel = false;
        egui::Window::new(lang.choose("Recover unsaved work", "恢复未保存的进度"))
            .id(egui::Id::new("recovery-prompt"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(&info.project_name);
                let time = chrono::DateTime::from_timestamp_millis(info.captured_unix_ms as i64)
                    .map(|v| {
                        v.with_timezone(&chrono::Local)
                            .format("%Y-%m-%d %H:%M:%S")
                            .to_string()
                    })
                    .unwrap_or_default();
                ui.label(format!("{} {time}", lang.choose("Checkpoint:", "检查点：")));
                ui.label(lang.choose(
                    "The saved project is unchanged. Choose which version to open.",
                    "已保存的工程未改变。请选择要打开的版本。",
                ));
                ui.add_enabled_ui(!self.busy(), |ui| {
                    restore = ui
                        .button(lang.choose("Recover checkpoint", "恢复检查点"))
                        .clicked();
                    saved = ui
                        .button(lang.choose(
                            "Discard recovery and use saved project",
                            "丢弃恢复点，打开已保存工程",
                        ))
                        .clicked();
                    cancel = ui.button(lang.choose("Cancel", "取消")).clicked();
                });
                ui.label(&self.status);
            });
        if restore {
            self.restore_recovery();
        } else if saved {
            self.clear_recovery(false);
        } else if cancel {
            self.cancel_recovery_offer();
        }
    }
    pub(super) fn cancel_recovery_offer(&mut self) {
        if self.safety.recovery_busy() {
            return;
        }
        if let Some(store) = &self.safety.store {
            store.invalidate();
        }
        self.safety.pending = None;
        self.safety.decision = None;
        self.safety.store = None;
        self.safety.last = None;
        self.safety.epoch = self.safety.epoch.wrapping_add(1);
        self.safety.dirty = false;
        self.send(Control::Enable(false));
        self.send(Control::Action(crate::engine::core::Action::Panic));
        self.active_project_idx = None;
        self.app_state = AppState::Init;
    }
    pub(super) fn cancel_audio_wait_for_exit(&mut self) {
        if self.safety.pending.is_some() {
            self.safety.pending = None;
            self.saving = false;
        }
        self.reconnecting = false;
        self.safety.opening = None;
        self.safety.attach = None;
    }
    #[cfg(debug_assertions)]
    pub(crate) fn regression_stall_checkpoint(&mut self) {
        self.audio.retire();
        if let Some(p) = &mut self.safety.pending {
            p.started = Instant::now() - Duration::from_secs(3);
        }
    }
    #[cfg(debug_assertions)]
    pub(crate) fn regression_recovery_state(&self) -> (bool, Option<String>, usize, [f32; 2]) {
        (
            self.safety.dirty,
            self.safety.store.as_ref().map(|s| s.entry().file.clone()),
            self.safety
                .last
                .as_ref()
                .map_or(0, |p| p.audio.tracks[0].len),
            self.safety
                .last
                .as_ref()
                .map_or([0.0; 2], |p| p.audio.tracks[0].read(0)),
        )
    }
    #[cfg(debug_assertions)]
    pub(crate) fn regression_recovery_due(&mut self) {
        self.safety.next_capture = Instant::now();
        self.safety.next_disk = Instant::now();
    }
}
