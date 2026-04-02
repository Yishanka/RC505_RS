// src/config/app_config.rs

use crate::config::{BeatConfigs, InputFxConfig, SystemConfigs, TrackFxConfig};

pub struct AppConfig {
    pub beat_config: BeatConfigs,
    pub system_config: SystemConfigs,
    pub input_fx: InputFxConfig,
    pub track_fx: TrackFxConfig,
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
            input_fx: InputFxConfig::new(),
            track_fx: TrackFxConfig::new(track_count),
         }
    }
}
