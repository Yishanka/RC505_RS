//! Single control-thread conversion used by every AHDSR owner.
use crate::{config::envelope_configs::*, dsp::envelope::AhdsrParams};
pub fn from_config(c: &EnvelopeConfigs) -> AhdsrParams {
    let tension =
        |value: usize| 2.0f32.powf((value.min(ENVELOPE_TENSION_MAX) as f32 - 100.0) / 50.0);
    AhdsrParams {
        attack_ms: c.attack_ms.bounded(0.0, ENVELOPE_ATTACK_MAX_MS),
        hold_ms: c.hold_ms.bounded(0.0, ENVELOPE_HOLD_MAX_MS),
        decay_ms: c.decay_ms.bounded(0.0, ENVELOPE_DECAY_MAX_MS),
        release_ms: c
            .release_ms
            .bounded(ENVELOPE_RELEASE_MIN_MS, ENVELOPE_RELEASE_MAX_MS),
        sustain_level: (c.sustain_pct.value.min(ENVELOPE_SUSTAIN_MAX_PCT) as f32 / 100.0)
            .clamp(0.0, 1.0),
        start_level: (c.start_pct.value.min(ENVELOPE_START_MAX_PCT) as f32 / 100.0).clamp(0.0, 1.0),
        tension_attack: tension(c.tension_a.value),
        tension_decay: tension(c.tension_d.value),
        tension_release: tension(c.tension_r.value),
    }
}

#[cfg(test)]
#[path = "envelope_precision_tests.rs"]
mod tests;
