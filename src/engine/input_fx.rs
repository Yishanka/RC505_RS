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

#[derive(Clone)]
pub struct FxSlotRuntime {
    pub enabled: bool,
    pub osc: Option<OscillatorRuntime>,
    pub filter: Option<FilterRuntime>,
    pub reverb: Option<ReverbRuntime>,
    pub my_delay: Option<MyDelayRuntime>,
    pub vocoder: Option<VocoderRuntime>,
}

#[derive(Clone)]
pub struct FxSlotState {
    pub trigger: StepTrigger,
    pub osc: OscillatorFxDspState,
    pub filter_l: FilterDspState,
    pub filter_r: FilterDspState,
    pub reverb: ReverbDspState,
    pub my_delay: MyDelayFxDspState,
    pub vocoder: VocoderDspState,
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
    pub banks: [FxBankState; FX_BANK_COUNT],
}

pub struct InputFxEngine {
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
    pub fn new(sample_rate: f32) -> Self {
        Self {
            routing: crate::config::track_options::InputRouting::Legacy,
            clock_active: false,
            input_envelope: crate::dsp::detector::PeakFollower::default(),
            runtime: InputFxRuntime::empty(),
            state: InputFxState::new(),
            metronome_start: None,
            bpm: DEFAULT_BPM,
            sample_rate,
        }
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        for bank in &mut self.state.banks {
            for slot in &mut bank.slots {
                slot.reverb.prepare(sample_rate);
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
    pub fn set_routing(&mut self, routing: crate::config::track_options::InputRouting) {
        self.routing = routing;
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
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
        std::mem::replace(&mut self.runtime, runtime)
    }

    pub fn process_frame(
        &mut self,
        elapsed_secs: f64,
        input_l: f32,
        input_r: f32,
        track_carriers: &[Option<(f32, f32)>],
    ) -> (f32, f32) {
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
            )
        } else {
            let mut result = (input_l, input_r);
            for slot in 0..FX_SLOT_COUNT {
                result = self.process_chain(
                    elapsed_secs,
                    result.0,
                    result.1,
                    Some(slot),
                    input_level,
                    track_carriers,
                );
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
    ) -> (f32, f32) {
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
            if !slot.enabled || only_slot.is_some_and(|only| only != idx) {
                continue;
            }
            let Some(osc) = slot.osc.as_ref() else {
                continue;
            };
            let note = if osc.note_seq.is_empty() {
                osc.note_current
            } else if !self.clock_active {
                osc.note_current
            } else {
                note_at_time(&osc.note_seq, self.bpm, elapsed_secs)
            };
            let note_on = if !self.clock_active || osc.note_on_seq.is_empty() {
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
            let osc_filtered = process_osc_fx_sample(
                &mut state_bank.slots[idx].osc,
                OscillatorFxParams {
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
                },
            );
            osc_mix += osc_filtered;
            active_osc_count += 1;
        }

        if active_osc_count > 1 {
            // Use conservative bus normalization so stacked oscillators do not hit hard clipping.
            osc_mix /= active_osc_count as f32;
        }

        let mut out_l = (input_l + osc_mix).clamp(-1.0, 1.0);
        let mut out_r = (input_r + osc_mix).clamp(-1.0, 1.0);

        for idx in 0..FX_SLOT_COUNT {
            let slot = &bank.slots[idx];
            if !slot.enabled || only_slot.is_some_and(|only| only != idx) {
                continue;
            }
            let Some(delay) = slot.my_delay.as_ref() else {
                continue;
            };

            let note_on = if !self.clock_active || delay.note_on_seq.is_empty() {
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
            let note = if delay.note_seq.is_empty() {
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

            out_l += filtered_l;
            out_r += filtered_r;
        }

        for idx in 0..FX_SLOT_COUNT {
            let slot = &bank.slots[idx];
            if !slot.enabled || only_slot.is_some_and(|only| only != idx) {
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
            let modulator_override =
                source_channel.map(|channel| if channel == 0 { out_r } else { out_l });
            let (carrier_l, carrier_r) = carrier.unwrap_or((0.0, 0.0));
            (out_l, out_r) = process_vocoder_frame(
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
        }

        for idx in 0..FX_SLOT_COUNT {
            let slot = &bank.slots[idx];
            if !slot.enabled || only_slot.is_some_and(|only| only != idx) {
                continue;
            }
            let Some(filter) = slot.filter else {
                continue;
            };
            out_l = process_filter_sample(
                &mut state_bank.slots[idx].filter_l,
                FilterParams {
                    filter_type: filter.filter_type,
                    cutoff_hz: filter.cutoff_hz,
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
                    cutoff_hz: filter.cutoff_hz,
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

        (out_l.clamp(-1.0, 1.0), out_r.clamp(-1.0, 1.0))
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
                let (osc, filter, reverb, my_delay, vocoder) = match slot.fx.as_ref() {
                    Some(InputFx::Oscillator(osc)) => (
                        Some(OscillatorRuntime {
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
                        let high_cut_hz = reverb.high_cut_hz.value.clamp(200, 20_000) as f32;
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
                FxSlotRuntime {
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
    pub fn new() -> Self {
        Self {
            banks: std::array::from_fn(|_| FxBankState::new()),
        }
    }
}

impl FxBankState {
    pub fn new() -> Self {
        Self {
            slots: std::array::from_fn(|_| FxSlotState {
                trigger: StepTrigger::default(),
                osc: OscillatorFxDspState::new(),
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
        assert!(peak > 0.8, "attack was held at its first sample: {peak}");
    }
}
