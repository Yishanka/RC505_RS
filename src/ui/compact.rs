use crate::app::MyApp;
use crate::config::{FxKind, TrackFxKind};
use crate::state::{AppState, ScreenState};
use eframe::egui;
const SCREEN_WIDTH: f32 = 500.0;
const SCREEN_HEIGHT: f32 = 150.0;
const SCREEN_ROUNDING: f32 = 10.0;
const SCREEN_STROKE_WIDTH: f32 = 2.0;
const SYS_VALUE_FONT_MAX: f32 = 28.0;
const SYS_VALUE_FONT_MIN: f32 = 10.0;
pub fn draw_screen(ui: &mut egui::Ui, app: &mut MyApp) {
    let border_color = if app.app_state == AppState::MainScreen {
        egui::Color32::RED
    } else {
        egui::Color32::DARK_GRAY
    };

    egui::Frame::none()
        .stroke(egui::Stroke::new(SCREEN_STROKE_WIDTH, border_color))
        .inner_margin(5.0)
        .rounding(SCREEN_ROUNDING)
        .show(ui, |ui| {
            ui.set_width(SCREEN_WIDTH);
            ui.set_height(SCREEN_HEIGHT);
            ui.set_min_size(egui::vec2(SCREEN_WIDTH, SCREEN_HEIGHT));

            ui.vertical(|ui| {
                if let Some(text) = screen_breadcrumb(app) {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(text)
                                .size(14.0)
                                .color(egui::Color32::from_rgb(160, 160, 160)),
                        );
                    });
                }

                match app.screen_state {
                    ScreenState::Empty => {
                        ui.centered_and_justified(|ui| {
                            ui.label(
                                egui::RichText::new(&app.projects[app.sel_project_idx].name)
                                    .size(48.0)
                                    .color(egui::Color32::WHITE),
                            );
                        });
                    }
                    ScreenState::Beat => {
                        let beat_settings = &app.config.beat_config;
                        let selected_idx = beat_settings.sel_idx.unwrap_or(0);
                        ui.horizontal_centered(|ui| {
                            ui.add_space(20.0);
                            for idx in page_indices(2, selected_idx) {
                                match idx {
                                    Some(0) => draw_setting_option_block(
                                        ui,
                                        &format!("{}", beat_settings.input_bpm.value),
                                        &beat_settings.input_bpm.label,
                                        beat_settings.sel_idx == Some(0),
                                    ),
                                    Some(1) => draw_setting_option_block(
                                        ui,
                                        &format!("{}", beat_settings.input_latency.value),
                                        &beat_settings.input_latency.label,
                                        beat_settings.sel_idx == Some(1),
                                    ),
                                    _ => draw_empty_block(ui),
                                }
                            }
                        });
                    }
                    ScreenState::SYS => {
                        let sys_config: &crate::config::SystemConfigs = &app.config.system_config;
                        let selected_idx = sys_config.sel_idx.unwrap_or(0);
                        ui.horizontal_centered(|ui| {
                            ui.add_space(20.0);
                            for idx in page_indices(2, selected_idx) {
                                match idx {
                                    Some(0) => draw_sys_setting_option_block(
                                        ui,
                                        &sys_config.input_device.value,
                                        &sys_config.input_device.label,
                                        sys_config.sel_idx == Some(0),
                                    ),
                                    Some(1) => draw_sys_setting_option_block(
                                        ui,
                                        &sys_config.output_device.value,
                                        &sys_config.output_device.label,
                                        sys_config.sel_idx == Some(1),
                                    ),
                                    _ => draw_empty_block(ui),
                                }
                            }
                        });
                    }
                    ScreenState::FxSelect => {
                        let bank_idx = app.config.input_fx.sel_bank_idx;
                        let slot_idx = app.fx_screen_slot_idx;
                        let selected = app.config.input_fx.slot_kind(bank_idx, slot_idx);
                        let selected_idx = match selected {
                            FxKind::Oscillator => 0,
                            FxKind::Filter => 1,
                            FxKind::Reverb => 2,
                            FxKind::MyDelay => 3,
                            FxKind::Vocoder => 4,
                            FxKind::None => 0,
                        };
                        ui.horizontal_centered(|ui| {
                            ui.add_space(20.0);
                            for idx in page_indices(5, selected_idx) {
                                match idx {
                                    Some(0) => draw_fx_choice_block(
                                        ui,
                                        "Oscillator",
                                        selected == FxKind::Oscillator,
                                    ),
                                    Some(1) => draw_fx_choice_block(
                                        ui,
                                        "Filter",
                                        selected == FxKind::Filter,
                                    ),
                                    Some(2) => draw_fx_choice_block(
                                        ui,
                                        "Reverb",
                                        selected == FxKind::Reverb,
                                    ),
                                    Some(3) => draw_fx_choice_block(
                                        ui,
                                        "MyDelay",
                                        selected == FxKind::MyDelay,
                                    ),
                                    Some(4) => draw_fx_choice_block(
                                        ui,
                                        "Vocoder",
                                        selected == FxKind::Vocoder,
                                    ),
                                    _ => draw_empty_block(ui),
                                }
                            }
                        });
                        draw_page_indicator(ui, 5, selected_idx);
                    }
                    ScreenState::TrackFxSelect => {
                        let bank_idx = app.config.track_fx.sel_bank_idx;
                        let slot_idx = app.track_fx_screen_slot_idx;
                        let selected = app.config.track_fx.slot_kind(bank_idx, slot_idx);
                        let selected_idx = match selected {
                            TrackFxKind::Delay => 0,
                            TrackFxKind::Roll => 1,
                            TrackFxKind::Filter => 2,
                            TrackFxKind::None => 0,
                        };
                        ui.horizontal_centered(|ui| {
                            ui.add_space(20.0);
                            for idx in page_indices(3, selected_idx) {
                                match idx {
                                    Some(0) => draw_fx_choice_block(
                                        ui,
                                        "Delay",
                                        selected == TrackFxKind::Delay,
                                    ),
                                    Some(1) => draw_fx_choice_block(
                                        ui,
                                        "Roll",
                                        selected == TrackFxKind::Roll,
                                    ),
                                    Some(2) => draw_fx_choice_block(
                                        ui,
                                        "Filter",
                                        selected == TrackFxKind::Filter,
                                    ),
                                    _ => draw_empty_block(ui),
                                }
                            }
                        });
                    }
                    ScreenState::InTrackFxDelay => {
                        let bank_idx = app.config.track_fx.sel_bank_idx;
                        let slot_idx = app.track_fx_screen_slot_idx;
                        if let Some(crate::config::TrackFx::Delay(delay)) =
                            app.config.track_fx.slot_fx(bank_idx, slot_idx)
                        {
                            let selected_idx = app.track_fx_edit_row_idx;
                            ui.horizontal_centered(|ui| {
                                ui.add_space(20.0);
                                for idx in page_indices(4, selected_idx) {
                                    match idx {
                                        Some(0) => draw_setting_option_block(
                                            ui,
                                            &format!("{}", delay.time_ms.value),
                                            &delay.time_ms.label,
                                            selected_idx == 0,
                                        ),
                                        Some(1) => draw_setting_option_block(
                                            ui,
                                            &format!("{}", delay.feedback_pct.value),
                                            &delay.feedback_pct.label,
                                            selected_idx == 1,
                                        ),
                                        Some(2) => draw_setting_option_block(
                                            ui,
                                            &format!("{}", delay.high_damp_hz.value),
                                            &delay.high_damp_hz.label,
                                            selected_idx == 2,
                                        ),
                                        Some(3) => draw_setting_option_block(
                                            ui,
                                            &format!("{}", delay.mix_pct.value),
                                            &delay.mix_pct.label,
                                            selected_idx == 3,
                                        ),
                                        _ => draw_empty_block(ui),
                                    }
                                }
                            });
                        }
                    }
                    ScreenState::InTrackFxRoll => {
                        let bank_idx = app.config.track_fx.sel_bank_idx;
                        let slot_idx = app.track_fx_screen_slot_idx;
                        if let Some(crate::config::TrackFx::Roll(roll)) =
                            app.config.track_fx.slot_fx(bank_idx, slot_idx)
                        {
                            ui.horizontal_centered(|ui| {
                                ui.add_space(20.0);
                                draw_setting_option_block(
                                    ui,
                                    &format!("{}", roll.step.value),
                                    &roll.step.label,
                                    true,
                                );
                                draw_empty_block(ui);
                                draw_empty_block(ui);
                                draw_empty_block(ui);
                            });
                        }
                    }
                    ScreenState::InTrackFxFilter => {
                        let bank_idx = app.config.track_fx.sel_bank_idx;
                        let slot_idx = app.track_fx_screen_slot_idx;
                        if let Some(crate::config::TrackFx::Filter(filter_cfg)) =
                            app.config.track_fx.slot_fx(bank_idx, slot_idx)
                        {
                            let selected_idx = filter_cfg.sel_idx.unwrap_or(0);
                            ui.horizontal_centered(|ui| {
                                ui.add_space(20.0);
                                for idx in page_indices(6, selected_idx) {
                                    match idx {
                                        Some(0) => draw_setting_option_block(
                                            ui,
                                            &format!("{}", filter_cfg.filter.filter_type.value),
                                            &filter_cfg.filter.filter_type.label,
                                            selected_idx == 0,
                                        ),
                                        Some(1) => draw_setting_option_block(
                                            ui,
                                            &format!("{}", filter_cfg.filter.cutoff_hz.value),
                                            &filter_cfg.filter.cutoff_hz.label,
                                            selected_idx == 1,
                                        ),
                                        Some(2) => draw_setting_option_block(
                                            ui,
                                            &format!(
                                                "{:.1}",
                                                filter_cfg.filter.resonance_x10.value as f32 / 10.0
                                            ),
                                            &filter_cfg.filter.resonance_x10.label,
                                            selected_idx == 2,
                                        ),
                                        Some(3) => draw_setting_option_block(
                                            ui,
                                            &format!("{}", filter_cfg.filter.drive.value),
                                            &filter_cfg.filter.drive.label,
                                            selected_idx == 3,
                                        ),
                                        Some(4) => draw_setting_option_block(
                                            ui,
                                            &format!("{}", filter_cfg.filter.mix.value),
                                            &filter_cfg.filter.mix.label,
                                            selected_idx == 4,
                                        ),
                                        Some(5) => draw_setting_option_block(
                                            ui,
                                            &format!("{}", filter_cfg.seq.seq().len()),
                                            "Seq",
                                            selected_idx == 5,
                                        ),
                                        Some(6) => draw_setting_option_block(
                                            ui,
                                            "Env",
                                            "Envelope",
                                            selected_idx == 6,
                                        ),
                                        _ => draw_empty_block(ui),
                                    }
                                }
                            });
                            draw_page_indicator(ui, 7, selected_idx);
                        }
                    }
                    ScreenState::InTrackFxFilterSeq => {
                        let bank_idx = app.config.track_fx.sel_bank_idx;
                        let slot_idx = app.track_fx_screen_slot_idx;
                        if let Some(crate::config::TrackFx::Filter(filter_cfg)) =
                            app.config.track_fx.slot_fx(bank_idx, slot_idx)
                        {
                            let selected_idx = filter_cfg.seq.sel_idx.unwrap_or(0);
                            ui.horizontal_centered(|ui| {
                                ui.add_space(20.0);
                                for idx in page_indices(2, selected_idx) {
                                    match idx {
                                        Some(0) => draw_setting_option_block(
                                            ui,
                                            &filter_cfg.seq.step.value,
                                            &filter_cfg.seq.step.label,
                                            selected_idx == 0,
                                        ),
                                        Some(1) => draw_setting_option_block(
                                            ui,
                                            &format!("{}", filter_cfg.seq.edit.value),
                                            &filter_cfg.seq.edit.label,
                                            selected_idx == 1,
                                        ),
                                        _ => draw_empty_block(ui),
                                    }
                                }
                            });
                        }
                    }
                    ScreenState::InTrackFxFilterEnv => {
                        let bank_idx = app.config.track_fx.sel_bank_idx;
                        let slot_idx = app.track_fx_screen_slot_idx;
                        if let Some(crate::config::TrackFx::Filter(filter_cfg)) =
                            app.config.track_fx.slot_fx(bank_idx, slot_idx)
                        {
                            let env_cfg = &filter_cfg.env;
                            let selected_idx = env_cfg.sel_idx.unwrap_or(0);
                            ui.horizontal_centered(|ui| {
                                ui.add_space(20.0);
                                for idx in page_indices(9, selected_idx) {
                                    match idx {
                                        Some(0) => draw_setting_option_block(
                                            ui,
                                            &format!("{}", env_cfg.attack_ms.value),
                                            &env_cfg.attack_ms.label,
                                            env_cfg.sel_idx == Some(0),
                                        ),
                                        Some(1) => draw_setting_option_block(
                                            ui,
                                            &format!("{}", env_cfg.hold_ms.value),
                                            &env_cfg.hold_ms.label,
                                            env_cfg.sel_idx == Some(1),
                                        ),
                                        Some(2) => draw_setting_option_block(
                                            ui,
                                            &format!("{}", env_cfg.decay_ms.value),
                                            &env_cfg.decay_ms.label,
                                            env_cfg.sel_idx == Some(2),
                                        ),
                                        Some(3) => draw_setting_option_block(
                                            ui,
                                            &format!("{}", env_cfg.sustain_pct.value),
                                            &env_cfg.sustain_pct.label,
                                            env_cfg.sel_idx == Some(3),
                                        ),
                                        Some(4) => draw_setting_option_block(
                                            ui,
                                            &format!("{}", env_cfg.release_ms.value),
                                            &env_cfg.release_ms.label,
                                            env_cfg.sel_idx == Some(4),
                                        ),
                                        Some(5) => draw_setting_option_block(
                                            ui,
                                            &format!("{}", env_cfg.start_pct.value),
                                            &env_cfg.start_pct.label,
                                            env_cfg.sel_idx == Some(5),
                                        ),
                                        Some(6) => draw_setting_option_block(
                                            ui,
                                            &format!("{}", env_cfg.tension_a.value),
                                            &env_cfg.tension_a.label,
                                            env_cfg.sel_idx == Some(6),
                                        ),
                                        Some(7) => draw_setting_option_block(
                                            ui,
                                            &format!("{}", env_cfg.tension_d.value),
                                            &env_cfg.tension_d.label,
                                            env_cfg.sel_idx == Some(7),
                                        ),
                                        Some(8) => draw_setting_option_block(
                                            ui,
                                            &format!("{}", env_cfg.tension_r.value),
                                            &env_cfg.tension_r.label,
                                            env_cfg.sel_idx == Some(8),
                                        ),
                                        _ => draw_empty_block(ui),
                                    }
                                }
                            });
                            draw_page_indicator(ui, 9, selected_idx);
                        }
                    }
                    ScreenState::InFxOsc => {
                        let bank_idx = app.config.input_fx.sel_bank_idx;
                        let slot_idx = app.fx_screen_slot_idx;
                        let slot = &app.config.input_fx.banks[bank_idx].slots[slot_idx];
                        if let Some(fx) = slot.fx.as_ref() {
                            match fx {
                                crate::config::InputFx::Oscillator(osc) => {
                                    let selected_idx = osc.sel_idx.unwrap_or(0);
                                    ui.horizontal_centered(|ui| {
                                        ui.add_space(20.0);
                                        for idx in page_indices(3, selected_idx) {
                                            match idx {
                                                Some(0) => draw_setting_option_block(
                                                    ui,
                                                    "Audi",
                                                    "Audi",
                                                    osc.sel_idx == Some(0),
                                                ),
                                                Some(1) => draw_setting_option_block(
                                                    ui,
                                                    "Note",
                                                    "Note",
                                                    osc.sel_idx == Some(1),
                                                ),
                                                Some(2) => draw_setting_option_block(
                                                    ui,
                                                    "Filter",
                                                    "Filter",
                                                    osc.sel_idx == Some(2),
                                                ),
                                                _ => draw_empty_block(ui),
                                            }
                                        }
                                    });
                                }
                                _ => {}
                            }
                        }
                    }
                    ScreenState::InFxOscAudio => {
                        let bank_idx = app.config.input_fx.sel_bank_idx;
                        let slot_idx = app.fx_screen_slot_idx;
                        let slot = &app.config.input_fx.banks[bank_idx].slots[slot_idx];
                        if let Some(fx) = slot.fx.as_ref() {
                            match fx {
                                crate::config::InputFx::Oscillator(osc) => {
                                    let selected_idx = osc.audio_sel_idx.unwrap_or(0);
                                    ui.horizontal_centered(|ui| {
                                        ui.add_space(20.0);
                                        for idx in page_indices(4, selected_idx) {
                                            match idx {
                                                Some(0) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", osc.waveform.value),
                                                    &osc.waveform.label,
                                                    osc.audio_sel_idx == Some(0),
                                                ),
                                                Some(1) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", osc.level.value),
                                                    &osc.level.label,
                                                    osc.audio_sel_idx == Some(1),
                                                ),
                                                Some(2) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", osc.threshold.value),
                                                    &osc.threshold.label,
                                                    osc.audio_sel_idx == Some(2),
                                                ),
                                                Some(3) => draw_setting_option_block(
                                                    ui,
                                                    "Env",
                                                    "Envelope",
                                                    osc.audio_sel_idx == Some(3),
                                                ),
                                                _ => draw_empty_block(ui),
                                            }
                                        }
                                    });
                                }
                                _ => {}
                            }
                        }
                    }
                    ScreenState::InFxNote => {
                        let bank_idx = app.config.input_fx.sel_bank_idx;
                        let slot_idx = app.fx_screen_slot_idx;
                        let slot = &app.config.input_fx.banks[bank_idx].slots[slot_idx];
                        if let Some(fx) = slot.fx.as_ref() {
                            match fx {
                                crate::config::InputFx::Oscillator(osc) => {
                                    let note_cfg = &osc.note;
                                    let selected_idx = note_cfg.sel_idx.unwrap_or(0);
                                    ui.horizontal_centered(|ui| {
                                        ui.add_space(20.0);
                                        for idx in page_indices(4, selected_idx) {
                                            match idx {
                                                Some(0) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", note_cfg.note.value),
                                                    &note_cfg.note.label,
                                                    note_cfg.sel_idx == Some(0),
                                                ),
                                                Some(1) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", note_cfg.octave.value),
                                                    &note_cfg.octave.label,
                                                    note_cfg.sel_idx == Some(1),
                                                ),
                                                Some(2) => draw_setting_option_block(
                                                    ui,
                                                    &note_cfg.step.value,
                                                    &note_cfg.step.label,
                                                    note_cfg.sel_idx == Some(2),
                                                ),
                                                Some(3) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", note_cfg.edit.value),
                                                    &note_cfg.edit.label,
                                                    note_cfg.sel_idx == Some(3),
                                                ),
                                                _ => draw_empty_block(ui),
                                            }
                                        }
                                    });
                                }
                                _ => {}
                            }
                        }
                    }
                    ScreenState::InFxOscAudioEnv => {
                        let bank_idx = app.config.input_fx.sel_bank_idx;
                        let slot_idx = app.fx_screen_slot_idx;
                        let slot = &app.config.input_fx.banks[bank_idx].slots[slot_idx];
                        if let Some(fx) = slot.fx.as_ref() {
                            match fx {
                                crate::config::InputFx::Oscillator(osc) => {
                                    let env_cfg = &osc.envelope;
                                    let selected_idx = env_cfg.sel_idx.unwrap_or(0);
                                    ui.horizontal_centered(|ui| {
                                        ui.add_space(20.0);
                                        for idx in page_indices(9, selected_idx) {
                                            match idx {
                                                Some(0) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.attack_ms.value),
                                                    &env_cfg.attack_ms.label,
                                                    env_cfg.sel_idx == Some(0),
                                                ),
                                                Some(1) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.hold_ms.value),
                                                    &env_cfg.hold_ms.label,
                                                    env_cfg.sel_idx == Some(1),
                                                ),
                                                Some(2) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.decay_ms.value),
                                                    &env_cfg.decay_ms.label,
                                                    env_cfg.sel_idx == Some(2),
                                                ),
                                                Some(3) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.sustain_pct.value),
                                                    &env_cfg.sustain_pct.label,
                                                    env_cfg.sel_idx == Some(3),
                                                ),
                                                Some(4) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.release_ms.value),
                                                    &env_cfg.release_ms.label,
                                                    env_cfg.sel_idx == Some(4),
                                                ),
                                                Some(5) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.start_pct.value),
                                                    &env_cfg.start_pct.label,
                                                    env_cfg.sel_idx == Some(5),
                                                ),
                                                Some(6) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.tension_a.value),
                                                    &env_cfg.tension_a.label,
                                                    env_cfg.sel_idx == Some(6),
                                                ),
                                                Some(7) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.tension_d.value),
                                                    &env_cfg.tension_d.label,
                                                    env_cfg.sel_idx == Some(7),
                                                ),
                                                Some(8) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.tension_r.value),
                                                    &env_cfg.tension_r.label,
                                                    env_cfg.sel_idx == Some(8),
                                                ),
                                                _ => draw_empty_block(ui),
                                            }
                                        }
                                    });
                                    draw_page_indicator(ui, 9, selected_idx);
                                }
                                _ => {}
                            }
                        }
                    }
                    ScreenState::InFxOscFilter => {
                        let bank_idx = app.config.input_fx.sel_bank_idx;
                        let slot_idx = app.fx_screen_slot_idx;
                        let slot = &app.config.input_fx.banks[bank_idx].slots[slot_idx];
                        if let Some(fx) = slot.fx.as_ref() {
                            match fx {
                                crate::config::InputFx::Oscillator(osc) => {
                                    let filter = &osc.osc_filter;
                                    let selected_idx = osc.osc_filter_sel_idx.unwrap_or(0);
                                    ui.horizontal_centered(|ui| {
                                        ui.add_space(20.0);
                                        for idx in page_indices(6, selected_idx) {
                                            match idx {
                                                Some(0) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", filter.filter_type.value),
                                                    &filter.filter_type.label,
                                                    osc.osc_filter_sel_idx == Some(0),
                                                ),
                                                Some(1) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", filter.cutoff_hz.value),
                                                    &filter.cutoff_hz.label,
                                                    osc.osc_filter_sel_idx == Some(1),
                                                ),
                                                Some(2) => draw_setting_option_block(
                                                    ui,
                                                    &format!(
                                                        "{:.1}",
                                                        filter.resonance_x10.value as f32 / 10.0
                                                    ),
                                                    "Resonance(Q)",
                                                    osc.osc_filter_sel_idx == Some(2),
                                                ),
                                                Some(3) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", filter.drive.value),
                                                    &filter.drive.label,
                                                    osc.osc_filter_sel_idx == Some(3),
                                                ),
                                                Some(4) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", filter.mix.value),
                                                    &filter.mix.label,
                                                    osc.osc_filter_sel_idx == Some(4),
                                                ),
                                                Some(5) => draw_setting_option_block(
                                                    ui,
                                                    "Env",
                                                    "Envelope",
                                                    osc.osc_filter_sel_idx == Some(5),
                                                ),
                                                _ => draw_empty_block(ui),
                                            }
                                        }
                                    });
                                    draw_page_indicator(ui, 6, selected_idx);
                                }
                                _ => {}
                            }
                        }
                    }
                    ScreenState::InFxOscFilterEnv => {
                        let bank_idx = app.config.input_fx.sel_bank_idx;
                        let slot_idx = app.fx_screen_slot_idx;
                        let slot = &app.config.input_fx.banks[bank_idx].slots[slot_idx];
                        if let Some(fx) = slot.fx.as_ref() {
                            match fx {
                                crate::config::InputFx::Oscillator(osc) => {
                                    let env_cfg = &osc.osc_filter_env;
                                    let selected_idx = env_cfg.sel_idx.unwrap_or(0);
                                    ui.horizontal_centered(|ui| {
                                        ui.add_space(20.0);
                                        for idx in page_indices(9, selected_idx) {
                                            match idx {
                                                Some(0) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.attack_ms.value),
                                                    &env_cfg.attack_ms.label,
                                                    env_cfg.sel_idx == Some(0),
                                                ),
                                                Some(1) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.hold_ms.value),
                                                    &env_cfg.hold_ms.label,
                                                    env_cfg.sel_idx == Some(1),
                                                ),
                                                Some(2) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.decay_ms.value),
                                                    &env_cfg.decay_ms.label,
                                                    env_cfg.sel_idx == Some(2),
                                                ),
                                                Some(3) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.sustain_pct.value),
                                                    &env_cfg.sustain_pct.label,
                                                    env_cfg.sel_idx == Some(3),
                                                ),
                                                Some(4) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.release_ms.value),
                                                    &env_cfg.release_ms.label,
                                                    env_cfg.sel_idx == Some(4),
                                                ),
                                                Some(5) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.start_pct.value),
                                                    &env_cfg.start_pct.label,
                                                    env_cfg.sel_idx == Some(5),
                                                ),
                                                Some(6) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.tension_a.value),
                                                    &env_cfg.tension_a.label,
                                                    env_cfg.sel_idx == Some(6),
                                                ),
                                                Some(7) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.tension_d.value),
                                                    &env_cfg.tension_d.label,
                                                    env_cfg.sel_idx == Some(7),
                                                ),
                                                Some(8) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.tension_r.value),
                                                    &env_cfg.tension_r.label,
                                                    env_cfg.sel_idx == Some(8),
                                                ),
                                                _ => draw_empty_block(ui),
                                            }
                                        }
                                    });
                                    draw_page_indicator(ui, 9, selected_idx);
                                }
                                _ => {}
                            }
                        }
                    }
                    ScreenState::InFxFilter => {
                        let bank_idx = app.config.input_fx.sel_bank_idx;
                        let slot_idx = app.fx_screen_slot_idx;
                        let slot = &app.config.input_fx.banks[bank_idx].slots[slot_idx];
                        if let Some(fx) = slot.fx.as_ref() {
                            match fx {
                                crate::config::InputFx::Filter(filter) => {
                                    let selected_idx = filter.sel_idx.unwrap_or(0);
                                    ui.horizontal_centered(|ui| {
                                        ui.add_space(20.0);
                                        for idx in page_indices(5, selected_idx) {
                                            match idx {
                                                Some(0) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", filter.filter_type.value),
                                                    &filter.filter_type.label,
                                                    filter.sel_idx == Some(0),
                                                ),
                                                Some(1) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", filter.cutoff_hz.value),
                                                    &filter.cutoff_hz.label,
                                                    filter.sel_idx == Some(1),
                                                ),
                                                Some(2) => draw_setting_option_block(
                                                    ui,
                                                    &format!(
                                                        "{:.1}",
                                                        filter.resonance_x10.value as f32 / 10.0
                                                    ),
                                                    "Resonance(Q)",
                                                    filter.sel_idx == Some(2),
                                                ),
                                                Some(3) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", filter.drive.value),
                                                    &filter.drive.label,
                                                    filter.sel_idx == Some(3),
                                                ),
                                                Some(4) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", filter.mix.value),
                                                    &filter.mix.label,
                                                    filter.sel_idx == Some(4),
                                                ),
                                                _ => draw_empty_block(ui),
                                            }
                                        }
                                    });
                                    draw_page_indicator(ui, 5, selected_idx);
                                }
                                _ => {}
                            }
                        }
                    }
                    ScreenState::InFxReverb => {
                        let bank_idx = app.config.input_fx.sel_bank_idx;
                        let slot_idx = app.fx_screen_slot_idx;
                        let slot = &app.config.input_fx.banks[bank_idx].slots[slot_idx];
                        if let Some(fx) = slot.fx.as_ref() {
                            match fx {
                                crate::config::InputFx::Reverb(reverb) => {
                                    let selected_idx = reverb.sel_idx.unwrap_or(0);
                                    ui.horizontal_centered(|ui| {
                                        ui.add_space(20.0);
                                        for idx in page_indices(6, selected_idx) {
                                            match idx {
                                                Some(0) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", reverb.size.value),
                                                    &reverb.size.label,
                                                    reverb.sel_idx == Some(0),
                                                ),
                                                Some(1) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", reverb.decay_ms.value),
                                                    &reverb.decay_ms.label,
                                                    reverb.sel_idx == Some(1),
                                                ),
                                                Some(2) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", reverb.predelay_ms.value),
                                                    &reverb.predelay_ms.label,
                                                    reverb.sel_idx == Some(2),
                                                ),
                                                Some(3) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", reverb.width.value),
                                                    &reverb.width.label,
                                                    reverb.sel_idx == Some(3),
                                                ),
                                                Some(4) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", reverb.high_cut.value),
                                                    &reverb.high_cut.label,
                                                    reverb.sel_idx == Some(4),
                                                ),
                                                Some(5) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", reverb.low_cut.value),
                                                    &reverb.low_cut.label,
                                                    reverb.sel_idx == Some(5),
                                                ),
                                                _ => draw_empty_block(ui),
                                            }
                                        }
                                    });
                                    draw_page_indicator(ui, 6, selected_idx);
                                }
                                _ => {}
                            }
                        }
                    }
                    ScreenState::InFxMyDelay => {
                        let bank_idx = app.config.input_fx.sel_bank_idx;
                        let slot_idx = app.fx_screen_slot_idx;
                        let slot = &app.config.input_fx.banks[bank_idx].slots[slot_idx];
                        if let Some(fx) = slot.fx.as_ref() {
                            match fx {
                                crate::config::InputFx::MyDelay(delay) => {
                                    let selected_idx = delay.sel_idx.unwrap_or(0);
                                    ui.horizontal_centered(|ui| {
                                        ui.add_space(20.0);
                                        for idx in page_indices(3, selected_idx) {
                                            match idx {
                                                Some(0) => draw_setting_option_block(
                                                    ui,
                                                    "Audio",
                                                    "Audio",
                                                    delay.sel_idx == Some(0),
                                                ),
                                                Some(1) => draw_setting_option_block(
                                                    ui,
                                                    "Note",
                                                    "Note",
                                                    delay.sel_idx == Some(1),
                                                ),
                                                Some(2) => draw_setting_option_block(
                                                    ui,
                                                    "Filter",
                                                    "Filter",
                                                    delay.sel_idx == Some(2),
                                                ),
                                                _ => draw_empty_block(ui),
                                            }
                                        }
                                    });
                                }
                                _ => {}
                            }
                        }
                    }
                    ScreenState::InFxMyDelayAudio => {
                        let bank_idx = app.config.input_fx.sel_bank_idx;
                        let slot_idx = app.fx_screen_slot_idx;
                        let slot = &app.config.input_fx.banks[bank_idx].slots[slot_idx];
                        if let Some(fx) = slot.fx.as_ref() {
                            match fx {
                                crate::config::InputFx::MyDelay(delay) => {
                                    let selected_idx = delay.audio_sel_idx.unwrap_or(0);
                                    ui.horizontal_centered(|ui| {
                                        ui.add_space(20.0);
                                        for idx in page_indices(3, selected_idx) {
                                            match idx {
                                                Some(0) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", delay.level.value),
                                                    &delay.level.label,
                                                    delay.audio_sel_idx == Some(0),
                                                ),
                                                Some(1) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", delay.threshold.value),
                                                    &delay.threshold.label,
                                                    delay.audio_sel_idx == Some(1),
                                                ),
                                                Some(2) => draw_setting_option_block(
                                                    ui,
                                                    "Env",
                                                    "Envelope",
                                                    delay.audio_sel_idx == Some(2),
                                                ),
                                                _ => draw_empty_block(ui),
                                            }
                                        }
                                    });
                                }
                                _ => {}
                            }
                        }
                    }
                    ScreenState::InFxMyDelayAudioEnv => {
                        let bank_idx = app.config.input_fx.sel_bank_idx;
                        let slot_idx = app.fx_screen_slot_idx;
                        let slot = &app.config.input_fx.banks[bank_idx].slots[slot_idx];
                        if let Some(fx) = slot.fx.as_ref() {
                            match fx {
                                crate::config::InputFx::MyDelay(delay) => {
                                    let env_cfg = &delay.audio_env;
                                    let selected_idx = env_cfg.sel_idx.unwrap_or(0);
                                    ui.horizontal_centered(|ui| {
                                        ui.add_space(20.0);
                                        for idx in page_indices(9, selected_idx) {
                                            match idx {
                                                Some(0) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.attack_ms.value),
                                                    &env_cfg.attack_ms.label,
                                                    env_cfg.sel_idx == Some(0),
                                                ),
                                                Some(1) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.hold_ms.value),
                                                    &env_cfg.hold_ms.label,
                                                    env_cfg.sel_idx == Some(1),
                                                ),
                                                Some(2) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.decay_ms.value),
                                                    &env_cfg.decay_ms.label,
                                                    env_cfg.sel_idx == Some(2),
                                                ),
                                                Some(3) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.sustain_pct.value),
                                                    &env_cfg.sustain_pct.label,
                                                    env_cfg.sel_idx == Some(3),
                                                ),
                                                Some(4) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.release_ms.value),
                                                    &env_cfg.release_ms.label,
                                                    env_cfg.sel_idx == Some(4),
                                                ),
                                                Some(5) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.start_pct.value),
                                                    &env_cfg.start_pct.label,
                                                    env_cfg.sel_idx == Some(5),
                                                ),
                                                Some(6) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.tension_a.value),
                                                    &env_cfg.tension_a.label,
                                                    env_cfg.sel_idx == Some(6),
                                                ),
                                                Some(7) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.tension_d.value),
                                                    &env_cfg.tension_d.label,
                                                    env_cfg.sel_idx == Some(7),
                                                ),
                                                Some(8) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.tension_r.value),
                                                    &env_cfg.tension_r.label,
                                                    env_cfg.sel_idx == Some(8),
                                                ),
                                                _ => draw_empty_block(ui),
                                            }
                                        }
                                    });
                                    draw_page_indicator(ui, 9, selected_idx);
                                }
                                _ => {}
                            }
                        }
                    }
                    ScreenState::InFxMyDelayNote => {
                        let bank_idx = app.config.input_fx.sel_bank_idx;
                        let slot_idx = app.fx_screen_slot_idx;
                        let slot = &app.config.input_fx.banks[bank_idx].slots[slot_idx];
                        if let Some(fx) = slot.fx.as_ref() {
                            match fx {
                                crate::config::InputFx::MyDelay(delay) => {
                                    let note_cfg = &delay.note;
                                    let selected_idx = note_cfg.sel_idx.unwrap_or(0);
                                    ui.horizontal_centered(|ui| {
                                        ui.add_space(20.0);
                                        for idx in page_indices(4, selected_idx) {
                                            match idx {
                                                Some(0) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", note_cfg.note.value),
                                                    &note_cfg.note.label,
                                                    note_cfg.sel_idx == Some(0),
                                                ),
                                                Some(1) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", note_cfg.octave.value),
                                                    &note_cfg.octave.label,
                                                    note_cfg.sel_idx == Some(1),
                                                ),
                                                Some(2) => draw_setting_option_block(
                                                    ui,
                                                    &note_cfg.step.value,
                                                    &note_cfg.step.label,
                                                    note_cfg.sel_idx == Some(2),
                                                ),
                                                Some(3) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", note_cfg.edit.value),
                                                    &note_cfg.edit.label,
                                                    note_cfg.sel_idx == Some(3),
                                                ),
                                                _ => draw_empty_block(ui),
                                            }
                                        }
                                    });
                                }
                                _ => {}
                            }
                        }
                    }
                    ScreenState::InFxMyDelayFilter => {
                        let bank_idx = app.config.input_fx.sel_bank_idx;
                        let slot_idx = app.fx_screen_slot_idx;
                        let slot = &app.config.input_fx.banks[bank_idx].slots[slot_idx];
                        if let Some(fx) = slot.fx.as_ref() {
                            match fx {
                                crate::config::InputFx::MyDelay(delay) => {
                                    let filter = &delay.filter;
                                    let selected_idx = delay.filter_sel_idx.unwrap_or(0);
                                    ui.horizontal_centered(|ui| {
                                        ui.add_space(20.0);
                                        for idx in page_indices(6, selected_idx) {
                                            match idx {
                                                Some(0) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", filter.filter_type.value),
                                                    &filter.filter_type.label,
                                                    delay.filter_sel_idx == Some(0),
                                                ),
                                                Some(1) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", filter.cutoff_hz.value),
                                                    &filter.cutoff_hz.label,
                                                    delay.filter_sel_idx == Some(1),
                                                ),
                                                Some(2) => draw_setting_option_block(
                                                    ui,
                                                    &format!(
                                                        "{:.1}",
                                                        filter.resonance_x10.value as f32 / 10.0
                                                    ),
                                                    "Resonance(Q)",
                                                    delay.filter_sel_idx == Some(2),
                                                ),
                                                Some(3) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", filter.drive.value),
                                                    &filter.drive.label,
                                                    delay.filter_sel_idx == Some(3),
                                                ),
                                                Some(4) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", filter.mix.value),
                                                    &filter.mix.label,
                                                    delay.filter_sel_idx == Some(4),
                                                ),
                                                Some(5) => draw_setting_option_block(
                                                    ui,
                                                    "Env",
                                                    "Envelope",
                                                    delay.filter_sel_idx == Some(5),
                                                ),
                                                _ => draw_empty_block(ui),
                                            }
                                        }
                                    });
                                    draw_page_indicator(ui, 6, selected_idx);
                                }
                                _ => {}
                            }
                        }
                    }
                    ScreenState::InFxMyDelayFilterEnv => {
                        let bank_idx = app.config.input_fx.sel_bank_idx;
                        let slot_idx = app.fx_screen_slot_idx;
                        let slot = &app.config.input_fx.banks[bank_idx].slots[slot_idx];
                        if let Some(fx) = slot.fx.as_ref() {
                            match fx {
                                crate::config::InputFx::MyDelay(delay) => {
                                    let env_cfg = &delay.filter_env;
                                    let selected_idx = env_cfg.sel_idx.unwrap_or(0);
                                    ui.horizontal_centered(|ui| {
                                        ui.add_space(20.0);
                                        for idx in page_indices(9, selected_idx) {
                                            match idx {
                                                Some(0) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.attack_ms.value),
                                                    &env_cfg.attack_ms.label,
                                                    env_cfg.sel_idx == Some(0),
                                                ),
                                                Some(1) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.hold_ms.value),
                                                    &env_cfg.hold_ms.label,
                                                    env_cfg.sel_idx == Some(1),
                                                ),
                                                Some(2) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.decay_ms.value),
                                                    &env_cfg.decay_ms.label,
                                                    env_cfg.sel_idx == Some(2),
                                                ),
                                                Some(3) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.sustain_pct.value),
                                                    &env_cfg.sustain_pct.label,
                                                    env_cfg.sel_idx == Some(3),
                                                ),
                                                Some(4) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.release_ms.value),
                                                    &env_cfg.release_ms.label,
                                                    env_cfg.sel_idx == Some(4),
                                                ),
                                                Some(5) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.start_pct.value),
                                                    &env_cfg.start_pct.label,
                                                    env_cfg.sel_idx == Some(5),
                                                ),
                                                Some(6) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.tension_a.value),
                                                    &env_cfg.tension_a.label,
                                                    env_cfg.sel_idx == Some(6),
                                                ),
                                                Some(7) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.tension_d.value),
                                                    &env_cfg.tension_d.label,
                                                    env_cfg.sel_idx == Some(7),
                                                ),
                                                Some(8) => draw_setting_option_block(
                                                    ui,
                                                    &format!("{}", env_cfg.tension_r.value),
                                                    &env_cfg.tension_r.label,
                                                    env_cfg.sel_idx == Some(8),
                                                ),
                                                _ => draw_empty_block(ui),
                                            }
                                        }
                                    });
                                    draw_page_indicator(ui, 9, selected_idx);
                                }
                                _ => {}
                            }
                        }
                    }
                    ScreenState::InFxVocoder => {
                        let bank_idx = app.config.input_fx.sel_bank_idx;
                        let slot_idx = app.fx_screen_slot_idx;
                        let slot = &app.config.input_fx.banks[bank_idx].slots[slot_idx];
                        if let Some(crate::config::InputFx::Vocoder(vocoder)) = slot.fx.as_ref() {
                            let selected_idx = vocoder.sel_idx.unwrap_or(0);
                            ui.horizontal_centered(|ui| {
                                ui.add_space(20.0);
                                for idx in page_indices(6, selected_idx) {
                                    match idx {
                                        Some(0) => draw_setting_option_block(
                                            ui,
                                            &format!("{}", vocoder.carrier.value),
                                            &vocoder.carrier.label,
                                            selected_idx == 0,
                                        ),
                                        Some(1) => draw_setting_option_block(
                                            ui,
                                            &format!("{}", vocoder.bands.value),
                                            &vocoder.bands.label,
                                            selected_idx == 1,
                                        ),
                                        Some(2) => draw_setting_option_block(
                                            ui,
                                            &format!("{}", vocoder.attack_ms.value),
                                            &vocoder.attack_ms.label,
                                            selected_idx == 2,
                                        ),
                                        Some(3) => draw_setting_option_block(
                                            ui,
                                            &format!("{}", vocoder.release_ms.value),
                                            &vocoder.release_ms.label,
                                            selected_idx == 3,
                                        ),
                                        Some(4) => draw_setting_option_block(
                                            ui,
                                            &format!("{}", vocoder.level.value),
                                            &vocoder.level.label,
                                            selected_idx == 4,
                                        ),
                                        Some(5) => draw_setting_option_block(
                                            ui,
                                            &format!("{}", vocoder.mix.value),
                                            &vocoder.mix.label,
                                            selected_idx == 5,
                                        ),
                                        _ => draw_empty_block(ui),
                                    }
                                }
                            });
                            draw_page_indicator(ui, 6, selected_idx);
                        }
                    }
                }
            });
        });
}

// 绘制 Screen 设置块
fn draw_setting_option_block(ui: &mut egui::Ui, value: &str, label: &str, is_selected: bool) {
    draw_setting_block(ui, value, label, is_selected, false);
}

// 绘制 SYS 设置块
fn draw_sys_setting_option_block(ui: &mut egui::Ui, value: &str, label: &str, is_selected: bool) {
    draw_setting_block(ui, value, label, is_selected, true);
}

fn draw_setting_block(
    ui: &mut egui::Ui,
    value: &str,
    label: &str,
    is_selected: bool,
    fit_text: bool,
) {
    let block_size = 120.0;
    let border_color = if is_selected {
        egui::Color32::from_rgb(100, 150, 255)
    } else {
        egui::Color32::from_rgb(60, 60, 60)
    };

    egui::Frame::none()
        .stroke(egui::Stroke::new(2.0, border_color))
        .inner_margin(10.0)
        .rounding(8.0)
        .show(ui, |ui| {
            ui.set_width(block_size);
            ui.set_height(block_size);
            ui.vertical_centered(|ui| {
                if fit_text {
                    let content_width = block_size - 24.0;
                    let value_font = fit_text_size(
                        ui,
                        value,
                        content_width,
                        40.0,
                        SYS_VALUE_FONT_MIN,
                        SYS_VALUE_FONT_MAX,
                    );
                    ui.label(
                        egui::RichText::new(value)
                            .size(value_font)
                            .color(egui::Color32::WHITE),
                    );
                } else {
                    ui.label(
                        egui::RichText::new(value)
                            .size(48.0)
                            .color(egui::Color32::WHITE),
                    );
                }

                ui.label(
                    egui::RichText::new(label)
                        .size(12.0)
                        .color(egui::Color32::from_rgb(150, 150, 150)),
                );
            });
        });
}

fn draw_empty_block(ui: &mut egui::Ui) {
    let block_size = 120.0;
    egui::Frame::none()
        .stroke(egui::Stroke::new(2.0, egui::Color32::from_rgb(40, 40, 40)))
        .inner_margin(10.0)
        .rounding(8.0)
        .show(ui, |ui| {
            ui.set_width(block_size);
            ui.set_height(block_size);
        });
}

fn page_indices(total: usize, selected_idx: usize) -> [Option<usize>; 4] {
    if total == 0 {
        return [None, None, None, None];
    }
    let page_start = (selected_idx / 4) * 4;
    [
        (page_start < total).then_some(page_start),
        (page_start + 1 < total).then_some(page_start + 1),
        (page_start + 2 < total).then_some(page_start + 2),
        (page_start + 3 < total).then_some(page_start + 3),
    ]
}

fn draw_page_indicator(ui: &mut egui::Ui, total: usize, selected_idx: usize) {
    if total <= 4 {
        return;
    }
    let page = selected_idx / 4 + 1;
    let pages = total.div_ceil(4);
    ui.add_space(4.0);
    ui.horizontal_centered(|ui| {
        ui.label(
            egui::RichText::new(format!("Page {}/{}", page, pages))
                .size(12.0)
                .color(egui::Color32::from_rgb(150, 150, 150)),
        );
    });
}

// 绘制 Fx 效果器选择的 block
fn draw_fx_choice_block(ui: &mut egui::Ui, label: &str, is_selected: bool) {
    let block_size = 120.0;
    let border_color = if is_selected {
        egui::Color32::from_rgb(220, 80, 80)
    } else {
        egui::Color32::from_rgb(60, 60, 60)
    };
    egui::Frame::none()
        .stroke(egui::Stroke::new(2.0, border_color))
        .inner_margin(10.0)
        .rounding(8.0)
        .show(ui, |ui| {
            ui.set_width(block_size);
            ui.set_height(block_size);
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new(label)
                        .size(18.0)
                        .color(egui::Color32::WHITE),
                );
            });
        });
}

fn screen_breadcrumb(app: &MyApp) -> Option<String> {
    match app.screen_state {
        ScreenState::Empty => None,
        ScreenState::Beat => Some("Beat".to_string()),
        ScreenState::SYS => Some("System".to_string()),
        ScreenState::FxSelect => {
            let bank = app.config.input_fx.sel_bank_idx + 1;
            let slot = match app.fx_screen_slot_idx {
                0 => "Q",
                1 => "W",
                2 => "E",
                3 => "R",
                _ => "?",
            };
            Some(format!("Input-Bank{}-Fx{}", bank, slot))
        }
        ScreenState::TrackFxSelect => {
            let bank = app.config.track_fx.sel_bank_idx + 1;
            let slot = match app.track_fx_screen_slot_idx {
                0 => "U",
                1 => "I",
                2 => "O",
                3 => "P",
                _ => "?",
            };
            Some(format!("Track-Bank{}-Fx{}", bank, slot))
        }
        ScreenState::InTrackFxDelay => {
            let bank = app.config.track_fx.sel_bank_idx + 1;
            let slot = match app.track_fx_screen_slot_idx {
                0 => "U",
                1 => "I",
                2 => "O",
                3 => "P",
                _ => "?",
            };
            Some(format!("Track-Bank{}-Fx{}-Delay", bank, slot))
        }
        ScreenState::InTrackFxRoll => {
            let bank = app.config.track_fx.sel_bank_idx + 1;
            let slot = match app.track_fx_screen_slot_idx {
                0 => "U",
                1 => "I",
                2 => "O",
                3 => "P",
                _ => "?",
            };
            Some(format!("Track-Bank{}-Fx{}-Roll", bank, slot))
        }
        ScreenState::InTrackFxFilter => {
            let bank = app.config.track_fx.sel_bank_idx + 1;
            let slot = match app.track_fx_screen_slot_idx {
                0 => "U",
                1 => "I",
                2 => "O",
                3 => "P",
                _ => "?",
            };
            Some(format!("Track-Bank{}-Fx{}-Filter", bank, slot))
        }
        ScreenState::InTrackFxFilterSeq => {
            let bank = app.config.track_fx.sel_bank_idx + 1;
            let slot = match app.track_fx_screen_slot_idx {
                0 => "U",
                1 => "I",
                2 => "O",
                3 => "P",
                _ => "?",
            };
            Some(format!("Track-Bank{}-Fx{}-Filter-Seq", bank, slot))
        }
        ScreenState::InTrackFxFilterEnv => {
            let bank = app.config.track_fx.sel_bank_idx + 1;
            let slot = match app.track_fx_screen_slot_idx {
                0 => "U",
                1 => "I",
                2 => "O",
                3 => "P",
                _ => "?",
            };
            Some(format!("Track-Bank{}-Fx{}-Filter-Envelope", bank, slot))
        }
        ScreenState::InFxOsc => {
            let bank = app.config.input_fx.sel_bank_idx + 1;
            let slot = match app.fx_screen_slot_idx {
                0 => "Q",
                1 => "W",
                2 => "E",
                3 => "R",
                _ => "?",
            };
            let fx_name = app.config.input_fx.banks[app.config.input_fx.sel_bank_idx].slots
                [app.fx_screen_slot_idx]
                .fx
                .as_ref()
                .map(|fx| fx.name())
                .unwrap_or("Empty");
            Some(format!("Input-Bank{}-Fx{}-{}", bank, slot, fx_name))
        }
        ScreenState::InFxOscAudio => {
            let bank = app.config.input_fx.sel_bank_idx + 1;
            let slot = match app.fx_screen_slot_idx {
                0 => "Q",
                1 => "W",
                2 => "E",
                3 => "R",
                _ => "?",
            };
            let fx_name = app.config.input_fx.banks[app.config.input_fx.sel_bank_idx].slots
                [app.fx_screen_slot_idx]
                .fx
                .as_ref()
                .map(|fx| fx.name())
                .unwrap_or("Empty");
            Some(format!(
                "Input-Bank{}-Fx{}-{}-OscAudio",
                bank, slot, fx_name
            ))
        }
        ScreenState::InFxNote => {
            let bank = app.config.input_fx.sel_bank_idx + 1;
            let slot = match app.fx_screen_slot_idx {
                0 => "Q",
                1 => "W",
                2 => "E",
                3 => "R",
                _ => "?",
            };
            let fx_name = app.config.input_fx.banks[app.config.input_fx.sel_bank_idx].slots
                [app.fx_screen_slot_idx]
                .fx
                .as_ref()
                .map(|fx| fx.name())
                .unwrap_or("Empty");
            Some(format!("Input-Bank{}-Fx{}-{}-Note", bank, slot, fx_name))
        }
        ScreenState::InFxOscAudioEnv => {
            let bank = app.config.input_fx.sel_bank_idx + 1;
            let slot = match app.fx_screen_slot_idx {
                0 => "Q",
                1 => "W",
                2 => "E",
                3 => "R",
                _ => "?",
            };
            let fx_name = app.config.input_fx.banks[app.config.input_fx.sel_bank_idx].slots
                [app.fx_screen_slot_idx]
                .fx
                .as_ref()
                .map(|fx| fx.name())
                .unwrap_or("Empty");
            Some(format!(
                "Input-Bank{}-Fx{}-{}-OscAudio-Envelope",
                bank, slot, fx_name
            ))
        }
        ScreenState::InFxOscFilter => {
            let bank = app.config.input_fx.sel_bank_idx + 1;
            let slot = match app.fx_screen_slot_idx {
                0 => "Q",
                1 => "W",
                2 => "E",
                3 => "R",
                _ => "?",
            };
            let fx_name = app.config.input_fx.banks[app.config.input_fx.sel_bank_idx].slots
                [app.fx_screen_slot_idx]
                .fx
                .as_ref()
                .map(|fx| fx.name())
                .unwrap_or("Empty");
            Some(format!(
                "Input-Bank{}-Fx{}-{}-OscFilter",
                bank, slot, fx_name
            ))
        }
        ScreenState::InFxOscFilterEnv => {
            let bank = app.config.input_fx.sel_bank_idx + 1;
            let slot = match app.fx_screen_slot_idx {
                0 => "Q",
                1 => "W",
                2 => "E",
                3 => "R",
                _ => "?",
            };
            let fx_name = app.config.input_fx.banks[app.config.input_fx.sel_bank_idx].slots
                [app.fx_screen_slot_idx]
                .fx
                .as_ref()
                .map(|fx| fx.name())
                .unwrap_or("Empty");
            Some(format!(
                "Input-Bank{}-Fx{}-{}-OscFilter-Envelope",
                bank, slot, fx_name
            ))
        }
        ScreenState::InFxFilter => {
            let bank = app.config.input_fx.sel_bank_idx + 1;
            let slot = match app.fx_screen_slot_idx {
                0 => "Q",
                1 => "W",
                2 => "E",
                3 => "R",
                _ => "?",
            };
            let fx_name = app.config.input_fx.banks[app.config.input_fx.sel_bank_idx].slots
                [app.fx_screen_slot_idx]
                .fx
                .as_ref()
                .map(|fx| fx.name())
                .unwrap_or("Empty");
            Some(format!("Input-Bank{}-Fx{}-{}", bank, slot, fx_name))
        }
        ScreenState::InFxReverb => {
            let bank = app.config.input_fx.sel_bank_idx + 1;
            let slot = match app.fx_screen_slot_idx {
                0 => "Q",
                1 => "W",
                2 => "E",
                3 => "R",
                _ => "?",
            };
            let fx_name = app.config.input_fx.banks[app.config.input_fx.sel_bank_idx].slots
                [app.fx_screen_slot_idx]
                .fx
                .as_ref()
                .map(|fx| fx.name())
                .unwrap_or("Empty");
            Some(format!("Input-Bank{}-Fx{}-{}", bank, slot, fx_name))
        }
        ScreenState::InFxMyDelay => {
            let bank = app.config.input_fx.sel_bank_idx + 1;
            let slot = match app.fx_screen_slot_idx {
                0 => "Q",
                1 => "W",
                2 => "E",
                3 => "R",
                _ => "?",
            };
            let fx_name = app.config.input_fx.banks[app.config.input_fx.sel_bank_idx].slots
                [app.fx_screen_slot_idx]
                .fx
                .as_ref()
                .map(|fx| fx.name())
                .unwrap_or("Empty");
            Some(format!("Input-Bank{}-Fx{}-{}", bank, slot, fx_name))
        }
        ScreenState::InFxMyDelayAudio => {
            let bank = app.config.input_fx.sel_bank_idx + 1;
            let slot = match app.fx_screen_slot_idx {
                0 => "Q",
                1 => "W",
                2 => "E",
                3 => "R",
                _ => "?",
            };
            let fx_name = app.config.input_fx.banks[app.config.input_fx.sel_bank_idx].slots
                [app.fx_screen_slot_idx]
                .fx
                .as_ref()
                .map(|fx| fx.name())
                .unwrap_or("Empty");
            Some(format!("Input-Bank{}-Fx{}-{}-Audio", bank, slot, fx_name))
        }
        ScreenState::InFxMyDelayAudioEnv => {
            let bank = app.config.input_fx.sel_bank_idx + 1;
            let slot = match app.fx_screen_slot_idx {
                0 => "Q",
                1 => "W",
                2 => "E",
                3 => "R",
                _ => "?",
            };
            let fx_name = app.config.input_fx.banks[app.config.input_fx.sel_bank_idx].slots
                [app.fx_screen_slot_idx]
                .fx
                .as_ref()
                .map(|fx| fx.name())
                .unwrap_or("Empty");
            Some(format!(
                "Input-Bank{}-Fx{}-{}-Audio-Envelope",
                bank, slot, fx_name
            ))
        }
        ScreenState::InFxMyDelayNote => {
            let bank = app.config.input_fx.sel_bank_idx + 1;
            let slot = match app.fx_screen_slot_idx {
                0 => "Q",
                1 => "W",
                2 => "E",
                3 => "R",
                _ => "?",
            };
            let fx_name = app.config.input_fx.banks[app.config.input_fx.sel_bank_idx].slots
                [app.fx_screen_slot_idx]
                .fx
                .as_ref()
                .map(|fx| fx.name())
                .unwrap_or("Empty");
            Some(format!("Input-Bank{}-Fx{}-{}-Note", bank, slot, fx_name))
        }
        ScreenState::InFxMyDelayFilter => {
            let bank = app.config.input_fx.sel_bank_idx + 1;
            let slot = match app.fx_screen_slot_idx {
                0 => "Q",
                1 => "W",
                2 => "E",
                3 => "R",
                _ => "?",
            };
            let fx_name = app.config.input_fx.banks[app.config.input_fx.sel_bank_idx].slots
                [app.fx_screen_slot_idx]
                .fx
                .as_ref()
                .map(|fx| fx.name())
                .unwrap_or("Empty");
            Some(format!("Input-Bank{}-Fx{}-{}-Filter", bank, slot, fx_name))
        }
        ScreenState::InFxMyDelayFilterEnv => {
            let bank = app.config.input_fx.sel_bank_idx + 1;
            let slot = match app.fx_screen_slot_idx {
                0 => "Q",
                1 => "W",
                2 => "E",
                3 => "R",
                _ => "?",
            };
            let fx_name = app.config.input_fx.banks[app.config.input_fx.sel_bank_idx].slots
                [app.fx_screen_slot_idx]
                .fx
                .as_ref()
                .map(|fx| fx.name())
                .unwrap_or("Empty");
            Some(format!(
                "Input-Bank{}-Fx{}-{}-Filter-Envelope",
                bank, slot, fx_name
            ))
        }
        ScreenState::InFxVocoder => {
            let bank = app.config.input_fx.sel_bank_idx + 1;
            let slot = match app.fx_screen_slot_idx {
                0 => "Q",
                1 => "W",
                2 => "E",
                3 => "R",
                _ => "?",
            };
            let fx_name = app.config.input_fx.banks[app.config.input_fx.sel_bank_idx].slots
                [app.fx_screen_slot_idx]
                .fx
                .as_ref()
                .map(|fx| fx.name())
                .unwrap_or("Empty");
            Some(format!("Input-Bank{}-Fx{}-{}", bank, slot, fx_name))
        }
    }
}

fn fit_text_size(
    ui: &egui::Ui,
    text: &str,
    max_width: f32,
    max_height: f32,
    min_size: f32,
    max_size: f32,
) -> f32 {
    let mut size = max_size;
    while size >= min_size {
        let galley = ui.painter().layout_no_wrap(
            text.to_owned(),
            egui::FontId::proportional(size),
            egui::Color32::WHITE,
        );
        let text_size = galley.size();
        if text_size.x <= max_width && text_size.y <= max_height {
            return size;
        }
        size -= 1.0;
    }
    min_size
}
