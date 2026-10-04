use std::time::Instant;

use crate::config::envelope_configs::{
    ENVELOPE_ATTACK_MAX_MS, ENVELOPE_DECAY_MAX_MS, ENVELOPE_HOLD_MAX_MS, ENVELOPE_RELEASE_MAX_MS,
    ENVELOPE_RELEASE_MIN_MS, ENVELOPE_START_MAX_PCT, ENVELOPE_SUSTAIN_MAX_PCT,
    ENVELOPE_TENSION_MAX,
};
use crate::config::filter_configs::{
    FILTER_CUTOFF_MAX_HZ, FILTER_CUTOFF_MIN_HZ, FILTER_DRIVE_MAX, FILTER_MIX_MAX, FILTER_Q_MAX_X10,
    FILTER_Q_MIN_X10, FilterType,
};
use crate::config::mydelay_configs::{MYDELAY_LEVEL_MAX, MYDELAY_THRESHOLD_MAX};
use crate::config::note_configs::NoteOct;
use crate::config::osc_configs::Waveform;
use crate::config::reverb_configs::{
    REVERB_HIGHCUT_MAX, REVERB_LOWCUT_MAX_HZ, REVERB_LOWCUT_MIN_HZ, REVERB_PREDELAY_MAX_MS,
    REVERB_RT60_MAX_MS, REVERB_RT60_MIN_MS, REVERB_SIZE_MAX, REVERB_SIZE_MAX_MS,
    REVERB_SIZE_MIN_MS, REVERB_WIDTH_MAX,
};
use crate::config::vocoder_configs::{
    VOCODER_ATTACK_MAX_MS, VOCODER_BANDS_MAX, VOCODER_BANDS_MIN, VOCODER_LEVEL_MAX,
    VOCODER_MIX_MAX, VOCODER_RELEASE_MAX_MS, VocoderCarrier,
};
use crate::config::{
    InputFx, InputFxConfigs, input_fx_configs::FX_BANK_COUNT, input_fx_configs::FX_SLOT_COUNT,
};
use crate::dsp::envelope::AhdsrParams;
use crate::dsp::filter::{FilterDspState, FilterParams, process_sample as process_filter_sample};
use crate::dsp::my_delay::{
    MyDelayFxDspState, MyDelayFxParams, process_fx_sample as process_mydelay_fx_sample,
};
use crate::dsp::note::{StepTrigger, note_at_time, seq_bool_at_time};
use crate::dsp::oscillator::{
    OscillatorFxDspState, OscillatorFxParams, process_fx_sample as process_osc_fx_sample,
};
use crate::dsp::reverb::{ReverbDspState, ReverbParams, process_sample as process_reverb_frame};
use crate::dsp::vocoder::{VocoderDspState, VocoderParams, process_frame as process_vocoder_frame};

const DEFAULT_BPM: usize = 120;

#[derive(Clone)]
pub struct OscillatorRuntime {
    pub poly: crate::dsp::oscillator::PolyOscRuntime,
    pub waveform: Waveform,
    pub level: f32,
    pub note_current: Option<NoteOct>,
    pub note_seq: Vec<Option<NoteOct>>,
    pub note_on_seq: Vec<bool>,
    pub note_trigger_seq: Vec<bool>,
    pub threshold: f32,
    pub envelope: AhdsrParams,
    pub osc_filter: FilterRuntime,
    pub osc_filter_envelope: AhdsrParams,
}

#[derive(Clone, Copy)]
pub struct FilterRuntime {
    pub sweep: crate::config::filter_configs::FilterSweepConfig,
    pub filter_type: FilterType,
    pub cutoff_hz: f32,
    pub q: f32,
    pub drive: f32,
    pub mix: f32,
}

#[derive(Clone, Copy)]
pub struct ReverbRuntime {
    pub dry_level: f32,
    pub wet_level: f32,
    pub density: f32,
    pub size_ms: f32,
    pub rt60_ms: f32,
    pub predelay_ms: f32,
    pub width: f32,
    pub high_cut_hz: f32,
    pub low_cut_hz: f32,
}

#[derive(Clone)]
pub struct MyDelayRuntime {
    pub level: f32,
    pub threshold: f32,
    pub note_current: Option<NoteOct>,
    pub note_seq: Vec<Option<NoteOct>>,
    pub note_on_seq: Vec<bool>,
    pub note_trigger_seq: Vec<bool>,
    pub audio_env: AhdsrParams,
    pub filter_env: AhdsrParams,
    pub filter: FilterRuntime,
}

#[derive(Clone, Copy)]
pub struct VocoderRuntime {
    pub tone: f32,
    pub mod_sens: f32,
    pub formant_semitones: f32,
    pub sibilance: f32,
    pub carrier_thru: bool,
    pub carrier: VocoderCarrier,
    pub bands: usize,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub level: f32,
    pub mix: f32,
}
impl VocoderRuntime {
    pub fn from_config(v: &crate::config::vocoder_configs::VocoderConfigs) -> Self {
        Self {
            tone: v.tone.clamp(-50, 50) as f32,
            mod_sens: v.mod_sens.clamp(-50, 50) as f32,
            formant_semitones: v.formant_semitones.clamp(-12, 12) as f32,
            sibilance: v.sibilance.value.min(100) as f32 / 100.0,
            carrier_thru: v.carrier_thru,
            carrier: v.carrier.value,
            bands: v.bands.value.clamp(VOCODER_BANDS_MIN, VOCODER_BANDS_MAX),
            attack_ms: v.attack_ms.value.min(VOCODER_ATTACK_MAX_MS) as f32,
            release_ms: v.release_ms.value.min(VOCODER_RELEASE_MAX_MS) as f32,
            level: v.level.value.min(VOCODER_LEVEL_MAX) as f32 / 100.0,
            mix: v.mix.value.min(VOCODER_MIX_MAX) as f32 / 100.0,
        }
    }
}

#[derive(Clone)]
pub struct FxSlotRuntime {
    pub source_key: u64,
    pub roll: Option<super::track_fx::RollRuntime>,
    pub audio: Option<crate::dsp::audio_fx::AudioFxParams>,
    pub enabled: bool,
    pub osc: Option<OscillatorRuntime>,
    pub filter: Option<FilterRuntime>,
    pub reverb: Option<ReverbRuntime>,
    pub my_delay: Option<MyDelayRuntime>,
    pub vocoder: Option<VocoderRuntime>,
}

#[derive(Clone)]
pub struct FxSlotState {
    pub generator_align: super::pdc::AlignDelay,
    pub modulator_align: super::pdc::AlignDelay,
    pub carrier_align: super::pdc::AlignDelay,
    pub roll: crate::dsp::roll::RollDspState,
    pub audio: Box<crate::dsp::audio_fx::AudioFxState>,
    pub trigger: StepTrigger,
    pub osc: OscillatorFxDspState,
    pub poly_osc: crate::dsp::oscillator::PolyOscState,
    pub filter_sweep: crate::dsp::filter_sweep::FilterSweepState,
    pub filter_l: FilterDspState,
    pub filter_r: FilterDspState,
    pub reverb: ReverbDspState,
    pub my_delay: MyDelayFxDspState,
    pub vocoder: VocoderDspState,
}
impl FxSlotRuntime {
    fn compatible(&self, other: &Self) -> bool {
        self.source_key == other.source_key
            && self.osc.is_some() == other.osc.is_some()
            && self.filter.is_some() == other.filter.is_some()
            && self.reverb.is_some() == other.reverb.is_some()
            && self.my_delay.is_some() == other.my_delay.is_some()
            && self.roll.is_some() == other.roll.is_some()
            && self.vocoder.map(|v| v.carrier) == other.vocoder.map(|v| v.carrier)
            && self.audio.as_ref().map(|p| p.config.kind)
                == other.audio.as_ref().map(|p| p.config.kind)
    }
}

#[derive(Clone)]
pub struct FxBankRuntime {
    pub slots: [FxSlotRuntime; FX_SLOT_COUNT],
}

#[derive(Clone)]
pub struct FxBankState {
    pub slots: [FxSlotState; FX_SLOT_COUNT],
}

#[derive(Clone)]
pub struct InputFxRuntime {
    pub banks: [FxBankRuntime; FX_BANK_COUNT],
    pub selected_bank_idx: usize,
}

#[derive(Clone)]
pub struct InputFxState {
    pub banks: Vec<FxBankState>,
}

pub struct InputFxEngine {
    control_clock: super::pdc::ClockHistory,
    pdc_enabled: bool,
    track_latencies: [usize; 5],
    legacy_fallback: bool,
    routing: crate::config::track_options::InputRouting,
    clock_active: bool,
    input_envelope: crate::dsp::detector::PeakFollower,
    runtime: InputFxRuntime,
    state: InputFxState,
    metronome_start: Option<Instant>,
    bpm: usize,
    sample_rate: f32,
}

impl InputFxEngine {
    pub fn phrase_views(&self) -> [[crate::dsp::oscillator::PhraseView; 4]; 4] {
        std::array::from_fn(|bank| {
            std::array::from_fn(|slot| {
                self.state
                    .banks
                    .get(bank)
                    .map(|b| b.slots[slot].poly_osc.phrase_view())
                    .unwrap_or_default()
            })
        })
    }
    pub fn set_legacy_fallback(&mut self, enabled: bool) {
        self.legacy_fallback = enabled;
    }
    pub fn new(sample_rate: f32) -> Self {
        Self {
            control_clock: super::pdc::ClockHistory::new(sample_rate),
            pdc_enabled: false,
            track_latencies: [0; 5],
            legacy_fallback: false,
            routing: crate::config::track_options::InputRouting::Legacy,
            clock_active: false,
            input_envelope: crate::dsp::detector::PeakFollower::default(),
            runtime: InputFxRuntime::empty(),
            state: InputFxState::new(sample_rate),
            metronome_start: None,
            bpm: DEFAULT_BPM,
            sample_rate,
        }
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        let changed = self.sample_rate != sample_rate;
        if changed {
            self.control_clock = super::pdc::ClockHistory::new(sample_rate);
        }
        self.sample_rate = sample_rate;
        for bank in &mut self.state.banks {
            for slot in &mut bank.slots {
                if changed {
                    slot.generator_align = super::pdc::AlignDelay::new(sample_rate);
                    slot.modulator_align = super::pdc::AlignDelay::new(sample_rate);
                    slot.carrier_align = super::pdc::AlignDelay::new(sample_rate);
                }
                slot.reverb.prepare(sample_rate);
                slot.audio.prepare(sample_rate);
                slot.roll.prepare(sample_rate);
                slot.my_delay.delay.prepare(sample_rate);
            }
        }
    }
    pub fn set_clock(&mut self, active: bool, bpm: usize) {
        if self.clock_active != active {
            for bank in &mut self.state.banks {
                for slot in &mut bank.slots {
                    slot.trigger = StepTrigger::default();
                }
            }
        }
        self.clock_active = active;
        self.bpm = bpm;
    }
    pub fn set_pdc(&mut self, enabled: bool, track_latencies: [usize; 5]) {
        self.pdc_enabled = enabled;
        self.track_latencies = track_latencies;
    }
    pub fn set_routing(&mut self, routing: crate::config::track_options::InputRouting) {
        self.routing = routing;
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.prepare(sample_rate);
    }

    pub fn update_metronome(&mut self, start: Option<Instant>, bpm: usize) {
        if self.metronome_start != start {
            for bank in &mut self.state.banks {
                for slot in &mut bank.slots {
                    slot.trigger = StepTrigger::default();
                }
            }
        }
        self.clock_active = start.is_some();
        self.metronome_start = start;
        self.bpm = bpm.max(1);
    }

    pub fn swap_runtime(&mut self, runtime: InputFxRuntime) -> InputFxRuntime {
        for (bank_index, bank) in self.state.banks.iter_mut().enumerate() {
            for (slot_index, state) in bank.slots.iter_mut().enumerate() {
                let old = &self.runtime.banks[bank_index].slots[slot_index];
                let new = &runtime.banks[bank_index].slots[slot_index];
                if self.runtime.selected_bank_idx != runtime.selected_bank_idx {
                    state.generator_align.reset();
                    state.modulator_align.reset();
                    state.carrier_align.reset();
                }
                if old.vocoder.map(|v| v.carrier) != new.vocoder.map(|v| v.carrier) {
                    state.modulator_align.reset();
                    state.carrier_align.reset();
                }
                if old.roll.is_some() != new.roll.is_some()
                    || self.runtime.selected_bank_idx != runtime.selected_bank_idx
                {
                    state.roll.reset();
                }
                if old.vocoder.is_some() != new.vocoder.is_some() {
                    state.vocoder = VocoderDspState::new();
                }
                if old.reverb.is_some() != new.reverb.is_some() {
                    state.reverb.reset();
                }
                if old.filter.is_some() != new.filter.is_some() {
                    state.filter_l = FilterDspState::new();
                    state.filter_r = FilterDspState::new();
                }
                if self.runtime.selected_bank_idx != runtime.selected_bank_idx
                    || old.audio.as_ref().map(|v| v.config.kind)
                        != new.audio.as_ref().map(|v| v.config.kind)
                    || (old.enabled
                        && !new.enabled
                        && !(self.pdc_enabled
                            && old
                                .audio
                                .as_ref()
                                .is_some_and(|p| p.latency_frames(self.sample_rate) > 0)))
                {
                    state.audio.reset();
                }
                if old.osc.is_some() != new.osc.is_some() || (old.enabled && !new.enabled) {
                    state.generator_align.reset();
                }
            }
        }
        std::mem::replace(&mut self.runtime, runtime)
    }
    /// Prepared duplicates let existing controls remain live while a structural
    /// graph edit waits for capture to finish. Retired slots go back to the worker.
    pub fn patch_compatible(&mut self, patch: &mut InputFxRuntime) {
        for bank in 0..FX_BANK_COUNT {
            for slot in 0..FX_SLOT_COUNT {
                let current = &mut self.runtime.banks[bank].slots[slot];
                let next = &mut patch.banks[bank].slots[slot];
                if current.compatible(next) {
                    let state = &mut self.state.banks[bank].slots[slot];
                    if current.enabled && !next.enabled {
                        state.generator_align.reset();
                        if !self.pdc_enabled
                            || !current
                                .audio
                                .as_ref()
                                .is_some_and(|p| p.latency_frames(self.sample_rate) > 0)
                        {
                            state.audio.reset();
                        }
                    }
                    std::mem::swap(current, next);
                }
            }
        }
    }

    pub fn process_frame(
        &mut self,
        elapsed_secs: f64,
        input_l: f32,
        input_r: f32,
        track_carriers: &[Option<(f32, f32)>],
    ) -> (f32, f32) {
        self.control_clock.push(
            elapsed_secs,
            elapsed_secs,
            self.sample_rate,
            self.bpm,
            self.clock_active,
        );
        let tick = crate::dsp::oscillator::transport_tick(elapsed_secs, self.sample_rate, self.bpm);
        for (bank, state) in self.runtime.banks.iter().zip(&mut self.state.banks) {
            for (slot, state) in bank.slots.iter().zip(&mut state.slots) {
                if let Some(osc) = &slot.osc {
                    state
                        .poly_osc
                        .advance_phrase(&osc.poly, tick, self.clock_active);
                }
            }
        }
        let input_level = self
            .input_envelope
            .next((input_l.abs() + input_r.abs()) * 0.5, self.sample_rate);
        if self.routing == crate::config::track_options::InputRouting::Legacy {
            self.process_chain(
                elapsed_secs,
                input_l,
                input_r,
                None,
                input_level,
                track_carriers,
                0,
            )
        } else {
            let mut result = (input_l, input_r);
            let mut prefix_latency = 0;
            for slot in 0..FX_SLOT_COUNT {
                result = self.process_chain(
                    elapsed_secs,
                    result.0,
                    result.1,
                    Some(slot),
                    input_level,
                    track_carriers,
                    prefix_latency,
                );
                if self.pdc_enabled {
                    let current = &self.runtime.banks
                        [self.runtime.selected_bank_idx.min(FX_BANK_COUNT - 1)]
                    .slots[slot];
                    if let Some(v) = current.vocoder {
                        if let Some(i) = v.carrier.track_idx() {
                            prefix_latency = prefix_latency.max(self.track_latencies[i]);
                        }
                    }
                    if let Some(audio) = &current.audio {
                        prefix_latency += audio.latency_frames(self.sample_rate);
                    }
                }
            }
            result
        }
    }

    fn process_chain(
        &mut self,
        elapsed_secs: f64,
        input_l: f32,
        input_r: f32,
        only_slot: Option<usize>,
        input_level: f32,
        track_carriers: &[Option<(f32, f32)>],
        prefix_latency: usize,
    ) -> (f32, f32) {
        let mut pipeline_latency = prefix_latency;
        let bank_idx = self.runtime.selected_bank_idx;
        if bank_idx >= FX_BANK_COUNT {
            return (input_l, input_r);
        }

        let bank = &self.runtime.banks[bank_idx];
        let state_bank = &mut self.state.banks[bank_idx];

        let mut osc_mix = 0.0f32;
        let mut active_osc_count = 0usize;

        for idx in 0..FX_SLOT_COUNT {
            let slot = &bank.slots[idx];
            let Some(osc) = slot.osc.as_ref() else {
                continue;
            };
            if only_slot.is_some_and(|only| only != idx) {
                continue;
            }
            state_bank.slots[idx].poly_osc.capture(
                &osc.poly,
                (input_l + input_r) * 0.5,
                self.sample_rate,
                osc.threshold,
            );
            if !slot.enabled {
                state_bank.slots[idx].poly_osc.reset();
                continue;
            }
            let note = if !self.legacy_fallback && (!self.clock_active || osc.note_seq.is_empty()) {
                None
            } else if osc.note_seq.is_empty() {
                osc.note_current
            } else if !self.clock_active {
                osc.note_current
            } else {
                note_at_time(&osc.note_seq, self.bpm, elapsed_secs)
            };
            let note_on =
                if !self.legacy_fallback && (!self.clock_active || osc.note_on_seq.is_empty()) {
                    false
                } else if !self.clock_active || osc.note_on_seq.is_empty() {
                    true
                } else {
                    seq_bool_at_time(&osc.note_on_seq, self.bpm, elapsed_secs)
                };
            let note_retrigger = state_bank.slots[idx].trigger.next(
                &osc.note_trigger_seq,
                self.bpm,
                elapsed_secs,
                self.clock_active,
            );
            let params = OscillatorFxParams {
                waveform: osc.waveform,
                level: osc.level,
                threshold: osc.threshold,
                input_level,
                sample_rate: self.sample_rate,
                note,
                note_on,
                note_retrigger,
                envelope: osc.envelope,
                filter_envelope: osc.osc_filter_envelope,
                filter: FilterParams {
                    filter_type: osc.osc_filter.filter_type,
                    cutoff_hz: osc.osc_filter.cutoff_hz,
                    q: osc.osc_filter.q,
                    drive: osc.osc_filter.drive,
                    mix: osc.osc_filter.mix,
                },
                cutoff_min_hz: FILTER_CUTOFF_MIN_HZ as f32,
            };
            let osc_filtered = if self.legacy_fallback
                && !matches!(osc.waveform, Waveform::Sample | Waveform::Vocal)
            {
                process_osc_fx_sample(&mut state_bank.slots[idx].osc, params)
            } else {
                crate::dsp::oscillator::process_poly_sample(
                    &mut state_bank.slots[idx].poly_osc,
                    &osc.poly,
                    params,
                    elapsed_secs,
                    self.bpm,
                    self.clock_active,
                )
            };
            let osc_filtered = if self.pdc_enabled {
                state_bank.slots[idx]
                    .generator_align
                    .process([osc_filtered; 2], pipeline_latency)[0]
            } else {
                osc_filtered
            };
            osc_mix += osc_filtered;
            active_osc_count += 1;
        }

        if active_osc_count > 1 {
            // Use conservative bus normalization so stacked oscillators do not hit hard clipping.
            osc_mix /= active_osc_count as f32;
        }

        let mut out_l = crate::dsp::headroom(input_l + osc_mix);
        let mut out_r = crate::dsp::headroom(input_r + osc_mix);

        for idx in 0..FX_SLOT_COUNT {
            let slot = &bank.slots[idx];
            if !slot.enabled || only_slot.is_some_and(|only| only != idx) {
                continue;
            }
            let Some(delay) = slot.my_delay.as_ref() else {
                continue;
            };

            let note_on =
                if !self.legacy_fallback && (!self.clock_active || delay.note_on_seq.is_empty()) {
                    false
                } else if !self.clock_active || delay.note_on_seq.is_empty() {
                    true
                } else {
                    seq_bool_at_time(&delay.note_on_seq, self.bpm, elapsed_secs)
                };
            let note_retrigger = state_bank.slots[idx].trigger.next(
                &delay.note_trigger_seq,
                self.bpm,
                elapsed_secs,
                self.clock_active,
            );
            let note = if !self.legacy_fallback && (!self.clock_active || delay.note_seq.is_empty())
            {
                None
            } else if delay.note_seq.is_empty() {
                delay.note_current
            } else if !self.clock_active {
                delay.note_current
            } else {
                note_at_time(&delay.note_seq, self.bpm, elapsed_secs)
            };

            let loop_len_samples = note.map(|n| (self.sample_rate / n.freq_hz()).round() as usize);
            let (filtered_l, filtered_r) = process_mydelay_fx_sample(
                &mut state_bank.slots[idx].my_delay,
                MyDelayFxParams {
                    level: delay.level,
                    threshold: delay.threshold,
                    loop_len_samples,
                    gate_on: note_on,
                    retrigger: note_retrigger,
                    input_mono: (input_l + input_r) * 0.5,
                    sample_rate: self.sample_rate,
                    envelope: delay.audio_env,
                    filter_envelope: delay.filter_env,
                    filter: FilterParams {
                        filter_type: delay.filter.filter_type,
                        cutoff_hz: delay.filter.cutoff_hz,
                        q: delay.filter.q,
                        drive: delay.filter.drive,
                        mix: delay.filter.mix,
                    },
                    cutoff_min_hz: FILTER_CUTOFF_MIN_HZ as f32,
                },
            );

            let generated = if self.pdc_enabled {
                state_bank.slots[idx]
                    .generator_align
                    .process([filtered_l, filtered_r], pipeline_latency)
            } else {
                [filtered_l, filtered_r]
            };
            out_l += generated[0];
            out_r += generated[1];
        }

        for idx in 0..FX_SLOT_COUNT {
            let slot = &bank.slots[idx];
            if (!slot.enabled && !self.pdc_enabled) || only_slot.is_some_and(|only| only != idx) {
                continue;
            }
            let Some(vocoder) = slot.vocoder else {
                continue;
            };
            let carrier_idx = vocoder.carrier.track_idx();
            let mut carrier = carrier_idx
                .and_then(|idx| track_carriers.get(idx))
                .copied()
                .flatten();
            let source_channel = match vocoder.carrier {
                VocoderCarrier::InputLeft => {
                    carrier = Some((input_l, input_l));
                    Some(0)
                }
                VocoderCarrier::InputRight => {
                    carrier = Some((input_r, input_r));
                    Some(1)
                }
                _ => None,
            };
            if self.pdc_enabled {
                let carrier_latency =
                    carrier_idx.map_or(pipeline_latency, |i| self.track_latencies[i]);
                let latency = pipeline_latency.max(carrier_latency);
                let modulator = state_bank.slots[idx]
                    .modulator_align
                    .process([out_l, out_r], latency - pipeline_latency);
                out_l = modulator[0];
                out_r = modulator[1];
                let c = carrier.unwrap_or((0.0, 0.0));
                let aligned = state_bank.slots[idx]
                    .carrier_align
                    .process([c.0, c.1], latency - carrier_latency);
                if carrier.is_some() {
                    carrier = Some((aligned[0], aligned[1]));
                }
                pipeline_latency = latency;
            }
            let modulator_override =
                source_channel.map(|channel| if channel == 0 { out_r } else { out_l });
            let (carrier_l, carrier_r) = carrier.unwrap_or((0.0, 0.0));
            if self.pdc_enabled && self.control_clock.get(pipeline_latency).is_none() {
                continue;
            }
            let wet = process_vocoder_frame(
                &mut state_bank.slots[idx].vocoder,
                VocoderParams {
                    tone: vocoder.tone,
                    mod_sens: vocoder.mod_sens,
                    formant_semitones: vocoder.formant_semitones,
                    sibilance: vocoder.sibilance,
                    modulator_override,
                    mute_carrier_channel: if vocoder.carrier_thru {
                        None
                    } else {
                        source_channel
                    },
                    bands: vocoder.bands,
                    attack_ms: vocoder.attack_ms,
                    release_ms: vocoder.release_ms,
                    level: vocoder.level,
                    mix: vocoder.mix,
                    sample_rate: self.sample_rate,
                    track_carrier_l: carrier_l,
                    track_carrier_r: carrier_r,
                    has_track_carrier: carrier.is_some(),
                },
                out_l,
                out_r,
            );
            if slot.enabled {
                (out_l, out_r) = wet;
            }
        }

        for idx in 0..FX_SLOT_COUNT {
            let slot = &bank.slots[idx];
            if !slot.enabled || only_slot.is_some_and(|only| only != idx) {
                continue;
            }
            let Some(filter) = slot.filter else {
                continue;
            };
            let Some(point) = self.control_clock.get(if self.pdc_enabled {
                pipeline_latency
            } else {
                0
            }) else {
                continue;
            };
            let cutoff_hz = state_bank.slots[idx].filter_sweep.cutoff(
                filter.sweep,
                filter.cutoff_hz,
                if filter.sweep.sync {
                    point.elapsed
                } else {
                    point.stream
                },
                point.bpm,
                point.active,
                self.sample_rate,
            );
            out_l = process_filter_sample(
                &mut state_bank.slots[idx].filter_l,
                FilterParams {
                    filter_type: filter.filter_type,
                    cutoff_hz,
                    q: filter.q,
                    drive: filter.drive,
                    mix: filter.mix,
                },
                self.sample_rate,
                out_l,
            );
            out_r = process_filter_sample(
                &mut state_bank.slots[idx].filter_r,
                FilterParams {
                    filter_type: filter.filter_type,
                    cutoff_hz,
                    q: filter.q,
                    drive: filter.drive,
                    mix: filter.mix,
                },
                self.sample_rate,
                out_r,
            );
        }

        for idx in 0..FX_SLOT_COUNT {
            let slot = &bank.slots[idx];
            if !slot.enabled || only_slot.is_some_and(|only| only != idx) {
                continue;
            }
            let Some(reverb) = slot.reverb else {
                continue;
            };
            if self.pdc_enabled && self.control_clock.get(pipeline_latency).is_none() {
                continue;
            }
            let (wet_l, wet_r) = process_reverb_frame(
                &mut state_bank.slots[idx].reverb,
                ReverbParams {
                    dry_level: reverb.dry_level,
                    wet_level: reverb.wet_level,
                    density: reverb.density,
                    size_ms: reverb.size_ms,
                    rt60_ms: reverb.rt60_ms,
                    predelay_ms: reverb.predelay_ms,
                    width: reverb.width,
                    high_cut_hz: reverb.high_cut_hz,
                    low_cut_hz: reverb.low_cut_hz,
                },
                self.sample_rate,
                out_l,
                out_r,
            );
            out_l = wet_l;
            out_r = wet_r;
        }

        for idx in 0..FX_SLOT_COUNT {
            let slot = &bank.slots[idx];
            if only_slot.is_some_and(|only| only != idx) {
                continue;
            }
            if let Some(audio) = &slot.audio {
                let latency = if self.pdc_enabled {
                    audio.latency_frames(self.sample_rate)
                } else {
                    0
                };
                let point = self.control_clock.get(if self.pdc_enabled {
                    pipeline_latency
                } else {
                    0
                });
                if point.is_none() {
                    pipeline_latency += latency;
                    continue;
                }
                let point = point.unwrap();
                if slot.enabled || latency > 0 {
                    state_bank.slots[idx].audio.set_pdc(self.pdc_enabled);
                    let wet = state_bank.slots[idx].audio.process(
                        audio,
                        point.bpm,
                        point.elapsed as f64 / self.sample_rate as f64,
                        point.active,
                        (out_l, out_r),
                    );
                    (out_l, out_r) = if slot.enabled {
                        wet
                    } else {
                        state_bank.slots[idx].audio.aligned_dry()
                    };
                } else {
                    state_bank.slots[idx]
                        .audio
                        .observe_bypass(audio, (out_l, out_r));
                }
                pipeline_latency += latency;
            }
        }
        // Roll also observes its upstream signal while bypassed. In Serial this
        // is exactly the preceding slot; Legacy deliberately puts Roll last.
        for idx in 0..FX_SLOT_COUNT {
            if only_slot.is_some_and(|only| only != idx) {
                continue;
            }
            let slot = &bank.slots[idx];
            if let Some(roll) = slot.roll {
                let point = self.control_clock.get(if self.pdc_enabled {
                    pipeline_latency
                } else {
                    0
                });
                if point.is_none() {
                    continue;
                }
                (out_l, out_r) = crate::dsp::roll::process_frame(
                    &mut state_bank.slots[idx].roll,
                    roll.params(point.unwrap().bpm),
                    slot.enabled,
                    out_l,
                    out_r,
                );
            }
        }
        (crate::dsp::headroom(out_l), crate::dsp::headroom(out_r))
    }

    pub fn metronome_start(&self) -> Option<Instant> {
        self.metronome_start
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }
}

impl InputFxRuntime {
    pub fn empty() -> Self {
        Self {
            banks: std::array::from_fn(|_| FxBankRuntime::empty()),
            selected_bank_idx: 0,
        }
    }

    pub fn from_config(config: &InputFxConfigs) -> Self {
        let banks = std::array::from_fn(|bank_idx| {
            let bank = &config.banks[bank_idx];
            let slots = std::array::from_fn(|slot_idx| {
                let slot = &bank.slots[slot_idx];
                let (mut osc, filter, reverb, my_delay, vocoder) = match slot.fx.as_ref() {
                    Some(InputFx::Oscillator(osc)) => (
                        Some(OscillatorRuntime {
                            poly: crate::dsp::oscillator::PolyOscRuntime::from_config(osc),
                            waveform: osc.waveform.value,
                            level: (osc.level.value as f32 / 100.0).clamp(0.0, 1.0),
                            note_current: match osc.note.note.value {
                                crate::config::note_configs::Note::N => None,
                                _ => Some(NoteOct {
                                    note: osc.note.note.value,
                                    octave: osc.note.octave.value,
                                }),
                            },
                            note_seq: osc.note.seq().to_vec(),
                            note_on_seq: osc.note.seq().iter().map(|n| n.is_some()).collect(),
                            note_trigger_seq: osc
                                .note
                                .step_len_seq()
                                .iter()
                                .enumerate()
                                .map(|(idx, step_len)| {
                                    *step_len > 0 && osc.note.seq()[idx].is_some()
                                })
                                .collect(),
                            threshold: (osc.threshold.value as f32 / 100.0).clamp(0.0, 1.0),
                            envelope: AhdsrParams {
                                attack_ms: osc.envelope.attack_ms.value.min(ENVELOPE_ATTACK_MAX_MS)
                                    as f32,
                                hold_ms: osc.envelope.hold_ms.value.min(ENVELOPE_HOLD_MAX_MS)
                                    as f32,
                                decay_ms: osc.envelope.decay_ms.value.min(ENVELOPE_DECAY_MAX_MS)
                                    as f32,
                                sustain_level: (osc
                                    .envelope
                                    .sustain_pct
                                    .value
                                    .min(ENVELOPE_SUSTAIN_MAX_PCT)
                                    as f32
                                    / 100.0)
                                    .clamp(0.0, 1.0),
                                release_ms: osc
                                    .envelope
                                    .release_ms
                                    .value
                                    .clamp(ENVELOPE_RELEASE_MIN_MS, ENVELOPE_RELEASE_MAX_MS)
                                    as f32,
                                start_level: (osc
                                    .envelope
                                    .start_pct
                                    .value
                                    .min(ENVELOPE_START_MAX_PCT)
                                    as f32
                                    / 100.0)
                                    .clamp(0.0, 1.0),
                                tension_attack: tension_to_exponent(
                                    osc.envelope.tension_a.value.min(ENVELOPE_TENSION_MAX),
                                ),
                                tension_decay: tension_to_exponent(
                                    osc.envelope.tension_d.value.min(ENVELOPE_TENSION_MAX),
                                ),
                                tension_release: tension_to_exponent(
                                    osc.envelope.tension_r.value.min(ENVELOPE_TENSION_MAX),
                                ),
                            },
                            osc_filter: FilterRuntime {
                                sweep: Default::default(),
                                filter_type: osc.osc_filter.filter_type.value,
                                cutoff_hz: osc
                                    .osc_filter
                                    .cutoff_hz
                                    .value
                                    .clamp(FILTER_CUTOFF_MIN_HZ, FILTER_CUTOFF_MAX_HZ)
                                    as f32,
                                q: (osc
                                    .osc_filter
                                    .resonance_x10
                                    .value
                                    .clamp(FILTER_Q_MIN_X10, FILTER_Q_MAX_X10)
                                    as f32)
                                    / 10.0,
                                drive: (osc.osc_filter.drive.value.min(FILTER_DRIVE_MAX) as f32
                                    / 100.0)
                                    .clamp(0.0, 1.0),
                                mix: (osc.osc_filter.mix.value.min(FILTER_MIX_MAX) as f32 / 100.0)
                                    .clamp(0.0, 1.0),
                            },
                            osc_filter_envelope: AhdsrParams {
                                attack_ms: osc
                                    .osc_filter_env
                                    .attack_ms
                                    .value
                                    .min(ENVELOPE_ATTACK_MAX_MS)
                                    as f32,
                                hold_ms: osc.osc_filter_env.hold_ms.value.min(ENVELOPE_HOLD_MAX_MS)
                                    as f32,
                                decay_ms: osc
                                    .osc_filter_env
                                    .decay_ms
                                    .value
                                    .min(ENVELOPE_DECAY_MAX_MS)
                                    as f32,
                                sustain_level: (osc
                                    .osc_filter_env
                                    .sustain_pct
                                    .value
                                    .min(ENVELOPE_SUSTAIN_MAX_PCT)
                                    as f32
                                    / 100.0)
                                    .clamp(0.0, 1.0),
                                release_ms: osc
                                    .osc_filter_env
                                    .release_ms
                                    .value
                                    .clamp(ENVELOPE_RELEASE_MIN_MS, ENVELOPE_RELEASE_MAX_MS)
                                    as f32,
                                start_level: (osc
                                    .osc_filter_env
                                    .start_pct
                                    .value
                                    .min(ENVELOPE_START_MAX_PCT)
                                    as f32
                                    / 100.0)
                                    .clamp(0.0, 1.0),
                                tension_attack: tension_to_exponent(
                                    osc.osc_filter_env.tension_a.value.min(ENVELOPE_TENSION_MAX),
                                ),
                                tension_decay: tension_to_exponent(
                                    osc.osc_filter_env.tension_d.value.min(ENVELOPE_TENSION_MAX),
                                ),
                                tension_release: tension_to_exponent(
                                    osc.osc_filter_env.tension_r.value.min(ENVELOPE_TENSION_MAX),
                                ),
                            },
                        }),
                        None,
                        None,
                        None,
                        None,
                    ),
                    Some(InputFx::Filter(filter)) => (
                        None,
                        Some(FilterRuntime {
                            sweep: filter.sweep.sanitized(),
                            filter_type: filter.filter_type.value,
                            cutoff_hz: filter
                                .cutoff_hz
                                .value
                                .clamp(FILTER_CUTOFF_MIN_HZ, FILTER_CUTOFF_MAX_HZ)
                                as f32,
                            q: (filter
                                .resonance_x10
                                .value
                                .clamp(FILTER_Q_MIN_X10, FILTER_Q_MAX_X10)
                                as f32)
                                / 10.0,
                            drive: (filter.drive.value.min(FILTER_DRIVE_MAX) as f32 / 100.0)
                                .clamp(0.0, 1.0),
                            mix: (filter.mix.value.min(FILTER_MIX_MAX) as f32 / 100.0)
                                .clamp(0.0, 1.0),
                        }),
                        None,
                        None,
                        None,
                    ),
                    Some(InputFx::Reverb(reverb)) => {
                        let size_pct = (reverb.size.value.min(REVERB_SIZE_MAX) as f32)
                            / REVERB_SIZE_MAX.max(1) as f32;
                        let size_ms = REVERB_SIZE_MIN_MS as f32
                            + (REVERB_SIZE_MAX_MS - REVERB_SIZE_MIN_MS) as f32 * size_pct;
                        let rt60_ms = reverb
                            .decay_ms
                            .value
                            .clamp(REVERB_RT60_MIN_MS, REVERB_RT60_MAX_MS)
                            as f32;
                        let predelay_ms =
                            reverb.predelay_ms.value.min(REVERB_PREDELAY_MAX_MS) as f32;
                        let width = (reverb.width.value.min(REVERB_WIDTH_MAX) as f32 / 100.0)
                            .clamp(0.0, 1.0);
                        let high_cut_hz = reverb.high_cut_hz.value.min(20_000) as f32;
                        let low_cut_hz = reverb
                            .low_cut
                            .value
                            .clamp(REVERB_LOWCUT_MIN_HZ, REVERB_LOWCUT_MAX_HZ)
                            as f32;
                        (
                            None,
                            None,
                            Some(ReverbRuntime {
                                dry_level: reverb.dry_level.value.min(100) as f32 / 100.0,
                                wet_level: reverb.wet_level.value.min(100) as f32 / 100.0,
                                density: reverb.density.value.clamp(1, 10) as f32,
                                size_ms,
                                rt60_ms,
                                predelay_ms,
                                width,
                                high_cut_hz,
                                low_cut_hz,
                            }),
                            None,
                            None,
                        )
                    }
                    Some(InputFx::MyDelay(delay)) => {
                        let level = (delay.level.value.min(MYDELAY_LEVEL_MAX) as f32 / 100.0)
                            .clamp(0.0, 1.0);
                        let threshold = (delay.threshold.value.min(MYDELAY_THRESHOLD_MAX) as f32
                            / 100.0)
                            .clamp(0.0, 1.0);
                        let note_current = match delay.note.note.value {
                            crate::config::note_configs::Note::N => None,
                            _ => Some(NoteOct {
                                note: delay.note.note.value,
                                octave: delay.note.octave.value,
                            }),
                        };
                        let note_seq = delay.note.seq().to_vec();
                        let note_on_seq = delay.note.seq().iter().map(|n| n.is_some()).collect();
                        let note_trigger_seq = delay
                            .note
                            .step_len_seq()
                            .iter()
                            .enumerate()
                            .map(|(idx, step_len)| *step_len > 0 && delay.note.seq()[idx].is_some())
                            .collect();
                        let audio_env = AhdsrParams {
                            attack_ms: delay.audio_env.attack_ms.value.min(ENVELOPE_ATTACK_MAX_MS)
                                as f32,
                            hold_ms: delay.audio_env.hold_ms.value.min(ENVELOPE_HOLD_MAX_MS) as f32,
                            decay_ms: delay.audio_env.decay_ms.value.min(ENVELOPE_DECAY_MAX_MS)
                                as f32,
                            sustain_level: (delay
                                .audio_env
                                .sustain_pct
                                .value
                                .min(ENVELOPE_SUSTAIN_MAX_PCT)
                                as f32
                                / 100.0)
                                .clamp(0.0, 1.0),
                            release_ms: delay
                                .audio_env
                                .release_ms
                                .value
                                .clamp(ENVELOPE_RELEASE_MIN_MS, ENVELOPE_RELEASE_MAX_MS)
                                as f32,
                            start_level: (delay
                                .audio_env
                                .start_pct
                                .value
                                .min(ENVELOPE_START_MAX_PCT)
                                as f32
                                / 100.0)
                                .clamp(0.0, 1.0),
                            tension_attack: tension_to_exponent(
                                delay.audio_env.tension_a.value.min(ENVELOPE_TENSION_MAX),
                            ),
                            tension_decay: tension_to_exponent(
                                delay.audio_env.tension_d.value.min(ENVELOPE_TENSION_MAX),
                            ),
                            tension_release: tension_to_exponent(
                                delay.audio_env.tension_r.value.min(ENVELOPE_TENSION_MAX),
                            ),
                        };
                        let filter_env = AhdsrParams {
                            attack_ms: delay.filter_env.attack_ms.value.min(ENVELOPE_ATTACK_MAX_MS)
                                as f32,
                            hold_ms: delay.filter_env.hold_ms.value.min(ENVELOPE_HOLD_MAX_MS)
                                as f32,
                            decay_ms: delay.filter_env.decay_ms.value.min(ENVELOPE_DECAY_MAX_MS)
                                as f32,
                            sustain_level: (delay
                                .filter_env
                                .sustain_pct
                                .value
                                .min(ENVELOPE_SUSTAIN_MAX_PCT)
                                as f32
                                / 100.0)
                                .clamp(0.0, 1.0),
                            release_ms: delay
                                .filter_env
                                .release_ms
                                .value
                                .clamp(ENVELOPE_RELEASE_MIN_MS, ENVELOPE_RELEASE_MAX_MS)
                                as f32,
                            start_level: (delay
                                .filter_env
                                .start_pct
                                .value
                                .min(ENVELOPE_START_MAX_PCT)
                                as f32
                                / 100.0)
                                .clamp(0.0, 1.0),
                            tension_attack: tension_to_exponent(
                                delay.filter_env.tension_a.value.min(ENVELOPE_TENSION_MAX),
                            ),
                            tension_decay: tension_to_exponent(
                                delay.filter_env.tension_d.value.min(ENVELOPE_TENSION_MAX),
                            ),
                            tension_release: tension_to_exponent(
                                delay.filter_env.tension_r.value.min(ENVELOPE_TENSION_MAX),
                            ),
                        };
                        let filter = FilterRuntime {
                            sweep: Default::default(),
                            filter_type: delay.filter.filter_type.value,
                            cutoff_hz: delay
                                .filter
                                .cutoff_hz
                                .value
                                .clamp(FILTER_CUTOFF_MIN_HZ, FILTER_CUTOFF_MAX_HZ)
                                as f32,
                            q: (delay
                                .filter
                                .resonance_x10
                                .value
                                .clamp(FILTER_Q_MIN_X10, FILTER_Q_MAX_X10)
                                as f32)
                                / 10.0,
                            drive: (delay.filter.drive.value.min(FILTER_DRIVE_MAX) as f32 / 100.0)
                                .clamp(0.0, 1.0),
                            mix: (delay.filter.mix.value.min(FILTER_MIX_MAX) as f32 / 100.0)
                                .clamp(0.0, 1.0),
                        };
                        (
                            None,
                            None,
                            None,
                            Some(MyDelayRuntime {
                                level,
                                threshold,
                                note_current,
                                note_seq,
                                note_on_seq,
                                note_trigger_seq,
                                audio_env,
                                filter_env,
                                filter,
                            }),
                            None,
                        )
                    }
                    Some(InputFx::Vocoder(vocoder)) => (
                        None,
                        None,
                        None,
                        None,
                        Some(VocoderRuntime {
                            tone: vocoder.tone.clamp(-50, 50) as f32,
                            mod_sens: vocoder.mod_sens.clamp(-50, 50) as f32,
                            formant_semitones: vocoder.formant_semitones.clamp(-12, 12) as f32,
                            sibilance: vocoder.sibilance.value.min(100) as f32 / 100.0,
                            carrier_thru: vocoder.carrier_thru,
                            carrier: vocoder.carrier.value,
                            bands: vocoder
                                .bands
                                .value
                                .clamp(VOCODER_BANDS_MIN, VOCODER_BANDS_MAX),
                            attack_ms: vocoder.attack_ms.value.min(VOCODER_ATTACK_MAX_MS) as f32,
                            release_ms: vocoder.release_ms.value.min(VOCODER_RELEASE_MAX_MS) as f32,
                            level: (vocoder.level.value.min(VOCODER_LEVEL_MAX) as f32 / 100.0)
                                .clamp(0.0, 1.0),
                            mix: (vocoder.mix.value.min(VOCODER_MIX_MAX) as f32 / 100.0)
                                .clamp(0.0, 1.0),
                        }),
                    ),
                    _ => (None, None, None, None, None),
                };
                if let Some(osc) = &mut osc {
                    osc.poly.phrase.source = crate::dsp::oscillator::source_key(&slot.source_id);
                }
                FxSlotRuntime {
                    source_key: {
                        use std::hash::{Hash, Hasher};
                        let mut key = std::collections::hash_map::DefaultHasher::new();
                        slot.source_id.hash(&mut key);
                        key.finish()
                    },
                    roll: match &slot.fx {
                        Some(InputFx::Roll(p)) => {
                            Some(super::track_fx::RollRuntime::from_config(p))
                        }
                        _ => None,
                    },
                    audio: match &slot.fx {
                        Some(InputFx::Audio(p)) => {
                            Some(crate::dsp::audio_fx::AudioFxParams::new(p))
                        }
                        _ => None,
                    },
                    enabled: slot.is_enabled,
                    osc,
                    filter,
                    reverb,
                    my_delay,
                    vocoder,
                }
            });
            FxBankRuntime { slots }
        });
        Self {
            banks,
            selected_bank_idx: config.sel_bank_idx,
        }
    }
}

impl FxBankRuntime {
    pub fn empty() -> Self {
        Self {
            slots: std::array::from_fn(|_| FxSlotRuntime {
                source_key: 0,
                roll: None,
                audio: None,
                enabled: false,
                osc: None,
                filter: None,
                reverb: None,
                my_delay: None,
                vocoder: None,
            }),
        }
    }
}

impl InputFxState {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            banks: (0..FX_BANK_COUNT)
                .map(|_| FxBankState::new(sample_rate))
                .collect(),
        }
    }
}

impl FxBankState {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            slots: std::array::from_fn(|slot_index| FxSlotState {
                generator_align: super::pdc::AlignDelay::new(sample_rate),
                modulator_align: super::pdc::AlignDelay::new(sample_rate),
                carrier_align: super::pdc::AlignDelay::new(sample_rate),
                roll: {
                    let mut roll = crate::dsp::roll::RollDspState::new();
                    roll.prepare(sample_rate);
                    roll
                },
                audio: Box::new(crate::dsp::audio_fx::AudioFxState::new_with_stagger(
                    sample_rate,
                    slot_index,
                )),
                trigger: StepTrigger::default(),
                osc: OscillatorFxDspState::new(),
                poly_osc: crate::dsp::oscillator::PolyOscState::new(),
                filter_sweep: Default::default(),
                filter_l: FilterDspState::new(),
                filter_r: FilterDspState::new(),
                reverb: ReverbDspState::new(),
                my_delay: MyDelayFxDspState::new(),
                vocoder: VocoderDspState::new(),
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
    fn stepped_modulation_and_phase_restart_follow_pdc_source_clock() {
        use crate::config::{FxKind, audio_fx::AudioFxKind as K, track_options::InputRouting};
        let sr = 8000.0;
        let latency = crate::dsp::pitch_shift::latency_frames(sr);
        for kind in [K::AutoPan, K::Tremolo, K::Phaser, K::Flanger] {
            let mut c = InputFxConfigs::new();
            c.set_slot_kind(0, 1, FxKind::Audio(kind));
            c.banks[0].slots[1].is_enabled = true;
            if let Some(InputFx::Audio(p)) = &mut c.banks[0].slots[1].fx {
                p.mod_stepped = true;
                p.mod_step_beats = 0.125;
                p.mod_shape = 0.87;
                p.mod_phase_degrees = 90.0;
                p.mod_retrigger = true;
                p.sync_beats = 0.75;
            }
            let mut direct = InputFxEngine::new(sr);
            direct.set_routing(InputRouting::Serial);
            direct.set_pdc(true, [0; 5]);
            direct.swap_runtime(InputFxRuntime::from_config(&c));
            c.set_slot_kind(0, 0, FxKind::Audio(K::Transpose));
            c.banks[0].slots[0].is_enabled = true;
            let mut delayed = InputFxEngine::new(sr);
            delayed.set_routing(InputRouting::Serial);
            delayed.set_pdc(true, [0; 5]);
            delayed.swap_runtime(InputFxRuntime::from_config(&c));
            let mut reference = Vec::with_capacity(12000);
            let count = crate::test_alloc::count(|| {
                for n in 0..12000 {
                    let active = n < 4000 || n >= 6500;
                    let origin = if n >= 6500 { 6500 } else { 0 };
                    let elapsed = if active {
                        (n - origin) as f64 / sr as f64
                    } else {
                        0.0
                    };
                    direct.set_clock(active, 137);
                    delayed.set_clock(active, 137);
                    let x = (n as f32 * 0.17).sin() * 0.03;
                    reference.push(direct.process_frame(elapsed, x, x, &[]));
                    let got = delayed.process_frame(elapsed, x, x, &[]);
                    let expected = if n >= latency {
                        reference[n - latency]
                    } else {
                        (0.0, 0.0)
                    };
                    assert!(
                        (got.0 - expected.0).abs() < 0.0003 && (got.1 - expected.1).abs() < 0.0003,
                        "{kind:?} frame{n}: {got:?}/{expected:?}"
                    );
                }
            });
            assert_eq!(count, 0);
        }
    }
    #[test]
    fn filter_sweep_follows_delayed_source_clock_for_sync_and_free_rate() {
        use crate::config::{FxKind, audio_fx::AudioFxKind, track_options::InputRouting};
        let sr = 8000.0;
        let latency = crate::dsp::pitch_shift::latency_frames(sr);
        for synced in [false, true] {
            let mut c = InputFxConfigs::new();
            c.set_slot_kind(0, 1, FxKind::Filter);
            c.banks[0].slots[1].is_enabled = true;
            if let Some(InputFx::Filter(f)) = &mut c.banks[0].slots[1].fx {
                f.cutoff_hz.value = 800;
                f.sweep.depth = 0.8;
                f.sweep.sync = synced;
                f.sweep.rate_hz = 7.0;
                f.sweep.beats = 0.25;
                f.sweep.stepped = true;
                f.sweep.step_hz = 31.0;
            }
            let mut direct = InputFxEngine::new(sr);
            direct.set_routing(InputRouting::Serial);
            direct.set_pdc(true, [0; 5]);
            direct.set_clock(synced, 137);
            direct.swap_runtime(InputFxRuntime::from_config(&c));
            c.set_slot_kind(0, 0, FxKind::Audio(AudioFxKind::Transpose));
            c.banks[0].slots[0].is_enabled = true;
            let mut delayed = InputFxEngine::new(sr);
            delayed.set_routing(InputRouting::Serial);
            delayed.set_pdc(true, [0; 5]);
            delayed.set_clock(synced, 137);
            delayed.swap_runtime(InputFxRuntime::from_config(&c));
            let mut reference = Vec::with_capacity(8000);
            let allocations = crate::test_alloc::count(|| {
                for n in 0..8000 {
                    let x = (n as f32 * 0.53).sin() * 0.03 + (n as f32 * 0.11).sin() * 0.02;
                    let time = if synced { n as f64 / sr as f64 } else { 0.0 };
                    reference.push(direct.process_frame(time, x, -x, &[]));
                    let got = delayed.process_frame(time, x, -x, &[]);
                    let expected = if n >= latency {
                        reference[n - latency]
                    } else {
                        (0.0, 0.0)
                    };
                    assert!(
                        (got.0 - expected.0).abs() < 0.0003 && (got.1 - expected.1).abs() < 0.0003,
                        "sync={synced},n={n}: {got:?}/{expected:?}"
                    );
                }
            });
            assert_eq!(allocations, 0);
        }
    }
    #[test]
    fn pdc_aligns_a_generator_added_after_a_pitch_stage() {
        use crate::config::sequence_edit::NoteEvent;
        use crate::config::{FxKind, audio_fx::AudioFxKind, track_options::InputRouting};
        let sr = 8000.0;
        let latency = crate::dsp::pitch_shift::latency_frames(sr);
        let mut c = InputFxConfigs::new();
        c.set_slot_kind(0, 1, FxKind::Oscillator);
        c.banks[0].slots[1].is_enabled = true;
        if let Some(InputFx::Oscillator(osc)) = &mut c.banks[0].slots[1].fx {
            osc.waveform.value = Waveform::Sine;
            osc.osc_filter.mix.value = 0;
            osc.note.replace_events(
                3840,
                &[NoteEvent::new(0, 3840, NoteOct::from_pitch_index(48))],
            );
        }
        let mut direct = InputFxEngine::new(sr);
        direct.set_routing(InputRouting::Serial);
        direct.set_pdc(true, [0; 5]);
        direct.set_clock(true, 120);
        direct.swap_runtime(InputFxRuntime::from_config(&c));
        c.set_slot_kind(0, 0, FxKind::Audio(AudioFxKind::Transpose));
        c.banks[0].slots[0].is_enabled = true;
        let mut delayed = InputFxEngine::new(sr);
        delayed.set_routing(InputRouting::Serial);
        delayed.set_pdc(true, [0; 5]);
        delayed.set_clock(true, 120);
        delayed.swap_runtime(InputFxRuntime::from_config(&c));
        let mut reference = Vec::with_capacity(5000);
        let count = crate::test_alloc::count(|| {
            for n in 0..5000 {
                let x = (n as f32 * 0.17).sin() * 0.05;
                reference.push(direct.process_frame(n as f64 / sr as f64, x, x, &[]));
                let got = delayed.process_frame(n as f64 / sr as f64, x, x, &[]);
                let expected = if n >= latency {
                    reference[n - latency]
                } else {
                    (0.0, 0.0)
                };
                assert!(
                    (got.0 - expected.0).abs() < 0.0003 && (got.1 - expected.1).abs() < 0.0003,
                    "{n}: {got:?}/{expected:?}"
                );
            }
        });
        assert_eq!(count, 0);
    }
    #[test]
    fn input_roll_captures_preceding_serial_effects_while_bypassed_without_allocating() {
        use crate::config::{
            FxKind, audio_fx::AudioFxKind, roll_configs::RollStep, time_mode::TimeMode,
            track_options::InputRouting,
        };
        let mut c = InputFxConfigs::new();
        c.set_slot_kind(0, 0, FxKind::Audio(AudioFxKind::Pan));
        c.banks[0].slots[0].is_enabled = true;
        if let Some(InputFx::Audio(p)) = &mut c.banks[0].slots[0].fx {
            p.level_db = -6.0206;
        }
        c.set_slot_kind(0, 1, FxKind::Roll);
        if let Some(InputFx::Roll(p)) = &mut c.banks[0].slots[1].fx {
            p.time_mode.value = TimeMode::Milliseconds;
            p.time_ms.value = 40;
            p.step.value = RollStep::Two;
        }
        let mut engine = InputFxEngine::new(8000.0);
        engine.set_routing(InputRouting::Serial);
        engine.swap_runtime(InputFxRuntime::from_config(&c));
        for n in 0..2000 {
            engine.process_frame(n as f64 / 8000.0, 0.6, -0.2, &[]);
        }
        c.banks[0].slots[1].is_enabled = true;
        let runtime = InputFxRuntime::from_config(&c);
        let mut retired = None;
        assert_eq!(
            crate::test_alloc::count(|| retired = Some(engine.swap_runtime(runtime))),
            0
        );
        let count = crate::test_alloc::count(|| {
            for n in 0..2000 {
                let (l, r) = engine.process_frame(n as f64 / 8000.0, -0.8, 0.4, &[]);
                if n > 1000 {
                    assert!(
                        (l - 0.3).abs() < 0.0001 && (r + 0.1).abs() < 0.0001,
                        "{l}/{r}"
                    );
                }
            }
        });
        assert_eq!(count, 0);
        c.banks[0].slots[1].is_enabled = false;
        engine.swap_runtime(InputFxRuntime::from_config(&c));
        for n in 0..2000 {
            engine.process_frame(n as f64 / 8000.0, -0.8, 0.4, &[]);
        }
        c.banks[0].slots[1].is_enabled = true;
        engine.swap_runtime(InputFxRuntime::from_config(&c));
        for n in 0..2000 {
            let (l, r) = engine.process_frame(n as f64 / 8000.0, 0.6, -0.2, &[]);
            if n > 1000 {
                assert!((l + 0.4).abs() < 0.0001 && (r - 0.2).abs() < 0.0001);
            }
        }
        // Bank/type changes clear the frozen material using a logical reset.
        c.select_bank(1);
        engine.swap_runtime(InputFxRuntime::from_config(&c));
        c.select_bank(0);
        engine.swap_runtime(InputFxRuntime::from_config(&c));
        for n in 0..2000 {
            assert_eq!(
                engine.process_frame(n as f64 / 8000.0, 0.0, 0.0, &[]),
                (0.0, 0.0)
            );
        }
        c.set_slot_kind(0, 1, FxKind::None);
        engine.swap_runtime(InputFxRuntime::from_config(&c));
        c.set_slot_kind(0, 1, FxKind::Roll);
        engine.swap_runtime(InputFxRuntime::from_config(&c));
        for n in 0..2000 {
            assert_eq!(
                engine.process_frame(n as f64 / 8000.0, 0.0, 0.0, &[]),
                (0.0, 0.0)
            );
        }
    }
    #[test]
    fn modern_voice_requires_running_notes_and_legacy_renderer_keeps_fixed_note() {
        let mut config = InputFxConfigs::new();
        config.set_slot_kind(0, 0, crate::config::FxKind::Oscillator);
        config.banks[0].slots[0].is_enabled = true;
        if let Some(InputFx::Oscillator(osc)) = &mut config.banks[0].slots[0].fx {
            osc.threshold.value = 0;
            osc.osc_filter.mix.value = 0;
            osc.envelope.attack_ms.value = 1;
        }
        let mut engine = InputFxEngine::new(8000.0);
        engine.swap_runtime(InputFxRuntime::from_config(&config));
        for active in [false, true] {
            engine.set_clock(active, 120);
            for i in 0..400 {
                assert_eq!(
                    engine.process_frame(i as f64 / 8000.0, 0.0, 0.0, &[]),
                    (0.0, 0.0)
                );
            }
        }
        engine.set_legacy_fallback(true);
        engine.set_clock(false, 120);
        let mut peak = 0.0f32;
        for i in 0..400 {
            peak = peak.max(
                engine
                    .process_frame(i as f64 / 8000.0, 0.0, 0.0, &[])
                    .0
                    .abs(),
            );
        }
        assert!(peak > 0.1);
        if let Some(InputFx::Oscillator(osc)) = &mut config.banks[0].slots[0].fx {
            osc.note.push();
        }
        let mut live = InputFxEngine::new(8000.0);
        live.swap_runtime(InputFxRuntime::from_config(&config));
        live.set_clock(true, 120);
        let mut peak = 0.0f32;
        for i in 0..400 {
            peak = peak.max(live.process_frame(i as f64 / 8000.0, 0.0, 0.0, &[]).0.abs());
        }
        assert!(peak > 0.1);
    }
    #[test]
    fn attack_completes_inside_the_first_sequence_tick() {
        let mut config = InputFxConfigs::new();
        config.set_slot_kind(0, 0, crate::config::FxKind::Oscillator);
        config.banks[0].slots[0].is_enabled = true;
        let Some(InputFx::Oscillator(osc)) = &mut config.banks[0].slots[0].fx else {
            panic!()
        };
        osc.threshold.value = 0;
        osc.envelope.attack_ms.value = 10;
        osc.osc_filter.mix.value = 0;
        osc.note.push();
        let mut engine = InputFxEngine::new(48000.0);
        engine.swap_runtime(InputFxRuntime::from_config(&config));
        engine.update_metronome(Some(Instant::now()), 120);
        let mut peak = 0.0_f32;
        for sample in 0..960 {
            let (value, _) = engine.process_frame(sample as f64 / 48000.0, 0.0, 0.0, &[]);
            if sample > 600 {
                peak = peak.max(value.abs());
            }
        }
        assert!(
            peak > 0.25,
            "attack was held at its first sample (poly headroom + velocity): {peak}"
        );
    }
    #[test]
    fn missing_sample_is_silent_even_when_opening_a_legacy_replay() {
        let mut c = InputFxConfigs::new();
        c.set_slot_kind(0, 0, crate::config::FxKind::Oscillator);
        c.banks[0].slots[0].is_enabled = true;
        let Some(InputFx::Oscillator(osc)) = &mut c.banks[0].slots[0].fx else {
            panic!()
        };
        osc.waveform.value = Waveform::Sample;
        osc.threshold.value = 0;
        osc.note.push();
        for legacy in [false, true] {
            let mut engine = InputFxEngine::new(48000.0);
            engine.swap_runtime(InputFxRuntime::from_config(&c));
            engine.set_legacy_fallback(legacy);
            engine.set_clock(true, 120);
            for frame in 0..4096 {
                assert_eq!(
                    engine.process_frame(frame as f64 / 48000.0, 0.0, 0.0, &[]),
                    (0.0, 0.0)
                );
            }
        }
    }
}
