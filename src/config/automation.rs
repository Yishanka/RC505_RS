//! One optional, bounded musical parameter lane per effect slot.
use serde::{Deserialize, Serialize};
pub const PPQ: u32 = 960;
pub const MIN_LENGTH: u32 = PPQ / 16;
pub const MAX_LENGTH: u32 = PPQ * 4 * 8;
pub const MAX_POINTS: usize = 64;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Filter,
    Delay,
    Reverb,
    ReverbAudio,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Target {
    #[default]
    FilterCutoff,
    FilterQ,
    DelayTime,
    DelayFeedback,
    DelayWet,
    ReverbDecay,
    ReverbAudioDecay,
    ReverbWet,
}
impl Target {
    pub fn family(self) -> Family {
        match self {
            Self::FilterCutoff | Self::FilterQ => Family::Filter,
            Self::DelayTime | Self::DelayFeedback | Self::DelayWet => Family::Delay,
            Self::ReverbAudioDecay => Family::ReverbAudio,
            _ => Family::Reverb,
        }
    }
    pub fn accepts(self, family: Option<Family>) -> bool {
        Some(self.family()) == family
            || self == Self::ReverbWet && family == Some(Family::ReverbAudio)
    }
    pub fn range(self) -> (f32, f32, bool) {
        match self {
            Self::FilterCutoff => (20.0, 20000.0, true),
            Self::FilterQ => (0.1, 10.0, true),
            Self::DelayTime => (1.0, 2000.0, true),
            Self::DelayFeedback => (0.0, 1.0, false),
            Self::DelayWet => (0.0, 1.2, false),
            Self::ReverbWet => (0.0, 1.0, false),
            Self::ReverbDecay => (100.0, 12000.0, true),
            Self::ReverbAudioDecay => (100.0, 15000.0, true),
        }
    }
    pub fn physical(self, value: f32) -> f32 {
        let (a, b, log) = self.range();
        if log {
            a * (b / a).powf(value.clamp(0.0, 1.0))
        } else {
            a + (b - a) * value.clamp(0.0, 1.0)
        }
    }
    pub fn normalized(self, value: f32) -> f32 {
        let (a, b, log) = self.range();
        if log {
            (value.clamp(a, b) / a).ln() / (b / a).ln()
        } else {
            (value.clamp(a, b) - a) / (b - a)
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Interpolation {
    Step,
    #[default]
    Linear,
    Curve,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub tick: u32,
    pub value: f32,
    #[serde(default)]
    pub curve: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ParameterLane {
    pub enabled: bool,
    pub target: Target,
    pub length: u32,
    pub interpolation: Interpolation,
    pub points: Vec<Point>,
}
impl Default for ParameterLane {
    fn default() -> Self {
        Self {
            enabled: false,
            target: Target::FilterCutoff,
            length: PPQ * 4,
            interpolation: Interpolation::Linear,
            points: Vec::new(),
        }
    }
}
impl ParameterLane {
    pub fn create(target: Target) -> Self {
        Self {
            target,
            points: vec![
                Point {
                    tick: 0,
                    value: 0.25,
                    curve: 0.0,
                },
                Point {
                    tick: PPQ * 2,
                    value: 0.75,
                    curve: 0.0,
                },
                Point {
                    tick: PPQ * 4,
                    value: 0.25,
                    curve: 0.0,
                },
            ],
            ..Self::default()
        }
    }
    pub fn sanitized(&self) -> Self {
        let mut lane = self.clone();
        lane.length = lane.length.clamp(MIN_LENGTH, MAX_LENGTH);
        lane.points.truncate(MAX_POINTS);
        for point in &mut lane.points {
            point.tick = point.tick.min(lane.length);
            point.value = if point.value.is_finite() {
                point.value.clamp(0.0, 1.0)
            } else {
                0.5
            };
            point.curve = if point.curve.is_finite() {
                point.curve.clamp(-1.0, 1.0)
            } else {
                0.0
            };
        }
        lane.points.sort_by_key(|p| p.tick);
        lane.points.dedup_by_key(|p| p.tick);
        if !lane.points.is_empty() {
            if lane.points[0].tick != 0 {
                lane.points.insert(
                    0,
                    Point {
                        tick: 0,
                        ..lane.points[0]
                    },
                );
            }
            if lane.points.last().unwrap().tick != lane.length {
                let end = Point {
                    tick: lane.length,
                    ..*lane.points.last().unwrap()
                };
                if lane.points.len() >= MAX_POINTS {
                    lane.points.pop();
                }
                lane.points.push(end);
            }
            lane.points.truncate(MAX_POINTS);
            if let Some(last) = lane.points.last_mut() {
                last.tick = lane.length;
            }
        }
        lane
    }
    /// Change the period without merging closely spaced, valid editing nodes.
    /// A tick is the smallest stored time unit, regardless of the visual grid.
    pub fn rescale_length(&mut self, length: u32) {
        *self = self.sanitized();
        let count = self.points.len();
        let length = length.clamp(MIN_LENGTH.max(count.saturating_sub(1) as u32), MAX_LENGTH);
        let old_length = self.length;
        let mut previous = 0;
        for (index, point) in self.points.iter_mut().enumerate() {
            let tick = if index == 0 {
                0
            } else if index + 1 == count {
                length
            } else {
                let scaled =
                    (point.tick as u64 * length as u64 + old_length as u64 / 2) / old_length as u64;
                (scaled as u32).clamp(previous + 1, length - (count - 1 - index) as u32)
            };
            point.tick = tick;
            previous = tick;
        }
        self.length = length;
    }
}
pub fn input_family(fx: Option<&super::InputFx>) -> Option<Family> {
    match fx {
        Some(super::InputFx::Filter(_)) => Some(Family::Filter),
        Some(super::InputFx::Reverb(_)) => Some(Family::Reverb),
        Some(super::InputFx::Audio(p)) => audio_family(p.kind),
        _ => None,
    }
}
pub fn track_family(fx: Option<&super::TrackFx>) -> Option<Family> {
    match fx {
        Some(super::TrackFx::Filter(_)) => Some(Family::Filter),
        Some(super::TrackFx::Delay(_)) => Some(Family::Delay),
        Some(super::TrackFx::Audio(p)) => audio_family(p.kind),
        _ => None,
    }
}
pub fn audio_family(kind: super::audio_fx::AudioFxKind) -> Option<Family> {
    use super::audio_fx::AudioFxKind as K;
    match kind {
        K::Delay | K::PanningDelay => Some(Family::Delay),
        K::Reverb => Some(Family::ReverbAudio),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{AppConfig, FxKind, TrackFxKind, audio_fx::AudioFxKind},
        presets::{self, FxTarget},
        project,
    };
    #[test]
    fn short_period_rescaling_preserves_dense_nodes_and_exact_saved_ticks() {
        let mut lane = ParameterLane::create(Target::FilterCutoff);
        lane.enabled = true;
        lane.interpolation = Interpolation::Curve;
        lane.points = (0..MAX_POINTS)
            .map(|i| Point {
                tick: (i as u32 * (lane.length - 1)) / (MAX_POINTS as u32 - 1),
                value: i as f32 / MAX_POINTS as f32,
                curve: i as f32 / MAX_POINTS as f32 - 0.5,
            })
            .collect();
        lane.points.last_mut().unwrap().tick = lane.length;
        let original_values: Vec<_> = lane.points.iter().map(|p| (p.value, p.curve)).collect();
        lane.rescale_length(MIN_LENGTH);
        assert_eq!(lane.length, MAX_POINTS as u32 - 1);
        assert_eq!(lane.points.len(), MAX_POINTS);
        assert!(lane.points.windows(2).all(|p| p[0].tick < p[1].tick));
        assert_eq!(lane.points.first().unwrap().tick, 0);
        assert_eq!(lane.points.last().unwrap().tick, lane.length);
        assert_eq!(
            lane.points
                .iter()
                .map(|p| (p.value, p.curve))
                .collect::<Vec<_>>(),
            original_values
        );
        assert_eq!(lane, lane.sanitized());
        let restored: ParameterLane =
            serde_json::from_slice(&serde_json::to_vec(&lane).unwrap()).unwrap();
        assert_eq!(restored, lane);
        let mut simple = ParameterLane::create(Target::DelayWet);
        simple.rescale_length(96); // 0.1 beat, independent of any UI grid.
        assert_eq!(simple.length, 96);
        assert_eq!(
            simple.points.iter().map(|p| p.tick).collect::<Vec<_>>(),
            vec![0, 48, 96]
        );
    }
    #[test]
    fn lanes_roundtrip_with_patches_and_disable_safely_on_incompatible_type() {
        let mut config = AppConfig::new(120, 0, 5);
        config.input_fx.set_slot_kind(0, 0, FxKind::Filter);
        config
            .track_fx
            .set_slot_kind(0, 0, TrackFxKind::Audio(AudioFxKind::Reverb));
        let mut input = ParameterLane::create(Target::FilterQ);
        input.enabled = true;
        input.interpolation = Interpolation::Curve;
        input.points[1].curve = -0.3;
        let mut track = ParameterLane::create(Target::ReverbAudioDecay);
        track.enabled = true;
        track.points[1].value = 1.0;
        config.input_fx.banks[0].slots[0].parameter_lane = input.clone();
        config.track_fx.banks[0].slots[0].parameter_lane = track.clone();
        let data = project::data_from_config(&config);
        let mut restored = AppConfig::new(120, 0, 5);
        project::apply_data_to_config(
            &mut restored,
            serde_json::from_str(&serde_json::to_string(&data).unwrap()).unwrap(),
        );
        assert_eq!(restored.input_fx.banks[0].slots[0].parameter_lane, input);
        assert_eq!(restored.track_fx.banks[0].slots[0].parameter_lane, track);
        let encoded = presets::encode(&config, FxTarget::Input { bank: 0, slot: 0 }).unwrap();
        presets::decode(
            &mut restored,
            FxTarget::Input { bank: 1, slot: 2 },
            &encoded,
        )
        .unwrap();
        assert_eq!(restored.input_fx.banks[1].slots[2].parameter_lane, input);
        let encoded = presets::encode(&config, FxTarget::Track { bank: 0, slot: 0 }).unwrap();
        presets::decode(
            &mut restored,
            FxTarget::Track { bank: 1, slot: 2 },
            &encoded,
        )
        .unwrap();
        assert_eq!(restored.track_fx.banks[1].slots[2].parameter_lane, track);
        restored.input_fx.set_slot_kind(1, 2, FxKind::Reverb);
        let retained = &restored.input_fx.banks[1].slots[2].parameter_lane;
        assert!(!retained.enabled);
        assert_eq!(retained.points, input.points);
        restored.input_fx.set_slot_kind(1, 2, FxKind::Filter);
        assert!(!restored.input_fx.banks[1].slots[2].parameter_lane.enabled);
        assert_eq!(Target::DelayFeedback.physical(1.0), 1.0);
        assert_eq!(Target::ReverbDecay.physical(1.0), 12000.0);
        assert_eq!(Target::ReverbAudioDecay.physical(1.0), 15000.0);
        let mut old = serde_json::to_value(data).unwrap();
        old["input_fx"]["banks"][0]["slots"][0]
            .as_object_mut()
            .unwrap()
            .remove("parameter_lane");
        project::apply_data_to_config(&mut restored, serde_json::from_value(old).unwrap());
        assert!(!restored.input_fx.banks[0].slots[0].parameter_lane.enabled);
        assert!(
            restored.input_fx.banks[0].slots[0]
                .parameter_lane
                .points
                .is_empty()
        );
    }
}
