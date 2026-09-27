use crate::app::MyApp;
use eframe::egui;
use std::sync::atomic::Ordering;

pub fn draw(ctx: &egui::Context, app: &mut MyApp) {
    let lang = crate::app_support::language::Language::current(ctx);
    if app.draft.is_some() {
        egui::Window::new(lang.text("Replay captured"))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(lang.text("Name this take, export audio, or leave it in draft history."));
                ui.add(egui::TextEdit::singleline(&mut app.take_name).desired_width(380.0));
                ui.add_enabled_ui(!app.busy(), |ui| {
                    ui.horizontal(|ui| {
                        if ui.button(lang.text("Save replay")).clicked() {
                            app.save_take();
                        }
                        if ui.button(lang.text("Export audio")).clicked() {
                            if let Some(path) = app.draft.clone() {
                                app.render_replay(path);
                                app.replay_browser = true;
                            }
                        }
                        if ui.button(lang.text("Keep as draft")).clicked() {
                            app.discard_take();
                        }
                    });
                });
            });
    }
    if app.replay_browser {
        let mut open = true;
        egui::Window::new(lang.text("Replay library")).open(&mut open).default_size([760.0,520.0]).show(ctx,|ui|{
            ui.horizontal(|ui|{
                ui.add(egui::TextEdit::singleline(&mut app.replay_import_path).hint_text(lang.text("Replay folder containing replay.json")).desired_width(460.0));
                if ui.add_enabled(!app.busy(),egui::Button::new(lang.text("Open folder"))).clicked(){app.render_replay(std::path::PathBuf::from(app.replay_import_path.trim()));}
            });
            ui.separator();
            egui::ScrollArea::vertical().max_height(200.0).show(ui,|ui|{
                for (path,name) in app.replay_list.clone(){ui.horizontal(|ui|{ui.label(name);if ui.add_enabled(!app.busy(),egui::Button::new(lang.text("Render / open"))).clicked(){app.render_replay(path);}});}
            });
            if app.job.is_some(){ui.spinner();ui.label(format!("Processed {} frames",app.render_progress.load(Ordering::Relaxed)));}
            if let Some((root,result))=&app.rendered {
                let source=crate::replay::info(root).ok().map(|v|v.project_id);
                let can_source=source.as_ref().is_some_and(|id|app.projects.iter().any(|p|&p.file==id));
                ui.separator();ui.label(format!("Exported WAV: {}",result.wav.display()));
                ui.add_enabled_ui(!app.busy()&&app.stopped()&&!app.taking(),|ui|{
                    if ui.add_enabled(app.audio.online,egui::Button::new(lang.text("Open independent player"))).clicked(){app.play_rendered();}
                    ui.horizontal(|ui|{
                        if ui.button(lang.text("Import into new project")).clicked(){app.import_rendered(true);}
                        if ui.add_enabled(can_source,egui::Button::new(lang.text("Import into source project"))).on_hover_text(lang.text("Creates a new snapshot revision in the source project; previous revisions remain. Opens the resulting project.")).clicked(){app.import_rendered(false);}
                    });
                });
            }
            ui.label(lang.text(&app.status));
        });
        app.replay_browser = open;
    }
    if app.player_open {
        egui::Window::new(lang.text("Replay player"))
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
                if super::theme::action(
                    ui,
                    if playing {
                        super::theme::Icon::Stop
                    } else {
                        super::theme::Icon::Play
                    },
                    lang.text(if playing { "Pause" } else { "Play" }),
                    "Space",
                )
                .clicked()
                {
                    app.send(crate::engine::audio_io::Control::PlayerToggle);
                }
                if super::theme::action(
                    ui,
                    super::theme::Icon::Back,
                    lang.text("Close player"),
                    "Esc",
                )
                .clicked()
                {
                    app.close_player();
                }
                ui.label(lang.text("Live input is muted while this player is open."));
            });
    }
}
