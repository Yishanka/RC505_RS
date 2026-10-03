//! Shared mouse-editable controls and visualizations for quick and expanded views.
use super::theme;
use crate::config::config_type::{EnumConfig, NumericConfig};
use crate::config::envelope_configs::*;
use crate::config::filter_configs::*;
use eframe::egui::{self, Color32, Stroke, pos2};

pub fn number(ui: &mut egui::Ui, config: &mut NumericConfig, min: usize, max: usize, log: bool) {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    if super::navigation::register(
        ui.add(
            egui::Slider::new(&mut config.value, min..=max)
                .text(lang.text(&config.label))
                .logarithmic(log),
        ),
    )
    .changed()
    {
        config.buffer.clear();
    }
}

pub fn choice<T: Clone + PartialEq + std::fmt::Display>(
    ui: &mut egui::Ui,
    config: &mut EnumConfig<T>,
) {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    let device = config.label.ends_with("Device");
    if device {
        ui.label(lang.text(&config.label));
    }
    ui.horizontal(|ui| {
        let response = egui::ComboBox::from_id_source(&config.label)
            .wrap(device)
            .selected_text(if config.label.ends_with("Device") {
                config.value.to_string()
            } else {
                lang.text(&config.value.to_string()).to_owned()
            })
            .show_ui(ui, |ui| {
                for value in &config.options {
                    let label = value.to_string();
                    ui.selectable_value(
                        &mut config.value,
                        value.clone(),
                        if config.label.ends_with("Device") {
                            &label
                        } else {
                            lang.text(&label)
                        },
                    );
                }
            })
            .response;
        let response = super::navigation::register(response);
        if response.has_focus() {
            if ui.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
                config.prev();
            }
            if ui.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
                config.next();
            }
        }
        if !device {
            ui.label(lang.text(&config.label));
        }
    });
}

pub fn selector<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    id: &str,
    value: &mut T,
    options: &[(T, &str)],
) -> bool {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    let previous = *value;
    let index = options.iter().position(|(v, _)| v == value).unwrap_or(0);
    let response = egui::ComboBox::from_id_source(id)
        .selected_text(lang.text(options[index].1))
        .show_ui(ui, |ui| {
            for (option, label) in options {
                ui.selectable_value(value, *option, lang.text(label));
            }
        })
        .response;
    if response.has_focus() {
        let delta = ui.input(|i| {
            i32::from(i.key_pressed(egui::Key::ArrowRight))
                - i32::from(i.key_pressed(egui::Key::ArrowLeft))
        });
        if delta != 0 {
            *value = options[(index as i32 + delta).rem_euclid(options.len() as i32) as usize].0;
        }
    }
    super::navigation::register(response);
    previous != *value
}

pub fn filter(ui: &mut egui::Ui, config: &mut FilterConfigs, full: bool) {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    choice(ui, &mut config.filter_type);
    number(
        ui,
        &mut config.cutoff_hz,
        FILTER_CUTOFF_MIN_HZ,
        FILTER_CUTOFF_MAX_HZ,
        true,
    );
    number(
        ui,
        &mut config.resonance_x10,
        FILTER_Q_MIN_X10,
        FILTER_Q_MAX_X10,
        false,
    );
    number(ui, &mut config.drive, 0, FILTER_DRIVE_MAX, false);
    number(ui, &mut config.mix, 0, FILTER_MIX_MAX, false);
    if full {
        theme::caption(
            ui,
            lang.text("FILTER RESPONSE / steady-state, 48 kHz • drag to set cutoff and resonance"),
        );
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), 210.0),
            egui::Sense::click_and_drag(),
        );
        let plot = rect.shrink2(egui::vec2(16.0, 22.0));
        ui.painter().rect_filled(rect, 6.0, theme::BACKGROUND);
        if let Some(pos) = response
            .interact_pointer_pos()
            .filter(|_| response.clicked() || response.dragged())
        {
            config.cutoff_hz.value = (20.0
                * 1000.0_f32.powf(((pos.x - plot.left()) / plot.width()).clamp(0.0, 1.0)))
                as usize;
            config.resonance_x10.value = (1.0
                + (1.0 - (pos.y - plot.top()) / plot.height()).clamp(0.0, 1.0) * 99.0)
                as usize;
        }
        for hz in [20.0_f32, 100.0, 1000.0, 10000.0, 20000.0] {
            let x = plot.left() + (hz / 20.0).log10() / 3.0 * plot.width();
            ui.painter()
                .vline(x, plot.y_range(), Stroke::new(1.0, Color32::from_gray(50)));
            ui.painter().text(
                pos2(x, rect.bottom() - 4.0),
                egui::Align2::CENTER_BOTTOM,
                format!("{hz:.0}"),
                egui::FontId::monospace(12.0),
                theme::MUTED,
            );
        }
        let points = (0..256)
            .map(|i| {
                let t = i as f32 / 255.0;
                let hz = 20.0 * 1000.0_f32.powf(t);
                let db = crate::dsp::filter::response_db(
                    config.filter_type.value,
                    config.cutoff_hz.value as f32,
                    config.resonance_x10.value as f32 / 10.0,
                    config.mix.value as f32 / 100.0,
                    hz,
                    48000.0,
                );
                pos2(
                    plot.left() + t * plot.width(),
                    plot.bottom() - (db.clamp(-48.0, 24.0) + 48.0) / 72.0 * plot.height(),
                )
            })
            .collect();
        ui.painter().add(egui::Shape::line(
            points,
            Stroke::new(2.0, theme::accent(ui)),
        ));
        theme::caption(
            ui,
            lang.text("Linear response includes dry/wet; drive and envelope motion are not shown."),
        );
    }
}

pub fn envelope(ui: &mut egui::Ui, config: &mut EnvelopeConfigs) {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    // Curve and precise controls edit the very same values. Zero-duration segments
    // remain selectable through the labels/knobs even when their nodes coincide.
    envelope_curve(ui, config);
    ui.columns(2, |cols| {
        number(
            &mut cols[0],
            &mut config.attack_ms,
            0,
            ENVELOPE_ATTACK_MAX_MS,
            true,
        );
        number(
            &mut cols[0],
            &mut config.hold_ms,
            0,
            ENVELOPE_HOLD_MAX_MS,
            true,
        );
        number(
            &mut cols[0],
            &mut config.decay_ms,
            0,
            ENVELOPE_DECAY_MAX_MS,
            true,
        );
        number(&mut cols[0], &mut config.sustain_pct, 0, 100, false);
        number(
            &mut cols[0],
            &mut config.release_ms,
            1,
            ENVELOPE_RELEASE_MAX_MS,
            true,
        );
        number(&mut cols[1], &mut config.start_pct, 0, 100, false);
        for (value, en, cn) in [
            (&mut config.tension_a, "Attack curve", "起音曲率"),
            (&mut config.tension_d, "Decay curve", "衰减曲率"),
            (&mut config.tension_r, "Release curve", "释音曲率"),
        ] {
            let mut curve = ((value.value as f32 - 100.0) / 100.0).clamp(-1.0, 1.0);
            if cols[1]
                .add(egui::Slider::new(&mut curve, -1.0..=1.0).text(lang.choose(en, cn)))
                .changed()
            {
                value.value = (100.0 + curve * 100.0).round() as usize;
            }
            if value.value > 200 {
                theme::caption(
                    &mut cols[1],
                    lang.choose(
                        "Legacy steep curve retained; moving this control replaces it.",
                        "已保留旧版陡峭曲线；拖动此控件会替换它。",
                    ),
                );
            }
        }
        if cols[1]
            .button(lang.choose("Reset to gentle envelope", "重置为平滑包络"))
            .clicked()
        {
            *config = EnvelopeConfigs::new();
        }
    });
    theme::caption(ui,lang.choose("Drag A/H/D/R nodes horizontally for time; drag Start/S vertically for level. Small midpoint handles bend the curve. Values and graph stay synchronized; the dotted line is Note Off.","横拖 A/H/D/R 节点调整时间，竖拖 Start/S 调整电平；小中点调整曲率。参数与图形同步，虚线为音符松开。"));
}
fn envelope_curve(ui: &mut egui::Ui, c: &mut EnvelopeConfigs) {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 230.0),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(rect, 6.0, theme::BACKGROUND);
    let plot = rect.shrink2(egui::vec2(20.0, 28.0));
    let a = c.attack_ms.value as f32;
    let h = c.hold_ms.value as f32;
    let d = c.decay_ms.value as f32;
    let r = c.release_ms.value.max(1) as f32;
    let sustain_hold = 300.0;
    let total = (a + h + d + r + sustain_hold).max(500.0);
    let snapshot_id = ui.id().with("envelope_drag_scale");
    let any_down = ui.input(|i| i.pointer.primary_down());
    if !any_down {
        ui.ctx().data_mut(|data| data.remove::<f32>(snapshot_id));
    }
    let scale = ui.ctx().data_mut(|data| {
        if any_down {
            *data.get_temp_mut_or_insert_with(snapshot_id, || total)
        } else {
            total
        }
    });
    let sustain = c.sustain_pct.value as f32 / 100.0;
    let start = c.start_pct.value as f32 / 100.0;
    let off = a + h + d + sustain_hold;
    let point = |ms: f32, v: f32| {
        pos2(
            plot.left() + plot.width() * ms / scale,
            plot.bottom() - plot.height() * v,
        )
    };
    let anchors = [
        (0.0, start, lang.choose("Start", "起点")),
        (a, 1.0, if h == 0.0 { "A/H" } else { "A" }),
        (a + h, 1.0, "H"),
        (a + h + d, sustain, "D / S"),
        (off, sustain, lang.choose("Off", "松键")),
        (off + r, 0.0, "R"),
    ];
    for n in 0..=4 {
        let y = plot.bottom() - plot.height() * n as f32 / 4.0;
        ui.painter()
            .hline(plot.x_range(), y, Stroke::new(1.0, Color32::from_gray(45)));
    }
    let shape = |ms: f32| {
        if ms < a && a > 0.0 {
            start + (1.0 - start) * (ms / a).powf(tension(c.tension_a.value))
        } else if ms < a + h {
            1.0
        } else if ms < a + h + d && d > 0.0 {
            sustain
                + (1.0 - sustain)
                    * (1.0 - (ms - a - h) / d)
                        .clamp(0.0, 1.0)
                        .powf(tension(c.tension_d.value))
        } else if ms < off {
            sustain
        } else {
            sustain
                * (1.0 - (ms - off) / r)
                    .clamp(0.0, 1.0)
                    .powf(tension(c.tension_r.value))
        }
    };
    let points = (0..600)
        .map(|i| {
            let time = scale * i as f32 / 599.0;
            point(time, shape(time))
        })
        .collect();
    ui.painter().add(egui::Shape::line(
        points,
        Stroke::new(2.0, theme::accent(ui)),
    ));
    ui.painter().vline(
        point(off, 0.0).x,
        plot.y_range(),
        Stroke::new(1.0, theme::MUTED),
    );
    for (index, (time, value, label)) in anchors.iter().enumerate() {
        // A zero hold shares the attack endpoint. One hit target avoids an
        // invisible H handle stealing drags intended for the visible A peak.
        if index == 2 && h == 0.0 {
            continue;
        }
        let pos = point(*time, *value);
        let hit = egui::Rect::from_center_size(pos, egui::vec2(14.0, 14.0));
        let response = ui.interact(
            hit,
            ui.id().with(("envelope_node", index)),
            egui::Sense::drag(),
        );
        ui.painter().circle_filled(
            pos,
            5.0,
            if index == 4 {
                theme::MUTED
            } else {
                theme::accent(ui)
            },
        );
        ui.painter().text(
            pos + egui::vec2(0.0, if *value > 0.8 { -8.0 } else { 10.0 }),
            if *value > 0.8 {
                egui::Align2::CENTER_BOTTOM
            } else {
                egui::Align2::CENTER_TOP
            },
            label,
            egui::FontId::monospace(12.0),
            theme::MUTED,
        );
        if response.dragged() {
            if let Some(p) = response.interact_pointer_pos() {
                let ms = ((p.x - plot.left()) / plot.width() * scale).max(0.0);
                let level =
                    ((plot.bottom() - p.y) / plot.height() * 100.0).clamp(0.0, 100.0) as usize;
                match index {
                    0 => c.start_pct.value = level,
                    1 => c.attack_ms.value = (ms as usize).min(ENVELOPE_ATTACK_MAX_MS),
                    2 => c.hold_ms.value = ((ms - a).max(0.0) as usize).min(ENVELOPE_HOLD_MAX_MS),
                    3 => {
                        c.decay_ms.value =
                            ((ms - a - h).max(0.0) as usize).min(ENVELOPE_DECAY_MAX_MS);
                        c.sustain_pct.value = level;
                    }
                    4 => c.sustain_pct.value = level,
                    5 => {
                        c.release_ms.value =
                            ((ms - off).max(1.0) as usize).min(ENVELOPE_RELEASE_MAX_MS)
                    }
                    _ => {}
                }
            }
        }
    }
    for (index, begin, end, y0, y1, value) in [
        (0, 0.0, a, start, 1.0, &mut c.tension_a),
        (1, a + h, a + h + d, 1.0, sustain, &mut c.tension_d),
        (2, off, off + r, sustain, 0.0, &mut c.tension_r),
    ] {
        if end - begin < 1.0 {
            continue;
        }
        let mid_value = if index == 0 {
            y0 + (y1 - y0) * 0.5f32.powf(tension(value.value))
        } else {
            y1 + (y0 - y1) * 0.5f32.powf(tension(value.value))
        };
        let pos = point((begin + end) * 0.5, mid_value);
        let response = ui.interact(
            egui::Rect::from_center_size(pos, egui::vec2(12.0, 12.0)),
            ui.id().with(("envelope_midpoint", index)),
            egui::Sense::click_and_drag(),
        );
        ui.painter().circle_filled(pos, 3.5, theme::secondary(ui));
        if response.dragged() {
            if let Some(p) = response.interact_pointer_pos() {
                let y = ((plot.bottom() - p.y) / plot.height()).clamp(0.0, 1.0);
                let normalized = if index == 0 {
                    (y - y0) / (y1 - y0).max(0.001)
                } else {
                    (y - y1) / (y0 - y1).max(0.001)
                };
                let exponent = normalized.clamp(0.01, 0.99).ln() / 0.5f32.ln();
                value.value = (100.0 + 50.0 * exponent.log2()).clamp(0.0, 200.0).round() as usize;
            }
        }
        if response.double_clicked() {
            value.value = 100;
        }
    }
}

fn tension(value: usize) -> f32 {
    2.0_f32.powf((value as f32 - 100.0) / 50.0)
}
