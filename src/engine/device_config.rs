//! Negotiate a format supported by both ends of the current shared-clock engine.
use anyhow::{Result, bail};
use cpal::{
    BufferSize, SampleFormat, SampleRate, StreamConfig, SupportedBufferSize,
    SupportedStreamConfigRange,
};

pub fn common_config(
    input: &[SupportedStreamConfigRange],
    output: &[SupportedStreamConfigRange],
    block: u32,
) -> Result<StreamConfig> {
    // The DSP and ring buffer currently use the same channel layout at both ends.
    for channels in [2, 1] {
        let mut candidates = Vec::new();
        for i in input
            .iter()
            .filter(|c| c.sample_format() == SampleFormat::F32 && c.channels() == channels)
        {
            for o in output
                .iter()
                .filter(|c| c.sample_format() == SampleFormat::F32 && c.channels() == channels)
            {
                let min = i.min_sample_rate().0.max(o.min_sample_rate().0).max(8_000);
                let max = i
                    .max_sample_rate()
                    .0
                    .min(o.max_sample_rate().0)
                    .min(192_000);
                if min > max {
                    continue;
                }
                let rate = if (min..=max).contains(&48000) {
                    48000
                } else if (min..=max).contains(&44100) {
                    44100
                } else {
                    48000_u32.clamp(min, max)
                };
                let supports_block = |size: &SupportedBufferSize| match size {
                    SupportedBufferSize::Range { min, max } => (*min..=*max).contains(&block),
                    SupportedBufferSize::Unknown => false,
                };
                let buffer_size =
                    if supports_block(i.buffer_size()) && supports_block(o.buffer_size()) {
                        BufferSize::Fixed(block)
                    } else {
                        BufferSize::Default
                    };
                candidates.push(StreamConfig {
                    channels,
                    sample_rate: SampleRate(rate),
                    buffer_size,
                });
            }
        }
        candidates.sort_by_key(|c| {
            if c.sample_rate.0 == 48000 {
                0
            } else if c.sample_rate.0 == 44100 {
                1
            } else {
                c.sample_rate.0.abs_diff(48000) + 2
            }
        });
        if let Some(config) = candidates.into_iter().next() {
            return Ok(config);
        }
    }
    bail!(
        "Input/output have no common mono/stereo f32 format. Choose devices with matching sample rates and channel layouts."
    )
}

/// Select each endpoint independently when their native rate/layout differs.
pub fn endpoint_config(
    ranges: &[SupportedStreamConfigRange],
    preferred: u32,
    block: u32,
    output: bool,
) -> Result<StreamConfig> {
    let mut candidates = Vec::new();
    for range in ranges.iter().filter(|v| {
        v.sample_format() == SampleFormat::F32
            && (1..=if output { 32 } else { 2 }).contains(&v.channels())
    }) {
        let min = range.min_sample_rate().0.max(8000);
        let max = range.max_sample_rate().0.min(192000);
        if min > max {
            continue;
        }
        let rate = preferred.clamp(min, max);
        let buffer_size = match range.buffer_size() {
            SupportedBufferSize::Range { min, max } if (*min..=*max).contains(&block) => {
                BufferSize::Fixed(block)
            }
            _ => BufferSize::Default,
        };
        candidates.push(StreamConfig {
            channels: range.channels(),
            sample_rate: SampleRate(rate),
            buffer_size,
        });
    }
    candidates.sort_by_key(|c| {
        (
            c.sample_rate.0.abs_diff(preferred),
            if c.channels == 2 {
                0
            } else {
                c.channels as u32
            },
        )
    });
    candidates
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("Device has no supported f32 audio format"))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn range(min: u32, max: u32, channels: u16) -> SupportedStreamConfigRange {
        SupportedStreamConfigRange::new(
            channels,
            SampleRate(min),
            SampleRate(max),
            SupportedBufferSize::Range {
                min: 128,
                max: 1024,
            },
            SampleFormat::F32,
        )
    }
    #[test]
    fn negotiates_common_rate_instead_of_input_maximum() {
        let config =
            common_config(&[range(44100, 192000, 2)], &[range(44100, 48000, 2)], 256).unwrap();
        assert_eq!(config.sample_rate.0, 48000);
        assert_eq!(config.channels, 2);
        assert_eq!(
            common_config(&[range(44100, 44100, 1)], &[range(44100, 48000, 1)], 256)
                .unwrap()
                .sample_rate
                .0,
            44100
        );
        assert!(common_config(&[range(96000, 192000, 2)], &[range(44100, 48000, 2)], 256).is_err());
    }
}
