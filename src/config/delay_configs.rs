use crate::config::config_type::NumericConfig;

pub const TRACK_DELAY_TIME_MIN_MS: usize = 1;
pub const TRACK_DELAY_TIME_MAX_MS: usize = 2_000;
pub const TRACK_DELAY_FEEDBACK_MAX_PCT: usize = 100;
pub const TRACK_DELAY_DAMP_MIN_HZ: usize = 0; // zero = FLAT
pub const TRACK_DELAY_DAMP_MAX_HZ: usize = 20_000;
pub const TRACK_DELAY_MIX_MAX_PCT: usize = 100;

pub struct TrackDelayConfigs {
    pub feedback_repeats: NumericConfig,
    pub direct_pct: NumericConfig,
    pub effect_pct: NumericConfig,
    pub low_cut_hz: NumericConfig,
    pub time_mode: super::config_type::EnumConfig<super::time_mode::TimeMode>,
    /// Milliseconds; fractional precision matters for short pitched delay loops.
    pub time_ms: f32,
    pub feedback_pct: NumericConfig,
    pub high_damp_hz: NumericConfig,
    pub mix_pct: NumericConfig,
}

impl TrackDelayConfigs {
    pub fn new() -> Self {
        Self {
            feedback_repeats: NumericConfig::new("Repeats (0 = manual feedback)", 8),
            direct_pct: NumericConfig::new("Direct level (%)", 100),
            effect_pct: NumericConfig::new("Effect level (%)", 50),
            low_cut_hz: NumericConfig::new("Feedback low cut (Hz; 0 = thru)", 0),
            time_mode: super::config_type::EnumConfig::new(
                "Time mode",
                super::time_mode::TimeMode::Milliseconds,
                super::time_mode::TimeMode::ALL.to_vec(),
            ),
            time_ms: 320.0,
            feedback_pct: NumericConfig::new("Feedback(%)", 35),
            high_damp_hz: NumericConfig::new("HighDamp(Hz)", 10_000),
            mix_pct: NumericConfig::new("Mix(%)", 40),
        }
    }
}
