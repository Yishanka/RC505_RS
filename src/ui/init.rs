use super::theme;
use crate::{app::MyApp, state::ProjectNameMode};
use eframe::egui;

pub fn draw_init(ui: &mut egui::Ui, app: &mut MyApp) {
    ui.add_space(30.0);
    ui.horizontal(|ui| {
        theme::brand(ui);
        theme::caption(ui, "LOOP WORKSTATION");
    });
    ui.heading("Your next idea starts with a loop.");
    theme::caption(
        ui,
        "Five tracks. Two effect racks. A visual sound-design workspace.",
    );
    ui.add_space(25.0);
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.columns(2,|columns| {
            theme::card().show(&mut columns[0],|ui| {
                ui.heading("Projects");
                theme::caption(ui,"Click to open / Up, Down, Enter / R rename");
                for index in 0..app.projects.len() {
                    let name = app.projects[index].name.clone();
                    if ui.add_sized([ui.available_width(),48.0],egui::SelectableLabel::new(index==app.sel_project_idx,format!("{:02}    {name}",index+1))).clicked() { app.open_project(index); }
                }
            });
            theme::card().show(&mut columns[1],|ui| {
                ui.heading("Create a project");
                if app.project_name_mode.is_some() {
                    ui.label(&app.project_name_input);
                    theme::caption(ui,"Type a name and press Enter. Esc cancels.");
                } else {
                    let response = ui.add(egui::TextEdit::singleline(&mut app.project_name_input).hint_text("Project name").desired_width(ui.available_width()));
                    if ui.button("Create and open").clicked() || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) { app.create_project(); }
                    if ui.button("Rename selected").clicked() && app.sel_project_idx<app.projects.len() {
                        app.project_name_mode=Some(ProjectNameMode::Rename); app.project_name_input=app.projects[app.sel_project_idx].name.clone();
                    }
                }
                ui.separator();
                ui.label("1. Check audio devices in the performance view.");
                ui.label("2. Record a phrase with a track's Record button.");
                ui.label("3. Finish, layer, and shape it with the FX racks.");
                ui.label("4. Expand an FX slot for piano roll and visual parameters.");
                ui.add_space(12.0);
                theme::caption(ui,"Projects save parameters and sequences. Recorded loop audio currently lasts only for this session.");
                theme::caption(ui,app.audio_status());
                if !app.status.is_empty() { ui.label(&app.status); }
            });
        });
    });
}
