// src/main.rs
mod app;
mod app_support;
mod config;
mod dsp;
mod engine;
mod project;
mod screen;
mod state;
mod track;
mod ui;
mod utils;

use app::MyApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native("RC505 RS", options, Box::new(|_cc| Box::new(MyApp::new())))
}
