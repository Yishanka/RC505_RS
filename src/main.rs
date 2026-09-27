// src/main.rs
mod app;
mod app_support;
mod config;
mod dsp;
mod engine;
mod presets;
mod project;
mod screen;
mod state;
mod track;
mod ui;
mod utils;

use app::MyApp;

fn main() -> eframe::Result<()> {
    let small = std::env::args().any(|arg| arg == "--ui-preview=performance-small");
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size(if small {
                [960.0, 720.0]
            } else {
                [1320.0, 900.0]
            })
            .with_min_inner_size([960.0, 640.0]),
        ..Default::default()
    };
    eframe::run_native(
        "RC505 RS",
        options,
        Box::new(|cc| {
            ui::theme::apply(&cc.egui_ctx);
            let app = MyApp::new();
            #[cfg(debug_assertions)]
            let app = {
                let mut app = app;
                if let Some(mode) = std::env::args()
                    .find_map(|arg| arg.strip_prefix("--ui-preview=").map(str::to_owned))
                {
                    assert!(
                        std::env::args().any(|arg| arg == "--offline")
                            && std::env::args().any(|arg| arg.starts_with("--data-dir=")),
                        "UI preview requires --offline and an isolated --data-dir"
                    );
                    ui::preview::configure(&mut app, &mode);
                }
                app
            };
            Box::new(app)
        }),
    )
}
