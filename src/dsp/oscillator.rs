use crate::config::note_configs::{Note, NoteConfigs, NoteOct};
use crate::config::osc_configs::Waveform;
use crate::dsp::envelope::{AhdsrParams, AhdsrState};

#[derive(Clone, Copy)]
pub struct OscillatorDspState {
    pub phase: f32,
    pub envelope: AhdsrState,
}

impl OscillatorDspState {
    pub fn new() -> Self {
        Self {
            phase: 0.0,
            envelope: AhdsrState::new(),
        }
    }
}

pub fn default_note() -> NoteOct {
    NoteOct {
        note: Note::C,
        octave: 4,
    }
}

pub fn note_at_time(seq: &[Option<NoteOct>], bpm: usize, elapsed_secs: f64) -> Option<NoteOct> {
    if seq.is_empty() {
        return Some(default_note());
    }
    let ticks_per_beat = NoteConfigs::ticks_per_beat();
    let secs_per_beat = 60.0 / bpm.max(1) as f64;
    let tick = ((elapsed_secs / secs_per_beat) * ticks_per_beat as f64).floor() as usize;
    let idx = tick % seq.len();
    seq[idx]
}

pub fn seq_bool_at_time(seq: &[bool], bpm: usize, elapsed_secs: f64) -> bool {
    if seq.is_empty() {
        return true;
    }
    let ticks_per_beat = NoteConfigs::ticks_per_beat();
    let secs_per_beat = 60.0 / bpm.max(1) as f64;
    let tick = ((elapsed_secs / secs_per_beat) * ticks_per_beat as f64).floor() as usize;
    let idx = tick % seq.len();
    seq[idx]
}

pub fn process_sample(
    state: &mut OscillatorDspState,
    waveform: Waveform,
    level: f32,
    threshold: f32,
    input_level: f32,
    sample_rate: f32,
    note: Option<NoteOct>,
    note_on: bool,
    note_retrigger: bool,
    envelope: AhdsrParams,
) -> f32 {
    let dt = 1.0 / sample_rate.max(1.0);
    let gate_on = note_on && input_level >= threshold;
    let retrigger = note_retrigger && input_level >= threshold;
    let amp = state.envelope.next(gate_on, retrigger, envelope, dt);
    if let Some(n) = note {
        let freq = n.freq_hz();
        let (sample, phase) = osc_sample(waveform, state.phase, freq, sample_rate);
        state.phase = phase;
        sample * level * amp
    } else {
        0.0
    }
}

fn osc_sample(waveform: Waveform, phase: f32, freq: f32, sample_rate: f32) -> (f32, f32) {
    let dt = (freq / sample_rate.max(1.0)).clamp(0.0, 0.5);
    let mut t = phase;
    let sample = match waveform {
        Waveform::Sine => (2.0 * std::f32::consts::PI * t).sin(),
        Waveform::Saw => {
            let mut v = 2.0 * t - 1.0;
            v -= poly_blep(t, dt);
            v
        }
        Waveform::Square => {
            let mut v = if t < 0.5 { 1.0 } else { -1.0 };
            v += poly_blep(t, dt);
            v -= poly_blep((t + 0.5) % 1.0, dt);
            v
        }
        Waveform::Triangle => {
            if t < 0.5 {
                4.0 * t - 1.0
            } else {
                3.0 - 4.0 * t
            }
        }
    };
    t += dt;
    if t >= 1.0 {
        t -= 1.0;
    }
    (sample, t)
}

fn poly_blep(t: f32, dt: f32) -> f32 {
    if dt <= 0.0 {
        return 0.0;
    }
    if t < dt {
        let x = t / dt;
        return x + x - x * x - 1.0;
    }
    if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        return x * x + x + x + 1.0;
    }
    0.0
}
