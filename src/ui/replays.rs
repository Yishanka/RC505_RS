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
                ui.set_enabled(!app.storage_ui.modal_open());
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
                            "Permanently delete this recording after confirmation.",
                            "确认后永久删除本次录制。",
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
        egui::Window::new(lang.choose("Global replay library", "全局回放库"))
            .id(egui::Id::new("replay-library"))
            .open(&mut open).default_size([860.0,600.0]).vscroll(true)
            .show(ctx, |ui| {
                ui.set_enabled(!app.storage_ui.modal_open());
                ui.label(lang.choose(
                    "Playback calculates sound from the recorded inputs and operations in real time. Drag the playhead to seek; pause anywhere to import into a project. WAV is created only when you choose Export.",
                    "播放按原始输入与操作实时演算。可拖动进度，任意位置暂停并导入工程；仅在点击导出时生成 WAV。"));
                theme::control_row(ui, |ui| {
                    if app.app_state != crate::state::AppState::Init && ui.add_enabled(!app.busy() && !app.taking(),egui::Button::new(lang.choose("Record a new replay", "录制新回放"))).clicked() {
                        app.toggle_take(); close_requested=app.take_pending;
                    }
                    if ui.button(lang.choose("Refresh", "刷新列表")).clicked() { app.open_replays(); }
                    if ui.add_enabled(app.app_state == crate::state::AppState::Init && !app.busy(),egui::Button::new(lang.choose("Storage / recycle bin", "数据管理 / 回收站"))).on_hover_text(lang.choose("Available from the project screen", "请从工程选择界面打开")).clicked() { app.open_storage(); }
                });
                ui.separator();
                egui::ScrollArea::vertical().id_source("replay-list").max_height(270.0).min_scrolled_height(100.0).show(ui, |ui| {
                    if app.replay_list.is_empty() {
                        let key=app.shortcuts.label(crate::app::shortcuts::Command::Take);
                        ui.label(format!("{} · {}: {key}",lang.choose("No replays yet. Open a project and choose Record replay.","还没有回放。进入工程后点击「录制回放」。"),lang.choose("Shortcut","快捷键")));
                    }
                    for (path,name) in app.replay_list.clone() {
                        theme::card().show(ui, |ui| {
                            ui.add(egui::Label::new(&name).truncate(true)).on_hover_text(&name);
                            theme::control_row(ui, |ui| {
                                ui.add_enabled_ui(!app.busy() && !app.taking(), |ui| {
                                    if theme::action(ui,theme::Icon::Play,lang.text("Play"),"").clicked() { app.play_replay(path.clone()); }
                                    if ui.add_enabled(!app.read_only,egui::Button::new(lang.choose("Export WAV", "导出 WAV")).wrap(false)).clicked() { app.render_replay(path.clone()); }
                                    if ui.add_enabled_ui(!app.read_only && !app.player_open,|ui|theme::action(ui,theme::Icon::Trash,lang.choose("Delete","删除"),"")).inner.clicked() { app.delete_replay(path); }
                                });
                            });
                        });
                    }
                });
                ui.collapsing(lang.choose("Open a replay folder", "打开外部回放目录"), |ui| {
                    ui.add(egui::TextEdit::singleline(&mut app.replay_import_path).hint_text(lang.text("Replay folder containing replay.json")).desired_width(f32::INFINITY));
                    if ui.add_enabled(!app.busy() && !app.taking(),egui::Button::new(lang.choose("Open / play", "打开并播放"))).clicked() {
                        app.play_replay(std::path::PathBuf::from(app.replay_import_path.trim()));
                    }
                });
                ui.collapsing(lang.choose("Exported WAV files", "已导出的 WAV 文件"), |ui| {
                    theme::caption(ui, lang.choose("Deleting an export frees its disk space and keeps the replay.", "删除导出文件会释放其磁盘空间，回放本身保留。"));
                    if app.replay_exports.is_empty() { ui.label(lang.choose("No exported audio", "没有导出音频")); }
                    for path in app.replay_exports.clone() {
                        ui.horizontal(|ui| {
                            ui.add_sized([(ui.available_width()-120.0).max(80.0),32.0],egui::Label::new(path.file_name().unwrap_or_default().to_string_lossy()).truncate(true)).on_hover_text(path.display().to_string());
                            if ui.add_enabled(!app.read_only && !app.busy(),egui::Button::new(lang.choose("Delete","删除")).wrap(false)).clicked() { app.delete_replay_export(path); }
                        });
                    }
                });
                if app.job.is_some() {
                    ui.horizontal(|ui| { ui.spinner(); ui.label(format!("{}: {}",lang.choose("Processed frames", "已处理采样帧"),app.render_progress.load(Ordering::Relaxed))); });
                }
                if let Some((_,result))=&app.rendered {
                    ui.separator();
                    ui.label(format!("{}: {}",lang.choose("Exported WAV", "已导出 WAV"),result.wav.display()));
                }
                ui.label(lang.text(&app.status));
            });
        if !open {
            app.replay_autoplay = false;
        }
        app.replay_browser = open && !close_requested;
    }
}
