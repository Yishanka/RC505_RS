use super::{editor::EditorState, theme};
use crate::{
    app::{Focus, LeftPage, MyApp},
    config::AppConfig,
    engine::core::{Action, EngineView},
    presets::FxTarget,
    replay::ReplayVisuals,
};
use eframe::egui;
use std::sync::{Arc, atomic::Ordering};
pub struct ReplayPanel {
    pub visuals: Arc<ReplayVisuals>,
    stream: Option<crate::replay::streaming::Session>,
    pending: std::collections::VecDeque<crate::replay::streaming::DisplayFrame>,
    current_data: Option<Arc<crate::project::ProjectData>>,
    stream_revision: u64,
    seek_position: Option<f64>,
    import_open: bool,
    import_target: Option<usize>,
    import_name: String,
    error: Option<String>,
    config: AppConfig,
    data: serde_json::Value,
    next_config: usize,
    position: u64,
    view: EngineView,
    editor: EditorState,
    last_action: Option<(u64, Action)>,
    track: Option<usize>,
    track_changed_at: u64,
}
impl ReplayPanel {
    pub fn modal_open(&self) -> bool {
        self.import_open
    }
    pub fn dismiss_modal(&mut self) -> bool {
        std::mem::take(&mut self.import_open)
    }
    pub fn request_import(&mut self) {
        if self.error.is_none()
            && self
                .stream
                .as_ref()
                .is_some_and(|s| !s.shared.playing() && !s.shared.seeking())
        {
            self.import_open = true;
        }
    }
    #[cfg(debug_assertions)]
    pub fn show_import_preview(&mut self) {
        self.import_open = true;
    }
    pub fn toggle(&self) {
        if let Some(session) = &self.stream {
            session.shared.toggle();
        }
    }
    pub fn new(visuals: Arc<ReplayVisuals>) -> Self {
        let mut config = AppConfig::new(120, 0, 5);
        crate::project::apply_data_to_config(&mut config, visuals.initial.clone());
        let data = serde_json::to_value(&visuals.initial).unwrap();
        let mut editor = EditorState::default();
        editor.select(FxTarget::Input {
            bank: config.input_fx.sel_bank_idx,
            slot: 0,
        });
        Self {
            visuals,
            stream: None,
            pending: std::collections::VecDeque::new(),
            current_data: None,
            stream_revision: 0,
            seek_position: None,
            import_open: false,
            import_target: None,
            import_name: String::new(),
            error: None,
            config,
            data,
            next_config: 0,
            position: 0,
            view: EngineView::default(),
            editor,
            last_action: None,
            track: Some(0),
            track_changed_at: 0,
        }
    }
    pub fn streaming(session: crate::replay::streaming::Session) -> Self {
        let mut panel = Self::new(Arc::new(ReplayVisuals {
            name: session.source.metadata.name.clone(),
            sample_rate: session.source.metadata.sample_rate,
            frames: session.source.metadata.frames,
            initial: (*session.initial).clone(),
            configs: Vec::new(),
            views: Vec::new(),
        }));
        panel.current_data = Some(session.initial.clone());
        panel.import_name = format!("{} - replay", session.source.metadata.name);
        panel.stream = Some(session);
        panel
    }
    fn update_stream(&mut self) {
        let Some(session) = &self.stream else {
            return;
        };
        let revision = session.shared.revision();
        if self.stream_revision != revision {
            self.pending.clear();
            self.stream_revision = revision;
        }
        while let Ok(event) = session.display.try_recv() {
            match event {
                crate::replay::streaming::DisplayEvent::Error(error) => self.error = Some(error),
                crate::replay::streaming::DisplayEvent::Frame(frame)
                    if frame.revision == revision =>
                {
                    self.pending.push_back(frame)
                }
                _ => {}
            }
        }
        self.position = session.shared.position();
        while self
            .pending
            .front()
            .is_some_and(|frame| frame.frame <= self.position)
        {
            let frame = self.pending.pop_front().unwrap();
            if self
                .current_data
                .as_ref()
                .is_none_or(|current| !Arc::ptr_eq(current, &frame.data))
            {
                if let Some(previous) = &self.current_data {
                    let delta =
                        crate::replay::ConfigPoint::focus(frame.frame, previous, &frame.data);
                    if let Some(target) = delta.target {
                        self.editor.select(target);
                    }
                    if let Some(track) = delta.track {
                        self.track = Some(track);
                        self.track_changed_at = frame.frame;
                    }
                }
                crate::project::apply_data_to_config(&mut self.config, (*frame.data).clone());
                self.current_data = Some(frame.data);
            }
            self.view = frame.view;
            if self.view.running {
                self.view.elapsed += self.position.saturating_sub(frame.frame);
            }
            self.last_action = frame.last_action;
            if let Some((
                at,
                Action::Trigger(i)
                | Action::Stop(i)
                | Action::Clear(i)
                | Action::Undo(i)
                | Action::UndoStep(i)
                | Action::RedoStep(i),
            )) = self.last_action
            {
                if at >= self.track_changed_at {
                    self.track = Some(i.min(4));
                    self.track_changed_at = at;
                }
            }
        }
    }
    fn update(&mut self, frame: u64, playback_rate: u32) {
        let position = ((frame as u128 * self.visuals.sample_rate as u128
            / playback_rate.max(1) as u128) as u64)
            .min(self.visuals.frames);
        let rewind = position < self.position;
        if rewind {
            self.next_config = 0;
            self.data = serde_json::to_value(&self.visuals.initial).unwrap();
            self.track = Some(0);
            self.track_changed_at = 0;
            self.editor.select(FxTarget::Input {
                bank: self.visuals.initial.input_fx.selected_bank_idx.min(3),
                slot: 0,
            });
        }
        self.position = position;
        let mut changed = rewind;
        while let Some(point) = self
            .visuals
            .configs
            .get(self.next_config)
            .filter(|c| c.frame <= position)
        {
            crate::replay::visuals::apply(&mut self.data, &point.changes);
            if let Some(target) = point.target {
                self.editor.select(target);
            }
            if let Some(track) = point.track {
                self.track = Some(track);
                self.track_changed_at = point.frame;
            }
            self.next_config += 1;
            changed = true;
        }
        if changed {
            crate::project::apply_data_to_config(
                &mut self.config,
                serde_json::from_value(self.data.clone()).expect("validated replay configuration"),
            );
        }
        let index = self
            .visuals
            .views
            .partition_point(|v| v.frame <= position)
            .saturating_sub(1);
        if let Some(frame) = self.visuals.views.get(index) {
            self.view = frame.view;
            // Advance the displayed beat/playhead between cached 30 Hz views.
            if self.view.running {
                self.view.elapsed += position.saturating_sub(frame.frame);
            }
            self.last_action = frame.last_action;
            if let Some((
                at,
                Action::Trigger(i)
                | Action::Stop(i)
                | Action::Clear(i)
                | Action::Undo(i)
                | Action::UndoStep(i)
                | Action::RedoStep(i),
            )) = frame.last_action
            {
                if at >= self.track_changed_at {
                    self.track = Some(i.min(4));
                    self.track_changed_at = at;
                }
            }
        }
    }
}
pub fn draw(ui: &mut egui::Ui, app: &mut MyApp) {
    let lang = app.language;
    let Some(mut panel) = app.replay_panel.take() else {
        ui.spinner();
        return;
    };
    if panel.stream.is_some() {
        panel.update_stream();
    } else {
        panel.update(
            app.audio.diagnostics.player_frame.load(Ordering::Relaxed),
            app.audio.config.sample_rate.0,
        );
    }
    let playing = panel
        .stream
        .as_ref()
        .map(|s| s.shared.playing())
        .unwrap_or_else(|| app.audio.diagnostics.player_playing.load(Ordering::Relaxed));
    let seeking = panel.stream.as_ref().is_some_and(|s| s.shared.seeking());
    let display_enabled = ui.is_enabled();
    ui.set_enabled(display_enabled && !panel.import_open);
    let mut close = false;
    theme::control_row(ui, |ui| {
        if theme::action(
            ui,
            theme::Icon::Back,
            if app.app_state == crate::state::AppState::Init {
                lang.choose("Projects", "返回工程列表")
            } else {
                lang.choose("Performance", "返回演奏")
            },
            "Esc",
        )
        .clicked()
        {
            close = true;
        }
        ui.strong(format!(
            "{} · {}",
            lang.choose("Replay", "回放"),
            panel.visuals.name
        ));
        theme::caption(
            ui,
            if panel.view.input_latency_pending {
                lang.choose("Effect change queued", "效果切换等待录音结束")
            } else {
                lang.choose("Live simulation · temporary panel", "实时演算 · 临时面板")
            },
        );
        app.language_switch(ui);
    });
    theme::control_row(ui, |ui| {
        if theme::action_fixed(
            ui,
            if playing {
                theme::Icon::Stop
            } else {
                theme::Icon::Play
            },
            lang.text(if playing { "Pause" } else { "Play" }),
            "Space",
        )
        .clicked()
        {
            panel.toggle();
        }
        if ui
            .add_enabled(
                !playing
                    && !seeking
                    && panel.stream.is_some()
                    && panel.error.is_none()
                    && !app.busy()
                    && !app.read_only,
                egui::Button::new(lang.choose("Import this position", "导入当前时刻")).wrap(false),
            )
            .clicked()
        {
            panel.request_import();
        }
        ui.label(format!(
            "{:.2} / {:.2} s",
            panel.position as f64 / panel.visuals.sample_rate as f64,
            panel.visuals.frames as f64 / panel.visuals.sample_rate as f64
        ));
        ui.label(format!("BPM {}", panel.config.beat_config.current_bpm()));
        ui.label(lang.choose(
            if panel.view.metronome {
                "Recorded metronome: on"
            } else {
                "Recorded metronome: off"
            },
            if panel.view.metronome {
                "录制时节拍器：开启"
            } else {
                "录制时节拍器：关闭"
            },
        ));
    });
    let duration = panel.visuals.frames as f64 / panel.visuals.sample_rate as f64;
    let mut seconds = panel
        .seek_position
        .unwrap_or(panel.position as f64 / panel.visuals.sample_rate as f64);
    let response = ui
        .scope(|ui| {
            ui.spacing_mut().slider_width = (ui.available_width() - 120.0).max(180.0);
            ui.add_enabled(
                panel.stream.is_some() && panel.error.is_none() && !app.busy(),
                egui::Slider::new(&mut seconds, 0.0..=duration)
                    .show_value(false)
                    .text(lang.choose("Seek", "跳转进度")),
            )
        })
        .inner;
    if response.dragged() {
        panel.seek_position = Some(seconds);
    }
    if response.drag_stopped() || (response.changed() && !response.dragged()) {
        if let Some(session) = &panel.stream {
            session
                .shared
                .seek((seconds * panel.visuals.sample_rate as f64).round() as u64);
        }
        panel.seek_position = None;
    }
    if seeking {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(lang.choose(
                "Reconstructing the selected position from original inputs…",
                "正在按原始输入重建所选位置……",
            ));
        });
    }
    if let Some(error) = &panel.error {
        ui.colored_label(ui.visuals().error_fg_color, error);
    }
    if let Some(session) = &panel.stream {
        let underruns = session.shared.underruns.load(Ordering::Relaxed);
        if underruns > 0 {
            theme::caption(ui,lang.choose("Playback waited for the simulation worker; the playhead stays sample-accurate during an audio gap.","演算线程曾未及时提供音频；发生等待时保持原采样位置，不跳过回放内容。"));
        }
    }
    if panel
        .stream
        .as_ref()
        .is_some_and(|s| s.source.metadata.renderer < crate::replay::RENDERER_VERSION)
    {
        theme::caption(
            ui,
            lang.choose(
                "Recorded with an older beta renderer; current algorithms may change its timbre.",
                "此回放录于旧测试版；当前算法重新演算时音色可能变化。",
            ),
        );
    }
    if panel
        .stream
        .as_ref()
        .is_some_and(|session| session.source.legacy_mydelay)
    {
        ui.label(lang.choose("This replay contains legacy MyDelay. The old captured source is interpreted as the new OSC; missing historical capture buffers prevent exact reproduction. Export audio in the older app version first if you need that original sound.","此回放包含旧版 MyDelay。旧捕获音源会按新版 OSC 解释；缺失旧捕获缓存时无法精确复现。需要保留原音色时，建议先用旧版软件导出音频。"));
    }
    if let Some((at, action)) = panel.last_action {
        let (label, track) = match action {
            Action::Trigger(i) => (
                lang.choose("Record / play / overdub", "录放 / 叠录"),
                Some(i),
            ),
            Action::Stop(i) => (lang.text("Stop"), Some(i)),
            Action::Clear(i) => (lang.choose("Clear audio", "清空音频"), Some(i)),
            Action::Undo(i) | Action::UndoStep(i) => (lang.text("Undo"), Some(i)),
            Action::RedoStep(i) => (lang.text("Redo"), Some(i)),
            Action::Metronome(_) => (lang.choose("Metronome", "节拍器"), None),
            _ => (lang.choose("Transport", "演奏启停"), None),
        };
        theme::caption(
            ui,
            format!(
                "{:.2}s · {}{}",
                at as f64 / panel.visuals.sample_rate as f64,
                track
                    .map(|i| format!("{} {} · ", lang.text("Track"), i + 1))
                    .unwrap_or_default(),
                label
            ),
        );
    } else {
        theme::caption(
            ui,
            lang.choose("Recorded operations appear here", "录制操作将在这里显示"),
        );
    }
    // Reuse the exact performance widgets with an isolated read-only display
    // model. Restore live state before sync_config or any audio command routing.
    ui.set_enabled(display_enabled);
    if panel.import_open {
        let mut open = true;
        egui::Window::new(lang.choose("Import paused replay", "导入暂停的回放"))
            .id(egui::Id::new("replay-import-position")).open(&mut open).collapsible(false).default_width(520.0)
            .anchor(egui::Align2::CENTER_CENTER,egui::Vec2::ZERO)
            .show(ui.ctx(),|ui| {
                ui.label(lang.choose("Imports all five track buffers, undo history and sound settings at this position.","导入此刻五轨音频、撤销历史和音色配置。"));
                ui.label(lang.choose("All tracks open stopped.","导入后所有轨道保持暂停。"));
                egui::ComboBox::from_id_source("replay-import-project")
                    .selected_text(panel.import_target.and_then(|i|app.projects.get(i)).map(|p|p.name.as_str()).unwrap_or(lang.choose("New project", "新建工程")))
                    .width(320.0).show_ui(ui,|ui| {
                        ui.selectable_value(&mut panel.import_target,None,lang.choose("New project", "新建工程"));
                        for (index,entry) in app.projects.iter().enumerate() { ui.selectable_value(&mut panel.import_target,Some(index),&entry.name); }
                    });
                if panel.import_target.is_none() { ui.add(egui::TextEdit::singleline(&mut panel.import_name).id(egui::Id::new("replay-import-name")).desired_width(400.0)); }
                ui.label(lang.choose("This replaces the current unsaved workspace with the selected project and replay.","导入会替换当前未保存的工作区。"));
                ui.label(lang.choose("The target project's saved files stay intact until you save. Choose Discard when leaving to restore its saved version.","主动保存前，目标工程的已保存文件保持原样。离开时「放弃修改」可恢复已保存版本。"));
                if ui.add_enabled(!app.busy() && !playing && !seeking,egui::Button::new(lang.choose("Confirm import", "确认导入")).wrap(false)).clicked() {
                    if let Some(session)=&panel.stream {
                        app.import_replay_position(session.source.clone(),panel.position,panel.import_target,panel.import_name.clone());
                        panel.import_open=false;
                    }
                }
            });
        panel.import_open &= open;
    }
    std::mem::swap(&mut app.config, &mut panel.config);
    std::mem::swap(&mut app.view, &mut panel.view);
    std::mem::swap(&mut app.editor, &mut panel.editor);
    let selection = std::mem::replace(&mut app.track_sel, panel.track);
    let left = std::mem::replace(&mut app.left_page, LeftPage::Track);
    let focus = std::mem::replace(&mut app.focus, Focus::Performance);
    let request = std::mem::replace(&mut app.focus_request, false);
    ui.push_id("replay-workspace", |ui| {
        super::performance::workspace(ui, app);
    });
    app.track_sel = selection;
    app.left_page = left;
    app.focus = focus;
    app.focus_request = request;
    std::mem::swap(&mut app.editor, &mut panel.editor);
    std::mem::swap(&mut app.view, &mut panel.view);
    std::mem::swap(&mut app.config, &mut panel.config);
    if close {
        app.close_player();
    } else {
        app.replay_panel = Some(panel);
    }
}

pub fn track_details(ui: &mut egui::Ui, app: &MyApp) {
    let lang = app.language;
    let index = app.track_sel.unwrap_or(0);
    let track = app.view.tracks[index];
    ui.strong(format!(
        "{} {} · {}",
        lang.text("Track"),
        index + 1,
        lang.choose("Recorded state", "录制状态")
    ));
    ui.separator();
    ui.label(format!(
        "{}: {}",
        lang.choose("Loop frames", "循环采样帧"),
        track.frames
    ));
    ui.label(format!(
        "{}: {}",
        lang.choose("Playback position", "播放位置"),
        track.cursor
    ));
    ui.label(format!(
        "{}: {:.1} dB",
        lang.choose("Fader", "音量推子"),
        crate::app::faders::decibels(app.config.track_levels[index])
    ));
    ui.label(format!(
        "{}: {}",
        lang.choose("Recording alignment", "录音对齐"),
        match app.config.track_options[index].record_reference {
            crate::config::track_options::RecordReference::External =>
                lang.choose("Live input", "现场输入"),
            crate::config::track_options::RecordReference::Internal =>
                lang.choose("Internal source", "内部音源"),
        }
    ));
    ui.label(lang.choose(
        "Track audio, FX and parameters follow the replay; the live project is preserved.",
        "轨道音频、效果开关与参数跟随回放；原工程保持原状。",
    ));
}

pub fn parameter_details(ui: &mut egui::Ui, app: &MyApp) {
    use crate::config::{InputFx, TrackFx};
    let lang = app.language;
    let Some(target) = app.editor.target else {
        return;
    };
    let (bank, slot, name) = match target {
        FxTarget::Input { bank, slot } => (
            bank,
            slot,
            super::editor::input_name(app.config.input_fx.slot_kind(bank, slot)),
        ),
        FxTarget::Track { bank, slot } => (
            bank,
            slot,
            super::editor::track_name(app.config.track_fx.slot_kind(bank, slot)),
        ),
    };
    ui.colored_label(
        theme::accent(ui),
        format!(
            "{} / {} / {}",
            bank + 1,
            ['A', 'B', 'C', 'D'][slot],
            lang.text(name)
        ),
    );
    let mut row = |en: &str, zh: &str, value: String| {
        ui.horizontal(|ui| {
            ui.add(egui::Label::new(lang.choose(en, zh)).wrap(false));
            ui.strong(value);
        });
    };
    match target {
        FxTarget::Input { bank, slot } => {
            match app.config.input_fx.banks[bank].slots[slot].fx.as_ref() {
                Some(InputFx::Roll(v)) => {
                    row(
                        "Mode / division",
                        "模式／细分",
                        format!("{} / {}", v.mode.value, v.step.value),
                    );
                    row(
                        "Time",
                        "时间",
                        format!(
                            "{:.2} ms",
                            v.time_mode.value.milliseconds(
                                v.time_ms.value,
                                app.config.beat_config.current_bpm()
                            )
                        ),
                    );
                    row(
                        "Feedback / repeats",
                        "反馈／次数",
                        format!("{} % / {}", v.feedback.value, v.repeat.value),
                    );
                }
                Some(InputFx::Audio(v)) => {
                    row("Effect", "效果", v.kind.name().into());
                    row(
                        "Mix / level",
                        "混合 / 电平",
                        format!("{:.0} % / {:.1} dB", v.mix * 100.0, v.level_db),
                    );
                }
                Some(InputFx::Oscillator(v)) => {
                    row(
                        "Waveform",
                        "波形",
                        lang.text(&v.waveform.value.to_string()).into(),
                    );
                    row(
                        "Level / threshold",
                        "电平 / 阈值",
                        format!("{} / {}", v.level.value, v.threshold.value),
                    );
                    row(
                        "Sequence notes",
                        "序列音符",
                        v.note.events().len().to_string(),
                    );
                }
                Some(InputFx::MyDelay(v)) => {
                    row(
                        "Level / threshold",
                        "电平 / 阈值",
                        format!("{} / {}", v.level.value, v.threshold.value),
                    );
                    row(
                        "Cutoff",
                        "截止频率",
                        format!("{} Hz", v.filter.cutoff_hz.value),
                    );
                    row(
                        "Sequence notes",
                        "序列音符",
                        v.note.events().len().to_string(),
                    );
                }
                Some(InputFx::Filter(v)) => {
                    row("Cutoff", "截止频率", format!("{} Hz", v.cutoff_hz.value));
                    row(
                        "Resonance",
                        "共振",
                        format!("{:.1}", v.resonance_x10.value as f32 / 10.0),
                    );
                    row(
                        "Drive / mix",
                        "驱动 / 混合",
                        format!("{} / {} %", v.drive.value, v.mix.value),
                    );
                }
                Some(InputFx::Reverb(v)) => {
                    row("Decay", "衰减", format!("{} ms", v.decay_ms.value));
                    row("Predelay", "预延迟", format!("{} ms", v.predelay_ms.value));
                    row(
                        "Dry / wet",
                        "干声 / 湿声",
                        format!("{} / {} %", v.dry_level.value, v.wet_level.value),
                    );
                }
                Some(InputFx::Vocoder(v)) => {
                    row("Carrier", "载波", v.carrier.value.to_string());
                    row("Formant", "共振峰", format!("{} st", v.formant_semitones));
                    row(
                        "Bands / mix",
                        "频段 / 混合",
                        format!("{} / {} %", v.bands.value, v.mix.value),
                    );
                }
                None => {}
            }
        }
        FxTarget::Track { bank, slot } => {
            match app.config.track_fx.banks[bank].slots[slot].fx.as_ref() {
                Some(TrackFx::Vocoder(v)) => {
                    row("Carrier", "载波", v.carrier.value.to_string());
                    row("Formant", "共振峰", format!("{} st", v.formant_semitones));
                    row(
                        "Bands / mix",
                        "频段 / 混合",
                        format!("{} / {} %", v.bands.value, v.mix.value),
                    );
                }
                Some(TrackFx::Audio(v)) => {
                    row("Effect", "效果", v.kind.name().into());
                    row(
                        "Mix / level",
                        "混合 / 电平",
                        format!("{:.0} % / {:.1} dB", v.mix * 100.0, v.level_db),
                    );
                }
                Some(TrackFx::Delay(v)) => {
                    row(
                        "Delay time",
                        "延迟时间",
                        format!(
                            "{:.2} ms",
                            v.time_mode
                                .value
                                .milliseconds_f32(v.time_ms, app.config.beat_config.current_bpm())
                        ),
                    );
                    row("Feedback", "反馈", format!("{} %", v.feedback_pct.value));
                    row(
                        "Direct / effect",
                        "直达 / 效果",
                        format!("{} / {} %", v.direct_pct.value, v.effect_pct.value),
                    );
                }
                Some(TrackFx::Roll(v)) => {
                    row("Step", "分割", v.step.value.to_string());
                    row(
                        "Time",
                        "时间",
                        format!(
                            "{:.2} ms",
                            v.time_mode.value.milliseconds(
                                v.time_ms.value,
                                app.config.beat_config.current_bpm()
                            )
                        ),
                    );
                    row(
                        "Feedback / repeats",
                        "反馈 / 次数",
                        format!("{} % / {}", v.feedback.value, v.repeat.value),
                    );
                }
                Some(TrackFx::Filter(v)) => {
                    row(
                        "Cutoff",
                        "截止频率",
                        format!("{} Hz", v.filter.cutoff_hz.value),
                    );
                    row(
                        "Resonance",
                        "共振",
                        format!("{:.1}", v.filter.resonance_x10.value as f32 / 10.0),
                    );
                    row("Mix", "混合", format!("{} %", v.filter.mix.value));
                }
                None => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn playback_cursor_drives_config_and_rewind_without_touching_live_state() {
        let config = AppConfig::new(120, 0, 5);
        let initial = crate::project::data_from_config(&config);
        let mut changed = initial.clone();
        changed.track_levels[0] = 0.25;
        let timeline = Arc::new(ReplayVisuals {
            name: "Test".into(),
            sample_rate: 8000,
            frames: 16000,
            views: Vec::new(),
            configs: vec![crate::replay::ConfigPoint::new(8000, &initial, &changed)],
            initial,
        });
        let mut panel = ReplayPanel::new(timeline);
        panel.update(48000, 48000);
        assert_eq!(panel.config.track_levels[0], 0.25);
        panel.update(0, 48000);
        assert_eq!(panel.config.track_levels[0], 1.0);
        assert_eq!(config.track_levels[0], 1.0);
    }
}
