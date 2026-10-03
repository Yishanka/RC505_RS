//! Bounded, decimated YIN difference detector for monophonic voiced input.
//! The original YIN method is de Cheveigné/Kawahara (JASA 2002).
//! Analysis is spread across samples to avoid a full autocorrelation callback spike.
#[derive(Clone)]
pub struct PitchTracker {
    ring: Box<[f32; 1024]>,
    snapshot: Box<[f32; 768]>,
    diff: Box<[f32; 257]>,
    write: usize,
    filled: usize,
    down_count: usize,
    decimate: usize,
    sample_rate: f32,
    sum: f32,
    lowpass: f32,
    lowpass_alpha: f32,
    hop: usize,
    tau: usize,
    max_tau: usize,
    min_tau: usize,
    analyzing: bool,
    pub frequency: Option<f32>,
}
impl PitchTracker {
    pub fn new(sr: f32) -> Self {
        let decimate = (sr / 12000.0).round().max(1.0) as usize;
        let sample_rate = sr / decimate as f32;
        Self {
            ring: Box::new([0.0; 1024]),
            snapshot: Box::new([0.0; 768]),
            diff: Box::new([0.0; 257]),
            write: 0,
            filled: 0,
            down_count: 0,
            decimate,
            sample_rate,
            sum: 0.0,
            lowpass: 0.0,
            lowpass_alpha: 1.0 - (-std::f32::consts::TAU * 2000.0 / sr).exp(),
            hop: 0,
            tau: 1,
            max_tau: (sample_rate / 65.0).min(256.0) as usize,
            min_tau: (sample_rate / 1000.0).max(2.0) as usize,
            analyzing: false,
            frequency: None,
        }
    }
    pub fn reset(&mut self) {
        self.ring.fill(0.0);
        self.filled = 0;
        self.write = 0;
        self.analyzing = false;
        self.frequency = None;
        self.lowpass = 0.0;
        self.hop = 0;
        self.down_count = 0;
    }
    pub fn next(&mut self, input: f32) -> Option<f32> {
        self.lowpass += (input - self.lowpass) * self.lowpass_alpha;
        self.down_count += 1;
        if self.down_count < self.decimate {
            return self.frequency;
        }
        self.down_count = 0;
        self.ring[self.write] = self.lowpass;
        self.write = (self.write + 1) % 1024;
        self.filled = (self.filled + 1).min(1024);
        self.hop += 1;
        // Two lag candidates per decimated sample: <=512 subtractions per step,
        // independent of callback size. Freeze the analysis window for consistency.
        if self.analyzing {
            for _ in 0..2 {
                if self.tau > self.max_tau {
                    self.finish();
                    break;
                }
                let tau = self.tau;
                let mut d = 0.0;
                for j in 0..256 {
                    let delta = self.snapshot[j] - self.snapshot[j + tau];
                    d += delta * delta;
                }
                self.sum += d;
                self.diff[tau] = if self.sum > 1e-12 {
                    d * tau as f32 / self.sum
                } else {
                    1.0
                };
                self.tau += 1;
            }
        }
        if !self.analyzing && self.filled >= 768 && self.hop >= 128 {
            self.hop = 0;
            self.sum = 0.0;
            self.tau = 1;
            self.analyzing = true;
            for j in 0..768 {
                self.snapshot[j] = self.ring[(self.write + 1023 - j) % 1024];
            }
        }
        self.frequency
    }
    fn finish(&mut self) {
        self.analyzing = false;
        let energy = self.snapshot[..256].iter().map(|v| v * v).sum::<f32>() / 256.0;
        if energy < 0.000001 {
            self.frequency = None;
            return;
        }
        let mut tau = self.min_tau;
        while tau < self.max_tau {
            if self.diff[tau] < 0.16 {
                while tau + 1 < self.max_tau && self.diff[tau + 1] < self.diff[tau] {
                    tau += 1;
                }
                let a = self.diff[tau - 1];
                let b = self.diff[tau];
                let c = self.diff[tau + 1];
                let divisor = 2.0 * (a - 2.0 * b + c);
                let offset = if divisor.abs() > 1e-6 {
                    ((a - c) / divisor).clamp(-0.5, 0.5)
                } else {
                    0.0
                };
                self.frequency = Some(self.sample_rate / (tau as f32 + offset));
                return;
            }
            tau += 1;
        }
        self.frequency = None;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tracks_voice_range_and_rejects_silence_without_allocating() {
        for sr in [8000.0, 44100.0, 48000.0, 96000.0] {
            for hz in [82.41, 220.0, 440.0] {
                let mut d = PitchTracker::new(sr);
                let count = crate::test_alloc::count(|| {
                    for n in 0..sr as usize / 2 {
                        d.next((std::f32::consts::TAU * hz * n as f32 / sr).sin() * 0.2);
                    }
                });
                assert_eq!(count, 0);
                let detected = d.frequency.unwrap();
                assert!((detected / hz - 1.0).abs() < 0.01, "{sr}/{hz}: {detected}");
                for _ in 0..sr as usize / 3 {
                    d.next(0.0);
                }
                assert!(d.frequency.is_none());
            }
        }
    }
}
