// src/config/reverb_configs.rs

use crate::config::config_type::{ConfigSet, NumericConfig};

pub const REVERB_SIZE_MAX: usize = 100;
pub const REVERB_SIZE_MIN_MS: usize = 20;
pub const REVERB_SIZE_MAX_MS: usize = 120;

pub const REVERB_RT60_MIN_MS: usize = 100;
pub const REVERB_RT60_MAX_MS: usize = 12_000;

pub const REVERB_PREDELAY_MAX_MS: usize = 500;

pub const REVERB_WIDTH_MAX: usize = 100;

pub const REVERB_HIGHCUT_MAX: usize = 100; // damping percent

pub const REVERB_LOWCUT_MIN_HZ: usize = 20;
pub const REVERB_LOWCUT_MAX_HZ: usize = 1_000;

pub struct ReverbConfigs {
    pub high_cut_hz: NumericConfig,
    pub dry_level: NumericConfig,
    pub wet_level: NumericConfig,
    pub density: NumericConfig,
    pub sel_idx: Option<usize>,
    pub size: NumericConfig,
    pub decay_ms: NumericConfig,
    pub predelay_ms: NumericConfig,
    pub width: NumericConfig,
    pub high_cut: NumericConfig,
    pub low_cut: NumericConfig,
}

impl ReverbConfigs {
    pub fn new() -> Self {
        Self {
            high_cut_hz: NumericConfig::new("High cut (Hz)", 13_350),
            dry_level: NumericConfig::new("Direct level(%)", 100),
            wet_level: NumericConfig::new("Effect level(%)", 35),
            density: NumericConfig::new("Density", 5),
            sel_idx: None,
            size: NumericConfig::new("Size", 50),
            decay_ms: NumericConfig::new("Decay(ms)", 2500),
            predelay_ms: NumericConfig::new("PreDelay(ms)", 20),
            width: NumericConfig::new("Width(%)", 70),
            high_cut: NumericConfig::new("HighCut(%)", 30),
            low_cut: NumericConfig::new("LowCut(Hz)", 120),
        }
    }
}

impl ConfigSet for ReverbConfigs {
    fn next(&mut self) {
        let curr = self.sel_idx.unwrap_or(0);
        self.sel_idx = Some((curr + 1).min(5));
    }

    fn prev(&mut self) {
        let curr = self.sel_idx.unwrap_or(0);
        self.sel_idx = Some(curr.saturating_sub(1));
    }

    fn confirm(&mut self) {}
}
