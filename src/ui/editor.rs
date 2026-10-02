use super::{
    parameters::{self, choice, number},
    piano_roll::{self, PianoRollState},
    theme,
};
use crate::{
    app::MyApp,
    config::{AppConfig, FxKind, InputFx, TrackFx, TrackFxKind},
    presets::{self, FxTarget},
};
use eframe::egui;

#[derive(Clone, Copy, PartialEq)]
pub enum EditorPage {
    Sound,
    Sequence,
    Envelope,
    Filter,
    FilterEnvelope,
}

pub struct EditorState {
    pub target: Option<FxTarget>,
    pub expanded: bool,
    pub page: EditorPage,
    pub piano: PianoRollState,
    pub preset_name: String,
    pub presets: Vec<String>,
    pub message: String,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            target: None,
            expanded: false,
            page: EditorPage::Sound,
            piano: PianoRollState::default(),
            preset_name: String::new(),
            presets: presets::list(),
            message: String::new(),
        }
    }
}

impl EditorState {
    pub fn cycle_page(&mut self, config: &AppConfig, backward: bool) {
        let synth = matches!(self.target,Some(FxTarget::Input{bank,slot}) if matches!(config.input_fx.banks[bank].slots[slot].fx,Some(InputFx::Oscillator(_)|InputFx::MyDelay(_))));
        if synth {
            let pages = [
                EditorPage::Sound,
                EditorPage::Sequence,
                EditorPage::Envelope,
                EditorPage::Filter,
                EditorPage::FilterEnvelope,
            ];
            let index = pages
                .iter()
                .position(|page| *page == self.page)
                .unwrap_or(0);
            self.page = pages[(index + if backward { pages.len() - 1 } else { 1 }) % pages.len()];
        }
    }
    pub fn select(&mut self, target: FxTarget) {
        if self.target != Some(target) {
            self.piano.reset_history();
            self.page = EditorPage::Sound;
            self.message.clear();
        }
        self.target = Some(target);
    }
}

pub fn draw(ui: &mut egui::Ui, app: &mut MyApp, full: bool) {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    let Some(target) = app.editor.target else {
        theme::card().show(ui, |ui| {
            ui.heading(lang.text("Shape your next loop"));
            theme::caption(
                ui,
                lang.text(
                    "Select an FX slot below to edit. Expand opens a full sound-design workspace.",
                ),
            );
            if full
                && super::navigation::register(theme::action(
                    ui,
                    theme::Icon::Back,
                    lang.text("Back to performance"),
                    "Esc",
                ))
                .clicked()
            {
                app.close_editor(ui.ctx());
            }
        });
        return;
    };
    let beats = app.editor_beats();
    let mut header_kind_changed = false;
    theme::card().show(ui, |ui| {
        theme::control_row(ui, |ui| {
            let (bank,slot) = match target { FxTarget::Input{bank,slot}|FxTarget::Track{bank,slot}=>(bank,slot) };
            let target_text=format!("{} / {} / {}",lang.text(if matches!(target,FxTarget::Input{..}) {"INPUT FX"} else {"TRACK FX"}),bank+1,['A','B','C','D'][slot]);
            ui.label(egui::RichText::new(target_text).color(theme::ACCENT).strong());
            if full {header_kind_changed=kind_picker(ui,&mut app.config,target);}
            match target {
                FxTarget::Input {bank,slot} => { super::navigation::register(ui.checkbox(&mut app.config.input_fx.banks[bank].slots[slot].is_enabled,lang.text("Enabled"))); }
                FxTarget::Track {bank,slot} => {
                    let index = app.track_sel.unwrap_or(0);
                    super::navigation::register(ui.checkbox(&mut app.config.track_fx.tracks[index].enabled[bank][slot],format!("{} {} · {}",lang.text("Track"),index+1,lang.text("Enabled"))));
                }
            }
            if full {
                if super::navigation::register(theme::action(ui, theme::Icon::Back, lang.text("Back to performance"), "Esc")).clicked() { app.close_editor(ui.ctx()); }
            } else if super::navigation::register(theme::action(ui, theme::Icon::Expand, lang.text("Expand"), "")).clicked() { app.open_editor(ui.ctx()); }
            if crate::engine::audition::supports(&app.config,target) {
                let reason=app.audition_reason();
                let response=super::navigation::register(ui.add_enabled(app.previewing || reason.is_none(),egui::Button::new(lang.choose(if app.previewing {"Stop audition"} else {"Audition"},if app.previewing {"停止试听"} else {"独立试听"}))));
                let response=response.on_hover_text(lang.choose("Private preview clock; bypasses the slot enable switch and oscillator threshold. Not recorded in tracks or replay audio.","使用独立时钟，不受槽位开关和振荡器阈值限制；不录入轨道或回放音频。"));
                if response.clicked() {app.toggle_audition();}
                if let Some(reason)=reason {response.on_disabled_hover_text(lang.text(reason));}
            }

        });
        if full && crate::engine::audition::supports(&app.config,target) {
            theme::caption(ui,lang.choose("Audition has its own clock and is monitor-only. MyDelay needs live input; Track Filter needs a recorded loop.","独立试听使用自己的时钟，只进入监听。MyDelay 需要实时输入；轨道滤波需要已有循环音频。"));
        }
        let active = match target { FxTarget::Input{bank,..}=>bank==app.config.input_fx.sel_bank_idx, FxTarget::Track{bank,..}=>bank==app.config.track_fx.sel_bank_idx };
        if !active {
            ui.horizontal(|ui| {
                theme::caption(ui,lang.text("Editing an inactive bank."));
                if ui.button(lang.text("Activate this bank")).clicked() { match target {FxTarget::Input{bank,..}=>app.config.input_fx.select_bank(bank),FxTarget::Track{bank,..}=>app.config.track_fx.select_bank(bank)} }
            });
        }
        if full {
            theme::control_row(ui, |ui| {
                ui.label(lang.text("Preset"));
                ui.add(egui::TextEdit::singleline(&mut app.editor.preset_name).hint_text(lang.text("Name for a new preset")).desired_width(180.0));
                if ui.button(lang.text("Save as new")).clicked() {
                    app.editor.message = match presets::save(&app.config, target, &app.editor.preset_name) {
                        Ok(()) => { app.editor.presets = presets::list(); lang.text("Preset saved").into() }, Err(e) => e.to_string()
                    };
                }
                egui::ComboBox::from_id_source("load_preset").selected_text(lang.text("Load preset…")).show_ui(ui, |ui| {
                    for name in app.editor.presets.clone() {
                        if ui.selectable_label(false, &name).clicked() {
                            app.editor.message = match presets::load(&mut app.config, target, &name) {
                                Ok(()) => { app.editor.piano.reset_history(); format!("Loaded {name}") }, Err(e) => e.to_string()
                            };
                        }
                    }
                });
            });
            if !app.editor.message.is_empty() { ui.label(&app.editor.message); }
            ui.separator();
        }
        ui.push_id(target.label(), |ui| {
            let changed = if full {header_kind_changed} else {kind_picker(ui, &mut app.config, target)};
            if changed { app.editor.piano.reset_history(); app.editor.page = EditorPage::Sound; }
            let synth = matches!(target, FxTarget::Input { bank, slot } if matches!(app.config.input_fx.banks[bank].slots[slot].fx, Some(InputFx::Oscillator(_)|InputFx::MyDelay(_))));
            if full && synth {
                theme::control_row(ui, |ui| {
                    for (page,label) in [(EditorPage::Sound,lang.text("Sound")),(EditorPage::Sequence,lang.text("Piano roll")),(EditorPage::Envelope,lang.text("Amp envelope")),(EditorPage::Filter,lang.text("Filter")),(EditorPage::FilterEnvelope,lang.text("Filter envelope"))] {
                        ui.selectable_value(&mut app.editor.page,page,label);
                    }
                    theme::keycap(ui,"Ctrl+Tab");
                });
                ui.separator();
            }
            if !full && synth && super::navigation::button(ui,lang.text("Open piano roll")).clicked() {
                app.editor.page = EditorPage::Sequence; app.open_editor(ui.ctx());
            }
            let page = if full { app.editor.page } else { EditorPage::Sound };
            match target {
                FxTarget::Input { bank, slot } => {
                    if let Some(fx) = app.config.input_fx.banks[bank].slots[slot].fx.as_mut() {
                        input_parameters(ui, fx, page, &mut app.editor.piano, beats, full);
                    } else { theme::caption(ui, lang.text("Choose an effect type, then enable the slot in the rack.")); }
                }
                FxTarget::Track { bank, slot } => {
                    if let Some(fx) = app.config.track_fx.banks[bank].slots[slot].fx.as_mut() { track_parameters(ui,fx,full); }
                    else { theme::caption(ui, lang.text("Choose a playback effect. Enable it independently for each track.")); }
                }
            }
        });
    });
}

fn kind_picker(ui: &mut egui::Ui, config: &mut AppConfig, target: FxTarget) -> bool {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    match target {
        FxTarget::Input { bank, slot } => {
            let previous = config.input_fx.slot_kind(bank, slot);
            let mut kind = previous;
            parameters::selector(
                ui,
                "kind",
                &mut kind,
                &[
                    (FxKind::None, lang.text("Empty")),
                    (FxKind::Oscillator, lang.text("Oscillator")),
                    (FxKind::Filter, lang.text("Filter")),
                    (FxKind::Reverb, lang.text("Reverb")),
                    (FxKind::MyDelay, lang.text("MyDelay")),
                    (FxKind::Vocoder, lang.text("Vocoder")),
                ],
            );
            if kind != previous {
                config.input_fx.set_slot_kind(bank, slot, kind);
                return true;
            }
        }
        FxTarget::Track { bank, slot } => {
            let previous = config.track_fx.slot_kind(bank, slot);
            let mut kind = previous;
            parameters::selector(
                ui,
                "kind",
                &mut kind,
                &[
                    (TrackFxKind::None, lang.text("Empty")),
                    (TrackFxKind::Delay, lang.text("Delay")),
                    (TrackFxKind::Roll, lang.text("Roll")),
                    (TrackFxKind::Filter, lang.text("Filter")),
                ],
            );
            if kind != previous {
                config.track_fx.set_slot_kind(bank, slot, kind);
                return true;
            }
        }
    }
    false
}

pub fn input_name(kind: FxKind) -> &'static str {
    match kind {
        FxKind::None => "Empty",
        FxKind::Oscillator => "Oscillator",
        FxKind::Filter => "Filter",
        FxKind::Reverb => "Reverb",
        FxKind::MyDelay => "MyDelay",
        FxKind::Vocoder => "Vocoder",
    }
}
pub fn track_name(kind: TrackFxKind) -> &'static str {
    match kind {
        TrackFxKind::None => "Empty",
        TrackFxKind::Delay => "Delay",
        TrackFxKind::Roll => "Roll",
        TrackFxKind::Filter => "Filter",
    }
}

fn input_parameters(
    ui: &mut egui::Ui,
    fx: &mut InputFx,
    page: EditorPage,
    piano: &mut PianoRollState,
    beats: Option<f64>,
    full: bool,
) {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    match fx {
        InputFx::Oscillator(osc) => match page {
            EditorPage::Sequence => piano_roll::draw(ui, &mut osc.note, piano, beats),
            EditorPage::Envelope => parameters::envelope(ui, &mut osc.envelope),
            EditorPage::Filter => parameters::filter(ui, &mut osc.osc_filter, true),
            EditorPage::FilterEnvelope => parameters::envelope(ui, &mut osc.osc_filter_env),
            EditorPage::Sound => {
                choice(ui, &mut osc.waveform);
                number(ui, &mut osc.level, 0, 100, false);
                number(ui, &mut osc.threshold, 0, 100, false);
                if full {
                    theme::caption(
                        ui,
                        lang.choose("Write notes in the piano roll. The sequencer runs during performance; audition has its own clock.","在钢琴卷帘中编写音符。序列随演出运行，独立试听使用自己的时钟。"),
                    );
                    waveform(ui, osc.waveform.value);
                }
            }
        },
        InputFx::MyDelay(delay) => match page {
            EditorPage::Sequence => piano_roll::draw(ui, &mut delay.note, piano, beats),
            EditorPage::Envelope => parameters::envelope(ui, &mut delay.audio_env),
            EditorPage::Filter => parameters::filter(ui, &mut delay.filter, true),
            EditorPage::FilterEnvelope => parameters::envelope(ui, &mut delay.filter_env),
            EditorPage::Sound => {
                number(ui, &mut delay.level, 0, 100, false);
                number(ui, &mut delay.threshold, 0, 100, false);
                if full {
                    theme::caption(
                        ui,
                        lang.text("Captures a short input fragment and repeats it at the selected pitch. This custom effect needs incoming audio."),
                    );
                }
            }
        },
        InputFx::Filter(filter) => parameters::filter(ui, filter, full),
        InputFx::Reverb(reverb) => {
            use crate::config::reverb_configs::*;
            number(ui, &mut reverb.size, 0, REVERB_SIZE_MAX, false);
            number(
                ui,
                &mut reverb.decay_ms,
                REVERB_RT60_MIN_MS,
                REVERB_RT60_MAX_MS,
                true,
            );
            number(
                ui,
                &mut reverb.predelay_ms,
                0,
                REVERB_PREDELAY_MAX_MS,
                false,
            );
            number(ui, &mut reverb.width, 0, REVERB_WIDTH_MAX, false);
            if full {
                number(ui, &mut reverb.dry_level, 0, 100, false);
                number(ui, &mut reverb.wet_level, 0, 100, false);
                number(ui, &mut reverb.density, 1, 10, false);
                number(ui, &mut reverb.high_cut_hz, 200, 20_000, true);
                number(
                    ui,
                    &mut reverb.low_cut,
                    REVERB_LOWCUT_MIN_HZ,
                    REVERB_LOWCUT_MAX_HZ,
                    true,
                );
                theme::caption(
                    ui,
                    lang.text("Diffusion + FDN reverb / High cut in Hz / Decay is RT60."),
                );
            }
        }
        InputFx::Vocoder(vocoder) => {
            use crate::config::vocoder_configs::*;
            choice(ui, &mut vocoder.carrier);
            number(
                ui,
                &mut vocoder.bands,
                VOCODER_BANDS_MIN,
                VOCODER_BANDS_MAX,
                false,
            );
            number(ui, &mut vocoder.level, 0, VOCODER_LEVEL_MAX, false);
            number(ui, &mut vocoder.mix, 0, VOCODER_MIX_MAX, false);
            if full {
                number(ui, &mut vocoder.attack_ms, 0, VOCODER_ATTACK_MAX_MS, false);
                number(
                    ui,
                    &mut vocoder.release_ms,
                    0,
                    VOCODER_RELEASE_MAX_MS,
                    false,
                );
                ui.add(egui::Slider::new(&mut vocoder.tone, -50..=50).text(lang.text("Tone")));
                ui.add(
                    egui::Slider::new(&mut vocoder.mod_sens, -50..=50)
                        .text(lang.text("Mod sensitivity")),
                );
                ui.add(
                    egui::Slider::new(&mut vocoder.formant_semitones, -12..=12)
                        .text(lang.text("Formant (semitones)")),
                );
                number(ui, &mut vocoder.sibilance, 0, 100, false);
                if vocoder.carrier.value.track_idx().is_none() {
                    ui.checkbox(
                        &mut vocoder.carrier_thru,
                        lang.text("Carrier thru (dry carrier channel)"),
                    );
                    theme::caption(
                        ui,
                        lang.text("Stereo input: carrier on selected channel, voice on the other. One stereo device, not two independent devices."),
                    );
                } else {
                    theme::caption(
                        ui,
                        lang.text("Record a harmonically rich carrier to the selected track. A paused carrier follows the running timeline without playing dry."),
                    );
                }
                theme::caption(
                    ui,
                    lang.text("Formant moves the spectral envelope; carrier pitch is preserved. Sibilance emphasizes upper analysis bands."),
                );
            }
        }
    }
}

fn track_parameters(ui: &mut egui::Ui, fx: &mut TrackFx, full: bool) {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    match fx {
        TrackFx::Delay(delay) => {
            use crate::config::delay_configs::*;
            choice(ui, &mut delay.time_mode);
            if delay.time_mode.value == crate::config::time_mode::TimeMode::Milliseconds {
                number(
                    ui,
                    &mut delay.time_ms,
                    TRACK_DELAY_TIME_MIN_MS,
                    TRACK_DELAY_TIME_MAX_MS,
                    true,
                );
            }
            number(
                ui,
                &mut delay.feedback_pct,
                0,
                TRACK_DELAY_FEEDBACK_MAX_PCT,
                false,
            );
            number(
                ui,
                &mut delay.high_damp_hz,
                TRACK_DELAY_DAMP_MIN_HZ,
                TRACK_DELAY_DAMP_MAX_HZ,
                true,
            );
            number(ui, &mut delay.direct_pct, 0, 100, false);
            number(ui, &mut delay.effect_pct, 0, 100, false);
            number(ui, &mut delay.low_cut_hz, 0, 1000, false);
        }
        TrackFx::Roll(roll) => {
            use crate::config::{
                roll_configs::{RollMode, RollStep},
                time_mode::TimeMode,
            };
            choice(ui, &mut roll.mode);
            choice(ui, &mut roll.time_mode);
            if roll.time_mode.value == TimeMode::Milliseconds {
                number(ui, &mut roll.time_ms, 1, 1000, true);
            }
            choice(ui, &mut roll.step);
            number(ui, &mut roll.mix, 0, 100, false);
            if full {
                ui.add_enabled_ui(roll.step.value == RollStep::Off, |ui| {
                    if roll.mode.value == RollMode::Roll1 {
                        number(ui, &mut roll.feedback, 1, 100, false);
                    } else {
                        number(ui, &mut roll.repeat, 0, 100, false);
                    }
                });
                theme::caption(
                    ui,
                    lang.text("Captures recent audio including preceding Track FX. Division shortens the frozen slice; Off repeats the full cycle using Feedback / Repeat. Repeat 0 = infinite."),
                );
                theme::caption(
                    ui,
                    lang.text("With no recent history, capture waits for one slice. Toggle the slot off/on to capture again. Sync time is limited to the 2 s capture buffer."),
                );
            }
        }
        TrackFx::Filter(filter) => {
            parameters::filter(ui, &mut filter.filter, full);
            if full {
                egui::CollapsingHeader::new(lang.text("Step gate sequencer"))
                    .default_open(true)
                    .show(ui, |ui| {
                        choice(ui, &mut filter.seq.step);
                        ui.horizontal(|ui| {
                            if ui.button(lang.text("Append step")).clicked() {
                                filter.seq.edit.value =
                                    crate::config::seq_configs::TrackSeqEdit::Push;
                                filter.seq.apply_edit();
                            }
                            if ui.button(lang.text("Remove last")).clicked() {
                                filter.seq.edit.value =
                                    crate::config::seq_configs::TrackSeqEdit::Pop;
                                filter.seq.apply_edit();
                            }
                        });
                        let mut seq = filter.seq.seq().to_vec();
                        let steps = filter.seq.step_len_seq().to_vec();
                        let mut changed = false;
                        theme::control_row(ui, |ui| {
                            for (start, len) in
                                steps.iter().enumerate().filter(|(_, len)| **len > 0)
                            {
                                if ui
                                    .selectable_label(seq[start], format!("{}", start + 1))
                                    .on_hover_text("Toggle the gate for this step")
                                    .clicked()
                                {
                                    let value = !seq[start];
                                    let end = (start + len).min(seq.len());
                                    seq[start..end].fill(value);
                                    changed = true;
                                }
                            }
                        });
                        if changed {
                            filter.seq.set_seq_with_steps(seq, steps);
                        }
                    });
                egui::CollapsingHeader::new("Cutoff envelope")
                    .show(ui, |ui| parameters::envelope(ui, &mut filter.env));
            }
        }
    }
}

fn waveform(ui: &mut egui::Ui, waveform: crate::config::osc_configs::Waveform) {
    use crate::config::osc_configs::Waveform;
    theme::caption(
        ui,
        crate::app_support::language::Language::current(ui.ctx()).choose(
            "WAVEFORM / ideal shape, two cycles",
            "波形 / 理想形状，两个周期",
        ),
    );
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 170.0),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(rect, 6.0, theme::BACKGROUND);
    let points = (0..400)
        .map(|i| {
            let phase = (i as f32 / 200.0).fract();
            let value = match waveform {
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
            };
            egui::pos2(
                rect.left() + rect.width() * i as f32 / 399.0,
                rect.center().y - value * rect.height() * 0.38,
            )
        })
        .collect();
    ui.painter().add(egui::Shape::line(
        points,
        egui::Stroke::new(2.0, theme::ACCENT),
    ));
}
