use crate::config::note_configs::{Note, NoteConfigs, NoteOct};

pub fn default_note() -> NoteOct {
    NoteOct {
        note: Note::C,
        octave: 4,
    }
}

pub fn note_at_time(seq: &[Option<NoteOct>], bpm: usize, elapsed_secs: f64) -> Option<NoteOct> {
    if seq.is_empty() {
        return Some(default_note());
    }
    let ticks_per_beat = NoteConfigs::ticks_per_beat();
    let secs_per_beat = 60.0 / bpm.max(1) as f64;
    let tick = ((elapsed_secs / secs_per_beat) * ticks_per_beat as f64).floor() as usize;
    let idx = tick % seq.len();
    seq[idx]
}

pub fn seq_bool_at_time(seq: &[bool], bpm: usize, elapsed_secs: f64) -> bool {
    if seq.is_empty() {
        return true;
    }
    let ticks_per_beat = NoteConfigs::ticks_per_beat();
    let secs_per_beat = 60.0 / bpm.max(1) as f64;
    let tick = ((elapsed_secs / secs_per_beat) * ticks_per_beat as f64).floor() as usize;
    let idx = tick % seq.len();
    seq[idx]
}
