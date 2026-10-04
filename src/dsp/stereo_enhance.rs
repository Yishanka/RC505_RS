//! Mono-compatible stereo generation using two short Schroeder-allpass paths.
//! Only the side signal is decorated: the original mid has no added delay.
//! This is an original design, not a claim about BOSS's proprietary algorithm.
use crate::config::audio_fx::AudioFxConfig;

#[derive(Clone)]
struct Allpass {
    memory: Vec<f32>,
    cursor: usize,
    filled: usize,
    gain: f32,
}
impl Allpass {
    fn new(sr: f32, samples48k: usize, gain: f32) -> Self {
        Self {
            memory: vec![0.0; ((samples48k as f32 * sr / 48000.0).round() as usize).max(1)],
            cursor: 0,
            filled: 0,
            gain,
        }
    }
    fn reset(&mut self) {
        self.cursor = 0;
        self.filled = 0;
    }
    fn next(&mut self, input: f32) -> f32 {
        let delayed = if self.filled >= self.memory.len() {
            self.memory[self.cursor]
        } else {
            0.0
        };
        let output = delayed - self.gain * input;
        let value = input + self.gain * output;
        self.memory[self.cursor] = if value.abs() < 1e-25 { 0.0 } else { value };
        self.cursor = (self.cursor + 1) % self.memory.len();
        self.filled = (self.filled + 1).min(self.memory.len());
        output
    }
}
#[derive(Clone, Copy, Default)]
struct OnePole {
    state: f32,
}
impl OnePole {
    fn low(&mut self, input: f32, g: f32) -> f32 {
        let v = (input - self.state) * g;
        let output = v + self.state;
        self.state = output + v;
        if self.state.abs() < 1e-25 {
            self.state = 0.0;
        }
        output
    }
    fn high(&mut self, input: f32, g: f32) -> f32 {
        input - self.low(input, g)
    }
}
fn coefficient(sr: f32, hz: f32) -> f32 {
    let tangent = (std::f32::consts::PI * hz.clamp(1.0, sr * 0.45) / sr).tan();
    tangent / (1.0 + tangent)
}
#[derive(Clone)]
pub struct StereoEnhance {
    sr: f32,
    smooth: f32,
    bass_g: f32,
    bass: [OnePole; 4],
    paths: [[Allpass; 2]; 2],
    amount: f32,
    target_amount: f32,
    filters: [[OnePole; 2]; 2],
    cut_g: [f32; 2],
    target_g: [f32; 2],
    cut_mix: [f32; 2],
    target_mix: [f32; 2],
}
impl StereoEnhance {
    pub fn new(sr: f32) -> Self {
        Self {
            sr,
            smooth: 1.0 - (-1.0 / (sr * 0.01)).exp(),
            bass_g: coefficient(sr, 200.0),
            bass: [OnePole::default(); 4],
            paths: [
                [Allpass::new(sr, 37, 0.55), Allpass::new(sr, 113, 0.65)],
                [Allpass::new(sr, 59, 0.55), Allpass::new(sr, 173, 0.65)],
            ],
            amount: 0.0,
            target_amount: 0.0,
            filters: [[OnePole::default(); 2]; 2],
            cut_g: [coefficient(sr, 20.0), coefficient(sr, 20000.0)],
            target_g: [coefficient(sr, 20.0), coefficient(sr, 20000.0)],
            cut_mix: [0.0; 2],
            target_mix: [0.0; 2],
        }
    }
    pub fn reset(&mut self) {
        for path in &mut self.paths {
            for section in path {
                section.reset();
            }
        }
        self.bass = [OnePole::default(); 4];
        self.filters = [[OnePole::default(); 2]; 2];
        self.amount = 0.0;
        self.cut_mix = [0.0; 2];
    }
    pub fn configure(&mut self, p: &AudioFxConfig) {
        self.target_amount = if p.enhance_mono {
            p.enhance_amount
        } else {
            0.0
        };
        for (index, hz) in [p.enhance_low_cut_hz, p.enhance_high_cut_hz]
            .into_iter()
            .enumerate()
        {
            self.target_mix[index] = if hz > 0.0 { 1.0 } else { 0.0 };
            if hz > 0.0 {
                self.target_g[index] = coefficient(self.sr, hz.max(20.0));
            }
        }
    }
    pub fn next(&mut self, input: [f32; 2], width: f32) -> [f32; 2] {
        self.amount += self.smooth * (self.target_amount - self.amount);
        if (self.target_amount - self.amount).abs() < 1e-7 {
            self.amount = self.target_amount;
        }
        let mid = (input[0] + input[1]) * 0.5;
        let mut side = (input[0] - input[1]) * 0.5;
        // Keep the decoration history warm while bypassed. The four-pole bass
        // exclusion is never bypassed, even when the optional side low-cut is.
        let mut band = mid;
        for filter in &mut self.bass {
            band = filter.high(band, self.bass_g);
        }
        let decorrelated = self.paths.each_mut().map(|path| {
            let mut value = band;
            for section in path {
                value = section.next(value);
            }
            value
        });
        side += (decorrelated[0] - decorrelated[1]) * 0.5 * self.amount;
        for i in 0..2 {
            self.cut_g[i] += self.smooth * (self.target_g[i] - self.cut_g[i]);
            self.cut_mix[i] += self.smooth * (self.target_mix[i] - self.cut_mix[i]);
            if (self.target_mix[i] - self.cut_mix[i]).abs() < 1e-7 {
                self.cut_mix[i] = self.target_mix[i];
            }
            let mut filtered = side;
            for filter in &mut self.filters[i] {
                filtered = if i == 0 {
                    filter.high(filtered, self.cut_g[i])
                } else {
                    filter.low(filtered, self.cut_g[i])
                };
            }
            side += (filtered - side) * self.cut_mix[i];
        }
        side *= width;
        [mid + side, mid - side]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::audio_fx::AudioFxKind;
    fn measure(sr: f32, hz: f32, p: AudioFxConfig) -> (f64, f64) {
        let mut state = StereoEnhance::new(sr);
        state.configure(&p);
        let mut mid = 0.0;
        let mut side = 0.0;
        for i in 0..sr as usize {
            let input = (std::f32::consts::TAU * hz * i as f32 / sr).sin() * 0.2;
            let out = state.next([input, input], 1.0);
            assert!(
                ((out[0] + out[1]) * 0.5 - input).abs() < 1e-6,
                "Mono sum changed at {sr} / {hz} / {i}"
            );
            if i > sr as usize / 2 {
                mid += (input as f64).powi(2);
                side += (((out[0] - out[1]) * 0.5) as f64).powi(2);
            }
        }
        (mid, side)
    }
    #[test]
    fn mono_has_stereo_energy_with_unchanged_mid_and_bass_stays_centered() {
        let p = AudioFxConfig::new(AudioFxKind::StereoEnhance);
        for sr in [8000.0, 44100.0, 48000.0, 96000.0, 192000.0] {
            let (mid, side) = measure(sr, 1400.0, p);
            assert!(
                side / mid > 0.015,
                "No useful stereo side at {sr}: {}",
                side / mid
            );
            let (bass, side) = measure(sr, 60.0, p);
            assert!(
                side / bass < 0.00005,
                "Bass spread at {sr}: {}",
                side / bass
            );
            let mut state = StereoEnhance::new(sr);
            state.configure(&p);
            assert_eq!(
                state.next([0.25, 0.25], 1.0),
                [0.25, 0.25],
                "The direct mid must appear without a sample delay"
            );
        }
    }
    #[test]
    fn disabled_legacy_mode_matches_original_width_and_side_cuts_preserve_mid() {
        let legacy: AudioFxConfig =
            serde_json::from_value(serde_json::json!({"kind":"StereoEnhance"})).unwrap();
        assert!(!legacy.enhance_mono);
        assert_eq!(legacy.enhance_low_cut_hz, 0.0);
        assert_eq!(legacy.enhance_high_cut_hz, 0.0);
        let mut state = StereoEnhance::new(48000.0);
        state.configure(&legacy);
        for i in 0..4000 {
            let input = [
                (i as f32 * 0.05).sin() * 0.2,
                (i as f32 * 0.11).cos() * 0.15,
            ];
            let mid = (input[0] + input[1]) * 0.5;
            let side = (input[0] - input[1]) * 0.5 * 1.7;
            assert_eq!(state.next(input, 1.7), [mid + side, mid - side]);
        }
        for p in [
            AudioFxConfig {
                enhance_low_cut_hz: 4000.0,
                ..AudioFxConfig::new(AudioFxKind::StereoEnhance)
            },
            AudioFxConfig {
                enhance_high_cut_hz: 200.0,
                ..AudioFxConfig::new(AudioFxKind::StereoEnhance)
            },
        ] {
            let (_, filtered) = measure(48000.0, 1400.0, p);
            let (_, plain) = measure(
                48000.0,
                1400.0,
                AudioFxConfig::new(AudioFxKind::StereoEnhance),
            );
            assert!(filtered < plain * 0.05);
        }
    }
    #[test]
    fn automation_reset_and_roundtrip_are_bounded_and_allocation_free() {
        let mut state = StereoEnhance::new(48000.0);
        let p = AudioFxConfig::new(AudioFxKind::StereoEnhance);
        let mut energy = 0.0;
        let count = crate::test_alloc::count(|| {
            for i in 0..48000 {
                if i % 511 == 0 {
                    let p = AudioFxConfig {
                        enhance_mono: i % 3 == 0,
                        enhance_amount: (i % 7) as f32 / 6.0,
                        enhance_low_cut_hz: if i % 2 == 0 { 300.0 } else { 0.0 },
                        enhance_high_cut_hz: if i % 3 == 0 { 9000.0 } else { 0.0 },
                        ..p
                    };
                    state.configure(&p);
                }
                if i % 10001 == 0 {
                    state.reset();
                }
                let out = state.next([(i as f32 * 0.173).sin() * 0.3; 2], 2.0);
                assert!(out.iter().all(|v| v.is_finite() && v.abs() < 2.0));
                energy += out[0].abs();
            }
        });
        assert_eq!(count, 0);
        assert!(energy > 10.0);
        let bounded = AudioFxConfig {
            enhance_amount: f32::NAN,
            enhance_low_cut_hz: 50000.0,
            enhance_high_cut_hz: -100.0,
            ..p
        }
        .sanitized();
        assert_eq!(bounded.enhance_amount, 0.5);
        assert_eq!(bounded.enhance_low_cut_hz, 12500.0);
        assert_eq!(bounded.enhance_high_cut_hz, 0.0);
        let restored: AudioFxConfig =
            serde_json::from_str(&serde_json::to_string(&bounded).unwrap()).unwrap();
        assert_eq!(bounded, restored);
    }
    #[test]
    fn dedicated_enhance_parameters_survive_project_and_sound_presets() {
        use crate::{
            config::{AppConfig, FxKind, InputFx, TrackFx, TrackFxKind},
            presets::{self, FxTarget},
        };
        let mut config = AppConfig::new(120, 0, 5);
        let p = AudioFxConfig {
            enhance_amount: 0.73,
            enhance_low_cut_hz: 320.0,
            enhance_high_cut_hz: 7100.0,
            width: 1.4,
            ..AudioFxConfig::new(AudioFxKind::StereoEnhance)
        };
        config
            .input_fx
            .set_slot_kind(0, 0, FxKind::Audio(AudioFxKind::StereoEnhance));
        config
            .track_fx
            .set_slot_kind(0, 0, TrackFxKind::Audio(AudioFxKind::StereoEnhance));
        config.input_fx.banks[0].slots[0].fx = Some(InputFx::Audio(p));
        config.track_fx.banks[0].slots[0].fx = Some(TrackFx::Audio(p));
        let input = presets::encode(&config, FxTarget::Input { bank: 0, slot: 0 }).unwrap();
        let track = presets::encode(&config, FxTarget::Track { bank: 0, slot: 0 }).unwrap();
        let mut restored = AppConfig::new(120, 0, 5);
        crate::project::apply_data_to_config(
            &mut restored,
            serde_json::from_str(
                &serde_json::to_string(&crate::project::data_from_config(&config)).unwrap(),
            )
            .unwrap(),
        );
        presets::decode(&mut restored, FxTarget::Input { bank: 1, slot: 2 }, &input).unwrap();
        presets::decode(&mut restored, FxTarget::Track { bank: 2, slot: 3 }, &track).unwrap();
        for (bank, slot) in [(0, 0), (1, 2)] {
            let Some(InputFx::Audio(actual)) = &restored.input_fx.banks[bank].slots[slot].fx else {
                panic!()
            };
            assert_eq!(*actual, p);
        }
        for (bank, slot) in [(0, 0), (2, 3)] {
            let Some(TrackFx::Audio(actual)) = &restored.track_fx.banks[bank].slots[slot].fx else {
                panic!()
            };
            assert_eq!(*actual, p);
        }
    }
    #[test]
    fn rack_mix_keeps_mid_dry_bypass_is_exact_and_reset_removes_tail() {
        use crate::dsp::audio_fx::{AudioFxParams, AudioFxState};
        let p = AudioFxParams::new(&AudioFxConfig::new(AudioFxKind::StereoEnhance));
        let bypass = AudioFxParams::new(&AudioFxConfig {
            mix: 0.0,
            ..p.config
        });
        assert_eq!(p.latency_frames(48000.0), 0);
        let mut wet = AudioFxState::new(48000.0);
        let mut dry = AudioFxState::new(48000.0);
        let mut side_energy = 0.0;
        let allocations = crate::test_alloc::count(|| {
            for i in 0..12000 {
                let input = (
                    (i as f32 * 0.117).sin() * 0.25,
                    (i as f32 * 0.093).cos() * 0.17,
                );
                let out = wet.process(&p, 120, 0.0, false, input);
                let unchanged = dry.process(&bypass, 120, 0.0, false, input);
                assert_eq!(unchanged, input);
                assert!(((out.0 + out.1) - (input.0 + input.1)).abs() < 1e-6);
                side_energy += (out.0 - input.0).abs();
            }
        });
        assert_eq!(allocations, 0);
        assert!(side_energy > 1.0);
        let allocations = crate::test_alloc::count(|| {
            wet.reset();
            for _ in 0..1000 {
                assert_eq!(wet.process(&p, 120, 0.0, false, (0.0, 0.0)), (0.0, 0.0));
            }
        });
        assert_eq!(allocations, 0);
    }
}
