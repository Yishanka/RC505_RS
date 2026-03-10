use std::time::{Duration, Instant};

pub struct Metronome {
    bpm: usize,
    start_time: Option<Instant>,
}

impl Metronome {
    pub fn new(bpm: usize) -> Self {
        Self {
            bpm,
            start_time: None,
        }
    }

    pub fn adjust_bpm(&mut self, bpm: usize) {
        if self.start_time.is_none() {
            self.bpm = bpm
        };
    }

    pub fn current_bpm(&self) -> usize {
        self.bpm
    }

    pub fn start_time(&self) -> Option<Instant> {
        self.start_time
    }

    pub fn reset(&mut self) {
        self.start_time = None;
    }

    pub fn get_beat_time(&mut self) -> Instant {
        match self.start_time {
            None => {
                let now = Instant::now() + Duration::new(0, 1_000_000);
                self.start_time = Some(now);
                now
            }
            Some(start) => {
                let now = Instant::now();
                let elapsed = now.duration_since(start);
                let beat_duration = Duration::from_secs_f64(60.0 / self.bpm as f64);
                let beats_passed =
                    (elapsed.as_secs_f64() / beat_duration.as_secs_f64()).floor() as u64;
                start + beat_duration * (beats_passed as u32 + 1)
            }
        }
    }

    /// Return one beat duration under the `self.bpm`
    pub fn beat_duration(&self) -> Duration {
        Duration::from_secs_f64(60.0 / self.bpm as f64)
    }

    /// Return the beat count of the the current time
    pub fn beat_phase(&self, now: Instant) -> Option<f32> {
        self.start_time.map(|start| {
            let beat_secs = self.beat_duration().as_secs_f64();
            let elapsed = now.saturating_duration_since(start).as_secs_f64();
            (elapsed / beat_secs).fract() as f32
        })
    }
}
