//! Audio processors shared by the input and track racks. Units are explicit;
//! serialized values are sanitized before crossing the audio-thread boundary.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioFxKind {
    Transpose,
    Electric,
    Harmonist,
    Distortion,
    Dynamics,
    Equalizer,
    Octave,
    AutoPan,
    PanningDelay,
    Phaser,
    Flanger,
    Sustainer,
    Pan,
    StereoEnhance,
    Tremolo,
    Vibrato,
    StepSlicer,
    Freeze,
    Chorus,
    Reverb,
    Delay,
}
impl AudioFxKind {
    pub const ALL: [Self; 21] = [
        Self::Transpose,
        Self::Electric,
        Self::Harmonist,
        Self::Distortion,
        Self::Dynamics,
        Self::Equalizer,
        Self::Octave,
        Self::AutoPan,
        Self::PanningDelay,
        Self::Phaser,
        Self::Flanger,
        Self::Sustainer,
        Self::Pan,
        Self::StereoEnhance,
        Self::Tremolo,
        Self::Vibrato,
        Self::StepSlicer,
        Self::Freeze,
        Self::Chorus,
        Self::Reverb,
        Self::Delay,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Transpose => "Transpose",
            Self::Electric => "Electric",
            Self::Harmonist => "Harmonist",
            Self::Distortion => "Distortion",
            Self::Dynamics => "Dynamics",
            Self::Equalizer => "Equalizer",
            Self::Octave => "Octave",
            Self::AutoPan => "Auto Pan",
            Self::PanningDelay => "Panning Delay",
            Self::Phaser => "Phaser",
            Self::Flanger => "Flanger",
            Self::Sustainer => "Sustainer",
            Self::Pan => "Manual Pan",
            Self::StereoEnhance => "Stereo Enhance",
            Self::Tremolo => "Tremolo",
            Self::Vibrato => "Vibrato",
            Self::StepSlicer => "Step Slicer",
            Self::Freeze => "Freeze",
            Self::Chorus => "Chorus",
            Self::Reverb => "Reverb",
            Self::Delay => "Delay",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Scale {
    #[default]
    Chromatic,
    Major,
    Minor,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DriveStyle {
    #[default]
    Soft,
    Hard,
    Fuzz,
}
/// These are independent software voicings, not measured BOSS circuit models.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DistortionType {
    #[default]
    Legacy,
    Vocal,
    Boost,
    Overdrive,
    Distortion,
    Metal,
    Fuzz,
}
impl DistortionType {
    pub const ALL: [Self; 7] = [
        Self::Legacy,
        Self::Vocal,
        Self::Boost,
        Self::Overdrive,
        Self::Distortion,
        Self::Metal,
        Self::Fuzz,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Legacy => "Legacy",
            Self::Vocal => "Vocal",
            Self::Boost => "Boost",
            Self::Overdrive => "OD",
            Self::Distortion => "DS",
            Self::Metal => "Metal",
            Self::Fuzz => "Fuzz",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DynamicsMode {
    #[default]
    Compressor,
    Limiter,
    Gate,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioFxConfig {
    pub kind: AudioFxKind,
    pub mix: f32,
    pub level_db: f32,
    pub semitones: f32,
    pub preserve_formants: bool,
    pub formant_shift_semitones: f32,
    pub pitch_steps: [f32; 16],
    pub pitch_step_count: u8,
    pub pitch_sequence: bool,
    pub pitch_step_beats: f32,
    pub root: u8,
    pub scale: Scale,
    /// Diatonic steps, e.g. +2 means a third. ±7 are octaves.
    pub harmony_steps: i8,
    pub direct: f32,
    pub voice: f32,
    pub octave_two: f32,
    pub retune_ms: f32,
    pub stability: f32,
    pub drive_db: f32,
    pub drive_style: DriveStyle,
    pub distortion_type: DistortionType,
    pub distortion_tone: f32,
    pub distortion_direct: f32,
    pub distortion_effect: f32,
    pub tone_hz: f32,
    pub dynamics_mode: DynamicsMode,
    pub dynamics_profile: super::dynamics_profiles::DynamicsProfile,
    pub dynamics_amount: f32,
    pub threshold_db: f32,
    pub ratio: f32,
    pub knee_db: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub makeup_db: f32,
    pub low_db: f32,
    pub low_hz: f32,
    pub mid_db: f32,
    pub mid_hz: f32,
    pub mid_q: f32,
    pub high_mid_db: f32,
    pub high_mid_hz: f32,
    pub high_mid_q: f32,
    pub high_db: f32,
    pub high_hz: f32,
    pub phaser_stages: u8,
    pub phaser_manual: f32,
    pub flanger_manual: f32,
    pub chorus_low_cut_hz: f32,
    pub chorus_high_cut_hz: f32,
    pub mod_shape: f32,
    pub mod_phase_degrees: f32,
    pub mod_retrigger: bool,
    pub mod_stepped: bool,
    pub mod_step_hz: f32,
    pub mod_step_beats: f32,
    pub slicer_duty: f32,
    pub slicer_compress: bool,
    pub rate_hz: f32,
    /// 0: free rate. Otherwise one LFO cycle / this many quarter-note beats.
    pub sync_beats: f32,
    pub depth: f32,
    pub pan: f32,
    pub width: f32,
    /// Legacy presets keep mono enhancement off; new instances opt in.
    pub enhance_mono: bool,
    pub enhance_amount: f32,
    pub enhance_low_cut_hz: f32,
    pub enhance_high_cut_hz: f32,
    pub time_ms: f32,
    pub feedback: f32,
    /// 0 is manual coefficient; 1..=16 maps to -60 dB after that many repeats.
    pub feedback_repeats: u8,
    pub effect_level: f32,
    pub low_cut_hz: f32,
    pub high_cut_hz: f32,
    pub delay_ratio: f32,
    pub steps: [f32; 16],
    pub step_count: u8,
    pub freeze: bool,
    pub decay_ms: f32,
    pub predelay_ms: f32,
    pub density: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MasterFxConfig {
    pub filter_enabled: bool,
    pub compressor_enabled: bool,
    pub reverb_enabled: bool,
    pub compressor: AudioFxConfig,
    pub reverb: AudioFxConfig,
    pub filter: super::filter_configs::FilterSettings,
}
impl Default for MasterFxConfig {
    fn default() -> Self {
        let mut compressor = AudioFxConfig::new(AudioFxKind::Dynamics);
        compressor.ratio = 2.0;
        compressor.threshold_db = -12.0;
        Self {
            filter_enabled: false,
            compressor_enabled: false,
            reverb_enabled: false,
            compressor,
            reverb: AudioFxConfig::new(AudioFxKind::Reverb),
            filter: super::filter_configs::FilterSettings::default(),
        }
    }
}
impl MasterFxConfig {
    pub fn sanitized(&self) -> Self {
        let mut result = *self;
        result.filter = result.filter.sanitized();
        result.compressor = result.compressor.sanitized();
        result.reverb = result.reverb.sanitized();
        result.compressor.kind = AudioFxKind::Dynamics;
        result.reverb.kind = AudioFxKind::Reverb;
        result
    }
}
impl Default for AudioFxConfig {
    fn default() -> Self {
        Self {
            kind: AudioFxKind::Transpose,
            mix: 1.0,
            level_db: 0.0,
            semitones: 0.0,
            preserve_formants: false,
            formant_shift_semitones: 0.0,
            pitch_steps: [0.0; 16],
            pitch_step_count: 8,
            pitch_sequence: false,
            pitch_step_beats: 0.5,
            root: 0,
            scale: Scale::Chromatic,
            harmony_steps: 2,
            direct: 1.0,
            voice: 0.8,
            octave_two: 0.0,
            retune_ms: 10.0,
            stability: 0.25,
            drive_db: 18.0,
            drive_style: DriveStyle::Soft,
            distortion_type: DistortionType::Legacy,
            distortion_tone: 0.0,
            distortion_direct: 0.0,
            distortion_effect: 0.5,
            tone_hz: 6000.0,
            dynamics_mode: DynamicsMode::Compressor,
            dynamics_profile: super::dynamics_profiles::DynamicsProfile::Custom,
            dynamics_amount: 0.0,
            threshold_db: -18.0,
            ratio: 4.0,
            knee_db: 6.0,
            attack_ms: 10.0,
            release_ms: 120.0,
            makeup_db: 0.0,
            low_db: 0.0,
            low_hz: 120.0,
            mid_db: 0.0,
            mid_hz: 1000.0,
            mid_q: 0.707,
            high_mid_db: 0.0,
            high_mid_hz: 3150.0,
            high_mid_q: 1.0,
            high_db: 0.0,
            high_hz: 6000.0,
            phaser_stages: 6,
            phaser_manual: 0.5,
            flanger_manual: 0.5,
            chorus_low_cut_hz: 0.0,
            chorus_high_cut_hz: 0.0,
            mod_shape: 0.0,
            mod_phase_degrees: 0.0,
            mod_retrigger: false,
            mod_stepped: false,
            mod_step_hz: 4.0,
            mod_step_beats: 0.0,
            slicer_duty: 1.0,
            slicer_compress: false,
            rate_hz: 1.0,
            sync_beats: 0.0,
            depth: 0.7,
            pan: 0.0,
            width: 1.0,
            enhance_mono: false,
            enhance_amount: 0.5,
            enhance_low_cut_hz: 0.0,
            enhance_high_cut_hz: 0.0,
            time_ms: 320.0,
            feedback: 0.35,
            delay_ratio: 0.5,
            feedback_repeats: 0,
            effect_level: 0.5,
            low_cut_hz: 0.0,
            high_cut_hz: 10000.0,
            steps: [
                1.0, 0.0, 0.7, 0.0, 1.0, 0.0, 0.7, 0.0, 1.0, 0.0, 0.7, 0.0, 1.0, 0.0, 0.7, 0.0,
            ],
            step_count: 16,
            freeze: true,
            decay_ms: 1800.0,
            predelay_ms: 15.0,
            density: 5,
        }
    }
}
impl AudioFxConfig {
    pub fn new(kind: AudioFxKind) -> Self {
        let mut p = Self {
            kind,
            ..Self::default()
        };
        match kind {
            AudioFxKind::Distortion => p.distortion_type = DistortionType::Vocal,
            AudioFxKind::Harmonist => {
                p.scale = Scale::Major;
                p.preserve_formants = true;
            }
            AudioFxKind::Electric => p.preserve_formants = true,
            AudioFxKind::PanningDelay | AudioFxKind::Delay => {
                p.mix = 1.0;
                p.feedback_repeats = 8;
            }
            AudioFxKind::Phaser => {
                p.mix = 0.5;
                p.phaser_stages = 4;
            }
            AudioFxKind::Flanger | AudioFxKind::Chorus => p.mix = 0.5,
            AudioFxKind::Equalizer => {
                p.mid_hz = 800.0;
                p.mid_q = 1.0;
            }
            AudioFxKind::StereoEnhance => p.enhance_mono = true,
            AudioFxKind::Reverb => p.mix = 0.25,
            AudioFxKind::Sustainer => {
                p.threshold_db = -28.0;
                p.ratio = 6.0;
                p.makeup_db = 9.0;
            }
            AudioFxKind::Octave => {
                p.voice = 0.65;
                p.octave_two = 0.25;
            }
            AudioFxKind::StepSlicer => p.sync_beats = 4.0,
            AudioFxKind::AutoPan => p.mod_retrigger = true,
            _ => {}
        }
        p
    }
    pub fn sanitized(&self) -> Self {
        let mut p = self.clone();
        macro_rules! bound {
            ($field:ident, $min:expr, $max:expr) => {
                p.$field = if p.$field.is_finite() {
                    p.$field.clamp($min, $max)
                } else {
                    Self::new(p.kind).$field
                };
            };
        }
        bound!(mix, 0.0, 1.0);
        bound!(level_db, -36.0, 12.0);
        bound!(pitch_step_beats, 0.0625, 8.0);
        p.pitch_step_count = p.pitch_step_count.clamp(1, 16);
        for step in &mut p.pitch_steps {
            *step = if step.is_finite() {
                step.clamp(-12.0, 12.0)
            } else {
                0.0
            };
        }
        bound!(semitones, -12.0, 12.0);
        bound!(formant_shift_semitones, -12.0, 12.0);
        p.root %= 12;
        p.harmony_steps = p.harmony_steps.clamp(-7, 7);
        bound!(direct, 0.0, 1.0);
        bound!(voice, 0.0, 1.0);
        bound!(octave_two, 0.0, 1.0);
        bound!(retune_ms, 0.0, 200.0);
        bound!(stability, 0.0, 1.0);
        bound!(drive_db, 0.0, 42.0);
        bound!(distortion_tone, -50.0, 50.0);
        bound!(distortion_direct, 0.0, 1.0);
        bound!(distortion_effect, 0.0, 1.0);
        bound!(dynamics_amount, -20.0, 20.0);
        bound!(tone_hz, 200.0, 20000.0);
        bound!(threshold_db, -60.0, 0.0);
        bound!(ratio, 1.0, 20.0);
        bound!(knee_db, 0.0, 18.0);
        bound!(attack_ms, 0.1, 200.0);
        bound!(release_ms, 10.0, 2000.0);
        bound!(makeup_db, -12.0, 24.0);
        bound!(low_db, -24.0, 24.0);
        bound!(low_hz, 30.0, 800.0);
        bound!(mid_db, -24.0, 24.0);
        bound!(mid_hz, 20.0, 12000.0);
        bound!(mid_q, 0.2, 16.0);
        bound!(high_mid_db, -24.0, 24.0);
        bound!(high_mid_hz, 20.0, 12000.0);
        bound!(high_mid_q, 0.2, 16.0);
        bound!(high_db, -24.0, 24.0);
        bound!(high_hz, 1000.0, 18000.0);
        p.phaser_stages = match p.phaser_stages {
            4 | 6 | 8 | 12 => p.phaser_stages,
            _ => 6,
        };
        bound!(phaser_manual, 0.0, 1.0);
        bound!(flanger_manual, 0.0, 1.0);
        bound!(chorus_low_cut_hz, 0.0, 12500.0);
        bound!(chorus_high_cut_hz, 0.0, 12500.0);
        bound!(enhance_amount, 0.0, 1.0);
        bound!(enhance_low_cut_hz, 0.0, 12500.0);
        bound!(enhance_high_cut_hz, 0.0, 12500.0);
        bound!(mod_shape, 0.0, 1.0);
        bound!(mod_phase_degrees, 0.0, 180.0);
        bound!(mod_step_hz, 0.1, 100.0);
        bound!(mod_step_beats, 0.0, 16.0);
        if p.mod_step_beats > 0.0 {
            p.mod_step_beats = p.mod_step_beats.max(0.015625);
        }
        bound!(slicer_duty, 0.01, 1.0);
        if p.kind == AudioFxKind::Sustainer {
            p.low_db = p.low_db.clamp(-20.0, 20.0);
            p.high_db = p.high_db.clamp(-20.0, 20.0);
        }
        bound!(rate_hz, 0.05, 20.0);
        bound!(sync_beats, 0.0, 16.0);
        bound!(depth, 0.0, 1.0);
        bound!(pan, -1.0, 1.0);
        bound!(width, 0.0, 2.0);
        bound!(time_ms, 1.0, 2000.0);
        bound!(feedback, 0.0, 1.0);
        if matches!(p.kind, AudioFxKind::Phaser | AudioFxKind::Flanger) {
            p.feedback = p.feedback.min(0.85);
        }
        bound!(delay_ratio, 0.1, 1.0);
        p.feedback_repeats = p.feedback_repeats.min(16);
        bound!(effect_level, 0.0, 1.2);
        bound!(low_cut_hz, 0.0, 12500.0);
        bound!(high_cut_hz, 0.0, 20000.0);
        for v in &mut p.steps {
            *v = if v.is_finite() {
                v.clamp(0.0, 1.0)
            } else {
                0.0
            };
        }
        p.step_count = p.step_count.clamp(1, 16);
        bound!(decay_ms, 100.0, 15000.0);
        bound!(predelay_ms, 0.0, 500.0);
        p.density = p.density.clamp(1, 10);
        p
    }
}
