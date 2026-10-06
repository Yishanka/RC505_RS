#[path = "library_delete.rs"]
mod library_delete;
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
pub use library_delete::{LibraryDelete, draw as draw_library_delete};

#[derive(Clone, Copy, PartialEq)]
pub enum EditorPage {
    Sound,
    Sequence,
    Envelope,
    Filter,
    FilterEnvelope,
    Modulation,
    Automation,
}

pub struct EditorState {
    pub target: Option<FxTarget>,
    pub expanded: bool,
    pub page: EditorPage,
    pub piano: PianoRollState,
    pub automation: super::automation::EditorState,
    pub preset_name: String,
    pub library_open: bool,
    pub clip_name: String,
    pub clip_next_loop: bool,
    pub phrase_source: String,
    pub phrase_manager_open: bool,
    pub clips: Vec<String>,
    pub presets: Vec<String>,
    pub candidate: Option<presets::SoundCandidate>,
    pub library_delete: Option<LibraryDelete>,
    pub library_delete_job: Option<presets::DeleteJob>,
    pub message: String,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            target: None,
            expanded: false,
            page: EditorPage::Sound,
            piano: PianoRollState::default(),
            automation: Default::default(),
            preset_name: String::new(),
            library_open: false,
            clip_name: String::new(),
            clip_next_loop: true,
            phrase_source: String::new(),
            phrase_manager_open: false,
            clips: presets::list_clips(),
            presets: presets::list(),
            candidate: None,
            library_delete: None,
            library_delete_job: None,
            message: String::new(),
        }
    }
}

impl EditorState {
    pub fn cycle_page(&mut self, config: &AppConfig, backward: bool) {
        if self
            .target
            .is_some_and(|target| super::automation::family(config, target).is_some())
        {
            self.page = if self.page == EditorPage::Automation {
                EditorPage::Sound
            } else {
                EditorPage::Automation
            };
            return;
        }
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
            self.automation.reset();
            self.page = EditorPage::Sound;
            self.message.clear();
            self.candidate = None;
            self.library_delete = None;
        }
        self.target = Some(target);
    }
}

pub fn draw(ui: &mut egui::Ui, app: &mut MyApp, full: bool) {
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::new("audio-fx-sample-rate"), app.view.sample_rate));
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::new("audio-fx-pdc"), app.config.pdc_enabled));
    ui.ctx().data_mut(|d| {
        d.insert_temp(
            egui::Id::new("audio-fx-bpm"),
            app.config.beat_config.current_bpm(),
        )
    });
    let lang = crate::app_support::language::Language::current(ui.ctx());
    let Some(target) = app.editor.target else {
        theme::card().show(ui, |ui| {
            ui.heading(lang.choose("Select an FX slot", "选择效果槽"));
            theme::caption(
                ui,
                lang.choose(
                    "Choose a slot on the performance screen.",
                    "在演奏台选择要调整的效果槽。",
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
    let phrase_before =
        presets::note_mut(&mut app.config, target).map(|n| piano_roll::Snapshot::capture(n));
    let pending_before = presets::note_mut(&mut app.config, target)
        .and_then(|n| n.pending.as_ref().map(|p| p.serial));
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
                let response=response.on_hover_text(lang.choose("Monitor only; not recorded.","仅试听，不录入轨道。"));
                if response.clicked() {app.toggle_audition();}
                if let Some(reason)=reason {response.on_disabled_hover_text(lang.text(reason));}
            }

        });
        if full {
            theme::caption(ui,lang.choose("↑↓ / Tab: select · ←→: adjust · Enter: type", "↑↓ / Tab 选参数 · ←→ 调值 · Enter 输入"));
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
                let name_field=super::navigation::text(ui.add(egui::TextEdit::singleline(&mut app.editor.preset_name).hint_text(lang.text("Name for a new preset")).desired_width(180.0)));
                #[cfg(debug_assertions)]
                ui.ctx().data_mut(|d|d.insert_temp(egui::Id::new("preset-name-field"),name_field.id));
                #[cfg(not(debug_assertions))]
                let _ = name_field;
                if super::navigation::button(ui,lang.text("Save as new")).clicked() {
                    app.editor.message = match presets::save(&mut app.config, target, &app.editor.preset_name) {
                        Ok(()) => { app.editor.presets = presets::list(); lang.text("Preset saved").into() }, Err(e) => e.to_string()
                    };
                }
                let response=egui::ComboBox::from_id_source("load_preset").selected_text(lang.choose("Browse sounds…","浏览候选音色…")).show_ui(ui, |ui| {
                    for name in app.editor.presets.clone() {
                        ui.horizontal(|ui| {
                            if ui.selectable_label(false, &name).clicked() {
                                app.choose_preset_candidate(target,&name);
                                ui.close_menu();
                            }
                            if ui.add_enabled(!app.read_only, egui::Button::new(lang.choose("Delete…", "删除…"))).clicked() {
                                app.editor.library_delete = Some(LibraryDelete::Sound(name.clone()));
                                app.editor.message.clear();
                                ui.close_menu();
                            }
                        });
                    }
                });super::navigation::register(response.response);
            });
            if let Some(candidate)=&app.editor.candidate {
                let name=candidate.name.clone();let source=candidate.source;let kind=candidate.kind;
                theme::card().show(ui,|ui| {
                    ui.add(egui::Label::new(egui::RichText::new(format!("{} · {} · {name}",lang.choose("Candidate sound","候选音色"),lang.text(kind))).strong()).truncate(true));
                    theme::control_row(ui,|ui| {
                        let reason=app.candidate_audition_reason();let playing=app.previewing&&app.candidate_audition;
                        let response=super::navigation::register(ui.add_enabled(playing||reason.is_none(),egui::Button::new(lang.choose(if playing{"Stop candidate preview"}else{"Preview candidate"},if playing{"停止候选试听"}else{"试听候选"}))));
                        if response.clicked(){app.toggle_candidate_audition();}
                        if let Some(reason)=reason {response.on_disabled_hover_text(lang.text(reason));}
                        if super::navigation::button(ui,lang.choose("Apply this sound","应用此音色")).clicked(){app.apply_preset_candidate();}
                        if super::navigation::button(ui,lang.choose("Cancel candidate","取消候选")).clicked(){app.cancel_preset_candidate();}
                    });
                    theme::caption(ui,match source {
                        presets::CandidateSource::Phrase=>lang.choose("Uses the current phrase; the live patch stays unchanged until Apply.","使用当前乐句试听；点击应用前，正式音色保持不变。"),
                        presets::CandidateSource::SingleNote=>lang.choose("No phrase: preview a short C4 note only.","没有乐句：仅试听一个 C4 短音，不写入工程。"),
                        presets::CandidateSource::LiveInput=>lang.choose("Live input → candidate FX. Audio is required; this is not a sequence preview.","实时输入 → 候选效果；需要输入声音，不是序列试听。"),
                        presets::CandidateSource::TrackLoop=>lang.choose("Selected track audio → candidate FX; the recorded loop is unchanged.","所选轨道音频 → 候选效果；已录音频保持不变。"),
                        presets::CandidateSource::Empty=>lang.choose("This preset clears the effect slot; there is no processor to preview.","此预设会清除效果类型，没有可试听的处理器。"),
                    });
                    theme::caption(ui,lang.choose("Candidate preview temporarily replaces monitoring. Stop or Cancel restores performance monitoring; preview audio is never recorded.","候选试听临时独立监听；停止或取消恢复演奏监听，试听声不会录入轨道或回放。"));
                });
            }
            if !app.editor.message.is_empty() { ui.label(&app.editor.message); }
            ui.separator();
        }
        if full && app.editor.library_open && presets::clip(&app.config,target).is_some() {
            theme::control_row(ui,|ui| {
                ui.label(lang.choose("Phrase", "乐句"));
                super::navigation::text(ui.add(egui::TextEdit::singleline(&mut app.editor.clip_name).hint_text(lang.choose("New phrase name", "新乐句名称")).desired_width(180.0)));
                if super::navigation::button(ui,lang.choose("Save phrase", "保存乐句")).clicked() {
                    app.editor.message=match presets::save_clip(&app.config,target,&app.editor.clip_name) {
                        Ok(())=>{app.editor.clips=presets::list_clips();lang.choose("Phrase saved", "乐句已保存").into()},Err(e)=>e.to_string()
                    };
                }
                let response=egui::ComboBox::from_id_source("load_clip").selected_text(lang.choose("Load phrase…", "载入乐句…")).show_ui(ui,|ui| {
                    for name in app.editor.clips.clone() {ui.horizontal(|ui| {
                        if ui.selectable_label(false,&name).clicked() {
                            let previous=presets::note_mut(&mut app.config,target).map(|n|piano_roll::Snapshot::capture(n));
                            let next=super::phrases::next_loop(app,target);
                            app.editor.message=match presets::load_clip_timed(&mut app.config,target,&name,next) {Ok(())=>{if let (Some(previous),Some(note))=(previous,presets::note_mut(&mut app.config,target)){app.editor.piano.remember_state(previous,note);}format!("{} {name}",lang.choose("Loaded", "已载入"))},Err(e)=>e.to_string()};
                            ui.close_menu();
                        }
                        if ui.add_enabled(!app.read_only, egui::Button::new(lang.choose("Delete…", "删除…"))).clicked() {
                            app.editor.library_delete = Some(LibraryDelete::Phrase(name.clone()));
                            app.editor.message.clear();
                            ui.close_menu();
                        }
                    });}
                });super::navigation::register(response.response);
                if let Some(clip)=presets::clip(&app.config,target) {if !clip.name.is_empty(){ui.label(format!("{} → {}",clip.name,target.label()));}}
            });
            theme::caption(ui,lang.choose("Phrase files contain notes only. Loading a phrase keeps the sound; loading a sound keeps this phrase.","乐句文件只保存音符。载入乐句保留音色；载入音色保留当前乐句。"));
        }
        ui.push_id(target.label(), |ui| {
            let changed = if full {header_kind_changed} else {
                theme::control_row(ui, |ui| {
                    let changed = kind_picker(ui, &mut app.config, target);
                    if let FxTarget::Input { bank, slot } = target {
                        if let Some(InputFx::Oscillator(osc)) = &mut app.config.input_fx.banks[bank].slots[slot].fx {
                            synth_controls::capture_button(ui, osc);
                        }
                    }
                    changed
                }).inner
            };
            if changed { app.editor.piano.reset_history();app.editor.automation.reset(); app.editor.page = EditorPage::Sound; }
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
            if super::automation::family(&app.config,target).is_some(){
                if full {theme::control_row(ui,|ui|{super::navigation::register(ui.selectable_value(&mut app.editor.page,EditorPage::Sound,lang.choose("Effect controls","效果参数")));super::navigation::register(ui.selectable_value(&mut app.editor.page,EditorPage::Automation,lang.choose("Parameter lane","参数自动化")));theme::keycap(ui,"Ctrl+Tab");});}
                else if super::navigation::button(ui,lang.choose("Open parameter lane","打开参数自动化")).clicked(){app.editor.page=EditorPage::Automation;app.open_editor(ui.ctx());}
                if full && app.editor.page==EditorPage::Automation {super::automation::draw(ui,&mut app.config,target,&mut app.editor.automation);return;}
            }
            if full && synth && page==EditorPage::Sequence {super::phrases::draw(ui,app,target);}
            if full && page==EditorPage::Sound {sample_save_controls(ui,app,target);}
            match target {
                FxTarget::Input { bank, slot } => {
                    if let Some(fx) = app.config.input_fx.banks[bank].slots[slot].fx.as_mut() {
                        input_parameters(ui, fx, page, &mut app.editor.piano, beats, full);
                    } else { theme::caption(ui, lang.choose("Choose an effect.","选择效果器。")); }
                }
                FxTarget::Track { bank, slot } => {
                    if let Some(fx) = app.config.track_fx.banks[bank].slots[slot].fx.as_mut() { track_parameters(ui,fx,full); }
                    else { theme::caption(ui, lang.choose("Choose an effect.","选择效果器。")); }
                }
            }
        });
    });
    let phrase_after =
        presets::note_mut(&mut app.config, target).map(|n| piano_roll::Snapshot::capture(n));
    if phrase_before != phrase_after {
        if pending_before.is_some()
            && presets::note_mut(&mut app.config, target)
                .is_some_and(|n| n.pending.is_none() && n.launch_serial != pending_before.unwrap())
        {
            crate::phrases::cancel_pending(&mut app.config, target);
        }
        crate::phrases::propagate(&mut app.config, target);
    }
    if let Some(note) = app.editor.piano.audition_note.take() {
        app.audition_note(target, note);
    }
}

fn sample_save_controls(ui: &mut egui::Ui, app: &mut MyApp, target: FxTarget) {
    let FxTarget::Input { bank, slot } = target else {
        return;
    };
    let Some(InputFx::Oscillator(osc)) = &app.config.input_fx.banks[bank].slots[slot].fx else {
        return;
    };
    if osc.waveform.value != crate::config::osc_configs::Waveform::Sample
        && osc.sample.is_none()
        && osc.sample_ref.is_none()
    {
        return;
    }
    let lang = app.language;
    let available = osc.sample.is_some() && osc.sample_job.is_none() && osc.capture.is_none();
    let status = if osc.sample.is_none() {
        lang.choose("No sample loaded", "尚未载入采样").to_owned()
    } else if osc.sample_temporary {
        lang.choose(
            "Temporary sample · discarded on exit unless saved as a sound",
            "临时采样 · 保存为音色后才能在下次载入",
        )
        .to_owned()
    } else if let Some(saved) = &osc.sample_ref {
        format!(
            "{} · {}",
            lang.choose("Saved sound", "已保存音色"),
            saved.preset
        )
    } else {
        lang.choose("Saved sample", "已保存采样").to_owned()
    };
    ui.add(egui::Label::new(status).truncate(true));
    if !app.editor.library_open {
        theme::control_row(ui, |ui| {
            super::navigation::text(
                ui.add(
                    egui::TextEdit::singleline(&mut app.editor.preset_name)
                        .hint_text(lang.choose("Sound name", "音色名称"))
                        .desired_width(180.0),
                ),
            );
            if super::navigation::register(ui.add_enabled(
                available,
                egui::Button::new(lang.choose("Save as sound", "保存为音色")),
            ))
            .clicked()
            {
                app.editor.message =
                    match presets::save(&mut app.config, target, &app.editor.preset_name) {
                        Ok(()) => {
                            app.editor.presets = presets::list();
                            lang.text("Preset saved").into()
                        }
                        Err(e) => e.to_string(),
                    };
            }
        });
        if !app.editor.message.is_empty() {
            ui.label(&app.editor.message);
        }
    }
}

fn kind_picker(ui: &mut egui::Ui, config: &mut AppConfig, target: FxTarget) -> bool {
    let lang = crate::app_support::language::Language::current(ui.ctx());
    let name = |english: &str| {
        let translated = lang.text(english);
        if translated == english || english == "Empty" {
            translated.to_owned()
        } else {
            format!("{english} · {translated}")
        }
    };
    match target {
        FxTarget::Input { bank, slot } => {
            let previous = config.input_fx.slot_kind(bank, slot);
            let mut kind = previous;
            let options = FxKind::available()
                .into_iter()
                .map(|kind| (kind, name(kind.name())))
                .collect::<Vec<_>>();
            parameters::selector(
                ui,
                "kind",
                &mut kind,
                &options
                    .iter()
                    .map(|(kind, label)| (*kind, label.as_str()))
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
            let options = TrackFxKind::available()
                .into_iter()
                .map(|kind| (kind, name(kind.name())))
                .collect::<Vec<_>>();
            parameters::selector(
                ui,
                "kind",
                &mut kind,
                &options
                    .iter()
                    .map(|(kind, label)| (*kind, label.as_str()))
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
    kind.name()
}
pub fn track_name(kind: TrackFxKind) -> &'static str {
    kind.name()
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
            EditorPage::Envelope => {
                ui.push_id("amp-envelope", |ui| {
                    parameters::envelope(ui, &mut osc.envelope)
                });
            }
            EditorPage::Filter => parameters::filter(ui, &mut osc.osc_filter, true),
            EditorPage::FilterEnvelope => {
                ui.push_id("filter-envelope", |ui| {
                    parameters::envelope(ui, &mut osc.osc_filter_env)
                });
            }
            EditorPage::Modulation => synth_controls::modulation(ui, osc),
            EditorPage::Sound | EditorPage::Automation => synth_controls::sound(ui, osc, full),
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
            EditorPage::Sound | EditorPage::Automation => {
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
        InputFx::Filter(filter) => {
            parameters::filter(ui, filter, full);
        }
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
                parameters::integer(ui, &mut vocoder.tone, -50, 50, lang.text("Tone"));
                parameters::integer(
                    ui,
                    &mut vocoder.mod_sens,
                    -50,
                    50,
                    lang.text("Mod sensitivity"),
                );
                parameters::integer(
                    ui,
                    &mut vocoder.formant_semitones,
                    -12,
                    12,
                    lang.text("Formant (semitones)"),
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
                parameters::float(
                    ui,
                    &mut delay.time_ms,
                    TRACK_DELAY_TIME_MIN_MS as f32,
                    TRACK_DELAY_TIME_MAX_MS as f32,
                    0.01,
                    lang.choose("Time (ms)", "时间（毫秒）"),
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
                theme::caption(
                    ui,
                    lang.choose(
                        "Repeats 0: manual feedback · Cutoff 0: off",
                        "次数 0：手动反馈 · 切频 0：关闭",
                    ),
                );
            }
        }
        TrackFx::Roll(roll) => roll_parameters(ui, roll, full),
        TrackFx::Filter(filter) => parameters::filter(ui, &mut filter.filter, full),
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
