use super::*;
use crate::{
    config::{AppConfig, FxKind, InputFx, TrackFx, TrackFxKind},
    presets::{self, FxTarget},
    project,
};
fn set(c: &mut EnvelopeConfigs, t: [f32; 4]) {
    c.attack_ms.value = t[0];
    c.hold_ms.value = t[1];
    c.decay_ms.value = t[2];
    c.release_ms.value = t[3];
}
fn times(p: AhdsrParams) -> [f32; 4] {
    [p.attack_ms, p.hold_ms, p.decay_ms, p.release_ms]
}
#[test]
fn fractional_envelope_all_owners_reach_runtime_project_and_presets() {
    let mut c = AppConfig::new(120, 0, 5);
    c.input_fx.set_slot_kind(0, 0, FxKind::Oscillator);
    c.input_fx.set_slot_kind(0, 1, FxKind::MyDelay);
    c.track_fx.set_slot_kind(0, 0, TrackFxKind::Filter);
    let values = [
        [1.3, 2.2, 3.4, 4.5],
        [5.6, 6.7, 7.8, 8.9],
        [9.1, 10.2, 11.3, 12.4],
        [13.5, 14.6, 15.7, 16.8],
        [17.9, 18.1, 19.2, 20.3],
    ];
    if let Some(InputFx::Oscillator(o)) = &mut c.input_fx.banks[0].slots[0].fx {
        set(&mut o.envelope, values[0]);
        set(&mut o.osc_filter_env, values[1]);
    }
    if let Some(InputFx::MyDelay(o)) = &mut c.input_fx.banks[0].slots[1].fx {
        set(&mut o.audio_env, values[2]);
        set(&mut o.filter_env, values[3]);
    }
    if let Some(TrackFx::Filter(f)) = c.track_fx.slot_fx_mut(0, 0) {
        set(&mut f.env, values[4]);
    }
    let input = crate::engine::input_fx::InputFxRuntime::from_config(&c.input_fx);
    let osc = input.banks[0].slots[0].osc.as_ref().unwrap();
    assert_eq!(times(osc.envelope), values[0]);
    assert_eq!(times(osc.osc_filter_envelope), values[1]);
    let delay = input.banks[0].slots[1].my_delay.as_ref().unwrap();
    assert_eq!(times(delay.audio_env), values[2]);
    assert_eq!(times(delay.filter_env), values[3]);
    let track = crate::engine::track_fx::TrackFxRuntime::from_config(&c.track_fx);
    assert_eq!(
        times(track.banks[0].slots[0].filter.as_ref().unwrap().envelope),
        values[4]
    );
    let json = serde_json::to_vec(&project::data_from_config(&c)).unwrap();
    let mut copy = AppConfig::new(120, 0, 5);
    project::apply_data_to_config(&mut copy, serde_json::from_slice(&json).unwrap());
    for (slot, a, b) in [(0, 0, 1), (1, 2, 3)] {
        let Some(InputFx::Oscillator(o)) = &copy.input_fx.banks[0].slots[slot].fx else {
            panic!("Legacy source must migrate without truncating envelopes")
        };
        assert_eq!(times(from_config(&o.envelope)), values[a]);
        assert_eq!(times(from_config(&o.osc_filter_env)), values[b]);
    }
    if let Some(TrackFx::Filter(f)) = copy.track_fx.slot_fx_mut(0, 0) {
        assert_eq!(times(from_config(&f.env)), values[4]);
    } else {
        panic!()
    }
    for target in [
        FxTarget::Input { bank: 0, slot: 0 },
        FxTarget::Track { bank: 0, slot: 0 },
    ] {
        let preset = presets::encode(&c, target).unwrap();
        presets::decode(&mut copy, target, &preset).unwrap();
    }
    let Some(InputFx::Oscillator(o)) = &copy.input_fx.banks[0].slots[0].fx else {
        panic!()
    };
    assert_eq!(times(from_config(&o.envelope)), values[0]);
    assert_eq!(times(from_config(&o.osc_filter_env)), values[1]);
}
#[test]
fn fractional_envelope_old_integer_json_keeps_old_sound_and_range_sanitizing_is_finite() {
    let old: project::EnvelopeData = serde_json::from_str(
        r#"{"attack_ms":20,"hold_ms":40,"decay_ms":180,"sustain_pct":70,"release_ms":120}"#,
    )
    .unwrap();
    let mut c = AppConfig::new(120, 0, 5);
    c.input_fx.set_slot_kind(0, 0, FxKind::Oscillator);
    let mut data = project::data_from_config(&c);
    data.input_fx.banks[0].slots[0]
        .osc
        .as_mut()
        .unwrap()
        .envelope = old;
    project::apply_data_to_config(&mut c, data);
    let Some(InputFx::Oscillator(o)) = &mut c.input_fx.banks[0].slots[0].fx else {
        panic!()
    };
    let runtime = from_config(&o.envelope);
    assert_eq!(times(runtime), [20.0, 40.0, 180.0, 120.0]);
    let expected = AhdsrParams {
        attack_ms: 20.0,
        hold_ms: 40.0,
        decay_ms: 180.0,
        release_ms: 120.0,
        sustain_level: 0.7,
        start_level: 0.0,
        tension_attack: 1.0,
        tension_decay: 1.0,
        tension_release: 1.0,
    };
    let (mut a, mut b) = (
        crate::dsp::envelope::AhdsrState::new(),
        crate::dsp::envelope::AhdsrState::new(),
    );
    for n in 0..30000 {
        let on = n < 10000 || n >= 20000;
        assert_eq!(
            a.next(on, false, runtime, 1.0 / 48000.0).to_bits(),
            b.next(on, false, expected, 1.0 / 48000.0).to_bits()
        );
    }
    set(&mut o.envelope, [f32::NAN, -1.0, f32::INFINITY, -0.1]);
    assert_eq!(times(from_config(&o.envelope)), [0.0, 0.0, 0.0, 1.0]);
    set(&mut o.envelope, [99999.0; 4]);
    assert_eq!(
        times(from_config(&o.envelope)),
        [2000.0, 5000.0, 10000.0, 5000.0]
    );
}
#[test]
fn fractional_envelope_dsp_uses_sub_millisecond_stage_durations_without_allocating() {
    let mut c = EnvelopeConfigs::new();
    set(&mut c, [1.5, 0.4, 0.6, 1.1]);
    c.sustain_pct.value = 50;
    let p = from_config(&c);
    let mut envelope = crate::dsp::envelope::AhdsrState::new();
    let allocations = crate::test_alloc::count(|| {
        let mut value = 0.0;
        for n in 1..=144 {
            value = envelope.next(true, false, p, 1.0 / 48000.0);
            if n == 36 {
                assert!((value - 0.5).abs() < 0.00001);
            }
            if n == 90 {
                assert!((value - 1.0).abs() < 0.00001);
            }
            if n == 96 {
                assert!((0.90..0.93).contains(&value));
            }
        }
        assert_eq!(value, 0.5);
        for _ in 0..48 {
            value = envelope.next(false, false, p, 1.0 / 48000.0);
        }
        assert!(
            (0.04..0.05).contains(&value),
            "1.1ms release must still be sounding after1ms"
        );
        for _ in 0..6 {
            value = envelope.next(false, false, p, 1.0 / 48000.0);
        }
        assert_eq!(value, 0.0);
    });
    assert_eq!(allocations, 0);
}
#[test]
fn fractional_envelope_replay_config_events_and_seek_remain_exact() {
    use crate::{
        config::{note_configs::NoteOct, sequence_edit::NoteEvent},
        engine::{
            core::{Action, AudioSnapshot, Parameters, RenderCore},
            loop_audio::OfflinePages,
        },
        replay::{
            EventKind, Writer,
            streaming::{Machine, Source},
        },
    };
    let mut c = AppConfig::new(120, 0, 5);
    c.input_fx.set_slot_kind(0, 0, FxKind::Oscillator);
    c.input_fx.banks[0].slots[0].is_enabled = true;
    if let Some(InputFx::Oscillator(o)) = &mut c.input_fx.banks[0].slots[0].fx {
        set(&mut o.envelope, [0.1, 0.2, 0.3, 1.4]);
        set(&mut o.osc_filter_env, [1.5, 2.6, 3.7, 4.8]);
        o.osc_filter.mix.value = 40;
        o.note.replace_events(
            1920,
            &[
                NoteEvent::new(0, 48, NoteOct::from_pitch_index(57)),
                NoteEvent::new(96, 48, NoteOct::from_pitch_index(60)),
            ],
        );
    }
    let mut core = RenderCore::new(48000);
    core.configure(&mut Parameters::from_config(&c, 48000));
    let root = std::path::PathBuf::from("var").join(format!(
        "fractional-envelope-replay-{}",
        crate::session::id()
    ));
    let mut snapshot = AudioSnapshot::empty(48000);
    core.snapshot(&mut snapshot, &mut OfflinePages);
    let mut writer = Writer::begin(
        root.clone(),
        "envelope.json".into(),
        0,
        snapshot,
        project::data_from_config(&c),
    )
    .unwrap();
    let mut live = Vec::new();
    for n in 0..6000u64 {
        if n == 0 {
            core.action(Action::Metronome(true), &mut OfflinePages);
            writer
                .event(n, EventKind::Action(Action::Metronome(true)))
                .unwrap();
        }
        if n == 1800 {
            if let Some(InputFx::Oscillator(o)) = &mut c.input_fx.banks[0].slots[0].fx {
                set(&mut o.envelope, [1.7, 2.8, 3.9, 4.1]);
            }
            core.configure(&mut Parameters::from_config(&c, 48000));
            writer
                .event(n, EventKind::Config(project::data_from_config(&c)))
                .unwrap();
        }
        writer.audio(n, &[[0.0; 2]]).unwrap();
        live.push(core.process([0.0; 2], &mut OfflinePages));
    }
    writer.finish(6000).unwrap();
    let source = Source::open(&root).unwrap();
    let mut replay = Machine::new(source.clone()).unwrap();
    for expected in &live {
        assert_eq!(
            replay.next().unwrap().unwrap().map(f32::to_bits),
            expected.map(f32::to_bits)
        );
    }
    for target in [0, 5, 70, 1200, 1799, 1800, 2400, 2450, 3600] {
        let mut seek = Machine::new(source.clone()).unwrap();
        seek.advance_to(target, || false).unwrap();
        for expected in &live[target as usize..target as usize + 64] {
            assert_eq!(
                seek.next().unwrap().unwrap().map(f32::to_bits),
                expected.map(f32::to_bits)
            );
        }
    }
    drop(replay);
    drop(source);
    drop(core);
    let path = root.canonicalize().unwrap();
    assert!(path.starts_with(std::path::Path::new("var").canonicalize().unwrap()));
    std::fs::remove_dir_all(path).unwrap();
}
