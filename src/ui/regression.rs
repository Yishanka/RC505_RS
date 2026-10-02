//! Process-isolated regression of production keyboard/UI routing, with accessibility.
use crate::{
    app::{Focus, MyApp},
    state::{AppState, ProjectNameMode},
};
use eframe::egui::{self, Key, Modifiers};
fn frame(ctx: &egui::Context, app: &mut MyApp, time: &mut f64, events: Vec<egui::Event>) {
    *time += 1.0 / 60.0;
    let modifiers = events
        .iter()
        .find_map(|e| {
            if let egui::Event::Key { modifiers, .. } = e {
                Some(*modifiers)
            } else {
                None
            }
        })
        .unwrap_or_default();
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1320.0, 900.0),
            )),
            time: Some(*time),
            modifiers,
            events,
            ..Default::default()
        },
        |ctx| app.render_frame(ctx),
    );
    assert!(
        !output.viewport_output.values().any(|v| v
            .commands
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::Close))),
        "Unexpected viewport close"
    );
    if let Some(tree) = output.platform_output.accesskit_update {
        // Windows accesskit_consumer enforces this invariant and panics on a dangling focus.
        assert!(
            tree.nodes.iter().any(|(id, _)| *id == tree.focus),
            "Focus absent from accessibility tree: {:?}, frame {}",
            tree.focus,
            time
        );
    }
}
fn press(ctx: &egui::Context, app: &mut MyApp, time: &mut f64, key: Key, modifiers: Modifiers) {
    for pressed in [true, false] {
        frame(
            ctx,
            app,
            time,
            vec![egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed,
                repeat: false,
                modifiers,
            }],
        );
    }
}
pub fn run() {
    assert!(
        std::env::args().any(|a| a == "--offline")
            && std::env::args().any(|a| a.starts_with("--data-dir=var/")),
        "UI regression requires isolated offline data"
    );
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    super::theme::apply(&ctx);
    let mut app = MyApp::new();
    let mut time = 0.0;
    for mode in ["performance", "vocoder", "roll", "reverb"] {
        super::preview::configure(&mut app, mode);
        for focus in [
            Focus::Right,
            Focus::Left,
            Focus::Transport,
            Focus::Performance,
        ] {
            app.editor.expanded = false;
            app.focus_panel(&ctx, focus);
            frame(&ctx, &mut app, &mut time, vec![]);
            app.open_editor(&ctx);
            frame(&ctx, &mut app, &mut time, vec![]);
            for _ in 0..12 {
                for key in [Key::ArrowDown, Key::Tab, Key::ArrowUp] {
                    press(&ctx, &mut app, &mut time, key, Modifiers::NONE);
                    assert!(
                        app.app_state == AppState::MainLoop && app.editor.expanded,
                        "Navigation left editor"
                    );
                }
            }
            let previous = app.editor.page;
            press(&ctx, &mut app, &mut time, Key::Tab, Modifiers::CTRL);
            if mode == "performance" {
                assert!(
                    app.editor.page != previous,
                    "Ctrl+Tab must select next editor page"
                );
            }
            press(
                &ctx,
                &mut app,
                &mut time,
                Key::Tab,
                Modifiers {
                    ctrl: true,
                    shift: true,
                    ..Default::default()
                },
            );
            assert!(app.editor.page == previous);
            for key in [Key::F6, Key::F7, Key::F8] {
                press(&ctx, &mut app, &mut time, key, Modifiers::NONE);
                assert!(app.editor.expanded, "A focus key must not leave the editor");
            }
            press(&ctx, &mut app, &mut time, Key::Enter, Modifiers::NONE);
            assert!(
                ctx.memory(|m| m.any_popup_open()),
                "Enter must open focused effect selector"
            );
            press(&ctx, &mut app, &mut time, Key::ArrowDown, Modifiers::NONE);
            press(&ctx, &mut app, &mut time, Key::Escape, Modifiers::NONE);
            assert!(
                app.editor.expanded && !ctx.memory(|m| m.any_popup_open()),
                "Esc closes the popup first"
            );
            press(&ctx, &mut app, &mut time, Key::Escape, Modifiers::NONE);
            assert!(!app.editor.expanded && app.app_state == AppState::MainLoop);
        }
    }
    app.back_to_projects();
    frame(&ctx, &mut app, &mut time, vec![]);
    press(&ctx, &mut app, &mut time, Key::ArrowDown, Modifiers::NONE);
    press(&ctx, &mut app, &mut time, Key::Escape, Modifiers::NONE);
    assert!(
        app.app_state == AppState::MainLoop,
        "Esc cancels save dialog"
    );
    press(&ctx, &mut app, &mut time, Key::F12, Modifiers::NONE);
    press(&ctx, &mut app, &mut time, Key::Escape, Modifiers::NONE);
    assert!(!app.help_open);
    // A native close request must be cancelled until the save dialog is resolved.
    time += 1.0 / 60.0;
    let mut close = egui::RawInput {
        time: Some(time),
        ..Default::default()
    };
    close
        .viewports
        .entry(egui::ViewportId::ROOT)
        .or_default()
        .events
        .push(egui::ViewportEvent::Close);
    let output = ctx.run(close, |ctx| app.render_frame(ctx));
    assert!(output.viewport_output.values().any(|v| {
        v.commands
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::CancelClose))
    }));
    assert!(!output.viewport_output.values().any(|v| {
        v.commands
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::Close))
    }));
    press(&ctx, &mut app, &mut time, Key::ArrowDown, Modifiers::NONE);
    press(&ctx, &mut app, &mut time, Key::Tab, Modifiers::CTRL);
    press(&ctx, &mut app, &mut time, Key::Escape, Modifiers::NONE);
    assert!(app.app_state == AppState::MainLoop);
    app.editor.target = None;
    app.open_editor(&ctx);
    frame(&ctx, &mut app, &mut time, vec![]);
    press(&ctx, &mut app, &mut time, Key::ArrowDown, Modifiers::NONE);
    press(&ctx, &mut app, &mut time, Key::Tab, Modifiers::CTRL);
    assert!(app.editor.expanded);
    press(&ctx, &mut app, &mut time, Key::Escape, Modifiers::NONE);
    // F9 remains discoverable from focused widgets/editors and reports why it
    // cannot start offline; it must not disappear behind the text-input gate.
    app.open_editor(&ctx);
    frame(&ctx, &mut app, &mut time, vec![]);
    press(&ctx, &mut app, &mut time, Key::F9, Modifiers::NONE);
    assert_eq!(
        app.status,
        app.language.text("Connect audio before recording a replay")
    );
    press(&ctx, &mut app, &mut time, Key::F10, Modifiers::NONE);
    assert!(app.replay_browser);
    press(&ctx, &mut app, &mut time, Key::Escape, Modifiers::NONE);
    assert!(!app.replay_browser);
    app.calibration_open = true;
    frame(&ctx, &mut app, &mut time, vec![]);
    press(&ctx, &mut app, &mut time, Key::ArrowDown, Modifiers::NONE);
    press(&ctx, &mut app, &mut time, Key::Escape, Modifiers::NONE);
    assert!(!app.calibration_open);
    app.view.running = true;
    assert!(
        app.tracks_stopped(),
        "Clock alone does not make stopped tracks busy"
    );
    super::preview::configure(&mut app, "playback-rose");
    let before = serde_json::to_vec(&crate::project::data_from_config(&app.config)).unwrap();
    for _ in 0..4 {
        frame(&ctx, &mut app, &mut time, vec![]);
    }
    assert_eq!(
        before,
        serde_json::to_vec(&crate::project::data_from_config(&app.config)).unwrap(),
        "Replay panel must not change live configuration"
    );
    press(&ctx, &mut app, &mut time, Key::ArrowDown, Modifiers::NONE);
    press(&ctx, &mut app, &mut time, Key::Escape, Modifiers::NONE);
    assert!(!app.player_open && app.replay_panel.is_none());
    app.app_state = AppState::Init;
    app.active_project_idx = None;
    let count = app.projects.len();
    let identity = app.projects[0].file.clone();
    app.sel_project_idx = 0;
    app.trash_project();
    assert_eq!(app.projects.len(), count - 1);
    app.restore_project();
    assert_eq!(app.projects.len(), count);
    assert!(app.projects.iter().any(|p| p.file == identity));
    app.project_name_mode = Some(ProjectNameMode::Add);
    frame(&ctx, &mut app, &mut time, vec![]);
    press(&ctx, &mut app, &mut time, Key::Escape, Modifiers::NONE);
    assert!(app.project_name_mode.is_none());
    frame(&ctx, &mut app, &mut time, vec![]);
    println!("UI regression passed: navigation, editor tabs, dialogs and accessibility focus.");
}
