//! One integer sample clock for transport, quantization, recording and replay.
#[derive(Clone, Copy, Default)]
pub struct SampleClock {
    pub frame: u64,
    pub origin: Option<u64>,
}
impl SampleClock {
    pub fn start(&mut self) {
        self.origin.get_or_insert(self.frame);
    }
    pub fn elapsed(&self) -> u64 {
        self.origin.map(|v| self.frame - v).unwrap_or(0)
    }
    pub fn next_grid(&self, sample_rate: u32, bpm: u32, beats: u64) -> u64 {
        let Some(origin) = self.origin else {
            return self.frame;
        };
        // Calculate each boundary from the origin; never accumulate rounded beat lengths.
        let numerator = sample_rate as u128 * 60 * beats.max(1) as u128;
        let elapsed = (self.frame - origin) as u128;
        let beat = (elapsed * bpm as u128).div_ceil(numerator);
        origin + (beat * numerator).div_ceil(bpm.max(1) as u128) as u64
    }
    pub fn beats_length(sample_rate: u32, bpm: u32, beats: u64) -> u64 {
        (sample_rate as u64 * 60 * beats).div_ceil(bpm.max(1) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quantization_has_no_cumulative_rounding_drift() {
        for sr in [44_100, 48_000, 96_000] {
            for bpm in [73, 117, 129, 299] {
                let mut clock = SampleClock {
                    frame: 1,
                    origin: Some(0),
                };
                for beat in 1..10_000u128 {
                    let at = clock.next_grid(sr, bpm, 1);
                    assert_eq!(at, (beat * sr as u128 * 60).div_ceil(bpm as u128) as u64);
                    clock.frame = at + 1;
                }
            }
        }
    }
}
