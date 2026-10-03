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
    pub vocoder: Option<super::input_fx::VocoderRuntime>,
    pub audio: Option<crate::dsp::audio_fx::AudioFxParams>,
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
    pub banks: [TrackFxBankState; TRACK_FX_BANK_COUNT],
}

#[derive(Clone)]
pub struct TrackFxState {
    pub tracks: Vec<TrackFxTrackState>,
}

pub struct TrackFxEngine {
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
                        || (self
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

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
        for track in &mut self.state.tracks {
            for (bank_idx, bank) in track.banks.iter_mut().enumerate() {
                for (slot_idx, slot) in bank.slots.iter_mut().enumerate() {
                    slot.delay.set_sample_rate(self.sample_rate);
                    slot.audio.prepare(self.sample_rate);
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
                    slot.audio.reset();
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
                if let Some(audio) = &slot.audio {
                    bank_state.slots[idx]
                        .audio
                        .observe_bypass(audio, (out_l, out_r));
                }
                bank_state.slots[idx].filter.trigger = StepTrigger::default();
                continue;
            }

            if let Some(audio) = &slot.audio {
                (out_l, out_r) = bank_state.slots[idx].audio.process(
                    audio,
                    self.bpm,
                    self.transport_elapsed.unwrap_or(elapsed_secs),
                    self.clock_active,
                    (out_l, out_r),
                );
            }
            if let Some(v) = &slot.vocoder {
                use crate::config::vocoder_configs::VocoderCarrier;
                let carrier = match v.carrier {
                    VocoderCarrier::InputLeft => Some((self.live_input.0, self.live_input.0)),
                    VocoderCarrier::InputRight => Some((self.live_input.1, self.live_input.1)),
                    _ => v.carrier.track_idx().and_then(|i| self.raw_carriers[i]),
                };
                let c = carrier.unwrap_or((0.0, 0.0));
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
                            time_ms: delay
                                .time_ms
                                .value
                                .clamp(TRACK_DELAY_TIME_MIN_MS, TRACK_DELAY_TIME_MAX_MS)
                                as f32,
                            feedback: if delay.feedback_repeats.value > 0 {
                                10.0_f32.powf(-3.0 / delay.feedback_repeats.value.min(16) as f32)
                            } else {
                                (delay.feedback_pct.value.min(TRACK_DELAY_FEEDBACK_MAX_PCT) as f32
                                    / 100.0)
                                    .clamp(0.0, 0.95)
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
                    None | Some(TrackFx::Audio(_) | TrackFx::Vocoder(_)) => (None, None, None),
                };
                TrackFxSlotRuntime {
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
            banks: std::array::from_fn(|_| TrackFxBankState::new(sample_rate, track_index)),
        }
    }
}

impl TrackFxBankState {
    pub fn new(sample_rate: f32, track_index: usize) -> Self {
        Self {
            slots: std::array::from_fn(|slot_index| TrackFxSlotState {
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

fn tension_to_exponent(value: usize) -> f32 {
    let t = value.min(ENVELOPE_TENSION_MAX) as f32;
    2.0_f32.powf((t - 100.0) / 50.0)
}

#[cfg(test)]
mod tests {
    use super::*;
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
