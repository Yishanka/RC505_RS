use crate::state::TrackState; 
use std::{time::{Duration, Instant}}; 
#[derive(Clone)]
pub struct Track {
    pub track_state: TrackState,
    pub prev_track_state: TrackState,
    pub track_volume: f32,
    pub track_record_start_at: Option<Instant>,
    pub track_loop_duration: Option<Duration>,
    pub track_play_anchor_at: Option<Instant>,
}

impl Track {
    pub fn new() -> Self {
        Self { 
            track_state: TrackState::Empty, 
            prev_track_state: TrackState::Empty, 
            track_volume: 1.0, 
            track_record_start_at: None, 
            track_loop_duration: None, 
            track_play_anchor_at: None
        }
    }
    
    pub fn track_play_progress(&self, now: Instant) -> f32 {
        match (
            self.track_play_anchor_at,
            self.track_loop_duration,
        ) {
            (Some(anchor), Some(loop_len)) if !loop_len.is_zero() => {
                let elapsed = now.saturating_duration_since(anchor).as_secs_f64();
                let loop_secs = loop_len.as_secs_f64();
                (elapsed.rem_euclid(loop_secs) / loop_secs) as f32
            }
            _ => 0.0,
        }
    }
}