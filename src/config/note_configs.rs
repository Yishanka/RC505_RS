use crate::config::config_type::EnumConfig;

const MAX_SEQ_LEN: usize = 12 * 32;
const TICKS_PER_BEAT: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Note {
    N,
    C,
    Cs,
    D,
    Ds,
    E,
    F,
    Fs,
    G,
    Gs,
    A,
    As,
    B,
}

impl std::fmt::Display for Note {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Note::N => "N",
            Note::C => "C",
            Note::Cs => "C#",
            Note::D => "D",
            Note::Ds => "D#",
            Note::E => "E",
            Note::F => "F",
            Note::Fs => "F#",
            Note::G => "G",
            Note::Gs => "G#",
            Note::A => "A",
            Note::As => "A#",
            Note::B => "B",
        };
        write!(f, "{}", label)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NoteOct {
    pub note: Note,
    pub octave: usize,
}

impl std::fmt::Display for NoteOct {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.note, self.octave)
    }
}

pub struct NoteConfigs {
    pub launch_serial: u64,
    pub pending: Option<super::sequence_edit::PendingClip>,
    pub(crate) note_seq: Vec<Option<NoteOct>>,
    pub(crate) events: Vec<super::sequence_edit::NoteEvent>,
    pub(crate) loop_ticks: usize,
    pub clip_name: String,
    pub clip_id: String,
    pub(crate) step_len_seq: Vec<usize>,
    pub sel_idx: Option<usize>,
    pub note: EnumConfig<Note>,
    pub octave: EnumConfig<usize>,
    pub step: EnumConfig<String>,
    pub edit: EnumConfig<NoteSeqEdit>,
}

impl NoteConfigs {
    pub fn new() -> Self {
        Self {
            launch_serial: 0,
            pending: None,
            note_seq: vec![],
            events: vec![],
            loop_ticks: 0,
            clip_name: String::new(),
            clip_id: new_clip_id(),
            step_len_seq: vec![],
            sel_idx: None,
            note: EnumConfig::new(
                "Note",
                Note::C,
                vec![
                    Note::N,
                    Note::C,
                    Note::Cs,
                    Note::D,
                    Note::Ds,
                    Note::E,
                    Note::F,
                    Note::Fs,
                    Note::G,
                    Note::Gs,
                    Note::A,
                    Note::As,
                    Note::B,
                ],
            ),
            octave: EnumConfig::new("Octave", 4, vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]),
            step: EnumConfig::new(
                "Step",
                "1/4".to_string(),
                vec![
                    "1/6".to_string(),
                    "1/4".to_string(),
                    "1/3".to_string(),
                    "1/2".to_string(),
                    "2/3".to_string(),
                    "3/4".to_string(),
                    "5/6".to_string(),
                    "1".to_string(),
                    "2".to_string(),
                ],
            ),
            edit: EnumConfig::new(
                "Seq",
                NoteSeqEdit::Push,
                vec![NoteSeqEdit::Push, NoteSeqEdit::Pop],
            ),
        }
    }

    pub fn ticks_per_beat() -> usize {
        TICKS_PER_BEAT
    }

    pub fn notes_per_beat(&self) -> f32 {
        let (num, den) = self.step_fraction();
        den as f32 / num as f32
    }

    pub fn ticks_per_note(&self) -> usize {
        let per_beat = self.notes_per_beat().max(0.0001);
        ((TICKS_PER_BEAT as f32 / per_beat).round() as usize).max(1)
    }

    pub fn seq(&self) -> &[Option<NoteOct>] {
        &self.note_seq
    }

    pub fn set_seq(&mut self, mut seq: Vec<Option<NoteOct>>) {
        seq.truncate(MAX_SEQ_LEN);
        for value in &mut seq {
            *value = value
                .filter(|note| note.note != Note::N)
                .map(|note| NoteOct {
                    octave: note.octave.min(9),
                    ..note
                });
        }
        self.step_len_seq = infer_step_len_seq(&seq);
        self.note_seq = seq;
        self.import_legacy_events();
    }

    pub fn step_len_seq(&self) -> &[usize] {
        &self.step_len_seq
    }

    pub fn set_seq_with_steps(&mut self, seq: Vec<Option<NoteOct>>, step_len_seq: Vec<usize>) {
        self.set_seq(seq);
        self.step_len_seq = super::sequence_edit::canonical_steps(&self.note_seq, &step_len_seq);
        self.import_legacy_events();
    }

    pub fn current_note_oct(&self) -> Option<NoteOct> {
        match self.note.value {
            Note::N => None,
            _ => Some(NoteOct {
                note: self.note.value,
                octave: self.octave.value,
            }),
        }
    }

    pub fn push(&mut self) {
        let ticks = self.ticks_per_note().max(1) * super::sequence_edit::LEGACY_SCALE;
        let start = self.loop_ticks;
        if start + ticks > super::sequence_edit::MAX_TICKS {
            return;
        }
        let mut events = self.events();
        if let Some(pitch) = self.current_note_oct() {
            events.push(super::sequence_edit::NoteEvent::new(start, ticks, pitch));
        }
        self.replace_events(start + ticks, &events);
    }

    pub fn pop(&mut self) {
        let end = self.events.last().map(|n| n.start).unwrap_or_else(|| {
            self.loop_ticks
                .saturating_sub(self.ticks_per_note() * super::sequence_edit::LEGACY_SCALE)
        });
        self.replace_events(end, &self.events());
    }

    pub fn apply_edit(&mut self) {
        match self.edit.value {
            NoteSeqEdit::Push => self.push(),
            NoteSeqEdit::Pop => self.pop(),
        }
    }
}

impl NoteConfigs {
    fn step_fraction(&self) -> (usize, usize) {
        match self.step.value.as_str() {
            "1/6" => (1, 6),
            "1/4" => (1, 4),
            "1/3" => (1, 3),
            "1/2" => (1, 2),
            "2/3" => (2, 3),
            "3/4" => (3, 4),
            "5/6" => (5, 6),
            "1" => (1, 1),
            "2" => (2, 1),
            _ => (1, 4),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum NoteSeqEdit {
    Push,
    Pop,
}

impl std::fmt::Display for NoteSeqEdit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            NoteSeqEdit::Push => "Push",
            NoteSeqEdit::Pop => "Pop",
        };
        write!(f, "{}", label)
    }
}

impl NoteOct {
    pub fn freq_hz(&self) -> f32 {
        let note_index = match self.note {
            Note::C => 0,
            Note::Cs => 1,
            Note::D => 2,
            Note::Ds => 3,
            Note::E => 4,
            Note::F => 5,
            Note::Fs => 6,
            Note::G => 7,
            Note::Gs => 8,
            Note::A => 9,
            Note::As => 10,
            Note::B => 11,
            Note::N => 0,
        };
        let semitones_from_a4: i32 = (self.octave as i32 - 4) * 12 + (note_index - 9);
        440.0 * 2.0_f32.powf(semitones_from_a4 as f32 / 12.0)
    }
}

impl crate::config::config_type::ConfigSet for NoteConfigs {
    fn next(&mut self) {
        if self.sel_idx.is_none() {
            self.sel_idx = Some(0);
        } else {
            self.sel_idx = Some((self.sel_idx.unwrap() + 1) % 4);
        }
    }

    fn prev(&mut self) {
        if self.sel_idx.is_none() {
            self.sel_idx = Some(0);
        } else {
            self.sel_idx = Some((self.sel_idx.unwrap() + 3) % 4);
        }
    }

    fn confirm(&mut self) {}
}

fn infer_step_len_seq(seq: &[Option<NoteOct>]) -> Vec<usize> {
    let mut out = vec![0; seq.len()];
    let mut i = 0usize;
    while i < seq.len() {
        let mut j = i + 1;
        while j < seq.len() && seq[j] == seq[i] {
            j += 1;
        }
        out[i] = j - i;
        i = j;
    }
    out
}

fn new_clip_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static ID: AtomicU64 = AtomicU64::new(1);
    format!(
        "phrase-{:x}-{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |v| v.as_nanos()),
        ID.fetch_add(1, Ordering::Relaxed)
    )
}
impl NoteConfigs {
    pub fn fork_clip_identity(&mut self) {
        self.clip_id = new_clip_id();
    }
}
