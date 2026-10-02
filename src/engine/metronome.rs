//! Original synthesized click: precomputed damped tones, no external samples.
pub struct Metronome {
    sounds: [Vec<f32>; 2],
    cursor: usize,
    last_beat: Option<u64>,
    accent: usize,
}
impl Metronome {
    pub fn new(sr: u32) -> Self {
        let length = (sr as f32 * 0.035) as usize;
        let sounds = std::array::from_fn(|accent| {
            (0..length)
                .map(|i| {
                    let t = i as f32 / sr as f32;
                    let frequency = if accent == 0 { 1100.0 } else { 1650.0 };
                    let attack = (t / 0.0005).min(1.0);
                    (std::f32::consts::TAU * frequency * t).sin()
                        * (-t * 180.0).exp()
                        * attack
                        * 0.45
                })
                .collect()
        });
        Self {
            sounds,
            cursor: length,
            last_beat: None,
            accent: 0,
        }
    }
    pub fn next(&mut self, active: bool, elapsed: u64, sr: u32, bpm: u32, volume: f32) -> f32 {
        if !active {
            self.last_beat = None;
            self.cursor = self.sounds[0].len();
            return 0.0;
        }
        let beat = (elapsed as u128 * bpm as u128 / (sr as u128 * 60)) as u64;
        if self.last_beat != Some(beat) {
            let boundary = elapsed == 0
                || beat != ((elapsed - 1) as u128 * bpm as u128 / (sr as u128 * 60)) as u64;
            if self.last_beat.is_some() || boundary {
                self.cursor = 0;
                self.accent = usize::from(beat % 4 == 0);
            }
            self.last_beat = Some(beat);
        }
        let value = self.sounds[self.accent]
            .get(self.cursor)
            .copied()
            .unwrap_or(0.0)
            * volume.clamp(0.0, 1.0);
        self.cursor = (self.cursor + 1).min(self.sounds[0].len());
        value
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn enabling_mid_beat_waits_for_grid_and_processing_does_not_allocate() {
        let mut click = Metronome::new(8000);
        let count = crate::test_alloc::count(|| {
            for frame in 2000..4000 {
                assert_eq!(click.next(true, frame, 8000, 120, 1.0), 0.0);
            }
            let mut peak = 0.0f32;
            for frame in 4000..4280 {
                peak = peak.max(click.next(true, frame, 8000, 120, 1.0).abs());
            }
            assert!(peak > 0.1);
            assert_eq!(click.next(false, 4280, 8000, 120, 1.0), 0.0);
        });
        assert_eq!(count, 0);
    }
}
