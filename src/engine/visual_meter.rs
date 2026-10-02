//! O(1) work per stereo frame; 64 ten-millisecond peak buckets, no FFT or locks.
pub const BARS: usize = 64;
pub struct VisualMeter {
    bars: [f32; BARS],
    index: usize,
    count: u32,
    bucket: u32,
    peak: f32,
}
impl VisualMeter {
    pub fn new(sr: u32) -> Self {
        Self {
            bars: [0.0; BARS],
            index: 0,
            count: 0,
            bucket: (sr / 100).max(1),
            peak: 0.0,
        }
    }
    pub fn push(&mut self, frame: [f32; 2]) {
        self.peak = self.peak.max(frame[0].abs()).max(frame[1].abs());
        self.count += 1;
        if self.count >= self.bucket {
            self.bars[self.index] = self.peak.min(1.0);
            self.index = (self.index + 1) % BARS;
            self.count = 0;
            self.peak = 0.0;
        }
    }
    pub fn snapshot(&self) -> [f32; BARS] {
        std::array::from_fn(|i| self.bars[(self.index + i) % BARS])
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stereo_peaks_order_and_silence_expire_without_allocations() {
        let mut meter = VisualMeter::new(8000);
        let count = crate::test_alloc::count(|| {
            for n in 0..80 {
                meter.push([
                    if n == 2 { 0.25 } else { 0.0 },
                    if n == 9 { -0.75 } else { 0.0 },
                ]);
            }
            assert_eq!(meter.snapshot()[BARS - 1], 0.75);
            for _ in 0..80 * BARS {
                meter.push([0.0; 2]);
            }
            assert_eq!(meter.snapshot(), [0.0; BARS]);
        });
        assert_eq!(count, 0);
    }
}
