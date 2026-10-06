use crate::config::config_type::{ConfigSet, EnumConfig, NumericConfig};

pub const FILTER_CUTOFF_MIN_HZ: usize = 20;
pub const FILTER_CUTOFF_MAX_HZ: usize = 20_000;
pub const FILTER_Q_MIN_X10: usize = 1; // 0.1
pub const FILTER_Q_MAX_X10: usize = 100; // 10.0
pub const FILTER_DRIVE_MAX: usize = 100;
pub const FILTER_MIX_MAX: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum FilterType {
    Lpf,
    Hpf,
    Bpf,
    Notch,
}

/// Plain, persistent filter settings for the master bus. Units and defaults are
/// shared with the input/track filter editor; no UI state crosses into DSP.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FilterSettings {
    pub filter_type: FilterType,
    pub cutoff_hz: usize,
    pub resonance_x10: usize,
    pub drive: usize,
    pub mix: usize,
}

impl Default for FilterSettings {
    fn default() -> Self {
        Self {
            filter_type: FilterType::Lpf,
            cutoff_hz: 1000,
            resonance_x10: 7,
            drive: 0,
            mix: 100,
        }
    }
}

impl FilterSettings {
    pub fn sanitized(self) -> Self {
        Self {
            cutoff_hz: self
                .cutoff_hz
                .clamp(FILTER_CUTOFF_MIN_HZ, FILTER_CUTOFF_MAX_HZ),
            resonance_x10: self.resonance_x10.clamp(FILTER_Q_MIN_X10, FILTER_Q_MAX_X10),
            drive: self.drive.min(FILTER_DRIVE_MAX),
            mix: self.mix.min(FILTER_MIX_MAX),
            ..self
        }
    }

    pub fn from_editor(c: &FilterConfigs) -> Self {
        Self {
            filter_type: c.filter_type.value,
            cutoff_hz: c.cutoff_hz.value,
            resonance_x10: c.resonance_x10.value,
            drive: c.drive.value,
            mix: c.mix.value,
        }
    }

    pub fn editor(self) -> FilterConfigs {
        let p = self.sanitized();
        let mut c = FilterConfigs::new();
        c.filter_type.value = p.filter_type;
        c.cutoff_hz.value = p.cutoff_hz;
        c.resonance_x10.value = p.resonance_x10;
        c.drive.value = p.drive;
        c.mix.value = p.mix;
        c
    }
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
        let defaults = FilterSettings::default();
        Self {
            sel_idx: None,
            filter_type: EnumConfig::new(
                "Type",
                defaults.filter_type,
                vec![
                    FilterType::Lpf,
                    FilterType::Hpf,
                    FilterType::Bpf,
                    FilterType::Notch,
                ],
            ),
            cutoff_hz: NumericConfig::new("Cutoff(Hz)", defaults.cutoff_hz),
            resonance_x10: NumericConfig::new("Q(x0.1)", defaults.resonance_x10),
            drive: NumericConfig::new("Drive(%)", defaults.drive),
            mix: NumericConfig::new("Mix(%)", defaults.mix),
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
