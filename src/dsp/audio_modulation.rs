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
