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
    base_length: usize,
    capture_end: usize,
    last_wet: [f32; 2],
    transition_from: [f32; 2],
    transition_left: usize,
    transition_length: usize,
    cycles: usize,
    was_enabled: bool,
    frozen: bool,
    mix: f32,
    sample_rate: f32,
    fade_coefficient: f32,
    feedback_key: Option<(u32, usize)>,
    feedback_gain: f32,
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
        self.feedback_key = None;
        self.base_length = 0;
        self.transition_left = 0;
        self.last_wet = [0.0; 2];
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
    let base_samples = p.time_ms.clamp(1.0, 2000.0) * 0.001 * state.sample_rate;
    let base_length = (base_samples.round().max(1.0) as usize).min(state.buffer.len());
    let length = ((base_samples / step as f32).round().max(1.0) as usize).min(base_length);
    if enabled && state.was_enabled && state.frozen && state.base_length != base_length {
        // Changing the base TIME requests a new capture. Division changes only
        // reinterpret the already frozen base window and never record new input.
        state.frozen = false;
        state.filled = 0;
        state.mix = 0.0;
        state.transition_left = 0;
    }
    if !enabled || (!state.frozen && state.filled < base_length) {
        state.buffer[state.write] = [input_l, input_r];
        state.write = (state.write + 1) % state.buffer.len();
        state.filled = (state.filled + 1).min(state.buffer.len());
    }
    if enabled && (!state.was_enabled || !state.frozen) && state.filled >= base_length {
        state.capture_end = state.write;
        state.base_length = base_length;
        state.anchor = (state.capture_end + state.buffer.len() - length) % state.buffer.len();
        state.length = length;
        state.phase = 0;
        state.cycles = 0;
        state.frozen = true;
        state.transition_left = 0;
        state.feedback_key = None;
    } else if enabled && state.frozen && state.length != length {
        state.transition_from = state.last_wet;
        state.transition_length = ((state.sample_rate * 0.002).round() as usize).min(length / 2);
        state.transition_left = state.transition_length;
        state.phase = ((state.phase as u64 * length as u64) / state.length.max(1) as u64) as usize;
        state.phase = state.phase.min(length - 1);
        state.anchor = (state.capture_end + state.buffer.len() - length) % state.buffer.len();
        state.length = length;
        state.cycles = 0;
        state.feedback_key = None;
    }
    let feedback = p.feedback.clamp(0.0, 1.0);
    let gain = if step == 1 && p.mode == RollMode::Roll1 {
        // Only the repeat boundary or an actual feedback change alters this
        // gain; avoid a transcendental operation for every stereo sample.
        let key = (feedback.to_bits(), state.cycles);
        if state.feedback_key != Some(key) {
            state.feedback_gain = feedback.powf(state.cycles as f32);
            state.feedback_key = Some(key);
        }
        state.feedback_gain
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
        if state.transition_left > 0 {
            let blend = 1.0 - state.transition_left as f32 / state.transition_length.max(1) as f32;
            wet = [
                state.transition_from[0] + (wet[0] - state.transition_from[0]) * blend,
                state.transition_from[1] + (wet[1] - state.transition_from[1]) * blend,
            ];
            state.transition_left -= 1;
        }
        state.phase = (phase + 1) % state.length;
        if state.phase == 0 {
            state.cycles = state.cycles.saturating_add(1);
        }
    }
    state.last_wet = wet;
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
    #[test]
    fn feedback_changes_apply_inside_cycle_and_tiny_slices_remain_bounded() {
        let mut state = RollDspState::new();
        state.prepare(48_000.0);
        let mut p = RollParams {
            step: 1,
            time_ms: 50.0,
            mode: RollMode::Roll1,
            feedback: 1.0,
            ..params()
        };
        for _ in 0..4800 {
            process_frame(&mut state, p, false, 0.4, -0.2);
        }
        for _ in 0..5000 {
            process_frame(&mut state, p, true, 0.0, 0.0);
        }
        assert_eq!(state.cycles, 2);
        p.feedback = 0.5;
        let (l, r) = process_frame(&mut state, p, true, 0.0, 0.0);
        assert!((l - 0.1).abs() < 1e-5 && (r + 0.05).abs() < 1e-5);
        p.feedback = 0.8;
        let (l, r) = process_frame(&mut state, p, true, 0.0, 0.0);
        assert!((l - 0.256).abs() < 1e-5 && (r + 0.128).abs() < 1e-5);
        let allocations = crate::test_alloc::count(|| {
            state.reset();
            let p = RollParams {
                step: 16,
                time_ms: 1.0,
                ..p
            };
            for _ in 0..10000 {
                let (l, r) = process_frame(&mut state, p, true, 0.9, -0.9);
                assert!(l.is_finite() && r.is_finite() && l.abs() <= 1.0 && r.abs() <= 1.0);
            }
        });
        assert_eq!(allocations, 0);
    }
    #[test]
    fn division_changes_reuse_the_frozen_base_instead_of_recording_new_audio() {
        let mut state = RollDspState::new();
        state.prepare(1000.0);
        let mut p = params();
        for n in 0..200 {
            let x = if n < 150 { 0.2 } else { 0.6 };
            process_frame(&mut state, p, false, x, -x);
        }
        for _ in 0..2000 {
            process_frame(&mut state, p, true, -0.8, 0.8);
        }
        let capture_end = state.capture_end;
        let filled = state.filled;
        for division in [8, 2, 16, 1, 4] {
            p.step = division;
            for _ in 0..220 {
                let (l, r) = process_frame(&mut state, p, true, -0.8, 0.8);
                assert!(l > 0.19 && l < 0.61, "Division recaptured new input: {l}");
                assert!((l + r).abs() < 1e-6);
            }
            assert!(state.frozen);
            assert_eq!(state.capture_end, capture_end);
            assert_eq!(state.filled, filled);
            assert_eq!(state.base_length, 200);
        }
    }
    #[test]
    fn rapid_division_changes_are_bounded_and_allocation_free() {
        let mut state = RollDspState::new();
        state.prepare(48000.0);
        let mut p = params();
        for n in 0..12000 {
            let x = (n as f32 * 0.07).sin() * 0.2;
            process_frame(&mut state, p, false, x, -x);
        }
        let count = crate::test_alloc::count(|| {
            for n in 0..24000 {
                if n % 31 == 0 {
                    p.step = [1, 2, 4, 8, 16][(n / 31) % 5];
                }
                let (l, r) = process_frame(&mut state, p, true, 0.7, -0.7);
                assert!(l.is_finite() && r.is_finite() && l.abs() <= 1.0 && r.abs() <= 1.0);
            }
        });
        assert_eq!(count, 0);
    }
}
