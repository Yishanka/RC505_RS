use std::time::Instant;

use crate::config::delay_configs::{
    TRACK_DELAY_DAMP_MAX_HZ, TRACK_DELAY_DAMP_MIN_HZ, TRACK_DELAY_FEEDBACK_MAX_PCT,
    TRACK_DELAY_MIX_MAX_PCT, TRACK_DELAY_TIME_MAX_MS, TRACK_DELAY_TIME_MIN_MS,
};
use crate::config::envelope_configs::{
    ENVELOPE_ATTACK_MAX_MS, ENVELOPE_DECAY_MAX_MS, ENVELOPE_HOLD_MAX_MS, ENVELOPE_RELEASE_MAX_MS,
    ENVELOPE_RELEASE_MIN_MS, ENVELOPE_START_MAX_PCT, ENVELOPE_SUSTAIN_MAX_PCT,
    ENVELOPE_TENSION_MAX,
};
use crate::config::filter_configs::{
    FILTER_CUTOFF_MAX_HZ, FILTER_CUTOFF_MIN_HZ, FILTER_DRIVE_MAX, FILTER_MIX_MAX, FILTER_Q_MAX_X10,
    FILTER_Q_MIN_X10, FilterType,
};
use crate::config::track_fx_configs::{
    TRACK_FX_BANK_COUNT, TRACK_FX_SLOT_COUNT, TrackFx, TrackFxConfigs,
};
use crate::dsp::delay::{DelayDspState, DelayParams, process_sample as process_delay_sample};
use crate::dsp::envelope::{AhdsrParams, AhdsrState};
use crate::dsp::filter::{FilterDspState, FilterParams, process_sample as process_filter_sample};
use crate::dsp::note::{StepTrigger, seq_bool_at_time};
use crate::dsp::roll::{RollDspState, RollParams, process_frame as process_roll_frame};

const DEFAULT_BPM: usize = 120;

#[derive(Clone, Copy)]
pub struct DelayRuntime {
    pub time_mode: crate::config::time_mode::TimeMode,
    pub time_ms: f32,
    pub feedback: f32,
    pub high_damp_hz: f32,
    pub direct: f32,
    pub effect: f32,
    pub low_cut_hz: f32,
}

#[derive(Clone, Copy)]
pub struct RollRuntime {
    pub step: usize,
    pub time_mode: crate::config::time_mode::TimeMode,
    pub time_ms: usize,
    pub mode: crate::config::roll_configs::RollMode,
    pub feedback: f32,
    pub repeat: usize,
    pub mix: f32,
}

#[derive(Clone)]
pub struct TrackFilterRuntime {
    pub filter_type: FilterType,
    pub cutoff_hz: f32,
    pub q: f32,
    pub drive: f32,
    pub mix: f32,
    pub envelope: AhdsrParams,
    pub seq: Vec<bool>,
    pub trigger_seq: Vec<bool>,
}

#[derive(Clone)]
pub struct TrackFxSlotRuntime {
    pub delay: Option<DelayRuntime>,
    pub roll: Option<RollRuntime>,
    pub filter: Option<TrackFilterRuntime>,
}

#[derive(Clone)]
pub struct TrackFxBankRuntime {
    pub slots: [TrackFxSlotRuntime; TRACK_FX_SLOT_COUNT],
}

#[derive(Clone)]
pub struct TrackFxRuntime {
    pub banks: [TrackFxBankRuntime; TRACK_FX_BANK_COUNT],
    pub track_enabled: Vec<[[bool; TRACK_FX_SLOT_COUNT]; TRACK_FX_BANK_COUNT]>,
    pub selected_bank_idx: usize,
}

#[derive(Clone)]
pub struct TrackFxSlotState {
    pub delay: DelayDspState,
    pub roll: RollDspState,
    pub filter: TrackFilterDspState,
}

#[derive(Clone)]
pub struct TrackFxBankState {
    pub slots: [TrackFxSlotState; TRACK_FX_SLOT_COUNT],
}

#[derive(Clone)]
pub struct TrackFxTrackState {
    pub banks: [TrackFxBankState; TRACK_FX_BANK_COUNT],
}

#[derive(Clone)]
pub struct TrackFxState {
    pub tracks: Vec<TrackFxTrackState>,
}

pub struct TrackFxEngine {
    runtime: TrackFxRuntime,
    state: TrackFxState,
    metronome_start: Option<Instant>,
    clock_active: bool,
    bpm: usize,
    sample_rate: f32,
}

#[derive(Clone, Copy)]
pub struct TrackFilterDspState {
    pub trigger: StepTrigger,
    pub env: AhdsrState,
    pub filter_l: FilterDspState,
    pub filter_r: FilterDspState,
}

impl TrackFilterDspState {
    pub fn new() -> Self {
        Self {
            trigger: StepTrigger::default(),
            env: AhdsrState::new(),
            filter_l: FilterDspState::new(),
            filter_r: FilterDspState::new(),
        }
    }
}

impl TrackFxEngine {
    pub fn new(sample_rate: f32, track_count: usize) -> Self {
        let sr = sample_rate.max(1.0);
        Self {
            runtime: TrackFxRuntime::empty(track_count),
            state: TrackFxState::new(track_count, sr),
            metronome_start: None,
            clock_active: false,
            bpm: DEFAULT_BPM,
            sample_rate: sr,
        }
    }

    pub fn prepare(&mut self) {
        for track in &mut self.state.tracks {
            for bank in &mut track.banks {
                for slot in &mut bank.slots {
                    slot.roll.prepare(self.sample_rate);
                }
            }
        }
    }
    pub fn set_clock(&mut self, bpm: usize, active: bool) {
        if active != self.clock_active {
            for track in &mut self.state.tracks {
                for bank in &mut track.banks {
                    for slot in &mut bank.slots {
                        slot.filter.trigger = StepTrigger::default();
                    }
                }
            }
        }
        self.bpm = bpm;
        self.clock_active = active;
    }
    pub fn exchange_runtime(&mut self, runtime: &mut TrackFxRuntime) {
        if runtime.selected_bank_idx != self.runtime.selected_bank_idx {
            for track in &mut self.state.tracks {
                for bank in &mut track.banks {
                    for slot in &mut bank.slots {
                        slot.roll.reset();
                    }
                }
            }
        }
        for track in &mut self.state.tracks {
            for (bank_index, bank) in track.banks.iter_mut().enumerate() {
                for (slot_index, state) in bank.slots.iter_mut().enumerate() {
                    let old = &self.runtime.banks[bank_index].slots[slot_index];
                    let new = &runtime.banks[bank_index].slots[slot_index];
                    if old.roll.is_some() != new.roll.is_some() {
                        state.roll.reset();
                    }
                    if old.delay.is_some() != new.delay.is_some() {
                        state.delay.reset();
                    }
                    if old.filter.is_some() != new.filter.is_some() {
                        state.filter = TrackFilterDspState::new();
                    }
                }
            }
        }
        std::mem::swap(&mut self.runtime, runtime);
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
        for track in &mut self.state.tracks {
            for (bank_idx, bank) in track.banks.iter_mut().enumerate() {
                for (slot_idx, slot) in bank.slots.iter_mut().enumerate() {
                    slot.delay.set_sample_rate(self.sample_rate);
                    if self.runtime.banks[bank_idx].slots[slot_idx].roll.is_some() {
                        slot.roll.prepare(self.sample_rate);
                    }
                }
            }
        }
    }

    pub fn update_metronome(&mut self, start: Option<Instant>, bpm: usize) {
        self.clock_active = start.is_some();
        self.metronome_start = start;
        self.bpm = bpm.max(1);
    }

    pub fn swap_runtime(&mut self, runtime: TrackFxRuntime) -> TrackFxRuntime {
        if self.state.tracks.len() != runtime.track_enabled.len() {
            self.state = TrackFxState::new(runtime.track_enabled.len(), self.sample_rate);
        }
        for track in &mut self.state.tracks {
            for (bank_index, bank) in track.banks.iter_mut().enumerate() {
                for (slot_index, slot) in bank.slots.iter_mut().enumerate() {
                    if runtime.banks[bank_index].slots[slot_index].roll.is_some() {
                        slot.roll.prepare(self.sample_rate);
                        if runtime.selected_bank_idx != self.runtime.selected_bank_idx
                            || self.runtime.banks[bank_index].slots[slot_index]
                                .roll
                                .is_none()
                        {
                            slot.roll.reset();
                        }
                    }
                }
            }
        }
        std::mem::replace(&mut self.runtime, runtime)
    }

    /// Called by the control thread after a recording is cleared. No previous
    /// phrase may survive as a frozen Roll slice or a delayed tail.
    pub fn reset_track(&mut self, index: usize) {
        if let Some(track) = self.state.tracks.get_mut(index) {
            for bank in &mut track.banks {
                for slot in &mut bank.slots {
                    slot.roll.reset();
                    slot.delay.reset();
                    slot.filter = TrackFilterDspState::new();
                }
            }
        }
    }

    pub fn reset_all_tracks(&mut self) {
        for index in 0..self.state.tracks.len() {
            self.reset_track(index);
        }
    }

    pub fn process_frame(
        &mut self,
        track_idx: usize,
        elapsed_secs: f64,
        input_l: f32,
        input_r: f32,
    ) -> (f32, f32) {
        let Some(track_enabled) = self.runtime.track_enabled.get(track_idx) else {
            return (input_l, input_r);
        };
        let Some(track_state) = self.state.tracks.get_mut(track_idx) else {
            return (input_l, input_r);
        };
        let bank_idx = self.runtime.selected_bank_idx.min(TRACK_FX_BANK_COUNT - 1);
        let bank_runtime = &self.runtime.banks[bank_idx];
        let bank_state = &mut track_state.banks[bank_idx];

        let mut out_l = input_l;
        let mut out_r = input_r;

        for idx in 0..TRACK_FX_SLOT_COUNT {
            let slot = &bank_runtime.slots[idx];
            if let Some(roll) = slot.roll {
                (out_l, out_r) = process_roll_frame(
                    &mut bank_state.slots[idx].roll,
                    RollParams {
                        step: roll.step,
                        time_ms: roll.time_mode.milliseconds(roll.time_ms, self.bpm),
                        mode: roll.mode,
                        feedback: roll.feedback,
                        repeat: roll.repeat,
                        mix: roll.mix,
                    },
                    track_enabled[bank_idx][idx],
                    out_l,
                    out_r,
                );
            }
            if !track_enabled[bank_idx][idx] {
                bank_state.slots[idx].filter.trigger = StepTrigger::default();
                continue;
            }

            if let Some(delay) = slot.delay {
                let (l, r) = process_delay_sample(
                    &mut bank_state.slots[idx].delay,
                    DelayParams {
                        time_ms: delay
                            .time_mode
                            .milliseconds(delay.time_ms as usize, self.bpm),
                        feedback: delay.feedback,
                        high_damp_hz: delay.high_damp_hz,
                        direct: delay.direct,
                        effect: delay.effect,
                        low_cut_hz: delay.low_cut_hz,
                    },
                    self.sample_rate,
                    out_l,
                    out_r,
                );
                out_l = l;
                out_r = r;
            }

            if let Some(filter) = slot.filter.as_ref() {
                let gate_on = if !self.clock_active || filter.seq.is_empty() {
                    true
                } else {
                    seq_bool_at_time(&filter.seq, self.bpm, elapsed_secs)
                };
                let retrigger = bank_state.slots[idx].filter.trigger.next(
                    &filter.trigger_seq,
                    self.bpm,
                    elapsed_secs,
                    self.clock_active,
                );
                let dt = 1.0 / self.sample_rate.max(1.0);
                let cutoff_env = bank_state.slots[idx]
                    .filter
                    .env
                    .next(gate_on, retrigger, filter.envelope, dt)
                    .clamp(0.0, 1.0);
                let cutoff_min = FILTER_CUTOFF_MIN_HZ as f32;
                let cutoff_max = filter.cutoff_hz.max(cutoff_min);
                let cutoff_hz = cutoff_min + (cutoff_max - cutoff_min) * cutoff_env;
                let filter_params = FilterParams {
                    filter_type: filter.filter_type,
                    cutoff_hz,
                    q: filter.q,
                    drive: filter.drive,
                    mix: filter.mix,
                };
                out_l = process_filter_sample(
                    &mut bank_state.slots[idx].filter.filter_l,
                    filter_params,
                    self.sample_rate,
                    out_l,
                );
                out_r = process_filter_sample(
                    &mut bank_state.slots[idx].filter.filter_r,
                    filter_params,
                    self.sample_rate,
                    out_r,
                );
            }
        }

        (out_l.clamp(-1.0, 1.0), out_r.clamp(-1.0, 1.0))
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    pub fn metronome_start(&self) -> Option<Instant> {
        self.metronome_start
    }
}

impl TrackFxRuntime {
    pub fn empty(track_count: usize) -> Self {
        Self {
            banks: std::array::from_fn(|_| TrackFxBankRuntime {
                slots: std::array::from_fn(|_| TrackFxSlotRuntime {
                    delay: None,
                    roll: None,
                    filter: None,
                }),
            }),
            track_enabled: vec![[[false; TRACK_FX_SLOT_COUNT]; TRACK_FX_BANK_COUNT]; track_count],
            selected_bank_idx: 0,
        }
    }

    pub fn from_config(config: &TrackFxConfigs) -> Self {
        let banks = std::array::from_fn(|bank_idx| {
            let bank = &config.banks[bank_idx];
            let slots = std::array::from_fn(|slot_idx| {
                let slot = &bank.slots[slot_idx];
                let (delay, roll, filter) = match slot.fx.as_ref() {
                    Some(TrackFx::Delay(delay)) => (
                        Some(DelayRuntime {
                            time_mode: delay.time_mode.value,
                            time_ms: delay
                                .time_ms
                                .value
                                .clamp(TRACK_DELAY_TIME_MIN_MS, TRACK_DELAY_TIME_MAX_MS)
                                as f32,
                            feedback: (delay.feedback_pct.value.min(TRACK_DELAY_FEEDBACK_MAX_PCT)
                                as f32
                                / 100.0)
                                .clamp(0.0, 0.95),
                            high_damp_hz: delay
                                .high_damp_hz
                                .value
                                .clamp(TRACK_DELAY_DAMP_MIN_HZ, TRACK_DELAY_DAMP_MAX_HZ)
                                as f32,
                            direct: delay.direct_pct.value.min(100) as f32 / 100.0,
                            effect: delay.effect_pct.value.min(100) as f32 / 100.0,
                            low_cut_hz: delay.low_cut_hz.value.min(1000) as f32,
                        }),
                        None,
                        None,
                    ),
                    Some(TrackFx::Roll(roll)) => (
                        None,
                        Some(RollRuntime {
                            time_mode: roll.time_mode.value,
                            time_ms: roll.time_ms.value.clamp(1, 1000),
                            mode: roll.mode.value,
                            feedback: roll.feedback.value.min(100) as f32 / 100.0,
                            repeat: roll.repeat.value.min(100),
                            mix: roll.mix.value.min(100) as f32 / 100.0,
                            step: roll.step.value.value(),
                        }),
                        None,
                    ),
                    Some(TrackFx::Filter(filter)) => (
                        None,
                        None,
                        Some(TrackFilterRuntime {
                            filter_type: filter.filter.filter_type.value,
                            cutoff_hz: filter
                                .filter
                                .cutoff_hz
                                .value
                                .clamp(FILTER_CUTOFF_MIN_HZ, FILTER_CUTOFF_MAX_HZ)
                                as f32,
                            q: (filter
                                .filter
                                .resonance_x10
                                .value
                                .clamp(FILTER_Q_MIN_X10, FILTER_Q_MAX_X10)
                                as f32)
                                / 10.0,
                            drive: (filter.filter.drive.value.min(FILTER_DRIVE_MAX) as f32 / 100.0)
                                .clamp(0.0, 1.0),
                            mix: (filter.filter.mix.value.min(FILTER_MIX_MAX) as f32 / 100.0)
                                .clamp(0.0, 1.0),
                            envelope: AhdsrParams {
                                attack_ms: filter.env.attack_ms.value.min(ENVELOPE_ATTACK_MAX_MS)
                                    as f32,
                                hold_ms: filter.env.hold_ms.value.min(ENVELOPE_HOLD_MAX_MS) as f32,
                                decay_ms: filter.env.decay_ms.value.min(ENVELOPE_DECAY_MAX_MS)
                                    as f32,
                                sustain_level: (filter
                                    .env
                                    .sustain_pct
                                    .value
                                    .min(ENVELOPE_SUSTAIN_MAX_PCT)
                                    as f32
                                    / 100.0)
                                    .clamp(0.0, 1.0),
                                release_ms: filter
                                    .env
                                    .release_ms
                                    .value
                                    .clamp(ENVELOPE_RELEASE_MIN_MS, ENVELOPE_RELEASE_MAX_MS)
                                    as f32,
                                start_level:
                                    (filter.env.start_pct.value.min(ENVELOPE_START_MAX_PCT) as f32
                                        / 100.0)
                                        .clamp(0.0, 1.0),
                                tension_attack: tension_to_exponent(
                                    filter.env.tension_a.value.min(ENVELOPE_TENSION_MAX),
                                ),
                                tension_decay: tension_to_exponent(
                                    filter.env.tension_d.value.min(ENVELOPE_TENSION_MAX),
                                ),
                                tension_release: tension_to_exponent(
                                    filter.env.tension_r.value.min(ENVELOPE_TENSION_MAX),
                                ),
                            },
                            seq: filter.seq.seq().to_vec(),
                            trigger_seq: filter
                                .seq
                                .step_len_seq()
                                .iter()
                                .enumerate()
                                .map(|(idx, step_len)| {
                                    *step_len > 0
                                        && filter.seq.seq().get(idx).copied().unwrap_or(false)
                                })
                                .collect(),
                        }),
                    ),
                    None => (None, None, None),
                };
                TrackFxSlotRuntime {
                    delay,
                    roll,
                    filter,
                }
            });
            TrackFxBankRuntime { slots }
        });

        let track_enabled = config.tracks.iter().map(|track| track.enabled).collect();

        Self {
            banks,
            track_enabled,
            selected_bank_idx: config.sel_bank_idx.min(TRACK_FX_BANK_COUNT - 1),
        }
    }
}

impl TrackFxState {
    pub fn new(track_count: usize, sample_rate: f32) -> Self {
        Self {
            tracks: (0..track_count)
                .map(|_| TrackFxTrackState::new(sample_rate))
                .collect(),
        }
    }
}

impl TrackFxTrackState {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            banks: std::array::from_fn(|_| TrackFxBankState::new(sample_rate)),
        }
    }
}

impl TrackFxBankState {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            slots: std::array::from_fn(|_| TrackFxSlotState {
                delay: DelayDspState::new(sample_rate),
                roll: RollDspState::new(),
                filter: TrackFilterDspState::new(),
            }),
        }
    }
}

fn tension_to_exponent(value: usize) -> f32 {
    let t = value.min(ENVELOPE_TENSION_MAX) as f32;
    2.0_f32.powf((t - 100.0) / 50.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sample_clock_activates_track_filter_sequence() {
        let mut config = TrackFxConfigs::new(1);
        config.set_slot_kind(0, 0, crate::config::TrackFxKind::Filter);
        config.tracks[0].enabled[0][0] = true;
        if let Some(TrackFx::Filter(filter)) = &mut config.banks[0].slots[0].fx {
            filter.filter.cutoff_hz.value = 2000;
            filter.env.attack_ms.value = 0;
            filter
                .seq
                .set_seq([vec![false; 12], vec![true; 12]].concat());
        }
        let mut engine = TrackFxEngine::new(8000.0, 1);
        engine.swap_runtime(TrackFxRuntime::from_config(&config));
        engine.set_clock(120, true);
        let mut off = 0.0;
        let mut on = 0.0;
        for frame in 0..8000 {
            let input = (std::f32::consts::TAU * 800.0 * frame as f32 / 8000.0).sin() * 0.1;
            let output = engine
                .process_frame(0, frame as f64 / 8000.0, input, input)
                .0;
            if (2000..4000).contains(&frame) {
                off += output * output;
            }
            if frame >= 6000 {
                on += output * output;
            }
        }
        assert!(
            on > off * 100.0,
            "Sequence gate must actually modulate the filter: {off} / {on}"
        );
    }
    use crate::config::TrackFxKind;
    #[test]
    fn clearing_track_discards_frozen_audio_and_delay_tail() {
        for kind in [TrackFxKind::Roll, TrackFxKind::Delay] {
            let mut config = TrackFxConfigs::new(1);
            config.set_slot_kind(0, 0, kind);
            let mut fx = TrackFxEngine::new(1000.0, 1);
            fx.swap_runtime(TrackFxRuntime::from_config(&config));
            for i in 0..600 {
                fx.process_frame(0, i as f64 / 1000.0, 0.3, -0.3);
            }
            config.toggle_slot_enabled(0, 0);
            fx.swap_runtime(TrackFxRuntime::from_config(&config));
            for i in 0..600 {
                fx.process_frame(0, i as f64 / 1000.0, 0.3, -0.3);
            }
            fx.reset_track(0);
            for i in 0..600 {
                assert_eq!(fx.process_frame(0, i as f64 / 1000.0, 0.0, 0.0), (0.0, 0.0));
            }
        }
    }
}
