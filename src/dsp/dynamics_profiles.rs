//! Named, inspectable software recipes. This processor is separate from the
//! legacy custom compressor so old stored sounds remain sample-for-sample.
use super::biquad::{self, Biquad, Coeff};
use crate::config::{
    audio_fx::AudioFxConfig,
    dynamics_profiles::{DynamicsProfile, Recipe},
};
#[derive(Clone)]
struct ProfileVoice {
    profile: DynamicsProfile,
    recipe: Recipe,
    coeff: [Coeff; 6],
    filters: [[Biquad; 6]; 2],
    power: f32,
    reduction: f32,
    attack: f32,
    release: f32,
    rms_rate: f32,
    makeup: f32,
    makeup_target: f32,
    ceiling: f32,
    smooth: f32,
    initialized: bool,
}
impl Default for ProfileVoice {
    fn default() -> Self {
        Self {
            profile: DynamicsProfile::Custom,
            recipe: DynamicsProfile::NaturalComp.recipe(0.0),
            coeff: [Coeff::default(); 6],
            filters: [[Biquad::default(); 6]; 2],
            power: 0.0,
            reduction: 0.0,
            attack: 1.0,
            release: 1.0,
            rms_rate: 1.0,
            makeup: 1.0,
            makeup_target: 1.0,
            ceiling: 1.0,
            smooth: 1.0,
            initialized: false,
        }
    }
}
impl ProfileVoice {
    pub fn reset(&mut self) {
        self.filters = [[Biquad::default(); 6]; 2];
        self.power = 0.0;
        self.reduction = 0.0;
        self.initialized = false;
    }
    pub fn configure(&mut self, p: &AudioFxConfig, sr: f32) {
        let r = p.dynamics_profile.recipe(p.dynamics_amount);
        if self.profile != p.dynamics_profile {
            self.filters = [[Biquad::default(); 6]; 2];
            self.power = 0.0;
        }
        self.profile = p.dynamics_profile;
        self.recipe = r;
        self.attack = 1.0 - (-1.0 / (sr * r.attack * 0.001)).exp();
        self.release = 1.0 - (-1.0 / (sr * r.release * 0.001)).exp();
        self.rms_rate = 1.0 - (-1.0 / (sr * 0.01)).exp();
        self.smooth = 1.0 - (-1.0 / (sr * 0.005)).exp();
        self.makeup_target = 10.0f32.powf((r.makeup + p.level_db) / 20.0);
        self.ceiling = 10.0f32.powf(r.threshold / 20.0) * self.makeup_target;
        if !self.initialized {
            self.makeup = self.makeup_target;
            self.initialized = true;
        }
        self.coeff = [
            if r.sidechain_hz > 0.0 {
                biquad::highpass(sr, r.sidechain_hz, 0.707)
            } else {
                Coeff::default()
            },
            if r.highpass_hz > 0.0 {
                biquad::highpass(sr, r.highpass_hz, 0.707)
            } else {
                Coeff::default()
            },
            if r.lowpass_hz > 0.0 {
                biquad::lowpass(sr, r.lowpass_hz.min(sr * 0.45), 0.707)
            } else {
                Coeff::default()
            },
            biquad::shelf(sr, 120.0, r.low_db, false),
            biquad::shelf(sr, 6000.0, r.high_db, true),
            biquad::peak(sr, 2000.0, 0.8, r.presence_db),
        ];
    }
    pub fn process(&mut self, x: [f32; 2], mix: f32) -> [f32; 2] {
        let r = self.recipe;
        let mut y = x;
        let mut peak = 0.0f32;
        for ch in 0..2 {
            let f = &mut self.filters[ch];
            y[ch] = f[1].next(y[ch], self.coeff[1]);
            y[ch] = f[2].next(y[ch], self.coeff[2]);
            peak = peak.max(f[0].next(y[ch], self.coeff[0]).abs());
        }
        self.power += (peak * peak - self.power) * self.rms_rate;
        let detected = if r.rms {
            self.power.max(0.0).sqrt()
        } else {
            peak
        };
        let db = 20.0 * detected.max(1e-9).log10();
        let desired = reduction_db(db, r);
        self.reduction += (desired - self.reduction)
            * if desired > self.reduction {
                self.attack
            } else {
                self.release
            };
        self.makeup += (self.makeup_target - self.makeup) * self.smooth;
        let gain = 10.0f32.powf(-self.reduction / 20.0) * self.makeup;
        for ch in 0..2 {
            y[ch] *= gain;
            for band in 3..6 {
                y[ch] = self.filters[ch][band].next(y[ch], self.coeff[band]);
            }
            if r.limiter {
                y[ch] = y[ch].clamp(-self.ceiling, self.ceiling);
            }
            y[ch] = super::headroom(x[ch] * (1.0 - mix) + y[ch] * mix);
        }
        y
    }
}
#[derive(Clone, Default)]
pub struct ProfileDynamics {
    current: ProfileVoice,
    previous: ProfileVoice,
    fade: f32,
    step: f32,
    active: bool,
}
impl ProfileDynamics {
    pub fn reset(&mut self) {
        self.current.reset();
        self.previous.reset();
        self.active = false;
        self.fade = 1.0;
    }
    pub fn configure(&mut self, p: &AudioFxConfig, sr: f32) {
        self.step = 1.0 / (sr * 0.005).max(1.0);
        if p.dynamics_profile == DynamicsProfile::Custom {
            self.active = false;
            return;
        }
        if self.active && self.current.profile != p.dynamics_profile {
            self.previous = self.current.clone();
            self.fade = 0.0;
        } else if !self.active {
            self.current.reset();
            self.fade = 1.0;
        }
        self.current.configure(p, sr);
        self.active = true;
    }
    pub fn process(&mut self, x: [f32; 2], mix: f32) -> [f32; 2] {
        let mut y = self.current.process(x, mix);
        if self.fade < 1.0 {
            let old = self.previous.process(x, mix);
            self.fade = (self.fade + self.step).min(1.0);
            for ch in 0..2 {
                y[ch] = old[ch] + (y[ch] - old[ch]) * self.fade;
            }
        }
        y
    }
}
pub fn reduction_db(input_db: f32, r: Recipe) -> f32 {
    let over = input_db - r.threshold;
    let slope = 1.0 - 1.0 / r.ratio;
    if over < -r.knee * 0.5 {
        0.0
    } else if r.knee > 0.0 && over < r.knee * 0.5 {
        slope * (over + r.knee * 0.5).powi(2) / (2.0 * r.knee)
    } else {
        slope * over.max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::audio_fx::{AudioFxConfig, AudioFxKind};
    fn signal(profile: DynamicsProfile, amount: f32, hz: f32, amp: f32) -> Vec<f32> {
        let mut p = AudioFxConfig::new(AudioFxKind::Dynamics);
        p.dynamics_profile = profile;
        p.dynamics_amount = amount;
        let mut d = ProfileDynamics::default();
        d.configure(&p, 48000.0);
        (0..24000)
            .map(|n| {
                let env = if n % 6000 < 2000 {
                    0.15
                } else if n % 6000 < 4000 {
                    1.0
                } else {
                    0.4
                };
                let x = amp * env * (std::f32::consts::TAU * hz * n as f32 / 48000.0).sin();
                d.process([x, -x * 0.5], 1.0)[0]
            })
            .collect()
    }
    fn energy(x: &[f32]) -> f64 {
        x.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>() / x.len() as f64
    }
    #[test]
    fn all_nineteen_recipes_have_distinct_audio_not_just_names() {
        let sounds = DynamicsProfile::ALL[1..]
            .iter()
            .map(|p| signal(*p, 0.0, 440.0, 0.8))
            .collect::<Vec<_>>();
        for a in 0..sounds.len() {
            for b in a + 1..sounds.len() {
                let diff = sounds[a]
                    .iter()
                    .zip(&sounds[b])
                    .map(|(a, b)| f64::from(a - b).powi(2))
                    .sum::<f64>()
                    / sounds[a].len() as f64;
                assert!(diff > 1e-7, "{a} and {b}: {diff}");
            }
        }
    }
    #[test]
    fn amount_increases_reduction_and_curves_have_the_declared_slope() {
        for profile in &DynamicsProfile::ALL[1..] {
            let less = profile.recipe(-20.0);
            let normal = profile.recipe(0.0);
            let more = profile.recipe(20.0);
            for db in [-30.0, -20.0, -10.0, 0.0] {
                assert!(reduction_db(db, less) <= reduction_db(db, normal) + 1e-6);
                assert!(reduction_db(db, normal) <= reduction_db(db, more) + 1e-6);
            }
            let start = normal.threshold + normal.knee * 0.5 + 1.0;
            let gain_change = reduction_db(start + 6.0, normal) - reduction_db(start, normal);
            assert!((6.0 - gain_change - 6.0 / normal.ratio).abs() < 1e-5);
            let a = signal(*profile, -20.0, 440.0, 0.8);
            let b = signal(*profile, 20.0, 440.0, 0.8);
            assert!(energy(&b) < energy(&a), "{profile:?}");
        }
    }
    #[test]
    fn recipes_have_real_phone_bandwidth_low_boost_and_brightening() {
        // Use stationary tones: amplitude steps themselves produce broadband
        // energy and must not be mistaken for leakage of the test frequency.
        let e = |profile, hz: f32| {
            let mut p = AudioFxConfig::new(AudioFxKind::Dynamics);
            p.dynamics_profile = profile;
            let mut d = ProfileDynamics::default();
            d.configure(&p, 48000.0);
            let mut power = 0.0f64;
            for n in 0..24000 {
                let x = 0.0001 * (std::f32::consts::TAU * hz * n as f32 / 48000.0).sin();
                let y = d.process([x; 2], 1.0);
                if n >= 12000 {
                    power += f64::from(y[0]).powi(2);
                }
            }
            power / 12000.0
        };
        let phone = e(DynamicsProfile::PhoneVox, 1000.0);
        assert!(e(DynamicsProfile::PhoneVox, 50.0) < phone * 0.005);
        assert!(e(DynamicsProfile::PhoneVox, 10000.0) < phone * 0.02);
        assert!(e(DynamicsProfile::LowBoost, 40.0) > e(DynamicsProfile::LowBoost, 5000.0) * 3.0);
        assert!(e(DynamicsProfile::Brighten, 12000.0) > e(DynamicsProfile::Brighten, 100.0) * 2.0);
    }
    #[test]
    fn rms_rejects_short_peaks_and_release_recovers() {
        let mut p = AudioFxConfig::new(AudioFxKind::Dynamics);
        p.dynamics_profile = DynamicsProfile::NaturalComp;
        let mut rms = ProfileVoice::default();
        rms.configure(&p, 48000.0);
        rms.recipe.sidechain_hz = 0.0;
        rms.coeff[0] = Coeff::default();
        let mut peak = rms.clone();
        peak.recipe.rms = false;
        for _ in 0..10 {
            rms.process([0.8; 2], 1.0);
            peak.process([0.8; 2], 1.0);
        }
        assert!(peak.reduction > rms.reduction * 2.0);
        for _ in 0..24000 {
            rms.process([0.8; 2], 1.0);
        }
        let reduction = rms.reduction;
        assert!(reduction > 3.0);
        for _ in 0..96000 {
            rms.process([0.0; 2], 1.0);
        }
        assert!(rms.reduction < reduction * 0.001);
    }
    #[test]
    fn all_profiles_automate_and_preserve_stereo_without_allocations() {
        for sr in [8000.0, 48000.0, 192000.0] {
            let mut d = ProfileDynamics::default();
            let mut p = AudioFxConfig::new(AudioFxKind::Dynamics);
            let count = crate::test_alloc::count(|| {
                for n in 0..60000 {
                    if n % 701 == 0 {
                        p.dynamics_profile = DynamicsProfile::ALL[1 + n / 701 % 19];
                        p.dynamics_amount = if n % 2 == 0 { -20.0 } else { 20.0 };
                        d.configure(&p, sr);
                    }
                    let x = (n as f32 * 0.07).sin() * 4.0;
                    let y = d.process([x, -x * 0.5], 1.0);
                    assert!(y.iter().all(|x| x.is_finite() && x.abs() <= 16.0));
                    if !d.current.recipe.limiter && d.fade == 1.0 && y[0].abs() < 15.0 {
                        assert!((y[0] * 0.5 + y[1]).abs() < 0.0001);
                    }
                }
            });
            assert_eq!(count, 0);
        }
    }
}
