use crate::config::config_type::NumericConfig;

pub const TRACK_DELAY_TIME_MIN_MS: usize = 20;
pub const TRACK_DELAY_TIME_MAX_MS: usize = 1_500;
pub const TRACK_DELAY_FEEDBACK_MAX_PCT: usize = 95;
pub const TRACK_DELAY_DAMP_MIN_HZ: usize = 200;
pub const TRACK_DELAY_DAMP_MAX_HZ: usize = 20_000;
pub const TRACK_DELAY_MIX_MAX_PCT: usize = 100;

pub struct TrackDelayConfigs {
    pub time_ms: NumericConfig,
    pub feedback_pct: NumericConfig,
    pub high_damp_hz: NumericConfig,
    pub mix_pct: NumericConfig,
}

impl TrackDelayConfigs {
    pub fn new() -> Self {
        Self {
            time_ms: NumericConfig::new("Time(ms)", 320),
            feedback_pct: NumericConfig::new("Feedback(%)", 35),
            high_damp_hz: NumericConfig::new("HighDamp(Hz)", 10_000),
            mix_pct: NumericConfig::new("Mix(%)", 40),
        }
    }
}
