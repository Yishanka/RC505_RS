//! Auditable software recipes named after the public TYPE vocabulary. Values
//! are our design, not undocumented BOSS constants or nineteen claimed models.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DynamicsProfile {
    #[default]
    Custom,
    NaturalComp,
    MixerComp,
    LiveComp,
    NaturalLim,
    HardLim,
    JinglComp,
    HardComp,
    SoftComp,
    CleanComp,
    DanceComp,
    OrchComp,
    VocalComp,
    Acoustic,
    RockBand,
    Orchestra,
    LowBoost,
    Brighten,
    DjsVoice,
    PhoneVox,
}
#[derive(Clone, Copy, Debug)]
pub struct Recipe {
    pub rms: bool,
    pub limiter: bool,
    pub threshold: f32,
    pub ratio: f32,
    pub knee: f32,
    pub attack: f32,
    pub release: f32,
    pub sidechain_hz: f32,
    pub makeup: f32,
    pub low_db: f32,
    pub high_db: f32,
    pub presence_db: f32,
    pub highpass_hz: f32,
    pub lowpass_hz: f32,
}
impl DynamicsProfile {
    pub const ALL: [Self; 20] = [
        Self::Custom,
        Self::NaturalComp,
        Self::MixerComp,
        Self::LiveComp,
        Self::NaturalLim,
        Self::HardLim,
        Self::JinglComp,
        Self::HardComp,
        Self::SoftComp,
        Self::CleanComp,
        Self::DanceComp,
        Self::OrchComp,
        Self::VocalComp,
        Self::Acoustic,
        Self::RockBand,
        Self::Orchestra,
        Self::LowBoost,
        Self::Brighten,
        Self::DjsVoice,
        Self::PhoneVox,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Custom => "Custom",
            Self::NaturalComp => "Natural Comp",
            Self::MixerComp => "Mixer Comp",
            Self::LiveComp => "Live Comp",
            Self::NaturalLim => "Natural Lim",
            Self::HardLim => "Hard Lim",
            Self::JinglComp => "Jingl Comp",
            Self::HardComp => "Hard Comp",
            Self::SoftComp => "Soft Comp",
            Self::CleanComp => "Clean Comp",
            Self::DanceComp => "Dance Comp",
            Self::OrchComp => "Orch Comp",
            Self::VocalComp => "Vocal Comp",
            Self::Acoustic => "Acoustic",
            Self::RockBand => "Rock Band",
            Self::Orchestra => "Orchestra",
            Self::LowBoost => "Low Boost",
            Self::Brighten => "Brighten",
            Self::DjsVoice => "DJs Voice",
            Self::PhoneVox => "Phone Vox",
        }
    }
    pub fn recipe(self, amount: f32) -> Recipe {
        // T, ratio, knee, attack, release, sidechain, makeup, RMS detector.
        let (t, r, k, a, d, sc, m, rms) = match self {
            Self::Custom | Self::NaturalComp => (-18.0, 2.0, 9.0, 20.0, 180.0, 60.0, 3.0, true),
            Self::MixerComp => (-20.0, 3.0, 6.0, 10.0, 120.0, 80.0, 4.0, true),
            Self::LiveComp => (-15.0, 4.0, 4.0, 3.0, 80.0, 100.0, 3.0, false),
            Self::NaturalLim => (-2.0, 1000.0, 6.0, 2.0, 120.0, 0.0, 0.0, false),
            Self::HardLim => (-1.0, 1000.0, 0.0, 0.1, 50.0, 0.0, 0.0, false),
            Self::JinglComp => (-26.0, 6.0, 5.0, 2.0, 180.0, 100.0, 7.0, true),
            Self::HardComp => (-22.0, 8.0, 0.0, 1.0, 90.0, 80.0, 5.0, false),
            Self::SoftComp => (-22.0, 2.5, 12.0, 25.0, 240.0, 60.0, 3.0, true),
            Self::CleanComp => (-14.0, 1.6, 8.0, 30.0, 160.0, 120.0, 1.0, true),
            Self::DanceComp => (-24.0, 5.0, 4.0, 12.0, 85.0, 35.0, 6.0, false),
            Self::OrchComp => (-16.0, 1.5, 12.0, 50.0, 450.0, 80.0, 1.0, true),
            Self::VocalComp => (-24.0, 3.5, 6.0, 6.0, 110.0, 130.0, 5.0, true),
            Self::Acoustic => (-18.0, 2.2, 8.0, 30.0, 220.0, 100.0, 2.0, true),
            Self::RockBand => (-22.0, 4.0, 5.0, 15.0, 130.0, 90.0, 4.0, false),
            Self::Orchestra => (-20.0, 1.8, 10.0, 60.0, 600.0, 120.0, 2.0, true),
            Self::LowBoost => (-20.0, 3.0, 6.0, 12.0, 160.0, 110.0, 3.0, true),
            Self::Brighten => (-18.0, 2.0, 8.0, 18.0, 130.0, 100.0, 2.0, true),
            Self::DjsVoice => (-28.0, 6.0, 4.0, 1.0, 100.0, 140.0, 7.0, false),
            Self::PhoneVox => (-24.0, 4.0, 3.0, 3.0, 100.0, 0.0, 4.0, false),
        };
        let amount = if amount.is_finite() {
            amount.clamp(-20.0, 20.0)
        } else {
            0.0
        };
        let limiter = matches!(self, Self::NaturalLim | Self::HardLim);
        let mut p = Recipe {
            rms,
            limiter,
            threshold: (t - amount * if limiter { 0.12 } else { 0.35 }).clamp(-50.0, 0.0),
            ratio: if limiter {
                r
            } else {
                1.0 + (r - 1.0) * 2.0f32.powf(amount / 20.0)
            },
            knee: k,
            attack: a,
            release: d,
            sidechain_hz: sc,
            makeup: m,
            low_db: 0.0,
            high_db: 0.0,
            presence_db: 0.0,
            highpass_hz: 0.0,
            lowpass_hz: 0.0,
        };
        match self {
            Self::JinglComp => p.high_db = 2.0,
            Self::VocalComp => p.presence_db = 2.0,
            Self::Acoustic => p.high_db = 1.5,
            Self::RockBand => p.presence_db = 1.5,
            Self::Orchestra => {
                p.low_db = 0.5;
                p.high_db = 0.5;
            }
            Self::LowBoost => p.low_db = 6.0,
            Self::Brighten => p.high_db = 5.0,
            Self::DjsVoice => {
                p.highpass_hz = 100.0;
                p.presence_db = 3.0;
            }
            Self::PhoneVox => {
                p.highpass_hz = 300.0;
                p.lowpass_hz = 3400.0;
            }
            _ => {}
        }
        p
    }
}
