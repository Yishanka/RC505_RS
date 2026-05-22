use crate::config::filter_configs::FilterType;
use crate::dsp::filter::{FilterDspState, FilterParams, process_sample as process_filter_sample};

pub const VOCODER_MAX_BANDS: usize = 16;
// Bandpass voice envelopes are much smaller than full-scale input; lift them
// before they drive the carrier bands.
const MODULATOR_ENV_GAIN: f32 = 24.0;

#[derive(Clone, Copy)]
pub struct VocoderParams {
    pub bands: usize,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub level: f32,
    pub mix: f32,
    pub sample_rate: f32,
    pub track_carrier_l: f32,
    pub track_carrier_r: f32,
    pub has_track_carrier: bool,
}

#[derive(Clone, Copy)]
pub struct VocoderDspState {
    mod_filters: [FilterDspState; VOCODER_MAX_BANDS],
    carrier_filters_l: [FilterDspState; VOCODER_MAX_BANDS],
    carrier_filters_r: [FilterDspState; VOCODER_MAX_BANDS],
    env: [f32; VOCODER_MAX_BANDS],
}

impl VocoderDspState {
    pub fn new() -> Self {
        Self {
            mod_filters: std::array::from_fn(|_| FilterDspState::new()),
            carrier_filters_l: std::array::from_fn(|_| FilterDspState::new()),
            carrier_filters_r: std::array::from_fn(|_| FilterDspState::new()),
            env: [0.0; VOCODER_MAX_BANDS],
        }
    }
}

pub fn process_frame(
    state: &mut VocoderDspState,
    params: VocoderParams,
    input_l: f32,
    input_r: f32,
) -> (f32, f32) {
    let sr = params.sample_rate.max(1.0);
    let band_count = params.bands.clamp(4, VOCODER_MAX_BANDS);
    let modulator = (input_l + input_r) * 0.5;
    let (carrier_l, carrier_r) = if params.has_track_carrier {
        (params.track_carrier_l, params.track_carrier_r)
    } else {
        (0.0, 0.0)
    };

    let attack = coeff(params.attack_ms.max(1.0), sr);
    let release = coeff(params.release_ms.max(1.0), sr);
    let mut wet_l = 0.0f32;
    let mut wet_r = 0.0f32;
    let q = 2.4;

    for idx in 0..band_count {
        let center_hz = band_center_hz(idx, band_count);
        let filter_params = FilterParams {
            filter_type: FilterType::Bpf,
            cutoff_hz: center_hz,
            q,
            drive: 0.0,
            mix: 1.0,
        };
        let mod_band =
            process_filter_sample(&mut state.mod_filters[idx], filter_params, sr, modulator);
        let target = (mod_band.abs() * MODULATOR_ENV_GAIN).min(1.0);
        let env_coeff = if target > state.env[idx] {
            attack
        } else {
            release
        };
        state.env[idx] += (target - state.env[idx]) * env_coeff;

        let car_l = process_filter_sample(
            &mut state.carrier_filters_l[idx],
            filter_params,
            sr,
            carrier_l,
        );
        let car_r = process_filter_sample(
            &mut state.carrier_filters_r[idx],
            filter_params,
            sr,
            carrier_r,
        );
        wet_l += car_l * state.env[idx];
        wet_r += car_r * state.env[idx];
    }

    let norm = (band_count as f32).sqrt().max(1.0);
    let wet_l = (wet_l / norm * params.level).clamp(-1.0, 1.0);
    let wet_r = (wet_r / norm * params.level).clamp(-1.0, 1.0);
    let mix = params.mix.clamp(0.0, 1.0);
    (
        (input_l * (1.0 - mix) + wet_l * mix).clamp(-1.0, 1.0),
        (input_r * (1.0 - mix) + wet_r * mix).clamp(-1.0, 1.0),
    )
}

fn coeff(ms: f32, sample_rate: f32) -> f32 {
    1.0 - (-1.0 / (ms * 0.001 * sample_rate.max(1.0))).exp()
}

fn band_center_hz(idx: usize, bands: usize) -> f32 {
    let low = 120.0_f32;
    let high = 5200.0_f32;
    if bands <= 1 {
        return low;
    }
    let t = idx as f32 / (bands - 1) as f32;
    low * (high / low).powf(t)
}
