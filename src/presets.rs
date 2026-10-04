//! Versioned single-slot presets reuse the project codec, so old parameters have
//! one migration and validation path. Loading never changes audio devices/BPM.
use crate::{
    config::AppConfig,
    project::{self, FxSlotData, TrackFxSlotData},
};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

mod sample_reference;
pub use sample_reference::{localize_replay_samples, read_saved_sample, resolve_project_samples};

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
    let mut slot = match target {
        FxTarget::Input { bank, slot } => {
            SlotData::Input(data.input_fx.banks[bank].slots.remove(slot))
        }
        FxTarget::Track { bank, slot } => {
            SlotData::Track(data.track_fx.banks[bank].slots.remove(slot))
        }
    };
    if let SlotData::Input(slot) = &mut slot {
        slot.source_id.clear();
        slot.clip_link = None;
        slot.detached_clip = None;
        if let Some(osc) = &mut slot.osc {
            // Saving a sound is the explicit retention action. Keep it portable:
            // the preset owns its PCM; projects only refer to this immutable file.
            osc.sample_ref = None;
            osc.sample_temporary = false;
            osc.sample_error = None;
            osc.phrase_serial = 0;
            osc.pending_clip = None;
            osc.clip = None;
            osc.note_seq.clear();
            osc.note_step_len_seq.clear();
        }
        if let Some(osc) = &mut slot.my_delay {
            osc.note_seq.clear();
            osc.note_step_len_seq.clear();
        }
    }
    Ok(serde_json::to_string_pretty(&Preset { version: 2, slot })?)
}

pub fn decode(config: &mut AppConfig, target: FxTarget, text: &str) -> Result<()> {
    let preset: Preset = serde_json::from_str(text).context("Invalid preset JSON")?;
    if preset.version != 1 && preset.version != 2 {
        bail!("Unsupported preset version {}", preset.version);
    }
    let mut staging = AppConfig::new(120, 0, 5);
    let mut data = project::data_from_config(&staging);
    match (target, preset.slot) {
        (FxTarget::Input { bank, slot }, SlotData::Input(value)) => {
            let playback = note_mut(config, target).map(|n| (n.launch_serial, n.pending.clone()));
            let previous = clip(config, target)
                .or_else(|| config.input_fx.banks[bank].slots[slot].clip.clone());
            data.input_fx.banks[0].slots[0] = value;
            project::apply_data_to_config(&mut staging, data);
            // Keep live bypass state; loading a preset should not turn an effect on.
            config.input_fx.banks[bank].slots[slot].fx =
                staging.input_fx.banks[0].slots[0].fx.take();
            config.input_fx.banks[bank].slots[slot].parameter_lane =
                std::mem::take(&mut staging.input_fx.banks[0].slots[0].parameter_lane);
            if let Some(note) = note_mut(config, target) {
                if let Some(previous) = &previous {
                    note.set_clip(previous);
                } else {
                    note.replace_events(0, &[]);
                }
                if let Some((serial, pending)) = playback {
                    note.launch_serial = serial;
                    note.pending = pending;
                }
            }
            config.input_fx.banks[bank].slots[slot].clip = previous;
        }
        (FxTarget::Track { bank, slot }, SlotData::Track(value)) => {
            data.track_fx.banks[0].slots[0] = value;
            project::apply_data_to_config(&mut staging, data);
            config.track_fx.banks[bank].slots[slot].fx =
                staging.track_fx.banks[0].slots[0].fx.take();
            config.track_fx.banks[bank].slots[slot].parameter_lane =
                std::mem::take(&mut staging.track_fx.banks[0].slots[0].parameter_lane);
        }
        _ => bail!("Input FX and Track FX presets belong to different chains."),
    }
    Ok(())
}

pub fn save(config: &mut AppConfig, target: FxTarget, name: &str) -> Result<()> {
    let path = file(name)?;
    if let FxTarget::Input { bank, slot } = target {
        if let Some(crate::config::InputFx::Oscillator(osc)) =
            &config.input_fx.banks[bank].slots[slot].fx
        {
            if let Some(sample) = &osc.sample {
                anyhow::ensure!(
                    (4..=crate::config::osc_configs::SampleAsset::MAX_FRAMES)
                        .contains(&sample.frames.len())
                        && sample
                            .frames
                            .iter()
                            .all(|x| x.is_finite() && (-1.0..=1.0).contains(x)),
                    "Capture or import a valid sample before saving this sound"
                );
            }
        }
    }
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
    output.sync_all()?;
    sample_reference::mark_saved(config, target, name, &text);
    Ok(())
}

pub fn load(config: &mut AppConfig, target: FxTarget, name: &str) -> Result<()> {
    let text = fs::read_to_string(file(name)?)?;
    decode(config, target, &text)?;
    sample_reference::mark_saved(config, target, name, &text);
    Ok(())
}

/// A browsed sound is separate from the live project until explicitly applied.
/// Keep the validated text so a file changing on disk cannot change what Apply
/// means after the user has auditioned it.
pub struct SoundCandidate {
    pub name: String,
    pub target: FxTarget,
    text: String,
    source_id: Option<String>,
    pub source: CandidateSource,
    pub kind: &'static str,
    stored: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateSource {
    Phrase,
    SingleNote,
    LiveInput,
    TrackLoop,
    Empty,
}
impl SoundCandidate {
    pub fn load(config: &AppConfig, target: FxTarget, name: &str) -> Result<Self> {
        let mut candidate = Self::from_text(
            config,
            target,
            name.to_owned(),
            fs::read_to_string(file(name)?)?,
        )?;
        candidate.stored = true;
        Ok(candidate)
    }
    pub fn from_text(
        config: &AppConfig,
        target: FxTarget,
        name: String,
        text: String,
    ) -> Result<Self> {
        let source_id = match target {
            FxTarget::Input { bank, slot } => {
                Some(config.input_fx.banks[bank].slots[slot].source_id.clone())
            }
            _ => None,
        };
        let mut candidate = Self {
            name,
            target,
            text,
            source_id,
            source: CandidateSource::Empty,
            kind: "Empty",
            stored: false,
        };
        let staging = candidate.staging(config)?;
        candidate.kind = match target {
            FxTarget::Input { bank, slot } => {
                staging.input_fx.banks[bank].slots[slot].kind().name()
            }
            FxTarget::Track { bank, slot } => {
                staging.track_fx.banks[bank].slots[slot].kind().name()
            }
        };
        candidate.source = match target {
            FxTarget::Input { bank, slot } => match &staging.input_fx.banks[bank].slots[slot].fx {
                Some(crate::config::InputFx::Oscillator(osc)) => {
                    if osc.note.event_slice().is_empty() {
                        CandidateSource::SingleNote
                    } else {
                        CandidateSource::Phrase
                    }
                }
                Some(_) => CandidateSource::LiveInput,
                None => CandidateSource::Empty,
            },
            FxTarget::Track { bank, slot } => {
                if staging.track_fx.banks[bank].slots[slot].fx.is_some() {
                    CandidateSource::TrackLoop
                } else {
                    CandidateSource::Empty
                }
            }
        };
        Ok(candidate)
    }
    pub fn matches_target(&self, config: &AppConfig, target: Option<FxTarget>) -> bool {
        target == Some(self.target)
            && match self.target {
                FxTarget::Input { bank, slot } => {
                    self.source_id.as_ref()
                        == Some(&config.input_fx.banks[bank].slots[slot].source_id)
                }
                FxTarget::Track { .. } => true,
            }
    }
    pub fn staging(&self, config: &AppConfig) -> Result<AppConfig> {
        anyhow::ensure!(
            self.matches_target(config, Some(self.target)),
            "The candidate source moved; select the sound again"
        );
        let mut staging = AppConfig::new(120, 0, 5);
        project::apply_data_to_config(&mut staging, project::data_from_config(config));
        decode(&mut staging, self.target, &self.text)?;
        Ok(staging)
    }
    pub fn apply(&self, config: &mut AppConfig) -> Result<()> {
        anyhow::ensure!(
            self.matches_target(config, Some(self.target)),
            "The candidate source moved; select the sound again"
        );
        decode(config, self.target, &self.text)?;
        if self.stored {
            // The preview owns the browsed bytes. If someone replaced the file,
            // keep that selected sound embedded rather than link to new bytes.
            if fs::read_to_string(file(&self.name)?).is_ok_and(|text| text == self.text) {
                sample_reference::mark_saved(config, self.target, &self.name, &self.text);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{FxKind, InputFx};
    #[test]
    fn candidate_selection_and_cancel_do_not_mutate_live_patch_and_apply_keeps_music() {
        let mut config = AppConfig::new(137, 41, 5);
        let source = FxTarget::Input { bank: 0, slot: 0 };
        let target = FxTarget::Input { bank: 1, slot: 2 };
        config.input_fx.set_slot_kind(0, 0, FxKind::Oscillator);
        config.input_fx.set_slot_kind(1, 2, FxKind::Oscillator);
        if let Some(InputFx::Oscillator(osc)) = &mut config.input_fx.banks[0].slots[0].fx {
            osc.level.value = 17;
        }
        note_mut(&mut config, target).unwrap().push();
        let music = clip(&config, target).unwrap();
        config.input_fx.banks[1].slots[2].clip_link = Some("existing-link".into());
        let text = encode(&config, source).unwrap();
        let before = serde_json::to_vec(&project::data_from_config(&config)).unwrap();
        let candidate =
            SoundCandidate::from_text(&config, target, "Quiet patch".into(), text.clone()).unwrap();
        assert_eq!(candidate.source, CandidateSource::Phrase);
        let staged = candidate.staging(&config).unwrap();
        assert_eq!(clip(&staged, target).unwrap(), music);
        let Some(InputFx::Oscillator(osc)) = &staged.input_fx.banks[1].slots[2].fx else {
            panic!()
        };
        assert_eq!(osc.level.value, 17);
        assert_eq!(
            before,
            serde_json::to_vec(&project::data_from_config(&config)).unwrap()
        );
        drop(candidate);
        assert_eq!(
            before,
            serde_json::to_vec(&project::data_from_config(&config)).unwrap()
        );
        let candidate =
            SoundCandidate::from_text(&config, target, "Quiet patch".into(), text).unwrap();
        candidate.apply(&mut config).unwrap();
        assert_eq!(clip(&config, target).unwrap(), music);
        assert_eq!(
            config.input_fx.banks[1].slots[2].clip_link.as_deref(),
            Some("existing-link")
        );
        assert!(!config.input_fx.banks[1].slots[2].is_enabled);
        assert_eq!(config.beat_config.current_bpm(), 137);
        let Some(InputFx::Oscillator(osc)) = &config.input_fx.banks[1].slots[2].fx else {
            panic!()
        };
        assert_eq!(osc.level.value, 17);
        config.input_fx.banks[1].slots.swap(2, 3);
        assert!(candidate.apply(&mut config).is_err());
    }
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
        assert_eq!(
            osc.note.events().len(),
            0,
            "Sound presets must not replace the phrase"
        );
        assert_eq!(config.beat_config.current_bpm(), 137);
        assert!(!config.input_fx.banks[1].slots[2].is_enabled);
        assert!(decode(&mut config, FxTarget::Track { bank: 0, slot: 0 }, &encoded).is_err());
    }
    #[test]
    fn sound_swap_type_change_and_slot_move_preserve_independent_phrase_identity() {
        use crate::config::note_configs::NoteOct;
        use crate::config::sequence_edit::NoteEvent;
        let mut c = AppConfig::new(120, 0, 5);
        c.input_fx.set_slot_kind(0, 0, FxKind::Oscillator);
        c.input_fx.set_slot_kind(0, 1, FxKind::Oscillator);
        let target = FxTarget::Input { bank: 0, slot: 1 };
        note_mut(&mut c, target).unwrap().replace_events(
            3840,
            &[
                NoteEvent::new(0, 960, NoteOct::from_pitch_index(48)),
                NoteEvent::new(0, 960, NoteOct::from_pitch_index(52)),
                NoteEvent::new(0, 960, NoteOct::from_pitch_index(55)),
            ],
        );
        note_mut(&mut c, target).unwrap().clip_id = "independent-destination-phrase".into();
        let before = clip(&c, target).unwrap();
        let source = c.input_fx.banks[0].slots[1].source_id.clone();
        let encoded = encode(&c, FxTarget::Input { bank: 0, slot: 0 }).unwrap();
        decode(&mut c, target, &encoded).unwrap();
        assert_eq!(clip(&c, target).unwrap(), before);
        assert_eq!(c.input_fx.banks[0].slots[1].source_id, source);
        c.input_fx.set_slot_kind(0, 1, FxKind::Filter);
        assert_eq!(c.input_fx.banks[0].slots[1].clip.as_ref(), Some(&before));
        let data = project::data_from_config(&c);
        let json = serde_json::to_string(&data).unwrap();
        let mut restored = AppConfig::new(120, 0, 5);
        project::apply_data_to_config(&mut restored, serde_json::from_str(&json).unwrap());
        restored.input_fx.set_slot_kind(0, 1, FxKind::Oscillator);
        assert_eq!(clip(&restored, target).unwrap(), before);
        restored.input_fx.banks[0].slots.swap(1, 2);
        assert_eq!(restored.input_fx.banks[0].slots[2].source_id, source);
        assert_eq!(
            clip(&restored, FxTarget::Input { bank: 0, slot: 2 }).unwrap(),
            before
        );
        assert!(
            clip(&restored, FxTarget::Input { bank: 0, slot: 0 })
                .unwrap()
                .events
                .is_empty()
        );
    }
}

pub fn note_mut(
    config: &mut AppConfig,
    target: FxTarget,
) -> Option<&mut crate::config::note_configs::NoteConfigs> {
    let FxTarget::Input { bank, slot } = target else {
        return None;
    };
    match config
        .input_fx
        .banks
        .get_mut(bank)?
        .slots
        .get_mut(slot)?
        .fx
        .as_mut()?
    {
        crate::config::InputFx::Oscillator(o) => Some(&mut o.note),
        crate::config::InputFx::MyDelay(o) => Some(&mut o.note),
        _ => None,
    }
}
pub fn clip(
    config: &AppConfig,
    target: FxTarget,
) -> Option<crate::config::sequence_edit::NoteClip> {
    let FxTarget::Input { bank, slot } = target else {
        return None;
    };
    match config
        .input_fx
        .banks
        .get(bank)?
        .slots
        .get(slot)?
        .fx
        .as_ref()?
    {
        crate::config::InputFx::Oscillator(o) => Some(o.note.clip()),
        crate::config::InputFx::MyDelay(o) => Some(o.note.clip()),
        _ => None,
    }
}
fn clip_root() -> PathBuf {
    root().with_file_name("clips")
}
fn clip_file(name: &str) -> Result<PathBuf> {
    let validated = file(name)?;
    Ok(clip_root().join(validated.file_name().unwrap()))
}
pub fn list_clips() -> Vec<String> {
    let mut names: Vec<_> = fs::read_dir(clip_root())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| {
            e.path()
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
        })
        .collect();
    names.sort();
    names
}
#[derive(Serialize, Deserialize)]
struct ClipFile {
    version: u32,
    clip: crate::config::sequence_edit::NoteClip,
}
pub fn save_clip(config: &AppConfig, target: FxTarget, name: &str) -> Result<()> {
    let mut clip = clip(config, target).context("This effect does not accept notes")?;
    let path = clip_file(name)?;
    clip.name = name.trim().to_owned();
    clip.id = format!(
        "clip-{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    );
    let text = serde_json::to_string_pretty(&ClipFile { version: 1, clip })?;
    fs::create_dir_all(clip_root())?;
    use std::io::Write;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .context("Clip already exists; choose another name")?
        .write_all(text.as_bytes())?;
    Ok(())
}
pub fn load_clip(config: &mut AppConfig, target: FxTarget, name: &str) -> Result<()> {
    load_clip_timed(config, target, name, false)
}
pub fn load_clip_timed(
    config: &mut AppConfig,
    target: FxTarget,
    name: &str,
    next_loop: bool,
) -> Result<()> {
    anyhow::ensure!(
        !next_loop || note_mut(config, target).is_none_or(|n| n.pending.is_none()),
        "Cancel the queued phrase change first"
    );
    let path = clip_file(name)?;
    if fs::metadata(&path)?.len() > 1024 * 1024 {
        bail!("Clip exceeds the 1 MB size limit");
    }
    let mut value: ClipFile = serde_json::from_str(&fs::read_to_string(path)?)?;
    if value.version != 1 {
        bail!("Unsupported clip version");
    }
    // Loading copies the phrase; editing one source never mutates another slot.
    value.clip.id = format!(
        "copy-{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    );
    note_mut(config, target)
        .context("This effect does not accept notes")?
        .launch_clip(&value.clip, next_loop);
    crate::phrases::propagate(config, target);
    Ok(())
}
