//! Editing operations for the monophonic tick sequence. UI and persistence share
//! the existing tick representation, including explicit repeated-note boundaries.
use super::note_configs::{Note, NoteConfigs, NoteOct};

pub const MAX_TICKS: usize = 384;
pub const TICKS_PER_BAR: usize = 48;

/// Rebuild lengths from explicit start markers and value changes. Malformed or
/// legacy lengths cannot produce out-of-range indexing in the audio snapshot.
pub fn canonical_steps<T: PartialEq>(seq: &[T], markers: &[usize]) -> Vec<usize> {
    let mut steps = vec![0; seq.len()];
    let mut start = 0;
    while start < seq.len() {
        let mut end = start + 1;
        while end < seq.len()
            && markers.get(end).copied().unwrap_or(0) == 0
            && seq[end] == seq[start]
        {
            end += 1;
        }
        steps[start] = end - start;
        start = end;
    }
    steps
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoteEvent {
    pub start: usize,
    pub len: usize,
    pub pitch: NoteOct,
}

impl NoteOct {
    pub fn pitch_index(self) -> usize {
        let semitone = match self.note {
            Note::C | Note::N => 0,
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
        };
        self.octave.min(9) * 12 + semitone
    }

    pub fn from_pitch_index(index: usize) -> Self {
        let index = index.min(119);
        let notes = [
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
        ];
        Self {
            note: notes[index % 12],
            octave: index / 12,
        }
    }
}

impl NoteConfigs {
    pub fn events(&self) -> Vec<NoteEvent> {
        let mut events = Vec::new();
        let mut start = 0;
        while start < self.seq().len() {
            let mut end = start + 1;
            while end < self.seq().len()
                && self.step_len_seq()[end] == 0
                && self.seq()[end] == self.seq()[start]
            {
                end += 1;
            }
            if let Some(pitch) = self.seq()[start] {
                events.push(NoteEvent {
                    start,
                    len: end - start,
                    pitch,
                });
            }
            start = end;
        }
        events
    }

    pub fn replace_events(&mut self, length: usize, events: &[NoteEvent]) {
        let length = length.min(MAX_TICKS);
        let mut seq = vec![None; length];
        let mut starts = vec![false; length];
        for event in events {
            if event.len == 0 || event.start >= length {
                continue;
            }
            let end = event.start.saturating_add(event.len).min(length);
            seq[event.start..end].fill(Some(event.pitch));
            starts[event.start..end].fill(false);
            starts[event.start] = true;
            if end < length {
                starts[end] = true;
            }
        }
        let mut steps = vec![0; length];
        let mut start = 0;
        while start < length {
            let mut end = start + 1;
            while end < length && !starts[end] && seq[end] == seq[start] {
                end += 1;
            }
            steps[start] = end - start;
            start = end;
        }
        self.set_seq_with_steps(seq, steps);
    }

    pub fn insert_event(&mut self, event: NoteEvent) {
        if event.start >= MAX_TICKS || event.len == 0 {
            return;
        }
        let end = event.start.saturating_add(event.len).min(MAX_TICKS);
        let mut events = Vec::new();
        for old in self.events() {
            let old_end = old.start + old.len;
            if old_end <= event.start || old.start >= end {
                events.push(old);
            } else {
                if old.start < event.start {
                    events.push(NoteEvent {
                        len: event.start - old.start,
                        ..old
                    });
                }
                if old_end > end {
                    events.push(NoteEvent {
                        start: end,
                        len: old_end - end,
                        ..old
                    });
                }
            }
        }
        events.push(NoteEvent {
            len: end - event.start,
            ..event
        });
        self.replace_events(self.seq().len().max(end), &events);
    }

    pub fn remove_event(&mut self, start: usize) {
        let events: Vec<_> = self
            .events()
            .into_iter()
            .filter(|n| n.start != start)
            .collect();
        self.replace_events(self.seq().len(), &events);
    }
    /// Editor operations preserve the explicitly selected loop length.
    pub fn insert_within_loop(&mut self, mut event: NoteEvent) -> bool {
        let length = self.seq().len();
        if event.start >= length || event.len == 0 {
            return false;
        }
        event.len = event.len.min(length - event.start);
        self.insert_event(event);
        true
    }

    pub fn transpose(&mut self, semitones: i32) {
        let mut events = self.events();
        for event in &mut events {
            event.pitch = NoteOct::from_pitch_index(
                (event.pitch.pitch_index() as i32 + semitones).clamp(0, 119) as usize,
            );
        }
        self.replace_events(self.seq().len(), &events);
    }

    pub fn duplicate(&mut self) {
        let length = self.seq().len();
        if length == 0 || length * 2 > MAX_TICKS {
            return;
        }
        let mut events = self.events();
        events.extend(self.events().into_iter().map(|n| NoteEvent {
            start: n.start + length,
            ..n
        }));
        self.replace_events(length * 2, &events);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_editing_cannot_extend_the_loop() {
        let mut c = NoteConfigs::new();
        c.replace_events(12, &[]);
        assert!(!c.insert_within_loop(note(24, 3, 48)));
        assert_eq!(c.seq().len(), 12);
        assert!(c.insert_within_loop(note(10, 12, 48)));
        assert_eq!(c.events()[0].len, 2);
        c.remove_event(10);
        assert_eq!(c.seq().len(), 12);
    }
    fn note(start: usize, len: usize, pitch: usize) -> NoteEvent {
        NoteEvent {
            start,
            len,
            pitch: NoteOct::from_pitch_index(pitch),
        }
    }
    #[test]
    fn overlap_splits_and_preserves_neighbours() {
        let mut seq = NoteConfigs::new();
        seq.insert_event(note(0, 12, 48));
        seq.insert_event(note(3, 3, 55));
        assert_eq!(
            seq.events(),
            vec![note(0, 3, 48), note(3, 3, 55), note(6, 6, 48)]
        );
        seq.remove_event(3);
        assert_eq!(&seq.seq()[3..6], &[None; 3]);
        assert_eq!(seq.seq().len(), 12);
    }
    #[test]
    fn repeated_notes_survive_duplication_and_serializable_ticks() {
        let mut seq = NoteConfigs::new();
        seq.replace_events(12, &[note(0, 3, 48), note(3, 3, 48)]);
        seq.duplicate();
        let mut restored = NoteConfigs::new();
        restored.set_seq_with_steps(seq.seq().to_vec(), seq.step_len_seq().to_vec());
        assert_eq!(restored.events().len(), 4);
        assert_eq!(restored.seq().len(), 24);
        restored.transpose(-1000);
        assert!(restored.events().iter().all(|e| e.pitch.pitch_index() == 0));
    }
    #[test]
    fn malformed_import_is_bounded_and_has_valid_spans() {
        let mut seq = NoteConfigs::new();
        seq.set_seq_with_steps(
            vec![
                Some(NoteOct {
                    note: Note::C,
                    octave: 100
                });
                1000
            ],
            vec![usize::MAX; 1000],
        );
        assert_eq!(seq.seq().len(), MAX_TICKS);
        assert!(seq.step_len_seq().iter().all(|len| *len == 1));
        assert!(seq.events().iter().all(|event| event.pitch.octave == 9));
        seq.set_seq_with_steps(vec![Some(NoteOct::from_pitch_index(48)); 12], vec![]);
        assert_eq!(seq.events(), vec![note(0, 12, 48)]);
    }
}
