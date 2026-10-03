// src/config/beat_configs.rs

use std::time::Instant;

use crate::config::config_type::{ConfigSet, NumericConfig};

/// Config adjusted by tap
pub struct BeatTapCaculator {
    pub value: usize,
    pub tap_count: usize,
    pub last_tap_time: Option<Instant>,
    intervals_ns: [u64; 8],
    next_interval: usize,
}

impl BeatTapCaculator {
    pub fn new(initial_value: usize) -> Self {
        Self {
            value: initial_value,
            tap_count: 0,
            last_tap_time: None,
            intervals_ns: [0; 8],
            next_interval: 0,
        }
    }

    pub fn confirm(&self) -> usize {
        self.value
    }

    pub fn reset(&mut self, value: usize) {
        self.value = value.clamp(30, 300);
        self.tap_count = 0;
        self.last_tap_time = None;
        self.intervals_ns.fill(0);
        self.next_interval = 0;
    }
    fn tap_at(&mut self, now: Instant) {
        let Some(previous) = self.last_tap_time else {
            self.last_tap_time = Some(now);
            return;
        };
        if now <= previous {
            return;
        }
        let interval = now.duration_since(previous);
        if interval > std::time::Duration::from_secs(3) {
            self.reset(self.value);
        } else {
            self.intervals_ns[self.next_interval] =
                interval.as_nanos().clamp(200_000_000, 2_000_000_000) as u64;
            self.next_interval = (self.next_interval + 1) % self.intervals_ns.len();
            self.tap_count = (self.tap_count + 1).min(self.intervals_ns.len());
            let sum = self.intervals_ns.iter().sum::<u64>() as u128;
            self.value = ((60_000_000_000u128 * self.tap_count as u128 + sum / 2) / sum)
                .clamp(30, 300) as usize;
        }
        self.last_tap_time = Some(now);
    }
}

/// All beat settings
pub struct BeatConfigs {
    // bpm: usize,
    // latency: usize,
    pub sel_idx: Option<usize>,
    pub input_bpm: NumericConfig,
    pub input_latency: NumericConfig,
    pub tap_calc: BeatTapCaculator,
}

impl BeatConfigs {
    pub fn new(initial_bpm: usize, initial_latency: usize) -> Self {
        Self {
            // bpm: initial_bpm,
            // latency: initial_latency,
            input_bpm: NumericConfig::new("BPM", initial_bpm),
            input_latency: NumericConfig::new("Compensation (ms)", initial_latency),
            tap_calc: BeatTapCaculator::new(initial_bpm),
            sel_idx: Some(0),
        }
    }

    pub fn current_bpm(&self) -> usize {
        self.input_bpm.value
    }

    pub fn current_latency(&self) -> usize {
        self.input_latency.value
    }
    pub fn tap(&mut self) {
        self.tap_at(Instant::now());
    }
    pub fn tap_at(&mut self, now: Instant) {
        // A typed/dragged BPM invalidates the previous tap series. Its first
        // tap measures an origin and must not restore a stale calculator value.
        if self.tap_calc.value != self.input_bpm.value {
            self.tap_calc.reset(self.input_bpm.value);
        }
        self.tap_calc.tap_at(now);
        self.input_bpm.value = self.tap_calc.value;
        self.input_bpm.buffer = self.input_bpm.value.to_string();
    }
    pub fn accept_engine_bpm(&mut self, bpm: usize) {
        let bpm = bpm.clamp(30, 300);
        if self.input_bpm.value != bpm {
            self.input_bpm.value = bpm;
            self.input_bpm.buffer = bpm.to_string();
            self.tap_calc.reset(bpm);
        }
    }

    pub fn set_values(&mut self, bpm: usize, latency: usize) {
        // self.bpm = bpm;
        // self.latency = latency;
        self.input_bpm.value = bpm;
        self.input_bpm.buffer = bpm.to_string();
        self.tap_calc.reset(bpm);
        self.input_latency.value = latency;
        self.input_latency.buffer = latency.to_string();
    }

    pub fn set_latency(&mut self, latency: usize) {
        self.input_latency.value = latency;
        self.input_latency.buffer = latency.to_string();
    }
}

impl ConfigSet for BeatConfigs {
    fn next(&mut self) {
        if self.sel_idx.is_none() {
            self.sel_idx = Some(0);
        } else {
            self.sel_idx = Some((self.sel_idx.unwrap() + 1) % 2);
        }
    }

    fn prev(&mut self) {
        if self.sel_idx.is_none() {
            self.sel_idx = Some(0);
        } else {
            self.sel_idx = Some((self.sel_idx.unwrap() + 1) % 2);
        }
    }

    fn confirm(&mut self) {
        match self.sel_idx {
            Some(0) => {
                let fv_n = self.input_bpm.confirm();
                let fv_t = self.tap_calc.confirm();
                if fv_n != self.input_bpm.value {
                    self.input_bpm.value = fv_n;
                    self.tap_calc.reset(fv_n);
                    self.input_bpm.buffer = fv_n.to_string();
                } else {
                    self.input_bpm.value = fv_t;
                    self.input_bpm.value = fv_t;
                    self.input_bpm.buffer = fv_t.to_string();
                }
            }
            // Some(1) => {
            //     self.latency = self.input_latency.confirm();
            // }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tap_tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn first_tap_preserves_tempo_and_second_tap_measures_without_old_value_bias() {
        let mut beat = BeatConfigs::new(120, 0);
        let start = Instant::now();
        beat.tap_at(start);
        assert_eq!(beat.current_bpm(), 120);
        beat.tap_at(start + Duration::from_millis(250));
        assert_eq!(beat.current_bpm(), 240);
        beat.tap_at(start + Duration::from_millis(500));
        assert_eq!(beat.current_bpm(), 240);
        beat.tap_at(start + Duration::from_millis(500));
        assert_eq!(beat.tap_calc.tap_count, 2);
    }
    #[test]
    fn manual_changes_and_timeouts_begin_a_fresh_tap_series() {
        let mut beat = BeatConfigs::new(120, 17);
        let start = Instant::now();
        beat.tap_at(start);
        beat.tap_at(start + Duration::from_millis(250));
        beat.input_bpm.value = 77;
        beat.tap_at(start + Duration::from_millis(500));
        assert_eq!(beat.current_bpm(), 77);
        beat.tap_at(start + Duration::from_millis(1000));
        assert_eq!(beat.current_bpm(), 120);
        beat.tap_at(start + Duration::from_secs(5));
        assert_eq!(beat.current_bpm(), 120);
        beat.tap_at(start + Duration::from_millis(5800));
        assert_eq!(beat.current_bpm(), 75);
        assert_eq!(beat.current_latency(), 17);
    }
    #[test]
    fn rolling_interval_average_and_engine_readback_are_bounded() {
        let mut beat = BeatConfigs::new(99, 0);
        let mut time = Instant::now();
        beat.tap_at(time);
        for ms in [450, 550, 450, 550, 450, 550, 450, 550] {
            time += Duration::from_millis(ms);
            beat.tap_at(time);
        }
        assert_eq!(beat.current_bpm(), 120);
        assert_eq!(beat.tap_calc.tap_count, 8);
        beat.accept_engine_bpm(137);
        assert_eq!(beat.current_bpm(), 137);
        beat.tap_at(time + Duration::from_millis(500));
        assert_eq!(beat.current_bpm(), 137);
    }
}
