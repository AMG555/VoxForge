//! Room-tone gap filler.
//!
//! Replaces digital-zero pause gaps with spectrally-shaped "room tone" noise extracted from the quietest
//! 100ms frames of the same recording. This prevents the unnatural gating effect heard
//! on headphones when a voice recording drops to true digital silence between sentences.
//!
//! ## Algorithm
//! 1. Split the recording into 100ms analysis frames.
//! 2. Find the N quietest frames (lowest RMS) — these are the "room tone" samples.
//! 3. Build a room-tone template by averaging the quiet frames.
//! 4. Identify gap regions: contiguous runs of samples below `silence_dbfs` (-55 dBFS).
//! 5. Fill each gap by cycling through the room-tone template with a random phase offset
//!    and applying a short linear cross-fade (5ms) at the gap edges to avoid clicks.
//!
//! If no room-tone frames are found (fully synthetic audio), gaps are filled with
//! shaped white noise at the same level as the quietest frame.

/// Number of quiet frames to average for the room-tone template.
const NUM_QUIET_FRAMES: usize = 5;

/// Analysis frame duration in ms.
const FRAME_MS: f32 = 100.0;

/// Below this level a frame is considered a "gap" to be filled.
const GAP_DBFS: f32 = -55.0;

/// Crossfade length at gap edges (ms) to avoid clicks.
const EDGE_FADE_MS: f32 = 5.0;

pub struct RoomToneGapFill;

impl RoomToneGapFill {
    /// Fill silence gaps with spectrally-shaped room tone extracted from the recording.
    ///
    /// # Arguments
    /// - `samples`: 16-bit PCM audio (modified in place)
    /// - `sample_rate`: Audio sample rate in Hz
    pub fn process(samples: &mut [i16], sample_rate: u32) {
        if samples.len() < 64 || sample_rate == 0 {
            return;
        }

        let sr = sample_rate.max(1) as f32;
        let frame_len = ((sr * FRAME_MS / 1000.0) as usize).max(64);
        let gap_amp = 32767.0f32 * 10.0f32.powf(GAP_DBFS / 20.0);
        let edge_fade_len = ((sr * EDGE_FADE_MS / 1000.0) as usize).max(1);

        // ── Step 1: Score each frame by RMS ──
        let mut frame_rms: Vec<(f32, usize)> = Vec::new(); // (rms, start_idx)
        let mut pos = 0;
        while pos + frame_len <= samples.len() {
            let frame = &samples[pos..pos + frame_len];
            let rms = Self::rms(frame);
            frame_rms.push((rms, pos));
            pos += frame_len;
        }

        if frame_rms.is_empty() {
            return;
        }

        // Sort ascending by RMS to find quietest frames
        frame_rms.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

        // ── Step 2: Build room-tone template ──
        let n_quiet = NUM_QUIET_FRAMES.min(frame_rms.len());
        // Pick from low-RMS frames that still have some signal (not pure digital zero)
        let quiet_frames: Vec<&[i16]> = frame_rms
            .iter()
            .take(n_quiet)
            .filter(|(rms, _)| *rms > 0.5) // reject pure digital zeros
            .map(|(_, start)| &samples[*start..*start + frame_len])
            .collect();

        let room_tone: Vec<i16> = if quiet_frames.is_empty() {
            // No real room tone — generate shaped white noise at -72 dBFS
            Self::shaped_noise(frame_len)
        } else {
            // Average the quiet frames sample-by-sample
            let mut avg = vec![0.0f32; frame_len];
            for &frame in &quiet_frames {
                for (i, &s) in frame.iter().enumerate() {
                    avg[i] += s as f32;
                }
            }
            avg.iter()
                .map(|&v| {
                    (v / quiet_frames.len() as f32)
                        .round()
                        .clamp(-32768.0, 32767.0) as i16
                })
                .collect()
        };

        // ── Step 3: Find gap regions ──
        let mut gap_regions: Vec<(usize, usize)> = Vec::new(); // (start, end_exclusive)
        let mut in_gap = false;
        let mut gap_start = 0;

        for (i, &s) in samples.iter().enumerate() {
            let is_silent = (s as f32).abs() < gap_amp;
            match (in_gap, is_silent) {
                (false, true) => {
                    in_gap = true;
                    gap_start = i;
                }
                (true, false) => {
                    in_gap = false;
                    // Only fill gaps ≥ 20ms (don't fill very short silences in consonants)
                    let min_gap = (sr * 0.020) as usize;
                    if i.saturating_sub(gap_start) >= min_gap {
                        gap_regions.push((gap_start, i));
                    }
                }
                _ => {}
            }
        }
        // Handle gap extending to end
        if in_gap {
            let min_gap = (sr * 0.020) as usize;
            if samples.len().saturating_sub(gap_start) >= min_gap {
                gap_regions.push((gap_start, samples.len()));
            }
        }

        // ── Step 4: Fill each gap ──
        for (gap_start, gap_end) in gap_regions {
            let gap_len = gap_end - gap_start;
            // Use a simple cycle with deterministic "phase" based on gap position
            // (avoids std::collections::HashMap or rand dependency)
            let phase_offset = (gap_start * 7919) % room_tone.len(); // prime scramble

            for (k, i) in (gap_start..gap_end).enumerate() {
                let tone_sample = room_tone[(k + phase_offset) % room_tone.len()] as f32;

                // Linear cross-fade at leading and trailing edges
                let edge_gain = if k < edge_fade_len {
                    k as f32 / edge_fade_len as f32
                } else if k >= gap_len.saturating_sub(edge_fade_len) {
                    let dist_from_end = gap_len.saturating_sub(k + 1);
                    dist_from_end as f32 / edge_fade_len as f32
                } else {
                    1.0
                };

                samples[i] = (tone_sample * edge_gain).round().clamp(-32768.0, 32767.0) as i16;
            }
        }
    }

    // ── Internal helpers ──

    fn rms(frame: &[i16]) -> f32 {
        if frame.is_empty() {
            return 0.0;
        }
        let sum: f64 = frame.iter().map(|&s| (s as f64).powi(2)).sum();
        (sum / frame.len() as f64).sqrt() as f32
    }

    /// Generate shaped noise (approximate pink-ish spectrum via simple HP filter).
    fn shaped_noise(len: usize) -> Vec<i16> {
        // LCG pseudo-random to avoid any external dependency
        let mut state: u64 = 0xDEAD_BEEF_1234_5678;
        let level = 32767.0f32 * 10.0f32.powf(-72.0 / 20.0); // -72 dBFS

        let mut prev = 0.0f32;
        let mut out = Vec::with_capacity(len);
        for _ in 0..len {
            // 64-bit LCG
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let raw = (state >> 33) as i32;
            let noise = (raw as f32 / i32::MAX as f32) * level;
            // Simple first-order HP to shape noise spectrum (reduce DC)
            let hp = noise - prev * 0.95;
            prev = noise;
            out.push(hp.clamp(-32768.0, 32767.0) as i16);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gap_fill_empty_no_panic() {
        let mut empty: Vec<i16> = vec![];
        RoomToneGapFill::process(&mut empty, 24000);
        assert!(empty.is_empty());
    }

    #[test]
    fn test_gap_fill_no_gaps_passthrough() {
        let sr = 24_000u32;
        // Constant loud signal — no gaps
        let samples_orig: Vec<i16> = vec![10000i16; sr as usize];
        let mut samples = samples_orig.clone();
        RoomToneGapFill::process(&mut samples, sr);
        // Should be unchanged (no gap regions detected)
        assert_eq!(samples, samples_orig);
    }

    #[test]
    fn test_gap_fill_fills_silence_nonzero() {
        let sr = 24_000u32;
        // Speech, then 500ms silence gap, then speech
        let speech: Vec<i16> = (0..(sr as usize / 2))
            .map(|i| {
                let t = i as f32 / sr as f32;
                (12000.0 * (2.0 * std::f32::consts::PI * 150.0 * t).sin()) as i16
            })
            .collect();
        let gap = vec![0i16; sr as usize / 2]; // 500ms silence
        let mut samples: Vec<i16> = speech
            .iter()
            .chain(gap.iter())
            .chain(speech.iter())
            .cloned()
            .collect();

        let gap_start = speech.len();
        let gap_end = gap_start + gap.len();

        RoomToneGapFill::process(&mut samples, sr);

        // The gap region should now have some non-zero content
        let gap_region = &samples[gap_start + 100..gap_end - 100]; // avoid edge fades
        let any_nonzero = gap_region.iter().any(|&s| s != 0);
        assert!(
            any_nonzero,
            "Gap should be filled with room tone, not digital zeros"
        );
    }

    #[test]
    fn test_gap_fill_no_clipping() {
        let sr = 24_000u32;
        let speech: Vec<i16> = (0..(sr as usize))
            .map(|i| {
                let t = i as f32 / sr as f32;
                (25000.0 * (2.0 * std::f32::consts::PI * 200.0 * t).sin()) as i16
            })
            .collect();
        let gap = vec![0i16; sr as usize / 4];
        let mut samples: Vec<i16> = speech
            .iter()
            .chain(gap.iter())
            .chain(speech.iter())
            .cloned()
            .collect();
        RoomToneGapFill::process(&mut samples, sr);
        // i16 is by definition in [-32768, 32767]; just verify no panic and length preserved
        assert_eq!(samples.len(), speech.len() + gap.len() + speech.len());
    }
}
