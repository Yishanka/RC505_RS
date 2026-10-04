use crate::config::{
    config_type::{EnumConfig, NumericConfig},
    envelope_configs::EnvelopeConfigs,
    filter_configs::FilterConfigs,
    note_configs::NoteConfigs,
};

#[derive(Clone, Copy, PartialEq)]
pub enum Waveform {
    Sine,
    Saw,
    Square,
    Triangle,
    Vocal,
    Sample,
    Rect,
    DetuneSaw,
    VintageSaw,
}

impl std::fmt::Display for Waveform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Waveform::Sine => "Sine",
                Waveform::Saw => "Saw",
                Waveform::Square => "Square",
                Waveform::Triangle => "Triangle",
                Waveform::Vocal => "Vocal",
                Waveform::Sample => "Sample",
                Waveform::Rect => "RECT (25%)",
                Waveform::DetuneSaw => "Detune Saw",
                Waveform::VintageSaw => "Vintage Saw",
            }
        )
    }
}

pub struct OscillatorConfigs {
    pub sel_idx: Option<usize>,
    pub audio_sel_idx: Option<usize>,
    pub osc_filter_sel_idx: Option<usize>,
    pub waveform: EnumConfig<Waveform>,
    pub voices: usize,
    pub input_gate: bool,
    pub dry_level: f32,
    pub input_mod_sens: Option<f32>,
    pub lfo: LfoConfig,
    pub lfo2: LfoConfig,
    pub mono_legato: bool,
    pub glide_ms: f32,
    pub glide_mode: GlideMode,
    pub sample: Option<std::sync::Arc<SampleAsset>>,
    pub sample_ref: Option<SavedSampleRef>,
    pub sample_temporary: bool,
    pub sample_mode: SampleMode,
    pub sample_root: usize,
    pub sample_fine_cents: f32,
    pub sample_loop: bool,
    pub sample_start: f32,
    pub sample_end: f32,
    pub vocal_formant: f32,
    pub capture_serial: u64,
    pub capture_ms: usize,
    pub capture: Option<std::sync::Arc<SampleCapture>>,
    pub sample_message: String,
    pub sample_job: Option<std::sync::mpsc::Receiver<Result<SampleAsset, String>>>,
    pub level: NumericConfig,
    pub threshold: NumericConfig,
    pub note: NoteConfigs,
    pub envelope: EnvelopeConfigs,
    pub osc_filter: FilterConfigs,
    pub osc_filter_env: EnvelopeConfigs,
}

impl OscillatorConfigs {
    pub fn new() -> Self {
        Self {
            sel_idx: None,
            audio_sel_idx: None,
            osc_filter_sel_idx: None,
            waveform: EnumConfig::new(
                "Waveform",
                Waveform::Sine,
                vec![
                    Waveform::Sine,
                    Waveform::Saw,
                    Waveform::Square,
                    Waveform::Triangle,
                    Waveform::Vocal,
                    Waveform::Sample,
                    Waveform::Rect,
                    Waveform::DetuneSaw,
                    Waveform::VintageSaw,
                ],
            ),
            voices: 8,
            input_gate: false,
            dry_level: 1.0,
            input_mod_sens: None,
            lfo: LfoConfig::default(),
            lfo2: LfoConfig::default(),
            mono_legato: false,
            glide_ms: 0.0,
            glide_mode: GlideMode::Overlap,
            sample: None,
            sample_ref: None,
            sample_temporary: true,
            sample_mode: SampleMode::Wavetable,
            sample_root: 48,
            sample_fine_cents: 0.0,
            sample_loop: true,
            sample_start: 0.0,
            sample_end: 1.0,
            vocal_formant: 0.0,
            capture_serial: 0,
            capture_ms: 100,
            capture: None,
            sample_message: String::new(),
            sample_job: None,
            level: NumericConfig::new("OSC level (%)", 70),
            threshold: NumericConfig::new("Threshold", 10),
            note: NoteConfigs::new(),
            envelope: EnvelopeConfigs::new(),
            osc_filter: {
                let mut filter = FilterConfigs::new();
                filter.mix.value = 0;
                filter.cutoff_hz.value = 8000;
                filter
            },
            osc_filter_env: EnvelopeConfigs::new(),
        }
    }
}

impl crate::config::config_type::ConfigSet for OscillatorConfigs {
    fn next(&mut self) {
        let curr = self.sel_idx.unwrap_or(0);
        self.sel_idx = Some((curr + 1).min(2));
    }

    fn prev(&mut self) {
        let curr = self.sel_idx.unwrap_or(0);
        self.sel_idx = Some(curr.saturating_sub(1));
    }

    fn confirm(&mut self) {}
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum GlideMode {
    #[default]
    Overlap,
    AllNotes,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum SampleMode {
    #[default]
    Wavetable,
    Sampler,
}
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum LfoShape {
    #[default]
    Sine,
    Triangle,
    Saw,
    Square,
    Custom,
}
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum LfoTarget {
    #[default]
    Volume,
    Cutoff,
    Pitch,
}
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum LfoMode {
    #[default]
    Free,
    Retrigger,
}
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CurvePoint {
    pub x: f32,
    pub y: f32,
    pub curve: f32,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct LfoConfig {
    pub enabled: bool,
    pub shape: LfoShape,
    pub target: LfoTarget,
    pub mode: LfoMode,
    pub sync: bool,
    pub rate_hz: f32,
    pub beats: f32,
    pub depth: f32,
    pub points: Vec<CurvePoint>,
}
impl Default for LfoConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            shape: LfoShape::Sine,
            target: LfoTarget::Volume,
            mode: LfoMode::Free,
            sync: true,
            rate_hz: 1.0,
            beats: 1.0,
            depth: 0.5,
            points: vec![
                CurvePoint {
                    x: 0.0,
                    y: 0.0,
                    curve: 0.0,
                },
                CurvePoint {
                    x: 0.5,
                    y: 1.0,
                    curve: 0.0,
                },
                CurvePoint {
                    x: 1.0,
                    y: 0.0,
                    curve: 0.0,
                },
            ],
        }
    }
}
impl LfoConfig {
    pub fn sanitize(&mut self) {
        self.rate_hz = finite(self.rate_hz, 1.0).clamp(0.01, 40.0);
        self.beats = finite(self.beats, 1.0).clamp(0.0625, 32.0);
        self.depth = finite(self.depth, 0.5).clamp(0.0, 1.0);
        self.points.truncate(32);
        self.points
            .retain(|p| p.x.is_finite() && p.y.is_finite() && p.curve.is_finite());
        for p in &mut self.points {
            p.x = p.x.clamp(0.0, 1.0);
            p.y = p.y.clamp(0.0, 1.0);
            p.curve = p.curve.clamp(-1.0, 1.0);
        }
        self.points.sort_by(|a, b| a.x.total_cmp(&b.x));
        self.points.dedup_by(|a, b| (a.x - b.x).abs() < 0.001);
        if self.points.len() < 2 {
            self.points = Self::default().points;
        }
        self.points[0].x = 0.0;
        self.points.last_mut().unwrap().x = 1.0;
    }
}
fn finite(v: f32, fallback: f32) -> f32 {
    if v.is_finite() { v } else { fallback }
}
impl OscillatorConfigs {
    pub fn sanitize_source(&mut self) {
        self.glide_ms = finite(self.glide_ms, 0.0).clamp(0.0, 2000.0);
        self.dry_level = finite(self.dry_level, 1.0).clamp(0.0, 1.0);
        self.input_mod_sens = self
            .input_mod_sens
            .filter(|v| v.is_finite())
            .map(|v| v.clamp(-50.0, 50.0));
        let minimum = self
            .sample
            .as_ref()
            .map_or(0.001, |sample| 2.0 / sample.frames.len().max(2) as f32)
            .min(1.0);
        self.sample_start = finite(self.sample_start, 0.0).clamp(0.0, (1.0 - minimum).max(0.0));
        self.sample_end =
            finite(self.sample_end, 1.0).clamp((self.sample_start + minimum).min(1.0), 1.0);
        self.sample_fine_cents = finite(self.sample_fine_cents, 0.0).clamp(-100.0, 100.0);
        self.vocal_formant = finite(self.vocal_formant, 0.0).clamp(0.0, 1.0);
    }
    pub fn select_sample_region(&mut self) {
        if let Some(sample) = &self.sample {
            if self.sample_mode == SampleMode::Wavetable && sample.cycle_end > sample.cycle_start {
                self.sample_start = sample.cycle_start as f32 / sample.frames.len().max(1) as f32;
                self.sample_end = sample.cycle_end.min(sample.frames.len()) as f32
                    / sample.frames.len().max(1) as f32;
            } else {
                self.sample_start = 0.0;
                self.sample_end = 1.0;
            }
        }
    }
    fn adopt_sample(&mut self, sample: SampleAsset) {
        self.sample_ref = None;
        self.sample_temporary = true;
        self.sample_message = sample.name.clone();
        if let Some(hz) = sample.root_hz {
            self.sample_root = (57.0 + 12.0 * (hz / 440.0).log2())
                .round()
                .clamp(0.0, 119.0) as usize;
            self.sample_fine_cents = (1200.0
                * (hz
                    / crate::config::note_configs::NoteOct::from_pitch_index(self.sample_root)
                        .freq_hz())
                .log2())
            .clamp(-100.0, 100.0);
        } else {
            self.sample_fine_cents = 0.0;
        }
        self.sample = Some(std::sync::Arc::new(sample));
        self.waveform.value = Waveform::Sample;
        self.select_sample_region();
    }
    pub fn poll_sample(&mut self) {
        self.sanitize_source();
        let imported = self
            .sample_job
            .as_ref()
            .and_then(|job| match job.try_recv() {
                Ok(result) => Some(result),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Some(Err("Sample import was cancelled".into()))
                }
                Err(_) => None,
            });
        if let Some(imported) = imported {
            self.sample_job = None;
            match imported {
                Ok(sample) => self.adopt_sample(sample),
                Err(message) => self.sample_message = message,
            }
        }
        if self.sample_job.is_none()
            && self
                .capture
                .as_ref()
                .is_some_and(|c| c.state.load(std::sync::atomic::Ordering::Acquire) == 2)
        {
            let capture = self.capture.take().unwrap();
            let (tx, rx) = std::sync::mpsc::channel();
            self.sample_job = Some(rx);
            std::thread::spawn(move || {
                let _ = tx.send(
                    capture
                        .completed()
                        .ok_or_else(|| "Capture incomplete".into()),
                );
            });
        }
    }
}
impl crate::config::AppConfig {
    pub fn poll_synth_assets(&mut self) {
        for bank in &mut self.input_fx.banks {
            for slot in &mut bank.slots {
                if let Some(crate::config::InputFx::Oscillator(osc)) = &mut slot.fx {
                    osc.poll_sample();
                }
            }
        }
    }
}

mod sample;
pub use sample::{SampleAsset, SampleCapture, SavedSampleRef};
