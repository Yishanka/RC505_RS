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
    Input(InputFxEngine),
    Track(TrackFxEngine),
}
pub struct Audition {
    voice: Voice,
    pub frame: u64,
    sr: u32,
    uses_input: bool,
    source: usize,
}
impl Audition {
    pub fn new(mut params: AuditionParameters, sr: u32) -> Self {
        let voice = match &mut params {
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
        let time = self.frame as f64 / self.sr as f64;
        let result = match &mut self.voice {
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
}
