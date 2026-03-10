//src/ui/init.rs
use eframe::egui;
use crate::app::{MyApp};
use crate::state::ProjectNameMode; 

pub fn draw_init(ui: &mut egui::Ui, app: &MyApp) {
    ui.vertical_centered(|ui| {
        ui.add_space(100.0);
        ui.heading("RC505-RS ENGINE");
        ui.add_space(20.0);

        for (i, proj) in app.projects.iter().enumerate() {
            let is_selected = app.sel_project_idx == i;
            let color = if is_selected { egui::Color32::RED } else { egui::Color32::GRAY };
            let text = if is_selected && app.project_name_mode == Some(ProjectNameMode::Rename) {
                format!("> {}", app.project_name_input)
            } else {
                format!("{} {}", if is_selected { ">" } else { " " }, proj.name)
            };
            ui.label(egui::RichText::new(text).size(24.0).color(color));
        }

        // New project
        let is_new_selected = app.sel_project_idx == app.projects.len();
        let new_text = if is_new_selected && app.project_name_mode == Some(ProjectNameMode::Add) {
            format!("> {}", app.project_name_input)
        } else {
            format!("{} [ NEW PROJECT ]", if is_new_selected { ">" } else { " " })
        };
        ui.label(egui::RichText::new(new_text)
            .size(24.0).color(if is_new_selected { egui::Color32::RED } else { egui::Color32::DARK_GRAY }));
        
        ui.add_space(10.0);
    });
}
