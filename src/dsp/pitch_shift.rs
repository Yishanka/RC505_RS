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
impl PitchShift {
    pub fn new(plan: Arc<Plan>) -> Self {
        Self::new_with_offset(plan, 0)
    }
    pub fn new_with_offset(plan: Arc<Plan>, offset: usize) -> Self {
        let n = plan.n;
        let offset = offset % (n / 4);
        Self {
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
        self.position = 0;
        self.filled = 0;
        self.hop_count = self.plan.n / 4 - self.offset - 1;
        self.epoch = self.epoch.wrapping_add(1).max(1);
        self.primed = false;
    }
    pub fn latency_frames(&self) -> usize {
        self.plan.n - 1
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
        self.magnitude.fill([0.0; 2]);
        self.frequency.fill([0.0; 2]);
        self.phase_strength.fill(0.0);
        self.phase_difference.fill(0.0);
        for k in 0..=half {
            let a = self.work[k];
            let b = self.work[(n - k) % n];
            let channels = [
                [(a[0] + b[0]) * 0.5, (a[1] - b[1]) * 0.5],
                [(a[1] + b[1]) * 0.5, (b[0] - a[0]) * 0.5],
            ];
            let phases = channels.map(|z| z[1].atan2(z[0]));
            let mags = channels.map(|z| z[0].hypot(z[1]));
            let destination = (k as f32 * ratio).round() as usize;
            if destination <= half && mags[0] + mags[1] > self.phase_strength[destination] {
                self.phase_strength[destination] = mags[0] + mags[1];
                self.phase_difference[destination] = phases[1] - phases[0];
            }
            for ch in 0..2 {
                let mag = mags[ch];
                let phase = phases[ch];
                let delta = if self.primed {
                    (phase - self.last_phase[k][ch] - k as f32 * expected + PI).rem_euclid(TAU) - PI
                } else {
                    0.0
                };
                self.last_phase[k][ch] = phase;
                let frequency = (k as f32 + delta / expected) * ratio;
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
                let mag = self.magnitude[k][ch];
                channels[ch] = [mag * phase.cos(), mag * phase.sin()];
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
