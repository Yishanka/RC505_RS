use crate::config::config_type::EnumConfig;

#[derive(Clone, Copy, PartialEq)]
pub enum RollStep {
    Two,
    Four,
    Eight,
}

impl RollStep {
    pub fn value(self) -> usize {
        match self {
            RollStep::Two => 2,
            RollStep::Four => 4,
            RollStep::Eight => 8,
        }
    }
}

impl std::fmt::Display for RollStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value())
    }
}

pub struct TrackRollConfigs {
    pub step: EnumConfig<RollStep>,
}

impl TrackRollConfigs {
    pub fn new() -> Self {
        Self {
            step: EnumConfig::new(
                "Step",
                RollStep::Four,
                vec![RollStep::Two, RollStep::Four, RollStep::Eight],
            ),
        }
    }
}
