use crate::config::{
    config_type::{EnumConfig, NumericConfig},
    envelope_configs::EnvelopeConfigs,
    filter_configs::FilterConfigs,
    note_configs::NoteConfigs,
};

#[derive(Clone, Copy, PartialEq)]
pub enum Waveform {
    Sine,
    Saw,
    Square,
    Triangle,
}

impl std::fmt::Display for Waveform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", match self {
            Waveform::Sine => "Sin",
            Waveform::Saw => "Saw",
            Waveform::Square => "Sqr",
            Waveform::Triangle => "Tri",
        })
    }
}

pub struct OscillatorConfigs {
    pub sel_idx: Option<usize>,
    pub audio_sel_idx: Option<usize>,
    pub osc_filter_sel_idx: Option<usize>,
    pub waveform: EnumConfig<Waveform>,
    pub level: NumericConfig,
    pub threshold: NumericConfig,
    pub note: NoteConfigs,
    pub envelope: EnvelopeConfigs,
    pub osc_filter: FilterConfigs,
    pub osc_filter_env: EnvelopeConfigs,
}

impl OscillatorConfigs {
    pub fn new() -> Self {
        Self {
            sel_idx: None,
            audio_sel_idx: None,
            osc_filter_sel_idx: None,
            waveform: EnumConfig::new(
                "Waveform",
                Waveform::Sine,
                vec![Waveform::Sine,
                    Waveform::Saw,
                    Waveform::Square,
                    Waveform::Triangle,
                ],
            ),
            level: NumericConfig::new("Level", 100),
            threshold: NumericConfig::new("Threshold", 10),
            note: NoteConfigs::new(),
            envelope: EnvelopeConfigs::new(),
            osc_filter: FilterConfigs::new(),
            osc_filter_env: EnvelopeConfigs::new(),
        }
    }
}

impl crate::config::config_type::ConfigSet for OscillatorConfigs {
    fn next(&mut self) {
        let curr = self.sel_idx.unwrap_or(0);
        self.sel_idx = Some((curr + 1).min(2));
    }

    fn prev(&mut self) {
        let curr = self.sel_idx.unwrap_or(0);
        self.sel_idx = Some(curr.saturating_sub(1));
    }

    fn confirm(&mut self) {}
}
