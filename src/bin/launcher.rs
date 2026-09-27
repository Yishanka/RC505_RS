//! Optional audio preflight. Project management belongs to the main application.
use cpal::traits::{DeviceTrait, HostTrait};
use eframe::egui;
#[path = "../app_support/mod.rs"]
mod app_support;
#[cfg(debug_assertions)]
#[path = "../ui/capture.rs"]
mod capture;
#[path = "../ui/theme.rs"]
mod theme;

struct Launcher {
    config: app_support::launcher_config::LauncherConfig,
    inputs: Vec<String>,
    outputs: Vec<String>,
    status: String,
    #[cfg(debug_assertions)]
    frame: usize,
}
impl Launcher {
    fn new() -> Self {
        let host = cpal::default_host();
        let mut config = app_support::launcher_config::load().unwrap_or_default();
        let inputs = host
            .input_devices()
            .into_iter()
            .flatten()
            .filter_map(|d| d.name().ok())
            .collect();
        let outputs = host
            .output_devices()
            .into_iter()
            .flatten()
            .filter_map(|d| d.name().ok())
            .collect();
        if config.input_device.is_empty() {
            config.input_device = host
                .default_input_device()
                .and_then(|d| d.name().ok())
                .unwrap_or_default();
        }
        if config.output_device.is_empty() {
            config.output_device = host
                .default_output_device()
                .and_then(|d| d.name().ok())
                .unwrap_or_default();
        }
        Self {
            config,
            inputs,
            outputs,
            status: String::new(),
            #[cfg(debug_assertions)]
            frame: 0,
        }
    }
    fn launch(&mut self, offline: bool) {
        let result = (|| -> anyhow::Result<()> {
            app_support::launcher_config::save(&self.config)?;
            let executable = std::env::current_exe()?.with_file_name("rc505_rs.exe");
            let mut command = std::process::Command::new(executable);
            if let Some(root) = app_support::paths::appdata_root() {
                command.arg(format!(
                    "--data-dir={}",
                    std::path::absolute(root)?.display()
                ));
            }
            if offline {
                command.arg("--offline");
            }
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x08000000);
            }
            command.spawn()?;
            Ok(())
        })();
        self.status = match result {
            Ok(()) => "RC505 RS opened at the project browser.".into(),
            Err(e) => format!("Cannot launch: {e}"),
        };
    }
}
impl eframe::App for Launcher {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(std::time::Duration::from_millis(33));
        #[cfg(debug_assertions)]
        if std::env::args().any(|a| a == "--ui-preview=launcher") {
            assert!(
                std::env::args().any(|a| a.starts_with("--data-dir=")),
                "Preview requires isolated data"
            );
            capture::capture(ctx, "launcher", &mut self.frame);
        }
        egui::CentralPanel::default().show(ctx,|ui|{
            ui.add_space(16.0);ui.horizontal(|ui|{theme::brand(ui);theme::caption(ui,"AUDIO SETUP");});ui.add_space(20.0);
            theme::card().show(ui,|ui|{
                ui.heading("Prepare your session");ui.add_space(12.0);
                ui.label("Input device");egui::ComboBox::from_id_source("input").width(ui.available_width()-16.0).selected_text(&self.config.input_device).show_ui(ui,|ui|{for value in &self.inputs{ui.selectable_value(&mut self.config.input_device,value.clone(),value);}});
                ui.label("Output device");egui::ComboBox::from_id_source("output").width(ui.available_width()-16.0).selected_text(&self.config.output_device).show_ui(ui,|ui|{for value in &self.outputs{ui.selectable_value(&mut self.config.output_device,value.clone(),value);}});
                ui.add_space(12.0);ui.label("Measure compensation inside RC505 RS → Audio. Project selection and data management live in the main application.");
            });
            ui.add_space(20.0);ui.horizontal(|ui|{if ui.button("Open RC505 RS").clicked(){self.launch(false);}if ui.button("Open offline editor").clicked(){self.launch(true);}});
            ui.add_space(20.0);theme::caption(ui,format!("Data: {}",app_support::paths::appdata_root().unwrap_or_default().display()));
            theme::caption(ui,&self.status);
        });
    }
}
fn main() -> eframe::Result<()> {
    eframe::run_native(
        "RC505 RS · Audio setup",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_icon(theme::window_icon())
                .with_inner_size([780.0, 560.0])
                .with_min_inner_size([660.0, 520.0]),
            ..Default::default()
        },
        Box::new(|cc| {
            theme::apply(&cc.egui_ctx);
            Box::new(Launcher::new())
        }),
    )
}
