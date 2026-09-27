use super::time_mode::TimeMode;
use crate::config::config_type::{EnumConfig, NumericConfig};

#[derive(Clone, Copy, PartialEq)]
pub enum RollStep {
    Off,
    Two,
    Four,
    Eight,
    Sixteen,
}

impl RollStep {
    pub fn value(self) -> usize {
        match self {
            RollStep::Off => 1,
            RollStep::Two => 2,
            RollStep::Four => 4,
            RollStep::Eight => 8,
            RollStep::Sixteen => 16,
        }
    }
}

impl std::fmt::Display for RollStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if *self == Self::Off {
            f.write_str("Off")
        } else {
            write!(f, "1/{}", self.value())
        }
    }
}

pub struct RollConfigs {
    pub step: EnumConfig<RollStep>,
    pub time_mode: EnumConfig<TimeMode>,
    pub time_ms: NumericConfig,
    pub mode: EnumConfig<RollMode>,
    pub feedback: NumericConfig,
    pub repeat: NumericConfig,
    pub mix: NumericConfig,
}

impl RollConfigs {
    pub fn new() -> Self {
        Self {
            time_mode: EnumConfig::new("Base cycle", TimeMode::Quarter, TimeMode::ALL.to_vec()),
            time_ms: NumericConfig::new("Time(ms)", 200),
            mode: EnumConfig::new(
                "Mode",
                RollMode::Roll2,
                vec![RollMode::Roll1, RollMode::Roll2],
            ),
            feedback: NumericConfig::new("Feedback(%)", 50),
            repeat: NumericConfig::new("Repeat (0 = infinite)", 0),
            mix: NumericConfig::new("Balance(%)", 100),
            step: EnumConfig::new(
                "Step",
                RollStep::Four,
                vec![
                    RollStep::Off,
                    RollStep::Two,
                    RollStep::Four,
                    RollStep::Eight,
                    RollStep::Sixteen,
                ],
            ),
        }
    }
}

#[derive(Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum RollMode {
    Roll1,
    #[default]
    Roll2,
}
impl std::fmt::Display for RollMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Roll1 => "Roll 1 / feedback",
            Self::Roll2 => "Roll 2 / repeat",
        })
    }
}
