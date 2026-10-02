//! Read-only visual timeline produced during the same render as the WAV.
//! Configuration changes store deltas, not repeated full FX/sequence snapshots.
use crate::{
    engine::core::{Action, EngineView},
    presets::FxTarget,
    project::ProjectData,
};
use serde_json::Value;
pub struct VisualFrame {
    pub frame: u64,
    pub view: EngineView,
    pub last_action: Option<(u64, Action)>,
}
#[derive(Clone)]
pub enum Segment {
    Key(String),
    Index(usize),
}
pub struct Change {
    path: Vec<Segment>,
    value: Value,
}
pub struct ConfigPoint {
    pub frame: u64,
    pub changes: Vec<Change>,
    pub target: Option<FxTarget>,
    pub track: Option<usize>,
}
pub struct ReplayVisuals {
    pub name: String,
    pub sample_rate: u32,
    pub frames: u64,
    pub views: Vec<VisualFrame>,
    pub initial: ProjectData,
    pub configs: Vec<ConfigPoint>,
}
pub fn changes(old: &ProjectData, new: &ProjectData) -> Vec<Change> {
    fn visit(old: &Value, new: &Value, path: &mut Vec<Segment>, out: &mut Vec<Change>) {
        if old == new {
            return;
        }
        match (old, new) {
            (Value::Object(a), Value::Object(b))
                if a.len() == b.len() && a.keys().all(|k| b.contains_key(k)) =>
            {
                for (key, value) in b {
                    path.push(Segment::Key(key.clone()));
                    visit(&a[key], value, path, out);
                    path.pop();
                }
            }
            (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
                for (index, value) in b.iter().enumerate() {
                    path.push(Segment::Index(index));
                    visit(&a[index], value, path, out);
                    path.pop();
                }
            }
            _ => out.push(Change {
                path: path.clone(),
                value: new.clone(),
            }),
        }
    }
    let mut out = Vec::new();
    visit(
        &serde_json::to_value(old).unwrap(),
        &serde_json::to_value(new).unwrap(),
        &mut Vec::new(),
        &mut out,
    );
    out
}
pub fn apply(value: &mut Value, changes: &[Change]) {
    for change in changes {
        let mut target = &mut *value;
        for part in &change.path {
            target = match part {
                Segment::Key(key) => &mut target[key],
                Segment::Index(index) => &mut target[*index],
            };
        }
        *target = change.value.clone();
    }
}
impl ConfigPoint {
    pub fn new(frame: u64, old: &ProjectData, new: &ProjectData) -> Self {
        let changes = changes(old, new);
        let target = changes.iter().find_map(|c| match c.path.as_slice() {
            [
                Segment::Key(kind),
                Segment::Key(tracks),
                Segment::Index(_),
                Segment::Key(enabled),
                Segment::Index(bank),
                Segment::Index(slot),
                ..,
            ] if kind == "track_fx"
                && tracks == "tracks"
                && enabled == "enabled"
                && *bank < 4
                && *slot < 4 =>
            {
                Some(FxTarget::Track {
                    bank: *bank,
                    slot: *slot,
                })
            }
            [
                Segment::Key(kind),
                Segment::Key(banks),
                Segment::Index(bank),
                Segment::Key(slots),
                Segment::Index(slot),
                ..,
            ] if banks == "banks" && slots == "slots" && *bank < 4 && *slot < 4 => {
                match kind.as_str() {
                    "input_fx" => Some(FxTarget::Input {
                        bank: *bank,
                        slot: *slot,
                    }),
                    "track_fx" => Some(FxTarget::Track {
                        bank: *bank,
                        slot: *slot,
                    }),
                    _ => None,
                }
            }
            _ => None,
        });
        let track = changes.iter().find_map(|c| match c.path.as_slice() {
            [Segment::Key(kind), Segment::Index(track), ..]
                if (kind == "track_levels" || kind == "track_options") && *track < 5 =>
            {
                Some(*track)
            }
            [
                Segment::Key(kind),
                Segment::Key(tracks),
                Segment::Index(track),
                ..,
            ] if kind == "track_fx" && tracks == "tracks" && *track < 5 => Some(*track),
            _ => None,
        });
        Self {
            frame,
            changes,
            target,
            track,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deltas_restore_faders_banks_and_resized_sequences() {
        let mut config = crate::config::AppConfig::new(120, 0, 5);
        let initial = crate::project::data_from_config(&config);
        let mut value = serde_json::to_value(&initial).unwrap();
        config.track_levels[2] = 0.3;
        config
            .input_fx
            .set_slot_kind(0, 1, crate::config::FxKind::Oscillator);
        if let Some(crate::config::InputFx::Oscillator(v)) =
            &mut config.input_fx.banks[0].slots[1].fx
        {
            v.note.push();
        }
        let next = crate::project::data_from_config(&config);
        let change = ConfigPoint::new(0, &initial, &next);
        assert!(change.target == Some(FxTarget::Input { bank: 0, slot: 1 }));
        apply(&mut value, &change.changes);
        assert_eq!(value, serde_json::to_value(&next).unwrap());
        apply(&mut value, &changes(&next, &initial));
        assert_eq!(value, serde_json::to_value(&initial).unwrap());
        let mut next = next;
        let previous = next.clone();
        next.track_fx.tracks[3].enabled[2][1] = true;
        let point = ConfigPoint::new(17, &previous, &next);
        assert_eq!(point.track, Some(3));
        assert!(point.target == Some(FxTarget::Track { bank: 2, slot: 1 }));
    }
}
