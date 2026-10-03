//! OSC-specific sound and modulation controls; quick and expanded views share config.
use super::{parameters, theme};
use crate::config::osc_configs::*;
use eframe::egui::{self, pos2, vec2};

pub fn sound(ui: &mut egui::Ui, osc: &mut OscillatorConfigs, full: bool) {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    osc.poll_sample();
    parameters::choice(ui, &mut osc.waveform);
    parameters::number(ui, &mut osc.level, 0, 100, false);
    ui.horizontal(|ui| {
        ui.label(lang.choose("Voices", "声部"));
        for (count, en, cn) in [
            (1, "Mono", "单音"),
            (8, "Poly 8", "复音 8"),
            (16, "Poly 16", "复音 16"),
        ] {
            selectable_value(ui, &mut osc.voices, count, lang.choose(en, cn));
        }
    });
    checkbox(
        ui,
        &mut osc.input_gate,
        lang.choose("Gate notes with live input", "用输入音量控制音符门控"),
    );
    if osc.input_gate || osc.waveform.value == Waveform::Sample {
        parameters::number(ui, &mut osc.threshold, 0, 100, false);
    }
    if !full {
        if osc.waveform.value == Waveform::Sample && osc.sample.is_none() {
            theme::caption(
                ui,
                lang.choose(
                    "Sample missing: expand to capture or import.",
                    "缺少采样：展开后捕获或导入素材。",
                ),
            );
        }
        return;
    }
    theme::caption(ui,lang.choose("Sound presets keep your phrase. The piano roll sends independent notes; Mono retains chords for later polyphonic playback.","切换音色保留乐句。卷帘发送独立音符；单音模式仍保留和弦，切回复音即可演奏。"));
    if osc.waveform.value == Waveform::Vocal {
        add(
            ui,
            egui::Slider::new(&mut osc.vocal_formant, 0.0..=1.0)
                .text(lang.choose("Vowel A → I", "元音 A → I")),
        );
        theme::caption(ui,lang.choose("Synthetic harmonic vowel wave; this is a voice-like oscillator, not a vocal recording.","合成的元音谐波波形，用于人声质感音色。"));
    }
    if osc.waveform.value == Waveform::Sample {
        sample_controls(ui, osc);
    } else {
        waveform(ui, osc);
    }
}

fn sample_controls(ui: &mut egui::Ui, osc: &mut OscillatorConfigs) {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    theme::control_row(ui, |ui| {
        if add_enabled(
            ui,
            osc.sample_job.is_none(),
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
        ui.label(lang.choose("Capture ms", "采样时长 ms"));
        add(
            ui,
            egui::DragValue::new(&mut osc.capture_ms).clamp_range(20..=2000),
        );
        if add_enabled(
            ui,
            osc.capture.is_none() && osc.sample_job.is_none(),
            egui::Button::new(lang.choose("Capture input", "捕获输入")),
        )
        .clicked()
        {
            osc.capture_serial = osc.capture_serial.wrapping_add(1);
            osc.capture = Some(std::sync::Arc::new(SampleCapture::new(osc.capture_ms)));
        }
        if osc.capture.is_some() && button(ui, lang.choose("Cancel capture", "取消捕获")).clicked()
        {
            osc.capture = None;
            osc.capture_serial = osc.capture_serial.wrapping_add(1);
        }
        if osc.sample.is_some() && button(ui, lang.choose("Clear sample", "移除采样")).clicked()
        {
            osc.sample = None;
        }
    });
    // A dropped WAV uses the same bounded worker import path as the native picker.
    if osc.sample_job.is_none() {
        if let Some(path) = ui.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone())) {
            let (tx, rx) = std::sync::mpsc::channel();
            osc.sample_job = Some(rx);
            std::thread::spawn(move || {
                let _ = tx.send(SampleAsset::load_wav(&path).map_err(|e| e.to_string()));
            });
        }
    }
    if let Some(capture) = &osc.capture {
        let started = capture.state.load(std::sync::atomic::Ordering::Acquire) > 0;
        ui.label(if started {
            lang.choose("Capturing…", "正在捕获…")
        } else {
            lang.choose(
                "Armed: waiting for input above Threshold in this bank.",
                "已准备：等待当前效果组的输入超过阈值。",
            )
        });
    }
    if osc.sample_job.is_some() {
        ui.spinner();
    }
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
        ui.horizontal(|ui| {
            ui.label(lang.choose("Root note", "素材原音高"));
            add(
                ui,
                egui::DragValue::new(&mut osc.sample_root).clamp_range(0..=119),
            );
            ui.label(
                crate::config::note_configs::NoteOct::from_pitch_index(osc.sample_root).to_string(),
            );
            checkbox(
                ui,
                &mut osc.sample_loop,
                lang.choose("Loop while held", "按住时循环"),
            );
            add(
                ui,
                egui::DragValue::new(&mut osc.sample_fine_cents)
                    .clamp_range(-100.0..=100.0)
                    .speed(0.1)
                    .suffix(lang.choose(" cents", " 音分")),
            );
        });
    }
    let Some(sample) = &osc.sample else {
        theme::caption(ui,lang.choose("No sample yet. Import a WAV, drop a WAV here, or capture live input. Imported/captured audio is embedded in the sound preset and project.","还没有采样。可导入或拖入 WAV，或捕获实时输入；素材随音色预设与工程保存。"));
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
                "No stable fundamental found; Waveform starts with a short region you can adjust.",
                "未检测到稳定基频；采样波形先选择一个可手动调整的短片段。",
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
    ui.horizontal(|ui| {
        if add(
            ui,
            egui::Slider::new(&mut start_ms, 0.0..=(end_ms - minimum_ms).max(0.0))
                .max_decimals(3)
                .text(lang.choose("Start (ms)", "起点（ms）")),
        )
        .changed()
            && start_ms.is_finite()
        {
            osc.sample_start = start_ms / duration_ms.max(0.001);
        }
        if add(
            ui,
            egui::Slider::new(
                &mut end_ms,
                (start_ms + minimum_ms).min(duration_ms)..=duration_ms,
            )
            .max_decimals(3)
            .text(lang.choose("End (ms)", "终点（ms）")),
        )
        .changed()
            && end_ms.is_finite()
        {
            osc.sample_end = end_ms / duration_ms.max(0.001);
        }
    });
    osc.sanitize_source();
    waveform(ui, osc);
    theme::caption(ui,lang.choose("Waveform maps the selected region to one periodic wave. Sampler preserves its temporal texture and uses Root note to transpose. Import uses at most the first 2 seconds; capture preserves up to 2 seconds at 8–192 kHz, then stores 48 kHz mono.","采样波形把所选区域映射为一个周期；采样音色保留素材时间纹理，按原音高移调。导入最多使用前 2 秒，实时捕获在 8–192 kHz 下最多保留 2 秒，随后保存为 48 kHz 单声道素材。"));
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
        let source_preview = if osc.waveform.value == Waveform::Vocal {
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
                    Waveform::Vocal => source_preview.as_ref().unwrap()[i],
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
        add(
            ui,
            egui::Slider::new(&mut lfo.beats, 0.0625..=32.0)
                .logarithmic(true)
                .text(lang.choose("Period (beats)", "周期（拍）")),
        );
    } else {
        add(
            ui,
            egui::Slider::new(&mut lfo.rate_hz, 0.01..=40.0)
                .logarithmic(true)
                .text(lang.choose("Rate (Hz)", "频率（Hz）")),
        );
    }
    add(
        ui,
        egui::Slider::new(&mut lfo.depth, 0.0..=1.0).text(lang.choose("Depth", "深度")),
    );
    lfo.sanitize();
    curve_editor(ui, lfo);
    theme::caption(ui,lang.choose("LFO runs independently from AHDSR. Volume modulation cannot reopen a released note. Cutoff moves in octaves; pitch depth spans up to ±12 semitones.","LFO 与 AHDSR 独立运行；音量调制不会重新打开已释放的音符。截止频率按八度调制，音高深度最大为 ±12 半音。"));
}

fn curve_editor(ui: &mut egui::Ui, lfo: &mut LfoConfig) {
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
            if lfo.points.len() < 32 {
                lfo.shape = LfoShape::Custom;
                lfo.points.push(CurvePoint {
                    x: ((p.x - plot.left()) / plot.width()).clamp(0.001, 0.999),
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
                        lfo.points[i].x = ((pos.x - plot.left()) / plot.width())
                            .clamp(lfo.points[i - 1].x + 0.001, lfo.points[i + 1].x - 0.001);
                    }
                    lfo.points[i].y = (1.0 - (pos.y - plot.top()) / plot.height()).clamp(0.0, 1.0);
                }
            }
            if handle.secondary_clicked() && i > 0 && i + 1 < lfo.points.len() {
                remove = Some(i);
            }
            ui.painter().circle_filled(pos, 5.0, theme::accent(ui));
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
                ui.painter().circle_filled(mid, 3.5, theme::secondary(ui));
            }
        }
        if let Some(i) = remove {
            lfo.points.remove(i);
        }
    }
    let points = (0..400)
        .map(|i| {
            let t = i as f32 / 399.0;
            pos2(
                plot.left() + plot.width() * t,
                plot.bottom() - plot.height() * crate::dsp::oscillator::lfo_value(lfo, t),
            )
        })
        .collect();
    ui.painter().add(egui::Shape::line(
        points,
        egui::Stroke::new(2.0, theme::accent(ui)),
    ));
    theme::caption(ui,lang.choose("Double-click: add point · Drag points: shape · Drag small midpoint: curvature · Right-click interior point: delete","双击加点 · 拖动节点塑形 · 拖动小中点调整曲率 · 右键删除内部节点"));
}

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

fn add(ui: &mut egui::Ui, widget: impl egui::Widget) -> egui::Response {
    crate::ui::navigation::register(ui.add(widget))
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
