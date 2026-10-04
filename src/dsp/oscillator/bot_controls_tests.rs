use super::*;
use crate::config::{OscillatorConfigs, sequence_edit::NoteEvent};
fn instrument(wave: Waveform) -> OscillatorConfigs {
    let mut c = OscillatorConfigs::new();
    c.waveform.value = wave;
    c.voices = 1;
    c.note.replace_events(
        15360,
        &[NoteEvent::new(0, 15360, NoteOct::from_pitch_index(57))],
    );
    c
}
fn params(sr: f32, wave: Waveform) -> OscillatorFxParams {
    let mut p = super::poly_tests::params(sr, wave);
    p.envelope.attack_ms = 0.0;
    p.envelope.hold_ms = 0.0;
    p.envelope.decay_ms = 0.0;
    p.envelope.sustain_level = 1.0;
    p
}
fn render(c: &OscillatorConfigs, sr: f32, seconds: f32, input: f32) -> Vec<f32> {
    let r = PolyOscRuntime::from_config(c);
    let mut state = PolyOscState::new();
    let mut p = params(sr, c.waveform.value);
    p.input_level = input;
    (0..(sr * seconds) as usize)
        .map(|n| process_poly_sample(&mut state, &r, p, n as f64 / sr as f64, 120, true))
        .collect()
}
fn amplitude(data: &[f32], hz: f64, sr: f64) -> f64 {
    let (sin, cos) = (std::f64::consts::TAU * hz / sr).sin_cos();
    let (mut re, mut im) = (1.0, 0.0);
    let (mut a, mut b) = (0.0, 0.0);
    for value in data {
        a += *value as f64 * re;
        b += *value as f64 * im;
        (re, im) = (re * cos - im * sin, re * sin + im * cos);
    }
    2.0 * a.hypot(b) / data.len() as f64
}
#[test]
fn osc_bot_rect_and_vintage_have_distinct_measured_harmonics() {
    let rect = render(&instrument(Waveform::Rect), 48000.0, 1.2, 0.0);
    let square = render(&instrument(Waveform::Square), 48000.0, 1.2, 0.0);
    let vintage = render(&instrument(Waveform::VintageSaw), 48000.0, 1.2, 0.0);
    let saw = render(&instrument(Waveform::Saw), 48000.0, 1.2, 0.0);
    let ratio = |data: &Vec<f32>, harmonic: f64| {
        amplitude(&data[9600..], 440.0 * harmonic, 48000.0)
            / amplitude(&data[9600..], 440.0, 48000.0)
    };
    assert!((0.65..0.76).contains(&ratio(&rect, 2.0)));
    assert!(ratio(&rect, 4.0) < 0.015);
    assert!(ratio(&square, 2.0) < 0.015);
    assert!(ratio(&vintage, 8.0) < ratio(&saw, 8.0) * 0.60);
    assert!(ratio(&vintage, 8.0) > 0.025);
    for data in [&rect, &vintage] {
        let mean = data[9600..].iter().sum::<f32>() / (data.len() - 9600) as f32;
        assert!(mean.abs() < 0.001);
    }
}
#[test]
fn osc_bot_detune_uses_two_independent_pitch_components() {
    for sr in [44100.0, 48000.0, 96000.0] {
        let data = render(&instrument(Waveform::DetuneSaw), sr, 3.2, 0.0);
        let slice = &data[(sr * 0.2) as usize..];
        let low = amplitude(slice, 440.0 * super::super::DETUNE_DOWN, sr as f64);
        let high = amplitude(slice, 440.0 * super::super::DETUNE_UP, sr as f64);
        let middle = amplitude(slice, 440.0, sr as f64);
        assert!(low > 0.08 && high > 0.08, "{sr}: {low}/{high}");
        assert!(
            middle < (low + high) * 0.25,
            "detune cannot be a renamed single saw: {middle}"
        );
    }
}
#[test]
fn osc_bot_input_envelope_is_continuous_and_disabled_mode_is_neutral() {
    let mut c = instrument(Waveform::Sine);
    let plain = render(&c, 48000.0, 0.2, 0.0);
    let plain_loud = render(&c, 48000.0, 0.2, 0.5);
    assert_eq!(plain, plain_loud);
    c.input_mod_sens = Some(0.0);
    for (input, expected) in [(0.0, 0.0), (0.0625, 0.25), (0.125, 0.5), (0.25, 1.0)] {
        let data = render(&c, 48000.0, 0.2, input);
        for (got, reference) in data.iter().zip(&plain) {
            assert!((*got - *reference * expected).abs() < 0.000001);
        }
    }
    let c_low = OscillatorConfigs {
        input_mod_sens: Some(-50.0),
        ..instrument(Waveform::Sine)
    };
    let c_high = OscillatorConfigs {
        input_mod_sens: Some(50.0),
        ..instrument(Waveform::Sine)
    };
    let power = |v: Vec<f32>| v.iter().map(|x| x * x).sum::<f32>();
    assert!(
        power(render(&c_high, 48000.0, 0.2, 0.01))
            > power(render(&c_low, 48000.0, 0.2, 0.01)) * 100.0
    );
}
#[test]
fn osc_bot_new_waves_are_silent_when_stopped_and_allocation_free_at_device_rates() {
    for sr in [8000.0, 44100.0, 48000.0, 96000.0, 192000.0] {
        for wave in [Waveform::Rect, Waveform::DetuneSaw, Waveform::VintageSaw] {
            let mut c = instrument(wave);
            c.voices = 16;
            c.input_mod_sens = Some(15.0);
            c.note.replace_events(
                15360,
                &(0..16)
                    .map(|i| NoteEvent::new(0, 15360, NoteOct::from_pitch_index(40 + i * 4)))
                    .collect::<Vec<_>>(),
            );
            let r = PolyOscRuntime::from_config(&c);
            let mut state = PolyOscState::new();
            let mut p = params(sr, wave);
            p.input_level = 0.25;
            let allocations = crate::test_alloc::count(|| {
                for n in 0..64 {
                    assert_eq!(
                        process_poly_sample(&mut state, &r, p, n as f64 / sr as f64, 120, false),
                        0.0
                    );
                }
                for n in 0..4000 {
                    let y = process_poly_sample(&mut state, &r, p, n as f64 / sr as f64, 120, true);
                    assert!(y.is_finite() && y.abs() < 16.0);
                }
            });
            assert_eq!(allocations, 0);
        }
    }
}
#[test]
fn osc_bot_dry_and_wet_controls_work_in_both_routes_without_collapsing_stereo() {
    use crate::{
        config::{AppConfig, FxKind, InputFx, track_options::InputRouting},
        engine::input_fx::{InputFxEngine, InputFxRuntime},
    };
    for route in [InputRouting::Serial, InputRouting::Legacy] {
        let mut c = AppConfig::new(120, 0, 5);
        for (slot, dry) in [(0, 0.5), (1, 0.25)] {
            c.input_fx.set_slot_kind(0, slot, FxKind::Oscillator);
            c.input_fx.banks[0].slots[slot].is_enabled = true;
            if let Some(InputFx::Oscillator(o)) = &mut c.input_fx.banks[0].slots[slot].fx {
                o.dry_level = dry;
                o.level.value = 0;
            }
        }
        let mut engine = InputFxEngine::new(8000.0);
        engine.set_routing(route);
        drop(engine.swap_runtime(InputFxRuntime::from_config(&c.input_fx)));
        assert_eq!(engine.process_frame(0.0, 0.4, -0.2, &[]), (0.05, -0.025));
        c.input_fx.set_slot_kind(0, 1, FxKind::None);
        if let Some(InputFx::Oscillator(o)) = &mut c.input_fx.banks[0].slots[0].fx {
            *o = instrument(Waveform::Rect);
            o.dry_level = 0.0;
        }
        let mut a = InputFxEngine::new(8000.0);
        a.set_routing(route);
        a.set_clock(true, 120);
        drop(a.swap_runtime(InputFxRuntime::from_config(&c.input_fx)));
        let mut b = InputFxEngine::new(8000.0);
        b.set_routing(route);
        b.set_clock(true, 120);
        drop(b.swap_runtime(InputFxRuntime::from_config(&c.input_fx)));
        let allocations = crate::test_alloc::count(|| {
            for n in 0..2000 {
                let time = n as f64 / 8000.0;
                assert_eq!(
                    a.process_frame(time, 0.2, -0.4, &[]),
                    b.process_frame(time, 0.0, 0.0, &[])
                );
            }
        });
        assert_eq!(allocations, 0);
    }
}
#[test]
fn osc_bot_project_preset_and_legacy_json_preserve_parameters_and_phrase() {
    use crate::{
        config::{AppConfig, FxKind, InputFx},
        presets::{self, FxTarget},
        project,
    };
    let target = FxTarget::Input { bank: 0, slot: 0 };
    for wave in [Waveform::Rect, Waveform::DetuneSaw, Waveform::VintageSaw] {
        let mut c = AppConfig::new(120, 0, 5);
        c.input_fx.set_slot_kind(0, 0, FxKind::Oscillator);
        if let Some(InputFx::Oscillator(o)) = &mut c.input_fx.banks[0].slots[0].fx {
            *o = instrument(wave);
            o.input_mod_sens = Some(24.5);
            o.dry_level = 0.37;
        }
        let json = serde_json::to_value(project::data_from_config(&c)).unwrap();
        let mut copy = AppConfig::new(120, 0, 5);
        project::apply_data_to_config(&mut copy, serde_json::from_value(json.clone()).unwrap());
        let Some(InputFx::Oscillator(o)) = &copy.input_fx.banks[0].slots[0].fx else {
            panic!()
        };
        assert!(o.waveform.value == wave);
        assert_eq!((o.dry_level, o.input_mod_sens), (0.37, Some(24.5)));
        let phrase = presets::clip(&copy, target).unwrap();
        let text = presets::encode(&c, target).unwrap();
        presets::decode(&mut copy, target, &text).unwrap();
        assert_eq!(presets::clip(&copy, target).unwrap(), phrase);
        let mut old = json;
        let fields = old["input_fx"]["banks"][0]["slots"][0]["osc"]
            .as_object_mut()
            .unwrap();
        fields.remove("dry_level");
        fields.remove("input_mod_sens");
        project::apply_data_to_config(&mut copy, serde_json::from_value(old).unwrap());
        let Some(InputFx::Oscillator(o)) = &copy.input_fx.banks[0].slots[0].fx else {
            panic!()
        };
        assert_eq!((o.dry_level, o.input_mod_sens), (1.0, None));
    }
}

#[test]
fn osc_bot_private_audition_does_not_require_live_envelope_input() {
    use crate::{
        config::{AppConfig, FxKind, InputFx},
        engine::audition::AuditionParameters,
        presets::FxTarget,
    };
    let mut c = AppConfig::new(120, 0, 5);
    c.input_fx.set_slot_kind(0, 0, FxKind::Oscillator);
    if let Some(InputFx::Oscillator(o)) = &mut c.input_fx.banks[0].slots[0].fx {
        *o = instrument(Waveform::Rect);
        o.input_mod_sens = Some(20.0);
    }
    let target = FxTarget::Input { bank: 0, slot: 0 };
    let AuditionParameters::Note { runtime, .. } =
        AuditionParameters::single_note(&c, target, NoteOct::from_pitch_index(57), 100).unwrap()
    else {
        panic!()
    };
    assert!(runtime.unwrap().poly.input_mod_gain.is_none());
    let AuditionParameters::Input { runtime, .. } = AuditionParameters::new(&c, target, 0).unwrap()
    else {
        panic!()
    };
    assert!(
        runtime.banks[0].slots[0]
            .osc
            .as_ref()
            .unwrap()
            .poly
            .input_mod_gain
            .is_none()
    );
    let Some(InputFx::Oscillator(o)) = &c.input_fx.banks[0].slots[0].fx else {
        panic!()
    };
    assert_eq!(o.input_mod_sens, Some(20.0));
}
#[test]
fn osc_bot_wave_and_envelope_changes_replay_and_seek_sample_exactly() {
    use crate::{
        config::{AppConfig, FxKind, InputFx},
        engine::{
            core::{Action, AudioSnapshot, Parameters, RenderCore},
            loop_audio::OfflinePages,
        },
        replay::{
            EventKind, Writer,
            streaming::{Machine, Source},
        },
    };
    let root =
        std::path::PathBuf::from("var").join(format!("osc-bot-replay-{}", crate::session::id()));
    let mut c = AppConfig::new(120, 0, 5);
    c.input_fx.set_slot_kind(0, 0, FxKind::Oscillator);
    c.input_fx.banks[0].slots[0].is_enabled = true;
    if let Some(InputFx::Oscillator(o)) = &mut c.input_fx.banks[0].slots[0].fx {
        *o = instrument(Waveform::Saw);
        o.envelope.sustain_pct.value = 100;
        o.envelope.decay_ms.value = 0;
    }
    let mut core = RenderCore::new(8000);
    core.configure(&mut Parameters::from_config(&c, 8000));
    let mut snapshot = AudioSnapshot::empty(8000);
    core.snapshot(&mut snapshot, &mut OfflinePages);
    let mut writer = Writer::begin(
        root.clone(),
        "osc-bot.json".into(),
        0,
        snapshot,
        crate::project::data_from_config(&c),
    )
    .unwrap();
    let mut live = Vec::new();
    for frame in 0..5500 {
        if frame == 0 {
            core.action(Action::Metronome(true), &mut OfflinePages);
            writer
                .event(frame, EventKind::Action(Action::Metronome(true)))
                .unwrap();
        }
        if matches!(frame, 1000 | 2000 | 3000 | 4000) {
            if let Some(InputFx::Oscillator(o)) = &mut c.input_fx.banks[0].slots[0].fx {
                o.waveform.value = match frame {
                    1000 => Waveform::Rect,
                    2000 => Waveform::DetuneSaw,
                    _ => Waveform::VintageSaw,
                };
                o.dry_level = 0.1 * (frame / 1000) as f32;
                o.input_mod_sens = if frame == 4000 {
                    None
                } else {
                    Some((frame / 1000) as f32 * 15.0 - 30.0)
                };
            }
            core.configure(&mut Parameters::from_config(&c, 8000));
            writer
                .event(
                    frame,
                    EventKind::Config(crate::project::data_from_config(&c)),
                )
                .unwrap();
        }
        let input = [
            (frame as f32 * 0.073).sin() * 0.04,
            (frame as f32 * 0.103).sin() * 0.015,
        ];
        writer.audio(frame, &[input]).unwrap();
        live.push(core.process(input, &mut OfflinePages));
    }
    writer.finish(5500).unwrap();
    let source = Source::open(&root).unwrap();
    let mut replay = Machine::new(source.clone()).unwrap();
    for expected in &live {
        assert_eq!(
            replay.next().unwrap().unwrap().map(f32::to_bits),
            expected.map(f32::to_bits)
        );
    }
    for target in [999, 1000, 1999, 2000, 2999, 3000, 3999, 4000, 4700] {
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
