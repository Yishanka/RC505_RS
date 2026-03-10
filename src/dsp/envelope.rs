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
    phase_ms: f32,
    level: f32,
    release_start_level: f32,
    prev_note_on: bool,
}

impl AhdsrState {
    pub fn new() -> Self {
        Self {
            phase: AhdsrPhase::Idle,
            phase_ms: 0.0,
            level: 0.0,
            release_start_level: 0.0,
            prev_note_on: false,
        }
    }

    pub fn next(&mut self, note_on: bool, retrigger: bool, params: AhdsrParams, dt_secs: f32) -> f32 {
        if note_on && (!self.prev_note_on || retrigger) {
            self.phase = AhdsrPhase::Attack;
            self.phase_ms = 0.0;
        } else if !note_on && self.prev_note_on {
            self.phase = AhdsrPhase::Release;
            self.phase_ms = 0.0;
            self.release_start_level = self.level;
        }
        self.prev_note_on = note_on;

        let dt_ms = (dt_secs * 1000.0).max(0.0);
        self.phase_ms += dt_ms;

        let attack_ms = params.attack_ms.max(0.0);
        let hold_ms = params.hold_ms.max(0.0);
        let decay_ms = params.decay_ms.max(0.0);
        let sustain = params.sustain_level.clamp(0.0, 1.0);
        let start = params.start_level.clamp(0.0, 1.0);
        let release_ms = params.release_ms.max(1.0);
        let tension_a = params.tension_attack.max(0.01);
        let tension_d = params.tension_decay.max(0.01);
        let tension_r = params.tension_release.max(0.01);

        match self.phase {
            AhdsrPhase::Idle => {
                self.level = 0.0;
            }
            AhdsrPhase::Attack => {
                if attack_ms <= 0.0 {
                    self.level = 1.0;
                    self.phase = AhdsrPhase::Hold;
                    self.phase_ms = 0.0;
                } else {
                    let progress = (self.phase_ms / attack_ms).clamp(0.0, 1.0);
                    let curve = pow_curve(progress, tension_a);
                    self.level = start + (1.0 - start) * curve;
                    if progress >= 1.0 {
                        self.phase = AhdsrPhase::Hold;
                        self.phase_ms = 0.0;
                    }
                }
            }
            AhdsrPhase::Hold => {
                self.level = 1.0;
                if self.phase_ms >= hold_ms {
                    self.phase = AhdsrPhase::Decay;
                    self.phase_ms = 0.0;
                }
            }
            AhdsrPhase::Decay => {
                if decay_ms <= 0.0 {
                    self.level = sustain;
                    self.phase = AhdsrPhase::Sustain;
                    self.phase_ms = 0.0;
                } else {
                    let progress = (self.phase_ms / decay_ms).clamp(0.0, 1.0);
                    let curve = pow_curve(1.0 - progress, tension_d);
                    self.level = sustain + (1.0 - sustain) * curve;
                    if progress >= 1.0 {
                        self.phase = AhdsrPhase::Sustain;
                        self.phase_ms = 0.0;
                    }
                }
            }
            AhdsrPhase::Sustain => {
                self.level = sustain;
            }
            AhdsrPhase::Release => {
                let progress = (self.phase_ms / release_ms).clamp(0.0, 1.0);
                let curve = pow_curve(1.0 - progress, tension_r);
                self.level = self.release_start_level * curve;
                if progress >= 1.0 {
                    self.phase = AhdsrPhase::Idle;
                    self.phase_ms = 0.0;
                    self.level = 0.0;
                }
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
