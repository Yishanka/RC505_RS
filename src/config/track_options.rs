use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Quantize {
    Off,
    #[default]
    Beat,
    Measure,
    Loop,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopMode {
    #[default]
    Immediate,
    LoopEnd,
    Fade,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TrackOptions {
    pub reverse: bool,
    pub one_shot: bool,
    pub stop_mode: StopMode,
    pub fade_ms: u32,
    /// Zero means manual finish; otherwise a fixed number of 4/4 measures.
    pub measures: u32,
    pub quantize: Quantize,
    pub fader_speed: f32,
}

impl Default for TrackOptions {
    fn default() -> Self {
        Self {
            reverse: false,
            one_shot: false,
            stop_mode: StopMode::Immediate,
            fade_ms: 1000,
            measures: 0,
            quantize: Quantize::Beat,
            fader_speed: 24.0,
        }
    }
}

impl TrackOptions {
    pub fn normalize(&mut self) {
        self.fade_ms = self.fade_ms.clamp(10, 30_000);
        self.measures = self.measures.min(128);
        self.fader_speed = if self.fader_speed.is_finite() {
            self.fader_speed.clamp(1.0, 60.0)
        } else {
            24.0
        };
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputRouting {
    /// Retains the processing order of projects written before version 2.
    #[default]
    Legacy,
    Serial,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LatencyCalibration {
    pub frames: u32,
    pub sample_rate: u32,
    pub input: String,
    pub output: String,
    pub buffer_frames: u32,
    pub displayed_ms: usize,
}
