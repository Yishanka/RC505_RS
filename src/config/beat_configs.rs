// src/config/beat_configs.rs

use std::time::Instant;

use crate::config::config_type::{ConfigSet, NumericConfig};

/// Config adjusted by tap
pub struct BeatTapCaculator {
    pub value: usize,
    pub tap_count: usize,
    pub last_tap_time: Option<Instant>,
}

impl BeatTapCaculator {
    pub fn new(initial_value: usize) -> Self {
        Self {
            value: initial_value,
            tap_count: 0,
            last_tap_time: None,
        }
    }

    pub fn confirm(&self) -> usize {
        self.value
    }

    pub fn calculate_avg_bpm(&mut self) {
        let now = Instant::now();
        self.tap_count += 1;

        if let Some(last_time) = self.last_tap_time {
            let interval = now.duration_since(last_time).as_millis() as usize;
            if interval > 3000 {
                self.tap_count = 0;
            } else if interval > 0 {
                let new_bpm = (60000 / interval).clamp(30, 300);
                self.value = (self.value * (self.tap_count - 1) + new_bpm) / self.tap_count;
            }
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
            input_latency: NumericConfig::new("Latency Complement", initial_latency),
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

    pub fn set_values(&mut self, bpm: usize, latency: usize) {
        // self.bpm = bpm;
        // self.latency = latency;
        self.input_bpm.value = bpm;
        self.input_bpm.buffer = bpm.to_string();
        self.tap_calc.value = bpm;
        self.tap_calc.tap_count = 0;
        self.tap_calc.last_tap_time = None;
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
                    self.tap_calc.value = fv_n;
                    self.tap_calc.tap_count = 0;
                    self.tap_calc.last_tap_time = None;
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
