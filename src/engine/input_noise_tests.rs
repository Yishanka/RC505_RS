use super::*;
use crate::{
    config::{
        FxKind, InputFx, audio_fx::AudioFxKind, input_noise::InputNoiseConfig,
        note_configs::NoteOct, osc_configs::Waveform, sequence_edit::NoteEvent,
    },
    engine::loop_audio::OfflinePages,
};
fn config() -> AppConfig {
    let mut c = AppConfig::new(120, 0, 5);
    c.input_noise = InputNoiseConfig {
        enabled: true,
        threshold_db: -30.0,
    };
    c.track_options[0].quantize = Quantize::Off;
    c
}
fn core(c: &AppConfig, sr: u32) -> RenderCore {
    let mut core = RenderCore::new(sr);
    core.configure(&mut Parameters::from_config(c, sr));
    core
}
fn cleanup(root: std::path::PathBuf) {
    let path = root.canonicalize().unwrap();
    assert!(path.starts_with(std::path::Path::new("var").canonicalize().unwrap()));
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn input_noise_precedes_high_gain_fx_and_keeps_osc_and_loop_audio_unchanged() {
    let mut c = config();
    c.input_fx
        .set_slot_kind(0, 1, FxKind::Audio(AudioFxKind::Dynamics));
    c.input_fx.banks[0].slots[1].is_enabled = true;
    if let Some(InputFx::Audio(p)) = &mut c.input_fx.banks[0].slots[1].fx {
        p.ratio = 1.0;
        p.makeup_db = 24.0;
        p.level_db = 12.0;
    }
    let mut quiet = core(&c, 8000);
    quiet.action(Action::Trigger(0), &mut OfflinePages);
    for _ in 0..1000 {
        assert_eq!(quiet.process([0.001, -0.001], &mut OfflinePages), [0.0; 2]);
    }
    assert!((0..quiet.tracks[0].audio.len).all(|i| quiet.tracks[0].audio.read(i) == [0.0; 2]));
    c.input_fx.set_slot_kind(0, 0, FxKind::Oscillator);
    c.input_fx.banks[0].slots[0].is_enabled = true;
    if let Some(InputFx::Oscillator(o)) = &mut c.input_fx.banks[0].slots[0].fx {
        o.waveform.value = Waveform::Sine;
        o.level.value = 1;
        o.note.replace_events(
            3840,
            &[NoteEvent::new(0, 3840, NoteOct::from_pitch_index(57))],
        );
    }
    let (mut a, mut b) = (core(&c, 8000), core(&c, 8000));
    for core in [&mut a, &mut b] {
        core.tracks[0]
            .audio
            .write(0, [0.1, -0.1], &mut OfflinePages);
        core.tracks[0].mode = Mode::Playing;
        core.action(Action::Metronome(true), &mut OfflinePages);
    }
    let mut heard = 0.0f32;
    for _ in 0..1500 {
        let one = a.process([0.001, -0.001], &mut OfflinePages);
        let other = b.process([0.0; 2], &mut OfflinePages);
        assert_eq!(one, other);
        heard = heard.max(one[0].abs());
    }
    assert!(
        heard > 0.15,
        "Internal OSC and existing loop must still be audible"
    );
    assert!(
        a.view().input_peak > 0.0,
        "Input meter remains pre-gate for threshold adjustment"
    );
}
#[test]
fn input_noise_preserves_delayed_effect_tail_after_external_gate_closes() {
    let mut c = config();
    c.input_fx
        .set_slot_kind(0, 0, FxKind::Audio(AudioFxKind::Delay));
    c.input_fx.banks[0].slots[0].is_enabled = true;
    if let Some(InputFx::Audio(p)) = &mut c.input_fx.banks[0].slots[0].fx {
        p.time_ms = 250.0;
        p.feedback_repeats = 0;
        p.feedback = 0.0;
        p.direct = 0.0;
        p.effect_level = 1.0;
        p.high_cut_hz = 0.0;
        p.low_cut_hz = 0.0;
    }
    let mut engine = core(&c, 8000);
    let mut tail = 0.0f32;
    for n in 0..3000 {
        let out = engine.process(if n < 100 { [0.5; 2] } else { [0.0; 2] }, &mut OfflinePages);
        if n > 2000 {
            assert_eq!(engine.conditioned_input(), [0.0; 2]);
            tail = tail.max(out[0].abs());
        }
    }
    assert!(
        tail > 0.1,
        "Noise suppression belongs before Delay, not across its tail"
    );
}
#[test]
fn input_noise_old_renderers_bypass_and_restore_resets_a_previously_open_gate() {
    let c = config();
    for renderer in 2..=7 {
        let mut old = core(&c, 8000);
        old.set_renderer_version(renderer);
        for _ in 0..100 {
            assert_eq!(
                old.process([0.001, -0.001], &mut OfflinePages),
                [0.001, -0.001]
            );
        }
    }
    let mut warm = core(&c, 8000);
    for _ in 0..1000 {
        warm.process([0.2; 2], &mut OfflinePages);
    }
    assert_eq!(warm.conditioned_input(), [0.2; 2]);
    let mut snapshot = AudioSnapshot::empty(8000);
    warm.snapshot(&mut snapshot, &mut OfflinePages);
    warm.restore(&mut snapshot);
    let mut fresh = core(&c, 8000);
    for n in 0..100 {
        let got = warm.process([0.2; 2], &mut OfflinePages);
        assert_eq!(got, fresh.process([0.2; 2], &mut OfflinePages));
        if n == 0 {
            assert!(got[0] < 0.02);
        }
    }
}
#[test]
fn input_noise_project_snapshot_sanitize_and_reconnect_are_complete() {
    let mut c = config();
    c.input_noise.threshold_db = -43.5;
    let data = crate::project::data_from_config(&c);
    let mut json = serde_json::to_value(&data).unwrap();
    let mut restored = AppConfig::new(120, 0, 5);
    crate::project::apply_data_to_config(
        &mut restored,
        serde_json::from_value(json.clone()).unwrap(),
    );
    assert_eq!(restored.input_noise, c.input_noise);
    json.as_object_mut().unwrap().remove("input_noise");
    crate::project::apply_data_to_config(&mut restored, serde_json::from_value(json).unwrap());
    assert_eq!(restored.input_noise, InputNoiseConfig::default());
    assert_eq!(
        InputNoiseConfig {
            enabled: true,
            threshold_db: f32::NAN
        }
        .sanitized()
        .threshold_db,
        -50.0
    );
    assert_eq!(
        InputNoiseConfig {
            enabled: true,
            threshold_db: 10.0
        }
        .sanitized()
        .threshold_db,
        0.0
    );
    let root = std::path::PathBuf::from("var")
        .join(format!("input-noise-snapshot-{}", crate::session::id()));
    crate::session::write_bundle(&root, &AudioSnapshot::empty(8000), data).unwrap();
    let (mut snapshot, data) = crate::session::read_bundle(&root).unwrap();
    crate::project::apply_data_to_config(&mut restored, data);
    for sr in [8000, 44100, 48000, 96000, 192000] {
        let mut engine = core(&restored, sr);
        if sr == 8000 {
            engine.restore(&mut snapshot);
        }
        for _ in 0..64 {
            assert_eq!(engine.process([0.0001; 2], &mut OfflinePages), [0.0; 2]);
        }
    }
    cleanup(root);
}
#[test]
fn input_noise_raw_replay_with_parameter_events_and_seek_is_bit_exact() {
    use crate::replay::{
        EventKind, Writer,
        streaming::{Machine, Source},
    };
    let mut c = config();
    let mut engine = core(&c, 8000);
    let root = std::path::PathBuf::from("var")
        .join(format!("input-noise-replay-{}", crate::session::id()));
    let mut snapshot = AudioSnapshot::empty(8000);
    engine.snapshot(&mut snapshot, &mut OfflinePages);
    let mut writer = Writer::begin(
        root.clone(),
        "noise.json".into(),
        0,
        snapshot,
        crate::project::data_from_config(&c),
    )
    .unwrap();
    let mut live = Vec::new();
    let mut raw = Vec::new();
    for n in 0..6400u64 {
        if n == 0 {
            engine.action(Action::Trigger(0), &mut OfflinePages);
            writer
                .event(n, EventKind::Action(Action::Trigger(0)))
                .unwrap();
        }
        if n == 6000 {
            engine.action(Action::Stop(0), &mut OfflinePages);
            writer.event(n, EventKind::Action(Action::Stop(0))).unwrap();
        }
        if matches!(n, 800 | 3200 | 4200) {
            c.input_noise.enabled = n != 3200;
            c.input_noise.threshold_db = if n == 4200 { -50.0 } else { -35.0 };
            engine.configure(&mut Parameters::from_config(&c, 8000));
            writer
                .event(n, EventKind::Config(crate::project::data_from_config(&c)))
                .unwrap();
        }
        let level = if (1000..2200).contains(&n) || n >= 4600 {
            0.2
        } else {
            0.001
        };
        let x = [level, -level * 0.5];
        raw.extend(x);
        writer.audio(n, &[x]).unwrap();
        live.push(engine.process(x, &mut OfflinePages));
    }
    writer.finish(6400).unwrap();
    let original: Vec<f32> = hound::WavReader::open(root.join("input.wav"))
        .unwrap()
        .into_samples()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        original, raw,
        "Replay stores pre-gate input, not a destructive gated recording"
    );
    let source = Source::open(&root).unwrap();
    let mut replay = Machine::new(source.clone()).unwrap();
    for expected in &live {
        assert_eq!(
            replay.next().unwrap().unwrap().map(f32::to_bits),
            expected.map(f32::to_bits)
        );
    }
    for target in [0, 799, 800, 1000, 2199, 2200, 3200, 4199, 4200, 4600] {
        let mut seek = Machine::new(source.clone()).unwrap();
        seek.advance_to(target, || false).unwrap();
        for expected in &live[target as usize..target as usize + 64] {
            assert_eq!(
                seek.next().unwrap().unwrap().map(f32::to_bits),
                expected.map(f32::to_bits)
            );
        }
    }
    for i in 0..engine.tracks[0].audio.len {
        assert_eq!(
            replay.core.tracks[0].audio.read(i),
            engine.tracks[0].audio.read(i)
        );
    }
    drop(replay);
    drop(source);
    drop(engine);
    cleanup(root);
}
