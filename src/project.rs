use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::delay_configs::{
    TRACK_DELAY_DAMP_MAX_HZ, TRACK_DELAY_DAMP_MIN_HZ, TRACK_DELAY_FEEDBACK_MAX_PCT,
    TRACK_DELAY_MIX_MAX_PCT, TRACK_DELAY_TIME_MAX_MS, TRACK_DELAY_TIME_MIN_MS,
};
use crate::config::envelope_configs::{
    ENVELOPE_ATTACK_MAX_MS, ENVELOPE_DECAY_MAX_MS, ENVELOPE_HOLD_MAX_MS, ENVELOPE_RELEASE_MAX_MS,
    ENVELOPE_RELEASE_MIN_MS, ENVELOPE_START_MAX_PCT, ENVELOPE_SUSTAIN_MAX_PCT,
    ENVELOPE_TENSION_MAX,
};
use crate::config::filter_configs::{
    FILTER_CUTOFF_MAX_HZ, FILTER_CUTOFF_MIN_HZ, FILTER_DRIVE_MAX, FILTER_MIX_MAX, FILTER_Q_MAX_X10,
    FILTER_Q_MIN_X10, FilterType,
};
use crate::config::input_fx_configs::{FX_BANK_COUNT, FX_SLOT_COUNT};
use crate::config::mydelay_configs::{MYDELAY_LEVEL_MAX, MYDELAY_THRESHOLD_MAX};
use crate::config::note_configs::{Note, NoteOct};
use crate::config::osc_configs::Waveform;
use crate::config::reverb_configs::{
    REVERB_HIGHCUT_MAX, REVERB_LOWCUT_MAX_HZ, REVERB_LOWCUT_MIN_HZ, REVERB_PREDELAY_MAX_MS,
    REVERB_RT60_MAX_MS, REVERB_RT60_MIN_MS, REVERB_SIZE_MAX, REVERB_WIDTH_MAX,
};
use crate::config::roll_configs::{RollMode, RollStep};
use crate::config::time_mode::TimeMode;
use crate::config::track_fx_configs::{TRACK_FX_BANK_COUNT, TRACK_FX_SLOT_COUNT};
use crate::config::vocoder_configs::{
    VOCODER_ATTACK_MAX_MS, VOCODER_BANDS_MAX, VOCODER_BANDS_MIN, VOCODER_LEVEL_MAX,
    VOCODER_MIX_MAX, VOCODER_RELEASE_MAX_MS, VocoderCarrier,
};
use crate::config::{AppConfig, FxKind, InputFx, TrackFx, TrackFxKind};

const INDEX_FILE: &str = "projects_index.json";

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectEntry {
    pub name: String,
    pub file: String,
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct ProjectIndex {
    projects: Vec<ProjectEntry>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectData {
    #[serde(default)]
    pub input_noise: crate::config::input_noise::InputNoiseConfig,
    #[serde(default)]
    pub pdc_enabled: bool,
    #[serde(default)]
    pub master_fx: crate::config::audio_fx::MasterFxConfig,
    #[serde(default = "default_input_thru")]
    pub input_thru: bool,
    #[serde(default = "default_metronome_volume")]
    pub metronome_volume: f32,
    #[serde(default)]
    pub calibration: Option<crate::config::track_options::LatencyCalibration>,
    #[serde(default)]
    pub snapshot: Option<String>,
    #[serde(default)]
    pub track_options: Vec<crate::config::track_options::TrackOptions>,
    #[serde(default)]
    pub input_routing: crate::config::track_options::InputRouting,
    #[serde(default = "default_fader_speed")]
    pub fader_speed_db: f32,
    #[serde(default)]
    pub track_levels: Vec<f32>,
    pub beat: BeatData,
    pub system: SystemData,
    pub input_fx: InputFxData,
    #[serde(default)]
    pub track_fx: TrackFxData,
}

fn default_input_thru() -> bool {
    true
}
fn default_metronome_volume() -> f32 {
    0.35
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct BeatData {
    pub bpm: usize,
    pub latency: usize,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemData {
    pub input_device: String,
    pub output_device: String,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct InputFxData {
    pub selected_bank_idx: usize,
    pub banks: Vec<FxBankData>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackFxData {
    pub selected_bank_idx: usize,
    #[serde(default)]
    pub banks: Vec<TrackFxBankData>,
    #[serde(default)]
    pub tracks: Vec<TrackFxTrackData>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackFxTrackData {
    #[serde(default)]
    pub enabled: Vec<Vec<bool>>,
    #[serde(default)]
    pub banks: Vec<TrackFxBankData>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackFxBankData {
    pub slots: Vec<TrackFxSlotData>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackFxSlotData {
    #[serde(default)]
    pub parameter_lane: crate::config::automation::ParameterLane,
    #[serde(default)]
    pub vocoder: Option<VocoderData>,
    #[serde(default)]
    pub audio: Option<crate::config::audio_fx::AudioFxConfig>,
    pub is_enabled: bool,
    pub kind: String,
    #[serde(default)]
    pub delay: Option<TrackDelayData>,
    #[serde(default)]
    pub roll: Option<TrackRollData>,
    #[serde(default)]
    pub filter: Option<TrackFilterData>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackDelayData {
    #[serde(default)]
    pub feedback_repeats: usize,
    #[serde(default)]
    pub direct_pct: Option<usize>,
    #[serde(default)]
    pub effect_pct: Option<usize>,
    #[serde(default)]
    pub low_cut_hz: usize,
    #[serde(default)]
    pub time_mode: TimeMode,
    pub time_ms: f32,
    pub feedback_pct: usize,
    pub high_damp_hz: usize,
    pub mix_pct: usize,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackRollData {
    pub step: usize,
    #[serde(default = "default_roll_time_mode")]
    pub time_mode: TimeMode,
    #[serde(default = "default_roll_time")]
    pub time_ms: usize,
    #[serde(default)]
    pub mode: RollMode,
    #[serde(default = "default_roll_feedback")]
    pub feedback: usize,
    #[serde(default)]
    pub repeat: usize,
    #[serde(default = "default_full_level")]
    pub mix: usize,
}
fn default_roll_time_mode() -> TimeMode {
    TimeMode::Quarter
}
fn default_roll_time() -> usize {
    200
}
fn default_roll_feedback() -> usize {
    50
}
fn default_full_level() -> usize {
    100
}
fn default_reverb_wet() -> usize {
    35
}
fn default_legacy_density() -> usize {
    1
}
fn default_sibilance() -> usize {
    20
}
fn default_fader_speed() -> f32 {
    24.0
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackFilterData {
    #[serde(default)]
    pub filter: FilterData,
    #[serde(default)]
    pub seq_step: String,
    #[serde(default)]
    pub seq: Vec<bool>,
    #[serde(default)]
    pub seq_step_len_seq: Vec<usize>,
    #[serde(default)]
    pub env: EnvelopeData,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct FxBankData {
    pub slots: Vec<FxSlotData>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct FxSlotData {
    #[serde(default)]
    pub parameter_lane: crate::config::automation::ParameterLane,
    #[serde(default)]
    pub clip_link: Option<String>,
    #[serde(default)]
    pub roll: Option<TrackRollData>,
    #[serde(default)]
    pub audio: Option<crate::config::audio_fx::AudioFxConfig>,
    #[serde(default)]
    pub source_id: String,
    #[serde(default)]
    pub detached_clip: Option<crate::config::sequence_edit::NoteClip>,
    pub is_enabled: bool,
    pub kind: String,
    pub osc: Option<OscData>,
    pub filter: Option<FilterData>,
    #[serde(default)]
    pub reverb: Option<ReverbData>,
    #[serde(default)]
    pub my_delay: Option<MyDelayData>,
    #[serde(default)]
    pub vocoder: Option<VocoderData>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct OscData {
    #[serde(default = "default_osc_dry_level")]
    pub dry_level: f32,
    #[serde(default)]
    pub input_mod_sens: Option<f32>,
    #[serde(default)]
    pub phrase_serial: u64,
    #[serde(default)]
    pub pending_clip: Option<crate::config::sequence_edit::PendingClip>,
    #[serde(default)]
    pub clip: Option<crate::config::sequence_edit::NoteClip>,
    #[serde(default = "default_osc_voices")]
    pub voices: usize,
    #[serde(default)]
    pub input_gate: bool,
    #[serde(default)]
    pub lfo: crate::config::osc_configs::LfoConfig,
    #[serde(default)]
    pub lfo2: crate::config::osc_configs::LfoConfig,
    #[serde(default)]
    pub mono_legato: bool,
    #[serde(default)]
    pub glide_ms: f32,
    #[serde(default)]
    pub glide_mode: crate::config::osc_configs::GlideMode,
    #[serde(default)]
    pub sample: Option<std::sync::Arc<crate::config::osc_configs::SampleAsset>>,
    #[serde(default)]
    pub sample_ref: Option<crate::config::osc_configs::SavedSampleRef>,
    // Old embedded samples were already saved; keep them until explicitly replaced.
    #[serde(default)]
    pub sample_temporary: bool,
    #[serde(skip)]
    pub sample_error: Option<String>,
    #[serde(default)]
    pub sample_mode: crate::config::osc_configs::SampleMode,
    #[serde(default = "default_sample_root")]
    pub sample_root: usize,
    #[serde(default)]
    pub sample_fine_cents: f32,
    #[serde(default = "default_sample_loop")]
    pub sample_loop: bool,
    #[serde(default)]
    pub sample_start: f32,
    #[serde(default = "default_sample_end")]
    pub sample_end: f32,
    #[serde(default)]
    pub vocal_formant: f32,
    #[serde(default)]
    pub capture_serial: u64,
    #[serde(default = "default_capture_ms")]
    pub capture_ms: usize,
    pub waveform: String,
    pub level: usize,
    pub threshold: usize,
    pub note_current: String,
    pub octave_current: usize,
    pub step: String,
    pub note_seq: Vec<Option<NoteOctData>>,
    pub note_step_len_seq: Vec<usize>,
    #[serde(default)]
    pub envelope: EnvelopeData,
    #[serde(default)]
    pub osc_filter: FilterData,
    #[serde(default)]
    pub osc_filter_envelope: EnvelopeData,
}

fn default_osc_dry_level() -> f32 {
    1.0
}
fn default_osc_voices() -> usize {
    8
}
fn default_sample_root() -> usize {
    48
}
fn default_sample_loop() -> bool {
    true
}
fn default_sample_end() -> f32 {
    1.0
}
fn default_capture_ms() -> usize {
    100
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteOctData {
    pub note: String,
    pub octave: usize,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvelopeData {
    pub attack_ms: usize,
    pub hold_ms: usize,
    pub decay_ms: usize,
    pub sustain_pct: usize,
    pub release_ms: usize,
    #[serde(default)]
    pub start_pct: usize,
    #[serde(default = "default_tension_value")]
    pub tension_a: usize,
    #[serde(default = "default_tension_value")]
    pub tension_d: usize,
    #[serde(default = "default_tension_value")]
    pub tension_r: usize,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct FilterData {
    #[serde(default)]
    pub sweep: crate::config::filter_configs::FilterSweepConfig,
    pub filter_type: String,
    pub cutoff_hz: usize,
    pub resonance_x10: usize,
    pub drive: usize,
    pub mix: usize,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct MyDelayData {
    pub level: usize,
    pub threshold: usize,
    pub note_current: String,
    pub octave_current: usize,
    pub step: String,
    pub note_seq: Vec<Option<NoteOctData>>,
    pub note_step_len_seq: Vec<usize>,
    pub filter: FilterData,
    #[serde(default)]
    pub audio_env: EnvelopeData,
    #[serde(default)]
    pub filter_env: EnvelopeData,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ReverbData {
    #[serde(default)]
    pub high_cut_hz: Option<usize>,
    #[serde(default = "default_full_level")]
    pub dry_level: usize,
    #[serde(default = "default_reverb_wet")]
    pub wet_level: usize,
    #[serde(default = "default_legacy_density")]
    pub density: usize,
    pub size: usize,
    pub decay_ms: usize,
    pub predelay_ms: usize,
    pub width: usize,
    pub high_cut: usize,
    pub low_cut: usize,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct VocoderData {
    #[serde(default)]
    pub tone: i32,
    #[serde(default)]
    pub mod_sens: i32,
    #[serde(default)]
    pub formant_semitones: i32,
    #[serde(default = "default_sibilance")]
    pub sibilance: usize,
    #[serde(default)]
    pub carrier_thru: bool,
    pub carrier: String,
    pub bands: usize,
    pub attack_ms: usize,
    pub release_ms: usize,
    pub level: usize,
    pub mix: usize,
}

impl Default for EnvelopeData {
    fn default() -> Self {
        Self {
            attack_ms: 20,
            hold_ms: 40,
            decay_ms: 180,
            sustain_pct: 70,
            release_ms: 120,
            start_pct: 0,
            tension_a: default_tension_value(),
            tension_d: default_tension_value(),
            tension_r: default_tension_value(),
        }
    }
}

impl Default for FilterData {
    fn default() -> Self {
        Self {
            sweep: Default::default(),
            filter_type: "LPF".to_string(),
            cutoff_hz: 1000,
            resonance_x10: 7,
            drive: 0,
            mix: 100,
        }
    }
}

impl Default for TrackFxData {
    fn default() -> Self {
        Self {
            selected_bank_idx: 0,
            banks: Vec::new(),
            tracks: Vec::new(),
        }
    }
}

pub fn load_index() -> Vec<ProjectEntry> {
    let path = index_path();
    if !path.exists() {
        return recover_index();
    }
    let content = fs::read_to_string(path);
    match content {
        Ok(raw) => match serde_json::from_str::<ProjectIndex>(&raw) {
            Ok(idx) => idx.projects,
            Err(_) => recover_index(),
        },
        Err(_) => recover_index(),
    }
}

pub fn save_index(entries: &[ProjectEntry]) -> anyhow::Result<()> {
    ensure_project_dir()?;
    let idx = ProjectIndex {
        projects: entries.to_vec(),
    };
    let raw = serde_json::to_string_pretty(&idx)?;
    if index_path().exists() {
        let previous = fs::read(index_path())?;
        if serde_json::from_slice::<ProjectIndex>(&previous).is_ok() {
            atomic_write(&index_path().with_extension("json.bak"), &previous)?;
        }
    }
    atomic_write(&index_path(), raw.as_bytes())?;
    Ok(())
}

pub fn load_project(entry: &ProjectEntry) -> anyhow::Result<Option<ProjectData>> {
    let path = crate::session::safe_child(&projects_root(), &entry.file)?;
    if !path.exists() {
        anyhow::bail!(
            "Project configuration is missing: {}. The index entry has been preserved.",
            entry.file
        );
    }
    let raw = fs::read_to_string(&path)?;
    let mut data = serde_json::from_str::<ProjectData>(&raw)?;
    crate::presets::resolve_project_samples(&mut data);
    Ok(Some(data))
}

/// Read the pointer without loading or resolving any OSC sound-library assets.
pub fn saved_snapshot(entry: &ProjectEntry) -> anyhow::Result<Option<String>> {
    #[derive(Deserialize)]
    struct Pointer {
        #[serde(default)]
        snapshot: Option<String>,
    }
    let path = crate::session::safe_child(&projects_root(), &entry.file)?;
    let reader = std::io::BufReader::new(fs::File::open(path)?);
    Ok(serde_json::from_reader::<_, Pointer>(reader)?.snapshot)
}

pub fn temporary_sample_count(config: &AppConfig) -> usize {
    config.input_fx.banks.iter().flat_map(|bank|&bank.slots).filter(|slot|
        matches!(&slot.fx,Some(InputFx::Oscillator(osc)) if osc.sample_temporary&&osc.sample.is_some())).count()
}

pub fn missing_saved_sample_count(config: &AppConfig) -> usize {
    config.input_fx.banks.iter().flat_map(|bank|&bank.slots).filter(|slot|
        matches!(&slot.fx,Some(InputFx::Oscillator(osc)) if osc.sample.is_none()&&osc.sample_ref.is_some())).count()
}

pub fn atomic_write(path: &std::path::Path, bytes: &[u8]) -> anyhow::Result<()> {
    use std::io::Write;
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    let result = (|| -> anyhow::Result<()> {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn save_project(entry: &ProjectEntry, config: &AppConfig) -> anyhow::Result<()> {
    ensure_project_dir()?;
    let mut data = data_from_config(config);
    data.snapshot = saved_snapshot(entry)?;
    save_project_data(entry, &data)
}

/// Normal project saves do not implicitly turn a temporary capture into an asset.
/// Runtime transfer and replay capture continue using the complete in-memory data.
pub fn persistable_data(data: &ProjectData) -> anyhow::Result<ProjectData> {
    let mut result = data.clone();
    for bank in &mut result.input_fx.banks {
        for slot in &mut bank.slots {
            let Some(osc) = &mut slot.osc else {
                continue;
            };
            osc.sample_error = None;
            if osc.sample_temporary {
                osc.sample = None;
                osc.sample_ref = None;
            } else if let Some(reference) = &osc.sample_ref {
                if let Some(sample) = &osc.sample {
                    anyhow::ensure!(
                        reference.matches(sample),
                        "Sample changed; save it as a sound before keeping it in the project"
                    );
                    crate::presets::read_saved_sample(reference)?;
                }
                osc.sample = None;
            }
        }
    }
    Ok(result)
}

pub fn save_project_data(entry: &ProjectEntry, data: &ProjectData) -> anyhow::Result<()> {
    use fs2::FileExt;
    let data = persistable_data(data)?;
    ensure_project_dir()?;
    let path = crate::session::safe_child(&projects_root(), &entry.file)?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path.with_extension("lock"))?;
    lock.try_lock_exclusive()
        .map_err(|_| anyhow::anyhow!("Project is being saved by another process"))?;
    if path.exists() {
        fs::copy(&path, path.with_extension("json.bak"))?;
    }
    atomic_write(&path, &serde_json::to_vec_pretty(&data)?)
}

pub fn remove_project_file(file: &str) {
    let path = project_file_path(file);
    let _ = fs::remove_file(path);
}

pub fn make_project_file_name(name: &str, idx: usize) -> String {
    let safe = sanitize_name(name);
    format!("{}_{}_{}.json", safe, idx, crate::session::id())
}

fn recover_index() -> Vec<ProjectEntry> {
    if let Ok(raw) = fs::read(index_path().with_extension("json.bak")) {
        if let Ok(index) = serde_json::from_slice::<ProjectIndex>(&raw) {
            return index.projects;
        }
    }
    fs::read_dir(projects_root())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|v| v.to_str()) != Some("json") {
                return None;
            }
            let data = fs::read(&path).ok()?;
            serde_json::from_slice::<ProjectData>(&data).ok()?;
            Some(ProjectEntry {
                name: path.file_stem()?.to_string_lossy().into_owned(),
                file: path.file_name()?.to_string_lossy().into_owned(),
            })
        })
        .collect()
}

pub fn trash_project(entry: &ProjectEntry) -> anyhow::Result<()> {
    let root = projects_root().join("trash").join(crate::session::id());
    fs::create_dir_all(&root)?;
    // Persist recovery metadata first; all paths are validated single components.
    atomic_write(&root.join("entry.json"), &serde_json::to_vec(entry)?)?;
    let source = crate::session::safe_child(&projects_root(), &entry.file)?;
    if source.exists() {
        fs::rename(&source, root.join(&entry.file))?;
    }
    let assets = crate::session::project_assets(entry)?;
    if assets.exists() {
        fs::rename(&assets, root.join("assets"))?;
    }
    Ok(())
}
pub fn restore_last_deleted() -> anyhow::Result<Option<ProjectEntry>> {
    let mut entries: Vec<_> = fs::read_dir(projects_root().join("trash"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().join("entry.json").exists())
        .collect();
    entries.sort_by_key(|e| e.file_name());
    let Some(last) = entries.pop() else {
        return Ok(None);
    };
    let root = last.path();
    let entry: ProjectEntry = serde_json::from_slice(&fs::read(root.join("entry.json"))?)?;
    let destination = crate::session::safe_child(&projects_root(), &entry.file)?;
    anyhow::ensure!(
        !destination.exists() && !crate::session::project_assets(&entry)?.exists(),
        "Project identity already exists; restore will not overwrite it"
    );
    if root.join(&entry.file).exists() {
        fs::rename(root.join(&entry.file), destination)?;
    }
    if root.join("assets").exists() {
        fs::rename(root.join("assets"), crate::session::project_assets(&entry)?)?;
    }
    fs::rename(root.join("entry.json"), root.join("restored.json"))?;
    Ok(Some(entry))
}

pub fn data_from_config(config: &AppConfig) -> ProjectData {
    let mut banks = Vec::with_capacity(FX_BANK_COUNT);
    for bank in &config.input_fx.banks {
        let mut slots = Vec::with_capacity(FX_SLOT_COUNT);
        for slot in &bank.slots {
            let mut slot_data = FxSlotData {
                parameter_lane: slot.parameter_lane.sanitized(),
                clip_link: slot.clip_link.clone(),
                roll: None,
                audio: None,
                source_id: slot.source_id.clone(),
                detached_clip: match &slot.fx {
                    Some(InputFx::Oscillator(_) | InputFx::MyDelay(_)) => None,
                    _ => slot.clip.clone(),
                },
                is_enabled: slot.is_enabled,
                kind: "None".to_string(),
                osc: None,
                filter: None,
                reverb: None,
                my_delay: None,
                vocoder: None,
            };
            if let Some(fx) = slot.fx.as_ref() {
                match fx {
                    InputFx::Roll(roll) => {
                        slot_data.kind = "Roll".to_owned();
                        slot_data.roll = Some(roll_data(roll));
                    }
                    InputFx::Audio(audio) => {
                        slot_data.kind = "Audio".to_owned();
                        slot_data.audio = Some(audio.sanitized());
                    }
                    InputFx::Oscillator(osc) => {
                        slot_data.kind = "Oscillator".to_string();
                        let seq = osc
                            .note
                            .seq()
                            .iter()
                            .map(|n| {
                                n.map(|x| NoteOctData {
                                    note: note_to_string(x.note).to_string(),
                                    octave: x.octave,
                                })
                            })
                            .collect();
                        slot_data.osc = Some(OscData {
                            phrase_serial: osc.note.launch_serial,
                            pending_clip: osc.note.pending.clone(),
                            clip: Some(osc.note.clip()),
                            voices: osc.voices,
                            input_gate: osc.input_gate,
                            dry_level: osc.dry_level,
                            input_mod_sens: osc.input_mod_sens,
                            lfo: osc.lfo.clone(),
                            lfo2: osc.lfo2.clone(),
                            mono_legato: osc.mono_legato,
                            glide_ms: osc.glide_ms,
                            glide_mode: osc.glide_mode,
                            sample: osc.sample.clone(),
                            sample_ref: osc.sample_ref.clone(),
                            sample_temporary: osc.sample_temporary,
                            sample_error: None,
                            sample_mode: osc.sample_mode,
                            sample_root: osc.sample_root,
                            sample_fine_cents: osc.sample_fine_cents,
                            sample_loop: osc.sample_loop,
                            sample_start: osc.sample_start,
                            sample_end: osc.sample_end,
                            vocal_formant: osc.vocal_formant,
                            capture_serial: osc.capture_serial,
                            capture_ms: osc.capture_ms,
                            waveform: waveform_to_string(osc.waveform.value).to_string(),
                            level: osc.level.value,
                            threshold: osc.threshold.value,
                            note_current: note_to_string(osc.note.note.value).to_string(),
                            octave_current: osc.note.octave.value,
                            step: osc.note.step.value.clone(),
                            note_seq: seq,
                            note_step_len_seq: osc.note.step_len_seq().to_vec(),
                            envelope: EnvelopeData {
                                attack_ms: osc.envelope.attack_ms.value,
                                hold_ms: osc.envelope.hold_ms.value,
                                decay_ms: osc.envelope.decay_ms.value,
                                sustain_pct: osc.envelope.sustain_pct.value,
                                release_ms: osc.envelope.release_ms.value,
                                start_pct: osc.envelope.start_pct.value,
                                tension_a: osc.envelope.tension_a.value,
                                tension_d: osc.envelope.tension_d.value,
                                tension_r: osc.envelope.tension_r.value,
                            },
                            osc_filter: FilterData {
                                sweep: Default::default(),
                                filter_type: filter_type_to_string(
                                    osc.osc_filter.filter_type.value,
                                )
                                .to_string(),
                                cutoff_hz: osc.osc_filter.cutoff_hz.value,
                                resonance_x10: osc.osc_filter.resonance_x10.value,
                                drive: osc.osc_filter.drive.value,
                                mix: osc.osc_filter.mix.value,
                            },
                            osc_filter_envelope: EnvelopeData {
                                attack_ms: osc.osc_filter_env.attack_ms.value,
                                hold_ms: osc.osc_filter_env.hold_ms.value,
                                decay_ms: osc.osc_filter_env.decay_ms.value,
                                sustain_pct: osc.osc_filter_env.sustain_pct.value,
                                release_ms: osc.osc_filter_env.release_ms.value,
                                start_pct: osc.osc_filter_env.start_pct.value,
                                tension_a: osc.osc_filter_env.tension_a.value,
                                tension_d: osc.osc_filter_env.tension_d.value,
                                tension_r: osc.osc_filter_env.tension_r.value,
                            },
                        });
                    }
                    InputFx::Filter(filter) => {
                        slot_data.kind = "Filter".to_string();
                        slot_data.filter = Some(FilterData {
                            sweep: filter.sweep.sanitized(),
                            filter_type: filter_type_to_string(filter.filter_type.value)
                                .to_string(),
                            cutoff_hz: filter.cutoff_hz.value,
                            resonance_x10: filter.resonance_x10.value,
                            drive: filter.drive.value,
                            mix: filter.mix.value,
                        });
                    }
                    InputFx::Reverb(reverb) => {
                        slot_data.kind = "Reverb".to_string();
                        slot_data.reverb = Some(ReverbData {
                            high_cut_hz: Some(reverb.high_cut_hz.value),
                            dry_level: reverb.dry_level.value,
                            wet_level: reverb.wet_level.value,
                            density: reverb.density.value,
                            size: reverb.size.value,
                            decay_ms: reverb.decay_ms.value,
                            predelay_ms: reverb.predelay_ms.value,
                            width: reverb.width.value,
                            high_cut: reverb.high_cut.value,
                            low_cut: reverb.low_cut.value,
                        });
                    }
                    InputFx::MyDelay(delay) => {
                        slot_data.kind = "MyDelay".to_string();
                        let seq = delay
                            .note
                            .seq()
                            .iter()
                            .map(|n| {
                                n.map(|x| NoteOctData {
                                    note: note_to_string(x.note).to_string(),
                                    octave: x.octave,
                                })
                            })
                            .collect();
                        slot_data.my_delay = Some(MyDelayData {
                            level: delay.level.value,
                            threshold: delay.threshold.value,
                            note_current: note_to_string(delay.note.note.value).to_string(),
                            octave_current: delay.note.octave.value,
                            step: delay.note.step.value.clone(),
                            note_seq: seq,
                            note_step_len_seq: delay.note.step_len_seq().to_vec(),
                            filter: FilterData {
                                sweep: Default::default(),
                                filter_type: filter_type_to_string(delay.filter.filter_type.value)
                                    .to_string(),
                                cutoff_hz: delay.filter.cutoff_hz.value,
                                resonance_x10: delay.filter.resonance_x10.value,
                                drive: delay.filter.drive.value,
                                mix: delay.filter.mix.value,
                            },
                            audio_env: EnvelopeData {
                                attack_ms: delay.audio_env.attack_ms.value,
                                hold_ms: delay.audio_env.hold_ms.value,
                                decay_ms: delay.audio_env.decay_ms.value,
                                sustain_pct: delay.audio_env.sustain_pct.value,
                                release_ms: delay.audio_env.release_ms.value,
                                start_pct: delay.audio_env.start_pct.value,
                                tension_a: delay.audio_env.tension_a.value,
                                tension_d: delay.audio_env.tension_d.value,
                                tension_r: delay.audio_env.tension_r.value,
                            },
                            filter_env: EnvelopeData {
                                attack_ms: delay.filter_env.attack_ms.value,
                                hold_ms: delay.filter_env.hold_ms.value,
                                decay_ms: delay.filter_env.decay_ms.value,
                                sustain_pct: delay.filter_env.sustain_pct.value,
                                release_ms: delay.filter_env.release_ms.value,
                                start_pct: delay.filter_env.start_pct.value,
                                tension_a: delay.filter_env.tension_a.value,
                                tension_d: delay.filter_env.tension_d.value,
                                tension_r: delay.filter_env.tension_r.value,
                            },
                        });
                    }
                    InputFx::Vocoder(vocoder) => {
                        slot_data.kind = "Vocoder".to_string();
                        slot_data.vocoder = Some(VocoderData {
                            tone: vocoder.tone,
                            mod_sens: vocoder.mod_sens,
                            formant_semitones: vocoder.formant_semitones,
                            sibilance: vocoder.sibilance.value,
                            carrier_thru: vocoder.carrier_thru,
                            carrier: vocoder_carrier_to_string(vocoder.carrier.value).to_string(),
                            bands: vocoder.bands.value,
                            attack_ms: vocoder.attack_ms.value,
                            release_ms: vocoder.release_ms.value,
                            level: vocoder.level.value,
                            mix: vocoder.mix.value,
                        });
                    }
                }
            }
            slots.push(slot_data);
        }
        banks.push(FxBankData { slots });
    }

    let mut track_fx_banks = Vec::with_capacity(TRACK_FX_BANK_COUNT);
    for bank in &config.track_fx.banks {
        let mut track_slots = Vec::with_capacity(TRACK_FX_SLOT_COUNT);
        for slot in &bank.slots {
            let mut slot_data = TrackFxSlotData {
                parameter_lane: slot.parameter_lane.sanitized(),
                vocoder: None,
                audio: None,
                is_enabled: false,
                kind: "None".to_string(),
                delay: None,
                roll: None,
                filter: None,
            };
            if let Some(fx) = slot.fx.as_ref() {
                match fx {
                    TrackFx::Audio(audio) => {
                        slot_data.kind = "Audio".to_owned();
                        slot_data.audio = Some(audio.sanitized());
                    }
                    TrackFx::Vocoder(v) => {
                        slot_data.kind = "Vocoder".to_owned();
                        slot_data.vocoder = Some(vocoder_data(v));
                    }
                    TrackFx::Delay(delay) => {
                        slot_data.kind = "Delay".to_string();
                        slot_data.delay = Some(TrackDelayData {
                            feedback_repeats: delay.feedback_repeats.value,
                            direct_pct: Some(delay.direct_pct.value),
                            effect_pct: Some(delay.effect_pct.value),
                            low_cut_hz: delay.low_cut_hz.value,
                            time_mode: delay.time_mode.value,
                            time_ms: delay.time_ms,
                            feedback_pct: delay.feedback_pct.value,
                            high_damp_hz: delay.high_damp_hz.value,
                            mix_pct: delay.mix_pct.value,
                        });
                    }
                    TrackFx::Roll(roll) => {
                        slot_data.kind = "Roll".to_string();
                        slot_data.roll = Some(roll_data(roll));
                    }
                    TrackFx::Filter(filter) => {
                        slot_data.kind = "Filter".to_string();
                        slot_data.filter = Some(TrackFilterData {
                            filter: FilterData {
                                sweep: filter.filter.sweep.sanitized(),
                                filter_type: filter_type_to_string(filter.filter.filter_type.value)
                                    .to_string(),
                                cutoff_hz: filter.filter.cutoff_hz.value,
                                resonance_x10: filter.filter.resonance_x10.value,
                                drive: filter.filter.drive.value,
                                mix: filter.filter.mix.value,
                            },
                            seq_step: filter.seq.step.value.clone(),
                            seq: filter.seq.seq().to_vec(),
                            seq_step_len_seq: filter.seq.step_len_seq().to_vec(),
                            env: EnvelopeData {
                                attack_ms: filter.env.attack_ms.value,
                                hold_ms: filter.env.hold_ms.value,
                                decay_ms: filter.env.decay_ms.value,
                                sustain_pct: filter.env.sustain_pct.value,
                                release_ms: filter.env.release_ms.value,
                                start_pct: filter.env.start_pct.value,
                                tension_a: filter.env.tension_a.value,
                                tension_d: filter.env.tension_d.value,
                                tension_r: filter.env.tension_r.value,
                            },
                        });
                    }
                }
            }
            track_slots.push(slot_data);
        }
        track_fx_banks.push(TrackFxBankData { slots: track_slots });
    }

    let mut track_fx_tracks = Vec::with_capacity(config.track_fx.tracks.len());
    for track in &config.track_fx.tracks {
        let enabled: Vec<Vec<bool>> = track.enabled.iter().map(|row| row.to_vec()).collect();
        track_fx_tracks.push(TrackFxTrackData {
            enabled,
            banks: Vec::new(),
        });
    }

    ProjectData {
        input_noise: config.input_noise.sanitized(),
        pdc_enabled: config.pdc_enabled,
        master_fx: config.master_fx.sanitized(),
        input_thru: config.input_thru,
        metronome_volume: config.metronome_volume.clamp(0.0, 1.0),
        calibration: config.calibration.clone(),
        snapshot: None,
        track_options: config.track_options.clone(),
        input_routing: config.input_routing,
        fader_speed_db: config.fader_speed_db,
        track_levels: config.track_levels.clone(),
        beat: BeatData {
            bpm: config.beat_config.current_bpm(),
            latency: config.beat_config.current_latency(),
        },
        system: SystemData {
            input_device: config.system_config.input_device.value.clone(),
            output_device: config.system_config.output_device.value.clone(),
        },
        input_fx: InputFxData {
            selected_bank_idx: config.input_fx.sel_bank_idx,
            banks,
        },
        track_fx: TrackFxData {
            selected_bank_idx: config.track_fx.sel_bank_idx,
            banks: track_fx_banks,
            tracks: track_fx_tracks,
        },
    }
}

pub fn apply_data_to_config(config: &mut AppConfig, data: ProjectData) {
    config.pdc_enabled = data.pdc_enabled;
    config.input_thru = data.input_thru;
    config.input_noise = data.input_noise.sanitized();
    config.master_fx = data.master_fx.sanitized();
    config.metronome_volume = if data.metronome_volume.is_finite() {
        data.metronome_volume.clamp(0.0, 1.0)
    } else {
        default_metronome_volume()
    };
    config.calibration = data.calibration;
    config.input_routing = data.input_routing;
    for (index, options) in config.track_options.iter_mut().enumerate() {
        *options = data.track_options.get(index).copied().unwrap_or_else(|| {
            let mut value = crate::config::track_options::TrackOptions::default();
            value.fader_speed = data.fader_speed_db;
            value
        });
        options.normalize();
    }
    config.fader_speed_db = if data.fader_speed_db.is_finite() {
        data.fader_speed_db.clamp(6.0, 60.0)
    } else {
        24.0
    };
    for (idx, level) in config.track_levels.iter_mut().enumerate() {
        *level = data
            .track_levels
            .get(idx)
            .copied()
            .filter(|v| v.is_finite())
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);
    }
    config
        .beat_config
        .set_values(data.beat.bpm.clamp(30, 300), data.beat.latency.min(500));
    config.system_config.input_device.value = data.system.input_device;
    config.system_config.output_device.value = data.system.output_device;
    config.input_fx.sel_bank_idx = data.input_fx.selected_bank_idx.min(FX_BANK_COUNT - 1);

    for (bank_idx, bank_data) in data.input_fx.banks.iter().take(FX_BANK_COUNT).enumerate() {
        for (slot_idx, slot_data) in bank_data.slots.iter().take(FX_SLOT_COUNT).enumerate() {
            let slot = &mut config.input_fx.banks[bank_idx].slots[slot_idx];
            slot.fx = None;
            slot.parameter_lane = slot_data.parameter_lane.sanitized();
            slot.clip = slot_data.detached_clip.clone();
            slot.clip_link = slot_data
                .clip_link
                .as_ref()
                .map(|s| s.chars().take(128).collect())
                .filter(|s: &String| !s.is_empty());
            if !slot_data.source_id.is_empty() {
                slot.source_id = slot_data.source_id.chars().take(128).collect();
            }
            slot.is_enabled = slot_data.is_enabled;
            match slot_data.kind.as_str() {
                "Roll" => {
                    slot.set_kind(FxKind::Roll);
                    if let (Some(InputFx::Roll(roll)), Some(data)) =
                        (slot.fx.as_mut(), &slot_data.roll)
                    {
                        apply_roll(roll, data);
                    }
                }
                "Audio" => {
                    if let Some(audio) = &slot_data.audio {
                        slot.fx = Some(InputFx::Audio(audio.sanitized()));
                    }
                }
                "Oscillator" => {
                    slot.set_kind(FxKind::Oscillator);
                    if let Some(InputFx::Oscillator(osc)) = slot.fx.as_mut() {
                        if let Some(osc_data) = &slot_data.osc {
                            if let Some(w) = string_to_waveform(&osc_data.waveform) {
                                osc.waveform.value = w;
                            }
                            osc.voices = osc_data.voices.clamp(1, 16);
                            osc.input_gate = osc_data.input_gate;
                            osc.dry_level = osc_data.dry_level;
                            osc.input_mod_sens = osc_data.input_mod_sens;
                            osc.lfo = osc_data.lfo.clone();
                            osc.lfo.sanitize();
                            osc.lfo2 = osc_data.lfo2.clone();
                            osc.lfo2.sanitize();
                            osc.mono_legato = osc_data.mono_legato;
                            osc.glide_ms = if osc_data.glide_ms.is_finite() {
                                osc_data.glide_ms.clamp(0.0, 2000.0)
                            } else {
                                0.0
                            };
                            osc.glide_mode = osc_data.glide_mode;
                            osc.sample = osc_data
                                .sample
                                .as_ref()
                                .map(crate::config::osc_configs::SampleAsset::validated_shared)
                                .filter(|sample| sample.frames.len() >= 4);
                            osc.sample_ref = osc_data.sample_ref.clone();
                            osc.sample_temporary = osc_data.sample_temporary;
                            osc.sample_message = osc_data.sample_error.clone().unwrap_or_default();
                            osc.sample_mode = osc_data.sample_mode;
                            osc.sample_root = osc_data.sample_root.min(119);
                            osc.sample_fine_cents = if osc_data.sample_fine_cents.is_finite() {
                                osc_data.sample_fine_cents.clamp(-100.0, 100.0)
                            } else {
                                0.0
                            };
                            osc.sample_loop = osc_data.sample_loop;
                            osc.sample_start = if osc_data.sample_start.is_finite() {
                                osc_data.sample_start.clamp(0.0, 1.0)
                            } else {
                                0.0
                            };
                            osc.sample_end = if osc_data.sample_end.is_finite() {
                                osc_data.sample_end.clamp(0.0, 1.0)
                            } else {
                                1.0
                            };
                            osc.vocal_formant = if osc_data.vocal_formant.is_finite() {
                                osc_data.vocal_formant.clamp(0.0, 1.0)
                            } else {
                                0.0
                            };
                            osc.capture_serial = osc_data.capture_serial;
                            osc.capture_ms = osc_data.capture_ms.clamp(20, 2000);
                            osc.sanitize_source();
                            osc.level.value = osc_data.level.min(100);
                            osc.threshold.value = osc_data.threshold.min(100);
                            if let Some(n) = string_to_note(&osc_data.note_current) {
                                osc.note.note.value = n;
                            }
                            osc.note.octave.value = osc_data.octave_current;
                            osc.note.step.value = osc_data.step.clone();
                            let seq = osc_data
                                .note_seq
                                .iter()
                                .map(|n| {
                                    n.as_ref().map(|x| NoteOct {
                                        note: string_to_note(&x.note).unwrap_or(Note::N),
                                        octave: x.octave,
                                    })
                                })
                                .collect();
                            osc.note
                                .set_seq_with_steps(seq, osc_data.note_step_len_seq.clone());
                            if let Some(clip) = &osc_data.clip {
                                osc.note.set_clip(clip);
                            }
                            osc.note.launch_serial = osc_data.phrase_serial;
                            osc.note.pending = osc_data
                                .pending_clip
                                .as_ref()
                                .filter(|p| p.serial != 0)
                                .map(|p| {
                                    let mut note = crate::config::note_configs::NoteConfigs::new();
                                    note.set_clip(&p.clip);
                                    crate::config::sequence_edit::PendingClip {
                                        serial: p.serial,
                                        clip: note.clip(),
                                    }
                                });
                            osc.envelope.attack_ms.value =
                                osc_data.envelope.attack_ms.min(ENVELOPE_ATTACK_MAX_MS);
                            osc.envelope.hold_ms.value =
                                osc_data.envelope.hold_ms.min(ENVELOPE_HOLD_MAX_MS);
                            osc.envelope.decay_ms.value =
                                osc_data.envelope.decay_ms.min(ENVELOPE_DECAY_MAX_MS);
                            osc.envelope.sustain_pct.value =
                                osc_data.envelope.sustain_pct.min(ENVELOPE_SUSTAIN_MAX_PCT);
                            osc.envelope.release_ms.value = osc_data
                                .envelope
                                .release_ms
                                .clamp(ENVELOPE_RELEASE_MIN_MS, ENVELOPE_RELEASE_MAX_MS);
                            osc.envelope.start_pct.value =
                                osc_data.envelope.start_pct.min(ENVELOPE_START_MAX_PCT);
                            osc.envelope.tension_a.value =
                                osc_data.envelope.tension_a.min(ENVELOPE_TENSION_MAX);
                            osc.envelope.tension_d.value =
                                osc_data.envelope.tension_d.min(ENVELOPE_TENSION_MAX);
                            osc.envelope.tension_r.value =
                                osc_data.envelope.tension_r.min(ENVELOPE_TENSION_MAX);
                            if let Some(t) = string_to_filter_type(&osc_data.osc_filter.filter_type)
                            {
                                osc.osc_filter.filter_type.value = t;
                            }
                            osc.osc_filter.cutoff_hz.value =
                                osc_data.osc_filter.cutoff_hz.clamp(20, 20_000);
                            osc.osc_filter.resonance_x10.value =
                                osc_data.osc_filter.resonance_x10.clamp(1, 100);
                            osc.osc_filter.drive.value = osc_data.osc_filter.drive.min(100);
                            osc.osc_filter.mix.value = osc_data.osc_filter.mix.min(100);
                            osc.osc_filter_env.attack_ms.value = osc_data
                                .osc_filter_envelope
                                .attack_ms
                                .min(ENVELOPE_ATTACK_MAX_MS);
                            osc.osc_filter_env.hold_ms.value = osc_data
                                .osc_filter_envelope
                                .hold_ms
                                .min(ENVELOPE_HOLD_MAX_MS);
                            osc.osc_filter_env.decay_ms.value = osc_data
                                .osc_filter_envelope
                                .decay_ms
                                .min(ENVELOPE_DECAY_MAX_MS);
                            osc.osc_filter_env.sustain_pct.value = osc_data
                                .osc_filter_envelope
                                .sustain_pct
                                .min(ENVELOPE_SUSTAIN_MAX_PCT);
                            osc.osc_filter_env.release_ms.value = osc_data
                                .osc_filter_envelope
                                .release_ms
                                .clamp(ENVELOPE_RELEASE_MIN_MS, ENVELOPE_RELEASE_MAX_MS);
                            osc.osc_filter_env.start_pct.value = osc_data
                                .osc_filter_envelope
                                .start_pct
                                .min(ENVELOPE_START_MAX_PCT);
                            osc.osc_filter_env.tension_a.value = osc_data
                                .osc_filter_envelope
                                .tension_a
                                .min(ENVELOPE_TENSION_MAX);
                            osc.osc_filter_env.tension_d.value = osc_data
                                .osc_filter_envelope
                                .tension_d
                                .min(ENVELOPE_TENSION_MAX);
                            osc.osc_filter_env.tension_r.value = osc_data
                                .osc_filter_envelope
                                .tension_r
                                .min(ENVELOPE_TENSION_MAX);
                        }
                    }
                }
                "Filter" => {
                    slot.set_kind(FxKind::Filter);
                    if let Some(InputFx::Filter(filter)) = slot.fx.as_mut() {
                        if let Some(filter_data) = &slot_data.filter {
                            if let Some(t) = string_to_filter_type(&filter_data.filter_type) {
                                filter.filter_type.value = t;
                            }
                            filter.cutoff_hz.value = filter_data.cutoff_hz.clamp(20, 20_000);
                            filter.resonance_x10.value = filter_data.resonance_x10.clamp(1, 100);
                            filter.drive.value = filter_data.drive.min(100);
                            filter.mix.value = filter_data.mix.min(100);
                            filter.sweep = filter_data.sweep.sanitized();
                        }
                    }
                }
                "Reverb" => {
                    slot.set_kind(FxKind::Reverb);
                    if let Some(InputFx::Reverb(reverb)) = slot.fx.as_mut() {
                        if let Some(reverb_data) = &slot_data.reverb {
                            reverb.dry_level.value = reverb_data.dry_level.min(100);
                            reverb.wet_level.value = reverb_data.wet_level.min(100);
                            reverb.density.value = reverb_data.density.clamp(1, 10);
                            reverb.size.value = reverb_data.size.min(REVERB_SIZE_MAX);
                            reverb.decay_ms.value = reverb_data
                                .decay_ms
                                .clamp(REVERB_RT60_MIN_MS, REVERB_RT60_MAX_MS);
                            reverb.predelay_ms.value =
                                reverb_data.predelay_ms.min(REVERB_PREDELAY_MAX_MS);
                            reverb.width.value = reverb_data.width.min(REVERB_WIDTH_MAX);
                            reverb.high_cut.value = reverb_data.high_cut.min(REVERB_HIGHCUT_MAX);
                            reverb.high_cut_hz.value = reverb_data
                                .high_cut_hz
                                .unwrap_or(18_000 - reverb.high_cut.value * 155)
                                .min(20_000);
                            reverb.low_cut.value = reverb_data
                                .low_cut
                                .clamp(REVERB_LOWCUT_MIN_HZ, REVERB_LOWCUT_MAX_HZ);
                        }
                    }
                }
                "MyDelay" => {
                    slot.set_kind(FxKind::MyDelay);
                    if let Some(InputFx::MyDelay(delay)) = slot.fx.as_mut() {
                        if let Some(delay_data) = &slot_data.my_delay {
                            delay.level.value = delay_data.level.min(MYDELAY_LEVEL_MAX);
                            delay.threshold.value = delay_data.threshold.min(MYDELAY_THRESHOLD_MAX);
                            if let Some(n) = string_to_note(&delay_data.note_current) {
                                delay.note.note.value = n;
                            }
                            delay.note.octave.value = delay_data.octave_current;
                            delay.note.step.value = delay_data.step.clone();
                            let seq = delay_data
                                .note_seq
                                .iter()
                                .map(|n| {
                                    n.as_ref().map(|x| NoteOct {
                                        note: string_to_note(&x.note).unwrap_or(Note::N),
                                        octave: x.octave,
                                    })
                                })
                                .collect();
                            delay
                                .note
                                .set_seq_with_steps(seq, delay_data.note_step_len_seq.clone());
                            if let Some(t) = string_to_filter_type(&delay_data.filter.filter_type) {
                                delay.filter.filter_type.value = t;
                            }
                            delay.filter.cutoff_hz.value =
                                delay_data.filter.cutoff_hz.clamp(20, 20_000);
                            delay.filter.resonance_x10.value =
                                delay_data.filter.resonance_x10.clamp(1, 100);
                            delay.filter.drive.value = delay_data.filter.drive.min(100);
                            delay.filter.mix.value = delay_data.filter.mix.min(100);
                            delay.audio_env.attack_ms.value =
                                delay_data.audio_env.attack_ms.min(ENVELOPE_ATTACK_MAX_MS);
                            delay.audio_env.hold_ms.value =
                                delay_data.audio_env.hold_ms.min(ENVELOPE_HOLD_MAX_MS);
                            delay.audio_env.decay_ms.value =
                                delay_data.audio_env.decay_ms.min(ENVELOPE_DECAY_MAX_MS);
                            delay.audio_env.sustain_pct.value = delay_data
                                .audio_env
                                .sustain_pct
                                .min(ENVELOPE_SUSTAIN_MAX_PCT);
                            delay.audio_env.release_ms.value = delay_data
                                .audio_env
                                .release_ms
                                .clamp(ENVELOPE_RELEASE_MIN_MS, ENVELOPE_RELEASE_MAX_MS);
                            delay.audio_env.start_pct.value =
                                delay_data.audio_env.start_pct.min(ENVELOPE_START_MAX_PCT);
                            delay.audio_env.tension_a.value =
                                delay_data.audio_env.tension_a.min(ENVELOPE_TENSION_MAX);
                            delay.audio_env.tension_d.value =
                                delay_data.audio_env.tension_d.min(ENVELOPE_TENSION_MAX);
                            delay.audio_env.tension_r.value =
                                delay_data.audio_env.tension_r.min(ENVELOPE_TENSION_MAX);
                            delay.filter_env.attack_ms.value =
                                delay_data.filter_env.attack_ms.min(ENVELOPE_ATTACK_MAX_MS);
                            delay.filter_env.hold_ms.value =
                                delay_data.filter_env.hold_ms.min(ENVELOPE_HOLD_MAX_MS);
                            delay.filter_env.decay_ms.value =
                                delay_data.filter_env.decay_ms.min(ENVELOPE_DECAY_MAX_MS);
                            delay.filter_env.sustain_pct.value = delay_data
                                .filter_env
                                .sustain_pct
                                .min(ENVELOPE_SUSTAIN_MAX_PCT);
                            delay.filter_env.release_ms.value = delay_data
                                .filter_env
                                .release_ms
                                .clamp(ENVELOPE_RELEASE_MIN_MS, ENVELOPE_RELEASE_MAX_MS);
                            delay.filter_env.start_pct.value =
                                delay_data.filter_env.start_pct.min(ENVELOPE_START_MAX_PCT);
                            delay.filter_env.tension_a.value =
                                delay_data.filter_env.tension_a.min(ENVELOPE_TENSION_MAX);
                            delay.filter_env.tension_d.value =
                                delay_data.filter_env.tension_d.min(ENVELOPE_TENSION_MAX);
                            delay.filter_env.tension_r.value =
                                delay_data.filter_env.tension_r.min(ENVELOPE_TENSION_MAX);
                        }
                    }
                    if let Some(InputFx::MyDelay(delay)) = slot.fx.take() {
                        let mut osc = crate::config::OscillatorConfigs::new();
                        osc.waveform.value = Waveform::Sample;
                        osc.sample_message =
                            "Legacy MyDelay was migrated to OSC; capture or import a sample first."
                                .into();
                        osc.level = delay.level;
                        osc.threshold = delay.threshold;
                        osc.note = delay.note;
                        osc.envelope = delay.audio_env;
                        osc.osc_filter = delay.filter;
                        osc.osc_filter_env = delay.filter_env;
                        osc.capture_serial = 0; // Old JSON never contained PCM; the user must capture/import a new sample.
                        slot.fx = Some(InputFx::Oscillator(osc));
                    }
                }
                "Vocoder" => {
                    slot.set_kind(FxKind::Vocoder);
                    if let Some(InputFx::Vocoder(vocoder)) = slot.fx.as_mut() {
                        if let Some(vocoder_data) = &slot_data.vocoder {
                            vocoder.tone = vocoder_data.tone.clamp(-50, 50);
                            vocoder.mod_sens = vocoder_data.mod_sens.clamp(-50, 50);
                            vocoder.formant_semitones =
                                vocoder_data.formant_semitones.clamp(-12, 12);
                            vocoder.sibilance.value = vocoder_data.sibilance.min(100);
                            vocoder.carrier_thru = vocoder_data.carrier_thru;
                            if let Some(carrier) = string_to_vocoder_carrier(&vocoder_data.carrier)
                            {
                                vocoder.carrier.value = carrier;
                            }
                            vocoder.bands.value = vocoder_data
                                .bands
                                .clamp(VOCODER_BANDS_MIN, VOCODER_BANDS_MAX);
                            vocoder.attack_ms.value =
                                vocoder_data.attack_ms.min(VOCODER_ATTACK_MAX_MS);
                            vocoder.release_ms.value =
                                vocoder_data.release_ms.min(VOCODER_RELEASE_MAX_MS);
                            vocoder.level.value = vocoder_data.level.min(VOCODER_LEVEL_MAX);
                            vocoder.mix.value = vocoder_data.mix.min(VOCODER_MIX_MAX);
                        }
                    }
                }
                _ => {
                    slot.set_kind(FxKind::None);
                }
            }
        }
    }

    // A manually copied slot must not alias another source's UI selection or
    // audio acknowledgement. Repairs are deterministic across replay rebuilds.
    let mut source_ids = std::collections::HashSet::new();
    for (bank_index, bank) in config.input_fx.banks.iter_mut().enumerate() {
        for (slot_index, slot) in bank.slots.iter_mut().enumerate() {
            if slot.source_id.is_empty() || !source_ids.insert(slot.source_id.clone()) {
                let base = format!("source-{}-{}", bank_index + 1, slot_index + 1);
                let mut candidate = base.clone();
                let mut suffix = 0;
                while !source_ids.insert(candidate.clone()) {
                    suffix += 1;
                    candidate = format!("{base}-{suffix}");
                }
                slot.source_id = candidate;
            }
        }
    }

    config.track_fx.sel_bank_idx = data.track_fx.selected_bank_idx.min(TRACK_FX_BANK_COUNT - 1);

    // Reset all per-track enable states before applying loaded data.
    for track in &mut config.track_fx.tracks {
        for bank in 0..TRACK_FX_BANK_COUNT {
            for slot in 0..TRACK_FX_SLOT_COUNT {
                track.enabled[bank][slot] = false;
            }
        }
    }

    let binding_banks: &[TrackFxBankData] = if !data.track_fx.banks.is_empty() {
        data.track_fx.banks.as_slice()
    } else if let Some(first_track) = data.track_fx.tracks.first() {
        // Backward compatibility: old schema stored bindings under each track.
        first_track.banks.as_slice()
    } else {
        &[]
    };

    for (bank_idx, bank_data) in binding_banks.iter().take(TRACK_FX_BANK_COUNT).enumerate() {
        for (slot_idx, slot_data) in bank_data.slots.iter().take(TRACK_FX_SLOT_COUNT).enumerate() {
            config.track_fx.banks[bank_idx].slots[slot_idx].parameter_lane =
                slot_data.parameter_lane.sanitized();
            match slot_data.kind.as_str() {
                "Audio" => {
                    if let Some(audio) = &slot_data.audio {
                        config.track_fx.banks[bank_idx].slots[slot_idx].fx =
                            Some(TrackFx::Audio(audio.sanitized()));
                    }
                }
                "Delay" => {
                    config
                        .track_fx
                        .set_slot_kind(bank_idx, slot_idx, TrackFxKind::Delay);
                    if let Some(TrackFx::Delay(delay)) =
                        config.track_fx.slot_fx_mut(bank_idx, slot_idx)
                    {
                        if let Some(delay_data) = &slot_data.delay {
                            delay.time_mode.value = delay_data.time_mode;
                            delay.feedback_repeats.value = delay_data.feedback_repeats.min(16);
                            delay.time_ms = delay_data.time_ms.clamp(
                                TRACK_DELAY_TIME_MIN_MS as f32,
                                TRACK_DELAY_TIME_MAX_MS as f32,
                            );
                            delay.feedback_pct.value =
                                delay_data.feedback_pct.min(TRACK_DELAY_FEEDBACK_MAX_PCT);
                            delay.high_damp_hz.value = delay_data
                                .high_damp_hz
                                .clamp(TRACK_DELAY_DAMP_MIN_HZ, TRACK_DELAY_DAMP_MAX_HZ);
                            delay.mix_pct.value = delay_data.mix_pct.min(TRACK_DELAY_MIX_MAX_PCT);
                            delay.direct_pct.value = delay_data
                                .direct_pct
                                .unwrap_or(100 - delay.mix_pct.value)
                                .min(100);
                            delay.effect_pct.value = delay_data
                                .effect_pct
                                .unwrap_or(delay.mix_pct.value)
                                .min(120);
                            delay.low_cut_hz.value = delay_data.low_cut_hz.min(12500);
                        }
                    }
                }
                "Vocoder" => {
                    config
                        .track_fx
                        .set_slot_kind(bank_idx, slot_idx, TrackFxKind::Vocoder);
                    if let (Some(TrackFx::Vocoder(v)), Some(data)) = (
                        config.track_fx.slot_fx_mut(bank_idx, slot_idx),
                        &slot_data.vocoder,
                    ) {
                        apply_vocoder(v, data);
                    }
                }
                "Roll" => {
                    config
                        .track_fx
                        .set_slot_kind(bank_idx, slot_idx, TrackFxKind::Roll);
                    if let Some(TrackFx::Roll(roll)) =
                        config.track_fx.slot_fx_mut(bank_idx, slot_idx)
                    {
                        if let Some(roll_data) = &slot_data.roll {
                            apply_roll(roll, roll_data);
                        }
                    }
                }
                "Filter" => {
                    config
                        .track_fx
                        .set_slot_kind(bank_idx, slot_idx, TrackFxKind::Filter);
                    if let Some(TrackFx::Filter(filter_cfg)) =
                        config.track_fx.slot_fx_mut(bank_idx, slot_idx)
                    {
                        if let Some(filter_data) = &slot_data.filter {
                            if let Some(ft) = string_to_filter_type(&filter_data.filter.filter_type)
                            {
                                filter_cfg.filter.filter_type.value = ft;
                            }
                            filter_cfg.filter.cutoff_hz.value = filter_data
                                .filter
                                .cutoff_hz
                                .clamp(FILTER_CUTOFF_MIN_HZ, FILTER_CUTOFF_MAX_HZ);
                            filter_cfg.filter.resonance_x10.value = filter_data
                                .filter
                                .resonance_x10
                                .clamp(FILTER_Q_MIN_X10, FILTER_Q_MAX_X10);
                            filter_cfg.filter.drive.value =
                                filter_data.filter.drive.min(FILTER_DRIVE_MAX);
                            filter_cfg.filter.mix.value =
                                filter_data.filter.mix.min(FILTER_MIX_MAX);
                            filter_cfg.filter.sweep = filter_data.filter.sweep.sanitized();

                            if !filter_data.seq_step.is_empty() {
                                filter_cfg.seq.step.value = filter_data.seq_step.clone();
                            }
                            filter_cfg.seq.set_seq_with_steps(
                                filter_data.seq.clone(),
                                filter_data.seq_step_len_seq.clone(),
                            );

                            filter_cfg.env.attack_ms.value =
                                filter_data.env.attack_ms.min(ENVELOPE_ATTACK_MAX_MS);
                            filter_cfg.env.hold_ms.value =
                                filter_data.env.hold_ms.min(ENVELOPE_HOLD_MAX_MS);
                            filter_cfg.env.decay_ms.value =
                                filter_data.env.decay_ms.min(ENVELOPE_DECAY_MAX_MS);
                            filter_cfg.env.sustain_pct.value =
                                filter_data.env.sustain_pct.min(ENVELOPE_SUSTAIN_MAX_PCT);
                            filter_cfg.env.release_ms.value = filter_data
                                .env
                                .release_ms
                                .clamp(ENVELOPE_RELEASE_MIN_MS, ENVELOPE_RELEASE_MAX_MS);
                            filter_cfg.env.start_pct.value =
                                filter_data.env.start_pct.min(ENVELOPE_START_MAX_PCT);
                            filter_cfg.env.tension_a.value =
                                filter_data.env.tension_a.min(ENVELOPE_TENSION_MAX);
                            filter_cfg.env.tension_d.value =
                                filter_data.env.tension_d.min(ENVELOPE_TENSION_MAX);
                            filter_cfg.env.tension_r.value =
                                filter_data.env.tension_r.min(ENVELOPE_TENSION_MAX);
                        }
                    }
                }
                _ => {
                    config
                        .track_fx
                        .set_slot_kind(bank_idx, slot_idx, TrackFxKind::None);
                }
            }
        }
    }

    for (track_idx, track_data) in data
        .track_fx
        .tracks
        .iter()
        .take(config.track_fx.tracks.len())
        .enumerate()
    {
        let track = &mut config.track_fx.tracks[track_idx];
        if !track_data.enabled.is_empty() {
            for (bank_idx, bank_enabled) in track_data
                .enabled
                .iter()
                .take(TRACK_FX_BANK_COUNT)
                .enumerate()
            {
                for (slot_idx, enabled) in bank_enabled.iter().take(TRACK_FX_SLOT_COUNT).enumerate()
                {
                    track.enabled[bank_idx][slot_idx] = *enabled;
                }
            }
        } else {
            // Backward compatibility: old schema stored per-track `is_enabled` in `banks.slots`.
            for (bank_idx, bank_data) in track_data
                .banks
                .iter()
                .take(TRACK_FX_BANK_COUNT)
                .enumerate()
            {
                for (slot_idx, slot_data) in
                    bank_data.slots.iter().take(TRACK_FX_SLOT_COUNT).enumerate()
                {
                    track.enabled[bank_idx][slot_idx] = slot_data.is_enabled;
                }
            }
        }
    }
}

fn default_tension_value() -> usize {
    100
}

fn projects_root() -> PathBuf {
    crate::app_support::paths::projects_dir()
}

fn ensure_project_dir() -> anyhow::Result<()> {
    fs::create_dir_all(projects_root())?;
    Ok(())
}

fn index_path() -> PathBuf {
    projects_root().join(INDEX_FILE)
}

fn project_file_path(file: &str) -> PathBuf {
    projects_root().join(file)
}

fn sanitize_name(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            out.push(c);
        } else if c.is_whitespace() {
            out.push('_');
        }
    }
    if out.is_empty() {
        "project".to_string()
    } else {
        out
    }
}

fn waveform_to_string(w: Waveform) -> &'static str {
    match w {
        Waveform::Sine => "Sine",
        Waveform::Saw => "Saw",
        Waveform::Square => "Square",
        Waveform::Triangle => "Triangle",
        Waveform::Vocal => "Vocal",
        Waveform::Sample => "Sample",
        Waveform::Rect => "Rect",
        Waveform::DetuneSaw => "DetuneSaw",
        Waveform::VintageSaw => "VintageSaw",
    }
}

fn string_to_waveform(s: &str) -> Option<Waveform> {
    match s {
        "Sine" => Some(Waveform::Sine),
        "Saw" => Some(Waveform::Saw),
        "Square" => Some(Waveform::Square),
        "Triangle" => Some(Waveform::Triangle),
        "Vocal" => Some(Waveform::Vocal),
        "Sample" => Some(Waveform::Sample),
        "Rect" => Some(Waveform::Rect),
        "DetuneSaw" => Some(Waveform::DetuneSaw),
        "VintageSaw" => Some(Waveform::VintageSaw),
        _ => None,
    }
}

fn roll_data(roll: &crate::config::roll_configs::RollConfigs) -> TrackRollData {
    TrackRollData {
        time_mode: roll.time_mode.value,
        time_ms: roll.time_ms.value,
        mode: roll.mode.value,
        feedback: roll.feedback.value,
        repeat: roll.repeat.value,
        mix: roll.mix.value,
        step: roll.step.value.value(),
    }
}
fn apply_roll(roll: &mut crate::config::roll_configs::RollConfigs, data: &TrackRollData) {
    roll.time_mode.value = data.time_mode;
    roll.time_ms.value = data.time_ms.clamp(1, 1000);
    roll.mode.value = data.mode;
    roll.feedback.value = data.feedback.clamp(1, 100);
    roll.repeat.value = data.repeat.min(100);
    roll.mix.value = data.mix.min(100);
    roll.step.value = match data.step {
        1 => RollStep::Off,
        2 => RollStep::Two,
        8 => RollStep::Eight,
        16 => RollStep::Sixteen,
        _ => RollStep::Four,
    };
}
fn vocoder_data(v: &crate::config::vocoder_configs::VocoderConfigs) -> VocoderData {
    VocoderData {
        tone: v.tone,
        mod_sens: v.mod_sens,
        formant_semitones: v.formant_semitones,
        sibilance: v.sibilance.value,
        carrier_thru: v.carrier_thru,
        carrier: vocoder_carrier_to_string(v.carrier.value).to_owned(),
        bands: v.bands.value,
        attack_ms: v.attack_ms.value,
        release_ms: v.release_ms.value,
        level: v.level.value,
        mix: v.mix.value,
    }
}

#[cfg(test)]
mod audio_effect_tests {
    use super::*;
    #[test]
    fn priority_distortion_and_dynamics_preserve_old_json_and_roundtrip_new_settings() {
        use crate::config::{
            audio_fx::{AudioFxConfig, AudioFxKind as K, DistortionType as D},
            dynamics_profiles::DynamicsProfile as P,
        };
        let old: AudioFxConfig = serde_json::from_str(
            r#"{"kind":"Distortion","drive_style":"Hard","drive_db":27.5,"mix":0.35}"#,
        )
        .unwrap();
        assert_eq!(old.distortion_type, D::Legacy);
        assert_eq!(old.drive_db, 27.5);
        assert_eq!(old.mix, 0.35);
        let old: AudioFxConfig =
            serde_json::from_str(r#"{"kind":"Dynamics","threshold_db":-23.0,"ratio":3.5}"#)
                .unwrap();
        assert_eq!(old.dynamics_profile, P::Custom);
        assert_eq!(old.threshold_db, -23.0);
        assert_eq!(old.ratio, 3.5);
        for (kind, dist, profile) in D::ALL
            .into_iter()
            .map(|d| (K::Distortion, d, P::Custom))
            .chain(P::ALL.into_iter().map(|p| (K::Dynamics, D::Legacy, p)))
        {
            let mut c = AppConfig::new(120, 0, 5);
            let mut p = AudioFxConfig::new(kind);
            p.distortion_type = dist;
            p.distortion_tone = -12.5;
            p.distortion_direct = 0.32;
            p.distortion_effect = 0.76;
            p.dynamics_profile = profile;
            p.dynamics_amount = 7.2;
            c.input_fx.set_slot_kind(0, 0, FxKind::Audio(kind));
            c.input_fx.banks[0].slots[0].fx = Some(InputFx::Audio(p));
            c.track_fx.set_slot_kind(1, 2, TrackFxKind::Audio(kind));
            *c.track_fx.slot_fx_mut(1, 2).unwrap() = TrackFx::Audio(p);
            c.master_fx.compressor = p;
            c.master_fx.compressor.kind = K::Dynamics;
            let saved = data_from_config(&c);
            let decoded: ProjectData =
                serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
            let mut restored = AppConfig::new(120, 0, 5);
            apply_data_to_config(&mut restored, decoded);
            let again = data_from_config(&restored);
            assert_eq!(again.input_fx.banks[0].slots[0].audio, Some(p));
            assert_eq!(again.track_fx.banks[1].slots[2].audio, Some(p));
            assert_eq!(again.master_fx, saved.master_fx);
            let preset =
                crate::presets::encode(&c, crate::presets::FxTarget::Input { bank: 0, slot: 0 })
                    .unwrap();
            crate::presets::decode(
                &mut restored,
                crate::presets::FxTarget::Input { bank: 1, slot: 0 },
                &preset,
            )
            .unwrap();
            assert_eq!(
                data_from_config(&restored).input_fx.banks[1].slots[0].audio,
                Some(p)
            );
        }
    }
    #[test]
    fn fractional_delay_time_and_unity_feedback_preserve_old_json_and_round_trip() {
        let mut c = AppConfig::new(120, 0, 5);
        c.track_fx.set_slot_kind(0, 0, TrackFxKind::Delay);
        if let Some(TrackFx::Delay(p)) = c.track_fx.slot_fx_mut(0, 0) {
            p.time_ms = 1.37;
            p.feedback_pct.value = 100;
            p.feedback_repeats.value = 0;
        }
        let d = data_from_config(&c);
        let text = serde_json::to_string(&d).unwrap();
        let mut r = AppConfig::new(120, 0, 5);
        apply_data_to_config(&mut r, serde_json::from_str(&text).unwrap());
        let Some(TrackFx::Delay(p)) = r.track_fx.slot_fx(0, 0) else {
            panic!()
        };
        assert_eq!(p.time_ms, 1.37);
        assert_eq!(p.feedback_pct.value, 100);
        let mut old = serde_json::to_value(d).unwrap();
        old["track_fx"]["banks"][0]["slots"][0]["delay"]["time_ms"] = serde_json::json!(320);
        apply_data_to_config(&mut r, serde_json::from_value(old).unwrap());
        let Some(TrackFx::Delay(p)) = r.track_fx.slot_fx(0, 0) else {
            panic!()
        };
        assert_eq!(p.time_ms, 320.0);
    }
    #[test]
    fn input_roll_project_and_preset_round_trip() {
        use crate::presets::FxTarget;
        let mut c = AppConfig::new(137, 0, 5);
        c.input_fx.set_slot_kind(2, 3, FxKind::Roll);
        c.input_fx.banks[2].slots[3].is_enabled = true;
        if let Some(InputFx::Roll(p)) = &mut c.input_fx.banks[2].slots[3].fx {
            p.time_mode.value = TimeMode::DottedEighth;
            p.time_ms.value = 713;
            p.mode.value = RollMode::Roll1;
            p.feedback.value = 73;
            p.repeat.value = 17;
            p.mix.value = 82;
            p.step.value = RollStep::Sixteen;
        }
        let saved = data_from_config(&c);
        let encoded = serde_json::to_string(&saved).unwrap();
        let mut loaded = AppConfig::new(120, 0, 5);
        apply_data_to_config(&mut loaded, serde_json::from_str(&encoded).unwrap());
        assert!(loaded.input_fx.slot_kind(2, 3) == FxKind::Roll);
        assert_eq!(
            saved.input_fx.banks[2].slots[3]
                .roll
                .as_ref()
                .unwrap()
                .time_ms,
            713
        );
        assert_eq!(
            serde_json::to_value(&data_from_config(&loaded).input_fx.banks[2].slots[3].roll)
                .unwrap(),
            serde_json::to_value(&saved.input_fx.banks[2].slots[3].roll).unwrap()
        );
        let preset = crate::presets::encode(&c, FxTarget::Input { bank: 2, slot: 3 }).unwrap();
        crate::presets::decode(&mut loaded, FxTarget::Input { bank: 1, slot: 0 }, &preset).unwrap();
        assert!(loaded.input_fx.slot_kind(1, 0) == FxKind::Roll);
        assert!(
            !loaded.input_fx.banks[1].slots[0].is_enabled,
            "Loading a preset must preserve bypass"
        );
        assert_eq!(
            serde_json::to_value(&data_from_config(&loaded).input_fx.banks[1].slots[0].roll)
                .unwrap(),
            serde_json::to_value(&saved.input_fx.banks[2].slots[3].roll).unwrap()
        );
    }
    #[test]
    fn every_new_audio_effect_and_master_bus_round_trip() {
        use crate::config::audio_fx::{AudioFxKind, Scale};
        for kind in AudioFxKind::ALL {
            let mut config = AppConfig::new(123, 0, 5);
            config.input_fx.set_slot_kind(2, 3, FxKind::Audio(kind));
            config
                .track_fx
                .set_slot_kind(3, 2, TrackFxKind::Audio(kind));
            if let Some(InputFx::Audio(p)) = &mut config.input_fx.banks[2].slots[3].fx {
                p.semitones = 7.0;
                p.low_cut_hz = 5200.0;
                p.scale = Scale::Minor;
                p.steps[3] = 0.3;
                p.pitch_sequence = true;
                p.pitch_steps[4] = -12.0;
                p.effect_level = 1.2;
            }
            config.master_fx.compressor_enabled = true;
            config.master_fx.compressor.threshold_db = -25.5;
            config.master_fx.reverb_enabled = true;
            config.master_fx.reverb.decay_ms = 4200.0;
            let saved = data_from_config(&config);
            let json = serde_json::to_vec(&saved).unwrap();
            let loaded: ProjectData = serde_json::from_slice(&json).unwrap();
            let mut restored = AppConfig::new(120, 0, 5);
            apply_data_to_config(&mut restored, loaded);
            let again = data_from_config(&restored);
            assert_eq!(
                saved.input_fx.banks[2].slots[3].audio,
                again.input_fx.banks[2].slots[3].audio
            );
            assert_eq!(
                saved.track_fx.banks[3].slots[2].audio,
                again.track_fx.banks[3].slots[2].audio
            );
            assert_eq!(saved.master_fx, again.master_fx);
        }
    }
    #[test]
    fn old_delay_keeps_manual_feedback_and_new_track_vocoder_persists() {
        let mut c = AppConfig::new(120, 0, 5);
        c.track_fx.set_slot_kind(0, 0, TrackFxKind::Delay);
        c.track_fx.set_slot_kind(0, 1, TrackFxKind::Vocoder);
        if let Some(TrackFx::Vocoder(v)) = c.track_fx.slot_fx_mut(0, 1) {
            v.carrier.value = VocoderCarrier::Track4;
            v.formant_semitones = 7;
        }
        let mut json = serde_json::to_value(data_from_config(&c)).unwrap();
        json["track_fx"]["banks"][0]["slots"][0]["delay"]
            .as_object_mut()
            .unwrap()
            .remove("feedback_repeats");
        json["track_fx"]["banks"][0]["slots"][0]["delay"]["feedback_pct"] = serde_json::json!(82);
        json.as_object_mut().unwrap().remove("master_fx");
        let d: ProjectData = serde_json::from_value(json).unwrap();
        let mut restored = AppConfig::new(120, 0, 5);
        apply_data_to_config(&mut restored, d);
        let Some(TrackFx::Delay(p)) = restored.track_fx.slot_fx(0, 0) else {
            panic!("delay disappeared")
        };
        assert_eq!(p.feedback_repeats.value, 0);
        assert_eq!(p.feedback_pct.value, 82);
        let Some(TrackFx::Vocoder(v)) = restored.track_fx.slot_fx(0, 1) else {
            panic!("vocoder disappeared")
        };
        assert!(v.carrier.value == VocoderCarrier::Track4);
        assert_eq!(v.formant_semitones, 7);
        assert!(!restored.master_fx.compressor_enabled && !restored.master_fx.reverb_enabled);
    }
}
fn apply_vocoder(v: &mut crate::config::vocoder_configs::VocoderConfigs, d: &VocoderData) {
    v.tone = d.tone.clamp(-50, 50);
    v.mod_sens = d.mod_sens.clamp(-50, 50);
    v.formant_semitones = d.formant_semitones.clamp(-12, 12);
    v.sibilance.value = d.sibilance.min(100);
    v.carrier_thru = d.carrier_thru;
    if let Some(carrier) = string_to_vocoder_carrier(&d.carrier) {
        v.carrier.value = carrier;
    }
    v.bands.value = d.bands.clamp(VOCODER_BANDS_MIN, VOCODER_BANDS_MAX);
    v.attack_ms.value = d.attack_ms.min(VOCODER_ATTACK_MAX_MS);
    v.release_ms.value = d.release_ms.min(VOCODER_RELEASE_MAX_MS);
    v.level.value = d.level.min(VOCODER_LEVEL_MAX);
    v.mix.value = d.mix.min(VOCODER_MIX_MAX);
}
fn vocoder_carrier_to_string(carrier: VocoderCarrier) -> &'static str {
    match carrier {
        VocoderCarrier::InputLeft => "InputLeft",
        VocoderCarrier::InputRight => "InputRight",
        VocoderCarrier::Track1 => "Track1",
        VocoderCarrier::Track2 => "Track2",
        VocoderCarrier::Track3 => "Track3",
        VocoderCarrier::Track4 => "Track4",
        VocoderCarrier::Track5 => "Track5",
    }
}

fn string_to_vocoder_carrier(s: &str) -> Option<VocoderCarrier> {
    match s {
        "InputLeft" => Some(VocoderCarrier::InputLeft),
        "InputRight" => Some(VocoderCarrier::InputRight),
        "Track1" | "Tr1" => Some(VocoderCarrier::Track1),
        "Track2" | "Tr2" => Some(VocoderCarrier::Track2),
        "Track3" | "Tr3" => Some(VocoderCarrier::Track3),
        "Track4" | "Tr4" => Some(VocoderCarrier::Track4),
        "Track5" | "Tr5" => Some(VocoderCarrier::Track5),
        _ => None,
    }
}

fn filter_type_to_string(t: FilterType) -> &'static str {
    match t {
        FilterType::Lpf => "LPF",
        FilterType::Hpf => "HPF",
        FilterType::Bpf => "BPF",
        FilterType::Notch => "Notch",
    }
}

fn string_to_filter_type(s: &str) -> Option<FilterType> {
    match s {
        "LPF" => Some(FilterType::Lpf),
        "HPF" => Some(FilterType::Hpf),
        "BPF" => Some(FilterType::Bpf),
        "Notch" => Some(FilterType::Notch),
        _ => None,
    }
}

fn note_to_string(n: Note) -> &'static str {
    match n {
        Note::N => "N",
        Note::C => "C",
        Note::Cs => "Cs",
        Note::D => "D",
        Note::Ds => "Ds",
        Note::E => "E",
        Note::F => "F",
        Note::Fs => "Fs",
        Note::G => "G",
        Note::Gs => "Gs",
        Note::A => "A",
        Note::As => "As",
        Note::B => "B",
    }
}

fn string_to_note(s: &str) -> Option<Note> {
    match s {
        "N" => Some(Note::N),
        "C" => Some(Note::C),
        "Cs" => Some(Note::Cs),
        "D" => Some(Note::D),
        "Ds" => Some(Note::Ds),
        "E" => Some(Note::E),
        "F" => Some(Note::F),
        "Fs" => Some(Note::Fs),
        "G" => Some(Note::G),
        "Gs" => Some(Note::Gs),
        "A" => Some(Note::A),
        "As" => Some(Note::As),
        "B" => Some(Note::B),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filter_sweep_legacy_defaults_and_both_racks_roundtrip() {
        use crate::config::filter_configs::FilterSweepConfig;
        let old: FilterData = serde_json::from_str(
            r#"{"filter_type":"LPF","cutoff_hz":1200,"resonance_x10":7,"drive":0,"mix":100}"#,
        )
        .unwrap();
        assert_eq!(old.sweep, FilterSweepConfig::default());
        let sweep = FilterSweepConfig {
            depth: 0.73,
            rate_hz: 2.37,
            sync: true,
            beats: 0.75,
            stepped: true,
            step_hz: 19.0,
            step_sync: true,
            step_beats: 0.125,
        };
        let mut source = AppConfig::new(137, 0, 5);
        source.input_fx.set_slot_kind(0, 0, FxKind::Filter);
        source.track_fx.set_slot_kind(0, 0, TrackFxKind::Filter);
        if let Some(InputFx::Filter(f)) = &mut source.input_fx.banks[0].slots[0].fx {
            f.sweep = sweep;
        }
        if let Some(TrackFx::Filter(f)) = source.track_fx.slot_fx_mut(0, 0) {
            f.filter.sweep = sweep;
        }
        let json = serde_json::to_vec(&data_from_config(&source)).unwrap();
        let mut restored = AppConfig::new(120, 0, 5);
        apply_data_to_config(&mut restored, serde_json::from_slice(&json).unwrap());
        let Some(InputFx::Filter(f)) = &restored.input_fx.banks[0].slots[0].fx else {
            panic!()
        };
        assert_eq!(f.sweep, sweep);
        let Some(TrackFx::Filter(f)) = restored.track_fx.slot_fx_mut(0, 0) else {
            panic!()
        };
        assert_eq!(f.filter.sweep, sweep);
        let bad = FilterSweepConfig {
            depth: f32::NAN,
            rate_hz: f32::INFINITY,
            beats: -1.0,
            step_hz: 9999.0,
            step_beats: 0.0,
            ..Default::default()
        }
        .sanitized();
        assert_eq!(
            (
                bad.depth,
                bad.rate_hz,
                bad.beats,
                bad.step_hz,
                bad.step_beats
            ),
            (0.0, 0.5, 0.0625, 100.0, 0.015625)
        );
    }
    #[test]
    fn old_project_defaults_and_new_faders_roundtrip() {
        let mut source = AppConfig::new(127, 85, 5);
        source.metronome_volume = 0.72;
        source.track_levels = vec![0.0, 0.25, 0.5, 0.75, 1.0];
        source.input_fx.set_slot_kind(0, 0, FxKind::Oscillator);
        if let Some(InputFx::Oscillator(osc)) = &mut source.input_fx.banks[0].slots[0].fx {
            osc.note.push();
            osc.note.push();
        }
        let mut json = serde_json::to_value(data_from_config(&source)).unwrap();
        let mut restored = AppConfig::new(120, 0, 5);
        apply_data_to_config(&mut restored, serde_json::from_value(json.clone()).unwrap());
        assert_eq!(restored.track_levels, source.track_levels);
        assert_eq!(restored.metronome_volume, 0.72);
        assert_eq!(restored.beat_config.current_bpm(), 127);
        let Some(InputFx::Oscillator(osc)) = &restored.input_fx.banks[0].slots[0].fx else {
            panic!()
        };
        assert_eq!(osc.note.events().len(), 2);
        json.as_object_mut().unwrap().remove("track_levels");
        json.as_object_mut().unwrap().remove("metronome_volume");
        json["beat"]["bpm"] = serde_json::json!(0);
        apply_data_to_config(&mut restored, serde_json::from_value(json).unwrap());
        assert_eq!(restored.track_levels, vec![1.0; 5]);
        assert_eq!(restored.metronome_volume, 0.35);
        assert_eq!(restored.beat_config.current_bpm(), 30);
    }
    #[test]
    fn existing_fx_extensions_roundtrip_and_legacy_fields_migrate() {
        let mut config = AppConfig::new(120, 0, 5);
        config.fader_speed_db = 36.0;
        config.input_fx.set_slot_kind(0, 0, FxKind::Vocoder);
        config.input_fx.set_slot_kind(0, 1, FxKind::Reverb);
        config.track_fx.set_slot_kind(0, 0, TrackFxKind::Roll);
        config.track_fx.set_slot_kind(0, 1, TrackFxKind::Delay);
        if let Some(InputFx::Vocoder(v)) = &mut config.input_fx.banks[0].slots[0].fx {
            v.carrier.value = VocoderCarrier::InputLeft;
            v.tone = -25;
            v.mod_sens = 13;
            v.formant_semitones = 7;
            v.sibilance.value = 68;
            v.carrier_thru = true;
        }
        if let Some(InputFx::Reverb(r)) = &mut config.input_fx.banks[0].slots[1].fx {
            r.dry_level.value = 40;
            r.wet_level.value = 81;
            r.density.value = 9;
            r.predelay_ms.value = 500;
        }
        if let Some(TrackFx::Roll(r)) = config.track_fx.slot_fx_mut(0, 0) {
            r.step.value = RollStep::Sixteen;
            r.time_mode.value = TimeMode::DottedEighth;
            r.mode.value = RollMode::Roll1;
            r.feedback.value = 72;
            r.repeat.value = 9;
            r.mix.value = 61;
        }
        if let Some(TrackFx::Delay(d)) = config.track_fx.slot_fx_mut(0, 1) {
            d.time_mode.value = TimeMode::EighthTriplet;
        }
        let before = serde_json::to_value(data_from_config(&config)).unwrap();
        let mut restored = AppConfig::new(90, 0, 5);
        apply_data_to_config(
            &mut restored,
            serde_json::from_value(before.clone()).unwrap(),
        );
        let after = serde_json::to_value(data_from_config(&restored)).unwrap();
        assert_eq!(before["input_fx"], after["input_fx"]);
        assert_eq!(before["track_fx"], after["track_fx"]);
        assert_eq!(restored.fader_speed_db, 36.0);
        let old: TrackRollData = serde_json::from_str(r#"{"step":4}"#).unwrap();
        assert!(
            old.time_mode == TimeMode::Quarter
                && old.mode == RollMode::Roll2
                && old.mix == 100
                && old.repeat == 0
        );
        let old:ReverbData=serde_json::from_str(r#"{"size":50,"decay_ms":2500,"predelay_ms":20,"width":70,"high_cut":30,"low_cut":120}"#).unwrap();
        assert_eq!((old.dry_level, old.wet_level, old.density), (100, 35, 1));
        let old:VocoderData=serde_json::from_str(r#"{"carrier":"Track1","bands":10,"attack_ms":6,"release_ms":80,"level":100,"mix":100}"#).unwrap();
        assert_eq!(
            (old.tone, old.mod_sens, old.formant_semitones, old.sibilance),
            (0, 0, 0, 20)
        );
    }
}

/// UI change detection excludes immutable PCM, retaining its content hash and metadata.
/// Saving and replay event serialization still use `data_from_config` with the full asset.
pub fn fingerprint_data_from_config(config: &AppConfig) -> ProjectData {
    let mut data = data_from_config(config);
    for bank in &mut data.input_fx.banks {
        for slot in &mut bank.slots {
            if let Some(osc) = &mut slot.osc {
                if let Some(asset) = &osc.sample {
                    osc.sample = Some(std::sync::Arc::new(
                        crate::config::osc_configs::SampleAsset {
                            name: asset.name.clone(),
                            sample_rate: asset.sample_rate,
                            content_hash: asset.content_hash,
                            frames: Vec::new(),
                            root_hz: asset.root_hz,
                            cycle_start: asset.cycle_start,
                            cycle_end: asset.cycle_end,
                        },
                    ));
                }
            }
        }
    }
    data
}

/// Visible warning for a destructive beta migration, before replacing old data.
pub fn has_legacy_mydelay(data: &ProjectData) -> bool {
    data.input_fx
        .banks
        .iter()
        .any(|bank| bank.slots.iter().any(|slot| slot.kind == "MyDelay"))
}
