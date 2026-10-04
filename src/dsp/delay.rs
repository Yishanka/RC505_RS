const DELAY_TIME_MIN_MS: f32 = 1.0;
const DELAY_TIME_MAX_MS: f32 = 2_000.0;
const FEEDBACK_MAX: f32 = 1.0;

#[derive(Clone, Copy)]
pub struct DelayParams {
    pub time_ms: f32,
    pub feedback: f32,
    pub high_damp_hz: f32,
    pub direct: f32,
    pub effect: f32,
    pub low_cut_hz: f32,
}

#[derive(Clone)]
pub struct DelayDspState {
    sample_rate: f32,
    buffer_l: Vec<f32>,
    buffer_r: Vec<f32>,
    write_idx: usize,
    valid: usize,
    smooth_time_samples: f32,
    smooth_feedback: f32,
    feedback_initialized: bool,
    feedback_step: f32,
    hold_mix: f32,
    hold_step: f32,
    allpass_previous: [f32; 2],
    allpass_integer: usize,
    allpass_initialized: bool,
    smooth_damp_hz: f32,
    smooth_direct: f32,
    smooth_effect: f32,
    hp_y: [f32; 2],
    hp_x: [f32; 2],
    fb_lp_l: f32,
    fb_lp_r: f32,
    smoothing: f32,
    lp_cache: (f32, f32),
    hp_cache: (f32, f32),
}

impl DelayDspState {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(1.0);
        let max_delay_samples = ((DELAY_TIME_MAX_MS / 1000.0) * sr).ceil() as usize + 2;
        Self {
            sample_rate: sr,
            buffer_l: vec![0.0; max_delay_samples.max(2)],
            buffer_r: vec![0.0; max_delay_samples.max(2)],
            write_idx: 0,
            valid: 0,
            smooth_time_samples: (f64::from(DELAY_TIME_MIN_MS) * f64::from(sr) / 1000.0) as f32,
            smooth_feedback: 0.0,
            feedback_initialized: false,
            feedback_step: 1.0 / (sr * 0.025).max(1.0),
            hold_mix: 0.0,
            hold_step: 1.0 / (sr * 0.005).max(1.0),
            allpass_previous: [0.0; 2],
            allpass_integer: 0,
            allpass_initialized: false,
            smooth_damp_hz: 20_000.0,
            smooth_direct: 1.0,
            smooth_effect: 0.0,
            hp_y: [0.0; 2],
            hp_x: [0.0; 2],
            fb_lp_l: 0.0,
            fb_lp_r: 0.0,
            smoothing: smoothing_coeff(sr, 25.0),
            lp_cache: (f32::NAN, 0.0),
            hp_cache: (f32::NAN, 0.0),
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        let sr = sample_rate.max(1.0);
        if (self.sample_rate - sr).abs() < f32::EPSILON {
            return;
        }
        self.sample_rate = sr;
        self.smoothing = smoothing_coeff(sr, 25.0);
        self.feedback_step = 1.0 / (sr * 0.025).max(1.0);
        self.hold_step = 1.0 / (sr * 0.005).max(1.0);
        self.feedback_initialized = false;
        self.allpass_initialized = false;
        self.allpass_previous = [0.0; 2];
        self.lp_cache.0 = f32::NAN;
        self.hp_cache.0 = f32::NAN;
        let max_delay_samples = ((DELAY_TIME_MAX_MS / 1000.0) * sr).ceil() as usize + 2;
        self.buffer_l = vec![0.0; max_delay_samples.max(2)];
        self.buffer_r = vec![0.0; max_delay_samples.max(2)];
        self.write_idx = 0;
        self.valid = 0;
        self.smooth_time_samples = (f64::from(DELAY_TIME_MIN_MS) * f64::from(sr) / 1000.0) as f32;
        self.fb_lp_l = 0.0;
        self.fb_lp_r = 0.0;
        self.hp_x = [0.0; 2];
        self.hp_y = [0.0; 2];
    }

    pub fn reset(&mut self) {
        self.feedback_initialized = false;
        self.allpass_initialized = false;
        self.allpass_previous = [0.0; 2];
        self.write_idx = 0;
        self.valid = 0;
        self.fb_lp_l = 0.0;
        self.fb_lp_r = 0.0;
        self.hp_x = [0.0; 2];
        self.hp_y = [0.0; 2];
    }
}

pub fn process_sample(
    state: &mut DelayDspState,
    p: DelayParams,
    sample_rate: f32,
    input_l: f32,
    input_r: f32,
) -> (f32, f32) {
    process_mode(state, p, sample_rate, input_l, input_r, None)
}

/// Stereo divided taps: L at Time*Ratio, R at Time; full-length taps cross-feed.
pub fn process_panning_sample(
    state: &mut DelayDspState,
    p: DelayParams,
    sample_rate: f32,
    input_l: f32,
    input_r: f32,
    ratio: f32,
    width: f32,
) -> (f32, f32) {
    process_mode(
        state,
        p,
        sample_rate,
        input_l,
        input_r,
        Some((ratio.clamp(0.1, 1.0), width.clamp(0.0, 2.0))),
    )
}
fn process_mode(
    state: &mut DelayDspState,
    p: DelayParams,
    sample_rate: f32,
    input_l: f32,
    input_r: f32,
    panning: Option<(f32, f32)>,
) -> (f32, f32) {
    let input_l = if input_l.is_finite() { input_l } else { 0.0 };
    let input_r = if input_r.is_finite() { input_r } else { 0.0 };
    state.set_sample_rate(sample_rate);
    let sr = state.sample_rate;
    let max_delay_samples = (state.buffer_l.len().saturating_sub(2)).max(1);

    let target_time_samples = ((f64::from(p.time_ms.clamp(DELAY_TIME_MIN_MS, DELAY_TIME_MAX_MS))
        * f64::from(sr)
        / 1000.0) as f32)
        .clamp(1.0, max_delay_samples as f32);
    let target_feedback = p.feedback.clamp(0.0, FEEDBACK_MAX);
    let target_damp_hz = if p.high_damp_hz <= 0.0 {
        sr * 0.49
    } else {
        p.high_damp_hz.clamp(20.0, 20_000.0)
    };
    let target_direct = p.direct.clamp(0.0, 1.0);
    let target_effect = p.effect.clamp(0.0, 1.2);

    let smooth_coeff = state.smoothing;
    let old_time = state.smooth_time_samples;
    state.smooth_time_samples += (target_time_samples - state.smooth_time_samples) * smooth_coeff;
    if state.smooth_time_samples == old_time {
        state.smooth_time_samples = target_time_samples;
    }
    // A finite ramp reaches exactly 1.0. Exponential smoothing asymptotically
    // sticks below unity in f32 and silently drains a 100% short-delay hold.
    let difference = target_feedback - state.smooth_feedback;
    let hold_target = if target_feedback == 1.0 { 1.0 } else { 0.0 };
    if !state.feedback_initialized {
        state.hold_mix = hold_target;
    }
    if !state.feedback_initialized || difference.abs() <= state.feedback_step {
        state.smooth_feedback = target_feedback;
        state.feedback_initialized = true;
    } else {
        state.smooth_feedback += difference.signum() * state.feedback_step;
    }
    state.hold_mix += (hold_target - state.hold_mix).clamp(-state.hold_step, state.hold_step);
    if (hold_target - state.hold_mix).abs() < state.hold_step {
        state.hold_mix = hold_target;
    }
    state.smooth_damp_hz += (target_damp_hz - state.smooth_damp_hz) * smooth_coeff;
    state.smooth_direct += (target_direct - state.smooth_direct) * smooth_coeff;
    state.smooth_effect += (target_effect - state.smooth_effect) * smooth_coeff;

    let delay_samples = state
        .smooth_time_samples
        .clamp(1.0, max_delay_samples as f32);
    let delayed_l = if delay_samples > state.valid as f32 {
        0.0
    } else {
        read_interp(&state.buffer_l, state.write_idx as f32 - delay_samples)
    };
    let delayed_r = if delay_samples > state.valid as f32 {
        0.0
    } else {
        read_interp(&state.buffer_r, state.write_idx as f32 - delay_samples)
    };
    let wet = if let Some((ratio, width)) = panning {
        let time = (delay_samples * ratio).max(1.0);
        let left = if time > state.valid as f32 {
            0.0
        } else {
            read_interp(&state.buffer_l, state.write_idx as f32 - time)
        };
        let mid = (left + delayed_r) * 0.5;
        let side = (left - delayed_r) * 0.5 * width;
        [mid + side, mid - side]
    } else {
        [delayed_l, delayed_r]
    };

    if state.lp_cache.0 != state.smooth_damp_hz {
        state.lp_cache = (
            state.smooth_damp_hz,
            one_pole_alpha(state.smooth_damp_hz, sr),
        );
    }
    let lp_alpha = if p.high_damp_hz <= 0.0 {
        1.0
    } else {
        state.lp_cache.1
    };
    let held = allpass_feedback(state, delay_samples, [delayed_l, delayed_r]);
    let feedback_l = delayed_l + (held[0] - delayed_l) * state.hold_mix;
    let feedback_r = delayed_r + (held[1] - delayed_r) * state.hold_mix;
    state.fb_lp_l += (feedback_l - state.fb_lp_l) * lp_alpha;
    state.fb_lp_r += (feedback_r - state.fb_lp_r) * lp_alpha;

    let mut feedback = [state.fb_lp_l, state.fb_lp_r];
    if p.low_cut_hz > 0.0 {
        if state.hp_cache.0 != p.low_cut_hz {
            state.hp_cache = (
                p.low_cut_hz,
                (-2.0 * std::f32::consts::PI * p.low_cut_hz.clamp(10.0, 12500.0).min(sr * 0.45)
                    / sr)
                    .exp(),
            );
        }
        let a = state.hp_cache.1;
        for ch in 0..2 {
            let x = feedback[ch];
            state.hp_y[ch] = a * (state.hp_y[ch] + x - state.hp_x[ch]);
            state.hp_x[ch] = x;
            feedback[ch] = state.hp_y[ch];
        }
    }
    let fb = state.smooth_feedback;
    let (source_l, source_r) = if panning.is_some() {
        feedback.swap(0, 1);
        let mono = (input_l + input_r) * 0.5;
        (mono, mono)
    } else {
        (input_l, input_r)
    };
    let write_l = (source_l + feedback[0] * fb).clamp(-1.0, 1.0);
    let write_r = (source_r + feedback[1] * fb).clamp(-1.0, 1.0);
    state.buffer_l[state.write_idx] = write_l;
    state.buffer_r[state.write_idx] = write_r;
    state.valid = (state.valid + 1).min(state.buffer_l.len());

    state.write_idx += 1;
    if state.write_idx >= state.buffer_l.len() {
        state.write_idx = 0;
    }

    let direct = if p.direct == 0.0 {
        0.0
    } else {
        state.smooth_direct
    };
    let effect = if p.effect == 0.0 {
        0.0
    } else {
        state.smooth_effect
    };
    (
        input_l * direct + wet[0] * effect,
        input_r * direct + wet[1] * effect,
    )
}

fn allpass_feedback(state: &mut DelayDspState, delay: f32, linear: [f32; 2]) -> [f32; 2] {
    // Put the fractional allpass delay in [0.5,1.5), so |a| <= 1/3.
    // Using [0,1) would approach a pole at -1 near integer boundaries and
    // inject long high-frequency ringing as the delay is adjusted.
    if delay < 1.5 {
        state.allpass_initialized = false;
        state.allpass_previous = linear;
        return linear;
    }
    let integer = (delay - 0.5).floor() as usize;
    let fraction = delay - integer as f32;
    let a = (1.0 - fraction) / (1.0 + fraction);
    let buffers = [&state.buffer_l, &state.buffer_r];
    let mut output = [0.0; 2];
    for ch in 0..2 {
        let buffer = buffers[ch];
        let read = |age: usize| {
            if age > state.valid {
                0.0
            } else {
                buffer[(state.write_idx + buffer.len() - age) % buffer.len()]
            }
        };
        if !state.allpass_initialized || state.allpass_integer != integer {
            // Re-seed from the adjacent interpolated history on an integer tap
            // change; do not carry a state belonging to a different base tap.
            state.allpass_previous[ch] = if delay + 1.0 > state.valid as f32 {
                0.0
            } else {
                read_interp(buffer, state.write_idx as f32 - delay - 1.0)
            };
        }
        output[ch] = (a * read(integer) + read(integer + 1) - a * state.allpass_previous[ch])
            .clamp(-4.0, 4.0);
        state.allpass_previous[ch] = output[ch];
    }
    state.allpass_initialized = true;
    state.allpass_integer = integer;
    output
}

fn read_interp(buffer: &[f32], read_pos: f32) -> f32 {
    if buffer.is_empty() {
        return 0.0;
    }
    let len = buffer.len() as f32;
    let wrapped = read_pos.rem_euclid(len);
    // f32::rem_euclid can round a tiny negative position up to `len`.
    // Normalize the integer index separately, retaining the original fraction.
    let base = wrapped.floor();
    let idx0 = base as usize % buffer.len();
    let idx1 = (idx0 + 1) % buffer.len();
    let frac = wrapped - base;
    buffer[idx0] * (1.0 - frac) + buffer[idx1] * frac
}

fn one_pole_alpha(cutoff_hz: f32, sample_rate: f32) -> f32 {
    let nyquist = (sample_rate * 0.5).max(1.0);
    let fc = cutoff_hz.clamp(1.0, nyquist);
    let x = (-2.0 * std::f32::consts::PI * fc / sample_rate.max(1.0)).exp();
    (1.0 - x).clamp(0.0, 1.0)
}

fn smoothing_coeff(sample_rate: f32, time_ms: f32) -> f32 {
    let tau = (time_ms.max(1.0) / 1000.0).max(1.0 / sample_rate.max(1.0));
    (1.0 - (-1.0 / (sample_rate.max(1.0) * tau)).exp()).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unity_fractional_feedback_retains_energy_without_interpolation_decay() {
        for sr in [8000.0, 44100.0, 48000.0, 96000.0, 192000.0] {
            for time_ms in [1.0, 1.01, 1.37] {
                for panning in [false, true] {
                    let mut state = DelayDspState::new(sr);
                    let p = DelayParams {
                        time_ms,
                        feedback: 1.0,
                        high_damp_hz: 0.0,
                        low_cut_hz: 0.0,
                        direct: 0.0,
                        effect: 1.0,
                    };
                    let mut process = |x| {
                        if panning {
                            process_panning_sample(&mut state, p, sr, x, x, 0.5, 1.0)
                        } else {
                            process_sample(&mut state, p, sr, x, x)
                        }
                    };
                    for _ in 0..sr as usize {
                        process(0.0);
                    }
                    let mut early = 0.0f64;
                    let mut late = 0.0f64;
                    let count = crate::test_alloc::count(|| {
                        for n in 0..sr as usize * 3 {
                            let y = process(if n == 0 { 0.1 } else { 0.0 });
                            let energy = f64::from(y.0 * y.0 + y.1 * y.1);
                            if n >= sr as usize / 2 && n < sr as usize {
                                early += energy;
                            }
                            if n >= sr as usize * 5 / 2 {
                                late += energy;
                            }
                            assert!(y.0.is_finite() && y.1.is_finite());
                        }
                    });
                    let ratio = late / early;
                    assert!(
                        early > 0.001 && ratio > 0.94 && ratio < 1.06,
                        "{sr} {time_ms} pan={panning}: early {early}, late {late}, ratio {ratio}"
                    );
                    assert_eq!(count, 0);
                    assert_eq!(state.smooth_feedback, 1.0);
                    assert_eq!(state.hold_mix, 1.0);
                }
            }
        }
    }
    #[test]
    fn unity_feedback_boundary_changes_reset_and_endpoint_switches_are_bounded() {
        for sr in [8000.0, 44100.0, 48000.0, 96000.0, 192000.0] {
            let mut state = DelayDspState::new(sr);
            let mut p = DelayParams {
                time_ms: 1.0,
                feedback: 1.0,
                high_damp_hz: 0.0,
                low_cut_hz: 0.0,
                direct: 0.0,
                effect: 1.0,
            };
            let count = crate::test_alloc::count(|| {
                for n in 0..sr as usize * 2 {
                    // Cross both ordinary integer and the chosen half-integer
                    // base-tap boundaries; then make a two-second jump.
                    let base = (sr * 0.001).floor();
                    p.time_ms = match n / 257 % 6 {
                        0 => 1.0,
                        1 => 1.01,
                        2 => (base + 1.49) / sr * 1000.0,
                        3 => (base + 1.51) / sr * 1000.0,
                        4 => 2000.0,
                        _ => 1.37,
                    };
                    p.feedback = if n / 6000 % 2 == 0 { 1.0 } else { 0.95 };
                    let x = if n < sr as usize {
                        (n as f32 * 0.013).sin() * 2.0
                    } else {
                        0.0
                    };
                    let y = process_panning_sample(&mut state, p, sr, x, -x * 0.2, 0.37, 2.0);
                    assert!(
                        y.0.is_finite() && y.1.is_finite() && y.0.abs() <= 2.0 && y.1.abs() <= 2.0
                    );
                }
                state.reset();
                p.feedback = 1.0;
                for n in 0..sr as usize {
                    p.time_ms = if n % 2 == 0 { 1.01 } else { 2000.0 };
                    let y = process_sample(&mut state, p, sr, 0.0, 0.0);
                    assert_eq!(y, (0.0, 0.0));
                }
            });
            assert_eq!(count, 0);
            assert_eq!(state.smooth_feedback, 1.0);
            assert_eq!(state.hold_mix, 1.0);
        }
    }
    #[test]
    fn unity_feedback_is_exact_and_holds_integer_delay_echoes_without_hidden_decay() {
        for sr in [8000.0, 48000.0, 96000.0, 192000.0] {
            for panning in [false, true] {
                let mut state = DelayDspState::new(sr);
                let p = DelayParams {
                    time_ms: 1.0,
                    feedback: 1.0,
                    high_damp_hz: 0.0,
                    low_cut_hz: 0.0,
                    direct: 0.0,
                    effect: 1.0,
                };
                let mut later = 0.0f32;
                let mut last = 0.0f32;
                let count = crate::test_alloc::count(|| {
                    for n in 0..sr as usize * 2 {
                        let x = if n == 0 { 0.25 } else { 0.0 };
                        let y = if panning {
                            process_panning_sample(&mut state, p, sr, x, x, 0.5, 1.0)
                        } else {
                            process_sample(&mut state, p, sr, x, x)
                        };
                        if n > sr as usize / 2 && n < sr as usize {
                            later = later.max(y.0.abs());
                        }
                        if n > sr as usize * 3 / 2 {
                            last = last.max(y.0.abs());
                        }
                        assert!(y.0.is_finite() && y.1.is_finite());
                    }
                });
                assert_eq!(count, 0);
                assert_eq!(state.smooth_feedback, 1.0);
                assert!(
                    (last - later).abs() < 0.00001,
                    "{sr} {panning}: {later} -> {last}"
                );
                assert!(last > 0.249);
            }
        }
    }
    #[test]
    fn unity_endpoint_ramp_and_fractional_time_stress_are_bounded() {
        let sr = 48000.0;
        let mut state = DelayDspState::new(sr);
        let mut p = DelayParams {
            time_ms: 1.01,
            feedback: 0.1,
            high_damp_hz: 0.0,
            low_cut_hz: 0.0,
            direct: 1.0,
            effect: 1.2,
        };
        process_sample(&mut state, p, sr, 0.0, 0.0);
        p.feedback = 1.0;
        for _ in 0..2000 {
            process_sample(&mut state, p, sr, 0.0, 0.0);
        }
        assert_eq!(state.smooth_feedback, 1.0);
        let count = crate::test_alloc::count(|| {
            for n in 0..200000 {
                p.time_ms = [1.0, 1.01, 1.13, 2000.0][n / 5000 % 4];
                let x = (n as f32 * 0.033).sin() * 4.0;
                let y = process_panning_sample(&mut state, p, sr, x, -x * 0.3, 0.37, 2.0);
                assert!(y.0.is_finite() && y.1.is_finite() && y.0.abs() <= 6.4 && y.1.abs() <= 6.4);
            }
        });
        assert_eq!(count, 0);
        assert_eq!(state.smooth_feedback, 1.0);
    }
    #[test]
    fn one_millisecond_bass_and_near_zero_read_position_remain_valid() {
        let mut state = DelayDspState::new(48000.0);
        let p = DelayParams {
            time_ms: 1.0,
            feedback: 0.95,
            high_damp_hz: 8000.0,
            direct: 1.0,
            effect: 1.0,
            low_cut_hz: 20.0,
        };
        for frame in 0..100_000 {
            let bass = (frame as f32 * std::f32::consts::TAU * 50.0 / 48000.0).sin() * 0.9;
            let (l, r) = process_sample(&mut state, p, 48000.0, bass, -bass);
            assert!(l.is_finite() && r.is_finite());
        }
        let buffer = vec![0.25; 96002];
        assert!((read_interp(&buffer, -0.000004) - 0.25).abs() < 1e-6);
    }
    #[test]
    fn short_delay_automation_is_bounded_and_allocation_free() {
        for sr in [8000.0, 44100.0, 48000.0, 96000.0, 192000.0] {
            let mut state = DelayDspState::new(sr);
            let count = crate::test_alloc::count(|| {
                for n in 0..200_000 {
                    let time_ms = match n / 10000 % 4 {
                        0 => 2000.0,
                        1 => 1.0,
                        2 => 1.01,
                        _ => 20.0,
                    };
                    let p = DelayParams {
                        time_ms,
                        feedback: 0.95,
                        high_damp_hz: 8000.0,
                        direct: 1.0,
                        effect: 1.0,
                        low_cut_hz: 0.0,
                    };
                    let input = if n < 100_000 {
                        (n as f32 * 0.05).sin() * 0.95
                    } else {
                        0.0
                    };
                    let (l, r) = process_sample(&mut state, p, sr, input, -input);
                    assert!(l.is_finite() && r.is_finite() && l.abs() <= 2.0 && r.abs() <= 2.0);
                }
            });
            assert_eq!(count, 0);
        }
    }
}
