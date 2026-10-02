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
    let playing = app.audio.diagnostics.player_playing.load(Ordering::Relaxed);
    let Some(mut panel) = app.replay_panel.take() else {
        ui.spinner();
        return;
    };
    panel.update(
        app.audio.diagnostics.player_frame.load(Ordering::Relaxed),
        app.audio.config.sample_rate.0,
    );
    let mut close = false;
    theme::control_row(ui, |ui| {
        if theme::action(
            ui,
            theme::Icon::Back,
            lang.choose("Performance", "返回演奏"),
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
            lang.choose("Temporary panel · read only", "临时面板 · 只读"),
        );
        app.language_switch(ui);
    });
    theme::control_row(ui, |ui| {
        if theme::action(
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
            app.send(crate::engine::audio_io::Control::PlayerToggle);
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
    ui.add(
        egui::ProgressBar::new(panel.position as f32 / panel.visuals.frames.max(1) as f32)
            .desired_height(5.0)
            .fill(theme::accent(ui)),
    );
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
        FxTarget::Track { bank, slot } => match app.config.track_fx.banks[bank].slots[slot]
            .fx
            .as_ref()
        {
            Some(TrackFx::Delay(v)) => {
                row(
                    "Delay time",
                    "延迟时间",
                    format!(
                        "{:.2} ms",
                        v.time_mode
                            .value
                            .milliseconds(v.time_ms.value, app.config.beat_config.current_bpm())
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
                        v.time_mode
                            .value
                            .milliseconds(v.time_ms.value, app.config.beat_config.current_bpm())
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
        },
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
