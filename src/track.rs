use crate::state::TrackState;
use std::time::{Duration, Instant};
#[derive(Clone)]
pub struct Track {
    pub track_state: TrackState,
    pub prev_track_state: TrackState,
    pub stop_after_finish: bool,
    pub track_record_start_at: Option<Instant>,
    pub track_loop_duration: Option<Duration>,
    pub track_play_anchor_at: Option<Instant>,
}

impl Track {
    pub fn new() -> Self {
        Self {
            track_state: TrackState::Empty,
            prev_track_state: TrackState::Empty,
            stop_after_finish: false,
            track_record_start_at: None,
            track_loop_duration: None,
            track_play_anchor_at: None,
        }
    }

    pub fn track_play_progress(&self, now: Instant) -> f32 {
        match (self.track_play_anchor_at, self.track_loop_duration) {
            (Some(anchor), Some(loop_len)) if !loop_len.is_zero() => {
                let elapsed = now.saturating_duration_since(anchor).as_secs_f64();
                let loop_secs = loop_len.as_secs_f64();
                (elapsed.rem_euclid(loop_secs) / loop_secs) as f32
            }
            _ => 0.0,
        }
    }

    pub fn trigger(&mut self) {
        // Wait for the audio engine to finish a scheduled transition before
        // accepting another overdub. Repeated taps must not cancel a stop.
        if self.track_state == TrackState::NxtPlay {
            return;
        }
        self.stop_after_finish = false;
        self.track_state = match self.track_state {
            TrackState::Empty => TrackState::Record,
            TrackState::Play => TrackState::Dub,
            TrackState::Record | TrackState::Dub => TrackState::NxtPlay,
            TrackState::Pause => TrackState::Play,
            TrackState::NxtPlay => unreachable!(),
        };
    }

    pub fn stop(&mut self) {
        match self.track_state {
            TrackState::Record => {
                self.stop_after_finish = true;
                self.track_state = TrackState::NxtPlay;
            }
            TrackState::NxtPlay => self.stop_after_finish = true,
            TrackState::Play | TrackState::Dub => self.track_state = TrackState::Pause,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stop_preserves_pending_record_finish_and_ignores_repeated_triggers() {
        let mut track = Track::new();
        track.trigger();
        assert!(track.track_state == TrackState::Record);
        track.stop();
        track.trigger();
        assert!(track.track_state == TrackState::NxtPlay && track.stop_after_finish);
        track.track_state = TrackState::Play;
        track.trigger();
        assert!(track.track_state == TrackState::Dub && !track.stop_after_finish);
        track.stop();
        assert!(track.track_state == TrackState::Pause);
    }
}
