mod actions;
pub mod faders;
mod keyboard;
use eframe::egui;
use std::time::{Duration, Instant};

use crate::app_support::launcher_config::{self, LauncherConfig};
use crate::config::delay_configs::{
    TRACK_DELAY_DAMP_MAX_HZ, TRACK_DELAY_DAMP_MIN_HZ, TRACK_DELAY_FEEDBACK_MAX_PCT,
    TRACK_DELAY_MIX_MAX_PCT, TRACK_DELAY_TIME_MAX_MS, TRACK_DELAY_TIME_MIN_MS,
};
use crate::config::envelope_configs::{
    ENVELOPE_ATTACK_MAX_MS, ENVELOPE_DECAY_MAX_MS, ENVELOPE_HOLD_MAX_MS, ENVELOPE_RELEASE_MAX_MS,
    ENVELOPE_START_MAX_PCT, ENVELOPE_SUSTAIN_MAX_PCT, ENVELOPE_TENSION_MAX,
};
use crate::config::filter_configs::{
    FILTER_CUTOFF_MAX_HZ, FILTER_CUTOFF_MIN_HZ, FILTER_DRIVE_MAX, FILTER_MIX_MAX, FILTER_Q_MAX_X10,
    FILTER_Q_MIN_X10,
};
use crate::config::mydelay_configs::{MYDELAY_LEVEL_MAX, MYDELAY_THRESHOLD_MAX};
use crate::config::reverb_configs::{
    REVERB_HIGHCUT_MAX, REVERB_LOWCUT_MAX_HZ, REVERB_LOWCUT_MIN_HZ, REVERB_PREDELAY_MAX_MS,
    REVERB_RT60_MAX_MS, REVERB_RT60_MIN_MS, REVERB_SIZE_MAX, REVERB_WIDTH_MAX,
};
use crate::config::vocoder_configs::{
    VOCODER_ATTACK_MAX_MS, VOCODER_BANDS_MAX, VOCODER_BANDS_MIN, VOCODER_LEVEL_MAX,
    VOCODER_MIX_MAX, VOCODER_RELEASE_MAX_MS,
};
use crate::config::{AppConfig, ConfigSet, FxKind};
use crate::config::{TrackFx, TrackFxKind};
use crate::engine::audio_io::AudioIO;
use crate::engine::metronome::Metronome;
use crate::project::{self, ProjectEntry};
use crate::state::{AppState, FxState, PendingExit, ProjectNameMode, ScreenState, TrackState};
use crate::track::Track;
use crate::ui;

const DEFAULT_BPM: usize = 120;
const DEFAULT_LATENCY_COMP: usize = 85;
const MAX_BPM: usize = 300;
const MAX_LATENCY_COMP: usize = 500;
const MAX_FX_LEVEL: usize = 100;
const MAX_FX_THRESHOLD: usize = 100;
const TRACK_COUNT: usize = 5;

pub struct MyApp {
    fader_keys: [faders::KeyFader; TRACK_COUNT],
    pub editor: ui::editor::EditorState,
    pub previewing: bool,
    pub status: String,
    pub waveforms: Vec<Vec<f32>>,
    last_waveform_refresh: Instant,
    next_audio_retry: Instant,
    audio_io: Result<AudioIO, anyhow::Error>,
    pub metronome: Metronome,

    pub config: AppConfig,

    pub app_state: AppState,

    pub track_sel: Option<usize>,
    pub tracks: Vec<Track>,

    pub screen_state: ScreenState,

    pub fx_state: FxState,
    pub fx_screen_slot_idx: usize,
    pub fx_edit_row_idx: usize,
    pub track_fx_screen_slot_idx: usize,
    pub track_fx_edit_row_idx: usize,

    pub projects: Vec<ProjectEntry>,
    pub sel_project_idx: usize,
    pub project_name_input: String,
    pub project_name_mode: Option<ProjectNameMode>,
    active_project_idx: Option<usize>,
    pending_exit: Option<PendingExit>,
    show_save_prompt: bool,
    allow_window_close: bool,
    close_window_queued: bool,

    fonts_initialized: bool,
    #[cfg(debug_assertions)]
    preview_frame: usize,
}

impl MyApp {
    pub fn new() -> Self {
        let launch_config = launcher_config::load();
        let mut config = AppConfig::new(
            DEFAULT_BPM,
            launch_config
                .as_ref()
                .map(LauncherConfig::latency_comp_ms)
                .unwrap_or(DEFAULT_LATENCY_COMP),
            TRACK_COUNT,
        );
        if let Some(launch_config) = launch_config.as_ref() {
            Self::apply_launcher_hardware_config(&mut config, launch_config);
        }
        let audio_io: Result<AudioIO, anyhow::Error> =
            if std::env::args().any(|arg| arg == "--offline") {
                Err(anyhow::anyhow!("Offline parameter editing (--offline)"))
            } else {
                AudioIO::new(
                    &config.system_config.input_device.value,
                    &config.system_config.output_device.value,
                    TRACK_COUNT,
                    config.beat_config.current_latency(),
                )
            };
        let mut projects = project::load_index();
        Self::ensure_default_project(&mut projects);
        let launch_project_idx = launch_config
            .as_ref()
            .filter(|config| !config.last_project.is_empty())
            .and_then(|config| projects.iter().position(|p| p.name == config.last_project));

        let mut app = Self {
            fader_keys: [faders::KeyFader::default(); TRACK_COUNT],
            editor: ui::editor::EditorState::default(),
            previewing: false,
            status: String::new(),
            waveforms: vec![vec![0.0; 96]; TRACK_COUNT],
            last_waveform_refresh: Instant::now(),
            next_audio_retry: Instant::now(),
            metronome: Metronome::new(config.beat_config.current_bpm()),
            audio_io,
            app_state: AppState::Init,
            tracks: vec![Track::new(); TRACK_COUNT],
            track_sel: Some(0),
            screen_state: ScreenState::Empty,
            fx_state: FxState::Single,
            fx_screen_slot_idx: 0,
            fx_edit_row_idx: 0,
            track_fx_screen_slot_idx: 0,
            track_fx_edit_row_idx: 0,
            // tracks: (0..TRACK_COUNT).map(Track::new).collect(),
            config,
            projects,
            sel_project_idx: launch_project_idx.unwrap_or(0),
            project_name_input: String::new(),
            project_name_mode: None,
            active_project_idx: None,
            pending_exit: None,
            show_save_prompt: false,
            allow_window_close: false,
            close_window_queued: false,
            fonts_initialized: false,
            #[cfg(debug_assertions)]
            preview_frame: 0,
        };

        if launch_project_idx.is_some() {
            app.load_selected_project_with_launcher(launch_config.as_ref());
        }

        app
    }

    fn ensure_default_project(projects: &mut Vec<ProjectEntry>) {
        if projects.is_empty() {
            projects.push(ProjectEntry {
                name: "DEFAULT".to_string(),
                file: project::make_project_file_name("DEFAULT", 0),
            });
            let _ = project::save_index(projects);
        }
    }

    fn apply_launcher_hardware_config(config: &mut AppConfig, launch_config: &LauncherConfig) {
        config
            .beat_config
            .set_latency(launch_config.latency_comp_ms());
        if !launch_config.input_device.is_empty() {
            config.system_config.input_device.value = launch_config.input_device.clone();
        }
        if !launch_config.output_device.is_empty() {
            config.system_config.output_device.value = launch_config.output_device.clone();
        }
    }

    fn normalize_project_selection(&mut self) {
        let max_idx = self.projects.len();
        if self.sel_project_idx > max_idx {
            self.sel_project_idx = max_idx;
        }
    }

    fn load_selected_project(&mut self) {
        self.load_selected_project_with_launcher(None);
    }

    fn load_selected_project_with_launcher(&mut self, launch_config: Option<&LauncherConfig>) {
        if self.sel_project_idx >= self.projects.len() {
            return;
        }
        let entry = self.projects[self.sel_project_idx].clone();
        let loaded = match project::load_project(&entry) {
            Ok(data) => data,
            Err(error) => {
                self.status = format!("Cannot open project: {error}");
                return;
            }
        };
        if let Ok(audio) = self.audio_io.as_ref() {
            audio.clear_all_tracks_now();
        }
        for track in &mut self.tracks {
            track.track_state = TrackState::Empty;
            track.prev_track_state = TrackState::Empty;
            track.track_record_start_at = None;
            track.track_loop_duration = None;
            track.stop_after_finish = false;
            track.track_play_anchor_at = None;
        }
        self.config = AppConfig::new(DEFAULT_BPM, DEFAULT_LATENCY_COMP, TRACK_COUNT);
        if let Some(data) = loaded {
            project::apply_data_to_config(&mut self.config, data);
        }
        if let Some(launch_config) = launch_config {
            Self::apply_launcher_hardware_config(&mut self.config, launch_config);
        }
        self.editor = ui::editor::EditorState::default();
        self.previewing = false;
        self.status.clear();
        self.active_project_idx = Some(self.sel_project_idx);
        self.app_state = AppState::MainLoop;
    }

    fn save_active_project(&mut self) -> bool {
        if let Some(idx) = self.active_project_idx {
            if let Some(entry) = self.projects.get(idx) {
                match project::save_project(entry, &self.config) {
                    Ok(()) => {
                        self.status =
                            "Project parameters saved (loop audio is session-only)".into();
                        return true;
                    }
                    Err(error) => {
                        self.status = format!("Save failed: {error}");
                        return false;
                    }
                }
            }
        }
        false
    }

    fn request_exit(&mut self, target: PendingExit) {
        self.pending_exit = Some(target);
        self.show_save_prompt = true;
    }

    fn finish_exit(&mut self, save: bool) {
        if save && !self.save_active_project() {
            return;
        }
        self.previewing = false;
        self.editor.expanded = false;
        let target = self.pending_exit.take();
        self.show_save_prompt = false;
        match target {
            Some(PendingExit::ToInit) => {
                if let Ok(audio) = self.audio_io.as_ref() {
                    audio.clear_all_tracks_now();
                }
                for track in &mut self.tracks {
                    track.track_state = TrackState::Empty;
                    track.prev_track_state = TrackState::Empty;
                    track.track_record_start_at = None;
                    track.track_loop_duration = None;
                    track.stop_after_finish = false;
                    track.track_play_anchor_at = None;
                }
                self.app_state = AppState::Init;
                self.screen_state = ScreenState::Empty;
            }
            Some(PendingExit::CloseWindow) => {
                self.allow_window_close = true;
                self.close_window_queued = true;
            }
            None => {}
        }
    }

    fn next_loop_boundary(&self, track_id: usize, now: Instant) -> Option<Instant> {
        let anchor = self.tracks[track_id].track_play_anchor_at?;
        let loop_len = self.tracks[track_id].track_loop_duration?;
        if loop_len.is_zero() {
            return None;
        }
        let elapsed = now.saturating_duration_since(anchor);
        let loops = (elapsed.as_secs_f64() / loop_len.as_secs_f64()).floor() as u64;
        Some(anchor + loop_len * (loops as u32 + 1))
    }

    fn restart_anchor_with_latency(
        beat_time: Instant,
        loop_len: Duration,
        latency_ms: usize,
    ) -> Instant {
        if loop_len.is_zero() || latency_ms == 0 {
            return beat_time;
        }

        let loop_secs = loop_len.as_secs_f64();
        if loop_secs <= 0.0 {
            return beat_time;
        }

        let comp_secs = latency_ms as f64 / 1000.0;
        let comp_mod_secs = comp_secs % loop_secs;
        if comp_mod_secs <= 1e-9 {
            return beat_time;
        }

        // Restart phase from the tail segment that corresponds to latency compensation.
        // Equivalent phase at beat_time is: 1 - (comp_mod / loop_len).
        let shift_back_secs = (loop_secs - comp_mod_secs).max(0.0);
        beat_time
            .checked_sub(Duration::from_secs_f64(shift_back_secs))
            .unwrap_or(beat_time)
    }

    fn setup_font_fallback(&mut self, ctx: &egui::Context) {
        if self.fonts_initialized {
            return;
        }

        let candidates = [
            r"C:\Windows\Fonts\msyh.ttc",
            r"C:\Windows\Fonts\msyh.ttf",
            r"C:\Windows\Fonts\simhei.ttf",
            r"C:\Windows\Fonts\simsun.ttc",
        ];

        for path in candidates {
            if let Ok(bytes) = std::fs::read(path) {
                let mut fonts = egui::FontDefinitions::default();
                fonts.font_data.insert(
                    "cjk_fallback".to_owned(),
                    egui::FontData::from_owned(bytes).into(),
                );
                fonts
                    .families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .push("cjk_fallback".to_owned());
                fonts
                    .families
                    .entry(egui::FontFamily::Monospace)
                    .or_default()
                    .push("cjk_fallback".to_owned());
                ctx.set_fonts(fonts);
                break;
            }
        }

        self.fonts_initialized = true;
    }

    fn is_input_fx_screen(screen_state: ScreenState) -> bool {
        matches!(
            screen_state,
            ScreenState::FxSelect
                | ScreenState::InFxOsc
                | ScreenState::InFxOscAudio
                | ScreenState::InFxNote
                | ScreenState::InFxOscAudioEnv
                | ScreenState::InFxOscFilter
                | ScreenState::InFxOscFilterEnv
                | ScreenState::InFxFilter
                | ScreenState::InFxReverb
                | ScreenState::InFxMyDelay
                | ScreenState::InFxMyDelayAudio
                | ScreenState::InFxMyDelayAudioEnv
                | ScreenState::InFxMyDelayNote
                | ScreenState::InFxMyDelayFilter
                | ScreenState::InFxMyDelayFilterEnv
                | ScreenState::InFxVocoder
        )
    }

    fn normalize_input_fx_screen_for_current_slot(&mut self) {
        if !Self::is_input_fx_screen(self.screen_state)
            || self.screen_state == ScreenState::FxSelect
        {
            return;
        }

        let bank_idx = self.config.input_fx.sel_bank_idx;
        let slot_idx = self.fx_screen_slot_idx;
        let kind = self.config.input_fx.slot_kind(bank_idx, slot_idx);

        match kind {
            FxKind::None => {
                self.screen_state = ScreenState::FxSelect;
            }
            FxKind::Oscillator => {
                if !matches!(
                    self.screen_state,
                    ScreenState::InFxOsc
                        | ScreenState::InFxOscAudio
                        | ScreenState::InFxNote
                        | ScreenState::InFxOscAudioEnv
                        | ScreenState::InFxOscFilter
                        | ScreenState::InFxOscFilterEnv
                ) {
                    self.screen_state = ScreenState::InFxOsc;
                }
            }
            FxKind::Filter => {
                if self.screen_state != ScreenState::InFxFilter {
                    self.screen_state = ScreenState::InFxFilter;
                }
            }
            FxKind::Reverb => {
                if self.screen_state != ScreenState::InFxReverb {
                    self.screen_state = ScreenState::InFxReverb;
                }
            }
            FxKind::MyDelay => {
                if !matches!(
                    self.screen_state,
                    ScreenState::InFxMyDelay
                        | ScreenState::InFxMyDelayAudio
                        | ScreenState::InFxMyDelayAudioEnv
                        | ScreenState::InFxMyDelayNote
                        | ScreenState::InFxMyDelayFilter
                        | ScreenState::InFxMyDelayFilterEnv
                ) {
                    self.screen_state = ScreenState::InFxMyDelay;
                }
            }
            FxKind::Vocoder => {
                if self.screen_state != ScreenState::InFxVocoder {
                    self.screen_state = ScreenState::InFxVocoder;
                }
            }
        }
    }

    fn handle_config(&mut self) {
        if self.metronome.start_time().is_some() {
            self.config.beat_config.input_bpm.value = self.metronome.current_bpm();
        }
        // beat config -> metronome
        self.metronome
            .adjust_bpm(self.config.beat_config.current_bpm());
        let desired_latency = self.config.beat_config.current_latency();

        // sys config -> io device
        let desired_input = self.config.system_config.input_device.value.clone();
        let desired_output = self.config.system_config.output_device.value.clone();
        match self.audio_io.as_mut() {
            Ok(audio) => {
                audio.set_realtime_enabled(self.app_state != AppState::Init);
                if let Err(err) = audio.adjust_latency_comp(desired_latency) {
                    eprintln!("Failed to adjust latency compensation: {err}");
                }
                audio.set_track_levels(&self.config.track_levels);
                audio.update_input_fx(&self.config.input_fx);
                audio.update_track_fx(&self.config.track_fx);
                audio.update_metronome(self.metronome.start_time(), self.metronome.current_bpm());
                if let Err(err) = audio.switch_devices(&desired_input, &desired_output) {
                    eprintln!("Failed to switch audio devices: {err}");
                    self.config.system_config.input_device.value =
                        audio.curr_input_name().to_string();
                    self.config.system_config.output_device.value =
                        audio.curr_output_name().to_string();
                }
            }
            Err(_) => {
                if std::env::args().any(|arg| arg == "--offline") {
                    return;
                }
                if Instant::now() < self.next_audio_retry {
                    return;
                }
                self.next_audio_retry = Instant::now() + Duration::from_secs(3);
                self.audio_io = AudioIO::new(
                    &desired_input,
                    &desired_output,
                    TRACK_COUNT,
                    desired_latency,
                );
                if let Ok(audio) = self.audio_io.as_mut() {
                    audio.set_realtime_enabled(self.app_state != AppState::Init);
                    audio.set_track_levels(&self.config.track_levels);
                    audio.update_input_fx(&self.config.input_fx);
                    audio.update_track_fx(&self.config.track_fx);
                    audio.update_metronome(
                        self.metronome.start_time(),
                        self.metronome.current_bpm(),
                    );
                }
            }
        }
    }

    fn handle_track(&mut self) {
        if self.app_state == AppState::Init {
            self.metronome.reset();
            if let Some(audio) = self.audio_io.as_ref().ok() {
                for idx in 0..TRACK_COUNT {
                    audio.pause_now(idx);
                }
            }
            for track in &mut self.tracks {
                track.track_state = TrackState::Empty;
                track.prev_track_state = TrackState::Empty;
                track.track_record_start_at = None;
                track.track_loop_duration = None;
                track.stop_after_finish = false;
                track.track_play_anchor_at = None;
            }
            return;
        }

        let on_track = self
            .tracks
            .iter()
            .filter(|t| {
                t.track_state == TrackState::Record
                    || t.track_state == TrackState::Play
                    || t.track_state == TrackState::Dub
                    || t.track_state == TrackState::NxtPlay
            })
            .count();
        let metronome_was_running = self.metronome.start_time().is_some();
        if on_track == 0 && !self.previewing {
            self.metronome.reset();
        }

        let now = Instant::now();
        let audio = self.audio_io.as_ref().ok();
        if on_track == 0 && metronome_was_running {
            if let Some(engine) = audio {
                engine.update_metronome(self.metronome.start_time(), self.metronome.current_bpm());
            }
        }

        let timeline_start_at = if self.metronome.start_time().is_none() && on_track > 0 {
            let beat_time = self.metronome.get_beat_time();
            let latency_ms = self.config.beat_config.current_latency();
            // When timeline restarts from all-paused, keep latency-compensated phase.
            // This makes the first resumed track enter from the tail compensation segment,
            // and paused tracks will re-enter phase-aligned later.
            for track in &mut self.tracks {
                if let Some(loop_len) = track.track_loop_duration {
                    track.track_play_anchor_at = Some(Self::restart_anchor_with_latency(
                        beat_time, loop_len, latency_ms,
                    ));
                }
            }
            if let Some(engine) = audio {
                engine.update_metronome(self.metronome.start_time(), self.metronome.current_bpm());
            }
            Some(beat_time)
        } else {
            None
        };

        for idx in 0..TRACK_COUNT {
            let current = self.tracks[idx].track_state;
            let previous = self.tracks[idx].prev_track_state;

            if current != previous {
                match current {
                    TrackState::Record => {
                        let beat_time =
                            timeline_start_at.unwrap_or_else(|| self.metronome.get_beat_time());
                        self.tracks[idx].track_record_start_at = Some(beat_time);
                        self.tracks[idx].track_loop_duration = None;
                        self.tracks[idx].track_play_anchor_at = None;
                        // self.tracks[idx].record(beat_time);
                        if let Some(engine) = audio {
                            engine.record_at(idx, beat_time);
                        }
                    }
                    TrackState::NxtPlay => {
                        let now = Instant::now();
                        let beat_time = self.metronome.get_beat_time();
                        if previous == TrackState::Record {
                            self.tracks[idx].track_play_anchor_at = Some(beat_time);
                            self.tracks[idx].track_loop_duration =
                                match self.tracks[idx].track_record_start_at {
                                    Some(start) if beat_time > start => {
                                        Some(beat_time.duration_since(start))
                                    }
                                    _ => Some(self.metronome.beat_duration()),
                                };
                            self.tracks[idx].track_record_start_at = None;
                        }
                        // self.tracks[idx].nxt_play(beat_time);
                        if let Some(engine) = audio {
                            if previous == TrackState::Record {
                                engine.stop_record_play_at(idx, beat_time);
                            }
                            let stop_at = self.next_loop_boundary(idx, now).unwrap_or(beat_time);
                            engine.stop_overdub_at(idx, stop_at);
                        }
                    }
                    TrackState::Play => {
                        let progress = if self.tracks[idx].track_play_anchor_at.is_some()
                            && self.tracks[idx].track_loop_duration.is_some()
                        {
                            Some(self.tracks[idx].track_play_progress(now))
                        } else {
                            Some(0.0)
                        };

                        // self.tracks[idx].play();
                        if let Some(engine) = audio {
                            engine.play_at_progress_now(idx, progress);
                        }
                    }
                    TrackState::Pause | TrackState::Empty => {
                        if current == TrackState::Empty {
                            self.tracks[idx].track_record_start_at = None;
                            self.tracks[idx].track_play_anchor_at = None;
                            self.tracks[idx].track_loop_duration = None;
                        }
                        // self.tracks[idx].pause();
                        if let Some(engine) = audio {
                            if current == TrackState::Pause {
                                let progress = if self.tracks[idx].track_play_anchor_at.is_some()
                                    && self.tracks[idx].track_loop_duration.is_some()
                                {
                                    Some(self.tracks[idx].track_play_progress(now))
                                } else {
                                    None
                                };
                                engine.pause_at_progress_now(idx, progress);
                            } else {
                                engine.clear_track_now(idx);
                            }
                        }
                    }
                    TrackState::Dub => {
                        let now = Instant::now();
                        let beat_time = self.metronome.get_beat_time();
                        if self.tracks[idx].track_play_anchor_at.is_none() {
                            self.tracks[idx].track_play_anchor_at = Some(beat_time);
                        }
                        if let Some(engine) = audio {
                            engine.play_now(idx);
                            let start_at = self.next_loop_boundary(idx, now).unwrap_or(beat_time);
                            engine.overdub_at(idx, start_at);
                        }
                    }
                }
            }

            // self.tracks[idx].update_timeline(now);
            self.tracks[idx].prev_track_state = current;
            if current == TrackState::NxtPlay
                && audio.is_some_and(|engine| engine.track_playing_only(idx))
            {
                if self.tracks[idx].stop_after_finish {
                    self.tracks[idx].track_state = TrackState::Pause;
                    self.tracks[idx].stop_after_finish = false;
                } else {
                    self.tracks[idx].track_state = TrackState::Play;
                    self.tracks[idx].prev_track_state = TrackState::Play;
                }
            }

            if matches!(
                current,
                TrackState::Play | TrackState::NxtPlay | TrackState::Dub
            ) {
                if let Some(engine) = audio {
                    if self.tracks[idx].track_play_anchor_at.is_some()
                        && self.tracks[idx].track_loop_duration.is_some()
                    {
                        let progress = self.tracks[idx].track_play_progress(now);
                        // Keep audio cursor synced with logical timeline to avoid phase drift buildup.
                        engine.sync_playhead_if_drift(idx, progress, 0.01);
                    }
                }
            }
        }
    }
}

impl eframe::App for MyApp {
    #[cfg(debug_assertions)]
    fn raw_input_hook(&mut self, _ctx: &egui::Context, input: &mut egui::RawInput) {
        if std::env::args().any(|arg| arg.starts_with("--ui-preview=")) {
            input.events.retain(|event| {
                !matches!(
                    event,
                    egui::Event::PointerMoved(_) | egui::Event::PointerButton { .. }
                )
            });
            input.events.push(egui::Event::PointerGone);
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(debug_assertions)]
        if let Some(mode) =
            std::env::args().find_map(|arg| arg.strip_prefix("--ui-preview=").map(str::to_owned))
        {
            if ui::preview::capture(ctx, &mode, &mut self.preview_frame) {
                self.allow_window_close = true;
            }
        }
        if ctx.input(|i| i.viewport().close_requested()) {
            if self.allow_window_close {
                // Allow OS close request to pass through without interception.
            } else if self.show_save_prompt {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            } else if self.app_state != AppState::Init {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.request_exit(PendingExit::CloseWindow);
            }
        }

        ctx.request_repaint_after(Duration::from_millis(16));
        self.setup_font_fallback(ctx);
        let keyboard_active =
            ctx.input(|i| i.focused) && !ctx.wants_keyboard_input() && !self.editor.expanded;
        if self.show_save_prompt || keyboard_active {
            self.handle_input(ctx);
        }
        if !keyboard_active || self.app_state != AppState::MainLoop || self.show_save_prompt {
            self.fader_keys.fill(faders::KeyFader::default());
        }
        if !self.show_save_prompt && self.app_state != AppState::Init {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::S)) {
                self.save_now();
            }
            if self.editor.expanded
                && !ctx.wants_keyboard_input()
                && ctx.input(|i| i.key_pressed(egui::Key::Escape))
            {
                self.editor.expanded = false;
            }
        }
        self.handle_config();
        self.handle_track();

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::Frame::none()
                .fill(ui::theme::BACKGROUND)
                .show(ui, |ui| {
                    ui.set_min_size(ui.available_size());

                    ui.set_enabled(!self.show_save_prompt);
                    match self.app_state {
                        AppState::Init => ui::init::draw_init(ui, self),
                        AppState::MainLoop | AppState::MainScreen => {
                            ui::performance::draw(ui, self)
                        }
                    }
                });
        });

        if self.show_save_prompt {
            egui::Window::new("Save Project")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.label("Save current project parameters before exit?");
                    ui.label("Loop recordings are session-only and are not saved.");
                    ui.horizontal(|ui| {
                        if ui.button("Save and exit [Y]").clicked() {
                            self.finish_exit(true);
                        }
                        if ui.button("Discard [N]").clicked() {
                            self.finish_exit(false);
                        }
                        if ui.button("Cancel [Esc]").clicked() {
                            self.show_save_prompt = false;
                            self.pending_exit = None;
                        }
                    });
                    if self.status.starts_with("Save failed") {
                        ui.colored_label(egui::Color32::LIGHT_RED, &self.status);
                    }
                });
        }

        if self.close_window_queued {
            self.close_window_queued = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}
