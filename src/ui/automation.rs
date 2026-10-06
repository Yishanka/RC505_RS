use super::{navigation, parameters, theme};
use crate::{
    config::{
        AppConfig,
        automation::{
            Family, Interpolation, MAX_LENGTH, MAX_POINTS, MIN_LENGTH, PPQ, ParameterLane, Point,
            Target,
        },
    },
    presets::FxTarget,
};
use eframe::egui::{self, Color32, Key, Rect, Stroke, pos2, vec2};
pub struct EditorState {
    selected: usize,
    snap: u32,
    drag: Option<Drag>,
    point_revision: u64,
    undo: Vec<ParameterLane>,
    redo: Vec<ParameterLane>,
    #[cfg(test)]
    plot: Option<Rect>,
    #[cfg(test)]
    time_id: Option<egui::Id>,
    #[cfg(test)]
    length_id: Option<egui::Id>,
    #[cfg(test)]
    value_id: Option<egui::Id>,
}
struct Drag {
    index: usize,
    curve: bool,
    before: ParameterLane,
    offset: egui::Vec2,
}
impl Default for EditorState {
    fn default() -> Self {
        Self {
            selected: 0,
            snap: 0,
            drag: None,
            point_revision: 0,
            undo: Vec::new(),
            redo: Vec::new(),
            #[cfg(test)]
            plot: None,
            #[cfg(test)]
            time_id: None,
            #[cfg(test)]
            length_id: None,
            #[cfg(test)]
            value_id: None,
        }
    }
}
impl EditorState {
    pub fn reset(&mut self) {
        *self = Self {
            point_revision: self.point_revision.wrapping_add(1),
            ..Self::default()
        };
    }
    fn push_history(stack: &mut Vec<ParameterLane>, lane: ParameterLane) {
        if stack.len() >= 64 {
            stack.remove(0);
        }
        stack.push(lane);
    }
    fn structure_changed(&mut self) {
        self.drag = None;
        self.point_revision = self.point_revision.wrapping_add(1);
    }
    fn remember(&mut self, before: ParameterLane, after: &ParameterLane) {
        if before != *after {
            Self::push_history(&mut self.undo, before);
            self.redo.clear();
        }
    }
    fn undo(&mut self, lane: &mut ParameterLane) {
        let old = self
            .drag
            .take()
            .map(|drag| drag.before)
            .or_else(|| self.undo.pop());
        if let Some(old) = old {
            Self::push_history(&mut self.redo, lane.clone());
            *lane = old;
            self.selected = 0;
            self.point_revision = self.point_revision.wrapping_add(1);
        }
    }
    fn redo(&mut self, lane: &mut ParameterLane) {
        self.drag = None;
        if let Some(next) = self.redo.pop() {
            Self::push_history(&mut self.undo, lane.clone());
            *lane = next;
            self.selected = 0;
            self.point_revision = self.point_revision.wrapping_add(1);
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
    if ui.input(|i| !i.pointer.primary_down() && !i.pointer.any_released()) {
        if let Some(drag) = state.drag.take() {
            state.remember(drag.before, lane);
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
        )).on_hover_text(lang.choose("While transport runs, automation overrides this parameter. Stopping or disabling restores its base value.","演奏时自动化覆盖所选参数，停止或禁用后恢复原旋钮值。"));
        let options: Vec<_> = targets(family)
            .iter()
            .map(|t| (*t, name(*t, lang)))
            .collect();
        if parameters::selector(ui, "lane-target", &mut lane.target, &options) {
            state.structure_changed();
        }
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
            "{:.6} {}",
            lane.length as f64 / PPQ as f64,
            lang.choose("beats", "拍")
        );
        if bars == 0 {
            lengths.insert(0, (0, exact.as_str()));
        }
        if parameters::selector(ui, "lane-bars", &mut bars, &lengths) && bars > 0 {
            state.structure_changed();
            lane.rescale_length(bars * PPQ * 4);
        }
        if parameters::selector(
            ui,
            "lane-interpolation",
            &mut lane.interpolation,
            &[
                (Interpolation::Step, lang.choose("Step", "步进")),
                (Interpolation::Linear, lang.choose("Linear", "线性")),
                (Interpolation::Curve, lang.choose("Curve", "曲线")),
            ],
        ) {
            state.structure_changed();
        }
        ui.label(lang.choose("Snap", "吸附"));
        parameters::selector(
            ui,
            "lane-snap",
            &mut state.snap,
            &[
                (0, lang.choose("Off · 1 tick", "关闭 · 1 tick")),
                (
                    15,
                    lang.choose("1/256 note · 1/64 beat", "1/256 音符 · 1/64 拍"),
                ),
                (
                    30,
                    lang.choose("1/128 note · 1/32 beat", "1/128 音符 · 1/32 拍"),
                ),
                (
                    60,
                    lang.choose("1/64 note · 1/16 beat", "1/64 音符 · 1/16 拍"),
                ),
                (
                    120,
                    lang.choose("1/32 note · 1/8 beat", "1/32 音符 · 1/8 拍"),
                ),
                (
                    240,
                    lang.choose("1/16 note · 1/4 beat", "1/16 音符 · 1/4 拍"),
                ),
                (480, lang.choose("1/8 note · 1/2 beat", "1/8 音符 · 1/2 拍")),
                (960, lang.choose("1/4 note · 1 beat", "1/4 音符 · 1 拍")),
            ],
        );
    });
    let minimum_length = MIN_LENGTH.max(lane.points.len().saturating_sub(1) as u32);
    let mut length_beats = lane.length as f64 / PPQ as f64;
    let length_response = super::parameter_input::slider(
        ui,
        &mut length_beats,
        minimum_length as f64 / PPQ as f64,
        MAX_LENGTH as f64 / PPQ as f64,
        super::parameter_input::Step::new(1.0 / PPQ as f64, 1.0 / PPQ as f64),
        lang.choose("Loop length (beats)", "循环长度（拍）"),
        false,
    );
    #[cfg(test)]
    {
        state.length_id = Some(length_response.id);
    }
    if length_response.changed() {
        state.structure_changed();
        lane.rescale_length(beats_to_tick(length_beats, MAX_LENGTH));
    }
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
            state.structure_changed();
            lane.points.remove(state.selected);
            state.selected = state.selected.saturating_sub(1);
        }
        if navigation::button(ui, lang.choose("Reset shape", "重置形状")).clicked() {
            state.structure_changed();
            reset_shape(lane);
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
    theme::caption(ui,match lane.interpolation {
        Interpolation::Step=>lang.choose("Step holds the left value until the next node. Snap Off uses 1/960 beat ticks.","步进：保持左侧值，直到下一节点跳变。关闭吸附时精度为 1/960 拍。"),
        Interpolation::Linear=>lang.choose("Linear follows the displayed axis continuously (Hz / ms use a logarithmic axis). Snap Off uses 1/960 beat ticks.","线性：沿显示坐标轴连续变化（Hz / ms 为对数轴）。关闭吸附时精度为 1/960 拍。"),
        Interpolation::Curve=>lang.choose("Curve bends the continuous segment on the displayed axis; drag its middle handle. Snap Off uses 1/960 beat ticks.","曲线：在显示坐标轴上弯曲连续段，拖动中点调整。关闭吸附时精度为 1/960 拍。"),
    });
    let height = (ui.clip_rect().bottom() - ui.cursor().top() - 170.0).clamp(170.0, 360.0);
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
    let base_grid = if state.snap == 0 {
        (lane.length / 8).max(1)
    } else {
        state.snap
    };
    let minimum_grid = (lane.length as f32 * 8.0 / plot.width().max(1.0)).ceil() as u32;
    let grid = minimum_grid.max(1).div_ceil(base_grid) * base_grid;
    for tick in (0..=lane.length).step_by(grid.max(1) as usize) {
        let x = plot.left() + tick as f32 / lane.length as f32 * plot.width();
        painter.vline(
            x,
            plot.y_range(),
            Stroke::new(
                1.0,
                Color32::from_gray(if tick % PPQ == 0 { 70 } else { 43 }),
            ),
        );
    }
    for part in 0..=4 {
        let x = plot.left() + plot.width() * part as f32 / 4.0;
        let label = format!(
            "{:.6}",
            lane.length as f64 * part as f64 / (4.0 * PPQ as f64)
        );
        painter.text(
            pos2(x, plot.bottom() + 6.0),
            if part == 4 {
                egui::Align2::RIGHT_TOP
            } else {
                egui::Align2::LEFT_TOP
            },
            format!(
                "{} {}",
                label.trim_end_matches('0').trim_end_matches('.'),
                lang.choose("beats", "拍")
            ),
            egui::FontId::monospace(12.0),
            theme::MUTED,
        );
    }
    // Hit-test the geometry from the start of this frame. Paint only after
    // pointer/keyboard/value edits, so handles and curves share the same state.
    let handles = curve_handles(lane, plot);
    let positions: Vec<_> = lane.points.iter().copied().map(position).collect();
    let pointer = response.interact_pointer_pos();
    if response.drag_started() {
        let origin = ui.input(|i| i.pointer.press_origin()).or(pointer);
        if let Some(origin) = origin {
            if let Some(index) = nearest_point(&positions, origin) {
                state.selected = index;
                state.drag = Some(Drag {
                    index,
                    curve: false,
                    before: lane.clone(),
                    offset: origin - positions[index],
                });
            } else if let Some((index, handle)) =
                handles.iter().find(|(_, p)| p.distance(origin) < 10.0)
            {
                state.selected = *index;
                state.drag = Some(Drag {
                    index: *index,
                    curve: true,
                    before: lane.clone(),
                    offset: origin - *handle,
                });
            }
        }
    }
    if state.drag.as_ref().is_some_and(|drag| {
        drag.index >= lane.points.len() || drag.curve && drag.index + 1 >= lane.points.len()
    }) {
        state.drag = None;
    }
    if let (Some(drag), Some(pointer)) = (&state.drag, pointer) {
        let index = drag.index;
        let pos = pointer - drag.offset;
        if drag.curve {
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
            let original = point_position(plot, lane.length, drag.before.points[index]);
            let tick = if (pos.x - original.x).abs() < 0.5 {
                drag.before.points[index].tick
            } else {
                pointer_tick(pos.x, plot, lane.length, state.snap)
            };
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
        if let Some(drag) = state.drag.take() {
            state.remember(drag.before, lane);
        }
        history = true;
    }
    if response.clicked() {
        if let Some(pos) = pointer {
            if let Some(index) = nearest_point(&positions, pos) {
                state.selected = index;
            } else if let Some((index, _)) = handles
                .iter()
                .find(|(_, handle)| handle.distance(pos) < 10.0)
            {
                // A curvature handle is not blank canvas. A click that never
                // reaches the drag threshold must not create a point on it.
                state.selected = *index;
            } else if plot.contains(pos) {
                let tick = pointer_tick(pos.x, plot, lane.length, state.snap);
                let value = ((plot.bottom() - pos.y) / plot.height()).clamp(0.0, 1.0);
                if let Some(index) = lane.points.iter().position(|p| p.tick == tick) {
                    lane.points[index].value = value;
                    state.selected = index;
                } else if lane.points.len() < MAX_POINTS {
                    state.structure_changed();
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
    if response.secondary_clicked() && !ui.input(|input| input.pointer.primary_down()) {
        if let Some(pos) = pointer {
            if let Some(index) =
                nearest_point(&positions, pos).filter(|i| *i > 0 && *i + 1 < lane.points.len())
            {
                state.structure_changed();
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
            state.structure_changed();
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
            lane.points[state.selected].tick = (lane.points[state.selected].tick as i64
                + dx as i64 * state.snap.max(1) as i64)
                .clamp(
                    lane.points[state.selected - 1].tick as i64 + 1,
                    lane.points[state.selected + 1].tick as i64 - 1,
                ) as u32;
        }
    }
    state.selected = state.selected.min(lane.points.len().saturating_sub(1));
    let point_count = lane.points.len();
    let (minimum, maximum) = point_tick_bounds(lane, state.selected);
    ui.push_id(("lane-point",state.selected,state.point_revision),|ui|{
    if let Some(point) = lane.points.get_mut(state.selected) {
        let mut beats=point.tick as f64/PPQ as f64;
        let time_response=ui.add_enabled_ui(minimum<maximum,|ui|{
            super::parameter_input::slider(ui,&mut beats,minimum as f64/PPQ as f64,maximum as f64/PPQ as f64,
                super::parameter_input::Step::new(1.0/PPQ as f64,1.0/PPQ as f64),lang.choose("Node time (beats)","节点位置（拍）"),false)
        }).inner;
        #[cfg(test)]{state.time_id=Some(time_response.id);}
        if time_response.changed(){point.tick=beats_to_tick(beats,lane.length).clamp(minimum,maximum);}
        theme::caption(ui,format!("{} {}/{} · {} / {} ticks · {}",lang.choose("Node","节点"),state.selected+1,point_count,point.tick,lane.length,
            if state.selected==0||state.selected+1==point_count{lang.choose("Endpoint time is fixed","端点时间固定")}
            else if minimum==maximum{lang.choose("No free tick between neighbours","邻点之间没有空闲 tick")}
            else{lang.choose("1 tick = 1/960 beat; entry rounds to the nearest tick, without snapping","1 tick = 1/960 拍；输入取最近 tick，不受吸附影响")}));
        let (min, max, log) = lane.target.range();
        let factor = if percentage(lane.target) { 100.0 } else { 1.0 };
        let mut value = shown(lane.target, point.value);
        let key_step = if lane.target == Target::FilterQ {
            0.1
        } else {
            1.0
        };
        let value_response=parameters::float_with_keys(
            ui,
            &mut value,
            min * factor,
            max * factor,
            if percentage(lane.target) { 0.1 } else { 0.01 },
            key_step,
            name(lane.target, lang),
            log,
        );
        #[cfg(test)]{state.value_id=Some(value_response.id);}
        if value_response.changed(){
            point.value = lane.target.normalized(value / factor);
        }
        if lane.interpolation == Interpolation::Curve && state.selected+1<point_count {
            let mut curve = point.curve * 100.0;
            if parameters::float_with_keys(
                ui,
                &mut curve,
                -100.0,
                100.0,
                0.1,
                1.0,
                lang.choose("Segment curve (%)", "后一段曲率（%）"),
                false,
            ).changed(){point.curve = curve / 100.0;}
        }
    }
    });
    if !history && state.drag.is_none() {
        state.remember(before, lane);
    }
    paint_curve(&painter, lane, plot, state.selected, theme::accent(ui));
}

fn pointer_tick(x: f32, plot: Rect, length: u32, snap: u32) -> u32 {
    let step = snap.max(1) as f64;
    let tick = ((x - plot.left()) as f64 / plot.width().max(1.0) as f64 * length as f64)
        .clamp(0.0, length as f64);
    ((tick / step).round() * step).clamp(0.0, length as f64) as u32
}
fn beats_to_tick(beats: f64, length: u32) -> u32 {
    if !beats.is_finite() {
        return 0;
    }
    (beats * PPQ as f64).round().clamp(0.0, length as f64) as u32
}
fn point_tick_bounds(lane: &ParameterLane, index: usize) -> (u32, u32) {
    if index == 0 {
        return (0, 0);
    }
    if index + 1 >= lane.points.len() {
        return (lane.length, lane.length);
    }
    (
        lane.points[index - 1].tick + 1,
        lane.points[index + 1].tick - 1,
    )
}
fn nearest_point(positions: &[egui::Pos2], pointer: egui::Pos2) -> Option<usize> {
    positions
        .iter()
        .enumerate()
        .filter(|(_, p)| p.distance(pointer) < 10.0)
        .min_by(|(_, a), (_, b)| a.distance_sq(pointer).total_cmp(&b.distance_sq(pointer)))
        .map(|(index, _)| index)
}
fn reset_shape(lane: &mut ParameterLane) {
    let (target, enabled, length, interpolation) =
        (lane.target, lane.enabled, lane.length, lane.interpolation);
    *lane = ParameterLane::create(target);
    lane.enabled = enabled;
    lane.interpolation = interpolation;
    lane.rescale_length(length);
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
                pos2(
                    plot.left()
                        + (a.tick as f32 + b.tick as f32) * 0.5 / lane.length.max(1) as f32
                            * plot.width(),
                    plot.bottom()
                        - (a.value
                            + (b.value - a.value) * crate::dsp::envelope::bend_curve(0.5, a.curve))
                            * plot.height(),
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
    fn type_number(
        ctx: &egui::Context,
        lane: &mut ParameterLane,
        state: &mut EditorState,
        id: egui::Id,
        text: Option<&str>,
    ) {
        ctx.memory_mut(|memory| memory.request_focus(id));
        frame(ctx, lane, state, vec![]);
        let event = |pressed| egui::Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        frame(ctx, lane, state, vec![event(true)]);
        frame(ctx, lane, state, vec![event(false)]);
        if let Some(text) = text {
            frame(ctx, lane, state, vec![egui::Event::Text(text.into())]);
        }
        frame(ctx, lane, state, vec![event(true)]);
        frame(ctx, lane, state, vec![event(false)]);
    }
    fn pending_number(
        ctx: &egui::Context,
        lane: &mut ParameterLane,
        state: &mut EditorState,
        id: egui::Id,
        text: Option<&str>,
    ) {
        ctx.memory_mut(|memory| memory.request_focus(id));
        frame(ctx, lane, state, vec![]);
        for pressed in [true, false] {
            frame(
                ctx,
                lane,
                state,
                vec![egui::Event::Key {
                    key: Key::Enter,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        if let Some(text) = text {
            frame(ctx, lane, state, vec![egui::Event::Text(text.into())]);
        }
    }
    fn quick_click(
        ctx: &egui::Context,
        lane: &mut ParameterLane,
        state: &mut EditorState,
        pos: Pos2,
        button: egui::PointerButton,
    ) {
        // Real OS input can contain both mouse edges inside one display frame.
        frame(
            ctx,
            lane,
            state,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    fn editing_fixture() -> ParameterLane {
        let mut lane = ParameterLane::create(Target::FilterCutoff);
        lane.points = vec![
            Point {
                tick: 0,
                value: 0.25,
                curve: 0.0,
            },
            Point {
                tick: PPQ,
                value: 0.4,
                curve: 0.0,
            },
            Point {
                tick: PPQ * 2,
                value: 0.8,
                curve: 0.0,
            },
            Point {
                tick: PPQ * 4,
                value: 0.25,
                curve: 0.0,
            },
        ];
        lane
    }
    #[test]
    fn pending_numeric_edit_cannot_overwrite_another_selected_node() {
        for typed in [None, Some("1234.5")] {
            for one_frame in [false, true] {
                let ctx = egui::Context::default();
                let mut lane = editing_fixture();
                let mut state = EditorState {
                    selected: 1,
                    ..Default::default()
                };
                frame(&ctx, &mut lane, &mut state, vec![]);
                frame(&ctx, &mut lane, &mut state, vec![]);
                let untouched = lane.points[2];
                let id = state.value_id.unwrap();
                pending_number(&ctx, &mut lane, &mut state, id, typed);
                let at = point_position(state.plot.unwrap(), lane.length, untouched);
                if one_frame {
                    quick_click(
                        &ctx,
                        &mut lane,
                        &mut state,
                        at,
                        egui::PointerButton::Primary,
                    );
                } else {
                    click(
                        &ctx,
                        &mut lane,
                        &mut state,
                        at,
                        egui::PointerButton::Primary,
                    );
                }
                assert_eq!(
                    state.selected, 2,
                    "The mouse must actually select the other node"
                );
                assert_eq!(
                    lane.points[2], untouched,
                    "typed={typed:?}, same-frame click={one_frame}: old text/before_edit leaked into another node"
                );
            }
        }
    }
    #[test]
    fn pending_numeric_edit_cannot_leak_through_delete_and_readd() {
        for typed in [None, Some("1234.5")] {
            let ctx = egui::Context::default();
            let mut lane = editing_fixture();
            let mut state = EditorState {
                selected: 1,
                ..Default::default()
            };
            frame(&ctx, &mut lane, &mut state, vec![]);
            frame(&ctx, &mut lane, &mut state, vec![]);
            let original = lane.clone();
            let id = state.value_id.unwrap();
            pending_number(&ctx, &mut lane, &mut state, id, typed);
            let at = point_position(state.plot.unwrap(), lane.length, original.points[1]);
            quick_click(
                &ctx,
                &mut lane,
                &mut state,
                at,
                egui::PointerButton::Secondary,
            );
            assert_eq!(lane.points.len(), 3);
            assert_eq!(
                lane.points[0], original.points[0],
                "Deleting a node must not commit its editor buffer into the newly selected endpoint"
            );
            assert_eq!(lane.points[1], original.points[2]);
            quick_click(
                &ctx,
                &mut lane,
                &mut state,
                at,
                egui::PointerButton::Primary,
            );
            frame(&ctx, &mut lane, &mut state, vec![]);
            assert_eq!(lane.points.len(), 4);
            assert_eq!(lane.points[1].tick, original.points[1].tick);
            assert!((lane.points[1].value - original.points[1].value).abs() < 1e-6);
            assert_eq!(lane.points[2], original.points[2]);
        }
    }
    #[test]
    fn snap_off_stays_off_and_places_ticks_between_the_fine_grids() {
        let ctx = egui::Context::default();
        let mut lane = ParameterLane::create(Target::FilterCutoff);
        let mut state = EditorState::default();
        for _ in 0..4 {
            frame(&ctx, &mut lane, &mut state, vec![]);
            assert_eq!(state.snap, 0);
        }
        let plot = state.plot.unwrap();
        let at = pos2(
            plot.left() + plot.width() * 127.0 / lane.length as f32,
            plot.bottom() - plot.height() * 0.6,
        );
        click(
            &ctx,
            &mut lane,
            &mut state,
            at,
            egui::PointerButton::Primary,
        );
        assert_eq!(lane.points[1].tick, 127);
        assert_eq!(state.snap, 0);
        let at = plot.left() + plot.width() * 129.0 / lane.length as f32;
        assert_eq!(pointer_tick(at, plot, lane.length, 30), 120);
        assert_eq!(pointer_tick(at, plot, lane.length, 15), 135);
        assert_eq!(pointer_tick(at, plot, lane.length, 0), 129);
    }
    #[test]
    fn typed_node_beats_are_tick_exact_ignore_grid_and_do_not_change_loop_length() {
        let ctx = egui::Context::default();
        let mut lane = ParameterLane::create(Target::FilterCutoff);
        lane.points[1].tick = 119;
        let mut state = EditorState {
            selected: 1,
            snap: PPQ,
            ..Default::default()
        };
        frame(&ctx, &mut lane, &mut state, vec![]);
        frame(&ctx, &mut lane, &mut state, vec![]);
        let id = state.time_id.unwrap();
        type_number(&ctx, &mut lane, &mut state, id, None);
        assert_eq!(
            lane.points[1].tick, 119,
            "Untouched Enter cannot round away a tick"
        );
        let id = state.time_id.unwrap();
        type_number(&ctx, &mut lane, &mut state, id, Some("0.129166666666667"));
        assert_eq!(lane.points[1].tick, 124);
        assert_eq!(lane.length, PPQ * 4);
        let id = state.time_id.unwrap();
        type_number(&ctx, &mut lane, &mut state, id, Some("999"));
        assert_eq!(lane.points[1].tick, lane.length - 1);
        assert_eq!(lane.points.len(), 3);
        for tick in 0..=MAX_LENGTH {
            assert_eq!(beats_to_tick(tick as f64 / PPQ as f64, MAX_LENGTH), tick);
        }
    }
    #[test]
    fn typed_short_loop_and_shape_reset_keep_period_and_nodes_well_defined() {
        let ctx = egui::Context::default();
        let mut lane = ParameterLane::create(Target::FilterCutoff);
        lane.enabled = true;
        lane.interpolation = Interpolation::Step;
        let mut state = EditorState::default();
        frame(&ctx, &mut lane, &mut state, vec![]);
        frame(&ctx, &mut lane, &mut state, vec![]);
        let id = state.length_id.unwrap();
        type_number(&ctx, &mut lane, &mut state, id, Some("0.1"));
        assert_eq!(lane.length, 96);
        assert_eq!(lane.points.len(), 3);
        assert!(
            lane.points
                .windows(2)
                .all(|pair| pair[0].tick < pair[1].tick)
        );
        reset_shape(&mut lane);
        assert_eq!(lane.length, 96);
        assert!(lane.enabled);
        assert_eq!(lane.interpolation, Interpolation::Step);
        assert_eq!(lane.points[0].tick, 0);
        assert_eq!(lane.points.last().unwrap().tick, 96);
    }
    #[test]
    fn node_drag_keeps_grab_offset_and_vertical_drag_keeps_offgrid_time() {
        for snap in [0, PPQ / 4] {
            let ctx = egui::Context::default();
            let mut lane = ParameterLane::create(Target::FilterCutoff);
            lane.points[1].tick = 1273;
            let mut state = EditorState {
                snap,
                ..Default::default()
            };
            frame(&ctx, &mut lane, &mut state, vec![]);
            frame(&ctx, &mut lane, &mut state, vec![]);
            let plot = state.plot.unwrap();
            let center = point_position(plot, lane.length, lane.points[1]);
            let grab = center + vec2(7.0, 4.0);
            let target = grab - vec2(0.0, 24.0);
            drag(&ctx, &mut lane, &mut state, grab, target);
            assert_eq!(
                lane.points[1].tick, 1273,
                "Vertical drag must not jump onto a nearby grid line"
            );
            let actual = point_position(plot, lane.length, lane.points[1]);
            assert!(
                actual.distance(center - vec2(0.0, 24.0)) < 0.01,
                "Drag should preserve its grab offset"
            );
            assert_eq!(lane.points.len(), 3);
        }
    }
    #[test]
    fn adding_deleting_and_readding_nodes_has_no_hidden_shape_history() {
        for mode in [
            Interpolation::Step,
            Interpolation::Linear,
            Interpolation::Curve,
        ] {
            let ctx = egui::Context::default();
            let mut lane = ParameterLane::create(Target::FilterCutoff);
            lane.interpolation = mode;
            lane.rescale_length(96);
            let mut state = EditorState::default();
            frame(&ctx, &mut lane, &mut state, vec![]);
            frame(&ctx, &mut lane, &mut state, vec![]);
            let plot = state.plot.unwrap();
            let at = pos2(
                plot.left() + plot.width() * 23.0 / 96.0,
                plot.bottom() - plot.height() * 0.61,
            );
            click(
                &ctx,
                &mut lane,
                &mut state,
                at,
                egui::PointerButton::Primary,
            );
            assert_eq!(lane.points.len(), 4);
            let first = lane.clone();
            click(
                &ctx,
                &mut lane,
                &mut state,
                at,
                egui::PointerButton::Secondary,
            );
            assert_eq!(lane.points.len(), 3);
            assert_eq!(lane.length, 96);
            click(
                &ctx,
                &mut lane,
                &mut state,
                at,
                egui::PointerButton::Primary,
            );
            assert_eq!(lane, first, "{mode:?} must depend only on current points");
            state.undo(&mut lane);
            state.redo(&mut lane);
            assert_eq!(lane, first);
        }
    }
    #[test]
    fn undo_during_drag_and_redo_keep_history_bounded() {
        let mut lane = ParameterLane::create(Target::FilterCutoff);
        let mut state = EditorState::default();
        for i in 0..100 {
            let old = lane.clone();
            lane.points[1].value = i as f32 / 100.0;
            state.remember(old, &lane);
        }
        state.drag = Some(Drag {
            index: 1,
            curve: false,
            before: lane.clone(),
            offset: egui::Vec2::ZERO,
        });
        lane.points[1].value = 0.0;
        for _ in 0..100 {
            state.undo(&mut lane);
        }
        for _ in 0..100 {
            state.redo(&mut lane);
            assert!(state.undo.len() <= 64 && state.redo.len() <= 64);
        }
    }
    #[test]
    fn odd_tick_segments_place_curve_handles_on_the_current_line_without_drifting() {
        let ctx = egui::Context::default();
        let mut lane = ParameterLane::create(Target::FilterCutoff);
        lane.rescale_length(96);
        lane.interpolation = Interpolation::Curve;
        lane.points[1].tick = 23;
        lane.points[0].curve = f32::from_bits(0x3dabcea1);
        let mut state = EditorState::default();
        frame(&ctx, &mut lane, &mut state, vec![]);
        let before = lane.clone();
        for _ in 0..10 {
            let output = frame(&ctx, &mut lane, &mut state, vec![]);
            assert_eq!(
                lane, before,
                "Merely displaying curvature cannot round/change it"
            );
            let plot = state.plot.unwrap();
            let handle = curve_handles(&lane, plot)[0].1;
            let paths = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Path(path)
                        if path.stroke.width == 2.0 && path.points.len() == 49 =>
                    {
                        Some(path)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(paths.len(), 2);
            assert!(paths[0].points[24].distance(handle) < 0.01);
        }
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
