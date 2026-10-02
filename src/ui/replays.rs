use super::theme;
use crate::app::MyApp;
use eframe::egui;
use std::sync::atomic::Ordering;
pub fn draw(ctx: &egui::Context, app: &mut MyApp) {
    let lang = app.language;
    if app.draft.is_some() {
        egui::Window::new(lang.text("Replay captured"))
            .id(egui::Id::new("replay-draft"))
            .default_width(580.0)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(lang.choose(
                    "Save or export the replay, keep a draft, or discard this recording.",
                    "可以保存回放、导出音频、保留草稿，或直接丢弃本次录制。",
                ));
                ui.add(egui::TextEdit::singleline(&mut app.take_name).desired_width(380.0));
                ui.add_enabled_ui(!app.busy(), |ui| {
                    theme::control_row(ui, |ui| {
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
                            app.keep_take();
                        }
                        if theme::action(
                            ui,
                            theme::Icon::Trash,
                            lang.choose("Discard recording", "丢弃录制"),
                            "",
                        )
                        .on_hover_text(lang.choose(
                            "Moves to replay trash; restore from the replay library.",
                            "移入回放回收站，可在回放库恢复。",
                        ))
                        .clicked()
                        {
                            app.discard_take();
                        }
                    });
                });
            });
    }
    if app.replay_browser {
        let mut open = true;
        let mut close_requested = false;
        egui::Window::new(lang.text("Replay library")).id(egui::Id::new("replay-library")).open(&mut open).default_size([860.0,600.0]).vscroll(true).show(ctx,|ui| {
            ui.label(lang.choose("Play opens a temporary performance panel and reproduces recorded track/FX states. Space pauses; Esc returns to your live project.","播放会打开临时演奏面板，随进度复现轨道与效果器状态。空格暂停，Esc 返回原工程。"));
            theme::control_row(ui,|ui| {
                if ui.add_enabled(!app.busy()&&!app.taking(),egui::Button::new(lang.choose("Record a new replay","录制新回放"))).clicked(){app.toggle_take();close_requested=app.take_pending;}
                if ui.button(lang.choose("Refresh","刷新列表")).clicked(){app.open_replays();}
                if ui.add_enabled(!app.read_only&&!app.busy(),egui::Button::new(lang.choose("Restore deleted replay","恢复最近删除的回放"))).clicked(){app.restore_replay();}
            });
            ui.separator();
            egui::ScrollArea::vertical().id_source("replay-list").max_height(240.0).min_scrolled_height(100.0).show(ui,|ui| {
                if app.replay_list.is_empty(){ui.label(lang.choose("No saved replay yet. Press F9 to begin; stop track recording before pressing F9 to finish.","还没有回放。按 F9 开始录制，结束轨道录音/叠录后再按 F9 收尾。"));}
                for (path,name) in app.replay_list.clone() {
                    ui.horizontal(|ui| {
                        ui.add_sized([(ui.available_width()-350.0).max(80.0),34.0],egui::Label::new(&name).truncate(true)).on_hover_text(&name);
                        ui.add_enabled_ui(!app.busy()&&!app.taking(),|ui| {
                            if theme::action(ui,theme::Icon::Play,lang.text("Play"),"").clicked(){app.play_replay(path.clone());}
                            if ui.add(egui::Button::new(lang.choose("Export WAV","导出 WAV")).wrap(false)).clicked(){app.render_replay(path.clone());}
                            if ui.add_enabled(!app.read_only&&!app.player_open,egui::Button::new(lang.choose("Delete","删除")).wrap(false)).on_hover_text(lang.choose("Move to replay trash","移入回放回收站")).clicked(){app.delete_replay(path);}
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
}
