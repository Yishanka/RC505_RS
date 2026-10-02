use super::theme;
use crate::app::MyApp;
use eframe::egui;
use std::sync::atomic::Ordering;
pub fn draw(ctx: &egui::Context, app: &mut MyApp) {
    let lang = app.language;
    if app.draft.is_some() {
        egui::Window::new(lang.text("Replay captured")).id(egui::Id::new("replay-draft")).collapsible(false).resizable(false).show(ctx,|ui| {
            ui.label(lang.choose("Save a named replay, export WAV, or keep the recording as a draft in the replay library.","可以命名保存回放、导出 WAV，或保留为回放库中的草稿。"));
            ui.add(egui::TextEdit::singleline(&mut app.take_name).desired_width(380.0));
            ui.add_enabled_ui(!app.busy(),|ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.button(lang.text("Save replay")).clicked(){app.save_take();}
                    if ui.button(lang.text("Export audio")).clicked(){if let Some(path)=app.draft.clone(){app.render_replay(path);app.replay_browser=true;}}
                    if ui.button(lang.text("Keep as draft")).clicked(){app.discard_take();}
                });
            });
        });
    }
    if app.replay_browser {
        let mut open = true;
        let mut close_requested = false;
        egui::Window::new(lang.text("Replay library")).id(egui::Id::new("replay-library")).open(&mut open).default_size([860.0,600.0]).show(ctx,|ui| {
            ui.label(lang.choose("Play renders the recorded inputs and operations, then opens the player automatically. Space pauses/resumes; Esc closes the player.","点击“播放”会重算录下的输入与操作，并自动打开播放器。空格暂停/继续，Esc 关闭播放器。"));
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(!app.busy()&&!app.taking(),egui::Button::new(lang.choose("Record a new replay","录制新回放"))).clicked(){app.toggle_take();close_requested=app.take_pending;}
                if ui.button(lang.choose("Refresh","刷新列表")).clicked(){app.open_replays();}
            });
            ui.separator();
            egui::ScrollArea::vertical().id_source("replay-list").max_height(240.0).min_scrolled_height(100.0).show(ui,|ui| {
                if app.replay_list.is_empty(){ui.label(lang.choose("No saved replay yet. Press F9 to begin; stop track recording before pressing F9 to finish.","还没有回放。按 F9 开始录制，结束轨道录音/叠录后再按 F9 收尾。"));}
                for (path,name) in app.replay_list.clone() {
                    ui.horizontal(|ui| {
                        ui.add_sized([ui.available_width().max(250.0)-240.0,34.0],egui::Label::new(name).truncate(true));
                        ui.add_enabled_ui(!app.busy()&&!app.taking(),|ui| {
                            if theme::action(ui,theme::Icon::Play,lang.text("Play"),"").clicked(){app.play_replay(path.clone());}
                            if ui.button(lang.choose("Export WAV","导出 WAV")).clicked(){app.render_replay(path);}
                        });
                    });
                }
            });
            ui.collapsing(lang.choose("Open a replay from another folder","打开其他目录的回放"),|ui| {
                ui.add(egui::TextEdit::singleline(&mut app.replay_import_path).hint_text(lang.text("Replay folder containing replay.json")).desired_width(530.0));
                if ui.add_enabled(!app.busy()&&!app.taking(),egui::Button::new(lang.choose("Open / play","打开并播放"))).clicked(){app.play_replay(std::path::PathBuf::from(app.replay_import_path.trim()));}
            });
            if app.job.is_some(){ui.horizontal(|ui|{ui.spinner();ui.label(format!("{}: {}",lang.choose("Processed frames","已处理采样帧"),app.render_progress.load(Ordering::Relaxed)));});}
            if let Some((_,result))=&app.rendered {
                let name=result.name.clone();let wav=result.wav.display().to_string();
                let can_source=app.projects.iter().any(|p|p.file==result.project_id);
                ui.separator();ui.strong(name);ui.label(format!("WAV: {wav}"));
                ui.add_enabled_ui(!app.busy()&&!app.taking()&&app.tracks_stopped(),|ui| {
                    if ui.add_enabled(app.audio.online&&!app.calibration_held(),egui::Button::new(lang.choose("Play rendered audio","播放已生成音频"))).clicked(){app.play_rendered();}
                    ui.horizontal_wrapped(|ui| {
                        if ui.button(lang.text("Import into new project")).clicked(){app.import_rendered(true);}
                        if ui.add_enabled(can_source,egui::Button::new(lang.text("Import into source project"))).clicked(){app.import_rendered(false);}
                    });
                    theme::caption(ui,lang.choose("Import restores the final track/config state as a new snapshot; earlier revisions remain.","导入会将回放结束时的轨道与配置保存为新快照，旧版本仍保留。"));
                });
            }
            ui.label(lang.text(&app.status));
        });
        if !open {
            app.replay_autoplay = false;
        }
        app.replay_browser = open && !close_requested;
    }
    if app.player_open {
        egui::Window::new(lang.text("Replay player"))
            .id(egui::Id::new("replay-player"))
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
                if theme::action(
                    ui,
                    if playing {
                        theme::Icon::Stop
                    } else {
                        theme::Icon::Play
                    },
                    lang.text(if playing { "Pause" } else { "Play" }),
                    "Space",
                )
                .clicked()
                {
                    app.send(crate::engine::audio_io::Control::PlayerToggle);
                }
                if theme::action(ui, theme::Icon::Back, lang.text("Close player"), "Esc").clicked()
                {
                    app.close_player();
                }
                ui.label(lang.text("Live input is muted while this player is open."));
            });
    }
}
