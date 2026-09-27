//! Delete gestures belong to one selected track and one uninterrupted focus.
#[derive(Clone, Default)]
pub struct ClearGesture {
    held: Option<(usize, f64)>,
    tap: Option<(usize, f64)>,
    pub progress: f32,
}
impl ClearGesture {
    pub fn cancel(&mut self) {
        *self = Self::default();
    }
    pub fn update(&mut self, track: usize, down: bool, pressed: bool, time: f64) -> bool {
        if self.held.is_some_and(|(target, _)| target != track)
            || self.tap.is_some_and(|(target, _)| target != track)
        {
            self.cancel();
        }
        if pressed {
            if self
                .tap
                .is_some_and(|(target, at)| target == track && time - at <= 0.35)
            {
                self.cancel();
                return true;
            }
            self.held = Some((track, time));
            self.tap = Some((track, time));
        }
        if !down {
            self.held = None;
            self.progress = 0.0;
        }
        if let Some((_, at)) = self.held {
            self.progress = ((time - at) / 0.75).clamp(0.0, 1.0) as f32;
            if time - at >= 0.75 {
                self.cancel();
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tap_does_not_clear_hold_fires_once_and_double_tap_fires_on_second_press() {
        let mut g = ClearGesture::default();
        assert!(!g.update(0, true, true, 0.0));
        assert!(!g.update(0, false, false, 0.1));
        assert!(g.update(0, true, true, 0.25));
        assert!(!g.update(0, true, false, 1.0));
        assert!(!g.update(0, false, false, 1.1));
        assert!(!g.update(0, true, true, 2.0));
        assert!(!g.update(0, true, false, 2.74));
        assert!(g.update(0, true, false, 2.75));
        assert!(!g.update(0, true, false, 4.0));
    }
    #[test]
    fn track_change_focus_loss_and_slow_double_press_do_not_erase() {
        let mut g = ClearGesture::default();
        g.update(0, true, true, 0.0);
        assert!(!g.update(1, true, false, 0.8));
        assert!(!g.update(0, true, false, 0.9));
        g.cancel();
        assert!(!g.update(0, true, false, 1.0));
        g.update(0, false, false, 1.1);
        g.update(0, true, true, 2.0);
        g.update(0, false, false, 2.1);
        assert!(!g.update(0, true, true, 2.4));
        g.cancel();
        assert!(!g.update(0, true, true, 2.5));
    }
}
