use super::theme;
use crate::{
    app::MyApp,
    config::InputFx,
    presets::{self, FxTarget},
};
use eframe::egui;
fn enabled(ui: &mut egui::Ui, yes: bool, widget: impl egui::Widget) -> egui::Response {
    super::navigation::register(ui.add_enabled(yes, widget))
}

pub fn next_loop(app: &MyApp, target: FxTarget) -> bool {
    let FxTarget::Input { bank, slot } = target else {
        return false;
    };
    app.editor.clip_next_loop
        && app.view.running
        && app.config.input_fx.sel_bank_idx == bank
        && app.config.input_fx.banks[bank].slots[slot].is_enabled
}
pub fn draw(ui: &mut egui::Ui, app: &mut MyApp, target: FxTarget) {
    let lang = app.language;
    let current = presets::clip(&app.config, target);
    let pending = presets::note_mut(&mut app.config, target).and_then(|n| n.pending.clone());
    theme::control_row(ui, |ui| {
        let name = current
            .as_ref()
            .map(|c| c.name.as_str())
            .filter(|n| !n.is_empty())
            .unwrap_or(lang.choose("Untitled phrase", "未命名乐句"));
        let FxTarget::Input { bank, slot } = target else {
            return;
        };
        ui.add_sized(
            [ui.available_width().min(270.0), 28.0],
            egui::Label::new(
                egui::RichText::new(format!(
                    "{} → {} {} / {}",
                    name,
                    lang.choose("Source", "声源"),
                    bank + 1,
                    ['A', 'B', 'C', 'D'][slot]
                ))
                .strong(),
            )
            .truncate(true),
        );
        super::navigation::register(ui.checkbox(&mut app.editor.clip_next_loop,lang.choose("Next loop","下轮切换"))).on_hover_text(lang.choose("Running sources switch at their next loop boundary. Stopped or bypassed sources update immediately.","运行中的声源在下一循环边界换乐句；新载入停止或旁路声源时立即更新。"));
        if super::navigation::register(ui.selectable_label(
            app.editor.phrase_manager_open,
            lang.choose("Sources / links…", "声源 / 链接…"),
        ))
        .clicked()
        {
            app.editor.phrase_manager_open = !app.editor.phrase_manager_open;
        }
        let linked = crate::phrases::members(&app.config, target);
        if linked > 1 {
            ui.label(format!(
                "{} {linked}",
                lang.choose("Linked sources:", "链接声源：")
            ));
        }
    });
    if let Some(pending) = pending {
        theme::control_row(ui, |ui| {
            ui.add(
                egui::Label::new(format!(
                    "{}: {}",
                    lang.choose("Queued phrase", "待切换乐句"),
                    pending.clip.name
                ))
                .truncate(true),
            );
            if super::navigation::button(ui, lang.choose("Switch now", "立即切换")).clicked() {
                if let Some(note) = presets::note_mut(&mut app.config, target) {
                    note.launch_clip(&pending.clip, false);
                }
                crate::phrases::propagate(&mut app.config, target);
            }
            if super::navigation::button(ui, lang.choose("Cancel queued change", "取消待切换"))
                .clicked()
            {
                crate::phrases::cancel_pending(&mut app.config, target);
            }
        });
    }
    if app.editor.phrase_manager_open {
        theme::card().show(ui, |ui| {
            let mut sources = Vec::new();
            for (bank, b) in app.config.input_fx.banks.iter().enumerate() {
                for (slot, s) in b.slots.iter().enumerate() {
                    let t = FxTarget::Input { bank, slot };
                    if let Some(clip) = crate::phrases::stored_clip(&app.config, t) {
                        let sounding = matches!(s.fx, Some(InputFx::Oscillator(_)));
                        sources.push((
                            t,
                            s.source_id.clone(),
                            format!(
                                "{} {} / {} · {} · {}",
                                lang.choose("Bank", "组"),
                                bank + 1,
                                ['A', 'B', 'C', 'D'][slot],
                                if clip.name.is_empty() {
                                    lang.choose("Untitled", "未命名")
                                } else {
                                    &clip.name
                                },
                                lang.choose(
                                    if sounding { "OSC" } else { "disconnected" },
                                    if sounding { "OSC" } else { "未连接" }
                                )
                            ),
                        ));
                    }
                }
            }
            let choices: Vec<_> = sources.iter().filter(|(t, _, _)| *t != target).collect();
            if !choices
                .iter()
                .any(|(_, id, _)| *id == app.editor.phrase_source)
            {
                app.editor.phrase_source = choices
                    .first()
                    .map_or(String::new(), |(_, id, _)| id.clone());
            }
            if choices.is_empty() {
                ui.label(lang.choose("No other source", "没有其他声源"));
            } else {
                let mut selected = choices
                    .iter()
                    .position(|(_, id, _)| *id == app.editor.phrase_source)
                    .unwrap_or(0);
                super::parameters::selector(
                    ui,
                    "phrase-link-source",
                    &mut selected,
                    &choices
                        .iter()
                        .enumerate()
                        .map(|(i, (_, _, name))| (i, name.as_str()))
                        .collect::<Vec<_>>(),
                );
                app.editor.phrase_source = choices[selected].1.clone();
            }
            let source = choices
                .iter()
                .find(|(_, id, _)| *id == app.editor.phrase_source)
                .map(|(t, _, _)| *t);
            theme::control_row(ui, |ui| {
                if crate::phrases::members(&app.config, target) > 1
                    && super::navigation::button(
                        ui,
                        lang.choose("Unlink this source", "解除当前链接"),
                    )
                    .clicked()
                {
                    crate::phrases::unlink(&mut app.config, target);
                }
                if enabled(
                    ui,
                    source.is_some(),
                    egui::Button::new(lang.choose("Copy its phrase here", "复制它的乐句")),
                )
                .clicked()
                {
                    let next = next_loop(app, target);
                    let before = presets::note_mut(&mut app.config, target)
                        .map(|n| super::piano_roll::Snapshot::capture(n));
                    match crate::phrases::copy_from(&mut app.config, source.unwrap(), target, next)
                    {
                        Ok(()) => {
                            if let (Some(before), Some(note)) =
                                (before, presets::note_mut(&mut app.config, target))
                            {
                                app.editor.piano.remember_state(before, note);
                            }
                        }
                        Err(e) => app.editor.message = e.to_string(),
                    }
                }
                if enabled(
                    ui,
                    source.is_some(),
                    egui::Button::new(lang.choose("Link shared edits", "链接并共享编辑")),
                )
                .clicked()
                {
                    let next = next_loop(app, target);
                    match crate::phrases::link_timed(&mut app.config, source.unwrap(), target, next)
                    {
                        Ok(()) => app.editor.piano.reset_history(),
                        Err(e) => app.editor.message = e.to_string(),
                    }
                }
            });
            theme::caption(
                ui,
                lang.choose(
                    "Copies are independent. Links share notes; sounds stay independent.",
                    "复制互不影响，链接共享音符；音色独立。",
                ),
            );
            egui::ScrollArea::vertical()
                .id_source("phrase-source-list")
                .max_height(140.0)
                .show(ui, |ui| {
                    for (source, _, name) in &sources {
                        if super::navigation::register(ui.selectable_label(*source == target, name))
                            .clicked()
                        {
                            app.editor.select(*source);
                            app.editor.page = super::editor::EditorPage::Sequence;
                        }
                    }
                });
        });
    }
}
