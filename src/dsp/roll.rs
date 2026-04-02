#[derive(Clone, Copy)]
pub struct RollParams {
    pub step: usize,
}

#[derive(Clone)]
pub struct RollDspState {
    pub last_step: usize,
}

impl RollDspState {
    pub fn new() -> Self {
        Self { last_step: 4 }
    }
}

pub fn process_sample(
    state: &mut RollDspState,
    p: RollParams,
    buffer: &[f32],
    play_cursor: usize,
    channels: usize,
    sample_rate: f32,
    bpm: usize,
    dry: f32,
) -> f32 {
    if buffer.is_empty() || channels == 0 {
        return dry;
    }

    let step = match p.step {
        2 => 2,
        8 => 8,
        _ => 4,
    };
    state.last_step = step;

    let len = buffer.len();
    let frames = (len / channels).max(1);
    let frames_per_beat = (sample_rate.max(1.0) * 60.0 / bpm.max(1) as f32).max(1.0);
    let loop_beats = (frames as f32 / frames_per_beat).max(1.0);
    let repeat_frames = ((frames_per_beat * loop_beats) / step as f32).round().max(1.0) as usize;
    let repeat_samples = (repeat_frames * channels).clamp(channels, len.max(channels));
    let layers = step.min((len / repeat_samples).max(1));
    let phase = play_cursor % repeat_samples;

    let beat_samples: usize = (frames_per_beat.round() as usize).saturating_mul(channels);
    let fade_samples: usize = (beat_samples / 128)
        .clamp(channels, (repeat_samples / 4).max(channels))
        .max(1);

    let mut wet = 0.0f32;
    for layer in 0..layers {
        let layer_offset = layer * repeat_samples;
        let start_idx = (phase + layer_offset) % len;
        let curr = buffer[start_idx];
        let sample = if phase < fade_samples && repeat_samples > fade_samples {
            let end_phase = repeat_samples - fade_samples + phase;
            let end_idx = (end_phase + layer_offset) % len;
            let t = phase as f32 / fade_samples as f32;
            buffer[end_idx] * (1.0 - t) + curr * t
        } else {
            curr
        };
        wet += sample;
    }

    // Keep a bit more energy than strict averaging so Roll does not sound too quiet.
    let normalized = wet / (layers as f32).sqrt();
    (normalized * ROLL_OUTPUT_GAIN).clamp(-1.0, 1.0)
}
const ROLL_OUTPUT_GAIN: f32 = 1.25;
