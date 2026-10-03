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
    pitch: [super::pitch_shift::PitchShift; 2],
    stagger: usize,
    delay: super::delay::DelayDspState,
    delay_feedback: f32,
    dynamics: super::dynamics::DynamicsState,
    dynamics_params: AudioFxParams,
    sr: f32,
    ring: Vec<[f32; 2]>,
    write: usize,
    filled: usize,
    signature: u64,
    kind: Option<K>,
    phase: f64,
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
    eq: [[Biquad; 3]; 2],
    coeff: [Coeff; 3],
    phaser: [[f32; 6]; 2],
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
            stagger,
            pitch: std::array::from_fn(|voice| {
                super::pitch_shift::PitchShift::new_with_offset(
                    pitch_plan.clone(),
                    (stagger * 2 + voice) * hop / 48,
                )
            }),
            delay: super::delay::DelayDspState::new(sr),
            delay_feedback: 0.35,
            dynamics: super::dynamics::DynamicsState::default(),
            dynamics_params: AudioFxParams::new(&AudioFxConfig::new(K::Dynamics)),
            sr,
            ring: vec![[0.0; 2]; (sr * 0.12).ceil() as usize + 4],
            write: 0,
            filled: 0,
            signature: 0,
            kind: None,
            phase: 0.0,
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
            eq: [[Biquad::default(); 3]; 2],
            coeff: [Coeff::default(); 3],
            phaser: [[0.0; 6]; 2],
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
    pub fn reset(&mut self) {
        for pitch in &mut self.pitch {
            pitch.reset();
        }
        self.delay.reset();
        self.dynamics.reset();
        // Logical clearing: reads are gated by `filled`, never scan the two-second ring in a callback.
        self.write = 0;
        self.filled = 0;
        self.phase = 0.0;
        self.pitch_step = None;
        self.ratio = [1.0; 2];
        self.target_ratio = [1.0; 2];
        self.lp = [0.0; 2];
        self.dc_x = [0.0; 2];
        self.dc_y = [0.0; 2];
        self.drive_previous = [0.0; 2];
        self.gate = 0.0;
        self.eq = [[Biquad::default(); 3]; 2];
        self.phaser = [[0.0; 6]; 2];
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
            self.mix = p.mix;
            self.ratio = [2.0_f32.powf(p.semitones / 12.0), 0.25];
        }
        self.signature = r.signature;
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
            biquad::shelf(self.sr, p.low_hz, p.low_db, false),
            biquad::peak(self.sr, p.mid_hz, p.mid_q, p.mid_db),
            biquad::shelf(self.sr, p.high_hz, p.high_db, true),
        ];
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
    pub fn process(
        &mut self,
        r: &AudioFxParams,
        bpm: usize,
        elapsed: f64,
        clock_active: bool,
        input: (f32, f32),
    ) -> (f32, f32) {
        if self.signature != r.signature || self.kind != Some(r.config.kind) {
            self.configure(r);
        }
        let p = &r.config;
        let dry = [finite(input.0), finite(input.1)];
        self.level += (self.level_target - self.level) * self.smooth;
        self.mix += (p.mix - self.mix) * self.smooth;
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
        let lfo = (TAU * phase).sin();
        let mut write = dry;
        let mut wet = dry;
        let mut mix_dry = dry;
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
                if !p.pitch_sequence && p.semitones == 0.0 && (self.ratio[0] - 1.0).abs() < 0.00001
                {
                    wet = dry;
                    mix_dry = dry;
                }
            }
            K::Electric | K::Harmonist => {
                let voiced = self.tracker.next((dry[0] + dry[1]) * 0.5);
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
                for ch in 0..2 {
                    // Two oversampled shaping evaluations, plus DC rejection and
                    // post tone filtering. This reduces aliases but is not a brickwall oversampler.
                    let previous = self.drive_previous[ch];
                    self.drive_previous[ch] = dry[ch];
                    let shaped = (shape((previous + dry[ch]) * 0.5 * self.drive, p.drive_style)
                        + shape(dry[ch] * self.drive, p.drive_style))
                        * 0.5;
                    self.lp[ch] += (shaped - self.lp[ch]) * self.tone_alpha;
                    let dc = self.lp[ch] - self.dc_x[ch] + self.dc_coefficient * self.dc_y[ch];
                    self.dc_x[ch] = self.lp[ch];
                    self.dc_y[ch] = dc;
                    wet[ch] = dc * 0.5;
                }
            }
            K::Dynamics | K::Sustainer => {
                wet = self.dynamics.process(&self.dynamics_params, self.sr, dry);
            }
            K::Equalizer => {
                for ch in 0..2 {
                    for band in 0..3 {
                        wet[ch] = self.eq[ch][band].next(wet[ch], self.coeff[band]);
                    }
                }
            }
            K::AutoPan => wet = pan_balance(dry, lfo * p.depth),
            K::Pan => wet = pan_balance(dry, p.pan),
            K::StereoEnhance => {
                let mid = (dry[0] + dry[1]) * 0.5;
                let side = (dry[0] - dry[1]) * 0.5 * p.width;
                wet = [mid + side, mid - side];
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
                    time_ms,
                    feedback: self.delay_feedback,
                    high_damp_hz: p.high_cut_hz,
                    low_cut_hz: p.low_cut_hz,
                    direct: p.direct,
                    effect: p.effect_level,
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
                if self.control_tick == 0 {
                    let hz = 200.0 * 20.0_f32.powf((0.5 + 0.5 * lfo) * p.depth);
                    let t = (PI * hz.min(self.sr * 0.2) / self.sr).tan();
                    self.phaser_a = (1.0 - t) / (1.0 + t);
                }
                for ch in 0..2 {
                    let mut x = dry[ch] + self.phaser_feedback[ch] * p.feedback.min(0.85);
                    for stage in 0..6 {
                        let out = -self.phaser_a * x + self.phaser[ch][stage];
                        self.phaser[ch][stage] = x + self.phaser_a * out;
                        x = out;
                    }
                    self.phaser_feedback[ch] = x.tanh();
                    wet[ch] = x;
                }
            }
            K::Flanger | K::Chorus | K::Vibrato => {
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
                    let delay = (base + span * (1.0 + motion * p.depth)) * self.sr * 0.001;
                    wet[ch] = self.read(delay.max(1.0))[ch];
                    if p.kind == K::Flanger {
                        write[ch] = (dry[ch] + wet[ch] * p.feedback.min(0.85)).tanh();
                    }
                }
            }
            K::StepSlicer => {
                let index = (phase * p.step_count as f32) as usize % p.step_count as usize;
                let target = 1.0 - p.depth + p.depth * p.steps[index];
                self.gate += (target - self.gate) * self.smooth;
                wet = [dry[0] * self.gate, dry[1] * self.gate];
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
                        rt60_ms: p.decay_ms,
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
        let out = [
            (mix_dry[0] * (1.0 - self.mix) + wet[0] * self.mix) * self.level,
            (mix_dry[1] * (1.0 - self.mix) + wet[1] * self.mix) * self.level,
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
