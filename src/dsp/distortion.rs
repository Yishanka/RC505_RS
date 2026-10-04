//! Six distinct software voicings. Public BOSS names are control vocabulary;
//! the filters, nonlinearities and gain calibration here are our own design.
use super::biquad::{self, Biquad, Coeff};
use crate::config::audio_fx::{AudioFxConfig, DistortionType as T};

#[derive(Clone)]
struct Path {
    kind: T,
    coeff: [Coeff; 8],
    filter: [[Biquad; 12]; 2],
    drive: f32,
    target_drive: f32,
    smooth: f32,
    dc: [[f32; 2]; 2],
    dc_a: f32,
}
impl Path {
    fn new(p: &AudioFxConfig, sr: f32) -> Self {
        let mut path = Self {
            kind: p.distortion_type,
            coeff: [Coeff::default(); 8],
            filter: [[Biquad::default(); 12]; 2],
            drive: 10.0f32.powf(p.drive_db / 20.0),
            target_drive: 1.0,
            smooth: 1.0 - (-1.0 / (sr * 0.005)).exp(),
            dc: [[0.0; 2]; 2],
            dc_a: (-std::f32::consts::TAU * 10.0 / (sr * 2.0)).exp(),
        };
        path.configure(p, sr);
        path
    }
    fn configure(&mut self, p: &AudioFxConfig, sr: f32) {
        self.kind = p.distortion_type;
        self.target_drive = 10.0f32.powf(p.drive_db / 20.0);
        let os = sr * 2.0;
        // Two fourth-order Butterworth IIR anti-imaging/anti-alias filters.
        // No fixed lookahead/buffering; their frequency-dependent phase is part
        // of the voiced filter response, as for the other rack filters.
        self.coeff[0] = biquad::lowpass(os, sr * 0.42, 0.5411961);
        self.coeff[1] = biquad::lowpass(os, sr * 0.42, 1.306563);
        let (hp, pre_db, mid_db, cut) = match self.kind {
            T::Vocal => (150.0, 0.0, 2.5, 6500.0),
            T::Boost => (0.0, 0.0, 0.0, 15000.0),
            T::Overdrive => (160.0, 6.0, 0.0, 4800.0),
            T::Distortion => (65.0, 0.0, 0.0, 6500.0),
            T::Metal => (95.0, 2.0, -7.0, 8500.0),
            T::Fuzz => (35.0, 0.0, -2.0, 3800.0),
            T::Legacy => (0.0, 0.0, 0.0, 10000.0),
        };
        self.coeff[2] = if hp > 0.0 {
            biquad::highpass(os, hp, 0.707)
        } else {
            Coeff::default()
        };
        self.coeff[3] = biquad::peak(os, 750.0, 0.8, pre_db);
        self.coeff[4] = biquad::lowpass(os, 3500.0, 0.707);
        self.coeff[5] = biquad::peak(
            os,
            if self.kind == T::Vocal { 1800.0 } else { 850.0 },
            0.8,
            mid_db,
        );
        self.coeff[6] = biquad::lowpass(
            os,
            (cut * 2.0f32.powf(p.distortion_tone / 50.0)).min(sr * 0.45),
            0.707,
        );
        self.coeff[7] = biquad::highpass(os, 10.0, 0.707);
    }
    fn process(&mut self, x: [f32; 2]) -> [f32; 2] {
        self.drive += (self.target_drive - self.drive) * self.smooth;
        let mut out = [0.0; 2];
        for phase in 0..2 {
            for ch in 0..2 {
                let f = &mut self.filter[ch];
                let mut v = if phase == 0 { x[ch] * 2.0 } else { 0.0 };
                v = f[0].next(v, self.coeff[0]);
                v = f[1].next(v, self.coeff[1]);
                v = f[2].next(v, self.coeff[2]);
                v = f[3].next(v, self.coeff[3]);
                v *= self.drive;
                v = match self.kind {
                    T::Boost => {
                        if v.abs() <= 1.25 {
                            v
                        } else {
                            v.signum() * (1.25 + (v.abs() - 1.25).tanh() * 0.75)
                        }
                    }
                    T::Vocal => v.tanh() + 0.18 * (v * 0.6).tanh().powi(2),
                    T::Overdrive => v / (1.0 + v.abs()),
                    T::Distortion => v.clamp(-0.9, 0.9),
                    T::Metal => {
                        let stage = v.tanh();
                        (f[4].next(stage, self.coeff[4]) * 3.0).tanh() * 0.8
                    }
                    T::Fuzz => ((v * 2.8 + 0.32).tanh() - 0.30950692) * 0.8,
                    T::Legacy => v,
                };
                v = f[5].next(v, self.coeff[5]);
                v = f[6].next(v, self.coeff[6]);
                let dc = v - self.dc[ch][0] + self.dc_a * self.dc[ch][1];
                self.dc[ch] = [v, dc];
                v = dc;
                v = f[8].next(v, self.coeff[0]);
                v = f[9].next(v, self.coeff[1]);
                if phase == 1 {
                    out[ch] = super::headroom(v);
                }
            }
        }
        out
    }
}

#[derive(Clone)]
pub struct DistortionState {
    current: Path,
    old: Path,
    fade: f32,
    step: f32,
    direct: f32,
    effect: f32,
    target_direct: f32,
    target_effect: f32,
    initialized: bool,
}
impl DistortionState {
    pub fn new(sr: f32) -> Self {
        let p = AudioFxConfig::default();
        let path = Path::new(&p, sr);
        Self {
            current: path.clone(),
            old: path,
            fade: 1.0,
            step: 1.0 / (sr * 0.005).max(1.0),
            direct: 0.0,
            effect: 0.5,
            target_direct: 0.0,
            target_effect: 0.5,
            initialized: false,
        }
    }
    pub fn reset(&mut self) {
        self.current.filter = [[Biquad::default(); 12]; 2];
        self.old.filter = [[Biquad::default(); 12]; 2];
        self.current.dc = [[0.0; 2]; 2];
        self.old.dc = [[0.0; 2]; 2];
        self.initialized = false;
        self.fade = 1.0;
    }
    pub fn configure(&mut self, p: &AudioFxConfig, sr: f32) {
        if p.distortion_type == T::Legacy {
            self.current.kind = T::Legacy;
            self.initialized = false;
            return;
        }
        if self.current.kind != p.distortion_type {
            self.old = self.current.clone();
            self.current = Path::new(p, sr);
            self.fade = if self.initialized { 0.0 } else { 1.0 };
        } else {
            self.current.configure(p, sr);
        }
        self.target_direct = p.distortion_direct;
        self.target_effect = p.distortion_effect;
        if !self.initialized {
            self.direct = self.target_direct;
            self.effect = self.target_effect;
            self.initialized = true;
        }
    }
    pub fn process(&mut self, x: [f32; 2]) -> [f32; 2] {
        let mut wet = self.current.process(x);
        if self.fade < 1.0 {
            let old = self.old.process(x);
            self.fade = (self.fade + self.step).min(1.0);
            for ch in 0..2 {
                wet[ch] = old[ch] + (wet[ch] - old[ch]) * self.fade;
            }
        }
        self.direct += (self.target_direct - self.direct).clamp(-self.step, self.step);
        self.effect += (self.target_effect - self.effect).clamp(-self.step, self.step);
        [
            x[0] * self.direct + wet[0] * self.effect,
            x[1] * self.direct + wet[1] * self.effect,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn render(kind: T, sr: f32, hz: f32, drive: f32, amp: f32) -> Vec<f32> {
        let mut p = AudioFxConfig::default();
        p.distortion_type = kind;
        p.drive_db = drive;
        p.distortion_effect = 1.0;
        let mut d = DistortionState::new(sr);
        d.configure(&p, sr);
        (0..sr as usize / 2)
            .map(|n| {
                let x = amp * (std::f32::consts::TAU * hz * n as f32 / sr).sin();
                d.process([x, -x])[0]
            })
            .collect()
    }
    fn rms(x: &[f32]) -> f64 {
        (x.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>() / x.len() as f64).sqrt()
    }
    fn amplitude(x: &[f32], hz: f64, sr: f64) -> f64 {
        let mut sum = [0.0; 2];
        for (n, x) in x.iter().enumerate() {
            let a = std::f64::consts::TAU * hz * n as f64 / sr;
            sum[0] += *x as f64 * a.sin();
            sum[1] += *x as f64 * a.cos();
        }
        (sum[0].powi(2) + sum[1].powi(2)).sqrt() * 2.0 / x.len() as f64
    }
    #[test]
    fn six_voicings_have_measured_distinct_responses_and_boost_is_linear() {
        let sr = 48000.0;
        let sounds = T::ALL[1..]
            .iter()
            .map(|k| render(*k, sr, 400.0, 18.0, 0.2))
            .collect::<Vec<_>>();
        for a in 0..sounds.len() {
            for b in a + 1..sounds.len() {
                let diff = sounds[a][12000..]
                    .iter()
                    .zip(&sounds[b][12000..])
                    .map(|(a, b)| (*a - *b).powi(2) as f64)
                    .sum::<f64>()
                    / 12000.0;
                assert!(diff > 1e-4, "{a}={b}: {diff}");
            }
        }
        let boost = render(T::Boost, sr, 400.0, 0.0, 0.05);
        let boost = &boost[12000..];
        assert!((rms(boost) - 0.05 / std::f64::consts::SQRT_2).abs() < 0.001);
        assert!(amplitude(boost, 1200.0, 48000.0) < amplitude(boost, 400.0, 48000.0) * 0.002);
        let vocal = &sounds[0][12000..];
        let ds = &sounds[3][12000..];
        assert!(amplitude(vocal, 800.0, 48000.0) > amplitude(ds, 800.0, 48000.0) * 10.0);
        assert!(amplitude(ds, 1200.0, 48000.0) > amplitude(vocal, 1200.0, 48000.0));
    }
    #[test]
    fn direct_endpoint_reset_and_fast_switches_are_finite_without_allocation() {
        for sr in [8000.0, 44100.0, 48000.0, 192000.0] {
            let mut d = DistortionState::new(sr);
            let mut p = AudioFxConfig::default();
            p.distortion_type = T::Vocal;
            p.distortion_direct = 1.0;
            p.distortion_effect = 0.0;
            d.configure(&p, sr);
            assert_eq!(d.process([0.2, -0.3]), [0.2, -0.3]);
            let count = crate::test_alloc::count(|| {
                for n in 0..30000 {
                    if n % 511 == 0 {
                        p.distortion_type = T::ALL[1 + n / 511 % 6];
                        p.drive_db = 42.0;
                        p.distortion_tone = if n % 2 == 0 { -50.0 } else { 50.0 };
                        p.distortion_effect = 1.0;
                        d.configure(&p, sr);
                    }
                    let x = (n as f32 * 0.19).sin() * 4.0;
                    let y = d.process([x, -x]);
                    assert!(y.iter().all(|x| x.is_finite() && x.abs() <= 20.0));
                }
                d.reset();
                d.configure(&p, sr);
                for _ in 0..100 {
                    let y = d.process([0.0; 2]);
                    assert!(y.iter().all(|x| x.abs() < 1e-8));
                }
            });
            assert_eq!(count, 0);
        }
    }
    #[test]
    fn dc_from_asymmetric_shaping_decays() {
        for kind in [T::Vocal, T::Fuzz] {
            let x = render(kind, 48000.0, 300.0, 24.0, 0.5);
            let tail = &x[12000..];
            let mean = tail.iter().sum::<f32>() / tail.len() as f32;
            assert!(mean.abs() < 0.001, "{kind:?} {mean}");
        }
    }
    #[test]
    #[ignore = "manual release callback benchmark, not a device deadline guarantee"]
    fn benchmark_twenty_priority_voicings_in_128_frame_blocks() {
        use crate::config::dynamics_profiles::DynamicsProfile;
        for dynamics in [false, true] {
            let sr = 48000.0;
            let mut distortions = (0..20)
                .map(|i| {
                    let mut d = DistortionState::new(sr);
                    let mut p = AudioFxConfig::default();
                    p.distortion_type = T::ALL[1 + i % 6];
                    p.drive_db = 36.0;
                    d.configure(&p, sr);
                    d
                })
                .collect::<Vec<_>>();
            let mut profiles = (0..20)
                .map(|i| {
                    let mut d = super::super::dynamics_profiles::ProfileDynamics::default();
                    let mut p = AudioFxConfig::default();
                    p.dynamics_profile = DynamicsProfile::ALL[1 + i % 19];
                    p.dynamics_amount = 10.0;
                    d.configure(&p, sr);
                    d
                })
                .collect::<Vec<_>>();
            let mut times = Vec::with_capacity(750);
            let mut checksum = 0.0f64;
            let count = crate::test_alloc::count(|| {
                for block in 0..750 {
                    let now = std::time::Instant::now();
                    for offset in 0..128 {
                        let x = ((block * 128 + offset) as f32 * 0.037).sin() * 0.8;
                        for i in 0..20 {
                            let y = if dynamics {
                                profiles[i].process([x, -x * 0.7], 1.0)
                            } else {
                                distortions[i].process([x, -x * 0.7])
                            };
                            assert!(y.iter().all(|x| x.is_finite()));
                            checksum += y[0] as f64;
                        }
                    }
                    times.push(now.elapsed().as_secs_f64() * 1000.0);
                }
            });
            assert_eq!(count, 0);
            let sum = times.iter().sum::<f64>();
            times.sort_by(f64::total_cmp);
            println!(
                "20 {} / 2s@48k: CPU {sum:.2}ms;128f p95 {:.3},p99 {:.3},max {:.3}ms;alloc {count};checksum {checksum}",
                if dynamics {
                    "Dynamics profiles"
                } else {
                    "DIST voicings"
                },
                times[times.len() * 95 / 100],
                times[times.len() * 99 / 100],
                times[times.len() - 1]
            );
        }
    }
}
