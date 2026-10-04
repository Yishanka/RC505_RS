use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum TimeMode {
    #[default]
    Milliseconds,
    Quarter,
    Eighth,
    DottedEighth,
    Sixteenth,
    EighthTriplet,
}
impl TimeMode {
    pub const ALL: [Self; 6] = [
        Self::Milliseconds,
        Self::Quarter,
        Self::Eighth,
        Self::DottedEighth,
        Self::Sixteenth,
        Self::EighthTriplet,
    ];
    pub fn milliseconds(self, time_ms: usize, bpm: usize) -> f32 {
        self.milliseconds_f32(time_ms as f32, bpm)
    }
    pub fn milliseconds_f32(self, time_ms: f32, bpm: usize) -> f32 {
        let beat = 60000.0 / bpm.max(1) as f32;
        match self {
            Self::Milliseconds => time_ms,
            Self::Quarter => beat,
            Self::Eighth => beat * 0.5,
            Self::DottedEighth => beat * 0.75,
            Self::Sixteenth => beat * 0.25,
            Self::EighthTriplet => beat / 3.0,
        }
    }
}
impl std::fmt::Display for TimeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Milliseconds => "Milliseconds",
            Self::Quarter => "1/4 note",
            Self::Eighth => "1/8 note",
            Self::DottedEighth => "1/8 dotted",
            Self::Sixteenth => "1/16 note",
            Self::EighthTriplet => "1/8 triplet",
        })
    }
}
