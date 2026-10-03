//! Post-fader main bus. Recording inputs and track buffers remain dry with respect
//! to master processors; replay exports pass through this exact same bus.
use super::{
    audio_fx::AudioFxParams,
    dynamics::DynamicsState,
    reverb::{ReverbDspState, ReverbParams},
};
use crate::config::audio_fx::{AudioFxKind, MasterFxConfig};

#[derive(Clone, Copy)]
pub struct MasterFxRuntime {
    compressor_enabled: bool,
    reverb_enabled: bool,
    compressor: AudioFxParams,
    reverb: AudioFxParams,
}
impl MasterFxRuntime {
    pub fn from_config(c: &MasterFxConfig) -> Self {
        let mut compressor = c.compressor;
        compressor.kind = AudioFxKind::Dynamics;
        let mut reverb = c.reverb;
        reverb.kind = AudioFxKind::Reverb;
        Self {
            compressor_enabled: c.compressor_enabled,
            reverb_enabled: c.reverb_enabled,
            compressor: AudioFxParams::new(&compressor),
            reverb: AudioFxParams::new(&reverb),
        }
    }
}
pub struct MasterFxState {
    runtime: MasterFxRuntime,
    dynamics: DynamicsState,
    reverb: ReverbDspState,
    sr: f32,
    compressor_mix: f32,
    reverb_mix: f32,
    smooth: f32,
    output_gain: f32,
}
impl MasterFxState {
    pub fn new(sr: f32) -> Self {
        let mut reverb = ReverbDspState::new();
        reverb.prepare(sr);
        Self {
            runtime: MasterFxRuntime::from_config(&MasterFxConfig::default()),
            dynamics: DynamicsState::default(),
            reverb,
            sr,
            compressor_mix: 0.0,
            reverb_mix: 0.0,
            smooth: 1.0 - (-1.0 / (sr * 0.005)).exp(),
            output_gain: 1.0,
        }
    }
    pub fn configure(&mut self, r: MasterFxRuntime) {
        if r.compressor_enabled && !self.runtime.compressor_enabled {
            self.dynamics.reset();
        }
        if r.reverb_enabled && !self.runtime.reverb_enabled {
            self.reverb.reset();
        }
        self.output_gain = 10.0_f32.powf(r.reverb.config.level_db / 20.0);
        self.runtime = r;
    }
    pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        let mut out = input;
        self.compressor_mix += ((if self.runtime.compressor_enabled {
            1.0
        } else {
            0.0
        }) - self.compressor_mix)
            * self.smooth;
        self.reverb_mix += ((if self.runtime.reverb_enabled {
            1.0
        } else {
            0.0
        }) - self.reverb_mix)
            * self.smooth;
        if self.compressor_mix > 1e-6 {
            let wet = self
                .dynamics
                .process(&self.runtime.compressor, self.sr, out);
            for ch in 0..2 {
                out[ch] += (wet[ch] - out[ch]) * self.compressor_mix;
            }
        }
        if self.reverb_mix > 1e-6 {
            let p = &self.runtime.reverb.config;
            let wet = super::reverb::process_sample(
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
                out[0],
                out[1],
            );
            let processed = [
                (out[0] * (1.0 - p.mix) + wet.0 * p.mix) * self.output_gain,
                (out[1] * (1.0 - p.mix) + wet.1 * p.mix) * self.output_gain,
            ];
            for ch in 0..2 {
                out[ch] += (processed[ch] - out[ch]) * self.reverb_mix;
            }
        }
        out
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bypass_is_exact_and_master_compression_does_not_allocate() {
        let mut state = MasterFxState::new(8000.0);
        assert_eq!(state.process([0.7, -0.2]), [0.7, -0.2]);
        let mut c = MasterFxConfig::default();
        c.compressor_enabled = true;
        c.compressor.threshold_db = -24.0;
        c.compressor.ratio = 10.0;
        c.compressor.attack_ms = 0.1;
        state.configure(MasterFxRuntime::from_config(&c));
        let mut out = [0.0; 2];
        let n = crate::test_alloc::count(|| {
            for _ in 0..8000 {
                out = state.process([0.8, 0.4]);
            }
        });
        assert_eq!(n, 0);
        assert!(out[0] < 0.1);
        assert!((out[0] / out[1] - 2.0).abs() < 1e-5);
    }
}
