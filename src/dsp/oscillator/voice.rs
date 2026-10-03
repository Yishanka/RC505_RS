use super::note_schedule::{NoteBoundary, compiled_schedule};
use super::sample_tables::{SamplePyramid, WaveBank, periodic_sample, sample_bounds};
use super::{OscillatorFxParams, osc_sample};
use crate::config::note_configs::NoteOct;
use crate::config::osc_configs::Waveform;
use crate::dsp::envelope::{AhdsrParams, AhdsrState};
use crate::dsp::filter::{FilterDspState, FilterParams, process_sample as process_filter_sample};
// Unified OSC: fixed-capacity voices with per-note envelopes/filter state. The
// note schedule, sample and modulation curves are prepared off the callback.
use crate::config::osc_configs::{
    LfoConfig, LfoMode, LfoShape, LfoTarget, SampleAsset, SampleCapture, SampleMode,
};
use std::sync::Arc;
#[derive(Clone)]
pub struct PolyOscRuntime {
    pub schedule: Arc<Vec<NoteBoundary>>,
    pub loop_ticks: usize,
    pub note_revision: u64,
    pub voices: usize,
    pub input_gate: bool,
    pub lfo: LfoConfig,
    pub lfo_table: Arc<[f32; 257]>,
    pub tone_revision: u64,
    pub sample: Option<Arc<SampleAsset>>,
    sample_pyramid: Option<Arc<SamplePyramid>>,
    pub capture: Option<Arc<SampleCapture>>,
    pub sample_mode: SampleMode,
    pub root_frequency: f32,
    pub loop_sample: bool,
    pub sample_start: f32,
    pub sample_end: f32,
    wave_bank: Arc<WaveBank>,
}
impl PolyOscRuntime {
    pub fn from_config(c: &crate::config::OscillatorConfigs) -> Self {
        let (note_revision, schedule) = compiled_schedule(&c.note);
        let fine_cents = if c.sample_fine_cents.is_finite() {
            c.sample_fine_cents.clamp(-100.0, 100.0)
        } else {
            0.0
        };
        let mut lfo = c.lfo.clone();
        lfo.sanitize();
        let table = std::array::from_fn(|i| {
            let wave = lfo_value(&lfo, i as f32 / 256.0);
            match lfo.target {
                LfoTarget::Volume => wave,
                LfoTarget::Pitch => 2.0f32.powf((wave * 2.0 - 1.0) * lfo.depth),
                LfoTarget::Cutoff => 2.0f32.powf((wave * 2.0 - 1.0) * lfo.depth * 4.0),
            }
        });
        let tone_revision = c.sample.as_ref().map_or(0, |s| s.content_hash)
            ^ ((c.waveform.value as u64) << 60)
            ^ ((c.sample_mode as u64) << 58)
            ^ c.sample_start.to_bits() as u64
            ^ ((c.sample_end.to_bits() as u64) << 32)
            ^ ((c.vocal_formant.to_bits() as u64) << 16);
        Self {
            schedule,
            tone_revision,
            loop_ticks: c.note.loop_len(),
            note_revision,
            voices: c.voices.clamp(1, 16),
            input_gate: c.input_gate,
            lfo,
            lfo_table: Arc::new(table),
            sample: c.sample.clone(),
            sample_pyramid: c.sample.as_ref().map(SamplePyramid::prepare),
            capture: c.capture.clone(),
            sample_mode: c.sample_mode,
            root_frequency: NoteOct::from_pitch_index(c.sample_root).freq_hz()
                * 2.0f32.powf(fine_cents / 1200.0),
            loop_sample: c.sample_loop,
            sample_start: c.sample_start,
            sample_end: c.sample_end,
            wave_bank: WaveBank::prepare(c),
        }
    }
}

#[derive(Clone, Copy)]
struct PolyVoice {
    id: u64,
    cycle: u64,
    frequency: f32,
    velocity: f32,
    target_velocity: f32,
    gate: bool,
    active: bool,
    phase: f64,
    sample_pos: f64,
    lfo_phase: f64,
    amp: AhdsrState,
    filter_env: AhdsrState,
    filter: FilterDspState,
    last_output: f32,
    steal_tail: f32,
    steal_left: u32,
    age: u64,
    cutoff_base: f32,
    cutoff_max: f32,
    control_tick: u8,
    wave_level: usize,
    wave_limit: f32,
    wave_rate: f32,
    sample_level: usize,
    sample_limit: f64,
}
impl PolyVoice {
    fn new() -> Self {
        Self {
            id: 0,
            cycle: 0,
            frequency: 440.0,
            velocity: 1.0,
            target_velocity: 1.0,
            gate: false,
            active: false,
            phase: 0.0,
            sample_pos: 0.0,
            lfo_phase: 0.0,
            amp: AhdsrState::new(),
            filter_env: AhdsrState::new(),
            filter: FilterDspState::new(),
            last_output: 0.0,
            steal_tail: 0.0,
            steal_left: 0,
            age: 0,
            cutoff_base: 20.0,
            cutoff_max: 0.0,
            control_tick: 0,
            wave_level: 0,
            wave_limit: 0.0,
            wave_rate: 0.0,
            sample_level: 0,
            sample_limit: 1.0,
        }
    }
}
#[derive(Clone)]
pub struct PolyOscState {
    voices: Box<[PolyVoice; 16]>,
    tick: Option<u64>,
    age: u64,
    last_time: f64,
    last_running: bool,
    capture_identity: usize,
    capture_position: usize,
    capture_started: bool,
    sample_identity: u64,
    fade: f32,
    free_phase: f64,
    note_revision: u64,
    last_output: f32,
    transition_tail: f32,
    last_voice_limit: usize,
    level: f32,
}
impl PolyOscState {
    pub fn new() -> Self {
        Self {
            voices: Box::new(std::array::from_fn(|_| PolyVoice::new())),
            tick: None,
            age: 0,
            last_time: -1.0,
            last_running: false,
            capture_identity: 0,
            capture_position: 0,
            capture_started: false,
            sample_identity: 0,
            fade: 1.0,
            free_phase: 0.0,
            note_revision: 0,
            last_output: 0.0,
            transition_tail: 0.0,
            last_voice_limit: 0,
            level: -1.0,
        }
    }
    pub fn reset(&mut self) {
        if self.last_time < 0.0 {
            return;
        }
        for v in self.voices.iter_mut() {
            *v = PolyVoice::new();
        }
        self.tick = None;
        self.last_time = -1.0;
        self.level = -1.0;
    }
    pub fn capture(&mut self, r: &PolyOscRuntime, input: f32, sr: f32, threshold: f32) {
        use std::sync::atomic::Ordering;
        let Some(c) = &r.capture else {
            self.capture_identity = 0;
            return;
        };
        let identity = Arc::as_ptr(c) as usize;
        if self.capture_identity != identity {
            self.capture_identity = identity;
            self.capture_position = 0;
            self.capture_started = false;
        }
        if c.state.load(Ordering::Relaxed) == 2 {
            return;
        }
        if !self.capture_started && input.abs() >= threshold {
            self.capture_started = true;
            c.state.store(1, Ordering::Release);
            c.sample_rate.store(sr as u32, Ordering::Relaxed);
        }
        if !self.capture_started {
            return;
        }
        let length =
            ((sr * c.milliseconds as f32 / 1000.0).round() as usize).clamp(8, c.samples.len());
        if self.capture_position < length {
            c.samples[self.capture_position].store(input.to_bits(), Ordering::Relaxed);
            self.capture_position += 1;
        }
        if self.capture_position >= length {
            c.frames.store(length, Ordering::Relaxed);
            c.state.store(2, Ordering::Release);
        }
    }
}
pub fn process_poly_sample(
    state: &mut PolyOscState,
    r: &PolyOscRuntime,
    p: OscillatorFxParams,
    elapsed: f64,
    bpm: usize,
    running: bool,
) -> f32 {
    let sr = p.sample_rate.max(1.0);
    if running
        && state.last_running
        && state.last_time >= 0.0
        && (elapsed < state.last_time || elapsed - state.last_time > 4.0 / sr as f64)
    {
        state.reset();
    }
    if state.note_revision != r.note_revision {
        // Reconcile note IDs instead of restarting every unrelated held voice
        // when one note is moved, resized or has its velocity edited.
        state.tick = None;
        state.note_revision = r.note_revision;
    }
    state.last_time = elapsed;
    state.last_running = running;
    let sample_hash = r.tone_revision;
    if state.last_voice_limit != r.voices {
        state.tick = None;
        state.last_voice_limit = r.voices;
    }
    if state.level < 0.0 {
        state.level = p.level;
    } else {
        state.level += (p.level - state.level) / (0.005 * sr).max(1.0);
    }
    if state.sample_identity != sample_hash {
        state.sample_identity = sample_hash;
        state.transition_tail = state.last_output;
        state.fade = 0.0;
        for v in state.voices.iter_mut() {
            v.sample_pos = 0.0;
            v.sample_level = 0;
            v.sample_limit = 1.0;
            v.wave_limit = 0.0;
        }
    }
    state.fade = (state.fade + 1.0 / (0.005 * sr)).min(1.0);
    let absolute_tick = sequence_tick(elapsed, sr, bpm);
    let enabled = running
        && r.loop_ticks > 0
        && !r.schedule.is_empty()
        && (!r.input_gate || p.input_level >= p.threshold);
    let cycle = absolute_tick / r.loop_ticks.max(1) as u64;
    let local_tick = (absolute_tick % r.loop_ticks.max(1) as u64) as usize;
    let boundary = r
        .schedule
        .partition_point(|boundary| boundary.at <= local_tick)
        .saturating_sub(1);
    let schedule_key = (cycle << 32) | (boundary as u64);
    if !enabled {
        for v in state.voices.iter_mut() {
            v.gate = false;
        }
        state.tick = None;
    } else if state.tick != Some(schedule_key) {
        state.tick = Some(schedule_key);
        let row = &r.schedule[boundary].notes;
        let chosen = &row[row.len().saturating_sub(r.voices)..];
        for v in state.voices.iter_mut() {
            v.gate = chosen.iter().any(|n| n.id == v.id && v.cycle == cycle);
        }
        for n in chosen {
            if let Some(voice) = state
                .voices
                .iter_mut()
                .find(|v| v.active && v.gate && v.id == n.id && v.cycle == cycle)
            {
                voice.frequency = n.frequency;
                voice.target_velocity = n.velocity;
                continue;
            }
            let limit = r.voices;
            let index = (0..limit)
                .find(|i| !state.voices[*i].active)
                .unwrap_or_else(|| {
                    (0..limit)
                        .min_by(|a, b| {
                            let va = &state.voices[*a];
                            let vb = &state.voices[*b];
                            va.gate
                                .cmp(&vb.gate)
                                .then_with(|| va.last_output.abs().total_cmp(&vb.last_output.abs()))
                                .then_with(|| va.age.cmp(&vb.age))
                        })
                        .unwrap_or(0)
                });
            let tail = state.voices[index].last_output;
            state.age = state.age.wrapping_add(1);
            let mut v = PolyVoice::new();
            v.id = n.id;
            v.cycle = cycle;
            v.frequency = n.frequency;
            v.velocity = n.velocity;
            v.target_velocity = n.velocity;
            v.gate = true;
            v.active = true;
            v.age = state.age;
            v.steal_tail = tail;
            v.steal_left = 64;
            state.voices[index] = v;
        }
    }
    let rate = if r.lfo.sync {
        bpm as f64 / (60.0 * r.lfo.beats as f64)
    } else {
        r.lfo.rate_hz as f64
    };
    if running {
        state.free_phase = (state.free_phase + rate / sr as f64).fract();
    }
    let free_modulation = if r.lfo.enabled {
        table_read(&r.lfo_table, state.free_phase as f32)
    } else {
        1.0
    };
    let mut output = 0.0;
    for v in state.voices.iter_mut() {
        if !v.active {
            continue;
        }
        v.velocity += (v.target_velocity - v.velocity) / (0.005 * sr).max(1.0);
        let amp = v.amp.next(v.gate, false, p.envelope, 1.0 / sr);
        if amp <= 0.000001 && !v.gate {
            v.active = false;
            v.last_output = 0.0;
            continue;
        }
        v.lfo_phase = (v.lfo_phase + rate / sr as f64).fract();
        let modulation = if !r.lfo.enabled {
            1.0
        } else if r.lfo.mode == LfoMode::Free {
            free_modulation
        } else {
            table_read(&r.lfo_table, v.lfo_phase as f32)
        };
        let frequency = v.frequency
            * if r.lfo.enabled && r.lfo.target == LfoTarget::Pitch {
                modulation
            } else {
                1.0
            };
        if v.wave_rate != sr {
            v.wave_limit = 0.0;
            v.wave_rate = sr;
        }
        if frequency > v.wave_limit
            || (v.wave_level > 0 && frequency <= sr * 0.45 / (1024usize >> v.wave_level) as f32)
        {
            v.wave_level = WaveBank::level(frequency, sr);
            v.wave_limit = if v.wave_level == 9 {
                f32::INFINITY
            } else {
                sr * 0.45 / (512usize >> v.wave_level) as f32
            };
        }
        let nyquist_gain = ((sr * 0.5 - frequency) / (sr * 0.05)).clamp(0.0, 1.0);
        let raw = if nyquist_gain <= 0.0 {
            0.0
        } else {
            (match p.waveform {
                Waveform::Sample => sample_voice(v, r, frequency, sr),
                Waveform::Vocal | Waveform::Triangle => {
                    r.wave_bank
                        .read_transition(v.phase, v.wave_level, frequency / v.wave_limit)
                }
                _ => osc_sample(p.waveform, v.phase as f32, frequency, sr).0,
            }) * nyquist_gain
        };
        v.phase = (v.phase + frequency as f64 / sr as f64).fract();
        let cutoff_env = v
            .filter_env
            .next(v.gate, false, p.filter_envelope, 1.0 / sr);
        let min = p.cutoff_min_hz.max(10.0);
        let max = p.filter.cutoff_hz.max(min);
        let volume = if r.lfo.enabled && r.lfo.target == LfoTarget::Volume {
            (1.0 - r.lfo.depth) + r.lfo.depth * modulation
        } else {
            1.0
        };
        let dry = raw * amp * v.velocity * volume * state.level;
        let mut sample = if p.filter.mix <= 0.0 {
            dry
        } else {
            if v.control_tick == 0 || v.cutoff_max != max {
                v.cutoff_base = min * (max / min).powf(cutoff_env);
                v.cutoff_max = max;
            }
            let cutoff = v.cutoff_base
                * if r.lfo.enabled && r.lfo.target == LfoTarget::Cutoff {
                    modulation
                } else {
                    1.0
                };
            process_filter_sample(
                &mut v.filter,
                FilterParams {
                    cutoff_hz: cutoff.clamp(20.0, 20000.0),
                    ..p.filter
                },
                sr,
                dry,
            )
        };
        v.control_tick = (v.control_tick + 1) & 7;
        if v.steal_left > 0 {
            let mix = v.steal_left as f32 / 64.0;
            sample = sample * (1.0 - mix) + v.steal_tail * mix;
            v.steal_left -= 1;
        }
        v.last_output = sample;
        output += sample;
    }
    // Fixed per-patch headroom; adding a note never rescales existing voices.
    let output = output * 0.5;
    let output = output * state.fade + state.transition_tail * (1.0 - state.fade);
    state.last_output = output;
    output
}
fn sequence_tick(elapsed: f64, sample_rate: f32, bpm: usize) -> u64 {
    // elapsed is derived from an integer audio frame. Recover it before doing
    // rational PPQ conversion; floating beat-flooring can be late near long takes.
    let rate = sample_rate.max(1.0).round() as u64;
    let frame = (elapsed.max(0.0) * rate as f64).round() as u64;
    ((frame as u128 * bpm.max(1) as u128 * crate::config::sequence_edit::PPQ as u128)
        / (rate as u128 * 60))
        .min(u64::MAX as u128) as u64
}
fn table_read(table: &[f32; 257], phase: f32) -> f32 {
    let p = phase.clamp(0.0, 1.0) * 256.0;
    let i = (p as usize).min(255);
    let t = p - i as f32;
    table[i] * (1.0 - t) + table[i + 1] * t
}
pub fn lfo_value(c: &LfoConfig, phase: f32) -> f32 {
    match c.shape {
        LfoShape::Sine => 0.5 - 0.5 * (phase * std::f32::consts::TAU).cos(),
        LfoShape::Triangle => 1.0 - (phase * 2.0 - 1.0).abs(),
        LfoShape::Saw => phase,
        LfoShape::Square => {
            if phase < 0.5 {
                1.0
            } else {
                0.0
            }
        }
        LfoShape::Custom => {
            let a = c
                .points
                .windows(2)
                .find(|p| phase <= p[1].x)
                .unwrap_or_else(|| &c.points[c.points.len() - 2..]);
            let t = ((phase - a[0].x) / (a[1].x - a[0].x).max(0.0001)).clamp(0.0, 1.0);
            let shape = crate::dsp::envelope::bend_curve(t, a[0].curve);
            a[0].y + (a[1].y - a[0].y) * shape
        }
    }
}
fn sample_voice(v: &mut PolyVoice, r: &PolyOscRuntime, frequency: f32, sr: f32) -> f32 {
    let Some(s) = &r.sample else {
        return 0.0;
    };
    if s.frames.len() < 4 {
        return 0.0;
    }
    if r.sample_mode == SampleMode::Wavetable {
        return r
            .wave_bank
            .read_transition(v.phase, v.wave_level, frequency / v.wave_limit);
    }
    let (start, end) = sample_bounds(s.frames.len(), r.sample_start, r.sample_end);
    let len = end - start;
    if v.sample_pos >= len as f64 && !r.loop_sample {
        return 0.0;
    }
    let step = (frequency / r.root_frequency.max(1.0) * s.sample_rate as f32 / sr) as f64;
    // Prepared low-pass decimation levels prevent the high notes of full samples
    // from folding ultrasonic source energy back into the audible range.
    if step > v.sample_limit || (v.sample_level > 0 && step <= v.sample_limit * 0.5) {
        v.sample_level = step.max(1.0).log2().ceil() as usize;
        v.sample_limit = (1usize << v.sample_level.min(31)) as f64;
    }
    let (data, scale) = if let Some(pyramid) = &r.sample_pyramid {
        let level = v.sample_level.min(pyramid.levels.len());
        if level > 0 {
            (&pyramid.levels[level - 1][..], (1usize << level) as f64)
        } else {
            (&s.frames[..], 1.0)
        }
    } else {
        (&s.frames[..], 1.0)
    };
    let low = (start as f64 / scale).floor() as usize;
    let high = ((end as f64 / scale).ceil() as usize)
        .min(data.len())
        .max(low + 1);
    let mut value = periodic_sample(&data[low..high], v.sample_pos / scale, r.loop_sample);
    if !r.loop_sample {
        value *= ((len as f64 - v.sample_pos) / (len.min(128) as f64)).clamp(0.0, 1.0) as f32;
    }
    v.sample_pos += step;
    if r.loop_sample {
        v.sample_pos = v.sample_pos.rem_euclid(len as f64);
    }
    value
}

#[cfg(test)]
mod poly_tests {
    use super::*;
    use crate::config::{OscillatorConfigs, sequence_edit::NoteEvent};
    fn params(sr: f32, waveform: Waveform) -> OscillatorFxParams {
        OscillatorFxParams {
            waveform,
            level: 0.7,
            threshold: 0.0,
            input_level: 0.0,
            sample_rate: sr,
            note: None,
            note_on: false,
            note_retrigger: false,
            envelope: AhdsrParams {
                attack_ms: 5.0,
                hold_ms: 0.0,
                decay_ms: 10.0,
                sustain_level: 0.8,
                release_ms: 30.0,
                start_level: 0.0,
                tension_attack: 1.0,
                tension_decay: 1.0,
                tension_release: 1.0,
            },
            filter_envelope: AhdsrParams {
                attack_ms: 0.0,
                hold_ms: 0.0,
                decay_ms: 0.0,
                sustain_level: 1.0,
                release_ms: 30.0,
                start_level: 0.0,
                tension_attack: 1.0,
                tension_decay: 1.0,
                tension_release: 1.0,
            },
            filter: FilterParams {
                filter_type: crate::config::filter_configs::FilterType::Lpf,
                cutoff_hz: 20000.0,
                q: 0.7,
                drive: 0.0,
                mix: 0.0,
            },
            cutoff_min_hz: 20.0,
        }
    }
    #[test]
    fn chord_spectrum_and_note_ids_are_independent() {
        let mut config = OscillatorConfigs::new();
        config.note.replace_events(
            3840,
            &[
                NoteEvent::new(0, 960, NoteOct::from_pitch_index(48)),
                NoteEvent::new(0, 960, NoteOct::from_pitch_index(52)),
                NoteEvent::new(0, 1920, NoteOct::from_pitch_index(55)),
            ],
        );
        let runtime = PolyOscRuntime::from_config(&config);
        let mut state = PolyOscState::new();
        let sr = 48000.0;
        let mut sums = [(0.0f64, 0.0f64); 3];
        let frequencies = [48, 52, 55].map(|p| NoteOct::from_pitch_index(p).freq_hz());
        for i in 0..24000 {
            let t = i as f64 / sr as f64;
            let value = process_poly_sample(
                &mut state,
                &runtime,
                params(sr, Waveform::Sine),
                t,
                120,
                true,
            );
            if i > 2000 {
                for (j, f) in frequencies.iter().enumerate() {
                    let phase = std::f64::consts::TAU * t * (*f as f64);
                    sums[j].0 += value as f64 * phase.cos();
                    sums[j].1 += value as f64 * phase.sin();
                }
            }
        }
        assert_eq!(state.voices.iter().filter(|v| v.gate).count(), 3);
        for (re, im) in sums {
            assert!(
                (re * re + im * im).sqrt() / 22000.0 > 0.05,
                "Chord tone missing"
            );
        }
        for i in 24000..28000 {
            let _ = process_poly_sample(
                &mut state,
                &runtime,
                params(sr, Waveform::Sine),
                i as f64 / sr as f64,
                120,
                true,
            );
        }
        assert_eq!(
            state.voices.iter().filter(|v| v.active).count(),
            1,
            "Independent note-off must leave G playing"
        );
        for i in 28000..32000 {
            let _ = process_poly_sample(
                &mut state,
                &runtime,
                params(sr, Waveform::Sine),
                i as f64 / sr as f64,
                120,
                false,
            );
        }
        assert!(
            state.voices.iter().all(|v| !v.active),
            "Stop must release all notes"
        );
    }
    #[test]
    fn overlap_same_pitch_and_mono_keep_clip_data() {
        let mut c = OscillatorConfigs::new();
        c.note.replace_events(
            3840,
            &[
                NoteEvent::new(0, 1920, NoteOct::from_pitch_index(48)),
                NoteEvent::new(240, 240, NoteOct::from_pitch_index(48)),
            ],
        );
        let runtime = PolyOscRuntime::from_config(&c);
        let mut state = PolyOscState::new();
        for i in 0..16000 {
            process_poly_sample(
                &mut state,
                &runtime,
                params(48000.0, Waveform::Sine),
                i as f64 / 48000.0,
                120,
                true,
            );
        }
        assert_eq!(state.voices.iter().filter(|v| v.gate).count(), 1);
        assert_eq!(c.note.event_slice().len(), 2);
        c.voices = 1;
        let runtime = PolyOscRuntime::from_config(&c);
        let mut state = PolyOscState::new();
        for i in 0..8000 {
            process_poly_sample(
                &mut state,
                &runtime,
                params(48000.0, Waveform::Sine),
                i as f64 / 48000.0,
                120,
                true,
            );
        }
        assert_eq!(state.voices.iter().filter(|v| v.gate).count(), 1);
        assert_eq!(c.note.event_slice().len(), 2);
    }
    #[test]
    fn voice_budget_and_wavetable_are_finite_at_every_supported_rate() {
        let mut c = OscillatorConfigs::new();
        let notes: Vec<_> = (0..24)
            .map(|i| NoteEvent::new(0, 960, NoteOct::from_pitch_index(36 + i)))
            .collect();
        c.note.replace_events(3840, &notes);
        c.sample = Some(Arc::new(SampleAsset::new(
            "Test".into(),
            48000,
            (0..173)
                .map(|i| (i as f32 * std::f32::consts::TAU / 173.0).sin())
                .collect(),
        )));
        c.waveform.value = Waveform::Sample;
        c.voices = 16;
        for sr in [8000.0, 44100.0, 48000.0, 96000.0, 192000.0] {
            let runtime = PolyOscRuntime::from_config(&c);
            let mut a = PolyOscState::new();
            let mut b = PolyOscState::new();
            for i in 0..2048 {
                let p = params(sr, Waveform::Sample);
                let t = i as f64 / sr as f64;
                let va = process_poly_sample(&mut a, &runtime, p, t, 120, true);
                let vb = process_poly_sample(&mut b, &runtime, p, t, 120, true);
                assert!(va.is_finite());
                assert_eq!(va.to_bits(), vb.to_bits());
            }
            assert!(a.voices.iter().filter(|v| v.active).count() <= 16);
        }
    }
    #[test]
    fn capture_mailbox_publishes_only_complete_bounded_sample() {
        let mut c = OscillatorConfigs::new();
        c.capture = Some(Arc::new(SampleCapture::new(20)));
        let runtime = PolyOscRuntime::from_config(&c);
        let mut state = PolyOscState::new();
        for _ in 0..100 {
            state.capture(&runtime, 0.0, 48000.0, 0.2);
        }
        assert!(c.capture.as_ref().unwrap().completed().is_none());
        for i in 0..960 {
            state.capture(&runtime, if i % 2 == 0 { 0.5 } else { -0.5 }, 48000.0, 0.2);
        }
        let sample = c.capture.as_ref().unwrap().completed().unwrap();
        assert_eq!(sample.frames.len(), 960);
        c.capture = None;
        c.sample = Some(Arc::new(sample));
        assert!(c.capture.is_none());
        assert_eq!(c.sample.as_ref().unwrap().frames.len(), 960);
        assert!(
            c.sample
                .as_ref()
                .unwrap()
                .frames
                .iter()
                .all(|x| x.is_finite())
        );
    }
    #[test]
    fn busy_voice_processing_and_capture_do_not_allocate_or_free() {
        let mut c = OscillatorConfigs::new();
        c.voices = 16;
        c.lfo.enabled = true;
        c.lfo.target = LfoTarget::Cutoff;
        let notes: Vec<_> = (0..24)
            .map(|i| NoteEvent::new((i % 4) * 240, 960, NoteOct::from_pitch_index(36 + i)))
            .collect();
        c.note.replace_events(3840, &notes);
        c.capture = Some(Arc::new(SampleCapture::new(20)));
        let runtime = PolyOscRuntime::from_config(&c);
        let mut state = PolyOscState::new();
        let mut p = params(48000.0, Waveform::Vocal);
        p.filter.mix = 1.0;
        let count = crate::test_alloc::count(|| {
            for i in 0..120000 {
                state.capture(&runtime, 0.5, 48000.0, 0.2);
                let output =
                    process_poly_sample(&mut state, &runtime, p, i as f64 / 48000.0, 300, true);
                assert!(output.is_finite());
            }
        });
        assert_eq!(count, 0, "OSC allocated or freed on audio callback");
    }
    #[test]
    fn wavebank_has_stable_pitch_and_rejects_nyquist_alias_harmonics() {
        let mut c = OscillatorConfigs::new();
        c.waveform.value = Waveform::Sample;
        c.sample = Some(Arc::new(SampleAsset::new(
            "Saw".into(),
            48000,
            (0..2048).map(|i| i as f32 / 1024.0 - 1.0).collect(),
        )));
        let runtime = PolyOscRuntime::from_config(&c);
        for sr in [44100.0, 48000.0, 96000.0] {
            let freq = NoteOct::from_pitch_index(105).freq_hz(); // A8 = 7040 Hz: integer periods would be badly detuned.
            let mut phase = 0.0f64;
            let mut previous = 0.0;
            let mut crossings = 0usize;
            for _ in 0..sr as usize {
                let value = runtime.wave_bank.read(phase, freq, sr);
                assert!(value.is_finite());
                if previous <= 0.0 && value > 0.0 {
                    crossings += 1;
                }
                previous = value;
                phase = (phase + freq as f64 / sr as f64).fract();
            }
            assert!(
                (crossings as f32 - freq).abs() <= 1.0,
                "Pitch shifted at sample rate {sr}: {crossings}"
            );
        }
    }
    #[test]
    #[ignore = "manual CPU measurement; deterministic DSP, not a hardware deadline test"]
    fn benchmark_polyphonic_osc() {
        for waveform in [Waveform::Sine, Waveform::Vocal, Waveform::Sample] {
            let mut c = OscillatorConfigs::new();
            c.voices = 16;
            c.waveform.value = waveform;
            c.lfo.enabled = true;
            c.lfo.target = LfoTarget::Cutoff;
            c.lfo.rate_hz = 13.0;
            c.lfo.sync = false;
            let notes: Vec<_> = (0..16)
                .map(|i| NoteEvent::new(0, 3840, NoteOct::from_pitch_index(36 + i)))
                .collect();
            c.note.replace_events(3840, &notes);
            c.sample = Some(Arc::new(SampleAsset::new(
                "Bench".into(),
                48000,
                (0..997).map(|i| i as f32 / 498.5 - 1.0).collect(),
            )));
            let runtime = PolyOscRuntime::from_config(&c);
            let mut state = PolyOscState::new();
            let mut p = params(48000.0, waveform);
            p.filter.mix = 1.0;
            p.filter.drive = 0.25;
            let started = std::time::Instant::now();
            let mut checksum = 0.0f64;
            for i in 0..480000 {
                checksum += std::hint::black_box(process_poly_sample(
                    &mut state,
                    &runtime,
                    p,
                    i as f64 / 48000.0,
                    120,
                    true,
                )) as f64;
            }
            println!(
                "OSC {} 16 voices, filter + cutoff LFO + drive: 10 s audio in {:.3} s, checksum={checksum}",
                waveform,
                started.elapsed().as_secs_f64()
            );
        }
    }

    #[test]
    fn synthesis_inline_state_is_bounded() {
        println!(
            "PolyOscState={} bytes PolyVoice={} OscillatorRuntime={} InputFxEngine={} FxSlotState={} RenderCore={}",
            std::mem::size_of::<PolyOscState>(),
            std::mem::size_of::<PolyVoice>(),
            std::mem::size_of::<crate::engine::input_fx::OscillatorRuntime>(),
            std::mem::size_of::<crate::engine::input_fx::InputFxEngine>(),
            std::mem::size_of::<crate::engine::input_fx::FxSlotState>(),
            std::mem::size_of::<crate::engine::core::RenderCore>()
        );
        assert!(
            std::mem::size_of::<PolyOscState>() < 256,
            "Voice buffers must stay on the prepared heap"
        );
    }

    #[test]
    fn fine_events_start_and_stop_at_the_exact_first_sample_after_the_boundary() {
        let mut c = OscillatorConfigs::new();
        c.voices = 1;
        c.note.replace_events(
            3840,
            &[NoteEvent::new(7, 121, NoteOct::from_pitch_index(57))],
        );
        let runtime = PolyOscRuntime::from_config(&c);
        for sr in [44100u32, 48000, 96000] {
            let mut state = PolyOscState::new();
            let bpm = 137;
            let start = (7u64 * 60 * sr as u64).div_ceil(bpm * 960);
            let end = (128u64 * 60 * sr as u64).div_ceil(bpm * 960);
            let mut first = None;
            let mut last = None;
            for i in 0..end + 10 {
                process_poly_sample(
                    &mut state,
                    &runtime,
                    params(sr as f32, Waveform::Sine),
                    i as f64 / sr as f64,
                    bpm as usize,
                    true,
                );
                if state.voices.iter().any(|v| v.gate) {
                    first.get_or_insert(i);
                    last = Some(i);
                }
            }
            assert_eq!(first, Some(start));
            assert_eq!(last, Some(end - 1));
        }
    }

    #[test]
    fn long_take_ppq_boundaries_use_integer_frames() {
        for sr in [44100u64, 48000, 96000, 192000] {
            for bpm in [37u64, 137, 299] {
                for at_seconds in [0u64, 59, 600, 1800] {
                    let tick = at_seconds * bpm * 960 / 60 + 7;
                    let boundary = (tick * 60 * sr).div_ceil(bpm * 960);
                    assert!(
                        sequence_tick((boundary - 1) as f64 / sr as f64, sr as f32, bpm as usize)
                            < tick
                    );
                    assert_eq!(
                        sequence_tick(boundary as f64 / sr as f64, sr as f32, bpm as usize),
                        tick
                    );
                }
            }
        }
    }
    #[test]
    fn above_nyquist_notes_are_muted_instead_of_folded_to_lower_pitch() {
        let mut c = OscillatorConfigs::new();
        c.note.replace_events(
            3840,
            &[NoteEvent::new(0, 3840, NoteOct::from_pitch_index(119))],
        );
        for waveform in [
            Waveform::Sine,
            Waveform::Saw,
            Waveform::Square,
            Waveform::Triangle,
            Waveform::Vocal,
            Waveform::Sample,
        ] {
            c.waveform.value = waveform;
            let runtime = PolyOscRuntime::from_config(&c);
            let mut state = PolyOscState::new();
            for i in 0..1024 {
                assert_eq!(
                    process_poly_sample(
                        &mut state,
                        &runtime,
                        params(8000.0, waveform),
                        i as f64 / 8000.0,
                        120,
                        true
                    ),
                    0.0
                );
            }
        }
    }

    #[test]
    fn changing_poly_to_mono_releases_other_note_ids_immediately() {
        let mut c = OscillatorConfigs::new();
        c.voices = 16;
        let notes: Vec<_> = (0..16)
            .map(|i| NoteEvent::new(0, 3840, NoteOct::from_pitch_index(36 + i)))
            .collect();
        c.note.replace_events(3840, &notes);
        let mut state = PolyOscState::new();
        let poly = PolyOscRuntime::from_config(&c);
        for i in 0..1000 {
            process_poly_sample(
                &mut state,
                &poly,
                params(48000.0, Waveform::Sine),
                i as f64 / 48000.0,
                120,
                true,
            );
        }
        assert_eq!(state.voices.iter().filter(|v| v.gate).count(), 16);
        let kept = c.note.event_slice().last().unwrap().id;
        c.voices = 1;
        let mono = PolyOscRuntime::from_config(&c);
        process_poly_sample(
            &mut state,
            &mono,
            params(48000.0, Waveform::Sine),
            1000.0 / 48000.0,
            120,
            true,
        );
        assert_eq!(state.voices.iter().filter(|v| v.gate).count(), 1);
        assert_eq!(state.voices.iter().find(|v| v.gate).unwrap().id, kept);
        for i in 1001..3000 {
            process_poly_sample(
                &mut state,
                &mono,
                params(48000.0, Waveform::Sine),
                i as f64 / 48000.0,
                120,
                true,
            );
        }
        assert_eq!(state.voices.iter().filter(|v| v.active).count(), 1);
    }
    #[test]
    fn full_sample_octave_up_uses_antialias_levels() {
        let mut c = OscillatorConfigs::new();
        c.sample_mode = SampleMode::Sampler;
        c.sample_loop = false;
        c.sample_root = 48;
        let high: Vec<_> = (0..9600)
            .map(|i| (std::f32::consts::TAU * 18000.0 * i as f32 / 48000.0).sin() * 0.8)
            .collect();
        c.sample = Some(Arc::new(SampleAsset::new(
            "Ultrasonic on transpose".into(),
            48000,
            high,
        )));
        let runtime = PolyOscRuntime::from_config(&c);
        let mut voice = PolyVoice::new();
        let mut power = 0.0;
        for i in 0..4500 {
            let value = sample_voice(
                &mut voice,
                &runtime,
                NoteOct::from_pitch_index(60).freq_hz(),
                48000.0,
            );
            if i > 100 {
                power += value as f64 * value as f64;
            }
        }
        assert!(
            power / 4400.0 < 1e-5,
            "18 kHz shifted up must not alias down to 12 kHz: {power}"
        );
    }
    #[test]
    fn maximum_phrase_reuses_compilation_during_parameter_automation() {
        let mut c = OscillatorConfigs::new();
        c.voices = 16;
        let notes: Vec<_> = (0..2048)
            .map(|i| NoteEvent::new(i * 14, 13, NoteOct::from_pitch_index(36 + i % 60)))
            .collect();
        c.note.replace_events(30720, &notes);
        let reference = PolyOscRuntime::from_config(&c);
        assert!(reference.schedule.len() <= 4097);
        for i in 0..100 {
            c.level.value = i;
            c.osc_filter.cutoff_hz.value = 100 + i * 50;
            let changed = PolyOscRuntime::from_config(&c);
            assert!(Arc::ptr_eq(&reference.schedule, &changed.schedule));
        }
        let mut state = PolyOscState::new();
        assert_eq!(
            crate::test_alloc::count(|| for i in 0..20000 {
                std::hint::black_box(process_poly_sample(
                    &mut state,
                    &reference,
                    params(48000.0, Waveform::Sine),
                    i as f64 / 48000.0,
                    300,
                    true,
                ));
            }),
            0
        );
        let mut changed = c.note.events();
        changed[1024].pitch = NoteOct::from_pitch_index(80);
        c.note.replace_events(30720, &changed);
        let changed = PolyOscRuntime::from_config(&c);
        assert!(!Arc::ptr_eq(&reference.schedule, &changed.schedule));
    }
    #[test]
    fn stopping_transport_preserves_release_when_elapsed_clock_returns_to_zero() {
        let mut c = OscillatorConfigs::new();
        c.note.replace_events(
            3840,
            &[NoteEvent::new(0, 3840, NoteOct::from_pitch_index(48))],
        );
        let runtime = PolyOscRuntime::from_config(&c);
        let mut state = PolyOscState::new();
        for i in 0..2000 {
            process_poly_sample(
                &mut state,
                &runtime,
                params(48000.0, Waveform::Sine),
                i as f64 / 48000.0,
                120,
                true,
            );
        }
        process_poly_sample(
            &mut state,
            &runtime,
            params(48000.0, Waveform::Sine),
            0.0,
            120,
            false,
        );
        assert_eq!(
            state.voices.iter().filter(|v| v.active).count(),
            1,
            "Stop must release, not abruptly discard the voice"
        );
        for _ in 0..2000 {
            process_poly_sample(
                &mut state,
                &runtime,
                params(48000.0, Waveform::Sine),
                0.0,
                120,
                false,
            );
        }
        assert!(state.voices.iter().all(|v| !v.active));
    }
    #[test]
    fn editing_one_note_does_not_restart_other_held_voices() {
        let mut c = OscillatorConfigs::new();
        c.note.replace_events(
            3840,
            &[
                NoteEvent::new(0, 3840, NoteOct::from_pitch_index(48)),
                NoteEvent::new(0, 3840, NoteOct::from_pitch_index(55)),
            ],
        );
        let runtime = PolyOscRuntime::from_config(&c);
        let mut state = PolyOscState::new();
        for i in 0..2000 {
            process_poly_sample(
                &mut state,
                &runtime,
                params(48000.0, Waveform::Sine),
                i as f64 / 48000.0,
                120,
                true,
            );
        }
        let id = c.note.events()[0].id;
        let old = *state.voices.iter().find(|v| v.id == id).unwrap();
        let mut notes = c.note.events();
        notes[1].pitch = NoteOct::from_pitch_index(57);
        c.note.replace_events(3840, &notes);
        let changed = PolyOscRuntime::from_config(&c);
        process_poly_sample(
            &mut state,
            &changed,
            params(48000.0, Waveform::Sine),
            2000.0 / 48000.0,
            120,
            true,
        );
        let current = state.voices.iter().find(|v| v.id == id).unwrap();
        assert_eq!(current.age, old.age);
        assert!(
            (current.phase - (old.phase + old.frequency as f64 / 48000.0).fract()).abs() < 1e-12
        );
    }
}
