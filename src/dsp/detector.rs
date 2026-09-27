//! A peak envelope prevents threshold gates from reopening at each waveform cycle.
#[derive(Clone, Copy)]
pub struct PeakFollower {
    level: f32,
    release: f32,
    sample_rate: f32,
}
impl Default for PeakFollower {
    fn default() -> Self {
        Self {
            level: 0.0,
            release: 0.0,
            sample_rate: 0.0,
        }
    }
}
impl PeakFollower {
    pub fn next(&mut self, input: f32, sample_rate: f32) -> f32 {
        if sample_rate != self.sample_rate {
            self.sample_rate = sample_rate;
            self.release = (-1.0 / (0.03 * sample_rate.max(1.0))).exp();
        }
        self.level = input.abs().max(self.level * self.release);
        self.level
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn threshold_stays_open_across_audio_zero_crossings() {
        let mut env = PeakFollower::default();
        for sample in 0..4800 {
            let level = env.next(
                (sample as f32 * std::f32::consts::TAU * 100.0 / 48000.0).sin() * 0.5,
                48000.0,
            );
            if sample > 480 {
                assert!(level > 0.3);
            }
        }
        for _ in 0..48000 {
            env.next(0.0, 48000.0);
        }
        assert!(env.level < 1e-6);
    }
}
