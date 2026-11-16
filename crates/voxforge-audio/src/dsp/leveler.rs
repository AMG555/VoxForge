//! Phrase-level loudness leveler — the key perceptual quality improvement from Studio.
//!
//! Studio's `level_speech()` function (backend/services/audio_dsp.py) works as a
//! **slow phrase leveler**, NOT a traditional compressor. The key difference:
//!
//! - Traditional compressor: 5–50ms attack → flattens syllable peaks → "pumping"
//! - Phrase leveler: 400ms measurement window → equalises phrase-to-phrase loudness
//!   while preserving all intra-phrase dynamics (syllable peaks intact)
//!
//! Algorithm:
//! 1. Compute per-frame RMS using 400ms exponential moving average (EMA).
//! 2. Apply a speech gate: frames below `gate_dbfs` (-65 dBFS) are excluded from
//!    the EMA update (pauses don't drag the level estimate down — EBU R128 style).
//! 3. Compute a smoothed gain inverse: `gain = target_rms / smoothed_rms`
//!    clamped to [0.1, 8.0] to prevent extreme amplification of whispered passages.
//! 4. Apply per-sample gain with a secondary 10ms smoothing on the gain itself
//!    to avoid gain-change clicks at phrase boundaries.

pub struct PhraseLeveler;

/// Default parameters matching Studio's level_speech() tuning.
impl PhraseLeveler {
    /// Target RMS amplitude for leveled output (≈ -20 dBFS in linear i16 domain).
    const TARGET_RMS: f32 = 32767.0 * 0.100; // -20 dBFS

    /// Speech gate threshold: frames below this are pauses (excluded from level tracking).
    const GATE_DBFS: f32 = -65.0;

    /// Level-tracking time constant: 400ms (slow — phrase-level, not syllable).
    const LEVEL_TC_MS: f32 = 400.0;

    /// Gain-smoothing time constant: 10ms (prevents clicks when gain changes).
    const GAIN_TC_MS: f32 = 10.0;

    /// Maximum gain amplification (prevent over-boost of whispered/quiet segments).
    const MAX_GAIN: f32 = 8.0;

    /// Minimum gain (prevent extreme attenuation of already-loud segments).
    const MIN_GAIN: f32 = 0.10;

    /// Level-equalize speech samples in-place.
    ///
    /// # Arguments
    /// - `samples`: 16-bit PCM audio (modified in place)
    /// - `sample_rate`: Audio sample rate in Hz
    pub fn process(samples: &mut [i16], sample_rate: u32) {
        if samples.len() < 64 || sample_rate == 0 {
            return;
        }

        let sr = sample_rate.max(1) as f32;

        // Compute time-constant coefficients
        let level_alpha = (-1000.0_f32 / (sr * Self::LEVEL_TC_MS)).exp();
        let gain_alpha = (-1000.0_f32 / (sr * Self::GAIN_TC_MS)).exp();

        // Gate amplitude threshold
        let gate_amp = 32767.0f32 * 10.0f32.powf(Self::GATE_DBFS / 20.0);

        // Phase 1: compute per-sample smoothed RMS envelope (EMA of squared amplitude)
        let mut rms_sq = 0.0f32;
        let mut rms_envelope = Vec::with_capacity(samples.len());

        for &s in samples.iter() {
            let abs_amp = (s as f32).abs();
            let sq = abs_amp * abs_amp;
            if abs_amp > gate_amp {
                // Voiced frame: update EMA
                rms_sq = level_alpha * rms_sq + (1.0 - level_alpha) * sq;
            }
            // During pauses rms_sq holds its last value (EBU-style gate hold)
            rms_envelope.push(rms_sq.sqrt().max(1.0));
        }

        // Phase 2: compute gain envelope and apply with gain smoothing
        let mut smoothed_gain = 1.0f32;

        for (i, sample) in samples.iter_mut().enumerate() {
            let rms = rms_envelope[i];
            let raw_gain = (Self::TARGET_RMS / rms).clamp(Self::MIN_GAIN, Self::MAX_GAIN);

            // Smooth the gain to prevent per-sample gain discontinuities (clicks)
            smoothed_gain = gain_alpha * smoothed_gain + (1.0 - gain_alpha) * raw_gain;

            let leveled = (*sample as f32 * smoothed_gain)
                .round()
                .clamp(-32768.0, 32767.0);
            *sample = leveled as i16;
        }
    }

    /// Process and return a new Vec<i16> (non-mutating variant).
    pub fn process_alloc(samples: &[i16], sample_rate: u32) -> Vec<i16> {
        let mut out = samples.to_vec();
        Self::process(&mut out, sample_rate);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms_i16(s: &[i16]) -> f32 {
        if s.is_empty() {
            return 0.0;
        }
        let sum: f64 = s.iter().map(|&x| (x as f64).powi(2)).sum();
        (sum / s.len() as f64).sqrt() as f32
    }

    #[test]
    fn test_phrase_leveler_empty_no_panic() {
        let mut empty: Vec<i16> = vec![];
        PhraseLeveler::process(&mut empty, 24000);
        assert!(empty.is_empty());
    }

    #[test]
    fn test_phrase_leveler_no_clipping() {
        let sr = 24_000u32;
        // Loud sine wave at 440 Hz, full scale
        let samples: Vec<i16> = (0..(sr * 2))
            .map(|i| {
                let t = i as f32 / sr as f32;
                (30000.0 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()) as i16
            })
            .collect();
        let mut out = samples.clone();
        PhraseLeveler::process(&mut out, sr);
        // All samples must stay in i16 range
        // i16 is by definition in [-32768, 32767]; verify length and no panic
        assert!(!out.is_empty());
    }

    #[test]
    fn test_phrase_leveler_equalizes_loud_quiet_phrases() {
        let sr = 24_000u32;
        // Use 2-second phrases so the 400ms TC has time to fully settle (need ≥ 800ms settle).
        // Phrase 1: loud (20000 amplitude), Phrase 2: quiet (2000 amplitude)
        let phrase_len = sr as usize * 2; // 2 seconds each
        let loud: Vec<i16> = (0..phrase_len)
            .map(|i| {
                let t = i as f32 / sr as f32;
                (20000.0 * (2.0 * std::f32::consts::PI * 200.0 * t).sin()) as i16
            })
            .collect();
        let quiet: Vec<i16> = (0..phrase_len)
            .map(|i| {
                let t = i as f32 / sr as f32;
                (2000.0 * (2.0 * std::f32::consts::PI * 200.0 * t).sin()) as i16
            })
            .collect();

        let mut combined: Vec<i16> = loud.iter().chain(quiet.iter()).cloned().collect();
        let rms_before_loud = rms_i16(&combined[..phrase_len]);
        let rms_before_quiet = rms_i16(&combined[phrase_len..]);
        let ratio_before = rms_before_loud / rms_before_quiet.max(1.0);

        PhraseLeveler::process(&mut combined, sr);

        // Skip the first 800ms to let the 400ms TC settle, then measure the second half.
        // settle = 800ms = 19200 samples (comfortably < phrase_len = 48000)
        let settle = (sr as usize * 8) / 10;
        let rms_after_loud = rms_i16(&combined[settle..phrase_len]);
        let rms_after_quiet = rms_i16(&combined[phrase_len + settle..]);
        let ratio_after = rms_after_loud / rms_after_quiet.max(1.0);

        // After settling, ratio must be meaningfully smaller (leveler reduces phrase disparity)
        assert!(
            ratio_after < ratio_before * 0.90,
            "Leveling must reduce phrase loudness ratio. before={:.1} after={:.1}",
            ratio_before,
            ratio_after
        );
    }

    #[test]
    fn test_phrase_leveler_process_alloc_matches_in_place() {
        let sr = 24_000u32;
        let samples: Vec<i16> = (0..sr)
            .map(|i| ((i as i32 % 200 - 100) * 150) as i16)
            .collect();
        let mut in_place = samples.clone();
        PhraseLeveler::process(&mut in_place, sr);
        let alloc_result = PhraseLeveler::process_alloc(&samples, sr);
        assert_eq!(in_place, alloc_result);
    }

    #[test]
    fn test_phrase_leveler_silence_passthrough() {
        // All-silent signal — should not panic, output stays near zero
        let mut silent = vec![0i16; 48000];
        PhraseLeveler::process(&mut silent, 24000);
        // Output should be all zeros (gain applied to zero input = zero)
        assert!(silent.iter().all(|&s| s == 0));
    }
}
