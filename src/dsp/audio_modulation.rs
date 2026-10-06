//! Shared phase retrigger and sample-and-hold for rack modulation effects.
use crate::config::audio_fx::AudioFxConfig;
#[derive(Clone, Default)]
pub struct ModulationState {
    frames: u64,
    held: Option<f32>,
    index: u64,
    step_key: Option<(u32, u32, bool)>,
    origin: Option<f64>,
    active: bool,
    retrigger: bool,
    previous_elapsed: f64,
}
impl ModulationState {
    /// Renderer 12+: synchronized motion samples the musical grid itself,
    /// rather than whichever sample happened to enable this processor. Free-Hz
    /// LFO motion continues to use phase() and its historical local clock.
    pub fn synced_phase(
        &mut self,
        p: &AudioFxConfig,
        elapsed: f64,
        bpm: usize,
        sr: f32,
        shape_controls: bool,
    ) -> f32 {
        let restart = shape_controls && p.mod_retrigger;
        if self.origin.is_none()
            || (!self.retrigger && restart)
            || !self.active
            || elapsed < self.previous_elapsed
        {
            self.origin = Some(elapsed);
        }
        self.retrigger = restart;
        self.active = true;
        self.previous_elapsed = elapsed;
        let rate = sr.max(1.0).round() as f64;
        let now = (elapsed.max(0.0) * rate).round();
        let origin = if restart {
            (self.origin.unwrap_or(0.0) * rate).round()
        } else {
            0.0
        };
        let frame = (now - origin).max(0.0);
        let sample_frame = if p.mod_stepped {
            // Use the first representable source sample of the step. Both a
            // continuously running instance and one enabled halfway through a
            // step therefore see the same held value, including Hz step grids.
            let period = if p.mod_step_beats > 0.0 {
                rate * 60.0 * p.mod_step_beats as f64 / bpm.max(1) as f64
            } else {
                rate / p.mod_step_hz.max(0.1) as f64
            };
            ((frame / period).floor() * period).ceil().min(frame)
        } else {
            frame
        };
        let mut phase = (sample_frame * bpm.max(1) as f64
            / (rate * 60.0 * p.sync_beats.max(f32::MIN_POSITIVE) as f64))
            .fract() as f32;
        if shape_controls {
            phase = (phase + p.mod_phase_degrees / 360.0).rem_euclid(1.0);
        }
        self.held = None;
        self.step_key = None;
        self.frames = self.frames.wrapping_add(1);
        phase
    }
    pub fn phase(
        &mut self,
        p: &AudioFxConfig,
        raw: f32,
        freq: f32,
        elapsed: f64,
        active: bool,
        bpm: usize,
        sr: f32,
        shape_controls: bool,
    ) -> f32 {
        let restart = shape_controls && p.mod_retrigger;
        if self.origin.is_none()
            || (!self.retrigger && restart)
            || (!self.active && active)
            || elapsed < self.previous_elapsed
        {
            self.origin = Some(elapsed);
            if restart {
                self.held = None;
            }
        }
        let source = if restart && p.sync_beats > 0.0 && active {
            ((elapsed - self.origin.unwrap_or(elapsed)).max(0.0) * freq as f64).fract() as f32
        } else {
            raw
        };
        self.retrigger = restart;
        self.active = active;
        self.previous_elapsed = elapsed;
        let key = (p.mod_step_hz.to_bits(), p.mod_step_beats.to_bits(), active);
        let mut phase = source;
        if p.mod_stepped {
            let position = if p.mod_step_beats > 0.0 && active {
                let origin = if restart {
                    self.origin.unwrap_or(0.0)
                } else {
                    0.0
                };
                (elapsed - origin).max(0.0) * bpm.max(1) as f64 / (60.0 * p.mod_step_beats as f64)
            } else {
                self.frames as f64 * p.mod_step_hz as f64 / sr.max(1.0) as f64
            };
            let index = position.floor() as u64;
            if self.held.is_none() || self.index != index || self.step_key != Some(key) {
                self.held = Some(source);
                self.index = index;
                self.step_key = Some(key);
            }
            phase = self.held.unwrap_or(source);
        } else {
            self.held = None;
            self.step_key = None;
        }
        self.frames = self.frames.wrapping_add(1);
        if shape_controls && p.mod_phase_degrees != 0.0 {
            phase = (phase + p.mod_phase_degrees / 360.0).rem_euclid(1.0);
        }
        phase
    }
}
/// Shared integer-source-frame phase for synchronized effects without an
/// explicit retrigger/step controller (for example Step Slicer).
pub fn synced_cycle(elapsed: f64, bpm: usize, sr: f32, beats: f32) -> f32 {
    let rate = sr.max(1.0).round() as f64;
    let frame = (elapsed.max(0.0) * rate).round();
    (frame * bpm.max(1) as f64 / (rate * 60.0 * beats.max(f32::MIN_POSITIVE) as f64)).fract() as f32
}
/// Higher sharpness steepens the center crossings continuously; zero is exactly
/// the historical sine without a changed floating-point evaluation order.
pub fn wave(phase: f32, sharpness: f32) -> f32 {
    let sine = (std::f32::consts::TAU * phase).sin();
    if sharpness <= 0.0 {
        sine
    } else {
        let amount = sharpness.clamp(0.0, 1.0) * 0.95;
        sine / (1.0 - amount + amount * sine.abs())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn synced_steps_join_the_same_grid_when_enabled_mid_step_or_reopened() {
        for sr in [8000.0, 44100.0, 48000.0] {
            for beat_step in [0.0, 0.25, 0.375] {
                let p = AudioFxConfig {
                    sync_beats: 1.5,
                    mod_stepped: true,
                    mod_step_beats: beat_step,
                    mod_step_hz: 3.7,
                    mod_retrigger: false,
                    ..Default::default()
                };
                let mut reference = ModulationState::default();
                let mut late = ModulationState::default();
                let mut reopened = ModulationState::default();
                let count = crate::test_alloc::count(|| {
                    for frame in 0..10000 {
                        let elapsed = frame as f64 / sr as f64;
                        let expected = reference.synced_phase(&p, elapsed, 137, sr, true);
                        if frame >= 1373 {
                            assert_eq!(
                                late.synced_phase(&p, elapsed, 137, sr, true),
                                expected,
                                "sr{sr}, step{beat_step}, frame{frame}"
                            );
                        }
                        if frame == 3401 {
                            reopened = ModulationState::default();
                        }
                        if frame < 2101 || frame >= 4703 {
                            assert_eq!(reopened.synced_phase(&p, elapsed, 137, sr, true), expected);
                        }
                    }
                });
                assert_eq!(count, 0);
            }
        }
    }
    #[test]
    fn explicit_retrigger_retains_its_enable_origin_and_legacy_capture_is_unchanged() {
        let mut p = AudioFxConfig {
            sync_beats: 1.0,
            mod_stepped: true,
            mod_step_beats: 0.25,
            mod_retrigger: false,
            ..Default::default()
        };
        let sr = 8000.0;
        let elapsed = 1373.0 / sr as f64;
        let mut old = ModulationState::default();
        let mut late_old = ModulationState::default();
        let mut old_value = 0.0;
        for n in 0..=1373 {
            let t = n as f64 / sr as f64;
            old_value = old.phase(&p, (t * 2.0).fract() as f32, 2.0, t, true, 120, sr, true);
        }
        let late_value = late_old.phase(
            &p,
            (elapsed * 2.0).fract() as f32,
            2.0,
            elapsed,
            true,
            120,
            sr,
            true,
        );
        assert_eq!(old_value, 0.25);
        assert!(
            (late_value - 0.34325).abs() < 1e-6,
            "Document the old per-enable capture"
        );
        let mut common = ModulationState::default();
        assert_eq!(common.synced_phase(&p, elapsed, 120, sr, true), old_value);
        p.mod_retrigger = true;
        let mut early = ModulationState::default();
        early.synced_phase(&p, 0.0, 120, sr, true);
        let mut late = ModulationState::default();
        assert_eq!(late.synced_phase(&p, elapsed, 120, sr, true), 0.0);
        assert_eq!(early.synced_phase(&p, elapsed, 120, sr, true), 0.25);
        // Transport restart is observed at the source timeline, not wall time.
        late.phase(&p, 0.0, 2.0, 0.0, false, 120, sr, true);
        assert_eq!(late.synced_phase(&p, 0.0, 120, sr, true), 0.0);
    }
    #[test]
    fn zero_shape_and_disabled_steps_preserve_exact_old_phase_and_wave() {
        let p = AudioFxConfig::default();
        let mut s = ModulationState::default();
        for n in 0..2000 {
            let phase = (n as f32 * 0.00137).fract();
            let got = s.phase(&p, phase, 1.0, n as f64 / 48000.0, true, 120, 48000.0, true);
            assert_eq!(got.to_bits(), phase.to_bits());
            assert_eq!(
                wave(got, 0.0).to_bits(),
                (std::f32::consts::TAU * phase).sin().to_bits()
            );
        }
    }
    #[test]
    fn shape_is_bipolar_bounded_and_phase_controls_reverse_the_motion() {
        assert!(wave(0.01, 1.0) > wave(0.01, 0.0) * 5.0);
        for n in 0..1000 {
            let phase = n as f32 / 1000.0;
            let value = wave(phase, 1.0);
            assert!(value.abs() <= 1.0);
            assert!((value + wave((phase + 0.5) % 1.0, 1.0)).abs() < 0.0001);
        }
        let p = AudioFxConfig {
            mod_phase_degrees: 90.0,
            ..Default::default()
        };
        let mut s = ModulationState::default();
        assert_eq!(s.phase(&p, 0.0, 1.0, 0.0, true, 120, 48000.0, true), 0.25);
    }
    #[test]
    fn stepped_phase_holds_and_retrigger_uses_the_effect_enable_origin() {
        let mut p = AudioFxConfig {
            mod_stepped: true,
            mod_step_hz: 4.0,
            ..Default::default()
        };
        let mut s = ModulationState::default();
        for n in 0..500 {
            let got = s.phase(
                &p,
                n as f32 / 1000.0,
                1.0,
                n as f64 / 1000.0,
                false,
                120,
                1000.0,
                true,
            );
            assert_eq!(got, if n < 250 { 0.0 } else { 0.25 });
        }
        p.mod_stepped = false;
        p.mod_retrigger = true;
        p.sync_beats = 2.0;
        p.mod_phase_degrees = 90.0;
        s = ModulationState::default();
        assert_eq!(
            s.phase(&p, 0.75, 1.0, 17.75, true, 120, 48000.0, true),
            0.25
        );
        assert_eq!(s.phase(&p, 0.0, 1.0, 18.0, true, 120, 48000.0, true), 0.5);
    }
}
