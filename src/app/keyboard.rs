use super::*;

impl MyApp {
    pub(super) fn handle_input(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.modifiers.ctrl || i.modifiers.command) {
            self.fader_keys.fill(faders::KeyFader::default());
            return;
        }
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
                    self.fx_state = if self.fx_state == FxState::Single {
                        FxState::Bank
                    } else {
                        FxState::Single
                    };
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
                let pairs = [
                    (egui::Key::Z, egui::Key::X),
                    (egui::Key::C, egui::Key::V),
                    (egui::Key::B, egui::Key::N),
                    (egui::Key::M, egui::Key::Comma),
                    (egui::Key::Period, egui::Key::Slash),
                ];
                for (index, (down, up)) in pairs.into_iter().enumerate() {
                    let direction = i8::from(i.key_down(up)) - i8::from(i.key_down(down));
                    self.fader_keys[index].advance(
                        &mut self.config.track_levels[index],
                        direction,
                        i.modifiers.shift,
                        i.stable_dt,
                        self.config.fader_speed_db,
                    );
                }
                if pressed(i, egui::Key::Space) {
                    self.toggle_all();
                }
                if pressed(i, egui::Key::T) {
                    self.fx_state = if self.fx_state == FxState::Single {
                        FxState::Bank
                    } else {
                        FxState::Single
                    };
                }
                if pressed(i, egui::Key::Escape) {
                    self.request_exit(PendingExit::ToInit);
                }
                if pressed(i, egui::Key::S) {
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
                    if pressed(i, *key) {
                        self.trigger_track(idx);
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
                    if pressed(i, *key) {
                        self.pause_track(idx);
                    }
                }

                if pressed(i, egui::Key::ArrowLeft) {
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
                if pressed(i, egui::Key::ArrowRight) {
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
                    if pressed(i, egui::Key::Delete) {
                        self.clear_track(sel_idx);
                    }
                }

                // input fx
                let fx_keys = [egui::Key::Q, egui::Key::W, egui::Key::E, egui::Key::R];
                for (slot_idx, key) in fx_keys.iter().enumerate() {
                    if pressed(i, *key) {
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

                let track_fx_keys = [egui::Key::U, egui::Key::I, egui::Key::O, egui::Key::P];
                for (slot_idx, key) in track_fx_keys.iter().enumerate() {
                    if pressed(i, *key) {
                        match self.fx_state {
                            FxState::Bank => {
                                self.config.track_fx.select_bank(slot_idx);
                            }
                            FxState::Single => {
                                if let Some(track_idx) = self.track_sel {
                                    self.config
                                        .track_fx
                                        .toggle_slot_enabled(track_idx, slot_idx);
                                }
                            }
                        }
                    }
                }
            }
            AppState::MainScreen => {
                if i.key_pressed(egui::Key::T) {
                    self.fx_state = if self.fx_state == FxState::Single {
                        FxState::Bank
                    } else {
                        FxState::Single
                    };
                }
                if i.key_pressed(egui::Key::Escape) {
                    match self.screen_state {
                        ScreenState::Empty => self.request_exit(PendingExit::ToInit),
                        ScreenState::TrackFxSelect => self.screen_state = ScreenState::Empty,
                        ScreenState::InTrackFxDelay => {
                            self.screen_state = ScreenState::TrackFxSelect
                        }
                        ScreenState::InTrackFxRoll => {
                            self.screen_state = ScreenState::TrackFxSelect
                        }
                        ScreenState::InTrackFxFilter => {
                            self.screen_state = ScreenState::TrackFxSelect
                        }
                        ScreenState::InTrackFxFilterSeq => {
                            self.screen_state = ScreenState::InTrackFxFilter
                        }
                        ScreenState::InTrackFxFilterEnv => {
                            self.screen_state = ScreenState::InTrackFxFilter
                        }
                        ScreenState::InFxFilter => self.screen_state = ScreenState::FxSelect,
                        ScreenState::InFxReverb => self.screen_state = ScreenState::FxSelect,
                        ScreenState::InFxMyDelay => self.screen_state = ScreenState::FxSelect,
                        ScreenState::InFxMyDelayAudio => {
                            self.screen_state = ScreenState::InFxMyDelay
                        }
                        ScreenState::InFxMyDelayAudioEnv => {
                            self.screen_state = ScreenState::InFxMyDelayAudio
                        }
                        ScreenState::InFxMyDelayNote => {
                            self.screen_state = ScreenState::InFxMyDelay
                        }
                        ScreenState::InFxMyDelayFilter => {
                            self.screen_state = ScreenState::InFxMyDelay
                        }
                        ScreenState::InFxMyDelayFilterEnv => {
                            self.screen_state = ScreenState::InFxMyDelayFilter
                        }
                        ScreenState::InFxVocoder => self.screen_state = ScreenState::FxSelect,
                        ScreenState::InFxOscAudioEnv => {
                            self.screen_state = ScreenState::InFxOscAudio
                        }
                        ScreenState::InFxOscFilterEnv => {
                            self.screen_state = ScreenState::InFxOscFilter
                        }
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
                let input_fx_keys = [egui::Key::Q, egui::Key::W, egui::Key::E, egui::Key::R];
                for (slot_idx, key) in input_fx_keys.iter().enumerate() {
                    if i.key_pressed(*key) {
                        match self.fx_state {
                            FxState::Bank => {
                                self.config.input_fx.select_bank(slot_idx);
                            }
                            FxState::Single => {
                                if self.screen_state == ScreenState::Empty {
                                    self.fx_screen_slot_idx = slot_idx;
                                    self.fx_edit_row_idx = 0;
                                    self.screen_state = ScreenState::FxSelect;
                                } else if Self::is_input_fx_screen(self.screen_state) {
                                    self.fx_screen_slot_idx = slot_idx;
                                    self.fx_edit_row_idx = 0;
                                    self.normalize_input_fx_screen_for_current_slot();
                                }
                            }
                        }
                    }
                }
                if self.fx_state == FxState::Bank && self.screen_state != ScreenState::Empty {
                    let track_fx_keys = [egui::Key::U, egui::Key::I, egui::Key::O, egui::Key::P];
                    for (slot_idx, key) in track_fx_keys.iter().enumerate() {
                        if i.key_pressed(*key) {
                            self.config.track_fx.select_bank(slot_idx);
                        }
                    }
                }

                if self.screen_state == ScreenState::Empty {
                    let track_fx_keys = [egui::Key::U, egui::Key::I, egui::Key::O, egui::Key::P];
                    for (slot_idx, key) in track_fx_keys.iter().enumerate() {
                        if i.key_pressed(*key) {
                            match self.fx_state {
                                FxState::Bank => {
                                    self.config.track_fx.select_bank(slot_idx);
                                }
                                FxState::Single => {
                                    self.track_fx_screen_slot_idx = slot_idx;
                                    self.track_fx_edit_row_idx = 0;
                                    self.screen_state = ScreenState::TrackFxSelect;
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
                            if let Some(fx) = self.config.input_fx.banks[bank_idx].slots[slot_idx]
                                .fx
                                .as_mut()
                            {
                                if let Some(osc) = fx.as_osc_mut() {
                                    osc.sel_idx = Some(0);
                                    self.screen_state = ScreenState::InFxOsc;
                                } else if let Some(filter) = fx.as_filter_mut() {
                                    filter.sel_idx = Some(0);
                                    self.screen_state = ScreenState::InFxFilter;
                                } else if let Some(reverb) = fx.as_reverb_mut() {
                                    reverb.sel_idx = Some(0);
                                    self.screen_state = ScreenState::InFxReverb;
                                } else if let Some(delay) = fx.as_mydelay_mut() {
                                    delay.sel_idx = Some(0);
                                    self.screen_state = ScreenState::InFxMyDelay;
                                } else if let Some(vocoder) = fx.as_vocoder_mut() {
                                    vocoder.sel_idx = Some(0);
                                    self.screen_state = ScreenState::InFxVocoder;
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
                                    Some(3) => {
                                        env_cfg.sustain_pct.input(i, ENVELOPE_SUSTAIN_MAX_PCT)
                                    }
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
                                        filter.cutoff_hz.value = filter
                                            .cutoff_hz
                                            .value
                                            .clamp(FILTER_CUTOFF_MIN_HZ, FILTER_CUTOFF_MAX_HZ);
                                        if i.key_pressed(egui::Key::ArrowUp) {
                                            filter.cutoff_hz.value =
                                                ((filter.cutoff_hz.value as f32) * 1.06).round()
                                                    as usize;
                                            filter.cutoff_hz.value = filter
                                                .cutoff_hz
                                                .value
                                                .clamp(FILTER_CUTOFF_MIN_HZ, FILTER_CUTOFF_MAX_HZ);
                                        }
                                        if i.key_pressed(egui::Key::ArrowDown) {
                                            filter.cutoff_hz.value =
                                                ((filter.cutoff_hz.value as f32) / 1.06).round()
                                                    as usize;
                                            filter.cutoff_hz.value = filter
                                                .cutoff_hz
                                                .value
                                                .clamp(FILTER_CUTOFF_MIN_HZ, FILTER_CUTOFF_MAX_HZ);
                                        }
                                    }
                                    Some(2) => {
                                        filter.resonance_x10.input(i, FILTER_Q_MAX_X10);
                                        filter.resonance_x10.value = filter
                                            .resonance_x10
                                            .value
                                            .clamp(FILTER_Q_MIN_X10, FILTER_Q_MAX_X10);
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
                                    Some(3) => {
                                        env_cfg.sustain_pct.input(i, ENVELOPE_SUSTAIN_MAX_PCT)
                                    }
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
                                        filter.cutoff_hz.value = filter
                                            .cutoff_hz
                                            .value
                                            .clamp(FILTER_CUTOFF_MIN_HZ, FILTER_CUTOFF_MAX_HZ);
                                        if i.key_pressed(egui::Key::ArrowUp) {
                                            filter.cutoff_hz.value =
                                                ((filter.cutoff_hz.value as f32) * 1.06).round()
                                                    as usize;
                                            filter.cutoff_hz.value = filter
                                                .cutoff_hz
                                                .value
                                                .clamp(FILTER_CUTOFF_MIN_HZ, FILTER_CUTOFF_MAX_HZ);
                                        }
                                        if i.key_pressed(egui::Key::ArrowDown) {
                                            filter.cutoff_hz.value =
                                                ((filter.cutoff_hz.value as f32) / 1.06).round()
                                                    as usize;
                                            filter.cutoff_hz.value = filter
                                                .cutoff_hz
                                                .value
                                                .clamp(FILTER_CUTOFF_MIN_HZ, FILTER_CUTOFF_MAX_HZ);
                                        }
                                    }
                                    Some(2) => {
                                        filter.resonance_x10.input(i, FILTER_Q_MAX_X10);
                                        filter.resonance_x10.value = filter
                                            .resonance_x10
                                            .value
                                            .clamp(FILTER_Q_MIN_X10, FILTER_Q_MAX_X10);
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
                    ScreenState::InFxReverb => {
                        let bank_idx = self.config.input_fx.sel_bank_idx;
                        let slot_idx = self.fx_screen_slot_idx;
                        let slot = &mut self.config.input_fx.banks[bank_idx].slots[slot_idx];
                        if let Some(fx) = slot.fx.as_mut() {
                            if let Some(reverb) = fx.as_reverb_mut() {
                                if i.key_pressed(egui::Key::ArrowLeft) {
                                    reverb.prev();
                                }
                                if i.key_pressed(egui::Key::ArrowRight) {
                                    reverb.next();
                                }

                                match reverb.sel_idx {
                                    Some(0) => reverb.size.input(i, REVERB_SIZE_MAX),
                                    Some(1) => {
                                        reverb.decay_ms.input(i, REVERB_RT60_MAX_MS);
                                        reverb.decay_ms.value = reverb
                                            .decay_ms
                                            .value
                                            .clamp(REVERB_RT60_MIN_MS, REVERB_RT60_MAX_MS);
                                    }
                                    Some(2) => reverb.predelay_ms.input(i, REVERB_PREDELAY_MAX_MS),
                                    Some(3) => reverb.width.input(i, REVERB_WIDTH_MAX),
                                    Some(4) => reverb.high_cut.input(i, REVERB_HIGHCUT_MAX),
                                    Some(5) => {
                                        reverb.low_cut.input(i, REVERB_LOWCUT_MAX_HZ);
                                        reverb.low_cut.value = reverb
                                            .low_cut
                                            .value
                                            .clamp(REVERB_LOWCUT_MIN_HZ, REVERB_LOWCUT_MAX_HZ);
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    ScreenState::InFxMyDelay => {
                        let bank_idx = self.config.input_fx.sel_bank_idx;
                        let slot_idx = self.fx_screen_slot_idx;
                        let slot = &mut self.config.input_fx.banks[bank_idx].slots[slot_idx];
                        if let Some(fx) = slot.fx.as_mut() {
                            if let Some(delay) = fx.as_mydelay_mut() {
                                if i.key_pressed(egui::Key::ArrowLeft) {
                                    delay.prev();
                                }
                                if i.key_pressed(egui::Key::ArrowRight) {
                                    delay.next();
                                }

                                if i.key_pressed(egui::Key::Enter) {
                                    match delay.sel_idx {
                                        Some(0) => {
                                            delay.audio_sel_idx = Some(0);
                                            self.screen_state = ScreenState::InFxMyDelayAudio;
                                        }
                                        Some(1) => {
                                            delay.note.sel_idx = Some(0);
                                            self.screen_state = ScreenState::InFxMyDelayNote;
                                        }
                                        Some(2) => {
                                            delay.filter_sel_idx = Some(0);
                                            self.screen_state = ScreenState::InFxMyDelayFilter;
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                    ScreenState::InFxMyDelayAudio => {
                        let bank_idx = self.config.input_fx.sel_bank_idx;
                        let slot_idx = self.fx_screen_slot_idx;
                        let slot = &mut self.config.input_fx.banks[bank_idx].slots[slot_idx];
                        if let Some(fx) = slot.fx.as_mut() {
                            if let Some(delay) = fx.as_mydelay_mut() {
                                let curr = delay.audio_sel_idx.unwrap_or(0);
                                if i.key_pressed(egui::Key::ArrowLeft) {
                                    delay.audio_sel_idx = Some(curr.saturating_sub(1));
                                }
                                if i.key_pressed(egui::Key::ArrowRight) {
                                    delay.audio_sel_idx = Some((curr + 1).min(2));
                                }

                                match delay.audio_sel_idx {
                                    Some(0) => delay.level.input(i, MYDELAY_LEVEL_MAX),
                                    Some(1) => delay.threshold.input(i, MYDELAY_THRESHOLD_MAX),
                                    _ => {}
                                }

                                if i.key_pressed(egui::Key::Enter) && delay.audio_sel_idx == Some(2)
                                {
                                    delay.audio_env.sel_idx = Some(0);
                                    self.screen_state = ScreenState::InFxMyDelayAudioEnv;
                                }
                            }
                        }
                    }
                    ScreenState::InFxMyDelayAudioEnv => {
                        let bank_idx = self.config.input_fx.sel_bank_idx;
                        let slot_idx = self.fx_screen_slot_idx;
                        let slot = &mut self.config.input_fx.banks[bank_idx].slots[slot_idx];
                        if let Some(fx) = slot.fx.as_mut() {
                            if let Some(delay) = fx.as_mydelay_mut() {
                                let env_cfg = &mut delay.audio_env;
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
                                    Some(3) => {
                                        env_cfg.sustain_pct.input(i, ENVELOPE_SUSTAIN_MAX_PCT)
                                    }
                                    Some(4) => env_cfg.release_ms.input(i, ENVELOPE_RELEASE_MAX_MS),
                                    Some(5) => env_cfg.start_pct.input(i, ENVELOPE_START_MAX_PCT),
                                    Some(6) => env_cfg.tension_a.input(i, ENVELOPE_TENSION_MAX),
                                    Some(7) => env_cfg.tension_d.input(i, ENVELOPE_TENSION_MAX),
                                    Some(8) => env_cfg.tension_r.input(i, ENVELOPE_TENSION_MAX),
                                    _ => {}
                                }
                            }
                        }
                    }
                    ScreenState::InFxMyDelayNote => {
                        let bank_idx = self.config.input_fx.sel_bank_idx;
                        let slot_idx = self.fx_screen_slot_idx;
                        let slot = &mut self.config.input_fx.banks[bank_idx].slots[slot_idx];
                        if let Some(fx) = slot.fx.as_mut() {
                            if let Some(delay) = fx.as_mydelay_mut() {
                                let note_cfg = &mut delay.note;
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
                    ScreenState::InFxMyDelayFilter => {
                        let bank_idx = self.config.input_fx.sel_bank_idx;
                        let slot_idx = self.fx_screen_slot_idx;
                        let slot = &mut self.config.input_fx.banks[bank_idx].slots[slot_idx];
                        if let Some(fx) = slot.fx.as_mut() {
                            if let Some(delay) = fx.as_mydelay_mut() {
                                let filter = &mut delay.filter;
                                let curr = delay.filter_sel_idx.unwrap_or(0);
                                if i.key_pressed(egui::Key::ArrowLeft) {
                                    delay.filter_sel_idx = Some(curr.saturating_sub(1));
                                }
                                if i.key_pressed(egui::Key::ArrowRight) {
                                    delay.filter_sel_idx = Some((curr + 1).min(5));
                                }

                                match delay.filter_sel_idx {
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
                                        filter.cutoff_hz.value = filter
                                            .cutoff_hz
                                            .value
                                            .clamp(FILTER_CUTOFF_MIN_HZ, FILTER_CUTOFF_MAX_HZ);
                                        if i.key_pressed(egui::Key::ArrowUp) {
                                            filter.cutoff_hz.value =
                                                ((filter.cutoff_hz.value as f32) * 1.06).round()
                                                    as usize;
                                            filter.cutoff_hz.value = filter
                                                .cutoff_hz
                                                .value
                                                .clamp(FILTER_CUTOFF_MIN_HZ, FILTER_CUTOFF_MAX_HZ);
                                        }
                                        if i.key_pressed(egui::Key::ArrowDown) {
                                            filter.cutoff_hz.value =
                                                ((filter.cutoff_hz.value as f32) / 1.06).round()
                                                    as usize;
                                            filter.cutoff_hz.value = filter
                                                .cutoff_hz
                                                .value
                                                .clamp(FILTER_CUTOFF_MIN_HZ, FILTER_CUTOFF_MAX_HZ);
                                        }
                                    }
                                    Some(2) => {
                                        filter.resonance_x10.input(i, FILTER_Q_MAX_X10);
                                        filter.resonance_x10.value = filter
                                            .resonance_x10
                                            .value
                                            .clamp(FILTER_Q_MIN_X10, FILTER_Q_MAX_X10);
                                    }
                                    Some(3) => {
                                        filter.drive.input(i, FILTER_DRIVE_MAX);
                                    }
                                    Some(4) => {
                                        filter.mix.input(i, FILTER_MIX_MAX);
                                    }
                                    Some(5) => {
                                        if i.key_pressed(egui::Key::Enter) {
                                            delay.filter_env.sel_idx = Some(0);
                                            self.screen_state = ScreenState::InFxMyDelayFilterEnv;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    ScreenState::InFxMyDelayFilterEnv => {
                        let bank_idx = self.config.input_fx.sel_bank_idx;
                        let slot_idx = self.fx_screen_slot_idx;
                        let slot = &mut self.config.input_fx.banks[bank_idx].slots[slot_idx];
                        if let Some(fx) = slot.fx.as_mut() {
                            if let Some(delay) = fx.as_mydelay_mut() {
                                let env_cfg = &mut delay.filter_env;
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
                                    Some(3) => {
                                        env_cfg.sustain_pct.input(i, ENVELOPE_SUSTAIN_MAX_PCT)
                                    }
                                    Some(4) => env_cfg.release_ms.input(i, ENVELOPE_RELEASE_MAX_MS),
                                    Some(5) => env_cfg.start_pct.input(i, ENVELOPE_START_MAX_PCT),
                                    Some(6) => env_cfg.tension_a.input(i, ENVELOPE_TENSION_MAX),
                                    Some(7) => env_cfg.tension_d.input(i, ENVELOPE_TENSION_MAX),
                                    Some(8) => env_cfg.tension_r.input(i, ENVELOPE_TENSION_MAX),
                                    _ => {}
                                }
                            }
                        }
                    }
                    ScreenState::InFxVocoder => {
                        let bank_idx = self.config.input_fx.sel_bank_idx;
                        let slot_idx = self.fx_screen_slot_idx;
                        let slot = &mut self.config.input_fx.banks[bank_idx].slots[slot_idx];
                        if let Some(fx) = slot.fx.as_mut() {
                            if let Some(vocoder) = fx.as_vocoder_mut() {
                                if i.key_pressed(egui::Key::ArrowLeft) {
                                    vocoder.prev();
                                }
                                if i.key_pressed(egui::Key::ArrowRight) {
                                    vocoder.next();
                                }

                                match vocoder.sel_idx {
                                    Some(0) => {
                                        if i.key_pressed(egui::Key::ArrowUp) {
                                            vocoder.carrier.prev();
                                        }
                                        if i.key_pressed(egui::Key::ArrowDown) {
                                            vocoder.carrier.next();
                                        }
                                    }
                                    Some(1) => {
                                        vocoder.bands.input(i, VOCODER_BANDS_MAX);
                                        vocoder.bands.value = vocoder
                                            .bands
                                            .value
                                            .clamp(VOCODER_BANDS_MIN, VOCODER_BANDS_MAX);
                                    }
                                    Some(2) => vocoder.attack_ms.input(i, VOCODER_ATTACK_MAX_MS),
                                    Some(3) => vocoder.release_ms.input(i, VOCODER_RELEASE_MAX_MS),
                                    Some(4) => vocoder.level.input(i, VOCODER_LEVEL_MAX),
                                    Some(5) => vocoder.mix.input(i, VOCODER_MIX_MAX),
                                    _ => {}
                                }
                            }
                        }
                    }
                    ScreenState::TrackFxSelect => {
                        let slot_keys = [egui::Key::U, egui::Key::I, egui::Key::O, egui::Key::P];
                        for (slot_idx, key) in slot_keys.iter().enumerate() {
                            if i.key_pressed(*key) {
                                match self.fx_state {
                                    FxState::Bank => self.config.track_fx.select_bank(slot_idx),
                                    FxState::Single => {
                                        self.track_fx_screen_slot_idx = slot_idx;
                                        self.track_fx_edit_row_idx = 0;
                                    }
                                }
                            }
                        }

                        let bank_idx = self.config.track_fx.sel_bank_idx;
                        let slot_idx = self.track_fx_screen_slot_idx;
                        if self.fx_state == FxState::Single {
                            if i.key_pressed(egui::Key::ArrowLeft) {
                                self.config.track_fx.cycle_slot_kind(bank_idx, slot_idx, -1);
                            }
                            if i.key_pressed(egui::Key::ArrowRight) {
                                self.config.track_fx.cycle_slot_kind(bank_idx, slot_idx, 1);
                            }

                            if i.key_pressed(egui::Key::Enter) {
                                self.track_fx_edit_row_idx = 0;
                                match self.config.track_fx.slot_kind(bank_idx, slot_idx) {
                                    TrackFxKind::Delay => {
                                        self.screen_state = ScreenState::InTrackFxDelay
                                    }
                                    TrackFxKind::Roll => {
                                        self.screen_state = ScreenState::InTrackFxRoll
                                    }
                                    TrackFxKind::Filter => {
                                        if let Some(TrackFx::Filter(filter)) =
                                            self.config.track_fx.slot_fx_mut(bank_idx, slot_idx)
                                        {
                                            filter.sel_idx = Some(0);
                                        }
                                        self.screen_state = ScreenState::InTrackFxFilter;
                                    }
                                    TrackFxKind::None => {}
                                }
                            }
                        }
                    }
                    ScreenState::InTrackFxDelay => {
                        if i.key_pressed(egui::Key::ArrowLeft) {
                            self.track_fx_edit_row_idx =
                                self.track_fx_edit_row_idx.saturating_sub(1);
                        }
                        if i.key_pressed(egui::Key::ArrowRight) {
                            self.track_fx_edit_row_idx = (self.track_fx_edit_row_idx + 1).min(3);
                        }

                        let bank_idx = self.config.track_fx.sel_bank_idx;
                        let slot_idx = self.track_fx_screen_slot_idx;
                        if let Some(TrackFx::Delay(delay)) =
                            self.config.track_fx.slot_fx_mut(bank_idx, slot_idx)
                        {
                            match self.track_fx_edit_row_idx {
                                0 => {
                                    delay.time_ms.input(i, TRACK_DELAY_TIME_MAX_MS);
                                    delay.time_ms.value = delay
                                        .time_ms
                                        .value
                                        .clamp(TRACK_DELAY_TIME_MIN_MS, TRACK_DELAY_TIME_MAX_MS);
                                }
                                1 => {
                                    delay.feedback_pct.input(i, TRACK_DELAY_FEEDBACK_MAX_PCT);
                                    delay.feedback_pct.value =
                                        delay.feedback_pct.value.min(TRACK_DELAY_FEEDBACK_MAX_PCT);
                                }
                                2 => {
                                    delay.high_damp_hz.input(i, TRACK_DELAY_DAMP_MAX_HZ);
                                    delay.high_damp_hz.value = delay
                                        .high_damp_hz
                                        .value
                                        .clamp(TRACK_DELAY_DAMP_MIN_HZ, TRACK_DELAY_DAMP_MAX_HZ);
                                }
                                3 => {
                                    delay.mix_pct.input(i, TRACK_DELAY_MIX_MAX_PCT);
                                    delay.mix_pct.value =
                                        delay.mix_pct.value.min(TRACK_DELAY_MIX_MAX_PCT);
                                }
                                _ => {}
                            }
                        }
                    }
                    ScreenState::InTrackFxRoll => {
                        let bank_idx = self.config.track_fx.sel_bank_idx;
                        let slot_idx = self.track_fx_screen_slot_idx;
                        if let Some(TrackFx::Roll(roll)) =
                            self.config.track_fx.slot_fx_mut(bank_idx, slot_idx)
                        {
                            if i.key_pressed(egui::Key::ArrowUp) {
                                roll.step.prev();
                            }
                            if i.key_pressed(egui::Key::ArrowDown) {
                                roll.step.next();
                            }
                        }
                    }
                    ScreenState::InTrackFxFilter => {
                        let bank_idx = self.config.track_fx.sel_bank_idx;
                        let slot_idx = self.track_fx_screen_slot_idx;
                        if let Some(TrackFx::Filter(filter_cfg)) =
                            self.config.track_fx.slot_fx_mut(bank_idx, slot_idx)
                        {
                            if i.key_pressed(egui::Key::ArrowLeft) {
                                filter_cfg.prev();
                            }
                            if i.key_pressed(egui::Key::ArrowRight) {
                                filter_cfg.next();
                            }

                            match filter_cfg.sel_idx {
                                Some(0) => {
                                    if i.key_pressed(egui::Key::ArrowUp) {
                                        filter_cfg.filter.filter_type.prev();
                                    }
                                    if i.key_pressed(egui::Key::ArrowDown) {
                                        filter_cfg.filter.filter_type.next();
                                    }
                                }
                                Some(1) => {
                                    filter_cfg.filter.cutoff_hz.input(i, FILTER_CUTOFF_MAX_HZ);
                                    filter_cfg.filter.cutoff_hz.value = filter_cfg
                                        .filter
                                        .cutoff_hz
                                        .value
                                        .clamp(FILTER_CUTOFF_MIN_HZ, FILTER_CUTOFF_MAX_HZ);
                                    if i.key_pressed(egui::Key::ArrowUp) {
                                        filter_cfg.filter.cutoff_hz.value =
                                            ((filter_cfg.filter.cutoff_hz.value as f32) * 1.06)
                                                .round()
                                                as usize;
                                        filter_cfg.filter.cutoff_hz.value = filter_cfg
                                            .filter
                                            .cutoff_hz
                                            .value
                                            .clamp(FILTER_CUTOFF_MIN_HZ, FILTER_CUTOFF_MAX_HZ);
                                    }
                                    if i.key_pressed(egui::Key::ArrowDown) {
                                        filter_cfg.filter.cutoff_hz.value =
                                            ((filter_cfg.filter.cutoff_hz.value as f32) / 1.06)
                                                .round()
                                                as usize;
                                        filter_cfg.filter.cutoff_hz.value = filter_cfg
                                            .filter
                                            .cutoff_hz
                                            .value
                                            .clamp(FILTER_CUTOFF_MIN_HZ, FILTER_CUTOFF_MAX_HZ);
                                    }
                                }
                                Some(2) => {
                                    filter_cfg.filter.resonance_x10.input(i, FILTER_Q_MAX_X10);
                                    filter_cfg.filter.resonance_x10.value = filter_cfg
                                        .filter
                                        .resonance_x10
                                        .value
                                        .clamp(FILTER_Q_MIN_X10, FILTER_Q_MAX_X10);
                                }
                                Some(3) => {
                                    filter_cfg.filter.drive.input(i, FILTER_DRIVE_MAX);
                                    filter_cfg.filter.drive.value =
                                        filter_cfg.filter.drive.value.min(FILTER_DRIVE_MAX);
                                }
                                Some(4) => {
                                    filter_cfg.filter.mix.input(i, FILTER_MIX_MAX);
                                    filter_cfg.filter.mix.value =
                                        filter_cfg.filter.mix.value.min(FILTER_MIX_MAX);
                                }
                                Some(5) => {
                                    if i.key_pressed(egui::Key::Enter) {
                                        filter_cfg.seq.sel_idx = Some(0);
                                        self.screen_state = ScreenState::InTrackFxFilterSeq;
                                    }
                                }
                                Some(6) => {
                                    if i.key_pressed(egui::Key::Enter) {
                                        filter_cfg.env.sel_idx = Some(0);
                                        self.screen_state = ScreenState::InTrackFxFilterEnv;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    ScreenState::InTrackFxFilterSeq => {
                        let bank_idx = self.config.track_fx.sel_bank_idx;
                        let slot_idx = self.track_fx_screen_slot_idx;
                        if let Some(TrackFx::Filter(filter_cfg)) =
                            self.config.track_fx.slot_fx_mut(bank_idx, slot_idx)
                        {
                            let seq_cfg = &mut filter_cfg.seq;
                            if i.key_pressed(egui::Key::ArrowLeft) {
                                seq_cfg.prev();
                            }
                            if i.key_pressed(egui::Key::ArrowRight) {
                                seq_cfg.next();
                            }
                            match seq_cfg.sel_idx {
                                Some(0) => {
                                    if i.key_pressed(egui::Key::ArrowUp) {
                                        seq_cfg.step.prev();
                                    }
                                    if i.key_pressed(egui::Key::ArrowDown) {
                                        seq_cfg.step.next();
                                    }
                                }
                                Some(1) => {
                                    if i.key_pressed(egui::Key::ArrowUp) {
                                        seq_cfg.edit.prev();
                                    }
                                    if i.key_pressed(egui::Key::ArrowDown) {
                                        seq_cfg.edit.next();
                                    }
                                    if i.key_pressed(egui::Key::Enter) {
                                        seq_cfg.apply_edit();
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    ScreenState::InTrackFxFilterEnv => {
                        let bank_idx = self.config.track_fx.sel_bank_idx;
                        let slot_idx = self.track_fx_screen_slot_idx;
                        if let Some(TrackFx::Filter(filter_cfg)) =
                            self.config.track_fx.slot_fx_mut(bank_idx, slot_idx)
                        {
                            let env_cfg = &mut filter_cfg.env;
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
                                Some(4) => env_cfg.release_ms.input(i, ENVELOPE_RELEASE_MAX_MS),
                                Some(5) => env_cfg.start_pct.input(i, ENVELOPE_START_MAX_PCT),
                                Some(6) => env_cfg.tension_a.input(i, ENVELOPE_TENSION_MAX),
                                Some(7) => env_cfg.tension_d.input(i, ENVELOPE_TENSION_MAX),
                                Some(8) => env_cfg.tension_r.input(i, ENVELOPE_TENSION_MAX),
                                _ => {}
                            }
                        }
                    }
                    ScreenState::Empty => {}
                }
            }
        });
    }
}

// Toggles must react once per physical press, never to OS autorepeat.
fn pressed(input: &egui::InputState, key: egui::Key) -> bool {
    input.events.iter().any(|event| {
        matches!(event,egui::Event::Key {
        key: event_key, pressed:true, repeat:false, ..
    } if *event_key==key)
    })
}
