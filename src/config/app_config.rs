// src/config/app_config.rs

use crate::config::{BeatConfigs, InputFxConfig, SystemConfigs};

pub struct AppConfig {
    pub beat_config: BeatConfigs,
    pub system_config: SystemConfigs,
    pub input_fx: InputFxConfig,
}

impl AppConfig {
    pub fn new(
        bpm: usize,
        latency_comp: usize
    ) -> Self {
        Self { 
            beat_config: BeatConfigs::new(
                bpm, 
                latency_comp), 
            system_config: SystemConfigs::new(),
            input_fx: InputFxConfig::new(),
         }
    }
}
