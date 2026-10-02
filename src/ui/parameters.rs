//! Shared mouse-editable controls and visualizations for quick and expanded views.
use super::theme;
use crate::config::config_type::{EnumConfig, NumericConfig};
use crate::config::envelope_configs::*;
use crate::config::filter_configs::*;
use crate::dsp::envelope::{AhdsrParams, AhdsrState};
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
    ui.columns(2, |columns| {
        number(
            &mut columns[0],
            &mut config.attack_ms,
            0,
            ENVELOPE_ATTACK_MAX_MS,
            false,
        );
        number(
            &mut columns[0],
            &mut config.hold_ms,
            0,
            ENVELOPE_HOLD_MAX_MS,
            false,
        );
        number(
            &mut columns[0],
            &mut config.decay_ms,
            0,
            ENVELOPE_DECAY_MAX_MS,
            false,
        );
        number(&mut columns[0], &mut config.sustain_pct, 0, 100, false);
        number(
            &mut columns[0],
            &mut config.release_ms,
            1,
            ENVELOPE_RELEASE_MAX_MS,
            false,
        );
        number(&mut columns[1], &mut config.start_pct, 0, 100, false);
        number(
            &mut columns[1],
            &mut config.tension_a,
            0,
            ENVELOPE_TENSION_MAX,
            false,
        );
        number(
            &mut columns[1],
            &mut config.tension_d,
            0,
            ENVELOPE_TENSION_MAX,
            false,
        );
        number(
            &mut columns[1],
            &mut config.tension_r,
            0,
            ENVELOPE_TENSION_MAX,
            false,
        );
    });
    let params = AhdsrParams {
        attack_ms: config.attack_ms.value as f32,
        hold_ms: config.hold_ms.value as f32,
        decay_ms: config.decay_ms.value as f32,
        sustain_level: config.sustain_pct.value as f32 / 100.0,
        release_ms: config.release_ms.value.max(1) as f32,
        start_level: config.start_pct.value as f32 / 100.0,
        tension_attack: tension(config.tension_a.value),
        tension_decay: tension(config.tension_d.value),
        tension_release: tension(config.tension_r.value),
    };
    let gate_ms = params.attack_ms + params.hold_ms + params.decay_ms + 300.0;
    let total_ms = gate_ms + params.release_ms + 100.0;
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 200.0),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(rect, 6.0, theme::BACKGROUND);
    let plot = rect.shrink(12.0);
    let mut env = AhdsrState::new();
    let points = (0..600)
        .map(|i| {
            let elapsed = total_ms * i as f32 / 600.0;
            let level = env.next(elapsed < gate_ms, false, params, total_ms / 600000.0);
            pos2(
                plot.left() + plot.width() * i as f32 / 599.0,
                plot.bottom() - level * plot.height(),
            )
        })
        .collect();
    ui.painter().add(egui::Shape::line(
        points,
        Stroke::new(2.0, theme::accent(ui)),
    ));
    let off_x = plot.left() + plot.width() * gate_ms / total_ms;
    ui.painter()
        .vline(off_x, plot.y_range(), Stroke::new(1.0, theme::MUTED));
    theme::caption(
        ui,
        format!(
            "A / H / D / S / R • {:.2} s preview • vertical line = note off • Tension 100 = linear",
            total_ms / 1000.0
        ),
    );
}

fn tension(value: usize) -> f32 {
    2.0_f32.powf((value as f32 - 100.0) / 50.0)
}
