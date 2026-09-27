use super::theme;
use crate::config::{
    note_configs::{NoteConfigs, NoteOct},
    sequence_edit::{MAX_TICKS, NoteEvent, TICKS_PER_BAR},
};
use eframe::egui::{self, Color32, Key, Rect, Stroke, pos2, vec2};

#[derive(Clone, PartialEq)]
struct Snapshot {
    notes: Vec<Option<NoteOct>>,
    steps: Vec<usize>,
}
impl Snapshot {
    fn capture(config: &NoteConfigs) -> Self {
        Self {
            notes: config.seq().to_vec(),
            steps: config.step_len_seq().to_vec(),
        }
    }
    fn restore(&self, config: &mut NoteConfigs) {
        config.set_seq_with_steps(self.notes.clone(), self.steps.clone());
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
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1300.0, 1000.0))),
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
        let start = state.grid_rect.min + vec2(5.0, 12.0 * 18.0 + 8.0);
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
                start: 0,
                len: 3,
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
        assert_eq!(config.events()[0].start, 6);
        assert_eq!(config.events()[0].pitch.pitch_index(), 50);
        let edge = state.grid_rect.min + vec2(9.0 * 9.0 - 3.0, 10.0 * 18.0 + 8.0);
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
        assert_eq!(config.events()[0].len, 6);
        state.undo(&mut config);
        assert_eq!(config.events()[0].len, 3);
        state.redo(&mut config);
        assert_eq!(config.events()[0].len, 6);
        let old = Snapshot::capture(&config);
        config.remove_event(6);
        state.remember(old, &config);
        assert!(config.events().is_empty());
        state.undo(&mut config);
        assert_eq!(config.events()[0].len, 6);
    }
}

struct Drag {
    before: Snapshot,
    note: NoteEvent,
    origin: egui::Pos2,
    resize: bool,
}

pub struct PianoRollState {
    pub snap: usize,
    pub octave: usize,
    pub zoom: f32,
    selected: Option<usize>,
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
            snap: 3,
            octave: 3,
            zoom: 9.0,
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
    pub fn reset_history(&mut self) {
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
    let before = Snapshot::capture(config);
    let mut history_action = false;
    ui.horizontal_wrapped(|ui| {
        ui.strong(lang.text("PIANO ROLL"));
        let snap_name = match state.snap {
            1 => "1/48",
            2 => lang.text("1/24 triplet"),
            3 => "1/16",
            4 => lang.text("1/12 triplet"),
            6 => "1/8",
            _ => "1/4",
        };
        egui::ComboBox::from_id_source("snap")
            .selected_text(format!("{}: {snap_name}", lang.text("Snap")))
            .show_ui(ui, |ui| {
                for (ticks, name) in [
                    (1, "1/48"),
                    (2, lang.text("1/24 triplet")),
                    (3, "1/16"),
                    (4, lang.text("1/12 triplet")),
                    (6, "1/8"),
                    (12, "1/4"),
                ] {
                    ui.selectable_value(&mut state.snap, ticks, name);
                }
            });
        ui.label(lang.text("View octave"));
        ui.add(egui::DragValue::new(&mut state.octave).clamp_range(0..=7));
        ui.add(egui::Slider::new(&mut state.zoom, 4.0..=24.0).text(lang.text("Zoom")));
        let mut bars = config.seq().len().max(1).div_ceil(TICKS_PER_BAR);
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
    ui.horizontal_wrapped(|ui| {
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
        }
        if ui
            .add_enabled(
                !config.seq().is_empty() && config.seq().len() * 2 <= MAX_TICKS,
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
            config.replace_events(config.seq().len(), &[]);
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
                    start: i * 6,
                    len: 5,
                    pitch: NoteOct::from_pitch_index(*p),
                })
                .collect::<Vec<_>>();
            config.replace_events(48, &notes);
        }
    });
    // Buttons for undo/redo manage their own stacks; don't re-record those actions.
    if !history_action {
        state.remember(before, config);
    }
    if !ui.ctx().wants_keyboard_input() && state.drag.is_none() {
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, Key::Z)) {
            state.undo(config);
        }
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, Key::Y)) {
            state.redo(config);
        }
        if ui.input(|i| i.key_pressed(Key::Delete)) {
            if let Some(start) = state.selected.take() {
                let old = Snapshot::capture(config);
                config.remove_event(start);
                state.remember(old, config);
            }
        }
    }
    if let Some(mut event) = config
        .events()
        .into_iter()
        .find(|n| Some(n.start) == state.selected)
    {
        ui.horizontal(|ui| {
            ui.set_min_height(24.0);
            ui.label(format!("{} {}", lang.text("Selected"), event.pitch));
            let old = event;
            ui.label(lang.text("Start tick"));
            ui.add(egui::DragValue::new(&mut event.start).clamp_range(0..=MAX_TICKS - 1));
            ui.label(lang.text("Length"));
            ui.add(egui::DragValue::new(&mut event.len).clamp_range(1..=MAX_TICKS - event.start));
            if event != old {
                let before = Snapshot::capture(config);
                config.remove_event(old.start);
                config.insert_event(event);
                state.selected = Some(event.start);
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
        lang.text("Left click: draw / select  •  Drag: move  •  Drag right edge: resize  •  Right click / Delete: erase  •  Ctrl+Z / Ctrl+Y  •  Monophonic"),
    );
    let length = config.seq().len().max(TICKS_PER_BAR);
    let row_height = 18.0;
    let highest = state.octave * 12 + 24;
    egui::ScrollArea::horizontal()
        .id_source("piano_scroll")
        .show(ui, |ui| {
            let (rect, response) = ui.allocate_exact_size(
                vec2(58.0 + length as f32 * state.zoom, 25.0 + 25.0 * row_height),
                egui::Sense::click_and_drag(),
            );
            let grid = Rect::from_min_max(rect.min + vec2(58.0, 25.0), rect.max);
            #[cfg(test)]
            {
                state.grid_rect = grid;
            }
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 4.0, theme::BACKGROUND);
            for row in 0..25 {
                let pitch = highest - row;
                let black = matches!(pitch % 12, 1 | 3 | 6 | 8 | 10);
                let y = grid.top() + row as f32 * row_height;
                painter.rect_filled(
                    Rect::from_min_size(pos2(grid.left(), y), vec2(grid.width(), row_height)),
                    0.0,
                    if black {
                        Color32::from_rgb(22, 28, 37)
                    } else {
                        Color32::from_rgb(29, 36, 47)
                    },
                );
                painter.text(
                    pos2(grid.left() - 8.0, y + row_height * 0.5),
                    egui::Align2::RIGHT_CENTER,
                    NoteOct::from_pitch_index(pitch).to_string(),
                    egui::FontId::monospace(13.0),
                    if black { theme::MUTED } else { Color32::WHITE },
                );
                painter.hline(grid.x_range(), y, Stroke::new(0.5, Color32::from_gray(44)));
            }
            for tick in 0..=length {
                let x = grid.left() + tick as f32 * state.zoom;
                if tick % state.snap == 0 {
                    let strong = tick % 12 == 0;
                    painter.vline(
                        x,
                        grid.y_range(),
                        Stroke::new(1.0, Color32::from_gray(if strong { 75 } else { 41 })),
                    );
                }
                if tick % 12 == 0 && tick < length {
                    painter.text(
                        pos2(x + 4.0, rect.top() + 5.0),
                        egui::Align2::LEFT_TOP,
                        format!("{}.{}", tick / 48 + 1, (tick / 12) % 4 + 1),
                        egui::FontId::monospace(13.0),
                        theme::MUTED,
                    );
                }
            }
            let note_rect = |event: NoteEvent| {
                let y = grid.top()
                    + (highest as i32 - event.pitch.pitch_index() as i32) as f32 * row_height;
                Rect::from_min_size(
                    pos2(grid.left() + event.start as f32 * state.zoom, y + 1.0),
                    vec2(event.len as f32 * state.zoom - 1.0, row_height - 2.0),
                )
            };
            for event in config.events() {
                let r = note_rect(event);
                if !grid.contains(r.center()) {
                    continue;
                }
                let selected = state.selected == Some(event.start);
                painter.rect_filled(
                    r,
                    3.0,
                    if selected {
                        Color32::from_rgb(199, 246, 226)
                    } else {
                        theme::ACCENT
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
                let tick = (beats * 12.0).rem_euclid(config.seq().len() as f64) as f32;
                painter.vline(
                    grid.left() + tick * state.zoom,
                    grid.y_range(),
                    Stroke::new(2.0, Color32::from_rgb(255, 199, 106)),
                );
            }
            let pointer = response.interact_pointer_pos();
            if let Some(pos) = pointer.filter(|p| grid.contains(*p)) {
                let hit = config
                    .events()
                    .into_iter()
                    .find(|event| note_rect(*event).contains(pos));
                if response.secondary_clicked() {
                    if let Some(event) = hit {
                        let old = Snapshot::capture(config);
                        config.remove_event(event.start);
                        state.remember(old, config);
                        state.selected = None;
                    }
                } else if response.drag_started() {
                    // egui reports the drag after threshold movement: use the press origin for hit testing.
                    let origin = ui.input(|i| i.pointer.press_origin()).unwrap_or(pos);
                    if let Some(note) = config
                        .events()
                        .into_iter()
                        .find(|e| note_rect(*e).contains(origin))
                    {
                        state.selected = Some(note.start);
                        state.drag = Some(Drag {
                            before: Snapshot::capture(config),
                            note,
                            origin,
                            resize: origin.x > note_rect(note).right() - 7.0,
                        });
                    }
                } else if response.clicked() {
                    if let Some(event) = hit {
                        state.selected = Some(event.start);
                    } else {
                        let old = Snapshot::capture(config);
                        let tick = (((pos.x - grid.left()) / state.zoom) as usize / state.snap)
                            * state.snap;
                        let pitch = highest - ((pos.y - grid.top()) / row_height) as usize;
                        if config.seq().is_empty() {
                            config.replace_events(length, &[]);
                        }
                        config.insert_event(NoteEvent {
                            start: tick,
                            len: state.snap,
                            pitch: NoteOct::from_pitch_index(pitch),
                        });
                        state.selected = Some(tick);
                        state.remember(old, config);
                    }
                }
            }
            if let (Some(drag), Some(pos)) = (&state.drag, pointer) {
                let delta = ((pos.x - drag.origin.x) / (state.zoom * state.snap as f32)).round()
                    as i32
                    * state.snap as i32;
                let mut event = drag.note;
                if drag.resize {
                    event.len = (event.len as i32 + delta)
                        .clamp(1, (MAX_TICKS - event.start) as i32)
                        as usize;
                } else {
                    event.start = (event.start as i32 + delta)
                        .clamp(0, (MAX_TICKS - event.len) as i32)
                        as usize;
                    let semitones = ((drag.origin.y - pos.y) / row_height).round() as i32;
                    event.pitch = NoteOct::from_pitch_index(
                        (event.pitch.pitch_index() as i32 + semitones).clamp(0, 119) as usize,
                    );
                }
                drag.before.restore(config);
                config.remove_event(drag.note.start);
                config.insert_event(event);
                state.selected = Some(event.start);
            }
            if response.drag_stopped() {
                if let Some(drag) = state.drag.take() {
                    state.remember(drag.before, config);
                }
            }
        });
    let hidden = config
        .events()
        .iter()
        .filter(|n| n.pitch.pitch_index() < state.octave * 12 || n.pitch.pitch_index() > highest)
        .count();
    theme::caption(
        ui,
        format!(
            "{} notes • {} ticks • {:.2} beats • {} notes outside current octave view",
            config.events().len(),
            config.seq().len(),
            config.seq().len() as f32 / 12.0,
            hidden
        ),
    );
}
