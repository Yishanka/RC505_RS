mod actions;
pub(crate) mod clear_gesture;
pub mod faders;
mod keyboard;
mod monitoring;
mod performance_keys;
mod workflow;
use crate::{
    config::AppConfig,
    engine::{
        audio_io::{AudioIO, Control, Response},
        core::{EngineView, RenderCore},
    },
    project::{self, ProjectEntry},
    state::{AppState, PendingExit, ProjectNameMode},
    ui,
};
use eframe::egui;
use std::{
    fs::File,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Focus {
    #[default]
    Performance,
    Transport,
    Left,
    Right,
    Editor,
}
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum LeftPage {
    #[default]
    Track,
    Audio,
    Session,
}
#[derive(Clone, Copy)]
pub struct HeldFx {
    pub bank: usize,
    pub slot: usize,
    pub track: Option<usize>,
    pub previous: bool,
}
pub enum JobResult {
    PlayerReady(Vec<[f32; 2]>),
    Update(crate::updater::Release),
    UpdateDownloaded(PathBuf),
    Loaded {
        index: usize,
        data: project::ProjectData,
        core: Box<RenderCore>,
    },
    Saved,
    Rendered {
        root: PathBuf,
        result: crate::replay::RenderResult,
    },
    Error(String),
}
pub struct MyApp {
    pub audition_target: Option<(crate::presets::FxTarget, usize)>,
    pub calibration_open: bool,
    pub loopback_connected: bool,
    pub visualizer_enabled: bool,
    pub replay_autoplay: bool,
    last_ui_scene: Option<[u64; 16]>,
    output_watch: Option<crate::engine::output_watch::OutputWatch>,
    output_endpoint_id: String,
    output_error_seen: u64,
    next_output_retry: std::time::Instant,
    performance_keys: performance_keys::PerformanceKeys,
    fader_keys: [faders::KeyFader; 5],
    speed_keys: [faders::KeyFader; 5],
    pub editor: ui::editor::EditorState,
    pub previewing: bool,
    pub status: String,
    pub config: AppConfig,
    pub audio: AudioIO,
    pub view: EngineView,
    pub app_state: AppState,
    pub track_sel: Option<usize>,
    pub focus: Focus,
    pub focus_request: bool,
    pub left_page: LeftPage,
    pub help_open: bool,
    pub help_tab: usize,
    pub buffer_frames: u32,
    pub measurement: Option<crate::engine::latency::Measurement>,
    pub projects: Vec<ProjectEntry>,
    pub sel_project_idx: usize,
    pub project_name_input: String,
    pub project_name_mode: Option<ProjectNameMode>,
    pub active_project_idx: Option<usize>,
    pending_exit: Option<PendingExit>,
    show_save_prompt: bool,
    exit_after_save: bool,
    allow_window_close: bool,
    close_window_queued: bool,
    pub saving: bool,
    pub take_pending: bool,
    take_ending: bool,
    pub draft: Option<PathBuf>,
    pub take_name: String,
    pub replay_list: Vec<(PathBuf, String)>,
    pub player_open: bool,
    pub rendered: Option<(PathBuf, crate::replay::RenderResult)>,
    pub render_progress: Arc<AtomicU64>,
    pub replay_import_path: String,
    pub replay_browser: bool,
    pub reconnecting: bool,
    pub job: Option<mpsc::Receiver<JobResult>>,
    pub engine_transition: bool,
    last_config: Vec<u8>,
    _editor_lock: Option<File>,
    pub read_only: bool,
    pub held_fx: [Option<HeldFx>; 8],
    pub clear_gesture: clear_gesture::ClearGesture,
    pub language: crate::app_support::language::Language,
    pub update: Option<crate::updater::Release>,
    pub update_installer: Option<PathBuf>,
    update_after_save: bool,
    #[cfg(debug_assertions)]
    preview_frame: usize,
}

impl MyApp {
    pub fn new() -> Self {
        let launch = crate::app_support::launcher_config::load();
        let language = launch.as_ref().map(|v| v.language).unwrap_or_default();
        let guard = launch.as_ref().is_some_and(|v| v.calibration_guard);
        let visualizer_enabled = launch.as_ref().is_none_or(|v| v.visualizer_enabled);
        let buffer_frames = launch.as_ref().map(|v| v.buffer_frames()).unwrap_or(128);
        let mut config = AppConfig::new(120, 85, 5);
        if !std::env::args().any(|v| v == "--offline") {
            config.system_config.refresh();
        }
        if let Some(settings) = launch.as_ref() {
            config.beat_config.set_latency(settings.latency_comp_ms());
            if !settings.input_device.is_empty() {
                config.system_config.input_device.value = settings.input_device.clone();
            }
            config.system_config.follow_system_output = settings.follow_system_output;
            if !settings.follow_system_output && !settings.output_device.is_empty() {
                config.system_config.output_device.value = settings.output_device.clone();
            }
        }
        let mut audio = if std::env::args().any(|v| v == "--offline") {
            AudioIO::offline("Offline editing".into())
        } else {
            (if config.system_config.follow_system_output {
                AudioIO::with_system_output(&config.system_config.input_device.value, buffer_frames)
            } else {
                AudioIO::with_buffer(
                    &config.system_config.input_device.value,
                    &config.system_config.output_device.value,
                    buffer_frames,
                )
            })
            .unwrap_or_else(|e| AudioIO::offline(format!("Audio unavailable: {e}")))
        };
        if guard {
            let _ = audio.send(Control::CalibrationHold(true));
        }
        let root = crate::app_support::paths::projects_dir();
        let _ = std::fs::create_dir_all(&root);
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join("editor.lock"))
            .ok()
            .filter(|file| fs2::FileExt::try_lock_exclusive(file).is_ok());
        let read_only = lock.is_none();
        let mut projects = project::load_index();
        if projects.is_empty() && !read_only {
            projects.push(ProjectEntry {
                name: "DEFAULT".into(),
                file: project::make_project_file_name("DEFAULT", 0),
            });
            let _ = project::save_project_data(&projects[0], &project::data_from_config(&config));
            let _ = project::save_index(&projects);
        }
        let selected = launch
            .and_then(|s| projects.iter().position(|p| p.name == s.last_project))
            .unwrap_or(0);
        Self {
            audition_target: None,
            calibration_open: guard,
            loopback_connected: false,
            visualizer_enabled,
            replay_autoplay: false,
            last_ui_scene: None,
            output_endpoint_id: if config.system_config.follow_system_output && audio.online {
                crate::engine::output_watch::default_id().unwrap_or_default()
            } else {
                String::new()
            },
            output_watch: (!std::env::args().any(|v| v == "--offline"))
                .then(crate::engine::output_watch::OutputWatch::new),
            output_error_seen: 0,
            next_output_retry: std::time::Instant::now(),
            performance_keys: performance_keys::PerformanceKeys::default(),
            fader_keys: [faders::KeyFader::default(); 5],
            speed_keys: [faders::KeyFader::default(); 5],
            editor: ui::editor::EditorState::default(),
            previewing: false,
            status: if read_only {
                "Another editor owns this data folder; saving is disabled.".into()
            } else {
                String::new()
            },
            config,
            audio,
            view: EngineView::default(),
            app_state: AppState::Init,
            track_sel: Some(0),
            focus: Focus::Performance,
            focus_request: false,
            left_page: LeftPage::Track,
            help_open: false,
            help_tab: 0,
            buffer_frames,
            measurement: None,
            projects,
            sel_project_idx: selected,
            project_name_input: String::new(),
            project_name_mode: None,
            active_project_idx: None,
            pending_exit: None,
            show_save_prompt: false,
            exit_after_save: false,
            allow_window_close: false,
            close_window_queued: false,
            saving: false,
            take_pending: false,
            take_ending: false,
            draft: None,
            take_name: String::new(),
            replay_list: Vec::new(),
            player_open: false,
            rendered: None,
            render_progress: Arc::new(AtomicU64::new(0)),
            replay_import_path: String::new(),
            replay_browser: false,
            reconnecting: false,
            job: None,
            engine_transition: false,
            last_config: Vec::new(),
            _editor_lock: lock,
            read_only,
            held_fx: [None; 8],
            clear_gesture: clear_gesture::ClearGesture::default(),
            language,
            update: None,
            update_installer: None,
            update_after_save: false,
            #[cfg(debug_assertions)]
            preview_frame: 0,
        }
    }
    pub fn busy(&self) -> bool {
        self.job.is_some()
            || self.saving
            || self.take_pending
            || self.reconnecting
            || self.audio.diagnostics.calibrating.load(Ordering::Relaxed)
    }
    pub fn language_switch(&mut self, ui: &mut egui::Ui) {
        if ui::navigation::register(ui::theme::language_switch(ui, &mut self.language)).changed()
            && !self.read_only
        {
            let mut preferences = crate::app_support::launcher_config::load().unwrap_or_default();
            preferences.language = self.language;
            if let Err(error) = crate::app_support::launcher_config::save(&preferences) {
                self.status = format!("Cannot save language preference: {error}");
            }
        }
    }
    pub fn performance_locked(&self) -> bool {
        self.engine_transition
            || self.reconnecting
            || self.audio.diagnostics.calibrating.load(Ordering::Relaxed)
    }
    pub fn taking(&self) -> bool {
        self.audio.diagnostics.taking.load(Ordering::Relaxed)
    }
    pub fn stopped(&self) -> bool {
        !self.view.running && self.view.tracks.iter().all(|t| !t.pending)
    }
    pub fn beats(&self) -> Option<f64> {
        self.view.running.then(|| {
            self.view.elapsed as f64 / self.view.sample_rate as f64
                * self.config.beat_config.current_bpm() as f64
                / 60.0
        })
    }
    pub fn send(&mut self, command: Control) -> bool {
        match self.audio.send(command) {
            Ok(()) => true,
            Err(error) => {
                self.status = error.to_string();
                false
            }
        }
    }
    fn sync_config(&mut self) -> bool {
        if self.previewing
            && (self.audition_target != self.audition_selection()
                || self
                    .editor
                    .target
                    .is_none_or(|t| !crate::engine::audition::supports(&self.config, t)))
        {
            self.stop_audition();
        }
        let data = project::data_from_config(&self.config);
        let Ok(bytes) = serde_json::to_vec(&data) else {
            return false;
        };
        if self.last_config != bytes {
            match self.audio.configure(&self.config) {
                Ok(()) => {
                    self.last_config = bytes;
                    if let Some((target, track)) = self.audition_target.filter(|_| self.previewing)
                    {
                        if let Some(params) = crate::engine::audition::AuditionParameters::new(
                            &self.config,
                            target,
                            track,
                        ) {
                            self.send(Control::AuditionUpdate(Box::new(params)));
                        }
                    }
                }
                Err(error) => {
                    self.status = error.to_string();
                    return false;
                }
            }
        }
        true
    }
    fn request_exit(&mut self, target: PendingExit) {
        self.replay_autoplay = false;
        if self.taking() || self.take_pending || self.draft.is_some() {
            self.status = "Finish and save or discard the replay take first.".into();
            return;
        }
        self.pending_exit = Some(target);
        self.show_save_prompt = true;
    }
    fn finish_exit(&mut self) {
        if self.busy() {
            return;
        }
        if self.update_after_save {
            let result = self
                .update_installer
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("No verified installer"))
                .and_then(|p| crate::updater::install_after_exit(p));
            if let Err(error) = result {
                self.status = error.to_string();
                self.exit_after_save = false;
                self.update_after_save = false;
                return;
            }
            self.update_after_save = false;
        }
        self.send(Control::Player(None));
        self.stop_audition();
        self.player_open = false;
        self.send(Control::Action(crate::engine::core::Action::Panic));
        self.send(Control::Enable(false));
        self.previewing = false;
        self.editor.expanded = false;
        self.show_save_prompt = false;
        self.exit_after_save = false;
        match self.pending_exit.take() {
            Some(PendingExit::ToInit) => {
                self.app_state = AppState::Init;
                self.active_project_idx = None;
            }
            Some(PendingExit::CloseWindow) => {
                self.allow_window_close = true;
                self.close_window_queued = true;
            }
            None => {}
        }
    }
    fn poll(&mut self) {
        if let Some(view) = self.audio.poll() {
            self.view = view;
        }
        self.previewing = self.audio.diagnostics.auditioning.load(Ordering::Relaxed);
        if self.taking() && !self.take_ending {
            self.take_pending = false;
        }
        let responses: Vec<_> = self.audio.responses().collect();
        for response in responses {
            match response {
                Response::Saved(_) => {
                    self.saving = false;
                    self.status = "Configuration and audio snapshot saved.".into();
                    if self.exit_after_save {
                        self.finish_exit();
                    }
                }
                Response::Take(path) => {
                    self.take_pending = false;
                    self.take_ending = false;
                    self.draft = Some(path);
                    self.take_name =
                        format!("Take {}", chrono::Local::now().format("%Y-%m-%d %H-%M"));
                    self.status = "Replay captured. Save the take or export audio.".into();
                }
                Response::Error(error) => {
                    self.update_after_save = false;
                    self.saving = false;
                    self.take_pending = false;
                    self.take_ending = false;
                    self.exit_after_save = false;
                    self.status = error;
                }
                Response::Calibrated(value) => {
                    if value.output_generation
                        == self
                            .audio
                            .diagnostics
                            .output_generation
                            .load(Ordering::Relaxed)
                    {
                        self.measurement = Some(value);
                        self.status =
                            "Loopback measured. Review and apply the recommendation in Audio."
                                .into();
                    }
                }
                Response::Captured(snapshot) => self.finish_reconnect(*snapshot),
            }
        }
        let result = self.job.as_ref().and_then(|job| match job.try_recv() {
            Ok(v) => Some(v),
            Err(mpsc::TryRecvError::Disconnected) => Some(JobResult::Error(
                "Background task ended unexpectedly".into(),
            )),
            Err(_) => None,
        });
        if let Some(result) = result {
            self.job = None;
            self.engine_transition = false;
            match result {
                JobResult::PlayerReady(samples) => {
                    if self.replay_autoplay && !self.show_save_prompt {
                        self.player_open = self.send(Control::Player(Some(Box::new(
                            crate::engine::audio_io::Player {
                                samples,
                                cursor: 0,
                                playing: true,
                            },
                        ))));
                    }
                    self.replay_autoplay = false;
                }
                JobResult::Update(release) => {
                    self.status = if crate::updater::newer(&release.version) {
                        format!("Update {} available.", release.version)
                    } else {
                        "Already up to date.".into()
                    };
                    self.update = Some(release);
                }
                JobResult::UpdateDownloaded(path) => {
                    self.status = format!("Verified installer downloaded: {}", path.display());
                    self.update_installer = Some(path);
                }
                JobResult::Loaded { index, data, core } => {
                    self.stop_audition();
                    let input = self.config.system_config.input_device.value.clone();
                    let output = self.config.system_config.output_device.value.clone();
                    project::apply_data_to_config(&mut self.config, data);
                    self.config.system_config.input_device.value = input;
                    self.config.system_config.output_device.value = output;
                    if self
                        .config
                        .calibration
                        .as_ref()
                        .is_some_and(|c| c.buffer_frames != self.buffer_frames)
                    {
                        self.config.calibration = None;
                    }
                    if self.send(Control::Replace(core)) {
                        self.active_project_idx = Some(index);
                        self.app_state = AppState::MainLoop;
                        self.editor = ui::editor::EditorState::default();
                        self.previewing = false;
                        self.focus = Focus::Performance;
                        self.last_config.clear();
                        self.sync_config();
                        self.send(Control::Enable(true));
                        self.replay_list = crate::replay::list(&self.projects[index]);
                        self.status = "Project ready.".into();
                        let mut preferences =
                            crate::app_support::launcher_config::load().unwrap_or_default();
                        preferences.last_project = self.projects[index].name.clone();
                        preferences.input_device =
                            self.config.system_config.input_device.value.clone();
                        preferences.output_device =
                            self.config.system_config.output_device.value.clone();
                        preferences.buffer_frames = self.buffer_frames;
                        preferences.follow_system_output =
                            self.config.system_config.follow_system_output;
                        if !self.read_only {
                            if let Err(e) = crate::app_support::launcher_config::save(&preferences)
                            {
                                self.status =
                                    format!("Project ready; cannot save audio preferences: {e}");
                            }
                        }
                    }
                }
                JobResult::Saved => {
                    self.saving = false;
                    self.status =
                        "Configuration saved; the previous audio snapshot is retained.".into();
                    if self.exit_after_save {
                        self.finish_exit();
                    }
                }
                JobResult::Rendered { root, result } => {
                    self.status = format!("Rendered WAV: {}", result.wav.display());
                    self.rendered = Some((root, result));
                    if std::mem::take(&mut self.replay_autoplay) {
                        self.play_rendered();
                    }
                }
                JobResult::Error(error) => {
                    self.replay_autoplay = false;
                    self.update_after_save = false;
                    self.saving = false;
                    self.exit_after_save = false;
                    self.status = error;
                }
            }
        }
        if self.view.exhausted {
            self.status="Recording stopped: five-minute track limit or audio page pool exhausted. Existing audio is preserved.".into();
        }
    }
}
impl eframe::App for MyApp {
    #[cfg(debug_assertions)]
    fn raw_input_hook(&mut self, _ctx: &egui::Context, input: &mut egui::RawInput) {
        if std::env::args().any(|arg| arg.starts_with("--ui-preview=")) {
            input.events.retain(|e| {
                !matches!(
                    e,
                    egui::Event::PointerMoved(_) | egui::Event::PointerButton { .. }
                )
            });
            input.events.push(egui::Event::PointerGone);
        }
    }
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.render_frame(ctx);
    }
}
impl MyApp {
    fn ui_scene(&self) -> [u64; 16] {
        let target = self.editor.target.map_or(0, |target| match target {
            crate::presets::FxTarget::Input { bank, slot } => {
                1 + (bank * 4 + slot) * 16 + self.config.input_fx.slot_kind(bank, slot) as usize
            }
            crate::presets::FxTarget::Track { bank, slot } => {
                512 + (bank * 4 + slot) * 16 + self.config.track_fx.slot_kind(bank, slot) as usize
            }
        });
        [
            matches!(self.app_state, AppState::Init) as u64,
            self.editor.expanded as u64,
            self.editor.page as u64,
            target as u64,
            self.help_open as u64,
            self.help_tab as u64,
            self.replay_browser as u64,
            self.player_open as u64,
            self.draft.is_some() as u64,
            self.show_save_prompt as u64,
            self.project_name_mode.is_some() as u64,
            self.language as u64,
            self.calibration_open as u64,
            self.calibration_held() as u64,
            self.audio.diagnostics.calibrating.load(Ordering::Relaxed) as u64,
            self.measurement.is_some() as u64,
        ]
    }
    fn release_hidden_focus(&mut self, ctx: &egui::Context) {
        ctx.memory_mut(|m| m.stop_text_input());
        self.focus_request = self.focus != Focus::Performance
            && !self.show_save_prompt
            && !self.help_open
            && !self.player_open
            && !self.replay_browser;
    }
    pub(crate) fn render_frame(&mut self, ctx: &egui::Context) {
        let scene = self.ui_scene();
        if self.last_ui_scene.is_some_and(|old| old != scene) {
            self.release_hidden_focus(ctx);
        }
        // Expanded and compact editors have different widget IDs and focus scopes.
        if self.editor.expanded
            && matches!(self.focus, Focus::Performance | Focus::Left | Focus::Right)
        {
            self.focus_panel(ctx, Focus::Editor);
        } else if !self.editor.expanded && self.focus == Focus::Editor {
            self.focus_panel(ctx, Focus::Performance);
        }
        let lang = self.language;
        self.language.apply(ctx);
        #[cfg(debug_assertions)]
        if let Some(mode) =
            std::env::args().find_map(|a| a.strip_prefix("--ui-preview=").map(str::to_owned))
        {
            if ui::preview::capture(ctx, &mode, &mut self.preview_frame) {
                self.allow_window_close = true;
            }
        }
        self.poll();
        #[cfg(debug_assertions)]
        if let Some(mode) =
            std::env::args().find_map(|a| a.strip_prefix("--ui-preview=").map(str::to_owned))
        {
            ui::preview::sample_visuals(self, &mode);
        }
        self.follow_default_output();
        ctx.request_repaint_after(Duration::from_millis(16));
        if ctx.input(|i| i.viewport().close_requested())
            && !self.allow_window_close
            && self.app_state != AppState::Init
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if !self.show_save_prompt {
                self.request_exit(PendingExit::CloseWindow);
            }
        }
        self.handle_input(ctx);
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.visualizer_enabled {
                ui::visualizer::draw(ui, &self.view.output_wave);
            }
            ui.set_enabled(!self.show_save_prompt);
            match self.app_state {
                AppState::Init => ui::init::draw_init(ui, self),
                _ => ui::performance::draw(ui, self),
            }
        });
        ui::help::draw(ctx, self);
        ui::replays::draw(ctx, self);
        ui::calibration::draw(ctx, self);
        if self.show_save_prompt {
            let title = if matches!(self.pending_exit, Some(PendingExit::CloseWindow)) {
                lang.choose("Save before closing RC505 RS", "关闭 RC505 RS 前保存")
            } else {
                lang.choose("Save before returning to projects", "返回工程选择前保存")
            };
            egui::Window::new(title).id(egui::Id::new("save-session-dialog")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER,egui::Vec2::ZERO).show(ctx,|ui|{
                ui.label(lang.text("Choose what to keep in this project."));
                ui.add_enabled_ui(!self.busy(),|ui|{
                    if ui.button(lang.text("Save configuration and audio snapshot")).clicked(){self.exit_after_save=true;self.save_snapshot();}
                    if ui.button(lang.text("Save configuration only")).clicked(){self.exit_after_save=true;self.save_now();}
                    ui.label(lang.text("Configuration only keeps the previous saved audio, not the current loops."));
                    ui.horizontal(|ui|{
                        if ui.button(lang.text("Discard session changes")).clicked(){self.finish_exit();}
                        if ui::theme::action(ui, ui::theme::Icon::Back, lang.text("Cancel"), "Esc").clicked(){self.show_save_prompt=false;self.pending_exit=None;}
                    });
                });ui.label(&self.status);
            });
        }
        self.sync_config();
        let scene_after = self.ui_scene();
        if scene_after != scene {
            self.release_hidden_focus(ctx);
        }
        self.last_ui_scene = Some(scene_after);
        if self.close_window_queued {
            self.close_window_queued = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}
