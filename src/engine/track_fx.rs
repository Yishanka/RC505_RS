use std::time::Instant;

use crate::config::delay_configs::{
    TRACK_DELAY_DAMP_MAX_HZ, TRACK_DELAY_DAMP_MIN_HZ, TRACK_DELAY_FEEDBACK_MAX_PCT,
    TRACK_DELAY_MIX_MAX_PCT, TRACK_DELAY_TIME_MAX_MS, TRACK_DELAY_TIME_MIN_MS,
};
use crate::config::envelope_configs::{
    ENVELOPE_ATTACK_MAX_MS, ENVELOPE_DECAY_MAX_MS, ENVELOPE_HOLD_MAX_MS,
    ENVELOPE_RELEASE_MAX_MS, ENVELOPE_RELEASE_MIN_MS, ENVELOPE_START_MAX_PCT, ENVELOPE_SUSTAIN_MAX_PCT,
    ENVELOPE_TENSION_MAX,
};
use crate::config::filter_configs::{
    FILTER_CUTOFF_MAX_HZ, FILTER_CUTOFF_MIN_HZ, FILTER_DRIVE_MAX, FILTER_MIX_MAX, FILTER_Q_MAX_X10,
    FILTER_Q_MIN_X10, FilterType,
};
use crate::config::track_fx_configs::{
    TRACK_FX_BANK_COUNT, TRACK_FX_SLOT_COUNT, TrackFx, TrackFxConfig,
};
use crate::dsp::envelope::{AhdsrParams, AhdsrState};
use crate::dsp::filter::{process_sample as process_filter_sample, FilterDspState, FilterParams};
use crate::dsp::note::seq_bool_at_time;
use crate::dsp::delay::{process_frame as process_delay_frame, DelayDspState, DelayParams};
use crate::dsp::roll::{process_sample as process_roll_sample, RollDspState, RollParams};

const DEFAULT_BPM: usize = 120;

#[derive(Clone, Copy)]
pub struct DelayRuntime {
    pub time_ms: f32,
    pub feedback: f32,
    pub high_damp_hz: f32,
    pub mix: f32,
}

#[derive(Clone, Copy)]
pub struct RollRuntime {
    pub step: usize,
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
    bpm: usize,
    sample_rate: f32,
}

#[derive(Clone, Copy)]
pub struct TrackFilterDspState {
    pub env: AhdsrState,
    pub filter_l: FilterDspState,
    pub filter_r: FilterDspState,
}

impl TrackFilterDspState {
    pub fn new() -> Self {
        Self {
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
            bpm: DEFAULT_BPM,
            sample_rate: sr,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
        for track in &mut self.state.tracks {
            for bank in &mut track.banks {
                for slot in &mut bank.slots {
                    slot.delay.set_sample_rate(self.sample_rate);
                }
            }
        }
    }

    pub fn update_metronome(&mut self, start: Option<Instant>, bpm: usize) {
        self.metronome_start = start;
        self.bpm = bpm.max(1);
    }

    pub fn update_from_config(&mut self, config: &TrackFxConfig) {
        let track_count = config.tracks.len();
        self.runtime = TrackFxRuntime::from_config(config);
        if self.state.tracks.len() != track_count {
            self.state = TrackFxState::new(track_count, self.sample_rate);
        }
    }

    pub fn process_frame(
        &mut self,
        track_idx: usize,
        elapsed_secs: f64,
        input_l: f32,
        input_r: f32,
        track_buffer: &[f32],
        play_cursor: usize,
        channels: usize,
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
            if !track_enabled[bank_idx][idx] {
                continue;
            }

            if let Some(delay) = slot.delay {
                let (l, r) = process_delay_frame(
                    &mut bank_state.slots[idx].delay,
                    DelayParams {
                        time_ms: delay.time_ms,
                        feedback: delay.feedback,
                        high_damp_hz: delay.high_damp_hz,
                        mix: delay.mix,
                    },
                    self.sample_rate,
                    out_l,
                    out_r,
                );
                out_l = l;
                out_r = r;
            }

            if let Some(roll) = slot.roll {
                let _ = elapsed_secs; // Reserved for future beat-quantized roll scheduling.
                let roll_l = process_roll_sample(
                    &mut bank_state.slots[idx].roll,
                    RollParams { step: roll.step },
                    track_buffer,
                    play_cursor,
                    channels,
                    self.sample_rate,
                    self.bpm,
                    out_l,
                );
                let roll_r = if channels > 1 {
                    process_roll_sample(
                        &mut bank_state.slots[idx].roll,
                        RollParams { step: roll.step },
                        track_buffer,
                        play_cursor.saturating_add(1),
                        channels,
                        self.sample_rate,
                        self.bpm,
                        out_r,
                    )
                } else {
                    roll_l
                };
                out_l = roll_l;
                out_r = roll_r;
            }

            if let Some(filter) = slot.filter.as_ref() {
                let gate_on = if self.metronome_start.is_none() || filter.seq.is_empty() {
                    true
                } else {
                    seq_bool_at_time(&filter.seq, self.bpm, elapsed_secs)
                };
                let retrigger = if self.metronome_start.is_none() || filter.trigger_seq.is_empty() {
                    false
                } else {
                    seq_bool_at_time(&filter.trigger_seq, self.bpm, elapsed_secs)
                };
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

    pub fn metronome_start(&self) -> Option<Instant> {
        self.metronome_start
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
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

    pub fn from_config(config: &TrackFxConfig) -> Self {
        let banks = std::array::from_fn(|bank_idx| {
            let bank = &config.banks[bank_idx];
            let slots = std::array::from_fn(|slot_idx| {
                let slot = &bank.slots[slot_idx];
                let (delay, roll, filter) = match slot.fx.as_ref() {
                    Some(TrackFx::Delay(delay)) => (
                        Some(DelayRuntime {
                            time_ms: delay
                                .time_ms
                                .value
                                .clamp(TRACK_DELAY_TIME_MIN_MS, TRACK_DELAY_TIME_MAX_MS)
                                as f32,
                            feedback: (delay.feedback_pct.value.min(TRACK_DELAY_FEEDBACK_MAX_PCT) as f32 / 100.0)
                                .clamp(0.0, 0.95),
                            high_damp_hz: delay
                                .high_damp_hz
                                .value
                                .clamp(TRACK_DELAY_DAMP_MIN_HZ, TRACK_DELAY_DAMP_MAX_HZ)
                                as f32,
                            mix: (delay.mix_pct.value.min(TRACK_DELAY_MIX_MAX_PCT) as f32 / 100.0)
                                .clamp(0.0, 1.0),
                        }),
                        None,
                        None,
                    ),
                    Some(TrackFx::Roll(roll)) => (
                        None,
                        Some(RollRuntime {
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
                                .clamp(FILTER_CUTOFF_MIN_HZ, FILTER_CUTOFF_MAX_HZ) as f32,
                            q: (filter
                                .filter
                                .resonance_x10
                                .value
                                .clamp(FILTER_Q_MIN_X10, FILTER_Q_MAX_X10) as f32)
                                / 10.0,
                            drive: (filter.filter.drive.value.min(FILTER_DRIVE_MAX) as f32 / 100.0)
                                .clamp(0.0, 1.0),
                            mix: (filter.filter.mix.value.min(FILTER_MIX_MAX) as f32 / 100.0)
                                .clamp(0.0, 1.0),
                            envelope: AhdsrParams {
                                attack_ms: filter.env.attack_ms.value.min(ENVELOPE_ATTACK_MAX_MS) as f32,
                                hold_ms: filter.env.hold_ms.value.min(ENVELOPE_HOLD_MAX_MS) as f32,
                                decay_ms: filter.env.decay_ms.value.min(ENVELOPE_DECAY_MAX_MS) as f32,
                                sustain_level: (filter.env.sustain_pct.value.min(ENVELOPE_SUSTAIN_MAX_PCT) as f32
                                    / 100.0)
                                    .clamp(0.0, 1.0),
                                release_ms: filter
                                    .env
                                    .release_ms
                                    .value
                                    .clamp(ENVELOPE_RELEASE_MIN_MS, ENVELOPE_RELEASE_MAX_MS)
                                    as f32,
                                start_level: (filter.env.start_pct.value.min(ENVELOPE_START_MAX_PCT) as f32 / 100.0)
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
                                    *step_len > 0 && filter.seq.seq().get(idx).copied().unwrap_or(false)
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

        let track_enabled = config
            .tracks
            .iter()
            .map(|track| track.enabled)
            .collect();

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
