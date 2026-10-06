use std::time::Instant;

use crate::config::delay_configs::{
    TRACK_DELAY_DAMP_MAX_HZ, TRACK_DELAY_DAMP_MIN_HZ, TRACK_DELAY_FEEDBACK_MAX_PCT,
    TRACK_DELAY_MIX_MAX_PCT, TRACK_DELAY_TIME_MAX_MS, TRACK_DELAY_TIME_MIN_MS,
};
use crate::config::filter_configs::{
    FILTER_CUTOFF_MAX_HZ, FILTER_CUTOFF_MIN_HZ, FILTER_DRIVE_MAX, FILTER_MIX_MAX, FILTER_Q_MAX_X10,
    FILTER_Q_MIN_X10, FilterType,
};
use crate::config::track_fx_configs::{
    TRACK_FX_BANK_COUNT, TRACK_FX_SLOT_COUNT, TrackFx, TrackFxConfigs,
};
use crate::dsp::delay::{DelayDspState, DelayParams, process_sample as process_delay_sample};
use crate::dsp::filter::{FilterDspState, FilterParams, process_sample as process_filter_sample};
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
impl RollRuntime {
    pub fn from_config(roll: &crate::config::roll_configs::RollConfigs) -> Self {
        Self {
            time_mode: roll.time_mode.value,
            time_ms: roll.time_ms.value.clamp(1, 1000),
            mode: roll.mode.value,
            feedback: roll.feedback.value.min(100) as f32 / 100.0,
            repeat: roll.repeat.value.min(100),
            mix: roll.mix.value.min(100) as f32 / 100.0,
            step: roll.step.value.value(),
        }
    }
    pub fn params(self, bpm: usize) -> RollParams {
        RollParams {
            step: self.step,
            time_ms: self.time_mode.milliseconds(self.time_ms, bpm),
            mode: self.mode,
            feedback: self.feedback,
            repeat: self.repeat,
            mix: self.mix,
        }
    }
}

pub type TrackFilterRuntime = super::input_fx::FilterRuntime;

#[derive(Clone)]
pub struct TrackFxSlotRuntime {
    pub parameter_lane: Option<std::sync::Arc<crate::dsp::automation::PreparedLane>>,
    pub vocoder: Option<super::input_fx::VocoderRuntime>,
    pub audio: Option<crate::dsp::audio_fx::AudioFxParams>,
    pub delay: Option<DelayRuntime>,
    pub roll: Option<RollRuntime>,
    pub filter: Option<TrackFilterRuntime>,
}
impl TrackFxSlotRuntime {
    fn compatible(&self, other: &Self) -> bool {
        self.delay.is_some() == other.delay.is_some()
            && self.roll.is_some() == other.roll.is_some()
            && self.filter.is_some() == other.filter.is_some()
            && self.vocoder.map(|v| v.carrier) == other.vocoder.map(|v| v.carrier)
            && self.audio.as_ref().map(|p| p.config.kind)
                == other.audio.as_ref().map(|p| p.config.kind)
    }
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
    pub carrier_align: super::pdc::AlignDelay,
    pub vocoder: Box<crate::dsp::vocoder::VocoderDspState>,
    pub audio: Box<crate::dsp::audio_fx::AudioFxState>,
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
    control_clock: super::pdc::ClockHistory,
    pub banks: [TrackFxBankState; TRACK_FX_BANK_COUNT],
}

#[derive(Clone)]
pub struct TrackFxState {
    pub tracks: Vec<TrackFxTrackState>,
}

pub struct TrackFxEngine {
    shared_modulation_clock: bool,
    fine_automation: bool,
    pdc_enabled: bool,
    raw_carriers: [Option<(f32, f32)>; 5],
    live_input: (f32, f32),
    transport_elapsed: Option<f64>,
    runtime: TrackFxRuntime,
    state: TrackFxState,
    metronome_start: Option<Instant>,
    clock_active: bool,
    bpm: usize,
    sample_rate: f32,
}

#[derive(Clone, Copy)]
pub struct TrackFilterDspState {
    pub filter_l: FilterDspState,
    pub filter_r: FilterDspState,
}

impl TrackFilterDspState {
    pub fn new() -> Self {
        Self {
            filter_l: FilterDspState::new(),
            filter_r: FilterDspState::new(),
        }
    }
}

impl TrackFxEngine {
    pub fn set_shared_modulation_clock(&mut self, enabled: bool) {
        self.shared_modulation_clock = enabled;
    }
    pub fn set_automation_precision(&mut self, fine: bool) {
        self.fine_automation = fine;
    }
    pub fn new(sample_rate: f32, track_count: usize) -> Self {
        let sr = sample_rate.max(1.0);
        Self {
            shared_modulation_clock: true,
            fine_automation: true,
            pdc_enabled: false,
            raw_carriers: [None; 5],
            live_input: (0.0, 0.0),
            transport_elapsed: None,
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
    pub fn set_vocoder_sources(&mut self, input: (f32, f32), carriers: [Option<(f32, f32)>; 5]) {
        self.live_input = input;
        self.raw_carriers = carriers;
    }
    pub fn set_transport_elapsed(&mut self, seconds: f64) {
        self.transport_elapsed = Some(seconds.max(0.0));
    }
    pub fn set_pdc(&mut self, enabled: bool) {
        self.pdc_enabled = enabled;
    }
    pub fn set_clock(&mut self, bpm: usize, active: bool) {
        self.bpm = bpm;
        self.clock_active = active;
    }
    pub fn exchange_runtime(&mut self, runtime: &mut TrackFxRuntime) {
        if runtime.selected_bank_idx != self.runtime.selected_bank_idx {
            for track in &mut self.state.tracks {
                for bank in &mut track.banks {
                    for slot in &mut bank.slots {
                        slot.roll.reset();
                        slot.audio.reset();
                    }
                }
            }
        }
        for (track_index, track) in self.state.tracks.iter_mut().enumerate() {
            for (bank_index, bank) in track.banks.iter_mut().enumerate() {
                for (slot_index, state) in bank.slots.iter_mut().enumerate() {
                    let old = &self.runtime.banks[bank_index].slots[slot_index];
                    let new = &runtime.banks[bank_index].slots[slot_index];
                    if old.audio.as_ref().map(|p| p.config.kind)
                        != new.audio.as_ref().map(|p| p.config.kind)
                        || (!(self.pdc_enabled
                            && old
                                .audio
                                .as_ref()
                                .is_some_and(|p| p.latency_frames(self.sample_rate) > 0))
                            && self
                                .runtime
                                .track_enabled
                                .get(track_index)
                                .is_some_and(|v| v[bank_index][slot_index])
                            && !runtime
                                .track_enabled
                                .get(track_index)
                                .is_some_and(|v| v[bank_index][slot_index]))
                    {
                        state.audio.reset();
                    }
                    if old.roll.is_some() != new.roll.is_some() {
                        state.roll.reset();
                    }
                    if old.delay.is_some() != new.delay.is_some() {
                        state.delay.reset();
                    }
                    if old.filter.is_some() != new.filter.is_some() {
                        state.filter = TrackFilterDspState::new();
                    }
                    if old.vocoder.is_some() != new.vocoder.is_some() {
                        *state.vocoder = crate::dsp::vocoder::VocoderDspState::new();
                    }
                }
            }
        }
        std::mem::swap(&mut self.runtime, runtime);
    }
    pub fn patch_compatible(&mut self, patch: &mut TrackFxRuntime) {
        for bank in 0..TRACK_FX_BANK_COUNT {
            for slot in 0..TRACK_FX_SLOT_COUNT {
                if self.runtime.banks[bank].slots[slot].compatible(&patch.banks[bank].slots[slot]) {
                    for (index, flags) in self.runtime.track_enabled.iter_mut().enumerate() {
                        if let Some(new) = patch.track_enabled.get_mut(index) {
                            if flags[bank][slot] && !new[bank][slot] {
                                let state = &mut self.state.tracks[index].banks[bank].slots[slot];
                                if !self.pdc_enabled
                                    || !self.runtime.banks[bank].slots[slot]
                                        .audio
                                        .as_ref()
                                        .is_some_and(|p| p.latency_frames(self.sample_rate) > 0)
                                {
                                    state.audio.reset();
                                }
                            }
                            std::mem::swap(&mut flags[bank][slot], &mut new[bank][slot]);
                        }
                    }
                    std::mem::swap(
                        &mut self.runtime.banks[bank].slots[slot],
                        &mut patch.banks[bank].slots[slot],
                    );
                }
            }
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if self.sample_rate == sample_rate.max(1.0) {
            return;
        }
        self.sample_rate = sample_rate.max(1.0);
        for track in &mut self.state.tracks {
            track.control_clock = super::pdc::ClockHistory::new(self.sample_rate);
            for (bank_idx, bank) in track.banks.iter_mut().enumerate() {
                for (slot_idx, slot) in bank.slots.iter_mut().enumerate() {
                    slot.delay.set_sample_rate(self.sample_rate);
                    slot.audio.prepare(self.sample_rate);
                    slot.carrier_align = super::pdc::AlignDelay::new(self.sample_rate);
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
            track.control_clock.reset();
            for bank in &mut track.banks {
                for slot in &mut bank.slots {
                    slot.roll.reset();
                    slot.delay.reset();
                    slot.audio.reset();
                    slot.carrier_align.reset();
                    slot.filter = TrackFilterDspState::new();
                    *slot.vocoder = crate::dsp::vocoder::VocoderDspState::new();
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
        track_state.control_clock.push(
            self.transport_elapsed.unwrap_or(elapsed_secs),
            elapsed_secs,
            self.sample_rate,
            self.bpm,
            self.clock_active,
        );
        let bank_idx = self.runtime.selected_bank_idx.min(TRACK_FX_BANK_COUNT - 1);
        let bank_runtime = &self.runtime.banks[bank_idx];
        let bank_state = &mut track_state.banks[bank_idx];

        let mut out_l = input_l;
        let mut out_r = input_r;
        let mut pipeline_latency = 0;

        for idx in 0..TRACK_FX_SLOT_COUNT {
            let slot = &bank_runtime.slots[idx];
            let point = track_state.control_clock.get(if self.pdc_enabled {
                pipeline_latency
            } else {
                0
            });
            if point.is_none() {
                if self.pdc_enabled {
                    if let Some(audio) = &slot.audio {
                        pipeline_latency += audio.latency_frames(self.sample_rate);
                    }
                }
                continue;
            }
            let point = point.unwrap();
            let lane = crate::dsp::automation::sample_with_precision(
                &slot.parameter_lane,
                point,
                self.sample_rate,
                self.fine_automation,
            );
            if let Some(roll) = slot.roll {
                (out_l, out_r) = process_roll_frame(
                    &mut bank_state.slots[idx].roll,
                    RollParams {
                        step: roll.step,
                        time_ms: roll.time_mode.milliseconds(roll.time_ms, point.bpm),
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
            if !track_enabled[bank_idx][idx]
                && !(self.pdc_enabled
                    && slot
                        .audio
                        .as_ref()
                        .is_some_and(|p| p.latency_frames(self.sample_rate) > 0))
            {
                if let Some(audio) = &slot.audio {
                    bank_state.slots[idx]
                        .audio
                        .observe_bypass(audio, (out_l, out_r));
                }
                continue;
            }

            if let Some(audio) = &slot.audio {
                bank_state.slots[idx].audio.set_pdc(self.pdc_enabled);
                bank_state.slots[idx]
                    .audio
                    .set_shared_modulation_clock(self.shared_modulation_clock);
                let wet = bank_state.slots[idx].audio.process_automated(
                    audio,
                    point.bpm,
                    point.elapsed as f64 / self.sample_rate as f64,
                    point.active,
                    (out_l, out_r),
                    lane,
                );
                (out_l, out_r) = if track_enabled[bank_idx][idx] {
                    wet
                } else {
                    bank_state.slots[idx].audio.aligned_dry()
                };
                if self.pdc_enabled {
                    pipeline_latency += audio.latency_frames(self.sample_rate);
                }
            }
            if let Some(v) = &slot.vocoder {
                use crate::config::vocoder_configs::VocoderCarrier;
                let carrier = match v.carrier {
                    VocoderCarrier::InputLeft => Some((self.live_input.0, self.live_input.0)),
                    VocoderCarrier::InputRight => Some((self.live_input.1, self.live_input.1)),
                    _ => v.carrier.track_idx().and_then(|i| self.raw_carriers[i]),
                };
                let mut c = carrier.unwrap_or((0.0, 0.0));
                if self.pdc_enabled {
                    let aligned = bank_state.slots[idx]
                        .carrier_align
                        .process([c.0, c.1], pipeline_latency);
                    c = (aligned[0], aligned[1]);
                }
                (out_l, out_r) = crate::dsp::vocoder::process_frame(
                    &mut bank_state.slots[idx].vocoder,
                    crate::dsp::vocoder::VocoderParams {
                        bands: v.bands,
                        attack_ms: v.attack_ms,
                        release_ms: v.release_ms,
                        level: v.level,
                        mix: v.mix,
                        sample_rate: self.sample_rate,
                        track_carrier_l: c.0,
                        track_carrier_r: c.1,
                        has_track_carrier: carrier.is_some(),
                        tone: v.tone,
                        mod_sens: v.mod_sens,
                        formant_semitones: v.formant_semitones,
                        sibilance: v.sibilance,
                        modulator_override: Some((out_l + out_r) * 0.5),
                        mute_carrier_channel: None,
                    },
                    out_l,
                    out_r,
                );
            }
            if let Some(delay) = slot.delay {
                let (l, r) = process_delay_sample(
                    &mut bank_state.slots[idx].delay,
                    DelayParams {
                        time_ms: crate::dsp::automation::Value::get(
                            lane,
                            crate::config::automation::Target::DelayTime,
                            delay.time_mode.milliseconds_f32(delay.time_ms, point.bpm),
                        ),
                        feedback: crate::dsp::automation::Value::get(
                            lane,
                            crate::config::automation::Target::DelayFeedback,
                            delay.feedback,
                        ),
                        high_damp_hz: delay.high_damp_hz,
                        direct: delay.direct,
                        effect: crate::dsp::automation::Value::get(
                            lane,
                            crate::config::automation::Target::DelayWet,
                            delay.effect,
                        ),
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
                if self.shared_modulation_clock && lane.is_some() {
                    bank_state.slots[idx]
                        .filter
                        .filter_l
                        .set_control_frame(point.elapsed);
                    bank_state.slots[idx]
                        .filter
                        .filter_r
                        .set_control_frame(point.elapsed);
                }
                let cutoff_hz = crate::dsp::automation::Value::get(
                    lane,
                    crate::config::automation::Target::FilterCutoff,
                    filter.cutoff_hz,
                );
                let filter_params = FilterParams {
                    filter_type: filter.filter_type,
                    cutoff_hz,
                    q: crate::dsp::automation::Value::get(
                        lane,
                        crate::config::automation::Target::FilterQ,
                        filter.q,
                    ),
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

        (crate::dsp::headroom(out_l), crate::dsp::headroom(out_r))
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
                    parameter_lane: None,
                    vocoder: None,
                    audio: None,
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
                            time_ms: delay.time_ms.clamp(
                                TRACK_DELAY_TIME_MIN_MS as f32,
                                TRACK_DELAY_TIME_MAX_MS as f32,
                            ),
                            feedback: if delay.feedback_repeats.value > 0 {
                                10.0_f32.powf(-3.0 / delay.feedback_repeats.value.min(16) as f32)
                            } else {
                                (delay.feedback_pct.value.min(TRACK_DELAY_FEEDBACK_MAX_PCT) as f32
                                    / 100.0)
                                    .clamp(0.0, 1.0)
                            },
                            high_damp_hz: delay
                                .high_damp_hz
                                .value
                                .clamp(TRACK_DELAY_DAMP_MIN_HZ, TRACK_DELAY_DAMP_MAX_HZ)
                                as f32,
                            direct: delay.direct_pct.value.min(100) as f32 / 100.0,
                            effect: delay.effect_pct.value.min(120) as f32 / 100.0,
                            low_cut_hz: delay.low_cut_hz.value.min(12500) as f32,
                        }),
                        None,
                        None,
                    ),
                    Some(TrackFx::Roll(roll)) => (None, Some(RollRuntime::from_config(roll)), None),
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
                        }),
                    ),
                    None | Some(TrackFx::Audio(_) | TrackFx::Vocoder(_)) => (None, None, None),
                };
                TrackFxSlotRuntime {
                    parameter_lane: crate::dsp::automation::PreparedLane::prepare(
                        &slot.parameter_lane,
                        crate::config::automation::track_family(slot.fx.as_ref()),
                    ),
                    vocoder: match &slot.fx {
                        Some(TrackFx::Vocoder(v)) => {
                            Some(super::input_fx::VocoderRuntime::from_config(v))
                        }
                        _ => None,
                    },
                    audio: match &slot.fx {
                        Some(TrackFx::Audio(p)) => {
                            Some(crate::dsp::audio_fx::AudioFxParams::new(p))
                        }
                        _ => None,
                    },
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
                .map(|track_index| TrackFxTrackState::new(sample_rate, track_index))
                .collect(),
        }
    }
}

impl TrackFxTrackState {
    pub fn new(sample_rate: f32, track_index: usize) -> Self {
        Self {
            control_clock: super::pdc::ClockHistory::new(sample_rate),
            banks: std::array::from_fn(|_| TrackFxBankState::new(sample_rate, track_index)),
        }
    }
}

impl TrackFxBankState {
    pub fn new(sample_rate: f32, track_index: usize) -> Self {
        Self {
            slots: std::array::from_fn(|slot_index| TrackFxSlotState {
                carrier_align: super::pdc::AlignDelay::new(sample_rate),
                vocoder: Box::new(crate::dsp::vocoder::VocoderDspState::new()),
                audio: Box::new(crate::dsp::audio_fx::AudioFxState::new_with_stagger(
                    sample_rate,
                    4 + track_index * 4 + slot_index,
                )),
                delay: DelayDspState::new(sample_rate),
                roll: RollDspState::new(),
                filter: TrackFilterDspState::new(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn late_synced_fx_enable_and_reopen_match_unequal_tracks_at_pdc_source_time() {
        use crate::config::{TrackFxKind, audio_fx::AudioFxKind as K};
        let sr = 8000.0;
        let latency = crate::dsp::pitch_shift::latency_frames(sr);
        for beat_step in [0.0, 0.25] {
            let mut config = TrackFxConfigs::new(2);
            config.set_slot_kind(0, 0, TrackFxKind::Audio(K::Transpose));
            config.set_slot_kind(0, 1, TrackFxKind::Audio(K::Tremolo));
            if let Some(TrackFx::Audio(p)) = config.slot_fx_mut(0, 1) {
                p.sync_beats = 1.5;
                p.mod_stepped = true;
                p.mod_step_beats = beat_step;
                p.mod_step_hz = 3.7;
                p.mod_retrigger = false;
                p.depth = 1.0;
                p.mix = 1.0;
            }
            for track in 0..2 {
                config.tracks[track].enabled[0][0] = true;
                config.tracks[track].enabled[0][1] = true;
            }
            let mut on = TrackFxRuntime::from_config(&config);
            let mut reopen = on.clone();
            config.tracks[1].enabled[0][1] = false;
            let mut off = TrackFxRuntime::from_config(&config);
            let mut engine = TrackFxEngine::new(sr, 2);
            engine.set_pdc(true);
            engine.exchange_runtime(&mut TrackFxRuntime::from_config(&config));
            let count = crate::test_alloc::count(|| {
                for frame in 0..14000 {
                    if frame == 1739 {
                        engine.exchange_runtime(&mut on);
                    }
                    if frame == 4301 {
                        engine.exchange_runtime(&mut off);
                    }
                    if frame == 5717 {
                        engine.exchange_runtime(&mut reopen);
                    }
                    let active = !(8000..9300).contains(&frame);
                    let origin = if frame >= 9300 { 9300 } else { 0 };
                    let elapsed = if active {
                        (frame - origin) as f64 / sr as f64
                    } else {
                        0.0
                    };
                    engine.set_clock(137, active);
                    engine.set_transport_elapsed(elapsed);
                    let a = engine.process_frame(0, (frame % 797) as f64 / sr as f64, 0.2, 0.1);
                    let b = engine.process_frame(
                        1,
                        ((frame + 239) % 1301) as f64 / sr as f64,
                        0.2,
                        0.1,
                    );
                    let enabled = (1739..4301).contains(&frame) || frame >= 5717;
                    if enabled && (frame < 8000 || frame >= 9300 + latency) {
                        assert!(
                            (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6,
                            "step{beat_step},frame{frame}: {a:?}/{b:?}"
                        );
                    }
                }
            });
            assert_eq!(count, 0);
        }
    }
    #[test]
    fn equal_final_filter_curve_has_equal_audio_after_different_edit_and_codec_paths() {
        use crate::{
            config::{
                AppConfig, FxKind, InputFx,
                automation::{Interpolation, ParameterLane, Target},
            },
            engine::input_fx::{InputFxEngine, InputFxRuntime},
            project,
        };
        let mut direct = AppConfig::new(131, 0, 5);
        direct.input_fx.set_slot_kind(0, 0, FxKind::Filter);
        direct.input_fx.banks[0].slots[0].is_enabled = true;
        let mut final_lane = ParameterLane::create(Target::FilterCutoff);
        final_lane.enabled = true;
        final_lane.interpolation = Interpolation::Curve;
        final_lane.points[0].curve = -0.7;
        final_lane.points[1].curve = 0.5;
        direct.input_fx.banks[0].slots[0].parameter_lane = final_lane.clone();
        let mut edited = AppConfig::new(131, 0, 5);
        edited.input_fx.set_slot_kind(0, 0, FxKind::Filter);
        edited.input_fx.banks[0].slots[0].is_enabled = true;
        edited.input_fx.banks[0].slots[0].parameter_lane = ParameterLane::create(Target::FilterQ);
        edited.input_fx.banks[0].slots[0].parameter_lane.enabled = true;
        let _discarded = InputFxRuntime::from_config(&edited.input_fx);
        edited.input_fx.banks[0].slots[0].parameter_lane = final_lane;
        let json = serde_json::to_vec(&project::data_from_config(&edited)).unwrap();
        project::apply_data_to_config(&mut edited, serde_json::from_slice(&json).unwrap());
        let mut a = InputFxEngine::new(8000.0);
        a.set_clock(true, 131);
        a.swap_runtime(InputFxRuntime::from_config(&direct.input_fx));
        let mut b = InputFxEngine::new(8000.0);
        b.set_clock(true, 131);
        b.swap_runtime(InputFxRuntime::from_config(&edited.input_fx));
        let allocations = crate::test_alloc::count(|| {
            for frame in 0..12000 {
                let x = (frame as f32 * 0.31).sin() * 0.1;
                let time = frame as f64 / 8000.0;
                let a = a.process_frame(time, x, -x, &[]);
                let b = b.process_frame(time, x, -x, &[]);
                assert_eq!(
                    [a.0.to_bits(), a.1.to_bits()],
                    [b.0.to_bits(), b.1.to_bits()]
                );
            }
        });
        assert_eq!(allocations, 0);
        let Some(InputFx::Filter(f)) = &edited.input_fx.banks[0].slots[0].fx else {
            panic!()
        };
        assert_eq!(
            f.cutoff_hz.value, 1000,
            "Automation must not rewrite static base values"
        );
    }
    #[test]
    fn pdc_later_modulation_matches_processing_then_delaying_even_across_clock_edges() {
        use crate::config::{TrackFxKind, audio_fx::AudioFxKind as K};
        let sr = 8000.0;
        let latency = crate::dsp::pitch_shift::latency_frames(sr);
        for kind in [K::StepSlicer, K::AutoPan, K::Tremolo, K::Phaser] {
            let mut c = TrackFxConfigs::new(1);
            c.set_slot_kind(0, 1, TrackFxKind::Audio(kind));
            c.tracks[0].enabled[0][1] = true;
            if let Some(TrackFx::Audio(p)) = c.slot_fx_mut(0, 1) {
                p.sync_beats = 0.5;
                p.depth = 1.0;
            }
            let mut reference = TrackFxEngine::new(sr, 1);
            reference.set_pdc(true);
            reference.exchange_runtime(&mut TrackFxRuntime::from_config(&c));
            c.set_slot_kind(0, 0, TrackFxKind::Audio(K::Transpose));
            c.tracks[0].enabled[0][0] = true;
            let mut delayed = TrackFxEngine::new(sr, 1);
            delayed.set_pdc(true);
            delayed.exchange_runtime(&mut TrackFxRuntime::from_config(&c));
            let mut samples = Vec::with_capacity(12000);
            let count = crate::test_alloc::count(|| {
                for n in 0..12000 {
                    let active = (400..10000).contains(&n);
                    let elapsed = if active {
                        (n - 400) as f64 / sr as f64
                    } else {
                        0.0
                    };
                    reference.set_clock(120, active);
                    delayed.set_clock(120, active);
                    reference.set_transport_elapsed(elapsed);
                    delayed.set_transport_elapsed(elapsed);
                    let x = (n as f32 * 0.113).sin() * 0.15 + 0.05;
                    let time = (n % 997) as f64 / sr as f64;
                    samples.push(reference.process_frame(0, time, x, -x));
                    let got = delayed.process_frame(0, time, x, -x);
                    let expected = if n >= latency {
                        samples[n - latency]
                    } else {
                        (0.0, 0.0)
                    };
                    assert!(
                        (got.0 - expected.0).abs() < 0.0005 && (got.1 - expected.1).abs() < 0.0005,
                        "{kind:?} frame{n}: {got:?}/{expected:?}"
                    );
                }
            });
            assert_eq!(count, 0);
        }
    }
    #[test]
    fn pdc_track_filter_lane_uses_source_transport_time_not_wrapping_loop_cursor() {
        use crate::config::{
            TrackFxKind,
            audio_fx::AudioFxKind,
            automation::{Interpolation, ParameterLane, Target},
        };
        let sr = 8000.0;
        let latency = crate::dsp::pitch_shift::latency_frames(sr);
        let mut c = TrackFxConfigs::new(1);
        c.set_slot_kind(0, 1, TrackFxKind::Filter);
        c.tracks[0].enabled[0][1] = true;
        let mut lane = ParameterLane::create(Target::FilterCutoff);
        lane.enabled = true;
        lane.interpolation = Interpolation::Curve;
        lane.points[0].curve = -0.8;
        c.banks[0].slots[1].parameter_lane = lane;
        let mut reference = TrackFxEngine::new(sr, 1);
        reference.set_pdc(true);
        reference.exchange_runtime(&mut TrackFxRuntime::from_config(&c));
        c.set_slot_kind(0, 0, TrackFxKind::Audio(AudioFxKind::Transpose));
        c.tracks[0].enabled[0][0] = true;
        let mut delayed = TrackFxEngine::new(sr, 1);
        delayed.set_pdc(true);
        delayed.exchange_runtime(&mut TrackFxRuntime::from_config(&c));
        let mut samples = Vec::with_capacity(10000);
        let allocations = crate::test_alloc::count(|| {
            for n in 0..10000 {
                let pos = (n + 311) % 997;
                let time = pos as f64 / sr as f64;
                let x = (pos as f32 * 0.317).sin() * 0.2;
                let active = (120..7500).contains(&n);
                reference.set_clock(120, active);
                delayed.set_clock(120, active);
                reference.set_transport_elapsed(n as f64 / sr as f64);
                delayed.set_transport_elapsed(n as f64 / sr as f64);
                samples.push(reference.process_frame(0, time, x, x));
                let got = delayed.process_frame(0, time, x, x);
                let expected = if n >= latency {
                    samples[n - latency]
                } else {
                    (0.0, 0.0)
                };
                assert!(
                    (got.0 - expected.0).abs() < 0.0005,
                    "sample {n}: {got:?}/{expected:?}"
                );
            }
        });
        assert_eq!(allocations, 0);
    }
    #[test]
    fn track_vocoder_uses_explicit_pre_fx_carrier_and_keeps_stereo() {
        use crate::config::vocoder_configs::VocoderCarrier;
        let mut c = TrackFxConfigs::new(1);
        c.set_slot_kind(0, 0, crate::config::TrackFxKind::Vocoder);
        c.tracks[0].enabled[0][0] = true;
        if let Some(TrackFx::Vocoder(v)) = c.slot_fx_mut(0, 0) {
            v.carrier.value = VocoderCarrier::Track2;
            v.attack_ms.value = 0;
            v.release_ms.value = 20;
            v.mix.value = 100;
        }
        let mut e = TrackFxEngine::new(8000.0, 1);
        e.exchange_runtime(&mut TrackFxRuntime::from_config(&c));
        for n in 0..1000 {
            assert_eq!(e.process_frame(0, n as f64 / 8000.0, 0.2, 0.2), (0.0, 0.0));
        }
        let mut energy = 0.0;
        let count = crate::test_alloc::count(|| {
            for n in 0..8000 {
                let carrier = (std::f32::consts::TAU * 220.0 * n as f32 / 8000.0).sin() * 0.3;
                e.set_vocoder_sources(
                    (0.0, 0.0),
                    [None, Some((carrier, -carrier)), None, None, None],
                );
                let voice = (std::f32::consts::TAU * 440.0 * n as f32 / 8000.0).sin() * 0.4;
                let y = e.process_frame(0, n as f64 / 8000.0, voice, voice);
                energy += y.0 * y.0;
                assert!((y.0 + y.1).abs() < 1e-5);
            }
        });
        assert_eq!(count, 0);
        assert!(energy > 0.01);
        e.reset_track(0);
        e.set_vocoder_sources((0.0, 0.0), [None; 5]);
        for n in 0..1000 {
            assert_eq!(e.process_frame(0, n as f64 / 8000.0, 0.2, 0.2), (0.0, 0.0));
        }
    }
    #[test]
    fn replacing_new_fx_or_clearing_a_track_cannot_resume_an_old_tail() {
        use crate::config::audio_fx::AudioFxKind as K;
        for kind in [
            K::Delay,
            K::PanningDelay,
            K::Freeze,
            K::Transpose,
            K::Reverb,
        ] {
            let mut c = TrackFxConfigs::new(1);
            c.set_slot_kind(0, 0, crate::config::TrackFxKind::Audio(kind));
            c.tracks[0].enabled[0][0] = true;
            if let Some(TrackFx::Audio(p)) = c.slot_fx_mut(0, 0) {
                p.semitones = 7.0;
            }
            let mut e = TrackFxEngine::new(8000.0, 1);
            e.exchange_runtime(&mut TrackFxRuntime::from_config(&c));
            for n in 0..3000 {
                e.process_frame(0, n as f64 / 8000.0, 0.3, -0.2);
            }
            e.reset_track(0);
            for n in 0..3000 {
                assert_eq!(e.process_frame(0, n as f64 / 8000.0, 0.0, 0.0), (0.0, 0.0));
            }
            c.tracks[0].enabled[0][0] = false;
            let mut off = TrackFxRuntime::from_config(&c);
            let allocations = crate::test_alloc::count(|| e.exchange_runtime(&mut off));
            assert_eq!(allocations, 0);
            c.tracks[0].enabled[0][0] = true;
            e.exchange_runtime(&mut TrackFxRuntime::from_config(&c));
            for n in 0..3000 {
                assert_eq!(e.process_frame(0, n as f64 / 8000.0, 0.0, 0.0), (0.0, 0.0));
            }
        }
    }
    #[test]
    fn shared_transport_keeps_modulation_aligned_across_unequal_track_loops() {
        use crate::config::audio_fx::AudioFxKind;
        let mut c = TrackFxConfigs::new(2);
        c.set_slot_kind(
            0,
            0,
            crate::config::TrackFxKind::Audio(AudioFxKind::Tremolo),
        );
        if let Some(TrackFx::Audio(p)) = c.slot_fx_mut(0, 0) {
            p.sync_beats = 1.0;
            p.depth = 1.0;
            p.mix = 1.0;
        }
        c.tracks[0].enabled[0][0] = true;
        c.tracks[1].enabled[0][0] = true;
        let mut e = TrackFxEngine::new(8000.0, 2);
        e.swap_runtime(TrackFxRuntime::from_config(&c));
        e.set_clock(120, true);
        for n in 0..8000 {
            e.set_transport_elapsed(n as f64 / 8000.0);
            let a = e.process_frame(0, (n % 797) as f64 / 8000.0, 0.2, 0.2);
            let b = e.process_frame(1, ((n + 239) % 1301) as f64 / 8000.0, 0.2, 0.2);
            assert_eq!(
                a, b,
                "Different loop lengths changed synchronized modulation at {n}"
            );
        }
    }
    #[test]
    #[ignore = "manual worst-case callback timing, not a physical device deadline guarantee"]
    fn benchmark_twenty_automated_filters_and_unity_delays() {
        use crate::config::{
            audio_fx::AudioFxKind as K,
            automation::{Interpolation, ParameterLane, Target},
        };
        for (kind, target) in [
            (TrackFxKind::Filter, Target::FilterCutoff),
            (TrackFxKind::Delay, Target::DelayTime),
            (TrackFxKind::Audio(K::PanningDelay), Target::DelayTime),
        ] {
            let mut config = TrackFxConfigs::new(5);
            for slot in 0..4 {
                config.set_slot_kind(0, slot, kind);
                let fx_slot = &mut config.banks[0].slots[slot];
                let mut lane = ParameterLane::create(target);
                lane.enabled = true;
                lane.interpolation = Interpolation::Curve;
                for (i, p) in lane.points.iter_mut().enumerate() {
                    p.value = if i == 1 { 1.0 } else { 0.0 };
                    p.curve = if i == 0 { -0.5 } else { 0.5 };
                }
                fx_slot.parameter_lane = lane;
                match &mut fx_slot.fx {
                    Some(TrackFx::Filter(p)) => {
                        p.filter.mix.value = 100;
                        p.filter.resonance_x10.value = 20;
                    }
                    Some(TrackFx::Delay(p)) => {
                        p.feedback_repeats.value = 0;
                        p.feedback_pct.value = 100;
                        p.high_damp_hz.value = 0;
                        p.low_cut_hz.value = 0;
                        p.effect_pct.value = 100;
                    }
                    Some(TrackFx::Audio(p)) => {
                        p.feedback_repeats = 0;
                        p.feedback = 1.0;
                        p.high_cut_hz = 0.0;
                        p.low_cut_hz = 0.0;
                        p.effect_level = 1.0;
                        p.sync_beats = 0.0;
                    }
                    _ => unreachable!(),
                }
                for track in &mut config.tracks {
                    track.enabled[0][slot] = true;
                }
            }
            let mut engine = TrackFxEngine::new(48000.0, 5);
            engine.prepare();
            engine.swap_runtime(TrackFxRuntime::from_config(&config));
            engine.set_clock(120, true);
            let mut times = Vec::with_capacity(1125);
            let mut checksum = 0.0;
            let allocations = crate::test_alloc::count(|| {
                for block in 0..1125 {
                    let before = std::time::Instant::now();
                    for offset in 0..128 {
                        let n = block * 128 + offset;
                        let t = n as f64 / 48000.0;
                        engine.set_transport_elapsed(t);
                        let input = if n < 24000 {
                            (std::f32::consts::TAU * 55.0 * t as f32).sin() * 0.35
                        } else {
                            0.0
                        };
                        for track in 0..5 {
                            let (l, r) = engine.process_frame(track, t, input, input * 0.8);
                            assert!(l.is_finite() && r.is_finite());
                            checksum += std::hint::black_box(l + r) as f64;
                        }
                    }
                    times.push(before.elapsed().as_secs_f64() * 1000.0);
                }
            });
            assert_eq!(allocations, 0);
            let total = times.iter().sum::<f64>();
            times.sort_by(f64::total_cmp);
            println!(
                "20 x {} with continuous lane: 3 s audio, CPU {total:.2} ms; 128f p95 {:.3}, p99 {:.3}, max {:.3} ms; allocations {allocations}; checksum {checksum}",
                kind.name(),
                times[1068],
                times[1113],
                times[1124]
            );
        }
    }
    #[test]
    #[ignore = "manual worst-case callback timing, not a physical device deadline guarantee"]
    fn benchmark_twenty_audio_fx_in_128_frame_callbacks() {
        use crate::config::audio_fx::AudioFxKind as K;
        for kind in [K::Transpose, K::Electric, K::Octave, K::Dynamics, K::Reverb] {
            let mut c = TrackFxConfigs::new(5);
            for slot in 0..4 {
                c.set_slot_kind(0, slot, crate::config::TrackFxKind::Audio(kind));
                if let Some(TrackFx::Audio(p)) = c.slot_fx_mut(0, slot) {
                    p.semitones = 7.0;
                }
                for track in 0..5 {
                    c.tracks[track].enabled[0][slot] = true;
                }
            }
            let mut e = TrackFxEngine::new(48000.0, 5);
            e.prepare();
            e.swap_runtime(TrackFxRuntime::from_config(&c));
            e.set_clock(120, true);
            let mut timings = Vec::with_capacity(1125);
            let mut checksum = 0.0;
            for block in 0..1125 {
                let before = std::time::Instant::now();
                for offset in 0..128 {
                    let n = block * 128 + offset;
                    let t = n as f64 / 48000.0;
                    e.set_transport_elapsed(t);
                    for track in 0..5 {
                        let input =
                            (std::f32::consts::TAU * (110.0 + track as f32 * 31.0) * t as f32)
                                .sin()
                                * 0.15;
                        checksum +=
                            std::hint::black_box(e.process_frame(track, t, input, -input * 0.8).0)
                                as f64;
                    }
                }
                timings.push(before.elapsed().as_secs_f64() * 1000.0);
            }
            let total = timings.iter().sum::<f64>();
            timings.sort_by(f64::total_cmp);
            println!(
                "20 x {kind:?}: 3 s audio, CPU {total:.2} ms, 128f budget2.667 ms; p95 {:.3}, p99 {:.3}, max {:.3} ms; checksum {checksum}",
                timings[timings.len() * 95 / 100],
                timings[timings.len() * 99 / 100],
                timings[timings.len() - 1]
            );
        }
    }
    #[test]
    fn static_track_filter_matches_input_filter_before_start_and_across_transport_changes() {
        use crate::{
            config::{AppConfig, FxKind, InputFx, TrackFxKind},
            engine::input_fx::{InputFxEngine, InputFxRuntime},
        };
        for sr in [8000.0, 48000.0] {
            for kind in [
                FilterType::Lpf,
                FilterType::Hpf,
                FilterType::Bpf,
                FilterType::Notch,
            ] {
                let mut c = AppConfig::new(137, 0, 5);
                c.input_fx.set_slot_kind(0, 0, FxKind::Filter);
                c.input_fx.banks[0].slots[0].is_enabled = true;
                c.track_fx.set_slot_kind(0, 0, TrackFxKind::Filter);
                c.track_fx.tracks[0].enabled[0][0] = true;
                let Some(InputFx::Filter(input)) = &mut c.input_fx.banks[0].slots[0].fx else {
                    panic!()
                };
                input.filter_type.value = kind;
                input.cutoff_hz.value = 1800;
                input.resonance_x10.value = 19;
                input.drive.value = 23;
                input.mix.value = 81;
                let Some(TrackFx::Filter(track)) = c.track_fx.slot_fx_mut(0, 0) else {
                    panic!()
                };
                track.filter.filter_type.value = kind;
                track.filter.cutoff_hz.value = 1800;
                track.filter.resonance_x10.value = 19;
                track.filter.drive.value = 23;
                track.filter.mix.value = 81;
                let mut input = InputFxEngine::new(sr);
                input.swap_runtime(InputFxRuntime::from_config(&c.input_fx));
                let mut track = TrackFxEngine::new(sr, 1);
                track.exchange_runtime(&mut TrackFxRuntime::from_config(&c.track_fx));
                let mut energy = 0.0;
                let allocations = crate::test_alloc::count(|| {
                    for n in 0..6000 {
                        let active = (701..3800).contains(&n);
                        input.set_clock(active, 137);
                        track.set_clock(137, active);
                        let dry = (
                            (n as f32 * 0.137).sin() * 0.12,
                            (n as f32 * 0.083).cos() * 0.1,
                        );
                        let a = input.process_frame(n as f64 / sr as f64, dry.0, dry.1, &[]);
                        track.set_transport_elapsed(n as f64 / sr as f64);
                        let b = track.process_frame(0, (n % 797) as f64 / sr as f64, dry.0, dry.1);
                        assert_eq!(
                            [a.0.to_bits(), a.1.to_bits()],
                            [b.0.to_bits(), b.1.to_bits()],
                            "{kind} at {sr} / sample {n}"
                        );
                        if n < 600 {
                            energy += b.0.abs() + b.1.abs();
                        }
                    }
                });
                assert_eq!(allocations, 0);
                assert!(energy > 1.0, "Static filter must sound without transport");
            }
        }
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
