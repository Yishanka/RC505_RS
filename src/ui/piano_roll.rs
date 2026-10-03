use super::theme;
use crate::config::{
    note_configs::{NoteConfigs, NoteOct},
    sequence_edit::{MAX_TICKS, NoteEvent, PPQ, TICKS_PER_BAR},
};
use eframe::egui::{self, Color32, Key, Rect, Stroke, pos2, vec2};

#[derive(Clone, PartialEq)]
struct Snapshot {
    notes: Vec<NoteEvent>,
    length: usize,
    name: String,
    id: String,
}
impl Snapshot {
    fn capture(config: &NoteConfigs) -> Self {
        Self {
            notes: config.events(),
            length: config.loop_len(),
            name: config.clip_name.clone(),
            id: config.clip_id.clone(),
        }
    }
    fn restore(&self, config: &mut NoteConfigs) {
        config.replace_events(self.length, &self.notes);
        config.clip_name = self.name.clone();
        config.clip_id = self.id.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(
        ctx: &egui::Context,
        config: &mut NoteConfigs,
        state: &mut PianoRollState,
        events: Vec<egui::Event>,
    ) {
        let modifiers = events
            .iter()
            .find_map(|event| {
                if let egui::Event::PointerButton { modifiers, .. } = event {
                    Some(*modifiers)
                } else {
                    None
                }
            })
            .unwrap_or_default();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1300.0, 1000.0))),
            modifiers,
            events,
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| draw(ui, config, state, None));
        });
    }
    fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }
    }
    #[test]
    fn mouse_draw_move_resize_delete_and_history() {
        let ctx = egui::Context::default();
        let mut config = NoteConfigs::new();
        let mut state = PianoRollState::default();
        frame(&ctx, &mut config, &mut state, vec![]);
        frame(&ctx, &mut config, &mut state, vec![]);
        let start = state.grid_rect.min + vec2(5.0, 71.0 * 18.0 + 8.0);
        frame(
            &ctx,
            &mut config,
            &mut state,
            vec![egui::Event::PointerMoved(start), button(start, true)],
        );
        frame(&ctx, &mut config, &mut state, vec![button(start, false)]);
        assert_eq!(
            config.events(),
            vec![NoteEvent {
                id: 1,
                velocity: 100,
                start: 0,
                len: 240,
                pitch: NoteOct::from_pitch_index(48)
            }]
        );
        let grid_y = state.grid_rect.top();
        let moved = start + vec2(54.0, -36.0);
        frame(&ctx, &mut config, &mut state, vec![button(start, true)]);
        frame(
            &ctx,
            &mut config,
            &mut state,
            vec![egui::Event::PointerMoved(moved)],
        );
        frame(&ctx, &mut config, &mut state, vec![button(moved, false)]);
        assert_eq!(
            state.grid_rect.top(),
            grid_y,
            "selection must not move the grid"
        );
        assert_eq!(config.events()[0].start, 480);
        assert_eq!(config.events()[0].pitch.pitch_index(), 50);
        let edge = state.grid_rect.min + vec2(9.0 * 9.0 - 3.0, 69.0 * 18.0 + 8.0);
        frame(
            &ctx,
            &mut config,
            &mut state,
            vec![egui::Event::PointerMoved(edge), button(edge, true)],
        );
        frame(
            &ctx,
            &mut config,
            &mut state,
            vec![egui::Event::PointerMoved(edge + vec2(27.0, 0.0))],
        );
        frame(
            &ctx,
            &mut config,
            &mut state,
            vec![button(edge + vec2(27.0, 0.0), false)],
        );
        assert_eq!(config.events()[0].len, 480);
        state.undo(&mut config);
        assert_eq!(config.events()[0].len, 240);
        state.redo(&mut config);
        assert_eq!(config.events()[0].len, 480);
        let old = Snapshot::capture(&config);
        config.remove_event(480);
        state.remember(old, &config);
        assert!(config.events().is_empty());
        state.undo(&mut config);
        assert_eq!(config.events()[0].len, 480);
        state.undo(&mut config);
        assert_eq!(config.events()[0].len, 240);
        state.undo(&mut config);
        assert_eq!(config.events()[0].start, 0);
        state.undo(&mut config);
        assert!(config.events().is_empty());
        for _ in 0..4 {
            state.redo(&mut config);
        }
        assert!(config.events().is_empty());
    }
    #[test]
    fn chord_mouse_edits_and_same_pitch_overlap_delete_only_one_note() {
        let ctx = egui::Context::default();
        let mut config = NoteConfigs::new();
        let mut state = PianoRollState::default();
        frame(&ctx, &mut config, &mut state, vec![]);
        frame(&ctx, &mut config, &mut state, vec![]);
        let c = state.grid_rect.min + vec2(5.0, 71.0 * 18.0 + 8.0);
        let e = c - vec2(0.0, 4.0 * 18.0);
        for pos in [c, e] {
            frame(
                &ctx,
                &mut config,
                &mut state,
                vec![egui::Event::PointerMoved(pos), button(pos, true)],
            );
            frame(&ctx, &mut config, &mut state, vec![button(pos, false)]);
        }
        assert_eq!(config.events().len(), 2);
        for pressed in [true, false] {
            frame(
                &ctx,
                &mut config,
                &mut state,
                vec![
                    egui::Event::PointerMoved(c),
                    egui::Event::PointerButton {
                        pos: c,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::CTRL,
                    },
                ],
            );
        }
        assert_eq!(config.events().len(), 3);
        let selected = state.selected.unwrap();
        assert_eq!(
            selected,
            config.events().iter().map(|e| e.id).max().unwrap()
        );
        frame(
            &ctx,
            &mut config,
            &mut state,
            vec![egui::Event::Key {
                key: Key::Delete,
                physical_key: Some(Key::Delete),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(config.events().len(), 2);
        assert!(!config.events().iter().any(|n| n.id == selected));
        state.undo(&mut config);
        assert_eq!(config.events().len(), 3);
        state.redo(&mut config);
        assert_eq!(config.events().len(), 2);
    }
}

#[cfg(test)]
mod scroll_tests {
    use super::*;
    fn frame(
        ctx: &egui::Context,
        config: &mut NoteConfigs,
        state: &mut PianoRollState,
        events: Vec<egui::Event>,
        modifiers: egui::Modifiers,
    ) {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1300.0, 1000.0))),
            modifiers,
            events,
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| draw(ui, config, state, None));
        });
    }
    #[test]
    fn wheel_shift_wheel_pan_and_edit_after_scrolling() {
        let ctx = egui::Context::default();
        let mut config = NoteConfigs::new();
        config.replace_events(MAX_TICKS, &[]);
        let mut state = PianoRollState::default();
        let plain = egui::Modifiers::NONE;
        frame(&ctx, &mut config, &mut state, vec![], plain);
        frame(&ctx, &mut config, &mut state, vec![], plain);
        let pos = state.viewport_rect.center();
        let before = state.scroll;
        frame(
            &ctx,
            &mut config,
            &mut state,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::Scroll(vec2(0.0, -120.0)),
            ],
            plain,
        );
        assert!(state.scroll.y > before.y, "Wheel must move pitch viewport");
        frame(
            &ctx,
            &mut config,
            &mut state,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::Scroll(vec2(0.0, -140.0)),
            ],
            egui::Modifiers::SHIFT,
        );
        assert!(state.scroll.x > 0.0, "Shift+wheel must move time viewport");
        let scroll = state.scroll;
        let mouse = |pos, pressed, button| egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: Default::default(),
        };
        frame(
            &ctx,
            &mut config,
            &mut state,
            vec![mouse(pos, true, egui::PointerButton::Middle)],
            plain,
        );
        frame(
            &ctx,
            &mut config,
            &mut state,
            vec![egui::Event::PointerMoved(pos - vec2(70.0, 36.0))],
            plain,
        );
        frame(
            &ctx,
            &mut config,
            &mut state,
            vec![mouse(
                pos - vec2(70.0, 36.0),
                false,
                egui::PointerButton::Middle,
            )],
            plain,
        );
        assert!(state.scroll.x > scroll.x && state.scroll.y > scroll.y);
        assert!(config.events().is_empty(), "Panning must not draw notes");
        for _ in 0..15 {
            frame(&ctx, &mut config, &mut state, vec![], plain);
        }
        let click = state.viewport_rect.center();
        let expected_pitch =
            119usize.saturating_sub(((click.y - state.grid_rect.top()) / 18.0) as usize);
        let expected_tick =
            (((click.x - state.grid_rect.left()) / state.zoom) as usize / state.snap) * state.snap;
        frame(
            &ctx,
            &mut config,
            &mut state,
            vec![
                egui::Event::PointerMoved(click),
                mouse(click, true, egui::PointerButton::Primary),
            ],
            plain,
        );
        frame(
            &ctx,
            &mut config,
            &mut state,
            vec![mouse(click, false, egui::PointerButton::Primary)],
            plain,
        );
        assert_eq!(config.events()[0].pitch.pitch_index(), expected_pitch);
        assert_eq!(config.events()[0].start, expected_tick);
    }
}

struct Drag {
    before: Snapshot,
    note: NoteEvent,
    origin: egui::Pos2,
    resize: bool,
    scroll_origin: egui::Vec2,
}

pub struct PianoRollState {
    pub snap: usize,
    pub octave: usize,
    pub zoom: f32,
    row_height: f32,
    center_notes: bool,
    scroll: egui::Vec2,
    viewport_rect: Rect,
    selected: Option<u64>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    clipboard: Option<Snapshot>,
    drag: Option<Drag>,
    #[cfg(test)]
    grid_rect: Rect,
}

impl Default for PianoRollState {
    fn default() -> Self {
        Self {
            snap: 240,
            octave: 4,
            zoom: 9.0 / 80.0,
            row_height: 18.0,
            center_notes: true,
            scroll: vec2(0.0, 59.0 * 18.0),
            viewport_rect: Rect::NOTHING,
            selected: None,
            undo: vec![],
            redo: vec![],
            clipboard: None,
            drag: None,
            #[cfg(test)]
            grid_rect: Rect::NOTHING,
        }
    }
}

impl PianoRollState {
    pub fn remember_clip(
        &mut self,
        clip: crate::config::sequence_edit::NoteClip,
        after: &NoteConfigs,
    ) {
        self.remember(
            Snapshot {
                notes: clip.events,
                length: clip.length,
                name: clip.name,
                id: clip.id,
            },
            after,
        );
        self.selected = None;
    }
    pub fn reset_history(&mut self) {
        self.center_notes = true;
        self.undo.clear();
        self.redo.clear();
        self.selected = None;
        self.drag = None;
    }
    fn remember(&mut self, before: Snapshot, config: &NoteConfigs) {
        if before != Snapshot::capture(config) {
            if self.undo.len() == 64 {
                self.undo.remove(0);
            }
            self.undo.push(before);
            self.redo.clear();
        }
    }
    fn undo(&mut self, config: &mut NoteConfigs) {
        if let Some(previous) = self.undo.pop() {
            self.redo.push(Snapshot::capture(config));
            previous.restore(config);
            self.selected = None;
        }
    }
    fn redo(&mut self, config: &mut NoteConfigs) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(Snapshot::capture(config));
            next.restore(config);
            self.selected = None;
        }
    }
}

pub fn draw(
    ui: &mut egui::Ui,
    config: &mut NoteConfigs,
    state: &mut PianoRollState,
    elapsed_beats: Option<f64>,
) {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    // Finish a drag whose release happened while this page was hidden.
    if ui.input(|i| !i.pointer.primary_down() && !i.pointer.any_released()) {
        if let Some(drag) = state.drag.take() {
            state.remember(drag.before, config);
        }
    }
    let before = Snapshot::capture(config);
    let mut history_action = false;
    let mut fit = false;
    let mut jump_pitch = None;
    theme::control_row(ui, |ui| {
        ui.strong(lang.text("PIANO ROLL"));
        let snap_options = [
            (1, lang.choose("Free (960 PPQ)", "自由（960 PPQ）")),
            (60, "1/64"),
            (80, lang.choose("1/32 triplet", "1/32 三连音")),
            (120, "1/32"),
            (160, lang.choose("1/16 triplet", "1/16 三连音")),
            (240, "1/16"),
            (320, lang.choose("1/8 triplet", "1/8 三连音")),
            (480, "1/8"),
            (960, "1/4"),
        ];
        let snap_name = snap_options
            .iter()
            .find(|(ticks, _)| *ticks == state.snap)
            .map_or("1/16", |(_, name)| *name);
        egui::ComboBox::from_id_source("snap")
            .selected_text(format!("{}: {snap_name}", lang.text("Snap")))
            .show_ui(ui, |ui| {
                for (ticks, name) in snap_options {
                    ui.selectable_value(&mut state.snap, ticks, name);
                }
            });
        ui.label(lang.choose("Jump to octave", "定位八度"));
        if ui
            .add(egui::DragValue::new(&mut state.octave).clamp_range(0..=9))
            .changed()
        {
            state.center_notes = false;
            state.row_height = 18.0;
            jump_pitch = Some((state.octave * 12).min(119) as f32);
        }
        ui.add(
            egui::Slider::new(&mut state.zoom, (1.5 / 80.0)..=(24.0 / 80.0))
                .show_value(false)
                .text(lang.text("Zoom")),
        );
        fit = ui
            .button(lang.choose("Fit timeline", "适配时间轴"))
            .clicked();
        let mut bars = config.loop_len().max(1).div_ceil(TICKS_PER_BAR);
        ui.label(lang.text("Bars"));
        if ui
            .add(egui::DragValue::new(&mut bars).clamp_range(1..=8))
            .on_hover_text(
                lang.text("Resize the loop; shortening trims notes. Undo restores them."),
            )
            .changed()
        {
            config.replace_events(bars * TICKS_PER_BAR, &config.events());
        }
    });
    theme::control_row(ui, |ui| {
        if ui
            .add_enabled(!state.undo.is_empty(), egui::Button::new(lang.text("Undo")))
            .clicked()
        {
            state.undo(config);
            history_action = true;
        }
        if ui
            .add_enabled(!state.redo.is_empty(), egui::Button::new(lang.text("Redo")))
            .clicked()
        {
            state.redo(config);
            history_action = true;
        }
        if ui.button(lang.text("Copy pattern")).clicked() {
            state.clipboard = Some(Snapshot::capture(config));
        }
        if ui
            .add_enabled(
                state.clipboard.is_some(),
                egui::Button::new(lang.text("Paste")),
            )
            .clicked()
        {
            state.clipboard.as_ref().unwrap().restore(config);
            config.fork_clip_identity();
        }
        if ui
            .add_enabled(
                !config.seq().is_empty() && config.loop_len() * 2 <= MAX_TICKS,
                egui::Button::new(lang.text("Duplicate")),
            )
            .clicked()
        {
            config.duplicate();
        }
        for (label, amount) in [("−12", -12), ("−1", -1), ("+1", 1), ("+12", 12)] {
            if ui
                .button(label)
                .on_hover_text(lang.text("Transpose the entire pattern in semitones"))
                .clicked()
            {
                config.transpose(amount);
            }
        }
        if ui.button(lang.text("Clear notes")).clicked() {
            config.replace_events(config.loop_len(), &[]);
            state.selected = None;
        }
        if ui
            .button(lang.text("Starter melody"))
            .on_hover_text(
                lang.text(
                    "Replace with a one-bar C-minor phrase; Undo restores the previous pattern",
                ),
            )
            .clicked()
        {
            let notes = [48, 51, 55, 58, 55, 51, 46, 48]
                .iter()
                .enumerate()
                .map(|(i, p)| NoteEvent {
                    id: 0,
                    velocity: 100,
                    start: i * 480,
                    len: 400,
                    pitch: NoteOct::from_pitch_index(*p),
                })
                .collect::<Vec<_>>();
            config.replace_events(TICKS_PER_BAR, &notes);
        }
    });
    // Buttons for undo/redo manage their own stacks; don't re-record those actions.
    if !history_action {
        state.remember(before, config);
    }
    let editing_text = ui
        .memory(|m| m.focused())
        .is_some_and(|id| egui::TextEdit::load_state(ui.ctx(), id).is_some());
    if !editing_text && state.drag.is_none() {
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, Key::Z)) {
            state.undo(config);
        }
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, Key::Y)) {
            state.redo(config);
        }
        if ui.input(|i| i.key_pressed(Key::Delete)) {
            if let Some(id) = state.selected.take() {
                let old = Snapshot::capture(config);
                config.remove_id(id);
                state.remember(old, config);
            }
        }
    }
    if let Some(mut event) = config
        .events()
        .into_iter()
        .find(|n| Some(n.id) == state.selected)
    {
        ui.horizontal(|ui| {
            ui.set_min_height(24.0);
            ui.label(format!("{} {}", lang.text("Selected"), event.pitch));
            let old = event;
            ui.label(lang.choose("Start (beats)", "起点（拍）"));
            let mut start_beats = event.start as f64 / PPQ as f64;
            if ui
                .add(
                    egui::DragValue::new(&mut start_beats)
                        .speed(1.0 / PPQ as f64)
                        .max_decimals(4)
                        .clamp_range(
                            0.0..=config.loop_len().saturating_sub(event.len) as f64 / PPQ as f64,
                        ),
                )
                .changed()
            {
                event.start = (start_beats * PPQ as f64).round() as usize;
            }
            ui.label(lang.choose("Length (beats)", "时长（拍）"));
            let mut length_beats = event.len as f64 / PPQ as f64;
            if ui
                .add(
                    egui::DragValue::new(&mut length_beats)
                        .speed(1.0 / PPQ as f64)
                        .max_decimals(4)
                        .clamp_range(
                            1.0 / PPQ as f64
                                ..=config.loop_len().saturating_sub(event.start).max(1) as f64
                                    / PPQ as f64,
                        ),
                )
                .changed()
            {
                event.len = (length_beats * PPQ as f64).round().max(1.0) as usize;
            }
            ui.label(lang.choose("Velocity", "力度"));
            ui.add(egui::DragValue::new(&mut event.velocity).clamp_range(1..=127));
            if event != old {
                let before = Snapshot::capture(config);
                config.remove_id(old.id);
                config.insert_within_loop(event);
                state.selected = Some(event.id);
                state.remember(before, config);
            }
        });
    } else {
        ui.horizontal(|ui| {
            ui.set_min_height(24.0);
            theme::caption(ui, lang.text("Select a note to edit its start and length."));
        });
    }
    theme::caption(
        ui,
        lang.choose(
            "Left: edit · Ctrl+click: overlap · Right/Del: erase · Wheel: pitch · Shift+wheel: time · Middle: pan",
            "左键编辑 · Ctrl+点击叠加同音 · 右键/Del 删除 · 滚轮上下音高 · Shift+滚轮左右时间 · 中键平移",
        ),
    );
    let length = config.loop_len().max(TICKS_PER_BAR);
    let mut row_height = state.row_height;
    let highest = 119usize;
    // Bound the canvas so its scrollbars stay inside the workspace.
    let height = (ui.clip_rect().bottom() - ui.cursor().top() - 32.0).clamp(170.0, 600.0);
    if fit || state.center_notes {
        if fit {
            state.zoom =
                ((ui.available_width() - 86.0) / length as f32).clamp(1.5 / 80.0, 24.0 / 80.0);
        }
        let events = config.events();
        let low = events
            .iter()
            .map(|n| n.pitch.pitch_index())
            .min()
            .unwrap_or(48);
        let high = events
            .iter()
            .map(|n| n.pitch.pitch_index())
            .max()
            .unwrap_or(60);
        row_height = ((height - 41.0) / (high - low + 3) as f32).clamp(14.0, 18.0);
        state.row_height = row_height;
        let top = ((low + high) as f32 * 0.5 + (height - 41.0) / (2.0 * row_height)).min(119.0);
        state.scroll = vec2(0.0, (119.0 - top).max(0.0) * row_height);
        state.center_notes = false;
    }
    if let Some(pitch) = jump_pitch {
        let top = (pitch + (height - 41.0) / (2.0 * row_height)).min(119.0);
        state.scroll.y = (119.0 - top).max(0.0) * row_height;
    }
    let mut pan = egui::Vec2::ZERO;
    let scroll = state.scroll;
    let hover = state.viewport_rect;
    ui.input_mut(|i| {
        if i.modifiers.shift && i.pointer.hover_pos().is_some_and(|p| hover.contains(p)) {
            i.smooth_scroll_delta.x += i.smooth_scroll_delta.y;
            i.smooth_scroll_delta.y = 0.0;
        }
    });
    let area = egui::ScrollArea::both()
        .id_source("piano_scroll")
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .auto_shrink([false, false])
        .drag_to_scroll(false)
        .max_height(height)
        .min_scrolled_height(height)
        .scroll_offset(scroll)
        .show_viewport(ui, |ui, viewport| {
            let (rect, response) = ui.allocate_exact_size(
                vec2(58.0 + length as f32 * state.zoom, 25.0 + 120.0 * row_height),
                egui::Sense::click_and_drag(),
            );
            let grid = Rect::from_min_max(rect.min + vec2(58.0, 25.0), rect.max);
            let visible = Rect::from_min_size(rect.min + viewport.min.to_vec2(), viewport.size())
                .intersect(ui.clip_rect());
            let grid_clip = Rect::from_min_max(visible.min + vec2(58.0, 25.0), visible.max);
            state.viewport_rect = grid_clip;
            #[cfg(test)]
            {
                state.grid_rect = grid;
            }
            let painter = ui.painter().with_clip_rect(grid_clip);
            painter.rect_filled(grid, 4.0, theme::BACKGROUND);
            for row in 0..120 {
                let pitch = highest - row;
                let black = matches!(pitch % 12, 1 | 3 | 6 | 8 | 10);
                let y = grid.top() + row as f32 * row_height;
                if y + row_height < grid_clip.top() || y > grid_clip.bottom() {
                    continue;
                }
                painter.rect_filled(
                    Rect::from_min_size(pos2(grid.left(), y), vec2(grid.width(), row_height)),
                    0.0,
                    if black {
                        Color32::from_rgb(22, 28, 37)
                    } else {
                        Color32::from_rgb(29, 36, 47)
                    },
                );
                painter.hline(grid.x_range(), y, Stroke::new(0.5, Color32::from_gray(44)));
            }
            for tick in (0..=length).step_by(state.snap.max(60)) {
                let x = grid.left() + tick as f32 * state.zoom;
                if tick % state.snap == 0 {
                    painter.vline(
                        x,
                        grid.y_range(),
                        Stroke::new(
                            1.0,
                            Color32::from_gray(if tick % PPQ == 0 { 75 } else { 41 }),
                        ),
                    );
                }
            }
            if !config.seq().is_empty() && config.loop_len() < length {
                let edge = grid.left() + config.loop_len() as f32 * state.zoom;
                painter.rect_filled(
                    Rect::from_min_max(pos2(edge, grid.top()), grid.max),
                    0.0,
                    Color32::from_black_alpha(130),
                );
                painter.vline(edge, grid.y_range(), Stroke::new(2.0, theme::accent(ui)));
            }
            let note_rect = |event: NoteEvent| {
                let y = grid.top()
                    + (highest as i32 - event.pitch.pitch_index() as i32) as f32 * row_height;
                Rect::from_min_size(
                    pos2(grid.left() + event.start as f32 * state.zoom, y + 1.0),
                    vec2(
                        (event.len as f32 * state.zoom - 1.0).max(2.0),
                        row_height - 2.0,
                    ),
                )
            };
            for event in config.events() {
                let r = note_rect(event);
                if !grid.intersects(r) {
                    continue;
                }
                let selected = state.selected == Some(event.id);
                painter.rect_filled(
                    r,
                    3.0,
                    if selected {
                        theme::accent(ui).gamma_multiply(1.35)
                    } else {
                        theme::accent(ui)
                    },
                );
                if r.width() > 28.0 {
                    painter.text(
                        r.left_center() + vec2(4.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        event.pitch.to_string(),
                        egui::FontId::monospace(12.0),
                        theme::BACKGROUND,
                    );
                }
                painter.vline(
                    r.right() - 3.0,
                    r.y_range().shrink(3.0),
                    Stroke::new(1.0, theme::BACKGROUND),
                );
            }
            if let Some(beats) = elapsed_beats.filter(|_| !config.seq().is_empty()) {
                let tick = (beats * PPQ as f64).rem_euclid(config.loop_len() as f64) as f32;
                painter.vline(
                    grid.left() + tick * state.zoom,
                    grid.y_range(),
                    Stroke::new(2.0, Color32::from_rgb(255, 199, 106)),
                );
            }
            // Sticky keyboard and ruler remain visible while either axis moves.
            let headers = ui.painter().with_clip_rect(visible);
            headers.rect_filled(
                Rect::from_min_max(visible.min, pos2(visible.left() + 58.0, visible.bottom())),
                0.0,
                theme::BACKGROUND,
            );
            headers.rect_filled(
                Rect::from_min_max(visible.min, pos2(visible.right(), visible.top() + 25.0)),
                0.0,
                theme::BACKGROUND,
            );
            let keys = ui.painter().with_clip_rect(Rect::from_min_max(
                pos2(visible.left(), grid_clip.top()),
                pos2(grid_clip.left(), visible.bottom()),
            ));
            for row in 0..120 {
                let pitch = highest - row;
                let y = grid.top() + row as f32 * row_height;
                if y + row_height < grid_clip.top() || y > grid_clip.bottom() {
                    continue;
                }
                keys.text(
                    pos2(grid_clip.left() - 8.0, y + row_height * 0.5),
                    egui::Align2::RIGHT_CENTER,
                    NoteOct::from_pitch_index(pitch).to_string(),
                    egui::FontId::monospace(13.0),
                    theme::MUTED,
                );
            }
            let ruler = ui.painter().with_clip_rect(Rect::from_min_max(
                pos2(grid_clip.left(), visible.top()),
                pos2(visible.right(), grid_clip.top()),
            ));
            for tick in (0..length).step_by(PPQ) {
                ruler.text(
                    pos2(
                        grid.left() + tick as f32 * state.zoom + 4.0,
                        visible.top() + 5.0,
                    ),
                    egui::Align2::LEFT_TOP,
                    format!("{}.{}", tick / TICKS_PER_BAR + 1, (tick / PPQ) % 4 + 1),
                    egui::FontId::monospace(13.0),
                    theme::MUTED,
                );
            }
            if response.dragged_by(egui::PointerButton::Middle) {
                pan = -ui.input(|i| i.pointer.delta());
                ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::Grabbing);
            }
            let pointer = response.interact_pointer_pos();
            if let Some(pos) = pointer.filter(|p| grid_clip.contains(*p) && grid.contains(*p)) {
                let hit = config
                    .events()
                    .into_iter()
                    .rev()
                    .find(|event| note_rect(*event).contains(pos));
                if response.secondary_clicked() {
                    if let Some(event) = hit {
                        let old = Snapshot::capture(config);
                        config.remove_id(event.id);
                        state.remember(old, config);
                        state.selected = None;
                    }
                } else if response.drag_started() && ui.input(|i| i.pointer.primary_down()) {
                    // egui reports the drag after threshold movement: use the press origin for hit testing.
                    let origin = ui.input(|i| i.pointer.press_origin()).unwrap_or(pos);
                    if let Some(note) = config
                        .events()
                        .into_iter()
                        .rev()
                        .find(|e| note_rect(*e).contains(origin))
                    {
                        state.selected = Some(note.id);
                        state.drag = Some(Drag {
                            before: Snapshot::capture(config),
                            note,
                            origin,
                            scroll_origin: scroll,
                            resize: origin.x
                                > note_rect(note).right()
                                    - (note_rect(note).width() * 0.3).clamp(1.0, 7.0),
                        });
                    }
                } else if response.clicked() {
                    if let Some(event) = hit.filter(|_| !ui.input(|i| i.modifiers.ctrl)) {
                        state.selected = Some(event.id);
                    } else {
                        let old = Snapshot::capture(config);
                        let tick = (((pos.x - grid.left()) / state.zoom) as usize / state.snap)
                            * state.snap;
                        let pitch =
                            highest.saturating_sub(((pos.y - grid.top()) / row_height) as usize);
                        if config.seq().is_empty() {
                            config.replace_events(length, &[]);
                        }
                        if config.insert_within_loop(NoteEvent {
                            id: 0,
                            velocity: 100,
                            start: tick,
                            len: state.snap,
                            pitch: NoteOct::from_pitch_index(pitch),
                        }) {
                            state.selected = config
                                .event_slice()
                                .iter()
                                .rev()
                                .find(|e| e.start == tick && e.pitch.pitch_index() == pitch)
                                .map(|e| e.id);
                        }
                        state.remember(old, config);
                    }
                }
            }
            if let (Some(drag), Some(pos)) = (&state.drag, pointer) {
                let delta = ((pos.x - drag.origin.x + scroll.x - drag.scroll_origin.x)
                    / (state.zoom * state.snap as f32))
                    .round() as i32
                    * state.snap as i32;
                let mut event = drag.note;
                if drag.resize {
                    event.len = (event.len as i32 + delta)
                        .clamp(1, (config.loop_len().max(1) - event.start) as i32)
                        as usize;
                } else {
                    event.start = (event.start as i32 + delta)
                        .clamp(0, config.loop_len().saturating_sub(event.len) as i32)
                        as usize;
                    let semitones = ((drag.origin.y - pos.y + drag.scroll_origin.y - scroll.y)
                        / row_height)
                        .round() as i32;
                    event.pitch = NoteOct::from_pitch_index(
                        (event.pitch.pitch_index() as i32 + semitones).clamp(0, 119) as usize,
                    );
                }
                drag.before.restore(config);
                config.remove_id(drag.note.id);
                config.insert_within_loop(event);
                state.selected = Some(event.id);
            }
            if response.drag_stopped() {
                if let Some(drag) = state.drag.take() {
                    state.remember(drag.before, config);
                }
            }
        });
    state.scroll = (area.state.offset + pan).max(egui::Vec2::ZERO);
    theme::caption(
        ui,
        format!(
            "{} {} · {:.2} {}",
            config.events().len(),
            lang.choose("notes", "音符"),
            config.loop_len() as f32 / PPQ as f32,
            lang.choose("beats", "拍")
        ),
    );
}
