use crate::engine::spectrum::BARS;
use eframe::egui;
pub fn draw(ui: &egui::Ui, bars: &[f32; BARS]) {
    let rect = ui.max_rect();
    let width = rect.width() / BARS as f32;
    let center = rect.top() + rect.height() * 0.62;
    for (i, peak) in bars.iter().enumerate() {
        let height = peak.clamp(0.0, 1.0).sqrt() * rect.height() * 0.6;
        if height < 1.0 {
            continue;
        }
        let bar = egui::Rect::from_center_size(
            egui::pos2(rect.left() + (i as f32 + 0.5) * width, center),
            egui::vec2((width - 3.0).max(1.0), height),
        );
        ui.painter().rect_filled(bar, 2.0, {
            let color = super::theme::accent(ui);
            egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 26)
        });
    }
}
