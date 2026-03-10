use crate::config::config_type::{ConfigSet, EnumConfig, NumericConfig};

pub const FILTER_CUTOFF_MIN_HZ: usize = 20;
pub const FILTER_CUTOFF_MAX_HZ: usize = 20_000;
pub const FILTER_Q_MIN_X10: usize = 1; // 0.1
pub const FILTER_Q_MAX_X10: usize = 100; // 10.0
pub const FILTER_DRIVE_MAX: usize = 100;
pub const FILTER_MIX_MAX: usize = 100;

#[derive(Clone, Copy, PartialEq)]
pub enum FilterType {
    Lpf,
    Hpf,
    Bpf,
    Notch,
}

impl std::fmt::Display for FilterType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            FilterType::Lpf => "LPF",
            FilterType::Hpf => "HPF",
            FilterType::Bpf => "BPF",
            FilterType::Notch => "Notch",
        };
        write!(f, "{label}")
    }
}

pub struct FilterConfigs {
    pub sel_idx: Option<usize>,
    pub filter_type: EnumConfig<FilterType>,
    pub cutoff_hz: NumericConfig,
    pub resonance_x10: NumericConfig,
    pub drive: NumericConfig,
    pub mix: NumericConfig,
}

impl FilterConfigs {
    pub fn new() -> Self {
        Self {
            sel_idx: None,
            filter_type: EnumConfig::new(
                "Type",
                FilterType::Lpf,
                vec![FilterType::Lpf, FilterType::Hpf, FilterType::Bpf, FilterType::Notch],
            ),
            cutoff_hz: NumericConfig::new("Cutoff(Hz)", 1000),
            resonance_x10: NumericConfig::new("Q(x0.1)", 7),
            drive: NumericConfig::new("Drive(%)", 0),
            mix: NumericConfig::new("Mix(%)", 100),
        }
    }
}

impl ConfigSet for FilterConfigs {
    fn next(&mut self) {
        let curr = self.sel_idx.unwrap_or(0);
        self.sel_idx = Some((curr + 1).min(4));
    }

    fn prev(&mut self) {
        let curr = self.sel_idx.unwrap_or(0);
        self.sel_idx = Some(curr.saturating_sub(1));
    }

    fn confirm(&mut self) {}
}
