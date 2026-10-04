//! Resolve navigation only against controls drawn in the current frame.
use crate::app::Focus;
use eframe::egui::{self, Id, Response, Ui};
#[derive(Clone)]
struct Entry {
    id: Id,
    rect: egui::Rect,
}
#[derive(Clone)]
struct Group {
    focus: Focus,
    previous: Vec<Entry>,
    current: Vec<Entry>,
    first: bool,
    step: Option<isize>,
    focused: Option<Id>,
}
fn key(group: Focus) -> Id {
    Id::new(match group {
        Focus::Transport => "nav-top",
        Focus::Left => "nav-left",
        Focus::Right => "nav-right",
        Focus::Editor => "nav-editor",
        Focus::Performance => "nav-performance",
    })
}
pub fn begin(ui: &Ui, focus: Focus, first: bool) {
    let focused = ui.memory(|m| m.focused());
    ui.ctx().data_mut(|d| {
        let previous = d.get_temp::<Vec<Entry>>(key(focus)).unwrap_or_default();
        let step = d.get_temp::<isize>(key(focus).with("step"));
        d.remove::<isize>(key(focus).with("step"));
        d.insert_temp(
            Id::new("nav-group"),
            Group {
                focus,
                previous,
                current: vec![],
                first,
                step,
                focused,
            },
        );
    });
}
pub fn register(response: Response) -> Response {
    if response.enabled() && response.sense.focusable {
        response.ctx.data_mut(|d| {
            if let Some(mut group) = d.get_temp::<Group>(Id::new("nav-group")) {
                if !group.current.iter().any(|item| item.id == response.id) {
                    group.current.push(Entry {
                        id: response.id,
                        rect: response.rect,
                    });
                }
                d.insert_temp(Id::new("nav-group"), group);
            }
        });
    }
    response
}
pub fn parameter(response: Response) -> Response {
    response
        .ctx
        .data_mut(|d| d.insert_temp(response.id.with("parameter"), true));
    register(response)
}
pub fn parameter_focused(ctx: &egui::Context) -> bool {
    ctx.memory(|m| m.focused())
        .is_some_and(|id| ctx.data(|d| d.get_temp::<bool>(id.with("parameter")).unwrap_or(false)))
}
pub fn focused_scope(ctx: &egui::Context) -> Option<Focus> {
    let focused = ctx.memory(|m| m.focused())?;
    [Focus::Transport, Focus::Left, Focus::Right, Focus::Editor]
        .into_iter()
        .find(|scope| {
            ctx.data(|d| {
                d.get_temp::<Vec<Entry>>(key(*scope))
                    .is_some_and(|entries| entries.iter().any(|entry| entry.id == focused))
            })
        })
}
pub fn end(ui: &Ui) {
    let group = ui.ctx().data_mut(|d| {
        let group = d.get_temp::<Group>(Id::new("nav-group"));
        d.remove::<Group>(Id::new("nav-group"));
        group
    });
    let Some(group) = group else {
        return;
    };
    let mut target = None;
    if group.first {
        target = group.current.first();
    } else if let Some(step) = group.step {
        if !group.current.is_empty() {
            let index = group
                .current
                .iter()
                .position(|v| Some(v.id) == group.focused);
            let next = match index {
                Some(i) => (i as isize + step).rem_euclid(group.current.len() as isize) as usize,
                None if step < 0 => group.current.len() - 1,
                None => 0,
            };
            target = group.current.get(next);
        }
    }
    if let Some(target) = target {
        ui.memory_mut(|m| m.request_focus(target.id));
        ui.scroll_to_rect(target.rect, Some(egui::Align::Center));
    } else if let Some(id) = group.focused {
        if group.previous.iter().any(|item| item.id == id)
            && !group.current.iter().any(|item| item.id == id)
        {
            ui.memory_mut(|m| m.surrender_focus(id));
        }
    }
    // Prevent egui's default traversal from also moving focus on these keys.
    // Text fields still receive the events; the app leaves their arrow keys alone.
    if let Some(id) = ui.memory(|m| m.focused()) {
        if group.current.iter().any(|item| item.id == id) {
            ui.memory_mut(|m| {
                let popup = m.any_popup_open();
                m.set_focus_lock_filter(
                    id,
                    if popup {
                        egui::EventFilter::default()
                    } else {
                        egui::EventFilter {
                            tab: true,
                            vertical_arrows: true,
                            horizontal_arrows: true,
                            escape: true,
                            ..Default::default()
                        }
                    },
                )
            });
        }
    }
    ui.ctx()
        .data_mut(|d| d.insert_temp(key(group.focus), group.current));
}
pub fn advance(ctx: &egui::Context, group: Focus, direction: isize) {
    ctx.data_mut(|d| d.insert_temp(key(group).with("step"), direction));
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
        let draw = |ctx: &egui::Context, request: bool, value: &mut f32, ids: &mut Vec<Id>| {
            egui::CentralPanel::default().show(ctx, |ui| {
                begin(ui, Focus::Left, request);
                ids.clear();
                ids.push(register(ui.add(egui::Slider::new(value, 0.0..=10.0).step_by(1.0))).id);
                ids.push(button(ui, "Second").id);
                end(ui);
                let _ = ui.button("Other panel");
            });
        };
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            draw(ctx, true, &mut value, &mut ids)
        });
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
            |ctx| draw(ctx, false, &mut value, &mut ids),
        );
        assert!(value > 5.0);
        advance(&ctx, Focus::Left, 1);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            draw(ctx, false, &mut value, &mut ids)
        });
        assert_eq!(ctx.memory(|m| m.focused()), Some(ids[1]));
        advance(&ctx, Focus::Left, 1);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            draw(ctx, false, &mut value, &mut ids)
        });
        assert_eq!(ctx.memory(|m| m.focused()), Some(ids[0]));
    }
}
