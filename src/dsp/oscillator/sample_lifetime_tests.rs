use super::*;
use crate::config::{OscillatorConfigs, sequence_edit::NoteEvent};
fn asset(index: usize, mode: SampleMode) -> Arc<SampleAsset> {
    let frames = (0..if mode == SampleMode::Sampler {
        1024
    } else {
        32
    })
        .map(|i| {
            (std::f32::consts::TAU * i as f32 / 32.0).sin()
                * (0.15 + index as f32 * 0.001)
                * if index % 2 == 0 { 1.0 } else { -1.0 }
        })
        .collect();
    Arc::new(SampleAsset::new(format!("version {index}"), 8000, frames))
}
fn config(sample: Arc<SampleAsset>, mode: SampleMode, notes: &[NoteEvent]) -> OscillatorConfigs {
    let mut c = OscillatorConfigs::new();
    c.waveform.value = Waveform::Sample;
    c.sample = Some(sample);
    c.sample_mode = mode;
    c.sample_loop = true;
    c.sample_root = 48;
    c.voices = 16;
    c.note.replace_events(3840, notes);
    c
}
fn params() -> OscillatorFxParams {
    let mut p = super::poly_tests::params(8000.0, Waveform::Sample);
    p.envelope.attack_ms = 0.0;
    p.envelope.hold_ms = 0.0;
    p.envelope.decay_ms = 0.0;
    p.envelope.sustain_level = 1.0;
    p.envelope.release_ms = 200.0;
    p
}
#[test]
fn sample_generation_old_held_and_release_voices_keep_their_original_pcm_and_tables() {
    for mode in [SampleMode::Sampler, SampleMode::Wavetable] {
        for change in [200usize, 600] {
            let notes = [
                NoteEvent::new(0, 96, NoteOct::from_pitch_index(48)),
                NoteEvent::new(192, 96, NoteOct::from_pitch_index(52)),
            ];
            let mut c = config(asset(0, mode), mode, &notes);
            let mut runtime = PolyOscRuntime::from_config(&c);
            let first = config(c.sample.clone().unwrap(), mode, &notes[..1]);
            let first_runtime = PolyOscRuntime::from_config(&first);
            c.sample = Some(asset(1, mode));
            c.sample_root = 60;
            c.sample_start = 0.125;
            c.sample_end = 0.875;
            let mut next = Some(PolyOscRuntime::from_config(&c));
            c.note.replace_events(3840, &notes[1..]);
            let second_runtime = PolyOscRuntime::from_config(&c);
            let (mut actual, mut a, mut b) = (
                PolyOscState::new(),
                PolyOscState::new(),
                PolyOscState::new(),
            );
            let mut retired = None;
            for frame in 0..3200 {
                let allocations = crate::test_alloc::count(|| {
                    if frame == change {
                        actual.retire_materials(&mut runtime);
                        retired = Some(std::mem::replace(&mut runtime, next.take().unwrap()));
                    }
                    let time = frame as f64 / 8000.0;
                    let got = process_poly_sample(&mut actual, &runtime, params(), time, 120, true);
                    let expected =
                        process_poly_sample(&mut a, &first_runtime, params(), time, 120, true)
                            + process_poly_sample(
                                &mut b,
                                &second_runtime,
                                params(),
                                time,
                                120,
                                true,
                            );
                    assert!(
                        (got - expected).abs() < 0.000002,
                        "mode={mode:?},change={change},frame={frame}: {got}/{expected}"
                    );
                });
                assert_eq!(allocations, 0);
                drop(retired.take());
            }
        }
    }
}
#[test]
fn sample_generation_rapid_replacements_are_bounded_and_last_references_retire_off_thread() {
    let mut notes = Vec::new();
    let mut c = config(asset(0, SampleMode::Sampler), SampleMode::Sampler, &[]);
    let mut runtime = PolyOscRuntime::from_config(&c);
    let mut state = PolyOscState::new();
    let mut weak = Vec::new();
    for generation in 0..64 {
        c.sample = Some(asset(generation, SampleMode::Sampler));
        weak.push(Arc::downgrade(c.sample.as_ref().unwrap()));
        notes.push(NoteEvent::new(
            generation * 24,
            2000,
            NoteOct::from_pitch_index(48 + generation % 12),
        ));
        c.note.replace_events(3840, &notes);
        let mut next = Some(PolyOscRuntime::from_config(&c));
        let mut returned = None;
        let allocations = crate::test_alloc::count(|| {
            state.retire_materials(&mut runtime);
            returned = Some(std::mem::replace(&mut runtime, next.take().unwrap()));
            for frame in generation * 100..(generation + 1) * 100 {
                let y = process_poly_sample(
                    &mut state,
                    &runtime,
                    params(),
                    frame as f64 / 8000.0,
                    120,
                    true,
                );
                assert!(y.is_finite());
            }
        });
        assert_eq!(
            allocations, 0,
            "generation {generation} allocated or freed in callback"
        );
        drop(returned.take()); // Existing worker retirement path.
        assert!(state.materials.iter().filter(|slot| slot.is_some()).count() <= MATERIAL_CAPACITY);
        assert!(weak.iter().filter(|old| old.strong_count() > 0).count() <= MATERIAL_CAPACITY);
    }
    let allocations = crate::test_alloc::count(|| {
        state.reset();
        state.retire_materials(&mut runtime);
    });
    assert_eq!(allocations, 0);
    c.sample = None;
    assert!(
        weak.iter().any(|v| v.strong_count() > 0),
        "The outgoing control payload still owns retired samples"
    );
    drop(runtime);
    assert!(
        weak.iter().all(|v| v.strong_count() == 0),
        "Worker drop must release all finished generations"
    );
}
#[test]
fn sample_generation_removing_oscillator_uses_the_engine_retirement_path() {
    use crate::{
        config::{AppConfig, FxKind, InputFx},
        engine::input_fx::{InputFxEngine, InputFxRuntime},
    };
    let mut c = AppConfig::new(120, 0, 5);
    c.input_fx.set_slot_kind(0, 0, FxKind::Oscillator);
    c.input_fx.banks[0].slots[0].is_enabled = true;
    let sample = asset(9, SampleMode::Sampler);
    let weak = Arc::downgrade(&sample);
    if let Some(InputFx::Oscillator(osc)) = &mut c.input_fx.banks[0].slots[0].fx {
        *osc = config(
            sample,
            SampleMode::Sampler,
            &[NoteEvent::new(0, 960, NoteOct::from_pitch_index(48))],
        );
    }
    let mut engine = InputFxEngine::new(8000.0);
    engine.set_clock(true, 120);
    drop(engine.swap_runtime(InputFxRuntime::from_config(&c.input_fx)));
    for frame in 0..500 {
        engine.process_frame(frame as f64 / 8000.0, 0.0, 0.0, &[]);
    }
    c.input_fx.set_slot_kind(0, 0, FxKind::Filter);
    let mut next = Some(InputFxRuntime::from_config(&c.input_fx));
    let mut returned = None;
    let allocations =
        crate::test_alloc::count(|| returned = Some(engine.swap_runtime(next.take().unwrap())));
    assert_eq!(allocations, 0);
    assert!(weak.strong_count() > 0);
    drop(returned);
    assert_eq!(weak.strong_count(), 0);
}
#[test]
fn sample_generation_replay_and_seek_keep_the_same_release_versions() {
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
    let root = std::path::PathBuf::from("var")
        .join(format!("sample-version-replay-{}", crate::session::id()));
    let mut c = AppConfig::new(120, 0, 5);
    c.input_fx.set_slot_kind(0, 0, FxKind::Oscillator);
    c.input_fx.banks[0].slots[0].is_enabled = true;
    if let Some(InputFx::Oscillator(osc)) = &mut c.input_fx.banks[0].slots[0].fx {
        *osc = config(
            asset(0, SampleMode::Sampler),
            SampleMode::Sampler,
            &[
                NoteEvent::new(0, 96, NoteOct::from_pitch_index(48)),
                NoteEvent::new(192, 96, NoteOct::from_pitch_index(52)),
                NoteEvent::new(384, 96, NoteOct::from_pitch_index(55)),
            ],
        );
        osc.envelope.attack_ms.value = 0;
        osc.envelope.hold_ms.value = 0;
        osc.envelope.decay_ms.value = 0;
        osc.envelope.sustain_pct.value = 100;
        osc.envelope.release_ms.value = 300;
        osc.osc_filter.mix.value = 0;
    }
    let mut core = RenderCore::new(8000);
    core.configure(&mut Parameters::from_config(&c, 8000));
    let mut initial = AudioSnapshot::empty(8000);
    core.snapshot(&mut initial, &mut OfflinePages);
    let mut writer = Writer::begin(
        root.clone(),
        "sample-versions.json".into(),
        0,
        initial,
        crate::project::data_from_config(&c),
    )
    .unwrap();
    let mut live = Vec::new();
    for frame in 0..4800 {
        if frame == 0 {
            core.action(Action::Metronome(true), &mut OfflinePages);
            writer
                .event(frame, EventKind::Action(Action::Metronome(true)))
                .unwrap();
        }
        if matches!(frame, 600 | 1000 | 1800) {
            if let Some(InputFx::Oscillator(osc)) = &mut c.input_fx.banks[0].slots[0].fx {
                osc.sample = Some(asset((frame / 100) as usize, SampleMode::Sampler));
                osc.sample_root = if frame == 1000 { 60 } else { 48 };
            }
            core.configure(&mut Parameters::from_config(&c, 8000));
            writer
                .event(
                    frame,
                    EventKind::Config(crate::project::data_from_config(&c)),
                )
                .unwrap();
        }
        writer.audio(frame, &[[0.0; 2]]).unwrap();
        live.push(core.process([0.0; 2], &mut OfflinePages));
    }
    writer.finish(4800).unwrap();
    let source = Source::open(&root).unwrap();
    let mut replay = Machine::new(source.clone()).unwrap();
    for (frame, expected) in live.iter().enumerate() {
        assert_eq!(
            replay.next().unwrap().unwrap().map(f32::to_bits),
            expected.map(f32::to_bits),
            "sample-version replay frame {frame}"
        );
    }
    for target in [599, 600, 601, 999, 1000, 1001, 1799, 1800, 1801, 2800] {
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
    let workspace = std::path::Path::new("var").canonicalize().unwrap();
    assert!(path.starts_with(workspace));
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn sample_generation_mono_legato_new_note_uses_latest_material_without_envelope_restart() {
    let notes = [
        NoteEvent::new(0, 960, NoteOct::from_pitch_index(48)),
        NoteEvent::new(192, 96, NoteOct::from_pitch_index(52)),
    ];
    let mut c = config(asset(0, SampleMode::Sampler), SampleMode::Sampler, &notes);
    c.voices = 1;
    c.mono_legato = true;
    let mut runtime = PolyOscRuntime::from_config(&c);
    let mut state = PolyOscState::new();
    let mut p = params();
    p.envelope.attack_ms = 100.0;
    for frame in 0..600 {
        process_poly_sample(&mut state, &runtime, p, frame as f64 / 8000.0, 120, true);
    }
    c.sample = Some(asset(1, SampleMode::Sampler));
    let next = PolyOscRuntime::from_config(&c);
    state.retire_materials(&mut runtime);
    runtime = next;
    for frame in 600..800 {
        process_poly_sample(&mut state, &runtime, p, frame as f64 / 8000.0, 120, true);
    }
    let mut expected_env = state.voices[0].amp;
    process_poly_sample(&mut state, &runtime, p, 0.1, 120, true);
    let v = &mut state.voices[0];
    assert_eq!(v.id, c.note.events()[1].id);
    let material = state.materials[v.sample_slot.unwrap() as usize]
        .as_ref()
        .unwrap();
    assert_eq!(material.sample.name, "version 1");
    assert_eq!(v.amp.next(true, false, p.envelope, 1.0 / 8000.0), {
        expected_env.next(true, false, p.envelope, 1.0 / 8000.0);
        expected_env.next(true, false, p.envelope, 1.0 / 8000.0)
    });
}
