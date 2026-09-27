// src/main.rs
mod app;
mod app_support;
mod config;
mod dsp;
mod engine;
mod maintenance;
mod presets;
mod project;
mod replay;
mod session;
mod state;
#[cfg(test)]
mod test_alloc;
mod ui;
mod updater;
mod utils;

use app::MyApp;

fn main() -> eframe::Result<()> {
    if let Some(result) = maintenance::cli() {
        if let Err(error) = result {
            eprintln!("{error:#}");
            std::process::exit(1);
        }
        return Ok(());
    }
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
