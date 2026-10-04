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
fn finish_jobs(ctx: &egui::Context, app: &mut MyApp, time: &mut f64) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        frame(ctx, app, time, vec![]);
        if !app.busy() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Background UI job timed out: {}",
            app.status
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    // Apply the prepared replacement queued by the completed UI job.
    frame(ctx, app, time, vec![]);
}
fn replay_import_regression(ctx: &egui::Context, app: &mut MyApp, time: &mut f64) {
    use crate::{
        config::AppConfig,
        engine::core::{Action, AudioSnapshot},
        project, replay,
    };
    let entry = project::ProjectEntry {
        name: "Replay import regression".into(),
        file: format!("replay-import-{}.json", crate::session::id()),
    };
    project::save_project_data(
        &entry,
        &project::data_from_config(&AppConfig::new(120, 0, 5)),
    )
    .unwrap();
    let target = app.projects.len();
    app.projects.push(entry.clone());
    let path = crate::session::safe_child(&crate::app_support::paths::projects_dir(), &entry.file)
        .unwrap();
    let saved = crate::session::checksum(&path).unwrap();
    let root = replay::library::root().join(format!("ui-import-{}", crate::session::id()));
    let mut config = AppConfig::new(120, 0, 5);
    config.track_levels[0] = 0.23;
    for options in &mut config.track_options {
        options.quantize = crate::config::track_options::Quantize::Off;
    }
    let mut writer = replay::Writer::begin(
        root.clone(),
        "other-project.json".into(),
        0,
        AudioSnapshot::empty(48000),
        project::data_from_config(&config),
    )
    .unwrap();
    writer
        .event(0, replay::EventKind::Action(Action::Trigger(0)))
        .unwrap();
    writer.audio(0, &[[0.25; 2]; 8]).unwrap();
    writer
        .event(8, replay::EventKind::Action(Action::Trigger(0)))
        .unwrap();
    writer.finish(8).unwrap();
    let (consumer, session) = replay::streaming::start(&root, 48000).unwrap();
    session
        .shared
        .playing
        .store(false, std::sync::atomic::Ordering::Release);
    let shared = session.shared.clone();
    let mut panel = super::replay_panel::ReplayPanel::streaming(session);
    panel.request_import();
    app.replay_panel = Some(Box::new(panel));
    app.player_open = true;
    app.app_state = AppState::Init;
    frame(ctx, app, time, vec![]);
    ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("replay-import-name")));
    press(ctx, app, time, Key::Space, Modifiers::NONE);
    assert!(
        !shared.playing(),
        "Import name editing must not resume replay"
    );
    press(ctx, app, time, Key::Escape, Modifiers::NONE);
    assert!(
        app.player_open
            && app
                .replay_panel
                .as_ref()
                .is_some_and(|panel| !panel.modal_open()),
        "First Esc closes import dialog only"
    );
    press(ctx, app, time, Key::Escape, Modifiers::NONE);
    assert!(
        !app.player_open && app.app_state == AppState::Init,
        "Second Esc returns to project library"
    );
    drop(consumer);
    let source = replay::streaming::Source::open(&root).unwrap();
    app.import_replay_position(source.clone(), 4, Some(target), String::new());
    finish_jobs(ctx, app, time);
    assert_eq!(app.active_project_idx, Some(target));
    assert_eq!(app.config.track_levels[0], 0.23);
    assert_eq!(app.view.tracks[0].frames, 4);
    assert_eq!(
        crate::session::checksum(&path).unwrap(),
        saved,
        "Import must not overwrite the saved project"
    );
    // Reopening the unchanged saved file is the same baseline selected by
    // leaving with Discard: no implicit snapshot/import write occurred.
    app.open_project(target);
    finish_jobs(ctx, app, time);
    assert_eq!(app.config.track_levels[0], 1.0);
    assert_eq!(app.view.tracks[0].frames, 0);
    app.import_replay_position(source, 4, Some(target), String::new());
    finish_jobs(ctx, app, time);
    app.save_snapshot();
    finish_jobs(ctx, app, time);
    assert!(
        project::load_project(&entry)
            .unwrap()
            .unwrap()
            .snapshot
            .is_some()
    );
    app.open_project(target);
    finish_jobs(ctx, app, time);
    assert_eq!(app.config.track_levels[0], 0.23);
    assert_eq!(
        app.view.tracks[0].frames, 4,
        "Explicit save snapshot must persist imported audio"
    );
    assert!(
        root.join("input.wav").exists(),
        "Import must preserve replay inputs"
    );
    let allowed =
        std::fs::canonicalize(crate::app_support::paths::appdata_root().unwrap()).unwrap();
    assert!(allowed.starts_with(std::fs::canonicalize("var").unwrap()));
    for directory in [root, crate::session::project_assets(&entry).unwrap()] {
        let directory = std::fs::canonicalize(directory).unwrap();
        assert!(directory.starts_with(&allowed));
        std::fs::remove_dir_all(directory).unwrap();
    }
    std::fs::remove_file(path).unwrap();
    app.projects.pop();
    app.active_project_idx = None;
    project::save_index(&app.projects).unwrap();
}
fn tap_start_regression(ctx: &egui::Context, app: &mut MyApp, time: &mut f64) {
    // Use the same production event clock for mouse and keyboard taps. The
    // offline callback bridge applies commands without accessing any device.
    app.app_state = AppState::MainLoop;
    app.focus_panel(ctx, Focus::Performance);
    app.audio.online = true;
    app.action(crate::engine::core::Action::Panic);
    frame(ctx, app, time, vec![]);
    app.config.beat_config.set_values(31, 0);
    frame(ctx, app, time, vec![]);
    let next = *time + 1.0 / 60.0;
    app.tap_tempo(next - 0.5);
    assert_eq!(
        app.config.beat_config.current_bpm(),
        31,
        "First tap is only an origin"
    );
    let events = [Key::Space, Key::T].map(|key| egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    frame(ctx, app, time, events.into());
    assert_eq!(
        app.config.beat_config.current_bpm(),
        120,
        "Same-frame Tap is processed before transport starts"
    );
    assert!(
        !app.tempo_edit_allowed(),
        "Queued start must lock late BPM mouse edits before audio view arrives"
    );
    let release = [Key::Space, Key::T].map(|key| egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed: false,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    frame(ctx, app, time, release.into());
    assert!(app.view.running);
    assert_eq!(app.view.bpm, 120);
    assert_eq!(app.config.beat_config.current_bpm(), app.view.bpm as usize);
    // Even an external edit bypassing the GUI is reconciled to actual running
    // engine tempo; there is no unacknowledged permanent display mismatch.
    app.config.beat_config.input_bpm.value = 211;
    frame(ctx, app, time, vec![]);
    assert_eq!(app.config.beat_config.current_bpm(), 120);
    app.action(crate::engine::core::Action::Panic);
    frame(ctx, app, time, vec![]);
    assert!(app.tempo_edit_allowed());
    app.audio.online = false;
}
fn mouse_fader_keyboard_regression(ctx: &egui::Context, app: &mut MyApp, time: &mut f64) {
    app.app_state = AppState::MainLoop;
    app.focus_panel(ctx, Focus::Performance);
    app.editor.expanded = false;
    app.track_sel = Some(0);
    frame(ctx, app, time, vec![]);
    let (id, rect) = ctx
        .data(|d| d.get_temp::<(egui::Id, egui::Rect)>(egui::Id::new(("regression-fader", 0usize))))
        .unwrap();
    let before = app.config.track_levels[0];
    let pos = rect.center();
    for down in [true, false] {
        frame(
            ctx,
            app,
            time,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: down,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
    assert_ne!(
        app.config.track_levels[0], before,
        "The mouse must really operate the visible fader"
    );
    // Mouse drag alone does not claim egui keyboard focus. A subsequent Tab
    // focus does; request the actual response ID to test that widget state.
    ctx.memory_mut(|memory| memory.request_focus(id));
    frame(ctx, app, time, vec![]);
    assert!(
        ctx.wants_keyboard_input(),
        "Focused fader should expose egui's broad keyboard-focus flag"
    );
    let focused = ctx.memory(|m| m.focused()).unwrap();
    assert!(
        egui::TextEdit::load_state(ctx, focused).is_none(),
        "A fader is not a text editor"
    );
    let monitor = app.config.input_thru;
    press(ctx, app, time, Key::J, Modifiers::NONE);
    assert_ne!(
        app.config.input_thru, monitor,
        "Non-text fader focus must not disable performance keys"
    );
    let level = app.config.track_levels[0];
    press(ctx, app, time, Key::ArrowRight, Modifiers::NONE);
    assert_eq!(app.track_sel, Some(1));
    assert_eq!(
        app.config.track_levels[0], level,
        "Track-selection arrow must not also adjust the focused fader"
    );
    // Master parameters allow performance shortcuts as well.
    app.master_fx_open = true;
    frame(ctx, app, time, vec![]);
    let monitor = app.config.input_thru;
    press(ctx, app, time, Key::J, Modifiers::NONE);
    assert_ne!(app.config.input_thru, monitor);
    press(ctx, app, time, Key::Escape, Modifiers::NONE);
}
fn editor_performance_regression(ctx: &egui::Context, app: &mut MyApp, time: &mut f64) {
    use crate::{
        config::{InputFx, track_options::Quantize},
        engine::core::{Action, Mode},
    };
    super::preview::configure(app, "audio-fx-panning-small-en");
    app.language = crate::app_support::language::Language::English;
    app.audio.online = true;
    app.action(Action::Panic);
    for track in 0..5 {
        app.action(Action::Clear(track));
        app.config.track_options[track].quantize = Quantize::Off;
    }
    app.track_sel = Some(0);
    if let Some(InputFx::Audio(delay)) = &mut app.config.input_fx.banks[0].slots[0].fx {
        delay.time_ms = 1.23;
        delay.sync_beats = 0.0;
    }
    frame(ctx, app, time, vec![]);
    let parameter = ctx
        .data(|d| d.get_temp::<egui::Id>(egui::Id::new(("parameter", "Time (ms)"))))
        .expect("Delay control is visible");
    ctx.memory_mut(|m| m.request_focus(parameter));
    frame(ctx, app, time, vec![]);
    for down in [true, false] {
        frame(
            ctx,
            app,
            time,
            [Key::ArrowRight, Key::Num1]
                .into_iter()
                .map(|key| egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed: down,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                })
                .collect(),
        );
    }
    let delay_time = |app: &MyApp| match &app.config.input_fx.banks[0].slots[0].fx {
        Some(InputFx::Audio(p)) => p.time_ms,
        _ => panic!("Changed pinned effect"),
    };
    assert!(
        (delay_time(app) - 1.24).abs() < 1e-5,
        "Arrow precision is independent of slider width"
    );
    // The offline bridge accepts commands without advancing audio samples.
    assert!(
        app.view.tracks[0].pending,
        "Parameter editing must send Record to the engine in the same frame"
    );
    assert_eq!(
        app.track_sel,
        Some(0),
        "Parameter arrows must not select another track"
    );
    press(ctx, app, time, Key::Num2, Modifiers::CTRL);
    press(ctx, app, time, Key::Delete, Modifiers::NONE);
    press(ctx, app, time, Key::Delete, Modifiers::NONE);
    assert_eq!(app.track_sel, Some(0));
    assert!(
        app.view.tracks[0].pending,
        "Editor Delete must not clear a track or cancel its Record command"
    );
    press(ctx, app, time, Key::ArrowUp, Modifiers::NONE);
    assert!(
        (delay_time(app) - 1.34).abs() < 1e-5,
        "Up is ten fine steps, not focus traversal"
    );
    app.config.input_fx.banks[0].slots[1].is_enabled = false;
    let shift = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };
    frame(
        ctx,
        app,
        time,
        vec![egui::Event::Key {
            key: Key::W,
            physical_key: Some(Key::W),
            pressed: true,
            repeat: false,
            modifiers: shift,
        }],
    );
    assert!(app.config.input_fx.banks[0].slots[1].is_enabled);
    press(ctx, app, time, Key::ArrowRight, shift);
    assert!(
        (delay_time(app) - 1.35).abs() < 1e-5,
        "A held momentary FX must not block arrow adjustment"
    );
    assert!(app.config.input_fx.banks[0].slots[1].is_enabled);
    frame(
        ctx,
        app,
        time,
        vec![egui::Event::Key {
            key: Key::W,
            physical_key: Some(Key::W),
            pressed: false,
            repeat: false,
            modifiers: shift,
        }],
    );
    assert!(!app.config.input_fx.banks[0].slots[1].is_enabled);
    press(ctx, app, time, Key::Enter, Modifiers::NONE);
    frame(
        ctx,
        app,
        time,
        vec![
            egui::Event::Text("7.89".into()),
            egui::Event::Key {
                key: Key::Num2,
                physical_key: Some(Key::Num2),
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
        ],
    );
    frame(
        ctx,
        app,
        time,
        vec![egui::Event::Key {
            key: Key::Num2,
            physical_key: Some(Key::Num2),
            pressed: false,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );
    press(ctx, app, time, Key::Enter, Modifiers::NONE);
    assert!(
        (delay_time(app) - 7.89).abs() < 1e-5,
        "Enter commits a precise typed value"
    );
    assert_eq!(app.view.tracks[1].mode, Mode::Empty);
    assert!(
        !app.view.tracks[1].pending,
        "Typing a number must not record another track"
    );
    press(ctx, app, time, Key::Enter, Modifiers::NONE);
    frame(ctx, app, time, vec![egui::Event::Text("500".into())]);
    press(ctx, app, time, Key::Escape, Modifiers::NONE);
    assert!(app.editor.expanded);
    assert!(
        (delay_time(app) - 7.89).abs() < 1e-5,
        "Escape cancels numeric entry before closing the editor"
    );
    frame(ctx, app, time, vec![]);
    press(ctx, app, time, Key::F1, Modifiers::NONE);
    assert!(
        !app.view.tracks[0].pending,
        "Stop key stays available while editing"
    );
    app.action(Action::Panic);
    for track in 0..5 {
        app.action(Action::Clear(track));
    }
    frame(ctx, app, time, vec![]);
    app.audio.online = false;
    app.close_editor(ctx);
}
fn fader_panel_transition_regression(ctx: &egui::Context, app: &mut MyApp, time: &mut f64) {
    super::preview::configure(app, "performance");
    app.focus_panel(ctx, Focus::Performance);
    app.config.track_levels[0] = 0.5;
    app.config.track_options[0].fader_speed = 24.0;
    frame(ctx, app, time, vec![]);
    frame(
        ctx,
        app,
        time,
        vec![egui::Event::Key {
            key: Key::Z,
            physical_key: Some(Key::Z),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );
    for _ in 0..45 {
        frame(ctx, app, time, vec![]);
    }
    let before = crate::app::faders::decibels(app.config.track_levels[0]);
    app.open_editor(ctx);
    frame(ctx, app, time, vec![]);
    let after = crate::app::faders::decibels(app.config.track_levels[0]);
    assert!(
        (before - after - 0.4).abs() < 0.02,
        "Panel focus must preserve the running fader ramp, not restart its 0.5 dB tap"
    );
    frame(
        ctx,
        app,
        time,
        vec![egui::Event::Key {
            key: Key::Z,
            physical_key: Some(Key::Z),
            pressed: false,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );
    app.close_editor(ctx);
}

fn phrase_keyboard_regression(ctx: &egui::Context, app: &mut MyApp, time: &mut f64) {
    use crate::{
        config::{FxKind, InputFx},
        presets::FxTarget,
    };
    super::preview::configure(app, "sequence-links");
    app.language = crate::app_support::language::Language::English;
    app.config.input_fx.set_slot_kind(0, 2, FxKind::Oscillator);
    app.open_editor(ctx);
    frame(ctx, app, time, vec![]);
    let source_id = ctx
        .data(|d| d.get_temp::<egui::Id>(egui::Id::new(("selector", "phrase-link-source"))))
        .expect("Source selector is drawn");
    let mut reached = false;
    for _ in 0..40 {
        if ctx.memory(|m| m.focused()) == Some(source_id) {
            reached = true;
            break;
        }
        press(ctx, app, time, Key::Tab, Modifiers::NONE);
    }
    assert!(
        reached,
        "Tab must reach source selection in the phrase manager"
    );
    let before = app.editor.phrase_source.clone();
    press(ctx, app, time, Key::ArrowDown, Modifiers::NONE);
    assert_ne!(
        app.editor.phrase_source, before,
        "A source enum accepts arrows without leaving the editor"
    );
    assert!(app.editor.expanded);
    app.editor.phrase_manager_open = false;
    frame(ctx, app, time, vec![]);
    let snap = ctx
        .data(|d| d.get_temp::<egui::Id>(egui::Id::new(("selector", "snap"))))
        .expect("Piano toolbar is drawn");
    let mut reached = false;
    for _ in 0..40 {
        if ctx.memory(|m| m.focused()) == Some(snap) {
            reached = true;
            break;
        }
        press(ctx, app, time, Key::Tab, Modifiers::NONE);
    }
    assert!(reached, "Tab must reach the piano toolbar");
    let old_snap = app.editor.piano.snap;
    press(ctx, app, time, Key::ArrowRight, Modifiers::NONE);
    assert_ne!(app.editor.piano.snap, old_snap);
    let canvas = ctx
        .data(|d| d.get_temp::<egui::Id>(egui::Id::new("piano-canvas")))
        .expect("Piano canvas is drawn");
    let mut reached = false;
    for _ in 0..40 {
        if ctx.memory(|m| m.focused()) == Some(canvas) {
            reached = true;
            break;
        }
        press(ctx, app, time, Key::Tab, Modifiers::NONE);
    }
    assert!(reached, "Tab must reach the piano canvas");
    let notes = |app: &MyApp| match &app.config.input_fx.banks[0].slots[0].fx {
        Some(InputFx::Oscillator(o)) => o.note.events(),
        _ => panic!("OSC source changed"),
    };
    let old = notes(app);
    press(ctx, app, time, Key::A, Modifiers::CTRL);
    press(ctx, app, time, Key::ArrowUp, Modifiers::NONE);
    let moved = notes(app);
    assert_eq!(moved.len(), old.len());
    assert_ne!(
        moved, old,
        "Canvas arrows move selected notes instead of panel focus"
    );
    press(ctx, app, time, Key::Z, Modifiers::CTRL);
    assert_eq!(
        notes(app),
        old,
        "Piano undo is local while performance shortcuts remain available"
    );
    assert_eq!(
        app.editor.target,
        Some(FxTarget::Input { bank: 0, slot: 0 })
    );
    app.close_editor(ctx);
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
            // Arrow keys now edit enum/numeric values. Restore the fixture
            // before independently exercising the page navigation contract.
            super::preview::configure(&mut app, mode);
            app.open_editor(&ctx);
            frame(&ctx, &mut app, &mut time, vec![]);
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
    app.shortcuts.overrides.insert(
        "top".into(),
        vec![crate::app::shortcuts::Chord::new(
            Key::F11,
            false,
            false,
            false,
        )],
    );
    press(&ctx, &mut app, &mut time, Key::F11, Modifiers::NONE);
    assert!(
        app.focus == Focus::Transport && app.editor.expanded,
        "Custom top shortcut must route while editor is open"
    );
    app.shortcut_editor.open(&app.shortcuts);
    frame(&ctx, &mut app, &mut time, vec![]);
    let monitor = app.config.input_thru;
    press(&ctx, &mut app, &mut time, Key::J, Modifiers::NONE);
    assert_eq!(
        monitor, app.config.input_thru,
        "Shortcut editor must suspend performance actions"
    );
    press(&ctx, &mut app, &mut time, Key::Escape, Modifiers::NONE);
    assert!(!app.shortcut_editor.open && app.editor.expanded);
    app.shortcuts = crate::app::shortcuts::Bindings::defaults();
    app.master_fx_open = true;
    frame(&ctx, &mut app, &mut time, vec![]);
    let monitor = app.config.input_thru;
    for key in [Key::ArrowDown, Key::Tab, Key::J, Key::Num1] {
        press(&ctx, &mut app, &mut time, key, Modifiers::NONE);
        assert!(app.master_fx_open && app.app_state == AppState::MainLoop);
    }
    assert_ne!(monitor, app.config.input_thru);
    press(&ctx, &mut app, &mut time, Key::Escape, Modifiers::NONE);
    assert!(
        !app.master_fx_open && app.editor.expanded,
        "Esc leaves master editor before leaving project"
    );
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
    replay_import_regression(&ctx, &mut app, &mut time);
    tap_start_regression(&ctx, &mut app, &mut time);
    mouse_fader_keyboard_regression(&ctx, &mut app, &mut time);
    editor_performance_regression(&ctx, &mut app, &mut time);
    fader_panel_transition_regression(&ctx, &mut app, &mut time);
    phrase_keyboard_regression(&ctx, &mut app, &mut time);
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
