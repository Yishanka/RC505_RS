// src/config/app_config.rs

use crate::config::{BeatConfigs, InputFxConfigs, SystemConfigs, TrackFxConfigs};

pub struct AppConfig {
    pub beat_config: BeatConfigs,
    pub system_config: SystemConfigs,
    pub input_fx: InputFxConfigs,
    pub track_fx: TrackFxConfigs,
}

impl AppConfig {
    pub fn new(
        bpm: usize,
        latency_comp: usize,
        track_count: usize,
    ) -> Self {
        Self { 
            beat_config: BeatConfigs::new(
                bpm, 
                latency_comp), 
            system_config: SystemConfigs::new(),
            input_fx: InputFxConfigs::new(),
            track_fx: TrackFxConfigs::new(track_count),
         }
    }
}
