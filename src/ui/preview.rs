//! Debug-only, offline visual regression fixture. The application's renderer
//! captures actual frames; it never touches the user's normal project directory.
use super::editor::EditorPage;
use crate::{
    app::MyApp,
    config::{FxKind, InputFx},
    presets::FxTarget,
    state::AppState,
};

pub fn configure(app: &mut MyApp, mode: &str) {
    let mode = mode.strip_suffix("-en").unwrap_or(mode);
    if mode.contains("rose") {
        app.theme = crate::app_support::appearance::ThemeColor::Rose;
    }
    if mode.contains("ember") {
        app.theme = crate::app_support::appearance::ThemeColor::Ember;
    }
    if std::env::args().any(|a| a.ends_with("-en")) {
        app.language = crate::app_support::language::Language::English;
    }
    if mode.starts_with("projects") {
        return;
    }
    app.active_project_idx = Some(0);
    if mode.starts_with("performance") && mode.contains("audio") {
        app.left_page = crate::app::LeftPage::Audio;
        app.config.system_config.input_device.value =
            "USB microphone — multichannel audio interface with a long device name".into();
        app.config.system_config.output_device.value =
            "System output — digital audio interface with a long device name".into();
    }
    app.app_state = AppState::MainLoop;
    if mode.starts_with("calibration") {
        app.calibration_open = true;
    }
    if mode == "calibration-held" {
        app.audio
            .diagnostics
            .calibration_hold
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
    if mode.starts_with("replays") {
        app.replay_browser = true;
    }
    app.config.input_fx.set_slot_kind(0, 0, FxKind::Oscillator);
    app.config.input_fx.set_slot_kind(0, 1, FxKind::Filter);
    app.config.input_fx.set_slot_kind(0, 2, FxKind::Vocoder);
    app.config.input_fx.set_slot_kind(0, 3, FxKind::Reverb);
    if let Some(InputFx::Oscillator(osc)) = &mut app.config.input_fx.banks[0].slots[0].fx {
        let pitches = [48, 51, 55, 58, 55, 51, 46, 48];
        osc.note.replace_events(
            96,
            &pitches
                .iter()
                .enumerate()
                .map(|(i, p)| crate::config::sequence_edit::NoteEvent {
                    start: i * 6,
                    len: 5,
                    pitch: crate::config::note_configs::NoteOct::from_pitch_index(*p),
                })
                .collect::<Vec<_>>(),
        );
        osc.envelope.attack_ms.value = 80;
        osc.envelope.decay_ms.value = 350;
        osc.envelope.sustain_pct.value = 40;
        osc.envelope.release_ms.value = 300;
    }
    app.editor.select(FxTarget::Input { bank: 0, slot: 0 });
    app.editor.expanded = !mode.starts_with("performance")
        && !mode.starts_with("calibration")
        && !mode.starts_with("replays")
        && mode != "help";
    if mode == "help" {
        app.help_open = true;
        app.help_tab = 2;
    }
    if mode == "vocoder" {
        app.editor.select(FxTarget::Input { bank: 0, slot: 2 });
    }
    if mode == "reverb" {
        app.editor.select(FxTarget::Input { bank: 0, slot: 3 });
    }
    if mode == "roll" {
        app.config
            .track_fx
            .set_slot_kind(0, 0, crate::config::TrackFxKind::Roll);
        app.editor.select(FxTarget::Track { bank: 0, slot: 0 });
    }
    app.editor.page = match mode {
        "sequence" | "sequence-small" => EditorPage::Sequence,
        "filter" => EditorPage::Filter,
        "envelope" => EditorPage::Envelope,
        _ => EditorPage::Sound,
    };
    if mode.starts_with("playback") {
        let mut view = crate::engine::core::EngineView::default();
        view.running = true;
        view.elapsed = 48000;
        view.sample_rate = 48000;
        for (i, track) in view.tracks.iter_mut().enumerate().take(3) {
            track.mode = if i == 1 {
                crate::engine::core::Mode::Overdub
            } else {
                crate::engine::core::Mode::Playing
            };
            track.frames = 96000;
            track.cursor = 24000;
            track.wave = std::array::from_fn(|x| ((x as f32 * 0.39 + i as f32).sin() * 0.7).abs());
        }
        app.config.input_fx.banks[0].slots[0].is_enabled = true;
        app.config.track_levels[0] = 0.55;
        let visuals = crate::replay::ReplayVisuals {
            name: "BASS SESSION / 01".into(),
            sample_rate: 48000,
            frames: 192000,
            initial: crate::project::data_from_config(&app.config),
            configs: Vec::new(),
            views: vec![crate::replay::VisualFrame {
                frame: 0,
                view,
                last_action: Some((0, crate::engine::core::Action::Trigger(1))),
            }],
        };
        app.replay_panel = Some(Box::new(super::replay_panel::ReplayPanel::new(
            std::sync::Arc::new(visuals),
        )));
        app.player_open = true;
        app.editor.expanded = false;
    }
    if mode.starts_with("replays") {
        app.replay_list = vec![(
            std::path::PathBuf::from("var/preview-replay"),
            "BASS SESSION / 测试回放：一段较长的名字".into(),
        )];
    }
    if mode.starts_with("draft") {
        app.draft = Some(std::path::PathBuf::from("var/preview-draft"));
        app.editor.expanded = false;
        app.take_name = "BASS SESSION / 01".into();
    }
}

pub use super::capture::capture;

/// UI-only fixture; it does not pretend to record real audio.
pub fn sample_visuals(app: &mut MyApp, mode: &str) {
    if mode.starts_with("playback") {
        app.audio
            .diagnostics
            .player_frame
            .store(24000, std::sync::atomic::Ordering::Relaxed);
    }
    if mode.starts_with("performance-recording") {
        app.view.output_spectrum = std::array::from_fn(|i| ((i as f32 * 0.29).sin() * 0.7).abs());
    }
    if !mode.starts_with("performance-recording") {
        return;
    }
    app.view.running = true;
    app.view.sample_rate = 48000;
    app.view.elapsed = if mode.ends_with("dim") { 12000 } else { 0 };
    for (i, track) in app.view.tracks.iter_mut().enumerate().take(2) {
        track.mode = if i == 0 {
            crate::engine::core::Mode::Recording
        } else {
            crate::engine::core::Mode::Overdub
        };
        track.frames = 96000;
        track.cursor = 24000;
        track.wave = std::array::from_fn(|bin| ((bin as f32 * 0.8).sin() * 0.65).abs());
    }
}
