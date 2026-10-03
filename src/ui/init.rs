use super::theme;
use crate::{app::MyApp, state::ProjectNameMode};
use eframe::egui;

pub fn draw_init(ui: &mut egui::Ui, app: &mut MyApp) {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    let width = ui.available_width().min(760.0);
    let margin = ((ui.available_width() - width) * 0.5).max(0.0);
    ui.add_space((ui.available_height() * 0.08).min(65.0));
    ui.horizontal(|ui| {
        ui.add_space(margin);
        ui.vertical(|ui| {
            ui.set_width(width);
            ui.horizontal(|ui| {
                theme::brand(ui);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    app.language_switch(ui);
                    if ui.button(lang.choose("Keys", "键位")).clicked() {
                        app.shortcut_editor.open(&app.shortcuts);
                    }
                    if theme::action(ui, theme::Icon::Help, lang.text("Help"), "F12").clicked() {
                        app.help_open = true;
                    }
                });
            });
            ui.add_space(16.0);
            ui.heading(lang.text("Choose your project"));
            theme::caption(ui, lang.text("↑ ↓ select · Enter opens · N creates"));
            ui.add_space(12.0);
            theme::card().show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_source("project-list")
                    .max_height(280.0)
                    .min_scrolled_height(200.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for index in 0..app.projects.len() {
                            let label =
                                format!("{:02}     {}", index + 1, app.projects[index].name);
                            let response = ui.add_sized(
                                [ui.available_width(), 48.0],
                                egui::SelectableLabel::new(index == app.sel_project_idx, label),
                            );
                            if response.clicked() {
                                app.sel_project_idx = index;
                            }
                            if response.double_clicked() {
                                app.open_project(index);
                            }
                        }
                    });
                ui.separator();
                theme::control_row(ui, |ui| {
                    if ui
                        .add_enabled(
                            !app.busy() && !app.projects.is_empty(),
                            egui::Button::new(lang.text("Open project")).shortcut_text("Enter"),
                        )
                        .clicked()
                    {
                        app.open_project(app.sel_project_idx);
                    }
                    if theme::action(ui, theme::Icon::None, lang.text("New"), "N").clicked() {
                        app.project_name_mode = Some(ProjectNameMode::Add);
                        app.project_name_input.clear();
                    }
                    if ui.button(lang.text("Rename")).clicked() {
                        if let Some(entry) = app.projects.get(app.sel_project_idx) {
                            app.project_name_input = entry.name.clone();
                            app.project_name_mode = Some(ProjectNameMode::Rename);
                        }
                    }
                    ui.add_enabled_ui(!app.read_only && !app.busy(), |ui| {
                        if theme::action(
                            ui,
                            theme::Icon::Trash,
                            lang.choose("Delete project", "删除工程"),
                            "",
                        )
                        .on_hover_text(lang.choose(
                            "Moves the selected project and its audio into project trash.",
                            "将所选工程及音频移入工程回收站，可恢复。",
                        ))
                        .clicked()
                        {
                            app.trash_project();
                        }
                        if ui.button(lang.text("Restore last deleted")).clicked() {
                            app.restore_project();
                        }
                    });
                });
            });
            ui.add_space(12.0);
            if theme::action(ui, theme::Icon::Play, lang.choose("Replay library", "全局回放库"), "F10").clicked() {
                app.open_replays();
            }
            theme::caption(ui, lang.choose("Replays are shared across all projects. Play, seek, export, or import any paused position.", "回放独立于工程保存，可播放、跳转、导出，或将暂停位置导入任意工程。"));
            ui.add_space(12.0);
            ui.add(egui::Label::new(lang.text(&app.status)).wrap(true));
            theme::caption(ui, lang.text(&app.audio_status()));
            if let Some(mode) = app.project_name_mode {
                egui::Window::new(if mode == ProjectNameMode::Add {
                    lang.text("New project")
                } else {
                    lang.text("Rename project")
                })
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ui.ctx(), |ui| {
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut app.project_name_input)
                            .hint_text(lang.text("Project name"))
                            .desired_width(340.0),
                    );
                    if !response.has_focus() {
                        response.request_focus();
                    }
                    ui.horizontal(|ui| {
                        if ui.button(lang.text("Confirm")).clicked()
                            || ui.input(|i| i.key_pressed(egui::Key::Enter))
                        {
                            app.create_project();
                        }
                        if ui.button(lang.text("Cancel")).clicked()
                            || ui.input(|i| i.key_pressed(egui::Key::Escape))
                        {
                            app.project_name_mode = None;
                        }
                    });
                });
            }
        });
    });
}
