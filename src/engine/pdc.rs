//! Algorithmic plugin delay compensation. This is independent of physical I/O
//! calibration. External performances following the audible click are recorded
//! at H + input_frames + output_frames; internally clocked sources use only
//! input_frames. Parallel monitoring and the click align to output_frames.
use super::{input_fx::InputFxRuntime, track_fx::TrackFxRuntime};
use crate::config::{track_options::InputRouting, vocoder_configs::VocoderCarrier};
pub type Frame = [f32; 2];

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct LatencyPlan {
    pub enabled: bool,
    pub input_frames: usize,
    pub track_frames: [usize; 5],
    pub output_frames: usize,
}
impl LatencyPlan {
    pub fn new(
        input: &InputFxRuntime,
        track: &TrackFxRuntime,
        sr: f32,
        routing: InputRouting,
        enabled: bool,
    ) -> Self {
        if !enabled {
            return Self::default();
        }
        let mut result = Self {
            enabled: true,
            ..Self::default()
        };
        let bank = &track.banks[track.selected_bank_idx.min(track.banks.len() - 1)];
        // Bypassed plugins keep their declared latency, so playing FX switches do
        // not change the recording clock or move parallel loops against each other.
        let track_latency = bank
            .slots
            .iter()
            .filter_map(|slot| slot.audio.as_ref())
            .map(|p| p.latency_frames(sr))
            .sum();
        result.track_frames.fill(track_latency);
        let bank = &input.banks[input.selected_bank_idx.min(input.banks.len() - 1)];
        let carrier_latency =
            |carrier: VocoderCarrier| carrier.track_idx().map_or(0, |i| result.track_frames[i]);
        if routing == InputRouting::Serial {
            for slot in &bank.slots {
                if let Some(v) = slot.vocoder {
                    result.input_frames = result.input_frames.max(carrier_latency(v.carrier));
                }
                if let Some(p) = &slot.audio {
                    result.input_frames += p.latency_frames(sr);
                }
            }
        } else {
            result.input_frames = bank
                .slots
                .iter()
                .filter_map(|s| s.vocoder)
                .map(|v| carrier_latency(v.carrier))
                .max()
                .unwrap_or(0);
            result.input_frames += bank
                .slots
                .iter()
                .filter_map(|s| s.audio.as_ref())
                .map(|p| p.latency_frames(sr))
                .sum::<usize>();
        }
        result.output_frames = result.input_frames.max(track_latency);
        result
    }
}

pub fn capacity(sr: f32) -> usize {
    // latency_frames=N-1, so this is exactly 8*N (N is an FFT power of two).
    let capacity = crate::dsp::pitch_shift::latency_frames(sr) * 8 + 8;
    debug_assert!(capacity.is_power_of_two());
    capacity
}

#[derive(Clone, Copy, Default)]
pub struct ClockPoint {
    pub stream: u64,
    pub elapsed: u64,
    pub local: u64,
    pub bpm: usize,
    pub active: bool,
}
#[derive(Clone)]
pub struct ClockHistory {
    ring: Vec<ClockPoint>,
    write: usize,
    valid: usize,
    stream: u64,
}
impl ClockHistory {
    pub fn new(sr: f32) -> Self {
        Self {
            ring: vec![ClockPoint::default(); capacity(sr)],
            write: 0,
            valid: 0,
            stream: 0,
        }
    }
    pub fn reset(&mut self) {
        self.write = 0;
        self.valid = 0;
        self.stream = 0;
    }
    pub fn push(&mut self, elapsed: f64, local: f64, sr: f32, bpm: usize, active: bool) {
        self.ring[self.write] = ClockPoint {
            stream: self.stream,
            elapsed: (elapsed.max(0.0) * sr as f64).round() as u64,
            local: (local.max(0.0) * sr as f64).round() as u64,
            bpm,
            active,
        };
        self.stream = self.stream.wrapping_add(1);
        self.write = (self.write + 1) & (self.ring.len() - 1);
        self.valid = (self.valid + 1).min(self.ring.len());
    }
    pub fn get(&self, delay: usize) -> Option<ClockPoint> {
        if delay >= self.valid {
            None
        } else {
            Some(self.ring[(self.write + self.ring.len() - 1 - delay) & (self.ring.len() - 1)])
        }
    }
}

/// Prepared variable delay with an O(1) logical clear and a five-ms transition
/// between old/new taps when the graph is structurally edited. First activation
/// sets the tap directly: no early dry impulse leaks through the initial fade.
#[derive(Clone)]
pub struct AlignDelay {
    ring: Vec<Frame>,
    write: usize,
    valid: usize,
    delay: usize,
    old_delay: usize,
    fade: usize,
    fade_frames: usize,
}
impl AlignDelay {
    pub fn new(sr: f32) -> Self {
        let fade_frames = (sr * 0.005).round().max(1.0) as usize;
        Self {
            ring: vec![[0.0; 2]; capacity(sr)],
            write: 0,
            valid: 0,
            delay: 0,
            old_delay: 0,
            fade: fade_frames,
            fade_frames,
        }
    }
    pub fn reset(&mut self) {
        self.valid = 0;
        self.write = 0;
        self.fade = self.fade_frames;
    }
    fn read(&self, delay: usize) -> Frame {
        if delay > self.valid {
            [0.0; 2]
        } else {
            self.ring[(self.write + self.ring.len() - delay) & (self.ring.len() - 1)]
        }
    }
    pub fn process(&mut self, input: Frame, delay: usize) -> Frame {
        let delay = delay.min(self.ring.len() - 1);
        if delay != self.delay {
            self.old_delay = self.delay;
            self.delay = delay;
            self.fade = if self.valid == 0 { self.fade_frames } else { 0 };
        }
        self.ring[self.write] = input;
        let mut output = self.read(self.delay);
        if self.fade < self.fade_frames {
            let old = self.read(self.old_delay);
            self.fade += 1;
            let mix = self.fade as f32 / self.fade_frames as f32;
            output = [
                old[0] + (output[0] - old[0]) * mix,
                old[1] + (output[1] - old[1]) * mix,
            ];
        }
        self.write = (self.write + 1) & (self.ring.len() - 1);
        self.valid = (self.valid + 1).min(self.ring.len() - 1);
        output
    }
}
pub struct Compensation {
    pub plan: LatencyPlan,
    pub tracks: [AlignDelay; 5],
    pub gains: [AlignDelay; 5],
    pub input: AlignDelay,
    pub input_gain: AlignDelay,
    pub click: AlignDelay,
}
impl Compensation {
    pub fn new(sr: f32) -> Self {
        Self {
            plan: LatencyPlan::default(),
            tracks: std::array::from_fn(|_| AlignDelay::new(sr)),
            gains: std::array::from_fn(|_| AlignDelay::new(sr)),
            input: AlignDelay::new(sr),
            input_gain: AlignDelay::new(sr),
            click: AlignDelay::new(sr),
        }
    }
    pub fn reset_track(&mut self, index: usize) {
        self.tracks[index].reset();
        self.gains[index].reset();
    }
    pub fn reset(&mut self) {
        for i in 0..5 {
            self.reset_track(i);
        }
        self.input.reset();
        self.input_gain.reset();
        self.click.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn graph_includes_cross_track_carrier_and_actual_serial_or_legacy_order() {
        use crate::config::{AppConfig, FxKind, InputFx, TrackFxKind, audio_fx::AudioFxKind};
        let mut c = AppConfig::new(120, 0, 5);
        for slot in 0..4 {
            c.track_fx
                .set_slot_kind(0, slot, TrackFxKind::Audio(AudioFxKind::Transpose));
        }
        c.input_fx
            .set_slot_kind(0, 0, FxKind::Audio(AudioFxKind::Transpose));
        c.input_fx.set_slot_kind(0, 1, FxKind::Vocoder);
        c.input_fx
            .set_slot_kind(0, 2, FxKind::Audio(AudioFxKind::Transpose));
        if let Some(InputFx::Vocoder(v)) = &mut c.input_fx.banks[0].slots[1].fx {
            v.carrier.value = VocoderCarrier::Track1;
        }
        let input = InputFxRuntime::from_config(&c.input_fx);
        let track = TrackFxRuntime::from_config(&c.track_fx);
        let window = crate::dsp::pitch_shift::latency_frames(48000.0);
        let serial = LatencyPlan::new(&input, &track, 48000.0, InputRouting::Serial, true);
        assert_eq!(serial.input_frames, window * 5);
        assert_eq!(serial.track_frames, [window * 4; 5]);
        let legacy = LatencyPlan::new(&input, &track, 48000.0, InputRouting::Legacy, true);
        assert_eq!(legacy.input_frames, window * 6);
    }
    #[test]
    fn exact_delay_impulse_and_initial_zero_latency_do_not_allocate() {
        for sr in [8000.0, 44100.0, 48000.0, 96000.0, 192000.0] {
            let delay = crate::dsp::pitch_shift::latency_frames(sr);
            let mut line = AlignDelay::new(sr);
            let n = crate::test_alloc::count(|| {
                for i in 0..delay * 2 {
                    let y = line.process(if i == 0 { [0.25, -0.5] } else { [0.0; 2] }, delay);
                    assert_eq!(y, if i == delay { [0.25, -0.5] } else { [0.0; 2] });
                }
            });
            assert_eq!(n, 0);
            line.reset();
            assert_eq!(line.process([0.2, 0.3], 0), [0.2, 0.3]);
        }
    }
}
