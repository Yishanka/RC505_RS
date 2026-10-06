use super::*;
use crate::engine::loop_audio::OfflinePages;

fn engine() -> RenderCore {
    let mut config = AppConfig::new(120, 0, TRACKS);
    for option in &mut config.track_options {
        option.quantize = Quantize::Off;
    }
    let mut core = RenderCore::new(8000);
    core.configure(&mut Parameters::from_config(&config, 8000));
    core
}

fn tick(core: &mut RenderCore, frames: usize) {
    for _ in 0..frames {
        core.process([0.0; 2], &mut OfflinePages);
    }
}

fn stopped_loop(core: &mut RenderCore, track: usize, length: usize) {
    for i in 0..length {
        core.tracks[track]
            .audio
            .write(i, [0.1, -0.1], &mut OfflinePages);
    }
    core.tracks[track].mode = Mode::Stopped;
}

#[test]
fn transport_empty_all_and_metronome_off_do_not_leave_an_idle_clock_running() {
    let mut core = engine();
    core.action(Action::All, &mut OfflinePages);
    tick(&mut core, 10);
    assert!(core.idle() && !core.view().running);
    core.action(Action::Metronome(true), &mut OfflinePages);
    tick(&mut core, 100);
    assert!(!core.idle() && core.view().running);
    core.action(Action::Metronome(false), &mut OfflinePages);
    tick(&mut core, 1);
    assert!(core.idle() && !core.view().running);
    tick(&mut core, 47);
    let restart = core.clock.frame;
    core.action(Action::Metronome(true), &mut OfflinePages);
    assert_eq!(core.clock.origin, Some(restart));
    assert_eq!(core.clock.elapsed(), 0);
    // The previous renderer's clock semantics remain readable in old replays.
    core.set_renderer_version(11);
    core.action(Action::Metronome(true), &mut OfflinePages);
    core.action(Action::Metronome(false), &mut OfflinePages);
    tick(&mut core, 1);
    assert!(core.view().running);
}

#[test]
fn transport_follows_last_active_track_including_clear_undo_and_one_shot() {
    let mut core = engine();
    stopped_loop(&mut core, 0, 128);
    stopped_loop(&mut core, 1, 128);
    core.action(Action::All, &mut OfflinePages);
    tick(&mut core, 1);
    assert!(core.view().running);
    core.action(Action::Stop(0), &mut OfflinePages);
    tick(&mut core, 1);
    assert!(core.view().running);
    core.action(Action::Stop(1), &mut OfflinePages);
    tick(&mut core, 1);
    assert!(!core.view().running);
    core.options[0].one_shot = true;
    core.action(Action::Trigger(0), &mut OfflinePages);
    tick(&mut core, 127);
    assert!(core.view().running);
    tick(&mut core, 1);
    assert!(!core.view().running);
    core.options[0].one_shot = false;
    core.action(Action::Trigger(0), &mut OfflinePages);
    tick(&mut core, 1);
    core.action(Action::Clear(0), &mut OfflinePages);
    tick(&mut core, 1);
    assert!(!core.view().running);
    core.action(Action::UndoStep(0), &mut OfflinePages);
    tick(&mut core, 1);
    assert!(core.tracks[0].audio.len > 0);
    assert!(
        !core.view().running,
        "Restoring audio does not start playback"
    );
}

#[test]
fn transport_waits_for_quantized_start_fade_and_capture_tail() {
    let mut core = engine();
    stopped_loop(&mut core, 0, 256);
    core.action(Action::Metronome(true), &mut OfflinePages);
    tick(&mut core, 100);
    core.options[0].quantize = Quantize::Beat;
    core.action(Action::Trigger(0), &mut OfflinePages);
    core.action(Action::Metronome(false), &mut OfflinePages);
    tick(&mut core, 100);
    assert!(core.view().running && core.tracks[0].pending.is_some());
    tick(&mut core, 3801);
    assert!(core.tracks[0].mode == Mode::Playing);
    core.options[0].stop_mode = StopMode::Fade;
    core.options[0].fade_ms = 10;
    core.action(Action::Stop(0), &mut OfflinePages);
    tick(&mut core, 40);
    assert!(core.view().running);
    tick(&mut core, 41);
    assert!(!core.view().running);
    core.latency = 16;
    core.action(Action::Trigger(1), &mut OfflinePages);
    tick(&mut core, 100);
    core.action(Action::Stop(1), &mut OfflinePages);
    tick(&mut core, 8);
    assert!(core.view().running && core.tracks[1].finish.is_some());
    tick(&mut core, 9);
    assert!(!core.view().running);
}

#[test]
fn transport_current_replay_restart_stop_and_seek_match_live_output() {
    use crate::replay::{
        EventKind, Writer,
        streaming::{Machine, Source},
    };
    let mut config = AppConfig::new(120, 0, TRACKS);
    for option in &mut config.track_options {
        option.quantize = Quantize::Off;
    }
    config
        .input_fx
        .set_slot_kind(0, 0, crate::config::FxKind::Oscillator);
    config.input_fx.banks[0].slots[0].is_enabled = true;
    let root =
        std::path::PathBuf::from("var").join(format!("transport-replay-{}", crate::session::id()));
    let mut core = RenderCore::new(8000);
    core.configure(&mut Parameters::from_config(&config, 8000));
    let mut writer = Writer::begin(
        root.clone(),
        "transport.json".into(),
        0,
        AudioSnapshot::empty(8000),
        crate::project::data_from_config(&config),
    )
    .unwrap();
    let mut outputs = Vec::new();
    let mut running = Vec::new();
    for frame in 0..6500 {
        let action = match frame {
            0 | 2500 | 5500 => Some(Action::Metronome(true)),
            2000 | 3200 => Some(Action::Metronome(false)),
            3000 => Some(Action::Trigger(0)),
            4000 => Some(Action::Stop(0)),
            4300 | 5300 => Some(Action::All),
            5000 => Some(Action::Clear(0)),
            _ => None,
        };
        if let Some(action) = action {
            core.action(action, &mut OfflinePages);
            writer.event(frame, EventKind::Action(action)).unwrap();
        }
        let input = [(frame as f32 * 0.14).sin() * 0.1; 2];
        writer.audio(frame, &[input]).unwrap();
        outputs.push(core.process(input, &mut OfflinePages));
        running.push(core.view().running);
    }
    writer.finish(6500).unwrap();
    assert!(!running[2100] && running[3300] && !running[4100] && !running[5301]);
    let source = Source::open(&root).unwrap();
    let mut replay = Machine::new(source.clone()).unwrap();
    for expected in &outputs {
        assert_eq!(
            replay.next().unwrap().unwrap().map(f32::to_bits),
            expected.map(f32::to_bits)
        );
    }
    for at in [1999, 2000, 2499, 3200, 4000, 4300, 5000, 5500] {
        let mut seek = Machine::new(source.clone()).unwrap();
        seek.advance_to(at, || false).unwrap();
        for expected in &outputs[at as usize..at as usize + 32] {
            assert_eq!(
                seek.next().unwrap().unwrap().map(f32::to_bits),
                expected.map(f32::to_bits)
            );
        }
    }
    drop(replay);
    drop(source);
    let root = std::fs::canonicalize(root).unwrap();
    assert_eq!(
        root.parent(),
        Some(std::fs::canonicalize("var").unwrap().as_path())
    );
    std::fs::remove_dir_all(root).unwrap();
}
