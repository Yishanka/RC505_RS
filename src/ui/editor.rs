#[path = "synth_controls.rs"]
mod synth_controls;
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
    Modulation,
}

pub struct EditorState {
    pub target: Option<FxTarget>,
    pub expanded: bool,
    pub page: EditorPage,
    pub piano: PianoRollState,
    pub preset_name: String,
    pub library_open: bool,
    pub clip_name: String,
    pub clips: Vec<String>,
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
            library_open: false,
            clip_name: String::new(),
            clips: presets::list_clips(),
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
                EditorPage::Modulation,
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
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::new("audio-fx-sample-rate"), app.view.sample_rate));
    ui.ctx().data_mut(|d| {
        d.insert_temp(
            egui::Id::new("audio-fx-bpm"),
            app.config.beat_config.current_bpm(),
        )
    });
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
            ui.label(egui::RichText::new(target_text).color(theme::accent(ui)).strong());
            if full {header_kind_changed=kind_picker(ui,&mut app.config,target);}
            if full && super::navigation::register(ui.selectable_label(app.editor.library_open,lang.choose("Sounds / phrases", "音色 / 乐句库"))).clicked(){app.editor.library_open=!app.editor.library_open;}
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
            theme::caption(ui,lang.choose("Audition has its own clock and is monitor-only. OSC needs a phrase; Sample also needs captured/imported audio. Track Filter needs a recorded loop.","独立试听使用自己的时钟，只进入监听。OSC 需要乐句；采样模式还需要素材。轨道滤波需要已有循环音频。"));
        }
        let active = match target { FxTarget::Input{bank,..}=>bank==app.config.input_fx.sel_bank_idx, FxTarget::Track{bank,..}=>bank==app.config.track_fx.sel_bank_idx };
        if !active {
            ui.horizontal(|ui| {
                theme::caption(ui,lang.text("Editing an inactive bank."));
                if ui.button(lang.text("Activate this bank")).clicked() { match target {FxTarget::Input{bank,..}=>app.config.input_fx.select_bank(bank),FxTarget::Track{bank,..}=>app.config.track_fx.select_bank(bank)} }
            });
        }
        if full && app.editor.library_open {
            theme::control_row(ui, |ui| {
                ui.label(lang.choose("Sound preset", "音色预设"));
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
                                Ok(()) => { format!("Loaded {name}") }, Err(e) => e.to_string()
                            };
                        }
                    }
                });
            });
            if !app.editor.message.is_empty() { ui.label(&app.editor.message); }
            ui.separator();
        }
        if full && app.editor.library_open && presets::clip(&app.config,target).is_some() {
            theme::control_row(ui,|ui| {
                ui.label(lang.choose("Phrase", "乐句"));
                ui.add(egui::TextEdit::singleline(&mut app.editor.clip_name).hint_text(lang.choose("New phrase name", "新乐句名称")).desired_width(180.0));
                if ui.button(lang.choose("Save phrase", "保存乐句")).clicked() {
                    app.editor.message=match presets::save_clip(&app.config,target,&app.editor.clip_name) {
                        Ok(())=>{app.editor.clips=presets::list_clips();lang.choose("Phrase saved", "乐句已保存").into()},Err(e)=>e.to_string()
                    };
                }
                egui::ComboBox::from_id_source("load_clip").selected_text(lang.choose("Load phrase…", "载入乐句…")).show_ui(ui,|ui| {
                    for name in app.editor.clips.clone() {if ui.selectable_label(false,&name).clicked() {
                        let previous=presets::clip(&app.config,target);
                        app.editor.message=match presets::load_clip(&mut app.config,target,&name) {Ok(())=>{if let (Some(previous),Some(note))=(previous,presets::note_mut(&mut app.config,target)){app.editor.piano.remember_clip(previous,note);}format!("{} {name}",lang.choose("Loaded", "已载入"))},Err(e)=>e.to_string()};
                    }}
                });
                if let Some(clip)=presets::clip(&app.config,target) {if !clip.name.is_empty(){ui.label(format!("{} → {}",clip.name,target.label()));}}
            });
            theme::caption(ui,lang.choose("Phrase files contain notes only. Loading a phrase keeps the sound; loading a sound keeps this phrase.","乐句文件只保存音符。载入乐句保留音色；载入音色保留当前乐句。"));
        }
        ui.push_id(target.label(), |ui| {
            let changed = if full {header_kind_changed} else {kind_picker(ui, &mut app.config, target)};
            if changed { app.editor.piano.reset_history(); app.editor.page = EditorPage::Sound; }
            let synth = matches!(target, FxTarget::Input { bank, slot } if matches!(app.config.input_fx.banks[bank].slots[slot].fx, Some(InputFx::Oscillator(_)|InputFx::MyDelay(_))));
            if full && synth {
                theme::control_row(ui, |ui| {
                    for (page,label) in [(EditorPage::Sound,lang.text("Sound")),(EditorPage::Sequence,lang.text("Piano roll")),(EditorPage::Envelope,lang.text("Amp envelope")),(EditorPage::Filter,lang.text("Filter")),(EditorPage::FilterEnvelope,lang.text("Filter envelope")),(EditorPage::Modulation,lang.choose("LFO","LFO 调制"))] {
                        super::navigation::register(ui.selectable_value(&mut app.editor.page,page,label));
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
                &FxKind::available()
                    .into_iter()
                    .map(|kind| (kind, input_name(kind)))
                    .collect::<Vec<_>>(),
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
                &TrackFxKind::available()
                    .into_iter()
                    .map(|kind| (kind, track_name(kind)))
                    .collect::<Vec<_>>(),
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
        FxKind::Roll => "Roll",
        FxKind::Audio(kind) => kind.name(),
        FxKind::None => "Empty",
        FxKind::Oscillator => "OSC",
        FxKind::Filter => "Filter",
        FxKind::Reverb => "Reverb",
        FxKind::MyDelay => "MyDelay",
        FxKind::Vocoder => "Vocoder",
    }
}
pub fn track_name(kind: TrackFxKind) -> &'static str {
    match kind {
        TrackFxKind::Audio(kind) => kind.name(),
        TrackFxKind::None => "Empty",
        TrackFxKind::Delay => "Delay",
        TrackFxKind::Roll => "Roll",
        TrackFxKind::Filter => "Filter",
        TrackFxKind::Vocoder => "Vocoder",
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
        InputFx::Roll(roll) => roll_parameters(ui, roll, full),
        InputFx::Audio(audio) => super::audio_fx_panel::draw(ui, audio, full),
        InputFx::Oscillator(osc) => match page {
            EditorPage::Sequence => piano_roll::draw(ui, &mut osc.note, piano, beats),
            EditorPage::Envelope => parameters::envelope(ui, &mut osc.envelope),
            EditorPage::Filter => parameters::filter(ui, &mut osc.osc_filter, true),
            EditorPage::FilterEnvelope => parameters::envelope(ui, &mut osc.osc_filter_env),
            EditorPage::Modulation => synth_controls::lfo(ui, &mut osc.lfo),
            EditorPage::Sound => synth_controls::sound(ui, osc, full),
        },
        InputFx::MyDelay(delay) => match page {
            EditorPage::Sequence => piano_roll::draw(ui, &mut delay.note, piano, beats),
            EditorPage::Envelope => parameters::envelope(ui, &mut delay.audio_env),
            EditorPage::Filter => parameters::filter(ui, &mut delay.filter, true),
            EditorPage::FilterEnvelope => parameters::envelope(ui, &mut delay.filter_env),
            EditorPage::Modulation => {
                theme::caption(
                    ui,
                    lang.choose(
                        "Reload this project to migrate MyDelay into OSC.",
                        "重新载入工程即可把 MyDelay 迁移为 OSC。",
                    ),
                );
            }
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
                number(ui, &mut reverb.high_cut_hz, 0, 20_000, true);
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
        TrackFx::Audio(audio) => super::audio_fx_panel::draw(ui, audio, full),
        TrackFx::Vocoder(vocoder) => super::audio_fx_panel::track_vocoder(ui, vocoder, full),
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
            number(ui, &mut delay.feedback_repeats, 0, 16, false);
            if delay.feedback_repeats.value == 0 {
                number(
                    ui,
                    &mut delay.feedback_pct,
                    0,
                    TRACK_DELAY_FEEDBACK_MAX_PCT,
                    false,
                );
            }
            number(
                ui,
                &mut delay.high_damp_hz,
                TRACK_DELAY_DAMP_MIN_HZ,
                TRACK_DELAY_DAMP_MAX_HZ,
                true,
            );
            number(ui, &mut delay.direct_pct, 0, 100, false);
            number(ui, &mut delay.effect_pct, 0, 120, false);
            number(ui, &mut delay.low_cut_hz, 0, 12500, true);
            if full {
                theme::caption(ui,lang.choose("Repeat count maps feedback to -60 dB after 1–16 echoes; choose 0 for a manual coefficient. Cutoff 0 = FLAT. This mapping is documented, not a measured BOSS feedback law.","重复次数将反馈映射为第 1～16 次回声降至 -60 dB；选择 0 手动设置反馈。高低切设 0 表示直通；此映射不是实测的 BOSS 内部反馈曲线。"));
            }
        }
        TrackFx::Roll(roll) => roll_parameters(ui, roll, full),
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

fn roll_parameters(
    ui: &mut egui::Ui,
    roll: &mut crate::config::roll_configs::RollConfigs,
    full: bool,
) {
    use crate::config::{
        roll_configs::{RollMode, RollStep},
        time_mode::TimeMode,
    };
    let lang = crate::app_support::language::Language::current(ui.ctx());
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
        theme::caption(ui,lang.choose("Captures recent audio including preceding FX. Division shortens the frozen slice; Off repeats the full cycle using Feedback / Repeat. Repeat 0 = infinite.","捕获包含前级效果的最近声音。细分缩短冻结片段；Off 保留完整周期，并使用反馈／次数释放。重复 0 表示无限。"));
        theme::caption(ui,lang.text("With no recent history, capture waits for one slice. Toggle the slot off/on to capture again. Sync time is limited to the 2 s capture buffer."));
    }
}
