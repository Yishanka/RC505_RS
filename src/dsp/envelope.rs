#[derive(Clone, Copy)]
pub struct AhdsrParams {
    pub attack_ms: f32,
    pub hold_ms: f32,
    pub decay_ms: f32,
    pub sustain_level: f32,
    pub release_ms: f32,
    pub start_level: f32,
    pub tension_attack: f32,
    pub tension_decay: f32,
    pub tension_release: f32,
}

#[derive(Clone, Copy, PartialEq)]
enum AhdsrPhase {
    Idle,
    Attack,
    Hold,
    Decay,
    Sustain,
    Release,
}

#[derive(Clone, Copy)]
pub struct AhdsrState {
    phase: AhdsrPhase,
    phase_ms: f64,
    attack_start_level: f32,
    level: f32,
    release_start_level: f32,
    prev_note_on: bool,
}

impl AhdsrState {
    pub fn new() -> Self {
        Self {
            phase: AhdsrPhase::Idle,
            phase_ms: 0.0,
            attack_start_level: 0.0,
            level: 0.0,
            release_start_level: 0.0,
            prev_note_on: false,
        }
    }

    pub fn next(
        &mut self,
        note_on: bool,
        retrigger: bool,
        params: AhdsrParams,
        dt_secs: f32,
    ) -> f32 {
        let sustain = params.sustain_level.clamp(0.0, 1.0);
        if note_on && (!self.prev_note_on || retrigger) {
            self.attack_start_level = if self.phase == AhdsrPhase::Idle {
                params.start_level.clamp(0.0, 1.0)
            } else {
                self.level
            };
            self.phase = AhdsrPhase::Attack;
            self.phase_ms = 0.0;
        } else if !note_on && self.prev_note_on {
            self.phase = AhdsrPhase::Release;
            self.phase_ms = 0.0;
            self.release_start_level = self.level;
        }
        self.prev_note_on = note_on;
        let mut remaining = (dt_secs.max(0.0) as f64) * 1000.0;
        // Carry leftover time through zero/short stages within the same sample.
        // f64 stage time avoids accumulated f32 error in long high-rate envelopes.
        for _ in 0..6 {
            let (duration, next) = match self.phase {
                AhdsrPhase::Idle => {
                    self.level = 0.0;
                    return 0.0;
                }
                AhdsrPhase::Sustain => {
                    self.level = sustain;
                    return sustain;
                }
                AhdsrPhase::Attack => (params.attack_ms.max(0.0), AhdsrPhase::Hold),
                AhdsrPhase::Hold => (params.hold_ms.max(0.0), AhdsrPhase::Decay),
                AhdsrPhase::Decay => (params.decay_ms.max(0.0), AhdsrPhase::Sustain),
                AhdsrPhase::Release => (params.release_ms.max(1.0), AhdsrPhase::Idle),
            };
            let duration = duration as f64;
            let advance = remaining.min((duration - self.phase_ms).max(0.0));
            self.phase_ms += advance;
            remaining -= advance;
            let progress = if duration <= 0.0 {
                1.0
            } else {
                (self.phase_ms / duration).clamp(0.0, 1.0) as f32
            };
            self.level = match self.phase {
                AhdsrPhase::Attack => {
                    self.attack_start_level
                        + (1.0 - self.attack_start_level)
                            * pow_curve(progress, params.tension_attack.max(0.01))
                }
                AhdsrPhase::Hold => 1.0,
                AhdsrPhase::Decay => {
                    sustain
                        + (1.0 - sustain)
                            * pow_curve(1.0 - progress, params.tension_decay.max(0.01))
                }
                AhdsrPhase::Release => {
                    self.release_start_level
                        * pow_curve(1.0 - progress, params.tension_release.max(0.01))
                }
                _ => self.level,
            };
            if self.phase_ms < duration {
                return self.level.clamp(0.0, 1.0);
            }
            self.phase = next;
            self.phase_ms = 0.0;
            if remaining <= 0.0 && duration > 0.0 {
                return self.level.clamp(0.0, 1.0);
            }
        }
        self.level.clamp(0.0, 1.0)
    }
}

fn pow_curve(progress: f32, exponent: f32) -> f32 {
    let x = progress.clamp(0.0, 1.0);
    if (exponent - 1.0).abs() < 0.0001 {
        x
    } else if (exponent - 2.0).abs() < 0.0001 {
        x * x
    } else if (exponent - 0.5).abs() < 0.0001 {
        x.sqrt()
    } else {
        x.powf(exponent)
    }
}

/// Monotonic normalized segment curve, shared by the curve editor and LFO.
/// A bounded curvature replaces the old demo's unbounded exponent UI.
pub fn bend_curve(t: f32, curve: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let c = curve.clamp(-1.0, 1.0);
    if c.abs() < 0.0001 {
        t
    } else if c > 0.0 {
        t.powf(1.0 + c * 7.0)
    } else {
        1.0 - (1.0 - t).powf(1.0 - c * 7.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p() -> AhdsrParams {
        AhdsrParams {
            attack_ms: 0.0,
            hold_ms: 0.0,
            decay_ms: 0.0,
            sustain_level: 0.35,
            release_ms: 100.0,
            start_level: 0.0,
            tension_attack: 1.0,
            tension_decay: 1.0,
            tension_release: 1.0,
        }
    }
    #[test]
    fn zero_stages_and_release_retrigger_are_continuous() {
        let mut e = AhdsrState::new();
        assert_eq!(e.next(true, false, p(), 1.0 / 48000.0), 0.35);
        for _ in 0..2400 {
            e.next(false, false, p(), 1.0 / 48000.0);
        }
        let before = e.level;
        let after = e.next(
            true,
            true,
            AhdsrParams {
                attack_ms: 20.0,
                ..p()
            },
            1.0 / 48000.0,
        );
        assert!(
            after >= before && after - before < 0.01,
            "Retrigger must rise from current release level"
        );
    }
    #[test]
    fn long_release_duration_is_sample_accurate_at_high_rates() {
        for sr in [8000.0, 44100.0, 48000.0, 96000.0, 192000.0] {
            let params = AhdsrParams {
                release_ms: 5000.0,
                ..p()
            };
            let mut e = AhdsrState::new();
            e.next(true, false, params, 1.0 / sr);
            let frames = (sr * 5.0) as usize;
            for i in 0..frames - 1 {
                let value = e.next(false, false, params, 1.0 / sr);
                assert!(value > 0.0, "Release ended before frame {i} at {sr}");
            }
            let final_sample = e.next(false, false, params, 1.0 / sr);
            let next = e.next(false, false, params, 1.0 / sr);
            assert!(final_sample < 1e-6);
            assert_eq!(next, 0.0);
        }
    }
}
