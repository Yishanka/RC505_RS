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
        ui::navigation::restore_input(ctx);
        let mut input = ctx.input(Clone::clone);
        if !self.master_fx_open {
            if let Some(scope) = ui::navigation::focused_scope(ctx) {
                self.focus = scope;
            }
        }
        let text = ctx.wants_keyboard_input();
        let pending_text = ui::parameters::take_pending_text(ctx);
        let typing =
            pending_text || ui::navigation::text_focused(ctx) || ui::navigation::clicking_text(ctx);
        if !typing {
            // egui treats repeated Enter/Space as another widget click. Keep
            // first activation and held state, without repeatedly toggling.
            remove_button_repeat(&mut input);
            ctx.input_mut(remove_button_repeat);
        }
        // Physical performance polling intentionally removes auto-repeat from
        // buttons. Panel traversal keeps the original navigation key events.
        let navigation_input = input.clone();
        let global_keys = !typing && !self.player_open && !self.replay_browser;
        let editing_controls =
            self.editor.expanded || self.master_fx_open || self.focus != Focus::Performance;
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
            && !popup_open
            && !self.calibration_open
            && !self.help_open
            && !self.player_open
            && !self.replay_browser
            && self.draft.is_none()
            && self.app_state == AppState::MainLoop
            && !self.show_save_prompt
            && !self.performance_locked();
        if performance {
            self.performance_keys.poll(&mut input);
        } else {
            self.performance_keys.suspend();
        }
        let performance_input = scoped_performance_input(&input, &self.shortcuts, editing_controls);
        for index in 0..8 {
            if !performance
                || !self
                    .shortcuts
                    .held(Command::HoldFx(index), &performance_input)
            {
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
            if !typing && pressed(&input, Key::Escape) {
                self.master_fx_open = false;
            } else if !typing {
                if !input.modifiers.ctrl && !input.modifiers.alt && !popup_open {
                    let next = navigation_input.key_pressed(Key::Tab) && !input.modifiers.shift
                        || navigation_input.key_pressed(Key::ArrowDown);
                    let previous = navigation_input.key_pressed(Key::Tab) && input.modifiers.shift
                        || navigation_input.key_pressed(Key::ArrowUp);
                    if next || previous {
                        consume_navigation_keys(ctx);
                        ui::navigation::advance(ctx, Focus::Editor, if next { 1 } else { -1 });
                    }
                }
                if performance {
                    self.handle_performance_commands(ctx, &performance_input, true);
                }
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
        // Numeric/text entry owns Escape, digits and editing shortcuts until it
        // commits or loses focus; the sound engine continues independently.
        if typing {
            if pressed(&input, Key::Escape)
                && !ctx.memory(|m| m.focused()).is_some_and(|id| {
                    ctx.data(|d| {
                        d.get_temp::<bool>(id.with("numeric-entry"))
                            .unwrap_or(false)
                    })
                })
            {
                ctx.memory_mut(|m| m.stop_text_input());
            }
            if !input.modifiers.ctrl && !input.modifiers.alt && input.key_pressed(Key::Tab) {
                consume_navigation_keys(ctx);
                ui::navigation::advance(
                    ctx,
                    self.focus,
                    if input.modifiers.shift { -1 } else { 1 },
                );
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
            let canvas = ui::navigation::canvas_focused(ctx);
            let next = no_command
                && ((!canvas && navigation_input.key_pressed(Key::ArrowDown))
                    || navigation_input.key_pressed(Key::Tab) && !input.modifiers.shift);
            let previous = no_command
                && ((!canvas && navigation_input.key_pressed(Key::ArrowUp))
                    || navigation_input.key_pressed(Key::Tab) && input.modifiers.shift);
            if next || previous {
                consume_navigation_keys(ctx);
                ui::navigation::advance(ctx, self.focus, if next { 1 } else { -1 });
            }
        }
        if !performance {
            return;
        }
        self.handle_performance_commands(ctx, &performance_input, editing_controls);
    }

    fn handle_performance_commands(
        &mut self,
        ctx: &egui::Context,
        input: &egui::InputState,
        editing_controls: bool,
    ) {
        consume_performance_keys(ctx, &self.shortcuts, editing_controls);
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
        if !editing_controls && !changing_track && !input.pointer.any_pressed() {
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
                self.focus_panel(
                    ctx,
                    if self.editor.expanded {
                        Focus::Editor
                    } else {
                        Focus::Right
                    },
                );
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

fn available_during_edit(command: Command, key: Key, modifiers: egui::Modifiers) -> bool {
    !matches!(
        command,
        Command::Clear
            | Command::Select(_)
            | Command::PreviousTrack
            | Command::NextTrack
            | Command::UndoSelected
            | Command::RedoSelected
    ) && !matches!(
        key,
        Key::ArrowLeft
            | Key::ArrowRight
            | Key::ArrowUp
            | Key::ArrowDown
            | Key::Delete
            | Key::Tab
            | Key::Enter
            | Key::Escape
    ) && !(modifiers.ctrl
        && matches!(
            key,
            Key::A | Key::C | Key::V | Key::X | Key::D | Key::Z | Key::Y
        ))
}
fn bound_for_performance(
    bindings: &shortcuts::Bindings,
    editing: bool,
    key: Key,
    modifiers: egui::Modifiers,
) -> bool {
    bindings.entries().iter().any(|definition| {
        (!editing || available_during_edit(definition.command, key, modifiers))
            && bindings.chords(definition).iter().any(|chord| {
                Key::from_name(&chord.key) == Some(key)
                    && chord.ctrl == modifiers.ctrl
                    && chord.alt == modifiers.alt
                    && chord.shift == modifiers.shift
                    && !modifiers.mac_cmd
            })
    })
}
fn scoped_performance_input(
    input: &egui::InputState,
    bindings: &shortcuts::Bindings,
    editing: bool,
) -> egui::InputState {
    let mut result = input.clone();
    result.events.retain(|event| match event {
        egui::Event::Key {
            key,
            physical_key,
            modifiers,
            ..
        } => bound_for_performance(bindings, editing, physical_key.unwrap_or(*key), *modifiers),
        _ => false,
    });
    result
        .keys_down
        .retain(|key| bound_for_performance(bindings, editing, *key, input.modifiers));
    result
}
fn consume_performance_keys(ctx: &egui::Context, bindings: &shortcuts::Bindings, editing: bool) {
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
            bound_for_performance(bindings, editing, key, *modifiers)
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
