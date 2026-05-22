use crate::config::config_type::{ConfigSet, EnumConfig, NumericConfig};

pub const VOCODER_LEVEL_MAX: usize = 100;
pub const VOCODER_MIX_MAX: usize = 100;
pub const VOCODER_BANDS_MIN: usize = 4;
pub const VOCODER_BANDS_MAX: usize = 16;
pub const VOCODER_ATTACK_MAX_MS: usize = 200;
pub const VOCODER_RELEASE_MAX_MS: usize = 1000;

#[derive(Clone, Copy, PartialEq)]
pub enum VocoderCarrier {
    Track1,
    Track2,
    Track3,
    Track4,
    Track5,
}

impl std::fmt::Display for VocoderCarrier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            VocoderCarrier::Track1 => "Tr1",
            VocoderCarrier::Track2 => "Tr2",
            VocoderCarrier::Track3 => "Tr3",
            VocoderCarrier::Track4 => "Tr4",
            VocoderCarrier::Track5 => "Tr5",
        };
        write!(f, "{label}")
    }
}

impl VocoderCarrier {
    pub fn track_idx(self) -> Option<usize> {
        match self {
            VocoderCarrier::Track1 => Some(0),
            VocoderCarrier::Track2 => Some(1),
            VocoderCarrier::Track3 => Some(2),
            VocoderCarrier::Track4 => Some(3),
            VocoderCarrier::Track5 => Some(4),
        }
    }
}

pub struct VocoderConfigs {
    pub sel_idx: Option<usize>,
    pub carrier: EnumConfig<VocoderCarrier>,
    pub bands: NumericConfig,
    pub attack_ms: NumericConfig,
    pub release_ms: NumericConfig,
    pub level: NumericConfig,
    pub mix: NumericConfig,
}

impl VocoderConfigs {
    pub fn new() -> Self {
        Self {
            sel_idx: None,
            carrier: EnumConfig::new(
                "Carrier",
                VocoderCarrier::Track1,
                vec![
                    VocoderCarrier::Track1,
                    VocoderCarrier::Track2,
                    VocoderCarrier::Track3,
                    VocoderCarrier::Track4,
                    VocoderCarrier::Track5,
                ],
            ),
            bands: NumericConfig::new("Bands", 16),
            attack_ms: NumericConfig::new("Attack(ms)", 6),
            release_ms: NumericConfig::new("Release(ms)", 80),
            level: NumericConfig::new("Level", 100),
            mix: NumericConfig::new("Mix(%)", 100),
        }
    }
}

impl ConfigSet for VocoderConfigs {
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
