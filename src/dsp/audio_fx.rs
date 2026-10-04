//! Shared rack processors. Algorithms are original implementations of public
//! DSP structures, not reverse-engineered BOSS code or a hardware-equivalence claim.
use super::{
    biquad::{self, Biquad, Coeff},
    pitch_tracker::PitchTracker,
    reverb::{ReverbDspState, ReverbParams},
};
use crate::config::audio_fx::{AudioFxConfig, AudioFxKind as K, DriveStyle, Scale};
use std::f32::consts::{PI, TAU};

#[derive(Clone, Copy)]
pub struct AudioFxParams {
    pub config: AudioFxConfig,
    pub signature: u64,
}
impl AudioFxParams {
    pub fn latency_frames(&self, sr: f32) -> usize {
        if matches!(
            self.config.kind,
            K::Transpose | K::Electric | K::Harmonist | K::Octave
        ) {
            super::pitch_shift::latency_frames(sr)
        } else {
            0
        }
    }
    /// Called on the control thread, never from process().
    pub fn new(config: &AudioFxConfig) -> Self {
        use std::hash::{Hash, Hasher};
        let config = config.sanitized();
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        serde_json::to_vec(&config)
            .unwrap_or_default()
            .hash(&mut hash);
        Self {
            config,
            signature: hash.finish(),
        }
    }
}

#[derive(Clone)]
pub struct AudioFxState {
    pdc: bool,
    aligned_dry: [f32; 2],
    pitch: [super::pitch_shift::PitchShift; 2],
    stagger: usize,
    delay: super::delay::DelayDspState,
    delay_feedback: f32,
    dynamics: super::dynamics::DynamicsState,
    distortion: super::distortion::DistortionState,
    dynamics_params: AudioFxParams,
    sr: f32,
    ring: Vec<[f32; 2]>,
    write: usize,
    filled: usize,
    signature: u64,
    kind: Option<K>,
    phase: f64,
    modulation: super::audio_modulation::ModulationState,
    slicer_compress_mix: f32,
    sustainer_tone: [f32; 2],
    pitch_step: Option<usize>,
    ratio: [f32; 2],
    target_ratio: [f32; 2],
    level: f32,
    level_target: f32,
    mix: f32,
    smooth: f32,
    lp: [f32; 2],
    dc_x: [f32; 2],
    dc_y: [f32; 2],
    drive_previous: [f32; 2],
    tone_alpha: f32,
    drive: f32,
    retune: f32,
    gate: f32,
    eq: [[Biquad; 4]; 2],
    coeff: [Coeff; 4],
    phaser: [[f32; 12]; 2],
    phaser_stages: usize,
    phaser_target_stages: usize,
    phaser_old: [[f32; 12]; 2],
    phaser_old_feedback: [f32; 2],
    phaser_old_stages: usize,
    phaser_fade_left: usize,
    phaser_shift: f32,
    phaser_shift_target: f32,
    flanger_shift: f32,
    flanger_shift_target: f32,
    chorus_filters: [[super::filter::FilterDspState; 2]; 2],
    chorus_filter_mix: [f32; 2],
    chorus_cut_hz: [f32; 2],
    enhance: super::stereo_enhance::StereoEnhance,
    phaser_feedback: [f32; 2],
    phaser_a: f32,
    control_tick: u8,
    tracker: PitchTracker,
    last_note: Option<f32>,
    freeze_end: usize,
    freeze_length: usize,
    frozen: bool,
    freeze_phase: [f32; 2],
    reverb: ReverbDspState,
    freeze_mix: f32,
    freeze_level: f32,
    freeze_attack: f32,
    freeze_release: f32,
    freeze_decay: f32,
    dc_coefficient: f32,
}
impl AudioFxState {
    pub fn new(sr: f32) -> Self {
        Self::new_with_stagger(sr, 0)
    }
    pub fn new_with_stagger(sr: f32, stagger: usize) -> Self {
        let sr = sr.max(1000.0);
        let pitch_plan = super::pitch_shift::Plan::new(sr);
        let hop = (super::pitch_shift::latency_frames(sr) + 1) / 4;
        let mut reverb = ReverbDspState::new();
        reverb.prepare(sr);
        Self {
            pdc: false,
            aligned_dry: [0.0; 2],
            stagger,
            pitch: std::array::from_fn(|voice| {
                super::pitch_shift::PitchShift::new_with_offset_and_rate(
                    pitch_plan.clone(),
                    (stagger * 2 + voice) * hop / 48,
                    sr,
                )
            }),
            delay: super::delay::DelayDspState::new(sr),
            delay_feedback: 0.35,
            dynamics: super::dynamics::DynamicsState::default(),
            distortion: super::distortion::DistortionState::new(sr),
            dynamics_params: AudioFxParams::new(&AudioFxConfig::new(K::Dynamics)),
            sr,
            ring: vec![[0.0; 2]; (sr * 0.12).ceil() as usize + 4],
            write: 0,
            filled: 0,
            signature: 0,
            kind: None,
            phase: 0.0,
            modulation: Default::default(),
            slicer_compress_mix: 0.0,
            sustainer_tone: [0.0; 2],
            pitch_step: None,
            ratio: [1.0; 2],
            target_ratio: [1.0; 2],
            level: 1.0,
            level_target: 1.0,
            mix: 1.0,
            smooth: 1.0 - (-1.0 / (sr * 0.005)).exp(),
            lp: [0.0; 2],
            dc_x: [0.0; 2],
            dc_y: [0.0; 2],
            drive_previous: [0.0; 2],
            tone_alpha: 0.5,
            drive: 1.0,
            retune: 1.0,
            gate: 0.0,
            eq: [[Biquad::default(); 4]; 2],
            coeff: [Coeff::default(); 4],
            phaser: [[0.0; 12]; 2],
            phaser_stages: 6,
            phaser_target_stages: 6,
            phaser_old: [[0.0; 12]; 2],
            phaser_old_feedback: [0.0; 2],
            phaser_old_stages: 6,
            phaser_fade_left: 0,
            phaser_shift: 1.0,
            phaser_shift_target: 1.0,
            flanger_shift: 1.0,
            flanger_shift_target: 1.0,
            chorus_filters: [[super::filter::FilterDspState::new(); 2]; 2],
            chorus_filter_mix: [0.0; 2],
            chorus_cut_hz: [20.0, 12500.0],
            enhance: super::stereo_enhance::StereoEnhance::new(sr),
            phaser_feedback: [0.0; 2],
            phaser_a: 0.0,
            control_tick: 0,
            tracker: PitchTracker::new(sr),
            last_note: None,
            freeze_end: 0,
            freeze_length: 0,
            frozen: false,
            freeze_phase: [0.0, 0.5],
            reverb,
            freeze_mix: 0.0,
            freeze_level: 1.0,
            freeze_attack: 0.1,
            freeze_release: 0.01,
            freeze_decay: 0.001,
            dc_coefficient: (-TAU * 10.0 / sr).exp(),
        }
    }
    pub fn prepare(&mut self, sr: f32) {
        if (self.sr - sr).abs() > 0.5 {
            *self = Self::new_with_stagger(sr, self.stagger);
        }
    }
    pub fn set_pdc(&mut self, enabled: bool) {
        self.pdc = enabled;
    }
    pub fn aligned_dry(&self) -> (f32, f32) {
        (self.aligned_dry[0], self.aligned_dry[1])
    }
    pub fn reset(&mut self) {
        for pitch in &mut self.pitch {
            pitch.reset();
        }
        self.delay.reset();
        self.dynamics.reset();
        self.distortion.reset();
        // Logical clearing: reads are gated by `filled`, never scan the two-second ring in a callback.
        self.write = 0;
        self.filled = 0;
        self.phase = 0.0;
        self.modulation = Default::default();
        self.slicer_compress_mix = 0.0;
        self.sustainer_tone = [0.0; 2];
        self.pitch_step = None;
        self.ratio = [1.0; 2];
        self.target_ratio = [1.0; 2];
        self.lp = [0.0; 2];
        self.dc_x = [0.0; 2];
        self.dc_y = [0.0; 2];
        self.drive_previous = [0.0; 2];
        self.gate = 0.0;
        self.eq = [[Biquad::default(); 4]; 2];
        self.phaser = [[0.0; 12]; 2];
        self.phaser_old = [[0.0; 12]; 2];
        self.phaser_old_feedback = [0.0; 2];
        self.phaser_fade_left = 0;
        self.chorus_filters = [[super::filter::FilterDspState::new(); 2]; 2];
        self.chorus_filter_mix = [0.0; 2];
        self.enhance.reset();
        self.phaser_feedback = [0.0; 2];
        self.tracker.reset();
        self.last_note = None;
        self.frozen = false;
        self.freeze_length = 0;
        self.freeze_mix = 0.0;
        self.freeze_level = 1.0;
        self.reverb.reset();
        self.kind = None;
    }
    fn configure(&mut self, r: &AudioFxParams) {
        let p = &r.config;
        if self.kind != Some(p.kind) {
            self.reset();
            self.kind = Some(p.kind);
            self.phaser_stages = p.phaser_stages as usize;
            self.phaser_shift = 2.0f32.powf((p.phaser_manual - 0.5) * 8.0);
            self.flanger_shift = 2.0f32.powf((0.5 - p.flanger_manual) * 4.0);
            self.mix = p.mix;
            self.sustainer_tone = [p.low_db, p.high_db];
            self.ratio = [2.0_f32.powf(p.semitones / 12.0), 0.25];
        }
        self.signature = r.signature;
        if p.kind == K::Distortion {
            self.distortion.configure(p, self.sr);
        }
        self.enhance.configure(p);
        let formant_ratio = 2.0_f32.powf(p.formant_shift_semitones / 12.0);
        for voice in &mut self.pitch {
            voice.set_formants(p.preserve_formants, formant_ratio);
        }
        self.pitch_step = None;
        self.delay_feedback = if p.feedback_repeats == 0 {
            p.feedback
        } else {
            10.0_f32.powf(-3.0 / p.feedback_repeats as f32)
        };
        self.level_target = 10.0_f32.powf(p.level_db / 20.0);
        self.tone_alpha = 1.0 - (-TAU * p.tone_hz.min(self.sr * 0.45) / self.sr).exp();
        self.drive = 10.0_f32.powf(p.drive_db / 20.0);
        self.dynamics_params = *r;
        self.dynamics_params.config.level_db = 0.0;
        self.dynamics_params.config.mix = 1.0;
        if p.kind == K::StepSlicer {
            self.dynamics_params.config.dynamics_mode =
                crate::config::audio_fx::DynamicsMode::Compressor;
            self.dynamics_params.config.ratio = 4.0;
            self.dynamics_params.config.attack_ms = 2.0;
            self.dynamics_params.config.release_ms = 60.0;
        }
        self.retune = if p.retune_ms <= 0.0 {
            1.0
        } else {
            1.0 - (-1.0 / (self.sr * p.retune_ms * 0.001)).exp()
        };
        self.freeze_attack = 1.0 - (-1.0 / (self.sr * p.attack_ms * 0.001)).exp();
        self.freeze_release = 1.0 - (-1.0 / (self.sr * p.release_ms * 0.001)).exp();
        self.freeze_decay = 1.0 - (-1.0 / (self.sr * p.decay_ms * 0.001)).exp();
        self.target_ratio = [2.0_f32.powf(p.semitones / 12.0), 0.25];
        self.coeff = [
            biquad::shelf(
                self.sr,
                if p.kind == K::Sustainer {
                    120.0
                } else {
                    p.low_hz
                },
                if p.kind == K::Sustainer {
                    self.sustainer_tone[0]
                } else {
                    p.low_db
                },
                false,
            ),
            biquad::peak(self.sr, p.mid_hz, p.mid_q, p.mid_db),
            biquad::peak(self.sr, p.high_mid_hz, p.high_mid_q, p.high_mid_db),
            biquad::shelf(
                self.sr,
                if p.kind == K::Sustainer {
                    6000.0
                } else {
                    p.high_hz
                },
                if p.kind == K::Sustainer {
                    self.sustainer_tone[1]
                } else {
                    p.high_db
                },
                true,
            ),
        ];
        self.phaser_target_stages = p.phaser_stages as usize;
        self.phaser_shift_target = 2.0f32.powf((p.phaser_manual - 0.5) * 8.0);
        self.flanger_shift_target = 2.0f32.powf((0.5 - p.flanger_manual) * 4.0);
    }
    fn read(&self, delay: f32) -> [f32; 2] {
        if delay > self.filled as f32 {
            return [0.0; 2];
        }
        let pos = (self.write as f32 - delay).rem_euclid(self.ring.len() as f32);
        let base = pos.floor();
        let i = base as usize % self.ring.len();
        let j = (i + 1) % self.ring.len();
        let f = pos - base;
        [
            self.ring[i][0] * (1.0 - f) + self.ring[j][0] * f,
            self.ring[i][1] * (1.0 - f) + self.ring[j][1] * f,
        ]
    }
    fn shift(&mut self, voice: usize, ratio: f32) -> [f32; 2] {
        let dry = self.ring[self.write];
        self.pitch[voice].next(dry, ratio)
    }
    pub fn observe_bypass(&mut self, r: &AudioFxParams, input: (f32, f32)) {
        if r.config.kind != K::Freeze {
            return;
        }
        if self.signature != r.signature || self.kind != Some(r.config.kind) {
            self.configure(r);
        }
        self.frozen = false;
        self.ring[self.write] = [finite(input.0), finite(input.1)];
        self.write = (self.write + 1) % self.ring.len();
        self.filled = (self.filled + 1).min(self.ring.len());
    }
    fn chorus_cuts(&mut self, p: &AudioFxConfig, mut wet: [f32; 2]) -> [f32; 2] {
        for (band, cutoff) in [p.chorus_low_cut_hz, p.chorus_high_cut_hz]
            .into_iter()
            .enumerate()
        {
            let target = if cutoff > 0.0 { 1.0 } else { 0.0 };
            if cutoff > 0.0 {
                self.chorus_cut_hz[band] = cutoff;
            }
            self.chorus_filter_mix[band] += (target - self.chorus_filter_mix[band]) * self.smooth;
            if target == 0.0 && self.chorus_filter_mix[band] < 1e-5 {
                if self.chorus_filter_mix[band] > 0.0 {
                    for ch in 0..2 {
                        self.chorus_filters[ch][band] = super::filter::FilterDspState::new();
                    }
                }
                self.chorus_filter_mix[band] = 0.0;
                continue;
            }
            for ch in 0..2 {
                let filtered = super::filter::process_sample(
                    &mut self.chorus_filters[ch][band],
                    super::filter::FilterParams {
                        filter_type: if band == 0 {
                            crate::config::filter_configs::FilterType::Hpf
                        } else {
                            crate::config::filter_configs::FilterType::Lpf
                        },
                        cutoff_hz: self.chorus_cut_hz[band],
                        q: 0.70710677,
                        drive: 0.0,
                        mix: 1.0,
                    },
                    self.sr,
                    wet[ch],
                );
                wet[ch] += (filtered - wet[ch]) * self.chorus_filter_mix[band];
            }
        }
        wet
    }
    pub fn process(
        &mut self,
        r: &AudioFxParams,
        bpm: usize,
        elapsed: f64,
        clock_active: bool,
        input: (f32, f32),
    ) -> (f32, f32) {
        self.process_automated(r, bpm, elapsed, clock_active, input, None)
    }
    pub fn process_automated(
        &mut self,
        r: &AudioFxParams,
        bpm: usize,
        elapsed: f64,
        clock_active: bool,
        input: (f32, f32),
        lane: Option<super::automation::Value>,
    ) -> (f32, f32) {
        use super::automation::Value;
        use crate::config::automation::Target;
        if self.signature != r.signature || self.kind != Some(r.config.kind) {
            self.configure(r);
        }
        let p = &r.config;
        let dry = [finite(input.0), finite(input.1)];
        self.level += (self.level_target - self.level) * self.smooth;
        let target_mix = if p.kind == K::Reverb {
            Value::get(lane, Target::ReverbWet, p.mix)
        } else {
            p.mix
        };
        self.mix += (target_mix - self.mix) * self.smooth;
        let freq = if p.sync_beats > 0.0 {
            bpm.max(1) as f32 / (60.0 * p.sync_beats)
        } else {
            p.rate_hz
        };
        // Sync uses the shared sample clock, so replay/offline and live agree.
        let phase = if p.sync_beats > 0.0 && clock_active {
            (elapsed * freq as f64).fract() as f32
        } else {
            self.phase as f32
        };
        self.phase = (self.phase + freq as f64 / self.sr as f64).fract();
        let shape_controls = matches!(p.kind, K::AutoPan | K::Tremolo);
        let phase = if shape_controls || matches!(p.kind, K::Phaser | K::Flanger) {
            self.modulation.phase(
                p,
                phase,
                freq,
                elapsed,
                clock_active,
                bpm,
                self.sr,
                shape_controls,
            )
        } else {
            phase
        };
        let lfo =
            super::audio_modulation::wave(phase, if shape_controls { p.mod_shape } else { 0.0 });
        let mut write = dry;
        let mut wet = dry;
        let mut mix_dry = dry;
        let voiced = if matches!(p.kind, K::Electric | K::Harmonist)
            || (p.preserve_formants && matches!(p.kind, K::Transpose | K::Octave))
        {
            self.tracker.next((dry[0] + dry[1]) * 0.5)
        } else {
            None
        };
        if p.preserve_formants {
            for voice in &mut self.pitch {
                voice.set_fundamental_hint(voiced.unwrap_or(0.0));
            }
        }
        // Write before reading pitch heads; current index remains the causal origin.
        if !self.frozen {
            self.ring[self.write] = dry;
        }
        match p.kind {
            K::Transpose => {
                if p.pitch_sequence && clock_active {
                    let beat = elapsed * bpm.max(1) as f64 / 60.0;
                    let step = (beat / p.pitch_step_beats as f64).floor() as usize
                        % p.pitch_step_count as usize;
                    if self.pitch_step != Some(step) {
                        self.target_ratio[0] = 2.0_f32.powf(p.pitch_steps[step] / 12.0);
                        self.pitch_step = Some(step);
                    }
                } else if self.pitch_step.take().is_some() {
                    self.target_ratio[0] = 2.0_f32.powf(p.semitones / 12.0);
                }
                self.ratio[0] += (self.target_ratio[0] - self.ratio[0]) * self.smooth;
                wet = self.shift(0, self.ratio[0]);
                mix_dry = self.read(self.pitch[0].latency_frames() as f32);
                if !self.pdc
                    && !p.pitch_sequence
                    && p.semitones == 0.0
                    && (!p.preserve_formants || p.formant_shift_semitones == 0.0)
                    && (self.ratio[0] - 1.0).abs() < 0.00001
                {
                    wet = dry;
                    mix_dry = dry;
                }
            }
            K::Electric | K::Harmonist => {
                if let Some(hz) = voiced {
                    let note = 69.0 + 12.0 * (hz / 440.0).log2();
                    let nearest = quantize(note, p.root, p.scale);
                    let stable = match self.last_note {
                        Some(old) if (note - old).abs() < 0.5 + p.stability * 0.45 => old,
                        _ => {
                            self.last_note = Some(nearest);
                            nearest
                        }
                    };
                    let target = if p.kind == K::Harmonist {
                        harmony(stable, p.root, p.scale, p.harmony_steps)
                    } else {
                        stable + p.semitones
                    };
                    self.target_ratio[0] = 2.0_f32.powf((target - note).clamp(-24.0, 24.0) / 12.0);
                }
                // Unvoiced consonants and silence pass through the Electric effect;
                // suppress harmony voices without a reliable monophonic pitch.
                self.ratio[0] += (self.target_ratio[0] - self.ratio[0]) * self.retune;
                let v = self.shift(0, self.ratio[0]);
                let aligned = self.read(self.pitch[0].latency_frames() as f32);
                mix_dry = aligned;
                self.gate += ((if voiced.is_some() { 1.0 } else { 0.0 }) - self.gate) * self.smooth;
                if p.kind == K::Electric {
                    wet = [
                        aligned[0] * (1.0 - self.gate) + v[0] * self.gate,
                        aligned[1] * (1.0 - self.gate) + v[1] * self.gate,
                    ];
                } else {
                    let v = pan_balance(v, p.pan);
                    wet = [
                        aligned[0] * p.direct + v[0] * p.voice * self.gate,
                        aligned[1] * p.direct + v[1] * p.voice * self.gate,
                    ];
                }
            }
            K::Octave => {
                let a = self.shift(0, 0.5);
                let b = self.shift(1, 0.25);
                let aligned = self.read(self.pitch[0].latency_frames() as f32);
                mix_dry = aligned;
                wet = [
                    aligned[0] * p.direct + a[0] * p.voice + b[0] * p.octave_two,
                    aligned[1] * p.direct + a[1] * p.voice + b[1] * p.octave_two,
                ];
            }
            K::Distortion => {
                if p.distortion_type != crate::config::audio_fx::DistortionType::Legacy {
                    wet = self.distortion.process(dry);
                } else {
                    for ch in 0..2 {
                        // Two oversampled shaping evaluations, plus DC rejection and
                        // post tone filtering. This reduces aliases but is not a brickwall oversampler.
                        let previous = self.drive_previous[ch];
                        self.drive_previous[ch] = dry[ch];
                        let shaped =
                            (shape((previous + dry[ch]) * 0.5 * self.drive, p.drive_style)
                                + shape(dry[ch] * self.drive, p.drive_style))
                                * 0.5;
                        self.lp[ch] += (shaped - self.lp[ch]) * self.tone_alpha;
                        let dc = self.lp[ch] - self.dc_x[ch] + self.dc_coefficient * self.dc_y[ch];
                        self.dc_x[ch] = self.lp[ch];
                        self.dc_y[ch] = dc;
                        wet[ch] = dc * 0.5;
                    }
                }
            }
            K::Dynamics | K::Sustainer => {
                wet = self.dynamics.process(&self.dynamics_params, self.sr, dry);
                if p.kind == K::Sustainer {
                    for (index, target) in [p.low_db, p.high_db].iter().enumerate() {
                        self.sustainer_tone[index] +=
                            (*target - self.sustainer_tone[index]) * self.smooth;
                        let band = if index == 0 { 0 } else { 3 };
                        if self.control_tick == 0 {
                            self.coeff[band] = biquad::shelf(
                                self.sr,
                                if index == 0 { 120.0 } else { 6000.0 },
                                self.sustainer_tone[index],
                                index == 1,
                            );
                        }
                        for ch in 0..2 {
                            if self.sustainer_tone[index].abs() > 1e-5 {
                                wet[ch] = self.eq[ch][band].next(wet[ch], self.coeff[band]);
                            } else {
                                self.eq[ch][band] = Biquad::default();
                            }
                        }
                    }
                }
            }
            K::Equalizer => {
                for ch in 0..2 {
                    for band in 0..4 {
                        wet[ch] = self.eq[ch][band].next(wet[ch], self.coeff[band]);
                    }
                }
            }
            K::AutoPan => wet = pan_balance(dry, lfo * p.depth),
            K::Pan => wet = pan_balance(dry, p.pan),
            K::StereoEnhance => {
                wet = self.enhance.next(dry, p.width);
            }
            K::Tremolo => {
                let gain = 1.0 - p.depth * (0.5 + 0.5 * lfo);
                wet = [dry[0] * gain, dry[1] * gain];
            }
            K::PanningDelay | K::Delay => {
                let time_ms = if p.sync_beats > 0.0 {
                    60000.0 / bpm.max(1) as f32 * p.sync_beats
                } else {
                    p.time_ms
                };
                let params = super::delay::DelayParams {
                    time_ms: Value::get(lane, Target::DelayTime, time_ms),
                    feedback: Value::get(lane, Target::DelayFeedback, self.delay_feedback),
                    high_damp_hz: p.high_cut_hz,
                    low_cut_hz: p.low_cut_hz,
                    direct: p.direct,
                    effect: Value::get(lane, Target::DelayWet, p.effect_level),
                };
                let y = if p.kind == K::PanningDelay {
                    super::delay::process_panning_sample(
                        &mut self.delay,
                        params,
                        self.sr,
                        dry[0],
                        dry[1],
                        p.delay_ratio,
                        p.width,
                    )
                } else {
                    super::delay::process_sample(&mut self.delay, params, self.sr, dry[0], dry[1])
                };
                wet = [y.0, y.1];
            }
            K::Phaser => {
                self.phaser_shift += (self.phaser_shift_target - self.phaser_shift) * self.smooth;
                if self.phaser_fade_left == 0 && self.phaser_stages != self.phaser_target_stages {
                    self.phaser_old = self.phaser;
                    self.phaser_old_feedback = self.phaser_feedback;
                    self.phaser_old_stages = self.phaser_stages;
                    self.phaser = [[0.0; 12]; 2];
                    self.phaser_feedback = [0.0; 2];
                    self.phaser_stages = self.phaser_target_stages;
                    self.phaser_fade_left = (self.sr * 0.005).round().max(1.0) as usize;
                }
                if self.control_tick == 0 {
                    let hz =
                        (200.0 * 20.0_f32.powf((0.5 + 0.5 * lfo) * p.depth) * self.phaser_shift)
                            .max(20.0);
                    let t = (PI * hz.min(self.sr * 0.2) / self.sr).tan();
                    self.phaser_a = (1.0 - t) / (1.0 + t);
                }
                let fade_length = (self.sr * 0.005).round().max(1.0);
                for ch in 0..2 {
                    let run = |states: &mut [f32; 12], feedback: &mut f32, stages: usize| {
                        let mut x = dry[ch] + *feedback * p.feedback.min(0.85);
                        for state in &mut states[..stages] {
                            let out = -self.phaser_a * x + *state;
                            *state = x + self.phaser_a * out;
                            x = out;
                        }
                        *feedback = x.tanh();
                        x
                    };
                    let current = run(
                        &mut self.phaser[ch],
                        &mut self.phaser_feedback[ch],
                        self.phaser_stages,
                    );
                    wet[ch] = if self.phaser_fade_left > 0 {
                        let previous = run(
                            &mut self.phaser_old[ch],
                            &mut self.phaser_old_feedback[ch],
                            self.phaser_old_stages,
                        );
                        let amount = 1.0 - self.phaser_fade_left as f32 / fade_length;
                        previous + (current - previous) * amount
                    } else {
                        current
                    };
                }
                self.phaser_fade_left = self.phaser_fade_left.saturating_sub(1);
            }
            K::Flanger | K::Chorus | K::Vibrato => {
                self.flanger_shift +=
                    (self.flanger_shift_target - self.flanger_shift) * self.smooth;
                let (base, span) = match p.kind {
                    K::Flanger => (0.8, 3.5),
                    K::Chorus => (15.0, 7.0),
                    _ => (7.0, 5.0),
                };
                for ch in 0..2 {
                    let motion = if ch == 0 {
                        lfo
                    } else {
                        (TAU * (phase
                            + if p.kind == K::Vibrato {
                                0.0
                            } else {
                                0.25 * p.width
                            }))
                        .sin()
                    };
                    let mut delay = (base + span * (1.0 + motion * p.depth)) * self.sr * 0.001;
                    if p.kind == K::Flanger {
                        delay *= self.flanger_shift;
                    }
                    wet[ch] = self.read(delay.max(1.0))[ch];
                    if p.kind == K::Flanger {
                        write[ch] = (dry[ch] + wet[ch] * p.feedback.min(0.85)).tanh();
                    }
                }
                if p.kind == K::Chorus {
                    wet = self.chorus_cuts(p, wet);
                }
            }
            K::StepSlicer => {
                let index = (phase * p.step_count as f32) as usize % p.step_count as usize;
                let in_gate = (phase * p.step_count as f32).fract() < p.slicer_duty;
                let target = 1.0 - p.depth + p.depth * if in_gate { p.steps[index] } else { 0.0 };
                self.gate += (target - self.gate) * self.smooth;
                let target_compress = if p.slicer_compress { 1.0 } else { 0.0 };
                self.slicer_compress_mix +=
                    (target_compress - self.slicer_compress_mix) * self.smooth;
                let mut shaped = dry;
                if p.slicer_compress || self.slicer_compress_mix > 1e-5 {
                    let compressed = self.dynamics.process(&self.dynamics_params, self.sr, dry);
                    shaped = std::array::from_fn(|ch| {
                        dry[ch] + (compressed[ch] - dry[ch]) * self.slicer_compress_mix
                    });
                } else {
                    self.dynamics.reset();
                }
                wet = [shaped[0] * self.gate, shaped[1] * self.gate];
            }
            K::Freeze => {
                if p.freeze && !self.frozen && self.filled > self.sr as usize / 10 {
                    self.frozen = true;
                    self.freeze_end = self.write;
                    self.freeze_length = (self.sr * 0.09) as usize;
                    self.freeze_phase = [0.0, 0.5];
                    self.freeze_level = 1.0;
                }
                if self.frozen {
                    let length = self.freeze_length.max(2);
                    let len = self.ring.len();
                    let start = (self.freeze_end + len - length) % len;
                    wet = [0.0; 2];
                    for voice in 0..2 {
                        let ph = self.freeze_phase[voice];
                        let offset = ph * length as f32;
                        let base = offset as usize;
                        let i = (start + base) % len;
                        let j = (i + 1) % len;
                        let frac = offset - base as f32;
                        let weight = 0.5 - 0.5 * (TAU * ph).cos();
                        for ch in 0..2 {
                            wet[ch] += (self.ring[i][ch] * (1.0 - frac) + self.ring[j][ch] * frac)
                                * weight;
                        }
                        self.freeze_phase[voice] = (ph + 1.0 / length as f32).fract();
                    }
                    self.freeze_mix += ((if p.freeze { 1.0 } else { 0.0 }) - self.freeze_mix)
                        * if p.freeze {
                            self.freeze_attack
                        } else {
                            self.freeze_release
                        };
                    self.freeze_level += (p.depth - self.freeze_level) * self.freeze_decay;
                    for ch in 0..2 {
                        wet[ch] = dry[ch] * (1.0 - self.freeze_mix)
                            + wet[ch] * self.freeze_level * self.freeze_mix;
                    }
                    if !p.freeze && self.freeze_mix < 1e-4 {
                        self.frozen = false;
                        self.filled = 0;
                        self.freeze_mix = 0.0;
                    }
                }
            }
            K::Reverb => {
                let y = super::reverb::process_sample(
                    &mut self.reverb,
                    ReverbParams {
                        dry_level: 0.0,
                        wet_level: 1.0,
                        density: p.density as f32,
                        size_ms: 20.0 + 100.0 * p.depth,
                        rt60_ms: Value::get(lane, Target::ReverbAudioDecay, p.decay_ms),
                        predelay_ms: p.predelay_ms,
                        width: p.width.min(1.0),
                        high_cut_hz: p.high_cut_hz,
                        low_cut_hz: p.low_cut_hz,
                    },
                    self.sr,
                    dry[0],
                    dry[1],
                );
                wet = [y.0, y.1];
            }
        }
        if !self.frozen {
            self.ring[self.write] = write;
            self.write = (self.write + 1) % self.ring.len();
            self.filled = (self.filled + 1).min(self.ring.len());
        }
        self.control_tick = (self.control_tick + 1) % 16;
        self.aligned_dry = mix_dry;
        let mix = if p.kind == K::Distortion
            && p.distortion_type != crate::config::audio_fx::DistortionType::Legacy
        {
            1.0
        } else {
            self.mix
        };
        let out = [
            (mix_dry[0] * (1.0 - mix) + wet[0] * mix) * self.level,
            (mix_dry[1] * (1.0 - mix) + wet[1] * mix) * self.level,
        ];
        (finite(out[0]), finite(out[1]))
    }
}
fn finite(x: f32) -> f32 {
    if x.is_finite() {
        x.clamp(-16.0, 16.0)
    } else {
        0.0
    }
}
fn shape(x: f32, style: DriveStyle) -> f32 {
    match style {
        DriveStyle::Soft => x.tanh(),
        DriveStyle::Hard => x.clamp(-1.0, 1.0),
        DriveStyle::Fuzz => {
            let z = (x.abs() * 2.0).tanh();
            x.signum() * z * z
        }
    }
}
fn pan_balance(x: [f32; 2], pan: f32) -> [f32; 2] {
    // Stereo balance: center is identity, extremes silence the opposite side.
    [
        x[0] * (pan.max(0.0) * PI * 0.5).cos(),
        x[1] * ((-pan).max(0.0) * PI * 0.5).cos(),
    ]
}
pub fn quantize(note: f32, root: u8, scale: Scale) -> f32 {
    if scale == Scale::Chromatic {
        return note.round();
    }
    let scale = if scale == Scale::Major {
        &[0, 2, 4, 5, 7, 9, 11][..]
    } else {
        &[0, 2, 3, 5, 7, 8, 10][..]
    };
    let n = note.round() as i32;
    (n - 2..=n + 2)
        .filter(|v| scale.contains(&(v - i32::from(root)).rem_euclid(12)))
        .min_by(|a, b| ((*a as f32 - note).abs()).total_cmp(&(*b as f32 - note).abs()))
        .unwrap_or(n) as f32
}
pub fn harmony(note: f32, root: u8, scale: Scale, steps: i8) -> f32 {
    let scale = if scale == Scale::Minor {
        &[0, 2, 3, 5, 7, 8, 10][..]
    } else {
        &[0, 2, 4, 5, 7, 9, 11][..]
    };
    let note = quantize(
        note,
        root,
        if scale[2] == 3 {
            Scale::Minor
        } else {
            Scale::Major
        },
    ) as i32;
    let relative = note - i32::from(root);
    let degree = scale
        .iter()
        .position(|v| *v == relative.rem_euclid(12))
        .unwrap_or(0) as i32;
    let target = degree + i32::from(steps);
    (i32::from(root)
        + (relative.div_euclid(12) + target.div_euclid(7)) * 12
        + scale[target.rem_euclid(7) as usize]) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diatonic_harmonies_follow_key_and_scale() {
        assert_eq!(harmony(60.0, 0, Scale::Major, 2), 64.0);
        assert_eq!(harmony(62.0, 0, Scale::Major, 2), 65.0);
        assert_eq!(harmony(60.0, 0, Scale::Minor, 2), 63.0);
        assert_eq!(harmony(60.0, 0, Scale::Major, -2), 57.0);
        assert_eq!(harmony(60.0, 0, Scale::Major, 7), 72.0);
    }
    #[test]
    fn every_effect_is_finite_and_allocation_free_under_automation() {
        let sr = 8000.0;
        let mut state = AudioFxState::new(sr);
        for kind in K::ALL {
            let mut config = AudioFxConfig::new(kind);
            config.semitones = 7.0;
            let p = AudioFxParams::new(&config);
            let allocations = crate::test_alloc::count(|| {
                for n in 0..8000 {
                    let x = (TAU * 220.0 * n as f32 / sr).sin() * 0.2;
                    let y = state.process(&p, 120, n as f64 / sr as f64, true, (x, x * 0.7));
                    assert!(y.0.is_finite() && y.1.is_finite(), "{kind:?}");
                }
            });
            assert_eq!(allocations, 0, "{kind:?}");
        }
    }
    #[test]
    fn transpose_moves_frequency_without_changing_duration() {
        let sr = 48000.0;
        let mut state = AudioFxState::new(sr);
        let mut config = AudioFxConfig::new(K::Transpose);
        config.semitones = 12.0;
        let p = AudioFxParams::new(&config);
        let mut previous = 0.0;
        let mut crossings = 0;
        for n in 0..96000 {
            let x = (TAU * 220.0 * n as f32 / sr).sin() * 0.2;
            let y = state.process(&p, 120, n as f64 / sr as f64, true, (x, x)).0;
            if n >= 48000 && previous < 0.0 && y >= 0.0 {
                crossings += 1;
            }
            previous = y;
        }
        assert!((crossings - 440i32).abs() < 4, "Detected {crossings} Hz");
    }
    #[test]
    fn compressor_reduces_high_levels_and_links_stereo() {
        let mut state = AudioFxState::new(8000.0);
        let mut c = AudioFxConfig::new(K::Dynamics);
        c.threshold_db = -20.0;
        c.ratio = 10.0;
        c.attack_ms = 0.1;
        let p = AudioFxParams::new(&c);
        let mut y = (0.0, 0.0);
        for n in 0..8000 {
            y = state.process(&p, 120, n as f64 / 8000.0, true, (0.8, 0.4));
        }
        assert!(y.0 < 0.15 && y.0 > 0.1);
        assert!((y.0 / y.1 - 2.0).abs() < 1e-5);
    }
}

#[cfg(test)]
mod extension_tests {
    use super::*;
    #[test]
    fn neutral_fourth_eq_band_matches_the_previous_three_band_chain() {
        let p = AudioFxConfig {
            kind: K::Equalizer,
            low_db: 2.0,
            mid_db: -4.0,
            mid_hz: 800.0,
            high_db: 3.0,
            ..Default::default()
        };
        let runtime = AudioFxParams::new(&p);
        let mut state = AudioFxState::new(48000.0);
        let coefficients = [
            biquad::shelf(48000.0, p.low_hz, p.low_db, false),
            biquad::peak(48000.0, p.mid_hz, p.mid_q, p.mid_db),
            biquad::shelf(48000.0, p.high_hz, p.high_db, true),
        ];
        let mut old = [Biquad::default(); 3];
        for n in 0..48000 {
            let x = (n as f32 * 0.13).sin() * 0.05;
            let mut reference = x;
            for i in 0..3 {
                reference = old[i].next(reference, coefficients[i]);
            }
            let y = state.process(&runtime, 120, n as f64 / 48000.0, true, (x, 0.0));
            assert!((y.0 - reference).abs() < 1e-6);
            assert_eq!(y.1, 0.0);
        }
    }
    #[test]
    fn separate_mid_bands_have_independent_frequency_response() {
        let p = AudioFxConfig {
            kind: K::Equalizer,
            mid_db: -6.0,
            mid_hz: 300.0,
            mid_q: 2.0,
            high_mid_db: 9.0,
            high_mid_hz: 3000.0,
            high_mid_q: 2.0,
            ..Default::default()
        };
        for (hz, expected) in [(300.0f32, -6.0), (3000.0, 9.0)] {
            let mut state = AudioFxState::new(48000.0);
            let runtime = AudioFxParams::new(&p);
            let (mut input, mut output) = (0.0f64, 0.0f64);
            for n in 0..48000 {
                let x = (TAU * hz * n as f32 / 48000.0).sin() * 0.005;
                let y = state.process(&runtime, 120, n as f64 / 48000.0, true, (x, x));
                if n > 4000 {
                    input += (x as f64).powi(2);
                    output += (y.0 as f64).powi(2);
                }
            }
            let gain = 10.0 * (output / input).log10();
            assert!(
                (gain - expected).abs() < 0.25,
                "{hz} Hz: {gain} dB vs{expected}"
            );
        }
    }
    #[test]
    fn chorus_cuts_only_the_wet_signal_and_flat_is_neutral() {
        let base = AudioFxConfig {
            kind: K::Chorus,
            depth: 0.0,
            mix: 1.0,
            ..Default::default()
        };
        for (hz, low, high) in [(2000.0, 0.0, 500.0), (100.0, 1000.0, 0.0)] {
            let mut dry = AudioFxState::new(48000.0);
            let mut filtered = AudioFxState::new(48000.0);
            let mut bypass = AudioFxState::new(48000.0);
            let p = AudioFxParams::new(&base);
            let filtered_p = AudioFxParams::new(&AudioFxConfig {
                chorus_low_cut_hz: low,
                chorus_high_cut_hz: high,
                ..base
            });
            let bypass_p = AudioFxParams::new(&AudioFxConfig {
                mix: 0.0,
                ..filtered_p.config
            });
            let (mut normal_power, mut filtered_power) = (0.0f64, 0.0f64);
            for n in 0..24000 {
                let x = (TAU * hz * n as f32 / 48000.0).sin() * 0.1;
                let t = n as f64 / 48000.0;
                let a = dry.process(&p, 120, t, true, (x, -x));
                let b = filtered.process(&filtered_p, 120, t, true, (x, -x));
                let clean = bypass.process(&bypass_p, 120, t, true, (x, -x));
                assert_eq!(clean, (x, -x));
                if n > 4000 {
                    normal_power += (a.0 as f64).powi(2);
                    filtered_power += (b.0 as f64).powi(2);
                    assert!((b.0 + b.1).abs() < 1e-6);
                }
            }
            assert!(
                filtered_power / normal_power < 0.01,
                "Wet filter did not reject {hz} Hz"
            );
        }
    }
    #[test]
    fn new_control_defaults_preserve_legacy_json_and_ranges_are_bounded() {
        let p: AudioFxConfig =
            serde_json::from_value(serde_json::json!({"kind":"Phaser"})).unwrap();
        assert_eq!(p.phaser_stages, 6);
        assert_eq!(p.phaser_manual, 0.5);
        assert_eq!(p.flanger_manual, 0.5);
        assert_eq!(p.high_mid_db, 0.0);
        assert_eq!(p.chorus_low_cut_hz, 0.0);
        assert_eq!(p.chorus_high_cut_hz, 0.0);
        assert_eq!(AudioFxConfig::new(K::Phaser).phaser_stages, 4);
        let p = AudioFxConfig {
            phaser_stages: 255,
            phaser_manual: f32::NAN,
            flanger_manual: 10.0,
            high_mid_q: 99.0,
            chorus_low_cut_hz: 99999.0,
            ..p
        }
        .sanitized();
        assert_eq!(p.phaser_stages, 6);
        assert_eq!(p.phaser_manual, 0.5);
        assert_eq!(p.flanger_manual, 1.0);
        assert_eq!(p.high_mid_q, 16.0);
        assert_eq!(p.chorus_low_cut_hz, 12500.0);
    }
    #[test]
    fn modulation_and_tone_changes_are_bounded_recover_and_do_not_allocate() {
        for kind in [K::Phaser, K::Flanger, K::Chorus, K::Equalizer] {
            let mut state = AudioFxState::new(48000.0);
            let variants: Vec<_> = (0..16)
                .map(|i| {
                    AudioFxParams::new(&AudioFxConfig {
                        kind,
                        phaser_stages: [4, 8, 12, 6][i % 4],
                        phaser_manual: (i % 3) as f32 * 0.5,
                        flanger_manual: (i % 3) as f32 * 0.5,
                        feedback: 0.8,
                        chorus_low_cut_hz: if i % 2 == 0 { 20.0 } else { 3000.0 },
                        chorus_high_cut_hz: if i % 3 == 0 { 500.0 } else { 0.0 },
                        high_mid_hz: if i % 2 == 0 { 200.0 } else { 12000.0 },
                        high_mid_db: if i % 3 == 0 { 12.0 } else { -12.0 },
                        high_mid_q: 16.0,
                        mix: 0.6,
                        ..Default::default()
                    })
                })
                .collect();
            let count = crate::test_alloc::count(|| {
                for n in 0..48000 {
                    let p = &variants[(n / 64) % variants.len()];
                    let x = (TAU * 175.0 * n as f32 / 48000.0).sin() * 0.1;
                    let y = state.process(p, 120, n as f64 / 48000.0, true, (x, -x));
                    assert!(
                        y.0.is_finite()
                            && y.1.is_finite()
                            && y.0.abs() <= 16.0
                            && y.1.abs() <= 16.0
                    );
                }
            });
            assert_eq!(count, 0, "{kind:?}");
            let neutral = AudioFxParams::new(&AudioFxConfig::new(kind));
            let mut power = 0.0;
            for n in 0..48000 {
                let x = (TAU * 400.0 * n as f32 / 48000.0).sin() * 0.1;
                let y = state.process(&neutral, 120, n as f64 / 48000.0, true, (x, x));
                if n > 24000 {
                    power += y.0 * y.0;
                }
            }
            assert!(
                power > 1.0 && power.is_finite(),
                "{kind:?} stayed silent/unstable after automation: {power}"
            );
        }
    }
}

#[cfg(test)]
mod final_control_tests {
    use super::*;
    #[test]
    fn old_json_keeps_neutral_new_controls_and_bounds_are_explicit() {
        let p: AudioFxConfig = serde_json::from_str(r#"{"kind":"AutoPan"}"#).unwrap();
        assert!(!p.mod_retrigger && !p.mod_stepped && !p.slicer_compress);
        assert_eq!(
            (p.mod_shape, p.mod_phase_degrees, p.slicer_duty),
            (0.0, 0.0, 1.0)
        );
        assert!(AudioFxConfig::new(K::AutoPan).mod_retrigger);
        let p = AudioFxConfig {
            kind: K::Sustainer,
            mod_shape: f32::NAN,
            mod_phase_degrees: 999.0,
            mod_step_hz: f32::INFINITY,
            mod_step_beats: 0.0001,
            slicer_duty: -4.0,
            low_db: -99.0,
            high_db: 99.0,
            ..Default::default()
        }
        .sanitized();
        assert_eq!(
            (
                p.mod_shape,
                p.mod_phase_degrees,
                p.mod_step_hz,
                p.mod_step_beats,
                p.slicer_duty,
                p.low_db,
                p.high_db
            ),
            (0.0, 180.0, 4.0, 0.015625, 0.01, -20.0, 20.0)
        );
    }
    #[test]
    fn neutral_sustainer_tone_is_exact_previous_dynamics_output() {
        let r = AudioFxParams::new(&AudioFxConfig::new(K::Sustainer));
        let mut s = AudioFxState::new(48000.0);
        let mut reference = super::super::dynamics::DynamicsState::default();
        let mut prepared = r;
        prepared.config.mix = 1.0;
        prepared.config.level_db = 0.0;
        for n in 0..12000 {
            let x = (TAU * 220.0 * n as f32 / 48000.0).sin() * 0.1;
            let y = s.process(&r, 120, n as f64 / 48000.0, true, (x, -x));
            let e = reference.process(&prepared, 48000.0, [x, -x]);
            assert_eq!(
                (y.0.to_bits(), y.1.to_bits()),
                (e[0].to_bits(), e[1].to_bits())
            );
        }
    }
    #[test]
    fn sustainer_shelves_change_bass_and_treble_independently() {
        for (hz, low, high) in [(40.0, 12.0, 0.0), (14000.0, 0.0, -12.0)] {
            let p = AudioFxConfig {
                kind: K::Sustainer,
                ratio: 1.0,
                makeup_db: 0.0,
                low_db: low,
                high_db: high,
                ..Default::default()
            };
            let r = AudioFxParams::new(&p);
            let mut s = AudioFxState::new(48000.0);
            let mut input = 0.0;
            let mut output = 0.0;
            for n in 0..48000 {
                let x = (TAU * hz * n as f32 / 48000.0).sin() * 0.03;
                let y = s.process(&r, 120, n as f64 / 48000.0, true, (x, x)).0;
                if n > 24000 {
                    input += x * x;
                    output += y * y;
                }
            }
            let db = 10.0 * (output / input).log10();
            assert!((db - (low + high)).abs() < 0.5, "{hz} {db}");
        }
    }
    #[test]
    fn neutral_slicer_matches_old_gate_and_duty_produces_real_silence() {
        let mut p = AudioFxConfig::new(K::StepSlicer);
        p.sync_beats = 0.0;
        p.rate_hz = 2.0;
        let r = AudioFxParams::new(&p);
        let mut s = AudioFxState::new(48000.0);
        let mut phase = 0.0f64;
        let mut gate = 0.0;
        let smooth = 1.0 - (-1.0 / (48000.0f32 * 0.005)).exp();
        for n in 0..12000 {
            let index = (phase as f32 * p.step_count as f32) as usize % p.step_count as usize;
            let target = 1.0 - p.depth + p.depth * p.steps[index];
            gate += (target - gate) * smooth;
            phase = (phase + p.rate_hz as f64 / 48000.0).fract();
            let y = s.process(&r, 120, n as f64 / 48000.0, false, (0.25, -0.25));
            assert_eq!(y, (0.25 * gate, -0.25 * gate));
        }
        p.step_count = 4;
        p.steps = [1.0; 16];
        p.slicer_duty = 0.25;
        p.depth = 1.0;
        p.rate_hz = 1.0;
        let r = AudioFxParams::new(&p);
        let mut s = AudioFxState::new(48000.0);
        let mut peak = 0.0f32;
        for n in 0..12000 {
            let y = s
                .process(&r, 120, n as f64 / 48000.0, false, (0.25, 0.25))
                .0;
            if (500..2000).contains(&n) {
                peak = peak.max(y);
            }
            if n > 8000 {
                assert!(y.abs() < 1e-5);
            }
        }
        assert!(peak > 0.24);
    }
    #[test]
    fn optional_slicer_compression_reduces_input_before_the_gate() {
        let p = AudioFxConfig {
            kind: K::StepSlicer,
            depth: 0.0,
            slicer_compress: true,
            threshold_db: -24.0,
            makeup_db: 0.0,
            ..Default::default()
        };
        let r = AudioFxParams::new(&p);
        let mut s = AudioFxState::new(48000.0);
        let mut last = 0.0;
        for n in 0..24000 {
            last = s.process(&r, 120, n as f64 / 48000.0, false, (0.5, -0.5)).0;
        }
        assert!(last > 0.07 && last < 0.14, "{last}");
    }
    #[test]
    fn all_added_controls_can_be_automated_without_allocating_or_unbounded_audio() {
        for sr in [8000.0, 48000.0, 192000.0] {
            for kind in [
                K::AutoPan,
                K::Tremolo,
                K::Phaser,
                K::Flanger,
                K::Sustainer,
                K::StepSlicer,
            ] {
                let variants: Vec<_> = (0..8)
                    .map(|i| {
                        let mut p = AudioFxConfig::new(kind);
                        p.mod_shape = i as f32 / 7.0;
                        p.mod_phase_degrees = i as f32 * 25.0;
                        p.mod_retrigger = i % 2 == 0;
                        p.mod_stepped = i % 3 != 0;
                        p.mod_step_hz = 0.1 + i as f32 * 13.0;
                        p.mod_step_beats = if i % 2 == 0 { 0.0 } else { 0.0625 };
                        p.slicer_duty = 0.01 + i as f32 * 0.14;
                        p.slicer_compress = i % 2 == 0;
                        p.low_db = i as f32 * 5.0 - 17.5;
                        p.high_db = -p.low_db;
                        p.feedback = 0.85;
                        AudioFxParams::new(&p)
                    })
                    .collect();
                let mut s = AudioFxState::new(sr);
                let allocations = crate::test_alloc::count(|| {
                    for n in 0..12000 {
                        let x = (n as f32 * 0.037).sin() * 0.03;
                        let y = s.process(
                            &variants[(n / 31) % 8],
                            137,
                            n as f64 / sr as f64,
                            n % 3000 > 300,
                            (x, -x),
                        );
                        assert!(
                            y.0.is_finite()
                                && y.1.is_finite()
                                && y.0.abs() <= 16.0
                                && y.1.abs() <= 16.0
                        );
                    }
                });
                assert_eq!(allocations, 0);
            }
        }
    }
}
