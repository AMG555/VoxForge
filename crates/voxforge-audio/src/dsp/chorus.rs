//! Multi-voice modulated chorus and vocal thickening processor.
//!
//! Adds width, warmth, and subtle pitch-vibrato movement to synthesized
//! neural voices by modulating short delay lines with low-frequency oscillators.

/// Multi-voice modulated chorus processor.
///
/// Pre-allocates the circular delay buffer in `new()` so repeated calls to
/// `process()` on the same instance incur zero heap allocation.
pub struct VocalChorus {
    buffer: Vec<f32>,
    write_idx: usize,
    lfo_phase: f32,
    lfo_step: f32,
    base_delay_samples: f32,
    mod_depth_samples: f32,
    max_delay_samples: usize,
}

impl VocalChorus {
    /// Create a new chorus processor pre-allocated for the given sample rate and parameters.
    ///
    /// - `rate_hz`: LFO modulation rate in Hz (0.1–10.0 Hz). Typical: 1.5 Hz.
    /// - `depth_ms`: Delay modulation depth in milliseconds (0.5–15.0 ms). Typical: 3.5 ms.
    /// - `sample_rate`: Audio sample rate in Hz.
    pub fn new(rate_hz: f32, depth_ms: f32, sample_rate: u32) -> Self {
        let rate_hz = if rate_hz.is_finite() {
            rate_hz.clamp(0.1, 10.0)
        } else {
            1.5
        };
        let depth_ms = if depth_ms.is_finite() {
            depth_ms.clamp(0.5, 15.0)
        } else {
            3.5
        };

        let sr = sample_rate as f32;
        let base_delay_ms = 10.0f32;
        let max_delay_ms = base_delay_ms + depth_ms;
        let max_delay_samples = ((sr * (max_delay_ms / 1000.0)).ceil() as usize) + 2;

        Self {
            buffer: vec![0.0f32; max_delay_samples],
            write_idx: 0,
            lfo_phase: 0.0,
            lfo_step: 2.0 * std::f32::consts::PI * rate_hz / sr,
            base_delay_samples: sr * (base_delay_ms / 1000.0),
            mod_depth_samples: sr * (depth_ms / 1000.0),
            max_delay_samples,
        }
    }

    /// Apply vocal chorus modulation in-place.
    ///
    /// - `wet_mix`: Level of modulated voice blend (0.0–1.0).
    /// - `dry_mix`: Level of dry voice signal (0.0–1.0).
    pub fn process(&mut self, samples: &mut [i16], wet_mix: f32, dry_mix: f32) {
        if samples.is_empty() || !wet_mix.is_finite() || !dry_mix.is_finite() || wet_mix <= 0.001 {
            return;
        }

        let wet = wet_mix.clamp(0.0, 1.0);
        let dry = dry_mix.clamp(0.0, 1.0);
        let max = self.max_delay_samples;

        for sample in samples.iter_mut() {
            let input = (*sample as f32) / 32768.0;
            self.buffer[self.write_idx] = input;

            let mod_delay =
                self.base_delay_samples + (self.lfo_phase.sin() * self.mod_depth_samples);
            let read_pos = (self.write_idx as f32) - mod_delay;
            let read_pos_wrapped = if read_pos < 0.0 {
                read_pos + (max as f32) * ((-read_pos / max as f32).ceil() + 1.0)
            } else {
                read_pos
            } % (max as f32);

            let idx0 = read_pos_wrapped.floor() as usize % max;
            let idx1 = (idx0 + 1) % max;
            let frac = read_pos_wrapped - read_pos_wrapped.floor();
            let delayed = self.buffer[idx0] * (1.0 - frac) + self.buffer[idx1] * frac;

            self.write_idx = (self.write_idx + 1) % max;
            self.lfo_phase += self.lfo_step;
            if self.lfo_phase >= 2.0 * std::f32::consts::PI {
                self.lfo_phase -= 2.0 * std::f32::consts::PI;
            }

            let mixed = (input * dry) + (delayed * wet);
            *sample = (mixed * 32768.0).round().clamp(-32768.0, 32767.0) as i16;
        }
    }

    /// Reset the internal delay buffer and LFO phase to silence.
    pub fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.write_idx = 0;
        self.lfo_phase = 0.0;
    }

    /// One-shot static helper: allocate, process, discard.
    ///
    /// Equivalent to the old API for call sites that do not retain state.
    pub fn process_once(
        samples: &mut [i16],
        rate_hz: f32,
        depth_ms: f32,
        wet_mix: f32,
        dry_mix: f32,
        sample_rate: u32,
    ) {
        let mut chorus = Self::new(rate_hz, depth_ms, sample_rate);
        chorus.process(samples, wet_mix, dry_mix);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_wet_leaves_samples_unaltered() {
        let mut samples = vec![1000, -2000, 3000, -4000];
        let original = samples.clone();
        VocalChorus::process_once(&mut samples, 1.5, 3.0, 0.0, 1.0, 24000);
        assert_eq!(samples, original);
    }

    #[test]
    fn test_chorus_modulates_output() {
        let mut samples = vec![15000; 1000];
        VocalChorus::process_once(&mut samples, 2.0, 4.0, 0.5, 0.5, 24000);
        // Due to delay ramp-up and LFO, samples should vary across time
        assert!(samples.iter().any(|&s| s != 15000));
    }

    #[test]
    fn test_stateful_chorus_reset() {
        let mut chorus = VocalChorus::new(1.5, 3.0, 24000);
        let mut a = vec![10000i16; 500];
        chorus.process(&mut a, 0.5, 0.5);
        chorus.reset();
        // After reset the buffer is zeroed — first output should be dry * input
        let mut b = vec![10000i16; 1];
        chorus.process(&mut b, 0.5, 1.0);
        // dry path only on first sample (no delayed signal yet)
        assert!((b[0] as i32 - 10000).abs() < 1500); // tolerance for the mix
    }

    #[test]
    fn test_process_once_matches_stateful() {
        let mut a = vec![8000i16; 100];
        let mut b = a.clone();
        VocalChorus::process_once(&mut a, 2.0, 4.0, 0.4, 0.8, 24000);
        let mut chorus = VocalChorus::new(2.0, 4.0, 24000);
        chorus.process(&mut b, 0.4, 0.8);
        assert_eq!(a, b);
    }
}
