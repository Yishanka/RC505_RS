//! Typing/mouse precision is independent of the performance key step.
use super::navigation;
use crate::app_support::language::Language;
use eframe::egui::{self, Key};

#[derive(Clone, Copy)]
pub struct Step {
    mouse: f64,
    keyboard: f64,
}
impl Step {
    pub fn new(mouse: f64, keyboard: f64) -> Self {
        Self { mouse, keyboard }
    }
}
impl From<f64> for Step {
    fn from(mouse: f64) -> Self {
        Self::new(mouse, 1.0)
    }
}

#[derive(Clone, Default)]
struct State {
    slider: Option<egui::Id>,
    edit: Option<String>,
    before_edit: f64,
    edited_text: bool,
    focus_edit: bool,
    held: Held,
}
#[derive(Clone, Default)]
struct Held {
    direction: i32,
    start: f64,
    emitted: u64,
}
impl Held {
    fn steps(&mut self, direction: i32, now: f64) -> i32 {
        if direction == 0 {
            *self = Self::default();
            return 0;
        }
        if direction != self.direction {
            *self = Self {
                direction,
                start: now,
                emitted: 0,
            };
            return direction;
        }
        let elapsed = (now - self.start - 0.3).max(0.0);
        let count = (elapsed.min(1.2) * 25.0 + (elapsed - 1.2).max(0.0) * 75.0).floor() as u64;
        let steps = count.saturating_sub(self.emitted).min(12) as i32;
        self.emitted = count;
        direction * steps
    }
}
fn arrow(key: Key) -> bool {
    matches!(key, Key::ArrowLeft | Key::ArrowRight)
}
fn strip_arrows(ctx: &egui::Context) {
    ctx.input_mut(|input| {
        input
            .events
            .retain(|event| !matches!(event, egui::Event::Key {key, ..} if arrow(*key)))
    });
}
fn decimal_places(step: f64) -> usize {
    for n in 0..=6 {
        let scaled = step * 10.0_f64.powi(n);
        if (scaled - scaled.round()).abs() < 1e-5 {
            return n as usize;
        }
    }
    6
}
pub fn take_pending_text(ctx: &egui::Context) -> bool {
    ctx.data_mut(|data| {
        let id = egui::Id::new("parameter-text-pending");
        let pending = data.get_temp::<bool>(id).unwrap_or(false);
        data.remove::<bool>(id);
        pending
    })
}
pub fn slider(
    ui: &mut egui::Ui,
    value: &mut f64,
    min: f64,
    max: f64,
    step: impl Into<Step>,
    label: &str,
    log: bool,
) -> egui::Response {
    let step = step.into();
    let lang = Language::current(ui.ctx());
    let id = ui.next_auto_id().with("precise-parameter");
    let text_id = id.with("text");
    let mut state = ui
        .ctx()
        .data_mut(|data| data.get_temp::<State>(id).unwrap_or_default());
    let focused = state
        .slider
        .is_some_and(|slider| ui.memory(|m| m.has_focus(slider)));
    let before = *value;
    let input = ui.input(Clone::clone);
    let popup_open = ui.memory(|m| m.any_popup_open());
    let mut request_edit = state.focus_edit;
    state.focus_edit = false;
    if focused && state.edit.is_none() && input.focused && ui.is_enabled() && !popup_open {
        let direction = if !input.modifiers.ctrl && !input.modifiers.alt && !input.modifiers.mac_cmd
        {
            i32::from(input.key_down(Key::ArrowRight)) - i32::from(input.key_down(Key::ArrowLeft))
        } else {
            0
        };
        let delta = state.held.steps(direction, input.time);
        if delta != 0 {
            // Preserve typed fractions, e.g. 7.53 ms -> 8.53 ms. Display
            // precision never determines the size of a performance gesture.
            *value = (*value + f64::from(delta) * step.keyboard).clamp(min, max);
        }
        strip_arrows(ui.ctx());
        if input.key_pressed(Key::Enter) {
            ui.input_mut(|input| {
                input.consume_key(egui::Modifiers::NONE, Key::Enter);
            });
            request_edit = true;
        }
    } else {
        state.held = Held::default();
    }
    let decimals = decimal_places(step.mouse);
    if request_edit && state.edit.is_none() {
        state.before_edit = *value;
        state.edited_text = false;
        state.edit = Some(format!("{value:.decimals$}"));
    }
    let result = ui
        .horizontal(|ui| {
            let slider = ui.add_enabled(
                !popup_open && input.focused,
                egui::Slider::new(value, min..=max)
                    .show_value(false)
                    .step_by(step.mouse)
                    .logarithmic(log),
            );
            state.slider = Some(slider.id);
            if slider.clicked() || slider.drag_started() {
                slider.request_focus();
            }
            let slider = navigation::parameter(slider);
            if let Some(buffer) = &mut state.edit {
                let text = ui.add_sized(
                    [96.0, ui.spacing().interact_size.y],
                    egui::TextEdit::singleline(buffer)
                        .id(text_id)
                        .desired_width(96.0)
                        .font(egui::TextStyle::Monospace),
                );
                let text = navigation::register(text);
                state.edited_text |= text.changed();
                ui.ctx()
                    .data_mut(|data| data.insert_temp(text_id.with("numeric-entry"), true));
                if request_edit {
                    text.request_focus();
                    if let Some(mut saved) = egui::TextEdit::load_state(ui.ctx(), text_id) {
                        saved
                            .cursor
                            .set_char_range(Some(egui::text::CCursorRange::two(
                                egui::text::CCursor::new(0),
                                egui::text::CCursor::new(buffer.chars().count()),
                            )));
                        saved.store(ui.ctx(), text_id);
                    }
                }
                let owns_text = text.has_focus() || text.lost_focus() || request_edit;
                let cancel = owns_text && input.key_pressed(Key::Escape);
                let submit = owns_text && !request_edit && input.key_pressed(Key::Enter);
                if cancel || submit || (text.lost_focus() && !request_edit) {
                    if cancel || !state.edited_text {
                        *value = state.before_edit;
                    } else if let Ok(parsed) = buffer.trim().parse::<f64>() {
                        if parsed.is_finite() {
                            *value = parsed.clamp(min, max);
                        }
                    }
                    state.edit = None;
                    ui.ctx()
                        .data_mut(|data| data.remove::<bool>(text_id.with("numeric-entry")));
                    if cancel || submit {
                        slider.request_focus();
                    }
                }
            } else {
                let number = ui.add_sized(
                    [96.0, ui.spacing().interact_size.y],
                    egui::Button::new(
                        egui::RichText::new(format!("{value:.decimals$}")).monospace(),
                    ),
                );
                if number.clicked() {
                    state.before_edit = *value;
                    state.edited_text = false;
                    state.edit = Some(format!("{value:.decimals$}"));
                    state.focus_edit = true;
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(egui::Id::new("parameter-text-pending"), true)
                    });
                }
            }
            ui.add(egui::Label::new(label).truncate(true))
                .on_hover_text(label);
            slider
        })
        .inner;
    ui.ctx().data_mut(|data| data.insert_temp(id, state));
    let mut response = result.on_hover_text(format!(
        "{} {} · {}",
        lang.choose("↑↓ select · ←→ step", "↑↓ 选参数 · ←→ 步长"),
        step.keyboard,
        lang.choose("Enter type value", "Enter 输入数值")
    ));
    if *value != before {
        response.mark_changed();
    }
    #[cfg(debug_assertions)]
    ui.ctx()
        .data_mut(|data| data.insert_temp(egui::Id::new(("parameter", label)), response.id));
    response
}

/// Closed enum selectors accept only horizontal keys. Vertical keys navigate.
pub fn enum_step(response: &egui::Response) -> i32 {
    navigation::parameter(response.clone());
    let id = response.id.with("enum-repeat");
    if !response.has_focus()
        || !response.enabled()
        || !response.ctx.input(|i| i.focused)
        || response.ctx.memory(|m| m.any_popup_open())
    {
        response.ctx.data_mut(|data| data.remove::<Held>(id));
        return 0;
    }
    let input = response.ctx.input(Clone::clone);
    let direction = if !input.modifiers.ctrl && !input.modifiers.alt && !input.modifiers.mac_cmd {
        i32::from(input.key_down(Key::ArrowRight)) - i32::from(input.key_down(Key::ArrowLeft))
    } else {
        0
    };
    let delta = response.ctx.data_mut(|data| {
        let mut held = data.get_temp::<Held>(id).unwrap_or_default();
        let delta = held.steps(direction, input.time);
        data.insert_temp(id, held);
        delta
    });
    strip_arrows(&response.ctx);
    delta
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opening_and_accepting_a_fractional_beat_keeps_its_exact_value() {
        let ctx = egui::Context::default();
        let mut value = 1.0 / 960.0;
        let mut slider_id = None;
        let draw = |raw: egui::RawInput, value: &mut f64, slider_id: &mut Option<egui::Id>| {
            let _ = ctx.run(raw, |ctx| {
                egui::CentralPanel::default()
                    .show(ctx, |ui| {
                        *slider_id = Some(
                            slider(ui, value, 0.0, 4.0, 1.0 / 960.0, "Start (beats)", false).id,
                        );
                    })
                    .inner
            });
        };
        draw(egui::RawInput::default(), &mut value, &mut slider_id);
        ctx.memory_mut(|m| m.request_focus(slider_id.unwrap()));
        for pressed in [true, false, true, false] {
            draw(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key: Key::Enter,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                    ..Default::default()
                },
                &mut value,
                &mut slider_id,
            );
        }
        assert_eq!(
            value,
            1.0 / 960.0,
            "An untouched numeric entry must not round away a PPQ tick"
        );
        assert_eq!(decimal_places(0.0625), 4);
    }
    #[test]
    fn blocked_parameter_widgets_do_not_change_behind_dialogs_or_after_focus_loss() {
        for (enabled, popup, focused) in [
            (false, false, true),
            (true, true, true),
            (true, false, false),
        ] {
            let ctx = egui::Context::default();
            let mut value = 4.0;
            let mut id = None;
            let draw =
                |ctx: &egui::Context, value: &mut f64, id: &mut Option<egui::Id>, enabled| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.set_enabled(enabled);
                        navigation::begin(ui, crate::app::Focus::Right, false);
                        *id = Some(
                            slider(ui, value, 0.0, 100.0, Step::new(0.01, 1.0), "Time", false).id,
                        );
                        navigation::end(ui);
                    });
                };
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                draw(ctx, &mut value, &mut id, true)
            });
            ctx.memory_mut(|m| m.request_focus(id.unwrap()));
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                draw(ctx, &mut value, &mut id, true)
            });
            if popup {
                ctx.memory_mut(|m| m.open_popup(egui::Id::new("test-popup")));
            }
            let mut raw = egui::RawInput {
                focused,
                events: vec![egui::Event::Key {
                    key: Key::ArrowRight,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            };
            navigation::prepare_input(&ctx, &mut raw);
            let _ = ctx.run(raw, |ctx| {
                navigation::restore_input(ctx);
                draw(ctx, &mut value, &mut id, enabled);
            });
            assert_eq!(
                value, 4.0,
                "enabled={enabled}, popup={popup}, focused={focused}"
            );
        }
    }
    #[test]
    fn held_parameter_adjustment_uses_elapsed_time_not_os_repeat_or_fps() {
        let measure = |fps: u32| {
            let mut held = Held::default();
            (0..=fps * 3)
                .map(|frame| held.steps(1, f64::from(frame) / f64::from(fps)))
                .sum::<i32>()
        };
        assert_eq!(measure(30), measure(60));
        assert_eq!(measure(60), measure(120));
        let mut held = Held::default();
        assert_eq!(held.steps(1, 0.0), 1);
        assert_eq!(held.steps(1, 0.2), 0);
        assert_eq!(held.steps(0, 0.3), 0);
        assert_eq!(held.steps(-10, 0.4), -10);
    }
}
