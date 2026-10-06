use crate::{app::MyApp, presets};
use eframe::egui;

#[derive(Clone)]
pub enum LibraryDelete {
    Sound(String),
    Phrase(String),
}
pub fn draw(ctx: &egui::Context, app: &mut MyApp) {
    let Some(request) = app.editor.library_delete.clone() else {
        return;
    };
    let completed = app
        .editor
        .library_delete_job
        .as_ref()
        .and_then(|job| match job.try_recv() {
            Ok(result) => Some(result),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Some(Err(anyhow::anyhow!("Library operation did not finish")))
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
        });
    if let Some(result) = completed {
        app.editor.library_delete_job = None;
        complete(app, &request, result);
        if app.editor.library_delete.is_none() {
            return;
        }
    }
    let lang = app.language;
    let name = match &request {
        LibraryDelete::Sound(name) | LibraryDelete::Phrase(name) => name,
    };
    let working = app.editor.library_delete_job.is_some();
    let mut open = true;
    let mut cancel = false;
    let mut confirm = false;
    egui::Window::new(lang.choose("Delete library file", "删除库文件"))
        .id(egui::Id::new("library-delete-dialog"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(460.0)
        .max_width(620.0)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.label(name);
            ui.label(lang.choose("Permanently delete this file?", "永久删除此文件？"));
            if working {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(lang.choose("Checking and deleting…", "正在检查并删除…"));
                });
            }
            ui.add_enabled_ui(
                ctx.input(|i| i.focused) && !app.busy() && !app.read_only,
                |ui| {
                    ui.horizontal(|ui| {
                        let cancel_button =
                            crate::ui::navigation::register(ui.button(lang.text("Cancel")));
                        cancel = cancel_button.clicked();
                        let confirm_button = crate::ui::navigation::register(
                            ui.button(lang.choose("Delete permanently", "永久删除")),
                        );
                        confirm = confirm_button.clicked();
                        #[cfg(debug_assertions)]
                        ctx.data_mut(|d| {
                            d.insert_temp(
                                egui::Id::new("library-delete-buttons"),
                                (cancel_button.rect, confirm_button.rect),
                            )
                        });
                    });
                },
            );
            if !app.editor.message.is_empty() {
                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .show(ui, |ui| {
                        ui.label(&app.editor.message);
                    });
            }
        });
    if working {
        ctx.request_repaint_after(std::time::Duration::from_millis(30));
        return;
    }
    if !open || cancel {
        app.editor.library_delete = None;
        return;
    }
    if !confirm {
        return;
    }
    if app.candidate_audition && !app.stop_audition() {
        return;
    }
    let job = match &request {
        LibraryDelete::Sound(name) => presets::start_delete_sound(name, &app.config),
        LibraryDelete::Phrase(name) => presets::start_delete_clip(name),
    };
    match job {
        Ok(job) => app.editor.library_delete_job = Some(job),
        Err(error) => app.editor.message = error.to_string(),
    }
    ctx.request_repaint();
}
fn complete(
    app: &mut MyApp,
    request: &LibraryDelete,
    result: anyhow::Result<presets::DeleteSoundResult>,
) {
    let lang = app.language;
    match result {
        Ok(presets::DeleteSoundResult::Deleted) => {
            if matches!(&request, LibraryDelete::Sound(name) if app.editor.candidate.as_ref().is_some_and(|c| &c.name == name))
            {
                app.editor.candidate = None;
            }
            app.editor.presets = presets::list();
            app.editor.clips = presets::list_clips();
            app.editor.library_delete = None;
            app.editor.message = lang.choose("Deleted", "已删除").into();
        }
        Ok(presets::DeleteSoundResult::UsedBy(owners)) => {
            let owners: Vec<_> = owners
                .iter()
                .map(|s| {
                    if s == "Current project" {
                        lang.choose("Current project", "当前工程")
                    } else {
                        s.as_str()
                    }
                })
                .collect();
            app.editor.message = format!(
                "{} {}",
                lang.choose("Used by:", "使用方："),
                owners.join("、")
            );
        }
        Err(error) => app.editor.message = error.to_string(),
    }
}
