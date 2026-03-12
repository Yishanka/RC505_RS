// src/dsp/reverb.rs

use crate::config::reverb_configs::{
    REVERB_PREDELAY_MAX_MS, REVERB_SIZE_MAX_MS, REVERB_SIZE_MIN_MS,
};

const DELAY_RATIOS: [f32; 4] = [0.73, 0.89, 1.0, 1.11];
const FEEDBACK_MATRIX_SCALE: f32 = 0.5; // 2 / n for n=4
const INPUT_GAIN: f32 = 0.5;
const WET_GAIN: f32 = 0.35;

#[derive(Clone, Copy)]
pub struct ReverbParams {
    pub size_ms: f32,
    pub rt60_ms: f32,
    pub predelay_ms: f32,
    pub width: f32,
    pub high_cut_damp: f32,
    pub low_cut_hz: f32,
}

#[derive(Clone)]
struct DelayLine {
    buffer: Vec<f32>,
    write_idx: usize,
    delay_samples: usize,
}

impl DelayLine {
    fn new(max_delay_samples: usize) -> Self {
        let len = max_delay_samples.max(1) + 1;
        Self {
            buffer: vec![0.0; len],
            write_idx: 0,
            delay_samples: 1,
        }
    }

    fn set_delay_samples(&mut self, delay_samples: usize) {
        let max_delay = self.buffer.len().saturating_sub(1);
        self.delay_samples = delay_samples.clamp(1, max_delay);
    }

    fn delay_samples(&self) -> usize {
        self.delay_samples
    }

    fn read(&self) -> f32 {
        let len = self.buffer.len();
        let idx = (self.write_idx + len - self.delay_samples) % len;
        self.buffer[idx]
    }

    fn write(&mut self, sample: f32) {
        self.buffer[self.write_idx] = sample;
        self.write_idx += 1;
        if self.write_idx >= self.buffer.len() {
            self.write_idx = 0;
        }
    }
}

#[derive(Clone, Copy)]
struct OnePoleLp {
    z: f32,
}

impl OnePoleLp {
    fn new() -> Self {
        Self { z: 0.0 }
    }

    fn process(&mut self, x: f32, cutoff_hz: f32, sample_rate: f32) -> f32 {
        let sr = sample_rate.max(1.0);
        let fc = cutoff_hz.max(10.0).min(sr * 0.49);
        let a = (-2.0 * std::f32::consts::PI * fc / sr).exp();
        self.z = (1.0 - a) * x + a * self.z;
        self.z
    }
}

#[derive(Clone, Copy)]
struct OnePoleHp {
    y: f32,
    x_prev: f32,
}

impl OnePoleHp {
    fn new() -> Self {
        Self { y: 0.0, x_prev: 0.0 }
    }

    fn process(&mut self, x: f32, cutoff_hz: f32, sample_rate: f32) -> f32 {
        let sr = sample_rate.max(1.0);
        let fc = cutoff_hz.max(10.0).min(sr * 0.49);
        let a = (-2.0 * std::f32::consts::PI * fc / sr).exp();
        let y = a * (self.y + x - self.x_prev);
        self.x_prev = x;
        self.y = y;
        y
    }
}

#[derive(Clone)]
pub struct ReverbDspState {
    sample_rate: f32,
    predelay: DelayLine,
    lines: [DelayLine; 4],
    lp: [OnePoleLp; 4],
    hp: [OnePoleHp; 4],
    smooth_size_ms: f32,
    smooth_rt60_ms: f32,
    last_size_ms: f32,
    inited: bool,
}

impl ReverbDspState {
    pub fn new() -> Self {
        let sample_rate = 48_000.0;
        let max_delay = max_delay_samples(sample_rate);
        let predelay_len = predelay_samples(sample_rate, REVERB_PREDELAY_MAX_MS as f32);
        Self {
            sample_rate,
            predelay: DelayLine::new(predelay_len),
            lines: std::array::from_fn(|_| DelayLine::new(max_delay)),
            lp: std::array::from_fn(|_| OnePoleLp::new()),
            hp: std::array::from_fn(|_| OnePoleHp::new()),
            smooth_size_ms: REVERB_SIZE_MIN_MS as f32,
            smooth_rt60_ms: 2000.0,
            last_size_ms: -1.0,
            inited: false,
        }
    }

    fn ensure_sample_rate(&mut self, sample_rate: f32) {
        if (self.sample_rate - sample_rate).abs() < 1.0 {
            return;
        }
        self.sample_rate = sample_rate.max(1.0);
        let max_delay = max_delay_samples(self.sample_rate);
        for line in &mut self.lines {
            *line = DelayLine::new(max_delay);
        }
        let predelay_len = predelay_samples(self.sample_rate, REVERB_PREDELAY_MAX_MS as f32);
        self.predelay = DelayLine::new(predelay_len);
        self.inited = false;
        self.last_size_ms = -1.0;
    }
}

pub fn process_frame(
    state: &mut ReverbDspState,
    p: ReverbParams,
    sample_rate: f32,
    input_l: f32,
    input_r: f32,
) -> (f32, f32) {
    let sr = sample_rate.max(1.0);
    state.ensure_sample_rate(sr);

    let target_size = p.size_ms.clamp(REVERB_SIZE_MIN_MS as f32, REVERB_SIZE_MAX_MS as f32);
    let target_rt60 = p.rt60_ms.max(50.0);
    let alpha = 1.0 - (-1.0 / (0.05 * sr)).exp();
    if !state.inited {
        state.smooth_size_ms = target_size;
        state.smooth_rt60_ms = target_rt60;
        state.inited = true;
    } else {
        state.smooth_size_ms += (target_size - state.smooth_size_ms) * alpha;
        state.smooth_rt60_ms += (target_rt60 - state.smooth_rt60_ms) * alpha;
    }

    let predelay_samples = predelay_samples(sr, p.predelay_ms.max(0.0));
    state.predelay.set_delay_samples(predelay_samples);

    if (state.smooth_size_ms - state.last_size_ms).abs() > 0.5 {
        let max_delay = state.lines[0].buffer.len().saturating_sub(1);
        let mut used: Vec<usize> = Vec::with_capacity(4);
        for (idx, line) in state.lines.iter_mut().enumerate() {
            let delay_ms = state.smooth_size_ms * DELAY_RATIOS[idx];
            let base_samples = (delay_ms * sr / 1000.0).round() as usize;
            let mut prime = next_prime(base_samples.max(2));
            while used.contains(&prime) {
                prime = next_prime(prime + 1);
            }
            used.push(prime);
            line.set_delay_samples(prime.min(max_delay));
        }
        state.last_size_ms = state.smooth_size_ms;
    }

    let input_mono = (input_l + input_r) * 0.5;
    let predelayed = {
        let out = state.predelay.read();
        state.predelay.write(input_mono);
        out
    };

    let mut y = [0.0f32; 4];
    for (idx, line) in state.lines.iter().enumerate() {
        y[idx] = line.read();
    }

    let mut fb = [0.0f32; 4];
    let damp = p.high_cut_damp.clamp(0.0, 1.0);
    let high_cut_hz = lerp(18_000.0, 2_500.0, damp).min(sr * 0.49);
    let low_cut_hz = p.low_cut_hz.max(10.0).min(high_cut_hz * 0.95);

    for idx in 0..4 {
        let hp = state.hp[idx].process(y[idx], low_cut_hz, sr);
        let lp = state.lp[idx].process(hp, high_cut_hz, sr);
        let delay_sec = (state.lines[idx].delay_samples() as f32) / sr;
        let rt60_sec = state.smooth_rt60_ms.max(10.0) / 1000.0;
        let gain = 10.0_f32.powf(-3.0 * delay_sec / rt60_sec);
        fb[idx] = lp * gain;
    }

    let sum = fb.iter().sum::<f32>();
    for idx in 0..4 {
        let mixed = fb[idx] - FEEDBACK_MATRIX_SCALE * sum;
        let input = predelayed * INPUT_GAIN + mixed;
        state.lines[idx].write(input);
    }

    let wet_l = (y[0] + y[2] - y[1] - y[3]) * 0.25;
    let wet_r = (y[0] + y[1] - y[2] - y[3]) * 0.25;

    let width = p.width.clamp(0.0, 1.0);
    let mid = (wet_l + wet_r) * 0.5;
    let side = (wet_l - wet_r) * 0.5 * width;
    let wet_l = mid + side;
    let wet_r = mid - side;

    let out_l = input_l + wet_l * WET_GAIN;
    let out_r = input_r + wet_r * WET_GAIN;

    (out_l, out_r)
}

fn max_delay_samples(sample_rate: f32) -> usize {
    let max_ms = REVERB_SIZE_MAX_MS as f32 * DELAY_RATIOS.iter().cloned().fold(0.0, f32::max);
    (max_ms * sample_rate / 1000.0).ceil() as usize + 1
}

fn predelay_samples(sample_rate: f32, predelay_ms: f32) -> usize {
    (predelay_ms.max(0.0) * sample_rate / 1000.0).round() as usize
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn next_prime(n: usize) -> usize {
    if n <= 2 {
        return 2;
    }
    let mut candidate = if n % 2 == 0 { n + 1 } else { n };
    loop {
        if is_prime(candidate) {
            return candidate;
        }
        candidate += 2;
    }
}

fn is_prime(n: usize) -> bool {
    if n < 2 {
        return false;
    }
    if n % 2 == 0 {
        return n == 2;
    }
    let mut d = 3usize;
    while d * d <= n {
        if n % d == 0 {
            return false;
        }
        d += 2;
    }
    true
}
