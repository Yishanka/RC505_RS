// src/config/envelope_configs.rs

use crate::config::config_type::NumericConfig;

pub const ENVELOPE_ATTACK_MAX_MS: f32 = 2000.0;
pub const ENVELOPE_HOLD_MAX_MS: f32 = 5000.0;
pub const ENVELOPE_DECAY_MAX_MS: f32 = 10000.0;
pub const ENVELOPE_SUSTAIN_MAX_PCT: usize = 100;
pub const ENVELOPE_START_MAX_PCT: usize = 100;
pub const ENVELOPE_TENSION_MAX: usize = 1000;
pub const ENVELOPE_RELEASE_MIN_MS: f32 = 1.0;
pub const ENVELOPE_RELEASE_MAX_MS: f32 = 5000.0;

/// Only time values are fractional. Level and curvature keep their existing units.
pub struct EnvelopeTime {
    pub label: &'static str,
    pub value: f32,
}
impl EnvelopeTime {
    fn new(label: &'static str, value: f32) -> Self {
        Self { label, value }
    }
    pub fn bounded(&self, min: f32, max: f32) -> f32 {
        clamp_time(self.value, min, max)
    }
}
pub fn clamp_time(value: f32, min: f32, max: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        min
    }
}
pub struct EnvelopeConfigs {
    pub sel_idx: Option<usize>,
    pub attack_ms: EnvelopeTime,
    pub hold_ms: EnvelopeTime,
    pub decay_ms: EnvelopeTime,
    pub sustain_pct: NumericConfig,
    pub release_ms: EnvelopeTime,
    pub start_pct: NumericConfig,
    pub tension_a: NumericConfig,
    pub tension_d: NumericConfig,
    pub tension_r: NumericConfig,
}

impl EnvelopeConfigs {
    pub fn sanitize_times(&mut self) {
        self.attack_ms.value = self.attack_ms.bounded(0.0, ENVELOPE_ATTACK_MAX_MS);
        self.hold_ms.value = self.hold_ms.bounded(0.0, ENVELOPE_HOLD_MAX_MS);
        self.decay_ms.value = self.decay_ms.bounded(0.0, ENVELOPE_DECAY_MAX_MS);
        self.release_ms.value = self
            .release_ms
            .bounded(ENVELOPE_RELEASE_MIN_MS, ENVELOPE_RELEASE_MAX_MS);
    }
    pub fn new() -> Self {
        Self {
            sel_idx: None,
            attack_ms: EnvelopeTime::new("Attack(ms)", 5.0),
            hold_ms: EnvelopeTime::new("Hold(ms)", 0.0),
            decay_ms: EnvelopeTime::new("Decay(ms)", 150.0),
            sustain_pct: NumericConfig::new("Sustain(%)", 75),
            release_ms: EnvelopeTime::new("Release(ms)", 100.0),
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
