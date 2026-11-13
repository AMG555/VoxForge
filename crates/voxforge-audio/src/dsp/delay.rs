//! Studio feedback echo and delay line processor.
//!
//! Enriches vocal and ambient acoustics with adjustable echo delay,
//! feedback recirculation, and stereo-ready wet/dry blend.

/// Feedback echo delay processor.
///
/// Pre-allocates the circular delay buffer in `new()` so repeated calls to
/// `process()` on the same instance incur zero heap allocation.
pub struct EchoDelay {
    buffer: Vec<f32>,
    write_idx: usize,
    feedback: f32,
}

impl EchoDelay {
    /// Create a new echo delay processor pre-allocated for the given parameters.
    ///
    /// - `delay_ms`: Echo delay time in milliseconds (1.0–2000.0 ms).
    /// - `feedback`: Feedback recirculation coefficient (0.0–0.95).
    /// - `sample_rate`: Audio sample rate in Hz.
    ///
    /// Returns `None` if `delay_ms` rounds to zero samples.
    pub fn new(delay_ms: f32, feedback: f32, sample_rate: u32) -> Option<Self> {
        if !delay_ms.is_finite() || !feedback.is_finite() {
            return None;
        }
        let delay_sec = delay_ms.clamp(1.0, 2000.0) / 1000.0;
        let delay_samples = ((sample_rate as f32) * delay_sec).round() as usize;
        if delay_samples == 0 {
            return None;
        }
        Some(Self {
            buffer: vec![0.0f32; delay_samples],
            write_idx: 0,
            feedback: feedback.clamp(0.0, 0.95),
        })
    }

    /// Apply echo delay in-place.
    ///
    /// - `wet_mix`: Echo level mix (0.0–1.0).
    /// - `dry_mix`: Direct signal level mix (0.0–1.0).
    pub fn process(&mut self, samples: &mut [i16], wet_mix: f32, dry_mix: f32) {
        if samples.is_empty()
            || !wet_mix.is_finite()
            || !dry_mix.is_finite()
            || wet_mix <= 0.001
        {
            return;
        }

        let wet = wet_mix.clamp(0.0, 1.0);
        let dry = dry_mix.clamp(0.0, 1.0);
        let len = self.buffer.len();

        for sample in samples.iter_mut() {
            let input = (*sample as f32) / 32768.0;
            let delayed = self.buffer[self.write_idx];

            self.buffer[self.write_idx] = input + (delayed * self.feedback);
            self.write_idx = (self.write_idx + 1) % len;

            let mixed = (input * dry) + (delayed * wet);
            *sample = (mixed * 32768.0).round().clamp(-32768.0, 32767.0) as i16;
        }
    }

    /// Reset the internal delay buffer to silence.
    pub fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.write_idx = 0;
    }

    /// One-shot static helper: allocate, process, discard.
    ///
    /// Equivalent to the old API for call sites that do not retain state.
    pub fn process_once(
        samples: &mut [i16],
        delay_ms: f32,
        feedback: f32,
        wet_mix: f32,
        dry_mix: f32,
        sample_rate: u32,
    ) {
        if let Some(mut delay) = Self::new(delay_ms, feedback, sample_rate) {
            delay.process(samples, wet_mix, dry_mix);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_wet_leaves_samples_unaltered() {
        let mut samples = vec![1000, -2000, 3000, -4000];
        let original = samples.clone();
        EchoDelay::process_once(&mut samples, 100.0, 0.5, 0.0, 1.0, 24000);
        assert_eq!(samples, original);
    }

    #[test]
    fn test_echo_recirculates_signal() {
        let mut samples = vec![0i16; 5000];
        samples[0] = 30000;
        let delay_ms = 50.0; // 50ms at 24kHz = 1200 samples
        EchoDelay::process_once(&mut samples, delay_ms, 0.5, 0.8, 1.0, 24000);
        // Delay hit should appear around index 1200
        assert!(
            samples[1200].abs() > 5000,
            "Echo must create delayed replica"
        );
    }

    #[test]
    fn test_stateful_echo_reset() {
        let mut delay = EchoDelay::new(50.0, 0.5, 24000).unwrap();
        let mut a = vec![10000i16; 500];
        delay.process(&mut a, 0.8, 1.0);
        delay.reset();
        // After reset, first sample has no echo, only dry
        let mut b = vec![10000i16; 1];
        delay.process(&mut b, 0.8, 1.0);
        // With a fresh buffer the delayed sample is 0, so output = dry * input = 10000
        assert_eq!(b[0], 10000);
    }

    #[test]
    fn test_new_returns_none_for_non_finite_delay() {
        assert!(EchoDelay::new(f32::NAN, 0.5, 24000).is_none());
        assert!(EchoDelay::new(f32::INFINITY, 0.5, 24000).is_none());
    }

    #[test]
    fn test_process_once_matches_stateful() {
        let mut a = vec![8000i16; 200];
        let mut b = a.clone();
        EchoDelay::process_once(&mut a, 50.0, 0.4, 0.6, 1.0, 24000);
        let mut delay = EchoDelay::new(50.0, 0.4, 24000).unwrap();
        delay.process(&mut b, 0.6, 1.0);
        assert_eq!(a, b);
    }
}
