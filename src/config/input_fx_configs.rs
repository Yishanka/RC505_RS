// src/config/input_fx_configs

use crate::config::filter_configs::FilterConfigs;
use crate::config::reverb_configs::ReverbConfigs;
use crate::config::OscillatorConfigs;

pub const FX_BANK_COUNT: usize = 4;
pub const FX_SLOT_COUNT: usize = 4;

pub enum InputFx {
    Oscillator(OscillatorConfigs),
    Filter(FilterConfigs),
    Reverb(ReverbConfigs),
}

impl InputFx {
    pub fn name(&self) -> &'static str {
        match self {
            InputFx::Oscillator(_) => "Oscillator",
            InputFx::Filter(_) => "Filter",
            InputFx::Reverb(_) => "Reverb",
        }
    }

    pub fn as_osc_mut(&mut self) -> Option<&mut OscillatorConfigs> {
        match self {
            InputFx::Oscillator(osc) => Some(osc),
            _ => None,
        }
    }

    pub fn as_filter_mut(&mut self) -> Option<&mut FilterConfigs> {
        match self {
            InputFx::Filter(filter) => Some(filter),
            _ => None,
        }
    }

    pub fn as_reverb_mut(&mut self) -> Option<&mut ReverbConfigs> {
        match self {
            InputFx::Reverb(reverb) => Some(reverb),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum FxKind {
    None,
    Oscillator,
    Filter,
    Reverb,
}

pub struct FxSlot {
    pub fx: Option<InputFx>,
    pub is_enabled: bool,
}

impl FxSlot {
    pub fn new() -> Self {
        Self {
            fx: None,
            is_enabled: false,
        }
    }

    pub fn kind(&self) -> FxKind {
        match self.fx {
            None => FxKind::None,
            Some(InputFx::Oscillator(_)) => FxKind::Oscillator,
            Some(InputFx::Filter(_)) => FxKind::Filter,
            Some(InputFx::Reverb(_)) => FxKind::Reverb,
        }
    }

    pub fn set_kind(&mut self, kind: FxKind) {
        self.fx = match kind {
            FxKind::None => None,
            FxKind::Oscillator => Some(InputFx::Oscillator(OscillatorConfigs::new())),
            FxKind::Filter => Some(InputFx::Filter(FilterConfigs::new())),
            FxKind::Reverb => Some(InputFx::Reverb(ReverbConfigs::new())),
        };
    }
}

pub struct FxBank {
    pub slots: [FxSlot; FX_SLOT_COUNT],
}

impl FxBank {
    pub fn new() -> Self {
        Self {
            slots: std::array::from_fn(|_| FxSlot::new()),
        }
    }
}

pub struct InputFxConfig {
    pub banks: [FxBank; FX_BANK_COUNT],
    pub sel_bank_idx: usize,
}

impl InputFxConfig {
    pub fn new() -> Self {
        Self {
            banks: std::array::from_fn(|_| FxBank::new()),
            sel_bank_idx: 0,
        }
    }

    pub fn active_bank(&self) -> &FxBank {
        &self.banks[self.sel_bank_idx]
    }

    pub fn active_bank_mut(&mut self) -> &mut FxBank {
        &mut self.banks[self.sel_bank_idx]
    }

    pub fn select_bank(&mut self, idx: usize) {
        if idx < FX_BANK_COUNT {
            self.sel_bank_idx = idx;
        }
    }

    pub fn toggle_slot_enabled(&mut self, slot_idx: usize) {
        if slot_idx < FX_SLOT_COUNT {
            let slot: &mut FxSlot = &mut self.active_bank_mut().slots[slot_idx];
            slot.is_enabled = !slot.is_enabled;
        }
    }

    pub fn slot_kind(&self, bank_idx: usize, slot_idx: usize) -> FxKind {
        if bank_idx < FX_BANK_COUNT && slot_idx < FX_SLOT_COUNT {
            return self.banks[bank_idx].slots[slot_idx].kind();
        }
        FxKind::None
    }

    pub fn set_slot_kind(&mut self, bank_idx: usize, slot_idx: usize, kind: FxKind) {
        if bank_idx < FX_BANK_COUNT && slot_idx < FX_SLOT_COUNT {
            self.banks[bank_idx].slots[slot_idx].set_kind(kind);
        }
    }

    pub fn cycle_slot_kind(&mut self, bank_idx: usize, slot_idx: usize, dir: i32) {
        let current = self.slot_kind(bank_idx, slot_idx);
        let next = match (current, dir.signum()) {
            (FxKind::None, 1) => FxKind::Oscillator,
            (FxKind::Oscillator, 1) => FxKind::Filter,
            (FxKind::Filter, 1) => FxKind::Reverb,
            (FxKind::Reverb, 1) => FxKind::None,
            (FxKind::None, -1) => FxKind::Reverb,
            (FxKind::Reverb, -1) => FxKind::Filter,
            (FxKind::Filter, -1) => FxKind::Oscillator,
            (FxKind::Oscillator, -1) => FxKind::None,
            (_, _) => current,
        };
        self.set_slot_kind(bank_idx, slot_idx, next);
    }
}
