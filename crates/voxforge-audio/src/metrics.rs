use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioQualityMetrics {
    pub duration_seconds: f64,
    pub sample_rate: u32,
    pub channels: u16,
    pub total_samples: usize,
    pub peak_amplitude: i16,
    pub peak_dbfs: f32,
    pub rms_amplitude: f32,
    pub rms_dbfs: f32,
    pub clipping_samples_count: usize,
    pub is_silent: bool,
    pub snr_estimate_db: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SignalAnomalyKind {
    Clipping,
    LongSilence,
    RelativeVolumeJump,
    DcOffset,
    EmptyAudio,
    InvalidSample,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AnomalySeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SignalAnomaly {
    pub kind: SignalAnomalyKind,
    pub severity: AnomalySeverity,
    pub timestamp_seconds: f64,
    pub duration_seconds: Option<f64>,
    pub value: f32,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioSignalReport {
    pub metrics: AudioQualityMetrics,
    pub is_clean: bool,
    pub warnings_count: usize,
    pub anomalies: Vec<SignalAnomaly>,
    pub scan_bounded: bool,
}

pub struct AudioAnalyzer;

impl AudioAnalyzer {
    pub fn analyze_pcm16(samples: &[i16], sample_rate: u32, channels: u16) -> AudioQualityMetrics {
        if samples.is_empty() {
            return AudioQualityMetrics {
                duration_seconds: 0.0,
                sample_rate,
                channels,
                total_samples: 0,
                peak_amplitude: 0,
                peak_dbfs: -96.0,
                rms_amplitude: 0.0,
                rms_dbfs: -96.0,
                clipping_samples_count: 0,
                is_silent: true,
                snr_estimate_db: 0.0,
            };
        }

        let total_samples = samples.len();
        let safe_sample_rate = sample_rate.max(1);
        let safe_channels = channels.max(1);
        let duration_seconds =
            total_samples as f64 / (safe_sample_rate as f64 * safe_channels as f64);

        let mut max_abs: i16 = 0;
        let mut sum_squares: f64 = 0.0;
        let mut clipping_count = 0;

        for &sample in samples {
            let abs_val = sample.saturating_abs();
            if abs_val > max_abs {
                max_abs = abs_val;
            }
            if sample == i16::MAX || sample == i16::MIN {
                clipping_count += 1;
            }
            let norm = sample as f64 / 32768.0;
            sum_squares += norm * norm;
        }

        let rms_norm = (sum_squares / total_samples as f64).sqrt() as f32;
        let peak_norm = max_abs as f32 / 32768.0;

        let rms_dbfs = if rms_norm > 1e-5 {
            20.0 * rms_norm.log10()
        } else {
            -96.0
        };

        let peak_dbfs = if peak_norm > 1e-5 {
            20.0 * peak_norm.log10()
        } else {
            -96.0
        };

        // Estimate SNR across 20ms frames
        let frame_size = (safe_sample_rate / 50).max(1) as usize;
        let mut speech_energy = 0.0f64;
        let mut speech_count = 0usize;
        let mut noise_energy = 0.0f64;
        let mut noise_count = 0usize;

        for chunk in samples.chunks(frame_size) {
            let chunk_sum_sq: f64 = chunk
                .iter()
                .map(|&s| {
                    let n = s as f64 / 32768.0;
                    n * n
                })
                .sum();
            let chunk_rms = (chunk_sum_sq / chunk.len() as f64).sqrt();
            if chunk_rms > 0.02 {
                speech_energy += chunk_rms;
                speech_count += 1;
            } else if chunk_rms > 1e-5 {
                noise_energy += chunk_rms;
                noise_count += 1;
            }
        }

        let snr_estimate_db = if speech_count > 0 && noise_count > 0 {
            let avg_speech = speech_energy / speech_count as f64;
            let avg_noise = noise_energy / noise_count as f64;
            (20.0 * (avg_speech / avg_noise).log10()).clamp(0.0, 60.0) as f32
        } else if speech_count > 0 {
            35.0
        } else {
            0.0
        };

        AudioQualityMetrics {
            duration_seconds,
            sample_rate,
            channels,
            total_samples,
            peak_amplitude: max_abs,
            peak_dbfs,
            rms_amplitude: rms_norm,
            rms_dbfs,
            clipping_samples_count: clipping_count,
            is_silent: max_abs < 10,
            snr_estimate_db,
        }
    }

    /// Performs bounded diagnostic signal checks across finished audio:
    /// - Checks peak clipping runs
    /// - Detects extended digital silence holes (>850ms)
    /// - Identifies DC offset drift
    /// - Flags sharp dynamic loudness jumps (>22 dB)
    pub fn check_signal_anomalies(
        samples: &[i16],
        sample_rate: u32,
        channels: u16,
    ) -> AudioSignalReport {
        let metrics = Self::analyze_pcm16(samples, sample_rate, channels);
        let mut anomalies = Vec::new();
        let max_anomalies = 100;
        let mut scan_bounded = false;

        if samples.is_empty() || metrics.is_silent {
            anomalies.push(SignalAnomaly {
                kind: SignalAnomalyKind::EmptyAudio,
                severity: AnomalySeverity::Critical,
                timestamp_seconds: 0.0,
                duration_seconds: Some(0.0),
                value: 0.0,
                message: "Audio stream is empty or entirely silent".to_string(),
            });
            return AudioSignalReport {
                metrics,
                is_clean: false,
                warnings_count: anomalies.len(),
                anomalies,
                scan_bounded: false,
            };
        }

        let sr = sample_rate.max(1) as f64;
        let ch = channels.max(1) as usize;

        // 1. DC Offset check
        let sum_samples: i64 = samples.iter().map(|&s| s as i64).sum();
        let mean_offset = sum_samples as f64 / samples.len() as f64;
        let dc_offset_norm = (mean_offset / 32768.0).abs() as f32;
        if dc_offset_norm > 0.02 {
            anomalies.push(SignalAnomaly {
                kind: SignalAnomalyKind::DcOffset,
                severity: AnomalySeverity::Warning,
                timestamp_seconds: 0.0,
                duration_seconds: None,
                value: dc_offset_norm,
                message: format!(
                    "DC offset detected: mean amplitude shift {:.3} ({:.1}%)",
                    dc_offset_norm,
                    dc_offset_norm * 100.0
                ),
            });
        }

        // 2. Scan for clipping runs
        let mut in_clipping_run = false;
        let mut run_start_sample = 0usize;

        for (i, &sample) in samples.iter().enumerate() {
            if sample.abs() >= 32760 {
                if !in_clipping_run {
                    in_clipping_run = true;
                    run_start_sample = i;
                }
            } else if in_clipping_run {
                in_clipping_run = false;
                let ts = (run_start_sample / ch) as f64 / sr;
                let dur = ((i - run_start_sample) / ch) as f64 / sr;
                if anomalies.len() < max_anomalies {
                    anomalies.push(SignalAnomaly {
                        kind: SignalAnomalyKind::Clipping,
                        severity: AnomalySeverity::Critical,
                        timestamp_seconds: (ts * 100.0).round() / 100.0,
                        duration_seconds: Some((dur * 1000.0).round() / 1000.0),
                        value: 0.0,
                        message: format!("Signal clipping detected at {:.2}s", ts),
                    });
                } else {
                    scan_bounded = true;
                }
            }
        }
        if in_clipping_run && anomalies.len() < max_anomalies {
            let ts = (run_start_sample / ch) as f64 / sr;
            anomalies.push(SignalAnomaly {
                kind: SignalAnomalyKind::Clipping,
                severity: AnomalySeverity::Critical,
                timestamp_seconds: (ts * 100.0).round() / 100.0,
                duration_seconds: None,
                value: 0.0,
                message: format!("Signal clipping near end of audio at {:.2}s", ts),
            });
        }

        // 3. Frame-by-frame analysis for silence and relative volume jumps (20ms frames)
        let frame_len = (sample_rate as usize / 50).max(1) * ch;
        let mut last_speech_rms_db: Option<f32> = None;
        let mut consecutive_silent_frames = 0usize;
        let mut silence_start_sample = 0usize;

        for (frame_idx, chunk) in samples.chunks(frame_len).enumerate() {
            if anomalies.len() >= max_anomalies {
                scan_bounded = true;
                break;
            }

            let frame_start_sample = frame_idx * frame_len;
            let frame_sum_sq: f64 = chunk
                .iter()
                .map(|&s| {
                    let n = s as f64 / 32768.0;
                    n * n
                })
                .sum();
            let frame_rms = (frame_sum_sq / chunk.len() as f64).sqrt() as f32;
            let frame_rms_db = if frame_rms > 1e-5 {
                20.0 * frame_rms.log10()
            } else {
                -96.0
            };

            // Silence threshold: below -52 dBFS
            if frame_rms_db < -52.0 {
                if consecutive_silent_frames == 0 {
                    silence_start_sample = frame_start_sample;
                }
                consecutive_silent_frames += 1;
            } else {
                if consecutive_silent_frames > 0 {
                    let silence_duration = (consecutive_silent_frames * frame_len / ch) as f64 / sr;
                    // Flag silence if > 850ms
                    if silence_duration >= 0.85 && anomalies.len() < max_anomalies {
                        let ts = (silence_start_sample / ch) as f64 / sr;
                        anomalies.push(SignalAnomaly {
                            kind: SignalAnomalyKind::LongSilence,
                            severity: AnomalySeverity::Warning,
                            timestamp_seconds: (ts * 100.0).round() / 100.0,
                            duration_seconds: Some((silence_duration * 100.0).round() / 100.0),
                            value: silence_duration as f32,
                            message: format!(
                                "Extended silence gap of {:.2}s at {:.2}s",
                                silence_duration, ts
                            ),
                        });
                    }
                    consecutive_silent_frames = 0;
                }

                // Check sudden volume jump between active speech frames
                if frame_rms_db > -35.0 {
                    if let Some(prev_db) = last_speech_rms_db {
                        let diff = (frame_rms_db - prev_db).abs();
                        if diff > 22.0 && anomalies.len() < max_anomalies {
                            let ts = (frame_start_sample / ch) as f64 / sr;
                            anomalies.push(SignalAnomaly {
                                kind: SignalAnomalyKind::RelativeVolumeJump,
                                severity: AnomalySeverity::Info,
                                timestamp_seconds: (ts * 100.0).round() / 100.0,
                                duration_seconds: None,
                                value: diff,
                                message: format!(
                                    "Sudden dynamic volume jump of {:.1} dB at {:.2}s",
                                    diff, ts
                                ),
                            });
                        }
                    }
                    last_speech_rms_db = Some(frame_rms_db);
                }
            }
        }

        let is_clean = anomalies.is_empty();
        let warnings_count = anomalies.len();

        AudioSignalReport {
            metrics,
            is_clean,
            warnings_count,
            anomalies,
            scan_bounded,
        }
    }
}
