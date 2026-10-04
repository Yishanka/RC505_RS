//! Curves are compiled off the callback; sampling uses the delayed source clock.
use crate::config::automation::{Family, Interpolation, ParameterLane, Target};
use std::sync::{Arc, Mutex, OnceLock, Weak};
pub struct PreparedLane {
    target: Target,
    length: u32,
    segments: Vec<Segment>,
    step: bool,
}
struct Segment {
    start: u32,
    end: u32,
    values: [f32; 65],
}
#[derive(Clone, Copy, Debug)]
pub struct Value {
    pub target: Target,
    pub value: f32,
}
impl Value {
    pub fn get(lane: Option<Self>, target: Target, base: f32) -> f32 {
        lane.filter(|v| v.target == target)
            .map_or(base, |v| v.value)
    }
}
impl PreparedLane {
    pub fn prepare(lane: &ParameterLane, family: Option<Family>) -> Option<Arc<Self>> {
        if !lane.enabled || !lane.target.accepts(family) || lane.points.is_empty() {
            return None;
        }
        let lane = lane.sanitized();
        let key = serde_json::to_vec(&lane).ok()?;
        static CACHE: OnceLock<Mutex<std::collections::HashMap<Vec<u8>, Weak<PreparedLane>>>> =
            OnceLock::new();
        let cache = CACHE.get_or_init(Default::default);
        let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(value) = cache.get(&key).and_then(Weak::upgrade) {
            return Some(value);
        }
        let segments = lane
            .points
            .windows(2)
            .map(|pair| Segment {
                start: pair[0].tick,
                end: pair[1].tick,
                values: std::array::from_fn(|i| {
                    let t = i as f32 / 64.0;
                    let t = if lane.interpolation == Interpolation::Curve {
                        crate::dsp::envelope::bend_curve(t, pair[0].curve)
                    } else {
                        t
                    };
                    lane.target
                        .physical(pair[0].value + (pair[1].value - pair[0].value) * t)
                }),
            })
            .collect();
        let value = Arc::new(Self {
            target: lane.target,
            length: lane.length,
            segments,
            step: lane.interpolation == Interpolation::Step,
        });
        if cache.len() >= 128 {
            cache.retain(|_, v| v.strong_count() > 0);
            if cache.len() >= 128 {
                cache.clear();
            }
        }
        cache.insert(key, Arc::downgrade(&value));
        Some(value)
    }
    pub fn at(&self, point: crate::engine::pdc::ClockPoint, sr: f32) -> Option<Value> {
        if !point.active || self.segments.is_empty() {
            return None;
        }
        let denominator = (sr.max(1.0).round() as u128) * 60;
        let numerator = point.elapsed as u128
            * point.bpm.max(1) as u128
            * crate::config::automation::PPQ as u128;
        let tick = (numerator % (denominator * self.length as u128)) as f64 / denominator as f64;
        let index = self
            .segments
            .partition_point(|s| s.end as f64 <= tick)
            .min(self.segments.len() - 1);
        let segment = &self.segments[index];
        let value = if self.step {
            segment.values[0]
        } else {
            let p = ((tick - segment.start as f64) / (segment.end - segment.start).max(1) as f64
                * 64.0)
                .clamp(0.0, 64.0);
            let i = (p as usize).min(63);
            segment.values[i] + (segment.values[i + 1] - segment.values[i]) * (p - i as f64) as f32
        };
        Some(Value {
            target: self.target,
            value,
        })
    }
}
pub fn sample(
    lane: &Option<Arc<PreparedLane>>,
    point: crate::engine::pdc::ClockPoint,
    sr: f32,
) -> Option<Value> {
    lane.as_ref().and_then(|lane| lane.at(point, sr))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::automation::{MAX_LENGTH, MAX_POINTS, PPQ, Point};
    use crate::engine::pdc::ClockPoint;
    #[test]
    fn step_boundaries_use_exact_sample_ratios_and_inactive_transport_restores_base() {
        for sr in [8000, 44100, 48000, 192000] {
            let mut lane = ParameterLane::create(Target::DelayWet);
            lane.enabled = true;
            lane.length = PPQ * 2;
            lane.interpolation = Interpolation::Step;
            lane.points = vec![
                Point {
                    tick: 0,
                    value: 0.0,
                    curve: 0.0,
                },
                Point {
                    tick: PPQ,
                    value: 1.0,
                    curve: 0.0,
                },
                Point {
                    tick: PPQ * 2,
                    value: 0.0,
                    curve: 0.0,
                },
            ];
            let prepared = PreparedLane::prepare(&lane, Some(Family::Delay)).unwrap();
            let boundary = (sr as u64 * 60).div_ceil(137);
            for (frame, want) in [(boundary - 1, 0.0), (boundary, 1.2)] {
                let actual = prepared
                    .at(
                        ClockPoint {
                            elapsed: frame,
                            bpm: 137,
                            active: true,
                            ..Default::default()
                        },
                        sr as f32,
                    )
                    .unwrap()
                    .value;
                assert_eq!(actual, want, "{sr} / {frame}");
            }
            assert!(
                prepared
                    .at(
                        ClockPoint {
                            elapsed: boundary,
                            bpm: 137,
                            active: false,
                            ..Default::default()
                        },
                        sr as f32
                    )
                    .is_none()
            );
            assert!(PreparedLane::prepare(&lane, Some(Family::Filter)).is_none());
        }
    }
    #[test]
    fn curve_compilation_is_cached_bounded_and_does_not_allocate_during_playback() {
        let mut lane = ParameterLane::create(Target::FilterCutoff);
        lane.enabled = true;
        lane.interpolation = Interpolation::Curve;
        lane.points[0].curve = 0.7;
        let a = PreparedLane::prepare(&lane, Some(Family::Filter)).unwrap();
        let b = PreparedLane::prepare(&lane.clone(), Some(Family::Filter)).unwrap();
        assert!(Arc::ptr_eq(&a, &b));
        let allocations = crate::test_alloc::count(|| {
            for frame in 0..100000 {
                let value = a
                    .at(
                        ClockPoint {
                            elapsed: frame,
                            bpm: 123,
                            active: true,
                            ..Default::default()
                        },
                        48000.0,
                    )
                    .unwrap()
                    .value;
                assert!(value.is_finite() && (20.0..=20000.0).contains(&value));
            }
        });
        assert_eq!(allocations, 0);
        lane.length = u32::MAX;
        lane.points = (0..1000)
            .map(|i| Point {
                tick: i,
                value: f32::NAN,
                curve: f32::INFINITY,
            })
            .collect();
        let lane = lane.sanitized();
        assert_eq!(lane.length, MAX_LENGTH);
        assert!(lane.points.len() <= MAX_POINTS);
        assert_eq!(lane.points.first().unwrap().tick, 0);
        assert_eq!(lane.points.last().unwrap().tick, MAX_LENGTH);
        assert!(
            lane.points
                .iter()
                .all(|p| p.value.is_finite() && p.curve == 0.0)
        );
    }
    #[test]
    fn linear_and_curved_lanes_preserve_point_values_and_curve_direction() {
        let mut lane = ParameterLane::create(Target::DelayWet);
        lane.enabled = true;
        lane.length = PPQ;
        lane.points = vec![
            Point {
                tick: 0,
                value: 0.0,
                curve: 1.0,
            },
            Point {
                tick: PPQ,
                value: 1.0,
                curve: 0.0,
            },
        ];
        let point = ClockPoint {
            elapsed: 12000,
            bpm: 120,
            active: true,
            ..Default::default()
        };
        let linear = PreparedLane::prepare(&lane, Some(Family::Delay))
            .unwrap()
            .at(point, 48000.0)
            .unwrap()
            .value;
        assert!((linear - 0.6).abs() < 1e-6);
        lane.interpolation = Interpolation::Curve;
        let curve = PreparedLane::prepare(&lane, Some(Family::Delay))
            .unwrap()
            .at(point, 48000.0)
            .unwrap()
            .value;
        assert!(curve < linear * 0.1);
        lane.points[0].curve = -1.0;
        let curve = PreparedLane::prepare(&lane, Some(Family::Delay))
            .unwrap()
            .at(point, 48000.0)
            .unwrap()
            .value;
        assert!(curve > 1.1);
    }
}
