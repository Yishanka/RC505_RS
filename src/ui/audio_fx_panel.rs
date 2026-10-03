//! The same controls back the compact rack and expanded audio-effect editor.
use super::{navigation, parameters, theme};
use crate::app_support::language::Language;
use crate::config::audio_fx::{AudioFxConfig, AudioFxKind as K, DriveStyle, DynamicsMode, Scale};
use eframe::egui::{self, Stroke, pos2};

fn value(ui: &mut egui::Ui, v: &mut f32, min: f32, max: f32, en: &str, zh: &str, log: bool) {
    let lang = Language::current(ui.ctx());
    let mut slider = egui::Slider::new(v, min..=max)
        .text(lang.choose(en, zh))
        .logarithmic(log);
    if min >= 0.0 && max <= 1.2 && max >= 0.5 {
        slider = slider
            .step_by(0.01)
            .custom_formatter(|v, _| format!("{:.0}%", v * 100.0))
            .custom_parser(|text| {
                text.trim()
                    .trim_end_matches('%')
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .map(|v| v / 100.0)
            });
    }
    navigation::register(ui.add(slider));
}
fn section(ui: &mut egui::Ui, en: &str, zh: &str) {
    let lang = Language::current(ui.ctx());
    theme::caption(ui, lang.choose(en, zh));
}
fn rate(ui: &mut egui::Ui, p: &mut AudioFxConfig) {
    let lang = Language::current(ui.ctx());
    parameters::selector(
        ui,
        "audio-fx-sync",
        &mut p.sync_beats,
        &[
            (0.0, lang.choose("Free rate", "自由速度")),
            (0.25, "1/16"),
            (0.5, "1/8"),
            (1.0, "1/4"),
            (2.0, "1/2"),
            (4.0, lang.choose("1 bar", "1 小节")),
            (8.0, lang.choose("2 bars", "2 小节")),
            (16.0, lang.choose("4 bars", "4 小节")),
        ],
    );
    if p.sync_beats == 0.0 {
        value(
            ui,
            &mut p.rate_hz,
            0.05,
            20.0,
            "Rate (Hz)",
            "速度（Hz）",
            true,
        );
    }
    value(ui, &mut p.depth, 0.0, 1.0, "Depth", "深度", false);
}
fn key(ui: &mut egui::Ui, p: &mut AudioFxConfig) {
    let lang = Language::current(ui.ctx());
    ui.horizontal(|ui| {
        parameters::selector(
            ui,
            "audio-fx-key",
            &mut p.root,
            &[
                (0, "C"),
                (1, "C♯"),
                (2, "D"),
                (3, "E♭"),
                (4, "E"),
                (5, "F"),
                (6, "F♯"),
                (7, "G"),
                (8, "A♭"),
                (9, "A"),
                (10, "B♭"),
                (11, "B"),
            ],
        );
        parameters::selector(
            ui,
            "audio-fx-scale",
            &mut p.scale,
            &[
                (Scale::Chromatic, lang.choose("Chromatic", "半音阶")),
                (Scale::Major, lang.choose("Major", "大调")),
                (Scale::Minor, lang.choose("Natural minor", "自然小调")),
            ],
        );
    });
}
pub fn draw(ui: &mut egui::Ui, p: &mut AudioFxConfig, full: bool) {
    let lang = Language::current(ui.ctx());
    if full
        && matches!(
            p.kind,
            K::Transpose | K::Electric | K::Harmonist | K::Octave
        )
    {
        let sr = ui
            .ctx()
            .data(|d| d.get_temp::<u32>(egui::Id::new("audio-fx-sample-rate")))
            .unwrap_or(48000)
            .max(1000);
        let frames = if p.kind == K::Transpose && !p.pitch_sequence && p.semitones == 0.0 {
            0
        } else {
            crate::dsp::pitch_shift::latency_frames(sr as f32)
        };
        ui.label(format!(
            "{}: {} {} · {:.2} ms @ {} Hz",
            lang.choose("Algorithm latency", "算法延迟"),
            frames,
            lang.choose("frames", "帧"),
            frames as f64 * 1000.0 / sr as f64,
            sr
        ));
        theme::caption(ui,lang.choose("Pitch-effect dry and wet paths are aligned. Physical loopback calibration excludes this effect window; automatic inter-track delay compensation is not implemented.","此音高效果的内部干湿声已对齐。声卡回环测量不包含该窗口延迟；目前不自动补偿不同轨道间的效果延迟。"));
    }
    ui.spacing_mut().slider_width = if full {
        ui.available_width().min(500.0) * 0.55
    } else {
        ui.available_width().min(340.0) * 0.46
    };
    match p.kind {
        K::Transpose => {
            navigation::register(
                ui.add(
                    egui::Slider::new(&mut p.semitones, -12.0..=12.0)
                        .step_by(1.0)
                        .text(lang.choose("Semitones", "半音")),
                ),
            );
            if full {
                pitch_lane(ui, p);
            }
            if full {
                theme::caption(ui,lang.choose("Shifts existing audio without changing loop duration. A stereo phase vocoder uses a 2048-frame window at 48 kHz (about 43 ms). Transients can soften and formants are not preserved.","改变已有音频的音高，保持循环时长。立体声相位声码器在 48 kHz 下使用 2048 帧窗口（约 43 ms）；瞬态会变柔和，不保留原共振峰。"));
            }
        }
        K::Electric => {
            key(ui, p);
            navigation::register(
                ui.add(
                    egui::Slider::new(&mut p.semitones, -12.0..=12.0)
                        .step_by(1.0)
                        .text(lang.choose("Pitch shift", "移调（半音）")),
                ),
            );
            value(
                ui,
                &mut p.retune_ms,
                0.0,
                200.0,
                "Retune (ms)",
                "校音过渡（ms）",
                false,
            );
            if full {
                value(
                    ui,
                    &mut p.stability,
                    0.0,
                    1.0,
                    "Note stability",
                    "音高稳定度",
                    false,
                );
                theme::caption(ui,lang.choose("Stepped vocal pitch correction: 0 ms retune creates a hard electric-vocal transition. Use one clear voice at a time. Unvoiced consonants pass through; this is not a polyphonic tuner or a formant-preserving vocal engine.","阶梯式人声校音：过渡为 0 ms 时产生明显电音跳阶。输入应为清晰的单声部人声；无音高的辅音保留原声，不适合多声部校音，目前不保留共振峰。"));
            }
        }
        K::Harmonist => {
            key(ui, p);
            parameters::selector(
                ui,
                "harmony-voice",
                &mut p.harmony_steps,
                &[
                    (-7, "−8"),
                    (-5, "−6"),
                    (-4, "−5"),
                    (-3, "−4"),
                    (-2, "−3"),
                    (0, lang.choose("Unison", "同度")),
                    (2, "+3"),
                    (3, "+4"),
                    (4, "+5"),
                    (5, "+6"),
                    (7, "+8"),
                ],
            );
            value(
                ui,
                &mut p.voice,
                0.0,
                1.0,
                "Harmony level",
                "和声音量",
                false,
            );
            if full {
                value(
                    ui,
                    &mut p.direct,
                    0.0,
                    1.0,
                    "Direct level",
                    "原声音量",
                    false,
                );
                value(ui, &mut p.pan, -1.0, 1.0, "Harmony pan", "和声声像", false);
                value(
                    ui,
                    &mut p.retune_ms,
                    0.0,
                    200.0,
                    "Transition (ms)",
                    "过渡（ms）",
                    false,
                );
                theme::caption(ui,lang.choose("Intervals follow the selected scale, e.g. C major: C → E and D → F for +3. Monophonic detection; formants are not preserved. Chromatic selection uses major-scale harmony degrees.","和声随调式选择音程，例如 C 大调 +3：C → E，D → F。仅分析单声部输入，不保留共振峰；选择半音阶时，和声音程按大调音阶计算。"));
            }
        }
        K::Octave => {
            value(
                ui,
                &mut p.voice,
                0.0,
                1.0,
                "One octave down",
                "低八度",
                false,
            );
            value(
                ui,
                &mut p.octave_two,
                0.0,
                1.0,
                "Two octaves down",
                "低两八度",
                false,
            );
            value(
                ui,
                &mut p.direct,
                0.0,
                1.0,
                "Direct level",
                "原声音量",
                false,
            );
        }
        K::Distortion => {
            parameters::selector(
                ui,
                "drive-style",
                &mut p.drive_style,
                &[
                    (DriveStyle::Soft, lang.choose("Overdrive", "柔和过载")),
                    (DriveStyle::Hard, lang.choose("Hard clip", "硬削波")),
                    (DriveStyle::Fuzz, lang.choose("Fuzz", "法兹")),
                ],
            );
            value(
                ui,
                &mut p.drive_db,
                0.0,
                42.0,
                "Drive (dB)",
                "驱动（dB）",
                false,
            );
            value(
                ui,
                &mut p.tone_hz,
                200.0,
                20000.0,
                "Tone low-pass (Hz)",
                "音色低通（Hz）",
                true,
            );
            if full {
                theme::caption(ui,lang.choose("Three original clipping curves, interpolated 2× shaping, DC removal and a tone filter. These are not simulations of BOSS amplifier or distortion model names.","三种自研削波曲线，插值二倍整形、直流去除与音色滤波；不冒充 BOSS 的音箱或失真型号。"));
            }
        }
        K::Dynamics | K::Sustainer => {
            parameters::selector(
                ui,
                "dynamics-mode",
                &mut p.dynamics_mode,
                &[
                    (DynamicsMode::Compressor, lang.choose("Compressor", "压缩")),
                    (DynamicsMode::Limiter, lang.choose("Limiter", "限幅")),
                    (DynamicsMode::Gate, lang.choose("Noise gate", "噪声门")),
                ],
            );
            value(
                ui,
                &mut p.threshold_db,
                -60.0,
                0.0,
                "Threshold (dBFS)",
                "阈值（dBFS）",
                false,
            );
            if p.dynamics_mode == DynamicsMode::Compressor {
                value(ui, &mut p.ratio, 1.0, 20.0, "Ratio", "压缩比", false);
            }
            value(
                ui,
                &mut p.makeup_db,
                -12.0,
                24.0,
                "Makeup (dB)",
                "补偿增益（dB）",
                false,
            );
            if full {
                value(
                    ui,
                    &mut p.attack_ms,
                    0.1,
                    200.0,
                    "Attack (ms)",
                    "启动（ms）",
                    true,
                );
                value(
                    ui,
                    &mut p.release_ms,
                    10.0,
                    2000.0,
                    "Release (ms)",
                    "释放（ms）",
                    true,
                );
                if p.dynamics_mode != DynamicsMode::Gate {
                    value(
                        ui,
                        &mut p.knee_db,
                        0.0,
                        18.0,
                        "Knee (dB)",
                        "软拐点（dB）",
                        false,
                    );
                }
                theme::caption(ui,lang.choose("Stereo-linked peak detector. Sustainer starts with deeper compression and makeup. Limiter is causal with an output ceiling, not a true-peak or lookahead mastering limiter.","左右声道联动的峰值检测。Sustainer 默认较深压缩与增益补偿。限幅采用无前瞻处理和输出上限，不是真峰值母带限幅器。"));
            }
        }
        K::Equalizer => {
            if full {
                eq_graph(ui, p);
            }
            value(
                ui,
                &mut p.low_db,
                -24.0,
                24.0,
                "Low shelf (dB)",
                "低频搁架（dB）",
                false,
            );
            value(
                ui,
                &mut p.mid_db,
                -24.0,
                24.0,
                "Mid peak (dB)",
                "中频峰值（dB）",
                false,
            );
            value(
                ui,
                &mut p.high_db,
                -24.0,
                24.0,
                "High shelf (dB)",
                "高频搁架（dB）",
                false,
            );
            if full {
                value(
                    ui,
                    &mut p.low_hz,
                    30.0,
                    800.0,
                    "Low frequency (Hz)",
                    "低频频率（Hz）",
                    true,
                );
                value(
                    ui,
                    &mut p.mid_hz,
                    100.0,
                    12000.0,
                    "Mid frequency (Hz)",
                    "中频频率（Hz）",
                    true,
                );
                value(ui, &mut p.mid_q, 0.2, 12.0, "Mid Q", "中频 Q", true);
                value(
                    ui,
                    &mut p.high_hz,
                    1000.0,
                    18000.0,
                    "High frequency (Hz)",
                    "高频频率（Hz）",
                    true,
                );
            }
        }
        K::Pan => value(
            ui,
            &mut p.pan,
            -1.0,
            1.0,
            "Stereo balance L / R",
            "左右声道平衡",
            false,
        ),
        K::StereoEnhance => {
            value(
                ui,
                &mut p.width,
                0.0,
                2.0,
                "Width (0 mono, 1 original)",
                "宽度（0 单声道、1 原声）",
                false,
            );
            if full {
                theme::caption(ui,lang.choose("Mid/side width control preserves the mono sum. It does not synthesize stereo from mono.","中侧声道宽度控制，保持合并单声道后的原始内容，不为单声道凭空生成空间。"));
            }
        }
        K::AutoPan | K::Tremolo | K::Vibrato | K::Phaser | K::Flanger | K::Chorus => {
            rate(ui, p);
            if matches!(p.kind, K::Phaser | K::Flanger) {
                value(ui, &mut p.feedback, 0.0, 0.85, "Feedback", "反馈", false);
            }
            if matches!(p.kind, K::Flanger | K::Chorus) {
                value(
                    ui,
                    &mut p.width,
                    0.0,
                    2.0,
                    "Stereo phase spread",
                    "立体声相位差",
                    false,
                );
            }
            if full {
                theme::caption(ui,lang.choose("Free mode runs at the chosen Hz rate. Synced mode follows the performance sample clock; modulation continues freely while transport is stopped.","自由模式按 Hz 运行；同步模式跟随演出采样时钟。演出停止时调制仍按对应速度自由运行。"));
            }
        }
        K::PanningDelay | K::Delay => {
            parameters::selector(
                ui,
                "delay-sync",
                &mut p.sync_beats,
                &[
                    (0.0, lang.choose("Milliseconds", "毫秒")),
                    (0.25, "1/16"),
                    (0.5, "1/8"),
                    (0.75, "1/8 ·"),
                    (1.0, "1/4"),
                    (1.5, "1/4 ·"),
                    (2.0, "1/2"),
                    (4.0, "1 bar"),
                ],
            );
            if p.sync_beats == 0.0 {
                value(
                    ui,
                    &mut p.time_ms,
                    1.0,
                    2000.0,
                    "Delay time (ms)",
                    "延迟时间（ms）",
                    true,
                );
            } else {
                let bpm = ui
                    .ctx()
                    .data(|d| d.get_temp::<usize>(egui::Id::new("audio-fx-bpm")))
                    .unwrap_or(120)
                    .max(1);
                let requested = 60000.0 / bpm as f32 * p.sync_beats;
                if requested > 2000.0 {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        format!(
                            "{}: {:.0} ms → 2000 ms",
                            lang.choose("2-second buffer limit", "受 2 秒缓冲上限限制"),
                            requested
                        ),
                    );
                } else {
                    ui.label(format!(
                        "{}: {:.1} ms",
                        lang.choose("Effective delay time", "实际延迟时间"),
                        requested
                    ));
                }
            }
            navigation::register(
                ui.add(
                    egui::Slider::new(&mut p.feedback_repeats, 0..=16)
                        .text(lang.choose("Repeats (0 = manual)", "重复次数（0 手动反馈）")),
                ),
            );
            if p.feedback_repeats == 0 {
                value(ui, &mut p.feedback, 0.0, 0.95, "Feedback", "反馈", false);
            }
            value(
                ui,
                &mut p.direct,
                0.0,
                1.0,
                "Direct level",
                "原声音量",
                false,
            );
            value(
                ui,
                &mut p.effect_level,
                0.0,
                1.2,
                "Effect level",
                "效果音量",
                false,
            );
            if p.kind == K::PanningDelay {
                value(
                    ui,
                    &mut p.delay_ratio,
                    0.1,
                    1.0,
                    "Left / right tap ratio",
                    "左／右回声时间比",
                    false,
                );
            }
            if full {
                value(
                    ui,
                    &mut p.high_cut_hz,
                    0.0,
                    20000.0,
                    "Feedback high cut (Hz; 0 = flat)",
                    "反馈高切（Hz；0 直通）",
                    true,
                );
                value(
                    ui,
                    &mut p.low_cut_hz,
                    0.0,
                    12500.0,
                    "Feedback low cut (Hz; 0 = flat)",
                    "反馈低切（Hz；0 直通）",
                    true,
                );
                theme::caption(ui,lang.choose("Repeat count maps feedback to -60 dB after the selected number of echoes; manual mode exposes the coefficient. This decay mapping is not a measured BOSS law.","重复次数将反馈映射为对应次数后降至 -60 dB；手动模式直接设置系数。此衰减映射不是实测的 BOSS 曲线。"));
                if p.kind == K::PanningDelay {
                    value(
                        ui,
                        &mut p.width,
                        0.0,
                        2.0,
                        "Stereo width",
                        "立体声宽度",
                        false,
                    );
                    theme::caption(ui,lang.choose("The left echo arrives at Time × Ratio; the right echo arrives at Time. Later echoes circulate through crossed feedback. This is a stereo multi-tap interpretation of Panning Delay, not a verified BOSS feedback topology.","左侧回声出现在「时间 × 比例」，右侧出现在「时间」，随后交叉反馈循环。这是立体声多抽头的实现，BOSS 的具体内部反馈结构尚未实机核验。"));
                }
            }
        }
        K::StepSlicer => {
            rate(ui, p);
            if full {
                navigation::register(ui.add(
                    egui::Slider::new(&mut p.step_count, 1..=16).text(lang.choose("Steps", "步数")),
                ));
                section(
                    ui,
                    "DRAW STEP LEVELS / drag each bar",
                    "绘制每步音量／拖动柱条",
                );
                let (rect, response) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), 180.0),
                    egui::Sense::click_and_drag(),
                );
                ui.painter().rect_filled(rect, 6.0, theme::BACKGROUND);
                if let Some(pos) = response
                    .interact_pointer_pos()
                    .filter(|_| response.clicked() || response.dragged())
                {
                    let i = (((pos.x - rect.left()) / rect.width()) * p.step_count as f32)
                        .floor()
                        .clamp(0.0, p.step_count as f32 - 1.0) as usize;
                    p.steps[i] = ((rect.bottom() - pos.y) / rect.height()).clamp(0.0, 1.0);
                }
                for i in 0..p.step_count as usize {
                    let w = rect.width() / p.step_count as f32;
                    let bar = egui::Rect::from_min_max(
                        pos2(
                            rect.left() + i as f32 * w + 2.0,
                            rect.bottom() - p.steps[i] * rect.height(),
                        ),
                        pos2(rect.left() + (i + 1) as f32 * w - 2.0, rect.bottom()),
                    );
                    ui.painter().rect_filled(bar, 2.0, theme::accent(ui));
                }
            }
        }
        K::Freeze => {
            navigation::register(ui.checkbox(
                &mut p.freeze,
                lang.choose("Hold current texture", "保持当前声音纹理"),
            ));
            value(
                ui,
                &mut p.attack_ms,
                0.1,
                200.0,
                "Fade in (ms)",
                "渐入（ms）",
                true,
            );
            value(
                ui,
                &mut p.release_ms,
                10.0,
                2000.0,
                "Fade out (ms)",
                "渐出（ms）",
                true,
            );
            if full {
                value(
                    ui,
                    &mut p.decay_ms,
                    100.0,
                    15000.0,
                    "Decay (ms)",
                    "衰减（ms）",
                    true,
                );
                value(
                    ui,
                    &mut p.depth,
                    0.0,
                    1.0,
                    "Sustain level",
                    "持续电平",
                    false,
                );
            }
            if full {
                theme::caption(ui,lang.choose("Holds a 90 ms input texture with overlapping, complementary windows. Turn Hold off to capture a fresh texture. This granular sustain is not a spectral freeze or a beat-synced Roll.","用重叠互补窗延续最近约 90 ms 的声音纹理。取消保持后可捕获新声音；这是颗粒式延音，不是频谱冻结，也不按节拍细分。"));
            }
        }
        K::Reverb => {
            value(
                ui,
                &mut p.decay_ms,
                100.0,
                15000.0,
                "Decay RT60 (ms)",
                "衰减 RT60（ms）",
                true,
            );
            value(ui, &mut p.depth, 0.0, 1.0, "Room size", "空间大小", false);
            if full {
                value(
                    ui,
                    &mut p.predelay_ms,
                    0.0,
                    500.0,
                    "Pre-delay (ms)",
                    "预延迟（ms）",
                    false,
                );
                navigation::register(ui.add(
                    egui::Slider::new(&mut p.density, 1..=10).text(lang.choose("Density", "密度")),
                ));
                value(
                    ui,
                    &mut p.high_cut_hz,
                    0.0,
                    20000.0,
                    "High cut (Hz; 0 = flat)",
                    "高切（Hz；0 直通）",
                    true,
                );
                value(
                    ui,
                    &mut p.low_cut_hz,
                    0.0,
                    12500.0,
                    "Low cut (Hz; 0 = flat)",
                    "低切（Hz；0 直通）",
                    true,
                );
                value(ui, &mut p.width, 0.0, 1.0, "Width", "宽度", false);
            }
        }
    }
    if full {
        ui.separator();
    }
    if !matches!(p.kind, K::PanningDelay | K::Delay) {
        value(ui, &mut p.mix, 0.0, 1.0, "Wet / dry", "干湿比例", false);
    }
    value(
        ui,
        &mut p.level_db,
        -36.0,
        12.0,
        "Output (dB)",
        "输出（dB）",
        false,
    );
}
fn eq_graph(ui: &mut egui::Ui, p: &mut AudioFxConfig) {
    section(
        ui,
        "EQ RESPONSE / drag points · 48 kHz",
        "均衡响应／拖动控制点 · 48 kHz",
    );
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 190.0),
        egui::Sense::click_and_drag(),
    );
    ui.painter().rect_filled(rect, 6.0, theme::BACKGROUND);
    let plot = rect.shrink(12.0);
    let to_pos = |hz: f32, db: f32| {
        pos2(
            plot.left() + (hz / 20.0).log10() / 3.0 * plot.width(),
            plot.center().y - db / 24.0 * plot.height() * 0.5,
        )
    };
    let nodes = [
        to_pos(p.low_hz, p.low_db),
        to_pos(p.mid_hz, p.mid_db),
        to_pos(p.high_hz, p.high_db),
    ];
    let id = response.id.with("drag-band");
    if response.drag_started() || response.clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            let i = (0..3)
                .min_by(|a, b| {
                    nodes[*a]
                        .distance_sq(pos)
                        .total_cmp(&nodes[*b].distance_sq(pos))
                })
                .unwrap();
            ui.ctx().data_mut(|d| d.insert_temp(id, i));
        }
    }
    if let Some(pos) = response
        .interact_pointer_pos()
        .filter(|_| response.dragged() || response.clicked())
    {
        let i = ui.ctx().data(|d| d.get_temp::<usize>(id)).unwrap_or(1);
        let hz = 20.0 * 1000.0_f32.powf(((pos.x - plot.left()) / plot.width()).clamp(0.0, 1.0));
        let db = ((plot.center().y - pos.y) / (plot.height() * 0.5) * 24.0).clamp(-24.0, 24.0);
        match i {
            0 => {
                p.low_hz = hz.clamp(30.0, 800.0);
                p.low_db = db;
            }
            1 => {
                p.mid_hz = hz.clamp(100.0, 12000.0);
                p.mid_db = db;
            }
            _ => {
                p.high_hz = hz.clamp(1000.0, 18000.0);
                p.high_db = db;
            }
        }
    }
    for db in [-24.0, -12.0, 0.0, 12.0, 24.0] {
        ui.painter().hline(
            plot.x_range(),
            to_pos(20.0, db).y,
            Stroke::new(1.0, theme::MUTED.gamma_multiply(0.25)),
        );
    }
    let coeff = [
        crate::dsp::biquad::shelf(48000.0, p.low_hz, p.low_db, false),
        crate::dsp::biquad::peak(48000.0, p.mid_hz, p.mid_q, p.mid_db),
        crate::dsp::biquad::shelf(48000.0, p.high_hz, p.high_db, true),
    ];
    let points = (0..256)
        .map(|i| {
            let hz = 20.0 * 1000.0_f32.powf(i as f32 / 255.0);
            let db = coeff
                .iter()
                .map(|c| c.response_db(48000.0, hz))
                .sum::<f32>();
            to_pos(hz, db.clamp(-24.0, 24.0))
        })
        .collect();
    ui.painter().add(egui::Shape::line(
        points,
        Stroke::new(2.0, theme::accent(ui)),
    ));
    for pos in nodes {
        ui.painter().circle_filled(pos, 6.0, theme::accent(ui));
    }
}
pub fn track_vocoder(
    ui: &mut egui::Ui,
    v: &mut crate::config::vocoder_configs::VocoderConfigs,
    full: bool,
) {
    use crate::config::vocoder_configs::*;
    let lang = Language::current(ui.ctx());
    parameters::selector(
        ui,
        "track-vocoder-carrier",
        &mut v.carrier.value,
        &[
            (
                VocoderCarrier::InputLeft,
                lang.choose("Live input L", "实时输入左声道"),
            ),
            (
                VocoderCarrier::InputRight,
                lang.choose("Live input R", "实时输入右声道"),
            ),
            (VocoderCarrier::Track1, "Track 1"),
            (VocoderCarrier::Track2, "Track 2"),
            (VocoderCarrier::Track3, "Track 3"),
            (VocoderCarrier::Track4, "Track 4"),
            (VocoderCarrier::Track5, "Track 5"),
        ],
    );
    parameters::number(
        ui,
        &mut v.bands,
        VOCODER_BANDS_MIN,
        VOCODER_BANDS_MAX,
        false,
    );
    parameters::number(ui, &mut v.level, 0, 100, false);
    parameters::number(ui, &mut v.mix, 0, 100, false);
    if full {
        parameters::number(ui, &mut v.attack_ms, 0, VOCODER_ATTACK_MAX_MS, false);
        parameters::number(ui, &mut v.release_ms, 0, VOCODER_RELEASE_MAX_MS, false);
        navigation::register(
            ui.add(egui::Slider::new(&mut v.tone, -50..=50).text(lang.text("Tone"))),
        );
        navigation::register(
            ui.add(egui::Slider::new(&mut v.mod_sens, -50..=50).text(lang.text("Mod sensitivity"))),
        );
        navigation::register(
            ui.add(
                egui::Slider::new(&mut v.formant_semitones, -12..=12)
                    .text(lang.text("Formant (semitones)")),
            ),
        );
        parameters::number(ui, &mut v.sibilance, 0, 100, false);
        theme::caption(ui,lang.choose("This track supplies the voice/modulator. The carrier comes from live input or a pre-FX track snapshot shared by all tracks, so track order cannot create feedback. An empty carrier produces silence at 100% wet. Live carrier monitoring is controlled by Input Thru.","当前轨道作为人声／调制信号。载波来自实时输入或所有轨道共享的 FX 前快照，不随轨道处理顺序形成反馈。载波为空时，100% 湿声静音。实时输入载波是否直接监听由 Input Thru 控制。"));
    }
}

pub fn master(
    ui: &mut egui::Ui,
    config: &mut crate::config::audio_fx::MasterFxConfig,
    open: &mut bool,
) {
    let lang = Language::current(ui.ctx());
    section(
        ui,
        "MASTER BUS / after all track faders",
        "主输出效果／位于轨道推子之后",
    );
    ui.horizontal_wrapped(|ui| {
        navigation::register(ui.checkbox(
            &mut config.compressor_enabled,
            lang.choose("Compressor", "主压缩"),
        ));
        navigation::register(
            ui.checkbox(&mut config.reverb_enabled, lang.choose("Reverb", "主混响")),
        );
    });
    if navigation::button(ui, lang.choose("Edit master effects…", "调整主输出效果…")).clicked()
    {
        *open = true;
    }
}
pub fn draw_master(ctx: &egui::Context, app: &mut crate::app::MyApp) {
    if !app.master_fx_open {
        return;
    }
    let mut open = app.master_fx_open;
    let config = &mut app.config.master_fx;
    let lang = app.language;
    let id = egui::Id::new("master-fx-window");
    egui::Window::new(lang.choose("Master effects","主输出效果"))
        .id(id).open(&mut open).default_size(egui::vec2(740.0,680.0)).resizable(true).show(ctx,|ui| {
            navigation::begin(ui,crate::app::Focus::Editor,ui.memory(|m|m.focused().is_none()));
            let page_id=id.with("page");let mut page=ui.ctx().data(|d|d.get_temp::<bool>(page_id)).unwrap_or(false);
            ui.horizontal(|ui| {
                navigation::register(ui.selectable_value(&mut page,false,lang.choose("Compressor / limiter / gate","压缩／限幅／噪声门")));
                navigation::register(ui.selectable_value(&mut page,true,lang.choose("Reverb","混响")));
            });ui.ctx().data_mut(|d|d.insert_temp(page_id,page));ui.separator();
            egui::ScrollArea::vertical().show(ui,|ui| {
                if page {navigation::register(ui.checkbox(&mut config.reverb_enabled,lang.choose("Enabled","启用")));draw(ui,&mut config.reverb,true);}
                else {navigation::register(ui.checkbox(&mut config.compressor_enabled,lang.choose("Enabled","启用")));draw(ui,&mut config.compressor,true);}
                theme::caption(ui,lang.choose("Master processing affects the heard mix and replay export. Track recordings stay unaffected. The monitor-only metronome is added after this bus.","主输出效果作用于听到的混音与回放导出，不写入各轨录音。仅监听的节拍器在主输出处理之后加入。"));
            });
            navigation::end(ui);
        });
    app.master_fx_open = open;
}

/// Relative semitone automation is a control lane, deliberately not a piano roll:
/// the track/input remains audio and the lane does not synthesize new notes.
fn pitch_lane(ui: &mut egui::Ui, p: &mut AudioFxConfig) {
    let lang = Language::current(ui.ctx());
    ui.checkbox(
        &mut p.pitch_sequence,
        lang.choose("Semitone control sequence", "半音控制序列"),
    );
    if !p.pitch_sequence {
        return;
    }
    theme::control_row(ui, |ui| {
        ui.label(lang.choose("Steps", "步数"));
        ui.add(egui::DragValue::new(&mut p.pitch_step_count).clamp_range(1..=16));
        parameters::selector(
            ui,
            "pitch_step_beats",
            &mut p.pitch_step_beats,
            &[
                (0.125, "1/32"),
                (0.25, "1/16"),
                (0.5, "1/8"),
                (1.0, "1/4"),
                (2.0, "1/2"),
                (4.0, "1 bar"),
            ],
        );
        if ui.button(lang.choose("Reset steps", "全部归零")).clicked() {
            p.pitch_steps = [0.0; 16];
        }
        if ui
            .button(lang.choose("Octave pattern", "八度示例"))
            .clicked()
        {
            p.pitch_steps = [
                0.0, 0.0, 7.0, 7.0, 12.0, 12.0, 7.0, 0.0, 0.0, 0.0, 7.0, 7.0, 12.0, 12.0, 7.0, 0.0,
            ];
        }
    });
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 210.0),
        egui::Sense::click_and_drag(),
    );
    ui.painter().rect_filled(rect, 6.0, theme::BACKGROUND);
    let plot = rect.shrink2(egui::vec2(32.0, 20.0));
    if let Some(pos) = response
        .interact_pointer_pos()
        .filter(|_| response.clicked() || response.dragged())
    {
        if plot.contains(pos) {
            let step = ((pos.x - plot.left()) / plot.width() * p.pitch_step_count as f32)
                .floor()
                .clamp(0.0, p.pitch_step_count as f32 - 1.0) as usize;
            p.pitch_steps[step] = ((plot.center().y - pos.y) / plot.height() * 24.0)
                .round()
                .clamp(-12.0, 12.0);
        }
    }
    for semitones in [-12, -7, 0, 7, 12] {
        let y = plot.center().y - semitones as f32 / 24.0 * plot.height();
        ui.painter().hline(
            plot.x_range(),
            y,
            Stroke::new(
                1.0,
                theme::MUTED.gamma_multiply(if semitones == 0 { 0.7 } else { 0.2 }),
            ),
        );
        ui.painter().text(
            pos2(plot.left() - 5.0, y),
            egui::Align2::RIGHT_CENTER,
            format!("{semitones:+}"),
            egui::FontId::monospace(12.0),
            theme::MUTED,
        );
    }
    let width = plot.width() / p.pitch_step_count.max(1) as f32;
    for (i, value) in p
        .pitch_steps
        .iter()
        .enumerate()
        .take(p.pitch_step_count as usize)
    {
        let x = plot.left() + width * i as f32;
        let y = plot.center().y - value / 24.0 * plot.height();
        ui.painter().line_segment(
            [pos2(x + 2.0, y), pos2(x + width - 2.0, y)],
            Stroke::new(4.0, theme::accent(ui)),
        );
        ui.painter().text(
            pos2(x + width * 0.5, rect.bottom() - 2.0),
            egui::Align2::CENTER_BOTTOM,
            format!("{value:+.0}"),
            egui::FontId::monospace(12.0),
            theme::MUTED,
        );
    }
    theme::caption(ui,lang.choose("Draw relative semitones from −12 to +12. The running transport replaces the base Semitones value with each step; stopped transport uses the base value. The lane processes existing audio and produces no sound on its own.","拖画 −12 至 +12 半音。演出运行时每步替代基础移调值；停止时使用基础值。此序列只处理已有音频，不自行发声。"));
}
