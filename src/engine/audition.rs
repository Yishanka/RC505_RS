//! Independent editor audition: private clock and isolated monitor-only target.
use super::{
    core::RenderCore,
    input_fx::{InputFxEngine, InputFxRuntime},
    loop_audio::Frame,
    track_fx::{TrackFxEngine, TrackFxRuntime},
};
use crate::{
    config::{AppConfig, InputFx, TrackFx},
    presets::FxTarget,
};
pub enum AuditionParameters {
    Note {
        runtime: Option<super::input_fx::OscillatorRuntime>,
        seconds: f32,
    },
    Input {
        runtime: InputFxRuntime,
        bpm: usize,
        uses_input: bool,
    },
    Track {
        runtime: TrackFxRuntime,
        bpm: usize,
        source: usize,
    },
}
pub fn supports(config: &AppConfig, target: FxTarget) -> bool {
    match target {
        FxTarget::Input { bank, slot } => matches!(
            config.input_fx.banks[bank].slots[slot].fx,
            Some(InputFx::Oscillator(_) | InputFx::MyDelay(_))
        ),
        FxTarget::Track { bank, slot } => matches!(
            config.track_fx.banks[bank].slots[slot].fx,
            Some(TrackFx::Filter(_))
        ),
    }
}
impl AuditionParameters {
    pub fn single_note(
        config: &AppConfig,
        target: FxTarget,
        pitch: crate::config::note_configs::NoteOct,
        velocity: u8,
    ) -> Option<Self> {
        let FxTarget::Input { bank, slot } = target else {
            return None;
        };
        if !matches!(
            config.input_fx.banks.get(bank)?.slots.get(slot)?.fx,
            Some(InputFx::Oscillator(_))
        ) {
            return None;
        }
        let source = crate::project::data_from_config(config);
        let mut staging = AppConfig::new(120, 0, 5);
        let mut data = crate::project::data_from_config(&staging);
        data.input_fx.banks[0].slots[0] = source.input_fx.banks[bank].slots[slot].clone();
        crate::project::apply_data_to_config(&mut staging, data);
        if let Some(InputFx::Oscillator(osc)) = &mut staging.input_fx.banks[0].slots[0].fx {
            use crate::config::sequence_edit::{MAX_TICKS, NoteEvent, PPQ};
            osc.note.pending = None;
            osc.note.launch_serial = 0;
            osc.note.replace_events(
                MAX_TICKS,
                &[NoteEvent {
                    velocity: velocity.clamp(1, 127),
                    ..NoteEvent::new(0, PPQ / 2, pitch)
                }],
            );
            osc.input_gate = false;
            osc.capture = None;
        }
        let mut runtime = InputFxRuntime::from_config(&staging.input_fx).banks[0].slots[0]
            .osc
            .take()?;
        runtime.threshold = 0.0;
        runtime.poly.input_gate = false;
        runtime.poly.capture = None;
        let seconds = 0.25
            + runtime
                .envelope
                .release_ms
                .max(runtime.osc_filter_envelope.release_ms)
                / 1000.0
            + 0.02;
        Some(Self::Note {
            runtime: Some(runtime),
            seconds,
        })
    }
    pub fn new(config: &AppConfig, target: FxTarget, source: usize) -> Option<Self> {
        if !supports(config, target) {
            return None;
        }
        let bpm = config.beat_config.current_bpm();
        Some(match target {
            FxTarget::Input { bank, slot } => {
                let mut runtime = InputFxRuntime::from_config(&config.input_fx);
                for (b, group) in runtime.banks.iter_mut().enumerate() {
                    for (s, effect) in group.slots.iter_mut().enumerate() {
                        effect.enabled = b == bank && s == slot;
                        // Capture mailboxes have one writer: the formal input engine.
                        // Private audition must never race it or overwrite captured PCM.
                        if let Some(osc) = &mut effect.osc {
                            osc.poly.capture = None;
                        }
                    }
                }
                runtime.selected_bank_idx = bank;
                let chosen = &mut runtime.banks[bank].slots[slot];
                let uses_input = chosen.my_delay.is_some();
                if let Some(osc) = &mut chosen.osc {
                    osc.threshold = 0.0;
                    osc.poly.input_gate = false;
                    // Audition follows the phrase currently visible in the
                    // editor. A formal next-loop request must not secretly
                    // switch a private preview ahead of the performance clock.
                    osc.poly.phrase.queued = None;
                }
                if let Some(delay) = &mut chosen.my_delay {
                    delay.threshold = 0.0;
                }
                Self::Input {
                    runtime,
                    bpm,
                    uses_input,
                }
            }
            FxTarget::Track { bank, slot } => {
                let mut runtime = TrackFxRuntime::from_config(&config.track_fx);
                runtime.track_enabled.truncate(1);
                runtime.track_enabled[0] = [[false; 4]; 4];
                runtime.track_enabled[0][bank][slot] = true;
                runtime.selected_bank_idx = bank;
                Self::Track {
                    runtime,
                    bpm,
                    source,
                }
            }
        })
    }
}
enum Voice {
    Note(crate::dsp::oscillator::PolyOscState),
    Input(InputFxEngine),
    Track(TrackFxEngine),
}
pub struct Audition {
    note: Option<super::input_fx::OscillatorRuntime>,
    until: Option<u64>,
    voice: Voice,
    pub frame: u64,
    sr: u32,
    uses_input: bool,
    source: usize,
}
impl Audition {
    pub fn new(mut params: AuditionParameters, sr: u32) -> Self {
        let voice = match &mut params {
            AuditionParameters::Note { .. } => {
                Voice::Note(crate::dsp::oscillator::PolyOscState::new())
            }
            AuditionParameters::Input { .. } => {
                let mut e = InputFxEngine::new(sr as f32);
                e.prepare(sr as f32);
                Voice::Input(e)
            }
            AuditionParameters::Track { .. } => {
                let mut e = TrackFxEngine::new(sr as f32, 1);
                e.prepare();
                Voice::Track(e)
            }
        };
        let mut result = Self {
            note: None,
            until: None,
            voice,
            frame: 0,
            sr,
            uses_input: false,
            source: 0,
        };
        result.update(&mut params);
        result
    }
    pub fn update(&mut self, params: &mut AuditionParameters) {
        match (&mut self.voice, params) {
            (Voice::Note(_), AuditionParameters::Note { runtime, seconds }) => {
                std::mem::swap(&mut self.note, runtime);
                self.frame = 0;
                self.until = Some((*seconds * self.sr as f32).ceil() as u64);
            }
            (
                Voice::Input(engine),
                AuditionParameters::Input {
                    runtime,
                    bpm,
                    uses_input,
                },
            ) => {
                *runtime = engine.swap_runtime(std::mem::replace(runtime, InputFxRuntime::empty()));
                engine.set_clock(true, *bpm);
                self.uses_input = *uses_input;
            }
            (
                Voice::Track(engine),
                AuditionParameters::Track {
                    runtime,
                    bpm,
                    source,
                },
            ) => {
                engine.exchange_runtime(runtime);
                engine.set_clock(*bpm, true);
                self.source = *source;
            }
            _ => {}
        }
    }
    pub fn next(&mut self, dry: Frame, core: &RenderCore) -> Frame {
        if self.finished() {
            return [0.0; 2];
        }
        let time = self.frame as f64 / self.sr as f64;
        let result = match &mut self.voice {
            Voice::Note(state) => {
                let Some(osc) = &self.note else {
                    return [0.0; 2];
                };
                let p = crate::dsp::oscillator::OscillatorFxParams {
                    waveform: osc.waveform,
                    level: osc.level,
                    threshold: 0.0,
                    input_level: 1.0,
                    sample_rate: self.sr as f32,
                    note: None,
                    note_on: false,
                    note_retrigger: false,
                    envelope: osc.envelope,
                    filter_envelope: osc.osc_filter_envelope,
                    filter: crate::dsp::filter::FilterParams {
                        filter_type: osc.osc_filter.filter_type,
                        cutoff_hz: osc.osc_filter.cutoff_hz,
                        q: osc.osc_filter.q,
                        drive: osc.osc_filter.drive,
                        mix: osc.osc_filter.mix,
                    },
                    cutoff_min_hz: 20.0,
                };
                let value = crate::dsp::oscillator::process_poly_sample(
                    state, &osc.poly, p, time, 120, true,
                );
                [value, value]
            }
            Voice::Input(engine) => {
                let input = if self.uses_input { dry } else { [0.0; 2] };
                let (l, r) = engine.process_frame(time, input[0], input[1], &[None; 5]);
                [l - input[0], r - input[1]]
            }
            Voice::Track(engine) => {
                let audio = &core.tracks[self.source.min(4)].audio;
                let input = audio.read(self.frame as usize % audio.len.max(1));
                let (l, r) = engine.process_frame(0, time, input[0], input[1]);
                [l, r]
            }
        };
        self.frame += 1;
        result
    }
    pub fn finished(&self) -> bool {
        self.until.is_some_and(|until| self.frame >= until)
    }
}
#[cfg(test)]
mod note_tests {
    use super::*;
    use crate::config::{FxKind, note_configs::NoteOct};
    #[test]
    fn a_note_can_be_heard_without_a_phrase_and_never_mutates_live_tracks_or_patch() {
        let mut config = AppConfig::new(120, 0, 5);
        config.input_fx.set_slot_kind(0, 0, FxKind::Oscillator);
        let before = serde_json::to_vec(&crate::project::data_from_config(&config)).unwrap();
        let target = FxTarget::Input { bank: 0, slot: 0 };
        let params =
            AuditionParameters::single_note(&config, target, NoteOct::from_pitch_index(57), 96)
                .unwrap();
        let mut audition = Audition::new(params, 8000);
        let core = RenderCore::new(8000);
        let mut energy = 0.0;
        let count = crate::test_alloc::count(|| {
            for _ in 0..40000 {
                let out = audition.next([0.5; 2], &core);
                energy += out[0].abs();
            }
        });
        assert_eq!(count, 0);
        assert!(energy > 1.0);
        assert!(audition.finished());
        assert_eq!(audition.next([0.5; 2], &core), [0.0; 2]);
        assert!(core.tracks.iter().all(|t| t.audio.len == 0));
        assert_eq!(core.clock.frame, 0);
        assert_eq!(
            before,
            serde_json::to_vec(&crate::project::data_from_config(&config)).unwrap()
        );
    }
    #[test]
    fn private_phrase_preview_does_not_follow_the_formal_pending_launch() {
        use crate::config::{note_configs::NoteConfigs, sequence_edit::NoteEvent};
        let mut config = AppConfig::new(120, 0, 5);
        config.input_fx.set_slot_kind(0, 0, FxKind::Oscillator);
        let target = FxTarget::Input { bank: 0, slot: 0 };
        crate::presets::note_mut(&mut config, target)
            .unwrap()
            .replace_events(
                960,
                &[NoteEvent::new(0, 480, NoteOct::from_pitch_index(48))],
            );
        let mut next = NoteConfigs::new();
        next.replace_events(
            960,
            &[NoteEvent::new(0, 480, NoteOct::from_pitch_index(60))],
        );
        crate::presets::note_mut(&mut config, target)
            .unwrap()
            .launch_clip(&next.clip(), true);
        let mut queued = Audition::new(AuditionParameters::new(&config, target, 0).unwrap(), 8000);
        crate::presets::note_mut(&mut config, target)
            .unwrap()
            .pending = None;
        let mut plain = Audition::new(AuditionParameters::new(&config, target, 0).unwrap(), 8000);
        let core = RenderCore::new(8000);
        for frame in 0..10000 {
            assert_eq!(
                queued.next([0.0; 2], &core).map(f32::to_bits),
                plain.next([0.0; 2], &core).map(f32::to_bits),
                "Private preview switched at {frame}"
            );
        }
    }
}
