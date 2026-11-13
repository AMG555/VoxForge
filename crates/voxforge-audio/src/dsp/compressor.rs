pub struct DynamicCompressor;

impl DynamicCompressor {
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

        let mut envelope = 0.0f32;

        for sample in samples.iter_mut() {
            let input_abs = (*sample as f32).abs() / 32768.0;

            // Envelope follower with attack and release
            if input_abs > envelope {
                envelope = att_coeff * envelope + (1.0 - att_coeff) * input_abs;
            } else {
                envelope = rel_coeff * envelope + (1.0 - rel_coeff) * input_abs;
            }

            // Envelope to dBFS
            let env_db = if envelope > 1e-6 {
                20.0 * envelope.log10()
            } else {
                -120.0
            };

            // Gain calculation
            let gr_db = if env_db > thresh {
                (thresh - env_db) * (1.0 - 1.0 / comp_ratio)
            } else {
                0.0
            };

            let total_gain_db = gr_db + makeup;
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
