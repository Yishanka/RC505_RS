use super::{editor, navigation as nav, parameters, theme};
use crate::{
    app::{Focus, LeftPage, MyApp},
    config::track_options::{InputRouting, Quantize, StopMode},
    engine::core::Mode,
    presets::FxTarget,
};
use eframe::egui::{self, Color32, Stroke};
use std::sync::atomic::Ordering;

pub fn draw(ui: &mut egui::Ui, app: &mut MyApp) {
    transport(ui, app);
    ui.add_space(6.0);
    if app.editor.expanded {
        egui::ScrollArea::vertical()
            .id_source("expanded")
            .show(ui, |ui| editor::draw(ui, app, true));
        return;
    }
    egui::ScrollArea::vertical()
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .id_source("performance")
        .show(ui, |ui| {
            ui.columns(2, |columns| {
                fixed_panel(&mut columns[0], app, Focus::Left, |ui, app| left(ui, app));
                fixed_panel(&mut columns[1], app, Focus::Right, |ui, app| {
                    ui.horizontal(|ui| {
                        ui.strong("FX EDITOR");
                        theme::caption(ui, "D / F8");
                    });
                    editor::draw(ui, app, false);
                });
            });
            ui.add_space(8.0);
            ui.columns(2, |columns| {
                rack(&mut columns[0], app, false);
                rack(&mut columns[1], app, true);
            });
            ui.add_space(8.0);
            ui.columns(5, |columns| {
                for (index, column) in columns.iter_mut().enumerate() {
                    track(column, app, index);
                }
            });
        });
}
fn fixed_panel(
    ui: &mut egui::Ui,
    app: &mut MyApp,
    focus: Focus,
    draw: impl FnOnce(&mut egui::Ui, &mut MyApp),
) {
    let frame = theme::card().inner_margin(12.0).stroke(Stroke::new(
        1.0,
        if app.focus == focus {
            theme::ACCENT
        } else {
            Color32::from_rgb(43, 53, 67)
        },
    ));
    frame.show(ui, |ui| {
        let request = app.focus == focus && app.focus_request;
        nav::begin(ui, focus, request);
        if request {
            app.focus_request = false;
        }
        let height = if ui.ctx().screen_rect().height() < 800.0 {
            120.0
        } else {
            236.0
        };
        egui::ScrollArea::vertical()
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
            .id_source(if focus == Focus::Left {
                "left-controls"
            } else {
                "right-controls"
            })
            .max_height(height)
            .min_scrolled_height(height)
            .auto_shrink([false, false])
            .show(ui, |ui| draw(ui, app));
        nav::end(ui);
    });
}
fn transport(ui: &mut egui::Ui, app: &mut MyApp) {
    let request = app.focus == Focus::Transport && app.focus_request;
    nav::begin(ui, Focus::Transport, request);
    if request {
        app.focus_request = false;
    }
    ui.horizontal_wrapped(|ui| {
        theme::brand(ui);
        ui.separator();
        ui.label("BPM");
        nav::register(
            ui.add_enabled(
                app.stopped(),
                egui::DragValue::new(&mut app.config.beat_config.input_bpm.value)
                    .clamp_range(30..=300)
                    .speed(0.2),
            ),
        );
        if nav::register(ui.add_enabled(app.stopped(), egui::Button::new("Tap T"))).clicked() {
            app.config.beat_config.tap_calc.calculate_avg_bpm();
            app.config.beat_config.input_bpm.value = app.config.beat_config.tap_calc.value;
        }
        if nav::button(ui, "All  Space").clicked() {
            app.toggle_all();
        }
        ui.menu_button("Save", |ui| {
            if ui.button("Configuration  Ctrl+S").clicked() {
                app.save_now();
                ui.close_menu();
            }
            if ui.button("Configuration + audio  Ctrl+Shift+S").clicked() {
                app.save_snapshot();
                ui.close_menu();
            }
        });
        let taking = app.taking();
        if nav::register(
            ui.add_enabled(
                !app.busy(),
                egui::Button::new(if taking {
                    "End take  F9"
                } else {
                    "Record take  F9"
                })
                .fill(if taking {
                    Color32::from_rgb(132, 43, 61)
                } else {
                    Color32::from_rgb(37, 45, 58)
                }),
            ),
        )
        .clicked()
        {
            if taking {
                app.finish_take();
            } else {
                app.start_take();
            }
        }
        if nav::button(ui, "Projects").clicked() {
            app.back_to_projects();
        }
        if nav::button(ui, "Help  F12").clicked() {
            app.help_open = true;
        }
    });
    nav::end(ui);
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 24.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            theme::caption(ui, app.project_name());
            ui.separator();
            let mode = match app.focus {
                Focus::Performance => "Performance",
                Focus::Transport => "Top controls · F6",
                Focus::Left => "Left controls · A / F7",
                Focus::Right => "FX controls · D / F8",
            };
            theme::caption(ui, mode);
            let message = if app.status.is_empty() {
                app.audio_status()
            } else {
                app.status.clone()
            };
            ui.add(
                egui::Label::new(egui::RichText::new(&message).size(13.0).color(theme::MUTED))
                    .truncate(true),
            )
            .on_hover_text(message);
        },
    );
}
fn left(ui: &mut egui::Ui, app: &mut MyApp) {
    ui.horizontal(|ui| {
        for (page, label) in [
            (LeftPage::Track, "Track"),
            (LeftPage::Audio, "Audio"),
            (LeftPage::Session, "Session"),
        ] {
            nav::register(ui.selectable_value(&mut app.left_page, page, label));
        }
        theme::caption(ui, "A / F7");
    });
    ui.separator();
    match app.left_page {
        LeftPage::Track => {
            let index = app.track_sel.unwrap_or(0);
            ui.strong(format!("TRACK {}", index + 1));
            let options = &mut app.config.track_options[index];
            ui.add_enabled_ui(
                !matches!(app.view.tracks[index].mode, Mode::Recording | Mode::Overdub),
                |ui| {
                    ui.horizontal(|ui| {
                        nav::register(ui.checkbox(&mut options.reverse, "Reverse"));
                        nav::register(ui.checkbox(&mut options.one_shot, "One shot"));
                    });
                },
            );
            ui.horizontal(|ui| {
                ui.label("Stop");
                parameters::selector(
                    ui,
                    "stop-mode",
                    &mut options.stop_mode,
                    &[
                        (StopMode::Immediate, "Immediate"),
                        (StopMode::LoopEnd, "Loop end"),
                        (StopMode::Fade, "Fade out"),
                    ],
                );
                nav::register(
                    ui.add_enabled(
                        options.stop_mode == StopMode::Fade,
                        egui::DragValue::new(&mut options.fade_ms)
                            .clamp_range(10..=30_000)
                            .suffix(" ms"),
                    ),
                );
            });
            ui.horizontal(|ui|{
                ui.label("Quantize");parameters::selector(ui,"quantize",&mut options.quantize,&[(Quantize::Off,"Off"),(Quantize::Beat,"Beat"),(Quantize::Measure,"Measure"),(Quantize::Loop,"Loop")]);
                ui.label("Length");nav::register(ui.add(egui::DragValue::new(&mut options.measures).clamp_range(0..=128).suffix(" bars"))).on_hover_text("0 = finish manually; 1–128 = fixed length in 4/4. Maximum audio length is five minutes.");
            });
            theme::caption(
                ui,
                "0 bars = manual finish · Reverse / One shot disable overdub",
            );
            ui.horizontal(|ui| {
                if nav::register(ui.add_enabled(
                    app.view.tracks[index].undo || app.view.tracks[index].redo,
                    egui::Button::new(if app.view.tracks[index].redo {
                        "Redo  Ctrl+Y"
                    } else {
                        "Undo  Ctrl+Z"
                    }),
                ))
                .clicked()
                {
                    app.undo_track(index);
                }
                if nav::button(ui, "Expand selected FX").clicked() {
                    app.editor.expanded = true;
                }
            });
        }
        LeftPage::Audio => {
            parameters::choice(ui, &mut app.config.system_config.input_device);
            parameters::choice(ui, &mut app.config.system_config.output_device);
            ui.horizontal(|ui| {
                ui.label("Buffer");
                nav::register(
                    egui::ComboBox::from_id_source("buffer-frames")
                        .selected_text(format!("{} frames", app.buffer_frames))
                        .show_ui(ui, |ui| {
                            for frames in [64, 128, 256, 512, 1024] {
                                ui.selectable_value(
                                    &mut app.buffer_frames,
                                    frames,
                                    format!("{frames}"),
                                );
                            }
                        })
                        .response,
                );
                if nav::register(ui.add_enabled(
                    app.stopped() && !app.busy() && !app.taking(),
                    egui::Button::new("Reconnect"),
                ))
                .clicked()
                {
                    app.reconnect();
                }
            });
            ui.add_enabled_ui(app.stopped(), |ui| {
                parameters::number(ui, &mut app.config.beat_config.input_latency, 0, 500, false)
            });
            let d = &app.audio.diagnostics;
            theme::caption(
                ui,
                format!(
                    "Input gaps {} · overflow {} · errors {} · peak callback {:.2} ms",
                    d.underrun.load(Ordering::Relaxed),
                    d.overflow.load(Ordering::Relaxed),
                    d.stream_errors.load(Ordering::Relaxed),
                    d.maximum_callback_ns.load(Ordering::Relaxed) as f64 / 1e6
                ),
            );
            theme::caption(
                ui,
                "Loopback: connect output L to input L; disconnect speakers. Monitoring is muted during probes.",
            );
            ui.horizontal(|ui| {
                if nav::register(ui.add_enabled(
                    app.stopped() && app.audio.online && !app.taking(),
                    egui::Button::new("Measure loopback"),
                ))
                .clicked()
                {
                    app.calibrate();
                }
                if let Some(value) = app.measurement {
                    if nav::button(
                        ui,
                        format!(
                            "Apply {:.3} ms",
                            value.frames as f64 * 1000.0 / value.sample_rate as f64
                        ),
                    )
                    .clicked()
                    {
                        app.apply_measurement();
                    }
                }
            });
        }
        LeftPage::Session => {
            ui.strong(app.project_name());
            ui.horizontal(|ui| {
                if nav::button(ui, "Save configuration").clicked() {
                    app.save_now();
                }
                if nav::button(ui, "Save audio snapshot").clicked() {
                    app.save_snapshot();
                }
            });
            ui.horizontal(|ui| {
                ui.label("Input FX order");
                nav::register(
                    egui::ComboBox::from_id_source("routing")
                        .selected_text(match app.config.input_routing {
                            InputRouting::Legacy => "Legacy groups",
                            InputRouting::Serial => "Slot A → D",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut app.config.input_routing,
                                InputRouting::Legacy,
                                "Legacy groups",
                            );
                            ui.selectable_value(
                                &mut app.config.input_routing,
                                InputRouting::Serial,
                                "Slot A → D",
                            );
                        })
                        .response,
                );
            });
            if nav::button(ui, "Replay library & import").clicked() {
                app.replay_browser = true;
            }
            if nav::button(ui, "Signal flow and operation guide").clicked() {
                app.help_open = true;
                app.help_tab = 1;
            }
            let d = &app.audio.diagnostics;
            theme::caption(
                ui,
                format!(
                    "Input {:.1} dB · output {:.1} dB · clipped frames {}",
                    db(app.view.input_peak),
                    db(app.view.output_peak),
                    app.view.clipped
                ),
            );
            theme::caption(
                ui,
                format!(
                    "Audio clock {} frames · queued input {}",
                    app.view.frame,
                    d.queue_frames.load(Ordering::Relaxed)
                ),
            );
        }
    }
}
fn db(value: f32) -> f32 {
    20.0 * value.max(1e-5).log10()
}
fn rack(ui: &mut egui::Ui, app: &mut MyApp, track_fx: bool) {
    theme::card().inner_margin(10.0).show(ui, |ui| {
        let mut bank = if track_fx {
            app.config.track_fx.sel_bank_idx
        } else {
            app.config.input_fx.sel_bank_idx
        };
        ui.horizontal(|ui| {
            ui.colored_label(
                if track_fx {
                    theme::TRACK
                } else {
                    theme::ACCENT
                },
                if track_fx { "TRACK FX" } else { "INPUT FX" },
            );
            for i in 0..4 {
                ui.selectable_value(&mut bank, i, format!("{}", i + 1));
            }
            theme::caption(ui, if track_fx { "U I O P" } else { "Q W E R" });
        });
        if track_fx {
            app.config.track_fx.select_bank(bank);
        } else {
            app.config.input_fx.select_bank(bank);
        }
        ui.columns(4, |columns| {
            for (slot, column) in columns.iter_mut().enumerate() {
                let target = if track_fx {
                    FxTarget::Track { bank, slot }
                } else {
                    FxTarget::Input { bank, slot }
                };
                let name = if track_fx {
                    editor::track_name(app.config.track_fx.slot_kind(bank, slot))
                } else {
                    editor::input_name(app.config.input_fx.slot_kind(bank, slot))
                };
                if column
                    .add_sized(
                        [column.available_width(), 29.0],
                        egui::SelectableLabel::new(
                            app.editor.target == Some(target),
                            format!("{} {name}", ['A', 'B', 'C', 'D'][slot]),
                        ),
                    )
                    .clicked()
                {
                    app.editor.select(target);
                }
                let index = app.track_sel.unwrap_or(0);
                let enabled = if track_fx {
                    &mut app.config.track_fx.tracks[index].enabled[bank][slot]
                } else {
                    &mut app.config.input_fx.banks[bank].slots[slot].is_enabled
                };
                column.add_enabled(name != "Empty", egui::Checkbox::new(enabled, "On"));
            }
        });
        theme::caption(ui, "Shift: hold effect · Alt: bank · Ctrl: edit");
    });
}
fn track(ui: &mut egui::Ui, app: &mut MyApp, index: usize) {
    let view = app.view.tracks[index];
    let selected = app.track_sel == Some(index);
    let (state, color) = match view.mode {
        Mode::Empty => ("EMPTY", theme::MUTED),
        Mode::Recording => ("RECORDING", Color32::from_rgb(255, 109, 118)),
        Mode::Overdub => ("OVERDUB", Color32::from_rgb(255, 196, 106)),
        Mode::Playing => ("PLAYING", theme::ACCENT),
        Mode::Stopped => ("STOPPED", theme::MUTED),
    };
    theme::card()
        .inner_margin(10.0)
        .stroke(Stroke::new(
            if selected { 2.0 } else { 1.0 },
            if selected {
                theme::TRACK
            } else {
                Color32::from_gray(48)
            },
        ))
        .show(ui, |ui| {
            if ui.ctx().screen_rect().height() < 800.0 {
                ui.spacing_mut().item_spacing.y = 4.0;
            }
            if ui
                .selectable_label(
                    selected,
                    egui::RichText::new(format!("TRACK {}", index + 1)).strong(),
                )
                .clicked()
            {
                app.track_sel = Some(index);
            }
            ui.label(
                egui::RichText::new(if view.pending { "QUEUED" } else { state })
                    .size(13.0)
                    .color(color),
            );
            let (rect, _) = ui
                .allocate_exact_size(egui::vec2(ui.available_width(), 48.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 5.0, theme::BACKGROUND);
            for (bin, amplitude) in view.wave.iter().enumerate() {
                let x = rect.left() + bin as f32 / 24.0 * rect.width();
                let h = amplitude.min(1.0) * rect.height() * 0.45;
                ui.painter().vline(
                    x,
                    rect.center().y - h..=rect.center().y + h,
                    Stroke::new(2.0, color),
                );
            }
            if view.frames > 0 {
                ui.painter().vline(
                    rect.left() + view.cursor as f32 / view.frames as f32 * rect.width(),
                    rect.y_range(),
                    Stroke::new(1.5, Color32::WHITE),
                );
            }
            theme::caption(
                ui,
                format!("{:.2} s", view.frames as f64 / app.view.sample_rate as f64),
            );
            ui.spacing_mut().slider_width = (ui.available_width() - 4.0).max(40.0);
            let mut level = crate::app::faders::decibels(app.config.track_levels[index]);
            if ui
                .add(egui::Slider::new(&mut level, -60.0..=0.0).show_value(false))
                .changed()
            {
                app.config.track_levels[index] = crate::app::faders::gain(level);
            }
            let keys = ["Z / X", "C / V", "B / N", "M / ,", ". / /"][index];
            ui.label(format!("{level:.1} dB   {keys}"));
            ui.horizontal(|ui| {
                ui.label("Speed");
                ui.add(
                    egui::DragValue::new(&mut app.config.track_options[index].fader_speed)
                        .clamp_range(1.0..=60.0)
                        .speed(0.25)
                        .suffix(" dB/s"),
                );
            });
            theme::caption(ui, format!("Shift + {keys}: speed"));
            let action = match view.mode {
                Mode::Empty => "Record",
                Mode::Stopped => "Play",
                Mode::Recording | Mode::Overdub => "Finish",
                Mode::Playing if app.config.track_options[index].one_shot => "Retrigger",
                _ => "Overdub",
            };
            if ui
                .add_sized(
                    [ui.available_width(), 34.0],
                    egui::Button::new(format!("{action}  {}", index + 1)),
                )
                .clicked()
            {
                app.trigger_track(index);
            }
            ui.horizontal(|ui| {
                if ui.button(format!("Stop  F{}", index + 1)).clicked() {
                    app.pause_track(index);
                }
                if ui
                    .add_enabled(
                        view.undo || view.redo,
                        egui::Button::new(if view.redo { "Redo" } else { "Undo" }),
                    )
                    .clicked()
                {
                    app.undo_track(index);
                }
            });
            if selected && app.clear_held > 0.0 && app.clear_held < 1.0 {
                ui.painter()
                    .rect_stroke(rect, 5.0, Stroke::new(2.0, Color32::LIGHT_RED));
            }
        });
}
