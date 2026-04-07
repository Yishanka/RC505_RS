use crate::config::delay_configs::TrackDelayConfigs;
use crate::config::track_filter_configs::TrackFilterConfigs;
use crate::config::roll_configs::RollConfigs;

pub const TRACK_FX_BANK_COUNT: usize = 4;
pub const TRACK_FX_SLOT_COUNT: usize = 4;

pub enum TrackFx {
    Delay(TrackDelayConfigs),
    Roll(RollConfigs),
    Filter(TrackFilterConfigs),
}

#[derive(Clone, Copy, PartialEq)]
pub enum TrackFxKind {
    None,
    Delay,
    Roll,
    Filter,
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
            TrackFxKind::None => None,
            TrackFxKind::Delay => Some(TrackFx::Delay(TrackDelayConfigs::new())),
            TrackFxKind::Roll => Some(TrackFx::Roll(RollConfigs::new())),
            TrackFxKind::Filter => Some(TrackFx::Filter(TrackFilterConfigs::new())),
        };
    }

    pub fn kind(&self) -> TrackFxKind {
        match self.fx {
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

pub struct TrackFxConfig {
    pub banks: [TrackFxBank; TRACK_FX_BANK_COUNT],
    pub tracks: Vec<TrackFxTrackState>,
    pub sel_bank_idx: usize,
}

impl TrackFxConfig {
    pub fn new(track_count: usize) -> Self {
        let safe_count = track_count.max(1);
        Self {
            banks: std::array::from_fn(TrackFxBank::new_with_preset),
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
        let next = match (current, dir.signum()) {
            (TrackFxKind::Delay, 1) => TrackFxKind::Roll,
            (TrackFxKind::Roll, 1) => TrackFxKind::Filter,
            (TrackFxKind::Filter, 1) => TrackFxKind::None,
            (TrackFxKind::None, 1) => TrackFxKind::Delay,
            (TrackFxKind::Delay, -1) => TrackFxKind::None,
            (TrackFxKind::Roll, -1) => TrackFxKind::Delay,
            (TrackFxKind::Filter, -1) => TrackFxKind::Roll,
            (TrackFxKind::None, -1) => TrackFxKind::Filter,
            (_, _) => current,
        };
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
