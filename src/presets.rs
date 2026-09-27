//! Versioned single-slot presets reuse the project codec, so old parameters have
//! one migration and validation path. Loading never changes audio devices/BPM.
use crate::{
    config::AppConfig,
    project::{self, FxSlotData, TrackFxSlotData},
};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FxTarget {
    Input { bank: usize, slot: usize },
    Track { bank: usize, slot: usize },
}

impl FxTarget {
    pub fn label(self) -> String {
        match self {
            Self::Input { bank, slot } => {
                format!("INPUT / BANK {} / {}", bank + 1, ['A', 'B', 'C', 'D'][slot])
            }
            Self::Track { bank, slot } => format!(
                "TRACK FX / BANK {} / {}",
                bank + 1,
                ['A', 'B', 'C', 'D'][slot]
            ),
        }
    }
}

#[derive(Serialize, Deserialize)]
enum SlotData {
    Input(FxSlotData),
    Track(TrackFxSlotData),
}

#[derive(Serialize, Deserialize)]
struct Preset {
    version: u32,
    slot: SlotData,
}

fn root() -> PathBuf {
    crate::app_support::paths::projects_dir().with_file_name("presets")
}

fn file(name: &str) -> Result<PathBuf> {
    let name = name.trim();
    if name.is_empty()
        || name.len() > 100
        || name
            .chars()
            .any(|c| !c.is_alphanumeric() && c != '-' && c != '_' && c != ' ')
    {
        bail!("Use 1–100 characters: letters, numbers, spaces, - or _.");
    }
    Ok(root().join(format!("{name}.json")))
}

pub fn list() -> Vec<String> {
    let mut names: Vec<_> = fs::read_dir(root())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|e| e == "json"))
        .filter_map(|entry| {
            entry
                .path()
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
        })
        .collect();
    names.sort();
    names
}

pub fn encode(config: &AppConfig, target: FxTarget) -> Result<String> {
    let mut data = project::data_from_config(config);
    let slot = match target {
        FxTarget::Input { bank, slot } => {
            SlotData::Input(data.input_fx.banks[bank].slots.remove(slot))
        }
        FxTarget::Track { bank, slot } => {
            SlotData::Track(data.track_fx.banks[bank].slots.remove(slot))
        }
    };
    Ok(serde_json::to_string_pretty(&Preset { version: 1, slot })?)
}

pub fn decode(config: &mut AppConfig, target: FxTarget, text: &str) -> Result<()> {
    let preset: Preset = serde_json::from_str(text).context("Invalid preset JSON")?;
    if preset.version != 1 {
        bail!("Unsupported preset version {}", preset.version);
    }
    let mut staging = AppConfig::new(120, 0, 5);
    let mut data = project::data_from_config(&staging);
    match (target, preset.slot) {
        (FxTarget::Input { bank, slot }, SlotData::Input(value)) => {
            data.input_fx.banks[0].slots[0] = value;
            project::apply_data_to_config(&mut staging, data);
            // Keep live bypass state; loading a preset should not turn an effect on.
            config.input_fx.banks[bank].slots[slot].fx =
                staging.input_fx.banks[0].slots[0].fx.take();
        }
        (FxTarget::Track { bank, slot }, SlotData::Track(value)) => {
            data.track_fx.banks[0].slots[0] = value;
            project::apply_data_to_config(&mut staging, data);
            config.track_fx.banks[bank].slots[slot].fx =
                staging.track_fx.banks[0].slots[0].fx.take();
        }
        _ => bail!("Input FX and Track FX presets belong to different chains."),
    }
    Ok(())
}

pub fn save(config: &AppConfig, target: FxTarget, name: &str) -> Result<()> {
    let path = file(name)?;
    let text = encode(config, target)?;
    fs::create_dir_all(root())?;
    // Create-new avoids silently replacing a user's preset with the same name.
    use std::io::Write;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .context("Preset already exists or cannot be created; choose a new name")?;
    output.write_all(text.as_bytes())?;
    Ok(())
}

pub fn load(config: &mut AppConfig, target: FxTarget, name: &str) -> Result<()> {
    decode(config, target, &fs::read_to_string(file(name)?)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{FxKind, InputFx};
    #[test]
    fn preset_roundtrip_keeps_devices_tempo_and_bypass() {
        let mut config = AppConfig::new(137, 85, 5);
        let source = FxTarget::Input { bank: 0, slot: 0 };
        config.input_fx.set_slot_kind(0, 0, FxKind::Oscillator);
        if let Some(InputFx::Oscillator(osc)) = &mut config.input_fx.banks[0].slots[0].fx {
            osc.level.value = 42;
            osc.note.push();
            osc.note.push();
        }
        let encoded = encode(&config, source).unwrap();
        decode(&mut config, FxTarget::Input { bank: 1, slot: 2 }, &encoded).unwrap();
        let Some(InputFx::Oscillator(osc)) = &config.input_fx.banks[1].slots[2].fx else {
            panic!()
        };
        assert_eq!(osc.level.value, 42);
        assert_eq!(osc.note.events().len(), 2);
        assert_eq!(config.beat_config.current_bpm(), 137);
        assert!(!config.input_fx.banks[1].slots[2].is_enabled);
        assert!(decode(&mut config, FxTarget::Track { bank: 0, slot: 0 }, &encoded).is_err());
    }
}
