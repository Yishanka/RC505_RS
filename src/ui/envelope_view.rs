//! Stable envelope viewport: only explicit user navigation changes its scale.
use super::{tension, theme};
use crate::config::envelope_configs::*;
use eframe::egui::{self, Color32, Stroke, pos2};
#[derive(Clone, Copy, Debug, PartialEq)]
struct Viewport {
    span_ms: f32,
    offset_ms: f32,
}
impl Default for Viewport {
    fn default() -> Self {
        Self {
            span_ms: 1000.0,
            offset_ms: 0.0,
        }
    }
}
impl Viewport {
    fn sanitize(&mut self) {
        if !self.span_ms.is_finite() {
            self.span_ms = 1000.0;
        }
        self.span_ms = self.span_ms.clamp(100.0, 30000.0);
        if !self.offset_ms.is_finite() {
            self.offset_ms = 0.0;
        }
        self.offset_ms = self.offset_ms.clamp(0.0, 30000.0 - self.span_ms);
    }
}
#[cfg(test)]
#[derive(Clone, Copy, Debug)]
struct Geometry {
    id: egui::Id,
    view: Viewport,
    plot: egui::Rect,
    attack: egui::Pos2,
    release: egui::Pos2,
}
fn drag_time(value: f32, min: f32, max: f32) -> f32 {
    (value.clamp(min, max) * 10.0).round() / 10.0
}
pub(super) fn draw(ui: &mut egui::Ui, c: &mut EnvelopeConfigs) {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    let view_id = ui.id().with("envelope_fixed_view");
    let mut view = ui
        .ctx()
        .data(|data| data.get_temp::<Viewport>(view_id))
        .unwrap_or_default();
    theme::control_row(ui, |ui| {
        ui.label(lang.choose("Time view", "时间视窗"));
        super::selector(
            ui,
            "envelope_time_span",
            &mut view.span_ms,
            &[
                (100.0, "100 ms"),
                (500.0, "500 ms"),
                (1000.0, "1 s"),
                (5000.0, "5 s"),
                (30000.0, "30 s"),
            ],
        );
        if crate::ui::navigation::register(ui.button("←"))
            .on_hover_text(lang.choose("Earlier", "向前查看"))
            .clicked()
        {
            view.offset_ms = (view.offset_ms - view.span_ms * 0.5).max(0.0);
        }
        if crate::ui::navigation::register(ui.button("→"))
            .on_hover_text(lang.choose("Later", "向后查看"))
            .clicked()
        {
            view.offset_ms += view.span_ms * 0.5;
        }
        if crate::ui::navigation::register(ui.button(lang.choose("Start", "回到起点"))).clicked()
        {
            view.offset_ms = 0.0;
        }
    });
    view.sanitize();
    super::float(
        ui,
        &mut view.offset_ms,
        0.0,
        (30000.0 - view.span_ms).max(0.0),
        1.0,
        lang.choose("View start (ms)", "视窗起点（ms）"),
        false,
    );
    ui.ctx().data_mut(|data| data.insert_temp(view_id, view));
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 230.0),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(rect, 6.0, theme::BACKGROUND);
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    let plot = rect.shrink2(egui::vec2(20.0, 28.0));
    let a = c.attack_ms.value as f32;
    let h = c.hold_ms.value as f32;
    let d = c.decay_ms.value as f32;
    let r = c.release_ms.value.max(ENVELOPE_RELEASE_MIN_MS);
    let sustain_hold = 300.0;
    let scale = view.span_ms;
    let sustain = c.sustain_pct.value as f32 / 100.0;
    let start = c.start_pct.value as f32 / 100.0;
    let off = a + h + d + sustain_hold;
    let point = |ms: f32, v: f32| {
        pos2(
            plot.left() + plot.width() * (ms - view.offset_ms) / scale,
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
        painter.hline(plot.x_range(), y, Stroke::new(1.0, Color32::from_gray(45)));
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
            let time = view.offset_ms + scale * i as f32 / 599.0;
            point(time, shape(time))
        })
        .collect();
    painter.add(egui::Shape::line(
        points,
        Stroke::new(2.0, theme::accent(ui)),
    ));
    painter.vline(
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
        if pos.x < plot.left() - 1.0 || pos.x > plot.right() + 1.0 {
            continue;
        }
        let hit = egui::Rect::from_center_size(pos, egui::vec2(14.0, 14.0));
        let response = ui.interact(
            hit,
            ui.id().with(("envelope_node", index)),
            egui::Sense::drag(),
        );
        painter.circle_filled(
            pos,
            5.0,
            if index == 4 {
                theme::MUTED
            } else {
                theme::accent(ui)
            },
        );
        let above = *value > 0.8 || *value < 0.1;
        let (label_pos, align) = if index == 0 {
            (pos + egui::vec2(8.0, -6.0), egui::Align2::LEFT_BOTTOM)
        } else {
            (
                pos + egui::vec2(0.0, if above { -8.0 } else { 10.0 }),
                if above {
                    egui::Align2::CENTER_BOTTOM
                } else {
                    egui::Align2::CENTER_TOP
                },
            )
        };
        painter.text(
            label_pos,
            align,
            label,
            egui::FontId::monospace(12.0),
            theme::MUTED,
        );
        if response.dragged() {
            if let Some(p) = response.interact_pointer_pos() {
                let ms = (view.offset_ms + (p.x - plot.left()) / plot.width() * scale).max(0.0);
                let level =
                    ((plot.bottom() - p.y) / plot.height() * 100.0).clamp(0.0, 100.0) as usize;
                match index {
                    0 => c.start_pct.value = level,
                    1 => c.attack_ms.value = drag_time(ms, 0.0, ENVELOPE_ATTACK_MAX_MS),
                    2 => c.hold_ms.value = drag_time(ms - a, 0.0, ENVELOPE_HOLD_MAX_MS),
                    3 => {
                        c.decay_ms.value = drag_time(ms - a - h, 0.0, ENVELOPE_DECAY_MAX_MS);
                        c.sustain_pct.value = level;
                    }
                    4 => c.sustain_pct.value = level,
                    5 => {
                        c.release_ms.value =
                            drag_time(ms - off, ENVELOPE_RELEASE_MIN_MS, ENVELOPE_RELEASE_MAX_MS)
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
        if pos.x < plot.left() || pos.x > plot.right() {
            continue;
        }
        let response = ui.interact(
            egui::Rect::from_center_size(pos, egui::vec2(12.0, 12.0)),
            ui.id().with(("envelope_midpoint", index)),
            egui::Sense::click_and_drag(),
        );
        painter.circle_filled(pos, 3.5, theme::secondary(ui));
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
    for i in 0..=4 {
        let time = view.offset_ms + view.span_ms * i as f32 / 4.0;
        let x = point(time, 0.0).x;
        painter.text(
            pos2(x, rect.bottom() - 2.0),
            if i == 0 {
                egui::Align2::LEFT_BOTTOM
            } else if i == 4 {
                egui::Align2::RIGHT_BOTTOM
            } else {
                egui::Align2::CENTER_BOTTOM
            },
            format!("{time:.0} ms"),
            egui::FontId::monospace(11.0),
            theme::MUTED,
        );
    }
    #[cfg(test)]
    ui.ctx().data_mut(|data| {
        data.insert_temp(
            egui::Id::new("envelope_test_geometry"),
            Geometry {
                id: view_id,
                view,
                plot,
                attack: point(a, 1.0),
                release: point(off + r, 0.0),
            },
        )
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(ctx: &egui::Context, c: &mut EnvelopeConfigs, events: Vec<egui::Event>) -> Geometry {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100.0, 800.0),
            )),
            events,
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| draw(ui, c));
        });
        ctx.data(|d| d.get_temp::<Geometry>(egui::Id::new("envelope_test_geometry")))
            .unwrap()
    }
    fn pointer(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }
    #[test]
    fn parameter_changes_do_not_resize_or_recenter_the_envelope() {
        let ctx = egui::Context::default();
        let mut config = EnvelopeConfigs::new();
        let first = frame(&ctx, &mut config, vec![]);
        config.release_ms.value = 5000.0;
        config.decay_ms.value = 10000.0;
        let next = frame(&ctx, &mut config, vec![]);
        assert_eq!(first.view, next.view);
        assert_eq!(first.plot, next.plot);
        assert_eq!(first.attack, next.attack);
        assert!(next.release.x > next.plot.right());
        let fixed = Viewport {
            span_ms: 500.0,
            offset_ms: 300.0,
        };
        ctx.data_mut(|data| data.insert_temp(next.id, fixed));
        config.attack_ms.value = 300.0;
        config.release_ms.value = 20.0;
        let changed = frame(&ctx, &mut config, vec![]);
        assert_eq!(changed.view, fixed);
        assert_eq!(changed.plot, next.plot);
    }
    #[test]
    fn dragging_then_releasing_keeps_the_same_time_scale() {
        let ctx = egui::Context::default();
        let mut config = EnvelopeConfigs::new();
        config.attack_ms.value = 50.0;
        let before = frame(&ctx, &mut config, vec![]);
        let start = before.attack;
        let end = pos2(before.plot.left() + before.plot.width() * 0.2, start.y);
        frame(
            &ctx,
            &mut config,
            vec![egui::Event::PointerMoved(start), pointer(start, true)],
        );
        frame(&ctx, &mut config, vec![egui::Event::PointerMoved(end)]);
        frame(&ctx, &mut config, vec![pointer(end, false)]);
        let after = frame(&ctx, &mut config, vec![]);
        assert!((config.attack_ms.value as i32 - 200).abs() <= 1);
        assert_eq!(before.view, after.view);
        assert_eq!(before.plot, after.plot);
        assert!((after.attack.x - end.x).abs() < 2.0);
    }
    #[test]
    fn fractional_envelope_canvas_keeps_tenths_for_attack_and_release() {
        let ctx = egui::Context::default();
        let mut config = EnvelopeConfigs::new();
        let initial = frame(&ctx, &mut config, vec![]);
        ctx.data_mut(|d| {
            d.insert_temp(
                initial.id,
                Viewport {
                    span_ms: 100.0,
                    offset_ms: 0.0,
                },
            )
        });
        let geometry = frame(&ctx, &mut config, vec![]);
        let start = geometry.attack;
        let target = pos2(
            geometry.plot.left() + geometry.plot.width() * 0.013,
            start.y,
        );
        frame(
            &ctx,
            &mut config,
            vec![egui::Event::PointerMoved(start), pointer(start, true)],
        );
        frame(&ctx, &mut config, vec![egui::Event::PointerMoved(target)]);
        frame(&ctx, &mut config, vec![pointer(target, false)]);
        assert!((config.attack_ms.value - 1.3).abs() < 0.00001);
        config.attack_ms.value = 0.0;
        config.hold_ms.value = 0.0;
        config.decay_ms.value = 0.0;
        config.release_ms.value = 5.0;
        ctx.data_mut(|d| {
            d.insert_temp(
                initial.id,
                Viewport {
                    span_ms: 100.0,
                    offset_ms: 295.0,
                },
            )
        });
        let geometry = frame(&ctx, &mut config, vec![]);
        let start = geometry.release;
        let target = pos2(
            geometry.plot.left() + geometry.plot.width() * 0.067,
            start.y,
        );
        frame(
            &ctx,
            &mut config,
            vec![egui::Event::PointerMoved(start), pointer(start, true)],
        );
        frame(&ctx, &mut config, vec![egui::Event::PointerMoved(target)]);
        frame(&ctx, &mut config, vec![pointer(target, false)]);
        assert!((config.release_ms.value - 1.7).abs() < 0.00001);
    }
}
