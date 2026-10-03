//! Channel vocoder with cached analysis/synthesis filters, envelope formant
//! shifting, tilt and modulator sensitivity. Independent implementation.
pub const VOCODER_MAX_BANDS: usize = 16;

#[derive(Clone, Copy)]
pub struct VocoderParams {
    pub bands: usize,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub level: f32,
    pub mix: f32,
    pub sample_rate: f32,
    pub track_carrier_l: f32,
    pub track_carrier_r: f32,
    pub has_track_carrier: bool,
    pub tone: f32,
    pub mod_sens: f32,
    pub formant_semitones: f32,
    pub sibilance: f32,
    pub modulator_override: Option<f32>,
    pub mute_carrier_channel: Option<usize>,
}

#[derive(Clone, Copy, Default)]
struct Bandpass {
    b0: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}
impl Bandpass {
    fn configure(&mut self, hz: f32, q: f32, sr: f32) {
        let w = std::f32::consts::TAU * hz / sr;
        let alpha = w.sin() / (2.0 * q);
        self.b0 = alpha / (1.0 + alpha);
        self.b2 = -self.b0;
        self.a1 = -2.0 * w.cos() / (1.0 + alpha);
        self.a2 = (1.0 - alpha) / (1.0 + alpha);
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
    fn next(&mut self, input: f32) -> f32 {
        let output = self.b0 * input + self.z1;
        self.z1 = -self.a1 * output + self.z2;
        self.z2 = self.b2 * input - self.a2 * output;
        output
    }
}

#[derive(Clone, Copy)]
pub struct VocoderDspState {
    analysis: [Bandpass; VOCODER_MAX_BANDS],
    synthesis_l: [Bandpass; VOCODER_MAX_BANDS],
    synthesis_r: [Bandpass; VOCODER_MAX_BANDS],
    env: [f32; VOCODER_MAX_BANDS],
    bands: usize,
    sample_rate: f32,
    spacing_semitones: f32,
    attack_ms: f32,
    release_ms: f32,
    attack: f32,
    release: f32,
    smoothing: f32,
    formant: f32,
    tone: f32,
    sensitivity: f32,
    level: f32,
    mix: f32,
    sibilance: f32,
    gains: [f32; VOCODER_MAX_BANDS],
    gain_delta: [f32; VOCODER_MAX_BANDS],
    sensitivity_gain: f32,
    control_tick: u8,
}
impl VocoderDspState {
    pub fn new() -> Self {
        Self {
            analysis: [Bandpass::default(); VOCODER_MAX_BANDS],
            synthesis_l: [Bandpass::default(); VOCODER_MAX_BANDS],
            synthesis_r: [Bandpass::default(); VOCODER_MAX_BANDS],
            env: [0.0; VOCODER_MAX_BANDS],
            bands: 0,
            sample_rate: 0.0,
            spacing_semitones: 1.0,
            attack_ms: -1.0,
            release_ms: -1.0,
            attack: 0.0,
            release: 0.0,
            smoothing: 0.0,
            formant: 0.0,
            tone: 0.0,
            sensitivity: 0.0,
            level: 1.0,
            mix: 1.0,
            sibilance: 0.2,
            gains: [0.0; VOCODER_MAX_BANDS],
            gain_delta: [0.0; VOCODER_MAX_BANDS],
            sensitivity_gain: 6.0,
            control_tick: 0,
        }
    }
    fn configure(&mut self, p: VocoderParams) {
        let sr = p.sample_rate.max(1000.0);
        let bands = p.bands.clamp(4, VOCODER_MAX_BANDS);
        if bands != self.bands || sr != self.sample_rate {
            self.bands = bands;
            self.sample_rate = sr;
            self.env.fill(0.0);
            self.gains.fill(0.0);
            self.gain_delta.fill(0.0);
            self.control_tick = 0;
            let low = 90.0_f32;
            let high = 7200.0_f32.min(sr * 0.45);
            let ratio = (high / low).powf(1.0 / (bands - 1) as f32);
            let q = (1.0 / (ratio.sqrt() - 1.0 / ratio.sqrt())).clamp(0.5, 8.0);
            self.spacing_semitones = 12.0 * ratio.log2();
            for index in 0..bands {
                let hz = low * ratio.powf(index as f32);
                self.analysis[index].configure(hz, q, sr);
                self.synthesis_l[index].configure(hz, q, sr);
                self.synthesis_r[index].configure(hz, q, sr);
            }
            self.smoothing = coefficient(15.0, sr);
            self.attack_ms = -1.0;
            self.release_ms = -1.0;
        }
        if self.attack_ms != p.attack_ms {
            self.attack_ms = p.attack_ms;
            self.attack = coefficient(p.attack_ms.max(0.1), sr);
        }
        if self.release_ms != p.release_ms {
            self.release_ms = p.release_ms;
            self.release = coefficient(p.release_ms.max(1.0), sr);
        }
    }
}

pub fn process_frame(
    state: &mut VocoderDspState,
    p: VocoderParams,
    input_l: f32,
    input_r: f32,
) -> (f32, f32) {
    state.configure(p);
    let smoothing = state.smoothing;
    state.formant += (p.formant_semitones.clamp(-12.0, 12.0) - state.formant) * smoothing;
    state.tone += (p.tone.clamp(-50.0, 50.0) - state.tone) * smoothing;
    state.sensitivity += (p.mod_sens.clamp(-50.0, 50.0) - state.sensitivity) * smoothing;
    state.level += (p.level.clamp(0.0, 1.0) - state.level) * smoothing;
    state.mix += (p.mix.clamp(0.0, 1.0) - state.mix) * smoothing;
    state.sibilance += (p.sibilance.clamp(0.0, 1.0) - state.sibilance) * smoothing;
    let modulator = p.modulator_override.unwrap_or((input_l + input_r) * 0.5);
    // +/-18 dB envelope sensitivity, retaining the user's formant contrast idea.
    if state.control_tick == 0 {
        state.sensitivity_gain = 6.0 * 10.0_f32.powf(state.sensitivity * 0.36 / 20.0);
    }
    for index in 0..state.bands {
        let band = state.analysis[index].next(modulator);
        let target = band.abs() * state.sensitivity_gain;
        let rate = if target > state.env[index] {
            state.attack
        } else {
            state.release
        };
        state.env[index] += (target - state.env[index]) * rate;
    }
    if state.control_tick == 0 {
        // Envelope detection remains sample-rate. Spectral contrast, formant
        // interpolation and tilt run every 16 frames, with linear gain interpolation.
        // Move soft compression after envelope following instead of exp() per band/sample.
        let compressed = state.env.map(|v| 1.0 - (-v).exp());
        let avg = compressed[..state.bands].iter().sum::<f32>() / state.bands as f32;
        let mut shaped = [0.0; VOCODER_MAX_BANDS];
        if avg > 1e-6 {
            for (index, value) in shaped.iter_mut().enumerate().take(state.bands) {
                let env = compressed[index];
                let local_start = index.saturating_sub(1);
                let local_end = (index + 1).min(state.bands - 1);
                let local = compressed[local_start..=local_end].iter().sum::<f32>()
                    / (local_end - local_start + 1) as f32;
                let peak = (env / local.max(1e-6) - 1.0).max(0.0);
                *value = avg * 0.06 + avg * (env / avg).powf(1.9) * (1.0 + peak * 1.8);
            }
        }
        // Normalize the complete spectral envelope rather than clipping each band:
        // independent clipping flattens vowel peaks at ordinary microphone levels.
        let peak = shaped.iter().copied().fold(1.0_f32, f32::max);
        for value in &mut shaped {
            *value /= peak;
        }
        let shift = state.formant / state.spacing_semitones;
        let mut tilt = 2.0_f32.powf(-state.tone / 50.0);
        let tilt_step = 2.0_f32.powf(state.tone / 25.0 / (state.bands - 1) as f32);
        for index in 0..state.bands {
            let source = (index as f32 - shift).clamp(0.0, (state.bands - 1) as f32);
            let lower = source.floor() as usize;
            let upper = (lower + 1).min(state.bands - 1);
            let env = shaped[lower] + (shaped[upper] - shaped[lower]) * (source - lower as f32);
            let position = index as f32 / (state.bands - 1) as f32;
            let consonants = if position > 0.7 {
                1.0 + state.sibilance * 2.0
            } else {
                1.0
            };
            let target = env * tilt * consonants;
            state.gain_delta[index] = (target - state.gains[index]) / 16.0;
            tilt *= tilt_step;
        }
    }
    state.control_tick = (state.control_tick + 1) % 16;
    let (carrier_l, carrier_r) = if p.has_track_carrier {
        (p.track_carrier_l, p.track_carrier_r)
    } else {
        (0.0, 0.0)
    };
    let mut wet_l = 0.0;
    let mut wet_r = 0.0;
    for index in 0..state.bands {
        state.gains[index] += state.gain_delta[index];
        wet_l += state.synthesis_l[index].next(carrier_l) * state.gains[index];
        wet_r += state.synthesis_r[index].next(carrier_r) * state.gains[index];
    }
    // Overlap-aware banks sum at approximately unity. Avoid sqrt(N) attenuation
    // which made increased band counts unexpectedly quieter in the old version.
    let gain = state.level * 1.25;
    let wet_l = (wet_l * gain * 1.5).tanh() / 1.5_f32.tanh();
    let wet_r = (wet_r * gain * 1.5).tanh() / 1.5_f32.tanh();
    let dry_l = if p.mute_carrier_channel == Some(0) {
        0.0
    } else {
        input_l
    };
    let dry_r = if p.mute_carrier_channel == Some(1) {
        0.0
    } else {
        input_r
    };
    // Explicit endpoints keep dry identity and unavailable-carrier silence exact.
    let mix = if p.mix <= 0.0 {
        0.0
    } else if p.mix >= 1.0 {
        1.0
    } else {
        state.mix
    };
    (
        dry_l * (1.0 - mix) + wet_l * mix,
        dry_r * (1.0 - mix) + wet_r * mix,
    )
}
fn coefficient(ms: f32, sr: f32) -> f32 {
    1.0 - (-1.0 / (ms * 0.001 * sr)).exp()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn params() -> VocoderParams {
        VocoderParams {
            bands: 12,
            attack_ms: 6.0,
            release_ms: 80.0,
            level: 1.0,
            mix: 1.0,
            sample_rate: 48000.0,
            track_carrier_l: 0.0,
            track_carrier_r: 0.0,
            has_track_carrier: true,
            tone: 0.0,
            mod_sens: 0.0,
            formant_semitones: 0.0,
            sibilance: 0.2,
            modulator_override: None,
            mute_carrier_channel: None,
        }
    }
    #[test]
    fn dry_identity_and_missing_carrier_silence() {
        let mut state = VocoderDspState::new();
        let mut p = params();
        p.mix = 0.0;
        assert_eq!(process_frame(&mut state, p, 0.2, -0.3), (0.2, -0.3));
        p.mix = 1.0;
        p.has_track_carrier = false;
        for _ in 0..1000 {
            assert_eq!(process_frame(&mut state, p, 0.4, 0.2), (0.0, 0.0));
        }
    }
    #[test]
    fn formant_shift_moves_envelope_toward_upper_carrier_partial() {
        let ratio = |shift| {
            let mut state = VocoderDspState::new();
            let mut p = params();
            p.formant_semitones = shift;
            p.sibilance = 0.0;
            let mut spectrum = [(0.0_f64, 0.0_f64); 2];
            for sample in 0..48000 {
                let t = sample as f32 / 48000.0;
                let low = (std::f32::consts::TAU * 500.0 * t).sin();
                let high = (std::f32::consts::TAU * 1000.0 * t).sin();
                p.track_carrier_l = 0.2 * (low + high);
                p.track_carrier_r = p.track_carrier_l;
                let (out, _) = process_frame(&mut state, p, low * 0.3, low * 0.3);
                if sample >= 24000 {
                    for (i, hz) in [500.0, 1000.0].into_iter().enumerate() {
                        let angle = std::f64::consts::TAU * hz * sample as f64 / 48000.0;
                        spectrum[i].0 += out as f64 * angle.sin();
                        spectrum[i].1 += out as f64 * angle.cos();
                    }
                }
            }
            let energy = |index: usize| spectrum[index].0.powi(2) + spectrum[index].1.powi(2);
            energy(1) / energy(0).max(1e-12)
        };
        let neutral = ratio(0.0);
        let shifted = ratio(12.0);
        assert!(
            shifted > neutral * 2.0,
            "neutral {neutral}, shifted {shifted}"
        );
    }
    #[test]
    fn stereo_carrier_stays_independent_and_releases_to_silence() {
        let mut state = VocoderDspState::new();
        let mut p = params();
        let mut energy = 0.0;
        for sample in 0..12000 {
            let t = sample as f32 / 48000.0;
            p.track_carrier_l = (std::f32::consts::TAU * 800.0 * t).sin() * 0.5;
            p.track_carrier_r = 0.0;
            let (left, right) = process_frame(
                &mut state,
                p,
                (std::f32::consts::TAU * 800.0 * t).sin() * 0.5,
                0.0,
            );
            assert!(left.is_finite());
            assert_eq!(right, 0.0);
            energy += left * left;
        }
        assert!(energy > 1.0);
        let mut final_peak = 0.0_f32;
        for sample in 0..96000 {
            p.track_carrier_l = if sample % 2 == 0 { 0.5 } else { -0.5 };
            let (left, _) = process_frame(&mut state, p, 0.0, 0.0);
            if sample > 90000 {
                final_peak = final_peak.max(left.abs());
            }
        }
        assert!(final_peak < 1e-5);
    }
}
