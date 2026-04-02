use std::time::Instant;

use crate::config::track_delay_configs::{
    TRACK_DELAY_DAMP_MAX_HZ, TRACK_DELAY_DAMP_MIN_HZ, TRACK_DELAY_FEEDBACK_MAX_PCT,
    TRACK_DELAY_MIX_MAX_PCT, TRACK_DELAY_TIME_MAX_MS, TRACK_DELAY_TIME_MIN_MS,
};
use crate::config::track_fx_configs::{
    TRACK_FX_BANK_COUNT, TRACK_FX_SLOT_COUNT, TrackFx, TrackFxConfig,
};
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
pub struct TrackFxSlotRuntime {
    pub delay: Option<DelayRuntime>,
    pub roll: Option<RollRuntime>,
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
                let (delay, roll) = match slot.fx.as_ref() {
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
                    ),
                    Some(TrackFx::Roll(roll)) => (
                        None,
                        Some(RollRuntime {
                            step: roll.step.value.value(),
                        }),
                    ),
                    None => (None, None),
                };
                TrackFxSlotRuntime {
                    delay,
                    roll,
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
            }),
        }
    }
}
