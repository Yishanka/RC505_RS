//! Stereo rolling capture with a frozen slice, subdivisions and two release
//! models. Buffers are prepared by the control thread, never in process_frame.
use crate::config::roll_configs::RollMode;
#[derive(Clone, Copy)]
pub struct RollParams {
    pub step: usize,
    pub time_ms: f32,
    pub mode: RollMode,
    pub feedback: f32,
    pub repeat: usize,
    pub mix: f32,
}
#[derive(Clone, Default)]
pub struct RollDspState {
    buffer: Vec<[f32; 2]>,
    write: usize,
    filled: usize,
    anchor: usize,
    phase: usize,
    length: usize,
    cycles: usize,
    was_enabled: bool,
    frozen: bool,
    mix: f32,
    sample_rate: f32,
    fade_coefficient: f32,
}
impl RollDspState {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn prepare(&mut self, sr: f32) {
        if self.sample_rate != sr || self.buffer.is_empty() {
            *self = Self {
                buffer: vec![[0.0; 2]; (sr * 2.0).ceil().max(2.0) as usize],
                sample_rate: sr,
                fade_coefficient: 1.0 - (-1.0 / (0.005 * sr.max(1.0))).exp(),
                ..Self::default()
            };
        }
    }
    pub fn reset(&mut self) {
        self.was_enabled = false;
        self.frozen = false;
        self.mix = 0.0;
        self.filled = 0;
    }
}
pub fn process_frame(
    state: &mut RollDspState,
    p: RollParams,
    enabled: bool,
    input_l: f32,
    input_r: f32,
) -> (f32, f32) {
    if state.buffer.is_empty() {
        return (input_l, input_r);
    }
    let step = match p.step {
        1 | 2 | 4 | 8 | 16 => p.step,
        _ => 4,
    };
    let length = (p.time_ms.clamp(1.0, 2000.0) * 0.001 * state.sample_rate / step as f32)
        .round()
        .max(1.0) as usize;
    let length = length.min(state.buffer.len());
    if enabled && state.was_enabled && state.frozen && state.length != length {
        // A different slice length needs fresh material, not stale frozen history.
        state.frozen = false;
        state.filled = 0;
        state.mix = 0.0;
    }
    if !enabled || (!state.frozen && state.filled < length) {
        state.buffer[state.write] = [input_l, input_r];
        state.write = (state.write + 1) % state.buffer.len();
        state.filled = (state.filled + 1).min(state.buffer.len());
    }
    if enabled
        && (!state.was_enabled || state.length != length || !state.frozen)
        && state.filled >= length
    {
        state.anchor = (state.write + state.buffer.len() - length) % state.buffer.len();
        state.length = length;
        state.phase = 0;
        state.cycles = 0;
        state.frozen = true;
    }
    let feedback = p.feedback.clamp(0.0, 1.0);
    let gain = if step == 1 && p.mode == RollMode::Roll1 {
        feedback.powf(state.cycles as f32)
    } else {
        1.0
    };
    let finished = step == 1
        && match p.mode {
            RollMode::Roll1 => gain < 0.001,
            RollMode::Roll2 => p.repeat > 0 && state.cycles >= p.repeat,
        };
    let target = if enabled && state.frozen && !finished {
        p.mix.clamp(0.0, 1.0)
    } else {
        0.0
    };
    state.mix += (target - state.mix) * state.fade_coefficient;
    let mut wet = [input_l, input_r];
    if state.frozen {
        let fade = ((state.sample_rate * 0.002).round() as usize).min(state.length / 4);
        let phase = state.phase;
        wet = state.buffer[(state.anchor + phase) % state.buffer.len()];
        if fade > 0 && phase < fade {
            let tail =
                state.buffer[(state.anchor + state.length - fade + phase) % state.buffer.len()];
            let t = phase as f32 / fade as f32;
            wet = [
                tail[0] * (1.0 - t) + wet[0] * t,
                tail[1] * (1.0 - t) + wet[1] * t,
            ];
        }
        state.phase = (phase + 1) % state.length;
        if state.phase == 0 {
            state.cycles = state.cycles.saturating_add(1);
        }
    }
    state.was_enabled = enabled;
    if !enabled && state.mix < 1e-5 {
        state.frozen = false;
    }
    let mix = if p.mix <= 0.0 { 0.0 } else { state.mix };
    (
        input_l * (1.0 - mix) + wet[0] * gain * mix,
        input_r * (1.0 - mix) + wet[1] * gain * mix,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    fn params() -> RollParams {
        RollParams {
            step: 4,
            time_ms: 200.0,
            mode: RollMode::Roll2,
            feedback: 0.5,
            repeat: 0,
            mix: 1.0,
        }
    }
    #[test]
    fn freezes_processed_stereo_history_without_channel_crosstalk() {
        let mut state = RollDspState::new();
        state.prepare(1000.0);
        let p = params();
        for _ in 0..200 {
            process_frame(&mut state, p, false, 0.2, -0.4);
        }
        for i in 0..200 {
            let (left, right) = process_frame(&mut state, p, true, 0.9, 0.7);
            if i > 100 {
                assert!((left - 0.2).abs() < 1e-5 && (right + 0.4).abs() < 1e-5);
            }
            if i > 100 {
                assert_eq!(state.length, 50);
            }
        }
        let (left, right) = process_frame(&mut state, RollParams { mix: 0.0, ..p }, true, 0.9, 0.7);
        assert_eq!((left, right), (0.9, 0.7));
    }
    #[test]
    fn finite_roll2_returns_to_dry_after_requested_cycles() {
        let mut state = RollDspState::new();
        state.prepare(1000.0);
        let p = RollParams {
            step: 1,
            repeat: 2,
            time_ms: 20.0,
            ..params()
        };
        for _ in 0..100 {
            process_frame(&mut state, p, false, 0.3, -0.3);
        }
        let mut output = (0.0, 0.0);
        for _ in 0..200 {
            output = process_frame(&mut state, p, true, 0.0, 0.0);
        }
        assert!(output.0.abs() < 1e-5 && output.1.abs() < 1e-5);
    }
}
