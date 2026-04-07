// src/config/envelope_configs.rs

use crate::config::config_type::NumericConfig;

pub const ENVELOPE_ATTACK_MAX_MS: usize = 2000;
pub const ENVELOPE_HOLD_MAX_MS: usize = 5000;
pub const ENVELOPE_DECAY_MAX_MS: usize = 10000;
pub const ENVELOPE_SUSTAIN_MAX_PCT: usize = 100;
pub const ENVELOPE_START_MAX_PCT: usize = 100;
pub const ENVELOPE_TENSION_MAX: usize = 1000;
pub const ENVELOPE_RELEASE_MIN_MS: usize = 1;
pub const ENVELOPE_RELEASE_MAX_MS: usize = 5000;

pub struct EnvelopeConfigs {
    pub sel_idx: Option<usize>,
    pub attack_ms: NumericConfig,
    pub hold_ms: NumericConfig,
    pub decay_ms: NumericConfig,
    pub sustain_pct: NumericConfig,
    pub release_ms: NumericConfig,
    pub start_pct: NumericConfig,
    pub tension_a: NumericConfig,
    pub tension_d: NumericConfig,
    pub tension_r: NumericConfig,
}

impl EnvelopeConfigs {
    pub fn new() -> Self {
        Self {
            sel_idx: None,
            attack_ms: NumericConfig::new("Attack(ms)", 0),
            hold_ms: NumericConfig::new("Hold(ms)", 0),
            decay_ms: NumericConfig::new("Decay(ms)", 1000),
            sustain_pct: NumericConfig::new("Sustain(%)", 100),
            release_ms: NumericConfig::new("Release(ms)", 0),
            start_pct: NumericConfig::new("Start(%)", 0),
            tension_a: NumericConfig::new("Tension-A", 100),
            tension_d: NumericConfig::new("Tension-D", 100),
            tension_r: NumericConfig::new("Tension-R", 100),
        }
    }
}

impl crate::config::config_type::ConfigSet for EnvelopeConfigs {
    fn next(&mut self) {
        let curr = self.sel_idx.unwrap_or(0);
        self.sel_idx = Some((curr + 1).min(8));
    }

    fn prev(&mut self) {
        let curr = self.sel_idx.unwrap_or(0);
        self.sel_idx = Some(curr.saturating_sub(1));
    }

    fn confirm(&mut self) {}
}
