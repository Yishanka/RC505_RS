use crate::config::config_type::EnumConfig;

const MAX_SEQ_LEN: usize = 12 * 32; 
const TICKS_PER_BEAT: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Note {
    N, C, Cs, D, Ds, E, F, Fs, G, Gs, A, As, B,
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

#[derive(Debug, Clone, Copy, PartialEq)]
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
    note_seq: Vec<Option<NoteOct>>, 
    step_len_seq: Vec<usize>,
    pub sel_idx: Option<usize>, 
    pub note: EnumConfig<Note>, 
    pub octave: EnumConfig<usize>, 
    pub step: EnumConfig<String>, 
    pub edit: EnumConfig<NoteSeqEdit>,
}

impl NoteConfigs {
    pub fn new() -> Self {
        Self { 
            note_seq: vec![], 
            step_len_seq: vec![],
            sel_idx: None, 
            note: EnumConfig::new(
                "Note", 
                Note::C, 
                vec![
                    Note::N, 
                    Note::C, Note::Cs, 
                    Note::D, Note::Ds, 
                    Note::E, 
                    Note::F, Note::Fs, 
                    Note::G, Note::Gs, 
                    Note::A, Note::As, 
                    Note::B
                ]
            ), 
            octave: EnumConfig::new(
                "Octave", 
                4, 
                vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
            ),
            step: EnumConfig::new(
                "Step", 
                "1/4".to_string(),
                vec!["1/6".to_string(), "1/4".to_string(), "1/3".to_string(), "1/2".to_string(), "1".to_string()] 
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

    pub fn notes_per_beat(&self) -> usize {
        match self.step.value.as_str() {
            "1/6" => 6,
            "1/4" => 4,
            "1/3" => 3,
            "1/2" => 2,
            "1" => 1,
            _ => 4,
        }
    }

    pub fn ticks_per_note(&self) -> usize {
        let per_beat = self.notes_per_beat();
        if per_beat == 0 {
            TICKS_PER_BEAT
        } else {
            TICKS_PER_BEAT / per_beat
        }
    }

    pub fn seq(&self) -> &[Option<NoteOct>] {
        &self.note_seq
    }

    pub fn set_seq(&mut self, seq: Vec<Option<NoteOct>>) {
        self.step_len_seq = infer_step_len_seq(&seq);
        self.note_seq = seq;
    }

    pub fn step_len_seq(&self) -> &[usize] {
        &self.step_len_seq
    }

    pub fn set_seq_with_steps(&mut self, seq: Vec<Option<NoteOct>>, step_len_seq: Vec<usize>) {
        if seq.len() != step_len_seq.len() {
            self.set_seq(seq);
            return;
        }
        self.note_seq = seq;
        self.step_len_seq = step_len_seq;
    }

    pub fn current_note_oct(&self) -> Option<NoteOct> {
        match self.note.value {
            Note::N => None,
            _ => Some(NoteOct { note: self.note.value, octave: self.octave.value }),
        }
    }

    pub fn push(&mut self) {
        let ticks = self.ticks_per_note().max(1);
        if self.note_seq.len() + ticks > MAX_SEQ_LEN {
            return;
        }
        let value = self.current_note_oct();
        for i in 0..ticks {
            self.note_seq.push(value);
            self.step_len_seq.push(if i == 0 { ticks } else { 0 });
        }
    }

    pub fn pop(&mut self) {
        if self.note_seq.is_empty() || self.step_len_seq.is_empty() {
            return;
        }

        if let Some((start, len)) = self
            .step_len_seq
            .iter()
            .enumerate()
            .rev()
            .find(|(_, v)| **v > 0)
            .map(|(idx, v)| (idx, *v))
        {
            let expected_end = start + len;
            if expected_end == self.note_seq.len() {
                self.note_seq.truncate(start);
                self.step_len_seq.truncate(start);
                return;
            }
        }

        if let Some(last) = self.note_seq.last().copied() {
            while self.note_seq.last().copied() == Some(last) {
                self.note_seq.pop();
            }
            self.step_len_seq = infer_step_len_seq(&self.note_seq);
        }
    }

    pub fn apply_edit(&mut self) {
        match self.edit.value {
            NoteSeqEdit::Push => self.push(),
            NoteSeqEdit::Pop => self.pop(),
        }
    }

}

#[derive(Debug, Clone, Copy, PartialEq)]
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
