//! Immutable band-limited tables and sample decimation levels, built off-thread.
use crate::config::osc_configs::{SampleAsset, Waveform};
use std::sync::Arc;
#[derive(Clone)]
pub struct SamplePyramid {
    pub(super) levels: Vec<Vec<f32>>,
}
impl SamplePyramid {
    pub(super) fn prepare(sample: &Arc<SampleAsset>) -> Arc<Self> {
        use std::{
            collections::VecDeque,
            sync::{Mutex, OnceLock},
        };
        static CACHE: OnceLock<Mutex<VecDeque<(u64, usize, Arc<SamplePyramid>)>>> = OnceLock::new();
        let cache = CACHE.get_or_init(|| Mutex::new(VecDeque::new()));
        if let Ok(entries) = cache.lock() {
            if let Some((_, _, value)) = entries
                .iter()
                .find(|(hash, len, _)| *hash == sample.content_hash && *len == sample.frames.len())
            {
                return value.clone();
            }
        }
        let mut kernel: [f32; 63] = std::array::from_fn(|i| {
            let x = i as f64 - 31.0;
            let cutoff = 0.47;
            let sinc = if x.abs() < 1e-12 {
                cutoff
            } else {
                (std::f64::consts::PI * x * cutoff).sin() / (std::f64::consts::PI * x)
            };
            (sinc * (0.5 + 0.5 * (std::f64::consts::PI * x / 31.0).cos())) as f32
        });
        let sum: f32 = kernel.iter().sum();
        for k in &mut kernel {
            *k /= sum;
        }
        let mut levels: Vec<Vec<f32>> = Vec::new();
        loop {
            let data = levels.last().map_or(&sample.frames[..], |v| &v[..]);
            if data.len() < 64 || levels.len() >= 11 {
                break;
            }
            let next = (0..data.len().div_ceil(2))
                .map(|i| {
                    kernel
                        .iter()
                        .enumerate()
                        .map(|(j, k)| {
                            data[(i as isize * 2 + j as isize - 31)
                                .clamp(0, data.len() as isize - 1)
                                as usize]
                                * k
                        })
                        .sum()
                })
                .collect();
            levels.push(next);
        }
        let value = Arc::new(Self { levels });
        if let Ok(mut entries) = cache.lock() {
            if entries.len() >= 16 {
                entries.pop_front();
            }
            entries.push_back((sample.content_hash, sample.frames.len(), value.clone()));
        }
        value
    }
}

pub(super) fn periodic_sample(data: &[f32], pos: f64, crossfade: bool) -> f32 {
    let len = data.len();
    let p = pos.rem_euclid(len as f64);
    let a = p.floor() as usize;
    let frac = (p - a as f64) as f32;
    let value = data[a] * (1.0 - frac) + data[(a + 1) % len] * frac;
    let fade = (len / 32).clamp(1, 128) as f64;
    if crossfade && p > len as f64 - fade {
        let mix = ((p - (len as f64 - fade)) / fade) as f32;
        value * (1.0 - mix) + data[((p - (len as f64 - fade)) as usize).min(len - 1)] * mix
    } else {
        value
    }
}

/// Band-limited octave tables, generated off the callback using a fixed 2048-point
/// FFT. Sample material is one normalized wave; no integer period quantization.
#[derive(Clone)]
pub struct WaveBank {
    tables: Vec<Vec<f32>>,
}
#[derive(Clone, Copy, Default)]
struct Complex {
    re: f32,
    im: f32,
}
fn fft(data: &mut [Complex], inverse: bool) {
    let n = data.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            data.swap(i, j);
        }
    }
    let mut width = 2;
    while width <= n {
        let angle = std::f32::consts::TAU / width as f32 * if inverse { 1.0 } else { -1.0 };
        let (wi, wr) = angle.sin_cos();
        for base in (0..n).step_by(width) {
            let (mut re, mut im) = (1.0, 0.0);
            for k in 0..width / 2 {
                let a = data[base + k];
                let b = data[base + k + width / 2];
                let (br, bi) = (b.re * re - b.im * im, b.re * im + b.im * re);
                data[base + k] = Complex {
                    re: a.re + br,
                    im: a.im + bi,
                };
                data[base + k + width / 2] = Complex {
                    re: a.re - br,
                    im: a.im - bi,
                };
                (re, im) = (re * wr - im * wi, re * wi + im * wr);
            }
        }
        width *= 2;
    }
    if inverse {
        for x in data {
            x.re /= n as f32;
            x.im /= n as f32;
        }
    }
}
impl WaveBank {
    pub(super) fn prepare(c: &crate::config::OscillatorConfigs) -> Arc<Self> {
        use std::collections::VecDeque;
        use std::sync::{Mutex, OnceLock};
        type Key = (u64, u32, u32, u32, u8);
        static CACHE: OnceLock<Mutex<VecDeque<(Key, Arc<WaveBank>)>>> = OnceLock::new();
        let key = (
            c.sample.as_ref().map_or(0, |s| s.content_hash),
            c.sample_start.to_bits(),
            c.sample_end.to_bits(),
            c.vocal_formant.to_bits(),
            c.waveform.value as u8,
        );
        let cache = CACHE.get_or_init(|| Mutex::new(VecDeque::new()));
        if let Ok(entries) = cache.lock() {
            if let Some((_, value)) = entries.iter().find(|(k, _)| *k == key) {
                return value.clone();
            }
        }
        const N: usize = 2048;
        let mut wave = vec![Complex::default(); N];
        if c.waveform.value == Waveform::Vocal {
            let morph = if c.vocal_formant.is_finite() {
                c.vocal_formant.clamp(0.0, 1.0)
            } else {
                0.0
            };
            for (i, x) in wave.iter_mut().enumerate() {
                let phase = i as f32 / N as f32;
                for h in 1..=32 {
                    let a = 0.15 / h as f32
                        + (-0.5 * ((h as f32 - (3.0 - 1.5 * morph)) / 0.7).powi(2)).exp()
                        + 0.7 * (-0.5 * ((h as f32 - (4.5 + 4.0 * morph)) / 1.0).powi(2)).exp();
                    x.re += (phase * std::f32::consts::TAU * h as f32).sin() * a;
                }
            }
        } else if c.waveform.value == Waveform::Triangle {
            for (i, x) in wave.iter_mut().enumerate() {
                x.re = 1.0 - 4.0 * (i as f32 / N as f32 - 0.5).abs();
            }
        } else if let Some(s) = &c.sample {
            if s.frames.len() >= 4 {
                let (start, end) = sample_bounds(s.frames.len(), c.sample_start, c.sample_end);
                let len = end - start;
                for (i, x) in wave.iter_mut().enumerate() {
                    x.re = periodic_sample(
                        &s.frames[start..start + len],
                        i as f64 * len as f64 / N as f64,
                        true,
                    );
                }
            }
        }
        let mean = wave.iter().map(|x| x.re).sum::<f32>() / N as f32;
        let peak = wave
            .iter()
            .map(|x| (x.re - mean).abs())
            .fold(0.0f32, f32::max)
            .max(0.001);
        for x in &mut wave {
            x.re = (x.re - mean) / peak;
        }
        fft(&mut wave, false);
        let mut tables = Vec::with_capacity(10);
        for octave in 0..10 {
            let limit = 512usize >> octave;
            let mut spectrum = wave.clone();
            for (i, x) in spectrum.iter_mut().enumerate() {
                if i == 0 || (i > limit && i < N - limit) {
                    *x = Complex::default();
                }
            }
            fft(&mut spectrum, true);
            let mut table: Vec<_> = spectrum.iter().map(|x| x.re).collect();
            table.push(table[0]);
            tables.push(table);
        }
        let value = Arc::new(Self { tables });
        if let Ok(mut entries) = cache.lock() {
            if entries.len() >= 32 {
                entries.pop_front();
            }
            entries.push_back((key, value.clone()));
        }
        value
    }
    pub(super) fn level(frequency: f32, sr: f32) -> usize {
        let allowed = (sr * 0.45 / frequency.max(0.1)).max(1.0);
        ((512.0 / allowed).log2().max(0.0).ceil() as usize).min(9)
    }
    pub(super) fn read(&self, phase: f64, frequency: f32, sr: f32) -> f32 {
        self.read_level(phase, Self::level(frequency, sr))
    }
    pub(super) fn read_transition(&self, phase: f64, level: usize, relative_frequency: f32) -> f32 {
        let lower = self.read_level(phase, level);
        if level >= 9 || relative_frequency <= 0.8 {
            return lower;
        }
        let blend = ((relative_frequency - 0.8) * 5.0).clamp(0.0, 1.0);
        lower + (self.read_level(phase, level + 1) - lower) * blend
    }
    fn read_level(&self, phase: f64, level: usize) -> f32 {
        let data = &self.tables[level];
        let p = phase * 2048.0;
        let i = (p as usize).min(2047);
        let frac = (p - i as f64) as f32;
        data[i] * (1.0 - frac) + data[i + 1] * frac
    }
}

pub(super) fn sample_bounds(length: usize, start: f32, end: f32) -> (usize, usize) {
    let start = if start.is_finite() {
        start.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let end = if end.is_finite() {
        end.clamp(0.0, 1.0)
    } else {
        1.0
    };
    let a = ((start * length as f32).round() as usize).min(length.saturating_sub(2));
    let b = ((end * length as f32).round() as usize)
        .min(length)
        .max((a + 2).min(length));
    (a, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalized_regions_round_back_to_exact_sample_boundaries() {
        for (a, b) in [(0, 48), (12345, 12393), (95998, 96000), (0, 96000)] {
            assert_eq!(
                sample_bounds(96000, a as f32 / 96000.0, b as f32 / 96000.0),
                (a, b)
            );
        }
        assert_eq!(sample_bounds(96000, f32::NAN, f32::NAN), (0, 96000));
    }
}
