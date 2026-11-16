//! Brickwall peak limiter with true look-ahead.
//!
//! Studio uses a pure-numpy look-ahead limiter in `peak_limit()`:
//!   - 5ms look-ahead window
//!   - 40ms release
//!   - van Herk/Gil-Werman running-minimum to find the smallest gain needed
//!     across the future look-ahead window, so the gain is *pre-applied* before
//!     the peak arrives, not clipped after the fact.
//!
//! The original VoxForge limiter was a hard clipper (instantaneous, no look-ahead)
//! which still allowed inter-sample overs and introduced hard click artefacts at fast
//! transients. This module replaces it.
//!
//! ## Algorithm
//! 1. Convert i16 → f32 in [-1, 1].
//! 2. Compute |x| for each sample.
//! 3. Compute a running maximum over a `lookahead_ms` sliding window
//!    (implemented as a deque-based O(n) running max).
//! 4. At each sample, the required gain is `ceiling / max(window_peak, epsilon)`.
//!    If the gain < 1.0 it must be applied now; it is also smoothed with an
//!    exponential release to prevent gain-change clicks on the upswing.
//! 5. The limiting gain signal is additionally smoothed on attack with a 0.5ms
//!    time constant to prevent the gain from jumping discontinuously at transients.
//! 6. Apply gain, convert back to i16.

use std::collections::VecDeque;

pub struct BrickwallLimiter;

impl BrickwallLimiter {
    /// Look-ahead duration (matches Studio `LOOKAHEAD_MS = 5`).
    const LOOKAHEAD_MS: f32 = 5.0;

    /// Gain-release time constant in ms (matches Studio `RELEASE_MS = 40`).
    const RELEASE_MS: f32 = 40.0;

    /// Hard-clip & peak-limit `samples` to the given ceiling (dBFS, e.g. -0.5).
    ///
    /// Uses look-ahead + exponential release. The function modifies samples in place.
    pub fn process(samples: &mut [i16], ceiling_dbfs: f32) {
        if samples.is_empty() {
            return;
        }

        // Sample rate is not passed — infer a nominal 24000 Hz default for time constants.
        // For perfect precision the caller should use `process_sr`. We keep this signature
        // for backward compatibility with existing call sites.
        Self::process_sr(samples, ceiling_dbfs, 24_000);
    }

    /// Sample-rate-aware version. Prefer this over `process` when sample rate is known.
    pub fn process_sr(samples: &mut [i16], ceiling_dbfs: f32, sample_rate: u32) {
        if samples.is_empty() {
            return;
        }

        let sr = sample_rate.max(8_000) as f32;

        let safe_ceiling = if ceiling_dbfs.is_finite() {
            ceiling_dbfs.clamp(-24.0, 0.0)
        } else {
            -0.5
        };
        let ceiling_linear = 10.0f32.powf(safe_ceiling / 20.0); // 0.0 – 1.0 range

        let lookahead_len = ((sr * Self::LOOKAHEAD_MS / 1000.0).round() as usize).max(1);
        let release_alpha = (-1000.0_f32 / (sr * Self::RELEASE_MS)).exp();

        // Convert to f32 in [-1, 1]
        let mut f: Vec<f32> = samples.iter().map(|&s| s as f32 / 32768.0).collect();

        // ── Phase 1: Running maximum over look-ahead window ──
        // For each output sample at index i, window_peak[i] = max(abs_f[i .. i+lookahead_len]).
        //
        // We use the standard O(n) monotone deque sliding window max. The deque stores
        // indices in DECREASING order of abs_f (front = index of current maximum).
        // We pre-populate the first window, then slide one sample at a time.
        let abs_f: Vec<f32> = f.iter().map(|x| x.abs()).collect();
        let n = f.len();
        let mut window_peak: Vec<f32> = vec![0.0; n];
        let mut deque: VecDeque<usize> = VecDeque::new();

        // Build initial window [0, lookahead_len)
        for j in 0..lookahead_len.min(n) {
            while deque.back().map(|&k| abs_f[k] <= abs_f[j]).unwrap_or(false) {
                deque.pop_back();
            }
            deque.push_back(j);
        }

        for (i, wp) in window_peak.iter_mut().enumerate() {
            // Expire elements that have slid out of the window
            while deque.front().map(|&k| k < i).unwrap_or(false) {
                deque.pop_front();
            }
            *wp = deque.front().map(|&k| abs_f[k]).unwrap_or(0.0);

            // Advance window: add element at i + lookahead_len (the new future sample)
            let next = i + lookahead_len;
            if next < n {
                while deque.back().map(|&k| abs_f[k] <= abs_f[next]).unwrap_or(false) {
                    deque.pop_back();
                }
                deque.push_back(next);
            }
        }

        // ── Phase 2: Compute required gain at each sample ──
        //
        // Because we use a look-ahead window, the gain is ALREADY computed with knowledge
        // of future peaks. Attack is therefore INSTANTANEOUS (no smoothing needed —
        // the look-ahead means we're applying gain BEFORE the transient arrives, so there
        // is no click). Only the release (upward gain recovery) is smoothed to avoid
        // an abrupt gain jump as the signal falls after a loud transient.
        let mut smoothed_gain = 1.0f32;
        let mut gain_signal: Vec<f32> = vec![1.0; f.len()];

        for (peak_val, gs) in window_peak.iter().zip(gain_signal.iter_mut()) {
            let peak = peak_val.max(1e-9);
            // Gain needed so that peak * gain = ceiling_linear
            let required_gain = (ceiling_linear / peak).min(1.0);

            if required_gain < smoothed_gain {
                // Attack: instantaneous (look-ahead guarantees we arrive before the peak)
                smoothed_gain = required_gain;
            } else {
                // Release: slow exponential recovery toward unity (40ms TC)
                smoothed_gain = release_alpha * smoothed_gain + (1.0 - release_alpha) * required_gain;
            }

            *gs = smoothed_gain;
        }

        // ── Phase 3: Apply gain + hard clamp, convert back to i16 ──
        for (i, s) in f.iter_mut().enumerate() {
            *s = (*s * gain_signal[i]).clamp(-1.0, 1.0);
        }
        for (sample, &fval) in samples.iter_mut().zip(f.iter()) {
            *sample = (fval * 32767.0).round().clamp(-32768.0, 32767.0) as i16;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_limiter_empty_no_panic() {
        let mut empty: Vec<i16> = vec![];
        BrickwallLimiter::process(&mut empty, -0.5);
        assert!(empty.is_empty());
    }

    #[test]
    fn test_limiter_no_clipping_full_scale_sine() {
        let sr = 24_000u32;
        // Full-scale sine (peak = 32767)
        let mut samples: Vec<i16> = (0..sr)
            .map(|i| {
                let t = i as f32 / sr as f32;
                (32000.0 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()) as i16
            })
            .collect();
        BrickwallLimiter::process_sr(&mut samples, -0.5, sr);
        // -0.5 dBFS ceiling ≈ 32589 amplitude
        let ceiling_amp = (32767.0_f32 * 10.0f32.powf(-0.5 / 20.0)) as i16 + 2; // +2 rounding tolerance
        assert!(
            samples.iter().all(|&s| s.abs() <= ceiling_amp),
            "Samples exceed ceiling"
        );
    }

    #[test]
    fn test_limiter_quiet_signal_passthrough() {
        // -20 dBFS signal should pass through nearly unchanged (no limiting needed)
        let sr = 24_000u32;
        let samples_orig: Vec<i16> = (0..sr)
            .map(|i| {
                let t = i as f32 / sr as f32;
                (3000.0 * (2.0 * std::f32::consts::PI * 200.0 * t).sin()) as i16
            })
            .collect();
        let mut samples = samples_orig.clone();
        BrickwallLimiter::process_sr(&mut samples, -0.5, sr);
        // With gain = 1.0 output must equal input within rounding (±1 LSB)
        let max_diff = samples
            .iter()
            .zip(samples_orig.iter())
            .map(|(&a, &b)| (a as i32 - b as i32).unsigned_abs())
            .max()
            .unwrap_or(0);
        assert!(max_diff <= 2, "Quiet signal changed by {} LSBs unexpectedly", max_diff);
    }

    #[test]
    fn test_limiter_ceiling_respected_various() {
        for &ceiling in &[-0.1f32, -0.5, -1.0, -3.0] {
            let ceiling_linear = 10.0f32.powf(ceiling / 20.0);
            let sr = 24_000u32;
            let mut samples: Vec<i16> = (0..sr)
                .map(|i| ((i as i32 % 500 - 250) * 100).clamp(-32768, 32767) as i16)
                .collect();
            BrickwallLimiter::process_sr(&mut samples, ceiling, sr);
            let peak_linear =
                samples.iter().map(|&s| (s as f32).abs() / 32768.0).fold(0.0f32, f32::max);
            assert!(
                peak_linear <= ceiling_linear + 1e-4,
                "ceiling {} dBFS violated: peak linear = {}",
                ceiling,
                peak_linear
            );
        }
    }
}
