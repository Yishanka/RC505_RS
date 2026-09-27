use crate::config::note_configs::{Note, NoteConfigs, NoteOct};

/// Turns a tick-wide sequence marker into exactly one sample's trigger.
#[derive(Clone, Copy, Default)]
pub struct StepTrigger {
    last_tick: Option<usize>,
}

impl StepTrigger {
    pub fn next(&mut self, seq: &[bool], bpm: usize, seconds: f64, running: bool) -> bool {
        if !running || seq.is_empty() {
            self.last_tick = None;
            return false;
        }
        let tick = (seconds.max(0.0) * bpm.max(1) as f64 / 60.0
            * NoteConfigs::ticks_per_beat() as f64)
            .floor() as usize;
        let changed = self.last_tick != Some(tick);
        self.last_tick = Some(tick);
        changed && seq[tick % seq.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn one_trigger_per_tick_including_consecutive_notes_and_restart() {
        let mut trigger = StepTrigger::default();
        let seq = [true, true, false];
        let mut count = 0;
        for sample in 0..6000 {
            count += trigger.next(&seq, 120, sample as f64 / 48000.0, true) as usize;
        }
        assert_eq!(count, 2);
        assert!(!trigger.next(&seq, 120, 0.0, false));
        assert!(trigger.next(&seq, 120, 0.0, true));
    }
}

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
