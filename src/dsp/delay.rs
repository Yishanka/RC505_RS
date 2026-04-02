const DELAY_TIME_MIN_MS: f32 = 20.0;
const DELAY_TIME_MAX_MS: f32 = 1_500.0;
const FEEDBACK_MAX: f32 = 0.95;

#[derive(Clone, Copy)]
pub struct DelayParams {
    pub time_ms: f32,
    pub feedback: f32,
    pub high_damp_hz: f32,
    pub mix: f32,
}

#[derive(Clone)]
pub struct DelayDspState {
    sample_rate: f32,
    buffer_l: Vec<f32>,
    buffer_r: Vec<f32>,
    write_idx: usize,
    smooth_time_samples: f32,
    smooth_feedback: f32,
    smooth_damp_hz: f32,
    smooth_mix: f32,
    fb_lp_l: f32,
    fb_lp_r: f32,
}

impl DelayDspState {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(1.0);
        let max_delay_samples = ((DELAY_TIME_MAX_MS / 1000.0) * sr).ceil() as usize + 2;
        Self {
            sample_rate: sr,
            buffer_l: vec![0.0; max_delay_samples.max(2)],
            buffer_r: vec![0.0; max_delay_samples.max(2)],
            write_idx: 0,
            smooth_time_samples: (DELAY_TIME_MIN_MS / 1000.0) * sr,
            smooth_feedback: 0.0,
            smooth_damp_hz: 20_000.0,
            smooth_mix: 0.0,
            fb_lp_l: 0.0,
            fb_lp_r: 0.0,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        let sr = sample_rate.max(1.0);
        if (self.sample_rate - sr).abs() < f32::EPSILON {
            return;
        }
        self.sample_rate = sr;
        let max_delay_samples = ((DELAY_TIME_MAX_MS / 1000.0) * sr).ceil() as usize + 2;
        self.buffer_l = vec![0.0; max_delay_samples.max(2)];
        self.buffer_r = vec![0.0; max_delay_samples.max(2)];
        self.write_idx = 0;
        self.smooth_time_samples = (DELAY_TIME_MIN_MS / 1000.0) * sr;
        self.fb_lp_l = 0.0;
        self.fb_lp_r = 0.0;
    }
}

pub fn process_frame(
    state: &mut DelayDspState,
    p: DelayParams,
    sample_rate: f32,
    input_l: f32,
    input_r: f32,
) -> (f32, f32) {
    state.set_sample_rate(sample_rate);
    let sr = state.sample_rate;
    let max_delay_samples = (state.buffer_l.len().saturating_sub(2)).max(1);

    let target_time_samples = ((p.time_ms.clamp(DELAY_TIME_MIN_MS, DELAY_TIME_MAX_MS) / 1000.0) * sr)
        .clamp(1.0, max_delay_samples as f32);
    let target_feedback = p.feedback.clamp(0.0, FEEDBACK_MAX);
    let target_damp_hz = p.high_damp_hz.clamp(200.0, 20_000.0);
    let target_mix = p.mix.clamp(0.0, 1.0);

    let smooth_coeff = smoothing_coeff(sr, 25.0);
    state.smooth_time_samples += (target_time_samples - state.smooth_time_samples) * smooth_coeff;
    state.smooth_feedback += (target_feedback - state.smooth_feedback) * smooth_coeff;
    state.smooth_damp_hz += (target_damp_hz - state.smooth_damp_hz) * smooth_coeff;
    state.smooth_mix += (target_mix - state.smooth_mix) * smooth_coeff;

    let delay_samples = state.smooth_time_samples.clamp(1.0, max_delay_samples as f32);
    let delayed_l = read_interp(&state.buffer_l, state.write_idx as f32 - delay_samples);
    let delayed_r = read_interp(&state.buffer_r, state.write_idx as f32 - delay_samples);

    let lp_alpha = one_pole_alpha(state.smooth_damp_hz, sr);
    state.fb_lp_l += (delayed_l - state.fb_lp_l) * lp_alpha;
    state.fb_lp_r += (delayed_r - state.fb_lp_r) * lp_alpha;

    let fb = state.smooth_feedback;
    let write_l = (input_l + state.fb_lp_l * fb).clamp(-1.0, 1.0);
    let write_r = (input_r + state.fb_lp_r * fb).clamp(-1.0, 1.0);
    state.buffer_l[state.write_idx] = write_l;
    state.buffer_r[state.write_idx] = write_r;

    state.write_idx += 1;
    if state.write_idx >= state.buffer_l.len() {
        state.write_idx = 0;
    }

    let mix = state.smooth_mix;
    let out_l = input_l * (1.0 - mix) + delayed_l * mix;
    let out_r = input_r * (1.0 - mix) + delayed_r * mix;
    (out_l.clamp(-1.0, 1.0), out_r.clamp(-1.0, 1.0))
}

fn read_interp(buffer: &[f32], read_pos: f32) -> f32 {
    if buffer.is_empty() {
        return 0.0;
    }
    let len = buffer.len() as f32;
    let wrapped = read_pos.rem_euclid(len);
    let idx0 = wrapped.floor() as usize;
    let idx1 = (idx0 + 1) % buffer.len();
    let frac = wrapped - idx0 as f32;
    buffer[idx0] * (1.0 - frac) + buffer[idx1] * frac
}

fn one_pole_alpha(cutoff_hz: f32, sample_rate: f32) -> f32 {
    let nyquist = (sample_rate * 0.5).max(1.0);
    let fc = cutoff_hz.clamp(1.0, nyquist);
    let x = (-2.0 * std::f32::consts::PI * fc / sample_rate.max(1.0)).exp();
    (1.0 - x).clamp(0.0, 1.0)
}

fn smoothing_coeff(sample_rate: f32, time_ms: f32) -> f32 {
    let tau = (time_ms.max(1.0) / 1000.0).max(1.0 / sample_rate.max(1.0));
    (1.0 - (-1.0 / (sample_rate.max(1.0) * tau)).exp()).clamp(0.0, 1.0)
}
