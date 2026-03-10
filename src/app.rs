use eframe::egui;
use std::time::{Instant};

use crate::config::{AppConfig, ConfigSet};
use crate::config::envelope_configs::{
    ENVELOPE_ATTACK_MAX_MS, ENVELOPE_DECAY_MAX_MS, ENVELOPE_HOLD_MAX_MS,
    ENVELOPE_RELEASE_MAX_MS, ENVELOPE_START_MAX_PCT, ENVELOPE_SUSTAIN_MAX_PCT, ENVELOPE_TENSION_MAX,
};
use crate::config::filter_configs::{
    FILTER_CUTOFF_MAX_HZ, FILTER_CUTOFF_MIN_HZ, FILTER_DRIVE_MAX, FILTER_MIX_MAX, FILTER_Q_MAX_X10,
    FILTER_Q_MIN_X10,
};
use crate::engine::audio_io::AudioIO;
use crate::engine::metronome::Metronome;
use crate::project::{self, ProjectEntry};
use crate::state::{AppState, FxState, ScreenState, TrackState, ProjectNameMode, PendingExit};
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
}

impl MyApp {
    pub fn new() -> Self {
        let config = AppConfig::new(
            DEFAULT_BPM, 
            DEFAULT_LATENCY_COMP, 
        );
        let audio_io: Result<AudioIO, anyhow::Error> = AudioIO::new(
            &config.system_config.input_device.value,
            &config.system_config.output_device.value,
            TRACK_COUNT,
            config.beat_config.current_latency(),
        );
        let mut projects = project::load_index();
        if projects.is_empty() {
            projects.push(ProjectEntry {
                name: "DEFAULT".to_string(),
                file: project::make_project_file_name("DEFAULT", 0),
            });
            let _ = project::save_index(&projects);
        }

        Self {
            metronome: Metronome::new(DEFAULT_BPM),
            audio_io,
            app_state: AppState::Init,
            tracks: vec![Track::new(); TRACK_COUNT],
            track_sel: None,
            screen_state: ScreenState::Empty,
            fx_state: FxState::Single, 
            fx_screen_slot_idx: 0,
            fx_edit_row_idx: 0,
            // tracks: (0..TRACK_COUNT).map(Track::new).collect(),
            config,
            projects,
            sel_project_idx: 0,
            project_name_input: String::new(),
            project_name_mode: None,
            active_project_idx: None,
            pending_exit: None,
            show_save_prompt: false,
            allow_window_close: false,
            close_window_queued: false,
            fonts_initialized: false,
        }
    }

    fn normalize_project_selection(&mut self) {
        let max_idx = self.projects.len();
        if self.sel_project_idx > max_idx {
            self.sel_project_idx = max_idx;
        }
    }

    fn load_selected_project(&mut self) {
        if self.sel_project_idx >= self.projects.len() {
            return;
        }
        if let Ok(audio) = self.audio_io.as_ref() {
            audio.clear_all_tracks_now();
        }
        for track in &mut self.tracks {
            track.track_state = TrackState::Empty;
            track.prev_track_state = TrackState::Empty;
            track.track_record_start_at = None;
            track.track_loop_duration = None;
            track.track_play_anchor_at = None;
        }
        self.config = AppConfig::new(DEFAULT_BPM, DEFAULT_LATENCY_COMP);
        let entry = self.projects[self.sel_project_idx].clone();
        if let Some(data) = project::load_project(&entry) {
            project::apply_data_to_config(&mut self.config, data);
        }
        self.active_project_idx = Some(self.sel_project_idx);
        self.app_state = AppState::MainLoop;
    }

    fn save_active_project(&mut self) {
        if let Some(idx) = self.active_project_idx {
            if let Some(entry) = self.projects.get(idx) {
                let _ = project::save_project(entry, &self.config);
            }
        }
    }

    fn request_exit(&mut self, target: PendingExit) {
        self.pending_exit = Some(target);
        self.show_save_prompt = true;
    }

    fn finish_exit(&mut self, save: bool) {
        if save {
            self.save_active_project();
        }
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
                fonts
                    .font_data
                    .insert("cjk_fallback".to_owned(), egui::FontData::from_owned(bytes).into());
                fonts
                    .families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .insert(0, "cjk_fallback".to_owned());
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

    fn handle_input(&mut self, ctx: &egui::Context) {
        ctx.input(|i| match self.app_state {
            _ if self.show_save_prompt => {
                if i.key_pressed(egui::Key::Y) {
                    self.finish_exit(true);
                } else if i.key_pressed(egui::Key::N) {
                    self.finish_exit(false);
                } else if i.key_pressed(egui::Key::Escape) {
                    self.show_save_prompt = false;
                    self.pending_exit = None;
                }
            }
            AppState::Init => {
                if let Some(mode) = self.project_name_mode {
                    for event in &i.events {
                        if let egui::Event::Text(text) = event {
                            self.project_name_input.push_str(text);
                        }
                    }
                    if i.key_pressed(egui::Key::Backspace) {
                        self.project_name_input.pop();
                    }
                    if i.key_pressed(egui::Key::Escape) {
                        self.project_name_mode = None;
                        self.project_name_input.clear();
                    }
                    if i.key_pressed(egui::Key::Enter) {
                        let name = self.project_name_input.trim().to_string();
                        if !name.is_empty() {
                            match mode {
                                ProjectNameMode::Add => {
                                    let idx = self.projects.len();
                                    self.projects.push(ProjectEntry {
                                        name: name.clone(),
                                        file: project::make_project_file_name(&name, idx),
                                    });
                                    self.sel_project_idx = idx;
                                }
                                ProjectNameMode::Rename => {
                                    if self.sel_project_idx < self.projects.len() {
                                        self.projects[self.sel_project_idx].name = name;
                                    }
                                }
                            }
                            let _ = project::save_index(&self.projects);
                        }
                        self.project_name_mode = None;
                        self.project_name_input.clear();
                    }
                    return;
                }

                if i.key_pressed(egui::Key::T) {
                    self.fx_state = if self.fx_state == FxState::Single {FxState::Bank} else {FxState::Single}; 
                }
                if i.key_pressed(egui::Key::ArrowDown) {
                    self.sel_project_idx = (self.sel_project_idx + 1).min(self.projects.len());
                }
                if i.key_pressed(egui::Key::ArrowUp) && self.sel_project_idx > 0 {
                    self.sel_project_idx -= 1;
                }

                if i.key_pressed(egui::Key::Enter) {
                    if self.sel_project_idx == self.projects.len() {
                        self.project_name_mode = Some(ProjectNameMode::Add);
                        self.project_name_input = format!("PROJECT_{}", self.projects.len());
                    } else {
                        self.load_selected_project();
                    }
                }

                if i.key_pressed(egui::Key::Delete) && self.sel_project_idx < self.projects.len() {
                    if self.active_project_idx == Some(self.sel_project_idx) {
                        self.active_project_idx = None;
                    }
                    let removed = self.projects.remove(self.sel_project_idx);
                    project::remove_project_file(&removed.file);
                    let _ = project::save_index(&self.projects);
                    self.normalize_project_selection();
                }

                if i.key_pressed(egui::Key::R) && self.sel_project_idx < self.projects.len() {
                    self.project_name_mode = Some(ProjectNameMode::Rename);
                    self.project_name_input = self.projects[self.sel_project_idx].name.clone();
                }
            }
            AppState::MainLoop => {
                if i.key_pressed(egui::Key::T) {
                    self.fx_state = if self.fx_state == FxState::Single {FxState::Bank} else {FxState::Single}; 
                }
                if i.key_pressed(egui::Key::Escape) {
                    self.request_exit(PendingExit::ToInit);
                }
                if i.key_pressed(egui::Key::S) {
                    self.app_state = AppState::MainScreen
                }

                let track_record_keys = [
                    egui::Key::Num1,
                    egui::Key::Num2,
                    egui::Key::Num3,
                    egui::Key::Num4,
                    egui::Key::Num5,
                ];
                for (idx, key) in track_record_keys.iter().enumerate() {
                    if i.key_pressed(*key) {
                        self.tracks[idx].track_state = match self.tracks[idx].track_state {
                            TrackState::Empty => TrackState::Record,
                            TrackState::Play | TrackState::NxtPlay => TrackState::Dub,
                            TrackState::Record => TrackState::NxtPlay,
                            TrackState::Dub => TrackState::NxtPlay,
                            TrackState::Pause => TrackState::Play,
                        };
                    }
                }

                let track_pause_keys = [
                    egui::Key::F1,
                    egui::Key::F2,
                    egui::Key::F3,
                    egui::Key::F4,
                    egui::Key::F5,
                ];
                for (idx, key) in track_pause_keys.iter().enumerate() {
                    if i.key_pressed(*key) {
                        self.tracks[idx].track_state = match self.tracks[idx].track_state {
                            TrackState::Play | TrackState::NxtPlay => TrackState::Pause,
                            TrackState::Empty => TrackState::Empty,
                            TrackState::Record => TrackState::Record,
                            TrackState::Dub => TrackState::Dub,
                            TrackState::Pause => TrackState::Pause,
                        };
                    }
                }

                if i.key_pressed(egui::Key::ArrowLeft) {
                    self.track_sel = match self.track_sel {
                        None => Some(4),
                        Some(0) => Some(4),
                        Some(1) => Some(0),
                        Some(2) => Some(1),
                        Some(3) => Some(2),
                        Some(4) => Some(3),
                        _ => Some(4),
                    };
                }
                if i.key_pressed(egui::Key::ArrowRight) {
                    self.track_sel = match self.track_sel {
                        None => Some(0),
                        Some(0) => Some(1),
                        Some(1) => Some(2),
                        Some(2) => Some(3),
                        Some(3) => Some(4),
                        Some(4) => Some(0),
                        _ => Some(0),
                    };
                }
                if let Some(sel_idx) = self.track_sel {
                    if i.key_pressed(egui::Key::Delete) {
                        self.tracks[sel_idx].track_state = match self.tracks[sel_idx].track_state {
                            TrackState::Play | TrackState::NxtPlay => TrackState::Empty,
                            TrackState::Empty => TrackState::Empty,
                            TrackState::Record => TrackState::Record,
                            TrackState::Dub => TrackState::Dub,
                            TrackState::Pause => TrackState::Empty,
                        };
                    }
                }

                // input fx
                let fx_keys = [
                    egui::Key::Q,
                    egui::Key::W,
                    egui::Key::E,
                    egui::Key::R,
                ];
                for (slot_idx, key) in fx_keys.iter().enumerate() {
                    if i.key_pressed(*key) {
                        match self.fx_state {
                            FxState::Bank => {
                                self.config.input_fx.select_bank(slot_idx);
                            }
                            FxState::Single => {
                                self.config.input_fx.toggle_slot_enabled(slot_idx);
                            }
                        }
                    }
                }
                
            }
            AppState::MainScreen => {
                if i.key_pressed(egui::Key::T) {
                    self.fx_state = if self.fx_state == FxState::Single {FxState::Bank} else {FxState::Single}; 
                }
                if i.key_pressed(egui::Key::Escape) {
                    match self.screen_state {
                        ScreenState::Empty => self.request_exit(PendingExit::ToInit),
                        ScreenState::InFxFilter => self.screen_state = ScreenState::FxSelect,
                        ScreenState::InFxOscAudioEnv => self.screen_state = ScreenState::InFxOscAudio,
                        ScreenState::InFxOscFilterEnv => self.screen_state = ScreenState::InFxOscFilter,
                        ScreenState::InFxOscAudio => self.screen_state = ScreenState::InFxOsc,
                        ScreenState::InFxNote => self.screen_state = ScreenState::InFxOsc,
                        ScreenState::InFxOscFilter => self.screen_state = ScreenState::InFxOsc,
                        ScreenState::InFxOsc => self.screen_state = ScreenState::FxSelect,
                        _ => self.screen_state = ScreenState::Empty,
                    }
                }
                if i.key_pressed(egui::Key::S) {
                    self.app_state = AppState::MainLoop
                }
                if i.key_pressed(egui::Key::B) {
                    if self.screen_state != ScreenState::Beat {
                        self.screen_state = ScreenState::Beat;
                    } else {
                        self.screen_state = ScreenState::Empty;
                    }
                }
                if i.key_pressed(egui::Key::M) {
                    if self.screen_state != ScreenState::SYS {
                        self.screen_state = ScreenState::SYS;
                    } else {
                        self.screen_state = ScreenState::Empty;
                    }
                }

                if self.screen_state == ScreenState::Empty {
                    let fx_keys = [
                        egui::Key::Q,
                        egui::Key::W,
                        egui::Key::E,
                        egui::Key::R,
                    ];
                    for (slot_idx, key) in fx_keys.iter().enumerate() {
                        if i.key_pressed(*key) {
                            match self.fx_state {
                                FxState::Bank => {
                                    self.config.input_fx.select_bank(slot_idx);
                                }
                                FxState::Single => {
                                    self.fx_screen_slot_idx = slot_idx;
                                    self.fx_edit_row_idx = 0;
                                    self.screen_state = ScreenState::FxSelect;
                                }
                            }
                        }
                    }
                }

                match self.screen_state {
                    ScreenState::Beat => {
                        let beat_config = &mut self.config.beat_config;
                        if i.key_pressed(egui::Key::ArrowLeft) {
                            beat_config.prev();
                        }
                        if i.key_pressed(egui::Key::ArrowRight) {
                            beat_config.next();
                        }
                        if beat_config.sel_idx == Some(0) {
                            if i.key_pressed(egui::Key::Space) {
                                beat_config.tap_calc.calculate_avg_bpm();
                                beat_config.confirm();
                            }
                            beat_config.input_bpm.input(i, MAX_BPM);
                        }
                        if beat_config.sel_idx == Some(1) {
                            beat_config.input_latency.input(i, MAX_LATENCY_COMP);
                        }
                    }
                    ScreenState::SYS => {
                        let sys_config = &mut self.config.system_config;
                        if i.key_pressed(egui::Key::ArrowLeft) {
                            sys_config.prev();
                        }
                        if i.key_pressed(egui::Key::ArrowRight) {
                            sys_config.next();
                        }

                        match sys_config.sel_idx {
                            Some(0) => {
                                if i.key_pressed(egui::Key::ArrowUp) {
                                    sys_config.input_device.prev();
                                }
                                if i.key_pressed(egui::Key::ArrowDown) {
                                    sys_config.input_device.next();
                                }
                            }
                            Some(1) => {
                                if i.key_pressed(egui::Key::ArrowUp) {
                                    sys_config.output_device.prev();
                                }
                                if i.key_pressed(egui::Key::ArrowDown) {
                                    sys_config.output_device.next();
                                }
                            }
                            _ => {}
                        }

                        // if i.key_pressed(egui::Key::Enter) {
                        //     sys_config.confirm();
                        // }
                    }
                    ScreenState::FxSelect => {
                        let bank_idx = self.config.input_fx.sel_bank_idx;
                        let slot_idx = self.fx_screen_slot_idx;
                        if i.key_pressed(egui::Key::ArrowLeft) {
                            self.config.input_fx.cycle_slot_kind(bank_idx, slot_idx, -1);
                        }
                        if i.key_pressed(egui::Key::ArrowRight) {
                            self.config.input_fx.cycle_slot_kind(bank_idx, slot_idx, 1);
                        }

                        if i.key_pressed(egui::Key::Enter) {
                            self.fx_edit_row_idx = 0;
                            if let Some(fx) = self.config.input_fx.banks[bank_idx].slots[slot_idx].fx.as_mut() {
                                if let Some(osc) = fx.as_osc_mut() {
                                    osc.sel_idx = Some(0);
                                    self.screen_state = ScreenState::InFxOsc;
                                } else if let Some(filter) = fx.as_filter_mut() {
                                    filter.sel_idx = Some(0);
                                    self.screen_state = ScreenState::InFxFilter;
                                }
                            }
                        }
                    }
                    ScreenState::InFxOsc => {
                        let bank_idx = self.config.input_fx.sel_bank_idx;
                        let slot_idx = self.fx_screen_slot_idx;
                        let slot = &mut self.config.input_fx.banks[bank_idx].slots[slot_idx];
                        if let Some(fx) = slot.fx.as_mut() {
                            if let Some(osc) = fx.as_osc_mut() {
                                if i.key_pressed(egui::Key::ArrowLeft) {
                                    osc.prev();
                                }
                                if i.key_pressed(egui::Key::ArrowRight) {
                                    osc.next();
                                }

                                match osc.sel_idx {
                                    Some(0) => {}
                                    Some(1) => {}
                                    Some(2) => {}
                                    _ => {}
                                }

                                if i.key_pressed(egui::Key::Enter) {
                                    match osc.sel_idx {
                                        Some(0) => {
                                            osc.audio_sel_idx = Some(0);
                                            self.screen_state = ScreenState::InFxOscAudio;
                                        }
                                        Some(1) => {
                                            osc.note.sel_idx = Some(0);
                                            self.screen_state = ScreenState::InFxNote;
                                        }
                                        Some(2) => {
                                            osc.osc_filter_sel_idx = Some(0);
                                            self.screen_state = ScreenState::InFxOscFilter;
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                    ScreenState::InFxOscAudio => {
                        let bank_idx = self.config.input_fx.sel_bank_idx;
                        let slot_idx = self.fx_screen_slot_idx;
                        let slot = &mut self.config.input_fx.banks[bank_idx].slots[slot_idx];
                        if let Some(fx) = slot.fx.as_mut() {
                            if let Some(osc) = fx.as_osc_mut() {
                                let curr = osc.audio_sel_idx.unwrap_or(0);
                                if i.key_pressed(egui::Key::ArrowLeft) {
                                    osc.audio_sel_idx = Some(curr.saturating_sub(1));
                                }
                                if i.key_pressed(egui::Key::ArrowRight) {
                                    osc.audio_sel_idx = Some((curr + 1).min(3));
                                }

                                match osc.audio_sel_idx {
                                    Some(0) => {
                                        if i.key_pressed(egui::Key::ArrowUp) {
                                            osc.waveform.prev();
                                        }
                                        if i.key_pressed(egui::Key::ArrowDown) {
                                            osc.waveform.next();
                                        }
                                    }
                                    Some(1) => osc.level.input(i, MAX_FX_LEVEL),
                                    Some(2) => osc.threshold.input(i, MAX_FX_THRESHOLD),
                                    Some(3) => {
                                        if i.key_pressed(egui::Key::Enter) {
                                            osc.envelope.sel_idx = Some(0);
                                            self.screen_state = ScreenState::InFxOscAudioEnv;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    ScreenState::InFxNote => {
                        let bank_idx = self.config.input_fx.sel_bank_idx;
                        let slot_idx = self.fx_screen_slot_idx;
                        let slot = &mut self.config.input_fx.banks[bank_idx].slots[slot_idx];
                        if let Some(fx) = slot.fx.as_mut() {
                            if let Some(osc) = fx.as_osc_mut() {
                                let note_cfg = &mut osc.note;
                                if i.key_pressed(egui::Key::ArrowLeft) {
                                    note_cfg.prev();
                                }
                                if i.key_pressed(egui::Key::ArrowRight) {
                                    note_cfg.next();
                                }

                                match note_cfg.sel_idx {
                                    Some(0) => {
                                        if i.key_pressed(egui::Key::ArrowUp) {
                                            note_cfg.note.prev();
                                        }
                                        if i.key_pressed(egui::Key::ArrowDown) {
                                            note_cfg.note.next();
                                        }
                                    }
                                    Some(1) => {
                                        if i.key_pressed(egui::Key::ArrowUp) {
                                            note_cfg.octave.prev();
                                        }
                                        if i.key_pressed(egui::Key::ArrowDown) {
                                            note_cfg.octave.next();
                                        }
                                    }
                                    Some(2) => {
                                        if i.key_pressed(egui::Key::ArrowUp) {
                                            note_cfg.step.prev();
                                        }
                                        if i.key_pressed(egui::Key::ArrowDown) {
                                            note_cfg.step.next();
                                        }
                                    }
                                    Some(3) => {
                                        if i.key_pressed(egui::Key::ArrowUp) {
                                            note_cfg.edit.prev();
                                        }
                                        if i.key_pressed(egui::Key::ArrowDown) {
                                            note_cfg.edit.next();
                                        }
                                    }
                                    _ => {}
                                }

                                if i.key_pressed(egui::Key::Enter) && note_cfg.sel_idx == Some(3) {
                                    note_cfg.apply_edit();
                                }
                            }
                        }
                    }
                    ScreenState::InFxOscAudioEnv => {
                        let bank_idx = self.config.input_fx.sel_bank_idx;
                        let slot_idx = self.fx_screen_slot_idx;
                        let slot = &mut self.config.input_fx.banks[bank_idx].slots[slot_idx];
                        if let Some(fx) = slot.fx.as_mut() {
                            if let Some(osc) = fx.as_osc_mut() {
                                let env_cfg = &mut osc.envelope;
                                if i.key_pressed(egui::Key::ArrowLeft) {
                                    env_cfg.prev();
                                }
                                if i.key_pressed(egui::Key::ArrowRight) {
                                    env_cfg.next();
                                }

                                match env_cfg.sel_idx {
                                    Some(0) => env_cfg.attack_ms.input(i, ENVELOPE_ATTACK_MAX_MS),
                                    Some(1) => env_cfg.hold_ms.input(i, ENVELOPE_HOLD_MAX_MS),
                                    Some(2) => env_cfg.decay_ms.input(i, ENVELOPE_DECAY_MAX_MS),
                                    Some(3) => env_cfg.sustain_pct.input(i, ENVELOPE_SUSTAIN_MAX_PCT),
                                    Some(4) => {
                                        env_cfg.release_ms.input(i, ENVELOPE_RELEASE_MAX_MS);
                                    }
                                    Some(5) => env_cfg.start_pct.input(i, ENVELOPE_START_MAX_PCT),
                                    Some(6) => env_cfg.tension_a.input(i, ENVELOPE_TENSION_MAX),
                                    Some(7) => env_cfg.tension_d.input(i, ENVELOPE_TENSION_MAX),
                                    Some(8) => env_cfg.tension_r.input(i, ENVELOPE_TENSION_MAX),
                                    _ => {}
                                }
                            }
                        }
                    }
                    ScreenState::InFxOscFilter => {
                        let bank_idx = self.config.input_fx.sel_bank_idx;
                        let slot_idx = self.fx_screen_slot_idx;
                        let slot = &mut self.config.input_fx.banks[bank_idx].slots[slot_idx];
                        if let Some(fx) = slot.fx.as_mut() {
                            if let Some(osc) = fx.as_osc_mut() {
                                let curr = osc.osc_filter_sel_idx.unwrap_or(0);
                                if i.key_pressed(egui::Key::ArrowLeft) {
                                    osc.osc_filter_sel_idx = Some(curr.saturating_sub(1));
                                }
                                if i.key_pressed(egui::Key::ArrowRight) {
                                    osc.osc_filter_sel_idx = Some((curr + 1).min(5));
                                }
                                let filter = &mut osc.osc_filter;

                                match osc.osc_filter_sel_idx {
                                    Some(0) => {
                                        if i.key_pressed(egui::Key::ArrowUp) {
                                            filter.filter_type.prev();
                                        }
                                        if i.key_pressed(egui::Key::ArrowDown) {
                                            filter.filter_type.next();
                                        }
                                    }
                                    Some(1) => {
                                        filter.cutoff_hz.input(i, FILTER_CUTOFF_MAX_HZ);
                                        filter.cutoff_hz.value =
                                            filter.cutoff_hz.value.clamp(FILTER_CUTOFF_MIN_HZ, FILTER_CUTOFF_MAX_HZ);
                                        if i.key_pressed(egui::Key::ArrowUp) {
                                            filter.cutoff_hz.value = ((filter.cutoff_hz.value as f32) * 1.06).round()
                                                as usize;
                                            filter.cutoff_hz.value =
                                                filter.cutoff_hz.value.clamp(FILTER_CUTOFF_MIN_HZ, FILTER_CUTOFF_MAX_HZ);
                                        }
                                        if i.key_pressed(egui::Key::ArrowDown) {
                                            filter.cutoff_hz.value = ((filter.cutoff_hz.value as f32) / 1.06).round()
                                                as usize;
                                            filter.cutoff_hz.value =
                                                filter.cutoff_hz.value.clamp(FILTER_CUTOFF_MIN_HZ, FILTER_CUTOFF_MAX_HZ);
                                        }
                                    }
                                    Some(2) => {
                                        filter.resonance_x10.input(i, FILTER_Q_MAX_X10);
                                        filter.resonance_x10.value =
                                            filter.resonance_x10.value.clamp(FILTER_Q_MIN_X10, FILTER_Q_MAX_X10);
                                    }
                                    Some(3) => filter.drive.input(i, FILTER_DRIVE_MAX),
                                    Some(4) => filter.mix.input(i, FILTER_MIX_MAX),
                                    Some(5) => {
                                        if i.key_pressed(egui::Key::Enter) {
                                            osc.osc_filter_env.sel_idx = Some(0);
                                            self.screen_state = ScreenState::InFxOscFilterEnv;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    ScreenState::InFxOscFilterEnv => {
                        let bank_idx = self.config.input_fx.sel_bank_idx;
                        let slot_idx = self.fx_screen_slot_idx;
                        let slot = &mut self.config.input_fx.banks[bank_idx].slots[slot_idx];
                        if let Some(fx) = slot.fx.as_mut() {
                            if let Some(osc) = fx.as_osc_mut() {
                                let env_cfg = &mut osc.osc_filter_env;
                                if i.key_pressed(egui::Key::ArrowLeft) {
                                    env_cfg.prev();
                                }
                                if i.key_pressed(egui::Key::ArrowRight) {
                                    env_cfg.next();
                                }

                                match env_cfg.sel_idx {
                                    Some(0) => env_cfg.attack_ms.input(i, ENVELOPE_ATTACK_MAX_MS),
                                    Some(1) => env_cfg.hold_ms.input(i, ENVELOPE_HOLD_MAX_MS),
                                    Some(2) => env_cfg.decay_ms.input(i, ENVELOPE_DECAY_MAX_MS),
                                    Some(3) => env_cfg.sustain_pct.input(i, ENVELOPE_SUSTAIN_MAX_PCT),
                                    Some(4) => {
                                        env_cfg.release_ms.input(i, ENVELOPE_RELEASE_MAX_MS);
                                    }
                                    Some(5) => env_cfg.start_pct.input(i, ENVELOPE_START_MAX_PCT),
                                    Some(6) => env_cfg.tension_a.input(i, ENVELOPE_TENSION_MAX),
                                    Some(7) => env_cfg.tension_d.input(i, ENVELOPE_TENSION_MAX),
                                    Some(8) => env_cfg.tension_r.input(i, ENVELOPE_TENSION_MAX),
                                    _ => {}
                                }
                            }
                        }
                    }
                    ScreenState::InFxFilter => {
                        let bank_idx = self.config.input_fx.sel_bank_idx;
                        let slot_idx = self.fx_screen_slot_idx;
                        let slot = &mut self.config.input_fx.banks[bank_idx].slots[slot_idx];
                        if let Some(fx) = slot.fx.as_mut() {
                            if let Some(filter) = fx.as_filter_mut() {
                                if i.key_pressed(egui::Key::ArrowLeft) {
                                    filter.prev();
                                }
                                if i.key_pressed(egui::Key::ArrowRight) {
                                    filter.next();
                                }

                                match filter.sel_idx {
                                    Some(0) => {
                                        if i.key_pressed(egui::Key::ArrowUp) {
                                            filter.filter_type.prev();
                                        }
                                        if i.key_pressed(egui::Key::ArrowDown) {
                                            filter.filter_type.next();
                                        }
                                    }
                                    Some(1) => {
                                        filter.cutoff_hz.input(i, FILTER_CUTOFF_MAX_HZ);
                                        filter.cutoff_hz.value =
                                            filter.cutoff_hz.value.clamp(FILTER_CUTOFF_MIN_HZ, FILTER_CUTOFF_MAX_HZ);
                                        if i.key_pressed(egui::Key::ArrowUp) {
                                            filter.cutoff_hz.value = ((filter.cutoff_hz.value as f32) * 1.06)
                                                .round() as usize;
                                            filter.cutoff_hz.value =
                                                filter.cutoff_hz.value.clamp(FILTER_CUTOFF_MIN_HZ, FILTER_CUTOFF_MAX_HZ);
                                        }
                                        if i.key_pressed(egui::Key::ArrowDown) {
                                            filter.cutoff_hz.value = ((filter.cutoff_hz.value as f32) / 1.06)
                                                .round() as usize;
                                            filter.cutoff_hz.value =
                                                filter.cutoff_hz.value.clamp(FILTER_CUTOFF_MIN_HZ, FILTER_CUTOFF_MAX_HZ);
                                        }
                                    }
                                    Some(2) => {
                                        filter.resonance_x10.input(i, FILTER_Q_MAX_X10);
                                        filter.resonance_x10.value =
                                            filter.resonance_x10.value.clamp(FILTER_Q_MIN_X10, FILTER_Q_MAX_X10);
                                    }
                                    Some(3) => {
                                        filter.drive.input(i, FILTER_DRIVE_MAX);
                                    }
                                    Some(4) => {
                                        filter.mix.input(i, FILTER_MIX_MAX);
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    ScreenState::Empty => {}
                }
            }
        });
    }

    fn handle_config(&mut self) {
        // beat config -> metronome
        self.metronome.adjust_bpm(self.config.beat_config.current_bpm());
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
                audio.update_input_fx(&self.config.input_fx);
                audio.update_metronome(self.metronome.start_time(), self.metronome.current_bpm());
                if let Err(err) = audio.switch_devices(&desired_input, &desired_output) {
                    eprintln!("Failed to switch audio devices: {err}");
                    self.config.system_config.input_device.value = audio.curr_input_name().to_string();
                    self.config.system_config.output_device.value =
                        audio.curr_output_name().to_string();
                }
            }
            Err(_) => {
                self.audio_io = AudioIO::new(
                    &desired_input,
                    &desired_output,
                    TRACK_COUNT,
                    desired_latency,
                );
                if let Ok(audio) = self.audio_io.as_mut() {
                    audio.set_realtime_enabled(self.app_state != AppState::Init);
                    audio.update_input_fx(&self.config.input_fx);
                    audio.update_metronome(self.metronome.start_time(), self.metronome.current_bpm());
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
                track.track_play_anchor_at = None;
            }
            return;
        }

        let on_track = self.tracks
            .iter()
            .filter(|t| 
                t.track_state == TrackState::Record || 
                t.track_state == TrackState::Play || 
                t.track_state == TrackState::Dub || 
                t.track_state ==TrackState::NxtPlay
            )
            .count();
        if on_track == 0 {self.metronome.reset();}

        let now = Instant::now();
        let audio = self.audio_io.as_ref().ok();

        for idx in 0..TRACK_COUNT {
            let current = self.tracks[idx].track_state;
            let previous = self.tracks[idx].prev_track_state;

            if current != previous {
                match current {
                    TrackState::Record => {
                        let beat_time = self.metronome.get_beat_time();
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
                            self.tracks[idx].track_loop_duration = match self.tracks[idx].track_record_start_at {
                                Some(start) if beat_time > start => Some(beat_time.duration_since(start)),
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
                        let metronome_running = self.metronome.start_time().is_some();
                        let progress = if metronome_running {
                            Some(self.tracks[idx].track_play_progress(now))
                        } else {
                            self.tracks[idx].track_play_anchor_at = Some(now);
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
                            engine.pause_now(idx);
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

            if matches!(current, TrackState::Play | TrackState::NxtPlay | TrackState::Dub) {
                if let Some(engine) = audio {
                    if self.tracks[idx].track_play_anchor_at.is_some() && self.tracks[idx].track_loop_duration.is_some() {
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
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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

        ctx.request_repaint();
        self.setup_font_fallback(ctx);
        self.handle_input(ctx);
        self.handle_config();
        self.handle_track();

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(15, 15, 15))
                .show(ui, |ui| {
                    ui.set_min_size(ui.available_size());

                    match self.app_state {
                        AppState::Init => ui::init::draw_init(ui, self),
                        AppState::MainLoop | AppState::MainScreen => {
                            ui::looper::draw_loopstation_view(ui, self)
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
                    ui.label("[Y] Save  [N] Discard  [Esc] Cancel");
                });
        }

        if self.close_window_queued {
            self.close_window_queued = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}
