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
    app.open_project(0);
    app.app_state = AppState::MainLoop;
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
    app.editor.expanded = !mode.starts_with("performance");
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
        "sequence" => EditorPage::Sequence,
        "filter" => EditorPage::Filter,
        "envelope" => EditorPage::Envelope,
        _ => EditorPage::Sound,
    };
}

pub use super::capture::capture;
