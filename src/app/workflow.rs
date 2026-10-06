use super::*;
use crate::engine::core::{AudioSnapshot, Parameters};

impl MyApp {
    pub(super) fn saved_cleanup_warning(&mut self, error: String) {
        self.saving = false;
        self.exit_after_save = false;
        self.update_after_save = false;
        self.status = format!(
            "{} {error}",
            self.language.choose(
                "Saved; unused audio cleanup needs attention in Storage.",
                "已保存；部分未引用音频未能清理，请在数据管理中重试。"
            )
        );
    }
    pub fn set_follow_output(&mut self, follow: bool) {
        self.config.system_config.follow_system_output = follow;
        self.output_endpoint_id.clear();
        self.next_output_retry = std::time::Instant::now();
        if !self.read_only {
            let mut preferences = crate::app_support::launcher_config::load().unwrap_or_default();
            preferences.follow_system_output = follow;
            if !follow {
                preferences.output_device = self.config.system_config.output_device.value.clone();
            }
            if let Err(error) = crate::app_support::launcher_config::save(&preferences) {
                self.status = error.to_string();
            }
        }
    }
    pub(super) fn follow_default_output(&mut self) {
        let Some(result) = self.output_watch.as_ref().and_then(|watch| watch.poll()) else {
            return;
        };
        if !self.config.system_config.follow_system_output
            || self.reconnecting
            || self.engine_transition
            || !self.audio.has_live_input()
        {
            return;
        }
        if std::time::Instant::now() < self.next_output_retry {
            return;
        }
        match result {
            Ok(target) => {
                let errors = self.audio.diagnostics.output_errors.load(Ordering::Relaxed);
                if self.audio.online
                    && target.id == self.output_endpoint_id
                    && target.name == self.audio.curr_output_name()
                    && errors == self.output_error_seen
                {
                    return;
                }
                match self
                    .audio
                    .retarget_output(&target.device, self.buffer_frames)
                {
                    Ok(()) => {
                        self.output_endpoint_id = target.id;
                        self.output_error_seen = errors;
                        self.config.system_config.output_device.value = target.name.clone();
                        if !self
                            .config
                            .system_config
                            .output_device
                            .options
                            .contains(&target.name)
                        {
                            self.config
                                .system_config
                                .output_device
                                .options
                                .push(target.name.clone());
                        }
                        self.loopback_connected = false;
                        self.config.calibration = None;
                        self.measurement = None;
                        self.status = format!(
                            "{}: {}",
                            self.language
                                .choose("Following system output", "已跟随系统输出"),
                            target.name
                        );
                    }
                    Err(error) => {
                        self.audio.park_output();
                        self.status = format!(
                            "{}: {error}",
                            self.language
                                .choose("Cannot switch output; retrying", "输出切换失败，正在重试")
                        );
                        self.next_output_retry = std::time::Instant::now() + Duration::from_secs(2);
                    }
                }
            }
            Err(error) => {
                self.audio.park_output();
                self.output_endpoint_id.clear();
                self.loopback_connected = false;
                self.config.calibration = None;
                self.measurement = None;
                self.status = format!(
                    "{}: {error}",
                    self.language
                        .choose("Waiting for system output", "等待系统输出设备")
                );
            }
        }
    }
    pub fn check_update(&mut self) {
        if self.busy() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.job = Some(rx);
        self.status = "Checking the GitHub release channel...".into();
        std::thread::spawn(move || {
            let _ = tx.send(match crate::updater::check() {
                Ok(v) => JobResult::Update(v),
                Err(e) => JobResult::Error(e.to_string()),
            });
        });
    }
    pub fn download_update(&mut self) {
        if self.busy() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.job = Some(rx);
        self.status = "Downloading and verifying update...".into();
        std::thread::spawn(move || {
            let _ = tx.send(match crate::updater::download() {
                Ok(v) => JobResult::UpdateDownloaded(v),
                Err(e) => JobResult::Error(e.to_string()),
            });
        });
    }
    pub fn install_update(&mut self) {
        if self.busy() || self.taking() || self.draft.is_some() || !self.stopped() {
            return;
        }
        self.pending_exit = Some(PendingExit::CloseWindow);
        self.update_after_save = true;
        if self.active_project_idx.is_some() {
            self.exit_after_save = true;
            self.save_snapshot();
        } else {
            self.finish_exit();
        }
    }
    pub fn reconnect(&mut self) {
        if self.calibration_held() {
            self.status = self
                .language
                .text("Disconnect the loopback cable and restore monitoring first")
                .into();
            return;
        }
        self.stop_audition();
        if self.busy() || !self.stopped() || self.taking() {
            return;
        }
        if self.player_open {
            self.close_player();
        }
        self.reconnecting = self.send(Control::Capture(Box::new(AudioSnapshot::empty(
            self.audio.config.sample_rate.0,
        ))));
    }
    pub(super) fn finish_reconnect(&mut self, mut snapshot: AudioSnapshot) {
        self.reconnecting = false;
        self.audio.suspend();
        let result = if self.config.system_config.follow_system_output {
            AudioIO::with_system_output(
                &self.config.system_config.input_device.value,
                self.buffer_frames,
            )
        } else {
            AudioIO::with_buffer(
                &self.config.system_config.input_device.value,
                &self.config.system_config.output_device.value,
                self.buffer_frames,
            )
        };
        let audio = match result {
            Ok(value) => value,
            Err(error) => {
                self.audio.resume();
                self.status = format!("Reconnect failed; previous audio retained: {error}");
                return;
            }
        };
        self.audio = audio;
        self.audition_requests = Default::default();
        self.previewing = false;
        self.candidate_audition = false;
        self.note_audition = false;
        self.audition_target = None;
        self.send(Control::Spectrum(self.visualizer_enabled));
        if self.audio.online {
            self.config.system_config.output_device.value =
                self.audio.curr_output_name().to_owned();
        }
        self.loopback_connected = false;
        self.config.calibration = None;
        self.measurement = None;
        self.engine_transition = true;
        let sr = self.audio.config.sample_rate.0;
        let index = self.active_project_idx.unwrap_or(0);
        let data = project::data_from_config(&self.config);
        let (tx, rx) = mpsc::channel();
        self.job = Some(rx);
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<JobResult> {
                crate::session::resample(&mut snapshot, sr)?;
                let mut config = AppConfig::new(120, 85, 5);
                project::apply_data_to_config(&mut config, data.clone());
                let mut core = Box::new(RenderCore::new(sr));
                core.configure(&mut Parameters::from_config(&config, sr));
                core.restore(&mut snapshot);
                Ok(JobResult::Loaded { index, data, core })
            })();
            let _ = tx.send(
                result.unwrap_or_else(|e| JobResult::Error(format!("Audio restore failed: {e}"))),
            );
        });
    }
    pub fn open_project(&mut self, index: usize) {
        if self.busy() || index >= self.projects.len() {
            return;
        }
        self.engine_transition = true;
        self.sel_project_idx = index;
        let entry = self.projects[index].clone();
        let sr = self.audio.config.sample_rate.0;
        let (tx, rx) = mpsc::channel();
        self.job = Some(rx);
        self.status = "Opening project and verifying audio...".into();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<JobResult> {
                let data = project::load_project(&entry)?
                    .unwrap_or_else(|| project::data_from_config(&AppConfig::new(120, 85, 5)));
                let mut config = AppConfig::new(120, 85, 5);
                project::apply_data_to_config(&mut config, data.clone());
                let mut core = Box::new(RenderCore::new(sr));
                core.configure(&mut Parameters::from_config(&config, sr));
                if let Some(revision) = &data.snapshot {
                    let mut snapshot = crate::session::load_snapshot(&entry, revision)?;
                    crate::session::resample(&mut snapshot, sr)?;
                    core.restore(&mut snapshot);
                }
                Ok(JobResult::Loaded { index, data, core })
            })();
            let _ = tx.send(
                result.unwrap_or_else(|e| JobResult::Error(format!("Cannot open project: {e}"))),
            );
        });
    }
    pub fn save_now(&mut self) {
        if self.busy() || self.read_only {
            return;
        }
        let Some(entry) = self
            .active_project_idx
            .and_then(|i| self.projects.get(i))
            .cloned()
        else {
            return;
        };
        let mut data = project::data_from_config(&self.config);
        let (tx, rx) = mpsc::channel();
        self.job = Some(rx);
        self.saving = true;
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<()> {
                data.snapshot = project::saved_snapshot(&entry)?;
                project::save_project_data(&entry, &data)
            })();
            let _ = tx.send(match result {
                Ok(()) => JobResult::Saved,
                Err(e) if e.is::<project::storage::SaveCleanupWarning>() => {
                    JobResult::SavedCleanupWarning(e.to_string())
                }
                Err(e) => JobResult::Error(format!("Save failed: {e}")),
            });
        });
    }
    pub fn save_snapshot(&mut self) {
        if self.busy() || self.read_only {
            return;
        }
        let Some(entry) = self
            .active_project_idx
            .and_then(|i| self.projects.get(i))
            .cloned()
        else {
            return;
        };
        if !self.sync_config() {
            return;
        }
        self.saving = self.send(Control::Snapshot {
            snapshot: Box::new(AudioSnapshot::empty(self.audio.config.sample_rate.0)),
            entry,
            data: project::data_from_config(&self.config),
        });
        if self.saving {
            self.status = "Saving an immutable audio snapshot in the background...".into();
        }
    }
    pub fn start_take(&mut self) {
        if let Some(reason) = self.take_block_reason() {
            self.status = self.language.text(reason).into();
            return;
        }
        let Some(entry) = self
            .active_project_idx
            .and_then(|i| self.projects.get(i))
            .cloned()
        else {
            return;
        };
        let root = crate::replay::library::root();
        let sr = self.audio.config.sample_rate.0;
        let mut core = Box::new(RenderCore::new(sr));
        core.configure(&mut Parameters::from_config(&self.config, sr));
        let command = Control::BeginTake {
            retired_audition: None,
            core,
            snapshot: Box::new(AudioSnapshot::empty(sr)),
            root: root.join(format!("draft-{}", crate::session::id())),
            project_id: entry.file,
            data: project::data_from_config(&self.config),
        };
        self.take_pending = self.send(command);
        self.take_ending = false;
        self.status = "Replay armed: perform with track and FX controls.".into();
    }
    pub fn finish_take(&mut self) {
        if self.view.tracks.iter().any(|t| {
            matches!(
                t.mode,
                crate::engine::core::Mode::Recording | crate::engine::core::Mode::Overdub
            )
        }) {
            self.status =
                "Finish the active recordings / overdubs before ending the replay take.".into();
            return;
        }
        if self.taking() && !self.take_pending {
            self.take_pending = self.send(Control::EndTake);
            self.take_ending = self.take_pending;
            self.status = "Finalizing replay input and checksums...".into();
        }
    }
    pub fn save_take(&mut self) {
        let Some(path) = self.draft.as_ref() else {
            return;
        };
        let previous_path = path.clone();
        match crate::replay::save_as(path, &self.take_name) {
            Ok(path) => {
                self.draft = None;
                if let Some((root, result)) = &mut self.rendered {
                    if *root == previous_path {
                        *root = path.clone();
                        result.name = self.take_name.trim().into();
                    }
                }
                self.status = format!("Replay saved: {}", path.display());
                self.replay_list = crate::replay::library::list();
            }
            Err(e) => self.status = e.to_string(),
        }
    }
    pub fn keep_take(&mut self) {
        // Keep the valid draft in the on-disk replay browser for crash recovery;
        // Keeping a draft dismisses the prompt and retains its replay files.
        self.draft = None;
        self.status = "Draft kept in replay history. You can reopen it later.".into();
        self.replay_list = crate::replay::library::list();
    }
    pub fn render_replay(&mut self, root: PathBuf) {
        self.replay_autoplay = false;
        if self.busy() || self.taking() {
            return;
        }
        let export = crate::replay::library::root().join("exports");
        if let Err(e) = std::fs::create_dir_all(&export) {
            self.status = e.to_string();
            return;
        }
        let destination = export.join(format!("replay-{}.wav", crate::session::id()));
        let (tx, rx) = mpsc::channel();
        self.job = Some(rx);
        self.render_progress.store(0, Ordering::Relaxed);
        let progress = self.render_progress.clone();
        self.status = "Rendering replay through the audio engine...".into();
        std::thread::spawn(move || {
            let result = crate::replay::export(&root, &destination, &progress);
            let _ = tx.send(match result {
                Ok(result) => JobResult::Rendered { root, result },
                Err(e) => JobResult::Error(format!("Replay render failed: {e}")),
            });
        });
    }
}
