#![cfg_attr(windows, windows_subsystem = "windows")]
//! Optional audio preflight. Project management belongs to the main application.
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
    scan: Option<app_support::device_scan::DeviceScan>,
    scan_status: String,
    #[cfg(debug_assertions)]
    frame: usize,
}
impl Launcher {
    fn new() -> Self {
        Self::with_config(
            app_support::launcher_config::load().unwrap_or_default(),
            driver_queries_enabled(std::env::args()),
        )
    }
    fn with_config(
        config: app_support::launcher_config::LauncherConfig,
        scan_devices: bool,
    ) -> Self {
        let mut scan = scan_devices.then(app_support::device_scan::DeviceScan::new);
        if let Some(scan) = &mut scan {
            scan.request();
        }
        Self {
            config,
            inputs: Vec::new(),
            outputs: Vec::new(),
            status: String::new(),
            scan,
            scan_status: String::new(),
            #[cfg(debug_assertions)]
            frame: 0,
        }
    }
    fn apply_devices(&mut self, devices: app_support::device_scan::DeviceList) {
        devices.apply_selection(
            &mut self.config.input_device,
            &mut self.config.output_device,
            self.config.follow_system_output,
        );
        self.inputs = devices.inputs;
        self.outputs = devices.outputs;
        self.scan_status = devices.warnings.join("; ");
    }
    fn poll_devices(&mut self) {
        if let Some(result) = self
            .scan
            .as_mut()
            .and_then(app_support::device_scan::DeviceScan::poll)
        {
            match result {
                Ok(devices) => self.apply_devices(devices),
                Err(error) => self.scan_status = error,
            }
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
        self.poll_devices();
        self.config.language.apply(ctx);
        theme::set_palette(ctx, self.config.theme);
        let lang = crate::app_support::language::Language::current(ctx);
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
            ui.add_space(16.0);ui.horizontal(|ui|{theme::brand(ui);theme::caption(ui,lang.text("AUDIO SETUP"));
            if theme::theme_switch(ui,&mut self.config.theme){let _=app_support::launcher_config::save(&self.config);}
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if theme::language_switch(ui, &mut self.config.language).changed() {
                        if let Err(error) = app_support::launcher_config::save(&self.config) { self.status=error.to_string(); }
                    }
                });
            });ui.add_space(20.0);
            theme::card().show(ui,|ui|{
                ui.heading(lang.text("Prepare your session"));ui.add_space(12.0);
                if self.scan.as_ref().is_some_and(|scan|scan.pending()&&!scan.timed_out()) {
                    ui.horizontal(|ui|{ui.spinner();ui.label(lang.choose("Scanning audio devices…","正在检测音频设备…"));});
                }
                if !self.scan_status.is_empty() {
                    let message=if self.scan.as_ref().is_some_and(|scan|scan.timed_out()) {
                        lang.choose("Device scan timed out; you can still open the app or offline editor.","设备检测超时，仍可打开主程序或离线编辑。")
                    } else {lang.text(&self.scan_status)};
                    ui.label(message);
                }

                ui.label(lang.text("Input device"));egui::ComboBox::from_id_source("input").width(ui.available_width()-16.0).selected_text(&self.config.input_device).show_ui(ui,|ui|{for value in &self.inputs{ui.selectable_value(&mut self.config.input_device,value.clone(),value);}});
                ui.checkbox(&mut self.config.follow_system_output,lang.choose("Follow system output", "跟随系统输出设备")); ui.label(lang.text("Output device"));ui.add_enabled_ui(!self.config.follow_system_output, |ui| { egui::ComboBox::from_id_source("output").width(ui.available_width()-16.0).selected_text(&self.config.output_device).show_ui(ui,|ui|{for value in &self.outputs{ui.selectable_value(&mut self.config.output_device,value.clone(),value);}}); });
                ui.add_space(12.0);ui.label(lang.text("Measure compensation inside RC505 RS → Audio. Project selection and data management live in the main application."));
            });
            ui.add_space(20.0);ui.horizontal(|ui|{if ui.button(lang.text("Open RC505 RS")).clicked(){self.launch(false);}if ui.button(lang.text("Open offline editor")).clicked(){self.launch(true);}});
            ui.add_space(20.0);theme::caption(ui,format!("Data: {}",app_support::paths::appdata_root().unwrap_or_default().display()));
            theme::caption(ui,&self.status);
        });
    }
}
fn driver_queries_enabled(args: impl IntoIterator<Item = String>) -> bool {
    !args
        .into_iter()
        .any(|arg| arg == "--offline" || arg.starts_with("--ui-preview="))
}

fn main() -> eframe::Result<()> {
    app_support::diagnostics::install_panic_log();
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

#[cfg(test)]
mod tests {
    use super::*;
    use app_support::device_scan::DeviceList;
    #[test]
    fn launcher_offline_and_previews_never_start_device_queries() {
        for args in [
            vec!["--offline"],
            vec!["--ui-preview=launcher"],
            vec!["--data-dir=var/launcher-test", "--ui-preview=anything"],
        ] {
            let enabled = driver_queries_enabled(args.into_iter().map(str::to_owned));
            assert!(!enabled);
            let mut launcher = Launcher::with_config(Default::default(), enabled);
            launcher.poll_devices();
            assert!(
                launcher.scan.is_none()
                    && launcher.inputs.is_empty()
                    && launcher.outputs.is_empty()
            );
        }
        assert!(driver_queries_enabled(vec!["launcher".into()]));
    }
    #[test]
    fn launcher_late_device_results_keep_explicit_microphone_and_launch_status() {
        let mut config = app_support::launcher_config::LauncherConfig::default();
        config.input_device = "Chosen microphone".into();
        config.output_device = "Chosen headphones".into();
        config.follow_system_output = false;
        let mut launcher = Launcher::with_config(config, false);
        launcher.status = "Launch status".into();
        launcher.scan_status = "Old timeout".into();
        launcher.apply_devices(DeviceList {
            inputs: vec!["Other microphone".into()],
            outputs: vec!["Other output".into()],
            default_input: Some("Other microphone".into()),
            default_output: Some("Other output".into()),
            warnings: Vec::new(),
        });
        assert_eq!(launcher.config.input_device, "Chosen microphone");
        assert_eq!(launcher.config.output_device, "Chosen headphones");
        assert_eq!(launcher.status, "Launch status");
        assert!(launcher.scan_status.is_empty());
        launcher.config.input_device.clear();
        launcher.config.follow_system_output = true;
        launcher.apply_devices(DeviceList {
            default_input: Some("Default microphone".into()),
            default_output: Some("Default output".into()),
            ..Default::default()
        });
        assert_eq!(launcher.config.input_device, "Default microphone");
        assert_eq!(launcher.config.output_device, "Default output");
    }
}
