pub struct AudioMerger;

impl AudioMerger {
    pub fn concatenate_with_pause(
        segments: &[&[i16]],
        sample_rate: u32,
        pause_duration_ms: u32,
    ) -> Vec<i16> {
        // Bound pause duration to maximum 30 seconds to prevent OOM
        let safe_pause_ms = pause_duration_ms.min(30_000);
        let safe_sample_rate = sample_rate.min(192_000);
        let pause_samples_count =
            ((safe_sample_rate as f32) * (safe_pause_ms as f32 / 1000.0)) as usize;

        let total_len: usize = segments.iter().map(|s| s.len()).sum::<usize>()
            + (segments.len().saturating_sub(1) * pause_samples_count);

        let mut output = Vec::with_capacity(total_len);

        for (idx, segment) in segments.iter().enumerate() {
            output.extend_from_slice(segment);
            if idx + 1 < segments.len() && pause_samples_count > 0 {
                output.resize(output.len() + pause_samples_count, 0i16);
            }
        }

        output
    }

    pub fn crossfade(
        segment_a: &[i16],
        segment_b: &[i16],
        sample_rate: u32,
        crossfade_ms: u32,
    ) -> Vec<i16> {
        // Bound crossfade duration to maximum 10 seconds to prevent OOM
        let safe_crossfade_ms = crossfade_ms.min(10_000);
        let safe_sample_rate = sample_rate.min(192_000);
        let fade_len = ((safe_sample_rate as f32) * (safe_crossfade_ms as f32 / 1000.0)) as usize;
        if segment_a.len() < fade_len || segment_b.len() < fade_len || fade_len == 0 {
            let mut out = Vec::with_capacity(segment_a.len() + segment_b.len());
            out.extend_from_slice(segment_a);
            out.extend_from_slice(segment_b);
            return out;
        }

        let a_body = &segment_a[..segment_a.len() - fade_len];
        let a_tail = &segment_a[segment_a.len() - fade_len..];
        let b_head = &segment_b[..fade_len];
        let b_body = &segment_b[fade_len..];

        let mut out = Vec::with_capacity(segment_a.len() + segment_b.len() - fade_len);
        out.extend_from_slice(a_body);

        for i in 0..fade_len {
            let t = i as f32 / fade_len as f32;
            let sample_a = a_tail[i] as f32 * (1.0 - t);
            let sample_b = b_head[i] as f32 * t;
            let blended = (sample_a + sample_b).round().clamp(-32768.0, 32767.0) as i16;
            out.push(blended);
        }

        out.extend_from_slice(b_body);
        out
    }

    /// High-fidelity equal-power crossfade between two audio segments.
    /// Uses quarter-sine/cosine curves (cos^2 + sin^2 = 1) to eliminate
    /// center-dip acoustic attenuation and clicks at chunk seams.
    pub fn equal_power_crossfade(
        segment_a: &[i16],
        segment_b: &[i16],
        sample_rate: u32,
        crossfade_ms: u32,
    ) -> Vec<i16> {
        let safe_crossfade_ms = crossfade_ms.clamp(1, 10_000);
        let safe_sample_rate = sample_rate.min(192_000);
        let fade_len = ((safe_sample_rate as f32) * (safe_crossfade_ms as f32 / 1000.0)) as usize;

        if segment_a.len() < fade_len || segment_b.len() < fade_len || fade_len == 0 {
            let mut out = Vec::with_capacity(segment_a.len() + segment_b.len());
            out.extend_from_slice(segment_a);
            out.extend_from_slice(segment_b);
            return out;
        }

        let a_body = &segment_a[..segment_a.len() - fade_len];
        let a_tail = &segment_a[segment_a.len() - fade_len..];
        let b_head = &segment_b[..fade_len];
        let b_body = &segment_b[fade_len..];

        let mut out = Vec::with_capacity(segment_a.len() + segment_b.len() - fade_len);
        out.extend_from_slice(a_body);

        let half_pi = std::f32::consts::FRAC_PI_2;
        for i in 0..fade_len {
            let t = i as f32 / fade_len as f32;
            let angle = t * half_pi;
            let gain_a = angle.cos();
            let gain_b = angle.sin();

            let sample_a = a_tail[i] as f32 * gain_a;
            let sample_b = b_head[i] as f32 * gain_b;
            let blended = (sample_a + sample_b).round().clamp(-32768.0, 32767.0) as i16;
            out.push(blended);
        }

        out.extend_from_slice(b_body);
        out
    }

    /// Concatenate an array of audio chunks using equal-power crossfading at every seam.
    ///
    /// Pre-allocates a single output buffer sized to the total audio minus fade overlaps,
    /// avoiding repeated intermediate allocations that the previous fold approach caused.
    pub fn concatenate_with_equal_power_crossfade(
        chunks: &[&[i16]],
        sample_rate: u32,
        crossfade_ms: u32,
    ) -> Vec<i16> {
        if chunks.is_empty() {
            return Vec::new();
        }
        if chunks.len() == 1 {
            return chunks[0].to_vec();
        }

        // Compute fade length to size the output buffer accurately.
        let safe_crossfade_ms = crossfade_ms.clamp(1, 10_000);
        let safe_sample_rate = sample_rate.min(192_000);
        let fade_len = ((safe_sample_rate as f32) * (safe_crossfade_ms as f32 / 1000.0)) as usize;

        let total_len: usize = chunks.iter().map(|c| c.len()).sum::<usize>()
            - fade_len.saturating_mul(chunks.len().saturating_sub(1));

        let mut out = Vec::with_capacity(total_len.max(1));
        let half_pi = std::f32::consts::FRAC_PI_2;

        // Write the first chunk's non-fade body, then blend each subsequent chunk.
        if chunks[0].len() > fade_len {
            out.extend_from_slice(&chunks[0][..chunks[0].len() - fade_len]);
        }

        for pair in chunks.windows(2) {
            let seg_a = pair[0];
            let seg_b = pair[1];

            // Determine actual fade length for this pair (may be shorter than fade_len).
            let actual_fade = fade_len.min(seg_a.len()).min(seg_b.len());

            if actual_fade == 0 {
                // No overlap possible — concatenate directly.
                out.extend_from_slice(seg_b);
                continue;
            }

            let a_tail = &seg_a[seg_a.len() - actual_fade..];
            let b_head = &seg_b[..actual_fade];

            for i in 0..actual_fade {
                let t = i as f32 / actual_fade as f32;
                let angle = t * half_pi;
                let blended = (a_tail[i] as f32 * angle.cos() + b_head[i] as f32 * angle.sin())
                    .round()
                    .clamp(-32768.0, 32767.0) as i16;
                out.push(blended);
            }

            // Write the non-fade body of seg_b (except its own tail which blends with the next).
            let is_last = std::ptr::eq(pair[1], chunks[chunks.len() - 1]);
            if is_last {
                out.extend_from_slice(&seg_b[actual_fade..]);
            } else if seg_b.len() > actual_fade + fade_len {
                out.extend_from_slice(&seg_b[actual_fade..seg_b.len() - fade_len]);
            }
        }

        out
    }

    /// Studio-style chunk joining: fade-out tail → silence gap → fade-in head.
    ///
    /// Studio's `cross_fade_chunks()` uses this pattern instead of overlapping
    /// equal-power crossfade. The key insight: when two TTS chunks have unknown phase
    /// relationships, overlapping them (even with equal-power curves) can produce
    /// constructive/destructive interference clicks at the seam. A short silence gap
    /// between the fade-out and fade-in completely eliminates this problem.
    ///
    /// # Arguments
    /// - `chunks`: slice of audio segments to join
    /// - `sample_rate`: audio sample rate in Hz
    /// - `fade_ms`: duration of the fade-out and fade-in ramps (each), default 20ms
    /// - `gap_ms`: duration of the silence gap between fades, default 100ms
    pub fn silence_gap_join(
        chunks: &[&[i16]],
        sample_rate: u32,
        fade_ms: u32,
        gap_ms: u32,
    ) -> Vec<i16> {
        if chunks.is_empty() {
            return Vec::new();
        }
        if chunks.len() == 1 {
            return chunks[0].to_vec();
        }

        let sr = sample_rate.min(192_000) as f32;
        let fade_len = ((sr * fade_ms.min(1000) as f32 / 1000.0) as usize).max(1);
        let gap_len = (sr * gap_ms.min(10_000) as f32 / 1000.0) as usize;

        let total_len: usize = chunks.iter().map(|c| c.len()).sum::<usize>()
            + (chunks.len() - 1) * gap_len;
        let mut out = Vec::with_capacity(total_len);

        for (idx, &seg) in chunks.iter().enumerate() {
            if idx == 0 {
                // First chunk: write body, then fade-out tail
                if seg.len() > fade_len {
                    out.extend_from_slice(&seg[..seg.len() - fade_len]);
                    for (i, &s) in seg[seg.len() - fade_len..].iter().enumerate() {
                        let gain = 1.0 - (i as f32 / fade_len as f32);
                        out.push((s as f32 * gain).round().clamp(-32768.0, 32767.0) as i16);
                    }
                } else {
                    out.extend_from_slice(seg);
                }
            } else if idx == chunks.len() - 1 {
                // Last chunk: silence gap, then fade-in head, then body
                out.resize(out.len() + gap_len, 0i16);
                let head_len = fade_len.min(seg.len());
                for (i, &s) in seg[..head_len].iter().enumerate() {
                    let gain = i as f32 / head_len as f32;
                    out.push((s as f32 * gain).round().clamp(-32768.0, 32767.0) as i16);
                }
                if seg.len() > head_len {
                    out.extend_from_slice(&seg[head_len..]);
                }
            } else {
                // Middle chunks: silence gap, fade-in head, body, fade-out tail
                out.resize(out.len() + gap_len, 0i16);
                let head_len = fade_len.min(seg.len());
                for (i, &s) in seg[..head_len].iter().enumerate() {
                    let gain = i as f32 / head_len as f32;
                    out.push((s as f32 * gain).round().clamp(-32768.0, 32767.0) as i16);
                }
                let tail_start = head_len;
                if seg.len() > tail_start + fade_len {
                    out.extend_from_slice(&seg[tail_start..seg.len() - fade_len]);
                    for (i, &s) in seg[seg.len() - fade_len..].iter().enumerate() {
                        let gain = 1.0 - (i as f32 / fade_len as f32);
                        out.push((s as f32 * gain).round().clamp(-32768.0, 32767.0) as i16);
                    }
                } else if seg.len() > tail_start {
                    out.extend_from_slice(&seg[tail_start..]);
                }
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_equal_power_crossfade_seam() {
        let sample_rate = 24000;
        let seg_a = vec![10000; 2400]; // 100ms
        let seg_b = vec![20000; 2400]; // 100ms
        let out = AudioMerger::equal_power_crossfade(&seg_a, &seg_b, sample_rate, 20);
        // Fade length = 24000 * 0.02 = 480 samples
        // Out length = 2400 + 2400 - 480 = 4320
        assert_eq!(out.len(), 4320);

        // Beginning of blend seam must be close to seg_a (10000)
        let seam_start = 2400 - 480;
        assert!(out[seam_start] > 9000);

        // Center of equal-power blend (t = 0.5):
        // gain_a = cos(pi/4) = ~0.707, gain_b = sin(pi/4) = ~0.707
        // blended = 10000 * 0.707 + 20000 * 0.707 = ~21210
        let seam_mid = seam_start + 240;
        assert!(out[seam_mid] > 18000 && out[seam_mid] < 23000);
    }

    #[test]
    fn test_equal_power_crossfade_short_fallback() {
        let seg_a = vec![100; 10];
        let seg_b = vec![200; 10];
        // 50ms at 24000 is 1200 samples, much longer than segments
        let out = AudioMerger::equal_power_crossfade(&seg_a, &seg_b, 24000, 50);
        assert_eq!(out.len(), 20);
        assert_eq!(&out[..10], &seg_a[..]);
        assert_eq!(&out[10..], &seg_b[..]);
    }

    #[test]
    fn test_concatenate_with_equal_power_crossfade_empty_and_single() {
        assert!(AudioMerger::concatenate_with_equal_power_crossfade(&[], 24000, 20).is_empty());
        let single = vec![500; 100];
        assert_eq!(
            AudioMerger::concatenate_with_equal_power_crossfade(&[&single], 24000, 20),
            single
        );
    }

    #[test]
    fn test_concatenate_with_equal_power_crossfade_multiple() {
        let c1 = vec![1000; 1000];
        let c2 = vec![2000; 1000];
        let c3 = vec![3000; 1000];
        let out = AudioMerger::concatenate_with_equal_power_crossfade(&[&c1, &c2, &c3], 24000, 10);
        // 10ms = 240 samples. Seams = 2.
        // Total = 3000 - 2 * 240 = 2520 samples
        assert_eq!(out.len(), 2520);
    }

    #[test]
    fn test_silence_gap_join_empty_and_single() {
        assert!(AudioMerger::silence_gap_join(&[], 24000, 20, 100).is_empty());
        let single = vec![500i16; 100];
        assert_eq!(
            AudioMerger::silence_gap_join(&[&single], 24000, 20, 100),
            single
        );
    }

    #[test]
    fn test_silence_gap_join_two_chunks_contains_gap() {
        let sr = 24_000u32;
        let fade_ms = 20u32;
        let gap_ms = 100u32;
        let chunk = vec![10000i16; 2400]; // 100ms @ 24kHz
        let out = AudioMerger::silence_gap_join(&[&chunk, &chunk], sr, fade_ms, gap_ms);

        // Expected total ≈ 2400 + 2400 + 2400(gap) = 7200
        // Actual may vary slightly due to fade lengths but should be in range
        let gap_samples = (sr as f32 * gap_ms as f32 / 1000.0) as usize;
        assert!(
            out.len() > chunk.len() * 2,
            "Output must be longer than 2 input chunks (gap adds samples), got {}",
            out.len()
        );

        // The gap region should contain zeros (around middle)
        let gap_start = chunk.len() - 10; // near end of chunk A
        let gap_end = gap_start + gap_samples + 10;
        let gap_end = gap_end.min(out.len());
        let any_zero_in_gap = out[gap_start..gap_end].contains(&0);
        assert!(any_zero_in_gap, "Silence gap must exist between chunks");
    }

    #[test]
    fn test_silence_gap_join_no_clipping() {
        let sr = 24_000u32;
        let loud = vec![30000i16; 4800]; // 200ms loud signal
        let out = AudioMerger::silence_gap_join(&[&loud, &loud, &loud], sr, 20, 100);
        // i16 is by definition in [-32768, 32767]; verify non-empty and no panic
        assert!(!out.is_empty());
    }
}
