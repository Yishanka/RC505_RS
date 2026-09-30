use super::{audio_io::Diagnostics, loop_audio::Frame};
use ringbuf::HeapConsumer;
use std::sync::atomic::Ordering;

/// Slowly adapts capture to the output device clock. Cubic interpolation avoids
/// hard sample insertion/deletion in the normal drift range (up to 2000 ppm).
pub struct InputAdapter {
    history: [Frame; 4],
    phase: f64,
    ratio: f64,
    target: usize,
    primed: bool,
}
impl InputAdapter {
    pub fn reset_target(&mut self, target: usize) {
        self.target = target.max(16);
    }
    pub fn new(target: usize) -> Self {
        Self {
            history: [[0.0; 2]; 4],
            phase: 0.0,
            ratio: 1.0,
            target,
            primed: false,
        }
    }
    pub fn begin_block(&mut self, queued: usize, block: usize) {
        self.target = self.target.max(block).max(16);
        let error = (queued as f64 - self.target as f64 * 1.5) / self.target as f64;
        let desired = 1.0 + (error * 0.0005).clamp(-0.002, 0.002);
        self.ratio += (desired - self.ratio) * 0.01;
    }
    pub fn next(&mut self, input: &mut HeapConsumer<Frame>, diagnostics: &Diagnostics) -> Frame {
        if !self.primed {
            if input.len() < self.target + 4 {
                return [0.0; 2];
            }
            for value in &mut self.history {
                *value = input.pop().unwrap_or([0.0; 2]);
            }
            self.primed = true;
        }
        // A suspended device must not leave seconds of old input in the ring.
        if input.len() > self.target * 8 {
            let skip = input.len().saturating_sub(self.target * 2);
            for _ in 0..skip {
                let _ = input.pop();
            }
            diagnostics
                .overflow
                .fetch_add(skip as u64, Ordering::Relaxed);
        }
        let x = self.phase as f32;
        let result = std::array::from_fn(|ch| {
            let [a, b, c, d] = self.history.map(|v| v[ch]);
            b + 0.5
                * x
                * (c - a + x * (2.0 * a - 5.0 * b + 4.0 * c - d + x * (3.0 * (b - c) + d - a)))
        });
        self.phase += self.ratio;
        while self.phase >= 1.0 {
            self.phase -= 1.0;
            let Some(next) = input.pop() else {
                diagnostics.underrun.fetch_add(1, Ordering::Relaxed);
                self.primed = false;
                self.phase = 0.0;
                return [0.0; 2];
            };
            self.history.rotate_left(1);
            self.history[3] = next;
        }
        result
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Measurement {
    pub output_generation: u64,
    pub frames: u32,
    pub sample_rate: u32,
    pub correlation: f64,
    pub spread: u32,
}
pub struct Calibration {
    pub output_generation: u64,
    pub captured: Vec<f32>,
    signal: Vec<f32>,
    pattern: Vec<f32>,
    starts: [usize; 3],
    cursor: usize,
    sample_rate: u32,
}
impl Calibration {
    pub fn new(sr: u32) -> Self {
        let mut seed = 0x75c3u32;
        let pattern: Vec<f32> = (0..1023)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                if seed & 1 == 0 { 0.06 } else { -0.06 }
            })
            .collect();
        let starts = [sr as usize / 4, sr as usize, sr as usize * 7 / 4];
        let mut signal = vec![0.0; sr as usize * 3];
        for start in starts {
            signal[start..start + pattern.len()].copy_from_slice(&pattern);
        }
        Self {
            output_generation: 0,
            captured: vec![0.0; signal.len()],
            signal,
            pattern,
            starts,
            cursor: 0,
            sample_rate: sr,
        }
    }
    pub fn finished(&self) -> bool {
        self.cursor >= self.signal.len()
    }
    pub fn process(&mut self, input: Frame) -> Frame {
        if self.finished() {
            return [0.0; 2];
        }
        self.captured[self.cursor] = input[0];
        let value = self.signal[self.cursor];
        self.cursor += 1;
        [value, value]
    }
    pub fn analyze(&self) -> anyhow::Result<Measurement> {
        let pattern_energy = self
            .pattern
            .iter()
            .map(|v| (*v as f64).powi(2))
            .sum::<f64>();
        let mut lags = [0; 3];
        let mut minimum = 1.0f64;
        for (test, start) in self.starts.iter().copied().enumerate() {
            let mut best = (0.0, 0);
            for lag in 0..=self.sample_rate as usize / 2 {
                let window = &self.captured[start + lag..start + lag + self.pattern.len()];
                let (dot, energy) =
                    window
                        .iter()
                        .zip(&self.pattern)
                        .fold((0.0, 0.0), |(dot, energy), (&a, &b)| {
                            (dot + a as f64 * b as f64, energy + (a as f64).powi(2))
                        });
                let correlation = dot.abs() / (energy * pattern_energy).sqrt().max(1e-20);
                if correlation > best.0 {
                    best = (correlation, lag as u32);
                }
            }
            anyhow::ensure!(
                best.0 >= 0.70,
                "Loopback signal is missing or noisy (correlation {:.2}); no recommendation applied",
                best.0
            );
            lags[test] = best.1;
            minimum = minimum.min(best.0);
        }
        lags.sort();
        let spread = lags[2] - lags[0];
        anyhow::ensure!(
            spread <= (self.sample_rate as f64 * 0.0015).ceil() as u32,
            "Latency is unstable across probes; check driver/buffer settings"
        );
        Ok(Measurement {
            output_generation: self.output_generation,
            frames: lags[1],
            sample_rate: self.sample_rate,
            correlation: minimum,
            spread,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn correlation_recovers_exact_delay_and_rejects_silence() {
        let mut calibration = Calibration::new(8000);
        assert!(calibration.analyze().is_err());
        let delay = 173;
        for i in delay..calibration.signal.len() {
            calibration.captured[i] = calibration.signal[i - delay] * 0.4;
        }
        let result = calibration.analyze().unwrap();
        assert_eq!(result.frames, delay as u32);
        assert_eq!(result.spread, 0);
        assert!(result.correlation > 0.99999);
    }
}
