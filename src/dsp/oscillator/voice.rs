use super::modulation::{PreparedLfo, compose};
use super::note_schedule::{NoteBoundary, ScheduledNote, compiled_schedule};
use super::sample_tables::{SamplePyramid, WaveBank, periodic_sample, sample_bounds};
use super::{OscillatorFxParams, osc_sample};
use crate::config::note_configs::NoteOct;
use crate::config::osc_configs::Waveform;
use crate::dsp::envelope::{AhdsrParams, AhdsrState};
use crate::dsp::filter::{FilterDspState, FilterParams, process_sample as process_filter_sample};
// Unified OSC: fixed-capacity voices with per-note envelopes/filter state. The
// note schedule, sample and modulation curves are prepared off the callback.
use crate::config::osc_configs::{
    GlideMode, LfoMode, LfoTarget, SampleAsset, SampleCapture, SampleMode,
};
use std::sync::Arc;
#[derive(Clone)]
pub struct PolyOscRuntime {
    pub phrase: super::phrase::PhrasePlan,
    pub schedule: Arc<Vec<NoteBoundary>>,
    pub loop_ticks: usize,
    pub note_revision: u64,
    pub voices: usize,
    pub input_gate: bool,
    pub input_mod_gain: Option<f32>,
    pub lfos: [PreparedLfo; 2],
    pub mono_legato: bool,
    pub glide_ms: f32,
    pub glide_mode: GlideMode,
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
    sample_material: Option<Arc<SampleMaterial>>,
    // Moved out with the old runtime through the control envelope; dropped by
    // the worker, never by a voice ending or being stolen in the callback.
    retired_materials: [Option<Arc<SampleMaterial>>; MATERIAL_CAPACITY],
}
const MATERIAL_CAPACITY: usize = 17; // Sixteen held/releasing versions + current.
#[derive(Clone)]
struct SampleMaterial {
    sample: Arc<SampleAsset>,
    pyramid: Arc<SamplePyramid>,
    wave_bank: Arc<WaveBank>,
    mode: SampleMode,
    root_frequency: f32,
    loop_sample: bool,
    start: f32,
    end: f32,
}
impl SampleMaterial {
    fn matches(&self, other: &Self) -> bool {
        self.sample.content_hash == other.sample.content_hash
            && self.sample.sample_rate == other.sample.sample_rate
            && self.sample.frames.len() == other.sample.frames.len()
            && self.mode == other.mode
            && self.root_frequency == other.root_frequency
            && self.loop_sample == other.loop_sample
            && self.start == other.start
            && self.end == other.end
            && Arc::ptr_eq(&self.wave_bank, &other.wave_bank)
    }
}
impl PolyOscRuntime {
    pub fn from_config(c: &crate::config::OscillatorConfigs) -> Self {
        let (note_revision, schedule) = compiled_schedule(&c.note);
        let fine_cents = if c.sample_fine_cents.is_finite() {
            c.sample_fine_cents.clamp(-100.0, 100.0)
        } else {
            0.0
        };
        let lfos = [&c.lfo, &c.lfo2].map(PreparedLfo::from_config);
        let tone_revision = c.sample.as_ref().map_or(0, |s| s.content_hash)
            ^ ((c.waveform.value as u64) << 60)
            ^ ((c.sample_mode as u64) << 58)
            ^ c.sample_start.to_bits() as u64
            ^ ((c.sample_end.to_bits() as u64) << 32)
            ^ ((c.vocal_formant.to_bits() as u64) << 16);
        let wave_bank = WaveBank::prepare(c);
        let sample_pyramid = c.sample.as_ref().map(SamplePyramid::prepare);
        let root_frequency =
            NoteOct::from_pitch_index(c.sample_root).freq_hz() * 2.0f32.powf(fine_cents / 1200.0);
        let sample_material =
            c.sample
                .as_ref()
                .zip(sample_pyramid.as_ref())
                .map(|(sample, pyramid)| {
                    Arc::new(SampleMaterial {
                        sample: sample.clone(),
                        pyramid: pyramid.clone(),
                        wave_bank: wave_bank.clone(),
                        mode: c.sample_mode,
                        root_frequency,
                        loop_sample: c.sample_loop,
                        start: c.sample_start,
                        end: c.sample_end,
                    })
                });
        Self {
            sample_material,
            retired_materials: std::array::from_fn(|_| None),
            phrase: super::phrase::PhrasePlan::from_config(&c.note),
            schedule,
            tone_revision,
            loop_ticks: c.note.loop_len(),
            note_revision,
            voices: c.voices.clamp(1, 16),
            input_gate: c.input_gate,
            input_mod_gain: c
                .input_mod_sens
                .filter(|v| v.is_finite())
                .map(|v| 4.0 * 10.0f32.powf(v.clamp(-50.0, 50.0) * 0.48 / 20.0)),
            lfos,
            mono_legato: c.mono_legato,
            glide_ms: if c.glide_ms.is_finite() {
                c.glide_ms.clamp(0.0, 2000.0)
            } else {
                0.0
            },
            glide_mode: c.glide_mode,
            sample: c.sample.clone(),
            sample_pyramid,
            capture: c.capture.clone(),
            sample_mode: c.sample_mode,
            root_frequency,
            loop_sample: c.sample_loop,
            sample_start: c.sample_start,
            sample_end: c.sample_end,
            wave_bank,
        }
    }
}

#[derive(Clone, Copy)]
struct PolyVoice {
    id: u64,
    cycle: u64,
    frequency: f32,
    target_frequency: f32,
    glide_frequency: f64,
    glide_ratio: f64,
    glide_left: u32,
    velocity: f32,
    target_velocity: f32,
    gate: bool,
    active: bool,
    phase: f64,
    detune_phase: f64,
    sample_pos: f64,
    sample_slot: Option<u8>,
    sample_source: bool,
    lfo_phase: [f64; 2],
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
    fn set_pitch(&mut self, target: f32, time_ms: f32, sr: f32) {
        if self.target_frequency == target && (self.glide_left > 0 || self.frequency == target) {
            return;
        }
        self.target_frequency = target;
        if time_ms > 0.0 && self.frequency > 0.0 && self.frequency != target {
            self.glide_left = (time_ms as f64 * sr as f64 / 1000.0)
                .round()
                .clamp(1.0, u32::MAX as f64) as u32;
            self.glide_frequency = self.frequency as f64;
            self.glide_ratio =
                (target as f64 / self.glide_frequency).powf(1.0 / self.glide_left as f64);
        } else {
            self.frequency = target;
            self.glide_frequency = target as f64;
            self.glide_left = 0;
        }
    }
    fn advance_pitch(&mut self, enabled: bool) {
        if self.glide_left == 0 {
            return;
        }
        if !enabled {
            self.frequency = self.target_frequency;
            self.glide_frequency = self.target_frequency as f64;
            self.glide_left = 0;
            return;
        }
        self.glide_frequency *= self.glide_ratio;
        self.glide_left -= 1;
        self.frequency = if self.glide_left == 0 {
            self.target_frequency
        } else {
            self.glide_frequency as f32
        };
    }
    fn new() -> Self {
        Self {
            id: 0,
            cycle: 0,
            frequency: 440.0,
            target_frequency: 440.0,
            glide_frequency: 440.0,
            glide_ratio: 1.0,
            glide_left: 0,
            velocity: 1.0,
            target_velocity: 1.0,
            gate: false,
            active: false,
            phase: 0.0,
            detune_phase: 0.25,
            sample_pos: 0.0,
            sample_slot: None,
            sample_source: false,
            lfo_phase: [0.0; 2],
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
    phrase: super::phrase::PhraseState,
    phrase_restart_pending: bool,
    voices: Box<[PolyVoice; 16]>,
    materials: Box<[Option<Arc<SampleMaterial>>; MATERIAL_CAPACITY]>,
    current_material: Option<u8>,
    tick: Option<u64>,
    age: u64,
    last_time: f64,
    last_running: bool,
    capture_identity: usize,
    capture_position: usize,
    capture_started: bool,
    sample_identity: u64,
    fade: f32,
    free_phase: [f64; 2],
    mono_voice: Option<usize>,
    last_mono_frequency: Option<f32>,
    note_revision: u64,
    last_output: f32,
    transition_tail: f32,
    last_voice_limit: usize,
    level: f32,
    input_mod_level: f32,
}
impl PolyOscState {
    pub fn phrase_view(&self) -> super::phrase::PhraseView {
        self.phrase.view()
    }
    pub fn advance_phrase(&mut self, r: &PolyOscRuntime, tick: u64, running: bool) {
        self.phrase_restart_pending |= self
            .phrase
            .select(&r.phrase, tick, r.loop_ticks, running)
            .restarted;
    }
    pub fn new() -> Self {
        Self {
            phrase: super::phrase::PhraseState::default(),
            phrase_restart_pending: false,
            voices: Box::new(std::array::from_fn(|_| PolyVoice::new())),
            materials: Box::new(std::array::from_fn(|_| None)),
            current_material: None,
            tick: None,
            age: 0,
            last_time: -1.0,
            last_running: false,
            capture_identity: 0,
            capture_position: 0,
            capture_started: false,
            sample_identity: 0,
            fade: 1.0,
            free_phase: [0.0; 2],
            mono_voice: None,
            last_mono_frequency: None,
            note_revision: 0,
            last_output: 0.0,
            transition_tail: 0.0,
            last_voice_limit: 0,
            level: -1.0,
            input_mod_level: -1.0,
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
        self.input_mod_level = -1.0;
        self.mono_voice = None;
        self.last_mono_frequency = None;
    }
    /// Call before exchanging a prepared runtime. Every released generation is
    /// moved to the outgoing control payload; the audio callback never destroys
    /// a last reference. The new runtime starts with an empty retirement array.
    pub fn retire_materials(&mut self, outgoing: &mut PolyOscRuntime) {
        for slot in 0..MATERIAL_CAPACITY {
            if self
                .voices
                .iter()
                .any(|v| v.active && v.sample_slot == Some(slot as u8))
            {
                continue;
            }
            let Some(free) = outgoing.retired_materials.iter_mut().find(|v| v.is_none()) else {
                break;
            };
            if self.materials[slot].is_some() {
                *free = self.materials[slot].take();
                if self.current_material == Some(slot as u8) {
                    self.current_material = None;
                }
            }
        }
    }
    fn bind_material(&mut self, r: &PolyOscRuntime) -> Option<u8> {
        let Some(material) = &r.sample_material else {
            self.current_material = None;
            return None;
        };
        if let Some(slot) = self.current_material {
            if self.materials[slot as usize]
                .as_ref()
                .is_some_and(|old| Arc::ptr_eq(old, material) || old.matches(material))
            {
                return Some(slot as u8);
            }
        }
        if let Some(slot) = self.materials.iter().position(|old| {
            old.as_ref()
                .is_some_and(|old| Arc::ptr_eq(old, material) || old.matches(material))
        }) {
            self.current_material = Some(slot as u8);
            return Some(slot as u8);
        }
        let free = self.materials.iter().position(Option::is_none);
        debug_assert!(
            free.is_some(),
            "Prepared runtime exchange must retire unused sample generations"
        );
        if let Some(slot) = free {
            self.materials[slot] = Some(material.clone());
            self.current_material = Some(slot as u8);
        }
        free.map(|slot| slot as u8)
    }
    pub fn capture(&mut self, r: &PolyOscRuntime, input: f32, sr: f32, capture_threshold: f32) {
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
        if !self.capture_started && input.abs() >= capture_threshold {
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
    let absolute_tick = sequence_tick(elapsed, sr, bpm);
    let selection = state
        .phrase
        .select(&r.phrase, absolute_tick, r.loop_ticks, running);
    let (schedule, loop_ticks, note_revision) = if selection.queued {
        let queued = r.phrase.queued.as_ref().expect("selected prepared phrase");
        (&queued.schedule, queued.length, queued.revision)
    } else {
        (&r.schedule, r.loop_ticks, r.note_revision)
    };
    let phrase_restarted = std::mem::take(&mut state.phrase_restart_pending);
    if selection.restarted || phrase_restarted {
        for voice in state.voices.iter_mut() {
            voice.gate = false;
            voice.cycle = u64::MAX;
        }
        state.tick = None;
        state.mono_voice = None;
        state.last_mono_frequency = None;
    }
    if state.note_revision != note_revision {
        // Reconcile note IDs instead of restarting every unrelated held voice
        // when one note is moved, resized or has its velocity edited.
        state.tick = None;
        state.note_revision = note_revision;
    }
    state.last_time = elapsed;
    state.last_running = running;
    let target_input_mod = r
        .input_mod_gain
        .map_or(1.0, |gain| (p.input_level.max(0.0) * gain).clamp(0.0, 1.0));
    if state.input_mod_level < 0.0 {
        state.input_mod_level = target_input_mod;
    } else {
        state.input_mod_level += (target_input_mod - state.input_mod_level) / (0.005 * sr).max(1.0);
    }
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
    let current_material = if p.waveform == Waveform::Sample {
        state.bind_material(r)
    } else {
        None
    };
    if state.sample_identity != sample_hash {
        state.sample_identity = sample_hash;
        if p.waveform != Waveform::Sample
            && !state.voices.iter().any(|v| v.active && v.sample_source)
        {
            state.transition_tail = state.last_output;
            state.fade = 0.0;
        }
        for v in state.voices.iter_mut().filter(|v| !v.sample_source) {
            v.wave_limit = 0.0;
        }
    }
    state.fade = (state.fade + 1.0 / (0.005 * sr)).min(1.0);
    let enabled = running
        && loop_ticks > 0
        && !schedule.is_empty()
        && (!r.input_gate || p.input_level >= p.gate_threshold);
    let cycle = selection.elapsed_ticks / loop_ticks.max(1) as u64;
    let local_tick = (selection.elapsed_ticks % loop_ticks.max(1) as u64) as usize;
    let boundary = schedule
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
        let row = &schedule[boundary].notes;
        let chosen = &row[row.len().saturating_sub(r.voices)..];
        if r.voices == 1 && (r.mono_legato || r.glide_ms > 0.0) {
            reconcile_mono(
                state,
                r,
                chosen.last(),
                schedule[boundary].connected,
                cycle,
                sr,
                p.waveform == Waveform::Sample,
                current_material,
            );
        } else {
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
                    voice.target_frequency = n.frequency;
                    voice.glide_left = 0;
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
                                    .then_with(|| {
                                        va.last_output.abs().total_cmp(&vb.last_output.abs())
                                    })
                                    .then_with(|| va.age.cmp(&vb.age))
                            })
                            .unwrap_or(0)
                    });
                let tail = state.voices[index].last_output;
                state.age = state.age.wrapping_add(1);
                let mut v = PolyVoice::new();
                v.id = n.id;
                v.cycle = cycle;
                v.sample_source = p.waveform == Waveform::Sample;
                v.sample_slot = current_material;
                v.frequency = n.frequency;
                v.target_frequency = n.frequency;
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
    }
    let increments = r.lfos.each_ref().map(|lfo| lfo.increment(bpm, sr));
    for (phase, increment) in state.free_phase.iter_mut().zip(increments) {
        if running {
            *phase = (*phase + increment).fract();
        }
    }
    let free_modulation: [f32; 2] = std::array::from_fn(|i| {
        if r.lfos[i].enabled {
            r.lfos[i].value(state.free_phase[i])
        } else {
            1.0
        }
    });
    let mut output = 0.0;
    for (voice_index, v) in state.voices.iter_mut().enumerate() {
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
        for (phase, increment) in v.lfo_phase.iter_mut().zip(increments) {
            *phase = (*phase + increment).fract();
        }
        let values = std::array::from_fn(|i| {
            let lfo = &r.lfos[i];
            if !lfo.enabled {
                1.0
            } else if lfo.mode == LfoMode::Free {
                free_modulation[i]
            } else {
                lfo.value(v.lfo_phase[i])
            }
        });
        let modulation = compose(&r.lfos, values);
        v.advance_pitch(r.voices == 1 && r.glide_ms > 0.0);
        if r.voices == 1 && state.mono_voice == Some(voice_index) {
            state.last_mono_frequency = Some(v.frequency);
        }
        let frequency = v.frequency * modulation.pitch;
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
            (if v.sample_source {
                v.sample_slot
                    .and_then(|slot| state.materials[slot as usize].as_ref())
                    .map_or(0.0, |material| sample_voice(v, material, frequency, sr))
            } else {
                match p.waveform {
                    Waveform::Sample => 0.0,
                    Waveform::DetuneSaw => {
                        let down = frequency * super::DETUNE_DOWN as f32;
                        let up = frequency * super::DETUNE_UP as f32;
                        let low = osc_sample(Waveform::Saw, v.phase as f32, down, sr).0;
                        let high = osc_sample(Waveform::Saw, v.detune_phase as f32, up, sr).0;
                        (low + high * ((sr * 0.5 - up) / (sr * 0.05)).clamp(0.0, 1.0)) * 0.5
                    }
                    Waveform::Vocal
                    | Waveform::Triangle
                    | Waveform::Rect
                    | Waveform::VintageSaw => {
                        r.wave_bank
                            .read_transition(v.phase, v.wave_level, frequency / v.wave_limit)
                    }
                    _ => osc_sample(p.waveform, v.phase as f32, frequency, sr).0,
                }
            }) * nyquist_gain
        };
        let detuned = !v.sample_source && p.waveform == Waveform::DetuneSaw;
        v.phase = (v.phase
            + frequency as f64 / sr as f64 * if detuned { super::DETUNE_DOWN } else { 1.0 })
        .fract();
        if detuned {
            v.detune_phase =
                (v.detune_phase + frequency as f64 / sr as f64 * super::DETUNE_UP).fract();
        }
        let cutoff_env = v
            .filter_env
            .next(v.gate, false, p.filter_envelope, 1.0 / sr);
        let min = p.cutoff_min_hz.max(10.0);
        let max = p.filter.cutoff_hz.max(min);
        let volume = modulation.volume;
        let dry = raw * amp * v.velocity * volume * state.level * state.input_mod_level;
        let mut sample = if p.filter.mix <= 0.0 {
            dry
        } else {
            if v.control_tick == 0 || v.cutoff_max != max {
                v.cutoff_base = min * (max / min).powf(cutoff_env);
                v.cutoff_max = max;
            }
            let cutoff = v.cutoff_base * modulation.cutoff;
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
fn reconcile_mono(
    state: &mut PolyOscState,
    r: &PolyOscRuntime,
    note: Option<&ScheduledNote>,
    connected: bool,
    cycle: u64,
    sr: f32,
    sample_source: bool,
    sample_slot: Option<u8>,
) {
    let Some(n) = note else {
        for v in state.voices.iter_mut() {
            v.gate = false;
        }
        return;
    };
    let matching = state
        .voices
        .iter()
        .position(|v| v.active && v.gate && v.id == n.id && v.cycle == cycle);
    let index = matching
        .or(state.mono_voice)
        .or_else(|| {
            state
                .voices
                .iter()
                .enumerate()
                .filter(|(_, v)| v.active && v.gate)
                .max_by_key(|(_, v)| v.age)
                .map(|(i, _)| i)
        })
        .unwrap_or(0);
    let old = state.voices[index];
    let tied = old.active && old.gate && old.cycle == cycle && connected;
    let from = if old.active {
        old.frequency
    } else {
        state.last_mono_frequency.unwrap_or(n.frequency)
    };
    for v in state.voices.iter_mut() {
        v.gate = false;
    }
    state.mono_voice = Some(index);
    if matching == Some(index) || (r.mono_legato && tied) {
        let v = &mut state.voices[index];
        v.id = n.id;
        v.cycle = cycle;
        v.gate = true;
        v.target_velocity = n.velocity;
        if matching.is_none() && (v.sample_source != sample_source || v.sample_slot != sample_slot)
        {
            v.steal_tail = v.last_output;
            v.steal_left = 64;
            v.sample_source = sample_source;
            v.sample_slot = sample_slot;
            v.sample_pos = 0.0;
            v.sample_level = 0;
            v.sample_limit = 1.0;
            v.wave_limit = 0.0;
        }
        v.set_pitch(
            n.frequency,
            if tied || matching.is_some() {
                r.glide_ms
            } else {
                0.0
            },
            sr,
        );
    } else {
        state.age = state.age.wrapping_add(1);
        let mut voice = PolyVoice::new();
        voice.id = n.id;
        voice.sample_source = sample_source;
        voice.sample_slot = sample_slot;
        voice.cycle = cycle;
        voice.active = true;
        voice.gate = true;
        voice.age = state.age;
        voice.velocity = n.velocity;
        voice.target_velocity = n.velocity;
        let glide = r.glide_ms > 0.0 && (tied || r.glide_mode == GlideMode::AllNotes);
        voice.frequency = if glide { from } else { n.frequency };
        voice.target_frequency = voice.frequency;
        voice.set_pitch(n.frequency, if glide { r.glide_ms } else { 0.0 }, sr);
        voice.steal_tail = old.last_output;
        voice.steal_left = 64;
        state.voices[index] = voice;
    }
}
pub fn sequence_tick(elapsed: f64, sample_rate: f32, bpm: usize) -> u64 {
    // elapsed is derived from an integer audio frame. Recover it before doing
    // rational PPQ conversion; floating beat-flooring can be late near long takes.
    let rate = sample_rate.max(1.0).round() as u64;
    let frame = (elapsed.max(0.0) * rate as f64).round() as u64;
    ((frame as u128 * bpm.max(1) as u128 * crate::config::sequence_edit::PPQ as u128)
        / (rate as u128 * 60))
        .min(u64::MAX as u128) as u64
}

fn sample_voice(v: &mut PolyVoice, r: &SampleMaterial, frequency: f32, sr: f32) -> f32 {
    let s = &r.sample;
    if s.frames.len() < 4 {
        return 0.0;
    }
    if r.mode == SampleMode::Wavetable {
        return r
            .wave_bank
            .read_transition(v.phase, v.wave_level, frequency / v.wave_limit);
    }
    let (start, end) = sample_bounds(s.frames.len(), r.start, r.end);
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
    let (data, scale) = {
        let pyramid = &r.pyramid;
        let level = v.sample_level.min(pyramid.levels.len());
        if level > 0 {
            (&pyramid.levels[level - 1][..], (1usize << level) as f64)
        } else {
            (&s.frames[..], 1.0)
        }
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
    pub(super) fn params(sr: f32, waveform: Waveform) -> OscillatorFxParams {
        OscillatorFxParams {
            waveform,
            level: 0.7,
            gate_threshold: 0.0,
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
                runtime.sample_material.as_ref().unwrap(),
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
    #[test]
    fn independent_free_and_retrigger_phases_are_preserved_per_lfo_and_voice() {
        let mut c = OscillatorConfigs::new();
        c.note.replace_events(
            3840,
            &[
                NoteEvent::new(0, 1920, NoteOct::from_pitch_index(48)),
                NoteEvent::new(480, 1440, NoteOct::from_pitch_index(55)),
            ],
        );
        c.lfo.enabled = true;
        c.lfo.sync = false;
        c.lfo.rate_hz = 1.0;
        c.lfo.mode = LfoMode::Retrigger;
        c.lfo2.enabled = true;
        c.lfo2.sync = false;
        c.lfo2.rate_hz = 3.0;
        c.lfo2.mode = LfoMode::Retrigger;
        let runtime = PolyOscRuntime::from_config(&c);
        let mut state = PolyOscState::new();
        for frame in 0..24000 {
            process_poly_sample(
                &mut state,
                &runtime,
                params(48000.0, Waveform::Sine),
                frame as f64 / 48000.0,
                120,
                true,
            );
        }
        let ids = c.note.events();
        let first = state.voices.iter().find(|v| v.id == ids[0].id).unwrap();
        let second = state.voices.iter().find(|v| v.id == ids[1].id).unwrap();
        for (actual, expected) in [
            (first.lfo_phase[0], 0.5),
            (first.lfo_phase[1], 0.5),
            (second.lfo_phase[0], 0.25),
            (second.lfo_phase[1], 0.75),
            (state.free_phase[0], 0.5),
            (state.free_phase[1], 0.5),
        ] {
            assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
        }
    }
    #[test]
    fn mono_legato_keeps_envelopes_and_lfo_phase_but_adjacent_notes_retrigger() {
        let mut c = OscillatorConfigs::new();
        c.voices = 1;
        c.mono_legato = true;
        c.lfo.mode = LfoMode::Retrigger;
        c.lfo.sync = false;
        c.lfo.rate_hz = 1.0;
        c.note.replace_events(
            3840,
            &[
                NoteEvent::new(0, 1440, NoteOct::from_pitch_index(48)),
                NoteEvent::new(480, 480, NoteOct::from_pitch_index(55)),
                NoteEvent::new(1440, 480, NoteOct::from_pitch_index(52)),
            ],
        );
        let runtime = PolyOscRuntime::from_config(&c);
        let mut state = PolyOscState::new();
        let mut age = 0;
        for frame in 0..36001 {
            process_poly_sample(
                &mut state,
                &runtime,
                params(48000.0, Waveform::Sine),
                frame as f64 / 48000.0,
                120,
                true,
            );
            if frame == 100 {
                age = state.voices.iter().find(|v| v.gate).unwrap().age;
            }
            if frame == 12000 || frame == 24000 {
                let voice = state.voices.iter().find(|v| v.gate).unwrap();
                assert_eq!(voice.age, age);
                assert!(voice.lfo_phase[0] > 0.2);
            }
        }
        let voice = state.voices.iter().find(|v| v.gate).unwrap();
        assert!(voice.age > age);
        assert!(voice.lfo_phase[0] < 0.001);
    }
    #[test]
    fn glide_reaches_target_in_exact_time_and_does_not_allocate() {
        for sr in [44100.0, 48000.0, 96000.0, 192000.0] {
            let mut voice = PolyVoice::new();
            voice.frequency = 220.0;
            voice.target_frequency = 220.0;
            voice.set_pitch(880.0, 100.0, sr);
            let frames = (sr / 10.0) as usize;
            let mut middle = 0.0;
            let allocations = crate::test_alloc::count(|| {
                for frame in 0..frames {
                    voice.advance_pitch(true);
                    if frame + 1 == frames / 2 {
                        middle = voice.frequency;
                    }
                }
            });
            assert_eq!(allocations, 0);
            assert!((middle - 440.0).abs() < 0.01);
            assert_eq!(voice.frequency, 880.0);
            assert_eq!(voice.glide_left, 0);
        }
        let mut c = OscillatorConfigs::new();
        c.voices = 1;
        c.mono_legato = true;
        c.glide_ms = 100.0;
        c.lfo.enabled = true;
        c.lfo.target = LfoTarget::Cutoff;
        c.lfo2.enabled = true;
        c.lfo2.target = LfoTarget::Pitch;
        c.note.replace_events(
            3840,
            &[
                NoteEvent::new(0, 1920, NoteOct::from_pitch_index(48)),
                NoteEvent::new(480, 960, NoteOct::from_pitch_index(60)),
            ],
        );
        let runtime = PolyOscRuntime::from_config(&c);
        let mut state = PolyOscState::new();
        let mut p = params(48000.0, Waveform::Saw);
        p.filter.mix = 1.0;
        assert_eq!(
            crate::test_alloc::count(|| for frame in 0..48000 {
                assert!(
                    process_poly_sample(&mut state, &runtime, p, frame as f64 / 48000.0, 120, true)
                        .is_finite()
                );
            }),
            0
        );
    }
    #[test]
    fn legacy_patch_defaults_and_disabled_second_lfo_keep_identical_audio() {
        let mut config = crate::config::AppConfig::new(120, 0, 5);
        config
            .input_fx
            .set_slot_kind(0, 0, crate::config::FxKind::Oscillator);
        let Some(crate::config::InputFx::Oscillator(osc)) =
            &mut config.input_fx.banks[0].slots[0].fx
        else {
            panic!()
        };
        osc.note.replace_events(
            3840,
            &[NoteEvent::new(0, 3840, NoteOct::from_pitch_index(48))],
        );
        osc.lfo.enabled = true;
        osc.lfo.target = LfoTarget::Pitch;
        let current = PolyOscRuntime::from_config(osc);
        let mut data = serde_json::to_value(crate::project::data_from_config(&config)).unwrap();
        let object = data["input_fx"]["banks"][0]["slots"][0]["osc"]
            .as_object_mut()
            .unwrap();
        for field in ["lfo2", "mono_legato", "glide_ms", "glide_mode"] {
            object.remove(field);
        }
        let mut migrated = crate::config::AppConfig::new(120, 0, 5);
        crate::project::apply_data_to_config(&mut migrated, serde_json::from_value(data).unwrap());
        let Some(crate::config::InputFx::Oscillator(osc)) =
            &mut migrated.input_fx.banks[0].slots[0].fx
        else {
            panic!()
        };
        assert!(!osc.lfo2.enabled && !osc.mono_legato);
        assert_eq!(osc.glide_ms, 0.0);
        osc.lfo2.target = LfoTarget::Pitch;
        osc.lfo2.depth = 1.0;
        osc.lfo2.rate_hz = 17.0;
        let old = PolyOscRuntime::from_config(osc);
        let mut a = PolyOscState::new();
        let mut b = PolyOscState::new();
        for frame in 0..12000 {
            let p = params(48000.0, Waveform::Sine);
            let time = frame as f64 / 48000.0;
            assert_eq!(
                process_poly_sample(&mut a, &current, p, time, 120, true).to_bits(),
                process_poly_sample(&mut b, &old, p, time, 120, true).to_bits()
            );
        }
    }
    #[test]
    fn glide_overlap_and_all_notes_have_distinct_gap_behavior() {
        for (mode, overlap, should_glide) in [
            (GlideMode::Overlap, false, false),
            (GlideMode::AllNotes, false, true),
            (GlideMode::Overlap, true, true),
        ] {
            let mut c = OscillatorConfigs::new();
            c.voices = 1;
            c.glide_ms = 100.0;
            c.glide_mode = mode;
            c.note.replace_events(
                3840,
                &[
                    NoteEvent::new(
                        0,
                        if overlap { 960 } else { 240 },
                        NoteOct::from_pitch_index(48),
                    ),
                    NoteEvent::new(480, 960, NoteOct::from_pitch_index(57)),
                ],
            );
            let runtime = PolyOscRuntime::from_config(&c);
            let mut state = PolyOscState::new();
            for frame in 0..12001 {
                process_poly_sample(
                    &mut state,
                    &runtime,
                    params(48000.0, Waveform::Sine),
                    frame as f64 / 48000.0,
                    120,
                    true,
                );
            }
            let voice = state.voices.iter().find(|v| v.gate).unwrap();
            if should_glide {
                assert!(voice.frequency > 261.0 && voice.frequency < 300.0);
                assert!(voice.glide_left > 0);
            } else {
                assert_eq!(voice.frequency, 440.0);
                assert_eq!(voice.glide_left, 0);
            }
            for frame in 12001..16800 {
                process_poly_sample(
                    &mut state,
                    &runtime,
                    params(48000.0, Waveform::Sine),
                    frame as f64 / 48000.0,
                    120,
                    true,
                );
            }
            assert_eq!(
                state.voices.iter().find(|v| v.gate).unwrap().frequency,
                440.0
            );
        }
    }
    #[test]
    fn editing_lfo_two_keeps_lfo_one_phase_continuous() {
        let mut c = OscillatorConfigs::new();
        c.note.replace_events(
            3840,
            &[NoteEvent::new(0, 3840, NoteOct::from_pitch_index(48))],
        );
        c.lfo.enabled = true;
        c.lfo.sync = false;
        c.lfo.rate_hz = 1.0;
        c.lfo2.enabled = true;
        c.lfo2.sync = false;
        c.lfo2.rate_hz = 3.0;
        let runtime = PolyOscRuntime::from_config(&c);
        let mut state = PolyOscState::new();
        for frame in 0..1000 {
            process_poly_sample(
                &mut state,
                &runtime,
                params(48000.0, Waveform::Sine),
                frame as f64 / 48000.0,
                120,
                true,
            );
        }
        let before = state.free_phase;
        c.lfo2.rate_hz = 11.0;
        c.lfo2.depth = 0.9;
        let changed = PolyOscRuntime::from_config(&c);
        process_poly_sample(
            &mut state,
            &changed,
            params(48000.0, Waveform::Sine),
            1000.0 / 48000.0,
            120,
            true,
        );
        assert!((state.free_phase[0] - (before[0] + 1.0 / 48000.0)).abs() < 1e-12);
        assert!((state.free_phase[1] - (before[1] + 11.0 / 48000.0)).abs() < 1e-12);
    }
    #[test]
    fn sound_preset_roundtrip_preserves_both_lfos_and_mono_controls() {
        let mut c = crate::config::AppConfig::new(120, 0, 5);
        c.input_fx
            .set_slot_kind(0, 0, crate::config::FxKind::Oscillator);
        let Some(crate::config::InputFx::Oscillator(osc)) = &mut c.input_fx.banks[0].slots[0].fx
        else {
            panic!()
        };
        osc.voices = 1;
        osc.mono_legato = true;
        osc.glide_ms = 123.5;
        osc.glide_mode = GlideMode::AllNotes;
        osc.lfo.enabled = true;
        osc.lfo.target = LfoTarget::Cutoff;
        osc.lfo.beats = 0.25;
        osc.lfo2.enabled = true;
        osc.lfo2.target = LfoTarget::Pitch;
        osc.lfo2.sync = false;
        osc.lfo2.rate_hz = 7.25;
        osc.lfo2.mode = LfoMode::Retrigger;
        let lfos = [osc.lfo.clone(), osc.lfo2.clone()];
        let preset =
            crate::presets::encode(&c, crate::presets::FxTarget::Input { bank: 0, slot: 0 })
                .unwrap();
        crate::presets::decode(
            &mut c,
            crate::presets::FxTarget::Input { bank: 1, slot: 2 },
            &preset,
        )
        .unwrap();
        let Some(crate::config::InputFx::Oscillator(osc)) = &c.input_fx.banks[1].slots[2].fx else {
            panic!()
        };
        assert_eq!(osc.lfo, lfos[0]);
        assert_eq!(osc.lfo2, lfos[1]);
        assert!(osc.mono_legato);
        assert_eq!(osc.glide_ms, 123.5);
        assert_eq!(osc.glide_mode, GlideMode::AllNotes);
    }
}

#[cfg(test)]
#[path = "sample_lifetime_tests.rs"]
mod sample_lifetime_tests;

#[cfg(test)]
#[path = "bot_controls_tests.rs"]
mod bot_controls_tests;
