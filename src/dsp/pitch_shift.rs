//! Streaming, stereo phase-vocoder pitch shift. Bin phase differences estimate
//! true frequency; synthesis remaps frequencies rather than replaying buffers.
//! The two channels share one packed complex FFT but retain their relative phase.
use std::{
    collections::HashMap,
    f32::consts::{PI, TAU},
    sync::{Arc, Mutex, OnceLock, Weak},
};
pub fn latency_frames(sr: f32) -> usize {
    ((sr * 0.04).ceil() as usize)
        .max(64)
        .next_power_of_two()
        .min(8192)
        - 1
}

#[derive(Clone)]
pub struct Plan {
    n: usize,
    window: Vec<f32>,
    twiddle: Vec<[f32; 2]>,
    reversed: Vec<usize>,
}
impl Plan {
    pub fn new(sr: f32) -> Arc<Self> {
        let n = latency_frames(sr) + 1;
        Self::with_size(n)
    }
    fn with_size(n: usize) -> Arc<Self> {
        // Construction only: immutable plans are shared across slots, tracks and
        // replay renderers. Audio processing neither locks nor mutates this cache.
        static PLANS: OnceLock<Mutex<HashMap<usize, Weak<Plan>>>> = OnceLock::new();
        let mut plans = PLANS
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(plan) = plans.get(&n).and_then(Weak::upgrade) {
            return plan;
        }
        let plan = Arc::new(Self {
            n,
            window: (0..n)
                .map(|i| 0.5 - 0.5 * (TAU * i as f32 / n as f32).cos())
                .collect(),
            twiddle: (0..n / 2)
                .map(|i| {
                    let a = TAU * i as f32 / n as f32;
                    [a.cos(), -a.sin()]
                })
                .collect(),
            reversed: (0..n)
                .map(|i| i.reverse_bits() >> (usize::BITS - n.ilog2()))
                .collect(),
        });
        plans.insert(n, Arc::downgrade(&plan));
        plan
    }
    fn fft(&self, data: &mut [[f32; 2]], inverse: bool) {
        for i in 0..self.n {
            let j = self.reversed[i];
            if j > i {
                data.swap(i, j);
            }
        }
        let mut size = 2;
        while size <= self.n {
            for start in (0..self.n).step_by(size) {
                for j in 0..size / 2 {
                    let mut w = self.twiddle[j * self.n / size];
                    if inverse {
                        w[1] = -w[1];
                    }
                    let a = data[start + j];
                    let b = data[start + j + size / 2];
                    let t = [b[0] * w[0] - b[1] * w[1], b[0] * w[1] + b[1] * w[0]];
                    data[start + j] = [a[0] + t[0], a[1] + t[1]];
                    data[start + j + size / 2] = [a[0] - t[0], a[1] - t[1]];
                }
            }
            size *= 2;
        }
        if inverse {
            for v in data {
                v[0] /= self.n as f32;
                v[1] /= self.n as f32;
            }
        }
    }
}
#[derive(Clone)]
pub struct PitchShift {
    envelope: FormantEnvelope,
    source_magnitudes: Vec<[f32; 2]>,
    preserve_formants: bool,
    formant_ratio: f32,
    fundamental_hint: f32,
    envelope_tick: u8,
    envelope_ready: bool,
    plan: Arc<Plan>,
    input: Vec<[f32; 2]>,
    work: Vec<[f32; 2]>,
    output: Vec<[f32; 2]>,
    epochs: Vec<u64>,
    last_phase: Vec<[f32; 2]>,
    sum_phase: Vec<[f32; 2]>,
    magnitude: Vec<[f32; 2]>,
    frequency: Vec<[f32; 2]>,
    phase_difference: Vec<f32>,
    phase_strength: Vec<f32>,
    active: Vec<[bool; 2]>,
    position: usize,
    filled: usize,
    hop_count: usize,
    epoch: u64,
    primed: bool,
    offset: usize,
}

/// Bounded, subsampled true-envelope approximation: max-pool narrow spectral
/// regions, then three cepstral upper-envelope projections with a Hamming lifter.
/// The 512-point envelope FFT is separate from (and smaller than) the audio FFT.
/// Based on the public Röbel/Rodet DAFx2005 and source/filter PV literature;
/// fixed iteration count is deliberate, not a claim of exact convergence.
#[derive(Clone)]
struct FormantEnvelope {
    plan: Arc<Plan>,
    observed: Vec<[f32; 2]>,
    upper: Vec<[f32; 2]>,
    estimate: Vec<[f32; 2]>,
    work: Vec<[f32; 2]>,
    ranges: Vec<(usize, usize)>,
    lifters: Vec<Vec<f32>>,
    orders: [f32; 7],
    virtual_rate: f32,
    bin_to_grid: f32,
    valid: [bool; 2],
}
impl FormantEnvelope {
    fn new(audio_n: usize, sr: f32) -> Self {
        let n = 512;
        let high = (sr * 0.5).min(12000.0).max(500.0);
        let virtual_rate = high * 2.0;
        let bin_hz = sr / audio_n as f32;
        let grid_hz = high / (n / 2) as f32;
        let orders = [16.0, 24.0, 32.0, 48.0, 64.0, 96.0, 128.0];
        Self {
            plan: Plan::with_size(n),
            observed: vec![[0.0; 2]; n / 2 + 1],
            upper: vec![[0.0; 2]; n / 2 + 1],
            estimate: vec![[0.0; 2]; n / 2 + 1],
            work: vec![[0.0; 2]; n],
            ranges: (0..=n / 2)
                .map(|i| {
                    let lo = (((i as f32 - 0.5).max(0.0) * grid_hz) / bin_hz).floor() as usize;
                    let hi = (((i as f32 + 0.5) * grid_hz) / bin_hz).ceil() as usize;
                    (lo.min(audio_n / 2), hi.max(lo + 1).min(audio_n / 2 + 1))
                })
                .collect(),
            lifters: orders
                .iter()
                .map(|cutoff| {
                    (0..n)
                        .map(|i| {
                            let q = i.min(n - i) as f32;
                            if q > *cutoff {
                                0.0
                            } else {
                                0.54 + 0.46 * (PI * q / cutoff).cos()
                            }
                        })
                        .collect()
                })
                .collect(),
            orders,
            virtual_rate,
            bin_to_grid: bin_hz / grid_hz,
            valid: [false; 2],
        }
    }
    fn update(&mut self, magnitudes: &[[f32; 2]], fundamental: f32) {
        let target = (self.virtual_rate * 0.005).min(if fundamental > 0.0 {
            self.virtual_rate * 0.45 / fundamental
        } else {
            f32::INFINITY
        });
        let order = (0..self.orders.len())
            .min_by(|a, b| {
                (self.orders[*a] - target)
                    .abs()
                    .total_cmp(&(self.orders[*b] - target).abs())
            })
            .unwrap_or(3);
        let mut peak = [0.0f32; 2];
        for m in magnitudes {
            for ch in 0..2 {
                peak[ch] = peak[ch].max(m[ch]);
            }
        }
        for ch in 0..2 {
            let peaks = (1..magnitudes.len() - 1)
                .filter(|i| {
                    magnitudes[*i][ch] > peak[ch] * 0.05
                        && magnitudes[*i][ch] > magnitudes[*i - 1][ch]
                        && magnitudes[*i][ch] >= magnitudes[*i + 1][ch]
                })
                .count();
            self.valid[ch] =
                peaks >= 3 && peak[ch] > peak[0].max(peak[1]) * 0.0001 && peak[ch] > 1e-8;
        }
        for (i, (lo, hi)) in self.ranges.iter().copied().enumerate() {
            for ch in 0..2 {
                let local = magnitudes[lo..hi].iter().fold(0.0f32, |m, v| m.max(v[ch]));
                let log = local.max((peak[ch] * 1e-5).max(1e-10)).ln();
                self.observed[i][ch] = log;
                self.upper[i][ch] = log;
                self.estimate[i][ch] = log;
            }
        }
        let n = self.plan.n;
        for _ in 0..3 {
            for i in 0..n {
                let k = i.min(n - i);
                self.work[i] = self.upper[k];
            }
            self.plan.fft(&mut self.work, true);
            for (sample, lifter) in self.work.iter_mut().zip(&self.lifters[order]) {
                sample[0] *= *lifter;
                sample[1] *= *lifter;
            }
            self.plan.fft(&mut self.work, false);
            for i in 0..=n / 2 {
                self.estimate[i] = self.work[i];
                for ch in 0..2 {
                    self.upper[i][ch] = self.upper[i][ch].max(self.estimate[i][ch]);
                }
            }
        }
    }
    fn log_at(&self, bin: f32, ch: usize) -> f32 {
        let x = (bin * self.bin_to_grid).clamp(0.0, (self.estimate.len() - 1) as f32);
        let i = x as usize;
        let j = (i + 1).min(self.estimate.len() - 1);
        let f = x - i as f32;
        self.estimate[i][ch] * (1.0 - f) + self.estimate[j][ch] * f
    }
    fn correction(&self, source_bin: f32, target_bin: f32, ch: usize) -> f32 {
        if !self.valid[ch] {
            return 1.0;
        }
        // Bound valley whitening to ±36 dB; a separate frame-energy guard avoids
        // arbitrarily boosting a narrow spectral hole. No noise is generated.
        (self.log_at(target_bin, ch) - self.log_at(source_bin, ch))
            .clamp(-4.158883, 4.158883)
            .exp()
    }
}
impl PitchShift {
    pub fn new(plan: Arc<Plan>) -> Self {
        Self::new_with_offset(plan, 0)
    }
    pub fn new_with_offset(plan: Arc<Plan>, offset: usize) -> Self {
        Self::new_with_offset_and_rate(plan, offset, 48000.0)
    }
    pub fn new_with_offset_and_rate(plan: Arc<Plan>, offset: usize, sr: f32) -> Self {
        let n = plan.n;
        let offset = offset % (n / 4);
        Self {
            envelope: FormantEnvelope::new(n, sr),
            source_magnitudes: vec![[0.0; 2]; n / 2 + 1],
            preserve_formants: false,
            formant_ratio: 1.0,
            fundamental_hint: 0.0,
            envelope_tick: 0,
            envelope_ready: false,
            plan,
            input: vec![[0.0; 2]; n],
            work: vec![[0.0; 2]; n],
            output: vec![[0.0; 2]; n],
            epochs: vec![0; n],
            last_phase: vec![[0.0; 2]; n / 2 + 1],
            sum_phase: vec![[0.0; 2]; n / 2 + 1],
            magnitude: vec![[0.0; 2]; n / 2 + 1],
            frequency: vec![[0.0; 2]; n / 2 + 1],
            phase_difference: vec![0.0; n / 2 + 1],
            phase_strength: vec![0.0; n / 2 + 1],
            active: vec![[false; 2]; n / 2 + 1],
            position: 0,
            filled: 0,
            hop_count: n / 4 - offset - 1,
            epoch: 1,
            primed: false,
            offset,
        }
    }
    pub fn reset(&mut self) {
        self.envelope_ready = false;
        self.envelope_tick = 0;
        self.position = 0;
        self.filled = 0;
        self.hop_count = self.plan.n / 4 - self.offset - 1;
        self.epoch = self.epoch.wrapping_add(1).max(1);
        self.primed = false;
    }
    pub fn latency_frames(&self) -> usize {
        self.plan.n - 1
    }
    pub fn set_formants(&mut self, preserve: bool, ratio: f32) {
        if preserve && !self.preserve_formants {
            self.envelope_ready = false;
        }
        self.preserve_formants = preserve;
        self.formant_ratio = ratio.clamp(0.5, 2.0);
    }
    pub fn set_fundamental_hint(&mut self, hz: f32) {
        self.fundamental_hint = hz.max(0.0);
    }
    pub fn next(&mut self, input: [f32; 2], ratio: f32) -> [f32; 2] {
        let n = self.plan.n;
        self.input[self.position] = input;
        self.position = (self.position + 1) % n;
        self.filled = (self.filled + 1).min(n);
        self.hop_count += 1;
        // Process zero-padded initial windows immediately, using fixed staggered
        // hop phases. Every offset retains the same N-1 output latency, including
        // the first impulse; postponing analysis until a full window would not.
        if self.hop_count >= n / 4 {
            self.hop_count = 0;
            self.transform(ratio.clamp(0.25, 4.0));
        }
        let out = if self.epochs[self.position] == self.epoch {
            self.output[self.position]
        } else {
            [0.0; 2]
        };
        self.output[self.position] = [0.0; 2];
        self.epochs[self.position] = self.epoch;
        out
    }
    fn transform(&mut self, ratio: f32) {
        let n = self.plan.n;
        let half = n / 2;
        let hop = n / 4;
        let expected = TAU * hop as f32 / n as f32;
        for i in 0..n {
            let x = if i < n - self.filled {
                [0.0; 2]
            } else {
                self.input[(self.position + i) % n]
            };
            self.work[i] = [x[0] * self.plan.window[i], x[1] * self.plan.window[i]];
        }
        self.plan.fft(&mut self.work, false);
        if self.preserve_formants {
            for k in 0..=half {
                let a = self.work[k];
                let b = self.work[(n - k) % n];
                self.source_magnitudes[k] = [
                    ((a[0] + b[0]) * 0.5).hypot((a[1] - b[1]) * 0.5),
                    ((a[1] + b[1]) * 0.5).hypot((b[0] - a[0]) * 0.5),
                ];
            }
            if !self.envelope_ready || self.envelope_tick == 0 {
                self.envelope
                    .update(&self.source_magnitudes, self.fundamental_hint);
                self.envelope_ready = true;
            }
            self.envelope_tick = (self.envelope_tick + 1) % 2;
        }
        self.magnitude.fill([0.0; 2]);
        self.frequency.fill([0.0; 2]);
        self.phase_strength.fill(0.0);
        self.phase_difference.fill(0.0);
        let mut original_energy = 0.0f64;
        let mut corrected_energy = 0.0f64;
        for k in 0..=half {
            let a = self.work[k];
            let b = self.work[(n - k) % n];
            let channels = [
                [(a[0] + b[0]) * 0.5, (a[1] - b[1]) * 0.5],
                [(a[1] + b[1]) * 0.5, (b[0] - a[0]) * 0.5],
            ];
            let phases = channels.map(|z| z[1].atan2(z[0]));
            let mags = if self.preserve_formants {
                self.source_magnitudes[k]
            } else {
                channels.map(|z| z[0].hypot(z[1]))
            };
            let destination = (k as f32 * ratio).round() as usize;
            if destination <= half && mags[0] + mags[1] > self.phase_strength[destination] {
                self.phase_strength[destination] = mags[0] + mags[1];
                self.phase_difference[destination] = phases[1] - phases[0];
            }
            for ch in 0..2 {
                let mut mag = mags[ch];
                let phase = phases[ch];
                let delta = if self.primed {
                    (phase - self.last_phase[k][ch] - k as f32 * expected + PI).rem_euclid(TAU) - PI
                } else {
                    0.0
                };
                self.last_phase[k][ch] = phase;
                let frequency = (k as f32 + delta / expected) * ratio;
                if self.preserve_formants {
                    original_energy += f64::from(mag) * f64::from(mag);
                    mag *= self.envelope.correction(
                        frequency / ratio,
                        frequency / self.formant_ratio,
                        ch,
                    );
                    corrected_energy += f64::from(mag) * f64::from(mag);
                }
                if destination <= half {
                    self.magnitude[destination][ch] += mag;
                    self.frequency[destination][ch] += frequency * mag;
                    if !self.primed || !self.active[destination][ch] {
                        self.sum_phase[destination][ch] = phase;
                    }
                }
            }
        }
        self.work.fill([0.0; 2]);
        let formant_gain = if self.preserve_formants && corrected_energy > original_energy * 4.0 {
            ((original_energy * 4.0) / corrected_energy.max(1e-30)).sqrt() as f32
        } else {
            1.0
        };
        for k in 0..=half {
            let mut channels = [[0.0; 2]; 2];
            for ch in 0..2 {
                let mag = self.magnitude[k][ch];
                let freq = if mag > 1e-12 {
                    self.frequency[k][ch] / mag
                } else {
                    k as f32
                };
                if !self.primed && mag <= 1e-12 {
                    self.sum_phase[k][ch] = 0.0;
                }
                if ratio == 1.0 {
                    self.sum_phase[k][ch] = self.last_phase[k][ch];
                } else if self.primed && self.active[k][ch] {
                    self.sum_phase[k][ch] =
                        (self.sum_phase[k][ch] + freq * expected).rem_euclid(TAU);
                }
                self.active[k][ch] = mag > 1e-12;
            }
            // Lock the quieter channel to the stronger channel's synthesis phase
            // plus the source bin's stereo relation. Independent phase accumulation
            // can turn antiphase stereo into mono after a silent/zero-padded start.
            let left_frequency = if self.magnitude[k][0] > 1e-12 {
                self.frequency[k][0] / self.magnitude[k][0]
            } else {
                -1.0
            };
            let right_frequency = if self.magnitude[k][1] > 1e-12 {
                self.frequency[k][1] / self.magnitude[k][1]
            } else {
                -2.0
            };
            // Only lock genuinely equal-frequency stereo material. Nearby but
            // independent notes (e.g. L220/R221 Hz) must retain separate frequencies.
            if ratio != 1.0 && (left_frequency - right_frequency).abs() < 0.0001 {
                if self.magnitude[k][0] >= self.magnitude[k][1] {
                    self.sum_phase[k][1] = self.sum_phase[k][0] + self.phase_difference[k];
                } else {
                    self.sum_phase[k][0] = self.sum_phase[k][1] - self.phase_difference[k];
                }
            }
            for ch in 0..2 {
                let phase = self.sum_phase[k][ch];
                let mag = self.magnitude[k][ch] * formant_gain;
                channels[ch] = [mag * phase.cos(), mag * phase.sin()];
            }
            // DC and Nyquist are self-conjugate for each real channel. Leaving
            // an imaginary value here leaks the left channel into the packed
            // right channel (especially during a zero-padded attack).
            if k == 0 || k == half {
                channels[0][1] = 0.0;
                channels[1][1] = 0.0;
            }
            let l = channels[0];
            let r = channels[1];
            self.work[k] = [l[0] - r[1], l[1] + r[0]];
            if k > 0 && k < half {
                self.work[n - k] = [l[0] + r[1], -l[1] + r[0]];
            }
        }
        self.primed = true;
        self.plan.fft(&mut self.work, true);
        for i in 0..n {
            let idx = (self.position + i) % n;
            if self.epochs[idx] != self.epoch {
                self.output[idx] = [0.0; 2];
                self.epochs[idx] = self.epoch;
            }
            let gain = self.plan.window[i] * (2.0 / 3.0);
            self.output[idx][0] += self.work[i][0] * gain;
            self.output[idx][1] += self.work[i][1] * gain;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn formant_preservation_keeps_vowel_envelope_while_pitch_moves() {
        let sr = 48000.0;
        let ratio = 1.5;
        let fundamental = 80.0;
        let harmonics: Vec<_> = (1..90)
            .map(|h| {
                let f = h as f32 * fundamental;
                let envelope = 0.008
                    + (-0.5 * ((f - 600.0) / 95.0).powi(2)).exp()
                    + 0.8 * (-0.5 * ((f - 1800.0) / 150.0).powi(2)).exp()
                    + 0.5 * (-0.5 * ((f - 3000.0) / 180.0).powi(2)).exp();
                (f, envelope * 0.015)
            })
            .collect();
        let mut plain = PitchShift::new(Plan::new(sr));
        let mut preserved = PitchShift::new(Plan::new(sr));
        preserved.set_formants(true, 1.0);
        let bins: Vec<_> = (2..42).map(|h| h as f32 * fundamental * ratio).collect();
        let mut sums_plain = vec![[0.0f64; 2]; bins.len()];
        let mut sums_preserved = vec![[0.0f64; 2]; bins.len()];
        let allocations = crate::test_alloc::count(|| {
            for n in 0..72000 {
                let t = n as f32 / sr;
                let x = harmonics
                    .iter()
                    .map(|(f, a)| (TAU * f * t).sin() * a)
                    .sum::<f32>();
                let a = plain.next([x, -x], ratio);
                let b = preserved.next([x, -x], ratio);
                assert!(b.iter().all(|v| v.is_finite() && v.abs() < 2.0));
                assert!((b[0] + b[1]).abs() < 0.0005);
                if n >= 24000 {
                    for (i, f) in bins.iter().enumerate() {
                        let phase = std::f64::consts::TAU * (*f as f64) * n as f64 / sr as f64;
                        sums_plain[i][0] += a[0] as f64 * phase.cos();
                        sums_plain[i][1] += a[0] as f64 * phase.sin();
                        sums_preserved[i][0] += b[0] as f64 * phase.cos();
                        sums_preserved[i][1] += b[0] as f64 * phase.sin();
                    }
                }
            }
        });
        assert_eq!(allocations, 0);
        let energy = |sums: &[[f64; 2]], low: f32, high: f32| {
            bins.iter()
                .zip(sums)
                .filter(|(f, _)| **f >= low && **f <= high)
                .map(|(_, c)| c[0] * c[0] + c[1] * c[1])
                .sum::<f64>()
        };
        let plain_ratio = energy(&sums_plain, 480.0, 720.0) / energy(&sums_plain, 840.0, 1080.0);
        let preserved_ratio =
            energy(&sums_preserved, 480.0, 720.0) / energy(&sums_preserved, 840.0, 1080.0);
        println!(
            "Formant F1 region preserved/natural energy ratio: plain {plain_ratio:.3}, preserved {preserved_ratio:.3}"
        );
        assert!(plain_ratio < 0.3 && preserved_ratio > plain_ratio * 8.0 && preserved_ratio > 1.3);
        let plain_second =
            energy(&sums_plain, 1680.0, 1920.0) / energy(&sums_plain, 2520.0, 2880.0);
        let preserved_second =
            energy(&sums_preserved, 1680.0, 1920.0) / energy(&sums_preserved, 2520.0, 2880.0);
        assert!(
            preserved_second > plain_second * 6.0 && preserved_second > 1.0,
            "F2 {plain_second} / {preserved_second}"
        );
    }
    #[test]
    fn formant_offset_moves_the_envelope_without_moving_harmonic_frequencies() {
        let sr = 48000.0;
        let mut shifter = PitchShift::new(Plan::new(sr));
        shifter.set_formants(true, 1.5);
        shifter.set_fundamental_hint(80.0);
        let harmonics: Vec<_> = (1..70)
            .map(|h| {
                let f = h as f32 * 80.0;
                let a = 0.006
                    + (-0.5 * ((f - 600.0) / 110.0).powi(2)).exp()
                    + 0.5 * (-0.5 * ((f - 1800.0) / 170.0).powi(2)).exp();
                (f, a * 0.012)
            })
            .collect();
        let mut sums = vec![[0.0f64; 2]; harmonics.len()];
        for n in 0..72000 {
            let t = n as f32 / sr;
            let x = harmonics
                .iter()
                .map(|(f, a)| (TAU * f * t).sin() * a)
                .sum::<f32>();
            let y = shifter.next([x, x], 1.0)[0];
            if n >= 24000 {
                for (i, (f, _)) in harmonics.iter().enumerate() {
                    let phase = std::f64::consts::TAU * (*f as f64) * n as f64 / sr as f64;
                    sums[i][0] += y as f64 * phase.cos();
                    sums[i][1] += y as f64 * phase.sin();
                }
            }
        }
        let energy = |low: f32, high: f32| {
            harmonics
                .iter()
                .zip(&sums)
                .filter(|((f, _), _)| *f >= low && *f <= high)
                .map(|(_, v)| v[0] * v[0] + v[1] * v[1])
                .sum::<f64>()
        };
        assert!(energy(800.0, 1040.0) > energy(480.0, 720.0) * 2.0);
    }
    #[test]
    fn formant_mode_does_not_erase_a_sparse_tone_or_allocate_at_sample_rate_boundaries() {
        for sr in [8000.0, 44100.0, 48000.0, 96000.0, 192000.0] {
            let plan = Plan::new(sr);
            let mut plain = PitchShift::new_with_offset_and_rate(plan.clone(), 0, sr);
            let mut kept = PitchShift::new_with_offset_and_rate(plan, 0, sr);
            kept.set_formants(true, 1.0);
            let mut ordinary = 0.0f64;
            let mut preserved = 0.0f64;
            let count = crate::test_alloc::count(|| {
                for n in 0..sr as usize / 2 {
                    let x = (TAU * 220.0 * n as f32 / sr).sin() * 0.1;
                    let a = plain.next([x, 0.0], 1.5);
                    let b = kept.next([x, 0.0], 1.5);
                    assert!(b[0].is_finite() && b[1].abs() < 0.00001);
                    if n > sr as usize / 4 {
                        ordinary += f64::from(a[0] * a[0]);
                        preserved += f64::from(b[0] * b[0]);
                    }
                }
            });
            assert_eq!(count, 0);
            assert!(
                preserved / ordinary > 0.75 && preserved / ordinary < 1.25,
                "{sr}: {}",
                preserved / ordinary
            );
        }
    }
    #[test]
    fn frequency_and_stereo_phase_are_preserved_through_octave_shift() {
        for input_hz in [110.0, 220.0, 333.0, 440.0] {
            let mut p = PitchShift::new(Plan::new(48000.0));
            let mut crossings = 0i32;
            let mut prev = 0.0;
            let mut energy = 0.0;
            let allocations = crate::test_alloc::count(|| {
                for n in 0..96000 {
                    let x = (TAU * input_hz * n as f32 / 48000.0).sin() * 0.2;
                    let y = p.next([x, -x], 2.0);
                    if n >= 48000 {
                        if prev < 0.0 && y[0] >= 0.0 {
                            crossings += 1;
                        }
                        energy += y[0] * y[0];
                        assert!((y[0] + y[1]).abs() < 0.001);
                    }
                    prev = y[0];
                }
            });
            assert_eq!(allocations, 0);
            assert!(
                (crossings as f32 - input_hz * 2.0).abs() < 2.0,
                "{input_hz}: {crossings}"
            );
            assert!(energy > 10.0);
        }
    }
    #[test]
    fn logical_reset_cannot_replay_previous_sound() {
        let mut p = PitchShift::new(Plan::new(8000.0));
        for _ in 0..10000 {
            p.next([0.4, -0.2], 0.5);
        }
        let count = crate::test_alloc::count(|| {
            p.reset();
            for _ in 0..10000 {
                assert_eq!(p.next([0.0; 2], 0.5), [0.0; 2]);
            }
        });
        assert_eq!(count, 0);
    }
    #[test]
    fn unity_has_exact_reported_latency_and_stereo_waveform() {
        let mut p = PitchShift::new(Plan::new(48000.0));
        let latency = p.latency_frames();
        for n in 0..20000 {
            let source = |i: usize| {
                [
                    (i as f32 * 0.037).sin() * 0.2,
                    (i as f32 * 0.113).sin() * 0.1,
                ]
            };
            let y = p.next(source(n), 1.0);
            if n > latency * 3 {
                let x = source(n - latency);
                assert!((x[0] - y[0]).abs() < 0.0005, "{n}: {:?}/{:?}", x, y);
                assert!((x[1] - y[1]).abs() < 0.0005);
            }
        }
    }
    #[test]
    fn staggered_hops_have_identical_impulse_latency_from_the_first_sample() {
        for offset in [0, 1, 17, 127, 255, 511] {
            let mut p = PitchShift::new_with_offset(Plan::new(48000.0), offset);
            let latency = p.latency_frames();
            for n in 0..10000 {
                let x = if n == 0 || n == 2587 {
                    [0.4, -0.2]
                } else {
                    [0.0; 2]
                };
                let y = p.next(x, 1.0);
                let expected = if n == latency || n == 2587 + latency {
                    [0.4, -0.2]
                } else {
                    [0.0; 2]
                };
                assert!(
                    (y[0] - expected[0]).abs() < 1e-5 && (y[1] - expected[1]).abs() < 1e-5,
                    "offset {offset} frame {n}: {:?}",
                    y
                );
            }
        }
    }
    #[test]
    fn non_octave_intervals_and_chords_keep_their_target_frequencies() {
        for semitones in [-12.0_f32, 3.0, 7.0] {
            let ratio = 2.0_f32.powf(semitones / 12.0);
            let mut p = PitchShift::new(Plan::new(48000.0));
            let mut prev = [0.0; 2];
            let mut crossings = [0i32; 2];
            for n in 0..96000 {
                let t = n as f32 / 48000.0;
                let y = p.next(
                    [(TAU * 220.0 * t).sin() * 0.2, (TAU * 333.0 * t).sin() * 0.1],
                    ratio,
                );
                if n >= 48000 {
                    for ch in 0..2 {
                        if prev[ch] < 0.0 && y[ch] >= 0.0 {
                            crossings[ch] += 1;
                        }
                    }
                }
                prev = y;
            }
            assert!(
                (crossings[0] as f32 - 220.0 * ratio).abs() < 3.0,
                "{semitones}: {:?}",
                crossings
            );
            assert!(
                (crossings[1] as f32 - 333.0 * ratio).abs() < 3.0,
                "{semitones}: {:?}",
                crossings
            );
        }
        let ratio = 2.0_f32.powf(7.0 / 12.0);
        let tones = [130.8128, 196.0, 261.6256];
        let mut p = PitchShift::new_with_offset(Plan::new(48000.0), 157);
        let mut correlation = [[0.0f64; 2]; 3];
        for n in 0..96000 {
            let t = n as f64 / 48000.0;
            let x = tones
                .iter()
                .map(|f| (std::f64::consts::TAU * f * t).sin() * 0.1)
                .sum::<f64>() as f32;
            let y = p.next([x, x * 0.6], ratio);
            if n >= 48000 {
                for (i, hz) in tones.iter().enumerate() {
                    let a = std::f64::consts::TAU * hz * ratio as f64 * t;
                    correlation[i][0] += y[0] as f64 * a.cos();
                    correlation[i][1] += y[0] as f64 * a.sin();
                }
            }
        }
        for (i, c) in correlation.iter().enumerate() {
            let amplitude = c[0].hypot(c[1]) * 2.0 / 48000.0;
            assert!(amplitude > 0.025, "Chord partial {i} vanished: {amplitude}");
        }
    }
    #[test]
    fn near_unison_stereo_notes_do_not_pull_each_other_out_of_tune() {
        for semitones in [3.0_f32, 7.0] {
            let ratio = 2.0_f32.powf(semitones / 12.0);
            let frequencies = [220.0_f64, 221.0];
            let mut p = PitchShift::new(Plan::new(48000.0));
            let mut sums = [[[0.0f64; 2]; 2]; 2];
            for n in 0..144000 {
                let t = n as f64 / 48000.0;
                let x = frequencies.map(|f| (std::f64::consts::TAU * f * t).sin() as f32 * 0.2);
                let y = p.next(x, ratio);
                if n >= 48000 {
                    let block = (n - 48000) / 48000;
                    for ch in 0..2 {
                        let phase = std::f64::consts::TAU * frequencies[ch] * ratio as f64 * t;
                        sums[block][ch][0] += y[ch] as f64 * phase.cos();
                        sums[block][ch][1] += y[ch] as f64 * phase.sin();
                    }
                }
            }
            for ch in 0..2 {
                let a = sums[0][ch][1].atan2(sums[0][ch][0]);
                let b = sums[1][ch][1].atan2(sums[1][ch][0]);
                let error = ((b - a + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU)
                    - std::f64::consts::PI)
                    / std::f64::consts::TAU;
                assert!(
                    error.abs() < 0.04,
                    "{semitones} semitones, channel {ch}: frequency drift {error} Hz"
                );
            }
        }
    }
}
