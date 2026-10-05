//! Stereo-linked input noise gate. This processes external audio before any
//! generator/rack, not the final mixed signal. No lookahead or callback allocation.
use crate::config::input_noise::InputNoiseConfig;
#[derive(Clone, Copy)]
pub struct InputNoiseParams {
    pub enabled: bool,
    threshold: f32,
    close_threshold: f32,
    detector_decay: f32,
    attack_frames: u32,
    hold_frames: u32,
    release_frames: u32,
}
impl InputNoiseParams {
    /// Prepare dB conversion and time coefficients outside the audio callback.
    pub fn from_config(config: InputNoiseConfig, sample_rate: u32) -> Self {
        let config = config.sanitized();
        let sr = sample_rate.max(1) as f32;
        let threshold = 10.0f32.powf(config.threshold_db / 20.0);
        Self {
            enabled: config.enabled,
            threshold,
            close_threshold: threshold * 0.5011872,
            detector_decay: (-1.0 / (sr * 0.01)).exp(),
            attack_frames: (sr * 0.002).round().max(1.0) as u32,
            hold_frames: (sr * 0.03).round() as u32,
            release_frames: (sr * 0.08).round().max(1.0) as u32,
        }
    }
}
#[derive(Clone, Copy, Default)]
pub struct InputNoiseState {
    initialized: bool,
    detector: f32,
    open: bool,
    hold_left: u32,
    gain: f32,
    ramp_from: f32,
    ramp_target: f32,
    ramp_position: u32,
    ramp_length: u32,
}
impl InputNoiseState {
    /// Caller supplies finite, bounded input (RenderCore's existing headroom guard).
    pub fn process(&mut self, p: InputNoiseParams, input: [f32; 2]) -> [f32; 2] {
        let peak = input[0].abs().max(input[1].abs());
        self.detector = peak.max(self.detector * p.detector_decay);
        if self.detector < 1e-20 {
            self.detector = 0.0;
        }
        if self.detector >= p.threshold {
            self.open = true;
            self.hold_left = p.hold_frames;
        } else if self.open {
            if self.detector >= p.close_threshold {
                self.hold_left = p.hold_frames;
            } else if self.hold_left > 0 {
                self.hold_left -= 1;
            } else {
                self.open = false;
            }
        }
        if !self.initialized {
            self.gain = if p.enabled { 0.0 } else { 1.0 };
            self.ramp_target = self.gain;
            self.initialized = true;
        }
        let target = if !p.enabled || self.open { 1.0 } else { 0.0 };
        if target != self.ramp_target {
            self.ramp_from = self.gain;
            self.ramp_target = target;
            self.ramp_position = 0;
            self.ramp_length = if target > self.gain {
                p.attack_frames
            } else {
                p.release_frames
            };
        }
        if self.ramp_position < self.ramp_length {
            self.ramp_position += 1;
            let t = self.ramp_position as f32 / self.ramp_length as f32;
            let smooth = t * t * (3.0 - 2.0 * t);
            self.gain = self.ramp_from + (self.ramp_target - self.ramp_from) * smooth;
            if self.ramp_position == self.ramp_length {
                self.gain = self.ramp_target;
            }
        }
        if self.gain == 1.0 {
            input
        } else if self.gain == 0.0 {
            [0.0; 2]
        } else {
            [input[0] * self.gain, input[1] * self.gain]
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn enabled(db: f32, sr: u32) -> InputNoiseParams {
        InputNoiseParams::from_config(
            InputNoiseConfig {
                enabled: true,
                threshold_db: db,
            },
            sr,
        )
    }
    #[test]
    fn input_noise_disabled_is_bit_exact_and_low_noise_is_fully_rejected() {
        let mut bypass = InputNoiseState::default();
        let off = InputNoiseParams::from_config(InputNoiseConfig::default(), 48000);
        let mut gate = InputNoiseState::default();
        let on = enabled(-50.0, 48000);
        for n in 0..48000 {
            let x = [(n as f32 * 0.19).sin() * 0.0001, -0.0002];
            assert_eq!(
                bypass.process(off, x).map(f32::to_bits),
                x.map(f32::to_bits)
            );
            assert_eq!(gate.process(on, x), [0.0; 2]);
        }
    }
    #[test]
    fn input_noise_stereo_is_linked_and_antiphase_is_not_cancelled() {
        let mut gate = InputNoiseState::default();
        let p = enabled(-30.0, 48000);
        for n in 0..2000 {
            let x = if n < 1000 { [0.1, -0.1] } else { [0.0, -0.1] };
            let y = gate.process(p, x);
            if n > 100 {
                assert_eq!(y, x);
            }
            assert!((0.0..=1.0).contains(&gate.gain));
        }
    }
    #[test]
    fn input_noise_hold_hysteresis_and_transition_endpoints_work_at_all_rates() {
        for sr in [8000, 44100, 48000, 96000, 192000] {
            let mut gate = InputNoiseState::default();
            let p = enabled(-20.0, sr);
            for _ in 0..p.attack_frames {
                gate.process(p, [0.2; 2]);
            }
            assert_eq!(gate.gain, 1.0);
            for _ in 0..sr / 2 {
                gate.process(p, [0.075; 2]);
            }
            assert_eq!(
                gate.gain, 1.0,
                "Between open and close thresholds must not chatter"
            );
            for _ in 0..sr / 2 {
                gate.process(p, [0.001; 2]);
            }
            assert_eq!(gate.gain, 0.0);
            assert_eq!(gate.process(p, [0.001; 2]), [0.0; 2]);
            let mut previous = gate.gain;
            for _ in 0..p.attack_frames {
                gate.process(p, [0.2; 2]);
                assert!(
                    gate.gain >= previous && gate.gain - previous <= 1.6 / p.attack_frames as f32
                );
                previous = gate.gain;
            }
            assert_eq!(gate.gain, 1.0);
        }
    }
    #[test]
    fn input_noise_toggling_and_threshold_automation_are_continuous_and_allocation_free() {
        let variants = [
            enabled(-20.0, 48000),
            enabled(-60.0, 48000),
            InputNoiseParams::from_config(InputNoiseConfig::default(), 48000),
        ];
        let mut gate = InputNoiseState::default();
        let allocations = crate::test_alloc::count(|| {
            for n in 0..48000 {
                let before = gate.gain;
                let initialized = gate.initialized;
                let y = gate.process(variants[(n / 377) % 3], [0.01, -0.005]);
                assert!(y[0].is_finite() && y[1].is_finite());
                if initialized {
                    assert!((gate.gain - before).abs() < 0.017);
                }
            }
        });
        assert_eq!(allocations, 0);
    }
}
