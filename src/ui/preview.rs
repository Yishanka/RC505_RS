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
    if mode.contains("update") {
        app.update = Some(crate::updater::Release {
            schema: 1,
            version: "99.0.0".into(),
            file: String::new(),
            url: String::new(),
            sha256: String::new(),
        });
        app.startup_update.state = crate::updater::StartupState::Available("99.0.0".into());
    }
    if mode.starts_with("projects") {
        return;
    }
    if mode.starts_with("storage")
        || mode.starts_with("delete-project")
        || mode.starts_with("discard-replay")
    {
        super::storage::preview(app, mode);
        return;
    }
    app.active_project_idx = Some(0);
    if mode.starts_with("performance-audio-noise") {
        app.config.input_noise.enabled = true;
        app.config.input_noise.threshold_db = -45.0;
    }
    if mode.starts_with("shortcuts") {
        app.shortcut_editor.open(&app.shortcuts);
        return;
    }
    if mode.starts_with("performance") && mode.contains("audio") {
        app.left_page = crate::app::LeftPage::Audio;
        app.config.input_noise.enabled = true;
        if mode.contains("master") {
            app.master_fx_open = true;
            app.config.master_fx.filter_enabled = true;
            app.config.master_fx.compressor_enabled = true;
        }
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
            7680,
            &pitches
                .iter()
                .enumerate()
                .map(|(i, p)| crate::config::sequence_edit::NoteEvent {
                    id: 0,
                    velocity: 100,
                    start: i * 480,
                    len: 400,
                    pitch: crate::config::note_configs::NoteOct::from_pitch_index(*p),
                })
                .collect::<Vec<_>>(),
        );
        if mode.starts_with("osc-bot") {
            use crate::config::osc_configs::Waveform;
            osc.waveform.value = if mode.contains("rect") {
                Waveform::Rect
            } else if mode.contains("vintage") {
                Waveform::VintageSaw
            } else {
                Waveform::DetuneSaw
            };
            osc.input_mod_sens = Some(15.0);
            osc.dry_level = 0.3;
        }
        if mode.starts_with("poly") {
            use crate::config::{note_configs::NoteOct, sequence_edit::NoteEvent};
            osc.note.replace_events(
                7680,
                &[
                    (0, 24, 48),
                    (0, 24, 52),
                    (0, 24, 55),
                    (24, 24, 45),
                    (24, 24, 48),
                    (24, 24, 52),
                    (48, 24, 46),
                    (48, 24, 50),
                    (48, 24, 53),
                    (72, 24, 43),
                    (72, 24, 47),
                    (72, 24, 50),
                ]
                .map(|(start, len, p)| {
                    NoteEvent::new(start * 80, len * 80, NoteOct::from_pitch_index(p))
                }),
            );
            osc.note.clip_name = "Four chord phrase / 四个和弦".into();
        }
        if mode.starts_with("sample") {
            use crate::config::osc_configs::{SampleAsset, Waveform};
            osc.waveform.value = Waveform::Sample;
            osc.sample = Some(std::sync::Arc::new(SampleAsset::prepare_recording(
                "Vowel capture / 人声质感采样".into(),
                48000,
                &(0..24000)
                    .map(|i| {
                        let phase = i as f32 * std::f32::consts::TAU * 220.0 / 48000.0;
                        phase.sin() * 0.35 + (phase * 3.0).sin() * 0.2 + (phase * 7.0).sin() * 0.1
                    })
                    .collect::<Vec<_>>(),
            )));
            osc.select_sample_region();
            if mode.contains("thresholds") {
                osc.input_gate = true;
                osc.gate_threshold.value = 23;
                osc.capture_threshold.value = 7;
            }
            if mode.contains("saved") {
                if let Some(sample) = &osc.sample {
                    osc.sample_temporary = false;
                    osc.sample_ref = Some(crate::config::osc_configs::SavedSampleRef {
                        preset: "Glass keys".into(),
                        sha256: "0".repeat(64),
                        content_hash: sample.content_hash,
                        sample_rate: sample.sample_rate,
                        frames: sample.frames.len(),
                    });
                }
            }
            if mode.contains("capture") {
                osc.waveform.value = Waveform::Sine;
                osc.capture = Some(std::sync::Arc::new(
                    crate::config::osc_configs::SampleCapture::new(100),
                ));
            }
        }
        if mode.starts_with("lfo") {
            use crate::config::osc_configs::{CurvePoint, LfoShape, LfoTarget};
            osc.lfo2.enabled = true;
            osc.lfo2.target = LfoTarget::Pitch;
            osc.lfo2.sync = false;
            osc.lfo2.rate_hz = 5.0;
            osc.lfo2.depth = 0.25;
            osc.lfo2.mode = crate::config::osc_configs::LfoMode::Retrigger;
            osc.lfo.enabled = !mode.starts_with("lfo2");
            osc.lfo.shape = LfoShape::Custom;
            osc.lfo.target = LfoTarget::Cutoff;
            osc.lfo.points = vec![
                CurvePoint {
                    x: 0.0,
                    y: 0.0,
                    curve: -0.5,
                },
                CurvePoint {
                    x: 0.4,
                    y: 1.0,
                    curve: 0.6,
                },
                CurvePoint {
                    x: 0.75,
                    y: 0.2,
                    curve: 0.0,
                },
                CurvePoint {
                    x: 1.0,
                    y: 0.0,
                    curve: 0.0,
                },
            ];
        }
        osc.envelope.attack_ms.value = 80.0;
        osc.envelope.decay_ms.value = 350.0;
        osc.envelope.sustain_pct.value = 40;
        osc.envelope.release_ms.value = 300.0;
        if mode.starts_with("envelope-fractional") {
            osc.envelope.attack_ms.value = 40.1;
            osc.envelope.hold_ms.value = 50.2;
            osc.envelope.decay_ms.value = 200.3;
            osc.envelope.release_ms.value = 250.4;
        }
    }
    app.editor.select(FxTarget::Input { bank: 0, slot: 0 });
    app.editor.expanded = !mode.starts_with("performance")
        && !mode.contains("-quick")
        && !mode.starts_with("calibration")
        && !mode.starts_with("replays")
        && mode != "help";
    if mode == "help" || mode.starts_with("updates") {
        app.editor.expanded = false;
        app.help_open = true;
        app.help_tab = if mode.starts_with("updates") { 5 } else { 2 };
    }
    if mode == "vocoder" {
        app.editor.select(FxTarget::Input { bank: 0, slot: 2 });
    }
    if mode == "reverb" {
        app.editor.select(FxTarget::Input { bank: 0, slot: 3 });
    }
    if mode.starts_with("input-roll") {
        app.config.input_fx.set_slot_kind(0, 0, FxKind::Roll);
        app.config.input_fx.banks[0].slots[0].is_enabled = true;
        app.editor.select(FxTarget::Input { bank: 0, slot: 0 });
    }
    if mode == "roll" {
        app.config
            .track_fx
            .set_slot_kind(0, 0, crate::config::TrackFxKind::Roll);
        app.editor.select(FxTarget::Track { bank: 0, slot: 0 });
    }
    if mode.starts_with("audio-fx-") {
        use crate::config::audio_fx::AudioFxKind as K;
        let kind = if mode.contains("distortion") {
            K::Distortion
        } else if mode.contains("dynamics") {
            K::Dynamics
        } else if mode.contains("electric") {
            K::Electric
        } else if mode.contains("panning") {
            K::PanningDelay
        } else if mode.contains("slicer") {
            K::StepSlicer
        } else if mode.contains("transpose") {
            K::Transpose
        } else if mode.contains("enhance") {
            K::StereoEnhance
        } else if mode.contains("phaser") {
            K::Phaser
        } else if mode.contains("flanger") {
            K::Flanger
        } else if mode.contains("chorus") {
            K::Chorus
        } else if mode.contains("autopan") {
            K::AutoPan
        } else if mode.contains("tremolo") {
            K::Tremolo
        } else if mode.contains("sustainer") {
            K::Sustainer
        } else {
            K::Equalizer
        };
        app.config.input_fx.set_slot_kind(0, 0, FxKind::Audio(kind));
        app.config.input_fx.banks[0].slots[0].is_enabled = true;
        if let Some(InputFx::Audio(p)) = &mut app.config.input_fx.banks[0].slots[0].fx {
            p.low_db = 4.0;
            p.mid_db = -6.0;
            p.high_db = 2.0;
            p.high_mid_db = 3.0;
            p.semitones = 7.0;
            p.pitch_sequence = kind == K::Transpose;
            if kind == K::Dynamics {
                p.dynamics_profile = crate::config::dynamics_profiles::DynamicsProfile::PhoneVox;
                p.dynamics_amount = 4.0;
            }
            if matches!(kind, K::Phaser | K::Flanger | K::AutoPan | K::Tremolo) {
                p.mod_stepped = true;
                p.mod_step_beats = 0.25;
                p.mod_shape = 0.6;
            }
            if kind == K::StepSlicer {
                p.slicer_duty = 0.6;
                p.slicer_compress = true;
            }
            if kind == K::Chorus {
                p.chorus_low_cut_hz = 100.0;
                p.chorus_high_cut_hz = 10000.0;
            }
            if kind == K::StereoEnhance {
                p.enhance_low_cut_hz = 200.0;
                p.enhance_high_cut_hz = 10000.0;
            }
            if kind == K::PanningDelay && mode.contains("recording") {
                p.time_ms = 7.53;
                p.feedback_repeats = 0;
                p.feedback = 1.0;
            }
        }
    }
    if mode.starts_with("filter-standalone") {
        app.config.input_fx.set_slot_kind(0, 0, FxKind::Filter);
        app.config.input_fx.banks[0].slots[0].is_enabled = true;
    }
    if mode.starts_with("sequence-links") {
        let target = FxTarget::Input { bank: 0, slot: 0 };
        if let Some(note) = crate::presets::note_mut(&mut app.config, target) {
            note.clip_name = "Main phrase / 主乐句".into();
            app.editor.piano.select_all(note);
        }
        app.config.input_fx.set_slot_kind(0, 1, FxKind::Oscillator);
        let peer = FxTarget::Input { bank: 0, slot: 1 };
        let _ = crate::phrases::link(&mut app.config, target, peer);
        app.editor.phrase_manager_open = true;
        app.editor.clip_name = "Main phrase".into();
    }
    app.editor.page = if mode.starts_with("sequence") || mode.starts_with("poly") {
        EditorPage::Sequence
    } else if mode.starts_with("filter") {
        EditorPage::Filter
    } else if mode.starts_with("envelope") {
        EditorPage::Envelope
    } else if mode.starts_with("lfo") {
        EditorPage::Modulation
    } else {
        EditorPage::Sound
    };
    if mode.starts_with("automation-") {
        use crate::config::audio_fx::AudioFxKind;
        use crate::config::automation::{Interpolation, ParameterLane, Target};
        let (kind, target) = if mode.contains("delay") {
            (FxKind::Audio(AudioFxKind::PanningDelay), Target::DelayTime)
        } else if mode.contains("reverb") {
            (FxKind::Reverb, Target::ReverbWet)
        } else {
            (FxKind::Filter, Target::FilterCutoff)
        };
        app.config.input_fx.set_slot_kind(0, 0, kind);
        let mut lane = ParameterLane::create(target);
        lane.enabled = true;
        lane.interpolation = Interpolation::Curve;
        lane.points[0].curve = -0.5;
        lane.points[1].curve = 0.6;
        if mode.contains("short") {
            lane.rescale_length(96);
        }
        app.config.input_fx.banks[0].slots[0].parameter_lane = lane;
        app.editor.page = EditorPage::Automation;
    }
    if mode.starts_with("standalone-") {
        if mode.contains("track") {
            app.config
                .track_fx
                .set_slot_kind(0, 0, crate::config::TrackFxKind::Filter);
            app.editor.select(FxTarget::Track { bank: 0, slot: 0 });
        } else {
            app.config.input_fx.set_slot_kind(0, 0, FxKind::Filter);
            app.editor.select(FxTarget::Input { bank: 0, slot: 0 });
        }
        app.editor.page = EditorPage::Sound;
    }
    if mode.starts_with("candidate-") {
        let target = FxTarget::Input { bank: 0, slot: 0 };
        let mut source = crate::config::AppConfig::new(120, 0, 5);
        source.input_fx.set_slot_kind(
            0,
            0,
            if mode.contains("audio") {
                FxKind::Reverb
            } else {
                FxKind::Oscillator
            },
        );
        let text = crate::presets::encode(&source, target).unwrap();
        app.editor.candidate = Some(
            crate::presets::SoundCandidate::from_text(
                &app.config,
                target,
                "Warm room / 温暖空间".into(),
                text,
            )
            .unwrap(),
        );
        app.editor.library_open = true;
    }
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
        if mode.contains("import") {
            app.replay_panel.as_mut().unwrap().show_import_preview();
        }
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
    if mode.starts_with("performance-audio-noise") {
        app.view.input_peak = 0.002;
    }
    if mode.starts_with("playback") {
        app.audio
            .diagnostics
            .player_frame
            .store(24000, std::sync::atomic::Ordering::Relaxed);
    }
    if mode.contains("recording") {
        app.view.output_spectrum = std::array::from_fn(|i| ((i as f32 * 0.29).sin() * 0.7).abs());
    }
    if !mode.contains("recording") {
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
