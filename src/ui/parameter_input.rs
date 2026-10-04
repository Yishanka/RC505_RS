//! Parameter gestures share precise steps regardless of slider width or range.
use super::navigation;
use crate::app_support::language::Language;
use eframe::egui::{self, Key};

#[derive(Clone, Default)]
struct State {
    slider: Option<egui::Id>,
    edit: Option<String>,
    before_edit: f64,
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
    matches!(
        key,
        Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown
    )
}
fn strip_arrows(ctx: &egui::Context) {
    ctx.input_mut(|input| {
        input
            .events
            .retain(|event| !matches!(event, egui::Event::Key {key, ..} if arrow(*key)))
    });
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
    step: f64,
    label: &str,
    log: bool,
) -> egui::Response {
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
    let mut request_edit = state.focus_edit;
    state.focus_edit = false;
    if focused && state.edit.is_none() && input.focused {
        let direction = if !input.modifiers.ctrl && !input.modifiers.alt && !input.modifiers.mac_cmd
        {
            i32::from(input.key_down(Key::ArrowRight)) - i32::from(input.key_down(Key::ArrowLeft))
                + 10 * (i32::from(input.key_down(Key::ArrowUp))
                    - i32::from(input.key_down(Key::ArrowDown)))
        } else {
            0
        };
        let delta = state.held.steps(direction, input.time);
        if delta != 0 {
            *value = ((*value / step).round() * step + f64::from(delta) * step).clamp(min, max);
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
    let decimals = (-step.log10() - 1e-6).ceil().max(0.0).min(6.0) as usize;
    if request_edit && state.edit.is_none() {
        state.before_edit = *value;
        state.edit = Some(format!("{value:.decimals$}"));
    }
    let result = ui
        .horizontal(|ui| {
            let slider = ui.add(
                egui::Slider::new(value, min..=max)
                    .show_value(false)
                    .step_by(step)
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
                    if cancel {
                        *value = state.before_edit;
                    } else if let Ok(parsed) = buffer.trim().parse::<f64>() {
                        if parsed.is_finite() {
                            *value = parsed.clamp(min, max);
                        }
                    }
                    state.edit = None;
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
    let mut response = result.on_hover_text(lang.choose(
        "← → fine · ↑ ↓ coarse · Enter type",
        "← → 微调 · ↑ ↓ 快调 · Enter 输入",
    ));
    if *value != before {
        response.mark_changed();
    }
    #[cfg(debug_assertions)]
    ui.ctx()
        .data_mut(|data| data.insert_temp(egui::Id::new(("parameter", label)), response.id));
    response
}

/// Closed enum selectors accept all four directions; an open menu owns its keys.
pub fn enum_step(response: &egui::Response) -> i32 {
    navigation::parameter(response.clone());
    let id = response.id.with("enum-repeat");
    if !response.has_focus() || response.ctx.memory(|m| m.any_popup_open()) {
        response.ctx.data_mut(|data| data.remove::<Held>(id));
        return 0;
    }
    let input = response.ctx.input(Clone::clone);
    let direction = if !input.modifiers.ctrl && !input.modifiers.alt && !input.modifiers.mac_cmd {
        (i32::from(input.key_down(Key::ArrowRight) || input.key_down(Key::ArrowDown))
            - i32::from(input.key_down(Key::ArrowLeft) || input.key_down(Key::ArrowUp)))
            as i32
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
