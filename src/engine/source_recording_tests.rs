use super::*;
use crate::{
    config::{
        FxKind, InputFx, TrackFxKind,
        audio_fx::AudioFxKind,
        note_configs::NoteOct,
        osc_configs::{SampleAsset, SampleMode, Waveform},
        sequence_edit::NoteEvent,
        track_options::{InputRouting, RecordReference},
    },
    engine::loop_audio::OfflinePages,
};
use std::sync::Arc;
const LENGTH: usize = 1200;
fn configuration(
    sr: u32,
    h_ms: usize,
    route: InputRouting,
    osc_after_pitch: Option<bool>,
) -> AppConfig {
    let mut c = AppConfig::new(120, h_ms, 5);
    c.input_routing = route;
    c.input_thru = true;
    c.pdc_enabled = osc_after_pitch.is_some();
    c.track_options[0].quantize = Quantize::Off;
    let slot = usize::from(osc_after_pitch == Some(true));
    c.input_fx.set_slot_kind(0, slot, FxKind::Oscillator);
    c.input_fx.banks[0].slots[slot].is_enabled = true;
    if let Some(InputFx::Oscillator(o)) = &mut c.input_fx.banks[0].slots[slot].fx {
        o.waveform.value = Waveform::Sample;
        o.sample_mode = SampleMode::Sampler;
        o.sample_loop = false;
        o.sample_root = 57;
        o.sample_temporary = false;
        let mut pcm = vec![0.0; LENGTH];
        pcm[128] = 0.4;
        pcm[LENGTH - 1] = -0.3;
        o.sample = Some(Arc::new(SampleAsset::new("clocked pulses".into(), sr, pcm)));
        o.note.replace_events(
            15360,
            &[NoteEvent::new(0, 15360, NoteOct::from_pitch_index(57))],
        );
        o.envelope.attack_ms.value = 0.0;
        o.envelope.hold_ms.value = 0.0;
        o.envelope.decay_ms.value = 0.0;
        o.envelope.sustain_pct.value = 100;
        o.envelope.release_ms.value = 1.0;
        o.osc_filter.mix.value = 0;
    }
    if osc_after_pitch.is_some() {
        let pitch = 1 - slot;
        c.input_fx
            .set_slot_kind(0, pitch, FxKind::Audio(AudioFxKind::Transpose));
        c.input_fx.banks[0].slots[pitch].is_enabled = true;
        // A slower parallel branch makes M different from Li.
        for slot in 0..2 {
            c.track_fx
                .set_slot_kind(0, slot, TrackFxKind::Audio(AudioFxKind::Transpose));
        }
    }
    c
}
fn run(c: &AppConfig, sr: u32, h: usize, renderer: u32, mic: bool) -> (Vec<Frame>, Vec<Frame>) {
    let mut core = RenderCore::new(sr);
    core.set_renderer_version(renderer);
    let mut p = Parameters::from_config(c, sr);
    p.latency_frames = h;
    core.configure(&mut p);
    let m = core.pdc_frames();
    let delay = h + core.input_fx_latency_frames() + m;
    core.action(Action::Metronome(true), &mut OfflinePages);
    core.action(Action::Trigger(0), &mut OfflinePages);
    let mut heard = Vec::new();
    for n in 0..LENGTH + delay + 2 {
        if n == LENGTH {
            core.action(Action::Stop(0), &mut OfflinePages);
        }
        let input = if mic && n == h + m + 256 {
            [0.2, -0.1]
        } else {
            [0.0; 2]
        };
        heard.push(core.process(input, &mut OfflinePages));
    }
    assert!(!core.exhausted);
    assert_eq!(core.tracks[0].audio.len, LENGTH);
    assert_eq!(core.tracks[0].mode, Mode::Stopped);
    (
        (0..LENGTH).map(|n| core.tracks[0].audio.read(n)).collect(),
        heard,
    )
}
#[test]
fn source_comp_pure_osc_stays_at_the_same_sample_for_all_hardware_offsets() {
    for sr in [8000, 44100, 48000, 96000, 192000] {
        let c = configuration(sr, 0, InputRouting::Serial, None);
        let (base, heard) = run(&c, sr, 0, 7, false);
        assert!(
            base[128][0] > 0.1 && base[LENGTH - 1][0] < 0.0,
            "fixture must include onset and finish-tail pulse"
        );
        for h in [7, sr as usize / 2] {
            let (recorded, monitor) = run(&c, sr, h, 7, false);
            assert_eq!(recorded, base, "sr{sr},H{h}");
            assert_eq!(&monitor[..heard.len()], &heard);
        }
        let (old, _) = run(&c, sr, 32, 6, false);
        assert!(
            old[96][0] > 0.1 && old[128][0] == 0.0,
            "renderer6 must keep its former shifted capture"
        );
    }
}
#[test]
fn source_comp_mic_and_osc_align_together_with_serial_legacy_and_pdc() {
    for route in [InputRouting::Serial, InputRouting::Legacy] {
        for placement in [None, Some(false), Some(true)] {
            let c = configuration(8000, 0, route, placement);
            let (base, _) = run(&c, 8000, 0, 7, false);
            let (mix, _) = run(&c, 8000, 97, 7, true);
            for n in 0..LENGTH {
                let expected = [
                    base[n][0] + if n == 256 { 0.2 } else { 0.0 },
                    base[n][1] + if n == 256 { -0.1 } else { 0.0 },
                ];
                for ch in 0..2 {
                    assert!(
                        (mix[n][ch] - expected[ch]).abs() < 0.000001,
                        "{route:?}/{placement:?} sample{n}:{:?}/{expected:?}",
                        mix[n]
                    );
                }
            }
        }
    }
}
#[test]
fn source_comp_reference_and_offsets_are_latched_through_recording_and_overdub_tail() {
    let sr = 8000;
    let mut c = configuration(sr, 10, InputRouting::Serial, Some(false));
    let mut core = RenderCore::new(sr);
    core.configure(&mut Parameters::from_config(&c, sr));
    let li = core.input_fx_latency_frames();
    let m = core.pdc_frames();
    let delay = 80 + li + m;
    for _ in 0..LENGTH {
        core.tracks[0]
            .audio
            .write(core.tracks[0].audio.len, [0.01, -0.01], &mut OfflinePages);
    }
    core.tracks[0].mode = Mode::Playing;
    core.clock.start();
    core.action(Action::Metronome(true), &mut OfflinePages);
    core.action(Action::Trigger(0), &mut OfflinePages);
    for n in 0..LENGTH + delay + 2 {
        if n == 200 {
            c.track_options[0].record_reference = RecordReference::Internal;
            let mut p = Parameters::from_config(&c, sr);
            p.latency_frames = 400;
            core.configure(&mut p);
        }
        if n == LENGTH {
            core.action(Action::Stop(0), &mut OfflinePages);
        }
        core.process([0.0; 2], &mut OfflinePages);
        if n < LENGTH + delay {
            assert_eq!(core.tracks[0].capture_delay, delay);
            assert_eq!(core.tracks[0].generator_capture_delay, 80 + m);
        }
    }
    let (base, _) = run(
        &configuration(sr, 0, InputRouting::Serial, Some(false)),
        sr,
        0,
        7,
        false,
    );
    for n in 0..LENGTH {
        let got = core.tracks[0].audio.read(n);
        assert!(
            (got[0] - base[n][0] - 0.01).abs() < 1e-6 && (got[1] - base[n][1] + 0.01).abs() < 1e-6,
            "overdub {n}"
        );
    }
    assert!(!core.exhausted);
    assert_eq!(core.tracks[0].mode, Mode::Stopped);
    // A new Internal pass ignores H; the existing DSP latency remains.
    core.action(Action::Trigger(0), &mut OfflinePages);
    core.process([0.0; 2], &mut OfflinePages);
    core.action(Action::Trigger(0), &mut OfflinePages);
    core.process([0.0; 2], &mut OfflinePages);
    assert_eq!(core.tracks[0].capture_delay, li);
    assert_eq!(core.tracks[0].generator_capture_delay, 0);
}
#[test]
fn source_comp_zero_offset_bypasses_recombination_and_keeps_the_shadow_tail_warm() {
    let sr = 8000;
    let mut c = configuration(sr, 0, InputRouting::Serial, None);
    c.input_fx.set_slot_kind(0, 0, FxKind::None);
    c.input_fx
        .set_slot_kind(0, 1, FxKind::Audio(AudioFxKind::Delay));
    c.input_fx.banks[0].slots[1].is_enabled = true;
    if let Some(InputFx::Audio(p)) = &mut c.input_fx.banks[0].slots[1].fx {
        p.time_ms = 75.0;
        p.feedback_repeats = 0;
        p.feedback = 0.7;
        p.direct = 0.0;
        p.effect_level = 1.0;
        p.high_cut_hz = 0.0;
        p.low_cut_hz = 0.0;
    }
    let mut recordings = Vec::new();
    for renderer in [6, 7] {
        let mut core = RenderCore::new(sr);
        core.set_renderer_version(renderer);
        core.configure(&mut Parameters::from_config(&c, sr));
        for n in 0..1150 {
            core.process(
                if n == 0 { [0.2, -0.1] } else { [0.0; 2] },
                &mut OfflinePages,
            );
        }
        let mut p = Parameters::from_config(&c, sr);
        p.latency_frames = 137;
        core.configure(&mut p);
        core.action(Action::Trigger(0), &mut OfflinePages);
        for n in 0..1600 {
            if n == 1000 {
                core.action(Action::Stop(0), &mut OfflinePages);
            }
            core.process([0.0; 2], &mut OfflinePages);
            if renderer == 7 && n == 100 {
                assert_eq!(
                    core.tracks[0].audio.read(50),
                    [0.0; 2],
                    "External delay tail must not be mistaken for generated audio"
                );
            }
        }
        assert!(!core.exhausted);
        recordings.push(
            (0..1000)
                .map(|n| core.tracks[0].audio.read(n))
                .collect::<Vec<_>>(),
        );
    }
    assert_eq!(recordings[0], recordings[1]);
}
#[test]
fn source_comp_shadow_never_captures_or_commits_phrase_state() {
    use crate::config::osc_configs::SampleCapture;
    use std::sync::atomic::Ordering;
    let sr = 8000;
    let mut c = configuration(sr, 10, InputRouting::Serial, None);
    if let Some(InputFx::Oscillator(o)) = &mut c.input_fx.banks[0].slots[0].fx {
        o.waveform.value = Waveform::Sine;
        o.note.launch_serial = 42;
    }
    c.input_fx.set_slot_kind(0, 1, FxKind::Oscillator);
    let capture = Arc::new(SampleCapture::new(20));
    if let Some(InputFx::Oscillator(o)) = &mut c.input_fx.banks[0].slots[1].fx {
        o.capture = Some(capture.clone());
        o.threshold.value = 0;
    }
    let mut core = RenderCore::new(sr);
    core.configure(&mut Parameters::from_config(&c, sr));
    core.action(Action::Metronome(true), &mut OfflinePages);
    for _ in 0..200 {
        core.process([0.0; 2], &mut OfflinePages);
    }
    assert_eq!(capture.state.load(Ordering::Acquire), 2);
    assert!(
        capture.samples[..160]
            .iter()
            .any(|x| f32::from_bits(x.load(Ordering::Relaxed)).abs() > 0.01),
        "The shadow's silent OSC prefix must never overwrite capture PCM"
    );
    assert_eq!(core.external_input.phrase_views()[0][0].source, 0);
    assert_eq!(core.input.phrase_views()[0][0].applied_serial, 42);
}

#[test]
fn source_comp_parallel_passes_latch_different_h_without_moving_generated_notes() {
    let sr = 8000;
    let mut c = configuration(sr, 10, InputRouting::Serial, None);
    c.track_options[1].quantize = Quantize::Off;
    let mut core = RenderCore::new(sr);
    core.configure(&mut Parameters::from_config(&c, sr));
    core.action(Action::Metronome(true), &mut OfflinePages);
    core.action(Action::Trigger(0), &mut OfflinePages);
    for n in 0..1900 {
        if n == 400 {
            let mut p = Parameters::from_config(&c, sr);
            p.latency_frames = 400;
            core.configure(&mut p);
            core.action(Action::Trigger(1), &mut OfflinePages);
        }
        if n == 1200 {
            core.action(Action::Stop(0), &mut OfflinePages);
        }
        if n == 1400 {
            core.action(Action::Stop(1), &mut OfflinePages);
        }
        core.process([0.0; 2], &mut OfflinePages);
        if n >= 400 {
            assert_eq!(core.tracks[0].capture_delay, 80);
            assert_eq!(core.tracks[1].capture_delay, 400);
        }
    }
    assert!(!core.exhausted);
    assert_eq!(core.tracks[0].audio.len, 1200);
    assert_eq!(core.tracks[1].audio.len, 1000);
    assert!(core.tracks[0].audio.read(128)[0] > 0.1);
    assert!(core.tracks[0].audio.read(1199)[0] < 0.0);
    assert!(core.tracks[1].audio.read(799)[0] < 0.0);
    for n in 0..1000 {
        let expected = if n < 800 {
            core.tracks[0].audio.read(n + 400)
        } else {
            [0.0; 2]
        };
        assert_eq!(
            core.tracks[1].audio.read(n),
            expected,
            "second pass frame{n}"
        );
    }
}
struct PreparedPages {
    available: Vec<super::super::loop_audio::Page>,
    retired: Vec<super::super::loop_audio::Page>,
}
impl PageAllocator for PreparedPages {
    fn acquire(&mut self) -> Option<super::super::loop_audio::Page> {
        self.available.pop()
    }
    fn retire(&mut self, page: super::super::loop_audio::Page) {
        self.retired.push(page);
    }
}
#[test]
fn source_comp_runtime_swaps_shadow_and_deferred_graph_do_not_allocate_or_free() {
    use super::super::loop_audio::PAGE_FRAMES;
    let sr = 8000;
    let mut c = configuration(sr, 11, InputRouting::Serial, Some(false));
    let mut core = RenderCore::new(sr);
    let mut initial = Parameters::from_config(&c, sr);
    let old_delay = initial.latency_frames + initial.pdc.input_frames + initial.pdc.output_frames;
    c.input_fx.set_slot_kind(0, 1, FxKind::None);
    let mut next = Parameters::from_config(&c, sr);
    next.latency_frames = 299;
    let mut pool = PreparedPages {
        available: (0..64).map(|_| Arc::new([[0.0; 2]; PAGE_FRAMES])).collect(),
        retired: Vec::with_capacity(128),
    };
    let allocations = crate::test_alloc::count(|| {
        core.configure(&mut initial);
        core.action(Action::Metronome(true), &mut pool);
        core.action(Action::Trigger(0), &mut pool);
        for n in 0..LENGTH + old_delay + 500 {
            if n == 400 {
                core.configure(&mut next);
            }
            if n == LENGTH {
                core.action(Action::Stop(0), &mut pool);
            }
            core.process([0.0; 2], &mut pool);
        }
    });
    assert_eq!(allocations, 0);
    assert!(!core.exhausted);
    assert!(!core.view().input_latency_pending);
    assert_eq!(core.input_fx_latency_frames(), 0);
    let (base, _) = run(
        &configuration(sr, 0, InputRouting::Serial, None),
        sr,
        0,
        7,
        false,
    );
    for n in 0..LENGTH {
        let got = core.tracks[0].audio.read(n);
        for ch in 0..2 {
            assert!(
                (got[ch] - base[n][ch]).abs() < 1e-6,
                "deferred source offset frame{n}"
            );
        }
    }
}
#[test]
fn source_comp_replay_and_seek_preserve_mixed_record_and_overdub_positions() {
    use crate::replay::{
        EventKind, Writer,
        streaming::{Machine, Source},
    };
    let sr = 8000;
    let mut c = configuration(sr, 8, InputRouting::Serial, Some(true));
    let mut core = RenderCore::new(sr);
    core.configure(&mut Parameters::from_config(&c, sr));
    let root = std::path::PathBuf::from("var")
        .join(format!("source-comp-replay-{}", crate::session::id()));
    let mut snapshot = AudioSnapshot::empty(sr);
    core.snapshot(&mut snapshot, &mut OfflinePages);
    let mut writer = Writer::begin(
        root.clone(),
        "source-comp.json".into(),
        0,
        snapshot,
        crate::project::data_from_config(&c),
    )
    .unwrap();
    let mut live = Vec::new();
    for n in 0..10000u64 {
        if let Some(action) = match n {
            0 => Some(Action::Metronome(true)),
            1 => Some(Action::Trigger(0)),
            2200 | 7000 => Some(Action::Stop(0)),
            4400 | 4500 => Some(Action::Trigger(0)),
            _ => None,
        } {
            core.action(action, &mut OfflinePages);
            writer.event(n, EventKind::Action(action)).unwrap();
        }
        if n == 4600 {
            c.input_fx.set_slot_kind(0, 0, FxKind::None);
            core.configure(&mut Parameters::from_config(&c, sr));
            writer
                .event(n, EventKind::Config(crate::project::data_from_config(&c)))
                .unwrap();
        }
        let dry = if matches!(n, 1800 | 5000 | 5500) {
            [0.2, -0.1]
        } else {
            [0.0; 2]
        };
        writer.audio(n, &[dry]).unwrap();
        live.push(core.process(dry, &mut OfflinePages));
        if let Some(applied) = core.take_pdc_applied_event() {
            writer.event(n + 1, EventKind::PdcApplied(applied)).unwrap();
        }
    }
    writer.finish(10000).unwrap();
    assert!(!core.exhausted);
    let source = Source::open(&root).unwrap();
    assert_eq!(source.metadata.renderer, crate::replay::RENDERER_VERSION);
    let mut replay = Machine::new(source.clone()).unwrap();
    for (n, expected) in live.iter().enumerate() {
        assert_eq!(
            replay.next().unwrap().unwrap().map(f32::to_bits),
            expected.map(f32::to_bits),
            "mixed replay frame{n}"
        );
    }
    assert_eq!(replay.core.tracks[0].audio.len, core.tracks[0].audio.len);
    for n in 0..core.tracks[0].audio.len {
        assert_eq!(
            replay.core.tracks[0].audio.read(n).map(f32::to_bits),
            core.tracks[0].audio.read(n).map(f32::to_bits)
        );
    }
    for target in [0, 1800, 2200, 4300, 4500, 4600, 6999, 7000, 9000] {
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

#[test]
fn source_comp_first_generated_loop_is_audible_without_waiting_for_h_even_when_loop_is_shorter() {
    let sr = 8000;
    let length = 256;
    let mut c = configuration(sr, 0, InputRouting::Serial, None);
    c.input_thru = false;
    let mut outputs = Vec::new();
    for (renderer, h) in [(7, 0), (7, 512), (6, 512)] {
        let mut core = RenderCore::new(sr);
        core.set_renderer_version(renderer);
        let mut p = Parameters::from_config(&c, sr);
        p.latency_frames = h;
        core.configure(&mut p);
        core.action(Action::Metronome(true), &mut OfflinePages);
        core.action(Action::Trigger(0), &mut OfflinePages);
        let mut heard = Vec::new();
        for n in 0..length * 5 {
            if n == length {
                core.action(Action::Trigger(0), &mut OfflinePages);
            }
            heard.push(core.process([0.0; 2], &mut OfflinePages));
        }
        assert!(!core.exhausted);
        outputs.push(heard);
    }
    assert_eq!(
        &outputs[0][length..],
        &outputs[1][length..],
        "H must not postpone a pure OSC loop's first audible iteration"
    );
    assert!(outputs[1][length + 128][0] > 0.1);
    assert_eq!(
        outputs[2][length + 128],
        [0.0; 2],
        "renderer6 retains historical waiting behavior"
    );
}
#[test]
fn source_comp_incomplete_opposite_sign_sums_are_not_clipped_before_their_partner_arrives() {
    for overdub in [false, true] {
        for length in [1, 16] {
            let mut track = CoreTrack::new(8000);
            track.mode = if overdub {
                Mode::Overdub
            } else {
                Mode::Recording
            };
            track.start = 0;
            track.finish = Some(4);
            track.capture_delay = 10;
            track.generator_capture_delay = 10;
            if overdub {
                for _ in 0..length {
                    track
                        .audio
                        .write(track.audio.len, [15.0, -15.0], &mut OfflinePages);
                }
            }
            for n in 0..14 {
                let generated = if n < 4 {
                    if overdub {
                        [12.0, -12.0]
                    } else {
                        [29.0, -29.0]
                    }
                } else {
                    [0.0; 2]
                };
                let external = if n >= 10 {
                    if overdub {
                        [-12.0, 12.0]
                    } else {
                        [-15.0, 15.0]
                    }
                } else {
                    [0.0; 2]
                };
                assert!(track.write_separate_sources(n, external, generated, &mut OfflinePages));
                if overdub {
                    track.cursor = (track.cursor + 1) % length;
                }
            }
            for n in 0..if overdub { length } else { 4 } {
                assert_eq!(
                    track.audio.read(n),
                    if overdub {
                        [15.0, -15.0]
                    } else {
                        [14.0, -14.0]
                    }
                );
            }
        }
    }
}
#[test]
#[ignore = "manual release CPU/memory comparison for source-separated recording"]
fn benchmark_source_compensation_cost() {
    #[cfg(windows)]
    fn private_bytes() -> usize {
        #[repr(C)]
        struct Counters {
            cb: u32,
            faults: u32,
            peak_working: usize,
            working: usize,
            peak_paged: usize,
            paged: usize,
            peak_nonpaged: usize,
            nonpaged: usize,
            pagefile: usize,
            peak_pagefile: usize,
            private: usize,
        }
        #[link(name = "psapi")]
        unsafe extern "system" {
            fn GetProcessMemoryInfo(process: isize, info: *mut Counters, size: u32) -> i32;
        }
        let mut info: Counters = unsafe { std::mem::zeroed() };
        info.cb = std::mem::size_of::<Counters>() as u32;
        assert_ne!(unsafe { GetProcessMemoryInfo(-1, &mut info, info.cb) }, 0);
        info.private
    }
    #[cfg(not(windows))]
    fn private_bytes() -> usize {
        0
    }
    if std::env::var("RC505_SOURCE_MEMORY").is_ok() {
        let sr = std::env::var("RC505_SOURCE_MEMORY")
            .unwrap()
            .parse::<u32>()
            .unwrap();
        let before = private_bytes();
        let now = std::time::Instant::now();
        let core = Box::new(RenderCore::new(sr));
        let initialized = now.elapsed().as_secs_f64() * 1000.0;
        let after = private_bytes();
        let mut extra = Box::new(InputFxEngine::new(sr as f32));
        extra.prepare(sr as f32);
        let shadow = private_bytes().saturating_sub(after);
        println!(
            "SOURCE_MEMORY sr={sr} core_delta_MiB={:.2} prepared_shadow_MiB={:.2} init_ms={initialized:.2}",
            (after - before) as f64 / 1048576.0,
            shadow as f64 / 1048576.0
        );
        std::hint::black_box((&core, &extra));
        return;
    }
    use super::super::loop_audio::PAGE_FRAMES;
    for heavy in [false, true] {
        let mut reference = None;
        for renderer in [6, 7] {
            let sr = 48000;
            let mut c = AppConfig::new(120, 8, 5);
            c.input_routing = InputRouting::Serial;
            c.pdc_enabled = heavy;
            c.track_options[0].quantize = Quantize::Off;
            if heavy {
                c.input_fx.set_slot_kind(0, 0, FxKind::Oscillator);
                if let Some(InputFx::Oscillator(o)) = &mut c.input_fx.banks[0].slots[0].fx {
                    o.note.replace_events(
                        15360,
                        &[NoteEvent::new(0, 15360, NoteOct::from_pitch_index(36))],
                    );
                }
                for (slot, kind) in [
                    (1, AudioFxKind::Transpose),
                    (2, AudioFxKind::Delay),
                    (3, AudioFxKind::Reverb),
                ] {
                    c.input_fx.set_slot_kind(0, slot, FxKind::Audio(kind));
                    if let Some(InputFx::Audio(p)) = &mut c.input_fx.banks[0].slots[slot].fx {
                        p.semitones = 7.0;
                        p.time_ms = 1.37;
                        p.feedback = 1.0;
                        p.feedback_repeats = 0;
                        p.preserve_formants = kind == AudioFxKind::Transpose;
                    }
                }
                for slot in &mut c.input_fx.banks[0].slots {
                    slot.is_enabled = true;
                }
            }
            let mut core = RenderCore::new(sr);
            core.set_renderer_version(renderer);
            core.configure(&mut Parameters::from_config(&c, sr));
            let mut pool = PreparedPages {
                available: (0..64).map(|_| Arc::new([[0.0; 2]; PAGE_FRAMES])).collect(),
                retired: Vec::with_capacity(128),
            };
            core.action(Action::Metronome(true), &mut pool);
            core.action(Action::Trigger(0), &mut pool);
            let mut blocks = Vec::with_capacity(600);
            let mut checksum = 0u64;
            let total = std::time::Instant::now();
            let allocations = crate::test_alloc::count(|| {
                for block in 0..563 {
                    let start = std::time::Instant::now();
                    for i in 0..256 {
                        let n = block * 256 + i;
                        let x = (std::f32::consts::TAU * 70.0 * n as f32 / sr as f32).sin() * 0.07;
                        let y = core.process([x, x * 0.7], &mut pool);
                        checksum = checksum
                            .wrapping_mul(131)
                            .wrapping_add(y[0].to_bits() as u64);
                    }
                    blocks.push(start.elapsed().as_secs_f64() * 1000.0);
                }
            });
            let cpu = total.elapsed().as_secs_f64() * 1000.0;
            blocks.sort_by(f64::total_cmp);
            assert_eq!(allocations, 0);
            assert!(!core.exhausted);
            if let Some(previous) = reference {
                assert_eq!(
                    checksum, previous,
                    "Recording compensation must not alter monitored input"
                );
            } else {
                reference = Some(checksum);
            }
            println!(
                "SOURCE_CPU heavy={heavy} renderer={renderer} audio_s={:.4} cpu_ms={cpu:.3} p95_ms={:.4} p99_ms={:.4} max_ms={:.4} allocation_events={allocations}",
                563.0 * 256.0 / 48000.0,
                blocks[blocks.len() * 95 / 100],
                blocks[blocks.len() * 99 / 100],
                blocks.last().unwrap()
            );
        }
    }
}
