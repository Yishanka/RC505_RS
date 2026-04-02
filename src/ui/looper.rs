use eframe::egui;
use crate::app::MyApp;
use crate::state::{AppState, FxState, ScreenState, TrackState};
use crate::config::{FxKind, TrackFxKind};

use std::f32::consts::PI;
use std::time::Instant;

// UI 间距常量
const SPACING_LARGE: f32 = 20.0;

// 屏幕区域常量
const SCREEN_WIDTH: f32 = 500.0;
const SCREEN_HEIGHT: f32 = 150.0;
const SCREEN_ROUNDING: f32 = 10.0;
const SCREEN_STROKE_WIDTH: f32 = 2.0;

// 轨道区域常量
const TRACK_COUNT: usize = 5;
const TRACK_WIDTH: f32 = 200.0;
const TRACK_HEIGHT: f32 = 400.0;
const TRACK_ROUNDING: f32 = 10.0;
const TRACK_BORDER_WIDTH: f32 = 2.0;
const TRACK_BG_SELECTED: (u8, u8, u8) = (45, 45, 45);
const TRACK_BG_UNSELECTED: (u8, u8, u8) = (25, 25, 25);

// 播放按钮区域常量
const PLAY_CENTER_OFFSET_Y: f32 = 80.0;
const PLAY_RADIUS: f32 = 70.0;
const PLAY_RING_WIDTH: f32 = 5.0;
const PLAY_RING_RADIUS: f32 = PLAY_RADIUS - 5.0;
const PLAY_ARC_START_OFFSET: f32 = PI / 4.0;
const PLAY_ARC_POINTS: usize = 32;
const PLAY_INNER_RING_OFFSET: f32 = 20.0;
const PLAY_INDICATOR_OFFSET: f32 = 20.0;
const PLAY_INDICATOR_SIZE: f32 = 10.0;
const PLAY_TRIANGLE_OFFSET_X: f32 = 10.0;
const PLAY_TRIANGLE_OFFSET_Y: f32 = 25.0;
const PLAY_TRIANGLE_HEIGHT: f32 = 10.0;

// 轨道编号显示常量
const TRACK_NUMBER_ANGLE: f32 = 3.0 * PI / 4.0;
const TRACK_NUMBER_DISTANCE: f32 = 25.0;
const TRACK_NUMBER_SIZE: f32 = 20.0;

// 左侧功能按钮常量
const BUTTON_START_Y: f32 = 25.0;
const BUTTON_X: f32 = 25.0;
const BUTTON_WIDTH: f32 = 60.0;
const BUTTON_HEIGHT: f32 = 30.0;
const BUTTON_SPACING: f32 = 30.0;
const BUTTON_ROUNDING: f32 = 5.0;
const BUTTON_BORDER_WIDTH: f32 = 2.0;
const BUTTON_TEXT_SIZE: f32 = 14.0;

// 暂停按钮常量
const PAUSE_RADIUS: f32 = 30.0;
const PAUSE_OFFSET_Y: f32 = 10.0;
const PAUSE_BORDER_WIDTH: f32 = 2.0;
const PAUSE_SQUARE_SIZE: f32 = 16.0;
const PAUSE_INDICATOR_COLOR: (u8, u8, u8) = (200, 200, 200);

// 右侧音量推子常量
const SLIDER_X_OFFSET: f32 = 70.0;
const SLIDER_WIDTH: f32 = 30.0;
const SLIDER_ROUNDING: f32 = 2.0;
const SLIDER_BORDER_WIDTH: f32 = 1.0;
const SLIDER_KNOB_HEIGHT: f32 = 10.0;
const SLIDER_KNOB_X_OFFSET: f32 = 5.0;
const SLIDER_KNOB_WIDTH: f32 = SLIDER_WIDTH + 10.0;
const SLIDER_KNOB_BORDER_WIDTH: f32 = 2.0;

const SYS_VALUE_FONT_MAX: f32 = 28.0;
const SYS_VALUE_FONT_MIN: f32 = 10.0;
// const SYS_LABEL_FONT_SIZE: f32 = 10.0;
const STATE_RED: egui::Color32 = egui::Color32::from_rgb(220, 60, 60);
const STATE_RED_BEAT_FLASH: egui::Color32 = egui::Color32::from_rgb(245, 95, 95);
const STATE_GREEN: egui::Color32 = egui::Color32::from_rgb(60, 200, 90);
const STATE_YELLOW: egui::Color32 = egui::Color32::from_rgb(220, 200, 70);
const STATE_GRAY: egui::Color32 = egui::Color32::GRAY;
const STATE_BLUE: egui::Color32 = egui::Color32::from_rgb(90, 140, 255);

const FX_PANEL_HEIGHT: f32 = 120.0;
const FX_BUTTON_RADIUS: f32 = 18.0;
const FX_BUTTON_SPACING: f32 = 10.0;
const FX_BUTTON_LABEL_SIZE: f32 = 14.0;
const FX_PANEL_SIDE_MARGIN: f32 = 8.0;
const FX_BANK_WIDTH: f32 = 100.0;
const FX_BANK_HEIGHT: f32 = 30.0;
const FX_BANK_ROUNDING: f32 = 12.0;

// 绘制 Looper 主界面
pub fn draw_loopstation_view(ui: &mut egui::Ui, app: &mut MyApp) {
    egui::Frame::none()
        .show(ui, |ui| {
            ui.add_space(SPACING_LARGE);
            ui.horizontal(|ui| {
                let available_width = ui.available_width();
                let left_space = (available_width - SCREEN_WIDTH) / 2.0;
                if left_space > 0.0 {
                    ui.add_space(left_space);
                }
                draw_screen(ui, app);
            });

            ui.add_space(SPACING_LARGE);
            ui.horizontal(|ui| {
                let available_width = ui.available_width();
                let tracks_total_width = TRACK_COUNT as f32 * TRACK_WIDTH;
                let left_space = (available_width - tracks_total_width) / 2.0;
                if left_space > 0.0 {
                    ui.add_space(left_space);
                }
                draw_fx_panel(ui, app, tracks_total_width);
            });

            ui.horizontal(|ui| {
                let available_width = ui.available_width();
                let tracks_total_width = TRACK_COUNT as f32 * TRACK_WIDTH;
                let left_space = (available_width - tracks_total_width) / 2.0;
                if left_space > 0.0 {
                    ui.add_space(left_space);
                }
                
                for (idx, t) in app.tracks.iter().enumerate() {
                    draw_track_slot(
                        ui, 
                        &app,
                        idx, 
                        t.track_volume,
                    );
                }
            });
            
            ui.add_space(SPACING_LARGE);
        });
}

// 绘制单个轨道卡片
pub fn draw_track_slot(ui: &mut egui::Ui, app: &MyApp, track_id: usize, track_vol: f32,) {
    
    fn beat_pulse_red(app: &MyApp, now: Instant) -> bool {
        app.metronome
            .beat_phase(now)
            .map(|phase| phase < 0.05 || phase > 0.95)
            .unwrap_or(false)
    }

    fn beat_flash_subtle(app: &MyApp, now: Instant) -> bool {
        app.metronome
            .beat_phase(now)
            .map(|phase| phase < 0.05 || phase > 0.95)
            .unwrap_or(false)
    }

    let track_sel = app.track_sel;
    let (rect, _) = ui.allocate_at_least(
        egui::Vec2::new(TRACK_WIDTH, TRACK_HEIGHT), egui::Sense::hover()
    );
    let painter = ui.painter();

    let bg_color = if Some(track_id) == track_sel { 
        egui::Color32::from_rgb(TRACK_BG_SELECTED.0, TRACK_BG_SELECTED.1, TRACK_BG_SELECTED.2)
    } else { 
        egui::Color32::from_rgb(TRACK_BG_UNSELECTED.0, TRACK_BG_UNSELECTED.1, TRACK_BG_UNSELECTED.2)
    };
    painter.rect_filled(rect, TRACK_ROUNDING, bg_color);
    
    if Some(track_id) == track_sel {
        painter.rect_stroke(rect, TRACK_ROUNDING, egui::Stroke::new(TRACK_BORDER_WIDTH, egui::Color32::RED));
    }

    let play_center: egui::Pos2 = rect.center_bottom() - egui::vec2(0.0, PLAY_CENTER_OFFSET_Y);
    painter.circle_filled(play_center, PLAY_RADIUS, egui::Color32::BLACK);
    
    // Circle 1
    let start_angle = PI / 2.0 + PLAY_ARC_START_OFFSET;
    let end_angle = PI / 2.0 - PLAY_ARC_START_OFFSET + 2.0 * PI;
    let now = Instant::now();
    let track= &app.tracks[track_id]; 
    let state = track.track_state;
    let beat_pulse_red = beat_pulse_red(app, now);
    let beat_flash_subtle = beat_flash_subtle(app, now);
    let progress = track.track_play_progress(now);

    match state {
        TrackState::Record => {
            let color = if beat_pulse_red { STATE_RED } else { STATE_GRAY };
            draw_ring_arc(
                painter,
                play_center,
                PLAY_RING_RADIUS,
                start_angle,
                end_angle,
                PLAY_ARC_POINTS,
                color,
            );
        }
        TrackState::Play | TrackState::NxtPlay | TrackState::Dub => {
            let active_color = if beat_flash_subtle {
                STATE_RED_BEAT_FLASH
                // STATE_GRAY
            } else {
                STATE_RED
            };
            draw_ring_arc_progress(
                painter,
                play_center,
                PLAY_RING_RADIUS,
                start_angle,
                end_angle,
                PLAY_ARC_POINTS,
                progress,
                active_color,
                STATE_GRAY,
            );
        }
        TrackState::Pause | TrackState::Empty => {
            draw_ring_arc(
                painter,
                play_center,
                PLAY_RING_RADIUS,
                start_angle,
                end_angle,
                PLAY_ARC_POINTS,
                STATE_GRAY,
            );
        }
    }

    // Circle 2
    let center_color = match state {
        TrackState::Record => STATE_RED,
        TrackState::Play | TrackState::NxtPlay => STATE_GREEN,
        TrackState::Dub => STATE_YELLOW,
        TrackState::Pause | TrackState::Empty => STATE_GRAY,
    };

    painter.circle_stroke(play_center, PLAY_RADIUS - PLAY_INNER_RING_OFFSET, egui::Stroke::new(PLAY_RING_WIDTH, center_color));
    
    painter.circle_filled(play_center + egui::vec2(PLAY_INDICATOR_OFFSET, 0.0), PLAY_INDICATOR_SIZE, center_color);
    let triangle_points = vec![
        egui::pos2(play_center.x - PLAY_TRIANGLE_OFFSET_X, play_center.y),
        egui::pos2(play_center.x - PLAY_TRIANGLE_OFFSET_Y, play_center.y - PLAY_TRIANGLE_HEIGHT),
        egui::pos2(play_center.x - PLAY_TRIANGLE_OFFSET_Y, play_center.y + PLAY_TRIANGLE_HEIGHT),
    ];
    painter.add(egui::Shape::convex_polygon(triangle_points, center_color, egui::Stroke::NONE));

    let number_pos = play_center + egui::vec2(
        TRACK_NUMBER_ANGLE.cos() * (PLAY_RADIUS + TRACK_NUMBER_DISTANCE),
        - TRACK_NUMBER_ANGLE.sin() * (PLAY_RADIUS + TRACK_NUMBER_DISTANCE),
    );
    painter.text(
        number_pos, 
        egui::Align2::LEFT_TOP, 
        format!("{}", track_id + 1), 
        egui::FontId::proportional(TRACK_NUMBER_SIZE), 
        egui::Color32::WHITE
    );

    let button_start_y = rect.top() + BUTTON_START_Y;
    let button_x = rect.left() + BUTTON_X;

    let rect1 = egui::Rect::from_min_size(
        egui::pos2(button_x, button_start_y),
        egui::vec2(BUTTON_WIDTH, BUTTON_HEIGHT)
    );
    painter.rect_filled(rect1, BUTTON_ROUNDING, egui::Color32::BLACK);
    painter.rect_stroke(rect1, BUTTON_ROUNDING, egui::Stroke::new(BUTTON_BORDER_WIDTH, egui::Color32::GRAY));
    painter.text(rect1.center(), egui::Align2::CENTER_CENTER, "FX", egui::FontId::proportional(BUTTON_TEXT_SIZE), egui::Color32::WHITE);

    let rect2 = egui::Rect::from_min_size(
        egui::pos2(button_x, button_start_y + BUTTON_HEIGHT + BUTTON_SPACING),
        egui::vec2(BUTTON_WIDTH, BUTTON_HEIGHT)
    );
    painter.rect_filled(rect2, BUTTON_ROUNDING, egui::Color32::BLACK);
    painter.rect_stroke(rect2, BUTTON_ROUNDING, egui::Stroke::new(BUTTON_BORDER_WIDTH, egui::Color32::GRAY));
    painter.text(rect2.center(), egui::Align2::CENTER_CENTER, "Track", egui::FontId::proportional(BUTTON_TEXT_SIZE), egui::Color32::WHITE);

    let pause_y = button_start_y + BUTTON_HEIGHT + BUTTON_SPACING + BUTTON_HEIGHT + BUTTON_SPACING + PAUSE_OFFSET_Y;
    let pause_center = egui::pos2(button_x + BUTTON_WIDTH / 2.0, pause_y + PAUSE_RADIUS / 2.0);
    painter.circle_filled(pause_center, PAUSE_RADIUS, egui::Color32::BLACK);
    painter.circle_stroke(pause_center, PAUSE_RADIUS, egui::Stroke::new(PAUSE_BORDER_WIDTH, egui::Color32::from_rgb(PAUSE_INDICATOR_COLOR.0, PAUSE_INDICATOR_COLOR.1, PAUSE_INDICATOR_COLOR.2)));
    let square_rect = egui::Rect::from_center_size(pause_center, egui::vec2(PAUSE_SQUARE_SIZE, PAUSE_SQUARE_SIZE));
    painter.rect_filled(square_rect, 0.0, egui::Color32::from_rgb(PAUSE_INDICATOR_COLOR.0, PAUSE_INDICATOR_COLOR.1, PAUSE_INDICATOR_COLOR.2));

    let slider_x = rect.right() - SLIDER_X_OFFSET;
    let slider_height = (BUTTON_HEIGHT * 2.0) + BUTTON_SPACING + (PAUSE_RADIUS * 2.0) + BUTTON_SPACING;
    let slider_rect = egui::Rect::from_min_size(
        egui::pos2(slider_x, button_start_y),
        egui::vec2(SLIDER_WIDTH, slider_height)
    );
    painter.rect_filled(slider_rect, SLIDER_ROUNDING, egui::Color32::from_rgb(50, 50, 50));
    painter.rect_stroke(slider_rect, SLIDER_ROUNDING, egui::Stroke::new(SLIDER_BORDER_WIDTH, egui::Color32::GRAY));
    let knob_y = slider_rect.bottom() - SLIDER_KNOB_HEIGHT - (track_vol * (slider_height - SLIDER_KNOB_HEIGHT));
    let knob_rect = egui::Rect::from_min_size(
        egui::pos2(slider_x - SLIDER_KNOB_X_OFFSET, knob_y),
        egui::vec2(SLIDER_KNOB_WIDTH, SLIDER_KNOB_HEIGHT)
    );
    painter.rect_filled(knob_rect, SLIDER_ROUNDING, egui::Color32::from_rgb(150, 150, 150));
    painter.rect_stroke(knob_rect, SLIDER_ROUNDING, egui::Stroke::new(SLIDER_KNOB_BORDER_WIDTH, egui::Color32::WHITE));
}

// 绘制顶部屏幕区域
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
                            ui.label(egui::RichText::new(
                                &app.projects[app.sel_project_idx].name
                            ).size(48.0).color(egui::Color32::WHITE));
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
                        let sys_config:&crate::config::SystemConfigs  = &app.config.system_config;
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
                        ui.horizontal_centered(|ui| {
                            ui.add_space(20.0);
                            for idx in page_indices(4, 0) {
                                match idx {
                                    Some(0) => draw_fx_choice_block(ui, "Oscillator", selected == FxKind::Oscillator),
                                    Some(1) => draw_fx_choice_block(ui, "Filter", selected == FxKind::Filter),
                                    Some(2) => draw_fx_choice_block(ui, "Reverb", selected == FxKind::Reverb),
                                    Some(3) => draw_fx_choice_block(ui, "MyDelay", selected == FxKind::MyDelay),
                                    _ => draw_empty_block(ui),
                                }
                            }
                        });
                    }
                    ScreenState::TrackFxSelect => {
                        let bank_idx = app.config.track_fx.sel_bank_idx;
                        let slot_idx = app.track_fx_screen_slot_idx;
                        let selected = app.config.track_fx.slot_kind(bank_idx, slot_idx);
                        let selected_idx = if selected == TrackFxKind::Roll { 1 } else { 0 };
                        ui.horizontal_centered(|ui| {
                            ui.add_space(20.0);
                            for idx in page_indices(2, selected_idx) {
                                match idx {
                                    Some(0) => {
                                        draw_fx_choice_block(ui, "Delay", selected == TrackFxKind::Delay)
                                    }
                                    Some(1) => {
                                        draw_fx_choice_block(ui, "Roll", selected == TrackFxKind::Roll)
                                    }
                                    _ => draw_empty_block(ui),
                                }
                            }
                        });
                    }
                    ScreenState::InTrackFxDelay => {
                        let bank_idx = app.config.track_fx.sel_bank_idx;
                        let slot_idx = app.track_fx_screen_slot_idx;
                        if let Some(crate::config::TrackFx::Delay(delay)) = app.config.track_fx.slot_fx(bank_idx, slot_idx) {
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
                        if let Some(crate::config::TrackFx::Roll(roll)) = app.config.track_fx.slot_fx(bank_idx, slot_idx) {
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
                                                    &format!("{:.1}", filter.resonance_x10.value as f32 / 10.0),
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
                                                    &format!("{:.1}", filter.resonance_x10.value as f32 / 10.0),
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
                                                    &format!("{:.1}", filter.resonance_x10.value as f32 / 10.0),
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

fn draw_setting_block(ui: &mut egui::Ui, value: &str, label: &str, is_selected: bool, fit_text: bool) {
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
                    ui.label(egui::RichText::new(value)
                        .size(48.0)
                        .color(egui::Color32::WHITE));
                }

                ui.label(egui::RichText::new(label)
                    .size(12.0)
                    .color(egui::Color32::from_rgb(150, 150, 150)));
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
        ScreenState::InFxOsc => {
            let bank = app.config.input_fx.sel_bank_idx + 1;
            let slot = match app.fx_screen_slot_idx {
                0 => "Q",
                1 => "W",
                2 => "E",
                3 => "R",
                _ => "?",
            };
            let fx_name = app.config.input_fx.banks[app.config.input_fx.sel_bank_idx]
                .slots[app.fx_screen_slot_idx]
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
            let fx_name = app.config.input_fx.banks[app.config.input_fx.sel_bank_idx]
                .slots[app.fx_screen_slot_idx]
                .fx
                .as_ref()
                .map(|fx| fx.name())
                .unwrap_or("Empty");
            Some(format!("Input-Bank{}-Fx{}-{}-OscAudio", bank, slot, fx_name))
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
            let fx_name = app.config.input_fx.banks[app.config.input_fx.sel_bank_idx]
                .slots[app.fx_screen_slot_idx]
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
            let fx_name = app.config.input_fx.banks[app.config.input_fx.sel_bank_idx]
                .slots[app.fx_screen_slot_idx]
                .fx
                .as_ref()
                .map(|fx| fx.name())
                .unwrap_or("Empty");
            Some(format!("Input-Bank{}-Fx{}-{}-OscAudio-Envelope", bank, slot, fx_name))
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
            let fx_name = app.config.input_fx.banks[app.config.input_fx.sel_bank_idx]
                .slots[app.fx_screen_slot_idx]
                .fx
                .as_ref()
                .map(|fx| fx.name())
                .unwrap_or("Empty");
            Some(format!("Input-Bank{}-Fx{}-{}-OscFilter", bank, slot, fx_name))
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
            let fx_name = app.config.input_fx.banks[app.config.input_fx.sel_bank_idx]
                .slots[app.fx_screen_slot_idx]
                .fx
                .as_ref()
                .map(|fx| fx.name())
                .unwrap_or("Empty");
            Some(format!("Input-Bank{}-Fx{}-{}-OscFilter-Envelope", bank, slot, fx_name))
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
            let fx_name = app.config.input_fx.banks[app.config.input_fx.sel_bank_idx]
                .slots[app.fx_screen_slot_idx]
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
            let fx_name = app.config.input_fx.banks[app.config.input_fx.sel_bank_idx]
                .slots[app.fx_screen_slot_idx]
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
            let fx_name = app.config.input_fx.banks[app.config.input_fx.sel_bank_idx]
                .slots[app.fx_screen_slot_idx]
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
            let fx_name = app.config.input_fx.banks[app.config.input_fx.sel_bank_idx]
                .slots[app.fx_screen_slot_idx]
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
            let fx_name = app.config.input_fx.banks[app.config.input_fx.sel_bank_idx]
                .slots[app.fx_screen_slot_idx]
                .fx
                .as_ref()
                .map(|fx| fx.name())
                .unwrap_or("Empty");
            Some(format!("Input-Bank{}-Fx{}-{}-Audio-Envelope", bank, slot, fx_name))
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
            let fx_name = app.config.input_fx.banks[app.config.input_fx.sel_bank_idx]
                .slots[app.fx_screen_slot_idx]
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
            let fx_name = app.config.input_fx.banks[app.config.input_fx.sel_bank_idx]
                .slots[app.fx_screen_slot_idx]
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
            let fx_name = app.config.input_fx.banks[app.config.input_fx.sel_bank_idx]
                .slots[app.fx_screen_slot_idx]
                .fx
                .as_ref()
                .map(|fx| fx.name())
                .unwrap_or("Empty");
            Some(format!("Input-Bank{}-Fx{}-{}-Filter-Envelope", bank, slot, fx_name))
        }
    }
}

fn draw_fx_panel(ui: &mut egui::Ui, app: &MyApp, panel_width: f32) {
    let (rect, _) = ui.allocate_at_least(
        egui::Vec2::new(panel_width, FX_PANEL_HEIGHT), egui::Sense::hover()
    );
    let painter = ui.painter();

    let step_x = FX_BUTTON_RADIUS * 2.0 + FX_BUTTON_SPACING;
    let left_start_x = rect.left() + FX_PANEL_SIDE_MARGIN + FX_BUTTON_RADIUS;
    let right_start_x = rect.right() - FX_PANEL_SIDE_MARGIN - FX_BUTTON_RADIUS - step_x * 3.0;
    let button_y = rect.center().y;
    let input_keys = ["Q", "W", "E", "R"];
    let track_keys = ["U", "I", "O", "P"];

    for (idx, key) in input_keys.iter().enumerate() {
        let center = egui::pos2(left_start_x + idx as f32 * step_x, button_y);
        let mut fill = egui::Color32::from_rgb(30, 30, 30);
        match app.fx_state {
            FxState::Bank => {
                if app.config.input_fx.sel_bank_idx == idx {
                    fill = STATE_BLUE;
                }
            }
            FxState::Single => {
                let slot = &app.config.input_fx.active_bank().slots[idx];
                if slot.is_enabled {
                    fill = STATE_RED;
                }
            }
        }

        painter.circle_filled(center, FX_BUTTON_RADIUS, fill);
        painter.circle_stroke(
            center,
            FX_BUTTON_RADIUS,
            egui::Stroke::new(2.0, egui::Color32::from_rgb(80, 80, 80)),
        );
        painter.text(
            center,
            egui::Align2::CENTER_CENTER,
            *key,
            egui::FontId::proportional(FX_BUTTON_LABEL_SIZE),
            egui::Color32::WHITE,
        );
    }

    for (idx, key) in track_keys.iter().enumerate() {
        let center = egui::pos2(right_start_x + idx as f32 * step_x, button_y);
        let mut fill = egui::Color32::from_rgb(30, 30, 30);
        match app.fx_state {
            FxState::Bank => {
                if app.config.track_fx.sel_bank_idx == idx {
                    fill = STATE_BLUE;
                }
            }
            FxState::Single => {
                if let Some(track_idx) = app.track_sel {
                    if app
                        .config
                        .track_fx
                        .slot_enabled(track_idx, app.config.track_fx.sel_bank_idx, idx)
                    {
                        fill = STATE_RED;
                    }
                }
            }
        }

        painter.circle_filled(center, FX_BUTTON_RADIUS, fill);
        painter.circle_stroke(
            center,
            FX_BUTTON_RADIUS,
            egui::Stroke::new(2.0, egui::Color32::from_rgb(80, 80, 80)),
        );
        painter.text(
            center,
            egui::Align2::CENTER_CENTER,
            *key,
            egui::FontId::proportional(FX_BUTTON_LABEL_SIZE),
            egui::Color32::WHITE,
        );
    }

    let in_bank_rect = egui::Rect::from_center_size(
        egui::pos2(rect.center().x - 110.0, rect.center().y),
        egui::vec2(FX_BANK_WIDTH, FX_BANK_HEIGHT),
    );
    let in_bank_fill = if app.fx_state == FxState::Bank {
        STATE_RED
    } else {
        egui::Color32::from_rgb(30, 30, 30)
    };
    painter.rect_filled(in_bank_rect, FX_BANK_ROUNDING, in_bank_fill);
    painter.rect_stroke(
        in_bank_rect,
        FX_BANK_ROUNDING,
        egui::Stroke::new(2.0, egui::Color32::from_rgb(80, 80, 80)),
    );
    painter.text(
        in_bank_rect.center(),
        egui::Align2::CENTER_CENTER,
        format!("IN B{}", app.config.input_fx.sel_bank_idx + 1),
        egui::FontId::proportional(14.0),
        egui::Color32::WHITE,
    );

    let out_bank_rect = egui::Rect::from_center_size(
        egui::pos2(rect.center().x + 110.0, rect.center().y),
        egui::vec2(FX_BANK_WIDTH, FX_BANK_HEIGHT),
    );
    let out_bank_fill = if app.fx_state == FxState::Bank {
        STATE_RED
    } else {
        egui::Color32::from_rgb(30, 30, 30)
    };
    painter.rect_filled(out_bank_rect, FX_BANK_ROUNDING, out_bank_fill);
    painter.rect_stroke(
        out_bank_rect,
        FX_BANK_ROUNDING,
        egui::Stroke::new(2.0, egui::Color32::from_rgb(80, 80, 80)),
    );
    painter.text(
        out_bank_rect.center(),
        egui::Align2::CENTER_CENTER,
        format!("TR B{}", app.config.track_fx.sel_bank_idx + 1),
        egui::FontId::proportional(14.0),
        egui::Color32::WHITE,
    );

    // if app.fx_state == FxState::Single && app.track_sel.is_none() {
    //     painter.text(
    //         rect.center_top() + egui::vec2(0.0, 8.0),
    //         egui::Align2::CENTER_TOP,
    //         "Select Track To Toggle UIOP",
    //         egui::FontId::proportional(13.0),
    //         egui::Color32::from_rgb(170, 170, 170),
    //     );
    // }
}

// 根据可用空间计算最合适的字体大小
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

fn draw_ring_arc(
    painter: &egui::Painter,
    center: egui::Pos2,
    radius: f32,
    start_angle: f32,
    end_angle: f32,
    arc_points: usize,
    color: egui::Color32,
) {
    let mut path = egui::epaint::PathShape::line(vec![], egui::Stroke::new(PLAY_RING_WIDTH, color));
    for i in 0..=arc_points {
        let angle = start_angle + (end_angle - start_angle) * (i as f32 / arc_points as f32);
        let x = center.x + radius * angle.cos();
        let y = center.y + radius * angle.sin();
        path.points.push(egui::pos2(x, y));
    }
    painter.add(path);
}

fn draw_ring_arc_progress(
    painter: &egui::Painter,
    center: egui::Pos2,
    radius: f32,
    start_angle: f32,
    end_angle: f32,
    arc_points: usize,
    progress: f32,
    active_color: egui::Color32,
    inactive_color: egui::Color32,
) {
    let split = (progress.clamp(0.0, 1.0) * arc_points as f32).round() as usize;

    let mut active_points = Vec::new();
    let mut inactive_points = Vec::new();
    for i in 0..=arc_points {
        let angle = start_angle + 
        
        (end_angle - start_angle) * (i as f32 / arc_points as f32);
        let point = egui::pos2(center.x + radius * angle.cos(), center.y + radius * angle.sin());
        if i <= split {
            inactive_points.push(point);
        } else {
            active_points.push(point);
        }
    }

    if inactive_points.len() > 1 {
        painter.add(egui::epaint::PathShape::line(
            inactive_points,
            egui::Stroke::new(PLAY_RING_WIDTH, inactive_color),
        ));
    }
    
    if active_points.len() > 1 {
        painter.add(egui::epaint::PathShape::line(
            active_points,
            egui::Stroke::new(PLAY_RING_WIDTH, active_color),
        ));
    }
}



