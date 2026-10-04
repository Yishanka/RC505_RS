//! Two independent prepared LFOs; target-domain composition is bounded and allocation-free.
use crate::config::osc_configs::{LfoConfig, LfoMode, LfoShape, LfoTarget};
use std::sync::Arc;
#[derive(Clone)]
pub struct PreparedLfo {
    pub enabled: bool,
    pub target: LfoTarget,
    pub mode: LfoMode,
    pub sync: bool,
    pub rate_hz: f32,
    pub beats: f32,
    pub depth: f32,
    pub table: Arc<[f32; 257]>,
}
impl PreparedLfo {
    pub fn from_config(c: &LfoConfig) -> Self {
        let mut c = c.clone();
        c.sanitize();
        let table = std::array::from_fn(|i| {
            let wave = lfo_value(&c, i as f32 / 256.0);
            match c.target {
                LfoTarget::Volume => wave,
                LfoTarget::Pitch => 2.0f32.powf((wave * 2.0 - 1.0) * c.depth),
                LfoTarget::Cutoff => 2.0f32.powf((wave * 2.0 - 1.0) * c.depth * 4.0),
            }
        });
        Self {
            enabled: c.enabled,
            target: c.target,
            mode: c.mode,
            sync: c.sync,
            rate_hz: c.rate_hz,
            beats: c.beats,
            depth: c.depth,
            table: Arc::new(table),
        }
    }
    pub fn increment(&self, bpm: usize, sr: f32) -> f64 {
        let rate = if self.sync {
            bpm as f64 / (60.0 * self.beats as f64)
        } else {
            self.rate_hz as f64
        };
        rate / sr as f64
    }
    pub fn value(&self, phase: f64) -> f32 {
        let p = phase.clamp(0.0, 1.0) as f32 * 256.0;
        let i = (p as usize).min(255);
        let t = p - i as f32;
        self.table[i] * (1.0 - t) + self.table[i + 1] * t
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Modulation {
    pub volume: f32,
    pub pitch: f32,
    pub cutoff: f32,
}
impl Default for Modulation {
    fn default() -> Self {
        Self {
            volume: 1.0,
            pitch: 1.0,
            cutoff: 1.0,
        }
    }
}
pub fn compose(lfos: &[PreparedLfo; 2], values: [f32; 2]) -> Modulation {
    let mut result = Modulation::default();
    for (lfo, value) in lfos.iter().zip(values) {
        if lfo.enabled {
            match lfo.target {
                LfoTarget::Volume => result.volume *= (1.0 - lfo.depth) + lfo.depth * value,
                LfoTarget::Pitch => result.pitch *= value,
                LfoTarget::Cutoff => result.cutoff *= value,
            }
        }
    }
    result
}
pub fn lfo_value(c: &LfoConfig, phase: f32) -> f32 {
    match c.shape {
        LfoShape::Sine => 0.5 - 0.5 * (phase * std::f32::consts::TAU).cos(),
        LfoShape::Triangle => 1.0 - (phase * 2.0 - 1.0).abs(),
        LfoShape::Saw => phase,
        LfoShape::Square => {
            if phase < 0.5 {
                1.0
            } else {
                0.0
            }
        }
        LfoShape::Custom => {
            let a = c
                .points
                .windows(2)
                .find(|p| phase <= p[1].x)
                .unwrap_or_else(|| &c.points[c.points.len() - 2..]);
            let t = ((phase - a[0].x) / (a[1].x - a[0].x).max(0.0001)).clamp(0.0, 1.0);
            let shape = crate::dsp::envelope::bend_curve(t, a[0].curve);
            a[0].y + (a[1].y - a[0].y) * shape
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn two_lfos_compose_in_target_domains_without_cross_talk() {
        let one = LfoConfig {
            enabled: true,
            shape: LfoShape::Square,
            target: LfoTarget::Pitch,
            depth: 1.0,
            ..Default::default()
        };
        let two = LfoConfig {
            enabled: true,
            shape: LfoShape::Square,
            target: LfoTarget::Cutoff,
            depth: 1.0,
            ..Default::default()
        };
        let lfos = [
            PreparedLfo::from_config(&one),
            PreparedLfo::from_config(&two),
        ];
        let mix = compose(&lfos, [lfos[0].value(0.25), lfos[1].value(0.75)]);
        assert_eq!(mix.pitch, 2.0);
        assert_eq!(mix.cutoff, 1.0 / 16.0);
        assert_eq!(mix.volume, 1.0);
        let lfos = [
            PreparedLfo::from_config(&LfoConfig {
                target: LfoTarget::Volume,
                depth: 0.5,
                ..one.clone()
            }),
            PreparedLfo::from_config(&LfoConfig {
                target: LfoTarget::Volume,
                depth: 0.25,
                ..two.clone()
            }),
        ];
        assert_eq!(compose(&lfos, [0.0, 0.0]).volume, 0.375);
        let lfos = [
            PreparedLfo::from_config(&one),
            PreparedLfo::from_config(&one),
        ];
        assert_eq!(compose(&lfos, [2.0, 2.0]).pitch, 4.0);
    }
    #[test]
    fn disabled_lfo_is_exactly_neutral_for_every_target() {
        for target in [LfoTarget::Volume, LfoTarget::Pitch, LfoTarget::Cutoff] {
            let one = PreparedLfo::from_config(&LfoConfig {
                enabled: true,
                target,
                depth: 0.37,
                ..Default::default()
            });
            let off = PreparedLfo::from_config(&LfoConfig {
                enabled: false,
                target,
                depth: 1.0,
                shape: LfoShape::Square,
                ..Default::default()
            });
            for phase in 0..1000 {
                let v = one.value(phase as f64 / 1000.0);
                let result = compose(&[one.clone(), off.clone()], [v, 200.0]);
                let expected = if target == LfoTarget::Volume {
                    (1.0 - one.depth) + one.depth * v
                } else {
                    v
                };
                let actual = match target {
                    LfoTarget::Volume => result.volume,
                    LfoTarget::Pitch => result.pitch,
                    LfoTarget::Cutoff => result.cutoff,
                };
                assert_eq!(actual.to_bits(), expected.to_bits());
            }
        }
    }
}
