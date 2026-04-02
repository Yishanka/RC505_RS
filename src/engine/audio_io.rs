use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::{Context, Result};
// use cpal::Host;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::HeapRb;
use crate::engine::input_fx::InputFxEngine;
use crate::engine::track_fx::TrackFxEngine;

// 128 在 48kHz 下约 2.6ms，在 96kHz 下约 1.3ms
// 如果报错，可以尝试 256
const BUFFER_SIZE: u32 = 256; 

#[cfg(all(target_os = "windows", feature = "asio"))]
fn device_exists_in_host(host: &cpal::Host, input_name: &str, output_name: &str) -> Result<bool> {
    let has_input = host
        .input_devices()?
        .any(|d| d.name().ok().as_deref() == Some(input_name));
    let has_output = host
        .output_devices()?
        .any(|d| d.name().ok().as_deref() == Some(output_name));
    Ok(has_input && has_output)
}

fn select_host(_input_name: &str, _output_name: &str) -> Result<cpal::Host> {
    // On Windows, prefer ASIO for lower-latency paths.
    // If ASIO host or requested devices are unavailable, fallback to default host.
    #[cfg(all(target_os = "windows", feature = "asio"))]
    {
        if let Ok(asio_host) = cpal::host_from_id(cpal::HostId::Asio) {
            if device_exists_in_host(&asio_host, _input_name, _output_name)? {
                return Ok(asio_host);
            }
            eprintln!("ASIO host is available but selected devices are not in ASIO. Falling back to default host.");
        }
    }

    Ok(cpal::default_host())
}

// Shift recorded audio earlier by `shift` samples and zero-pad the tail.
// This keeps track length unchanged while compensating input-capture latency.
fn shift_buffer_earlier_in_place(buffer: &mut [f32], shift: usize) {
    if buffer.is_empty() || shift == 0 {
        return;
    }
    let actual_shift = shift.min(buffer.len());
    buffer.copy_within(actual_shift.., 0);
    let tail_start = buffer.len() - actual_shift;
    for sample in &mut buffer[tail_start..] {
        *sample = 0.0;
    }
}

struct EngineTrack {
    buffer: Vec<f32>,
    play_cursor: usize,
    overdub_cursor: usize,
    record_start_at: Option<Instant>,
    record_stop_at: Option<Instant>,
    overdub_start_at: Option<Instant>,
    overdub_stop_at: Option<Instant>,
    recording: bool,
    // After scheduled record stop, keep capturing this many samples so we can
    // compensate input latency without truncating loop tail content.
    record_tail_remaining: usize,
    // Track length at scheduled stop time; used to keep loop duration unchanged
    // after extra tail capture for latency compensation.
    record_target_len: Option<usize>,
    overdubbing: bool,
    playing: bool,
}

impl EngineTrack {
    fn new() -> Self {
        Self {
            buffer: Vec::new(),
            play_cursor: 0,
            overdub_cursor: 0,
            record_start_at: None,
            record_stop_at: None,
            overdub_start_at: None,
            overdub_stop_at: None,
            recording: false,
            record_tail_remaining: 0,
            record_target_len: None,
            overdubbing: false,
            playing: false,
        }
    }
}

fn align_cursor_to_channels(cursor: usize, len: usize, channels: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let ch = channels.max(1).min(len);
    let aligned = cursor - (cursor % ch);
    aligned % len
}

fn finalize_recording_stop(track: &mut EngineTrack, latency_comp_samples: usize, channels: usize) {
    track.recording = false;
    shift_buffer_earlier_in_place(&mut track.buffer, latency_comp_samples);
    if let Some(target_len) = track.record_target_len {
        if track.buffer.len() > target_len {
            track.buffer.truncate(target_len);
        } else if track.buffer.len() < target_len {
            track.buffer.resize(target_len, 0.0);
        }
    }
    track.playing = !track.buffer.is_empty();
    // Recording stop is finalized after an extra tail-capture delay.
    // To keep loop phase aligned with the original scheduled stop beat,
    // start playback at the compensated phase instead of restarting at 0.
    track.play_cursor = if track.buffer.is_empty() {
        0
    } else {
        align_cursor_to_channels(latency_comp_samples, track.buffer.len(), channels)
    };
    track.overdub_cursor = track.play_cursor;
    track.record_tail_remaining = 0;
    track.record_target_len = None;
}

struct EngineState {
    tracks: Vec<EngineTrack>,
}

impl EngineState {
    fn new(track_count: usize) -> Self {
        Self {
            tracks: (0..track_count).map(|_| EngineTrack::new()).collect(),
        }
    }

    fn process_timeline(&mut self, now: Instant, latency_comp_samples: usize, channels: usize) {
        for track in &mut self.tracks {
            if let Some(start_at) = track.record_start_at {
                if now >= start_at {
                    track.buffer.clear();
                    track.play_cursor = 0;
                    track.overdub_cursor = 0;
                    track.recording = true;
                    track.record_tail_remaining = 0;
                    track.record_target_len = None;
                    track.overdubbing = false;
                    track.playing = false;
                    track.record_start_at = None;
                }
            }

            if let Some(stop_at) = track.record_stop_at {
                if now >= stop_at {
                    if track.recording {
                        // Keep recording for a short tail so latency compensation
                        // doesn't truncate the audible end of the loop.
                        track.record_target_len = Some(track.buffer.len());
                        track.record_tail_remaining = latency_comp_samples;
                        if track.record_tail_remaining == 0 {
                            finalize_recording_stop(track, latency_comp_samples, channels);
                        }
                    }
                    track.record_stop_at = None;
                }
            }

            if let Some(start_at) = track.overdub_start_at {
                if now >= start_at && !track.buffer.is_empty() {
                    track.overdubbing = true;
                    track.playing = true;
                    track.overdub_cursor = track.play_cursor;
                    track.overdub_start_at = None;
                }
            }

            if let Some(stop_at) = track.overdub_stop_at {
                if now >= stop_at {
                    track.overdubbing = false;
                    track.overdub_stop_at = None;
                }
            }
        }
    }
}

pub struct AudioIO {
    input_stream: cpal::Stream,
    output_stream: cpal::Stream,
    pub config: cpal::StreamConfig,
    input_name: String,
    output_name: String,
    state: Arc<Mutex<EngineState>>,
    latency_comp: usize, // Input latency compensation (in milliseconds).
    fx_engine: Arc<Mutex<InputFxEngine>>,
    track_fx_engine: Arc<Mutex<TrackFxEngine>>,
    realtime_enabled: Arc<AtomicBool>,
}

impl AudioIO {
    pub fn new(input_name: &str, output_name: &str, track_count: usize, latency_comp: usize) -> Result<Self> {
        let state = Arc::new(Mutex::new(EngineState::new(track_count)));
        let fx_engine = Arc::new(Mutex::new(InputFxEngine::new(48_000.0)));
        let track_fx_engine = Arc::new(Mutex::new(TrackFxEngine::new(48_000.0, track_count)));
        let realtime_enabled = Arc::new(AtomicBool::new(true));
        let (input_stream, output_stream, config) =
            Self::build_streams(
                input_name,
                output_name,
                Arc::clone(&state),
                Arc::clone(&fx_engine),
                Arc::clone(&track_fx_engine),
                Arc::clone(&realtime_enabled),
                latency_comp,
            )?;

        Ok(Self {
            input_stream,
            output_stream,
            config,
            input_name: input_name.to_string(),
            output_name: output_name.to_string(),
            state,
            latency_comp,
            fx_engine,
            track_fx_engine,
            realtime_enabled,
        })
    }

    fn build_streams(
        input_name: &str,
        output_name: &str,
        state: Arc<Mutex<EngineState>>,
        fx_engine: Arc<Mutex<InputFxEngine>>,
        track_fx_engine: Arc<Mutex<TrackFxEngine>>,
        realtime_enabled: Arc<AtomicBool>,
        latency_comp: usize, 
    ) -> Result<(cpal::Stream, cpal::Stream, cpal::StreamConfig)> {
        let host = select_host(input_name, output_name)?;

        let input_device = host
            .input_devices()?
            .find(|d| d.name().ok().as_deref() == Some(input_name))
            .context("Failed to find an input device (Microphone)")?;

        let output_device = host
            .output_devices()?
            .find(|d| d.name().ok().as_deref() == Some(output_name))
            .context("Failed to find an output device (Speaker)")?;
        
        let supported_config = input_device
            .supported_input_configs()?
            .filter(|c| c.channels() == 2) // 强制双声道，减少映射开销
            .next()
            .map(|range| range.with_max_sample_rate())
            .unwrap_or(input_device.default_input_config()?);
        let mut config: cpal::StreamConfig = supported_config.into();
        config.buffer_size = cpal::BufferSize::Fixed(BUFFER_SIZE);
        if let Ok(mut fx) = fx_engine.lock() {
            fx.set_sample_rate(config.sample_rate.0 as f32);
        }
        if let Ok(mut track_fx) = track_fx_engine.lock() {
            track_fx.set_sample_rate(config.sample_rate.0 as f32);
        }
        let latency_comp_samples =
            ((config.sample_rate.0 as f32 * latency_comp as f32 / 1000.0) as usize)
                * config.channels as usize;
        let frame_size = BUFFER_SIZE; 
        let rb_size = frame_size as usize * config.channels as usize * 4; // 留 4 倍冗余防止断音
        let rb = HeapRb::<f32>::new(rb_size);
        let (mut prod, mut cons) = rb.split();

        let err_fn = |err: cpal::StreamError| eprintln!("Audio stream error: {err}");

        let input_state = Arc::clone(&state);
        let input_fx = Arc::clone(&fx_engine);
        let input_rt_enabled = Arc::clone(&realtime_enabled);
        let input_stream = input_device.build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if !input_rt_enabled.load(Ordering::Relaxed) {
                    for _ in data {
                        let _ = prod.push(0.0);
                    }
                    return;
                }
                let now = Instant::now();
                let mut fx_guard = input_fx.lock().ok();
                let (base_elapsed, sample_rate) = if let Some(fx) = fx_guard.as_ref() {
                    let elapsed = fx
                        .metronome_start()
                        .map(|start| now.saturating_duration_since(start).as_secs_f64())
                        .unwrap_or(0.0);
                    (elapsed, fx.sample_rate())
                } else {
                    (0.0, config.sample_rate.0 as f32)
                };
                let channels = config.channels as usize;
                let sec_per_frame = 1.0 / sample_rate as f64;
                let mut processed_block: Vec<f32> = Vec::with_capacity(data.len());

                for (frame_idx, frame) in data.chunks(channels).enumerate() {
                    let input_l = frame[0];
                    let input_r = if channels > 1 { frame[1] } else { frame[0] };
                    let elapsed = base_elapsed + frame_idx as f64 * sec_per_frame;
                    let (processed_l, processed_r) = if let Some(fx) = fx_guard.as_mut() {
                        fx.process_frame(elapsed, input_l, input_r)
                    } else {
                        (input_l, input_r)
                    };
                    // Keep capturing input samples into ring buffer for overdub alignment.
                    // We do not monitor this directly to output.
                    for ch in 0..channels {
                        let sample = if ch == 0 { processed_l } else if ch == 1 { processed_r } else { processed_l };
                        let _ = prod.push(sample);
                        processed_block.push(sample);
                    }
                }

                if let Some(engine) = input_state.lock().ok().as_mut() {
                    for track in &mut engine.tracks {
                        if track.recording {
                            track.buffer.extend_from_slice(&processed_block);
                            if track.record_tail_remaining > 0 {
                                let consumed = processed_block.len().min(track.record_tail_remaining);
                                track.record_tail_remaining -= consumed;
                                if track.record_tail_remaining == 0 {
                                    finalize_recording_stop(track, latency_comp_samples, channels);
                                }
                            }
                        }
                        // NOTE:
                        // Overdub writing and timeline scheduling are handled in output callback
                        // so all playback-phase decisions share one clock domain.
                    }
                }
            },
            err_fn,
            None,
        )?;

        let output_state = Arc::clone(&state);
        let output_track_fx = Arc::clone(&track_fx_engine);
        let output_rt_enabled = Arc::clone(&realtime_enabled);
        let output_stream = output_device.build_output_stream(
            &config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                if !output_rt_enabled.load(Ordering::Relaxed) {
                    for out in data.iter_mut() {
                        *out = 0.0;
                    }
                    return;
                }
                let now: Instant = Instant::now();
                let mut guard: Option<std::sync::MutexGuard<'_, EngineState>> = output_state.lock().ok();
                let channels = config.channels as usize;
                if let Some(engine) = guard.as_mut() {
                    engine.process_timeline(now, latency_comp_samples, channels);
                }
                let mut track_fx_guard = output_track_fx.lock().ok();
                let (base_elapsed, sample_rate) = if let Some(track_fx) = track_fx_guard.as_ref() {
                    let elapsed = track_fx
                        .metronome_start()
                        .map(|start| now.saturating_duration_since(start).as_secs_f64())
                        .unwrap_or(0.0);
                    (elapsed, track_fx.sample_rate())
                } else {
                    (0.0, config.sample_rate.0 as f32)
                };
                let sec_per_frame = 1.0 / sample_rate.max(1.0) as f64;

                for (frame_idx, frame) in data.chunks_mut(channels).enumerate() {
                    let input_l = cons.pop().unwrap_or(0.0);
                    let input_r = if channels > 1 {
                        cons.pop().unwrap_or(input_l)
                    } else {
                        input_l
                    };
                    for _ in 2..channels {
                        let _ = cons.pop();
                    }

                    let elapsed = base_elapsed + frame_idx as f64 * sec_per_frame;
                    let mut mixed_l = 0.0f32;
                    let mut mixed_r = 0.0f32;

                    if let Some(engine) = guard.as_mut() {
                        for (track_idx, track) in engine.tracks.iter_mut().enumerate() {
                            if !track.playing || track.buffer.is_empty() {
                                continue;
                            }

                            let len = track.buffer.len();
                            let idx_l = track.play_cursor % len;
                            let idx_r = if channels > 1 {
                                (idx_l + 1) % len
                            } else {
                                idx_l
                            };

                            if track.overdubbing {
                                let comp = latency_comp_samples % len;
                                let write_l = (idx_l + len - comp) % len;
                                track.overdub_cursor = write_l;
                                track.buffer[write_l] = (track.buffer[write_l] + input_l).clamp(-1.0, 1.0);
                                if channels > 1 {
                                    let write_r = (write_l + 1) % len;
                                    track.buffer[write_r] = (track.buffer[write_r] + input_r).clamp(-1.0, 1.0);
                                }
                            }

                            let dry_l = track.buffer[idx_l];
                            let dry_r = if channels > 1 { track.buffer[idx_r] } else { dry_l };
                            let (wet_l, wet_r) = if let Some(track_fx) = track_fx_guard.as_mut() {
                                track_fx.process_frame(
                                    track_idx,
                                    elapsed,
                                    dry_l,
                                    dry_r,
                                    &track.buffer,
                                    idx_l,
                                    channels,
                                )
                            } else {
                                (dry_l, dry_r)
                            };

                            mixed_l += wet_l;
                            mixed_r += wet_r;

                            track.play_cursor = (track.play_cursor + channels) % len;
                        }
                    }

                    let out_l = (mixed_l + input_l).clamp(-1.0, 1.0);
                    let out_r = (mixed_r + input_r).clamp(-1.0, 1.0);
                    frame[0] = out_l;
                    if channels > 1 {
                        frame[1] = out_r;
                    }
                    for ch in 2..channels {
                        frame[ch] = ((out_l + out_r) * 0.5).clamp(-1.0, 1.0);
                    }
                }
            },
            err_fn,
            None,
        )?;

        input_stream.play()?;
        output_stream.play()?;

        Ok((input_stream, output_stream, config))
    }

    pub fn switch_devices(&mut self, input_name: &str, output_name: &str) -> Result<()> {
        if input_name == self.input_name && output_name == self.output_name {
            return Ok(());
        }

        let (input_stream, output_stream, config) =
            Self::build_streams(
                input_name,
                output_name,
                Arc::clone(&self.state),
                Arc::clone(&self.fx_engine),
                Arc::clone(&self.track_fx_engine),
                Arc::clone(&self.realtime_enabled),
                self.latency_comp,
            )?;

        self.input_stream.pause()?;
        self.output_stream.pause()?;

        self.input_stream = input_stream;
        self.output_stream = output_stream;
        self.config = config;
        self.input_name = input_name.to_string();
        self.output_name = output_name.to_string();
        Ok(())
    }

    pub fn adjust_latency_comp(&mut self, latency_comp: usize) -> Result<()> {
        if self.latency_comp == latency_comp {
            return Ok(());
        }

        let (input_stream, output_stream, config) = Self::build_streams(
            &self.input_name,
            &self.output_name,
            Arc::clone(&self.state),
            Arc::clone(&self.fx_engine),
            Arc::clone(&self.track_fx_engine),
            Arc::clone(&self.realtime_enabled),
            latency_comp,
        )?;

        self.input_stream.pause()?;
        self.output_stream.pause()?;

        self.input_stream = input_stream;
        self.output_stream = output_stream;
        self.config = config;
        self.latency_comp = latency_comp;
        Ok(())
    }

    pub fn curr_input_name(&self) -> &str {
        &self.input_name
    }

    pub fn curr_output_name(&self) -> &str {
        &self.output_name
    }

    pub fn update_input_fx(&self, config: &crate::config::InputFxConfig) {
        if let Ok(mut fx) = self.fx_engine.lock() {
            fx.update_from_config(config);
        }
    }

    pub fn update_track_fx(&self, config: &crate::config::TrackFxConfig) {
        if let Ok(mut fx) = self.track_fx_engine.lock() {
            fx.update_from_config(config);
        }
    }

    pub fn update_metronome(&self, start_time: Option<Instant>, bpm: usize) {
        if let Ok(mut fx) = self.fx_engine.lock() {
            fx.update_metronome(start_time, bpm);
        }
        if let Ok(mut fx) = self.track_fx_engine.lock() {
            fx.update_metronome(start_time, bpm);
        }
    }

    pub fn set_realtime_enabled(&self, enabled: bool) {
        self.realtime_enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn record_at(&self, track_id: usize, at: Instant) {
        if let Ok(mut engine) = self.state.lock() {
            if let Some(track) = engine.tracks.get_mut(track_id) {
                track.record_start_at = Some(at);
                track.record_stop_at = None;
            }
        }
    }

    pub fn stop_record_play_at(&self, track_id: usize, at: Instant) {
        if let Ok(mut engine) = self.state.lock() {
            if let Some(track) = engine.tracks.get_mut(track_id) {
                if track.recording || track.record_start_at.is_some() {
                    track.record_stop_at = Some(at);
                }
            }
        }
    }

    pub fn play_now(&self, track_id: usize) {
        if let Ok(mut engine) = self.state.lock() {
            if let Some(track) = engine.tracks.get_mut(track_id) {
                if !track.buffer.is_empty() {
                    track.playing = true;
                }
            }
        }
    }

    pub fn play_at_progress_now(&self, track_id: usize, progress: Option<f32>) {
        if let Ok(mut engine) = self.state.lock() {
            if let Some(track) = engine.tracks.get_mut(track_id) {
                if !track.buffer.is_empty() {
                    let channels = self.config.channels as usize;
                    if let Some(p) = progress {
                        let len = track.buffer.len();
                        let normalized = p.rem_euclid(1.0);
                        let cursor = ((normalized * len as f32).floor() as usize).min(len - 1);
                        let aligned = align_cursor_to_channels(cursor, len, channels);
                        track.play_cursor = aligned;
                        track.overdub_cursor = aligned;
                    } else {
                        track.play_cursor = 0;
                        track.overdub_cursor = 0;
                    }
                    track.playing = true;
                }
            }
        }
    }

    pub fn sync_playhead_if_drift(&self, track_id: usize, progress: f32, drift_ratio: f32) {
        if let Ok(mut engine) = self.state.lock() {
            if let Some(track) = engine.tracks.get_mut(track_id) {
                if !track.playing || track.buffer.is_empty() {
                    return;
                }
                let len = track.buffer.len();
                let channels = self.config.channels as usize;
                let normalized = progress.rem_euclid(1.0);
                let target = ((normalized * len as f32).floor() as usize).min(len - 1);
                let target = align_cursor_to_channels(target, len, channels);
                let curr = track.play_cursor;
                let direct = curr.abs_diff(target);
                let cyclic = direct.min(len - direct);
                let max_drift = ((len as f32) * drift_ratio.clamp(0.0, 0.5)).round() as usize;
                if cyclic > max_drift {
                    track.play_cursor = target;
                    track.overdub_cursor = target;
                }
            }
        }
    }

    pub fn pause_now(&self, track_id: usize) {
        if let Ok(mut engine) = self.state.lock() {
            if let Some(track) = engine.tracks.get_mut(track_id) {
                track.playing = false;
                track.recording = false;
                track.overdubbing = false;
            }
        }
    }

    pub fn clear_all_tracks_now(&self) {
        if let Ok(mut engine) = self.state.lock() {
            for track in &mut engine.tracks {
                *track = EngineTrack::new();
            }
        }
    }

    pub fn overdub_at(&self, track_id: usize, at: Instant) {
        if let Ok(mut engine) = self.state.lock() {
            if let Some(track) = engine.tracks.get_mut(track_id) {
                track.overdub_start_at = Some(at);
                track.overdub_stop_at = None;
            }
        }
    }

    pub fn stop_overdub_at(&self, track_id: usize, at: Instant) {
        if let Ok(mut engine) = self.state.lock() {
            if let Some(track) = engine.tracks.get_mut(track_id) {
                // If overdub is scheduled but not started yet, and stop is requested
                // no later than the scheduled start, cancel pending overdub entirely.
                if !track.overdubbing {
                    if let Some(start_at) = track.overdub_start_at {
                        if at <= start_at {
                            track.overdub_start_at = None;
                            track.overdub_stop_at = None;
                            return;
                        }
                    }
                }
                track.overdub_stop_at = Some(at);
            }
        }
    }
}
