use crate::config::delay_configs::TrackDelayConfigs;
use crate::config::roll_configs::RollConfigs;
use crate::config::track_filter_configs::TrackFilterConfigs;

pub const TRACK_FX_BANK_COUNT: usize = 4;
pub const TRACK_FX_SLOT_COUNT: usize = 4;

pub enum TrackFx {
    Vocoder(super::vocoder_configs::VocoderConfigs),
    Audio(super::audio_fx::AudioFxConfig),
    Delay(TrackDelayConfigs),
    Roll(RollConfigs),
    Filter(TrackFilterConfigs),
}

#[derive(Clone, Copy, PartialEq)]
pub enum TrackFxKind {
    Vocoder,
    Audio(super::audio_fx::AudioFxKind),
    None,
    Delay,
    Roll,
    Filter,
}
impl TrackFxKind {
    pub fn ui_tag(self) -> usize {
        match self {
            Self::None => 0,
            Self::Delay => 1,
            Self::Roll => 2,
            Self::Filter => 3,
            Self::Vocoder => 4,
            Self::Audio(kind) => 32 + kind as usize,
        }
    }
    pub fn available() -> Vec<Self> {
        let mut kinds = vec![
            Self::None,
            Self::Delay,
            Self::Roll,
            Self::Filter,
            Self::Vocoder,
        ];
        kinds.extend(
            super::audio_fx::AudioFxKind::ALL
                .into_iter()
                .filter(|k| *k != super::audio_fx::AudioFxKind::Delay)
                .map(Self::Audio),
        );
        kinds
    }
}

pub struct TrackFxSlot {
    pub fx: Option<TrackFx>,
}

impl TrackFxSlot {
    pub fn new() -> Self {
        Self { fx: None }
    }

    pub fn set_kind(&mut self, kind: TrackFxKind) {
        self.fx = match kind {
            TrackFxKind::Vocoder => Some(TrackFx::Vocoder(
                super::vocoder_configs::VocoderConfigs::new(),
            )),
            TrackFxKind::Audio(kind) => {
                Some(TrackFx::Audio(super::audio_fx::AudioFxConfig::new(kind)))
            }
            TrackFxKind::None => None,
            TrackFxKind::Delay => Some(TrackFx::Delay(TrackDelayConfigs::new())),
            TrackFxKind::Roll => Some(TrackFx::Roll(RollConfigs::new())),
            TrackFxKind::Filter => Some(TrackFx::Filter(TrackFilterConfigs::new())),
        };
    }

    pub fn kind(&self) -> TrackFxKind {
        match self.fx {
            Some(TrackFx::Vocoder(_)) => TrackFxKind::Vocoder,
            Some(TrackFx::Audio(ref fx)) => TrackFxKind::Audio(fx.kind),
            None => TrackFxKind::None,
            Some(TrackFx::Delay(_)) => TrackFxKind::Delay,
            Some(TrackFx::Roll(_)) => TrackFxKind::Roll,
            Some(TrackFx::Filter(_)) => TrackFxKind::Filter,
        }
    }
}

pub struct TrackFxBank {
    pub slots: [TrackFxSlot; TRACK_FX_SLOT_COUNT],
}

impl TrackFxBank {
    pub fn new_with_preset(_bank_idx: usize) -> Self {
        Self {
            // Keep initial mapping empty like InputFx. User binds per bank-slot in FxSelect.
            slots: std::array::from_fn(|_| TrackFxSlot::new()),
        }
    }
}

pub struct TrackFxTrackState {
    pub enabled: [[bool; TRACK_FX_SLOT_COUNT]; TRACK_FX_BANK_COUNT],
}

impl TrackFxTrackState {
    pub fn new() -> Self {
        Self {
            enabled: [[false; TRACK_FX_SLOT_COUNT]; TRACK_FX_BANK_COUNT],
        }
    }
}

pub struct TrackFxConfigs {
    pub banks: Vec<TrackFxBank>,
    pub tracks: Vec<TrackFxTrackState>,
    pub sel_bank_idx: usize,
}

impl TrackFxConfigs {
    pub fn new(track_count: usize) -> Self {
        let safe_count = track_count.max(1);
        Self {
            banks: (0..TRACK_FX_BANK_COUNT)
                .map(TrackFxBank::new_with_preset)
                .collect(),
            tracks: (0..safe_count).map(|_| TrackFxTrackState::new()).collect(),
            sel_bank_idx: 0,
        }
    }

    pub fn select_bank(&mut self, idx: usize) {
        if idx < TRACK_FX_BANK_COUNT {
            self.sel_bank_idx = idx;
        }
    }

    pub fn slot_enabled(&self, track_idx: usize, bank_idx: usize, slot_idx: usize) -> bool {
        if bank_idx >= TRACK_FX_BANK_COUNT || slot_idx >= TRACK_FX_SLOT_COUNT {
            return false;
        }
        self.tracks
            .get(track_idx)
            .map(|track| track.enabled[bank_idx][slot_idx])
            .unwrap_or(false)
    }

    pub fn toggle_slot_enabled(&mut self, track_idx: usize, slot_idx: usize) {
        if slot_idx >= TRACK_FX_SLOT_COUNT {
            return;
        }
        if let Some(track) = self.tracks.get_mut(track_idx) {
            let enabled = &mut track.enabled[self.sel_bank_idx][slot_idx];
            *enabled = !*enabled;
        }
    }

    pub fn slot_kind(&self, bank_idx: usize, slot_idx: usize) -> TrackFxKind {
        if bank_idx >= TRACK_FX_BANK_COUNT || slot_idx >= TRACK_FX_SLOT_COUNT {
            return TrackFxKind::None;
        }
        self.banks[bank_idx].slots[slot_idx].kind()
    }

    pub fn set_slot_kind(&mut self, bank_idx: usize, slot_idx: usize, kind: TrackFxKind) {
        if bank_idx >= TRACK_FX_BANK_COUNT || slot_idx >= TRACK_FX_SLOT_COUNT {
            return;
        }
        self.banks[bank_idx].slots[slot_idx].set_kind(kind);
    }

    pub fn cycle_slot_kind(&mut self, bank_idx: usize, slot_idx: usize, dir: i32) {
        let current = self.slot_kind(bank_idx, slot_idx);
        let kinds = TrackFxKind::available();
        let index = kinds.iter().position(|k| *k == current).unwrap_or(0);
        let next = kinds[(index as i32 + dir.signum()).rem_euclid(kinds.len() as i32) as usize];
        self.set_slot_kind(bank_idx, slot_idx, next);
    }

    pub fn slot_fx(&self, bank_idx: usize, slot_idx: usize) -> Option<&TrackFx> {
        if bank_idx >= TRACK_FX_BANK_COUNT || slot_idx >= TRACK_FX_SLOT_COUNT {
            return None;
        }
        self.banks[bank_idx].slots[slot_idx].fx.as_ref()
    }

    pub fn slot_fx_mut(&mut self, bank_idx: usize, slot_idx: usize) -> Option<&mut TrackFx> {
        if bank_idx >= TRACK_FX_BANK_COUNT || slot_idx >= TRACK_FX_SLOT_COUNT {
            return None;
        }
        self.banks[bank_idx].slots[slot_idx].fx.as_mut()
    }
}
