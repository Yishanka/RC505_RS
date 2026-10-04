//! Bounded sample assets, import analysis and a one-writer capture mailbox.
#[cfg(test)]
use super::OscillatorConfigs;
use super::finite;
/// An explicitly saved, self-contained sound preset owns this sample.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SavedSampleRef {
    pub preset: String,
    pub sha256: String,
    pub content_hash: u64,
    pub sample_rate: u32,
    pub frames: usize,
}
impl SavedSampleRef {
    pub fn matches(&self, sample: &SampleAsset) -> bool {
        self.content_hash == sample.content_hash
            && self.sample_rate == sample.sample_rate
            && self.frames == sample.frames.len()
    }
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SampleAsset {
    pub name: String,
    pub sample_rate: u32,
    pub content_hash: u64,
    pub frames: Vec<f32>,
    #[serde(default)]
    pub root_hz: Option<f32>,
    #[serde(default)]
    pub cycle_start: usize,
    #[serde(default)]
    pub cycle_end: usize,
}
impl SampleAsset {
    pub const MAX_FRAMES: usize = 96_000;
    pub fn new(name: String, sample_rate: u32, mut frames: Vec<f32>) -> Self {
        frames.truncate(Self::MAX_FRAMES);
        for x in &mut frames {
            *x = finite(*x, 0.0).clamp(-1.0, 1.0);
        }
        let mut hash = 0xcbf29ce484222325u64;
        for x in &frames {
            hash = (hash ^ x.to_bits() as u64).wrapping_mul(0x100000001b3);
        }
        Self {
            name: name.chars().take(128).collect(),
            sample_rate: sample_rate.clamp(8000, 192000),
            content_hash: hash,
            frames,
            root_hz: None,
            cycle_start: 0,
            cycle_end: 0,
        }
    }
    pub fn load_wav(path: &std::path::Path) -> anyhow::Result<Self> {
        use anyhow::{Context, bail};
        let mut reader = hound::WavReader::open(path).context("Cannot open WAV sample")?;
        let spec = reader.spec();
        if spec.channels == 0 || spec.channels > 8 || !(8000..=192000).contains(&spec.sample_rate) {
            bail!("Use a mono/stereo WAV at 8–192 kHz");
        }
        if reader.duration() > spec.sample_rate.saturating_mul(30) {
            bail!(
                "Sample is longer than 30 seconds; trim it before import (OSC uses the first 2 seconds)."
            );
        }
        let limit = spec.sample_rate as usize * 2 * spec.channels as usize;
        let raw: Vec<f32> = if spec.sample_format == hound::SampleFormat::Float {
            reader
                .samples::<f32>()
                .take(limit)
                .collect::<Result<Vec<_>, _>>()?
        } else {
            let scale = 2.0f32.powi(spec.bits_per_sample as i32 - 1);
            reader
                .samples::<i32>()
                .take(limit)
                .map(|s| s.map(|x| x as f32 / scale))
                .collect::<Result<Vec<_>, _>>()?
        };
        let mono: Vec<f32> = raw
            .chunks_exact(spec.channels as usize)
            .map(|f| f.iter().sum::<f32>() / spec.channels as f32)
            .collect();
        if mono.len() < 8 {
            bail!("The sample contains too few frames");
        }
        Ok(Self::prepare_recording(
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            spec.sample_rate,
            &mono,
        ))
    }
}

/// One-shot capture mailbox. Allocated by the UI when armed, written by one audio
/// callback, consumed only after Release/Acquire publishes completion.
pub struct SampleCapture {
    pub samples: Vec<std::sync::atomic::AtomicU32>,
    pub state: std::sync::atomic::AtomicUsize,
    pub frames: std::sync::atomic::AtomicUsize,
    pub sample_rate: std::sync::atomic::AtomicU32,
    pub milliseconds: usize,
}
impl SampleCapture {
    pub fn new(milliseconds: usize) -> Self {
        Self {
            samples: (0..384_000)
                .map(|_| std::sync::atomic::AtomicU32::new(0))
                .collect(),
            state: std::sync::atomic::AtomicUsize::new(0),
            frames: std::sync::atomic::AtomicUsize::new(0),
            sample_rate: std::sync::atomic::AtomicU32::new(48000),
            milliseconds: milliseconds.clamp(20, 2000),
        }
    }
    pub fn completed(&self) -> Option<SampleAsset> {
        use std::sync::atomic::Ordering;
        if self.state.load(Ordering::Acquire) != 2 {
            return None;
        }
        let len = self.frames.load(Ordering::Relaxed).min(self.samples.len());
        let frames: Vec<_> = self.samples[..len]
            .iter()
            .map(|x| f32::from_bits(x.load(Ordering::Relaxed)))
            .collect();
        Some(SampleAsset::prepare_recording(
            "Captured input".into(),
            self.sample_rate.load(Ordering::Relaxed),
            &frames,
        ))
    }
}

impl SampleAsset {
    /// Import/capture preparation runs on a worker. Samples keep their duration;
    /// anti-alias filtering is centered, so it introduces no leading DSP delay.
    pub fn prepare_recording(name: String, sr: u32, input: &[f32]) -> Self {
        let frames = resample_mono(input, sr.clamp(8000, 192000), 48000, Self::MAX_FRAMES);
        let mut asset = Self::new(name, 48000, frames);
        asset.analyze_cycle();
        asset
    }
    pub fn validated_shared(asset: &std::sync::Arc<Self>) -> std::sync::Arc<Self> {
        use std::collections::VecDeque;
        use std::sync::{Arc, Mutex, OnceLock, Weak};
        // A Weak reference also prevents Arc::get_mut. Arc::make_mut changes the
        // allocation identity, so a hit cannot hide mutation of validated PCM.
        static CHECKED: OnceLock<Mutex<VecDeque<Weak<SampleAsset>>>> = OnceLock::new();
        let cache = CHECKED.get_or_init(|| Mutex::new(VecDeque::new()));
        if let Ok(entries) = cache.lock() {
            if entries
                .iter()
                .any(|old| old.as_ptr() == Arc::as_ptr(asset) && old.strong_count() > 0)
            {
                return asset.clone();
            }
        }
        let mut hash = 0xcbf29ce484222325u64;
        let clean = asset.frames.len() <= Self::MAX_FRAMES
            && (8000..=192000).contains(&asset.sample_rate)
            && asset.name.chars().count() <= 128
            && asset
                .root_hz
                .is_none_or(|f| f.is_finite() && (20.0..=10000.0).contains(&f))
            && asset.cycle_start <= asset.cycle_end
            && asset.cycle_end <= asset.frames.len()
            && asset.frames.iter().all(|x| {
                hash = (hash ^ x.to_bits() as u64).wrapping_mul(0x100000001b3);
                x.is_finite() && (-1.0..=1.0).contains(x)
            });
        let validated = if clean && hash == asset.content_hash {
            asset.clone()
        } else {
            Arc::new(asset.validated())
        };
        if let Ok(mut entries) = cache.lock() {
            if entries.len() >= 256 {
                entries.pop_front();
            }
            entries.push_back(Arc::downgrade(&validated));
        }
        validated
    }
    pub fn validated(&self) -> Self {
        let mut asset = Self::new(
            self.name.clone(),
            self.sample_rate,
            self.frames.iter().copied().take(Self::MAX_FRAMES).collect(),
        );
        asset.root_hz = self
            .root_hz
            .filter(|f| f.is_finite() && (20.0..=10000.0).contains(f));
        asset.cycle_start = self.cycle_start.min(asset.frames.len().saturating_sub(2));
        asset.cycle_end = self.cycle_end.min(asset.frames.len());
        if asset.cycle_end <= asset.cycle_start {
            asset.cycle_start = 0;
            asset.cycle_end = (asset.sample_rate as f32 / 261.6256).round() as usize;
            asset.cycle_end = asset.cycle_end.min(asset.frames.len());
        }
        asset
    }
    fn analyze_cycle(&mut self) {
        let data = &self.frames;
        if data.len() < 16 {
            return;
        }
        // Search a strong 80 ms window, away from the first transient if possible.
        let window = 9600usize.min(data.len());
        let mut begin = 0;
        let mut best = 0.0f64;
        for start in (0..=data.len() - window).step_by(480) {
            let energy = data[start..start + window]
                .iter()
                .step_by(4)
                .map(|x| (*x as f64).powi(2))
                .sum::<f64>();
            if energy > best {
                best = energy;
                begin = start;
            }
        }
        let stride = 4;
        let analysis: Vec<f32> = data[begin..begin + window]
            .iter()
            .step_by(stride)
            .copied()
            .collect();
        let max_lag = (analysis.len() / 2).min(480);
        let min_lag = 8;
        let mut cmnd = vec![1.0f64; max_lag + 1];
        let mut running = 0.0;
        for lag in 1..=max_lag {
            let count = analysis.len() - max_lag;
            let difference = (0..count)
                .map(|i| (analysis[i] as f64 - analysis[i + lag] as f64).powi(2))
                .sum::<f64>();
            running += difference;
            cmnd[lag] = if running > 1e-15 {
                difference * lag as f64 / running
            } else {
                1.0
            };
        }
        let mut lag = None;
        for i in min_lag..max_lag.saturating_sub(1) {
            if cmnd[i] < 0.18 && cmnd[i] <= cmnd[i - 1] && cmnd[i] < cmnd[i + 1] {
                lag = Some(i);
                break;
            }
        }
        let period = if let Some(lag) = lag {
            let ym = cmnd[lag - 1];
            let y = cmnd[lag];
            let yp = cmnd[lag + 1];
            let correction = (0.5 * (ym - yp) / (ym - 2.0 * y + yp).max(1e-12)).clamp(-0.5, 0.5);
            let estimate = (lag as f64 + correction) * stride as f64;
            let low = (estimate.floor() as usize).saturating_sub(8).max(2);
            let high = ((estimate.ceil() as usize) + 8).min(window / 2);
            let count = (window - high).min(2048);
            let difference = |lag: usize| {
                (0..count)
                    .map(|i| (data[begin + i] as f64 - data[begin + i + lag] as f64).powi(2))
                    .sum::<f64>()
            };
            let best = (low..=high)
                .min_by(|a, b| difference(*a).total_cmp(&difference(*b)))
                .unwrap_or(low);
            let (ym, y, yp) = (difference(best - 1), difference(best), difference(best + 1));
            let refined =
                best as f64 + (0.5 * (ym - yp) / (ym - 2.0 * y + yp).max(1e-12)).clamp(-0.5, 0.5);
            self.root_hz = Some((48000.0 / refined) as f32);
            refined.round() as usize
        } else {
            184
        };
        let start = (begin..(begin + period * 2).min(data.len().saturating_sub(period + 1)))
            .find(|i| data[*i] <= 0.0 && data[*i + 1] > 0.0)
            .unwrap_or(begin);
        self.cycle_start = start;
        self.cycle_end = (start + period.max(4)).min(data.len());
    }
}

fn resample_mono(input: &[f32], source: u32, target: u32, maximum: usize) -> Vec<f32> {
    if input.is_empty() {
        return Vec::new();
    }
    let clean = |x: f32| {
        if x.is_finite() {
            x.clamp(-1.0, 1.0)
        } else {
            0.0
        }
    };
    if source == target {
        return input.iter().take(maximum).map(|x| clean(*x)).collect();
    }
    const TAPS: usize = 64;
    const PHASES: usize = 256;
    let cutoff = (target as f64 / source as f64).min(1.0) * 0.94;
    let coefficients: Vec<[f32; TAPS]> = (0..PHASES)
        .map(|phase| {
            let fraction = phase as f64 / PHASES as f64;
            let mut row = std::array::from_fn(|i| {
                let x = i as f64 - 32.0 - fraction;
                if x.abs() >= 32.0 {
                    return 0.0;
                }
                let sinc = if x.abs() < 1e-10 {
                    cutoff
                } else {
                    (std::f64::consts::PI * x * cutoff).sin() / (std::f64::consts::PI * x)
                };
                (sinc * (0.5 + 0.5 * (std::f64::consts::PI * x / 32.0).cos())) as f32
            });
            let sum: f32 = row.iter().sum();
            for x in &mut row {
                *x /= sum;
            }
            row
        })
        .collect();
    let length = (input.len() as f64 * target as f64 / source as f64).round() as usize;
    (0..length.min(maximum))
        .map(|i| {
            let position = i as f64 * source as f64 / target as f64;
            let center = position.floor() as isize;
            let row =
                &coefficients[((position - position.floor()) * PHASES as f64).floor() as usize];
            row.iter()
                .enumerate()
                .map(|(tap, c)| {
                    clean(
                        input[(center + tap as isize - 32).clamp(0, input.len() as isize - 1)
                            as usize],
                    ) * c
                })
                .sum::<f32>()
        })
        .collect()
}

#[cfg(test)]
mod sample_tests {
    use super::*;
    #[test]
    fn sinc_import_preserves_duration_and_pitch_rejects_ultrasonic_input() {
        for sr in [8000u32, 44100, 48000, 96000, 192000] {
            let frames: Vec<_> = (0..sr as usize / 4)
                .map(|i| (std::f64::consts::TAU * 1000.0 * i as f64 / sr as f64).sin() as f32 * 0.5)
                .collect();
            let result = resample_mono(&frames, sr, 48000, SampleAsset::MAX_FRAMES);
            assert_eq!(result.len(), 12000);
            let error = result
                .iter()
                .enumerate()
                .skip(128)
                .take(11744)
                .map(|(i, x)| {
                    (*x as f64 - 0.5 * (std::f64::consts::TAU * 1000.0 * i as f64 / 48000.0).sin())
                        .powi(2)
                })
                .sum::<f64>()
                / 11744.0;
            assert!(
                error < 0.0001,
                "Pitch/phase/duration mismatch at {sr}: {error}"
            );
        }
        let high: Vec<_> = (0..24000)
            .map(|i| (std::f64::consts::TAU * 30000.0 * i as f64 / 96000.0).sin() as f32)
            .collect();
        let out = resample_mono(&high, 96000, 48000, SampleAsset::MAX_FRAMES);
        let power = out[128..out.len() - 128]
            .iter()
            .map(|x| (*x as f64).powi(2))
            .sum::<f64>()
            / (out.len() - 256) as f64;
        assert!(
            power < 1e-5,
            "Out-of-band source folded into the sample: {power}"
        );
    }
    #[test]
    fn captured_root_and_default_cycle_do_not_compress_the_whole_recording() {
        for hz in [27.5f32, 55.0, 220.0, 432.6, 1000.0] {
            let input: Vec<_> = (0..12000)
                .map(|i| {
                    let phase = std::f32::consts::TAU * hz * i as f32 / 48000.0;
                    0.5 * phase.sin() + 0.2 * (phase * 2.0).sin()
                })
                .collect();
            let asset = SampleAsset::prepare_recording("Test".into(), 48000, &input);
            let found = asset
                .root_hz
                .expect("Stable harmonic source must have a detected fundamental");
            let cents = (1200.0 * (found / hz).log2()).abs();
            assert!(cents < 1.0, "Expected {hz}, got {found}: {cents} cents");
            assert!(((asset.cycle_end - asset.cycle_start) as f32) < 48000.0 / hz + 2.0);
            let mut c = OscillatorConfigs::new();
            c.adopt_sample(asset);
            assert!(c.sample_end - c.sample_start < 0.2);
            let tuned = crate::config::note_configs::NoteOct::from_pitch_index(c.sample_root)
                .freq_hz()
                * 2.0f32.powf(c.sample_fine_cents / 1200.0);
            assert!((1200.0 * (tuned / found).log2()).abs() < 0.01);
        }
    }
    #[test]
    fn capture_keeps_two_seconds_at_high_sample_rate_and_untrusted_pcm_is_sanitized() {
        use std::sync::{Arc, atomic::Ordering};
        let capture = SampleCapture::new(2000);
        capture.sample_rate.store(192000, Ordering::Relaxed);
        capture.frames.store(384000, Ordering::Relaxed);
        capture.state.store(2, Ordering::Release);
        let asset = capture.completed().unwrap();
        assert_eq!(asset.frames.len(), 96000);
        assert_eq!(asset.sample_rate, 48000);
        let bad = Arc::new(SampleAsset {
            name: "bad".into(),
            sample_rate: u32::MAX,
            content_hash: 7,
            frames: vec![f32::NAN; 200000],
            root_hz: Some(f32::INFINITY),
            cycle_start: usize::MAX,
            cycle_end: usize::MAX,
        });
        let fixed = SampleAsset::validated_shared(&bad);
        assert_eq!(fixed.frames.len(), 96000);
        assert!(fixed.frames.iter().all(|x| *x == 0.0));
        assert!(fixed.root_hz.is_none());
        assert!(fixed.cycle_end <= fixed.frames.len());
        assert_ne!(fixed.content_hash, 7);
        let again = SampleAsset::validated_shared(&fixed);
        assert!(Arc::ptr_eq(&fixed, &again));
        let mut changed = fixed.clone();
        Arc::make_mut(&mut changed).frames[0] = f32::INFINITY;
        let checked = SampleAsset::validated_shared(&changed);
        assert_eq!(checked.frames[0], 0.0);
        assert!(!Arc::ptr_eq(&changed, &checked));
    }
    #[test]
    fn high_note_single_cycle_survives_project_save_reload() {
        use crate::config::{AppConfig, FxKind, InputFx};
        let mut c = AppConfig::new(120, 0, 5);
        c.input_fx.set_slot_kind(0, 0, FxKind::Oscillator);
        let input: Vec<_> = (0..96000)
            .map(|i| (std::f64::consts::TAU * 1000.0 * i as f64 / 48000.0).sin() as f32)
            .collect();
        let asset = SampleAsset::prepare_recording("High cycle".into(), 48000, &input);
        let Some(InputFx::Oscillator(osc)) = &mut c.input_fx.banks[0].slots[0].fx else {
            panic!()
        };
        osc.adopt_sample(asset);
        osc.sanitize_source();
        let before = (osc.sample_start, osc.sample_end);
        assert!(
            ((before.1 - before.0) * 96000.0 - 48.0).abs() < 0.02,
            "A high source must retain one cycle, not be expanded to 96 frames"
        );
        let encoded = serde_json::to_string(&crate::project::data_from_config(&c)).unwrap();
        let mut restored = AppConfig::new(120, 0, 5);
        crate::project::apply_data_to_config(
            &mut restored,
            serde_json::from_str(&encoded).unwrap(),
        );
        let Some(InputFx::Oscillator(osc)) = &mut restored.input_fx.banks[0].slots[0].fx else {
            panic!()
        };
        assert_eq!(osc.sample_start.to_bits(), before.0.to_bits());
        assert_eq!(osc.sample_end.to_bits(), before.1.to_bits());
    }
}
