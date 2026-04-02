use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::input_fx_configs::{FX_BANK_COUNT, FX_SLOT_COUNT};
use crate::config::note_configs::{Note, NoteOct};
use crate::config::osc_configs::Waveform;
use crate::config::filter_configs::FilterType;
use crate::config::track_delay_configs::{
    TRACK_DELAY_DAMP_MAX_HZ, TRACK_DELAY_DAMP_MIN_HZ, TRACK_DELAY_FEEDBACK_MAX_PCT,
    TRACK_DELAY_MIX_MAX_PCT, TRACK_DELAY_TIME_MAX_MS, TRACK_DELAY_TIME_MIN_MS,
};
use crate::config::track_fx_configs::{TRACK_FX_BANK_COUNT, TRACK_FX_SLOT_COUNT};
use crate::config::envelope_configs::{
    ENVELOPE_ATTACK_MAX_MS, ENVELOPE_DECAY_MAX_MS, ENVELOPE_HOLD_MAX_MS,
    ENVELOPE_RELEASE_MAX_MS, ENVELOPE_RELEASE_MIN_MS, ENVELOPE_START_MAX_PCT, ENVELOPE_SUSTAIN_MAX_PCT,
    ENVELOPE_TENSION_MAX,
};
use crate::config::reverb_configs::{
    REVERB_HIGHCUT_MAX, REVERB_LOWCUT_MAX_HZ, REVERB_LOWCUT_MIN_HZ, REVERB_PREDELAY_MAX_MS,
    REVERB_RT60_MAX_MS, REVERB_RT60_MIN_MS, REVERB_SIZE_MAX, REVERB_WIDTH_MAX,
};
use crate::config::track_roll_configs::RollStep;
use crate::config::mydelay_configs::{MYDELAY_LEVEL_MAX, MYDELAY_THRESHOLD_MAX};
use crate::config::{AppConfig, FxKind, InputFx, TrackFx, TrackFxKind};

const INDEX_FILE: &str = "projects_index.json";

#[derive(Clone, Serialize, Deserialize)]
pub struct ProjectEntry {
    pub name: String,
    pub file: String,
}

#[derive(Default, Serialize, Deserialize)]
struct ProjectIndex {
    projects: Vec<ProjectEntry>,
}

#[derive(Serialize, Deserialize)]
pub struct ProjectData {
    pub beat: BeatData,
    pub system: SystemData,
    pub input_fx: InputFxData,
    #[serde(default)]
    pub track_fx: TrackFxData,
}

#[derive(Serialize, Deserialize)]
pub struct BeatData {
    pub bpm: usize,
    pub latency: usize,
}

#[derive(Serialize, Deserialize)]
pub struct SystemData {
    pub input_device: String,
    pub output_device: String,
}

#[derive(Serialize, Deserialize)]
pub struct InputFxData {
    pub selected_bank_idx: usize,
    pub banks: Vec<FxBankData>,
}

#[derive(Serialize, Deserialize)]
pub struct TrackFxData {
    pub selected_bank_idx: usize,
    #[serde(default)]
    pub banks: Vec<TrackFxBankData>,
    #[serde(default)]
    pub tracks: Vec<TrackFxTrackData>,
}

#[derive(Serialize, Deserialize)]
pub struct TrackFxTrackData {
    #[serde(default)]
    pub enabled: Vec<Vec<bool>>,
    #[serde(default)]
    pub banks: Vec<TrackFxBankData>,
}

#[derive(Serialize, Deserialize)]
pub struct TrackFxBankData {
    pub slots: Vec<TrackFxSlotData>,
}

#[derive(Serialize, Deserialize)]
pub struct TrackFxSlotData {
    pub is_enabled: bool,
    pub kind: String,
    #[serde(default)]
    pub delay: Option<TrackDelayData>,
    #[serde(default)]
    pub roll: Option<TrackRollData>,
}

#[derive(Serialize, Deserialize)]
pub struct TrackDelayData {
    pub time_ms: usize,
    pub feedback_pct: usize,
    pub high_damp_hz: usize,
    pub mix_pct: usize,
}

#[derive(Serialize, Deserialize)]
pub struct TrackRollData {
    pub step: usize,
}

#[derive(Serialize, Deserialize)]
pub struct FxBankData {
    pub slots: Vec<FxSlotData>,
}

#[derive(Serialize, Deserialize)]
pub struct FxSlotData {
    pub is_enabled: bool,
    pub kind: String,
    pub osc: Option<OscData>,
    pub filter: Option<FilterData>,
    #[serde(default)]
    pub reverb: Option<ReverbData>,
    #[serde(default)]
    pub my_delay: Option<MyDelayData>,
}

#[derive(Serialize, Deserialize)]
pub struct OscData {
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

#[derive(Clone, Serialize, Deserialize)]
pub struct NoteOctData {
    pub note: String,
    pub octave: usize,
}

#[derive(Clone, Serialize, Deserialize)]
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

#[derive(Clone, Serialize, Deserialize)]
pub struct FilterData {
    pub filter_type: String,
    pub cutoff_hz: usize,
    pub resonance_x10: usize,
    pub drive: usize,
    pub mix: usize,
}

#[derive(Serialize, Deserialize)]
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

#[derive(Clone, Serialize, Deserialize)]
pub struct ReverbData {
    pub size: usize,
    pub decay_ms: usize,
    pub predelay_ms: usize,
    pub width: usize,
    pub high_cut: usize,
    pub low_cut: usize,
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
        return vec![];
    }
    let content = fs::read_to_string(path);
    match content {
        Ok(raw) => match serde_json::from_str::<ProjectIndex>(&raw) {
            Ok(idx) => idx.projects,
            Err(_) => vec![],
        },
        Err(_) => vec![],
    }
}

pub fn save_index(entries: &[ProjectEntry]) -> anyhow::Result<()> {
    ensure_project_dir()?;
    let idx = ProjectIndex {
        projects: entries.to_vec(),
    };
    let raw = serde_json::to_string_pretty(&idx)?;
    fs::write(index_path(), raw)?;
    Ok(())
}

pub fn load_project(entry: &ProjectEntry) -> Option<ProjectData> {
    let path = project_file_path(&entry.file);
    if !path.exists() {
        return None;
    }
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str::<ProjectData>(&raw).ok()
}

pub fn save_project(entry: &ProjectEntry, config: &AppConfig) -> anyhow::Result<()> {
    ensure_project_dir()?;
    let data = data_from_config(config);
    let raw = serde_json::to_string_pretty(&data)?;
    fs::write(project_file_path(&entry.file), raw)?;
    Ok(())
}

pub fn remove_project_file(file: &str) {
    let path = project_file_path(file);
    let _ = fs::remove_file(path);
}

pub fn make_project_file_name(name: &str, idx: usize) -> String {
    let safe = sanitize_name(name);
    format!("{}_{}.json", safe, idx)
}

pub fn data_from_config(config: &AppConfig) -> ProjectData {
    let mut banks = Vec::with_capacity(FX_BANK_COUNT);
    for bank in &config.input_fx.banks {
        let mut slots = Vec::with_capacity(FX_SLOT_COUNT);
        for slot in &bank.slots {
            let mut slot_data = FxSlotData {
                is_enabled: slot.is_enabled,
                kind: "None".to_string(),
                osc: None,
                filter: None,
                reverb: None,
                my_delay: None,
            };
            if let Some(fx) = slot.fx.as_ref() {
                match fx {
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
                                filter_type: filter_type_to_string(osc.osc_filter.filter_type.value).to_string(),
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
                            filter_type: filter_type_to_string(filter.filter_type.value).to_string(),
                            cutoff_hz: filter.cutoff_hz.value,
                            resonance_x10: filter.resonance_x10.value,
                            drive: filter.drive.value,
                            mix: filter.mix.value,
                        });
                    }
                    InputFx::Reverb(reverb) => {
                        slot_data.kind = "Reverb".to_string();
                        slot_data.reverb = Some(ReverbData {
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
                                filter_type: filter_type_to_string(delay.filter.filter_type.value).to_string(),
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
                is_enabled: false,
                kind: "None".to_string(),
                delay: None,
                roll: None,
            };
            if let Some(fx) = slot.fx.as_ref() {
                match fx {
                    TrackFx::Delay(delay) => {
                        slot_data.kind = "Delay".to_string();
                        slot_data.delay = Some(TrackDelayData {
                            time_ms: delay.time_ms.value,
                            feedback_pct: delay.feedback_pct.value,
                            high_damp_hz: delay.high_damp_hz.value,
                            mix_pct: delay.mix_pct.value,
                        });
                    }
                    TrackFx::Roll(roll) => {
                        slot_data.kind = "Roll".to_string();
                        slot_data.roll = Some(TrackRollData {
                            step: roll.step.value.value(),
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
    config.beat_config.set_values(data.beat.bpm, data.beat.latency);
    config.system_config.input_device.value = data.system.input_device;
    config.system_config.output_device.value = data.system.output_device;
    config.input_fx.sel_bank_idx = data.input_fx.selected_bank_idx.min(FX_BANK_COUNT - 1);

    for (bank_idx, bank_data) in data.input_fx.banks.iter().take(FX_BANK_COUNT).enumerate() {
        for (slot_idx, slot_data) in bank_data.slots.iter().take(FX_SLOT_COUNT).enumerate() {
            let slot = &mut config.input_fx.banks[bank_idx].slots[slot_idx];
            slot.is_enabled = slot_data.is_enabled;
            match slot_data.kind.as_str() {
                "Oscillator" => {
                    slot.set_kind(FxKind::Oscillator);
                    if let Some(InputFx::Oscillator(osc)) = slot.fx.as_mut() {
                        if let Some(osc_data) = &slot_data.osc {
                            if let Some(w) = string_to_waveform(&osc_data.waveform) {
                                osc.waveform.value = w;
                            }
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
                            if let Some(t) = string_to_filter_type(&osc_data.osc_filter.filter_type) {
                                osc.osc_filter.filter_type.value = t;
                            }
                            osc.osc_filter.cutoff_hz.value =
                                osc_data.osc_filter.cutoff_hz.clamp(20, 20_000);
                            osc.osc_filter.resonance_x10.value =
                                osc_data.osc_filter.resonance_x10.clamp(1, 100);
                            osc.osc_filter.drive.value = osc_data.osc_filter.drive.min(100);
                            osc.osc_filter.mix.value = osc_data.osc_filter.mix.min(100);
                            osc.osc_filter_env.attack_ms.value =
                                osc_data.osc_filter_envelope.attack_ms.min(ENVELOPE_ATTACK_MAX_MS);
                            osc.osc_filter_env.hold_ms.value =
                                osc_data.osc_filter_envelope.hold_ms.min(ENVELOPE_HOLD_MAX_MS);
                            osc.osc_filter_env.decay_ms.value =
                                osc_data.osc_filter_envelope.decay_ms.min(ENVELOPE_DECAY_MAX_MS);
                            osc.osc_filter_env.sustain_pct.value =
                                osc_data.osc_filter_envelope.sustain_pct.min(ENVELOPE_SUSTAIN_MAX_PCT);
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
                        }
                    }
                }
                "Reverb" => {
                    slot.set_kind(FxKind::Reverb);
                    if let Some(InputFx::Reverb(reverb)) = slot.fx.as_mut() {
                        if let Some(reverb_data) = &slot_data.reverb {
                            reverb.size.value = reverb_data.size.min(REVERB_SIZE_MAX);
                            reverb.decay_ms.value =
                                reverb_data.decay_ms.clamp(REVERB_RT60_MIN_MS, REVERB_RT60_MAX_MS);
                            reverb.predelay_ms.value = reverb_data.predelay_ms.min(REVERB_PREDELAY_MAX_MS);
                            reverb.width.value = reverb_data.width.min(REVERB_WIDTH_MAX);
                            reverb.high_cut.value = reverb_data.high_cut.min(REVERB_HIGHCUT_MAX);
                            reverb.low_cut.value =
                                reverb_data.low_cut.clamp(REVERB_LOWCUT_MIN_HZ, REVERB_LOWCUT_MAX_HZ);
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
                            delay.note
                                .set_seq_with_steps(seq, delay_data.note_step_len_seq.clone());
                            if let Some(t) = string_to_filter_type(&delay_data.filter.filter_type) {
                                delay.filter.filter_type.value = t;
                            }
                            delay.filter.cutoff_hz.value = delay_data.filter.cutoff_hz.clamp(20, 20_000);
                            delay.filter.resonance_x10.value = delay_data.filter.resonance_x10.clamp(1, 100);
                            delay.filter.drive.value = delay_data.filter.drive.min(100);
                            delay.filter.mix.value = delay_data.filter.mix.min(100);
                            delay.audio_env.attack_ms.value =
                                delay_data.audio_env.attack_ms.min(ENVELOPE_ATTACK_MAX_MS);
                            delay.audio_env.hold_ms.value =
                                delay_data.audio_env.hold_ms.min(ENVELOPE_HOLD_MAX_MS);
                            delay.audio_env.decay_ms.value =
                                delay_data.audio_env.decay_ms.min(ENVELOPE_DECAY_MAX_MS);
                            delay.audio_env.sustain_pct.value =
                                delay_data.audio_env.sustain_pct.min(ENVELOPE_SUSTAIN_MAX_PCT);
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
                            delay.filter_env.sustain_pct.value =
                                delay_data.filter_env.sustain_pct.min(ENVELOPE_SUSTAIN_MAX_PCT);
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
                }
                _ => {
                    slot.set_kind(FxKind::None);
                }
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
            match slot_data.kind.as_str() {
                "Delay" => {
                    config.track_fx.set_slot_kind(bank_idx, slot_idx, TrackFxKind::Delay);
                    if let Some(TrackFx::Delay(delay)) = config.track_fx.slot_fx_mut(bank_idx, slot_idx) {
                        if let Some(delay_data) = &slot_data.delay {
                            delay.time_ms.value = delay_data
                                .time_ms
                                .clamp(TRACK_DELAY_TIME_MIN_MS, TRACK_DELAY_TIME_MAX_MS);
                            delay.feedback_pct.value = delay_data.feedback_pct.min(TRACK_DELAY_FEEDBACK_MAX_PCT);
                            delay.high_damp_hz.value = delay_data
                                .high_damp_hz
                                .clamp(TRACK_DELAY_DAMP_MIN_HZ, TRACK_DELAY_DAMP_MAX_HZ);
                            delay.mix_pct.value = delay_data.mix_pct.min(TRACK_DELAY_MIX_MAX_PCT);
                        }
                    }
                }
                "Roll" => {
                    config.track_fx.set_slot_kind(bank_idx, slot_idx, TrackFxKind::Roll);
                    if let Some(TrackFx::Roll(roll)) = config.track_fx.slot_fx_mut(bank_idx, slot_idx) {
                        if let Some(roll_data) = &slot_data.roll {
                            roll.step.value = match roll_data.step {
                                2 => RollStep::Two,
                                8 => RollStep::Eight,
                                _ => RollStep::Four,
                            };
                        }
                    }
                }
                _ => {
                    config.track_fx.set_slot_kind(bank_idx, slot_idx, TrackFxKind::None);
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
            for (bank_idx, bank_enabled) in track_data.enabled.iter().take(TRACK_FX_BANK_COUNT).enumerate() {
                for (slot_idx, enabled) in bank_enabled.iter().take(TRACK_FX_SLOT_COUNT).enumerate() {
                    track.enabled[bank_idx][slot_idx] = *enabled;
                }
            }
        } else {
            // Backward compatibility: old schema stored per-track `is_enabled` in `banks.slots`.
            for (bank_idx, bank_data) in track_data.banks.iter().take(TRACK_FX_BANK_COUNT).enumerate() {
                for (slot_idx, slot_data) in bank_data.slots.iter().take(TRACK_FX_SLOT_COUNT).enumerate() {
                    track.enabled[bank_idx][slot_idx] = slot_data.is_enabled;
                }
            }
        }
    }
}

fn default_tension_value() -> usize {
    50
}

fn projects_root() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        return Path::new(&appdata).join("rc505_rs").join("projects");
    }
    PathBuf::from("projects")
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
    }
}

fn string_to_waveform(s: &str) -> Option<Waveform> {
    match s {
        "Sine" => Some(Waveform::Sine),
        "Saw" => Some(Waveform::Saw),
        "Square" => Some(Waveform::Square),
        "Triangle" => Some(Waveform::Triangle),
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
