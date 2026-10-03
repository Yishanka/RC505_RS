//! Sparse, cached note scheduling prepared outside the audio callback.
use std::sync::Arc;
#[derive(Clone, Copy)]
pub struct ScheduledNote {
    pub id: u64,
    pub frequency: f32,
    pub velocity: f32,
    pub start: usize,
}
#[derive(Clone)]
pub struct NoteBoundary {
    pub at: usize,
    pub notes: Vec<ScheduledNote>,
}
pub(super) fn compiled_schedule(
    c: &crate::config::note_configs::NoteConfigs,
) -> (u64, Arc<Vec<NoteBoundary>>) {
    use crate::config::sequence_edit::NoteEvent;
    use std::{
        collections::VecDeque,
        sync::{Mutex, OnceLock},
    };
    struct Cached {
        revision: u64,
        length: usize,
        events: Vec<NoteEvent>,
        schedule: Arc<Vec<NoteBoundary>>,
    }
    static CACHE: OnceLock<Mutex<VecDeque<Cached>>> = OnceLock::new();
    let revision = c.event_slice().iter().fold(c.loop_len() as u64, |h, n| {
        h.wrapping_mul(1099511628211)
            ^ n.id
            ^ ((n.start as u64) << 8)
            ^ ((n.len as u64) << 20)
            ^ ((n.pitch.pitch_index() as u64) << 40)
            ^ ((n.velocity as u64) << 48)
    });
    let cache = CACHE.get_or_init(|| Mutex::new(VecDeque::new()));
    if let Ok(entries) = cache.lock() {
        if let Some(old) = entries.iter().find(|old| {
            old.revision == revision && old.length == c.loop_len() && old.events == c.event_slice()
        }) {
            return (revision, old.schedule.clone());
        }
    }
    let notes: Vec<_> = c
        .event_slice()
        .iter()
        .map(|n| ScheduledNote {
            id: n.id,
            frequency: n.pitch.freq_hz(),
            velocity: n.velocity as f32 / 127.0,
            start: n.start,
        })
        .collect();
    // Stable sweep: all Note Off events at a boundary precede Note On events.
    // Keep the full active set so Mono can return to a still-held earlier note.
    let mut commands = Vec::with_capacity(c.event_slice().len() * 2 + 1);
    for (index, n) in c.event_slice().iter().enumerate() {
        commands.push((n.start, 1u8, index));
        commands.push((n.start + n.len, 0u8, index));
    }
    commands.sort_unstable();
    let mut active: Vec<usize> = Vec::with_capacity(notes.len());
    let mut boundaries = Vec::with_capacity(commands.len() + 1);
    let mut cursor = 0;
    if c.loop_len() > 0 && commands.first().is_none_or(|command| command.0 > 0) {
        boundaries.push(NoteBoundary {
            at: 0,
            notes: Vec::new(),
        });
    }
    while cursor < commands.len() {
        let at = commands[cursor].0;
        if at >= c.loop_len() {
            break;
        }
        while cursor < commands.len() && commands[cursor].0 == at {
            let (_, on, index) = commands[cursor];
            if on == 0 {
                if let Some(i) = active.iter().position(|i| *i == index) {
                    active.remove(i);
                }
            } else {
                let key = (notes[index].start, notes[index].id);
                let at = active.partition_point(|i| (notes[*i].start, notes[*i].id) < key);
                active.insert(at, index);
            }
            cursor += 1;
        }
        boundaries.push(NoteBoundary {
            at,
            notes: active
                .iter()
                .rev()
                .take(16)
                .rev()
                .map(|i| notes[*i])
                .collect(),
        });
    }
    let schedule = Arc::new(boundaries);
    if let Ok(mut entries) = cache.lock() {
        while entries.len() >= 16
            || entries.iter().map(|v| v.schedule.len()).sum::<usize>() + schedule.len() > 65_536
        {
            if entries.pop_front().is_none() {
                break;
            }
        }
        entries.push_back(Cached {
            revision,
            length: c.loop_len(),
            events: c.events(),
            schedule: schedule.clone(),
        });
    }
    (revision, schedule)
}
