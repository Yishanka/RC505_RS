//! A knob move must not append every embedded OSC sample to the event log.
//! Assets are lossless little-endian PCM, content addressed and independently
//! verified. References live inside the checksummed event envelope.
use super::*;
use crate::config::osc_configs::SampleAsset;
use sha2::{Digest, Sha256};
use std::{collections::HashMap, sync::Arc};
const MAX_ASSET_BYTES: usize = 256 * 1024 * 1024;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct SampleReference {
    pub bank: usize,
    pub slot: usize,
    pub name: String,
    pub sample_rate: u32,
    pub frames: usize,
    pub sha256: String,
    #[serde(default)]
    pub root_hz: Option<f32>,
    #[serde(default)]
    pub cycle_start: usize,
    #[serde(default)]
    pub cycle_end: usize,
}
#[derive(Default)]
pub struct AssetWriter {
    known: HashMap<usize, (Arc<SampleAsset>, SampleReference)>,
    retained_bytes: usize,
}
impl AssetWriter {
    pub fn detach(&mut self, root: &Path, data: &mut ProjectData) -> Result<Vec<SampleReference>> {
        let mut refs = Vec::new();
        for (bank, values) in data.input_fx.banks.iter_mut().enumerate() {
            for (slot, value) in values.slots.iter_mut().enumerate() {
                let Some(sample) = value.osc.as_mut().and_then(|osc| osc.sample.take()) else {
                    continue;
                };
                let identity = Arc::as_ptr(&sample) as usize;
                let mut reference = if let Some((_, reference)) = self.known.get(&identity) {
                    reference.clone()
                } else {
                    ensure!(
                        sample.frames.len() <= SampleAsset::MAX_FRAMES
                            && sample
                                .frames
                                .iter()
                                .all(|v| v.is_finite() && (-1.0..=1.0).contains(v)),
                        "Invalid OSC sample in replay configuration"
                    );
                    ensure!(
                        self.retained_bytes + sample.frames.len() * 4 <= MAX_ASSET_BYTES,
                        "Replay sample assets exceed the 256 MiB limit"
                    );
                    let bytes: Vec<u8> =
                        sample.frames.iter().flat_map(|v| v.to_le_bytes()).collect();
                    let sha256 = format!("{:x}", Sha256::digest(&bytes));
                    let directory = root.join("samples");
                    fs::create_dir_all(&directory)?;
                    let path = directory.join(format!("{sha256}.f32"));
                    if !path.exists() {
                        crate::project::atomic_write(&path, &bytes)?;
                    }
                    let reference = SampleReference {
                        bank,
                        slot,
                        name: sample.name.clone(),
                        sample_rate: sample.sample_rate,
                        frames: sample.frames.len(),
                        sha256,
                        root_hz: sample.root_hz,
                        cycle_start: sample.cycle_start,
                        cycle_end: sample.cycle_end,
                    };
                    self.retained_bytes += sample.frames.len() * 4;
                    self.known.insert(identity, (sample, reference.clone()));
                    reference
                };
                reference.bank = bank;
                reference.slot = slot;
                refs.push(reference);
            }
        }
        Ok(refs)
    }
}
#[derive(Clone)]
pub struct AssetReader {
    root: PathBuf,
    known: HashMap<(String, u32, String, Option<u32>, usize, usize), Arc<SampleAsset>>,
    retained_bytes: usize,
}
impl AssetReader {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_owned(),
            known: HashMap::new(),
            retained_bytes: 0,
        }
    }
    pub fn attach(&mut self, event: &mut Event) -> Result<()> {
        if event.sample_assets.is_empty() {
            return Ok(());
        }
        let EventKind::Config(data) = &mut event.kind else {
            anyhow::bail!("Only configuration events can reference samples");
        };
        self.attach_data(data, &event.sample_assets)
    }
    pub fn attach_data(
        &mut self,
        data: &mut ProjectData,
        references: &[SampleReference],
    ) -> Result<()> {
        ensure!(references.len() <= 16, "Too many replay sample references");
        for reference in references {
            ensure!(
                reference.frames <= SampleAsset::MAX_FRAMES
                    && (8000..=192000).contains(&reference.sample_rate),
                "Invalid replay sample length or rate"
            );
            ensure!(
                reference.sha256.len() == 64
                    && reference.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
                "Invalid replay sample hash"
            );
            let key = (
                reference.sha256.clone(),
                reference.sample_rate,
                reference.name.clone(),
                reference.root_hz.map(f32::to_bits),
                reference.cycle_start,
                reference.cycle_end,
            );
            let sample = if let Some(sample) = self.known.get(&key) {
                sample.clone()
            } else {
                ensure!(
                    self.retained_bytes + reference.frames * 4 <= MAX_ASSET_BYTES,
                    "Replay sample assets exceed the 256 MiB limit"
                );
                let root = fs::canonicalize(&self.root)?;
                let path = fs::canonicalize(
                    root.join("samples")
                        .join(format!("{}.f32", reference.sha256)),
                )?;
                ensure!(
                    path.starts_with(&root),
                    "Replay sample escaped its library folder"
                );
                ensure!(
                    fs::metadata(&path)?.len() == reference.frames as u64 * 4,
                    "Replay sample length mismatch"
                );
                ensure!(
                    session::checksum(&path)? == reference.sha256,
                    "Replay sample checksum mismatch"
                );
                let bytes = fs::read(path)?;
                let frames: Vec<f32> = bytes
                    .chunks_exact(4)
                    .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
                    .collect();
                ensure!(
                    frames
                        .iter()
                        .all(|v| v.is_finite() && (-1.0..=1.0).contains(v)),
                    "Invalid replay sample"
                );
                let mut sample =
                    SampleAsset::new(reference.name.clone(), reference.sample_rate, frames);
                sample.root_hz = reference.root_hz;
                sample.cycle_start = reference.cycle_start;
                sample.cycle_end = reference.cycle_end;
                let sample = Arc::new(sample);
                self.retained_bytes += reference.frames * 4;
                self.known.insert(key, sample.clone());
                sample
            };
            let osc = data
                .input_fx
                .banks
                .get_mut(reference.bank)
                .and_then(|bank| bank.slots.get_mut(reference.slot))
                .and_then(|slot| slot.osc.as_mut())
                .context("Replay sample targets a missing OSC")?;
            ensure!(osc.sample.is_none(), "Duplicate replay sample reference");
            osc.sample = Some(sample);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_config_events_share_exact_pcm_assets_and_detect_tampering() {
        let root = PathBuf::from("var").join(format!("replay-assets-test-{}", session::id()));
        fs::create_dir_all(&root).unwrap();
        let mut config = AppConfig::new(120, 0, 5);
        config
            .input_fx
            .set_slot_kind(0, 0, crate::config::FxKind::Oscillator);
        let frames: Vec<f32> = (0..SampleAsset::MAX_FRAMES)
            .map(|i| (i as f32 * 0.03).sin() * 0.5)
            .collect();
        if let Some(crate::config::InputFx::Oscillator(osc)) =
            &mut config.input_fx.banks[0].slots[0].fx
        {
            let mut sample = SampleAsset::new("Wave".into(), 48000, frames.clone());
            sample.root_hz = Some(220.0);
            sample.cycle_start = 40;
            sample.cycle_end = 250;
            osc.sample = Some(Arc::new(sample));
        }
        let mut writer = AssetWriter::default();
        let mut events = Vec::new();
        for frame in 0..12 {
            let mut data = crate::project::data_from_config(&config);
            let sample_assets = writer.detach(&root, &mut data).unwrap();
            assert!(
                data.input_fx.banks[0].slots[0]
                    .osc
                    .as_ref()
                    .unwrap()
                    .sample
                    .is_none()
            );
            events.push(Event {
                frame,
                sequence: frame,
                kind: EventKind::Config(data),
                sample_assets,
            });
        }
        assert_eq!(fs::read_dir(root.join("samples")).unwrap().count(), 1);
        let mut reader = AssetReader::new(&root);
        let mut first = None;
        for event in &mut events {
            reader.attach(event).unwrap();
            let EventKind::Config(data) = &event.kind else {
                unreachable!()
            };
            let sample = data.input_fx.banks[0].slots[0]
                .osc
                .as_ref()
                .unwrap()
                .sample
                .as_ref()
                .unwrap();
            assert_eq!(sample.frames, frames);
            assert_eq!(
                (sample.root_hz, sample.cycle_start, sample.cycle_end),
                (Some(220.0), 40, 250)
            );
            if let Some(first) = &first {
                assert!(Arc::ptr_eq(first, sample));
            } else {
                first = Some(sample.clone());
            }
        }
        let mut event = events[0].clone();
        if let EventKind::Config(data) = &mut event.kind {
            data.input_fx.banks[0].slots[0].osc.as_mut().unwrap().sample = None;
        }
        let path = fs::read_dir(root.join("samples"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let mut bytes = fs::read(&path).unwrap();
        bytes[4] ^= 1;
        fs::write(path, bytes).unwrap();
        assert!(AssetReader::new(&root).attach(&mut event).is_err());
        event.sample_assets[0].sha256 = "../outside".into();
        assert!(AssetReader::new(&root).attach(&mut event).is_err());
        let root = fs::canonicalize(root).unwrap();
        assert!(root.starts_with(fs::canonicalize("var").unwrap()));
        fs::remove_dir_all(root).unwrap();
    }
}
