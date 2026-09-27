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
struct BiquadState {
    z1: f32,
    z2: f32,
}

impl BiquadState {
    fn new() -> Self {
        Self { z1: 0.0, z2: 0.0 }
    }

    fn process(&mut self, x: f32, c: BiquadCoeffs) -> f32 {
        let y = c.b0 * x + self.z1;
        self.z1 = c.b1 * x - c.a1 * y + self.z2;
        self.z2 = c.b2 * x - c.a2 * y;
        y
    }
}

#[derive(Clone, Copy)]
pub struct FilterDspState {
    biquad: BiquadState,
    smooth_cutoff_log2: f32,
    smooth_q: f32,
    inited: bool,
}

impl FilterDspState {
    pub fn new() -> Self {
        Self {
            biquad: BiquadState::new(),
            smooth_cutoff_log2: 0.0,
            smooth_q: 0.707,
            inited: false,
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

    let target_log2 = cutoff.log2();
    if !state.inited {
        state.smooth_cutoff_log2 = target_log2;
        state.smooth_q = q;
        state.inited = true;
    }

    let alpha = 1.0 - (-1.0 / (0.02 * sr)).exp();
    state.smooth_cutoff_log2 += (target_log2 - state.smooth_cutoff_log2) * alpha;
    state.smooth_q += (q - state.smooth_q) * alpha;
    let smooth_cutoff = 2.0_f32.powf(state.smooth_cutoff_log2).clamp(20.0, nyquist);

    let coeffs = coeffs(p.filter_type, smooth_cutoff, state.smooth_q, sr);

    let drive = p.drive.clamp(0.0, 1.0);
    let gain = 1.0 + drive * 9.0;
    // Drive=0 must be linear. Blend in normalized saturation as drive rises.
    let saturated = (input * gain).tanh() / gain.tanh().max(1e-6);
    let driven = input + drive * (saturated - input);

    let wet_sig = state.biquad.process(driven, coeffs);
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
