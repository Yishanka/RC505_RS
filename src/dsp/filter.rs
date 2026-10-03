use crate::config::filter_configs::FilterType;

#[derive(Clone, Copy)]
pub struct FilterParams {
    pub filter_type: FilterType,
    pub cutoff_hz: f32,
    pub q: f32,
    pub drive: f32,
    pub mix: f32,
}

#[derive(Clone, Copy)]
struct BiquadCoeffs {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

#[derive(Clone, Copy)]
pub struct FilterDspState {
    // Trapezoidal, topology-preserving state variable integrators. Unlike
    // direct-form delay states these retain their meaning during cutoff changes.
    integrators: [f64; 2],
    smooth_cutoff_log2: f32,
    smooth_q: f32,
    inited: bool,
    cached: [f64; 4],
    last_cutoff: f32,
    target_log2: f32,
    last_kind: FilterType,
    sample_rate: f32,
    alpha: f32,
    control_tick: u8,
    last_drive: f32,
    drive_gain: f32,
    drive_norm: f32,
}

impl FilterDspState {
    pub fn new() -> Self {
        Self {
            integrators: [0.0; 2],
            smooth_cutoff_log2: 0.0,
            smooth_q: 0.707,
            inited: false,
            cached: [1.0, 0.0, 0.0, 1.0],
            last_cutoff: -1.0,
            target_log2: 0.0,
            last_kind: FilterType::Lpf,
            sample_rate: 0.0,
            alpha: 0.0,
            control_tick: 0,
            last_drive: -1.0,
            drive_gain: 1.0,
            drive_norm: 1.0,
        }
    }
}

pub fn process_sample(
    state: &mut FilterDspState,
    p: FilterParams,
    sample_rate: f32,
    input: f32,
) -> f32 {
    let sr = sample_rate.max(1.0);
    let nyquist = (sr * 0.5 - 1.0).max(21.0);
    let cutoff = p.cutoff_hz.clamp(20.0, nyquist);
    let q = p.q.clamp(0.1, 10.0);

    if state.sample_rate != sr {
        state.sample_rate = sr;
        state.alpha = 1.0 - (-1.0 / (0.02 * sr)).exp();
        state.inited = false;
    }
    if state.last_cutoff != cutoff {
        state.last_cutoff = cutoff;
        state.target_log2 = cutoff.log2();
    }
    let first = !state.inited;
    if first {
        state.smooth_cutoff_log2 = state.target_log2;
        state.smooth_q = q;
        state.inited = true;
    }
    let moving = (state.target_log2 - state.smooth_cutoff_log2).abs() > 1e-5
        || (q - state.smooth_q).abs() > 1e-5;
    state.smooth_cutoff_log2 += (state.target_log2 - state.smooth_cutoff_log2) * state.alpha;
    state.smooth_q += (q - state.smooth_q) * state.alpha;
    if first || state.last_kind != p.filter_type || (moving && state.control_tick == 0) {
        let frequency = 2.0_f32.powf(state.smooth_cutoff_log2).clamp(20.0, nyquist) as f64;
        let g = (std::f64::consts::PI * frequency / sr as f64).tan();
        let k = 1.0 / state.smooth_q as f64;
        let a1 = 1.0 / (1.0 + g * (g + k));
        state.cached = [a1, g * a1, g * g * a1, k];
        state.last_kind = p.filter_type;
    }
    state.control_tick = (state.control_tick + 1) % 8;
    let [a1, a2, a3, k] = state.cached;
    let drive = p.drive.clamp(0.0, 1.0);
    if drive != state.last_drive {
        state.last_drive = drive;
        state.drive_gain = 1.0 + drive * 9.0;
        state.drive_norm = 1.0 / state.drive_gain.tanh().max(1e-6);
    }
    let driven = if drive == 0.0 {
        input
    } else {
        let saturated = (input * state.drive_gain).tanh() * state.drive_norm;
        input + drive * (saturated - input)
    };

    let v3 = driven as f64 - state.integrators[1];
    let band = a1 * state.integrators[0] + a2 * v3;
    let low = state.integrators[1] + a2 * state.integrators[0] + a3 * v3;
    state.integrators = [
        2.0 * band - state.integrators[0],
        2.0 * low - state.integrators[1],
    ];
    for state in &mut state.integrators {
        if state.abs() < 1e-25 {
            *state = 0.0;
        }
    }
    let wet_sig = match p.filter_type {
        FilterType::Lpf => low,
        FilterType::Bpf => k * band,
        FilterType::Hpf => driven as f64 - k * band - low,
        FilterType::Notch => driven as f64 - k * band,
    } as f32;
    let wet = p.mix.clamp(0.0, 1.0);
    input * (1.0 - wet) + wet_sig * wet
}

/// Steady-state linear response, shared with the editor (including dry/wet phase).
pub fn response_db(kind: FilterType, cutoff: f32, q: f32, mix: f32, hz: f32, sr: f32) -> f32 {
    let c = coeffs(kind, cutoff.clamp(20.0, sr * 0.49), q.clamp(0.1, 10.0), sr);
    let w = 2.0 * std::f32::consts::PI * hz / sr;
    let nr = c.b0 + c.b1 * w.cos() + c.b2 * (2.0 * w).cos();
    let ni = -c.b1 * w.sin() - c.b2 * (2.0 * w).sin();
    let dr = 1.0 + c.a1 * w.cos() + c.a2 * (2.0 * w).cos();
    let di = -c.a1 * w.sin() - c.a2 * (2.0 * w).sin();
    let norm = (dr * dr + di * di).max(1e-20);
    let real = (1.0 - mix) + mix * (nr * dr + ni * di) / norm;
    let imag = mix * (ni * dr - nr * di) / norm;
    10.0 * (real * real + imag * imag).max(1e-12).log10()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn modulated_high_resonance_is_bounded_and_static_plot_matches_audio() {
        for sr in [8000.0, 44100.0, 48000.0, 96000.0, 192000.0] {
            for kind in [
                FilterType::Lpf,
                FilterType::Bpf,
                FilterType::Hpf,
                FilterType::Notch,
            ] {
                let mut state = FilterDspState::new();
                let n = crate::test_alloc::count(|| {
                    for i in 0..20000 {
                        let cutoff = if i % 50 < 25 { 20.0 } else { sr * 0.47 };
                        let x = (i as f32 * 0.173).sin() * 0.05;
                        let y = process_sample(
                            &mut state,
                            FilterParams {
                                filter_type: kind,
                                cutoff_hz: cutoff,
                                q: 10.0,
                                drive: 0.4,
                                mix: 1.0,
                            },
                            sr,
                            x,
                        );
                        assert!(y.is_finite() && y.abs() < 16.0, "Unstable {sr}: {y}");
                    }
                });
                assert_eq!(n, 0);
            }
        }
        for kind in [
            FilterType::Lpf,
            FilterType::Bpf,
            FilterType::Hpf,
            FilterType::Notch,
        ] {
            for frequency in [200.0, 500.0, 2000.0, 6000.0] {
                let mut state = FilterDspState::new();
                let mut input_power = 0.0f64;
                let mut output_power = 0.0f64;
                for i in 0..48000 {
                    let x = (std::f32::consts::TAU * frequency * i as f32 / 48000.0).sin() * 0.1;
                    let y = process_sample(
                        &mut state,
                        FilterParams {
                            filter_type: kind,
                            cutoff_hz: 1000.0,
                            q: 1.7,
                            drive: 0.0,
                            mix: 1.0,
                        },
                        48000.0,
                        x,
                    );
                    if i >= 24000 {
                        input_power += f64::from(x * x);
                        output_power += f64::from(y * y);
                    }
                }
                let db = 10.0 * (output_power / input_power).log10() as f32;
                assert!(
                    (db - response_db(kind, 1000.0, 1.7, 1.0, frequency, 48000.0)).abs() < 0.03
                );
            }
        }
    }
    #[test]
    fn zero_drive_is_linear_and_dry_mix_is_identity() {
        let p = FilterParams {
            filter_type: FilterType::Lpf,
            cutoff_hz: 2000.0,
            q: 0.707,
            drive: 0.0,
            mix: 1.0,
        };
        let mut a = FilterDspState::new();
        let mut b = FilterDspState::new();
        for i in 0..2000 {
            let x = (i as f32 * 0.1).sin() * 0.1;
            let y = process_sample(&mut a, p, 48000.0, x);
            let y2 = process_sample(&mut b, p, 48000.0, x * 4.0);
            assert!((y2 - y * 4.0).abs() < 1e-5);
        }
        assert_eq!(
            process_sample(&mut a, FilterParams { mix: 0.0, ..p }, 48000.0, 0.37),
            0.37
        );
        assert!(response_db(FilterType::Lpf, 1000.0, 0.707, 1.0, 10000.0, 48000.0) < -30.0);
    }
}

fn coeffs(filter_type: FilterType, cutoff: f32, q: f32, sample_rate: f32) -> BiquadCoeffs {
    let w0 = 2.0 * std::f32::consts::PI * (cutoff / sample_rate.max(1.0));
    let cos_w0 = w0.cos();
    let sin_w0 = w0.sin();
    let alpha = sin_w0 / (2.0 * q.max(0.1));

    let (b0, b1, b2, a0, a1, a2) = match filter_type {
        FilterType::Lpf => {
            let b0 = (1.0 - cos_w0) * 0.5;
            let b1 = 1.0 - cos_w0;
            let b2 = (1.0 - cos_w0) * 0.5;
            let a0 = 1.0 + alpha;
            let a1 = -2.0 * cos_w0;
            let a2 = 1.0 - alpha;
            (b0, b1, b2, a0, a1, a2)
        }
        FilterType::Hpf => {
            let b0 = (1.0 + cos_w0) * 0.5;
            let b1 = -(1.0 + cos_w0);
            let b2 = (1.0 + cos_w0) * 0.5;
            let a0 = 1.0 + alpha;
            let a1 = -2.0 * cos_w0;
            let a2 = 1.0 - alpha;
            (b0, b1, b2, a0, a1, a2)
        }
        FilterType::Bpf => {
            let b0 = alpha;
            let b1 = 0.0;
            let b2 = -alpha;
            let a0 = 1.0 + alpha;
            let a1 = -2.0 * cos_w0;
            let a2 = 1.0 - alpha;
            (b0, b1, b2, a0, a1, a2)
        }
        FilterType::Notch => {
            let b0 = 1.0;
            let b1 = -2.0 * cos_w0;
            let b2 = 1.0;
            let a0 = 1.0 + alpha;
            let a1 = -2.0 * cos_w0;
            let a2 = 1.0 - alpha;
            (b0, b1, b2, a0, a1, a2)
        }
    };

    BiquadCoeffs {
        b0: b0 / a0,
        b1: b1 / a0,
        b2: b2 / a0,
        a1: a1 / a0,
        a2: a2 / a0,
    }
}
