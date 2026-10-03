//! Polyphonic event editing. Legacy tick arrays are derived views used by old
//! renderers and UI counters; overlapping notes retain independent identities.
use super::note_configs::{Note, NoteConfigs, NoteOct};

pub const PPQ: usize = 960;
pub const LEGACY_SCALE: usize = PPQ / 12;
pub const TICKS_PER_BAR: usize = PPQ * 4;
pub const MAX_TICKS: usize = TICKS_PER_BAR * 8;

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

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NoteEvent {
    #[serde(default)]
    pub id: u64,
    #[serde(default = "default_velocity")]
    pub velocity: u8,
    pub start: usize,
    pub len: usize,
    pub pitch: NoteOct,
}

fn default_velocity() -> u8 {
    100
}
impl NoteEvent {
    pub fn new(start: usize, len: usize, pitch: NoteOct) -> Self {
        Self {
            id: 0,
            start,
            len,
            pitch,
            velocity: 100,
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NoteClip {
    #[serde(default = "legacy_ppq")]
    pub ppq: usize,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub length: usize,
    pub events: Vec<NoteEvent>,
}

fn legacy_ppq() -> usize {
    12
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
    pub fn loop_len(&self) -> usize {
        self.loop_ticks
    }
    pub fn events(&self) -> Vec<NoteEvent> {
        self.events.clone()
    }
    pub fn event_slice(&self) -> &[NoteEvent] {
        &self.events
    }
    pub fn clip(&self) -> NoteClip {
        NoteClip {
            ppq: PPQ,
            id: self.clip_id.clone(),
            name: self.clip_name.clone(),
            length: self.loop_ticks,
            events: self.events(),
        }
    }
    pub fn set_clip(&mut self, clip: &NoteClip) {
        if clip.id.is_empty() {
            self.fork_clip_identity();
        } else {
            self.clip_id = clip.id.chars().take(128).collect();
        }
        self.clip_name = clip.name.chars().take(128).collect();
        let ppq = clip.ppq.clamp(1, 960_000) as u128;
        let scale = |ticks: usize| {
            ((ticks as u128 * PPQ as u128 + ppq / 2) / ppq).min(MAX_TICKS as u128) as usize
        };
        if ppq == PPQ as u128 {
            self.replace_events(clip.length, &clip.events);
        } else {
            let events: Vec<_> = clip
                .events
                .iter()
                .take(2048)
                .map(|e| NoteEvent {
                    start: scale(e.start),
                    len: scale(e.len).max(1),
                    ..*e
                })
                .collect();
            self.replace_events(scale(clip.length), &events);
        }
    }
    pub(crate) fn import_legacy_events(&mut self) {
        self.events.clear();
        self.loop_ticks = self.note_seq.len() * LEGACY_SCALE;
        let mut start = 0;
        while start < self.note_seq.len() {
            let end = (start + self.step_len_seq.get(start).copied().unwrap_or(1).max(1))
                .min(self.note_seq.len());
            if let Some(pitch) = self.note_seq[start] {
                self.events.push(NoteEvent {
                    id: self.events.len() as u64 + 1,
                    start: start * LEGACY_SCALE,
                    len: (end - start) * LEGACY_SCALE,
                    pitch,
                    velocity: 100,
                });
            }
            start = end;
        }
    }
    pub fn replace_events(&mut self, length: usize, events: &[NoteEvent]) {
        let length = length.min(MAX_TICKS);
        let mut out = Vec::with_capacity(events.len().min(2048));
        let mut next_id = events
            .iter()
            .map(|e| e.id)
            .max()
            .unwrap_or(0)
            .wrapping_add(1)
            .max(1);
        for event in events.iter().take(2048) {
            if event.len == 0 || event.start >= length || event.pitch.note == Note::N {
                continue;
            }
            let mut e = *event;
            e.len = e.len.min(length - e.start);
            e.pitch = NoteOct::from_pitch_index(e.pitch.pitch_index());
            e.velocity = e.velocity.clamp(1, 127);
            if e.id == 0 || out.iter().any(|old: &NoteEvent| old.id == e.id) {
                while out.iter().any(|old: &NoteEvent| old.id == next_id) {
                    next_id = next_id.wrapping_add(1).max(1);
                }
                e.id = next_id;
                next_id = e.id.wrapping_add(1).max(1);
            }
            out.push(e);
        }
        out.sort_by_key(|e| (e.start, e.id));
        // Project old single-note data only for compatibility. The event list is authoritative.
        let legacy_length = length.div_ceil(LEGACY_SCALE);
        let mut seq = vec![None; legacy_length];
        let mut markers = vec![0; legacy_length];
        for e in &out {
            let start = e.start / LEGACY_SCALE;
            let end = (e.start + e.len).div_ceil(LEGACY_SCALE).min(legacy_length);
            seq[start..end].fill(Some(e.pitch));
            markers[start] = end - start;
            if end < legacy_length {
                markers[end] = 1;
            }
        }
        self.step_len_seq = canonical_steps(&seq, &markers);
        self.note_seq = seq;
        self.loop_ticks = length;
        self.events = out;
    }
    pub fn insert_event(&mut self, mut event: NoteEvent) {
        if event.start >= MAX_TICKS || event.len == 0 {
            return;
        }
        event.len = event.len.min(MAX_TICKS - event.start);
        let mut events = self.events();
        if event.id != 0 {
            events.retain(|old| old.id != event.id);
        }
        events.push(event);
        self.replace_events(self.loop_ticks.max(event.start + event.len), &events);
    }
    /// Legacy start-based removal is intentionally limited to one note. The editor uses IDs.
    pub fn remove_event(&mut self, start: usize) {
        if let Some(id) = self.events.iter().find(|e| e.start == start).map(|e| e.id) {
            self.remove_id(id);
        }
    }
    pub fn remove_id(&mut self, id: u64) {
        let events: Vec<_> = self.events.iter().copied().filter(|e| e.id != id).collect();
        self.replace_events(self.loop_ticks, &events);
    }
    pub fn insert_within_loop(&mut self, mut event: NoteEvent) -> bool {
        let length = self.loop_ticks;
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
        self.replace_events(self.loop_ticks, &events);
    }
    pub fn duplicate(&mut self) {
        let length = self.loop_ticks;
        if length == 0 || length * 2 > MAX_TICKS {
            return;
        }
        let mut events = self.events();
        events.extend(self.events().into_iter().map(|n| NoteEvent {
            id: 0,
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
        c.replace_events(12 * LEGACY_SCALE, &[]);
        assert!(!c.insert_within_loop(note(24, 3, 48)));
        assert_eq!(c.seq().len(), 12);
        assert!(c.insert_within_loop(note(10, 12, 48)));
        assert_eq!(c.events()[0].len, 2 * LEGACY_SCALE);
        c.remove_event(10 * LEGACY_SCALE);
        assert_eq!(c.seq().len(), 12);
    }
    fn note(start: usize, len: usize, pitch: usize) -> NoteEvent {
        NoteEvent {
            id: 0,
            velocity: 100,
            start: start * LEGACY_SCALE,
            len: len * LEGACY_SCALE,
            pitch: NoteOct::from_pitch_index(pitch),
        }
    }
    #[test]
    fn polyphonic_overlaps_keep_independent_ids() {
        let mut seq = NoteConfigs::new();
        seq.insert_event(note(0, 12, 48));
        seq.insert_event(note(0, 12, 52));
        seq.insert_event(note(3, 3, 48));
        assert_eq!(seq.events().len(), 3);
        let ids: Vec<_> = seq.events().iter().map(|e| e.id).collect();
        assert_ne!(ids[0], ids[1]);
        assert_ne!(ids[0], ids[2]);
        seq.remove_id(ids[2]);
        assert_eq!(seq.events().len(), 2);
        assert_eq!(seq.seq().len(), 12);
    }
    #[test]
    fn repeated_notes_survive_duplication_and_serializable_ticks() {
        let mut seq = NoteConfigs::new();
        seq.replace_events(12 * LEGACY_SCALE, &[note(0, 3, 48), note(3, 3, 48)]);
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
        assert_eq!(seq.seq().len(), MAX_TICKS / LEGACY_SCALE);
        assert!(seq.step_len_seq().iter().all(|len| *len == 1));
        assert!(seq.events().iter().all(|event| event.pitch.octave == 9));
        seq.set_seq_with_steps(vec![Some(NoteOct::from_pitch_index(48)); 12], vec![]);
        assert_eq!(seq.events()[0].len, 12 * LEGACY_SCALE);
        assert_eq!(seq.events()[0].pitch, note(0, 12, 48).pitch);
    }
    #[test]
    fn legacy_clip_resolution_is_exact_and_fine_notes_roundtrip() {
        let old = NoteClip {
            ppq: 12,
            id: "legacy".into(),
            name: "old phrase".into(),
            length: 48,
            events: vec![NoteEvent::new(3, 5, NoteOct::from_pitch_index(48))],
        };
        let mut c = NoteConfigs::new();
        c.set_clip(&old);
        assert_eq!(c.loop_len(), 3840);
        assert_eq!(c.events()[0].start, 240);
        assert_eq!(c.events()[0].len, 400);
        let mut fine = c.clip();
        fine.events[0].start = 241;
        fine.events[0].len = 121;
        let text = serde_json::to_string(&fine).unwrap();
        let decoded: NoteClip = serde_json::from_str(&text).unwrap();
        c.set_clip(&decoded);
        assert_eq!(c.events()[0].start, 241);
        assert_eq!(c.events()[0].len, 121);
        assert_eq!(c.clip().ppq, PPQ);
        let missing = serde_json::json!({"length":48,"events":[],"id":"old"});
        let old: NoteClip = serde_json::from_value(missing).unwrap();
        assert_eq!(old.ppq, 12);
    }
}
