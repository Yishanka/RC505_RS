use crate::app::MyApp;
use eframe::egui;
use std::sync::atomic::Ordering;

pub fn draw(ctx: &egui::Context, app: &mut MyApp) {
    if app.draft.is_some() {
        egui::Window::new("Replay captured")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Name this take, export audio, or leave it in draft history.");
                ui.add(egui::TextEdit::singleline(&mut app.take_name).desired_width(380.0));
                ui.add_enabled_ui(!app.busy(), |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Save replay").clicked() {
                            app.save_take();
                        }
                        if ui.button("Export audio").clicked() {
                            if let Some(path) = app.draft.clone() {
                                app.render_replay(path);
                                app.replay_browser = true;
                            }
                        }
                        if ui.button("Keep as draft").clicked() {
                            app.discard_take();
                        }
                    });
                });
            });
    }
    if app.replay_browser {
        let mut open = true;
        egui::Window::new("Replay library").open(&mut open).default_size([760.0,520.0]).show(ctx,|ui|{
            ui.horizontal(|ui|{
                ui.add(egui::TextEdit::singleline(&mut app.replay_import_path).hint_text("Replay folder containing replay.json").desired_width(460.0));
                if ui.add_enabled(!app.busy(),egui::Button::new("Open folder")).clicked(){app.render_replay(std::path::PathBuf::from(app.replay_import_path.trim()));}
            });
            ui.separator();
            egui::ScrollArea::vertical().max_height(200.0).show(ui,|ui|{
                for (path,name) in app.replay_list.clone(){ui.horizontal(|ui|{ui.label(name);if ui.add_enabled(!app.busy(),egui::Button::new("Render / open")).clicked(){app.render_replay(path);}});}
            });
            if app.job.is_some(){ui.spinner();ui.label(format!("Processed {} frames",app.render_progress.load(Ordering::Relaxed)));}
            if let Some((root,result))=&app.rendered {
                let source=crate::replay::info(root).ok().map(|v|v.project_id);
                let can_source=source.as_ref().is_some_and(|id|app.projects.iter().any(|p|&p.file==id));
                ui.separator();ui.label(format!("Exported WAV: {}",result.wav.display()));
                ui.add_enabled_ui(!app.busy()&&app.stopped()&&!app.taking(),|ui|{
                    if ui.add_enabled(app.audio.online,egui::Button::new("Open independent player")).clicked(){app.play_rendered();}
                    ui.horizontal(|ui|{
                        if ui.button("Import into new project").clicked(){app.import_rendered(true);}
                        if ui.add_enabled(can_source,egui::Button::new("Import into source project")).on_hover_text("Creates a new snapshot revision in the source project; previous revisions remain. Opens the resulting project.").clicked(){app.import_rendered(false);}
                    });
                });
            }
            ui.label(&app.status);
        });
        app.replay_browser = open;
    }
    if app.player_open {
        egui::Window::new("Replay player")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                let playing = app.audio.diagnostics.player_playing.load(Ordering::Relaxed);
                ui.label(format!(
                    "{:.2} s",
                    app.audio.diagnostics.player_frame.load(Ordering::Relaxed) as f64
                        / app.audio.config.sample_rate.0 as f64
                ));
                if ui
                    .button(if playing {
                        "Pause  Space"
                    } else {
                        "Play  Space"
                    })
                    .clicked()
                {
                    app.send(crate::engine::audio_io::Control::PlayerToggle);
                }
                if ui.button("Close player  Esc").clicked() {
                    app.close_player();
                }
                ui.label("Live input is muted while this player is open.");
            });
    }
}
