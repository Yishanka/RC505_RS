use super::*;
use crate::{engine::core::Action, presets::FxTarget};
use egui::Key;

fn pressed(input: &egui::InputState, key: Key) -> bool {
    input
        .events
        .iter()
        .any(|e| matches!(e,egui::Event::Key{key:k,pressed:true,repeat:false,..} if *k==key))
}
impl MyApp {
    fn release_momentary(&mut self, index: usize) {
        if let Some(held) = self.held_fx[index].take() {
            if let Some(track) = held.track {
                self.config.track_fx.tracks[track].enabled[held.bank][held.slot] = held.previous;
            } else {
                self.config.input_fx.banks[held.bank].slots[held.slot].is_enabled = held.previous;
            }
        }
    }
    pub(super) fn handle_input(&mut self, ctx: &egui::Context) {
        let mut input = ctx.input(Clone::clone);
        let text = ctx.wants_keyboard_input();
        let popup_open = ctx.memory(|m| m.any_popup_open());
        let performance = input.focused
            && !text
            && !popup_open
            && !self.editor.expanded
            && !self.calibration_open
            && !self.help_open
            && !self.player_open
            && !self.replay_browser
            && self.draft.is_none()
            && self.focus == Focus::Performance
            && self.app_state == AppState::MainLoop
            && !self.show_save_prompt
            && !self.performance_locked();
        if performance {
            self.performance_keys.poll(&mut input);
        } else {
            self.performance_keys.suspend();
        }
        let fx_keys = [
            Key::Q,
            Key::W,
            Key::E,
            Key::R,
            Key::U,
            Key::I,
            Key::O,
            Key::P,
        ];
        for (index, key) in fx_keys.iter().enumerate() {
            if !performance || !input.key_down(*key) || !input.modifiers.shift {
                self.release_momentary(index);
            }
        }
        if !performance {
            self.fader_keys.fill(faders::KeyFader::default());
            self.speed_keys.fill(faders::KeyFader::default());
            self.clear_gesture.cancel();
        }
        if !input.focused {
            return;
        }
        if self.app_state == AppState::MainLoop
            && input.modifiers.is_none()
            && pressed(&input, Key::F9)
        {
            if self.show_save_prompt || self.help_open || self.calibration_open {
                self.status = self
                    .language
                    .choose(
                        "Close the current dialog before recording.",
                        "请先关闭当前对话框再录制回放。",
                    )
                    .into();
            } else {
                self.toggle_take();
            }
            return;
        }
        if self.app_state == AppState::MainLoop
            && !self.show_save_prompt
            && !self.calibration_open
            && input.modifiers.is_none()
            && pressed(&input, Key::F10)
        {
            self.open_replays();
            return;
        }
        if self.show_save_prompt {
            if pressed(&input, Key::Escape) && !self.busy() {
                self.show_save_prompt = false;
                self.pending_exit = None;
            }
            return;
        }
        if self.calibration_open {
            if pressed(&input, Key::Escape) {
                self.calibration_open = false;
            }
            return;
        }
        if pressed(&input, Key::F12) {
            self.help_open = !self.help_open;
            return;
        }
        if self.help_open {
            if pressed(&input, Key::Escape) {
                self.help_open = false;
            }
            return;
        }
        if self.player_open {
            if input.modifiers.is_none() && pressed(&input, Key::Space) {
                ctx.input_mut(|i| {
                    i.consume_key(egui::Modifiers::NONE, Key::Space);
                });
                self.send(Control::PlayerToggle);
            }
            if pressed(&input, Key::Escape) {
                self.close_player();
            }
            return;
        }
        if self.replay_browser {
            if pressed(&input, Key::Escape) {
                self.replay_browser = false;
                self.replay_autoplay = false;
            }
            return;
        }
        if self.draft.is_some() {
            return;
        }
        if self.app_state == AppState::Init {
            if !text && !self.busy() {
                if pressed(&input, Key::ArrowUp) || pressed(&input, Key::ArrowLeft) {
                    self.sel_project_idx = self.sel_project_idx.saturating_sub(1);
                }
                if pressed(&input, Key::ArrowDown) || pressed(&input, Key::ArrowRight) {
                    self.sel_project_idx =
                        (self.sel_project_idx + 1).min(self.projects.len().saturating_sub(1));
                }
                if pressed(&input, Key::Enter) {
                    self.open_project(self.sel_project_idx);
                }
                if pressed(&input, Key::N) {
                    self.project_name_mode = Some(ProjectNameMode::Add);
                    self.project_name_input.clear();
                }
            }
            return;
        }
        if self.performance_locked() {
            return;
        }
        // A menu owns its arrows and Escape until it closes.
        if ctx.memory(|m| m.any_popup_open()) {
            return;
        }
        if input.modifiers.ctrl && pressed(&input, Key::S) {
            if input.modifiers.shift {
                self.save_snapshot();
            } else {
                self.save_now();
            }
            return;
        }
        if input.modifiers.is_none() && pressed(&input, Key::F6) {
            self.focus_panel(ctx, Focus::Transport);
            return;
        }
        if input.modifiers.is_none() && pressed(&input, Key::F7) && !self.editor.expanded {
            self.focus_panel(ctx, Focus::Left);
            return;
        }
        if input.modifiers.is_none() && pressed(&input, Key::F8) {
            self.focus_panel(
                ctx,
                if self.editor.expanded {
                    Focus::Editor
                } else {
                    Focus::Right
                },
            );
            return;
        }
        if self.editor.expanded
            && input.modifiers.ctrl
            && !input.modifiers.alt
            && pressed(&input, Key::Tab)
        {
            self.editor.cycle_page(&self.config, input.modifiers.shift);
            self.focus_panel(ctx, Focus::Editor);
            consume_navigation_keys(ctx);
            return;
        }
        if pressed(&input, Key::Escape) {
            if self.editor.expanded || self.focus != Focus::Performance || text {
                self.editor.expanded = false;
                self.focus_panel(ctx, Focus::Performance);
            } else {
                self.back_to_projects();
            }
            return;
        }
        if self.focus != Focus::Performance {
            let no_command =
                !input.modifiers.ctrl && !input.modifiers.alt && !input.modifiers.mac_cmd;
            let editing = ctx
                .memory(|m| m.focused())
                .is_some_and(|id| egui::TextEdit::load_state(ctx, id).is_some());
            let next = no_command
                && ((!editing && input.key_pressed(Key::ArrowDown))
                    || input.key_pressed(Key::Tab) && !input.modifiers.shift);
            let previous = no_command
                && ((!editing && input.key_pressed(Key::ArrowUp))
                    || input.key_pressed(Key::Tab) && input.modifiers.shift);
            if next || previous {
                consume_navigation_keys(ctx);
                ui::navigation::advance(ctx, self.focus, if next { 1 } else { -1 });
            }
            return;
        }
        if !performance {
            return;
        }
        let selected = self.track_sel.unwrap_or(0);
        if input.modifiers.is_none()
            && !pressed(&input, Key::ArrowLeft)
            && !pressed(&input, Key::ArrowRight)
            && !input.pointer.any_pressed()
        {
            if self.clear_gesture.update(
                selected,
                input.key_down(Key::Delete),
                pressed(&input, Key::Delete),
                input.time,
            ) {
                self.clear_track(selected);
            }
        } else {
            self.clear_gesture.cancel();
        }
        let numbers = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5];
        for (index, key) in numbers.into_iter().enumerate() {
            if pressed(&input, key) {
                if input.modifiers.alt {
                    if input.modifiers.ctrl {
                        self.redo_track(index);
                    } else {
                        self.undo_track(index);
                    }
                } else if input.modifiers.ctrl {
                    self.track_sel = Some(index);
                } else if input.modifiers.shift {
                    self.pause_track(index);
                } else {
                    self.trigger_track(index);
                }
            }
        }
        if input.modifiers.ctrl && input.modifiers.alt {
            return;
        }
        if input.modifiers.ctrl {
            let index = self.track_sel.unwrap_or(0);
            if pressed(&input, Key::Z) && !input.modifiers.shift && self.view.tracks[index].undo {
                self.undo_track(index);
            }
            if (pressed(&input, Key::Y) || pressed(&input, Key::Z) && input.modifiers.shift)
                && self.view.tracks[index].redo
            {
                self.redo_track(index);
            }
            for (index, key) in fx_keys.into_iter().enumerate() {
                if pressed(&input, key) {
                    let slot = index % 4;
                    let target = if index < 4 {
                        FxTarget::Input {
                            bank: self.config.input_fx.sel_bank_idx,
                            slot,
                        }
                    } else {
                        FxTarget::Track {
                            bank: self.config.track_fx.sel_bank_idx,
                            slot,
                        }
                    };
                    self.editor.select(target);
                    self.focus_panel(ctx, Focus::Right);
                }
            }
            return;
        }
        if pressed(&input, Key::Space) {
            if input.modifiers.shift {
                self.action(Action::Panic);
            } else {
                self.toggle_all();
            }
        }
        for (index, key) in [Key::F1, Key::F2, Key::F3, Key::F4, Key::F5]
            .into_iter()
            .enumerate()
        {
            if pressed(&input, key) && !input.modifiers.alt {
                self.pause_track(index);
            }
        }
        if pressed(&input, Key::ArrowLeft) {
            self.track_sel = Some((self.track_sel.unwrap_or(0) + 4) % 5);
        }
        if pressed(&input, Key::ArrowRight) {
            self.track_sel = Some((self.track_sel.unwrap_or(0) + 1) % 5);
        }
        if pressed(&input, Key::T) && self.stopped() {
            self.config.beat_config.tap_calc.calculate_avg_bpm();
            self.config.beat_config.input_bpm.value = self.config.beat_config.tap_calc.value;
        }
        for (index, key) in fx_keys.into_iter().enumerate() {
            let slot = index % 4;
            let track = (index >= 4).then_some(self.track_sel.unwrap_or(0));
            let bank = if track.is_some() {
                self.config.track_fx.sel_bank_idx
            } else {
                self.config.input_fx.sel_bank_idx
            };
            if input.modifiers.alt {
                if pressed(&input, key) {
                    if track.is_some() {
                        self.config.track_fx.select_bank(slot);
                    } else {
                        self.config.input_fx.select_bank(slot);
                    }
                }
            } else if input.modifiers.shift && input.key_down(key) && self.held_fx[index].is_none()
            {
                let previous = if let Some(track) = track {
                    let v = &mut self.config.track_fx.tracks[track].enabled[bank][slot];
                    let old = *v;
                    *v = true;
                    old
                } else {
                    let v = &mut self.config.input_fx.banks[bank].slots[slot].is_enabled;
                    let old = *v;
                    *v = true;
                    old
                };
                self.held_fx[index] = Some(HeldFx {
                    bank,
                    slot,
                    track,
                    previous,
                });
            } else if pressed(&input, key) && !input.modifiers.shift {
                if let Some(track) = track {
                    self.config.track_fx.toggle_slot_enabled(track, slot);
                } else {
                    self.config.input_fx.toggle_slot_enabled(slot);
                }
            }
        }
        for (index, (down, up)) in [
            (Key::Z, Key::X),
            (Key::C, Key::V),
            (Key::B, Key::N),
            (Key::M, Key::Comma),
            (Key::Period, Key::Slash),
        ]
        .into_iter()
        .enumerate()
        {
            let direction = i8::from(input.key_down(up)) - i8::from(input.key_down(down));
            if input.modifiers.alt {
                continue;
            }
            if input.modifiers.shift {
                self.fader_keys[index] = faders::KeyFader::default();
                let delta = self.speed_keys[index].delta(direction, input.stable_dt, 20.0, 1.0);
                self.config.track_options[index].fader_speed =
                    (self.config.track_options[index].fader_speed + delta).clamp(1.0, 60.0);
            } else {
                self.speed_keys[index] = faders::KeyFader::default();
                self.fader_keys[index].advance(
                    &mut self.config.track_levels[index],
                    direction,
                    input.stable_dt,
                    self.config.track_options[index].fader_speed,
                );
            }
        }
    }
}

fn consume_navigation_keys(ctx: &egui::Context) {
    ctx.input_mut(|i| {
        i.events.retain(|e| {
            !matches!(
                e,
                egui::Event::Key {
                    key: Key::Tab | Key::ArrowUp | Key::ArrowDown,
                    ..
                }
            )
        })
    });
}
