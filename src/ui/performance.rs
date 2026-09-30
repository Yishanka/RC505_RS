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
    let lang = crate::app_support::language::Language::current(ui.ctx());
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
                        ui.strong(lang.text("FX EDITOR"));
                        theme::keycap(ui, "D / F8");
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
    use theme::Icon;
    let lang = crate::app_support::language::Language::current(ui.ctx());
    let request = app.focus == Focus::Transport && app.focus_request;
    nav::begin(ui, Focus::Transport, request);
    if request {
        app.focus_request = false;
    }
    ui.horizontal(|ui| {
        theme::brand(ui);
        ui.separator();
        theme::caption(ui, lang.text("Top controls"));
        theme::keycap(ui, "F6");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            app.language_switch(ui);
            if nav::register(theme::action(ui, Icon::Help, lang.text("Help"), "F12")).clicked() {
                app.help_open = true;
            }
            if nav::register(theme::action(ui, Icon::Back, lang.text("Projects"), "Esc"))
                .on_hover_text(
                    lang.text("Esc: editor → performance → project browser (save prompt)."),
                )
                .clicked()
            {
                app.back_to_projects();
            }
        });
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("BPM");
        nav::register(
            ui.add_enabled(
                app.stopped(),
                egui::DragValue::new(&mut app.config.beat_config.input_bpm.value)
                    .clamp_range(30..=300)
                    .speed(0.2),
            ),
        );
        ui.add_enabled_ui(app.stopped(), |ui| {
            if nav::register(theme::action(ui, Icon::None, lang.text("Tap tempo"), "T")).clicked() {
                app.config.beat_config.tap_calc.calculate_avg_bpm();
                app.config.beat_config.input_bpm.value = app.config.beat_config.tap_calc.value;
            }
        });
        if nav::register(theme::action(
            ui,
            Icon::Play,
            lang.text("Start / stop all"),
            "Space",
        ))
        .clicked()
        {
            app.toggle_all();
        }
        let menu = ui.menu_button(lang.text("Save"), |ui| {
            if theme::action(ui, Icon::Save, lang.text("Configuration"), "Ctrl+S").clicked() {
                app.save_now();
                ui.close_menu();
            }
            if theme::action(
                ui,
                Icon::Save,
                lang.text("Configuration + audio"),
                "Ctrl+Shift+S",
            )
            .clicked()
            {
                app.save_snapshot();
                ui.close_menu();
            }
        });
        nav::register(menu.response);
        let taking = app.taking();
        ui.add_enabled_ui(!app.busy(), |ui| {
            if nav::register(theme::action(
                ui,
                if taking { Icon::Stop } else { Icon::Record },
                lang.text(if taking { "End take" } else { "Record take" }),
                "F9",
            ))
            .clicked()
            {
                if taking {
                    app.finish_take();
                } else {
                    app.start_take();
                }
            }
        });
    });
    nav::end(ui);
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 24.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            theme::caption(ui, app.project_name());
            ui.separator();
            theme::caption(
                ui,
                match app.focus {
                    Focus::Performance => lang.text("Performance"),
                    Focus::Transport => lang.text("Top controls"),
                    Focus::Left => lang.text("Left controls"),
                    Focus::Right => lang.text("FX controls"),
                },
            );
            let message = if app.status.is_empty() {
                app.audio_status()
            } else {
                format!(
                    "{} · {}: {}",
                    app.status,
                    lang.choose("Output", "输出"),
                    app.audio.curr_output_name()
                )
            };
            ui.add(
                egui::Label::new(
                    egui::RichText::new(lang.text(&message))
                        .size(13.0)
                        .color(theme::MUTED),
                )
                .truncate(true),
            )
            .on_hover_text(lang.text(&message));
        },
    );
}

fn left(ui: &mut egui::Ui, app: &mut MyApp) {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        for (page, label) in [
            (LeftPage::Track, lang.text("Track")),
            (LeftPage::Audio, lang.text("Audio")),
            (LeftPage::Session, lang.text("Session")),
        ] {
            nav::register(ui.selectable_value(&mut app.left_page, page, label));
        }
        theme::keycap(ui, "A / F7");
    });
    ui.separator();
    match app.left_page {
        LeftPage::Track => {
            let index = app.track_sel.unwrap_or(0);
            ui.strong(format!("{} {}", lang.text("Track"), index + 1));
            let options = &mut app.config.track_options[index];
            ui.add_enabled_ui(
                !matches!(app.view.tracks[index].mode, Mode::Recording | Mode::Overdub),
                |ui| {
                    ui.horizontal_wrapped(|ui| {
                        nav::register(ui.checkbox(&mut options.reverse, lang.text("Reverse")));
                        nav::register(ui.checkbox(&mut options.one_shot, lang.text("One shot")));
                    });
                },
            );
            ui.horizontal_wrapped(|ui| {
                ui.label(lang.text("Stop"));
                parameters::selector(
                    ui,
                    "stop-mode",
                    &mut options.stop_mode,
                    &[
                        (StopMode::Immediate, lang.text("Immediate")),
                        (StopMode::LoopEnd, lang.text("Loop end")),
                        (StopMode::Fade, lang.text("Fade out")),
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
            ui.horizontal_wrapped(|ui|{
                ui.label(lang.text("Quantize"));parameters::selector(ui,"quantize",&mut options.quantize,&[(Quantize::Off,lang.text("Off")),(Quantize::Beat,lang.text("Beat")),(Quantize::Measure,lang.text("Measure")),(Quantize::Loop,lang.text("Loop"))]);
                ui.label(lang.text("Length"));nav::register(ui.add(egui::DragValue::new(&mut options.measures).clamp_range(0..=128).suffix(" bars"))).on_hover_text(lang.text("0 = finish manually; 1–128 = fixed length in 4/4. Maximum audio length is five minutes."));
            });
            theme::caption(
                ui,
                lang.text("0 bars = manual finish · Reverse / One shot disable overdub"),
            );
            ui.horizontal_wrapped(|ui| {
                let redo = app.view.tracks[index].redo;
                ui.add_enabled_ui(app.view.tracks[index].undo || redo, |ui| {
                    if nav::register(theme::action(
                        ui,
                        theme::Icon::None,
                        lang.text(if redo { "Redo" } else { "Undo" }),
                        if redo { "Ctrl+Y" } else { "Ctrl+Z" },
                    ))
                    .clicked()
                    {
                        app.undo_track(index);
                    }
                });
                clear_button(ui, app, index);
                if nav::button(ui, lang.text("Expand selected FX")).clicked() {
                    app.editor.expanded = true;
                }
            });
        }
        LeftPage::Audio => {
            let mut follow = app.config.system_config.follow_system_output;
            if nav::register(ui.checkbox(
                &mut follow,
                lang.choose("Follow system output", "跟随系统输出设备"),
            ))
            .changed()
            {
                app.set_follow_output(follow);
            }
            ui.label(format!(
                "{}: {}",
                lang.choose("Active output", "实际输出"),
                if app.audio.online {
                    app.audio.curr_output_name()
                } else {
                    lang.choose("Unavailable", "尚未连接")
                }
            ));
            parameters::choice(ui, &mut app.config.system_config.input_device);
            ui.add_enabled_ui(!follow, |ui| {
                parameters::choice(ui, &mut app.config.system_config.output_device)
            });
            if nav::button(ui, lang.choose("Refresh devices", "刷新设备列表")).clicked() {
                app.config.system_config.refresh();
            }
            ui.horizontal_wrapped(|ui| {
                ui.label(lang.text("Buffer"));
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
                    egui::Button::new(lang.text("Reconnect")),
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
                lang.text("Loopback: connect output L to input L; disconnect speakers. Monitoring is muted during probes."),
            );
            ui.horizontal_wrapped(|ui| {
                if nav::register(ui.add_enabled(
                    app.stopped() && app.audio.online && !app.taking(),
                    egui::Button::new(lang.text("Measure loopback")),
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
            ui.horizontal_wrapped(|ui| {
                if nav::button(ui, lang.text("Save configuration")).clicked() {
                    app.save_now();
                }
                if nav::button(ui, lang.text("Save audio snapshot")).clicked() {
                    app.save_snapshot();
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(lang.text("Input FX order"));
                nav::register(
                    egui::ComboBox::from_id_source("routing")
                        .selected_text(match app.config.input_routing {
                            InputRouting::Legacy => lang.text("Legacy groups"),
                            InputRouting::Serial => lang.text("Slot A → D"),
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut app.config.input_routing,
                                InputRouting::Legacy,
                                lang.text("Legacy groups"),
                            );
                            ui.selectable_value(
                                &mut app.config.input_routing,
                                InputRouting::Serial,
                                lang.text("Slot A → D"),
                            );
                        })
                        .response,
                );
            });
            if nav::button(ui, lang.text("Replay library & import")).clicked() {
                app.replay_browser = true;
            }
            if nav::button(ui, lang.text("Signal flow and operation guide")).clicked() {
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
    let lang = crate::app_support::language::Language::current(ui.ctx());
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
                if track_fx {
                    lang.text("TRACK FX")
                } else {
                    lang.text("INPUT FX")
                },
            );
            for i in 0..4 {
                ui.selectable_value(&mut bank, i, format!("{}", i + 1));
            }
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
                            format!("{} {}", ['A', 'B', 'C', 'D'][slot], lang.text(name)),
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
                column.horizontal(|ui| {
                    ui.add_enabled(
                        name != "Empty",
                        egui::Checkbox::new(enabled, lang.text("On")),
                    );
                    theme::keycap(
                        ui,
                        if track_fx {
                            ["U", "I", "O", "P"][slot]
                        } else {
                            ["Q", "W", "E", "R"][slot]
                        },
                    );
                });
            }
        });
        theme::caption(ui, lang.text("Shift: hold effect · Alt: bank · Ctrl: edit"));
    });
}
fn track(ui: &mut egui::Ui, app: &mut MyApp, index: usize) {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    let view = app.view.tracks[index];
    let selected = app.track_sel == Some(index);
    let (state, color) = match view.mode {
        Mode::Empty => (lang.text("EMPTY"), theme::MUTED),
        Mode::Recording => (lang.text("RECORDING"), Color32::from_rgb(255, 109, 118)),
        Mode::Overdub => (lang.text("OVERDUB"), Color32::from_rgb(255, 196, 106)),
        Mode::Playing => (lang.text("PLAYING"), theme::ACCENT),
        Mode::Stopped => (lang.text("STOPPED"), theme::MUTED),
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
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.spacing_mut().button_padding.x = 6.0;
            if ui
                .selectable_label(
                    selected,
                    egui::RichText::new(format!("{} {}", lang.text("Track"), index + 1)).strong(),
                )
                .clicked()
            {
                app.track_sel = Some(index);
            }
            ui.label(
                egui::RichText::new(if view.pending {
                    lang.text("QUEUED")
                } else {
                    state
                })
                .size(13.0)
                .color(color),
            );
            let (rect, _) = ui
                .allocate_exact_size(egui::vec2(ui.available_width(), 36.0), egui::Sense::hover());
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
            ui.horizontal(|ui| {
                ui.label(format!("{level:.1} dB"));
                theme::keycap(ui, keys);
            });
            ui.horizontal(|ui| {
                ui.label(lang.text("Speed"));
                ui.add(
                    egui::DragValue::new(&mut app.config.track_options[index].fader_speed)
                        .clamp_range(1.0..=60.0)
                        .speed(0.25)
                        .suffix(" dB/s"),
                );
            });
            theme::keycap(ui, &format!("Shift + {keys}"));
            let action = match view.mode {
                Mode::Empty => lang.text("Record"),
                Mode::Stopped => lang.text("Play"),
                Mode::Recording | Mode::Overdub => lang.text("Finish"),
                Mode::Playing if app.config.track_options[index].one_shot => lang.text("Retrigger"),
                _ => lang.text("Overdub"),
            };
            let icon = match view.mode {
                Mode::Empty | Mode::Playing => theme::Icon::Record,
                Mode::Stopped => theme::Icon::Play,
                _ => theme::Icon::Stop,
            };
            if theme::action(ui, icon, lang.text(action), &format!("{}", index + 1)).clicked() {
                app.trigger_track(index);
            }
            ui.horizontal(|ui| {
                if theme::action(
                    ui,
                    theme::Icon::None,
                    lang.text("Stop"),
                    &format!("F{}", index + 1),
                )
                .on_hover_text(format!("Shift+{}", index + 1))
                .clicked()
                {
                    app.pause_track(index);
                }
                ui.add_enabled_ui(view.undo || view.redo, |ui| {
                    if theme::action(
                        ui,
                        if view.redo {
                            theme::Icon::Redo
                        } else {
                            theme::Icon::Undo
                        },
                        "",
                        "",
                    )
                    .on_hover_text(format!(
                        "{} · Alt+{}",
                        lang.text(if view.redo { "Redo" } else { "Undo" }),
                        index + 1
                    ))
                    .clicked()
                    {
                        app.undo_track(index);
                    }
                });
            });
            if selected {
                ui.add(
                    egui::ProgressBar::new(app.clear_gesture.progress)
                        .desired_height(4.0)
                        .fill(Color32::LIGHT_RED),
                );
            } else {
                ui.add_space(8.0);
            }
        });
}

fn clear_button(ui: &mut egui::Ui, app: &mut MyApp, index: usize) {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    let response = nav::register(theme::action(ui, theme::Icon::Trash, lang.text("Clear audio"), "Del"))
        .on_hover_text(format!("{}\n{}", lang.text("Hold Delete for 0.75 s or double-press within 350 ms. Clears the selected track and its undo audio."), lang.text("Click twice within 350 ms, or hold for 0.75 s. Release to cancel a hold.")));
    let id = response.id.with("clear-gesture");
    let mut gesture = ui
        .ctx()
        .data(|d| d.get_temp::<crate::app::clear_gesture::ClearGesture>(id))
        .unwrap_or_default();
    let button_focused = response.has_focus();
    let (now, down, pressed, focused) = ui.input(|i| {
        (
            i.time,
            response.is_pointer_button_down_on() || button_focused && i.key_down(egui::Key::Enter),
            response.contains_pointer() && i.pointer.button_pressed(egui::PointerButton::Primary)
                || button_focused
                    && i.events.iter().any(|e| {
                        matches!(
                            e,
                            egui::Event::Key {
                                key: egui::Key::Enter,
                                pressed: true,
                                repeat: false,
                                ..
                            }
                        )
                    }),
            i.focused,
        )
    });
    if focused {
        if gesture.update(index, down, pressed, now) {
            app.clear_track(index);
        }
    } else {
        gesture.cancel();
    }
    if gesture.progress > 0.0 {
        let mut rect = response.rect;
        rect.set_width(rect.width() * gesture.progress);
        ui.painter()
            .rect_stroke(rect, 4.0, Stroke::new(2.0, Color32::LIGHT_RED));
    }
    ui.ctx().data_mut(|d| d.insert_temp(id, gesture));
}
