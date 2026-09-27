// src/config/app_config.rs

use crate::config::{BeatConfigs, InputFxConfigs, SystemConfigs, TrackFxConfigs};

pub struct AppConfig {
    pub fader_speed_db: f32,
    pub track_levels: Vec<f32>,
    pub beat_config: BeatConfigs,
    pub system_config: SystemConfigs,
    pub input_fx: InputFxConfigs,
    pub track_fx: TrackFxConfigs,
}

impl AppConfig {
    pub fn new(bpm: usize, latency_comp: usize, track_count: usize) -> Self {
        Self {
            fader_speed_db: 24.0,
            track_levels: vec![1.0; track_count],
            beat_config: BeatConfigs::new(bpm, latency_comp),
            system_config: SystemConfigs::new(),
            input_fx: InputFxConfigs::new(),
            track_fx: TrackFxConfigs::new(track_count),
        }
    }
}
