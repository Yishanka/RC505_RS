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
    let mut raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            ctx.data(|d| d.get_temp::<egui::Vec2>(egui::Id::new("regression-size")))
                .unwrap_or(egui::vec2(1320.0, 900.0)),
        )),
        time: Some(*time),
        modifiers,
        events,
        ..Default::default()
    };
    <MyApp as eframe::App>::raw_input_hook(app, ctx, &mut raw);
    let output = ctx.run(raw, |ctx| app.render_frame(ctx));
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
    ctx.data_mut(|data| data.insert_temp(egui::Id::new("master-fx-window").with("page"), 0_u8));
    frame(ctx, app, time, vec![]);
    let cutoff = ctx
        .data(|data| {
            data.get_temp::<egui::Id>(egui::Id::new((
                "parameter",
                app.language.text("Cutoff(Hz)"),
            )))
        })
        .expect("Master filter cutoff must be rendered");
    ctx.memory_mut(|memory| memory.request_focus(cutoff));
    frame(ctx, app, time, vec![]);
    let before = app.config.master_fx.filter.cutoff_hz;
    press(ctx, app, time, Key::ArrowRight, Modifiers::NONE);
    assert_eq!(app.config.master_fx.filter.cutoff_hz, before + 1);
    press(ctx, app, time, Key::ArrowDown, Modifiers::NONE);
    let q = app.config.master_fx.filter.resonance_x10;
    press(ctx, app, time, Key::ArrowRight, Modifiers::NONE);
    assert_eq!(app.config.master_fx.filter.resonance_x10, q + 1);
    assert_eq!(app.config.master_fx.filter.cutoff_hz, before + 1);
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
        (delay_time(app) - 2.23).abs() < 1e-5,
        "Performance key adds one millisecond and retains the typed fraction"
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
    assert!((delay_time(app) - 2.23).abs() < 1e-5, "Up only moves focus");
    assert_ne!(ctx.memory(|m| m.focused()), Some(parameter));
    press(ctx, app, time, Key::ArrowDown, Modifiers::NONE);
    assert_eq!(ctx.memory(|m| m.focused()), Some(parameter));
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
        (delay_time(app) - 3.23).abs() < 1e-5,
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

fn compact_mouse_scroll_and_focus_regression(ctx: &egui::Context, app: &mut MyApp, time: &mut f64) {
    ctx.data_mut(|data| {
        data.insert_temp(egui::Id::new("regression-size"), egui::vec2(960.0, 720.0))
    });
    super::preview::configure(app, "audio-fx-panning");
    app.editor.expanded = false;
    app.focus_panel(ctx, Focus::Performance);
    let panel = || {
        ctx.data(|data| {
            data.get_temp::<(egui::Id, f32, egui::Rect, egui::Vec2)>(egui::Id::new((
                "quick-panel-scroll",
                Focus::Right as u8,
            )))
        })
        .unwrap()
    };
    for _ in 0..20 {
        frame(ctx, app, time, vec![]);
    }
    let before = panel();
    let bar = egui::pos2(before.2.right() + 6.0, before.2.center().y);
    frame(ctx, app, time, vec![egui::Event::PointerMoved(bar)]);
    for _ in 0..20 {
        frame(ctx, app, time, vec![]);
    }
    frame(
        ctx,
        app,
        time,
        vec![egui::Event::PointerButton {
            pos: bar,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        }],
    );
    let end = bar + egui::vec2(0.0, 20.0);
    frame(ctx, app, time, vec![egui::Event::PointerMoved(end)]);
    frame(
        ctx,
        app,
        time,
        vec![egui::Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
    );
    let dragged = panel();
    assert!(
        dragged.1 > before.1 + 40.0,
        "Mouse scrollbar must scroll the quick panel: before={before:?}, after={dragged:?}"
    );
    for _ in 0..25 {
        frame(ctx, app, time, vec![]);
    }
    assert!(
        app.focus == Focus::Performance,
        "Scrolling alone must not select the panel"
    );
    assert!(
        (panel().1 - dragged.1).abs() < 0.1,
        "Mouse scrollbar must not snap back to top: {dragged:?} -> {:?}",
        panel()
    );
    press(ctx, app, time, Key::F8, Modifiers::NONE);
    for _ in 0..8 {
        press(ctx, app, time, Key::ArrowDown, Modifiers::NONE);
    }
    for _ in 0..25 {
        frame(ctx, app, time, vec![]);
    }
    let selected = ctx.memory(|m| m.focused()).unwrap();
    let scrolled = panel();
    assert!(scrolled.1 > 30.0);
    press(ctx, app, time, Key::Escape, Modifiers::NONE);
    for _ in 0..25 {
        frame(ctx, app, time, vec![]);
    }
    assert!(
        (panel().1 - scrolled.1).abs() < 0.1,
        "Leaving the panel must retain its viewport"
    );
    press(ctx, app, time, Key::F8, Modifiers::NONE);
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(selected),
        "Re-entering the same panel must restore its last parameter"
    );
    let time_id = ctx
        .data(|data| {
            data.get_temp::<egui::Id>(egui::Id::new((
                "parameter",
                app.language.choose("Time (ms)", "时间（毫秒）"),
            )))
        })
        .unwrap();
    ctx.memory_mut(|memory| memory.request_focus(time_id));
    frame(ctx, app, time, vec![]);
    press(ctx, app, time, Key::Enter, Modifiers::NONE);
    assert!(super::navigation::text_focused(ctx));
    let bar = egui::pos2(panel().2.right() + 6.0, panel().2.center().y);
    frame(
        ctx,
        app,
        time,
        vec![
            egui::Event::PointerMoved(bar),
            egui::Event::PointerButton {
                pos: bar,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
    );
    let end = bar + egui::vec2(0.0, 20.0);
    frame(ctx, app, time, vec![egui::Event::PointerMoved(end)]);
    frame(
        ctx,
        app,
        time,
        vec![egui::Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
    );
    let offset = panel().1;
    for _ in 0..25 {
        frame(ctx, app, time, vec![]);
    }
    assert!(
        (panel().1 - offset).abs() < 0.1,
        "Leaving a numeric entry by scrollbar must not scroll back to its caret"
    );
    // Pointer scrolling after leaving a keyboard panel, and after a page/type
    // change, must not inherit a stale request to focus/reveal the first control.
    super::preview::configure(app, "performance");
    app.left_page = crate::app::LeftPage::Audio;
    for scope in [Focus::Right, Focus::Left] {
        app.focus_panel(ctx, Focus::Performance);
        assert!(
            !app.focus_request,
            "Returning to performance must clear a pending panel-entry request"
        );
        for _ in 0..20 {
            frame(ctx, app, time, vec![]);
        }
        let info = || {
            ctx.data(|data| {
                data.get_temp::<(egui::Id, f32, egui::Rect, egui::Vec2)>(egui::Id::new((
                    "quick-panel-scroll",
                    scope as u8,
                )))
            })
            .unwrap()
        };
        let scroll = info();
        let bar = egui::pos2(scroll.2.right() + 6.0, scroll.2.center().y);
        frame(ctx, app, time, vec![egui::Event::PointerMoved(bar)]);
        for _ in 0..12 {
            frame(ctx, app, time, vec![]);
        }
        frame(
            ctx,
            app,
            time,
            vec![egui::Event::PointerButton {
                pos: bar,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            }],
        );
        let end = bar + egui::vec2(0.0, 15.0);
        frame(ctx, app, time, vec![egui::Event::PointerMoved(end)]);
        frame(
            ctx,
            app,
            time,
            vec![egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }],
        );
        let offset = info().1;
        assert!(
            offset > 30.0,
            "Scrollbar must move the viewport for scope {}",
            scope as u8
        );
        for _ in 0..20 {
            frame(ctx, app, time, vec![]);
        }
        assert!(
            app.focus == Focus::Performance,
            "Mouse-only scrolling must not enter a keyboard scope"
        );
        assert!(
            (info().1 - offset).abs() < 0.1,
            "Mouse scrollbar must retain the offset for scope {}: {offset} -> {}",
            scope as u8,
            info().1
        );
        // Wheel input also stays local to the hovered panel and must not select it.
        let hover = info().2.center();
        frame(
            ctx,
            app,
            time,
            vec![
                egui::Event::PointerMoved(hover),
                egui::Event::Scroll(egui::vec2(0.0, -45.0)),
            ],
        );
        for _ in 0..20 {
            frame(ctx, app, time, vec![]);
        }
        assert!(
            info().1 >= offset - 0.1,
            "Wheel scrolling must not jump back to the beginning"
        );
        assert!(app.focus == Focus::Performance);
    }
    app.focus_panel(ctx, Focus::Performance);
    ctx.data_mut(|data| data.remove::<egui::Vec2>(egui::Id::new("regression-size")));
}

fn compact_parameter_navigation_regression(ctx: &egui::Context, app: &mut MyApp, time: &mut f64) {
    use crate::{config::InputFx, engine::core::Action};
    ctx.data_mut(|data| {
        data.insert_temp(egui::Id::new("regression-size"), egui::vec2(960.0, 720.0))
    });
    super::preview::configure(app, "audio-fx-panning");
    app.editor.expanded = false;
    app.action(Action::Panic);
    app.focus_panel(ctx, Focus::Right);
    frame(ctx, app, time, vec![]);
    let before = serde_json::to_vec(&crate::project::data_from_config(&app.config)).unwrap();
    let mut seen = std::collections::HashSet::new();
    for key in [Key::ArrowDown, Key::ArrowUp] {
        for n in 0..90 {
            frame(
                ctx,
                app,
                time,
                vec![egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed: true,
                    repeat: n > 0,
                    modifiers: Modifiers::NONE,
                }],
            );
            assert!(
                app.focus == Focus::Right,
                "Vertical traversal must stay in the quick FX panel"
            );
            if let Some(id) = ctx.memory(|m| m.focused()) {
                seen.insert(id);
            }
            let after = serde_json::to_vec(&crate::project::data_from_config(&app.config)).unwrap();
            if after != before {
                fn differences(
                    a: &serde_json::Value,
                    b: &serde_json::Value,
                    path: &str,
                    out: &mut Vec<String>,
                ) {
                    if a == b {
                        return;
                    }
                    match (a, b) {
                        (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
                            for (k, v) in a {
                                differences(v, &b[k], &format!("{path}/{k}"), out);
                            }
                        }
                        (serde_json::Value::Array(a), serde_json::Value::Array(b))
                            if a.len() == b.len() =>
                        {
                            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                                differences(a, b, &format!("{path}/{i}"), out);
                            }
                        }
                        _ => out.push(format!("{path}: {a} -> {b}")),
                    }
                }
                let mut changes = Vec::new();
                differences(
                    &serde_json::from_slice(&before).unwrap(),
                    &serde_json::from_slice(&after).unwrap(),
                    "",
                    &mut changes,
                );
                panic!("Vertical navigation changed config (event {n}, {key:?}): {changes:?}");
            }
            assert!(
                !ctx.memory(|m| m.any_popup_open()),
                "Vertical navigation must not open a picker"
            );
        }
        frame(
            ctx,
            app,
            time,
            vec![egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed: false,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
    }
    assert!(
        seen.len() > 5,
        "Held arrows must traverse controls in the small quick panel: {} distinct focus IDs",
        seen.len()
    );
    let kind = ctx
        .data(|d| d.get_temp::<egui::Id>(egui::Id::new(("selector", "kind"))))
        .unwrap();
    ctx.memory_mut(|m| m.request_focus(kind));
    frame(ctx, app, time, vec![]);
    let previous = app.config.input_fx.slot_kind(0, 0);
    press(ctx, app, time, Key::ArrowRight, Modifiers::NONE);
    assert!(
        app.config.input_fx.slot_kind(0, 0) != previous,
        "Horizontal keys edit the focused enum"
    );
    press(ctx, app, time, Key::ArrowLeft, Modifiers::NONE);
    assert!(app.config.input_fx.slot_kind(0, 0) == previous);
    if let Some(InputFx::Audio(delay)) = &mut app.config.input_fx.banks[0].slots[0].fx {
        delay.time_ms = 7.53;
        delay.sync_beats = 0.0;
    }
    frame(ctx, app, time, vec![]);
    let time_id = ctx
        .data(|d| {
            d.get_temp::<egui::Id>(egui::Id::new((
                "parameter",
                app.language.choose("Time (ms)", "时间（毫秒）"),
            )))
        })
        .unwrap();
    ctx.memory_mut(|m| m.request_focus(time_id));
    frame(ctx, app, time, vec![]);
    press(ctx, app, time, Key::ArrowRight, Modifiers::NONE);
    if let Some(InputFx::Audio(delay)) = &app.config.input_fx.banks[0].slots[0].fx {
        assert!((delay.time_ms - 8.53).abs() < 1e-5);
    }
    app.config.input_fx.set_slot_kind(
        0,
        0,
        crate::config::FxKind::Audio(crate::config::audio_fx::AudioFxKind::Equalizer),
    );
    frame(ctx, app, time, vec![]);
    let db_id = ctx
        .data(|d| {
            d.get_temp::<egui::Id>(egui::Id::new((
                "parameter",
                app.language.choose("Low shelf (dB)", "低频搁架（dB）"),
            )))
        })
        .unwrap();
    ctx.memory_mut(|m| m.request_focus(db_id));
    frame(ctx, app, time, vec![]);
    press(ctx, app, time, Key::ArrowRight, Modifiers::NONE);
    if let Some(InputFx::Audio(eq)) = &app.config.input_fx.banks[0].slots[0].fx {
        assert_eq!(eq.low_db, 0.5, "dB controls use half-dB keyboard steps");
    }
    // The first frame after Enter already belongs to the newly opened input,
    // even before egui has emitted IME geometry for it.
    frame(
        ctx,
        app,
        time,
        vec![egui::Event::Key {
            key: Key::Enter,
            physical_key: Some(Key::Enter),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );
    frame(
        ctx,
        app,
        time,
        vec![
            egui::Event::Key {
                key: Key::Num1,
                physical_key: Some(Key::Num1),
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
            egui::Event::Text("1".into()),
        ],
    );
    assert!(
        matches!(app.view.tracks[0].mode, crate::engine::core::Mode::Empty),
        "Typing cannot trigger track recording"
    );
    frame(
        ctx,
        app,
        time,
        vec![
            egui::Event::Key {
                key: Key::Num1,
                physical_key: Some(Key::Num1),
                pressed: false,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
            egui::Event::Key {
                key: Key::Enter,
                physical_key: Some(Key::Enter),
                pressed: false,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
        ],
    );
    press(ctx, app, time, Key::Escape, Modifiers::NONE);
    if let Some(InputFx::Audio(eq)) = &app.config.input_fx.banks[0].slots[0].fx {
        assert_eq!(eq.low_db, 0.5, "Cancel restores the value before typing");
    }
    ctx.data_mut(|data| data.remove::<egui::Vec2>(egui::Id::new("regression-size")));
    app.focus_panel(ctx, Focus::Performance);
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
    app.editor.library_open = true;
    frame(ctx, app, time, vec![]);
    let name_id = ctx
        .data(|d| d.get_temp::<egui::Id>(egui::Id::new("preset-name-field")))
        .unwrap();
    // Stop immediately on the traversal frame, before this TextEdit can draw
    // with focus and emit an IME rectangle.
    for _ in 0..35 {
        frame(
            ctx,
            app,
            time,
            vec![egui::Event::Key {
                key: Key::Tab,
                physical_key: Some(Key::Tab),
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
        if ctx.memory(|m| m.focused()) == Some(name_id) {
            break;
        }
        frame(
            ctx,
            app,
            time,
            vec![egui::Event::Key {
                key: Key::Tab,
                physical_key: Some(Key::Tab),
                pressed: false,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
    }
    assert_eq!(ctx.memory(|m| m.focused()), Some(name_id));
    let prior_mode = app.view.tracks[0].mode;
    frame(
        ctx,
        app,
        time,
        vec![
            egui::Event::Key {
                key: Key::Num1,
                physical_key: Some(Key::Num1),
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
            egui::Event::Text("1".into()),
        ],
    );
    assert!(
        app.view.tracks[0].mode == prior_mode,
        "First-frame preset text entry cannot trigger a track"
    );
    assert!(app.editor.preset_name.ends_with('1'));
    frame(
        ctx,
        app,
        time,
        vec![
            egui::Event::Key {
                key: Key::Num1,
                physical_key: Some(Key::Num1),
                pressed: false,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
            egui::Event::Key {
                key: Key::Tab,
                physical_key: Some(Key::Tab),
                pressed: false,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
        ],
    );
    ctx.memory_mut(|m| m.stop_text_input());
    app.editor.library_open = false;
    app.editor.preset_name.clear();
    app.focus_panel(ctx, Focus::Editor);
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
    press(ctx, app, time, Key::ArrowRight, Modifiers::NONE);
    assert_ne!(
        app.editor.phrase_source, before,
        "A source enum accepts horizontal arrows without leaving the editor"
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

fn sample_persistence_regression() {
    use crate::{
        config::{
            AppConfig, FxKind, InputFx,
            osc_configs::{SampleAsset, Waveform},
        },
        engine::core::AudioSnapshot,
        presets::{self, FxTarget},
        project, session,
    };
    use std::sync::Arc;
    let target = FxTarget::Input { bank: 0, slot: 0 };
    let entry = project::ProjectEntry {
        name: "Sample persistence".into(),
        file: format!("sample-policy-{}.json", session::id()),
    };
    let mut config = AppConfig::new(120, 0, 5);
    config.input_fx.set_slot_kind(0, 0, FxKind::Oscillator);
    let source = Arc::new(SampleAsset::new(
        "Temporary source".into(),
        8000,
        (0..32).map(|i| (i as f32 * 0.5).sin() * 0.2).collect(),
    ));
    let osc = |c: &AppConfig| match &c.input_fx.banks[0].slots[0].fx {
        Some(InputFx::Oscillator(o)) => o.sample.as_ref().map(|s| s.content_hash),
        _ => None,
    };
    if let Some(InputFx::Oscillator(o)) = &mut config.input_fx.banks[0].slots[0].fx {
        o.waveform.value = Waveform::Sample;
        o.sample = Some(source.clone());
    }
    let data = project::data_from_config(&config);
    project::save_project_data(&entry, &data).unwrap();
    let saved = project::load_project(&entry).unwrap().unwrap();
    assert!(
        saved.input_fx.banks[0].slots[0]
            .osc
            .as_ref()
            .unwrap()
            .sample
            .is_none(),
        "Ordinary save must discard temporary PCM"
    );
    assert_eq!(
        osc(&config),
        Some(source.content_hash),
        "Saving config must not clear the current live sample"
    );
    let revision = session::save_snapshot(&entry, &AudioSnapshot::empty(8000), data).unwrap();
    let bundle = session::project_assets(&entry)
        .unwrap()
        .join("snapshots")
        .join(revision);
    let (_, saved) = session::read_bundle(&bundle).unwrap();
    assert!(
        saved.input_fx.banks[0].slots[0]
            .osc
            .as_ref()
            .unwrap()
            .sample
            .is_none(),
        "Audio snapshot cannot silently save a temporary OSC sound"
    );
    assert!(
        std::fs::read_dir(&bundle)
            .unwrap()
            .flatten()
            .all(|f| f.path().extension().is_none_or(|e| e != "wav"))
    );
    // A saved replay must remain self-contained even when its sound was temporary.
    let take = crate::replay::library::root().join(format!("sample-policy-{}", session::id()));
    let writer = crate::replay::Writer::begin(
        take.clone(),
        entry.file.clone(),
        0,
        AudioSnapshot::empty(8000),
        project::data_from_config(&config),
    )
    .unwrap();
    writer.finish(0).unwrap();
    let (_, initial) = session::read_bundle(&take.join("initial")).unwrap();
    assert_eq!(
        initial.input_fx.banks[0].slots[0]
            .osc
            .as_ref()
            .unwrap()
            .sample
            .as_ref()
            .unwrap()
            .content_hash,
        source.content_hash
    );

    let name = format!("saved-sample-{}", session::id());
    presets::save(&mut config, target, &name).unwrap();
    if let Some(InputFx::Oscillator(o)) = &mut config.input_fx.banks[0].slots[0].fx {
        o.sample_start = 0.25;
        o.envelope.attack_ms.value = 317.0;
    }
    project::save_project_data(&entry, &project::data_from_config(&config)).unwrap();
    let path = crate::app_support::paths::projects_dir().join(&entry.file);
    let disk: project::ProjectData =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let disk_osc = disk.input_fx.banks[0].slots[0].osc.as_ref().unwrap();
    assert!(
        disk_osc.sample.is_none() && disk_osc.sample_ref.is_some(),
        "Project JSON should refer to one saved sound, not copy PCM"
    );
    let loaded = project::load_project(&entry).unwrap().unwrap();
    let loaded_osc = loaded.input_fx.banks[0].slots[0].osc.as_ref().unwrap();
    assert_eq!(loaded_osc.sample.as_ref().unwrap().frames, source.frames);
    assert_eq!(loaded_osc.sample_start, 0.25);
    assert_eq!(
        loaded_osc.envelope.attack_ms, 317.0,
        "Reference loading must not replace current sound controls"
    );
    let reference = loaded_osc.sample_ref.as_ref().unwrap().clone();
    let mut invalid = reference.clone();
    invalid.preset = "../outside".into();
    assert!(presets::read_saved_sample(&invalid).is_err());
    let preset_file = crate::app_support::paths::projects_dir()
        .with_file_name("presets")
        .join(format!("{name}.json"));
    let shared_take =
        crate::replay::library::root().join(format!("sample-library-reference-{}", session::id()));
    crate::replay::Writer::begin(
        shared_take.clone(),
        entry.file.clone(),
        0,
        AudioSnapshot::empty(8000),
        project::data_from_config(&config),
    )
    .unwrap()
    .finish(0)
    .unwrap();
    let original = std::fs::read(&preset_file).unwrap();
    let mut tampered = original.clone();
    tampered.push(b' ');
    std::fs::write(&preset_file, &tampered).unwrap();
    let source_replay = crate::replay::streaming::Source::open(&shared_take).unwrap();
    let (imported, imported_core) =
        crate::replay::streaming::prepare_import(source_replay, 0, 8000).unwrap();
    let imported_osc = imported.input_fx.banks[0].slots[0].osc.as_ref().unwrap();
    assert!(
        imported_osc.sample.is_some()
            && imported_osc.sample_ref.is_none()
            && imported_osc.sample_temporary,
        "Portable replay sources stay playable and require an explicit local sound save"
    );
    assert!(project::persistable_data(&imported).is_ok());
    drop(imported_core);
    let missing = project::load_project(&entry).unwrap().unwrap();
    let missing = missing.input_fx.banks[0].slots[0].osc.as_ref().unwrap();
    assert!(
        missing.sample.is_none() && missing.sample_error.is_some() && missing.sample_ref.is_some()
    );
    assert!(
        project::save_project_data(&entry, &project::data_from_config(&config)).is_err(),
        "Do not drop in-memory PCM into a broken reference"
    );
    assert_eq!(
        std::fs::read(&path).unwrap(),
        serde_json::to_vec_pretty(&disk).unwrap()
    );
    std::fs::write(&preset_file, original).unwrap();

    // Use the actual import-completion path: a new source must forget the old saved link.
    if let Some(InputFx::Oscillator(o)) = &mut config.input_fx.banks[0].slots[0].fx {
        let (tx, rx) = std::sync::mpsc::channel();
        o.sample_job = Some(rx);
        tx.send(Ok(SampleAsset::new(
            "Replacement".into(),
            8000,
            vec![0.1; 32],
        )))
        .unwrap();
        o.poll_sample();
        assert!(o.sample_temporary && o.sample_ref.is_none());
    }
    project::save_project_data(&entry, &project::data_from_config(&config)).unwrap();
    assert!(
        project::load_project(&entry)
            .unwrap()
            .unwrap()
            .input_fx
            .banks[0]
            .slots[0]
            .osc
            .as_ref()
            .unwrap()
            .sample
            .is_none()
    );
    assert_eq!(
        presets::read_saved_sample(&reference).unwrap().frames,
        source.frames,
        "A new temporary capture must not overwrite the saved sound"
    );

    // Older embedded samples count as already saved and keep working without migration I/O.
    let mut legacy = serde_json::to_value(project::data_from_config(&config)).unwrap();
    let object = legacy["input_fx"]["banks"][0]["slots"][0]["osc"]
        .as_object_mut()
        .unwrap();
    object.remove("sample_temporary");
    object.remove("sample_ref");
    let legacy: project::ProjectData = serde_json::from_value(legacy).unwrap();
    project::save_project_data(&entry, &legacy).unwrap();
    assert!(
        project::load_project(&entry)
            .unwrap()
            .unwrap()
            .input_fx
            .banks[0]
            .slots[0]
            .osc
            .as_ref()
            .unwrap()
            .sample
            .is_some()
    );
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
            // F8 now restores the previous control, which may be a parameter.
            // Test the picker explicitly instead of assuming entry resets focus.
            let picker = ctx
                .data(|data| data.get_temp::<egui::Id>(egui::Id::new("fx-kind-picker")))
                .unwrap();
            ctx.memory_mut(|memory| memory.request_focus(picker));
            frame(&ctx, &mut app, &mut time, vec![]);
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
    compact_parameter_navigation_regression(&ctx, &mut app, &mut time);
    compact_mouse_scroll_and_focus_regression(&ctx, &mut app, &mut time);
    fader_panel_transition_regression(&ctx, &mut app, &mut time);
    phrase_keyboard_regression(&ctx, &mut app, &mut time);
    sample_persistence_regression();
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
