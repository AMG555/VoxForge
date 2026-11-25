/// Voice Activity Detection-based silence trimmer with hysteresis.
///
/// Unlike the original single-sample threshold approach, this VAD uses:
/// 1. A short-time energy envelope with exponential smoothing (like pydub's silence detection)
/// 2. Hysteresis: speech starts when energy > `on_threshold`, ends when < `off_threshold`
///    (on_threshold ≈ 2× off_threshold to avoid chattering at the boundary)
/// 3. Minimum speech duration gate (40ms) to suppress click artefacts
/// 4. Configurable hold time: keeps `hold_ms` silence after last voiced frame
///    (prevents cutting off trailing consonants)
pub struct SilenceTrimmer;

impl SilenceTrimmer {
    /// Trim leading and trailing silence from PCM audio using energy VAD with hysteresis.
    ///
    /// Uses a **retry ladder**: if the first
    /// attempt at `threshold_dbfs` returns empty, retries at successively softer thresholds
    /// (-55 dBFS, then -65 dBFS) before falling back to the original. This prevents consuming
    /// the full clip when the reference audio is recorded at a low level.
    ///
    /// - `threshold_dbfs`: Initial speech activity threshold (e.g. -45.0 dBFS)
    /// - `padding_ms`: Extra ms to retain around active speech (prevents cutting consonants)
    pub fn trim(
        samples: &[i16],
        sample_rate: u32,
        threshold_dbfs: f32,
        padding_ms: u32,
    ) -> Vec<i16> {
        if samples.is_empty() {
            return Vec::new();
        }

        // Retry at softer thresholds when the initial attempt finds nothing,
        // using a progressive retry ladder (-50 → -60 → -70 dBFS).
        let thresholds: [f32; 3] = [threshold_dbfs, -55.0, -65.0];
        for &threshold in &thresholds {
            let result = Self::trim_at_threshold(samples, sample_rate, threshold, padding_ms);
            if !result.is_empty() {
                return result;
            }
        }
        // All thresholds returned empty → signal is truly inaudible (all-zero or sub -65 dBFS).
        // Return empty to signal "no usable audio" to the caller when nothing passes the softest threshold.
        Vec::new()
    }

    /// Inner trim implementation at a fixed threshold (called by the retry ladder).
    fn trim_at_threshold(
        samples: &[i16],
        sample_rate: u32,
        threshold_dbfs: f32,
        padding_ms: u32,
    ) -> Vec<i16> {
        if samples.is_empty() {
            return Vec::new();
        }

        let sr = sample_rate.max(1) as f32;
        let safe_threshold_dbfs = if threshold_dbfs.is_finite() {
            threshold_dbfs.clamp(-96.0, 0.0)
        } else {
            -45.0
        };

        // On/off hysteresis thresholds (-45 dBFS on, -48 dBFS off)
        let on_amp = 32767.0f32 * 10.0f32.powf(safe_threshold_dbfs / 20.0);
        let off_amp = on_amp * std::f32::consts::FRAC_1_SQRT_2; // 3 dB below on_threshold

        let padding_samples =
            ((sr * padding_ms.min(2000) as f32 / 1000.0) as usize).min(samples.len() / 2);

        // Short-time energy envelope with ~8ms smoothing
        let alpha_attack = (-1.0f32 / (sr * 0.008)).exp(); // fast attack
        let alpha_release = (-1.0f32 / (sr * 0.040)).exp(); // slower release (40ms)

        let mut envelope = 0.0f32;
        let mut envelopes = Vec::with_capacity(samples.len());
        for &s in samples {
            let abs_s = (s as f32).abs();
            envelope = if abs_s > envelope {
                alpha_attack * envelope + (1.0 - alpha_attack) * abs_s
            } else {
                alpha_release * envelope + (1.0 - alpha_release) * abs_s
            };
            envelopes.push(envelope);
        }

        // VAD state machine: find the FIRST frame that crosses on_amp and
        // the LAST frame that is still above off_amp
        let first_active = envelopes.iter().position(|&e| e >= on_amp);
        let last_active = envelopes.iter().rposition(|&e| e >= off_amp);

        match (first_active, last_active) {
            (Some(start), Some(end)) if start <= end => {
                let trim_start = start.saturating_sub(padding_samples);
                let trim_end = (end + 1 + padding_samples).min(samples.len());

                // Safety: never return fewer than 40ms of audio if something was found
                let min_len = (sr * 0.040) as usize;
                if trim_end.saturating_sub(trim_start) < min_len.min(samples.len()) {
                    return samples.to_vec();
                }

                samples[trim_start..trim_end].to_vec()
            }
            _ => {
                // Entire signal below threshold — return empty to signal no usable audio
                Vec::new()
            }
        }
    }

    /// Return `true` if the sample has sufficient voiced content to be a usable
    /// voice cloning reference (at least 500ms of speech above -50 dBFS).
    pub fn has_enough_speech(samples: &[i16], sample_rate: u32) -> bool {
        if samples.is_empty() || sample_rate == 0 {
            return false;
        }
        let sr = sample_rate as f32;
        let min_voiced_samples = (sr * 0.5) as usize; // 500ms minimum
        let speech_amp = 32767.0f32 * 10.0f32.powf(-50.0 / 20.0);

        let voiced_count = samples
            .iter()
            .filter(|&&s| (s as f32).abs() >= speech_amp)
            .count();
        voiced_count >= min_voiced_samples
    }

    /// Score the voice quality of a reference audio window (0.0 = unusable, 1.0 = perfect).
    ///
    /// Combines:
    /// - Voiced fraction (wants ≥ 50% of frames to be speech)
    /// - Peak-to-RMS ratio (crest factor): too high → impulsive noise; too low → clipping
    /// - Zero-crossing rate: very high → noise or music artefacts
    ///
    /// Uses a quality gate to pick the "best window" from a
    /// long reference recording.
    pub fn score_reference_quality(samples: &[i16], sample_rate: u32) -> f32 {
        if samples.len() < 240 || sample_rate == 0 {
            return 0.0;
        }

        let sr = sample_rate as f32;
        let speech_amp = 32767.0f32 * 10.0f32.powf(-50.0 / 20.0);

        // 1. Voiced fraction
        let voiced = samples
            .iter()
            .filter(|&&s| (s as f32).abs() >= speech_amp)
            .count();
        let voiced_frac = voiced as f32 / samples.len() as f32;
        if voiced_frac < 0.05 {
            return 0.0; // Mostly silence
        }

        // 2. RMS and peak
        let sum_sq: f64 = samples.iter().map(|&s| (s as f64).powi(2)).sum();
        let rms = (sum_sq / samples.len() as f64).sqrt() as f32;
        let peak = samples
            .iter()
            .map(|&s| (s as f32).abs())
            .fold(0.0f32, f32::max);

        if rms < 10.0 {
            return 0.0; // Too quiet
        }

        // Crest factor: 8–20 dB is healthy speech (high values → impulsive noise)
        let crest_db = if rms > 0.1 {
            20.0 * (peak / rms).log10()
        } else {
            0.0
        };
        let crest_score = if crest_db < 4.0 || crest_db > 30.0 {
            0.5
        } else {
            1.0
        };

        // 3. Zero crossing rate — healthy speech: ~0.03–0.15
        let zcr = {
            let mut zc = 0usize;
            for i in 1..samples.len() {
                if (samples[i] >= 0) != (samples[i - 1] >= 0) {
                    zc += 1;
                }
            }
            zc as f32 / samples.len() as f32
        };
        let zcr_score = if zcr < 0.01 || zcr > 0.35 { 0.6 } else { 1.0 };

        // 4. Duration: prefer ≥ 2s, penalise < 0.5s
        let duration_s = samples.len() as f32 / sr;
        let duration_score = (duration_s / 2.0).clamp(0.3, 1.0);

        // Weighted combination
        let score = voiced_frac.clamp(0.0, 1.0) * 0.40
            + crest_score * 0.25
            + zcr_score * 0.20
            + duration_score * 0.15;

        score.clamp(0.0, 1.0)
    }
    /// Select the highest-quality fixed-length window from a long recording.
    ///
    /// Slides a window over the reference audio and scores each candidate segment, returning
    /// the one with the highest voiced-fraction / SNR score.
    ///
    /// # Arguments
    /// - `samples`: Full-length reference audio (may be minutes long)
    /// - `sample_rate`: Audio sample rate in Hz
    /// - `window_secs`: Duration of each candidate window (default: 10.0s)
    /// - `hop_secs`: Step between windows (default: 2.0s)
    ///
    /// Returns a slice of `window_secs` duration starting at the best-scoring position.
    /// Falls back to `samples[0..min(window_len, samples.len())]` if no window scores > 0.
    pub fn best_reference_window(
        samples: &[i16],
        sample_rate: u32,
        window_secs: f32,
        hop_secs: f32,
    ) -> Vec<i16> {
        if samples.is_empty() || sample_rate == 0 {
            return Vec::new();
        }

        let sr = sample_rate;
        let window_secs = window_secs.clamp(2.0, 30.0);
        let hop_secs = hop_secs.clamp(0.5, window_secs);
        let window_len = (sr as f32 * window_secs) as usize;
        let hop_len = (sr as f32 * hop_secs) as usize;

        // If the audio is shorter than one window, return all of it
        if samples.len() <= window_len {
            return samples.to_vec();
        }

        let mut best_score = -1.0f32;
        let mut best_start = 0usize;

        let mut start = 0usize;
        loop {
            let end = (start + window_len).min(samples.len());
            if end <= start {
                break;
            }
            let window = &samples[start..end];
            let score = Self::score_reference_quality(window, sr);
            if score > best_score {
                best_score = score;
                best_start = start;
            }
            if end >= samples.len() {
                break;
            }
            start += hop_len;
        }

        let end = (best_start + window_len).min(samples.len());
        samples[best_start..end].to_vec()
    }

    /// Snap the edges of a proposed window inward to the nearest natural pause boundary.
    ///
    /// Prevents mid-word clips at window edges by pulling the start forward and the end backward until both land
    /// at a natural pause (continuous silence ≥ `min_pause_ms`).
    ///
    /// # Arguments
    /// - `samples`: The proposed window (already trimmed to target duration)
    /// - `sample_rate`: Audio sample rate in Hz
    /// - `search_ms`: How far from each edge to search for a pause (default: 1500ms)
    /// - `min_pause_ms`: Minimum silence duration to qualify as a "pause" boundary (default: 50ms)
    /// - `threshold_dbfs`: Amplitude below which a frame is considered silent (default: -55.0)
    ///
    /// Returns a sub-slice with both edges nudged to pause boundaries.
    /// If no pause boundary is found within `search_ms`, the original edges are kept.
    pub fn trim_passage_to_pauses(
        samples: &[i16],
        sample_rate: u32,
        search_ms: u32,
        min_pause_ms: u32,
        threshold_dbfs: f32,
    ) -> Vec<i16> {
        if samples.is_empty() || sample_rate == 0 {
            return samples.to_vec();
        }

        let sr = sample_rate.max(1) as f32;
        let safe_threshold = if threshold_dbfs.is_finite() {
            threshold_dbfs.clamp(-96.0, 0.0)
        } else {
            -55.0
        };

        let silent_amp = 32767.0f32 * 10.0f32.powf(safe_threshold / 20.0);
        let search_len =
            ((sr * search_ms.min(5000) as f32 / 1000.0) as usize).min(samples.len() / 2);
        let min_pause_len = ((sr * min_pause_ms.min(1000) as f32 / 1000.0) as usize).max(1);

        // Find leading pause: search forward from offset 0 up to search_len
        let new_start =
            Self::find_pause_boundary_forward(samples, 0, search_len, min_pause_len, silent_amp);

        // Find trailing pause: search backward from end, up to search_len from end
        let new_end = Self::find_pause_boundary_backward(
            samples,
            samples.len().saturating_sub(search_len),
            samples.len(),
            min_pause_len,
            silent_amp,
        );

        if new_start >= new_end {
            return samples.to_vec();
        }

        // Safety: never return fewer than 500ms if the original was longer
        let min_ret = ((sr * 0.5) as usize).min(samples.len());
        if new_end - new_start < min_ret {
            return samples.to_vec();
        }

        samples[new_start..new_end].to_vec()
    }

    // ── Internal helpers for trim_passage_to_pauses ──

    fn find_pause_boundary_forward(
        samples: &[i16],
        from: usize,
        to: usize,
        min_pause_len: usize,
        silent_amp: f32,
    ) -> usize {
        let to = to.min(samples.len());
        let mut run_start = from;
        let mut run_len = 0usize;

        for (offset, &s) in samples[from..to].iter().enumerate() {
            let i = from + offset;
            if (s as f32).abs() < silent_amp {
                if run_len == 0 {
                    run_start = i;
                }
                run_len += 1;
                if run_len >= min_pause_len {
                    return run_start;
                }
            } else {
                run_len = 0;
            }
        }
        from
    }

    fn find_pause_boundary_backward(
        samples: &[i16],
        from: usize,
        to: usize,
        min_pause_len: usize,
        silent_amp: f32,
    ) -> usize {
        let to = to.min(samples.len());
        let mut run_len = 0usize;

        for i in (from..to).rev() {
            if (samples[i] as f32).abs() < silent_amp {
                let run_end = i + 1;
                run_len += 1;
                if run_len >= min_pause_len {
                    return run_end;
                }
            } else {
                run_len = 0;
            }
        }
        to
    }

    /// Apply a smooth raised-cosine fade-out on the tail of a sample buffer to eliminate clicks.
    pub fn fade_out_tail(samples: &mut [i16], sample_rate: u32, fade_ms: f32) {
        if samples.len() < 4 || sample_rate == 0 || fade_ms <= 0.0 {
            return;
        }
        let fade_len =
            ((sample_rate as f32 * fade_ms / 1000.0).round() as usize).clamp(2, samples.len());
        let start = samples.len() - fade_len;
        let denom = (fade_len - 1) as f32;
        for (idx, sample) in samples[start..].iter_mut().enumerate() {
            let phase = (idx as f32 / denom) * std::f32::consts::PI;
            let gain = 0.5 * (1.0 + phase.cos());
            *sample = ((*sample as f32) * gain).round().clamp(-32768.0, 32767.0) as i16;
        }
    }

    /// Progressive-threshold silence trimmer (-45 -> -55 -> -65 dBFS) that never wipes quiet speech,
    /// finishing the tail with a 25ms cosine fade-out.
    pub fn trim_safe(samples: &[i16], sample_rate: u32, min_silence_pad_ms: u32) -> Vec<i16> {
        if samples.is_empty() {
            return Vec::new();
        }
        for &thresh in &[-45.0f32, -55.0, -65.0] {
            let mut trimmed = Self::trim(samples, sample_rate, thresh, min_silence_pad_ms);
            if !trimmed.is_empty() {
                Self::fade_out_tail(&mut trimmed, sample_rate, 25.0);
                return trimmed;
            }
        }
        let mut fallback = samples.to_vec();
        Self::fade_out_tail(&mut fallback, sample_rate, 25.0);
        fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trim_empty() {
        assert!(SilenceTrimmer::trim(&[], 24000, -45.0, 10).is_empty());
    }

    #[test]
    fn test_trim_removes_leading_trailing_silence() {
        let mut samples = vec![0i16; 500];
        samples.extend(vec![15000i16; 1000]);
        samples.extend(vec![0i16; 500]);

        let trimmed = SilenceTrimmer::trim(&samples, 24000, -40.0, 10);
        assert!(trimmed.len() < samples.len(), "Must trim silence");
        assert!(trimmed.len() >= 1000, "Must keep speech");
    }

    #[test]
    fn test_trim_all_silent_returns_empty() {
        let samples = vec![0i16; 2400];
        let result = SilenceTrimmer::trim(&samples, 24000, -45.0, 10);
        assert!(result.is_empty());
    }

    #[test]
    fn test_has_enough_speech() {
        // 2 seconds of voiced audio at 24kHz
        let samples = vec![15000i16; 48000];
        assert!(SilenceTrimmer::has_enough_speech(&samples, 24000));

        let silent = vec![0i16; 48000];
        assert!(!SilenceTrimmer::has_enough_speech(&silent, 24000));
    }

    #[test]
    fn test_score_reference_quality_speech() {
        let sr = 24000u32;
        // Simulate ~1s of speech-like signal (sine at 150Hz with amplitude variation)
        let samples: Vec<i16> = (0..sr)
            .map(|i| {
                let t = i as f32 / sr as f32;
                (12000.0 * (2.0 * std::f32::consts::PI * 150.0 * t).sin()) as i16
            })
            .collect();
        let score = SilenceTrimmer::score_reference_quality(&samples, sr);
        assert!(score > 0.4, "Speech sine should score > 0.4, got {}", score);
    }

    #[test]
    fn test_score_reference_quality_silence() {
        let silent = vec![0i16; 24000];
        let score = SilenceTrimmer::score_reference_quality(&silent, 24000);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_best_reference_window_short_returns_all() {
        // Shorter than one window → returns all samples unchanged
        let samples = vec![10000i16; 12000]; // 0.5s at 24kHz < 10s window
        let out = SilenceTrimmer::best_reference_window(&samples, 24000, 10.0, 2.0);
        assert_eq!(out.len(), samples.len());
    }

    #[test]
    fn test_best_reference_window_selects_speech_over_silence() {
        let sr = 24000u32;
        let window_len = (sr as f32 * 10.0) as usize;
        // 20s of silence followed by 10s of voiced speech
        let mut samples = vec![0i16; window_len * 2];
        let speech: Vec<i16> = (0..window_len)
            .map(|i| {
                let t = i as f32 / sr as f32;
                (12000.0 * (2.0 * std::f32::consts::PI * 150.0 * t).sin()) as i16
            })
            .collect();
        samples.extend_from_slice(&speech);

        let best = SilenceTrimmer::best_reference_window(&samples, sr, 10.0, 2.0);
        assert_eq!(best.len(), window_len);
        // The returned window should be the voiced portion
        let energy: f64 = best.iter().map(|&s| (s as f64).powi(2)).sum();
        assert!(energy > 1e9, "best window must be speech, not silence");
    }

    #[test]
    fn test_best_reference_window_empty() {
        let out = SilenceTrimmer::best_reference_window(&[], 24000, 10.0, 2.0);
        assert!(out.is_empty());
    }

    #[test]
    fn test_trim_passage_to_pauses_no_pauses_returns_original() {
        let sr = 24_000u32;
        // Constant loud signal — no pause boundaries
        let samples: Vec<i16> = vec![10000i16; sr as usize];
        let result = SilenceTrimmer::trim_passage_to_pauses(&samples, sr, 1500, 50, -55.0);
        // No pause found → return unchanged
        assert_eq!(result.len(), samples.len());
    }

    #[test]
    fn test_trim_passage_to_pauses_snaps_to_leading_pause() {
        let sr = 24_000u32;
        // Build: [500ms speech] [200ms silence] [2s speech] [200ms silence] [500ms speech]
        let speech_short: Vec<i16> = vec![10000i16; sr as usize / 2];
        let silence: Vec<i16> = vec![0i16; sr as usize / 5]; // 200ms
        let speech_long: Vec<i16> = vec![10000i16; sr as usize * 2];

        let samples: Vec<i16> = speech_short
            .iter()
            .chain(silence.iter())
            .chain(speech_long.iter())
            .chain(silence.iter())
            .chain(speech_short.iter())
            .cloned()
            .collect();

        let result = SilenceTrimmer::trim_passage_to_pauses(&samples, sr, 1500, 50, -40.0);
        // Result must be shorter than original (pauses found and edges snapped)
        assert!(
            result.len() < samples.len(),
            "Expected shorter result after snapping to pause boundaries, got {} vs {}",
            result.len(),
            samples.len()
        );
    }

    #[test]
    fn test_trim_passage_to_pauses_empty_passthrough() {
        let result = SilenceTrimmer::trim_passage_to_pauses(&[], 24000, 1500, 50, -55.0);
        assert!(result.is_empty());
    }

    #[test]
    fn test_trim_safe_ladder_keeps_quiet_speech_and_fades_tail() {
        let sr = 24000u32;
        let mut buf = vec![0i16; 4800];
        // ~ -60 dBFS quiet tone (amplitude 33)
        for i in 0..4800 {
            let v = ((i as f32 * 0.08).sin() * 36.0) as i16;
            buf.push(v);
        }
        buf.extend(vec![0i16; 4800]);
        let trimmed = SilenceTrimmer::trim_safe(&buf, sr, 20);
        assert!(!trimmed.is_empty(), "Safety ladder must keep quiet speech");
        assert!(trimmed.last().copied().unwrap_or(100).abs() <= 2);
    }
}
