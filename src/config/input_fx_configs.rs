// src/config/input_fx_configs

use crate::config::OscillatorConfigs;
use crate::config::filter_configs::FilterConfigs;
use crate::config::mydelay_configs::MyDelayConfigs;
use crate::config::reverb_configs::ReverbConfigs;
use crate::config::vocoder_configs::VocoderConfigs;

pub const FX_BANK_COUNT: usize = 4;
pub const FX_SLOT_COUNT: usize = 4;

pub enum InputFx {
    Roll(super::roll_configs::RollConfigs),
    Audio(super::audio_fx::AudioFxConfig),
    Oscillator(OscillatorConfigs),
    Filter(FilterConfigs),
    Reverb(ReverbConfigs),
    MyDelay(MyDelayConfigs),
    Vocoder(VocoderConfigs),
}

impl InputFx {
    pub fn name(&self) -> &'static str {
        match self {
            InputFx::Roll(_) => "Roll",
            InputFx::Audio(fx) => fx.kind.name(),
            InputFx::Oscillator(_) => "Oscillator",
            InputFx::Filter(_) => "Filter",
            InputFx::Reverb(_) => "Reverb",
            InputFx::MyDelay(_) => "MyDelay",
            InputFx::Vocoder(_) => "Vocoder",
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

    pub fn as_mydelay_mut(&mut self) -> Option<&mut MyDelayConfigs> {
        match self {
            InputFx::MyDelay(delay) => Some(delay),
            _ => None,
        }
    }

    pub fn as_vocoder_mut(&mut self) -> Option<&mut VocoderConfigs> {
        match self {
            InputFx::Vocoder(vocoder) => Some(vocoder),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum FxKind {
    Roll,
    Audio(super::audio_fx::AudioFxKind),
    None,
    Oscillator,
    Filter,
    Reverb,
    MyDelay,
    Vocoder,
}
impl FxKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "Empty",
            Self::Oscillator => "OSC",
            Self::Filter => "Filter",
            Self::Reverb => "Reverb",
            Self::MyDelay => "MyDelay",
            Self::Vocoder => "Vocoder",
            Self::Roll => "Roll",
            Self::Audio(kind) => kind.name(),
        }
    }
    pub fn ui_tag(self) -> usize {
        match self {
            Self::None => 0,
            Self::Oscillator => 1,
            Self::Filter => 2,
            Self::Reverb => 3,
            Self::MyDelay => 4,
            Self::Vocoder => 5,
            Self::Roll => 6,
            Self::Audio(kind) => 32 + kind as usize,
        }
    }
    pub fn available() -> Vec<Self> {
        let mut kinds = vec![
            Self::None,
            Self::Oscillator,
            Self::Filter,
            Self::Reverb,
            Self::Vocoder,
            Self::Roll,
        ];
        kinds.extend(
            super::audio_fx::AudioFxKind::ALL
                .into_iter()
                .filter(|k| *k != super::audio_fx::AudioFxKind::Reverb)
                .map(Self::Audio),
        );
        kinds[1..].sort_by_key(|kind| kind.name().to_ascii_lowercase());
        kinds
    }
}

pub struct FxSlot {
    pub clip_link: Option<String>,
    pub source_id: String,
    pub clip: Option<crate::config::sequence_edit::NoteClip>,
    pub fx: Option<InputFx>,
    pub is_enabled: bool,
}

impl FxSlot {
    pub fn new() -> Self {
        Self {
            clip_link: None,
            source_id: new_source_id(),
            clip: None,
            fx: None,
            is_enabled: false,
        }
    }

    pub fn kind(&self) -> FxKind {
        match self.fx {
            Some(InputFx::Roll(_)) => FxKind::Roll,
            Some(InputFx::Audio(ref fx)) => FxKind::Audio(fx.kind),
            None => FxKind::None,
            Some(InputFx::Oscillator(_)) => FxKind::Oscillator,
            Some(InputFx::Filter(_)) => FxKind::Filter,
            Some(InputFx::Reverb(_)) => FxKind::Reverb,
            Some(InputFx::MyDelay(_)) => FxKind::MyDelay,
            Some(InputFx::Vocoder(_)) => FxKind::Vocoder,
        }
    }

    pub fn set_kind(&mut self, kind: FxKind) {
        if let Some(InputFx::Oscillator(osc)) = &self.fx {
            self.clip = Some(osc.note.clip());
        }
        if let Some(InputFx::MyDelay(osc)) = &self.fx {
            self.clip = Some(osc.note.clip());
        }
        self.fx = match kind {
            FxKind::Roll => Some(InputFx::Roll(super::roll_configs::RollConfigs::new())),
            FxKind::Audio(kind) => Some(InputFx::Audio(super::audio_fx::AudioFxConfig::new(kind))),
            FxKind::None => None,
            FxKind::Oscillator => Some(InputFx::Oscillator(OscillatorConfigs::new())),
            FxKind::Filter => Some(InputFx::Filter(FilterConfigs::new())),
            FxKind::Reverb => Some(InputFx::Reverb(ReverbConfigs::new())),
            FxKind::MyDelay => Some(InputFx::MyDelay(MyDelayConfigs::new())),
            FxKind::Vocoder => Some(InputFx::Vocoder(VocoderConfigs::new())),
        };
        if let (Some(clip), Some(InputFx::Oscillator(osc))) = (&self.clip, &mut self.fx) {
            osc.note.set_clip(clip);
        }
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

pub struct InputFxConfigs {
    // Four fixed logical banks, heap-backed so GUI/config moves stay small.
    pub banks: Vec<FxBank>,
    pub sel_bank_idx: usize,
}

impl InputFxConfigs {
    pub fn new() -> Self {
        Self {
            banks: (0..FX_BANK_COUNT).map(|_| FxBank::new()).collect(),
            sel_bank_idx: 0,
        }
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
        let kinds = FxKind::available();
        let index = kinds.iter().position(|k| *k == current).unwrap_or(0);
        let next = kinds[(index as i32 + dir.signum()).rem_euclid(kinds.len() as i32) as usize];
        self.set_slot_kind(bank_idx, slot_idx, next);
    }
}

fn new_source_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(1);
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |v| v.as_nanos());
    format!(
        "src-{time:x}-{:x}",
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}
