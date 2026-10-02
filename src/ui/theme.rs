use eframe::egui::{self, Color32};

use crate::app_support::appearance::ThemeColor;
pub fn accent(ui: &egui::Ui) -> Color32 {
    ui.visuals().selection.stroke.color
}
pub fn secondary(ui: &egui::Ui) -> Color32 {
    ui.visuals().widgets.hovered.bg_stroke.color
}
pub fn set_palette(ctx: &egui::Context, color: ThemeColor) {
    let key = egui::Id::new("theme-color");
    if ctx.data(|d| d.get_temp::<ThemeColor>(key)) == Some(color) {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(key, color));
    let (main, other) = match color {
        ThemeColor::Mint => (
            Color32::from_rgb(125, 194, 175),
            Color32::from_rgb(160, 177, 214),
        ),
        ThemeColor::Rose => (
            Color32::from_rgb(208, 156, 179),
            Color32::from_rgb(180, 164, 213),
        ),
        ThemeColor::Ember => (
            Color32::from_rgb(219, 157, 132),
            Color32::from_rgb(209, 182, 143),
        ),
    };
    let mut style = (*ctx.style()).clone();
    style.visuals.selection.bg_fill = main.gamma_multiply(0.32);
    style.visuals.selection.stroke = egui::Stroke::new(1.0, main);
    style.visuals.widgets.active.bg_fill = main.gamma_multiply(0.4);
    style.visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, main);
    style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, other);
    ctx.set_style(style);
}
pub fn theme_switch(ui: &mut egui::Ui, color: &mut ThemeColor) -> bool {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    let before = *color;
    ui.scope(|ui| {
        ui.style_mut().wrap = Some(false);
        let title = match color {
            ThemeColor::Mint => lang.choose("Mint", "薄荷绿"),
            ThemeColor::Rose => lang.choose("Rose", "雾粉"),
            ThemeColor::Ember => lang.choose("Ember", "橙红"),
        };
        ui.menu_button(
            format!("{} · {title}", lang.choose("Theme", "主题")),
            |ui| {
                for (value, en, zh) in [
                    (ThemeColor::Mint, "Mint", "薄荷绿"),
                    (ThemeColor::Rose, "Rose", "雾粉"),
                    (ThemeColor::Ember, "Ember", "橙红"),
                ] {
                    if ui
                        .selectable_value(color, value, lang.choose(en, zh))
                        .clicked()
                    {
                        ui.close_menu();
                    }
                }
            },
        );
    });
    if *color != before {
        set_palette(ui.ctx(), *color);
        true
    } else {
        false
    }
}
pub const MUTED: Color32 = Color32::from_rgb(139, 153, 171);
pub const PANEL: Color32 = Color32::from_rgb(25, 31, 41);
pub const BACKGROUND: Color32 = Color32::from_rgb(15, 20, 28);

pub fn apply(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = BACKGROUND;
    style.visuals.window_fill = PANEL;
    style.visuals.override_text_color = Some(Color32::from_rgb(215, 224, 236));
    style.visuals.extreme_bg_color = BACKGROUND;
    style.visuals.selection.bg_fill = Color32::from_rgb(37, 94, 86);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(37, 45, 58);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(49, 65, 77);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(42, 101, 94);
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
    ctx.data_mut(|d| d.remove::<ThemeColor>(egui::Id::new("theme-color")));
    set_palette(ctx, ThemeColor::Mint);
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
        .fill(Color32::from_rgba_unmultiplied(25, 31, 41, 232))
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
            .color(accent(ui)),
    );
}

pub fn window_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../../assets/rc505-rs-icon-v1-256.png"))
        .expect("Bundled RC505 RS icon is invalid")
}

pub fn language_switch(
    ui: &mut egui::Ui,
    language: &mut crate::app_support::language::Language,
) -> egui::Response {
    use crate::app_support::language::Language;
    let label = if *language == Language::Chinese {
        "中文 / EN"
    } else {
        "EN / 中文"
    };
    let mut response = ui
        .button(label)
        .on_hover_text("切换界面语言 / Switch interface language");
    if response.clicked() {
        *language = if *language == Language::Chinese {
            Language::English
        } else {
            Language::Chinese
        };
        language.apply(ui.ctx());
        response.mark_changed();
    }
    response
}

/// Keycaps are hints, not additional clickable controls.
pub fn keycap(ui: &mut egui::Ui, key: &str) {
    let text = ui.painter().layout_no_wrap(
        key.into(),
        egui::FontId::monospace(12.0),
        Color32::from_gray(220),
    );
    let (rect, response) =
        ui.allocate_exact_size(text.size() + egui::vec2(10.0, 4.0), egui::Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, key));
    ui.painter()
        .rect_filled(rect, 4.0, Color32::from_rgb(53, 61, 74));
    ui.painter().galley(
        rect.min + egui::vec2(5.0, 2.0),
        text,
        Color32::from_gray(220),
    );
}

/// Give controls a common row center before layout. Labels in a wrapped row
/// otherwise use egui's paragraph layout and sit above adjacent buttons.
pub fn control_row<R>(
    ui: &mut egui::Ui,
    draw: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 34.0),
        egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true),
        |ui| {
            ui.style_mut().wrap = Some(false);
            draw(ui)
        },
    )
}

#[derive(Clone, Copy)]
pub enum Icon {
    Play,
    Stop,
    Record,
    Back,
    Undo,
    Redo,
    Trash,
    Save,
    Expand,
    Help,
    None,
}

/// Standard egui button behavior (mouse, Enter, focus, accessibility) with
/// separate action and shortcut paint; icons never depend on a symbol font.
pub fn action(ui: &mut egui::Ui, icon: Icon, label: &str, key: &str) -> egui::Response {
    let text = ui.painter().layout_no_wrap(
        label.into(),
        egui::FontId::proportional(15.0),
        ui.visuals().text_color(),
    );
    let cap = ui.painter().layout_no_wrap(
        key.into(),
        egui::FontId::monospace(12.0),
        Color32::from_gray(224),
    );
    let icon_width = if matches!(icon, Icon::None) {
        0.0
    } else {
        22.0
    };
    let key_width = if key.is_empty() {
        0.0
    } else {
        cap.size().x + 18.0
    };
    let compact = label.is_empty() && key.is_empty();
    let width = if compact {
        30.0
    } else {
        text.size().x + icon_width + key_width + 20.0
    };
    let response = ui.add(egui::Button::new("").min_size(egui::vec2(width, 34.0)));
    let lang = crate::app_support::language::Language::current(ui.ctx());
    let accessible = if label.is_empty() {
        lang.text(match icon {
            Icon::Stop => "Stop",
            Icon::Play => "Play",
            Icon::Undo => "Undo",
            Icon::Redo => "Redo",
            Icon::Record => "Record",
            _ => "Action",
        })
    } else {
        label
    };
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, accessible));
    if ui.is_rect_visible(response.rect) {
        let color = if ui.is_enabled() {
            ui.visuals().text_color()
        } else {
            MUTED
        };
        let center = if compact {
            response.rect.center()
        } else {
            egui::pos2(response.rect.left() + 17.0, response.rect.center().y)
        };
        let stroke = egui::Stroke::new(1.5, color);
        let p = ui.painter();
        match icon {
            Icon::Undo | Icon::Redo => {
                let direction = if matches!(icon, Icon::Redo) {
                    -1.0
                } else {
                    1.0
                };
                let point = |x: f32, y: f32| center + egui::vec2(x * direction, y);
                p.add(egui::Shape::line(
                    vec![
                        point(-5.0, -2.0),
                        point(0.0, -5.0),
                        point(5.0, -2.0),
                        point(5.0, 3.0),
                        point(1.0, 6.0),
                        point(-2.0, 5.0),
                    ],
                    stroke,
                ));
                p.add(egui::Shape::line(
                    vec![point(-5.0, -6.0), point(-5.0, -2.0), point(-1.0, -2.0)],
                    stroke,
                ));
            }
            Icon::Play => {
                p.add(egui::Shape::convex_polygon(
                    vec![
                        center + egui::vec2(-5.0, -6.0),
                        center + egui::vec2(6.0, 0.0),
                        center + egui::vec2(-5.0, 6.0),
                    ],
                    accent(ui),
                    egui::Stroke::NONE,
                ));
            }
            Icon::Stop => {
                p.rect_filled(
                    egui::Rect::from_center_size(center, egui::vec2(10.0, 10.0)),
                    1.0,
                    color,
                );
            }
            Icon::Record => {
                p.circle_filled(center, 5.0, Color32::from_rgb(255, 109, 118));
            }
            Icon::Back => {
                p.line_segment(
                    [
                        center + egui::vec2(-6.0, 0.0),
                        center + egui::vec2(6.0, 0.0),
                    ],
                    stroke,
                );
                p.add(egui::Shape::line(
                    vec![
                        center + egui::vec2(-1.0, -5.0),
                        center + egui::vec2(-6.0, 0.0),
                        center + egui::vec2(-1.0, 5.0),
                    ],
                    stroke,
                ));
            }
            Icon::Trash => {
                p.rect_stroke(
                    egui::Rect::from_center_size(
                        center + egui::vec2(0.0, 2.0),
                        egui::vec2(9.0, 10.0),
                    ),
                    1.0,
                    stroke,
                );
                p.line_segment(
                    [
                        center + egui::vec2(-7.0, -5.0),
                        center + egui::vec2(7.0, -5.0),
                    ],
                    stroke,
                );
                p.line_segment(
                    [
                        center + egui::vec2(-2.0, -7.0),
                        center + egui::vec2(2.0, -7.0),
                    ],
                    stroke,
                );
            }
            Icon::Save => {
                p.rect_stroke(
                    egui::Rect::from_center_size(center, egui::vec2(12.0, 14.0)),
                    1.0,
                    stroke,
                );
                p.line_segment(
                    [
                        center + egui::vec2(-3.0, 2.0),
                        center + egui::vec2(3.0, 2.0),
                    ],
                    stroke,
                );
            }
            Icon::Expand => {
                p.rect_stroke(
                    egui::Rect::from_center_size(center, egui::vec2(13.0, 11.0)),
                    1.0,
                    stroke,
                );
            }
            Icon::Help => {
                p.circle_stroke(center, 7.0, stroke);
                p.text(
                    center,
                    egui::Align2::CENTER_CENTER,
                    "?",
                    egui::FontId::proportional(13.0),
                    color,
                );
            }
            Icon::None => {}
        }
        p.galley(
            egui::pos2(
                response.rect.left() + 10.0 + icon_width,
                response.rect.center().y - text.size().y / 2.0,
            ),
            text,
            color,
        );
        if !key.is_empty() {
            let rect = egui::Rect::from_center_size(
                egui::pos2(
                    response.rect.right() - 10.0 - (cap.size().x + 10.0) / 2.0,
                    response.rect.center().y,
                ),
                cap.size() + egui::vec2(10.0, 5.0),
            );
            p.rect_filled(rect, 4.0, Color32::from_rgb(53, 61, 74));
            p.galley(rect.min + egui::vec2(5.0, 2.5), cap, color);
        }
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn action_button_accepts_enter_with_separate_keycap() {
        let ctx = egui::Context::default();
        let mut clicked = false;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                action(ui, Icon::Back, "Projects", "Esc").request_focus();
            });
        });
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    clicked = action(ui, Icon::Back, "Projects", "Esc").clicked();
                });
            },
        );
        assert!(clicked);
    }
}
