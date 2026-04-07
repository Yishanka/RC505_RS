use crate::config::envelope_configs::EnvelopeConfigs;
use crate::config::filter_configs::FilterConfigs;
use crate::config::seq_configs::SeqConfigs;

pub struct TrackFilterConfigs {
    pub sel_idx: Option<usize>,
    pub filter: FilterConfigs,
    pub seq: SeqConfigs,
    pub env: EnvelopeConfigs,
}

impl TrackFilterConfigs {
    pub fn new() -> Self {
        Self {
            sel_idx: None,
            filter: FilterConfigs::new(),
            seq: SeqConfigs::new(),
            env: EnvelopeConfigs::new(),
        }
    }
}

impl crate::config::config_type::ConfigSet for TrackFilterConfigs {
    fn next(&mut self) {
        let curr = self.sel_idx.unwrap_or(0);
        self.sel_idx = Some((curr + 1).min(6));
    }

    fn prev(&mut self) {
        let curr = self.sel_idx.unwrap_or(0);
        self.sel_idx = Some(curr.saturating_sub(1));
    }

    fn confirm(&mut self) {}
}
