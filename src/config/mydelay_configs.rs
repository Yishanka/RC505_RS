// src/config/mydelay_configs.rs

use crate::config::{
    config_type::{ConfigSet, NumericConfig},
    envelope_configs::EnvelopeConfigs,
    filter_configs::FilterConfigs,
    note_configs::NoteConfigs,
};

pub const MYDELAY_LEVEL_MAX: usize = 100;
pub const MYDELAY_THRESHOLD_MAX: usize = 100;

pub struct MyDelayConfigs {
    pub sel_idx: Option<usize>,
    pub audio_sel_idx: Option<usize>,
    pub filter_sel_idx: Option<usize>,
    pub level: NumericConfig,
    pub threshold: NumericConfig,
    pub note: NoteConfigs,
    pub filter: FilterConfigs,
    pub audio_env: EnvelopeConfigs,
    pub filter_env: EnvelopeConfigs,
}

impl MyDelayConfigs {
    pub fn new() -> Self {
        Self {
            sel_idx: None,
            audio_sel_idx: None,
            filter_sel_idx: None,
            level: NumericConfig::new("Level", 100),
            threshold: NumericConfig::new("Threshold", 35),
            note: NoteConfigs::new(),
            filter: FilterConfigs::new(),
            audio_env: EnvelopeConfigs::new(),
            filter_env: EnvelopeConfigs::new(),
        }
    }
}

impl ConfigSet for MyDelayConfigs {
    fn next(&mut self) {
        let curr = self.sel_idx.unwrap_or(0);
        self.sel_idx = Some((curr + 1).min(2));
    }

    fn prev(&mut self) {
        let curr = self.sel_idx.unwrap_or(0);
        self.sel_idx = Some(curr.saturating_sub(1));
    }

    fn confirm(&mut self) {}
}
