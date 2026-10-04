//! Shared mouse-editable controls and visualizations for quick and expanded views.
pub use super::parameter_input::take_pending_text;
use super::theme;
use crate::config::config_type::{EnumConfig, NumericConfig};
use crate::config::envelope_configs::*;
use crate::config::filter_configs::*;
use eframe::egui::{self, Color32, Stroke, pos2};

pub fn number(ui: &mut egui::Ui, config: &mut NumericConfig, min: usize, max: usize, log: bool) {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    let mut value = config.value as f64;
    if super::parameter_input::slider(
        ui,
        &mut value,
        min as f64,
        max as f64,
        1.0,
        lang.text(&config.label),
        log,
    )
    .changed()
    {
        config.value = value.round() as usize;
        config.buffer.clear();
    }
}

pub fn float(
    ui: &mut egui::Ui,
    value: &mut f32,
    min: f32,
    max: f32,
    step: f32,
    label: &str,
    log: bool,
) -> egui::Response {
    float_with_keys(ui, value, min, max, step, 1.0, label, log)
}
pub fn float_with_keys(
    ui: &mut egui::Ui,
    value: &mut f32,
    min: f32,
    max: f32,
    precision: f32,
    keyboard_step: f32,
    label: &str,
    log: bool,
) -> egui::Response {
    let mut edit = f64::from(*value);
    let response = super::parameter_input::slider(
        ui,
        &mut edit,
        f64::from(min),
        f64::from(max),
        super::parameter_input::Step::new(f64::from(precision), f64::from(keyboard_step)),
        label,
        log,
    );
    *value = edit as f32;
    response
}
pub fn integer<N: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    value: &mut N,
    min: N,
    max: N,
    label: &str,
) -> egui::Response {
    let mut edit = value.to_f64();
    let response = super::parameter_input::slider(
        ui,
        &mut edit,
        min.to_f64(),
        max.to_f64(),
        1.0,
        label,
        false,
    );
    *value = N::from_f64(edit.round());
    response
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
        let delta = super::parameter_input::enum_step(&response);
        if !config.options.is_empty() && delta != 0 {
            let index = config
                .options
                .iter()
                .position(|option| *option == config.value)
                .unwrap_or(0);
            config.value = config.options
                [(index as i32 + delta).rem_euclid(config.options.len() as i32) as usize]
                .clone();
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
    let selected = options.iter().position(|(v, _)| v == value);
    let index = selected.unwrap_or(0);
    let response = egui::ComboBox::from_id_source(id)
        .selected_text(
            selected
                .map(|index| lang.text(options[index].1))
                .unwrap_or_else(|| lang.choose("Custom", "自定义")),
        )
        .show_ui(ui, |ui| {
            for (option, label) in options {
                ui.selectable_value(value, *option, lang.text(label));
            }
        })
        .response;
    let delta = super::parameter_input::enum_step(&response);
    if id == "kind" {
        ui.ctx()
            .data_mut(|data| data.insert_temp(egui::Id::new("fx-kind-picker"), response.id));
    }
    #[cfg(debug_assertions)]
    ui.ctx()
        .data_mut(|data| data.insert_temp(egui::Id::new(("selector", id)), response.id));
    if delta != 0 {
        let origin = if selected.is_some() {
            index as i32
        } else if delta > 0 {
            -1
        } else {
            0
        };
        *value = options[(origin + delta).rem_euclid(options.len() as i32) as usize].0;
    }
    previous != *value
}

pub fn filter_sweep(ui: &mut egui::Ui, sweep: &mut FilterSweepConfig, full: bool) {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    if full {
        ui.separator();
        theme::caption(ui, lang.choose("Cutoff sweep", "截止扫频"));
    }
    let mut depth = sweep.depth * 100.0;
    if float(
        ui,
        &mut depth,
        0.0,
        100.0,
        0.1,
        lang.choose("Sweep depth (%)", "扫频深度（%）"),
        false,
    )
    .changed()
    {
        sweep.depth = depth / 100.0;
    }
    super::navigation::register(
        ui.checkbox(&mut sweep.sync, lang.choose("Sync to tempo", "跟随拍速")),
    );
    if sweep.sync {
        selector(
            ui,
            "filter-sweep-beats",
            &mut sweep.beats,
            &[
                (0.0625, "1/64"),
                (0.125, "1/32"),
                (0.25, "1/16"),
                (0.5, "1/8"),
                (1.0, "1/4"),
                (2.0, "1/2"),
                (4.0, lang.choose("1 bar", "1 小节")),
                (8.0, lang.choose("2 bars", "2 小节")),
                (16.0, lang.choose("4 bars", "4 小节")),
                (32.0, lang.choose("8 bars", "8 小节")),
                (64.0, lang.choose("16 bars", "16 小节")),
            ],
        );
    } else {
        float_with_keys(
            ui,
            &mut sweep.rate_hz,
            0.01,
            20.0,
            0.01,
            0.1,
            lang.choose("Sweep rate (Hz)", "扫频速度（Hz）"),
            true,
        );
    }
    if full {
        super::navigation::register(
            ui.checkbox(&mut sweep.stepped, lang.choose("Stepped sweep", "阶梯扫频")),
        );
        ui.add_enabled_ui(sweep.stepped, |ui| {
            super::navigation::register(ui.checkbox(
                &mut sweep.step_sync,
                lang.choose("Sync steps", "阶梯跟随拍速"),
            ));
            if sweep.step_sync {
                selector(
                    ui,
                    "filter-step-beats",
                    &mut sweep.step_beats,
                    &[
                        (0.015625, "1/256"),
                        (0.03125, "1/128"),
                        (0.0625, "1/64"),
                        (0.125, "1/32"),
                        (0.25, "1/16"),
                        (0.5, "1/8"),
                        (1.0, "1/4"),
                        (2.0, "1/2"),
                        (4.0, lang.choose("1 bar", "1 小节")),
                        (8.0, lang.choose("2 bars", "2 小节")),
                        (16.0, lang.choose("4 bars", "4 小节")),
                    ],
                );
            } else {
                float(
                    ui,
                    &mut sweep.step_hz,
                    0.1,
                    100.0,
                    0.1,
                    lang.choose("Step rate (Hz)", "阶梯速度（Hz）"),
                    true,
                );
            }
        });
    }
    *sweep = sweep.sanitized();
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
    let mut q = config.resonance_x10.value as f32 / 10.0;
    if float_with_keys(
        ui,
        &mut q,
        FILTER_Q_MIN_X10 as f32 / 10.0,
        FILTER_Q_MAX_X10 as f32 / 10.0,
        0.1,
        0.1,
        lang.choose("Resonance (Q)", "共振（Q）"),
        false,
    )
    .changed()
    {
        config.resonance_x10.value = (q * 10.0).round() as usize;
    }
    number(ui, &mut config.drive, 0, FILTER_DRIVE_MAX, false);
    number(ui, &mut config.mix, 0, FILTER_MIX_MAX, false);
    if full {
        theme::caption(
            ui,
            lang.choose(
                "Filter response · drag cutoff / resonance",
                "滤波响应 · 拖动调整截止与共振",
            ),
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
            if float_with_keys(
                &mut cols[1],
                &mut curve,
                -1.0,
                1.0,
                0.01,
                0.05,
                lang.choose(en, cn),
                false,
            )
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
    theme::caption(
        ui,
        lang.choose(
            "Drag nodes for time and level; drag midpoints for curvature. The view stays fixed.",
            "拖节点调整时长和电平，拖中点调整曲率；视窗保持固定。",
        ),
    );
}
#[path = "envelope_view.rs"]
mod envelope_view;
fn envelope_curve(ui: &mut egui::Ui, c: &mut EnvelopeConfigs) {
    envelope_view::draw(ui, c);
}

fn tension(value: usize) -> f32 {
    2.0_f32.powf((value as f32 - 100.0) / 50.0)
}
