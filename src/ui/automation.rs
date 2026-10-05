use super::{navigation, parameters, theme};
use crate::{
    config::{
        AppConfig,
        automation::{Family, Interpolation, MAX_POINTS, PPQ, ParameterLane, Point, Target},
    },
    presets::FxTarget,
};
use eframe::egui::{self, Color32, Key, Rect, Stroke, pos2, vec2};
#[derive(Default)]
pub struct EditorState {
    selected: usize,
    snap: u32,
    drag: Option<(usize, bool, ParameterLane)>,
    undo: Vec<ParameterLane>,
    redo: Vec<ParameterLane>,
    #[cfg(test)]
    plot: Option<Rect>,
}
impl EditorState {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    fn remember(&mut self, before: ParameterLane, after: &ParameterLane) {
        if before != *after {
            if self.undo.len() == 64 {
                self.undo.remove(0);
            }
            self.undo.push(before);
            self.redo.clear();
        }
    }
    fn undo(&mut self, lane: &mut ParameterLane) {
        let old = self
            .drag
            .take()
            .map(|(_, _, old)| old)
            .or_else(|| self.undo.pop());
        if let Some(old) = old {
            self.redo.push(lane.clone());
            *lane = old;
            self.selected = 0;
        }
    }
    fn redo(&mut self, lane: &mut ParameterLane) {
        self.drag = None;
        if let Some(next) = self.redo.pop() {
            self.undo.push(lane.clone());
            *lane = next;
            self.selected = 0;
        }
    }
}
pub fn family(config: &AppConfig, target: FxTarget) -> Option<Family> {
    match target {
        FxTarget::Input { bank, slot } => crate::config::automation::input_family(
            config.input_fx.banks[bank].slots[slot].fx.as_ref(),
        ),
        FxTarget::Track { bank, slot } => crate::config::automation::track_family(
            config.track_fx.banks[bank].slots[slot].fx.as_ref(),
        ),
    }
}
fn lane_mut(config: &mut AppConfig, target: FxTarget) -> &mut ParameterLane {
    match target {
        FxTarget::Input { bank, slot } => {
            &mut config.input_fx.banks[bank].slots[slot].parameter_lane
        }
        FxTarget::Track { bank, slot } => {
            &mut config.track_fx.banks[bank].slots[slot].parameter_lane
        }
    }
}
fn targets(family: Family) -> &'static [Target] {
    match family {
        Family::Filter => &[Target::FilterCutoff, Target::FilterQ],
        Family::Delay => &[Target::DelayTime, Target::DelayFeedback, Target::DelayWet],
        Family::Reverb => &[Target::ReverbDecay, Target::ReverbWet],
        Family::ReverbAudio => &[Target::ReverbAudioDecay, Target::ReverbWet],
    }
}
fn name(target: Target, lang: crate::app_support::language::Language) -> &'static str {
    match target {
        Target::FilterCutoff => lang.choose("Cutoff (Hz)", "截止频率（Hz）"),
        Target::FilterQ => lang.choose("Resonance (Q)", "共振（Q）"),
        Target::DelayTime => lang.choose("Delay time (ms)", "延迟时间（ms）"),
        Target::DelayFeedback => lang.choose("Feedback (%)", "反馈（%）"),
        Target::DelayWet => lang.choose("Delay effect level (%)", "延迟效果电平（%）"),
        Target::ReverbDecay | Target::ReverbAudioDecay => {
            lang.choose("Decay (ms)", "衰减时间（ms）")
        }
        Target::ReverbWet => lang.choose("Reverb wet / mix (%)", "混响湿声／干湿（%）"),
    }
}
fn percentage(target: Target) -> bool {
    matches!(
        target,
        Target::DelayFeedback | Target::DelayWet | Target::ReverbWet
    )
}
fn shown(target: Target, value: f32) -> f32 {
    target.physical(value) * if percentage(target) { 100.0 } else { 1.0 }
}
pub fn draw(ui: &mut egui::Ui, config: &mut AppConfig, target: FxTarget, state: &mut EditorState) {
    if let Some(family) = family(config, target) {
        draw_lane(ui, lane_mut(config, target), family, state);
    }
}
pub fn draw_lane(
    ui: &mut egui::Ui,
    lane: &mut ParameterLane,
    family: Family,
    state: &mut EditorState,
) {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    if state.snap == 0 {
        state.snap = PPQ / 4;
    }
    if ui.input(|i| !i.pointer.primary_down() && !i.pointer.any_released()) {
        if let Some((_, _, old)) = state.drag.take() {
            state.remember(old, lane);
        }
    }
    let before = lane.clone();
    let mut history = false;
    if lane.points.is_empty() {
        theme::caption(ui,lang.choose("One parameter lane per slot. Add up to 64 points; playback follows the effect's source clock.","每槽一条参数轨，最多 64 点；按当前效果的来源时钟循环。"));
        if navigation::button(ui, lang.choose("Create parameter lane", "创建参数轨")).clicked()
        {
            *lane = ParameterLane::create(targets(family)[0]);
            state.remember(before, lane);
        }
        return;
    }
    theme::control_row(ui, |ui| {
        let compatible = lane.target.accepts(Some(family));
        navigation::register(ui.add_enabled(
            compatible,
            egui::Checkbox::new(
                &mut lane.enabled,
                lang.choose("Enable automation", "启用自动化"),
            ),
        ));
        let options: Vec<_> = targets(family)
            .iter()
            .map(|t| (*t, name(*t, lang)))
            .collect();
        parameters::selector(ui, "lane-target", &mut lane.target, &options);
        let mut bars = if lane.length % (PPQ * 4) == 0 {
            lane.length / (PPQ * 4)
        } else {
            0
        };
        ui.label(lang.choose("Loop", "循环"));
        let labels: Vec<String> = (1..=8)
            .map(|n| {
                format!(
                    "{n} {}",
                    lang.choose(if n == 1 { "bar" } else { "bars" }, "小节")
                )
            })
            .collect();
        let mut lengths: Vec<_> = labels
            .iter()
            .enumerate()
            .map(|(i, s)| (i as u32 + 1, s.as_str()))
            .collect();
        let exact = format!(
            "{:.3} {}",
            lane.length as f32 / PPQ as f32,
            lang.choose("beats", "拍")
        );
        if bars == 0 {
            lengths.insert(0, (0, exact.as_str()));
        }
        if parameters::selector(ui, "lane-bars", &mut bars, &lengths) && bars > 0 {
            let length = bars * PPQ * 4;
            for p in &mut lane.points {
                p.tick = (p.tick as u64 * length as u64 / lane.length.max(1) as u64) as u32;
            }
            lane.length = length;
            *lane = lane.sanitized();
        }
        parameters::selector(
            ui,
            "lane-interpolation",
            &mut lane.interpolation,
            &[
                (Interpolation::Step, lang.choose("Step", "步进")),
                (Interpolation::Linear, lang.choose("Linear", "线性")),
                (Interpolation::Curve, lang.choose("Curve", "曲线")),
            ],
        );
        parameters::selector(
            ui,
            "lane-snap",
            &mut state.snap,
            &[
                (60, "1/64"),
                (120, "1/32"),
                (240, "1/16"),
                (480, "1/8"),
                (960, "1/4"),
            ],
        );
    });
    theme::control_row(ui, |ui| {
        if navigation::register(
            ui.add_enabled(!state.undo.is_empty(), egui::Button::new(lang.text("Undo"))),
        )
        .clicked()
        {
            state.undo(lane);
            history = true;
        }
        if navigation::register(
            ui.add_enabled(!state.redo.is_empty(), egui::Button::new(lang.text("Redo"))),
        )
        .clicked()
        {
            state.redo(lane);
            history = true;
        }
        let removable = state.selected > 0 && state.selected + 1 < lane.points.len();
        if navigation::register(ui.add_enabled(
            removable,
            egui::Button::new(lang.choose("Delete point", "删除点")),
        ))
        .clicked()
        {
            state.drag = None;
            lane.points.remove(state.selected);
            state.selected = state.selected.saturating_sub(1);
        }
        if navigation::button(ui, lang.choose("Reset shape", "重置形状")).clicked() {
            state.drag = None;
            let enabled = lane.enabled;
            *lane = ParameterLane::create(lane.target);
            lane.enabled = enabled;
            state.selected = 0;
        }
        ui.label(format!(
            "{} / {MAX_POINTS} {}",
            lane.points.len(),
            lang.choose("points", "点")
        ));
    });
    if !lane.target.accepts(Some(family)) {
        theme::caption(ui,lang.choose("This stored lane belongs to another effect family. Select a compatible target to resume it.","保留的参数轨属于其他效果类型；选择兼容目标后可恢复。"));
    }
    theme::caption(
        ui,
        lang.choose(
            "Click to add · Drag nodes / middle handles · Right-click to delete · Ctrl+Z/Y",
            "点击加点；拖动节点／中点；右键删除；Ctrl+Z/Y",
        ),
    );
    theme::caption(
        ui,
        lang.choose(
            "Enabled automation overrides its control; stopping transport restores the base value.",
            "启用时自动化覆盖旋钮，停止演奏恢复原值。",
        ),
    );
    let height = (ui.clip_rect().bottom() - ui.cursor().top() - 105.0).clamp(200.0, 380.0);
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), height),
        egui::Sense::click_and_drag(),
    );
    let response = navigation::canvas(response);
    if response.clicked() || response.drag_started() {
        response.request_focus();
    }
    let plot = Rect::from_min_max(rect.min + vec2(68.0, 18.0), rect.max - vec2(12.0, 28.0));
    let painter = ui.painter().clone();
    painter.rect_filled(rect, 6.0, theme::BACKGROUND);
    #[cfg(test)]
    {
        state.plot = Some(plot);
    }
    let position = |point: Point| {
        pos2(
            plot.left() + point.tick as f32 / lane.length.max(1) as f32 * plot.width(),
            plot.bottom() - point.value * plot.height(),
        )
    };
    for i in 0..=4 {
        let value = i as f32 / 4.0;
        let y = plot.bottom() - value * plot.height();
        painter.hline(plot.x_range(), y, Stroke::new(1.0, Color32::from_gray(48)));
        painter.text(
            pos2(plot.left() - 8.0, y),
            egui::Align2::RIGHT_CENTER,
            format!("{:.1}", shown(lane.target, value)),
            egui::FontId::monospace(12.0),
            theme::MUTED,
        );
    }
    for beat in 0..=lane.length / PPQ {
        let x = plot.left() + beat as f32 * PPQ as f32 / lane.length as f32 * plot.width();
        painter.vline(
            x,
            plot.y_range(),
            Stroke::new(1.0, Color32::from_gray(if beat % 4 == 0 { 70 } else { 43 })),
        );
        if beat % 4 == 0 {
            painter.text(
                pos2(x, plot.bottom() + 6.0),
                egui::Align2::LEFT_TOP,
                format!("{}", beat / 4 + 1),
                egui::FontId::monospace(12.0),
                theme::MUTED,
            );
        }
    }
    // Hit-test the geometry from the start of this frame. Paint only after
    // pointer/keyboard/value edits, so handles and curves share the same state.
    let handles = curve_handles(lane, plot);
    let positions: Vec<_> = lane.points.iter().copied().map(position).collect();
    let pointer = response.interact_pointer_pos();
    if response.drag_started() {
        let origin = ui.input(|i| i.pointer.press_origin()).or(pointer);
        if let Some(origin) = origin {
            if let Some(index) = positions.iter().position(|p| p.distance(origin) < 10.0) {
                state.selected = index;
                state.drag = Some((index, false, lane.clone()));
            } else if let Some((index, _)) = handles.iter().find(|(_, p)| p.distance(origin) < 10.0)
            {
                state.drag = Some((*index, true, lane.clone()));
            }
        }
    }
    if state.drag.as_ref().is_some_and(|(index, curve, _)| {
        *index >= lane.points.len() || *curve && *index + 1 >= lane.points.len()
    }) {
        state.drag = None;
    }
    if let (Some((index, curve, _)), Some(pos)) = (&state.drag, pointer) {
        let index = *index;
        if *curve {
            let a = lane.points[index].value;
            let b = lane.points[index + 1].value;
            if (b - a).abs() > 1e-5 {
                let desired = ((plot.bottom() - pos.y) / plot.height()).clamp(0.0, 1.0);
                let t = ((desired - a) / (b - a)).clamp(1e-5, 1.0 - 1e-5);
                lane.points[index].curve = if t < 0.5 {
                    (t.ln() / 0.5f32.ln() - 1.0) / 7.0
                } else {
                    (1.0 - (1.0 - t).ln() / 0.5f32.ln()) / 7.0
                }
                .clamp(-1.0, 1.0);
            }
        } else {
            let tick = (((pos.x - plot.left()) / plot.width() * lane.length as f32
                / state.snap as f32)
                .round()
                .max(0.0) as u32
                * state.snap)
                .min(lane.length);
            if index > 0 && index + 1 < lane.points.len() {
                lane.points[index].tick = tick.clamp(
                    lane.points[index - 1].tick + 1,
                    lane.points[index + 1].tick - 1,
                );
            }
            lane.points[index].value = ((plot.bottom() - pos.y) / plot.height()).clamp(0.0, 1.0);
        }
    }
    if response.drag_stopped() {
        if let Some((_, _, old)) = state.drag.take() {
            state.remember(old, lane);
        }
        history = true;
    }
    if response.clicked() {
        if let Some(pos) = pointer.filter(|p| plot.contains(*p)) {
            if let Some(index) = positions.iter().position(|p| p.distance(pos) < 10.0) {
                state.selected = index;
            } else if let Some((index, _)) = handles
                .iter()
                .find(|(_, handle)| handle.distance(pos) < 10.0)
            {
                // A curvature handle is not blank canvas. A click that never
                // reaches the drag threshold must not create a point on it.
                state.selected = *index;
            } else {
                let tick = (((pos.x - plot.left()) / plot.width() * lane.length as f32
                    / state.snap as f32)
                    .round()
                    .max(0.0) as u32
                    * state.snap)
                    .min(lane.length);
                let value = ((plot.bottom() - pos.y) / plot.height()).clamp(0.0, 1.0);
                if let Some(index) = lane.points.iter().position(|p| p.tick == tick) {
                    lane.points[index].value = value;
                    state.selected = index;
                } else if lane.points.len() < MAX_POINTS {
                    lane.points.push(Point {
                        tick,
                        value,
                        curve: 0.0,
                    });
                    lane.points.sort_by_key(|p| p.tick);
                    state.selected = lane.points.iter().position(|p| p.tick == tick).unwrap();
                }
            }
        }
    }
    if response.secondary_clicked() {
        if let Some(pos) = pointer {
            if let Some(index) = positions
                .iter()
                .position(|p| p.distance(pos) < 10.0)
                .filter(|i| *i > 0 && *i + 1 < lane.points.len())
            {
                lane.points.remove(index);
                state.selected = index.saturating_sub(1);
            }
        }
    }
    if ui.is_enabled()
        && ui.input(|i| i.focused)
        && !navigation::text_focused(ui.ctx())
        && !ui.memory(|m| m.any_popup_open())
        && state.drag.is_none()
    {
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, Key::Z)) {
            state.undo(lane);
            history = true;
        }
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, Key::Y)) {
            state.redo(lane);
            history = true;
        }
    }
    if ui.is_enabled()
        && ui.input(|i| i.focused)
        && !ui.memory(|m| m.any_popup_open())
        && response.has_focus()
        && state.drag.is_none()
    {
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Delete))
            && state.selected > 0
            && state.selected + 1 < lane.points.len()
        {
            lane.points.remove(state.selected);
            state.selected -= 1;
        }
        let dx = ui.input(|i| {
            i32::from(i.key_pressed(Key::ArrowRight)) - i32::from(i.key_pressed(Key::ArrowLeft))
        });
        let dy = ui.input(|i| {
            i32::from(i.key_pressed(Key::ArrowUp)) - i32::from(i.key_pressed(Key::ArrowDown))
        });
        if let Some(point) = lane.points.get_mut(state.selected) {
            point.value = (point.value + dy as f32 * 0.01).clamp(0.0, 1.0);
        }
        if dx != 0 && state.selected > 0 && state.selected + 1 < lane.points.len() {
            lane.points[state.selected].tick =
                (lane.points[state.selected].tick as i64 + dx as i64 * state.snap as i64).clamp(
                    lane.points[state.selected - 1].tick as i64 + 1,
                    lane.points[state.selected + 1].tick as i64 - 1,
                ) as u32;
        }
    }
    state.selected = state.selected.min(lane.points.len().saturating_sub(1));
    if let Some(point) = lane.points.get_mut(state.selected) {
        let (min, max, log) = lane.target.range();
        let factor = if percentage(lane.target) { 100.0 } else { 1.0 };
        let mut value = shown(lane.target, point.value);
        let key_step = if lane.target == Target::FilterQ {
            0.1
        } else {
            1.0
        };
        if parameters::float_with_keys(
            ui,
            &mut value,
            min * factor,
            max * factor,
            if percentage(lane.target) { 0.1 } else { 0.01 },
            key_step,
            name(lane.target, lang),
            log,
        )
        .changed()
        {
            point.value = lane.target.normalized(value / factor);
        }
        if lane.interpolation == Interpolation::Curve {
            let mut curve = point.curve * 100.0;
            parameters::float_with_keys(
                ui,
                &mut curve,
                -100.0,
                100.0,
                0.1,
                1.0,
                lang.choose("Segment curve (%)", "后一段曲率（%）"),
                false,
            );
            point.curve = curve / 100.0;
        }
    }
    if !history && state.drag.is_none() {
        state.remember(before, lane);
    }
    paint_curve(&painter, lane, plot, state.selected, theme::accent(ui));
}

fn point_position(plot: Rect, length: u32, point: Point) -> egui::Pos2 {
    pos2(
        plot.left() + point.tick as f32 / length.max(1) as f32 * plot.width(),
        plot.bottom() - point.value * plot.height(),
    )
}
fn curve_handles(lane: &ParameterLane, plot: Rect) -> Vec<(usize, egui::Pos2)> {
    if lane.interpolation != Interpolation::Curve {
        return Vec::new();
    }
    lane.points
        .windows(2)
        .enumerate()
        .map(|(index, pair)| {
            let (a, b) = (pair[0], pair[1]);
            (
                index,
                point_position(
                    plot,
                    lane.length,
                    Point {
                        tick: (a.tick + b.tick) / 2,
                        value: a.value
                            + (b.value - a.value) * crate::dsp::envelope::bend_curve(0.5, a.curve),
                        curve: 0.0,
                    },
                ),
            )
        })
        .collect()
}
fn paint_curve(
    painter: &egui::Painter,
    lane: &ParameterLane,
    plot: Rect,
    selected: usize,
    color: Color32,
) {
    let position = |point| point_position(plot, lane.length, point);
    for pair in lane.points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let line = if lane.interpolation == Interpolation::Step {
            vec![
                position(a),
                position(Point { tick: b.tick, ..a }),
                position(b),
            ]
        } else {
            (0..=48)
                .map(|j| {
                    let t = j as f32 / 48.0;
                    let shape = if lane.interpolation == Interpolation::Curve {
                        crate::dsp::envelope::bend_curve(t, a.curve)
                    } else {
                        t
                    };
                    // Keep the display's x coordinate continuous, even when a segment
                    // is only one PPQ tick wide.
                    pos2(
                        plot.left()
                            + (a.tick as f32 + (b.tick - a.tick) as f32 * t)
                                / lane.length.max(1) as f32
                                * plot.width(),
                        plot.bottom() - (a.value + (b.value - a.value) * shape) * plot.height(),
                    )
                })
                .collect()
        };
        painter.add(egui::Shape::line(line, Stroke::new(2.0, color)));
    }
    for (_, handle) in curve_handles(lane, plot) {
        painter.circle_stroke(handle, 4.0, Stroke::new(1.5, color));
    }
    for (index, point) in lane.points.iter().copied().enumerate() {
        painter.circle_filled(
            position(point),
            if index == selected { 6.0 } else { 4.5 },
            if index == selected {
                Color32::WHITE
            } else {
                color
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Pos2;
    fn frame(
        ctx: &egui::Context,
        lane: &mut ParameterLane,
        state: &mut EditorState,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let modifiers = events
            .iter()
            .find_map(|e| match e {
                egui::Event::Key { modifiers, .. }
                | egui::Event::PointerButton { modifiers, .. } => Some(*modifiers),
                _ => None,
            })
            .unwrap_or_default();
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1300.0, 1000.0))),
                events,
                modifiers,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default()
                    .show(ctx, |ui| draw_lane(ui, lane, Family::Filter, state));
            },
        )
    }
    fn click(
        ctx: &egui::Context,
        lane: &mut ParameterLane,
        state: &mut EditorState,
        pos: Pos2,
        button: egui::PointerButton,
    ) {
        for pressed in [true, false] {
            frame(
                ctx,
                lane,
                state,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
    }
    fn drag(
        ctx: &egui::Context,
        lane: &mut ParameterLane,
        state: &mut EditorState,
        from: Pos2,
        to: Pos2,
    ) {
        frame(
            ctx,
            lane,
            state,
            vec![
                egui::Event::PointerMoved(from),
                egui::Event::PointerButton {
                    pos: from,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame(ctx, lane, state, vec![egui::Event::PointerMoved(to)]);
        frame(
            ctx,
            lane,
            state,
            vec![egui::Event::PointerButton {
                pos: to,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
    }
    #[test]
    fn curve_midpoint_click_is_selection_and_drag_paints_only_current_geometry() {
        let ctx = egui::Context::default();
        let mut lane = ParameterLane::create(Target::FilterCutoff);
        lane.interpolation = Interpolation::Curve;
        let mut state = EditorState::default();
        frame(&ctx, &mut lane, &mut state, vec![]);
        frame(&ctx, &mut lane, &mut state, vec![]);
        let plot = state.plot.unwrap();
        let handle = curve_handles(&lane, plot)[0].1;
        let before = lane.clone();
        click(
            &ctx,
            &mut lane,
            &mut state,
            handle,
            egui::PointerButton::Primary,
        );
        assert_eq!(
            lane, before,
            "A curve handle click cannot create a duplicate node"
        );
        frame(
            &ctx,
            &mut lane,
            &mut state,
            vec![
                egui::Event::PointerMoved(handle),
                egui::Event::PointerButton {
                    pos: handle,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let to = handle - vec2(0.0, plot.height() * 0.15);
        let output = frame(
            &ctx,
            &mut lane,
            &mut state,
            vec![egui::Event::PointerMoved(to)],
        );
        assert_eq!(lane.points.len(), 3);
        assert!(lane.points[0].curve.abs() > 0.01);
        let current = curve_handles(&lane, plot)[0].1;
        let paths = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Path(path) if path.stroke.width == 2.0 && path.points.len() == 49 => {
                    Some(path)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            paths.len(),
            lane.points.len() - 1,
            "One current line per segment, without a stale copy"
        );
        assert!(
            paths[0].points[24].distance(current) < 0.01,
            "The line must use the new curvature in this same frame"
        );
        let handles = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Circle(circle)
                    if circle.radius == 4.0 && circle.stroke.width == 1.5 =>
                {
                    Some(circle.center)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(handles.len(), lane.points.len() - 1);
        assert!(handles.iter().any(|p| p.distance(current) < 0.01));
        assert!(!handles.iter().any(|p| p.distance(handle) < 0.01));
    }
    #[test]
    fn dragging_an_automation_node_paints_line_and_handle_at_the_same_updated_position() {
        let ctx = egui::Context::default();
        let mut lane = ParameterLane::create(Target::FilterCutoff);
        lane.interpolation = Interpolation::Curve;
        let mut state = EditorState::default();
        frame(&ctx, &mut lane, &mut state, vec![]);
        frame(&ctx, &mut lane, &mut state, vec![]);
        let plot = state.plot.unwrap();
        let old = point_position(plot, lane.length, lane.points[1]);
        frame(
            &ctx,
            &mut lane,
            &mut state,
            vec![
                egui::Event::PointerMoved(old),
                egui::Event::PointerButton {
                    pos: old,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let to = pos2(
            plot.left() + plot.width() * 0.625,
            plot.bottom() - plot.height() * 0.35,
        );
        let output = frame(
            &ctx,
            &mut lane,
            &mut state,
            vec![egui::Event::PointerMoved(to)],
        );
        let current = point_position(plot, lane.length, lane.points[1]);
        assert!(current.distance(to) < 0.01);
        let lines = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Path(path) if path.stroke.width == 2.0 && path.points.len() == 49 => {
                    Some(path)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].points.last().unwrap().distance(current) < 0.01);
        assert!(lines[1].points[0].distance(current) < 0.01);
        let circles = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Circle(circle)
                    if circle.radius == 6.0 && plot.contains(circle.center) =>
                {
                    Some(circle.center)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(circles.len(), 1);
        assert!(circles[0].distance(current) < 0.01);
        assert!(circles[0].distance(old) > 10.0);
    }
    #[test]
    fn mouse_points_curve_handles_and_undo_keep_a_bounded_loop() {
        let ctx = egui::Context::default();
        let mut lane = ParameterLane::create(Target::FilterCutoff);
        let mut state = EditorState::default();
        frame(&ctx, &mut lane, &mut state, vec![]);
        frame(&ctx, &mut lane, &mut state, vec![]);
        let plot = state.plot.unwrap();
        let at = |x: f32, y: f32| {
            pos2(
                plot.left() + plot.width() * x,
                plot.bottom() - plot.height() * y,
            )
        };
        click(
            &ctx,
            &mut lane,
            &mut state,
            at(0.25, 0.6),
            egui::PointerButton::Primary,
        );
        assert_eq!(lane.points.len(), 4);
        assert_eq!(lane.points[1].tick, PPQ);
        drag(&ctx, &mut lane, &mut state, at(0.25, 0.6), at(0.375, 0.8));
        assert_eq!(lane.points[1].tick, PPQ * 3 / 2);
        assert!((lane.points[1].value - 0.8).abs() < 0.01);
        state.undo(&mut lane);
        assert_eq!(lane.points[1].tick, PPQ);
        state.redo(&mut lane);
        assert_eq!(lane.points[1].tick, PPQ * 3 / 2);
        lane.interpolation = Interpolation::Curve;
        frame(&ctx, &mut lane, &mut state, vec![]);
        drag(
            &ctx,
            &mut lane,
            &mut state,
            at(0.1875, 0.525),
            at(0.1875, 0.7),
        );
        assert!(lane.points[0].curve < -0.1);
        click(
            &ctx,
            &mut lane,
            &mut state,
            at(0.375, 0.8),
            egui::PointerButton::Secondary,
        );
        assert_eq!(lane.points.len(), 3);
        state.undo(&mut lane);
        assert_eq!(lane.points.len(), 4);
        click(
            &ctx,
            &mut lane,
            &mut state,
            at(0.0, 0.25),
            egui::PointerButton::Secondary,
        );
        assert_eq!(lane.points.len(), 4);
        assert_eq!(lane.length, PPQ * 4);
    }
}
