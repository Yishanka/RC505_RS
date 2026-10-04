//! Stereo-linked, zero-lookahead feed-forward dynamics used by master and racks.
use super::audio_fx::AudioFxParams;
use crate::config::audio_fx::DynamicsMode;
#[derive(Clone)]
pub struct DynamicsState {
    profiles: super::dynamics_profiles::ProfileDynamics,
    signature: u64,
    sr: f32,
    attack: f32,
    release: f32,
    makeup: f32,
    ceiling: f32,
    reduction: f32,
    gate: f32,
}
impl Default for DynamicsState {
    fn default() -> Self {
        Self {
            profiles: Default::default(),
            signature: 0,
            sr: 0.0,
            attack: 0.0,
            release: 0.0,
            makeup: 1.0,
            ceiling: 1.0,
            reduction: 0.0,
            gate: 0.0,
        }
    }
}
impl DynamicsState {
    pub fn reset(&mut self) {
        self.profiles.reset();
        self.reduction = 0.0;
        self.gate = 0.0;
    }
    pub fn process(&mut self, r: &AudioFxParams, sr: f32, x: [f32; 2]) -> [f32; 2] {
        let p = &r.config;
        if self.signature != r.signature || self.sr != sr {
            self.signature = r.signature;
            self.sr = sr;
            self.attack = 1.0 - (-1.0 / (sr * p.attack_ms * 0.001)).exp();
            self.release = 1.0 - (-1.0 / (sr * p.release_ms * 0.001)).exp();
            self.makeup = 10.0_f32.powf((p.makeup_db + p.level_db) / 20.0);
            self.ceiling = 10.0_f32.powf(p.threshold_db / 20.0) * self.makeup;
            self.profiles.configure(p, sr);
        }
        if p.kind == crate::config::audio_fx::AudioFxKind::Dynamics
            && p.dynamics_profile != crate::config::dynamics_profiles::DynamicsProfile::Custom
        {
            return self.profiles.process(x, p.mix);
        }
        let peak = x[0].abs().max(x[1].abs()).max(1e-9);
        let db = 20.0 * peak.log10();
        let wet = if p.dynamics_mode == DynamicsMode::Gate {
            let target = if db >= p.threshold_db { 1.0 } else { 0.0 };
            self.gate += (target - self.gate)
                * if target > self.gate {
                    self.attack
                } else {
                    self.release
                };
            x.map(|v| v * self.gate * self.makeup)
        } else {
            let ratio = if p.dynamics_mode == DynamicsMode::Limiter {
                1000.0
            } else {
                p.ratio
            };
            let over = db - p.threshold_db;
            let knee = p.knee_db;
            let reduction = if over < -knee * 0.5 {
                0.0
            } else if knee > 0.0 && over < knee * 0.5 {
                (1.0 - 1.0 / ratio) * (over + knee * 0.5).powi(2) / (2.0 * knee)
            } else {
                (1.0 - 1.0 / ratio) * over.max(0.0)
            };
            self.reduction += (reduction - self.reduction)
                * if reduction > self.reduction {
                    self.attack
                } else {
                    self.release
                };
            let gain = 10.0_f32.powf(-self.reduction / 20.0) * self.makeup;
            x.map(|v| {
                if p.dynamics_mode == DynamicsMode::Limiter {
                    (v * gain).clamp(-self.ceiling, self.ceiling)
                } else {
                    v * gain
                }
            })
        };
        [
            x[0] * (1.0 - p.mix) + wet[0] * p.mix,
            x[1] * (1.0 - p.mix) + wet[1] * p.mix,
        ]
    }
}
