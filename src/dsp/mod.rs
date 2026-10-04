pub mod audio_fx;
pub mod audio_modulation;
pub mod automation;
pub mod biquad;
pub mod delay;
pub mod detector;
pub mod distortion;
pub mod dynamics;
pub mod dynamics_profiles;
pub mod envelope;
pub mod filter;
pub mod filter_sweep;
pub mod master;
pub mod my_delay;
pub mod note;
pub mod oscillator;
pub mod pitch_shift;
pub mod pitch_tracker;
pub mod reverb;
pub mod roll;
pub mod stereo_enhance;
pub mod vocoder;

/// Internal floating-point buses keep 24 dB of headroom. Physical output clipping
/// is performed once by RenderCore, after faders and the master processors.
pub fn headroom(sample: f32) -> f32 {
    if sample.is_finite() {
        sample.clamp(-16.0, 16.0)
    } else {
        0.0
    }
}
