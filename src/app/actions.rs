use super::*;
use crate::engine::core::Action;

impl MyApp {
    pub fn action(&mut self, action: Action) {
        if matches!(action, Action::Panic) {
            self.stop_audition();
        }
        if self.calibration_held() {
            self.status = self
                .language
                .text("Disconnect the loopback cable and restore monitoring first")
                .into();
            return;
        }
        if self.performance_locked() || self.player_open {
            return;
        }
        if !self.audio.online
            && matches!(
                action,
                Action::Trigger(_) | Action::Preview(true) | Action::Metronome(true) | Action::All
            )
        {
            self.status="Connect audio before performing. Offline preset and snapshot editing is available.".into();
            return;
        }
        if !self.sync_config() {
            return;
        }
        if self.send(Control::Action(action)) {
            if matches!(action, Action::Panic | Action::Preview(false)) {
                self.tempo_start_queued = None;
            } else if matches!(
                action,
                Action::All | Action::Trigger(_) | Action::Metronome(true) | Action::Preview(true)
            ) && self.stopped()
            {
                // The callback's view arrives later than this UI frame. Lock
                // tempo immediately so a mouse edit cannot follow a queued start.
                self.tempo_start_queued = Some(self.view.frame);
            }
        }
    }
    pub fn trigger_track(&mut self, index: usize) {
        self.action(Action::Trigger(index));
    }
    pub fn pause_track(&mut self, index: usize) {
        self.action(Action::Stop(index));
    }
    pub fn clear_track(&mut self, index: usize) {
        self.action(Action::Clear(index));
    }
    pub fn undo_track(&mut self, index: usize) {
        self.action(Action::UndoStep(index));
    }
    pub fn toggle_all(&mut self) {
        self.action(Action::All);
    }
    pub fn redo_track(&mut self, index: usize) {
        self.action(Action::RedoStep(index));
    }
    pub fn project_name(&self) -> &str {
        self.active_project_idx
            .and_then(|i| self.projects.get(i))
            .map(|p| p.name.as_str())
            .unwrap_or("Untitled")
    }
    pub fn back_to_projects(&mut self) {
        self.request_exit(PendingExit::ToInit);
    }
    pub fn audio_status(&self) -> String {
        if self.audio.curr_output_name().is_empty() {
            self.audio.status.clone()
        } else {
            format!("{} · {}", self.audio.curr_output_name(), self.audio.status)
        }
    }
    pub fn create_project(&mut self) {
        if self.read_only || self.busy() {
            return;
        }
        let name = self.project_name_input.trim().to_owned();
        if name.is_empty() {
            return;
        }
        if self.project_name_mode == Some(ProjectNameMode::Rename) {
            if let Some(entry) = self.projects.get_mut(self.sel_project_idx) {
                entry.name = name;
            }
            if let Err(e) = project::save_index(&self.projects) {
                self.status = e.to_string();
                return;
            }
            self.project_name_mode = None;
            self.project_name_input.clear();
            return;
        }
        let index = self.projects.len();
        let entry = ProjectEntry {
            file: project::make_project_file_name(&name, index),
            name,
        };
        if let Err(e) = project::save_project_data(
            &entry,
            &project::data_from_config(&AppConfig::new(120, 85, 5)),
        ) {
            self.status = e.to_string();
            return;
        }
        self.projects.push(entry);
        if let Err(e) = project::save_index(&self.projects) {
            self.projects.pop();
            self.status = e.to_string();
            return;
        }
        self.project_name_input.clear();
        self.project_name_mode = None;
        self.open_project(index);
    }
    pub fn trash_project(&mut self) {
        if self.read_only || self.busy() || self.sel_project_idx >= self.projects.len() {
            return;
        }
        let entry = self.projects[self.sel_project_idx].clone();
        if let Err(error) = crate::replay::library::migrate(&self.projects) {
            self.status = format!("Cannot detach replays before deleting project: {error}");
            return;
        }
        match project::trash_project(&entry) {
            Ok(()) => {
                self.projects.remove(self.sel_project_idx);
                self.sel_project_idx = self
                    .sel_project_idx
                    .min(self.projects.len().saturating_sub(1));
                let _ = project::save_index(&self.projects);
                self.status =
                    "Project moved to Trash; use Restore last deleted to recover it.".into();
            }
            Err(e) => self.status = e.to_string(),
        }
    }
    pub fn restore_project(&mut self) {
        if self.read_only || self.busy() {
            return;
        }
        match project::restore_last_deleted() {
            Ok(Some(entry)) => {
                self.projects.push(entry);
                let _ = project::save_index(&self.projects);
                self.status = "Deleted project restored.".into();
            }
            Ok(None) => self.status = "Trash is empty.".into(),
            Err(e) => self.status = e.to_string(),
        }
    }
    pub fn focus_panel(&mut self, ctx: &egui::Context, focus: Focus) {
        self.focus = focus;
        self.focus_request = focus != Focus::Performance;
        if let Some(id) = ctx.memory(|m| m.focused()) {
            ctx.memory_mut(|m| m.surrender_focus(id));
        }
        // Moving between live panels must not restart an already held fader.
        // Text/modal/window-focus gates reset continuous controls in keyboard.rs.
        self.clear_gesture.cancel();
    }
    pub fn open_editor(&mut self, ctx: &egui::Context) {
        self.editor.expanded = true;
        self.focus_panel(ctx, Focus::Editor);
    }
    pub fn close_editor(&mut self, ctx: &egui::Context) {
        self.editor.expanded = false;
        self.focus_panel(ctx, Focus::Performance);
    }
    pub fn apply_measurement(&mut self) {
        if let Some(value) = self.measurement {
            if value.output_generation
                != self
                    .audio
                    .diagnostics
                    .output_generation
                    .load(Ordering::Relaxed)
            {
                self.measurement = None;
                return;
            }
            self.config.beat_config.input_latency.value =
                (value.frames as f64 * 1000.0 / value.sample_rate as f64).round() as usize;
            self.config.calibration = Some(crate::config::track_options::LatencyCalibration {
                frames: value.frames,
                sample_rate: value.sample_rate,
                input: self.audio.curr_input_name().to_owned(),
                output: self.audio.curr_output_name().to_owned(),
                buffer_frames: self.buffer_frames,
                displayed_ms: self.config.beat_config.input_latency.value,
            });
            self.status = format!(
                "Compensation set to {} frames ({:.3} ms).",
                value.frames,
                value.frames as f64 * 1000.0 / value.sample_rate as f64
            );
        }
    }
    pub fn calibrate(&mut self) {
        if !self.calibration_held() || !self.loopback_connected {
            self.status = self
                .language
                .text("Prepare and confirm the loopback cable first")
                .into();
            return;
        }
        if !self.stopped() || self.taking() || !self.audio.online {
            return;
        }
        self.measurement = None;
        let mut probe = crate::engine::latency::Calibration::new(self.audio.config.sample_rate.0);
        probe.output_generation = self
            .audio
            .diagnostics
            .output_generation
            .load(Ordering::Relaxed);
        self.send(Control::Calibrate(Some(Box::new(probe))));
        self.status =
            "Measuring three loopback probes; monitoring is muted for three seconds.".into();
    }
}
