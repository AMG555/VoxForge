use std::io::Cursor;
use voxforge_core::error::{Result, VoxForgeError};

pub struct WavEncoder;

impl WavEncoder {
    pub fn encode_pcm16_to_wav(
        pcm_samples: &[i16],
        sample_rate: u32,
        channels: u16,
    ) -> Result<Vec<u8>> {
        let spec = hound::WavSpec {
            channels,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = hound::WavWriter::new(&mut cursor, spec).map_err(|e| {
                VoxForgeError::AudioProcessing(format!("Failed to init WAV writer: {}", e))
            })?;

            for &sample in pcm_samples {
                writer.write_sample(sample).map_err(|e| {
                    VoxForgeError::AudioProcessing(format!("Failed to write sample: {}", e))
                })?;
            }

            writer.finalize().map_err(|e| {
                VoxForgeError::AudioProcessing(format!("Failed to finalize WAV: {}", e))
            })?;
        }

        Ok(cursor.into_inner())
    }

    pub fn decode_wav_to_pcm16(wav_bytes: &[u8]) -> Result<(Vec<i16>, u32, u16)> {
        let cursor = Cursor::new(wav_bytes);
        let mut reader = hound::WavReader::new(cursor).map_err(|e| {
            VoxForgeError::AudioProcessing(format!("Failed to parse WAV header: {}", e))
        })?;

        let spec = reader.spec();
        if reader.duration() > 50_000_000 {
            return Err(VoxForgeError::AudioProcessing(
                "WAV sample count exceeds maximum supported limit (50M samples)".to_string(),
            ));
        }

        let pcm_samples = match (spec.sample_format, spec.bits_per_sample) {
            (hound::SampleFormat::Int, 16) => reader
                .samples::<i16>()
                .collect::<std::result::Result<Vec<i16>, _>>()
                .map_err(|e| {
                    VoxForgeError::AudioProcessing(format!("Failed to read 16-bit samples: {}", e))
                })?,
            (hound::SampleFormat::Int, 24) | (hound::SampleFormat::Int, 32) => {
                let shift = spec.bits_per_sample - 16;
                reader
                    .samples::<i32>()
                    .map(|res| res.map(|s| (s >> shift) as i16))
                    .collect::<std::result::Result<Vec<i16>, _>>()
                    .map_err(|e| {
                        VoxForgeError::AudioProcessing(format!(
                            "Failed to read {}-bit int samples: {}",
                            spec.bits_per_sample, e
                        ))
                    })?
            }
            (hound::SampleFormat::Float, 32) => reader
                .samples::<f32>()
                .map(|res| res.map(|s| (s.clamp(-1.0, 1.0) * 32767.0).round() as i16))
                .collect::<std::result::Result<Vec<i16>, _>>()
                .map_err(|e| {
                    VoxForgeError::AudioProcessing(format!(
                        "Failed to read 32-bit float samples: {}",
                        e
                    ))
                })?,
            _ => reader
                .samples::<i16>()
                .collect::<std::result::Result<Vec<i16>, _>>()
                .map_err(|e| {
                    VoxForgeError::AudioProcessing(format!("Failed to read samples: {}", e))
                })?,
        };

        Ok((pcm_samples, spec.sample_rate, spec.channels))
    }
}
