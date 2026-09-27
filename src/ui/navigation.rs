//! Explicit panel focus and deterministic Tab/Up/Down traversal.
use crate::app::Focus;
use eframe::egui::{self, Id, Response, Ui};
fn key(group: Focus) -> Id {
    Id::new(match group {
        Focus::Transport => "nav-top",
        Focus::Left => "nav-left",
        Focus::Right => "nav-right",
        Focus::Performance => "nav-performance",
    })
}
pub fn begin(ui: &Ui, group: Focus, request: bool) {
    ui.ctx().data_mut(|d| {
        d.insert_temp(Id::new("nav-group"), group);
        d.insert_temp(key(group), Vec::<Id>::new());
        d.insert_temp(Id::new("nav-request"), request);
    });
}
pub fn end(ui: &Ui) {
    ui.ctx()
        .data_mut(|d| d.remove::<Focus>(Id::new("nav-group")));
}
pub fn register(response: Response) -> Response {
    if !response.enabled() {
        return response;
    }
    let request = response.ctx.data_mut(|d| {
        let Some(group) = d.get_temp::<Focus>(Id::new("nav-group")) else {
            return false;
        };
        let mut ids = d.get_temp::<Vec<Id>>(key(group)).unwrap_or_default();
        ids.push(response.id);
        d.insert_temp(key(group), ids);
        let request = d.get_temp::<bool>(Id::new("nav-request")).unwrap_or(false);
        d.insert_temp(Id::new("nav-request"), false);
        request
    });
    if request {
        response.request_focus();
    }
    response
}
pub fn advance(ctx: &egui::Context, group: Focus, direction: isize) {
    let ids = ctx.data(|d| d.get_temp::<Vec<Id>>(key(group)).unwrap_or_default());
    if ids.is_empty() {
        return;
    }
    let focused = ctx.memory(|m| m.focused());
    let index = ids.iter().position(|id| Some(*id) == focused).unwrap_or(0);
    let next = (index as isize + direction).rem_euclid(ids.len() as isize) as usize;
    ctx.memory_mut(|m| m.request_focus(ids[next]));
}
pub fn button(ui: &mut Ui, label: impl Into<egui::WidgetText>) -> Response {
    register(ui.button(label))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn panel_focus_cycles_only_registered_controls_and_slider_accepts_keys() {
        let ctx = egui::Context::default();
        let mut value = 5.0f32;
        let mut ids = Vec::new();
        let mut draw = |ctx: &egui::Context, request: bool| {
            egui::CentralPanel::default().show(ctx, |ui| {
                begin(ui, Focus::Left, request);
                ids.clear();
                ids.push(
                    register(ui.add(egui::Slider::new(&mut value, 0.0..=10.0).step_by(1.0))).id,
                );
                ids.push(button(ui, "Second").id);
                end(ui);
                ui.button("Other panel");
            });
        };
        let _ = ctx.run(egui::RawInput::default(), |ctx| draw(ctx, true));
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::ArrowRight,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ctx| draw(ctx, false),
        );
        assert!(value > 5.0);
        advance(&ctx, Focus::Left, 1);
        assert_eq!(ctx.memory(|m| m.focused()), Some(ids[1]));
        advance(&ctx, Focus::Left, 1);
        assert_eq!(ctx.memory(|m| m.focused()), Some(ids[0]));
    }
}
