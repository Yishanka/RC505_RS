//! Projects refer to explicitly saved sound presets; runtime/replays carry PCM.
use super::*;
use crate::config::{
    InputFx,
    osc_configs::{SampleAsset, SavedSampleRef},
};
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub(super) fn mark_saved(config: &mut AppConfig, target: FxTarget, name: &str, text: &str) {
    let FxTarget::Input { bank, slot } = target else {
        return;
    };
    let Some(InputFx::Oscillator(osc)) = &mut config.input_fx.banks[bank].slots[slot].fx else {
        return;
    };
    let Some(sample) = &osc.sample else {
        return;
    };
    osc.sample_ref = Some(SavedSampleRef {
        preset: name.trim().into(),
        sha256: format!("{:x}", Sha256::digest(text.as_bytes())),
        content_hash: sample.content_hash,
        sample_rate: sample.sample_rate,
        frames: sample.frames.len(),
    });
    osc.sample_temporary = false;
}

pub fn read_saved_sample(reference: &SavedSampleRef) -> Result<Arc<SampleAsset>> {
    anyhow::ensure!(
        reference.sha256.len() == 64 && reference.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid saved sound checksum"
    );
    let directory = fs::canonicalize(root()).context("Saved sound library is missing")?;
    let path = fs::canonicalize(file(&reference.preset)?)
        .with_context(|| format!("Saved sound is missing: {}", reference.preset))?;
    anyhow::ensure!(
        path.starts_with(&directory),
        "Saved sound escaped its library folder"
    );
    anyhow::ensure!(
        fs::metadata(&path)?.len() <= 16 * 1024 * 1024,
        "Sound preset exceeds the size limit"
    );
    let bytes = fs::read(&path)?;
    anyhow::ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == reference.sha256,
        "Saved sound changed: {}",
        reference.preset
    );
    let preset: Preset = serde_json::from_slice(&bytes).context("Invalid saved sound")?;
    anyhow::ensure!(
        preset.version == 1 || preset.version == 2,
        "Unsupported saved sound version"
    );
    let SlotData::Input(slot) = preset.slot else {
        bail!("Saved sound is not an input source");
    };
    let sample = slot
        .osc
        .and_then(|osc| osc.sample)
        .context("Saved sound has no sample")?;
    anyhow::ensure!(
        sample.frames.len() >= 4 && sample.frames.len() <= SampleAsset::MAX_FRAMES,
        "Invalid saved sample length"
    );
    let sample = SampleAsset::validated_shared(&sample);
    anyhow::ensure!(reference.matches(&sample), "Saved sample metadata changed");
    Ok(sample)
}

/// Called by project loading on a worker, never by DSP/runtime conversion.
pub fn resolve_project_samples(data: &mut project::ProjectData) {
    for bank in &mut data.input_fx.banks {
        for slot in &mut bank.slots {
            let Some(osc) = &mut slot.osc else {
                continue;
            };
            if osc.sample.is_some() {
                continue;
            }
            if let Some(reference) = &osc.sample_ref {
                match read_saved_sample(reference) {
                    Ok(sample) => {
                        osc.sample = Some(sample);
                        osc.sample_error = None;
                    }
                    Err(error) => osc.sample_error = Some(format!("{}: {error}", reference.preset)),
                }
            }
        }
    }
}

/// A portable replay owns its source PCM. A missing library in the receiving
/// project must not block import or point at a different sound with the same name.
pub fn localize_replay_samples(data: &mut project::ProjectData) {
    for bank in &mut data.input_fx.banks {
        for slot in &mut bank.slots {
            let Some(osc) = &mut slot.osc else {
                continue;
            };
            if osc.sample.as_ref().is_some_and(|sample| {
                osc.sample_ref.as_ref().is_some_and(|reference| {
                    !reference.matches(sample) || read_saved_sample(reference).is_err()
                })
            }) {
                osc.sample_ref = None;
                osc.sample_temporary = true;
                osc.sample_error = None;
            }
        }
    }
}
