use super::shortcuts::Command;
use super::*;
use crate::{engine::core::Action, presets::FxTarget};
use egui::Key;

fn pressed(input: &egui::InputState, key: Key) -> bool {
    input
        .events
        .iter()
        .any(|e| matches!(e,egui::Event::Key{key:k,pressed:true,repeat:false,..} if *k==key))
}
fn remove_button_repeat(input: &mut egui::InputState) {
    input.events.retain(|event| {
        !matches!(
            event,
            egui::Event::Key {
                key: Key::Enter | Key::Space,
                pressed: true,
                repeat: true,
                ..
            }
        )
    });
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
        let typing = ctx
            .memory(|m| m.focused())
            .is_some_and(|id| egui::TextEdit::load_state(ctx, id).is_some());
        if !typing {
            // egui treats repeated Enter/Space as another widget click. Keep
            // first activation and held state, without repeatedly toggling.
            remove_button_repeat(&mut input);
            ctx.input_mut(remove_button_repeat);
        }
        let local_editor_key = self.editor.expanded
            && input.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::Key {
                        key: Key::Delete,
                        pressed: true,
                        ..
                    } | egui::Event::Key {
                        key: Key::Z | Key::Y,
                        pressed: true,
                        modifiers: egui::Modifiers { ctrl: true, .. },
                        ..
                    }
                )
            });
        let global_keys = !typing
            && !local_editor_key
            && !self.player_open
            && !self.replay_browser
            && !self.master_fx_open;
        let popup_open = ctx.memory(|m| m.any_popup_open());
        if self.shortcut_editor.open && !self.show_save_prompt {
            let capturing = self.shortcut_editor.capture.is_some();
            if capturing && input.focused {
                self.performance_keys.poll(&mut input);
            } else {
                self.performance_keys.suspend();
            }
            for index in 0..8 {
                self.release_momentary(index);
            }
            self.fader_keys.fill(faders::KeyFader::default());
            self.speed_keys.fill(faders::KeyFader::default());
            self.clear_gesture.cancel();
            if input.focused {
                self.shortcut_editor.handle_input(&input, self.language);
            }
            if capturing {
                // The listening gesture owns these events, even if a search
                // TextEdit still has focus from before the Assign click.
                ctx.input_mut(|i| {
                    i.events
                        .retain(|e| !matches!(e, egui::Event::Key { .. } | egui::Event::Text(_)))
                });
            }
            return;
        }
        let performance = input.focused
            && !typing
            && !self.master_fx_open
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
        for index in 0..8 {
            if !performance || !self.shortcuts.held(Command::HoldFx(index), &input) {
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
            && global_keys
            && self.shortcuts.pressed(Command::Take, &input)
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
        if !self.show_save_prompt
            && !self.calibration_open
            && global_keys
            && self.shortcuts.pressed(Command::Replays, &input)
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
        if self.master_fx_open {
            if pressed(&input, Key::Escape) {
                self.master_fx_open = false;
            }
            return;
        }
        if global_keys && self.shortcuts.pressed(Command::Help, &input) {
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
            let modal = self
                .replay_panel
                .as_ref()
                .is_some_and(|panel| panel.modal_open());
            if input.modifiers.is_none() && !typing && !modal && pressed(&input, Key::Space) {
                ctx.input_mut(|i| {
                    i.consume_key(egui::Modifiers::NONE, Key::Space);
                });
                if let Some(panel) = &self.replay_panel {
                    panel.toggle();
                }
            }
            if pressed(&input, Key::Escape) {
                if !self
                    .replay_panel
                    .as_mut()
                    .is_some_and(|panel| panel.dismiss_modal())
                {
                    self.close_player();
                }
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
        if global_keys
            && (self.shortcuts.pressed(Command::Save, &input)
                || self.shortcuts.pressed(Command::Snapshot, &input))
        {
            if self.shortcuts.pressed(Command::Snapshot, &input) {
                self.save_snapshot();
            } else {
                self.save_now();
            }
            return;
        }
        if global_keys && self.shortcuts.pressed(Command::Top, &input) {
            self.focus_panel(ctx, Focus::Transport);
            return;
        }
        if global_keys && self.shortcuts.pressed(Command::Left, &input) && !self.editor.expanded {
            self.focus_panel(ctx, Focus::Left);
            return;
        }
        if global_keys && self.shortcuts.pressed(Command::Right, &input) {
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
        consume_performance_keys(ctx, &self.shortcuts);
        // Commit Tap before every transport action in this input batch; action()
        // then sends the resulting Config before sending Start to the callback.
        if self.shortcuts.pressed(Command::Tap, &input) && self.tempo_edit_allowed() {
            self.tap_tempo(input.time);
        }
        let selected = self.track_sel.unwrap_or(0);
        let is_pressed = |c| self.shortcuts.pressed(c, &input);
        let clear = is_pressed(Command::Clear);
        let clear_held = self.shortcuts.held(Command::Clear, &input);
        let changing_track = is_pressed(Command::PreviousTrack)
            || is_pressed(Command::NextTrack)
            || (0..5).any(|i| is_pressed(Command::Select(i)));
        if !changing_track && !input.pointer.any_pressed() {
            if self
                .clear_gesture
                .update(selected, clear_held, clear, input.time)
            {
                self.clear_track(selected);
            }
        } else {
            self.clear_gesture.cancel();
        }
        for index in 0..5 {
            if self.shortcuts.pressed(Command::Track(index), &input) {
                self.trigger_track(index);
            }
            if self.shortcuts.pressed(Command::Stop(index), &input) {
                self.pause_track(index);
            }
            if self.shortcuts.pressed(Command::Select(index), &input) {
                self.track_sel = Some(index);
            }
            if self.shortcuts.pressed(Command::Undo(index), &input) {
                self.undo_track(index);
            }
            if self.shortcuts.pressed(Command::Redo(index), &input) {
                self.redo_track(index);
            }
        }
        if self.shortcuts.pressed(Command::UndoSelected, &input) {
            self.undo_track(selected);
        }
        if self.shortcuts.pressed(Command::RedoSelected, &input) {
            self.redo_track(selected);
        }
        if self.shortcuts.pressed(Command::All, &input) {
            self.toggle_all();
        }
        if self.shortcuts.pressed(Command::Panic, &input) {
            self.action(Action::Panic);
        }
        if self.shortcuts.pressed(Command::PreviousTrack, &input) {
            self.track_sel = Some((selected + 4) % 5);
        }
        if self.shortcuts.pressed(Command::NextTrack, &input) {
            self.track_sel = Some((selected + 1) % 5);
        }
        if self.shortcuts.pressed(Command::Metronome, &input) {
            self.action(Action::Metronome(!self.view.metronome));
        }
        if self.shortcuts.pressed(Command::InputThru, &input) {
            self.config.input_thru = !self.config.input_thru;
        }
        for index in 0..8 {
            let slot = index % 4;
            let track = (index >= 4).then_some(self.track_sel.unwrap_or(0));
            let bank = if track.is_some() {
                self.config.track_fx.sel_bank_idx
            } else {
                self.config.input_fx.sel_bank_idx
            };
            if self.shortcuts.pressed(Command::EditFx(index), &input) {
                self.editor.select(if track.is_some() {
                    FxTarget::Track { bank, slot }
                } else {
                    FxTarget::Input { bank, slot }
                });
                self.focus_panel(ctx, Focus::Right);
            }
            if self.shortcuts.pressed(Command::Bank(index), &input) {
                if track.is_some() {
                    self.config.track_fx.select_bank(slot);
                } else {
                    self.config.input_fx.select_bank(slot);
                }
            }
            if self.shortcuts.held(Command::HoldFx(index), &input) && self.held_fx[index].is_none()
            {
                let enabled = if let Some(track) = track {
                    &mut self.config.track_fx.tracks[track].enabled[bank][slot]
                } else {
                    &mut self.config.input_fx.banks[bank].slots[slot].is_enabled
                };
                let previous = *enabled;
                *enabled = true;
                self.held_fx[index] = Some(HeldFx {
                    bank,
                    slot,
                    track,
                    previous,
                });
            }
            if self.shortcuts.pressed(Command::Fx(index), &input) {
                if let Some(track) = track {
                    self.config.track_fx.toggle_slot_enabled(track, slot);
                } else {
                    self.config.input_fx.toggle_slot_enabled(slot);
                }
            }
        }
        for index in 0..5 {
            let fader_direction = i8::from(self.shortcuts.held(Command::FaderUp(index), &input))
                - i8::from(self.shortcuts.held(Command::FaderDown(index), &input));
            let speed_direction = i8::from(self.shortcuts.held(Command::Faster(index), &input))
                - i8::from(self.shortcuts.held(Command::Slower(index), &input));
            let delta = self.speed_keys[index].delta(speed_direction, input.stable_dt, 20.0, 1.0);
            self.config.track_options[index].fader_speed =
                (self.config.track_options[index].fader_speed + delta).clamp(1.0, 60.0);
            self.fader_keys[index].advance(
                &mut self.config.track_levels[index],
                fader_direction,
                input.stable_dt,
                self.config.track_options[index].fader_speed,
            );
        }
    }
}

fn consume_performance_keys(ctx: &egui::Context, bindings: &shortcuts::Bindings) {
    ctx.input_mut(|input| {
        let bound = |event: &egui::Event| {
            let egui::Event::Key {
                key,
                physical_key,
                modifiers,
                ..
            } = event
            else {
                return false;
            };
            let key = physical_key.unwrap_or(*key);
            bindings.entries().iter().any(|definition| {
                bindings.chords(definition).iter().any(|chord| {
                    Key::from_name(&chord.key) == Some(key)
                        && chord.ctrl == modifiers.ctrl
                        && chord.alt == modifiers.alt
                        && chord.shift == modifiers.shift
                        && !modifiers.mac_cmd
                })
            })
        };
        let consumed_text = input
            .events
            .iter()
            .any(|event| matches!(event, egui::Event::Key { pressed: true, .. }) && bound(event));
        input.events.retain(|event| {
            !bound(event) && !(consumed_text && matches!(event, egui::Event::Text(_)))
        });
    });
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn held_activation_key_does_not_reclick_native_buttons() {
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.button("Toggle").request_focus();
            });
        });
        for (pressed, repeat, expected) in [
            (true, false, true),
            (true, true, false),
            (false, false, false),
            (true, false, true),
        ] {
            let mut clicked = false;
            let _ = ctx.run(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key: Key::Enter,
                        physical_key: Some(Key::Enter),
                        pressed,
                        repeat,
                        modifiers: egui::Modifiers::NONE,
                    }],
                    ..Default::default()
                },
                |ctx| {
                    ctx.input_mut(remove_button_repeat);
                    egui::CentralPanel::default().show(ctx, |ui| {
                        clicked = ui.button("Toggle").clicked();
                    });
                },
            );
            assert_eq!(clicked, expected);
        }
    }
}
