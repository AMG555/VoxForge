//! Algorithmic studio reverberation processor.
//!
//! Provides spatial room acoustic reflections based on a multi-stage comb
//! and allpass filter network (Schroeder/Freeverb architecture), engineered
//! for low latency vocal and studio speech rendering.

/// Stateful Algorithmic Reverb processor.
///
/// Holds pre-allocated comb and allpass delay buffers so the caller can reuse
/// a single instance across multiple `process()` calls without heap allocation
/// on the hot path. For one-shot use the `AlgorithmicReverb::process_once`
/// static helper allocates-and-runs internally.
pub struct AlgorithmicReverb {
    combs: [CombFilter; 8],
    allpasses: [AllpassFilter; 4],
}

struct CombFilter {
    buffer: Vec<f32>,
    index: usize,
    filter_store: f32,
}

impl CombFilter {
    fn new(size: usize) -> Self {
        Self {
            buffer: vec![0.0; size.max(1)],
            index: 0,
            filter_store: 0.0,
        }
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.index = 0;
        self.filter_store = 0.0;
    }

    #[inline]
    fn process(&mut self, input: f32, feedback: f32, damp: f32) -> f32 {
        let output = self.buffer[self.index];
        self.filter_store = output * (1.0 - damp) + self.filter_store * damp;
        self.buffer[self.index] = input + self.filter_store * feedback;
        self.index = (self.index + 1) % self.buffer.len();
        output
    }
}

struct AllpassFilter {
    buffer: Vec<f32>,
    index: usize,
    feedback: f32,
}

impl AllpassFilter {
    fn new(size: usize, feedback: f32) -> Self {
        Self {
            buffer: vec![0.0; size.max(1)],
            index: 0,
            feedback,
        }
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.index = 0;
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let buf_out = self.buffer[self.index];
        let output = -input + buf_out;
        self.buffer[self.index] = input + (buf_out * self.feedback);
        self.index = (self.index + 1) % self.buffer.len();
        output
    }
}

impl AlgorithmicReverb {
    /// Tuned comb/allpass delay sizes at 44.1 kHz (Freeverb / Schroeder values).
    const COMB_TUNINGS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
    const ALLPASS_TUNINGS: [usize; 4] = [556, 441, 341, 225];

    /// Create a new reverb processor sized for `sample_rate`.
    ///
    /// All delay-line buffers are allocated once here; subsequent `process()`
    /// calls run on the hot path with zero heap traffic.
    pub fn new(sample_rate: u32) -> Self {
        let rate_scale = (sample_rate as f32) / 44100.0;
        let combs = Self::COMB_TUNINGS.map(|t| {
            CombFilter::new(((t as f32) * rate_scale).round() as usize)
        });
        let allpasses = Self::ALLPASS_TUNINGS.map(|t| {
            AllpassFilter::new(((t as f32) * rate_scale).round() as usize, 0.5)
        });
        Self { combs, allpasses }
    }

    /// Reset all delay-line state to zero (reuse with a new audio clip).
    pub fn reset(&mut self) {
        for c in &mut self.combs { c.reset(); }
        for a in &mut self.allpasses { a.reset(); }
    }

    /// Apply reverberation to `samples` in-place using this instance's delay lines.
    ///
    /// - `room_size`: Room reflection decay (0.0 to 1.0).
    /// - `damping`:   High-frequency absorption (0.0 to 1.0).
    /// - `wet_mix`:   Wet reverb level (0.0 to 1.0).
    /// - `dry_mix`:   Dry direct level (0.0 to 1.0).
    pub fn process(&mut self, samples: &mut [i16], room_size: f32, damping: f32, wet_mix: f32, dry_mix: f32) {
        if samples.is_empty()
            || !room_size.is_finite()
            || !damping.is_finite()
            || !wet_mix.is_finite()
            || !dry_mix.is_finite()
            || wet_mix <= 0.001
        {
            return;
        }

        let room = room_size.clamp(0.0, 0.98);
        let damp = damping.clamp(0.0, 0.95);
        let wet = wet_mix.clamp(0.0, 1.0);
        let dry = dry_mix.clamp(0.0, 1.0);
        let feedback = 0.7 + (room * 0.28);

        for sample in samples.iter_mut() {
            let input = (*sample as f32) / 32768.0;

            // Parallel comb filter bank
            let mut comb_sum = 0.0f32;
            for comb in &mut self.combs {
                comb_sum += comb.process(input, feedback, damp);
            }
            comb_sum *= 0.125; // Normalise 8 comb outputs

            // Serial allpass diffusor chain
            let mut diffused = comb_sum;
            for ap in &mut self.allpasses {
                diffused = ap.process(diffused);
            }

            let mixed = (input * dry) + (diffused * wet);
            *sample = (mixed * 32768.0).round().clamp(-32768.0, 32767.0) as i16;
        }
    }

    /// Convenience one-shot static helper: allocates a fresh reverb instance,
    /// processes, and discards. Use this for occasional reverb where the caller
    /// cannot hold state.
    pub fn process_once(
        samples: &mut [i16],
        room_size: f32,
        damping: f32,
        wet_mix: f32,
        dry_mix: f32,
        sample_rate: u32,
    ) {
        let mut rv = Self::new(sample_rate);
        rv.process(samples, room_size, damping, wet_mix, dry_mix);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_wet_leaves_samples_unaltered() {
        let mut samples = vec![500, -1000, 1500, -2000];
        let original = samples.clone();
        // process_once with wet=0.0 returns immediately (guard at top of process())
        AlgorithmicReverb::process_once(&mut samples, 0.7, 0.3, 0.0, 1.0, 24000);
        assert_eq!(samples, original);
    }

    #[test]
    fn test_reverb_generates_tail_decay() {
        // Feed an impulse into silence: reverb should fill subsequent silence
        let mut samples = vec![0i16; 4000];
        samples[0] = 30000;
        AlgorithmicReverb::process_once(&mut samples, 0.8, 0.2, 0.5, 1.0, 24000);
        let tail_energy: i64 = samples[500..1500].iter().map(|&s| s.abs() as i64).sum();
        assert!(
            tail_energy > 0,
            "Reverb must produce audible reflection decay tail"
        );
    }

    #[test]
    fn test_stateful_reverb_reset() {
        let mut rv = AlgorithmicReverb::new(24000);
        let mut samples = vec![0i16; 4000];
        samples[0] = 30000;
        rv.process(&mut samples, 0.8, 0.2, 0.5, 1.0);
        rv.reset();
        // After reset the first sample of a fresh buffer should be unaffected by prior state
        let mut clean = vec![0i16; 100];
        rv.process(&mut clean, 0.8, 0.2, 0.5, 1.0);
        // All clean input + reset state → output must be ~0 for the first few samples
        assert!(clean[..10].iter().all(|&s| s.abs() < 10));
    }
}
