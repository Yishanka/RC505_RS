// src/dsp/my_delay.rs
use crate::dsp::envelope::{AhdsrParams, AhdsrState};
use crate::dsp::filter::{process_sample as process_filter_sample, FilterDspState, FilterParams};

const RECORD_WINDOW_MS: f32 = 100.0;

#[derive(Clone, Copy)]
pub struct MyDelayParams {
    pub level: f32,
    pub threshold: f32,
    pub loop_len_samples: usize,
}

#[derive(Clone)]
pub struct MyDelayDspState {
    buffer: Vec<f32>,
    write_idx: usize,
    play_idx: usize,
    recording: bool,
    ready: bool,
    record_len_samples: usize,
    prev_above: bool,
    last_loop_len_samples: Option<usize>,
}

#[derive(Clone)]
pub struct MyDelayFxDspState {
    pub delay: MyDelayDspState,
    pub env: AhdsrState,
    pub filter_l: FilterDspState,
    pub filter_r: FilterDspState,
    pub filter_env: AhdsrState,
}

impl MyDelayFxDspState {
    pub fn new() -> Self {
        Self {
            delay: MyDelayDspState::new(),
            env: AhdsrState::new(),
            filter_l: FilterDspState::new(),
            filter_r: FilterDspState::new(),
            filter_env: AhdsrState::new(),
        }
    }
}

#[derive(Clone, Copy)]
pub struct MyDelayFxParams {
    pub level: f32,
    pub threshold: f32,
    pub loop_len_samples: Option<usize>,
    pub gate_on: bool,
    pub retrigger: bool,
    pub input_mono: f32,
    pub sample_rate: f32,
    pub envelope: AhdsrParams,
    pub filter_envelope: AhdsrParams,
    pub filter: FilterParams,
    pub cutoff_min_hz: f32,
}

impl MyDelayDspState {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            write_idx: 0,
            play_idx: 0,
            recording: false,
            ready: false,
            record_len_samples: 0,
            prev_above: false,
            last_loop_len_samples: None,
        }
    }

    // pub fn reset(&mut self) {
    //     self.buffer.clear();
    //     self.write_idx = 0;
    //     self.play_idx = 0;
    //     self.recording = false;
    //     self.ready = false;
    //     self.prev_above = false;
    //     self.record_len_samples = 0;
    // }

    pub fn clear_gate(&mut self) {
        self.prev_above = false;
    }

    pub fn resolve_loop_len(&mut self, current_loop_len: Option<usize>, gate_on: bool) -> Option<usize> {
        if let Some(loop_len) = current_loop_len {
            let clamped = loop_len.max(2);
            self.last_loop_len_samples = Some(clamped);
            return Some(clamped);
        }
        if !gate_on {
            return self.last_loop_len_samples;
        }
        None
    }

    pub fn clear_loop_len_memory(&mut self) {
        self.last_loop_len_samples = None;
    }
}

pub fn process_sample(
    state: &mut MyDelayDspState,
    p: MyDelayParams,
    sample_rate: f32,
    input: f32,
) -> f32 {
    let sr = sample_rate.max(1.0);
    let record_len_samples = (RECORD_WINDOW_MS * sr / 1000.0).round() as usize;
    let record_len_samples = record_len_samples.max(2);

    if state.record_len_samples != record_len_samples {
        state.record_len_samples = record_len_samples;
        if state.buffer.len() != record_len_samples {
            state.buffer = vec![0.0; record_len_samples];
            state.write_idx = 0;
            state.play_idx = 0;
            state.ready = false;
            state.recording = false;
        }
    }

    let above = input.abs() >= p.threshold.max(0.0);
    let rising = above && !state.prev_above;
    state.prev_above = above;

    if rising {
        state.recording = true;
        state.ready = false;
        state.write_idx = 0;
    }

    if state.recording {
        if state.write_idx < state.buffer.len() {
            state.buffer[state.write_idx] = input;
            state.write_idx += 1;
        }
        if state.write_idx >= state.buffer.len() {
            state.recording = false;
            state.ready = true;
            state.play_idx = 0;
        }
        return 0.0;
    }

    if state.ready && !state.buffer.is_empty() {
        let loop_len = p.loop_len_samples.max(2).min(state.buffer.len());
        if state.play_idx >= loop_len {
            state.play_idx = 0;
        }
        let idx = state.play_idx;
        state.play_idx += 1;

        let mut out = state.buffer[idx];
        out *= loop_window_gain(idx, loop_len);
        return out * p.level.clamp(0.0, 1.0);
    }

    0.0
}

pub fn process_fx_frame(state: &mut MyDelayFxDspState, p: MyDelayFxParams) -> (f32, f32) {
    if !p.gate_on {
        state.delay.clear_gate();
    }

    let loop_len_for_sample = state.delay.resolve_loop_len(p.loop_len_samples, p.gate_on);
    let input_mono = if p.gate_on { p.input_mono } else { 0.0 };
    let delay_out = if let Some(loop_len_samples) = loop_len_for_sample {
        process_sample(
            &mut state.delay,
            MyDelayParams {
                level: p.level,
                threshold: p.threshold,
                loop_len_samples,
            },
            p.sample_rate,
            input_mono,
        )
    } else {
        0.0
    };

    let dt = 1.0 / p.sample_rate.max(1.0);
    let amp = state
        .env
        .next(p.gate_on, p.retrigger, p.envelope, dt)
        .clamp(0.0, 1.0);
    if !p.gate_on && amp <= 0.0001 {
        state.delay.clear_loop_len_memory();
    }
    let delay_out = delay_out * amp;

    let cutoff_env = state
        .filter_env
        .next(p.gate_on, p.retrigger, p.filter_envelope, dt)
        .clamp(0.0, 1.0);
    let cutoff_max = p.filter.cutoff_hz.max(p.cutoff_min_hz);
    let cutoff_hz = p.cutoff_min_hz + (cutoff_max - p.cutoff_min_hz) * cutoff_env;
    let filter_params = FilterParams {
        cutoff_hz,
        ..p.filter
    };
    let filtered_l = process_filter_sample(&mut state.filter_l, filter_params, p.sample_rate, delay_out);
    let filtered_r = process_filter_sample(&mut state.filter_r, filter_params, p.sample_rate, delay_out);
    (filtered_l, filtered_r)
}

fn loop_window_gain(idx: usize, len: usize) -> f32 {
    if len < 4 {
        return 1.0;
    }
    let fade_len = ((len as f32) * 0.1).round() as usize;
    let fade_len = fade_len.clamp(4, 128).min(len / 2);
    if fade_len == 0 {
        return 1.0;
    }
    if idx < fade_len {
        idx as f32 / fade_len as f32
    } else if idx >= len - fade_len {
        (len - 1 - idx) as f32 / fade_len as f32
    } else {
        1.0
    }
}
