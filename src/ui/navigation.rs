//! Resolve navigation only against controls drawn in the current frame.
use crate::app::Focus;
use eframe::egui::{self, Id, Response, Ui};
#[derive(Clone)]
struct Entry {
    id: Id,
    rect: egui::Rect,
    hit_rect: egui::Rect,
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
    response
        .ctx
        .data_mut(|d| d.insert_temp(response.id.with("text-entry"), false));
    if response.enabled() && response.sense.focusable {
        response.ctx.data_mut(|d| {
            if let Some(mut group) = d.get_temp::<Group>(Id::new("nav-group")) {
                if !group.current.iter().any(|item| item.id == response.id) {
                    group.current.push(Entry {
                        id: response.id,
                        rect: response.rect,
                        hit_rect: response.interact_rect,
                    });
                }
                d.insert_temp(Id::new("nav-group"), group);
            }
        });
    }
    response
}
/// Text roles are known before a newly focused field emits IME geometry.
pub fn text(response: Response) -> Response {
    let response = register(response);
    response
        .ctx
        .data_mut(|d| d.insert_temp(response.id.with("text-entry"), true));
    response
}
pub fn parameter(response: Response) -> Response {
    response
        .ctx
        .data_mut(|d| d.remove::<bool>(response.id.with("canvas-arrows")));
    register(response)
}
/// A canvas uses vertical arrows locally; ordinary parameter fields do not.
pub fn canvas(response: Response) -> Response {
    response
        .ctx
        .data_mut(|data| data.insert_temp(response.id.with("canvas-arrows"), true));
    register(response)
}
pub fn canvas_focused(ctx: &egui::Context) -> bool {
    ctx.memory(|m| m.focused()).is_some_and(|id| {
        ctx.data(|data| {
            data.get_temp::<bool>(id.with("canvas-arrows"))
                .unwrap_or(false)
        })
    })
}
/// Capture actual text editing, not the persistent state left by an old TextEdit.
pub fn remember_text_focus(ctx: &egui::Context) {
    let focused = ctx.memory(|m| m.focused()).filter(|id| {
        ctx.read_response(*id).is_some_and(|response| {
            ctx.output(|o| o.ime.is_some_and(|ime| response.rect.intersects(ime.rect)))
        })
    });
    ctx.data_mut(|d| d.insert_temp(Id::new("nav-text-focus"), focused));
}
pub fn text_focused(ctx: &egui::Context) -> bool {
    ctx.memory(|m| m.focused()).is_some_and(|id| {
        ctx.data(|d| {
            d.get_temp::<Option<Id>>(Id::new("nav-text-focus"))
                .flatten()
        }) == Some(id)
            || ctx.data(|d| {
                d.get_temp::<bool>(id.with("numeric-entry"))
                    .unwrap_or(false)
            })
            || ctx.data(|d| d.get_temp::<bool>(id.with("text-entry")).unwrap_or(false))
    })
}
pub fn clicking_text(ctx: &egui::Context) -> bool {
    let Some(pos) = ctx.input(|i| {
        i.pointer
            .any_pressed()
            .then(|| i.pointer.interact_pos())
            .flatten()
    }) else {
        return false;
    };
    [Focus::Transport, Focus::Left, Focus::Right, Focus::Editor]
        .into_iter()
        .any(|scope| {
            ctx.data(|d| {
                d.get_temp::<Vec<Entry>>(key(scope)).is_some_and(|entries| {
                    entries.iter().any(|e| {
                        e.hit_rect.contains(pos)
                            && d.get_temp::<bool>(e.id.with("text-entry")).unwrap_or(false)
                    })
                })
            })
        })
}
/// egui 0.27 resolves native directional traversal from RawInput before our
/// handler runs. Newly focused controls cannot acquire a focus-lock filter until
/// the next frame. Keep scoped navigation out of that native traversal, then
/// restore it for the app and its widgets (including held-key state).
pub fn prepare_input(ctx: &egui::Context, input: &mut egui::RawInput) {
    let navigation_key = |key: egui::Key| {
        matches!(
            key,
            egui::Key::Tab
                | egui::Key::ArrowUp
                | egui::Key::ArrowDown
                | egui::Key::ArrowLeft
                | egui::Key::ArrowRight
        )
    };
    let blocked_id = Id::new("nav-until-release");
    let mut blocked = ctx
        .data(|d| d.get_temp::<Vec<egui::Key>>(blocked_id))
        .unwrap_or_default();
    if !input.focused {
        ctx.input_mut(|old| {
            for key in old
                .keys_down
                .iter()
                .copied()
                .filter(|key| navigation_key(*key))
            {
                if !blocked.contains(&key) {
                    blocked.push(key);
                }
            }
            old.keys_down.retain(|key| !navigation_key(*key));
        });
    }
    input.events.retain(|event| {
        if let egui::Event::Key { key, pressed, .. } = event {
            if navigation_key(*key) && (!input.focused || blocked.contains(key)) {
                if !pressed {
                    blocked.retain(|held| held != key);
                } else if !blocked.contains(key) {
                    blocked.push(*key);
                }
                return false;
            }
        }
        true
    });
    ctx.data_mut(|d| d.insert_temp(blocked_id, blocked));
    if focused_scope(ctx).is_none() || ctx.memory(|m| m.any_popup_open()) {
        return;
    }
    let typing = text_focused(ctx);
    let mut routed = Vec::new();
    input.events.retain(|event| {
        let take = matches!(event, egui::Event::Key { key, .. }
            if *key == egui::Key::Tab || !typing && matches!(key,
                egui::Key::ArrowUp | egui::Key::ArrowDown | egui::Key::ArrowLeft | egui::Key::ArrowRight));
        if take { routed.push(event.clone()); }
        !take
    });
    ctx.data_mut(|d| d.insert_temp(Id::new("nav-routed-input"), routed));
}
pub fn restore_input(ctx: &egui::Context) {
    let events = ctx
        .data_mut(|d| d.remove_temp::<Vec<egui::Event>>(Id::new("nav-routed-input")))
        .unwrap_or_default();
    ctx.input_mut(|input| {
        for mut event in events {
            if let egui::Event::Key {
                key,
                pressed,
                repeat,
                ..
            } = &mut event
            {
                if *pressed && input.focused {
                    *repeat = !input.keys_down.insert(*key);
                } else {
                    input.keys_down.remove(key);
                }
            }
            input.events.push(event);
        }
    });
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
    fn routed_keys_keep_native_repeat_and_disarm_across_focus_loss() {
        let ctx = egui::Context::default();
        let button_id = Id::new("routing-test-button");
        let draw = |raw: egui::RawInput| {
            let mut raw = raw;
            prepare_input(&ctx, &mut raw);
            let mut repeats = Vec::new();
            let _ = ctx.run(raw, |ctx| {
                restore_input(ctx);
                repeats = ctx.input(|i| {
                    i.events
                        .iter()
                        .filter_map(|e| {
                            if let egui::Event::Key {
                                key: egui::Key::Tab,
                                pressed: true,
                                repeat,
                                ..
                            } = e
                            {
                                Some(*repeat)
                            } else {
                                None
                            }
                        })
                        .collect()
                });
                egui::CentralPanel::default().show(ctx, |ui| {
                    begin(ui, Focus::Right, false);
                    let r = ui.interact(
                        ui.available_rect_before_wrap(),
                        button_id,
                        egui::Sense::click(),
                    );
                    register(r);
                    end(ui);
                });
            });
            repeats
        };
        draw(egui::RawInput::default());
        ctx.memory_mut(|m| m.request_focus(button_id));
        let press = || egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: Some(egui::Key::Tab),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::CTRL,
        };
        assert_eq!(
            draw(egui::RawInput {
                events: vec![press()],
                ..Default::default()
            }),
            vec![false]
        );
        assert_eq!(
            draw(egui::RawInput {
                events: vec![press()],
                ..Default::default()
            }),
            vec![true]
        );
        draw(egui::RawInput {
            focused: false,
            ..Default::default()
        });
        assert!(!ctx.input(|i| i.key_down(egui::Key::Tab)));
        assert!(
            draw(egui::RawInput {
                events: vec![press()],
                ..Default::default()
            })
            .is_empty()
        );
        draw(egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Tab,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        });
        assert_eq!(
            draw(egui::RawInput {
                events: vec![press()],
                ..Default::default()
            }),
            vec![false]
        );
    }
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
