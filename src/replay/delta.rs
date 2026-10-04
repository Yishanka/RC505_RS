//! Compact configuration events. One canonical JSON baseline is maintained per
//! reader/writer; the event list retains sparse replacements, not expanded states.
use super::*;
use serde_json::Value;

const MAX_OPERATIONS: usize = 4096;
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(untagged)]
pub enum Part {
    Key(String),
    Index(usize),
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Replacement {
    #[serde(rename = "p")]
    pub path: Vec<Part>,
    #[serde(rename = "v")]
    pub value: Value,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigDelta {
    #[serde(rename = "r")]
    pub revision: u64,
    #[serde(rename = "ops")]
    pub replacements: Vec<Replacement>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireConfig {
    config: ProjectData,
    sample_assets: Vec<assets::SampleReference>,
}
pub fn value(data: &ProjectData, samples: &[assets::SampleReference]) -> Result<Value> {
    Ok(serde_json::to_value(WireConfig {
        config: data.clone(),
        sample_assets: samples.to_vec(),
    })?)
}
fn compare(old: &Value, new: &Value, path: &mut Vec<Part>, out: &mut Vec<Replacement>) {
    if old == new || out.len() > MAX_OPERATIONS {
        return;
    }
    match (old, new) {
        (Value::Object(a), Value::Object(b))
            if a.len() == b.len() && a.keys().all(|key| b.contains_key(key)) =>
        {
            for (key, new) in b {
                path.push(Part::Key(key.clone()));
                compare(&a[key], new, path, out);
                path.pop();
            }
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            for (index, new) in b.iter().enumerate() {
                path.push(Part::Index(index));
                compare(&a[index], new, path, out);
                path.pop();
            }
        }
        _ => out.push(Replacement {
            path: path.clone(),
            value: new.clone(),
        }),
    }
}
pub fn between(old: &Value, new: &Value, revision: u64) -> Option<ConfigDelta> {
    let mut replacements = Vec::new();
    compare(old, new, &mut Vec::new(), &mut replacements);
    (replacements.len() <= MAX_OPERATIONS).then_some(ConfigDelta {
        revision,
        replacements,
    })
}
fn patched(old: &Value, delta: &ConfigDelta, revision: u64) -> Result<Value> {
    ensure!(
        delta.revision
            == revision
                .checked_add(1)
                .context("Configuration revision overflow")?,
        "Replay configuration revision is incomplete"
    );
    ensure!(
        delta.replacements.len() <= MAX_OPERATIONS,
        "Replay configuration delta has too many operations"
    );
    let mut paths = std::collections::BTreeSet::new();
    for change in &delta.replacements {
        ensure!(
            !change.path.is_empty() && change.path.len() <= 32,
            "Invalid replay delta path depth"
        );
        ensure!(
            paths.insert(change.path.clone()),
            "Duplicate replay delta path"
        );
    }
    let ordered: Vec<_> = paths.iter().collect();
    for pair in ordered.windows(2) {
        ensure!(
            !pair[1].starts_with(pair[0]),
            "Overlapping replay delta paths"
        );
    }
    let mut next = old.clone();
    for change in &delta.replacements {
        let mut target = &mut next;
        for part in &change.path {
            target = match (part, target) {
                (Part::Key(key), Value::Object(map)) => map
                    .get_mut(key)
                    .context("Replay delta targets a missing field")?,
                (Part::Index(index), Value::Array(array)) => array
                    .get_mut(*index)
                    .context("Replay delta index is out of bounds")?,
                _ => anyhow::bail!("Replay delta path has the wrong container type"),
            };
        }
        *target = change.value.clone();
    }
    Ok(next)
}
#[derive(Default)]
pub struct State {
    wire: Option<Value>,
    resolved: Option<ProjectData>,
    previous: Option<(ProjectData, Vec<assets::SampleReference>)>,
    pub revision: u64,
}
impl State {
    pub fn encode(
        &mut self,
        mut data: ProjectData,
        samples: Vec<assets::SampleReference>,
    ) -> Result<(EventKind, Vec<assets::SampleReference>)> {
        let revision = self
            .revision
            .checked_add(1)
            .context("Configuration revision overflow")?;
        if let Some((previous, refs)) = &mut self.previous {
            let levels = std::mem::take(&mut data.track_levels);
            let old_levels = std::mem::take(&mut previous.track_levels);
            let only_levels =
                *previous == data && *refs == samples && levels.len() == 5 && old_levels.len() == 5;
            data.track_levels = levels;
            previous.track_levels = old_levels;
            if only_levels {
                let replacements = data
                    .track_levels
                    .iter()
                    .zip(&previous.track_levels)
                    .enumerate()
                    .filter_map(|(index, (value, old))| {
                        (value != old).then(|| Replacement {
                            path: vec![
                                Part::Key("config".into()),
                                Part::Key("track_levels".into()),
                                Part::Index(index),
                            ],
                            value: Value::from(*value),
                        })
                    })
                    .collect();
                let wire = self
                    .wire
                    .as_mut()
                    .context("Missing replay writer baseline")?;
                wire["config"]["track_levels"] = serde_json::to_value(&data.track_levels)?;
                self.previous = Some((data, samples));
                self.revision = revision;
                return Ok((
                    EventKind::ConfigDelta(ConfigDelta {
                        revision,
                        replacements,
                    }),
                    Vec::new(),
                ));
            }
        }
        let next = value(&data, &samples)?;
        let patch = self
            .wire
            .as_ref()
            .and_then(|old| between(old, &next, revision));
        self.previous = Some((data.clone(), samples.clone()));
        self.wire = Some(next);
        self.revision = revision;
        Ok(if let Some(patch) = patch {
            (EventKind::ConfigDelta(patch), Vec::new())
        } else {
            (EventKind::Config(data), samples)
        })
    }
    pub fn apply(
        &mut self,
        event: &Event,
        assets: &mut assets::AssetReader,
    ) -> Result<Option<ProjectData>> {
        if let EventKind::ConfigDelta(delta) = &event.kind {
            // Fader gestures are frequent and affect five scalar values. Avoid
            // cloning/deserializing thousands of unchanged note JSON objects.
            if self.resolved.is_some() && delta.replacements.iter().all(|op|matches!(op.path.as_slice(),[Part::Key(root),Part::Key(field),Part::Index(_)] if root=="config" && field=="track_levels")) {
                ensure!(event.sample_assets.is_empty(),"Delta assets must be part of the configuration patch");
                ensure!(delta.revision==self.revision.checked_add(1).context("Configuration revision overflow")?,"Replay configuration revision is incomplete");
                ensure!(delta.replacements.len()<=5,"Duplicate replay fader paths");
                let mut data=self.resolved.as_ref().unwrap().clone();
                ensure!(data.track_levels.len()==5,"Replay delta changed track dimensions");
                let mut seen=0u8;
                for op in &delta.replacements {
                    let Part::Index(index)=op.path[2] else {unreachable!()};
                    ensure!(index<5 && seen&(1<<index)==0,"Invalid or duplicate replay fader index");seen|=1<<index;
                    let value:f32=serde_json::from_value(op.value.clone()).context("Invalid replay fader value")?;
                    ensure!(value.is_finite(),"Invalid replay fader value");data.track_levels[index]=value;
                }
                let wire=self.wire.as_mut().context("Replay delta has no configuration baseline")?;
                for op in &delta.replacements {let Part::Index(index)=op.path[2] else {unreachable!()};wire["config"]["track_levels"][index]=op.value.clone();}
                self.resolved=Some(data.clone());self.revision=delta.revision;
                return Ok(Some(data));
            }
        }
        let (next, is_delta) = match &event.kind {
            EventKind::Action(_) | EventKind::PdcApplied(_) => {
                ensure!(
                    event.sample_assets.is_empty(),
                    "Action events cannot reference samples"
                );
                return Ok(None);
            }
            EventKind::Config(data) => (value(data, &event.sample_assets)?, false),
            EventKind::ConfigDelta(delta) => {
                ensure!(
                    event.sample_assets.is_empty(),
                    "Delta assets must be part of the configuration patch"
                );
                let old = self
                    .wire
                    .as_ref()
                    .context("Replay delta has no configuration baseline")?;
                (patched(old, delta, self.revision)?, true)
            }
        };
        let mut decoded: WireConfig = serde_json::from_value(next.clone())
            .context("Invalid replay configuration delta values")?;
        if is_delta {
            ensure!(
                decoded.config.input_fx.banks.len() == 4
                    && decoded
                        .config
                        .input_fx
                        .banks
                        .iter()
                        .all(|b| b.slots.len() == 4),
                "Replay delta changed input bank dimensions"
            );
            ensure!(
                decoded.config.track_fx.banks.len() == 4
                    && decoded
                        .config
                        .track_fx
                        .banks
                        .iter()
                        .all(|b| b.slots.len() == 4),
                "Replay delta changed track FX dimensions"
            );
            ensure!(
                decoded.config.track_levels.len() == 5
                    && decoded.config.track_options.len() == 5
                    && decoded.config.track_fx.tracks.len() == 5,
                "Replay delta changed track dimensions"
            );
        }
        assets.attach_data(&mut decoded.config, &decoded.sample_assets)?;
        self.resolved = Some(decoded.config.clone());
        self.wire = Some(next);
        self.revision = self
            .revision
            .checked_add(1)
            .context("Configuration revision overflow")?;
        Ok(Some(decoded.config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dense() -> ProjectData {
        use crate::config::{
            FxKind, InputFx,
            note_configs::NoteOct,
            sequence_edit::{NoteEvent, PPQ},
        };
        let mut config = AppConfig::new(120, 0, 5);
        for bank in 0..4 {
            for slot in 0..4 {
                config
                    .input_fx
                    .set_slot_kind(bank, slot, FxKind::Oscillator);
                if let Some(InputFx::Oscillator(osc)) =
                    &mut config.input_fx.banks[bank].slots[slot].fx
                {
                    let notes: Vec<_> = (0..256)
                        .map(|n| {
                            NoteEvent::new(
                                n / 4 * PPQ / 2,
                                PPQ / 2,
                                NoteOct::from_pitch_index(48 + n % 12),
                            )
                        })
                        .collect();
                    osc.note.replace_events(PPQ * 32, &notes);
                }
            }
        }
        crate::project::data_from_config(&config)
    }
    #[test]
    fn quantify_dense_project_and_sparse_fader_event() {
        let mut data = dense();
        let full = serde_json::to_vec(&data).unwrap().len();
        let old = value(&data, &[]).unwrap();
        data.track_levels[2] = 0.43;
        let delta = between(&old, &value(&data, &[]).unwrap(), 2).unwrap();
        let bytes = serde_json::to_vec(&delta).unwrap().len();
        eprintln!(
            "16 OSC slots ×256 notes: full configuration {full} bytes; fader delta {bytes} bytes; full payload at60Hz {:.2}MiB/min",
            full as f64 * 3600.0 / 1048576.0
        );
        assert!(bytes < 160 && full > 100_000);
    }
    #[test]
    fn continuous_dense_fader_automation_writes_one_baseline_and_bounded_deltas() {
        let root = PathBuf::from("var").join(format!("replay-delta-test-{}", session::id()));
        let mut data = dense();
        let full = serde_json::to_vec(&data).unwrap().len();
        let mut writer = Writer::begin(
            root.clone(),
            "dense.json".into(),
            0,
            AudioSnapshot::empty(8000),
            data.clone(),
        )
        .unwrap();
        let began = std::time::Instant::now();
        for n in 0..120u64 {
            data.track_levels[2] = (n + 1) as f32 / 121.0;
            writer.event(n, EventKind::Config(data.clone())).unwrap();
            writer.audio(n, &[[0.0; 2]]).unwrap();
        }
        writer.finish(120).unwrap();
        let elapsed = began.elapsed();
        let bytes = fs::metadata(root.join("events.jsonl")).unwrap().len();
        let opened = std::time::Instant::now();
        let source = streaming::Source::open(&root).unwrap();
        let open_elapsed = opened.elapsed();
        assert_eq!(
            source
                .events
                .iter()
                .filter(|event| matches!(event.kind, EventKind::Config(_)))
                .count(),
            1,
            "Source must keep sparse events instead of expanding every configuration"
        );
        let played = std::time::Instant::now();
        let mut machine = streaming::Machine::new(source).unwrap();
        machine.advance_to(120, || false).unwrap();
        let play_elapsed = played.elapsed();
        assert_eq!(machine.data.track_levels[2], data.track_levels[2]);
        let events: Vec<Event> = fs::read_to_string(root.join("events.jsonl"))
            .unwrap()
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e.kind, EventKind::Config(_)))
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e.kind, EventKind::ConfigDelta(_)))
                .count(),
            119
        );
        assert!(
            bytes < full as u64 * 2,
            "Continuous faders must not repeat the full phrase"
        );
        assert!(
            events
                .iter()
                .skip(1)
                .all(|event| serde_json::to_vec(event).unwrap().len() < 200)
        );
        eprintln!(
            "dense 120 fader events: {bytes} bytes vs {} full bytes; writer {:?} ({:.1} events/s); Source {:?}; simulation {:?}",
            full * 120,
            elapsed,
            120.0 / elapsed.as_secs_f64(),
            open_elapsed,
            play_elapsed
        );
        drop(machine);
        let mut bad = events;
        if let EventKind::ConfigDelta(delta) = &mut bad.last_mut().unwrap().kind {
            delta.revision += 3;
        }
        let log = bad
            .iter()
            .map(|event| serde_json::to_string(event).unwrap() + "\n")
            .collect::<String>();
        fs::write(root.join("events.jsonl"), log).unwrap();
        let mut metadata = info(&root).unwrap();
        metadata.events_sha256 = session::checksum(&root.join("events.jsonl")).unwrap();
        fs::write(
            root.join("replay.json"),
            serde_json::to_vec(&metadata).unwrap(),
        )
        .unwrap();
        assert!(
            streaming::Source::open(&root)
                .err()
                .unwrap()
                .to_string()
                .contains("configuration revision"),
            "Valid checksums must not hide malformed deltas"
        );
        let root = fs::canonicalize(root).unwrap();
        assert!(root.starts_with(fs::canonicalize("var").unwrap()));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn malformed_paths_revisions_types_and_dimensions_are_rejected_transactionally() {
        let config = crate::project::data_from_config(&AppConfig::new(120, 0, 5));
        let initial = Event {
            frame: 0,
            sequence: 0,
            kind: EventKind::Config(config),
            sample_assets: Vec::new(),
        };
        let mut state = State::default();
        let mut assets = assets::AssetReader::new(Path::new("var"));
        let path = vec![
            Part::Key("config".into()),
            Part::Key("track_levels".into()),
            Part::Index(0),
        ];
        let make = |revision, path: Vec<Part>, value| Event {
            frame: 1,
            sequence: 1,
            kind: EventKind::ConfigDelta(ConfigDelta {
                revision,
                replacements: vec![Replacement { path, value }],
            }),
            sample_assets: Vec::new(),
        };
        assert!(
            state
                .apply(&make(1, path.clone(), Value::from(0.25)), &mut assets)
                .is_err()
        );
        state.apply(&initial, &mut assets).unwrap();
        let before = state.wire.clone();
        let failures = [
            make(7, path.clone(), Value::from(0.25)),
            make(2, vec![Part::Key("missing".into())], Value::Null),
            make(
                2,
                vec![
                    Part::Key("config".into()),
                    Part::Key("track_levels".into()),
                    Part::Index(99),
                ],
                Value::from(0.25),
            ),
            make(2, path.clone(), Value::String("not a gain".into())),
            make(
                2,
                vec![Part::Key("config".into()), Part::Key("track_levels".into())],
                serde_json::json!([0.25]),
            ),
            make(2, Vec::new(), Value::Null),
        ];
        for event in failures {
            assert!(state.apply(&event, &mut assets).is_err());
            assert_eq!(state.wire, before);
            assert_eq!(state.revision, 1);
        }
        let mut duplicate = make(2, path.clone(), Value::from(0.25));
        if let EventKind::ConfigDelta(delta) = &mut duplicate.kind {
            delta.replacements.push(delta.replacements[0].clone());
        }
        assert!(state.apply(&duplicate, &mut assets).is_err());
        let mut overlapping = make(2, path.clone(), Value::from(0.25));
        if let EventKind::ConfigDelta(delta) = &mut overlapping.kind {
            delta.replacements.push(Replacement {
                path: vec![Part::Key("config".into()), Part::Key("track_levels".into())],
                value: serde_json::json!([0.2, 0.2, 0.2, 0.2, 0.2]),
            });
        }
        assert!(state.apply(&overlapping, &mut assets).is_err());
        let valid = state
            .apply(&make(2, path, Value::from(0.25)), &mut assets)
            .unwrap()
            .unwrap();
        assert_eq!(valid.track_levels[0], 0.25);
        let mut replacement = valid;
        replacement.track_levels[0] = 0.75;
        let full = Event {
            frame: 2,
            sequence: 2,
            kind: EventKind::Config(replacement),
            sample_assets: Vec::new(),
        };
        assert_eq!(
            state
                .apply(&full, &mut assets)
                .unwrap()
                .unwrap()
                .track_levels[0],
            0.75,
            "Old complete Config events remain readable after deltas"
        );
    }
}
