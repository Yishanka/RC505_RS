// src/dsp/my_delay.rs

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
