//! Transport-aligned cutoff modulation for the standalone Filter effect.
//! Public RATE / DEPTH / STEP RATE semantics; octave depth is our documented law.
use crate::config::filter_configs::FilterSweepConfig;
#[derive(Clone, Copy, Debug)]
pub struct FilterSweepState {
    previous: Option<(FilterSweepConfig, usize)>,
    last_frame: u64,
    next_update: u64,
    step_index: u64,
    ratio: f32,
}
impl Default for FilterSweepState {
    fn default() -> Self {
        Self {
            previous: None,
            last_frame: 0,
            next_update: 0,
            step_index: u64::MAX,
            ratio: 1.0,
        }
    }
}
impl FilterSweepState {
    pub fn cutoff(
        &mut self,
        p: FilterSweepConfig,
        base: f32,
        frame: u64,
        bpm: usize,
        active: bool,
        sr: f32,
    ) -> f32 {
        if p.depth <= 0.0 || (p.sync && !active) {
            self.previous = None;
            self.ratio = 1.0;
            return base;
        }
        let sr = sr.max(1.0) as f64;
        let rate = if p.sync {
            bpm.max(1) as f64 / (60.0 * p.beats as f64)
        } else {
            p.rate_hz as f64
        };
        let step_rate = if p.step_sync {
            bpm.max(1) as f64 / (60.0 * p.step_beats as f64)
        } else {
            p.step_hz as f64
        };
        let time = frame as f64 / sr;
        let index = if p.stepped {
            (time * step_rate).floor() as u64
        } else {
            0
        };
        let discontinuity =
            self.previous != Some((p, bpm)) || frame != self.last_frame.saturating_add(1);
        if discontinuity
            || (p.stepped && index != self.step_index)
            || (!p.stepped && frame >= self.next_update)
        {
            let sample_time = if p.stepped {
                index as f64 / step_rate
            } else {
                time
            };
            let phase = (sample_time * rate).rem_euclid(1.0);
            self.ratio = (4.0 * p.depth * (std::f64::consts::TAU * phase).sin() as f32).exp2();
            self.previous = Some((p, bpm));
            self.next_update = frame.saturating_add(16);
            self.step_index = index;
        }
        self.last_frame = frame;
        (base * self.ratio).clamp(20.0, (sr as f32 * 0.45).min(20000.0).max(20.0))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disabled_or_stopped_sweep_preserves_old_cutoff_exactly() {
        let mut s = FilterSweepState::default();
        for frame in 0..1000 {
            assert_eq!(
                s.cutoff(
                    FilterSweepConfig::default(),
                    1379.0,
                    frame,
                    137,
                    true,
                    48000.0
                ),
                1379.0
            );
        }
        let p = FilterSweepConfig {
            depth: 1.0,
            sync: true,
            ..Default::default()
        };
        assert_eq!(s.cutoff(p, 1379.0, 999, 137, false, 48000.0), 1379.0);
    }
    #[test]
    fn phase_is_sample_clock_based_and_step_rate_holds_independent_of_lfo_rate() {
        for sr in [44100.0, 48000.0, 96000.0, 192000.0] {
            let p = FilterSweepConfig {
                depth: 0.25,
                sync: true,
                beats: 4.0,
                ..Default::default()
            };
            let mut s = FilterSweepState::default();
            assert!((s.cutoff(p, 1000.0, (sr * 0.5) as u64, 120, true, sr) - 2000.0).abs() < 0.01);
            assert!((s.cutoff(p, 1000.0, (sr * 1.5) as u64, 120, true, sr) - 500.0).abs() < 0.01);
        }
        let p = FilterSweepConfig {
            depth: 0.5,
            rate_hz: 1.0,
            stepped: true,
            step_hz: 4.0,
            ..Default::default()
        };
        let mut s = FilterSweepState::default();
        assert_eq!(s.cutoff(p, 1000.0, 11000, 120, true, 48000.0), 1000.0);
        let y = s.cutoff(p, 1000.0, 12000, 120, true, 48000.0);
        assert_eq!(y, 4000.0);
        assert_eq!(s.cutoff(p, 1000.0, 23999, 120, true, 48000.0), y);
    }
    #[test]
    fn free_rate_runs_before_transport_starts() {
        let mut s = FilterSweepState::default();
        let p = FilterSweepConfig {
            depth: 0.25,
            rate_hz: 1.0,
            ..Default::default()
        };
        assert_eq!(s.cutoff(p, 1000.0, 0, 120, false, 48000.0), 1000.0);
        assert!((s.cutoff(p, 1000.0, 12000, 120, false, 48000.0) - 2000.0).abs() < 0.001);
    }
    #[test]
    fn sweep_fast_automation_is_finite_and_does_not_allocate() {
        let p = [
            FilterSweepConfig {
                depth: 1.0,
                rate_hz: 20.0,
                ..Default::default()
            },
            FilterSweepConfig {
                depth: 0.5,
                stepped: true,
                step_sync: true,
                step_beats: 0.015625,
                ..Default::default()
            },
        ];
        let mut s = FilterSweepState::default();
        let alloc = crate::test_alloc::count(|| {
            for i in 0..48000 {
                let y = s.cutoff(p[(i / 64) as usize % 2], 1000.0, i, 137, true, 48000.0);
                assert!((20.0..=20000.0).contains(&y));
            }
        });
        assert_eq!(alloc, 0);
    }
}
