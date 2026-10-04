//! Prepared next-loop phrase changes. Only scalar state changes in the callback;
//! both schedules are compiled on the control thread and owned by the runtime.
use super::note_schedule::{NoteBoundary, compiled_schedule};
use crate::config::note_configs::NoteConfigs;
use std::sync::Arc;

#[derive(Clone)]
pub struct PreparedPhrase {
    pub serial: u64,
    pub revision: u64,
    pub length: usize,
    pub schedule: Arc<Vec<NoteBoundary>>,
}
#[derive(Clone)]
pub struct PhrasePlan {
    pub source: u64,
    pub serial: u64,
    pub queued: Option<PreparedPhrase>,
}
pub fn source_key(id: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut hash);
    hash.finish()
}
impl PhrasePlan {
    pub fn from_config(note: &NoteConfigs) -> Self {
        let queued = note.pending.as_ref().map(|pending| {
            let mut note = NoteConfigs::new();
            note.set_clip(&pending.clip);
            let (revision, schedule) = compiled_schedule(&note);
            PreparedPhrase {
                serial: pending.serial,
                revision,
                length: note.loop_len(),
                schedule,
            }
        });
        Self {
            source: 0,
            serial: note.launch_serial,
            queued,
        }
    }
}
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct PhraseView {
    pub source: u64,
    pub applied_serial: u64,
    pub pending_serial: u64,
    pub switch_tick: u64,
    pub origin_tick: u64,
}
#[derive(Clone, Default)]
pub struct PhraseState {
    initialized: bool,
    base_serial: u64,
    base_origin: u64,
    queued_serial: u64,
    using_queued: bool,
    last_tick: u64,
    last_running: bool,
    queue_length: usize,
    view: PhraseView,
}
pub struct Selection {
    pub queued: bool,
    pub elapsed_ticks: u64,
    pub restarted: bool,
}
impl PhraseState {
    pub fn view(&self) -> PhraseView {
        self.view
    }
    pub fn select(
        &mut self,
        plan: &PhrasePlan,
        tick: u64,
        length: usize,
        running: bool,
    ) -> Selection {
        let mut restarted = false;
        if self.initialized && self.view.source != plan.source {
            *self = Self::default();
            restarted = true;
        }
        if !self.initialized {
            self.view.source = plan.source;
            self.initialized = true;
            self.base_serial = plan.serial;
            self.view.applied_serial = plan.serial;
            self.base_origin = if plan.serial != 0 { tick } else { 0 };
            self.view.origin_tick = self.base_origin;
        } else if plan.serial != self.base_serial {
            if self.using_queued && plan.serial == self.view.applied_serial {
                self.base_serial = plan.serial;
                self.base_origin = self.view.origin_tick;
                self.using_queued = false;
            } else {
                self.base_serial = plan.serial;
                self.base_origin = tick;
                self.view.origin_tick = tick;
                self.view.applied_serial = plan.serial;
                self.using_queued = false;
                self.queued_serial = 0;
                restarted = true;
            }
        }
        if !running || (self.last_running && tick < self.last_tick) {
            self.base_origin = 0;
            self.view.origin_tick = 0;
            if !self.using_queued {
                self.view.switch_tick = 0;
            }
            if self.last_running {
                restarted = true;
            }
        }
        if let Some(next) = &plan.queued {
            if self.queued_serial != next.serial {
                if self.using_queued {
                    self.using_queued = false;
                    self.view.origin_tick = self.base_origin;
                    self.view.applied_serial = self.base_serial;
                    restarted = true;
                }
                self.queued_serial = next.serial;
                self.view.pending_serial = next.serial;
                self.view.switch_tick = 0;
            }
            if !self.using_queued && self.queue_length != length {
                self.queue_length = length;
                self.view.switch_tick = 0;
            }
            if !self.using_queued && running && self.view.switch_tick == 0 {
                self.view.switch_tick = if length > 0 {
                    self.base_origin
                        + (tick.saturating_sub(self.base_origin) / length as u64 + 1)
                            * length as u64
                } else {
                    tick
                };
            }
            if !self.using_queued && running && tick >= self.view.switch_tick {
                self.using_queued = true;
                self.view.applied_serial = next.serial;
                self.view.origin_tick = if running { self.view.switch_tick } else { 0 };
                self.view.pending_serial = 0;
                restarted = true;
            }
        } else {
            if self.using_queued {
                self.using_queued = false;
                self.view.origin_tick = self.base_origin;
                self.view.applied_serial = self.base_serial;
                restarted = true;
            }
            self.queued_serial = 0;
            self.view.pending_serial = 0;
        }
        self.last_tick = tick;
        self.last_running = running;
        Selection {
            queued: self.using_queued,
            elapsed_ticks: tick.saturating_sub(self.view.origin_tick),
            restarted,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn plan(serial: u64, queued: Option<u64>) -> PhrasePlan {
        PhrasePlan {
            source: 1,
            serial,
            queued: queued.map(|serial| PreparedPhrase {
                serial,
                revision: 77,
                length: 5760,
                schedule: Arc::new(Vec::new()),
            }),
        }
    }
    #[test]
    fn next_boundary_and_ui_commit_do_not_restart_the_new_phrase() {
        let mut state = PhraseState::default();
        state.select(&plan(0, None), 1000, 3840, true);
        let p = plan(0, Some(91));
        assert!(!state.select(&p, 1001, 3840, true).queued);
        assert_eq!(state.view().switch_tick, 3840);
        assert!(!state.select(&p, 3839, 3840, true).queued);
        let switched = state.select(&p, 3840, 3840, true);
        assert!(switched.queued && switched.restarted);
        assert_eq!(switched.elapsed_ticks, 0);
        let ack = state.select(&plan(91, None), 3850, 5760, true);
        assert!(!ack.queued && !ack.restarted);
        assert_eq!(ack.elapsed_ticks, 10);
        assert_eq!(
            state.select(&plan(91, None), 0, 5760, false).elapsed_ticks,
            0
        );
    }
    #[test]
    fn stopped_queue_waits_for_resumed_loop_and_cancel_is_safe() {
        let mut state = PhraseState::default();
        assert!(!state.select(&plan(0, Some(7)), 0, 3840, false).queued);
        assert_eq!(state.view().pending_serial, 7);
        assert_eq!(state.view().applied_serial, 0);
        assert!(!state.select(&plan(0, Some(7)), 0, 3840, true).queued);
        assert_eq!(state.view().switch_tick, 3840);
        assert!(!state.select(&plan(0, None), 1, 3840, true).queued);
        assert_eq!(state.view().applied_serial, 0);
    }
}
