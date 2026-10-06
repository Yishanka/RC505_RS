//! OSC-specific sound and modulation controls; quick and expanded views share config.
use super::{parameters, theme};
use crate::config::osc_configs::*;
use eframe::egui::{self, pos2, vec2};

#[cfg(test)]
#[path = "synth_threshold_tests.rs"]
mod synth_threshold_tests;

pub fn sound(ui: &mut egui::Ui, osc: &mut OscillatorConfigs, full: bool) {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    osc.poll_sample();
    parameters::choice(ui, &mut osc.waveform);
    if full
        && osc.waveform.value != Waveform::Sample
        && (osc.capture.is_some() || osc.sample_job.is_some())
    {
        theme::control_row(ui, |ui| capture_button(ui, osc));
        capture_threshold(ui, osc);
    }
    if !full || osc.waveform.value != Waveform::Sample {
        capture_status(ui, osc);
    }
    if full {
        ui.columns(2, |cols| {
            parameters::number(&mut cols[0], &mut osc.level, 0, 100, false);
            voice_count(&mut cols[0], osc);
            mono_controls(&mut cols[0], osc);
            input_controls(&mut cols[1], osc);
        });
    } else {
        parameters::number(ui, &mut osc.level, 0, 100, false);
        input_controls(ui, osc);
        voice_count(ui, osc);
        if osc.voices == 1 {
            mono_controls(ui, osc);
        }
    }
    if !full {
        if osc.waveform.value == Waveform::Sample
            || osc.capture.is_some()
            || osc.sample_job.is_some()
        {
            capture_threshold(ui, osc);
        }
        if osc.waveform.value == Waveform::Sample && osc.sample.is_none() {
            theme::caption(
                ui,
                lang.choose(
                    "Capture a sound here, or expand to import a WAV.",
                    "可直接捕获音色，或展开后导入 WAV。",
                ),
            );
        }
        return;
    }
    if osc.waveform.value == Waveform::Vocal {
        parameters::float_with_keys(
            ui,
            &mut osc.vocal_formant,
            0.0,
            1.0,
            0.01,
            0.05,
            lang.choose("Vowel A → I", "元音 A → I"),
            false,
        );
        theme::caption(
            ui,
            lang.choose(
                "Drag to move between A and I vowels.",
                "拖动参数，切换 A/I 元音。",
            ),
        );
    }
    if osc.waveform.value == Waveform::Sample {
        sample_controls(ui, osc);
    } else {
        waveform(ui, osc);
    }
}

fn mono_controls(ui: &mut egui::Ui, osc: &mut OscillatorConfigs) {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    ui.add_enabled_ui(osc.voices == 1, |ui| {
        checkbox(ui, &mut osc.mono_legato, lang.choose("Legato", "连奏")).on_hover_text(
            lang.choose(
                "Keep the envelope and LFO phase between overlapping notes.",
                "重叠音符保持包络与 LFO 相位。",
            ),
        );
        parameters::float(
            ui,
            &mut osc.glide_ms,
            0.0,
            2000.0,
            0.1,
            lang.choose("Glide (ms)", "滑音（ms）"),
            true,
        );
        if osc.glide_ms > 0.0 {
            parameters::selector(
                ui,
                "glide_mode",
                &mut osc.glide_mode,
                &[
                    (
                        GlideMode::Overlap,
                        lang.choose("Overlapping notes", "重叠音符"),
                    ),
                    (GlideMode::AllNotes, lang.choose("All notes", "所有音符")),
                ],
            );
        }
    });
}
fn voice_count(ui: &mut egui::Ui, osc: &mut OscillatorConfigs) {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    theme::control_row(ui, |ui| {
        ui.label(lang.choose("Voices", "声部"));
        for (count, en, cn) in [
            (1, "Mono", "单音"),
            (8, "Poly 8", "复音 8"),
            (16, "Poly 16", "复音 16"),
        ] {
            selectable_value(ui, &mut osc.voices, count, lang.choose(en, cn));
        }
    });
}
fn input_controls(ui: &mut egui::Ui, osc: &mut OscillatorConfigs) {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    let mut dry = osc.dry_level * 100.0;
    if parameters::float(
        ui,
        &mut dry,
        0.0,
        100.0,
        1.0,
        lang.choose("Dry level (%)", "原声电平（%）"),
        false,
    )
    .changed()
    {
        osc.dry_level = dry / 100.0;
    }
    let mut follow = osc.input_mod_sens.is_some();
    if checkbox(
        ui,
        &mut follow,
        lang.choose("Follow input envelope", "跟随输入包络"),
    )
    .on_hover_text(lang.choose(
        "Input loudness shapes OSC volume; the phrase still triggers notes.",
        "输入音量连续控制 OSC 音量，音符仍由乐句触发。",
    ))
    .changed()
    {
        osc.input_mod_sens = follow.then_some(0.0);
    }
    let mut sensitivity = osc.input_mod_sens.unwrap_or(0.0);
    ui.add_enabled_ui(follow, |ui| {
        if parameters::float(
            ui,
            &mut sensitivity,
            -50.0,
            50.0,
            1.0,
            lang.choose("Input sensitivity", "输入灵敏度"),
            false,
        )
        .changed()
        {
            osc.input_mod_sens = Some(sensitivity);
        }
    });
    checkbox(
        ui,
        &mut osc.input_gate,
        lang.choose("Gate notes with live input", "用输入音量控制音符门控"),
    );
    if osc.input_gate {
        parameters::number(ui, &mut osc.gate_threshold, 0, 100, false);
    }
}

pub(super) fn capture_button(ui: &mut egui::Ui, osc: &mut OscillatorConfigs) {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    if osc.capture.is_some() {
        if button(ui, lang.choose("Cancel capture", "取消捕获")).clicked() {
            osc.capture = None;
            osc.capture_serial = osc.capture_serial.wrapping_add(1);
        }
    } else if add_enabled(
        ui,
        osc.sample_job.is_none(),
        egui::Button::new(lang.choose("Capture sound", "捕获音色")),
    )
    .on_hover_text(lang.choose(
        "Capture input from this active FX bank. Adjust the trigger threshold beside the capture controls.",
        "从当前效果组的输入捕获；触发阈值在捕获控件旁调整。",
    ))
    .clicked()
    {
        osc.capture_serial = osc.capture_serial.wrapping_add(1);
        osc.capture = Some(std::sync::Arc::new(SampleCapture::new(osc.capture_ms)));
    }
}

fn capture_status(ui: &mut egui::Ui, osc: &OscillatorConfigs) {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    if let Some(capture) = &osc.capture {
        let started = capture.state.load(std::sync::atomic::Ordering::Acquire) > 0;
        theme::caption(
            ui,
            if started {
                lang.choose("Capturing…", "正在捕获…")
            } else {
                lang.choose(
                    "Waiting for input above capture threshold…",
                    "等待输入超过捕获阈值…",
                )
            },
        );
    } else if osc.sample_job.is_some() {
        ui.spinner();
    }
}

fn capture_threshold(ui: &mut egui::Ui, osc: &mut OscillatorConfigs) {
    parameters::number(ui, &mut osc.capture_threshold, 0, 100, false);
}

fn sample_controls(ui: &mut egui::Ui, osc: &mut OscillatorConfigs) {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    capture_threshold(ui, osc);
    parameters::integer(
        ui,
        &mut osc.capture_ms,
        20,
        2000,
        lang.choose("Capture (ms)", "采样时长（ms）"),
    );
    theme::control_row(ui, |ui| {
        if add_enabled(
            ui,
            osc.sample_job.is_none() && osc.capture.is_none(),
            egui::Button::new(lang.choose("Import WAV…", "导入 WAV…")),
        )
        .clicked()
        {
            let (tx, rx) = std::sync::mpsc::channel();
            osc.sample_job = Some(rx);
            std::thread::spawn(move || {
                let result = pick_sample()
                    .ok_or_else(|| "Import cancelled / 已取消导入".into())
                    .and_then(|p| SampleAsset::load_wav(&p).map_err(|e| e.to_string()));
                let _ = tx.send(result);
            });
        }
        capture_button(ui, osc);
        if (osc.sample.is_some()
            || osc.sample_ref.is_some()
            || osc.sample_job.is_some()
            || osc.capture.is_some())
            && button(ui, lang.choose("Clear current sample", "清除当前采样"))
                .on_hover_text(lang.choose(
                    "Remove this slot's sample. Saved sounds are managed in the sound library.",
                    "移除此槽的采样；已保存音色可在音色库中删除。",
                ))
                .clicked()
        {
            osc.clear_sample();
        }
    });
    // A dropped WAV uses the same bounded worker import path as the native picker.
    if osc.sample_job.is_none() && osc.capture.is_none() {
        if let Some(path) = ui.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone())) {
            let (tx, rx) = std::sync::mpsc::channel();
            osc.sample_job = Some(rx);
            std::thread::spawn(move || {
                let _ = tx.send(SampleAsset::load_wav(&path).map_err(|e| e.to_string()));
            });
        }
    }
    capture_status(ui, osc);
    if !osc.sample_message.is_empty() {
        ui.label(lang.text(&osc.sample_message));
    }
    let previous_mode = osc.sample_mode;
    ui.horizontal(|ui| {
        selectable_value(
            ui,
            &mut osc.sample_mode,
            SampleMode::Wavetable,
            lang.choose("Waveform", "采样波形"),
        );
        selectable_value(
            ui,
            &mut osc.sample_mode,
            SampleMode::Sampler,
            lang.choose("Sampler", "采样音色"),
        );
    });
    if previous_mode != osc.sample_mode {
        osc.select_sample_region();
    }
    if osc.sample_mode == SampleMode::Sampler {
        parameters::integer(
            ui,
            &mut osc.sample_root,
            0,
            119,
            lang.choose("Root note", "原音高"),
        );
        ui.horizontal(|ui| {
            ui.label(
                crate::config::note_configs::NoteOct::from_pitch_index(osc.sample_root).to_string(),
            );
            checkbox(
                ui,
                &mut osc.sample_loop,
                lang.choose("Loop while held", "按住时循环"),
            );
        });
        parameters::float(
            ui,
            &mut osc.sample_fine_cents,
            -100.0,
            100.0,
            0.1,
            lang.choose("Fine tune (cents)", "微调（音分）"),
            false,
        );
    }
    let Some(sample) = &osc.sample else {
        theme::caption(
            ui,
            lang.choose(
                "Import or drop a WAV here, or capture the input.",
                "导入或拖入 WAV，也可捕获输入。",
            ),
        );
        return;
    };
    ui.label(format!(
        "{} · {:.3} s · {} Hz",
        sample.name,
        sample.frames.len() as f32 / sample.sample_rate as f32,
        sample.sample_rate
    ));
    if let Some(hz) = sample.root_hz {
        ui.label(format!(
            "{}: {hz:.1} Hz",
            lang.choose("Detected fundamental", "检测到的基频")
        ));
    } else {
        theme::caption(
            ui,
            lang.choose(
                "No stable cycle found. Adjust the sample region.",
                "未识别稳定周期，请调整采样区域。",
            ),
        );
    }
    let duration_ms = sample.frames.len() as f32 * 1000.0 / sample.sample_rate.max(1) as f32;
    let minimum_ms = 2000.0 / sample.sample_rate.max(1) as f32;
    if osc.sample_mode == SampleMode::Wavetable
        && button(ui, lang.choose("Use detected cycle", "选择检测到的周期")).clicked()
    {
        osc.select_sample_region();
    }
    let mut start_ms = osc.sample_start * duration_ms;
    let mut end_ms = osc.sample_end * duration_ms;
    ui.columns(2, |cols| {
        if parameters::float(
            &mut cols[0],
            &mut start_ms,
            0.0,
            (end_ms - minimum_ms).max(0.0),
            minimum_ms * 0.5,
            lang.choose("Start (ms)", "起点（ms）"),
            false,
        )
        .changed()
            && start_ms.is_finite()
        {
            osc.sample_start = start_ms / duration_ms.max(0.001);
        }
        if parameters::float(
            &mut cols[1],
            &mut end_ms,
            (start_ms + minimum_ms).min(duration_ms),
            duration_ms,
            minimum_ms * 0.5,
            lang.choose("End (ms)", "终点（ms）"),
            false,
        )
        .changed()
            && end_ms.is_finite()
        {
            osc.sample_end = end_ms / duration_ms.max(0.001);
        }
    });
    osc.sanitize_source();
    waveform(ui, osc);
    theme::caption(ui,lang.choose("Waveform loops the selected cycle; Sampler follows the root note. Samples are mono, up to 2 seconds.","采样波形循环所选周期；采样音色按原音高重奏。素材转为单声道，最长 2 秒。"));
}

fn waveform(ui: &mut egui::Ui, osc: &OscillatorConfigs) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 170.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 6.0, theme::BACKGROUND);
    if let Some(sample) = osc
        .sample
        .as_ref()
        .filter(|_| osc.waveform.value == Waveform::Sample)
    {
        if !sample.frames.is_empty() {
            // Draw peak ranges, not only point samples, so short transients remain visible.
            for i in 0..400 {
                let a = i * sample.frames.len() / 400;
                let b = ((i + 1) * sample.frames.len() / 400)
                    .max(a + 1)
                    .min(sample.frames.len());
                let (low, high) = sample.frames[a..b]
                    .iter()
                    .fold((0.0f32, 0.0f32), |(lo, hi), x| (lo.min(*x), hi.max(*x)));
                let x = rect.left() + rect.width() * i as f32 / 399.0;
                ui.painter().vline(
                    x,
                    (rect.center().y - high * rect.height() * 0.42)
                        ..=(rect.center().y - low * rect.height() * 0.42),
                    egui::Stroke::new(1.0, theme::accent(ui)),
                );
            }
            for fraction in [osc.sample_start, osc.sample_end] {
                ui.painter().vline(
                    rect.left() + rect.width() * fraction,
                    rect.y_range(),
                    egui::Stroke::new(2.0, theme::secondary(ui)),
                );
            }
        }
    } else {
        let source_preview = if matches!(
            osc.waveform.value,
            Waveform::Vocal | Waveform::Rect | Waveform::VintageSaw | Waveform::DetuneSaw
        ) {
            Some(crate::dsp::oscillator::source_wave_preview(osc))
        } else {
            None
        };
        let points = (0..400)
            .map(|i| {
                let phase = (i as f32 / 200.0).fract();
                let value = match osc.waveform.value {
                    Waveform::Sine => (phase * std::f32::consts::TAU).sin(),
                    Waveform::Saw => 2.0 * phase - 1.0,
                    Waveform::Square => {
                        if phase < 0.5 {
                            1.0
                        } else {
                            -1.0
                        }
                    }
                    Waveform::Triangle => 1.0 - 4.0 * (phase - 0.5).abs(),
                    Waveform::Vocal
                    | Waveform::Rect
                    | Waveform::VintageSaw
                    | Waveform::DetuneSaw => source_preview.as_ref().unwrap()[i],
                    Waveform::Sample => 0.0,
                };
                pos2(
                    rect.left() + rect.width() * i as f32 / 399.0,
                    rect.center().y - value * rect.height() * 0.38,
                )
            })
            .collect();
        ui.painter().add(egui::Shape::line(
            points,
            egui::Stroke::new(2.0, theme::accent(ui)),
        ));
    }
}

pub fn modulation(ui: &mut egui::Ui, osc: &mut OscillatorConfigs) {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    let id = ui.id().with("lfo_editor_selection");
    let mut selected = ui
        .ctx()
        .data(|data| data.get_temp::<usize>(id))
        .unwrap_or_else(|| usize::from(!osc.lfo.enabled && osc.lfo2.enabled))
        .min(1);
    theme::control_row(ui, |ui| {
        for (index, enabled) in [osc.lfo.enabled, osc.lfo2.enabled].into_iter().enumerate() {
            let text = format!(
                "LFO {} · {}",
                index + 1,
                lang.choose(
                    if enabled { "On" } else { "Off" },
                    if enabled { "开" } else { "关" }
                )
            );
            selectable_value(ui, &mut selected, index, text);
        }
    });
    ui.ctx().data_mut(|data| data.insert_temp(id, selected));
    ui.push_id(("lfo", selected), |ui| {
        lfo(
            ui,
            if selected == 0 {
                &mut osc.lfo
            } else {
                &mut osc.lfo2
            },
        )
    });
    theme::caption(ui,lang.choose("Each LFO has its own clock and target. Same-target modulation combines; switching tabs does not reset playback.","两个 LFO 独立运行；同一目标可叠加调制，切换编辑页不重置播放。"));
    let active = if selected == 0 { &osc.lfo } else { &osc.lfo2 };
    let bypassed =
        active.enabled && active.target == LfoTarget::Cutoff && osc.osc_filter.mix.value == 0;
    ui.label(
        egui::RichText::new(if bypassed {
            lang.choose(
                "Cutoff modulation needs the internal filter enabled.",
                "截止调制需要打开内部滤波。",
            )
        } else {
            " "
        })
        .small()
        .color(theme::MUTED),
    );
}

pub fn lfo(ui: &mut egui::Ui, lfo: &mut LfoConfig) {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    checkbox(ui, &mut lfo.enabled, lang.choose("Enable LFO", "启用 LFO"));
    theme::control_row(ui, |ui| {
        parameters::selector(
            ui,
            "lfo_shape",
            &mut lfo.shape,
            &[
                (LfoShape::Sine, lang.choose("Sine", "正弦")),
                (LfoShape::Triangle, lang.choose("Triangle", "三角")),
                (LfoShape::Saw, lang.choose("Ramp", "斜坡")),
                (LfoShape::Square, lang.choose("Square", "方波")),
                (LfoShape::Custom, lang.choose("Draw curve", "绘制曲线")),
            ],
        );
        parameters::selector(
            ui,
            "lfo_target",
            &mut lfo.target,
            &[
                (LfoTarget::Volume, lang.choose("Volume", "音量")),
                (LfoTarget::Cutoff, lang.choose("Filter cutoff", "滤波截止")),
                (
                    LfoTarget::Pitch,
                    lang.choose("Pitch ±12 st", "音高 ±12 半音"),
                ),
            ],
        );
        parameters::selector(
            ui,
            "lfo_mode",
            &mut lfo.mode,
            &[
                (LfoMode::Free, lang.choose("Free phase", "自由相位")),
                (
                    LfoMode::Retrigger,
                    lang.choose("Retrigger per note", "每个音符重触发"),
                ),
            ],
        );
    });
    checkbox(ui, &mut lfo.sync, lang.choose("Sync to tempo", "跟随速度"));
    if lfo.sync {
        parameters::float_with_keys(
            ui,
            &mut lfo.beats,
            0.0625,
            32.0,
            0.0625,
            0.25,
            lang.choose("Period (beats)", "周期（拍）"),
            true,
        );
    } else {
        parameters::float_with_keys(
            ui,
            &mut lfo.rate_hz,
            0.01,
            40.0,
            0.01,
            0.1,
            lang.choose("Rate (Hz)", "频率（Hz）"),
            true,
        );
    }
    let mut depth_pct = lfo.depth * 100.0;
    if parameters::float(
        ui,
        &mut depth_pct,
        0.0,
        100.0,
        1.0,
        lang.choose("Depth (%)", "深度（%）"),
        false,
    )
    .changed()
    {
        lfo.depth = depth_pct / 100.0;
    }
    lfo.sanitize();
    curve_editor(ui, lfo);
    theme::caption(ui,lang.choose("LFO runs independently from AHDSR. Volume modulation cannot reopen a released note. Cutoff moves in octaves; pitch depth spans up to ±12 semitones.","音量调制受包络控制；每个音高 LFO 最大为 ±12 半音。"));
}

fn curve_editor(ui: &mut egui::Ui, lfo: &mut LfoConfig) -> egui::Rect {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), 220.0), egui::Sense::click());
    let plot = rect.shrink(14.0);
    ui.painter().rect_filled(rect, 6.0, theme::BACKGROUND);
    for i in 0..=4 {
        let x = plot.left() + plot.width() * i as f32 / 4.0;
        ui.painter().vline(
            x,
            plot.y_range(),
            egui::Stroke::new(1.0, egui::Color32::from_gray(45)),
        );
    }
    if response.double_clicked() {
        if let Some(p) = response.interact_pointer_pos() {
            let x = ((p.x - plot.left()) / plot.width()).clamp(LFO_POINT_GAP, 1.0 - LFO_POINT_GAP);
            if plot.contains(p)
                && lfo.points.len() < LFO_MAX_POINTS
                && lfo
                    .points
                    .iter()
                    .all(|point| (point.x - x).abs() >= LFO_POINT_GAP)
            {
                lfo.shape = LfoShape::Custom;
                lfo.points.push(CurvePoint {
                    x,
                    y: (1.0 - (p.y - plot.top()) / plot.height()).clamp(0.0, 1.0),
                    curve: 0.0,
                });
                lfo.sanitize();
            }
        }
    }
    if lfo.shape == LfoShape::Custom {
        let mut remove = None;
        for i in 0..lfo.points.len() {
            let p = lfo.points[i];
            let pos = pos2(
                plot.left() + plot.width() * p.x,
                plot.bottom() - plot.height() * p.y,
            );
            let handle = ui.interact(
                egui::Rect::from_center_size(pos, vec2(14.0, 14.0)),
                ui.id().with(("lfo_point", i)),
                egui::Sense::click_and_drag(),
            );
            if handle.dragged() {
                if let Some(pos) = handle.interact_pointer_pos() {
                    if i > 0 && i + 1 < lfo.points.len() {
                        let minimum = lfo.points[i - 1].x + LFO_POINT_GAP;
                        let maximum = lfo.points[i + 1].x - LFO_POINT_GAP;
                        // Extremely close imported points can leave no horizontal
                        // room after float rounding. Keep this node's identity
                        // and x position while still allowing a vertical drag.
                        if minimum <= maximum {
                            lfo.points[i].x =
                                ((pos.x - plot.left()) / plot.width()).clamp(minimum, maximum);
                        }
                    }
                    lfo.points[i].y = (1.0 - (pos.y - plot.top()) / plot.height()).clamp(0.0, 1.0);
                }
            }
            if handle.secondary_clicked()
                && !ui.input(|input| input.pointer.primary_down())
                && i > 0
                && i + 1 < lfo.points.len()
            {
                remove = Some(i);
            }
            if i + 1 < lfo.points.len() {
                let next = lfo.points[i + 1];
                let mid_x = (p.x + next.x) * 0.5;
                let mid_y = p.y + (next.y - p.y) * crate::dsp::envelope::bend_curve(0.5, p.curve);
                let mid = pos2(
                    plot.left() + plot.width() * mid_x,
                    plot.bottom() - plot.height() * mid_y,
                );
                let response = ui.interact(
                    egui::Rect::from_center_size(mid, vec2(12.0, 12.0)),
                    ui.id().with(("lfo_curve", i)),
                    egui::Sense::click_and_drag(),
                );
                if response.dragged() {
                    lfo.points[i].curve = (lfo.points[i].curve
                        + ui.input(|i| i.pointer.delta().y)
                            * 0.012
                            * if next.y > p.y { 1.0 } else { -1.0 })
                    .clamp(-1.0, 1.0);
                }
                if response.double_clicked() {
                    lfo.points[i].curve = 0.0;
                }
            }
        }
        if let Some(i) = remove {
            lfo.points.remove(i);
        }
    }
    let points = if lfo.shape == LfoShape::Custom {
        let mut path = Vec::with_capacity(lfo.points.len().saturating_sub(1) * 48 + 1);
        for (segment, pair) in lfo.points.windows(2).enumerate() {
            let (a, b) = (pair[0], pair[1]);
            for step in usize::from(segment > 0)..=48 {
                let t = step as f32 / 48.0;
                path.push(pos2(
                    plot.left() + plot.width() * (a.x + (b.x - a.x) * t),
                    plot.bottom()
                        - plot.height()
                            * (a.y + (b.y - a.y) * crate::dsp::envelope::bend_curve(t, a.curve)),
                ));
            }
        }
        path
    } else {
        (0..400)
            .map(|i| {
                let t = i as f32 / 399.0;
                pos2(
                    plot.left() + plot.width() * t,
                    plot.bottom() - plot.height() * crate::dsp::oscillator::lfo_value(lfo, t),
                )
            })
            .collect()
    };
    ui.painter().add(egui::Shape::line(
        points,
        egui::Stroke::new(2.0, theme::accent(ui)),
    ));
    // Paint once from the final edited values. The old loop painted each handle
    // before moving it, while the line already used its new position.
    if lfo.shape == LfoShape::Custom {
        for (index, point) in lfo.points.iter().enumerate() {
            let pos = pos2(
                plot.left() + plot.width() * point.x,
                plot.bottom() - plot.height() * point.y,
            );
            if let Some(next) = lfo.points.get(index + 1) {
                let mid = pos2(
                    plot.left() + plot.width() * (point.x + next.x) * 0.5,
                    plot.bottom()
                        - plot.height()
                            * (point.y
                                + (next.y - point.y)
                                    * crate::dsp::envelope::bend_curve(0.5, point.curve)),
                );
                ui.painter().circle_filled(mid, 3.5, theme::secondary(ui));
            }
            ui.painter().circle_filled(pos, 5.0, theme::accent(ui));
        }
    }
    theme::caption(ui,lang.choose("Double-click: add point · Drag points: shape · Drag small midpoint: curvature · Right-click interior point: delete","双击加点 · 拖动节点塑形 · 拖动小中点调整曲率 · 右键删除内部节点"));
    plot
}

#[cfg(test)]
#[path = "synth_controls_curve_tests.rs"]
mod curve_tests;

#[cfg(windows)]
fn pick_sample() -> Option<std::path::PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows::Win32::UI::Controls::Dialogs::{
        GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST, OPENFILENAMEW,
    };
    use windows::core::{PCWSTR, PWSTR};
    let filter: Vec<u16> = "WAV audio\0*.wav\0\0".encode_utf16().collect();
    let title: Vec<u16> = "OSC — Import sample / 导入采样\0".encode_utf16().collect();
    let mut path = vec![0u16; 32768];
    let mut dialog = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(path.as_mut_ptr()),
        nMaxFile: path.len() as u32,
        lpstrTitle: PCWSTR(title.as_ptr()),
        Flags: OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR,
        ..Default::default()
    };
    if unsafe { GetOpenFileNameW(&mut dialog) }.as_bool() {
        let len = path.iter().position(|c| *c == 0).unwrap_or(path.len());
        Some(std::ffi::OsString::from_wide(&path[..len]).into())
    } else {
        None
    }
}
#[cfg(not(windows))]
fn pick_sample() -> Option<std::path::PathBuf> {
    None
}

fn add_enabled(ui: &mut egui::Ui, enabled: bool, widget: impl egui::Widget) -> egui::Response {
    crate::ui::navigation::register(ui.add_enabled(enabled, widget))
}
fn button(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) -> egui::Response {
    crate::ui::navigation::register(ui.button(text))
}
fn checkbox(
    ui: &mut egui::Ui,
    value: &mut bool,
    text: impl Into<egui::WidgetText>,
) -> egui::Response {
    crate::ui::navigation::register(ui.checkbox(value, text))
}
fn selectable_value<T: PartialEq>(
    ui: &mut egui::Ui,
    value: &mut T,
    choice: T,
    text: impl Into<egui::WidgetText>,
) -> egui::Response {
    crate::ui::navigation::register(ui.selectable_value(value, choice, text))
}
