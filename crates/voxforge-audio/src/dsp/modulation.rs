//! Pure-Rust expressive prosody & speaker modulation transfer.
//!
//! Bridges monotonic cross-lingual base synthesis with speaker-specific modulation characteristics
//! extracted by [`ProsodyMirror`](crate::dsp::ProsodyMirror) and user delivery directives:
//! 1. Voiced-only Hann overlap-add pitch-contour & intonation-span scaling (`f0_std_semitones`, `f0_contour_8`).
//! 2. Syllabic stress dynamic contrast expansion/compression (`energy_dynamic_range_db`).
//! 3. Spectral tilt balance matching (`spectral_tilt_db`).
//! 4. Breathy aspiration air mix for intimate/whispered delivery (`hnr_db` / `instruct`).

use std::f32::consts::PI;

use crate::dsp::eq::Biquad;
use crate::dsp::prosody::AcousticFeatures;
use crate::dsp::realism::FastRng;

/// Acoustic modulation targets parsed from `request.instruct` delivery directives.
#[derive(Debug, Clone, PartialEq)]
pub struct InstructModulation {
    pub pitch_offset_st: f32,
    pub pitch_range_scale: f32,
    pub rate_scale: f32,
    pub energy_contrast: f32,
    pub spectral_tilt_delta_db: f32,
    pub breathiness_mix: f32,
    pub pause_scale: f32,
    pub target_lufs: f32,
}

impl Default for InstructModulation {
    fn default() -> Self {
        Self {
            pitch_offset_st: 0.0,
            pitch_range_scale: 1.0,
            rate_scale: 1.0,
            energy_contrast: 1.0,
            spectral_tilt_delta_db: 0.0,
            breathiness_mix: 0.0,
            pause_scale: 1.0,
            target_lufs: -16.0,
        }
    }
}

impl InstructModulation {
    /// Parse free-form delivery instructions (`"Whisper"`, `"Urgent"`, `"Cheerful"`,
    /// `"Storyteller"`, `"Authoritative"`, etc.) into bounded acoustic steering targets.
    pub fn from_str_opt(instruct: Option<&str>) -> Self {
        let mut cfg = Self::default();
        let Some(raw) = instruct else {
            return cfg;
        };
        let lower = raw.trim().to_ascii_lowercase();
        if lower.is_empty() {
            return cfg;
        }

        if lower.contains("whisper") || lower.contains("intimate") || lower.contains("soft") {
            cfg.pitch_range_scale *= 0.72;
            cfg.rate_scale *= 0.90;
            cfg.energy_contrast *= 0.85;
            cfg.spectral_tilt_delta_db += 2.0; // warm fundamentals, gentler bite
            cfg.breathiness_mix += 0.10;
            cfg.pause_scale *= 1.18;
            cfg.target_lufs = -20.5;
        }
        if lower.contains("urgent") || lower.contains("fast") || lower.contains("excited") {
            cfg.pitch_offset_st += 0.8;
            cfg.pitch_range_scale *= 1.30;
            cfg.rate_scale *= 1.14;
            cfg.energy_contrast *= 1.25;
            cfg.spectral_tilt_delta_db -= 1.8; // brighter articulation
            cfg.pause_scale *= 0.75;
            cfg.target_lufs = -14.8;
        }
        if lower.contains("cheerful") || lower.contains("happy") || lower.contains("smiling") {
            cfg.pitch_offset_st += 1.0;
            cfg.pitch_range_scale *= 1.28;
            cfg.rate_scale *= 1.05;
            cfg.energy_contrast *= 1.18;
            cfg.spectral_tilt_delta_db -= 2.2;
        }
        if lower.contains("storyteller")
            || lower.contains("narrative")
            || lower.contains("dramatic")
        {
            cfg.pitch_range_scale *= 1.35;
            cfg.rate_scale *= 0.92;
            cfg.energy_contrast *= 1.22;
            cfg.pause_scale *= 1.32;
        }
        if lower.contains("authoritative") || lower.contains("deep") || lower.contains("command") {
            cfg.pitch_offset_st -= 1.2;
            cfg.pitch_range_scale *= 0.90;
            cfg.rate_scale *= 0.94;
            cfg.energy_contrast *= 1.15;
            cfg.spectral_tilt_delta_db += 2.4;
            cfg.pause_scale *= 1.12;
        }
        if lower.contains("calm") || lower.contains("sad") || lower.contains("somber") {
            cfg.pitch_offset_st -= 0.7;
            cfg.pitch_range_scale *= 0.76;
            cfg.rate_scale *= 0.88;
            cfg.energy_contrast *= 0.84;
            cfg.spectral_tilt_delta_db += 1.6;
            cfg.pause_scale *= 1.28;
        }

        cfg.pitch_offset_st = cfg.pitch_offset_st.clamp(-4.0, 4.0);
        cfg.pitch_range_scale = cfg.pitch_range_scale.clamp(0.55, 1.65);
        cfg.rate_scale = cfg.rate_scale.clamp(0.78, 1.25);
        cfg.energy_contrast = cfg.energy_contrast.clamp(0.72, 1.45);
        cfg.spectral_tilt_delta_db = cfg.spectral_tilt_delta_db.clamp(-4.5, 4.5);
        cfg.breathiness_mix = cfg.breathiness_mix.clamp(0.0, 0.18);
        cfg.pause_scale = cfg.pause_scale.clamp(0.65, 1.55);
        cfg.target_lufs = cfg.target_lufs.clamp(-22.0, -13.5);
        cfg
    }
}

pub struct ModulationTransfer;

impl ModulationTransfer {
    /// Apply reference-conditioned prosody modulation and instruct steering in pure Rust.
    pub fn apply(
        pcm: &mut [i16],
        sample_rate: u32,
        ref_features: Option<&AcousticFeatures>,
        instruct: Option<&str>,
    ) {
        if pcm.len() < 512 || sample_rate < 8000 {
            return;
        }

        let inst = InstructModulation::from_str_opt(instruct);

        // 1. Pitch-contour and intonation-span shaping strictly on voiced regions
        Self::apply_voiced_f0_modulation(pcm, sample_rate, ref_features, &inst);

        // 2. Syllabic stress dynamic range shaping
        Self::apply_syllabic_energy_contrast(pcm, sample_rate, ref_features, &inst);

        // 3. Spectral tilt balance matching (warmth vs crispness)
        Self::apply_spectral_tilt_match(pcm, sample_rate, ref_features, &inst);

        // 4. Optional aspirative breathiness for whisper / low-HNR speakers
        let hnr_breath = ref_features
            .map(|f| ((15.0 - f.hnr_db) * 0.008).clamp(0.0, 0.06))
            .unwrap_or(0.0);
        let breath_mix = (inst.breathiness_mix + hnr_breath).clamp(0.0, 0.18);
        if breath_mix > 0.01 {
            Self::apply_whisper_air(pcm, sample_rate, breath_mix);
        }
    }

    fn apply_voiced_f0_modulation(
        pcm: &mut [i16],
        sample_rate: u32,
        ref_features: Option<&AcousticFeatures>,
        inst: &InstructModulation,
    ) {
        let sr = sample_rate as f32;
        let frame_len = ((sr * 0.030) as usize).max(240);
        let hop_len = (frame_len / 3).max(80);
        if pcm.len() <= frame_len + hop_len {
            return;
        }

        let min_lag = (sr / 420.0).round() as usize;
        let max_lag = (sr / 65.0).round() as usize;

        struct FramePitch {
            center: usize,
            f0_hz: Option<f32>,
        }

        let mut frames = Vec::new();
        let mut f0_vals = Vec::new();
        let mut pos = 0usize;
        while pos + frame_len <= pcm.len() {
            let slice = &pcm[pos..pos + frame_len];
            let f0_opt = detect_frame_f0(slice, sr, min_lag, max_lag);
            if let Some(f0) = f0_opt {
                f0_vals.push(f0);
            }
            frames.push(FramePitch {
                center: pos + frame_len / 2,
                f0_hz: f0_opt,
            });
            pos += hop_len;
        }

        if f0_vals.len() < 3 {
            return;
        }

        let mut sorted = f0_vals.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let syn_mean = sorted[sorted.len() / 2].max(60.0);
        let syn_var = f0_vals
            .iter()
            .map(|&f| {
                let st = 12.0 * (f / syn_mean).log2();
                st * st
            })
            .sum::<f32>()
            / f0_vals.len() as f32;
        let syn_std = syn_var.sqrt().max(0.75);

        let base_span_scale = ref_features
            .map(|rf| {
                if rf.f0_std_semitones > 0.25 {
                    (rf.f0_std_semitones / syn_std).clamp(0.65, 1.55)
                } else {
                    0.85
                }
            })
            .unwrap_or(1.0);
        let span_scale = (base_span_scale * inst.pitch_range_scale).clamp(0.55, 1.65);
        let contour = ref_features
            .map(|rf| rf.f0_contour_8.as_slice())
            .unwrap_or(&[]);

        // Skip grain overlap-add if negligible modulation delta is needed
        if (span_scale - 1.0).abs() < 0.04 && inst.pitch_offset_st.abs() < 0.15 && contour.len() < 4
        {
            return;
        }

        let mut frame_shifts = Vec::with_capacity(frames.len());
        let total_len = pcm.len() as f32;
        for fr in &frames {
            if let Some(f0) = fr.f0_hz {
                let dev_st = 12.0 * (f0 / syn_mean).log2();
                let progress = (fr.center as f32 / total_len).clamp(0.0, 1.0);
                let contour_bias =
                    sample_contour(contour, progress) * 0.45 * inst.pitch_range_scale;
                let desired_dev = dev_st * span_scale + contour_bias + inst.pitch_offset_st * 0.5;
                frame_shifts.push((desired_dev - dev_st).clamp(-3.5, 3.5));
            } else {
                frame_shifts.push(0.0);
            }
        }

        // Overlap-add Hann grains preserving exact output length
        let half = frame_len / 2;
        let window: Vec<f32> = (0..frame_len)
            .map(|k| 0.5 - 0.5 * (2.0 * PI * k as f32 / (frame_len - 1) as f32).cos())
            .collect();

        let mut accum = vec![0.0f32; pcm.len()];
        let mut weights = vec![0.0f32; pcm.len()];

        for (idx, fr) in frames.iter().enumerate() {
            let shift_st = frame_shifts[idx];
            let g_start = fr.center.saturating_sub(half);
            let g_end = (g_start + frame_len).min(pcm.len());
            let valid_len = g_end - g_start;

            if shift_st.abs() < 0.10 {
                for k in 0..valid_len {
                    let w = window[k];
                    accum[g_start + k] += pcm[g_start + k] as f32 * w;
                    weights[g_start + k] += w;
                }
            } else {
                let ratio = 2.0f32.powf(shift_st / 12.0);
                let c_f = fr.center as f32;
                let max_idx = (pcm.len() - 1) as f32;
                for k in 0..valid_len {
                    let offset = (g_start + k) as f32 - c_f;
                    let src_pos = (c_f + offset * ratio).clamp(0.0, max_idx);
                    let i0 = src_pos.floor() as usize;
                    let i1 = (i0 + 1).min(pcm.len() - 1);
                    let frac = src_pos - i0 as f32;
                    let interp = pcm[i0] as f32 + frac * (pcm[i1] as f32 - pcm[i0] as f32);
                    let w = window[k];
                    accum[g_start + k] += interp * w;
                    weights[g_start + k] += w;
                }
            }
        }

        for i in 0..pcm.len() {
            if weights[i] > 1e-4 {
                pcm[i] = (accum[i] / weights[i]).clamp(-32767.0, 32767.0) as i16;
            }
        }
    }

    fn apply_syllabic_energy_contrast(
        pcm: &mut [i16],
        sample_rate: u32,
        ref_features: Option<&AcousticFeatures>,
        inst: &InstructModulation,
    ) {
        let base_contrast = ref_features
            .map(|rf| (rf.energy_dynamic_range_db / 8.0).clamp(0.75, 1.45))
            .unwrap_or(1.0);
        let target_contrast = (base_contrast * inst.energy_contrast).clamp(0.72, 1.48);
        let alpha = (target_contrast - 1.0).clamp(-0.28, 0.42);
        if alpha.abs() < 0.03 {
            return;
        }

        let sr = sample_rate as f32;
        let short_coeff = (-1.0 / (0.040 * sr)).exp();
        let long_coeff = (-1.0 / (0.450 * sr)).exp();

        let mut short_sq = 0.0f32;
        let mut long_sq = 1e-6f32;
        let mut gain_smooth = 1.0f32;
        let smooth_coeff = (-1.0 / (0.025 * sr)).exp();

        for s in pcm.iter_mut() {
            let x = *s as f32 / 32768.0;
            let e = x * x;
            short_sq = short_coeff * short_sq + (1.0 - short_coeff) * e;
            long_sq = long_coeff * long_sq + (1.0 - long_coeff) * e;

            let short_rms = short_sq.sqrt();
            let long_rms = long_sq.sqrt().max(1e-4);

            let desired_gain = if short_rms > 0.012 {
                (short_rms / long_rms).powf(alpha).clamp(0.65, 1.55)
            } else {
                1.0
            };
            gain_smooth = smooth_coeff * gain_smooth + (1.0 - smooth_coeff) * desired_gain;

            let y = (x * gain_smooth * 32768.0).clamp(-32767.0, 32767.0);
            *s = y as i16;
        }
    }

    fn apply_spectral_tilt_match(
        pcm: &mut [i16],
        sample_rate: u32,
        ref_features: Option<&AcousticFeatures>,
        inst: &InstructModulation,
    ) {
        let syn_tilt = estimate_spectral_tilt(pcm);
        let ref_delta = ref_features
            .map(|rf| (rf.spectral_tilt_db - syn_tilt).clamp(-4.0, 4.0) * 0.60)
            .unwrap_or(0.0);
        let total_tilt = (ref_delta + inst.spectral_tilt_delta_db).clamp(-4.5, 4.5);
        if total_tilt.abs() < 0.25 {
            return;
        }

        let sr = sample_rate as f32;
        let mut low = Biquad::low_shelf(sr, 220.0, total_tilt * 0.45);
        let mut high = Biquad::high_shelf(sr, 4000.0, -total_tilt * 0.55);

        for s in pcm.iter_mut() {
            let mut x = *s as f32 / 32768.0;
            x = low.process(x);
            x = high.process(x);
            *s = (x * 32768.0).clamp(-32767.0, 32767.0) as i16;
        }
    }

    fn apply_whisper_air(pcm: &mut [i16], sample_rate: u32, mix: f32) {
        let sr = sample_rate as f32;
        let mut bp = Biquad::band_pass(sr, 1600.0, 1.1);
        let mut rng = FastRng::new(0xA1B2_C3D4_E5F6_0708);
        let env_coeff = (-1.0 / (0.020 * sr)).exp();
        let mut env = 0.0f32;

        for s in pcm.iter_mut() {
            let x = *s as f32 / 32768.0;
            env = env_coeff * env + (1.0 - env_coeff) * x.abs();
            let noise = rng.next_bipolar() * 0.35;
            let shaped_air = bp.process(noise) * (env * 2.4).min(1.0);
            let dry_attenuate = 1.0 - mix * 0.45;
            let blended = x * dry_attenuate + shaped_air * mix;
            *s = (blended * 32768.0).clamp(-32767.0, 32767.0) as i16;
        }
    }
}

fn detect_frame_f0(frame: &[i16], sample_rate: f32, min_lag: usize, max_lag: usize) -> Option<f32> {
    let mut sum_sq = 0.0f64;
    for &s in frame {
        let v = s as f64 / 32768.0;
        sum_sq += v * v;
    }
    let rms = (sum_sq / frame.len() as f64).sqrt();
    if rms < 0.008 {
        return None;
    }

    let limit = max_lag.min(frame.len() / 2);
    if limit <= min_lag + 1 {
        return None;
    }

    let mut best_lag = 0usize;
    let mut best_corr = -1.0f64;
    let mut corrs = vec![0.0f64; limit + 1];

    for lag in min_lag..=limit {
        let mut num = 0.0f64;
        let mut e1 = 0.0f64;
        let mut e2 = 0.0f64;
        for i in 0..(frame.len() - lag) {
            let s1 = frame[i] as f64 / 32768.0;
            let s2 = frame[i + lag] as f64 / 32768.0;
            num += s1 * s2;
            e1 += s1 * s1;
            e2 += s2 * s2;
        }
        let den = (e1 * e2).sqrt().max(1e-12);
        let norm_r = num / den;
        corrs[lag] = norm_r;
        if norm_r > best_corr {
            best_corr = norm_r;
            best_lag = lag;
        }
    }

    if best_corr > 0.42 && best_lag > min_lag && best_lag < limit {
        let r_prev = corrs[best_lag - 1];
        let r_curr = corrs[best_lag];
        let r_next = corrs[best_lag + 1];
        let denom = r_prev - 2.0 * r_curr + r_next;
        let refined = if denom.abs() > 1e-9 {
            best_lag as f64 + 0.5 * (r_prev - r_next) / denom
        } else {
            best_lag as f64
        };
        let f0 = sample_rate / refined.max(min_lag as f64) as f32;
        if (65.0..=420.0).contains(&f0) {
            return Some(f0);
        }
    }
    None
}

fn sample_contour(contour: &[f32], progress: f32) -> f32 {
    if contour.is_empty() {
        return 0.0;
    }
    if contour.len() == 1 {
        return contour[0];
    }
    let pos = progress.clamp(0.0, 1.0) * (contour.len() - 1) as f32;
    let idx0 = pos.floor() as usize;
    let idx1 = (idx0 + 1).min(contour.len() - 1);
    let frac = pos - idx0 as f32;
    contour[idx0] + frac * (contour[idx1] - contour[idx0])
}

fn estimate_spectral_tilt(pcm: &[i16]) -> f32 {
    if pcm.len() < 256 {
        return 0.0;
    }
    let mut low_e = 0.0f64;
    let mut high_e = 0.0f64;
    let n = pcm.len().min(48000);
    for i in 2..n {
        let s = pcm[i] as f64;
        let s1 = pcm[i - 1] as f64;
        let s2 = pcm[i - 2] as f64;
        let low = (s + 2.0 * s1 + s2) * 0.25;
        let high = s - s1;
        low_e += low * low;
        high_e += high * high;
    }
    (10.0 * (low_e / high_e.max(1.0)).log10()).clamp(-12.0, 12.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::prosody::ProsodyMirror;

    fn make_modulated_tone(
        sr: u32,
        secs: f32,
        base_f0: f32,
        swing_hz: f32,
        amp_depth: f32,
    ) -> Vec<i16> {
        let len = (sr as f32 * secs) as usize;
        let mut out = Vec::with_capacity(len);
        let mut phase = 0.0f32;
        for i in 0..len {
            let t = i as f32 / sr as f32;
            let f0 = base_f0 + swing_hz * (2.0 * PI * 1.4 * t).sin();
            let env = 1.0 - amp_depth * (0.5 - 0.5 * (2.0 * PI * 2.5 * t).cos());
            phase += 2.0 * PI * f0 / sr as f32;
            let s = (phase.sin() * 14000.0 + (phase * 2.0).sin() * 4000.0) * env;
            out.push(s.clamp(-32767.0, 32767.0) as i16);
        }
        out
    }

    #[test]
    fn test_instruct_modulation_keywords() {
        let whisper = InstructModulation::from_str_opt(Some("whisper softly"));
        assert!(whisper.breathiness_mix > 0.05);
        assert!(whisper.target_lufs < -18.0);
        assert!(whisper.pitch_range_scale < 0.85);

        let urgent = InstructModulation::from_str_opt(Some("Urgent excited breaking news"));
        assert!(urgent.rate_scale > 1.05);
        assert!(urgent.pitch_range_scale > 1.15);
        assert!(urgent.pause_scale < 0.90);
    }

    #[test]
    fn test_modulation_transfer_preserves_length_and_separates_expressive_vs_calm() {
        let sr = 24000;
        let base_pcm = make_modulated_tone(sr, 1.5, 160.0, 22.0, 0.25);

        let calm_ref = make_modulated_tone(sr, 1.5, 150.0, 4.0, 0.05);
        let expr_ref = make_modulated_tone(sr, 1.5, 165.0, 48.0, 0.55);

        let calm_feat = ProsodyMirror::analyze(&calm_ref, sr);
        let expr_feat = ProsodyMirror::analyze(&expr_ref, sr);

        let mut calm_out = base_pcm.clone();
        ModulationTransfer::apply(&mut calm_out, sr, Some(&calm_feat), Some("calm"));

        let mut expr_out = base_pcm.clone();
        ModulationTransfer::apply(
            &mut expr_out,
            sr,
            Some(&expr_feat),
            Some("cheerful storytelling"),
        );

        assert_eq!(calm_out.len(), base_pcm.len());
        assert_eq!(expr_out.len(), base_pcm.len());

        let calm_out_feat = ProsodyMirror::analyze(&calm_out, sr);
        let expr_out_feat = ProsodyMirror::analyze(&expr_out, sr);

        assert!(
            expr_out_feat.f0_std_semitones > calm_out_feat.f0_std_semitones,
            "Expressive modulation std ({:.2} st) should exceed calm std ({:.2} st)",
            expr_out_feat.f0_std_semitones,
            calm_out_feat.f0_std_semitones
        );
    }
}
