//! Stereo FFT analysis on a worker. The callback only writes to a bounded ring;
//! visualization may drop frames under load, but never blocks audio rendering.
use super::loop_audio::Frame;
use ringbuf::{HeapConsumer, HeapProducer, HeapRb};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
pub const BARS: usize = 64;
pub struct SpectrumFeed {
    input: HeapProducer<Frame>,
    output: HeapConsumer<[f32; BARS]>,
    latest: [f32; BARS],
    enabled: bool,
    active: Arc<AtomicBool>,
}
impl SpectrumFeed {
    pub fn new(sr: u32, stop: Arc<AtomicBool>) -> Self {
        let (input, mut frames) = HeapRb::<Frame>::new(8192).split();
        let (mut spectra, output) = HeapRb::new(2).split();
        let mut analyzer = Analyzer::new(sr);
        let active = Arc::new(AtomicBool::new(true));
        let worker_active = active.clone();
        std::thread::Builder::new()
            .name("monitor-spectrum".into())
            .spawn(move || {
                while !stop.load(Ordering::Acquire) {
                    for _ in 0..8192 {
                        let Some(frame) = frames.pop() else {
                            break;
                        };
                        if !worker_active.load(Ordering::Relaxed) {
                            continue;
                        }
                        if let Some(bars) = analyzer.push(frame) {
                            let _ = spectra.push(bars);
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(4));
                }
            })
            .expect("spectrum worker");
        Self {
            input,
            output,
            latest: [0.0; BARS],
            enabled: true,
            active,
        }
    }
    pub fn push(&mut self, frame: Frame) {
        if self.enabled {
            let _ = self.input.push(frame);
        }
    }
    pub fn set_enabled(&mut self, value: bool) {
        self.enabled = value;
        self.active.store(value, Ordering::Relaxed);
        if !value {
            self.latest = [0.0; BARS];
        }
    }
    pub fn snapshot(&mut self) -> [f32; BARS] {
        while let Some(bars) = self.output.pop() {
            if self.enabled {
                self.latest = bars;
            }
        }
        self.latest
    }
}

struct Analyzer {
    samples: Vec<Frame>,
    work: Vec<[f32; 2]>,
    window: Vec<f32>,
    twiddles: Vec<[f32; 2]>,
    reversed: Vec<usize>,
    power: Vec<f32>,
    ranges: [(usize, usize); BARS],
    bars: [f32; BARS],
    position: usize,
    count: usize,
    hop: usize,
    scale: f32,
    decay: f32,
}
impl Analyzer {
    fn new(sr: u32) -> Self {
        let n = if sr <= 48000 {
            4096usize
        } else if sr <= 96000 {
            8192
        } else {
            16384
        };
        let window = (0..n)
            .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n as f32).cos())
            .collect::<Vec<_>>();
        let scale = 2.0 / window.iter().sum::<f32>();
        let high = (sr as f32 * 0.5).min(20000.0);
        let ranges = std::array::from_fn(|i| {
            let edge = |j| 20.0 * (high / 20.0).powf(j as f32 / BARS as f32);
            let lo = (edge(i) * n as f32 / sr as f32).round() as usize;
            let hi = (edge(i + 1) * n as f32 / sr as f32).round() as usize;
            (lo.clamp(1, n / 2), hi.max(lo + 1).clamp(2, n / 2 + 1))
        });
        Self {
            samples: vec![[0.0; 2]; n],
            work: vec![[0.0; 2]; n],
            window,
            twiddles: (0..n / 2)
                .map(|i| {
                    let a = -std::f32::consts::TAU * i as f32 / n as f32;
                    [a.cos(), a.sin()]
                })
                .collect(),
            reversed: (0..n)
                .map(|i| i.reverse_bits() >> (usize::BITS - n.ilog2()))
                .collect(),
            power: vec![0.0; n / 2 + 1],
            ranges,
            bars: [0.0; BARS],
            position: 0,
            count: 0,
            hop: (sr / 30).max(1) as usize,
            scale,
            decay: (-1.0f32 / (30.0 * 0.18)).exp(),
        }
    }
    fn push(&mut self, frame: Frame) -> Option<[f32; BARS]> {
        self.samples[self.position] = frame.map(|x| {
            if x.is_finite() && x.abs() > 1e-12 {
                x
            } else {
                0.0
            }
        });
        self.position = (self.position + 1) % self.samples.len();
        self.count += 1;
        if self.count < self.hop {
            return None;
        }
        self.count = 0;
        if self.samples.iter().all(|v| *v == [0.0; 2]) {
            for value in &mut self.bars {
                *value *= self.decay;
                if *value < 0.001 {
                    *value = 0.0;
                }
            }
            return Some(self.bars);
        }
        self.power.fill(0.0);
        let n = self.samples.len();
        for ch in 0..2 {
            for i in 0..n {
                self.work[self.reversed[i]] = [
                    self.samples[(self.position + i) % n][ch] * self.window[i],
                    0.0,
                ];
            }
            let mut size = 2;
            while size <= n {
                for start in (0..n).step_by(size) {
                    for j in 0..size / 2 {
                        let w = self.twiddles[j * n / size];
                        let b = self.work[start + j + size / 2];
                        let a = self.work[start + j];
                        let t = [b[0] * w[0] - b[1] * w[1], b[0] * w[1] + b[1] * w[0]];
                        self.work[start + j] = [a[0] + t[0], a[1] + t[1]];
                        self.work[start + j + size / 2] = [a[0] - t[0], a[1] - t[1]];
                    }
                }
                size *= 2;
            }
            for (power, value) in self.power.iter_mut().zip(&self.work) {
                *power +=
                    (value[0] * value[0] + value[1] * value[1]) * 0.5 * self.scale * self.scale;
            }
        }
        for i in 0..BARS {
            let (lo, hi) = self.ranges[i];
            let peak = self.power[lo..hi].iter().copied().fold(0.0f32, f32::max);
            let target = ((10.0 * peak.max(1e-10).log10() + 80.0) / 80.0).clamp(0.0, 1.0);
            self.bars[i] = target.max(self.bars[i] * self.decay);
            if self.bars[i] < 0.001 {
                self.bars[i] = 0.0;
            }
        }
        Some(self.bars)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spectrum_locates_bass_and_treble_preserves_antiphase_and_allocates_nothing() {
        for sr in [8000, 48000, 192000] {
            for hz in [60.0, 1000.0, 3000.0] {
                let mut analyzer = Analyzer::new(sr);
                let mut bars = [0.0; BARS];
                let count = crate::test_alloc::count(|| {
                    for i in 0..sr / 2 {
                        let x = (std::f32::consts::TAU * hz * i as f32 / sr as f32).sin() * 0.5;
                        if let Some(v) = analyzer.push([x, -x]) {
                            bars = v;
                        }
                    }
                });
                assert_eq!(count, 0);
                let bin = bars
                    .iter()
                    .enumerate()
                    .max_by(|a, b| a.1.total_cmp(b.1))
                    .unwrap()
                    .0;
                let (lo, hi) = analyzer.ranges[bin];
                let found = hz * analyzer.samples.len() as f32 / sr as f32;
                assert!(
                    found >= lo as f32 - 1.5 && found <= hi as f32 + 1.5,
                    "{sr} Hz: {hz} -> {bin}"
                );
                assert!(bars[bin] > 0.75);
                for _ in 0..sr * 3 {
                    if let Some(v) = analyzer.push([0.0; 2]) {
                        bars = v;
                    }
                }
                assert!(bars.iter().all(|v| *v == 0.0));
            }
        }
    }
}
