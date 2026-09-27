//! Shared application actions for mouse controls and keyboard shortcuts.
use super::*;

impl MyApp {
    pub fn trigger_track(&mut self, index: usize) {
        if self.audio_io.is_err() {
            self.status = "Audio unavailable; parameter editing remains available.".into();
            return;
        }
        if let Some(track) = self.tracks.get_mut(index) {
            track.trigger();
        }
    }

    pub fn pause_track(&mut self, index: usize) {
        if let Some(track) = self.tracks.get_mut(index) {
            track.stop();
        }
    }

    pub fn clear_track(&mut self, index: usize) {
        if let Some(track) = self.tracks.get_mut(index) {
            if !matches!(track.track_state, TrackState::Record | TrackState::Dub) {
                track.track_state = TrackState::Empty;
                track.stop_after_finish = false;
                if let Ok(audio) = &self.audio_io {
                    audio.clear_track_now(index);
                }
            }
        }
    }

    pub fn toggle_all(&mut self) {
        let playing = self.tracks.iter().any(|t| {
            matches!(
                t.track_state,
                TrackState::Record | TrackState::Play | TrackState::NxtPlay | TrackState::Dub
            )
        });
        self.previewing = false;
        for index in 0..self.tracks.len() {
            if playing {
                self.pause_track(index);
            } else if self.tracks[index].track_state == TrackState::Pause {
                self.tracks[index].track_state = TrackState::Play;
            }
        }
    }

    pub fn toggle_preview(&mut self) {
        self.previewing = !self.previewing;
        if self.previewing && self.metronome.start_time().is_none() {
            self.metronome.get_beat_time();
        }
    }

    pub fn project_name(&self) -> &str {
        self.active_project_idx
            .and_then(|i| self.projects.get(i))
            .map(|p| p.name.as_str())
            .unwrap_or("Untitled")
    }

    pub fn save_now(&mut self) {
        self.save_active_project();
    }
    pub fn back_to_projects(&mut self) {
        self.request_exit(PendingExit::ToInit);
    }
    pub fn open_project(&mut self, index: usize) {
        self.sel_project_idx = index;
        self.load_selected_project();
    }

    pub fn create_project(&mut self) {
        let name = self.project_name_input.trim().to_owned();
        if name.is_empty() {
            return;
        }
        let idx = self.projects.len();
        self.projects.push(ProjectEntry {
            file: project::make_project_file_name(&name, idx),
            name,
        });
        if let Err(error) = project::save_index(&self.projects) {
            self.projects.pop();
            self.status = format!("Cannot create project: {error}");
            return;
        }
        self.project_name_input.clear();
        self.project_name_mode = None;
        self.open_project(idx);
    }

    pub fn audio_status(&self) -> String {
        match &self.audio_io {
            Ok(audio) => format!(
                "{} Hz / {} ch",
                audio.config.sample_rate.0, audio.config.channels
            ),
            Err(error) => format!("Audio unavailable: {error}"),
        }
    }

    pub fn refresh_waveforms(&mut self) {
        if self.last_waveform_refresh.elapsed() < Duration::from_millis(100) {
            return;
        }
        self.last_waveform_refresh = Instant::now();
        if let Ok(audio) = &self.audio_io {
            if let Some(waves) = audio.waveform_overviews() {
                self.waveforms = waves;
            }
        }
    }
}
