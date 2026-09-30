//! Output-only rate conversion. The renderer, loops and replay clock keep their
//! original rate when Windows changes output hardware. Equal rates bypass it.
use super::loop_audio::Frame;
const TAPS: usize = 64;
const PHASES: usize = 256;
pub struct OutputResampler {
    bypass: bool,
    step: f64,
    phase: f64,
    history: [Frame; TAPS],
    head: usize,
    coefficients: Vec<[f32; TAPS]>,
}
impl OutputResampler {
    pub fn new(source: u32, target: u32) -> Self {
        let bypass = source == target;
        let cutoff = (target as f64 / source as f64).min(1.0) * 0.94;
        let coefficients = if bypass {
            Vec::new()
        } else {
            (0..PHASES)
                .map(|p| {
                    let frac = p as f64 / PHASES as f64;
                    let mut row = std::array::from_fn(|i| {
                        let x = i as f64 - (TAPS / 2) as f64 - frac;
                        if x.abs() >= (TAPS / 2) as f64 {
                            return 0.0;
                        }
                        let sinc = if x.abs() < 1e-10 {
                            cutoff
                        } else {
                            (std::f64::consts::PI * x * cutoff).sin() / (std::f64::consts::PI * x)
                        };
                        let window =
                            0.5 + 0.5 * (std::f64::consts::PI * x / (TAPS / 2) as f64).cos();
                        (sinc * window.max(0.0)) as f32
                    });
                    let sum: f32 = row.iter().sum();
                    for v in &mut row {
                        *v /= sum;
                    }
                    row
                })
                .collect()
        };
        Self {
            bypass,
            step: source as f64 / target as f64,
            phase: 1.0,
            history: [[0.0; 2]; TAPS],
            head: 0,
            coefficients,
        }
    }
    pub fn next(&mut self, mut source: impl FnMut() -> Frame) -> Frame {
        if self.bypass {
            return source();
        }
        while self.phase >= 1.0 {
            self.history[self.head] = source();
            self.head = (self.head + 1) % TAPS;
            self.phase -= 1.0;
        }
        let row = &self.coefficients[(self.phase * PHASES as f64) as usize];
        let mut result = [0.0; 2];
        for (i, c) in row.iter().enumerate() {
            let value = self.history[(self.head + i) % TAPS];
            result[0] += value[0] * c;
            result[1] += value[1] * c;
        }
        self.phase += self.step;
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conversion_does_not_allocate_in_the_callback() {
        let mut r = OutputResampler::new(48000, 44100);
        assert_eq!(
            crate::test_alloc::count(|| {
                for _ in 0..1024 {
                    std::hint::black_box(r.next(|| [0.0; 2]));
                }
            }),
            0
        );
    }
    #[test]
    fn preserves_rate_duration_dc_and_equal_rate_samples() {
        let mut equal = OutputResampler::new(48000, 48000);
        assert_eq!(equal.next(|| [0.123, -0.456]), [0.123, -0.456]);
        for (source, target) in [(48000, 44100), (44100, 48000), (48000, 16000)] {
            let mut r = OutputResampler::new(source, target);
            let mut consumed = 0;
            for i in 0..target {
                let value = r.next(|| {
                    consumed += 1;
                    [0.25, -0.5]
                });
                if i > 200 {
                    assert!((value[0] - 0.25).abs() < 1e-5);
                    assert!((value[1] + 0.5).abs() < 1e-5);
                }
            }
            assert!((consumed - source as i64).abs() <= 3);
        }
    }
    #[test]
    fn downsampling_rejects_out_of_band_tones_and_preserves_in_band_pitch() {
        let power = |hz: f64| {
            let mut resampler = OutputResampler::new(48000, 16000);
            let mut at = 0;
            let mut sum = 0.0;
            for i in 0..16000 {
                let value = resampler.next(|| {
                    let x = (2.0 * std::f64::consts::PI * hz * at as f64 / 48000.0).sin() as f32;
                    at += 1;
                    [x, x]
                });
                if i > 200 {
                    sum += value[0] as f64 * value[0] as f64;
                }
            }
            sum / 15799.0
        };
        assert!((power(1000.0) - 0.5).abs() < 0.01);
        assert!(power(12000.0) < 0.00005);
    }
}
