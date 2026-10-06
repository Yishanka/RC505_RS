use super::*;
use crate::{
    engine::audition::{Audition, AuditionParameters},
    presets::FxTarget,
};
impl MyApp {
    fn enqueue_audition(&mut self, voice: Option<Box<Audition>>) -> bool {
        let enabled = voice.is_some();
        let serial = self.audition_requests.next(
            self.audio
                .diagnostics
                .audition_commands
                .load(Ordering::Acquire),
        );
        if !self.send(Control::Audition(voice)) {
            return false;
        }
        self.audition_requests.sent(serial, enabled);
        true
    }
    pub fn choose_preset_candidate(&mut self, target: FxTarget, name: &str) {
        match crate::presets::SoundCandidate::load(&self.config, target, name) {
            Ok(candidate) => {
                if self.previewing && !self.stop_audition() {
                    return;
                }
                self.editor.candidate = Some(candidate);
                self.editor.message.clear();
            }
            Err(error) => self.editor.message = error.to_string(),
        }
    }
    pub fn candidate_audition_reason(&self) -> Option<&'static str> {
        if !self.audio.online {
            return Some("Connect audio before auditioning");
        }
        if self.busy() {
            return Some("Wait for the current operation");
        }
        if self.calibration_held() {
            return Some("Disconnect the loopback cable and restore monitoring first");
        }
        if self.taking() || self.take_pending {
            return Some("Finish replay recording first");
        }
        if self.player_open {
            return Some("Close the replay player first");
        }
        let candidate = self.editor.candidate.as_ref()?;
        match candidate.source {
            crate::presets::CandidateSource::Empty => Some("Choose an effect."),
            crate::presets::CandidateSource::TrackLoop
                if self.view.tracks[self.track_sel.unwrap_or(0)].frames == 0 =>
            {
                Some("Record track audio before previewing this sound")
            }
            _ => None,
        }
    }
    pub fn toggle_candidate_audition(&mut self) {
        if self.candidate_audition && self.previewing {
            self.stop_audition();
            return;
        }
        if let Some(reason) = self.candidate_audition_reason() {
            self.editor.message = self.language.text(reason).into();
            return;
        }
        let Some(candidate) = self.editor.candidate.as_ref() else {
            return;
        };
        let target = candidate.target;
        let staging = match candidate.staging(&self.config) {
            Ok(config) => config,
            Err(e) => {
                self.editor.message = e.to_string();
                return;
            }
        };
        if let FxTarget::Input { bank, slot } = target {
            if let Some(crate::config::InputFx::Oscillator(osc)) =
                &staging.input_fx.banks[bank].slots[slot].fx
            {
                if osc.waveform.value == crate::config::osc_configs::Waveform::Sample
                    && osc.sample.is_none()
                {
                    self.editor.message = self
                        .language
                        .text("Capture or import a sample first")
                        .into();
                    return;
                }
            }
        }
        let track = if matches!(target, FxTarget::Track { .. }) {
            self.track_sel.unwrap_or(0)
        } else {
            0
        };
        if let Some(params) = AuditionParameters::candidate(&staging, target, track) {
            let note = matches!(params, AuditionParameters::Note { .. });
            let voice =
                Box::new(Audition::new(params, self.audio.config.sample_rate.0).exclusive());
            if self.enqueue_audition(Some(voice)) {
                self.previewing = true;
                self.candidate_audition = true;
                self.note_audition = note;
                self.audition_target = Some((target, track));
            }
        }
    }
    pub fn apply_preset_candidate(&mut self) {
        if self.candidate_audition && !self.stop_audition() {
            return;
        }
        let Some(candidate) = self.editor.candidate.take() else {
            return;
        };
        match candidate.apply(&mut self.config) {
            Ok(()) => {
                self.editor.message = format!(
                    "{} {}",
                    self.language.choose("Applied", "已应用"),
                    candidate.name
                );
                self.editor.page = crate::ui::editor::EditorPage::Sound;
            }
            Err(e) => {
                self.editor.message = e.to_string();
                self.editor.candidate = Some(candidate);
            }
        }
    }
    pub fn cancel_preset_candidate(&mut self) {
        if self.candidate_audition && !self.stop_audition() {
            return;
        }
        self.editor.candidate = None;
        self.editor.message.clear();
    }
    pub fn audition_note(
        &mut self,
        target: FxTarget,
        note: crate::config::sequence_edit::NoteEvent,
    ) {
        if !self.audio.online
            || self.busy()
            || self.taking()
            || self.take_pending
            || self.calibration_held()
            || self.player_open
        {
            return;
        }
        if let Some(params) =
            AuditionParameters::single_note(&self.config, target, note.pitch, note.velocity)
        {
            let voice = Box::new(Audition::new(params, self.audio.config.sample_rate.0));
            if self.enqueue_audition(Some(voice)) {
                self.candidate_audition = false;
                self.audition_target = Some((target, 0));
                self.previewing = true;
                self.note_audition = true;
            }
        }
    }
    pub fn tracks_stopped(&self) -> bool {
        self.view.tracks.iter().all(|t| {
            matches!(
                t.mode,
                crate::engine::core::Mode::Empty | crate::engine::core::Mode::Stopped
            ) && !t.pending
        })
    }
    pub fn calibration_held(&self) -> bool {
        self.audio
            .diagnostics
            .calibration_hold
            .load(Ordering::Relaxed)
    }
    pub fn calibration_reason(&self) -> Option<&'static str> {
        if !self.audio.online {
            Some("Connect audio before testing")
        } else if self.read_only {
            Some("Read-only editor cannot persist the monitoring safety guard")
        } else if self.taking() || self.take_pending {
            Some("Finish replay recording first")
        } else if self.player_open || self.previewing {
            Some("Stop the player and audition first")
        } else if !self.stopped() {
            Some("Stop the performance and all five tracks first")
        } else if self.busy() {
            Some("Wait for the current operation")
        } else {
            None
        }
    }
    pub fn prepare_calibration(&mut self) {
        if let Some(reason) = self.calibration_reason() {
            self.status = self.language.text(reason).into();
            return;
        }
        if let Err(error) = self.save_monitor_guard(true) {
            self.status = error.to_string();
            return;
        }
        self.loopback_connected = false;
        self.measurement = None;
        self.send(Control::CalibrationHold(true));
        self.status = self
            .language
            .choose(
                "Wait for MUTED before connecting the loopback cable.",
                "请等待显示“已静音”后再接回环线。",
            )
            .into();
    }
    pub fn restore_monitoring(&mut self) {
        if self.audio.diagnostics.calibrating.load(Ordering::Relaxed) {
            return;
        }
        if let Err(error) = self.save_monitor_guard(false) {
            self.status = error.to_string();
            return;
        }
        self.loopback_connected = false;
        self.send(Control::CalibrationHold(false));
        self.status = self
            .language
            .choose(
                "Monitoring restored after cable-disconnected confirmation.",
                "已按拔线确认恢复监听。",
            )
            .into();
    }
    fn save_monitor_guard(&self, guard: bool) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.read_only,
            "Read-only editor cannot change the monitoring guard"
        );
        let mut settings = crate::app_support::launcher_config::load().unwrap_or_default();
        settings.calibration_guard = guard;
        crate::app_support::launcher_config::save(&settings)
    }
    pub fn audition_selection(&self) -> Option<(FxTarget, usize)> {
        self.editor.target.map(|target| {
            (
                target,
                if matches!(target, FxTarget::Track { .. }) {
                    self.track_sel.unwrap_or(0)
                } else {
                    0
                },
            )
        })
    }
    pub fn audition_reason(&self) -> Option<&'static str> {
        if self.busy() {
            return Some("Wait for the current operation");
        }
        if !self.audio.online {
            return Some("Connect audio before auditioning");
        }
        if self.calibration_held() {
            return Some("Disconnect the loopback cable and restore monitoring first");
        }
        if self.taking() || self.take_pending {
            return Some("Finish replay recording first");
        }
        if self.player_open {
            return Some("Close the replay player first");
        }
        match self.editor.target {
            Some(FxTarget::Input { bank, slot }) => {
                match self.config.input_fx.banks[bank].slots[slot].fx.as_ref() {
                    Some(crate::config::InputFx::Oscillator(v))
                        if v.waveform.value == crate::config::osc_configs::Waveform::Sample
                            && v.sample.is_none() =>
                    {
                        Some("Capture or import a sample first")
                    }
                    Some(crate::config::InputFx::Oscillator(v)) if !v.note.events().is_empty() => {
                        None
                    }
                    Some(crate::config::InputFx::MyDelay(v)) if !v.note.events().is_empty() => None,
                    _ => Some("Add notes to the piano roll first"),
                }
            }
            _ => Some("Select an oscillator to audition notes"),
        }
    }
    pub fn toggle_audition(&mut self) {
        if self.previewing {
            self.stop_audition();
            return;
        }
        if let Some(reason) = self.audition_reason() {
            self.status = self.language.text(reason).into();
            return;
        }
        if let Some((target, track)) = self.audition_selection() {
            if let Some(params) = AuditionParameters::new(&self.config, target, track) {
                let voice = Box::new(Audition::new(params, self.audio.config.sample_rate.0));
                if self.enqueue_audition(Some(voice)) {
                    self.candidate_audition = false;
                    self.note_audition = false;
                    self.audition_target = Some((target, track));
                    self.previewing = true;
                }
            }
        }
    }
    pub fn stop_audition(&mut self) -> bool {
        if self.audition_requests.stopping() {
            return true;
        }
        if !self.previewing
            && !self.audition_requests.pending()
            && !self.audio.diagnostics.auditioning.load(Ordering::Relaxed)
        {
            return true;
        }
        self.enqueue_audition(None)
    }
    pub fn editor_beats(&self) -> Option<f64> {
        if self.previewing && !self.note_audition {
            Some(
                self.audio
                    .diagnostics
                    .audition_frame
                    .load(Ordering::Relaxed) as f64
                    / self.audio.config.sample_rate.0 as f64
                    * self.config.beat_config.current_bpm() as f64
                    / 60.0,
            )
        } else {
            self.beats().map(|beats| {
                let origin = if let Some(FxTarget::Input { bank, slot }) = self.editor.target {
                    self.view.phrases[bank][slot].origin_tick as f64
                        / crate::config::sequence_edit::PPQ as f64
                } else {
                    0.0
                };
                (beats - origin).max(0.0)
            })
        }
    }
    pub fn take_block_reason(&self) -> Option<&'static str> {
        if self.calibration_held() {
            Some("Disconnect the loopback cable and restore monitoring first")
        } else if self.draft.is_some() {
            Some("Save the replay or keep it as a draft first")
        } else if self.take_pending {
            Some("Replay recording is starting or finishing")
        } else if self.read_only {
            Some("This editor is read-only")
        } else if !self.audio.online {
            Some("Connect audio before recording a replay")
        } else if self.player_open {
            Some("Close the replay player first")
        } else if self.busy() {
            Some("Wait for the current operation")
        } else if !self.taking() && !self.tracks_stopped() {
            Some("Stop all five tracks before recording a replay")
        } else if self.taking()
            && self.view.tracks.iter().any(|t| {
                matches!(
                    t.mode,
                    crate::engine::core::Mode::Recording | crate::engine::core::Mode::Overdub
                )
            })
        {
            Some("Finish track recording or overdub before ending the replay")
        } else {
            None
        }
    }
    pub fn toggle_take(&mut self) {
        if let Some(reason) = self.take_block_reason() {
            self.status = self.language.text(reason).into();
            return;
        }
        if self.taking() {
            self.finish_take();
        } else {
            self.start_take();
        }
    }
    pub fn open_replays(&mut self) {
        if !self.read_only {
            if let Err(error) = crate::replay::library::migrate(&self.projects) {
                self.status = format!("Replay library migration failed: {error}");
            }
        }
        self.replay_list = crate::replay::library::list();
        self.replay_exports = crate::replay::library::exports();
        self.replay_browser = true;
    }
}
