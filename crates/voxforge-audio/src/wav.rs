use std::io::Cursor;
use voxforge_core::error::{Result, VoxForgeError};

pub struct WavEncoder;

impl WavEncoder {
    pub fn encode_pcm16_to_wav(
        pcm_samples: &[i16],
        sample_rate: u32,
        channels: u16,
    ) -> Result<Vec<u8>> {
        Self::encode_pcm_to_wav_with_precision(pcm_samples, sample_rate, channels, 16)
    }

    pub fn encode_pcm_to_wav_with_precision(
        pcm_samples: &[i16],
        sample_rate: u32,
        channels: u16,
        bits_per_sample: u16,
    ) -> Result<Vec<u8>> {
        let mut cursor = Cursor::new(Vec::new());
        match bits_per_sample {
            24 => {
                let spec = hound::WavSpec {
                    channels,
                    sample_rate,
                    bits_per_sample: 24,
                    sample_format: hound::SampleFormat::Int,
                };
                let mut writer = hound::WavWriter::new(&mut cursor, spec).map_err(|e| {
                    VoxForgeError::AudioProcessing(format!(
                        "Failed to init 24-bit WAV writer: {}",
                        e
                    ))
                })?;
                for &sample in pcm_samples {
                    let s24 = (sample as i32) << 8;
                    writer.write_sample(s24).map_err(|e| {
                        VoxForgeError::AudioProcessing(format!(
                            "Failed to write 24-bit sample: {}",
                            e
                        ))
                    })?;
                }
                writer.finalize().map_err(|e| {
                    VoxForgeError::AudioProcessing(format!("Failed to finalize 24-bit WAV: {}", e))
                })?;
            }
            32 => {
                let spec = hound::WavSpec {
                    channels,
                    sample_rate,
                    bits_per_sample: 32,
                    sample_format: hound::SampleFormat::Float,
                };
                let mut writer = hound::WavWriter::new(&mut cursor, spec).map_err(|e| {
                    VoxForgeError::AudioProcessing(format!(
                        "Failed to init 32-bit float WAV writer: {}",
                        e
                    ))
                })?;
                for &sample in pcm_samples {
                    let f = (sample as f32) / 32768.0;
                    writer.write_sample(f).map_err(|e| {
                        VoxForgeError::AudioProcessing(format!(
                            "Failed to write 32-bit float sample: {}",
                            e
                        ))
                    })?;
                }
                writer.finalize().map_err(|e| {
                    VoxForgeError::AudioProcessing(format!(
                        "Failed to finalize 32-bit float WAV: {}",
                        e
                    ))
                })?;
            }
            _ => {
                let spec = hound::WavSpec {
                    channels,
                    sample_rate,
                    bits_per_sample: 16,
                    sample_format: hound::SampleFormat::Int,
                };
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

    /// Encode 32-bit float samples [-1.0, 1.0] into RIFF WAV at 16-bit int, 24-bit int, or 32-bit float.
    pub fn encode_f32_to_wav_with_precision(
        samples: &[f32],
        sample_rate: u32,
        channels: u16,
        bits_per_sample: u16,
    ) -> Result<Vec<u8>> {
        let mut cursor = Cursor::new(Vec::new());
        match bits_per_sample {
            24 => {
                let spec = hound::WavSpec {
                    channels,
                    sample_rate,
                    bits_per_sample: 24,
                    sample_format: hound::SampleFormat::Int,
                };
                let mut writer = hound::WavWriter::new(&mut cursor, spec).map_err(|e| {
                    VoxForgeError::AudioProcessing(format!(
                        "Failed to init 24-bit WAV writer: {}",
                        e
                    ))
                })?;
                for &sample in samples {
                    let finite = if sample.is_finite() {
                        sample.clamp(-1.0, 1.0)
                    } else {
                        0.0
                    };
                    let s24 = (finite * 8_388_607.0).round() as i32;
                    writer.write_sample(s24).map_err(|e| {
                        VoxForgeError::AudioProcessing(format!(
                            "Failed to write 24-bit sample: {}",
                            e
                        ))
                    })?;
                }
                writer.finalize().map_err(|e| {
                    VoxForgeError::AudioProcessing(format!("Failed to finalize 24-bit WAV: {}", e))
                })?;
            }
            32 => {
                let spec = hound::WavSpec {
                    channels,
                    sample_rate,
                    bits_per_sample: 32,
                    sample_format: hound::SampleFormat::Float,
                };
                let mut writer = hound::WavWriter::new(&mut cursor, spec).map_err(|e| {
                    VoxForgeError::AudioProcessing(format!(
                        "Failed to init 32-bit float WAV writer: {}",
                        e
                    ))
                })?;
                for &sample in samples {
                    let finite = if sample.is_finite() {
                        sample.clamp(-1.0, 1.0)
                    } else {
                        0.0
                    };
                    writer.write_sample(finite).map_err(|e| {
                        VoxForgeError::AudioProcessing(format!(
                            "Failed to write 32-bit float sample: {}",
                            e
                        ))
                    })?;
                }
                writer.finalize().map_err(|e| {
                    VoxForgeError::AudioProcessing(format!(
                        "Failed to finalize 32-bit float WAV: {}",
                        e
                    ))
                })?;
            }
            _ => {
                let spec = hound::WavSpec {
                    channels,
                    sample_rate,
                    bits_per_sample: 16,
                    sample_format: hound::SampleFormat::Int,
                };
                let mut writer = hound::WavWriter::new(&mut cursor, spec).map_err(|e| {
                    VoxForgeError::AudioProcessing(format!("Failed to init WAV writer: {}", e))
                })?;
                for &sample in samples {
                    let finite = if sample.is_finite() {
                        sample.clamp(-1.0, 1.0)
                    } else {
                        0.0
                    };
                    let s16 = (finite * 32767.0).round() as i16;
                    writer.write_sample(s16).map_err(|e| {
                        VoxForgeError::AudioProcessing(format!("Failed to write sample: {}", e))
                    })?;
                }
                writer.finalize().map_err(|e| {
                    VoxForgeError::AudioProcessing(format!("Failed to finalize WAV: {}", e))
                })?;
            }
        }
        Ok(cursor.into_inner())
    }

    /// Decode WAV payload into lossless normalized mono f32 samples [-1.0, 1.0] + sample rate.
    pub fn decode_wav_to_mono_f32(wav_bytes: &[u8]) -> Result<(Vec<f32>, u32)> {
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
        let samples_f32: Vec<f32> = match (spec.sample_format, spec.bits_per_sample) {
            (hound::SampleFormat::Int, 16) => reader
                .samples::<i16>()
                .map(|r| r.map(|s| s as f32 / 32768.0))
                .collect::<std::result::Result<Vec<f32>, _>>()
                .map_err(|e| {
                    VoxForgeError::AudioProcessing(format!("Failed to read 16-bit samples: {}", e))
                })?,
            (hound::SampleFormat::Int, 24) => reader
                .samples::<i32>()
                .map(|r| r.map(|s| (s as f32 / 8_388_608.0).clamp(-1.0, 1.0)))
                .collect::<std::result::Result<Vec<f32>, _>>()
                .map_err(|e| {
                    VoxForgeError::AudioProcessing(format!("Failed to read 24-bit samples: {}", e))
                })?,
            (hound::SampleFormat::Int, 32) => reader
                .samples::<i32>()
                .map(|r| r.map(|s| (s as f32 / 2_147_483_648.0).clamp(-1.0, 1.0)))
                .collect::<std::result::Result<Vec<f32>, _>>()
                .map_err(|e| {
                    VoxForgeError::AudioProcessing(format!("Failed to read 32-bit samples: {}", e))
                })?,
            (hound::SampleFormat::Float, 32) => reader
                .samples::<f32>()
                .map(|r| {
                    r.map(|s| {
                        if s.is_finite() {
                            s.clamp(-1.0, 1.0)
                        } else {
                            0.0
                        }
                    })
                })
                .collect::<std::result::Result<Vec<f32>, _>>()
                .map_err(|e| {
                    VoxForgeError::AudioProcessing(format!(
                        "Failed to read 32-bit float samples: {}",
                        e
                    ))
                })?,
            _ => reader
                .samples::<i16>()
                .map(|r| r.map(|s| s as f32 / 32768.0))
                .collect::<std::result::Result<Vec<f32>, _>>()
                .map_err(|e| {
                    VoxForgeError::AudioProcessing(format!("Failed to read samples: {}", e))
                })?,
        };
        if spec.channels <= 1 {
            return Ok((samples_f32, spec.sample_rate));
        }
        Ok((
            Self::downmix_f32_to_mono(&samples_f32, spec.channels as usize),
            spec.sample_rate,
        ))
    }

    /// Phase-cancellation-safe mono downmix for interleaved f32 frames.
    /// Picks the loudest single channel if naïve averaging loses >50% RMS amplitude.
    pub fn downmix_f32_to_mono(samples: &[f32], channels: usize) -> Vec<f32> {
        if channels <= 1 || samples.is_empty() {
            return samples.to_vec();
        }
        let n_frames = samples.len() / channels;
        if n_frames == 0 {
            return Vec::new();
        }
        let mut mixed = Vec::with_capacity(n_frames);
        let mut chan_energy = vec![0.0f64; channels];
        let mut mixed_energy = 0.0f64;

        for frame in samples.chunks_exact(channels) {
            let mut sum = 0.0f64;
            for (c, &s) in frame.iter().enumerate() {
                let v = if s.is_finite() { s as f64 } else { 0.0 };
                chan_energy[c] += v * v;
                sum += v;
            }
            let avg = sum / channels as f64;
            mixed_energy += avg * avg;
            mixed.push(avg as f32);
        }

        let (loudest_ch, &max_energy) = chan_energy
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or((0, &0.0));
        let mixed_rms = (mixed_energy / n_frames as f64).sqrt();
        let loudest_rms = (max_energy / n_frames as f64).sqrt();

        if loudest_rms > 1e-6 && mixed_rms < 0.5 * loudest_rms {
            return samples
                .chunks_exact(channels)
                .map(|frame| {
                    let s = frame[loudest_ch];
                    if s.is_finite() {
                        s
                    } else {
                        0.0
                    }
                })
                .collect();
        }
        mixed
    }

    /// Phase-cancellation-safe mono downmix for interleaved i16 frames.
    pub fn downmix_interleaved_to_mono(samples: &[i16], channels: usize) -> Vec<i16> {
        if channels <= 1 || samples.is_empty() {
            return samples.to_vec();
        }
        let n_frames = samples.len() / channels;
        if n_frames == 0 {
            return Vec::new();
        }
        let mut mixed = Vec::with_capacity(n_frames);
        let mut chan_energy = vec![0.0f64; channels];
        let mut mixed_energy = 0.0f64;

        for frame in samples.chunks_exact(channels) {
            let mut sum = 0.0f64;
            for (c, &s) in frame.iter().enumerate() {
                let v = s as f64;
                chan_energy[c] += v * v;
                sum += v;
            }
            let avg = sum / channels as f64;
            mixed_energy += avg * avg;
            mixed.push(avg.round().clamp(-32768.0, 32767.0) as i16);
        }

        let (loudest_ch, &max_energy) = chan_energy
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or((0, &0.0));
        let mixed_rms = (mixed_energy / n_frames as f64).sqrt();
        let loudest_rms = (max_energy / n_frames as f64).sqrt();

        if loudest_rms > 1.0 && mixed_rms < 0.5 * loudest_rms {
            return samples
                .chunks_exact(channels)
                .map(|frame| frame[loudest_ch])
                .collect();
        }
        mixed
    }

    /// Decode a WAV payload and guarantee clean mono output without phase cancellation.
    pub fn decode_wav_to_mono_pcm16(wav_bytes: &[u8]) -> Result<(Vec<i16>, u32)> {
        let (samples, sr, channels) = Self::decode_wav_to_pcm16(wav_bytes)?;
        if channels <= 1 {
            Ok((samples, sr))
        } else {
            Ok((
                Self::downmix_interleaved_to_mono(&samples, channels as usize),
                sr,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_16bit() {
        let original: Vec<i16> = vec![0, 1000, -2000, 30000, -32000];
        let bytes = WavEncoder::encode_pcm_to_wav_with_precision(&original, 24000, 1, 16).unwrap();
        let (decoded, sr, ch) = WavEncoder::decode_wav_to_pcm16(&bytes).unwrap();
        assert_eq!(sr, 24000);
        assert_eq!(ch, 1);
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_encode_decode_24bit() {
        let original: Vec<i16> = vec![0, 1000, -2000, 30000, -32000];
        let bytes = WavEncoder::encode_pcm_to_wav_with_precision(&original, 48000, 1, 24).unwrap();
        let (decoded, sr, ch) = WavEncoder::decode_wav_to_pcm16(&bytes).unwrap();
        assert_eq!(sr, 48000);
        assert_eq!(ch, 1);
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_encode_decode_32bit_float() {
        let original: Vec<i16> = vec![0, 1000, -2000, 30000, -32000];
        let bytes = WavEncoder::encode_pcm_to_wav_with_precision(&original, 24000, 1, 32).unwrap();
        let (decoded, sr, ch) = WavEncoder::decode_wav_to_pcm16(&bytes).unwrap();
        assert_eq!(sr, 24000);
        assert_eq!(ch, 1);
        assert_eq!(decoded.len(), original.len());
        for (a, b) in original.iter().zip(decoded.iter()) {
            assert!((a - b).abs() <= 1);
        }
    }

    #[test]
    fn test_decode_wav_to_mono_pcm16_stereo_downmix() {
        // Interleaved stereo samples: L=1000, R=3000 -> avg 2000; L=-4000, R=-2000 -> avg -3000
        let stereo_samples: Vec<i16> = vec![1000, 3000, -4000, -2000];
        let bytes =
            WavEncoder::encode_pcm_to_wav_with_precision(&stereo_samples, 24000, 2, 16).unwrap();
        let (mono, sr) = WavEncoder::decode_wav_to_mono_pcm16(&bytes).unwrap();
        assert_eq!(sr, 24000);
        assert_eq!(mono.len(), 2);
        assert_eq!(mono[0], 2000);
        assert_eq!(mono[1], -3000);
    }

    #[test]
    fn test_downmix_prevents_phase_cancellation() {
        let mut inverted = Vec::with_capacity(200);
        for i in 0..100 {
            let s = ((i as f32 * 0.25).sin() * 14000.0) as i16;
            inverted.push(s);
            inverted.push(-s);
        }
        let mono = WavEncoder::downmix_interleaved_to_mono(&inverted, 2);
        assert_eq!(mono.len(), 100);
        let peak = mono.iter().map(|s| s.abs()).max().unwrap_or(0);
        assert!(
            peak > 10000,
            "Out-of-phase stereo should preserve signal energy, got peak {peak}"
        );
    }

    #[test]
    fn test_f32_24bit_and_32bit_roundtrip() {
        let original: Vec<f32> = vec![0.0, 0.12345, -0.54321, 0.87654, -0.25];
        for depth in [24u16, 32u16] {
            let wav =
                WavEncoder::encode_f32_to_wav_with_precision(&original, 24000, 1, depth).unwrap();
            let (decoded, sr) = WavEncoder::decode_wav_to_mono_f32(&wav).unwrap();
            assert_eq!(sr, 24000);
            assert_eq!(decoded.len(), original.len());
            for (a, b) in original.iter().zip(decoded.iter()) {
                assert!((a - b).abs() < 1e-4, "depth {depth}: expected {a}, got {b}");
            }
        }
    }
}
