//! RC505_RS Launcher — pre-flight configuration and project management
//!
//! This launcher provides:
//! - Audio device discovery and selection
//! - Project management (create, browse, delete)
//! - Quick-launch configuration (BPM, latency compensation)
//! - One-click launch into the main RC505_RS looper app

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use cpal::traits::{DeviceTrait, HostTrait};
use eframe::egui;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Data paths — mirrors the conventions in `src/project.rs`.
// ---------------------------------------------------------------------------

fn appdata_root() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        PathBuf::from(appdata).join("rc505_rs")
    } else {
        PathBuf::from("rc505_data")
    }
}

fn projects_dir() -> PathBuf {
    appdata_root().join("projects")
}

fn launcher_config_path() -> PathBuf {
    appdata_root().join("launcher_config.json")
}

// ---------------------------------------------------------------------------
// Launcher configuration (persisted between sessions).
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
struct LauncherConfig {
    input_device: String,
    output_device: String,
    default_bpm: usize,
    latency_comp_ms: usize,
    #[serde(default)]
    last_project: String,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            input_device: String::new(),
            output_device: String::new(),
            default_bpm: 120,
            latency_comp_ms: 85,
            last_project: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Project entry (mirrors project.rs ProjectEntry shape).
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
struct ProjectEntry {
    name: String,
    file: String,
}

#[derive(Default, Serialize, Deserialize)]
struct ProjectIndex {
    projects: Vec<ProjectEntry>,
}

// ---------------------------------------------------------------------------
// Launcher application state.
// ---------------------------------------------------------------------------

#[derive(PartialEq)]
enum LauncherTab {
    Audio,
    Projects,
    About,
}

struct Rc505Launcher {
    // Audio
    input_devices: Vec<String>,
    output_devices: Vec<String>,
    selected_input: usize,
    selected_output: usize,
    scan_error: Option<String>,
    scan_done: bool,

    // Config
    config: LauncherConfig,
    bpm_input: String,
    latency_input: String,

    // Projects
    projects: Vec<ProjectEntry>,
    selected_project: usize,
    new_project_name: String,
    rename_input: String,
    renaming_idx: Option<usize>,

    // UI
    current_tab: LauncherTab,
    status_message: String,
    status_is_error: bool,

    // Fonts
    fonts_initialized: bool,
}

impl Rc505Launcher {
    fn new() -> Self {
        let config = load_launcher_config().unwrap_or_default();
        let projects = load_project_list();

        // Pre-select the last-used project if it still exists.
        let selected_project = if config.last_project.is_empty() {
            projects.len() // "NEW PROJECT" row
        } else {
            projects
                .iter()
                .position(|p| p.name == config.last_project)
                .unwrap_or(projects.len())
        };

        Self {
            input_devices: vec!["(scanning...)".to_string()],
            output_devices: vec!["(scanning...)".to_string()],
            selected_input: 0,
            selected_output: 0,
            scan_error: None,
            scan_done: false,

            config,
            bpm_input: String::new(),
            latency_input: String::new(),

            projects,
            selected_project,
            new_project_name: String::new(),
            rename_input: String::new(),
            renaming_idx: None,

            current_tab: LauncherTab::Audio,
            status_message: String::new(),
            status_is_error: false,

            fonts_initialized: false,
        }
    }

    fn scan_audio_devices(&mut self) {
        self.scan_done = true;
        self.scan_error = None;

        let host = cpal::default_host();

        let inputs: Vec<String> = host
            .input_devices()
            .map(|iter| {
                iter.filter_map(|d| d.name().ok())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let outputs: Vec<String> = host
            .output_devices()
            .map(|iter| {
                iter.filter_map(|d| d.name().ok())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        if inputs.is_empty() && outputs.is_empty() {
            self.scan_error = Some(
                "No audio devices found. Please connect a microphone or audio interface."
                    .to_string(),
            );
        }

        // If the previously-configured device is still present,
        // keep it selected; otherwise pick the first available.
        if !self.config.input_device.is_empty() {
            if let Some(pos) = inputs.iter().position(|n| *n == self.config.input_device) {
                self.selected_input = pos;
            }
        }
        if !self.config.output_device.is_empty() {
            if let Some(pos) = outputs.iter().position(|n| *n == self.config.output_device) {
                self.selected_output = pos;
            }
        }

        self.input_devices = inputs;
        self.output_devices = outputs;
    }

    fn get_selected_input(&self) -> String {
        self.input_devices
            .get(self.selected_input)
            .cloned()
            .unwrap_or_default()
    }

    fn get_selected_output(&self) -> String {
        self.output_devices
            .get(self.selected_output)
            .cloned()
            .unwrap_or_default()
    }

    fn save_current_config(&mut self) {
        self.config.input_device = self.get_selected_input();
        self.config.output_device = self.get_selected_output();
        let _ = save_launcher_config(&self.config);
    }

    // ------------------------------------------------------------------
    // Project helpers
    // ------------------------------------------------------------------

    fn create_project(&mut self) {
        let name = self.new_project_name.trim().to_string();
        if name.is_empty() {
            self.set_status("Project name cannot be empty.", true);
            return;
        }
        let _ = fs::create_dir_all(projects_dir());
        let idx = self.projects.len();
        let file = make_project_file_name(&name, idx);
        self.projects.push(ProjectEntry { name, file });
        let _ = save_project_list(&self.projects);
        self.new_project_name.clear();
        self.selected_project = self.projects.len() - 1;
        self.set_status("Project created.", false);
    }

    fn delete_project(&mut self) {
        if self.selected_project >= self.projects.len() {
            return;
        }
        let entry = self.projects.remove(self.selected_project);
        let _ = fs::remove_file(projects_dir().join(&entry.file));
        let _ = save_project_list(&self.projects);
        if self.selected_project > 0 && self.selected_project >= self.projects.len() {
            self.selected_project = self.projects.len().saturating_sub(1);
        }
        self.set_status(&format!("Deleted project \"{}\".", entry.name), false);
    }

    fn start_rename(&mut self) {
        if self.selected_project < self.projects.len() {
            self.rename_input = self.projects[self.selected_project].name.clone();
            self.renaming_idx = Some(self.selected_project);
        }
    }

    fn finish_rename(&mut self) {
        if let Some(idx) = self.renaming_idx {
            let new_name = self.rename_input.trim().to_string();
            if !new_name.is_empty() && idx < self.projects.len() {
                self.projects[idx].name = new_name;
                let _ = save_project_list(&self.projects);
            }
        }
        self.rename_input.clear();
        self.renaming_idx = None;
    }

    // ------------------------------------------------------------------
    // Launch
    // ------------------------------------------------------------------

    fn launch_app(&mut self) {
        // Persist current selections.
        self.save_current_config();

        // Locate the main executable next to us.
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."));

        let main_exe = exe_dir.join("rc505_rs.exe");

        if !main_exe.exists() {
            self.set_status(
                &format!(
                    "Main executable not found at:\n{}\nPlease put rc505_rs.exe in the same directory.",
                    main_exe.display()
                ),
                true,
            );
            return;
        }

        // Save the selected project as "last project" for next time.
        if self.selected_project < self.projects.len() {
            self.config.last_project = self.projects[self.selected_project].name.clone();
            let _ = save_launcher_config(&self.config);
        }

        self.set_status("Launching RC505_RS...", false);

        match Command::new(&main_exe)
            .current_dir(&exe_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(_child) => {
                self.set_status("RC505_RS launched successfully!", false);
            }
            Err(e) => {
                self.set_status(&format!("Failed to launch: {e}"), true);
            }
        }
    }

    fn set_status(&mut self, msg: &str, is_error: bool) {
        self.status_message = msg.to_string();
        self.status_is_error = is_error;
    }

    // ------------------------------------------------------------------
    // CJK font fallback — same approach as the main app.
    // ------------------------------------------------------------------

    fn setup_font_fallback(&mut self, ctx: &egui::Context) {
        if self.fonts_initialized {
            return;
        }
        let candidates = [
            r"C:\Windows\Fonts\msyh.ttc",
            r"C:\Windows\Fonts\msyh.ttf",
            r"C:\Windows\Fonts\simhei.ttf",
            r"C:\Windows\Fonts\simsun.ttc",
        ];
        for path in candidates {
            if let Ok(bytes) = std::fs::read(path) {
                let mut fonts = egui::FontDefinitions::default();
                fonts
                    .font_data
                    .insert("cjk_fallback".to_owned(), egui::FontData::from_owned(bytes).into());
                fonts
                    .families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .insert(0, "cjk_fallback".to_owned());
                fonts
                    .families
                    .entry(egui::FontFamily::Monospace)
                    .or_default()
                    .push("cjk_fallback".to_owned());
                ctx.set_fonts(fonts);
                break;
            }
        }
        self.fonts_initialized = true;
    }
}

// ---------------------------------------------------------------------------
// eframe App impl
// ---------------------------------------------------------------------------

impl eframe::App for Rc505Launcher {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint();
        self.setup_font_fallback(ctx);

        // Scan audio on first frame.
        if !self.scan_done {
            self.scan_audio_devices();
            self.bpm_input = self.config.default_bpm.to_string();
            self.latency_input = self.config.latency_comp_ms.to_string();
        }

        // Top bar with tab switcher + Launch button.
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.current_tab, LauncherTab::Audio, "Audio Setup");
                ui.selectable_value(&mut self.current_tab, LauncherTab::Projects, "Projects");
                ui.selectable_value(&mut self.current_tab, LauncherTab::About, "About");

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_sized(
                            [120.0, 32.0],
                            egui::Button::new("Launch RC505")
                                .fill(egui::Color32::from_rgb(0, 140, 80)),
                        )
                        .clicked()
                    {
                        self.launch_app();
                    }
                });
            });
        });

        // Central area.
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(20, 20, 22))
                .show(ui, |ui| match self.current_tab {
                    LauncherTab::Audio => self.draw_audio_tab(ui),
                    LauncherTab::Projects => self.draw_projects_tab(ui),
                    LauncherTab::About => self.draw_about_tab(ui),
                });
        });

        // Bottom status bar.
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            if !self.status_message.is_empty() {
                let color = if self.status_is_error {
                    egui::Color32::from_rgb(255, 80, 80)
                } else {
                    egui::Color32::from_rgb(100, 200, 100)
                };
                ui.colored_label(color, &self.status_message);
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Tab drawing helpers
// ---------------------------------------------------------------------------

impl Rc505Launcher {
    fn draw_audio_tab(&mut self, ui: &mut egui::Ui) {
        if let Some(ref err) = self.scan_error {
            ui.colored_label(
                egui::Color32::from_rgb(255, 100, 80),
                format!("Warning: {}", err),
            );
            ui.separator();
        }

        ui.heading("Audio Devices");
        ui.label("Select the input and output devices for your looper session.");
        ui.add_space(8.0);

        // Refresh button.
        if ui.button("Rescan Devices").clicked() {
            self.scan_done = false;
            self.scan_audio_devices();
        }
        ui.add_space(12.0);

        let mut config_changed = false;

        // Input device
        ui.horizontal(|ui| {
            ui.label("Input:");
            egui::ComboBox::from_id_source("input_device")
                .width(350.0)
                .selected_text(
                    self.input_devices
                        .get(self.selected_input)
                        .cloned()
                        .unwrap_or_default(),
                )
                .show_ui(ui, |ui| {
                    for (i, name) in self.input_devices.iter().enumerate() {
                        if ui.selectable_value(&mut self.selected_input, i, name).clicked() {
                            config_changed = true;
                        }
                    }
                });
        });

        ui.add_space(8.0);

        // Output device
        ui.horizontal(|ui| {
            ui.label("Output:");
            egui::ComboBox::from_id_source("output_device")
                .width(350.0)
                .selected_text(
                    self.output_devices
                        .get(self.selected_output)
                        .cloned()
                        .unwrap_or_default(),
                )
                .show_ui(ui, |ui| {
                    for (i, name) in self.output_devices.iter().enumerate() {
                        if ui.selectable_value(&mut self.selected_output, i, name).clicked() {
                            config_changed = true;
                        }
                    }
                });
        });

        if config_changed {
            self.save_current_config();
        }

        ui.add_space(20.0);
        ui.separator();
        ui.heading("Default Session Settings");

        ui.horizontal(|ui| {
            ui.label("BPM:");
            if ui
                .add_sized([60.0, 20.0], egui::TextEdit::singleline(&mut self.bpm_input))
                .lost_focus()
            {
                if let Ok(v) = self.bpm_input.trim().parse::<usize>() {
                    self.config.default_bpm = v.clamp(30, 300);
                    self.bpm_input = self.config.default_bpm.to_string();
                    self.save_current_config();
                }
            }
            if ui.button("Reset").clicked() {
                self.config.default_bpm = 120;
                self.bpm_input = "120".to_string();
                self.save_current_config();
            }
        });

        ui.horizontal(|ui| {
            ui.label("Latency Comp (ms):");
            if ui
                .add_sized([60.0, 20.0], egui::TextEdit::singleline(&mut self.latency_input))
                .lost_focus()
            {
                if let Ok(v) = self.latency_input.trim().parse::<usize>() {
                    self.config.latency_comp_ms = v.clamp(0, 500);
                    self.latency_input = self.config.latency_comp_ms.to_string();
                    self.save_current_config();
                }
            }
            if ui.button("Reset").clicked() {
                self.config.latency_comp_ms = 85;
                self.latency_input = "85".to_string();
                self.save_current_config();
            }
            ui.label("(adjust based on your hardware)");
        });

        ui.add_space(16.0);
        ui.label("Tip: The main RC505_RS app will use the devices configured in its own System settings. Use this launcher to pre-configure your defaults.");
    }

    fn draw_projects_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Project Manager");
        ui.add_space(8.0);

        // New project row.
        ui.horizontal(|ui| {
            ui.label("New Project:");
            ui.add_sized(
                [200.0, 20.0],
                egui::TextEdit::singleline(&mut self.new_project_name)
                    .hint_text("project name..."),
            );
            if ui.button("Create").clicked() {
                self.create_project();
            }
        });

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);

        // Project list.
        let mut pending_finish_rename = false;
        let mut pending_start_rename = false;
        let mut pending_delete = false;

        egui::ScrollArea::vertical()
            .max_height(300.0)
            .show(ui, |ui| {
                for (i, proj) in self.projects.iter().enumerate() {
                    let is_selected = self.selected_project == i;
                    let is_renaming = self.renaming_idx == Some(i);

                    ui.horizontal(|ui| {
                        if is_renaming {
                            if ui
                                .add_sized(
                                    [200.0, 20.0],
                                    egui::TextEdit::singleline(&mut self.rename_input),
                                )
                                .lost_focus()
                            {
                                pending_finish_rename = true;
                            }
                            if ui.button("OK").clicked() {
                                pending_finish_rename = true;
                            }
                        } else {
                            let label = format!("{}  [{}]", proj.name, proj.file);
                            if ui
                                .selectable_label(is_selected, &label)
                                .clicked()
                            {
                                self.selected_project = i;
                            }
                            if is_selected {
                                if ui.button("Rename").clicked() {
                                    pending_start_rename = true;
                                }
                                if ui.button("Delete").clicked() {
                                    pending_delete = true;
                                }
                            }
                        }
                    });
                }

                // "[ NEW PROJECT ]" row.
                let is_new = self.selected_project >= self.projects.len();
                if ui.selectable_label(is_new, "[ + NEW PROJECT ]").clicked() {
                    self.selected_project = self.projects.len();
                }
            });

        if pending_finish_rename {
            self.finish_rename();
        }
        if pending_start_rename {
            self.start_rename();
        }
        if pending_delete {
            self.delete_project();
        }

        ui.add_space(12.0);
        ui.separator();

        if self.selected_project < self.projects.len() {
            let name = &self.projects[self.selected_project].name;
            ui.label(format!("Will launch into project: {}", name));
        } else {
            ui.label("Will launch with a new DEFAULT project.");
        }
    }

    fn draw_about_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("RC505_RS — BOSS RC-505 Style Looper");
        ui.add_space(12.0);

        ui.label("A free, open-source live looping application inspired by the BOSS RC-505 MK2.");
        ui.add_space(8.0);

        ui.label("Features:");
        ui.label("  * 5 independent loop tracks with record / play / overdub / pause");
        ui.label("  * Input FX: Oscillator, Filter, Reverb, MyDelay (4 banks x 4 slots)");
        ui.label("  * Track FX: Delay, Roll, Filter — per-track enable states");
        ui.label("  * Beat-synced recording with configurable BPM");
        ui.label("  * Latency compensation for precise alignment");
        ui.label("  * Project save/load via JSON");
        ui.label("  * WASAPI audio (default) or ASIO (compile-time feature)");

        ui.add_space(12.0);
        ui.label("Keyboard Controls (in Looper):");
        ui.label("  1-5: Track record/play/dub    F1-F5: Pause tracks");
        ui.label("  S: Switch Loop/Screen mode   T: Toggle FX Bank/Single");
        ui.label("  QWER: Input FX    UIOP: Track FX");
        ui.label("  Left/Right: Select track    Delete: Clear track");
        ui.label("  Esc: Exit to project list");

        ui.add_space(12.0);
        ui.label("Data stored in: %APPDATA%/rc505_rs/projects/");

        ui.add_space(16.0);
        ui.separator();
        ui.hyperlink_to("GitHub Repository", "https://github.com/Yishanka/RC505_RS");
        ui.label("Built with Rust, eframe/egui, and cpal.");
    }
}

// ---------------------------------------------------------------------------
// Persistence helpers
// ---------------------------------------------------------------------------

fn load_launcher_config() -> Option<LauncherConfig> {
    let path = launcher_config_path();
    if !path.exists() {
        return None;
    }
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn save_launcher_config(config: &LauncherConfig) -> anyhow::Result<()> {
    let _ = fs::create_dir_all(appdata_root());
    let raw = serde_json::to_string_pretty(config)?;
    fs::write(launcher_config_path(), raw)?;
    Ok(())
}

fn load_project_list() -> Vec<ProjectEntry> {
    let path = projects_dir().join("projects_index.json");
    if !path.exists() {
        return vec![];
    }
    match fs::read_to_string(&path) {
        Ok(raw) => match serde_json::from_str::<ProjectIndex>(&raw) {
            Ok(idx) => idx.projects,
            Err(_) => vec![],
        },
        Err(_) => vec![],
    }
}

fn save_project_list(entries: &[ProjectEntry]) -> anyhow::Result<()> {
    let _ = fs::create_dir_all(projects_dir());
    let idx = ProjectIndex {
        projects: entries.to_vec(),
    };
    let raw = serde_json::to_string_pretty(&idx)?;
    fs::write(projects_dir().join("projects_index.json"), raw)?;
    Ok(())
}

fn make_project_file_name(name: &str, idx: usize) -> String {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else if c.is_whitespace() {
                '_'
            } else {
                '_'
            }
        })
        .collect();
    let safe = if safe.is_empty() { "project".to_string() } else { safe };
    format!("{}_{}.json", safe, idx)
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> eframe::Result<()> {
    // Ensure the data directories exist before the launcher starts.
    let _ = fs::create_dir_all(projects_dir());

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([680.0, 520.0])
            .with_title("RC505_RS Launcher"),
        ..Default::default()
    };

    eframe::run_native(
        "RC505_RS Launcher",
        options,
        Box::new(|_cc| Box::new(Rc505Launcher::new())),
    )
}
