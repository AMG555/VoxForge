/// EBU R128 / LUFS loudness normalization target constants.
pub const LUFS_TARGET_VOICE: f32 = -16.0; // Podcast / voice content (-16 LUFS)
pub const LUFS_TARGET_YOUTUBE: f32 = -14.0; // YouTube / streaming (-14 LUFS)
pub const LUFS_TARGET_BROADCAST: f32 = -23.0; // EBU R128 broadcast (-23 LUFS)

/// Duration threshold above which the two-pass LUFS algorithm is used.
/// Two-pass loudnorm is applied for audiobook/dub content (>60s).
const TWO_PASS_THRESHOLD_SECS: f32 = 60.0;

pub struct AudioNormalizer;

impl AudioNormalizer {
    /// Integrated loudness normalization to a target LUFS value.
    ///
    /// For audio ≤ 60 seconds: single-pass K-weighted RMS measurement → constant gain.
    ///
    /// For audio > 60 seconds (audiobook / dub): **two-pass** algorithm matching
    /// standard broadcast two-pass ffmpeg `loudnorm` behaviour. Pass 1 measures integrated
    /// loudness over the whole file using 400ms gating blocks (EBU R128 §2.11) that
    /// exclude pauses and room noise. Pass 2 applies the correction gain from pass 1,
    /// giving an accurate integrated loudness reading for long-form content.
    ///
    /// # Arguments
    /// - `target_lufs`: Target integrated loudness (e.g. -16.0 for podcast, -14.0 for YouTube)
    pub fn lufs_normalize(samples: &mut [i16], sample_rate: u32, target_lufs: f32) {
        if samples.is_empty() || sample_rate == 0 || !target_lufs.is_finite() {
            return;
        }

        let duration_secs = samples.len() as f32 / sample_rate.max(1) as f32;
        if duration_secs > TWO_PASS_THRESHOLD_SECS {
            Self::lufs_normalize_two_pass(samples, sample_rate, target_lufs);
            return;
        }

        let sr = sample_rate as f32;

        // --- K-weighting stage 1: Pre-filter (high-shelf, +4dB at 1681 Hz) ---
        // IIR coefficients for the K-weighting pre-filter at 48kHz, re-computed for target rate.
        // Design: second-order high-shelf with Fc = 1681.97 Hz, +3.999843 dB gain
        let f0 = 1681.97f32;
        let q = std::f32::consts::FRAC_1_SQRT_2;
        let gain_db = 4.0f32;
        let a = 10.0f32.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * f0 / sr;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);

        let (b0, b1, b2, a0, a1, a2) = {
            let a0 = (a + 1.0) - (a - 1.0) * cos_w0 + 2.0 * a.sqrt() * alpha;
            let b0 = a * ((a + 1.0) + (a - 1.0) * cos_w0 + 2.0 * a.sqrt() * alpha) / a0;
            let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0) / a0;
            let b2 = a * ((a + 1.0) + (a - 1.0) * cos_w0 - 2.0 * a.sqrt() * alpha) / a0;
            let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_w0) / a0;
            let a2 = ((a + 1.0) - (a - 1.0) * cos_w0 - 2.0 * a.sqrt() * alpha) / a0;
            (b0, b1, b2, a0, a1, a2)
        };
        let _ = a0; // normalized; a0 = 1.0 after dividing above

        // --- Stage 2: RLB weighting (high-pass, Fc ≈ 38.1 Hz, 2nd order) ---
        let f_rlb = 38.13f32;
        let w_rlb = 2.0 * std::f32::consts::PI * f_rlb / sr;
        let cos_rlb = w_rlb.cos();
        let sin_rlb = w_rlb.sin();
        let alpha_rlb = sin_rlb / (2.0 * std::f32::consts::FRAC_1_SQRT_2);
        let a0_rlb = 1.0 + alpha_rlb;
        let rlb_b0 = ((1.0 + cos_rlb) / 2.0) / a0_rlb;
        let rlb_b1 = (-(1.0 + cos_rlb)) / a0_rlb;
        let rlb_b2 = rlb_b0;
        let rlb_a1 = (-2.0 * cos_rlb) / a0_rlb;
        let rlb_a2 = (1.0 - alpha_rlb) / a0_rlb;

        // Apply K-weighting filters and compute mean-square
        let mut x1 = 0.0f64;
        let mut x2 = 0.0f64;
        let mut y1 = 0.0f64;
        let mut y2 = 0.0f64;
        let mut rx1 = 0.0f64;
        let mut rx2 = 0.0f64;
        let mut ry1 = 0.0f64;
        let mut ry2 = 0.0f64;

        let mut mean_sq = 0.0f64;
        for &s in samples.iter() {
            let input = s as f64 / 32768.0;

            // Pre-filter
            let pf = b0 as f64 * input + b1 as f64 * x1 + b2 as f64 * x2
                - a1 as f64 * y1
                - a2 as f64 * y2;
            x2 = x1;
            x1 = input;
            y2 = y1;
            y1 = if pf.is_finite() { pf } else { 0.0 };

            // RLB high-pass
            let rlb = rlb_b0 as f64 * y1 + rlb_b1 as f64 * rx1 + rlb_b2 as f64 * rx2
                - rlb_a1 as f64 * ry1
                - rlb_a2 as f64 * ry2;
            rx2 = rx1;
            rx1 = y1;
            ry2 = ry1;
            ry1 = if rlb.is_finite() { rlb } else { 0.0 };

            mean_sq += ry1 * ry1;
        }

        mean_sq /= samples.len() as f64;

        if mean_sq < 1e-10 {
            return; // Silent input
        }

        // Integrated loudness in LUFS: L = -0.691 + 10 * log10(mean_sq)
        // (For mono: G = -0.691, for stereo each channel weighted by 1.0)
        let lufs = -0.691 + 10.0 * mean_sq.log10() as f32;

        // Required gain to reach target
        let gain_db = (target_lufs - lufs).clamp(-24.0, 24.0); // ±24 dB max
        let linear_gain = 10.0f32.powf(gain_db / 20.0);

        if !linear_gain.is_finite() || (linear_gain - 1.0).abs() < 0.001 {
            return;
        }

        for sample in samples.iter_mut() {
            let scaled = (*sample as f32 * linear_gain).round();
            if scaled.is_finite() {
                *sample = scaled.clamp(-32768.0, 32767.0) as i16;
            }
        }
    }

    /// Two-pass EBU R128-gated LUFS normalization for long-form content (>60s).
    ///
    /// Pass 1 — Gated loudness measurement (EBU R128 §2.11):
    ///   1. Split the K-weighted signal into 400ms blocks with 75% overlap (100ms hop).
    ///   2. Discard blocks whose mean-square is below the absolute gate (-70 LUFS).
    ///   3. Compute preliminary integrated loudness from the remaining blocks.
    ///   4. Discard blocks whose mean-square is more than 10 LU below the preliminary loudness
    ///      (relative gate). Recompute integrated loudness from surviving blocks.
    ///
    /// Pass 2 — Apply corrective gain derived from gated measurement.
    fn lufs_normalize_two_pass(samples: &mut [i16], sample_rate: u32, target_lufs: f32) {
        let sr = sample_rate.max(1) as f32;
        let block_len = (sr * 0.4) as usize; // 400ms block
        let hop_len = (sr * 0.1) as usize; // 100ms hop (75% overlap)

        // ── Compute K-weighted signal once ──
        let kw = Self::k_weight(samples, sample_rate);

        // ── Pass 1: collect per-block mean-square ──
        let abs_gate_ms = 10.0f64.powf((-70.0 + 0.691) / 10.0); // -70 LUFS in mean-sq
        let mut block_ms: Vec<f64> = Vec::new();
        let mut pos = 0;
        while pos + block_len <= kw.len() {
            let ms =
                kw[pos..pos + block_len].iter().map(|&x| x * x).sum::<f64>() / block_len as f64;
            if ms > abs_gate_ms {
                block_ms.push(ms);
            }
            pos += hop_len;
        }

        if block_ms.is_empty() {
            // Entirely below absolute gate — fall back to single-pass
            Self::lufs_normalize(samples, sample_rate, target_lufs);
            return;
        }

        // Preliminary integrated loudness
        let prelim_ms = block_ms.iter().sum::<f64>() / block_ms.len() as f64;
        let prelim_lufs = (-0.691 + 10.0 * prelim_ms.log10()) as f32;

        // Relative gate: -10 LU below preliminary
        let rel_gate_ms = 10.0f64.powf(((prelim_lufs - 10.0) as f64 + 0.691) / 10.0);
        let gated_ms: Vec<f64> = block_ms
            .into_iter()
            .filter(|&ms| ms > rel_gate_ms)
            .collect();

        if gated_ms.is_empty() {
            Self::lufs_normalize(samples, sample_rate, target_lufs);
            return;
        }

        // ── Pass 2: compute final loudness from gated blocks, apply gain ──
        let final_ms = gated_ms.iter().sum::<f64>() / gated_ms.len() as f64;
        let measured_lufs = (-0.691 + 10.0 * final_ms.log10()) as f32;

        let gain_db = (target_lufs - measured_lufs).clamp(-24.0, 24.0);
        let linear_gain = 10.0f32.powf(gain_db / 20.0);

        if !linear_gain.is_finite() || (linear_gain - 1.0).abs() < 0.001 {
            return;
        }

        for sample in samples.iter_mut() {
            let scaled = (*sample as f32 * linear_gain).round();
            if scaled.is_finite() {
                *sample = scaled.clamp(-32768.0, 32767.0) as i16;
            }
        }
    }

    /// Apply ITU-R BS.1770 K-weighting filters to samples and return the filtered signal as f64.
    /// Used by both single-pass and two-pass LUFS normalization.
    fn k_weight(samples: &[i16], sample_rate: u32) -> Vec<f64> {
        let sr = sample_rate.max(1) as f32;

        // Pre-filter (high-shelf +4dB at 1681 Hz)
        let f0 = 1681.97f32;
        let q = std::f32::consts::FRAC_1_SQRT_2;
        let gain_db = 4.0f32;
        let a = 10.0f32.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * f0 / sr;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);
        let a0_pf = (a + 1.0) - (a - 1.0) * cos_w0 + 2.0 * a.sqrt() * alpha;
        let pf_b0 = a * ((a + 1.0) + (a - 1.0) * cos_w0 + 2.0 * a.sqrt() * alpha) / a0_pf;
        let pf_b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0) / a0_pf;
        let pf_b2 = a * ((a + 1.0) + (a - 1.0) * cos_w0 - 2.0 * a.sqrt() * alpha) / a0_pf;
        let pf_a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_w0) / a0_pf;
        let pf_a2 = ((a + 1.0) - (a - 1.0) * cos_w0 - 2.0 * a.sqrt() * alpha) / a0_pf;

        // RLB high-pass (Fc ≈ 38.1 Hz)
        let f_rlb = 38.13f32;
        let w_rlb = 2.0 * std::f32::consts::PI * f_rlb / sr;
        let cos_rlb = w_rlb.cos();
        let sin_rlb = w_rlb.sin();
        let alpha_rlb = sin_rlb / (2.0 * std::f32::consts::FRAC_1_SQRT_2);
        let a0_rlb = 1.0 + alpha_rlb;
        let rlb_b0 = ((1.0 + cos_rlb) / 2.0) / a0_rlb;
        let rlb_b1 = (-(1.0 + cos_rlb)) / a0_rlb;
        let rlb_b2 = rlb_b0;
        let rlb_a1 = (-2.0 * cos_rlb) / a0_rlb;
        let rlb_a2 = (1.0 - alpha_rlb) / a0_rlb;

        let mut x1 = 0.0f64;
        let mut x2 = 0.0f64;
        let mut y1 = 0.0f64;
        let mut y2 = 0.0f64;
        let mut rx1 = 0.0f64;
        let mut rx2 = 0.0f64;
        let mut ry1 = 0.0f64;
        let mut ry2 = 0.0f64;
        let mut out = Vec::with_capacity(samples.len());

        for &s in samples {
            let input = s as f64 / 32768.0;
            let pf = pf_b0 as f64 * input + pf_b1 as f64 * x1 + pf_b2 as f64 * x2
                - pf_a1 as f64 * y1
                - pf_a2 as f64 * y2;
            x2 = x1;
            x1 = input;
            y2 = y1;
            y1 = if pf.is_finite() { pf } else { 0.0 };

            let rlb = rlb_b0 as f64 * y1 + rlb_b1 as f64 * rx1 + rlb_b2 as f64 * rx2
                - rlb_a1 as f64 * ry1
                - rlb_a2 as f64 * ry2;
            rx2 = rx1;
            rx1 = y1;
            ry2 = ry1;
            ry1 = if rlb.is_finite() { rlb } else { 0.0 };
            out.push(ry1);
        }
        out
    }

    pub fn peak_normalize(samples: &mut [i16], target_peak_ratio: f32) {
        if samples.is_empty() || !target_peak_ratio.is_finite() {
            return;
        }

        let max_abs = samples.iter().map(|&s| (s as i32).abs()).max().unwrap_or(0);

        if max_abs == 0 {
            return;
        }

        let target_peak = 32767.0 * target_peak_ratio.clamp(0.0, 1.0);
        let gain = target_peak / (max_abs as f32);

        if !gain.is_finite() || (gain - 1.0).abs() < 0.001 {
            return;
        }

        for sample in samples.iter_mut() {
            let scaled = (*sample as f32 * gain).round();
            if scaled.is_finite() {
                *sample = scaled.clamp(-32768.0, 32767.0) as i16;
            }
        }
    }

    pub fn apply_gain_db(samples: &mut [i16], gain_db: f32) {
        if !gain_db.is_finite() || gain_db.abs() < 0.01 {
            return;
        }

        let safe_gain_db = gain_db.clamp(-120.0, 60.0);
        let linear_factor = 10.0f32.powf(safe_gain_db / 20.0);
        if !linear_factor.is_finite() {
            return;
        }

        for sample in samples.iter_mut() {
            let scaled = (*sample as f32 * linear_factor).round();
            if scaled.is_finite() {
                *sample = scaled.clamp(-32768.0, 32767.0) as i16;
            }
        }
    }

    /// Normalizes audio so its Root Mean Square (RMS) energy matches `target_rms_ratio` (0.0 to 1.0 relative to full scale)
    pub fn rms_normalize(samples: &mut [i16], target_rms_ratio: f32) {
        if samples.is_empty() || !target_rms_ratio.is_finite() {
            return;
        }

        let sum_sq: f64 = samples.iter().map(|&s| (s as f64).powi(2)).sum();
        let rms = (sum_sq / samples.len() as f64).sqrt();
        if rms < 1.0 {
            return;
        }

        let target_rms = 32767.0 * target_rms_ratio.clamp(0.01, 1.0) as f64;
        let gain = (target_rms / rms) as f32;
        if !gain.is_finite() || (gain - 1.0).abs() < 0.001 {
            return;
        }

        for sample in samples.iter_mut() {
            let scaled = (*sample as f32 * gain).round();
            if scaled.is_finite() {
                *sample = scaled.clamp(-32768.0, 32767.0) as i16;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_and_nan_peak_normalize() {
        let mut samples = vec![];
        AudioNormalizer::peak_normalize(&mut samples, 0.9);
        assert!(samples.is_empty());

        let mut samples = vec![100, 200];
        let original = samples.clone();
        AudioNormalizer::peak_normalize(&mut samples, f32::NAN);
        assert_eq!(samples, original);
    }

    #[test]
    fn test_all_zeros_peak_normalize() {
        let mut samples = vec![0, 0, 0];
        AudioNormalizer::peak_normalize(&mut samples, 1.0);
        assert_eq!(samples, vec![0, 0, 0]);
    }

    #[test]
    fn test_rms_normalize_nan_and_silence() {
        let mut samples = vec![0, 0, 0];
        AudioNormalizer::rms_normalize(&mut samples, 0.2);
        assert_eq!(samples, vec![0, 0, 0]);

        let mut samples = vec![1000, 2000];
        let original = samples.clone();
        AudioNormalizer::rms_normalize(&mut samples, f32::NAN);
        assert_eq!(samples, original);
    }

    #[test]
    fn test_apply_gain_db_boundary() {
        let mut samples = vec![10000];
        // Large boost clamps to i16::MAX safely
        AudioNormalizer::apply_gain_db(&mut samples, 40.0);
        assert_eq!(samples[0], 32767);

        // Huge attenuation brings to near-zero
        AudioNormalizer::apply_gain_db(&mut samples, -100.0);
        assert!(samples[0].abs() <= 1);
    }
}
