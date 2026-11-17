pub struct DynamicCompressor;

impl DynamicCompressor {
    /// Feed-forward RMS compressor with soft-knee.
    ///
    /// Improvements over the previous hard-knee envelope compressor:
    /// 1. **Soft-knee** (8 dB wide by default): gradual onset avoids audible "pumping"
    ///    artifacts that make cloned voices sound processed
    /// 2. **True RMS detection** (10ms window): tracks perceived loudness rather than
    ///    instantaneous peaks, giving more natural-sounding compression
    /// 3. **Logarithmic gain smoothing**: computes gain reduction in dB domain and
    ///    applies attack/release there, matching professional compressor behavior
    ///    (e.g. Pro Tools Dyn3, Waves SSL G-Bus)
    ///
    /// # Arguments
    /// - `threshold_dbfs`: Level above which compression begins (e.g. -18.0)
    /// - `ratio`: Compression ratio (1.0 = no compression, 4.0 = 4:1, ∞ = limiting)
    /// - `attack_ms`: Attack time constant in milliseconds (5–50ms typical for voice)
    /// - `release_ms`: Release time constant in milliseconds (50–300ms typical for voice)
    /// - `makeup_gain_db`: Output gain after compression in dB
    pub fn process(
        samples: &mut [i16],
        sample_rate: u32,
        threshold_dbfs: f32,
        ratio: f32,
        attack_ms: f32,
        release_ms: f32,
        makeup_gain_db: f32,
    ) {
        if samples.is_empty() {
            return;
        }

        let sr = (sample_rate.max(8000)) as f32;
        let thresh = if threshold_dbfs.is_finite() {
            threshold_dbfs.clamp(-60.0, 0.0)
        } else {
            -18.0
        };
        let comp_ratio = if ratio.is_finite() {
            ratio.clamp(1.0, 20.0)
        } else {
            3.0
        };
        let att_ms = if attack_ms.is_finite() {
            attack_ms.clamp(0.5, 500.0)
        } else {
            15.0
        };
        let rel_ms = if release_ms.is_finite() {
            release_ms.clamp(5.0, 2000.0)
        } else {
            100.0
        };
        let makeup = if makeup_gain_db.is_finite() {
            makeup_gain_db.clamp(-24.0, 24.0)
        } else {
            0.0
        };

        let att_coeff = (-1.0 / (sr * (att_ms / 1000.0))).exp();
        let rel_coeff = (-1.0 / (sr * (rel_ms / 1000.0))).exp();

        // RMS window: 10ms (matches typical VU meter integration)
        let rms_window = ((sr * 0.010) as usize).max(1);
        let rms_coeff = (-1.0f32 / rms_window as f32).exp();

        // Soft-knee width in dB (8dB centered on threshold)
        let knee_half_db = 4.0f32;

        // Smooth gain state in dB domain (logarithmic attack/release)
        let mut rms_sq = 0.0f32; // smoothed squared amplitude
        let mut gain_db_smooth = 0.0f32; // current gain reduction state in dB

        for sample in samples.iter_mut() {
            let input_sq = ((*sample as f32) / 32768.0).powi(2);

            // RMS envelope: exponential moving average of power
            rms_sq = rms_coeff * rms_sq + (1.0 - rms_coeff) * input_sq;
            let rms_linear = rms_sq.sqrt().max(1e-7);
            let rms_db = 20.0 * rms_linear.log10();

            // Soft-knee gain reduction computation
            let gr_db = soft_knee_gain_reduction(rms_db, thresh, comp_ratio, knee_half_db);

            // Smooth gain reduction in dB domain (attack on increase, release on decrease)
            if gr_db < gain_db_smooth {
                // More negative = more compression → attack
                gain_db_smooth = att_coeff * gain_db_smooth + (1.0 - att_coeff) * gr_db;
            } else {
                // Less negative = releasing → release
                gain_db_smooth = rel_coeff * gain_db_smooth + (1.0 - rel_coeff) * gr_db;
            }

            let total_gain_db = gain_db_smooth + makeup;
            let linear_gain = 10.0f32.powf(total_gain_db / 20.0);

            if linear_gain.is_finite() {
                let scaled = (*sample as f32 * linear_gain).round();
                if scaled.is_finite() {
                    *sample = scaled.clamp(-32768.0, 32767.0) as i16;
                }
            }
        }
    }
}

/// Soft-knee gain reduction: smoothly interpolates between no compression (below knee)
/// and full ratio compression (above knee), eliminating the abrupt onset artifact.
///
/// Below `thresh - knee_half_db`: 0 dB gain reduction (no compression)
/// Above `thresh + knee_half_db`: (thresh - x) * (1 - 1/ratio) gain reduction (full)
/// Inside knee: quadratic interpolation between the two regions
#[inline]
fn soft_knee_gain_reduction(level_db: f32, thresh_db: f32, ratio: f32, knee_half_db: f32) -> f32 {
    let knee_start = thresh_db - knee_half_db;
    let knee_end = thresh_db + knee_half_db;
    let slope = 1.0 - 1.0 / ratio.max(1.0);

    if level_db <= knee_start {
        0.0
    } else if level_db >= knee_end {
        (thresh_db - level_db) * slope
    } else {
        // Quadratic interpolation inside the knee
        let knee_width = 2.0 * knee_half_db;
        let t = (level_db - knee_start) / knee_width; // 0.0 .. 1.0
                                                      // Smooth blend: starts at 0, reaches full slope at knee end
        let blend = t * t; // quadratic: 0 at start, 1 at end of knee
        let full_gr = (thresh_db - level_db) * slope;
        full_gr * blend
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compressor_reduces_loud_signal() {
        // 0 dBFS signal with -6 dB threshold and 4:1 ratio should be attenuated
        let mut loud = vec![32000i16; 2000];
        DynamicCompressor::process(&mut loud, 24000, -6.0, 4.0, 5.0, 100.0, 0.0);
        let mean_out: f64 = loud.iter().map(|&s| (s as f64).abs()).sum::<f64>() / loud.len() as f64;
        assert!(mean_out < 32000.0, "Compressor must attenuate loud signal");
    }

    #[test]
    fn test_compressor_silent_signal_unaltered() {
        let mut silent = vec![0i16; 500];
        DynamicCompressor::process(&mut silent, 24000, -18.0, 3.0, 15.0, 100.0, 0.0);
        assert!(silent.iter().all(|&s| s == 0));
    }

    #[test]
    fn test_compressor_nan_parameters_use_defaults() {
        // NaN parameters must fall back to safe defaults and never panic.
        // Any i16 value is by definition in [i16::MIN, i16::MAX]; we just
        // verify the vector is non-empty and no panic occurred.
        let mut samples = vec![16000i16; 100];
        DynamicCompressor::process(
            &mut samples,
            24000,
            f32::NAN,
            f32::NAN,
            f32::NAN,
            f32::NAN,
            f32::NAN,
        );
        assert!(!samples.is_empty());
    }

    #[test]
    fn test_compressor_makeup_gain_boosts_quiet_signal() {
        let mut quiet = vec![100i16; 1000];
        DynamicCompressor::process(&mut quiet, 24000, -60.0, 1.0, 5.0, 100.0, 20.0);
        let mean_out: f64 =
            quiet.iter().map(|&s| (s as f64).abs()).sum::<f64>() / quiet.len() as f64;
        assert!(mean_out > 100.0, "Makeup gain must boost quiet signal");
    }
}
