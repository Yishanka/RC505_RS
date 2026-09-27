//! Debug-only screenshot capture through the actual egui renderer.
use eframe::egui;
pub fn capture(ctx: &egui::Context, mode: &str, frame: &mut usize) -> bool {
    *frame += 1;
    if *frame == 8 {
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
    }
    let image = ctx.input(|i| {
        i.events.iter().find_map(|event| {
            if let egui::Event::Screenshot { image, .. } = event {
                Some(image.clone())
            } else {
                None
            }
        })
    });
    if let Some(image) = image {
        use std::io::Write;
        let directory = std::path::Path::new("var/ui-verification");
        std::fs::create_dir_all(directory).unwrap();
        let mut file = std::fs::File::create(directory.join(format!("{mode}.ppm"))).unwrap();
        write!(file, "P6\n{} {}\n255\n", image.width(), image.height()).unwrap();
        let pixels: Vec<u8> = image
            .pixels
            .iter()
            .flat_map(|pixel| [pixel.r(), pixel.g(), pixel.b()])
            .collect();
        file.write_all(&pixels).unwrap();
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        return true;
    }
    if *frame > 180 {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        return true;
    }
    false
}
