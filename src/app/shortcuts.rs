//! Application key bindings. The same action IDs drive dispatch, preferences and hints.
use eframe::egui::{self, Event, InputState, Key};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Track(usize),
    Stop(usize),
    Select(usize),
    Undo(usize),
    Redo(usize),
    Fx(usize),
    HoldFx(usize),
    EditFx(usize),
    Bank(usize),
    FaderDown(usize),
    FaderUp(usize),
    Slower(usize),
    Faster(usize),
    All,
    Panic,
    Clear,
    PreviousTrack,
    NextTrack,
    Tap,
    UndoSelected,
    RedoSelected,
    Save,
    Snapshot,
    Top,
    Left,
    Right,
    Take,
    Replays,
    Help,
    Metronome,
    InputThru,
}
#[derive(Clone)]
pub struct Definition {
    pub id: String,
    pub command: Command,
    pub group: &'static str,
    pub en: String,
    pub zh: String,
    pub defaults: Vec<Chord>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chord {
    pub key: String,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub shift: bool,
}
impl Chord {
    pub fn new(key: Key, ctrl: bool, alt: bool, shift: bool) -> Self {
        Self {
            key: key.name().into(),
            ctrl,
            alt,
            shift,
        }
    }
    pub fn from_event(event: &Event) -> Option<Self> {
        if let Event::Key {
            key,
            physical_key,
            pressed: true,
            repeat: false,
            modifiers,
        } = event
        {
            Some(Self::new(
                physical_key.unwrap_or(*key),
                modifiers.ctrl,
                modifiers.alt,
                modifiers.shift,
            ))
        } else {
            None
        }
    }
    fn modifiers_match(&self, m: egui::Modifiers) -> bool {
        self.ctrl == m.ctrl && self.alt == m.alt && self.shift == m.shift && !m.mac_cmd
    }
    pub fn pressed(&self, input: &InputState) -> bool {
        input.events.iter().any(|event| match event {
            Event::Key {
                key,
                pressed: true,
                repeat: false,
                modifiers,
                ..
            } => Key::from_name(&self.key) == Some(*key) && self.modifiers_match(*modifiers),
            _ => false,
        })
    }
    pub fn held(&self, input: &InputState) -> bool {
        self.modifiers_match(input.modifiers)
            && Key::from_name(&self.key).is_some_and(|key| input.key_down(key))
    }
    pub fn label(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.shift {
            parts.push("Shift");
        }
        let symbol = match Key::from_name(&self.key) {
            Some(Key::Comma) => ",",
            Some(Key::Period) => ".",
            Some(Key::Slash) => "/",
            Some(Key::Backslash) => "\\",
            Some(Key::Semicolon) => ";",
            Some(Key::Minus) => "−",
            Some(Key::Equals) => "=",
            Some(Key::OpenBracket) => "[",
            Some(Key::CloseBracket) => "]",
            Some(Key::Backtick) => "`",
            Some(Key::ArrowLeft) => "←",
            Some(Key::ArrowRight) => "→",
            Some(Key::ArrowUp) => "↑",
            Some(Key::ArrowDown) => "↓",
            _ => &self.key,
        };
        parts.push(symbol);
        parts.join("+")
    }
    pub fn allowed(&self) -> bool {
        let Some(key) = Key::from_name(&self.key) else {
            return false;
        };
        // Keep navigation/text editing and OS window management reachable.
        !matches!(key, Key::Escape | Key::Tab | Key::Enter)
            && !(self.alt && matches!(key, Key::F4 | Key::Space))
            && !(self.ctrl && self.alt && key == Key::Delete)
    }
}

pub fn definitions() -> Vec<Definition> {
    let mut result = Vec::new();
    let mut add = |id: String, command, group, en: String, zh: String, defaults| {
        result.push(Definition {
            id,
            command,
            group,
            en,
            zh,
            defaults,
        });
    };
    let chord = |k, c, a, s| Chord::new(k, c, a, s);
    for (id, command, en, zh, key, ctrl, alt, shift) in [
        (
            "all",
            Command::All,
            "Start / stop all",
            "全部启停",
            Key::Space,
            false,
            false,
            false,
        ),
        (
            "panic",
            Command::Panic,
            "Stop all immediately",
            "立即停止全部",
            Key::Space,
            false,
            false,
            true,
        ),
        (
            "clear",
            Command::Clear,
            "Clear selected: hold 0.75 s or double press",
            "清空所选轨：长按 0.75 秒或双击",
            Key::Delete,
            false,
            false,
            false,
        ),
        (
            "previous_track",
            Command::PreviousTrack,
            "Select previous track",
            "选择上一轨",
            Key::ArrowLeft,
            false,
            false,
            false,
        ),
        (
            "next_track",
            Command::NextTrack,
            "Select next track",
            "选择下一轨",
            Key::ArrowRight,
            false,
            false,
            false,
        ),
        (
            "tap",
            Command::Tap,
            "Tap tempo",
            "击拍定速",
            Key::T,
            false,
            false,
            false,
        ),
        (
            "undo_selected",
            Command::UndoSelected,
            "Undo selected track",
            "撤销所选轨",
            Key::Z,
            true,
            false,
            false,
        ),
        (
            "redo_selected",
            Command::RedoSelected,
            "Redo selected track",
            "重做所选轨",
            Key::Y,
            true,
            false,
            false,
        ),
        (
            "metronome",
            Command::Metronome,
            "Toggle metronome",
            "节拍器开关",
            Key::K,
            false,
            false,
            false,
        ),
        (
            "input_thru",
            Command::InputThru,
            "Silent input: toggle input monitoring",
            "静默录入：切换输入监听",
            Key::J,
            false,
            false,
            false,
        ),
        (
            "save",
            Command::Save,
            "Save configuration",
            "保存配置",
            Key::S,
            true,
            false,
            false,
        ),
        (
            "snapshot",
            Command::Snapshot,
            "Save configuration and audio",
            "保存配置与音频",
            Key::S,
            true,
            false,
            true,
        ),
        (
            "top",
            Command::Top,
            "Focus top controls",
            "进入顶部栏",
            Key::F6,
            false,
            false,
            false,
        ),
        (
            "left",
            Command::Left,
            "Focus left controls",
            "进入左面板",
            Key::F7,
            false,
            false,
            false,
        ),
        (
            "right",
            Command::Right,
            "Focus FX controls",
            "进入效果面板",
            Key::F8,
            false,
            false,
            false,
        ),
        (
            "take",
            Command::Take,
            "Start / finish replay capture",
            "开始 / 结束回放录制",
            Key::F9,
            false,
            false,
            false,
        ),
        (
            "replays",
            Command::Replays,
            "Open replay library",
            "打开回放库",
            Key::F10,
            false,
            false,
            false,
        ),
        (
            "help",
            Command::Help,
            "Open help",
            "打开帮助",
            Key::F12,
            false,
            false,
            false,
        ),
    ] {
        let group = if matches!(
            command,
            Command::Save
                | Command::Snapshot
                | Command::Top
                | Command::Left
                | Command::Right
                | Command::Take
                | Command::Replays
                | Command::Help
        ) {
            "global"
        } else {
            "performance"
        };
        let mut defaults = vec![chord(key, ctrl, alt, shift)];
        if command == Command::RedoSelected {
            defaults.push(chord(Key::Z, true, false, true));
        }
        add(id.into(), command, group, en.into(), zh.into(), defaults);
    }
    for (i, key) in [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5]
        .into_iter()
        .enumerate()
    {
        let fkey = [Key::F1, Key::F2, Key::F3, Key::F4, Key::F5][i];
        for (id, command, en, zh, defaults) in [
            (
                "track",
                Command::Track(i),
                "Record / play / overdub",
                "录音 / 播放 / 叠录",
                vec![chord(key, false, false, false)],
            ),
            (
                "stop",
                Command::Stop(i),
                "Stop",
                "停止",
                vec![
                    chord(fkey, false, false, false),
                    chord(key, false, false, true),
                ],
            ),
            (
                "select",
                Command::Select(i),
                "Select",
                "选择",
                vec![chord(key, true, false, false)],
            ),
            (
                "undo",
                Command::Undo(i),
                "Undo",
                "撤销",
                vec![chord(key, false, true, false)],
            ),
            (
                "redo",
                Command::Redo(i),
                "Redo",
                "重做",
                vec![chord(key, true, true, false)],
            ),
        ] {
            add(
                format!("{id}.{i}"),
                command,
                "tracks",
                format!("Track {} · {en}", i + 1),
                format!("轨道 {} · {zh}", i + 1),
                defaults,
            );
        }
    }
    for (i, key) in [
        Key::Q,
        Key::W,
        Key::E,
        Key::R,
        Key::U,
        Key::I,
        Key::O,
        Key::P,
    ]
    .into_iter()
    .enumerate()
    {
        for (id, command, en, zh, c, a, s) in [
            ("fx", Command::Fx(i), "Toggle", "切换", false, false, false),
            (
                "hold_fx",
                Command::HoldFx(i),
                "Hold temporarily",
                "按住临时开启",
                false,
                false,
                true,
            ),
            (
                "edit_fx",
                Command::EditFx(i),
                "Select for editing",
                "选中并编辑",
                true,
                false,
                false,
            ),
            (
                "bank",
                Command::Bank(i),
                "Select bank",
                "选择效果组",
                false,
                true,
                false,
            ),
        ] {
            let side = if i < 4 {
                ("Input", "输入")
            } else {
                ("Track", "轨道")
            };
            add(
                format!("{id}.{i}"),
                command,
                "fx",
                format!("{} {} · {en}", side.0, i % 4 + 1),
                format!("{} {} · {zh}", side.1, i % 4 + 1),
                vec![chord(key, c, a, s)],
            );
        }
    }
    for (i, (down, up)) in [
        (Key::Z, Key::X),
        (Key::C, Key::V),
        (Key::B, Key::N),
        (Key::M, Key::Comma),
        (Key::Period, Key::Slash),
    ]
    .into_iter()
    .enumerate()
    {
        for (id, command, en, zh, key, shift) in [
            (
                "fader_down",
                Command::FaderDown(i),
                "Fader down (hold)",
                "推子降低（按住）",
                down,
                false,
            ),
            (
                "fader_up",
                Command::FaderUp(i),
                "Fader up (hold)",
                "推子升高（按住）",
                up,
                false,
            ),
            (
                "slower",
                Command::Slower(i),
                "Fader slower (hold)",
                "推子减速（按住）",
                down,
                true,
            ),
            (
                "faster",
                Command::Faster(i),
                "Fader faster (hold)",
                "推子加速（按住）",
                up,
                true,
            ),
        ] {
            add(
                format!("{id}.{i}"),
                command,
                "faders",
                format!("Track {} · {en}", i + 1),
                format!("轨道 {} · {zh}", i + 1),
                vec![chord(key, false, false, shift)],
            );
        }
    }
    result
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Bindings {
    /// Missing entries inherit defaults; empty lists deliberately disable an action.
    pub overrides: BTreeMap<String, Vec<Chord>>,
    #[serde(skip)]
    entries: Vec<Definition>,
}
impl Bindings {
    pub fn defaults() -> Self {
        Self {
            entries: definitions(),
            ..Default::default()
        }
    }
    pub fn load() -> Self {
        let mut result: Self = std::fs::read(path())
            .ok()
            .and_then(|raw| serde_json::from_slice(&raw).ok())
            .unwrap_or_default();
        result.entries = definitions();
        result.overrides.retain(|id, chords| {
            result.entries.iter().any(|d| d.id == *id)
                && chords.len() <= 2
                && chords.iter().all(Chord::allowed)
        });
        if result.conflict().is_some() {
            return Self::defaults();
        }
        result
    }
    pub fn save(&self) -> anyhow::Result<()> {
        anyhow::ensure!(self.conflict().is_none(), "Conflicting keyboard shortcuts");
        let path = path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temp = path.with_extension("json.tmp");
        use std::io::Write;
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(&serde_json::to_vec_pretty(self)?)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(temp, path)?;
        Ok(())
    }
    pub fn entries(&self) -> &[Definition] {
        &self.entries
    }
    pub fn chords<'a>(&'a self, d: &'a Definition) -> &'a [Chord] {
        self.overrides.get(&d.id).unwrap_or(&d.defaults)
    }
    pub fn pressed(&self, command: Command, input: &InputState) -> bool {
        self.entries
            .iter()
            .find(|d| d.command == command)
            .is_some_and(|d| self.chords(d).iter().any(|c| c.pressed(input)))
    }
    pub fn held(&self, command: Command, input: &InputState) -> bool {
        self.entries
            .iter()
            .find(|d| d.command == command)
            .is_some_and(|d| self.chords(d).iter().any(|c| c.held(input)))
    }
    pub fn label(&self, command: Command) -> String {
        self.entries
            .iter()
            .find(|d| d.command == command)
            .map(|d| {
                self.chords(d)
                    .iter()
                    .map(Chord::label)
                    .collect::<Vec<_>>()
                    .join(" / ")
            })
            .unwrap_or_default()
    }
    pub fn conflict(&self) -> Option<(String, String, String)> {
        let mut seen = BTreeMap::new();
        for d in &self.entries {
            for c in self.chords(d) {
                if !c.allowed() {
                    return Some((c.label(), d.id.clone(), "reserved".into()));
                }
                if let Some(previous) = seen.insert(c.label(), d.id.clone()) {
                    return Some((c.label(), previous, d.id.clone()));
                }
            }
        }
        None
    }
    pub fn install_hints(&self, ctx: &egui::Context) {
        let mut hints = BTreeMap::new();
        for d in &self.entries {
            // Translate a legacy hint only when it is an unambiguous complete chord.
            for (index, default) in d.defaults.iter().enumerate() {
                let current = self.chords(d);
                hints.insert(
                    default.label(),
                    current
                        .get(index)
                        .or_else(|| current.first())
                        .map(Chord::label)
                        .unwrap_or_default(),
                );
            }
        }
        ctx.data_mut(|data| {
            data.insert_temp(egui::Id::new("shortcut-hints"), std::sync::Arc::new(hints))
        });
    }
}
fn path() -> std::path::PathBuf {
    crate::app_support::paths::launcher_config_path().with_file_name("keyboard.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_have_no_conflicts_and_round_trip_preserves_disabled_actions() {
        let mut bindings = Bindings::defaults();
        assert!(bindings.conflict().is_none());
        bindings.overrides.insert("all".into(), vec![]);
        let mut restored: Bindings =
            serde_json::from_slice(&serde_json::to_vec(&bindings).unwrap()).unwrap();
        restored.entries = definitions();
        assert!(restored.label(Command::All).is_empty());
        assert_eq!(restored.label(Command::Track(0)), "1");
    }
    #[test]
    fn exact_modifiers_repeat_and_collisions_are_enforced() {
        let mut input = InputState::default();
        let mut b = Bindings::defaults();
        input.events.push(Event::Key {
            key: Key::Num1,
            physical_key: Some(Key::Num1),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::ALT,
        });
        assert!(b.pressed(Command::Undo(0), &input));
        assert!(!b.pressed(Command::Track(0), &input));
        if let Event::Key { repeat, .. } = &mut input.events[0] {
            *repeat = true;
        }
        assert!(!b.pressed(Command::Undo(0), &input));
        b.overrides.insert(
            "all".into(),
            vec![Chord::new(Key::Num1, false, false, false)],
        );
        assert!(b.conflict().is_some());
        assert!(!Chord::new(Key::F4, false, true, false).allowed());
    }
}
