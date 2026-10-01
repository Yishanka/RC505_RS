//! Visual phase comes from the audio frame counter, never UI frame timing.
pub fn pulse(elapsed: u64, sample_rate: u32, bpm: usize) -> (usize, f32) {
    let denominator = sample_rate.max(1) as u128 * 60;
    let position = elapsed as u128 * bpm.clamp(30, 300) as u128;
    let beat = (position / denominator % 4) as usize;
    let phase = (position % denominator) as f32 / denominator as f32;
    (beat, (1.0 - phase * 4.0).max(0.0))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn follows_audio_beats_and_measure_boundaries() {
        assert_eq!(pulse(0, 48000, 120), (0, 1.0));
        assert_eq!(pulse(24000, 48000, 120), (1, 1.0));
        assert_eq!(pulse(96000, 48000, 120), (0, 1.0));
        assert_eq!(pulse(12000, 48000, 120), (0, 0.0));
        let a = pulse(6000, 48000, 90);
        let b = pulse(12000, 96000, 90);
        assert_eq!(a, b);
    }
}
