//! Independent key faders. Integrate the speed curve in time so frame rate
//! does not change the fade; key repeat never drives this controller.
pub const MIN_DB: f32 = -60.0;

pub fn decibels(gain: f32) -> f32 {
    if gain <= 0.001 {
        MIN_DB
    } else {
        (20.0 * gain.log10()).clamp(MIN_DB, 0.0)
    }
}

pub fn gain(db: f32) -> f32 {
    if db <= MIN_DB {
        0.0
    } else {
        10.0_f32.powf(db.min(0.0) / 20.0)
    }
}

#[derive(Clone, Copy, Default)]
pub struct KeyFader {
    direction: i8,
    held: f32,
}

impl KeyFader {
    pub fn delta(&mut self, direction: i8, dt: f32, speed: f32, tap: f32) -> f32 {
        if direction == 0 {
            *self = Self::default();
            return 0.0;
        }
        let mut delta = 0.0;
        if self.direction != direction {
            self.direction = direction;
            self.held = 0.0;
            delta = tap;
        }
        let previous = self.held;
        self.held += dt.clamp(0.0, 0.05);
        delta += distance(self.held, speed) - distance(previous, speed);
        direction as f32 * delta
    }
    pub fn advance(&mut self, level: &mut f32, direction: i8, dt: f32, speed: f32) {
        let delta = self.delta(direction, dt, speed.clamp(1.0, 60.0), 0.5);
        if delta == 0.0 {
            return;
        }
        *level = gain((decibels(*level) + delta).clamp(MIN_DB, 0.0));
    }
}

fn distance(time: f32, speed: f32) -> f32 {
    let moving = (time - 0.18).max(0.0);
    let ramp = moving.min(0.45);
    let initial = speed / 4.0;
    initial * ramp + (speed - initial) * ramp * ramp / 0.9 + speed * (moving - ramp)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tap_hold_and_frame_rate_are_predictable() {
        let mut key = KeyFader::default();
        let mut level = 1.0;
        key.advance(&mut level, -1, 0.016, 24.0);
        assert!((decibels(level) + 0.5).abs() < 0.001);
        key.advance(&mut level, 0, 1.0, 24.0);
        assert!((decibels(level) + 0.5).abs() < 0.001);
        key.advance(&mut level, -1, 0.016, 24.0);
        assert!((decibels(level) + 1.0).abs() < 0.001);
        let render = |fps: usize| {
            let mut key = KeyFader::default();
            let mut level = 1.0;
            for _ in 0..fps {
                key.advance(&mut level, -1, 1.0 / fps as f32, 24.0);
            }
            decibels(level)
        };
        assert!((render(30) - render(144)).abs() < 0.001);
    }
    #[test]
    fn simultaneous_tracks_opposition_mute_and_release() {
        let mut keys = [KeyFader::default(); 5];
        let mut levels = [0.5; 5];
        for _ in 0..120 {
            keys[0].advance(&mut levels[0], -1, 1.0 / 60.0, 60.0);
            keys[1].advance(&mut levels[1], 1, 1.0 / 60.0, 24.0);
            keys[2].advance(&mut levels[2], 0, 1.0 / 60.0, 24.0);
        }
        assert_eq!(levels[0], 0.0);
        assert_eq!(levels[1], 1.0);
        assert_eq!(levels[2], 0.5);
        let previous = levels;
        for i in 0..5 {
            keys[i].advance(&mut levels[i], 0, 1.0, 24.0);
        }
        assert_eq!(levels, previous);
        keys[0].advance(&mut levels[0], 1, 0.016, 24.0);
        assert!(levels[0] > 0.0);
    }
}
