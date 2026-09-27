use super::{editor, parameters, theme};
use crate::{
    app::MyApp,
    presets::FxTarget,
    state::{AppState, FxState, TrackState},
};
use eframe::egui::{self, Color32, Stroke};

pub fn draw(ui: &mut egui::Ui, app: &mut MyApp) {
    transport(ui, app);
    ui.add_space(6.0);
    if app.editor.expanded {
        egui::ScrollArea::vertical()
            .id_source("expanded_workspace")
            .show(ui, |ui| editor::draw(ui, app, true));
        return;
    }
    egui::ScrollArea::vertical().id_source("performance_workspace").show(ui, |ui| {
        if ui.available_width() >= 1100.0 {
            quick_area(ui,app);
        } else {
            egui::CollapsingHeader::new("Quick parameters & settings").show(ui,|ui| quick_area(ui,app));
        }
        ui.add_space(8.0);
        ui.columns(2,|columns| { rack(&mut columns[0],app,false); rack(&mut columns[1],app,true); });
        ui.add_space(8.0);
        app.refresh_waveforms();
        // Columns follow available width, unlike the old fixed 1000 px panel.
        ui.columns(5,|columns| {
            for (index,column) in columns.iter_mut().enumerate() { track(column,app,index); }
        });
        ui.add_space(6.0);
        theme::caption(ui,"Signal: Input + Oscillator → MyDelay → Vocoder → Filter / Reverb → recording / monitor | loop → Track FX → fader → master");
    });
}

fn quick_area(ui: &mut egui::Ui, app: &mut MyApp) {
    ui.columns(2,|columns| {
            if app.app_state == AppState::MainScreen {
                egui::ScrollArea::horizontal().id_source("legacy_screen").show(&mut columns[0],|ui| super::compact::draw_screen(ui,app));
            } else {
                theme::card().show(&mut columns[0],|ui| {
                    theme::caption(ui,"PERFORMANCE / 5 TRACK LOOP STATION");
                    ui.heading(app.project_name());
                    ui.add_space(6.0);
                    let phase = app.metronome.beat_phase(std::time::Instant::now()).unwrap_or(0.0);
                    ui.add(egui::ProgressBar::new(phase).fill(theme::ACCENT));
                    theme::caption(ui,if app.metronome.start_time().is_some() {"CLOCK RUNNING"} else {"READY TO RECORD"});
                    theme::caption(ui,"1–5 Record / Play / Dub    F1–F5 Stop    Space All start / stop");
                    theme::caption(ui,"Select an FX slot to edit it here, or expand for visual tools.");
                    ui.add_space(8.0);
                    egui::CollapsingHeader::new("Keyboard faders & shortcuts").show(ui,|ui| {
                        ui.add(egui::Slider::new(&mut app.config.fader_speed_db,6.0..=60.0).text("Hold speed (dB/s)"));
                        theme::caption(ui,"Down / up: Z X | C V | B N | M , | . / | tracks 1 to 5");
                        theme::caption(ui,"Tap 0.5 dB; hold accelerates after 180 ms. Shift: tap 0.1 dB, hold at 1/8 speed. Both keys: hold level.");
                        theme::caption(ui,"Q W E R: Input FX | U I O P: selected Track FX | T: bank / slot keys | Left / Right: select track");
                        theme::caption(ui,"Typing, expanded editors and inactive windows suspend performance keys.");
                    });
                    egui::CollapsingHeader::new("Audio devices & latency").show(ui,|ui| {
                        parameters::choice(ui,&mut app.config.system_config.input_device);
                        parameters::choice(ui,&mut app.config.system_config.output_device);
                        parameters::number(ui,&mut app.config.beat_config.input_latency,0,500,false);
                        theme::caption(ui,"Changing devices or latency restarts the audio streams.");
                    });
                });
            }
            editor::draw(&mut columns[1],app,false);
        });
}

fn transport(ui: &mut egui::Ui, app: &mut MyApp) {
    ui.horizontal_wrapped(|ui| {
        theme::brand(ui);
        ui.separator();
        let locked = app.metronome.start_time().is_some();
        ui.label("BPM");
        ui.add_enabled(!locked,egui::DragValue::new(&mut app.config.beat_config.input_bpm.value).clamp_range(30..=300).speed(0.2))
            .on_hover_text("Stop every track and preview before changing tempo. Recorded audio is not time-stretched.");
        if ui.add_enabled(!locked,egui::Button::new("Tap")).clicked() {
            app.config.beat_config.tap_calc.calculate_avg_bpm();
            app.config.beat_config.input_bpm.value = app.config.beat_config.tap_calc.value;
        }
        if ui.button("All start / stop").clicked() { app.toggle_all(); }
        if ui.button("Save  Ctrl+S").clicked() { app.save_now(); }
        if ui.button("Projects").clicked() { app.back_to_projects(); }
        if ui.selectable_label(app.app_state==AppState::MainScreen,"Legacy keys  S").clicked() {
            app.app_state = if app.app_state==AppState::MainScreen {AppState::MainLoop} else {AppState::MainScreen};
        }
    });
    theme::caption(ui, app.audio_status());
    if !app.status.is_empty() {
        theme::caption(ui, &app.status);
    }
}

fn rack(ui: &mut egui::Ui, app: &mut MyApp, track_fx: bool) {
    theme::card().show(ui, |ui| {
        let mut bank = if track_fx {
            app.config.track_fx.sel_bank_idx
        } else {
            app.config.input_fx.sel_bank_idx
        };
        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new(if track_fx { "TRACK FX" } else { "INPUT FX" })
                    .color(if track_fx {
                        theme::TRACK
                    } else {
                        theme::ACCENT
                    })
                    .strong(),
            );
            for index in 0..4 {
                ui.selectable_value(&mut bank, index, format!("Bank {}", index + 1));
            }
        });
        if track_fx {
            app.config.track_fx.select_bank(bank);
        } else {
            app.config.input_fx.select_bank(bank);
        }
        if track_fx {
            theme::caption(
                ui,
                format!(
                    "U I O P: {} / TRACK {}",
                    if app.fx_state == FxState::Single {
                        "enable slots"
                    } else {
                        "select banks"
                    },
                    app.track_sel.unwrap_or(0) + 1
                ),
            );
        } else {
            theme::caption(
                ui,
                if app.fx_state == FxState::Single {
                    "Q W E R = enable slots   •   T switches bank keys"
                } else {
                    "Q W E R = select banks   •   T switches slot keys"
                },
            );
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
                let selected = app.editor.target == Some(target);
                if column
                    .add_sized(
                        [column.available_width(), 40.0],
                        egui::SelectableLabel::new(
                            selected,
                            format!("{}  {}", ['A', 'B', 'C', 'D'][slot], name),
                        ),
                    )
                    .clicked()
                {
                    app.editor.select(target);
                }
                let index = app.track_sel.unwrap_or(0);
                let mut enabled = if track_fx {
                    app.config.track_fx.slot_enabled(index, bank, slot)
                } else {
                    app.config.input_fx.banks[bank].slots[slot].is_enabled
                };
                if column
                    .add_enabled(name != "Empty", egui::Checkbox::new(&mut enabled, "On"))
                    .changed()
                {
                    if track_fx {
                        app.config.track_fx.toggle_slot_enabled(index, slot);
                    } else {
                        app.config.input_fx.toggle_slot_enabled(slot);
                    }
                }
            }
        });
    });
}

fn track(ui: &mut egui::Ui, app: &mut MyApp, index: usize) {
    let state = app.tracks[index].track_state;
    let selected = app.track_sel == Some(index);
    let (label, color) = match state {
        TrackState::Empty => ("EMPTY", theme::MUTED),
        TrackState::Record => ("RECORDING", Color32::from_rgb(255, 109, 118)),
        TrackState::Dub => ("OVERDUB", Color32::from_rgb(255, 196, 106)),
        TrackState::Pause => ("STOPPED", theme::MUTED),
        TrackState::Play => ("PLAYING", theme::ACCENT),
        TrackState::NxtPlay => ("FINISHING", Color32::from_rgb(255, 196, 106)),
    };
    let frame = theme::card().inner_margin(12.0).stroke(Stroke::new(
        if selected { 2.0 } else { 1.0 },
        if selected {
            theme::TRACK
        } else {
            Color32::from_gray(48)
        },
    ));
    frame.show(ui,|ui| {
        ui.set_width(ui.available_width());
        if ui.selectable_label(selected,egui::RichText::new(format!("TRACK {:02}",index+1)).strong()).clicked() { app.track_sel=Some(index); }
        ui.label(egui::RichText::new(label).small().color(color));
        let (rect,_) = ui.allocate_exact_size(egui::vec2(ui.available_width(),72.0),egui::Sense::hover());
        ui.painter().rect_filled(rect,5.0,theme::BACKGROUND);
        if let Some(wave) = app.waveforms.get(index) {
            for (bin,amplitude) in wave.iter().enumerate() {
                let x = rect.left()+bin as f32 / wave.len() as f32*rect.width();
                let h = amplitude.min(1.0)*rect.height()*0.42;
                ui.painter().vline(x,rect.center().y-h..=rect.center().y+h,Stroke::new(1.0,color));
            }
        }
        let progress = app.tracks[index].track_play_progress(std::time::Instant::now());
        if matches!(state,TrackState::Play|TrackState::NxtPlay|TrackState::Dub) {ui.painter().vline(rect.left()+progress*rect.width(),rect.y_range(),Stroke::new(2.0,Color32::WHITE));}
        let duration = app.tracks[index].track_loop_duration.map(|d|format!("{:.2} s",d.as_secs_f64())).unwrap_or_else(||"—".into());
        theme::caption(ui,duration);
        ui.spacing_mut().slider_width = (ui.available_width()-48.0).max(48.0);
        let mut db = crate::app::faders::decibels(app.config.track_levels[index]);
        if ui.add(egui::Slider::new(&mut db,-60.0..=0.0).text("dB").show_value(false))
            .on_hover_text("Drag or click to set level; keyboard down / up keys can operate several tracks together.").changed() {
            app.config.track_levels[index] = crate::app::faders::gain(db);
        }
        theme::caption(ui,format!("{}   [{} - / {} +]",if app.config.track_levels[index]==0.0 {"MUTE".into()} else {format!("{db:.1} dB")},
            ["Z","C","B","M","."][index],["X","V","N",",","/"][index]));
        let action = match state {TrackState::Empty=>"Record",TrackState::Pause=>"Play",TrackState::Record|TrackState::Dub=>"Finish",TrackState::NxtPlay=>"Finishing...",_=>"Overdub"};
        if ui.add_sized([ui.available_width(),38.0],egui::Button::new(format!("{}  [{}]",action,index+1))).clicked() {app.trigger_track(index);}
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(matches!(state,TrackState::Record|TrackState::Play|TrackState::NxtPlay|TrackState::Dub),egui::Button::new("Stop")).clicked() {app.pause_track(index);}
            if ui.add_enabled(!matches!(state,TrackState::Empty|TrackState::Record|TrackState::Dub),egui::Button::new("Clear")).clicked() {app.clear_track(index);}
        });
    });
}
