use super::*;

impl MyApp {
    pub fn play_replay(&mut self, root: PathBuf) {
        if self.busy() || self.taking() {
            return;
        }
        if !self.tracks_stopped() || !self.audio.online || self.calibration_held() {
            self.status = self
                .language
                .choose(
                    "Stop tracks and restore monitoring before replay playback.",
                    "请先停止五轨、连接音频设备并恢复监听，再播放回放。",
                )
                .into();
            return;
        }
        self.stop_audition();
        self.replay_autoplay = true;
        let rate = self.audio.config.sample_rate.0;
        let (tx, rx) = mpsc::channel();
        self.job = Some(rx);
        self.engine_transition = true;
        self.status = self
            .language
            .choose(
                "Verifying replay inputs; playback simulates audio in real time.",
                "正在校验回放输入；播放将实时演算音频。",
            )
            .into();
        std::thread::spawn(move || {
            let result = crate::replay::streaming::start(&root, rate);
            let _ = tx.send(match result {
                Ok((stream, session)) => JobResult::PlayerReady(stream, session),
                Err(error) => JobResult::Error(format!("Cannot play replay: {error}")),
            });
        });
    }
    pub fn close_player(&mut self) {
        self.replay_autoplay = false;
        self.send(Control::Player(None));
        self.player_open = false;
        self.replay_panel = None;
    }
    pub fn import_replay_position(
        &mut self,
        source: Arc<crate::replay::streaming::Source>,
        frame: u64,
        target: Option<usize>,
        name: String,
    ) {
        if self.busy() || self.taking() || self.read_only || self.calibration_held() {
            return;
        }
        let index = if let Some(index) = target {
            if index >= self.projects.len() {
                return;
            }
            index
        } else {
            let index = self.projects.len();
            let name = if name.trim().is_empty() {
                format!("{} - replay", source.metadata.name)
            } else {
                name.trim().to_owned()
            };
            let entry = ProjectEntry {
                file: project::make_project_file_name(&name, index),
                name,
            };
            // A new destination also gets an empty saved baseline so Discard
            // has the same meaning for new and existing projects.
            if let Err(error) = project::save_project_data(
                &entry,
                &project::data_from_config(&AppConfig::new(120, 0, 5)),
            ) {
                self.status = error.to_string();
                return;
            }
            self.projects.push(entry);
            if let Err(error) = project::save_index(&self.projects) {
                self.projects.pop();
                self.status = error.to_string();
                return;
            }
            index
        };
        let sr = self.audio.config.sample_rate.0;
        let (tx, rx) = mpsc::channel();
        self.job = Some(rx);
        self.engine_transition = true;
        self.status = self
            .language
            .choose(
                "Reconstructing the paused replay position for import…",
                "正在重建暂停位置的完整轨道状态……",
            )
            .into();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<JobResult> {
                let (data, core) = crate::replay::streaming::prepare_import(source, frame, sr)?;
                // Loop audio, partial recordings and history are preserved;
                // import deliberately opens stopped to avoid unexpected sound.
                Ok(JobResult::ReplayImported { index, data, core })
            })();
            let _ = tx.send(result.unwrap_or_else(|error| {
                JobResult::Error(format!("Replay import failed: {error}"))
            }));
        });
    }
}
