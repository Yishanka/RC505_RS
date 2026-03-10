// src/main.rs
mod app;
mod ui;
mod utils;
mod config;
mod track;
mod screen;
mod engine;
mod dsp;
mod state;
mod project; 

use app::MyApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "RC505 RS",
        options,
        Box::new(|_cc| Box::new(MyApp::new())),
    )
}
