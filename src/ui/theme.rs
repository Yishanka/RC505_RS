use eframe::egui::{self, Color32};

pub const ACCENT: Color32 = Color32::from_rgb(85, 221, 190);
pub const TRACK: Color32 = Color32::from_rgb(139, 167, 255);
pub const MUTED: Color32 = Color32::from_rgb(139, 153, 171);
pub const PANEL: Color32 = Color32::from_rgb(25, 31, 41);
pub const BACKGROUND: Color32 = Color32::from_rgb(15, 20, 28);

pub fn apply(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = BACKGROUND;
    style.visuals.window_fill = PANEL;
    style.visuals.extreme_bg_color = BACKGROUND;
    style.visuals.selection.bg_fill = Color32::from_rgb(37, 94, 86);
    style.visuals.selection.stroke = egui::Stroke::new(1.0, ACCENT);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(37, 45, 58);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(49, 65, 77);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(42, 101, 94);
    style.visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, ACCENT);
    style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, TRACK);
    style.visuals.widgets.inactive.rounding = egui::Rounding::same(6.0);
    style.visuals.widgets.hovered.rounding = egui::Rounding::same(6.0);
    style.visuals.widgets.active.rounding = egui::Rounding::same(6.0);
    style
        .text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::proportional(24.0));
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(16.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(15.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::proportional(13.0));
    style
        .text_styles
        .insert(egui::TextStyle::Monospace, egui::FontId::monospace(15.0));
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.spacing.slider_width = 180.0;
    style.spacing.scroll = egui::style::ScrollStyle::solid();
    ctx.set_style(style);
    for path in [r"C:\Windows\Fonts\msyh.ttc", r"C:\Windows\Fonts\simhei.ttf"] {
        if let Ok(bytes) = std::fs::read(path) {
            let mut fonts = egui::FontDefinitions::default();
            fonts
                .font_data
                .insert("cjk".into(), egui::FontData::from_owned(bytes));
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts.families.entry(family).or_default().push("cjk".into());
            }
            ctx.set_fonts(fonts);
            break;
        }
    }
}

pub fn card() -> egui::Frame {
    egui::Frame::none()
        .fill(PANEL)
        .rounding(10.0)
        .inner_margin(16.0)
        .stroke(egui::Stroke::new(1.0, Color32::from_rgb(43, 53, 67)))
}

pub fn caption(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(egui::RichText::new(text).small().color(MUTED));
}

/// Shared visual identity for the launcher, performance and preset workspaces.
pub fn brand(ui: &mut egui::Ui) {
    let id = egui::Id::new("rc505-brand-texture");
    let texture = ui
        .ctx()
        .data(|d| d.get_temp::<egui::TextureHandle>(id))
        .unwrap_or_else(|| {
            let icon = window_icon();
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [icon.width as usize, icon.height as usize],
                &icon.rgba,
            );
            let texture = ui
                .ctx()
                .load_texture("rc505-brand", image, egui::TextureOptions::LINEAR);
            ui.ctx().data_mut(|d| d.insert_temp(id, texture.clone()));
            texture
        });
    ui.add(egui::Image::new((texture.id(), egui::vec2(32.0, 32.0))));
    ui.label(
        egui::RichText::new("RC505 RS")
            .size(23.0)
            .strong()
            .color(ACCENT),
    );
}

pub fn window_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../../assets/rc505-rs-icon-v1-256.png"))
        .expect("Bundled RC505 RS icon is invalid")
}
