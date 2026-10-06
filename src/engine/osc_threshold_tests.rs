use super::*;
use crate::{
    config::{AppConfig, FxKind, osc_configs::SampleCapture, track_options::InputRouting},
    presets::{self, FxTarget},
    project,
};
use std::sync::{Arc, atomic::Ordering};

fn osc(c: &mut AppConfig) -> &mut crate::config::osc_configs::OscillatorConfigs {
    c.input_fx.banks[0].slots[0]
        .fx
        .as_mut()
        .unwrap()
        .as_osc_mut()
        .unwrap()
}
fn fixture() -> AppConfig {
    let mut c = AppConfig::new(120, 0, 5);
    c.input_fx.set_slot_kind(0, 0, FxKind::Oscillator);
    c.input_fx.banks[0].slots[0].is_enabled = true;
    let o = osc(&mut c);
    o.input_gate = true;
    o.dry_level = 0.0;
    o.note.push();
    c
}
#[test]
fn osc_threshold_defaults_migrate_shared_values_and_save_independently() {
    let mut c = fixture();
    assert_eq!(
        (
            osc(&mut c).gate_threshold.value,
            osc(&mut c).capture_threshold.value
        ),
        (10, 10)
    );
    let initial = serde_json::to_value(project::data_from_config(&c)).unwrap();
    for (old, gate, capture, expected) in [
        (Some(37), None, None, (37, 37)),
        (None, None, None, (10, 10)),
        (Some(37), Some(5), None, (5, 37)),
        (Some(37), None, Some(90), (37, 90)),
        (Some(37), Some(125), Some(200), (100, 100)),
    ] {
        let mut json = initial.clone();
        let obj = json["input_fx"]["banks"][0]["slots"][0]["osc"]
            .as_object_mut()
            .unwrap();
        for (name, value) in [
            ("threshold", old),
            ("gate_threshold", gate),
            ("capture_threshold", capture),
        ] {
            obj.remove(name);
            if let Some(value) = value {
                obj.insert(name.into(), value.into());
            }
        }
        project::apply_data_to_config(&mut c, serde_json::from_value(json).unwrap());
        assert_eq!(
            (
                osc(&mut c).gate_threshold.value,
                osc(&mut c).capture_threshold.value
            ),
            expected
        );
    }
    osc(&mut c).gate_threshold.value = 13;
    osc(&mut c).capture_threshold.value = 71;
    let json = serde_json::to_value(project::data_from_config(&c)).unwrap();
    let data = &json["input_fx"]["banks"][0]["slots"][0]["osc"];
    assert!(data.get("threshold").is_none());
    assert_eq!(data["gate_threshold"], 13);
    assert_eq!(data["capture_threshold"], 71);
    let mut copy = fixture();
    project::apply_data_to_config(&mut copy, serde_json::from_value(json).unwrap());
    assert_eq!(
        (
            osc(&mut copy).gate_threshold.value,
            osc(&mut copy).capture_threshold.value
        ),
        (13, 71)
    );
    let target = FxTarget::Input { bank: 0, slot: 0 };
    presets::decode(&mut copy, target, &presets::encode(&c, target).unwrap()).unwrap();
    assert_eq!(
        (
            osc(&mut copy).gate_threshold.value,
            osc(&mut copy).capture_threshold.value
        ),
        (13, 71)
    );
    c.input_fx.set_slot_kind(0, 0, FxKind::MyDelay);
    if let Some(InputFx::MyDelay(d)) = &mut c.input_fx.banks[0].slots[0].fx {
        d.threshold.value = 43;
    }
    project::apply_data_to_config(&mut copy, project::data_from_config(&c));
    assert_eq!(
        (
            osc(&mut copy).gate_threshold.value,
            osc(&mut copy).capture_threshold.value
        ),
        (43, 43)
    );
}

#[test]
fn osc_threshold_gate_and_capture_are_independent_for_both_routes_without_allocations() {
    for routing in [InputRouting::Legacy, InputRouting::Serial] {
        for (gate, capture, gate_on, audible, captured) in [
            (10, 80, true, true, false),
            (80, 10, true, false, true),
            (80, 80, false, true, false),
        ] {
            let mut c = fixture();
            let o = osc(&mut c);
            o.gate_threshold.value = gate;
            o.capture_threshold.value = capture;
            o.input_gate = gate_on;
            let mailbox = Arc::new(SampleCapture::new(20));
            o.capture = Some(mailbox.clone());
            let mut engine = InputFxEngine::new(8000.0);
            engine.swap_runtime(InputFxRuntime::from_config(&c.input_fx));
            engine.set_routing(routing);
            engine.set_clock(true, 120);
            let mut peak = 0.0f32;
            let allocations = crate::test_alloc::count(|| {
                for n in 0..600 {
                    peak = peak.max(
                        engine
                            .process_frame(n as f64 / 8000.0, 0.2, 0.2, &[])
                            .0
                            .abs(),
                    );
                }
            });
            assert_eq!(allocations, 0);
            assert_eq!(
                peak > 0.05,
                audible,
                "gate={gate}, capture={capture}, peak={peak}"
            );
            assert_eq!(mailbox.completed().is_some(), captured);
            if !captured {
                assert_eq!(mailbox.state.load(Ordering::Acquire), 0);
            }
        }
    }
}

#[test]
fn osc_threshold_armed_capture_obeys_only_capture_control_and_runs_while_bypassed() {
    let mut c = fixture();
    c.input_fx.banks[0].slots[0].is_enabled = false;
    let o = osc(&mut c);
    o.gate_threshold.value = 0;
    o.capture_threshold.value = 80;
    let mailbox = Arc::new(SampleCapture::new(20));
    o.capture = Some(mailbox.clone());
    let mut engine = InputFxEngine::new(8000.0);
    engine.swap_runtime(InputFxRuntime::from_config(&c.input_fx));
    for n in 0..200 {
        engine.process_frame(n as f64 / 8000.0, 0.2, 0.2, &[]);
    }
    assert_eq!(mailbox.state.load(Ordering::Acquire), 0);
    osc(&mut c).capture_threshold.value = 10;
    osc(&mut c).gate_threshold.value = 100;
    engine.swap_runtime(InputFxRuntime::from_config(&c.input_fx));
    for n in 200..500 {
        engine.process_frame(n as f64 / 8000.0, 0.2, 0.2, &[]);
    }
    assert_eq!(mailbox.state.load(Ordering::Acquire), 2);
    assert_eq!(mailbox.frames.load(Ordering::Relaxed), 160);
    assert!(
        mailbox.samples[..160]
            .iter()
            .all(|v| f32::from_bits(v.load(Ordering::Relaxed)) == 0.2)
    );
}

#[test]
fn osc_threshold_private_auditions_bypass_note_gate_without_arming_capture() {
    use crate::engine::{
        audition::{Audition, AuditionParameters},
        core::RenderCore,
    };
    let mut c = fixture();
    let o = osc(&mut c);
    o.gate_threshold.value = 100;
    o.capture_threshold.value = 23;
    let mailbox = Arc::new(SampleCapture::new(20));
    o.capture = Some(mailbox.clone());
    let target = FxTarget::Input { bank: 0, slot: 0 };
    for params in [
        AuditionParameters::new(&c, target, 0).unwrap(),
        AuditionParameters::candidate(&c, target, 0).unwrap(),
        AuditionParameters::single_note(&c, target, NoteOct::from_pitch_index(48), 100).unwrap(),
    ] {
        let r = match &params {
            AuditionParameters::Input { runtime, .. } => {
                runtime.banks[0].slots[0].osc.as_ref().unwrap()
            }
            AuditionParameters::Note { runtime, .. } => runtime.as_ref().unwrap(),
            _ => panic!(),
        };
        assert_eq!(r.gate_threshold, 0.0);
        assert_eq!(r.capture_threshold, 0.23);
        assert!(!r.poly.input_gate && r.poly.capture.is_none());
        let mut audition = Audition::new(params, 8000);
        let core = RenderCore::new(8000);
        let peak = (0..600)
            .map(|_| audition.next([0.0; 2], &core)[0].abs())
            .fold(0.0, f32::max);
        assert!(peak > 0.05);
    }
    assert_eq!(mailbox.state.load(Ordering::Acquire), 0);
    assert_eq!(
        (
            osc(&mut c).gate_threshold.value,
            osc(&mut c).capture_threshold.value
        ),
        (100, 23)
    );
}

#[test]
fn osc_threshold_replay_config_and_seek_keep_separate_controls_sample_exactly() {
    use crate::{
        engine::{
            core::{Action, AudioSnapshot, Parameters, RenderCore},
            loop_audio::OfflinePages,
        },
        replay::{
            EventKind, Writer,
            streaming::{Machine, Source},
        },
    };
    let mut c = fixture();
    osc(&mut c).gate_threshold.value = 80;
    osc(&mut c).capture_threshold.value = 10;
    let mut core = RenderCore::new(8000);
    core.configure(&mut Parameters::from_config(&c, 8000));
    let root = std::path::PathBuf::from("var")
        .join(format!("osc-threshold-replay-{}", crate::session::id()));
    let mut snapshot = AudioSnapshot::empty(8000);
    core.snapshot(&mut snapshot, &mut OfflinePages);
    let mut writer = Writer::begin(
        root.clone(),
        "threshold.json".into(),
        0,
        snapshot,
        project::data_from_config(&c),
    )
    .unwrap();
    let mut live = Vec::new();
    for n in 0..2400u64 {
        if n == 0 {
            core.action(Action::Metronome(true), &mut OfflinePages);
            writer
                .event(n, EventKind::Action(Action::Metronome(true)))
                .unwrap();
        }
        if n == 400 || n == 1200 {
            osc(&mut c).gate_threshold.value = if n == 400 { 10 } else { 90 };
            osc(&mut c).capture_threshold.value = if n == 400 { 75 } else { 5 };
            core.configure(&mut Parameters::from_config(&c, 8000));
            writer
                .event(n, EventKind::Config(project::data_from_config(&c)))
                .unwrap();
        }
        let input = [0.2; 2];
        writer.audio(n, &[input]).unwrap();
        live.push(core.process(input, &mut OfflinePages));
    }
    writer.finish(2400).unwrap();
    let source = Source::open(&root).unwrap();
    for target in [0, 399, 400, 800, 1199, 1200, 1600] {
        let mut replay = Machine::new(source.clone()).unwrap();
        replay.advance_to(target, || false).unwrap();
        for expected in &live[target as usize..target as usize + 128] {
            assert_eq!(
                replay.next().unwrap().unwrap().map(f32::to_bits),
                expected.map(f32::to_bits)
            );
        }
        let current = &replay.data.input_fx.banks[0].slots[0].osc.as_ref().unwrap();
        let expected = if target + 128 < 400 {
            (80, 10)
        } else if target + 128 < 1200 {
            (10, 75)
        } else {
            (90, 5)
        };
        assert_eq!(
            (current.gate_threshold, current.capture_threshold),
            (Some(expected.0), Some(expected.1))
        );
    }
    drop(source);
    drop(core);
    let path = root.canonicalize().unwrap();
    assert!(path.starts_with(std::path::Path::new("var").canonicalize().unwrap()));
    std::fs::remove_dir_all(path).unwrap();
}
