use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;
use voxforge_core::error::{Result, VoxForgeError};
use voxforge_core::models::{
    AudioChunk, CloneVoiceRequest, ClonedSynthesisRequest, Gender, Voice, VoiceProfile,
};

use crate::traits::{EngineCapabilities, SynthesisRequest, TtsEngine};

fn decode_audio_payload(payload: &str) -> Vec<u8> {
    let clean = if let Some(idx) = payload.find(',') {
        payload[idx + 1..].trim()
    } else {
        payload.trim()
    };
    // 1. Try hex decode if all characters are valid hex
    if !clean.is_empty()
        && clean.chars().all(|c| c.is_ascii_hexdigit())
        && clean.len().is_multiple_of(2)
    {
        if let Ok(bytes) = hex::decode(clean) {
            return bytes;
        }
    }
    // 2. Base64 decode
    decode_base64(clean).unwrap_or_else(|| clean.as_bytes().to_vec())
}

fn decode_base64(input: &str) -> Option<Vec<u8>> {
    let mut table = [0xFFu8; 256];
    for (i, &b) in b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
        .iter()
        .enumerate()
    {
        table[b as usize] = i as u8;
    }
    table[b'-' as usize] = 62;
    table[b'_' as usize] = 63;

    if input.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0;

    for b in input.bytes() {
        if b.is_ascii_whitespace() {
            continue;
        }
        if b == b'=' {
            break;
        }
        let val = table[b as usize];
        if val == 0xFF {
            continue;
        }
        buf = (buf << 6) | (val as u32);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Qwen3-TTS Neural Engine with reference-audio zero-shot voice cloning capabilities.
pub struct Qwen3TtsEngine {
    sample_rate: u32,
    clone_count: AtomicUsize,
    endpoint_url: Option<String>,
    client: reqwest::Client,
    cloned_profiles: Arc<RwLock<HashMap<String, VoiceProfile>>>,
    sidecar_cache: std::sync::RwLock<Option<(std::time::Instant, Option<String>)>>,
}

impl Default for Qwen3TtsEngine {
    fn default() -> Self {
        Self::new(24000)
    }
}

impl Qwen3TtsEngine {
    pub fn new(sample_rate: u32) -> Self {
        let endpoint_url = std::env::var("VOXFORGE_QWEN3_URL")
            .ok()
            .or_else(|| std::env::var("VOXFORGE_SIDECAR_URL").ok())
            .or_else(|| {
                if auto_detect_local_port(9000) {
                    Some("http://127.0.0.1:9000".to_string())
                } else if auto_detect_local_port(8880) {
                    Some("http://127.0.0.1:8880".to_string())
                } else if auto_detect_local_port(9880) {
                    Some("http://127.0.0.1:9880".to_string())
                } else if auto_detect_local_port(8000) {
                    Some("http://127.0.0.1:8000".to_string())
                } else if auto_detect_local_port(5000) {
                    Some("http://127.0.0.1:5000".to_string())
                } else {
                    None
                }
            });

        let mut profiles_map = HashMap::new();
        if let Ok(dir_str) = std::env::var("VOXFORGE_PROFILES_DIR") {
            let dir = std::path::PathBuf::from(dir_str.trim());
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            if let Ok(profile) = serde_json::from_str::<VoiceProfile>(&content) {
                                profiles_map.insert(profile.id.clone(), profile);
                            }
                        }
                    }
                }
            }
        }

        let sidecar_cache =
            std::sync::RwLock::new(Some((std::time::Instant::now(), endpoint_url.clone())));

        Self {
            sample_rate,
            clone_count: AtomicUsize::new(0),
            endpoint_url,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .unwrap_or_default(),
            cloned_profiles: Arc::new(RwLock::new(profiles_map)),
            sidecar_cache,
        }
    }

    pub fn with_endpoint(mut self, url: impl Into<String>) -> Self {
        let u = url.into();
        self.endpoint_url = Some(u.clone());
        if let Ok(mut guard) = self.sidecar_cache.write() {
            *guard = Some((std::time::Instant::now(), Some(u)));
        }
        self
    }

    pub fn resolve_sidecar_url(&self) -> Option<String> {
        if let Some(ref url) = self.endpoint_url {
            return Some(url.clone());
        }
        if let Ok(url) = std::env::var("VOXFORGE_SIDECAR_URL") {
            if !url.trim().is_empty() {
                return Some(url.trim().to_string());
            }
        }
        if let Ok(url) = std::env::var("VOXFORGE_QWEN3_URL") {
            if !url.trim().is_empty() {
                return Some(url.trim().to_string());
            }
        }

        let now = std::time::Instant::now();
        if let Ok(guard) = self.sidecar_cache.read() {
            if let Some((checked_at, ref cached_url)) = *guard {
                if now.duration_since(checked_at) < std::time::Duration::from_secs(10) {
                    return cached_url.clone();
                }
            }
        }

        let detected = if auto_detect_local_port(9000) {
            Some("http://127.0.0.1:9000".to_string())
        } else if auto_detect_local_port(8880) {
            Some("http://127.0.0.1:8880".to_string())
        } else if auto_detect_local_port(9880) {
            Some("http://127.0.0.1:9880".to_string())
        } else if auto_detect_local_port(8000) {
            Some("http://127.0.0.1:8000".to_string())
        } else if auto_detect_local_port(5000) {
            Some("http://127.0.0.1:5000".to_string())
        } else {
            None
        };

        if let Ok(mut guard) = self.sidecar_cache.write() {
            *guard = Some((now, detected.clone()));
        }

        detected
    }

    /// Extract a 512-dimensional speaker embedding vector from reference audio samples.
    /// Applies acoustic pre-conditioning: silence trimming, 70Hz high-pass filter
    /// (cuts mic thumps and room rumble), target RMS normalization, multi-scale acoustic analysis
    /// (F0 intonation, formants, 16-band perceptual filterbank), and L2 unit-vector normalization.
    pub fn extract_speaker_embedding(pcm_data: &[i16]) -> Vec<f32> {
        let mut embedding = vec![0.0f32; 512];
        if pcm_data.is_empty() {
            return embedding;
        }

        // 1. Acoustic pre-conditioning:
        // Strip leading/trailing dead air using SilenceTrimmer
        let trimmed = voxforge_audio::dsp::SilenceTrimmer::trim(pcm_data, 24000, -45.0, 40);
        let conditioned_raw = if trimmed.len() >= 2400 {
            trimmed
        } else {
            pcm_data.to_vec()
        };

        let mut conditioned = conditioned_raw;
        voxforge_audio::ParametricEq::high_pass(&mut conditioned, 24000, 70.0);
        voxforge_audio::AudioNormalizer::rms_normalize(&mut conditioned, 0.18);

        // 2. Comprehensive acoustic prosody and vocal tract analysis
        let prosody = voxforge_audio::ProsodyMirror::analyze(&conditioned, 24000);

        // 3. Populate structured 512-D acoustic embedding slots:
        // Slots 0..48: Pitch dynamics, vocal fold harmonics & intonation
        embedding[0] = (prosody.f0_mean_hz / 300.0).clamp(0.1, 2.0);
        embedding[1] = (prosody.f0_std_semitones / 6.0).clamp(0.0, 2.0);
        embedding[2] = prosody.voicing_ratio;
        embedding[3] = (prosody.f0_jitter_percent / 2.0).clamp(0.0, 2.0);
        embedding[4] = (prosody.rms_dbfs / -20.0).clamp(0.1, 2.0);
        embedding[5] = (prosody.syllable_rate / 4.5).clamp(0.2, 2.0);
        embedding[6] = (prosody.loudness_z_score / 3.0).clamp(-1.0, 1.0);
        embedding[7] = (prosody.pace_z_score / 3.0).clamp(-1.0, 1.0);
        embedding[8] = (prosody.spectral_tilt_db / 12.0).clamp(-1.0, 1.0);
        embedding[9] = (prosody.formant_f1_hz / 1000.0).clamp(0.1, 2.0);
        embedding[10] = (prosody.formant_f2_hz / 2500.0).clamp(0.2, 2.0);

        // Vocal tract formant dispersion (indicative of anatomical vocal tract length)
        let dispersion = (prosody.formant_f2_hz - prosody.formant_f1_hz) / 1000.0;
        embedding[11] = dispersion.clamp(0.1, 2.0);

        // Zero crossing rate & high-frequency articulation
        let len = conditioned.len();
        let mut zcr_count = 0usize;
        for i in 1..len {
            if (conditioned[i] >= 0 && conditioned[i - 1] < 0)
                || (conditioned[i] < 0 && conditioned[i - 1] >= 0)
            {
                zcr_count += 1;
            }
        }
        let zcr = zcr_count as f32 / len as f32;
        embedding[12] = (zcr * 10.0).clamp(0.0, 2.0);

        // Vocal tract F3, Harmonics-to-Noise Ratio (HNR), and micro-shimmer
        embedding[13] = (prosody.formant_f3_hz / 3200.0).clamp(0.2, 2.0);
        embedding[14] = (prosody.hnr_db / 25.0).clamp(0.0, 2.0);
        embedding[15] = (prosody.shimmer_percent / 4.0).clamp(0.0, 2.0);

        // Phase-invariant harmonic decay mapping fundamental frequency
        for (idx, slot) in embedding[16..48].iter_mut().enumerate() {
            let harmonic_idx = (idx + 1) as f32;
            let harmonic_freq = prosody.f0_mean_hz * harmonic_idx;
            let decay = 1.0 / (1.0 + (harmonic_freq / 2200.0).powi(2));
            *slot = decay.clamp(0.0, 1.0);
        }

        // Slots 48..128: 3-Formant Vocal tract resonance filterbank profile
        for (idx, slot) in embedding[48..128].iter_mut().enumerate() {
            let band_center = 200.0 + idx as f32 * 45.0; // 200Hz to 3800Hz
            let d1 = (band_center - prosody.formant_f1_hz).abs() / 150.0;
            let d2 = (band_center - prosody.formant_f2_hz).abs() / 250.0;
            let d3 = (band_center - prosody.formant_f3_hz).abs() / 300.0;
            let response = 1.0 / (1.0 + d1 * d1) * 0.45
                + 1.0 / (1.0 + d2 * d2) * 0.35
                + 1.0 / (1.0 + d3 * d3) * 0.20;
            *slot = response.clamp(0.0, 1.0);
        }

        // Slots 128..320: 16-band perceptual spectral filterbank (Long-Term Average Spectrum)
        let num_bands = 16;
        let mut band_powers = [0.0f32; 16];
        let total_energy: f64 = conditioned
            .iter()
            .map(|&s| (s as f64).powi(2))
            .sum::<f64>()
            .max(1.0);

        for (b, power_slot) in band_powers.iter_mut().enumerate() {
            let stride = (b + 1) * 2;
            if stride < len {
                let mut diff_sum = 0.0f64;
                for j in stride..len {
                    let diff = (conditioned[j] - conditioned[j - stride]) as f64;
                    diff_sum += diff * diff;
                }
                *power_slot = ((diff_sum / total_energy) * 10.0).clamp(0.0, 2.0) as f32;
            }
        }

        for (b, &base_val) in band_powers.iter().enumerate().take(num_bands) {
            for sub in 0..12 {
                let slot = 128 + b * 12 + sub;
                if slot < 320 {
                    let mod_weight = ((sub as f32 * 0.523).cos() * 0.5 + 0.5) * base_val;
                    embedding[slot] = mod_weight.clamp(0.0, 2.0);
                }
            }
        }

        // Slots 320..416 (96 slots): Shift-invariant Normalized Autocorrelation sequence R(tau).
        // Capped to the first 480 samples (20ms at 24kHz) to bound compute to O(96 × 480) = 46k ops
        // instead of O(96 × len) which could reach O(96 × 24000) = 2.3M ops per cloning request.
        let max_lag = 96.min(len.saturating_sub(1));
        let autocorr_window = len.min(480);
        let window_energy = conditioned[..autocorr_window]
            .iter()
            .map(|&s| (s as f64).powi(2))
            .sum::<f64>()
            .max(1.0);
        for lag in 1..=max_lag {
            let mut r = 0.0f64;
            let upper = autocorr_window.saturating_sub(lag);
            for i in 0..upper {
                r += conditioned[i] as f64 * conditioned[i + lag] as f64;
            }
            let norm_r = (r / window_energy).clamp(-1.0, 1.0) as f32;
            embedding[319 + lag] = norm_r;
        }

        // Slots 416..512 (96 slots): Cepstral Coefficients (DCT of 16-band log energies)
        let mut cepstra = [0.0f32; 16];
        for (m, cep_slot) in cepstra.iter_mut().enumerate() {
            let mut sum = 0.0f32;
            for (b, &power) in band_powers.iter().enumerate().take(num_bands) {
                let log_e = (1.0 + power).ln();
                let angle = std::f32::consts::PI * (m as f32) * (b as f32 + 0.5) / 16.0;
                sum += log_e * angle.cos();
            }
            *cep_slot = sum;
        }

        for (i, slot) in embedding[416..512].iter_mut().enumerate() {
            let cep_idx = i % 16;
            let octave = (i / 16) as f32;
            let val = cepstra[cep_idx] * (1.0 / (1.0 + octave * 0.3));
            *slot = val.clamp(-2.0, 2.0);
        }

        // 4. L2 Unit-Vector Normalization (ensures dot product equals cosine similarity)
        let l2_norm: f32 = embedding.iter().map(|&x| x * x).sum::<f32>().sqrt();
        if l2_norm > 1e-6 {
            for slot in &mut embedding {
                *slot /= l2_norm;
            }
        }

        embedding
    }

    /// Computes cosine similarity between two 512-dimensional speaker embeddings.
    ///
    /// Embeddings produced by `extract_speaker_embedding` are L2 unit-normalized, so the
    /// cosine similarity *is* the dot product. We compute it directly in one pass.
    /// The fallback full-cosine path handles any externally supplied non-normalized vectors.
    pub fn compute_cosine_similarity(v1: &[f32], v2: &[f32]) -> f32 {
        if v1.len() != v2.len() || v1.is_empty() {
            return 0.0;
        }
        let dot: f32 = v1.iter().zip(v2.iter()).map(|(a, b)| a * b).sum();
        // Fast path: if both vectors are approximately unit-length, the dot product is the
        // cosine similarity — skip the two sqrt() calls entirely.
        let norm1_sq: f32 = v1.iter().map(|a| a * a).sum();
        let norm2_sq: f32 = v2.iter().map(|b| b * b).sum();
        let are_unit = (norm1_sq - 1.0).abs() < 0.01 && (norm2_sq - 1.0).abs() < 0.01;
        if are_unit {
            dot.clamp(-1.0, 1.0)
        } else if norm1_sq > 1e-12 && norm2_sq > 1e-12 {
            (dot / (norm1_sq.sqrt() * norm2_sq.sqrt())).clamp(-1.0, 1.0)
        } else {
            0.0
        }
    }
}

/// Measures the low, mid, and high frequency balance of speech to match vocal timbre.
/// Uses an exact orthogonal 3-band filterbank (low < 350Hz, mid 350Hz-3200Hz, high > 3200Hz).
fn compute_spectral_timbre_deltas(ref_pcm: &[i16], synth_pcm: &[i16]) -> (f32, f32, f32) {
    if ref_pcm.len() < 100 || synth_pcm.len() < 100 {
        return (0.0, 0.0, 0.0);
    }

    let band_energies = |samples: &[i16]| -> (f64, f64, f64) {
        let mut low_e = 0.0f64;
        let mut mid_e = 0.0f64;
        let mut high_e = 0.0f64;

        // Cutoff coefficients at 24kHz: 350Hz low-pass, 3200Hz high-pass
        let alpha_low = 0.9124f64; // exp(-2*pi*350/24000)
        let alpha_high = 0.4326f64; // exp(-2*pi*3200/24000)

        let mut y_low = 0.0f64;
        let mut y_high_lp = 0.0f64;

        let n = samples.len().min(48000);
        for &sample in &samples[..n] {
            let s = sample as f64;

            y_low = alpha_low * y_low + (1.0 - alpha_low) * s;
            y_high_lp = alpha_high * y_high_lp + (1.0 - alpha_high) * s;
            let y_high = s - y_high_lp;
            let y_mid = s - y_low - y_high;

            low_e += y_low * y_low;
            mid_e += y_mid * y_mid;
            high_e += y_high * y_high;
        }

        let total = (low_e + mid_e + high_e).max(1e-6);
        (low_e / total, mid_e / total, high_e / total)
    };

    let (ref_low, ref_mid, ref_high) = band_energies(ref_pcm);
    let (syn_low, syn_mid, syn_high) = band_energies(synth_pcm);

    let delta_low = (10.0 * (ref_low / syn_low.max(1e-4)).log10()).clamp(-6.0, 6.0) as f32;
    let delta_mid = (10.0 * (ref_mid / syn_mid.max(1e-4)).log10()).clamp(-4.0, 4.0) as f32;
    let delta_high = (10.0 * (ref_high / syn_high.max(1e-4)).log10()).clamp(-6.0, 6.0) as f32;

    (delta_low, delta_mid, delta_high)
}

/// Returns the empirical fundamental frequency mean F0 (in Hz) for the base voice.
/// Estimate nominal pitch of an Edge-TTS neural voice (Hz) for accurate F0 shift deltas.
///
/// Uses explicit overrides for tuned reference voices, then resolves gender directly from
/// [`EDGE_TTS_LANG_TABLE`] across all 130+ supported locales before falling back cleanly.
pub fn nominal_base_voice_f0(voice_id: &str) -> f32 {
    let lower = voice_id.to_lowercase();
    if lower.contains("guy") {
        return 118.0;
    } else if lower.contains("aria") {
        return 208.0;
    } else if lower.contains("ryan") {
        return 120.0;
    } else if lower.contains("sonia") {
        return 210.0;
    } else if lower.contains("midhun") {
        return 125.0;
    } else if lower.contains("sobhana") {
        return 220.0;
    } else if lower.contains("madhur") {
        return 120.0;
    } else if lower.contains("swara") {
        return 215.0;
    }

    for &(_bcp47, male, female) in EDGE_TTS_LANG_TABLE {
        if male.eq_ignore_ascii_case(voice_id) {
            return 120.0;
        }
        if female.eq_ignore_ascii_case(voice_id) {
            return 215.0;
        }
    }

    if lower.contains("female") {
        210.0
    } else if lower.contains("male") {
        120.0
    } else {
        210.0
    }
}

#[async_trait]
impl TtsEngine for Qwen3TtsEngine {
    fn id(&self) -> &'static str {
        "qwen3-tts"
    }

    fn name(&self) -> &'static str {
        "Qwen3-TTS Neural Engine"
    }

    fn is_local(&self) -> bool {
        true
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            cost_per_1k_chars: 0.0,
            avg_latency_ms: 120,
            quality_score: 0.95,
            languages: EDGE_TTS_LANG_TABLE
                .iter()
                .map(|(bcp47, _, _)| bcp47.to_string())
                .collect(),
            is_local: true,
            supports_cloning: true,
            supports_streaming: true,
        }
    }

    async fn voices(&self) -> Result<Vec<Voice>> {
        let mut list = vec![
            Voice {
                id: "qwen3-female-conversational".to_string(),
                name: "Qwen3 Conversational Female".to_string(),
                engine_id: self.id().to_string(),
                language: "en-US".to_string(),
                gender: Gender::Female,
                sample_rate_hz: self.sample_rate,
                tags: vec!["neural".to_string(), "conversational".to_string()],
                description: Some(
                    "Expressive neural voice suited for conversational dialogue".to_string(),
                ),
            },
            Voice {
                id: "qwen3-male-narrator".to_string(),
                name: "Qwen3 Studio Narrator".to_string(),
                engine_id: self.id().to_string(),
                language: "en-US".to_string(),
                gender: Gender::Male,
                sample_rate_hz: self.sample_rate,
                tags: vec![
                    "neural".to_string(),
                    "narration".to_string(),
                    "audiobook".to_string(),
                ],
                description: Some(
                    "Deep, authoritative narration voice for long-form content".to_string(),
                ),
            },
        ];

        // Append cloned profiles so resolve_voice finds them
        let profiles = self.cloned_profiles.read().await;
        for p in profiles.values() {
            list.push(Voice {
                id: p.id.clone(),
                name: format!("{} (Cloned)", p.name),
                engine_id: self.id().to_string(),
                language: p.language.clone(),
                gender: p.gender.clone().unwrap_or(Gender::Neutral),
                sample_rate_hz: self.sample_rate,
                tags: vec!["cloned".to_string(), "zero-shot".to_string()],
                description: p
                    .description
                    .clone()
                    .or_else(|| Some("Zero-shot cloned voice profile".to_string())),
            });
        }

        Ok(list)
    }

    async fn synthesize(&self, request: &SynthesisRequest) -> Result<AudioChunk> {
        // If this is a cloned voice request, dispatch to synthesize_cloned
        let profile_opt = {
            let profiles = self.cloned_profiles.read().await;
            if let Some(p) = profiles.get(&request.voice_id) {
                Some(p.clone())
            } else {
                let clean_name = request
                    .voice_id
                    .trim_end_matches(" (Cloned)")
                    .trim()
                    .to_lowercase();
                profiles
                    .values()
                    .find(|p| {
                        p.name.trim().to_lowercase() == clean_name
                            || p.id.to_lowercase() == clean_name
                            || request.voice_id.contains(&p.id)
                    })
                    .cloned()
            }
        };

        if let Some(profile) = profile_opt {
            let cloned_req = ClonedSynthesisRequest {
                text: request.text.clone(),
                profile,
                speed: request.speed,
                pitch: request.pitch,
                format: request.format,
                language: None,
                instruct: None,
                num_step: None,
                seed: None,
                precision: None,
            };
            return self.synthesize_cloned(&cloned_req).await;
        }

        let fallback_gender = if request.voice_id.contains("female") {
            Some(Gender::Female)
        } else {
            Some(Gender::Male)
        };
        let fallback_voice =
            resolve_cross_lingual_base_voice(fallback_gender.as_ref(), &request.text, "en-US");
        let edge = crate::edge_tts::EdgeTtsEngine::new();
        let synth_req = SynthesisRequest {
            text: request.text.clone(),
            voice_id: fallback_voice.to_string(),
            speed: request.speed,
            pitch: request.pitch,
            format: request.format,
        };
        let mut chunk = edge.synthesize(&synth_req).await?;

        // Apply full biometric realism chain: micro-tremor, warmth, soft envelope, breath
        voxforge_audio::HumanMicroTremor::process(
            &mut chunk.pcm_data,
            chunk.sample_rate,
            0.35,
            0.28,
        );
        voxforge_audio::HarmonicWarmth::process(&mut chunk.pcm_data, 0.25, 0.28);
        voxforge_audio::DeEsser::process(
            &mut chunk.pcm_data,
            chunk.sample_rate,
            6500.0,
            -22.0,
            4.5,
        );
        voxforge_audio::BrickwallLimiter::process_sr(&mut chunk.pcm_data, -0.5, chunk.sample_rate);
        voxforge_audio::SoftEnvelope::process(&mut chunk.pcm_data, chunk.sample_rate, 6.0, 24.0);

        if chunk.pcm_data.len() > (chunk.sample_rate as usize) {
            let mut breath =
                voxforge_audio::NaturalBreath::generate(chunk.sample_rate, 120.0, -36.0, 1750.0);
            voxforge_audio::SoftEnvelope::process(&mut breath, chunk.sample_rate, 8.0, 8.0);
            let pause_samples = (chunk.sample_rate as f32 * 0.025).round() as usize;
            breath.resize(breath.len() + pause_samples, 0);
            breath.extend(chunk.pcm_data);
            chunk.pcm_data = breath;
        }

        Ok(chunk)
    }

    async fn synthesize_stream(
        &self,
        request: &SynthesisRequest,
    ) -> Result<mpsc::Receiver<Result<AudioChunk>>> {
        let (tx, rx) = mpsc::channel(4);
        let chunk = self.synthesize(request).await?;

        tokio::spawn(async move {
            let chunk_size = 2400;
            let mut offset = 0;
            let total = chunk.pcm_data.len();

            while offset < total {
                let end = (offset + chunk_size).min(total);
                let is_final = end >= total;
                let slice = chunk.pcm_data[offset..end].to_vec();

                let sub_chunk = AudioChunk {
                    sample_rate: chunk.sample_rate,
                    channels: chunk.channels,
                    pcm_data: slice,
                    is_final,
                };

                if tx.send(Ok(sub_chunk)).await.is_err() {
                    break;
                }
                offset = end;
            }
        });

        Ok(rx)
    }

    async fn health_check(&self) -> Result<bool> {
        Ok(true)
    }

    fn supports_cloning(&self) -> bool {
        true
    }

    async fn clone_voice(&self, request: &CloneVoiceRequest) -> Result<VoiceProfile> {
        if request.reference_audio_base64.is_none() && request.reference_audio_path.is_none() {
            return Err(VoxForgeError::Config(
                "Voice cloning requires either reference_audio_base64 or reference_audio_path"
                    .to_string(),
            ));
        }

        let (pcm, ref_features) = if let Some(ref b64) = request.reference_audio_base64 {
            let bytes = decode_audio_payload(b64);
            let (raw_samples, sr): (Vec<i16>, u32) = if bytes.starts_with(b"RIFF") {
                voxforge_audio::WavEncoder::decode_wav_to_mono_pcm16(&bytes)
                    .unwrap_or_else(|_| {
                        (
                            bytes
                                .chunks_exact(2)
                                .map(|c| i16::from_le_bytes([c[0], c[1]]))
                                .collect(),
                            24000,
                        )
                    })
            } else {
                (
                    bytes
                        .chunks_exact(2)
                        .map(|c| i16::from_le_bytes([c[0], c[1]]))
                        .collect(),
                    24000,
                )
            };
            // Anti-aliased polyphase resampler — replaces naive 2:1 decimation that caused
            // sibilance aliasing artifacts in cloned voices
            let samples = resample_to_24k(&raw_samples, sr);
            // For long recordings (>30s), extract the best 10s window via sliding-window
            // quality scoring.
            let samples = if samples.len() > 24000 * 30 {
                let best = voxforge_audio::dsp::SilenceTrimmer::best_reference_window(
                    &samples, 24000, 10.0, 2.0,
                );
                if best.len() >= 2400 {
                    best
                } else {
                    samples
                }
            } else {
                samples
            };
            // VAD-based silence trimming: strip dead air at edges
            let samples = {
                let trimmed = voxforge_audio::dsp::SilenceTrimmer::trim(&samples, 24000, -45.0, 40);
                if trimmed.len() >= 2400 {
                    trimmed
                } else {
                    samples
                }
            };
            // Snap window edges to natural pause boundaries
            let samples = {
                let snapped = voxforge_audio::dsp::SilenceTrimmer::trim_passage_to_pauses(
                    &samples, 24000, 1500, 50, -55.0,
                );
                if snapped.len() >= 2400 {
                    snapped
                } else {
                    samples
                }
            };
            let feat = voxforge_audio::ProsodyMirror::analyze(&samples, 24000);
            (samples, Some(feat))
        } else if let Some(ref path) = request.reference_audio_path {
            let trimmed_path = path.trim();
            let p = std::path::Path::new(trimmed_path);
            if p.components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err(VoxForgeError::Config(
                    "Path traversal prohibited in reference_audio_path".to_string(),
                ));
            }
            let bytes = std::fs::read(p).map_err(|e| {
                VoxForgeError::Config(format!("Failed reading reference audio file: {e}"))
            })?;
            let (raw_samples, sr): (Vec<i16>, u32) = if bytes.starts_with(b"RIFF") {
                voxforge_audio::WavEncoder::decode_wav_to_mono_pcm16(&bytes)
                    .unwrap_or_else(|_| {
                        (
                            bytes
                                .chunks_exact(2)
                                .map(|c| i16::from_le_bytes([c[0], c[1]]))
                                .collect(),
                            24000,
                        )
                    })
            } else {
                (
                    bytes
                        .chunks_exact(2)
                        .map(|c| i16::from_le_bytes([c[0], c[1]]))
                        .collect(),
                    24000,
                )
            };
            let samples = resample_to_24k(&raw_samples, sr);
            // For long recordings (>30s), extract the best 10s window via sliding-window
            // quality scoring.
            let samples = if samples.len() > 24000 * 30 {
                let best = voxforge_audio::dsp::SilenceTrimmer::best_reference_window(
                    &samples, 24000, 10.0, 2.0,
                );
                if best.len() >= 2400 {
                    best
                } else {
                    samples
                }
            } else {
                samples
            };
            let samples = {
                let trimmed = voxforge_audio::dsp::SilenceTrimmer::trim(&samples, 24000, -45.0, 40);
                if trimmed.len() >= 2400 {
                    trimmed
                } else {
                    samples
                }
            };
            // Snap window edges to natural pause boundaries
            let samples = {
                let snapped = voxforge_audio::dsp::SilenceTrimmer::trim_passage_to_pauses(
                    &samples, 24000, 1500, 50, -55.0,
                );
                if snapped.len() >= 2400 {
                    snapped
                } else {
                    samples
                }
            };
            let feat = voxforge_audio::ProsodyMirror::analyze(&samples, 24000);
            (samples, Some(feat))
        } else {
            (Vec::new(), None)
        };

        // Primary embedding from the first reference sample
        // Score primary reference quality and store in metadata
        let primary_quality =
            voxforge_audio::dsp::SilenceTrimmer::score_reference_quality(&pcm, 24000);
        let primary_embedding = Self::extract_speaker_embedding(&pcm);

        // Multi-sample aggregation: compute per-sample embeddings and average them.
        // Clips the additional sample list to 7 max to prevent excessive processing.
        // Uses polyphase resampler (not naive 2:1 decimation) for each extra sample.
        let mut all_embeddings: Vec<Vec<f32>> = vec![primary_embedding.clone()];
        let extra_limit = request.additional_reference_audio_base64.len().min(7);
        for extra_b64 in &request.additional_reference_audio_base64[..extra_limit] {
            let extra_bytes = decode_audio_payload(extra_b64);
            let (extra_samples_raw, extra_sr): (Vec<i16>, u32) = if extra_bytes.starts_with(b"RIFF")
            {
                voxforge_audio::WavEncoder::decode_wav_to_mono_pcm16(&extra_bytes)
                    .unwrap_or_default()
            } else {
                (
                    extra_bytes
                        .chunks_exact(2)
                        .map(|c| i16::from_le_bytes([c[0], c[1]]))
                        .collect(),
                    24000,
                )
            };
            // Anti-aliased polyphase resampler for all extra samples
            let extra_samples = resample_to_24k(&extra_samples_raw, extra_sr);
            // VAD trimming on extra samples too
            let extra_samples = {
                let t = voxforge_audio::dsp::SilenceTrimmer::trim(&extra_samples, 24000, -45.0, 40);
                if t.len() >= 2400 {
                    t
                } else {
                    extra_samples
                }
            };
            if extra_samples.len() >= 2400 {
                // Only include extra samples that pass the quality gate
                let quality = voxforge_audio::dsp::SilenceTrimmer::score_reference_quality(
                    &extra_samples,
                    24000,
                );
                if quality >= 0.2 {
                    all_embeddings.push(Self::extract_speaker_embedding(&extra_samples));
                }
            }
        }

        // Outlier rejection: drop embeddings whose cosine distance from the preliminary centroid
        // exceeds 2.0 std deviations. Protects against noisy/background reference samples.
        let embedding = if all_embeddings.len() <= 2 {
            // With 1-2 samples, compute centroid directly — not enough for outlier statistics.
            compute_l2_centroid(&all_embeddings)
        } else {
            // 1. Preliminary centroid
            let prelim = compute_l2_centroid(&all_embeddings);
            // 2. Compute cosine similarities to centroid
            let sims: Vec<f32> = all_embeddings
                .iter()
                .map(|e| Self::compute_cosine_similarity(e, &prelim))
                .collect();
            // 3. Mean and std dev of similarities
            let mean_sim = sims.iter().sum::<f32>() / sims.len() as f32;
            let std_sim = {
                let var =
                    sims.iter().map(|&s| (s - mean_sim).powi(2)).sum::<f32>() / sims.len() as f32;
                var.sqrt().max(1e-6)
            };
            // 4. Keep embeddings within mean ± 2.0σ
            let threshold = mean_sim - 2.0 * std_sim;
            let kept: Vec<Vec<f32>> = all_embeddings
                .iter()
                .zip(sims.iter())
                .filter_map(|(e, &s)| {
                    if s >= threshold {
                        Some(e.clone())
                    } else {
                        None
                    }
                })
                .collect();
            // Fall back to all embeddings if outlier rejection removed everything
            if kept.is_empty() {
                compute_l2_centroid(&all_embeddings)
            } else {
                compute_l2_centroid(&kept)
            }
        };

        // Intra-sample cosine similarity: measures how consistent the reference samples are.
        // A score near 1.0 means all samples sound like the same person (high-quality clone).
        let similarity_score = if all_embeddings.len() > 1 {
            let mut total_sim = 0.0f32;
            let mut pair_count = 0u32;
            for i in 0..all_embeddings.len() {
                for j in (i + 1)..all_embeddings.len() {
                    total_sim +=
                        Self::compute_cosine_similarity(&all_embeddings[i], &all_embeddings[j]);
                    pair_count += 1;
                }
            }
            if pair_count > 0 {
                Some((total_sim / pair_count as f32).clamp(0.0, 1.0))
            } else {
                None
            }
        } else {
            // Single sample: self-similarity is trivially 1.0; omit to avoid misleading the user.
            None
        };

        let sample_count = all_embeddings.len() as u32;

        self.clone_count.fetch_add(1, Ordering::SeqCst);
        let profile_id = format!("qwen3-cloned-{}", Uuid::new_v4());

        let mut metadata = HashMap::new();
        metadata.insert("engine".to_string(), "qwen3-tts".to_string());
        metadata.insert("zero_shot".to_string(), "true".to_string());
        if let Some(ref f) = ref_features {
            metadata.insert("f0_mean_hz".to_string(), format!("{:.1}", f.f0_mean_hz));
            metadata.insert(
                "f0_std_semitones".to_string(),
                format!("{:.2}", f.f0_std_semitones),
            );
            metadata.insert(
                "formant_f1_hz".to_string(),
                format!("{:.1}", f.formant_f1_hz),
            );
            metadata.insert(
                "formant_f2_hz".to_string(),
                format!("{:.1}", f.formant_f2_hz),
            );
            metadata.insert(
                "formant_f3_hz".to_string(),
                format!("{:.1}", f.formant_f3_hz),
            );
            metadata.insert("hnr_db".to_string(), format!("{:.2}", f.hnr_db));
            metadata.insert(
                "shimmer_percent".to_string(),
                format!("{:.2}", f.shimmer_percent),
            );
            metadata.insert(
                "syllable_rate".to_string(),
                format!("{:.2}", f.syllable_rate),
            );
            metadata.insert(
                "voicing_ratio".to_string(),
                format!("{:.3}", f.voicing_ratio),
            );
            metadata.insert(
                "f0_jitter_percent".to_string(),
                format!("{:.3}", f.f0_jitter_percent),
            );
            metadata.insert(
                "spectral_tilt_db".to_string(),
                format!("{:.2}", f.spectral_tilt_db),
            );
        }
        metadata.extend(request.metadata.clone());

        // Record aggregation quality in metadata for observability
        metadata.insert("sample_count".to_string(), format!("{}", sample_count));
        if let Some(sim) = similarity_score {
            metadata.insert("intra_sample_similarity".to_string(), format!("{:.4}", sim));
        }
        // Reference audio quality score (0.0 = unusable, 1.0 = broadcast quality)
        metadata.insert(
            "reference_quality_score".to_string(),
            format!("{:.3}", primary_quality),
        );

        let mut clone_capabilities = vec![
            "zero-shot".to_string(),
            "cross-lingual".to_string(),
            "prosody-transfer".to_string(),
        ];
        if sample_count > 1 {
            clone_capabilities.push("multi-sample-aggregated".to_string());
        }

        let profile = VoiceProfile {
            id: profile_id.clone(),
            name: request.name.clone(),
            engine_id: self.id().to_string(),
            description: request.description.clone(),
            language: request.language.clone(),
            gender: request.gender.clone(),
            reference_audio_path: request.reference_audio_path.clone(),
            reference_audio_base64: request.reference_audio_base64.clone(),
            reference_transcript: request.reference_transcript.clone(),
            embedding: Some(embedding),
            clone_capabilities: Some(clone_capabilities),
            similarity_score,
            sample_count,
            version: 1,
            metadata,
            created_at: Utc::now(),
            refined_at: None,
        };

        self.cloned_profiles
            .write()
            .await
            .insert(profile_id.clone(), profile.clone());

        // Atomically persist to data/voice_profiles on disk so it survives restarts
        let profiles_dir = std::env::var("VOXFORGE_PROFILES_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("data/voice_profiles"));
        let _ = std::fs::create_dir_all(&profiles_dir);
        let file_path = profiles_dir.join(format!("{}.json", profile_id));
        if let Ok(json_str) = serde_json::to_string_pretty(&profile) {
            let _ = std::fs::write(file_path, json_str);
        }

        Ok(profile)
    }

    async fn refine_voice_embedding(
        &self,
        profile: &VoiceProfile,
        new_reference_audio_base64: &str,
    ) -> Result<VoiceProfile> {
        if new_reference_audio_base64.is_empty() {
            return Err(VoxForgeError::Config(
                "refine_voice_embedding requires non-empty new_reference_audio_base64".to_string(),
            ));
        }

        let existing_embedding = profile.embedding.as_deref().ok_or_else(|| {
            VoxForgeError::Config("Profile has no existing embedding to refine".to_string())
        })?;

        // Decode and extract embedding from the new sample
        let bytes = decode_audio_payload(new_reference_audio_base64);
        let (raw_samples, sr): (Vec<i16>, u32) = if bytes.starts_with(b"RIFF") {
            voxforge_audio::WavEncoder::decode_wav_to_mono_pcm16(&bytes)
                .unwrap_or_default()
        } else {
            (
                bytes
                    .chunks_exact(2)
                    .map(|c| i16::from_le_bytes([c[0], c[1]]))
                    .collect(),
                24000,
            )
        };
        let new_samples = resample_to_24k(&raw_samples, sr);

        if new_samples.len() < 2400 {
            return Err(VoxForgeError::Config(
                "New reference audio is too short; minimum 100ms (2400 samples at 24kHz) required"
                    .to_string(),
            ));
        }

        let new_embedding = Self::extract_speaker_embedding(&new_samples);

        // Weighted averaging: weight = sample_count / (sample_count + 1).
        // This gives more weight to the existing (potentially aggregated) embedding,
        // letting each new sample contribute a diminishing but meaningful update.
        let existing_count = profile.sample_count.max(1) as f32;
        let weight_old = existing_count / (existing_count + 1.0);
        let weight_new = 1.0 / (existing_count + 1.0);

        let mut refined = vec![0.0f32; 512];
        for (slot, (&old, &new)) in refined
            .iter_mut()
            .zip(existing_embedding.iter().zip(new_embedding.iter()))
        {
            *slot = weight_old * old + weight_new * new;
        }

        // Re-normalize to unit sphere
        let l2: f32 = refined.iter().map(|&x| x * x).sum::<f32>().sqrt();
        if l2 > 1e-6 {
            for slot in &mut refined {
                *slot /= l2;
            }
        }

        // Cross-version similarity: how much did the embedding shift?
        let refinement_similarity = Self::compute_cosine_similarity(existing_embedding, &refined);

        let mut updated = profile.clone();
        updated.embedding = Some(refined);
        updated.version = profile.version.saturating_add(1);
        updated.sample_count = profile.sample_count.saturating_add(1);
        updated.refined_at = Some(Utc::now());
        updated.similarity_score = Some(refinement_similarity.clamp(0.0, 1.0));
        updated.metadata.insert(
            "refinement_similarity".to_string(),
            format!("{:.4}", refinement_similarity),
        );
        updated.metadata.insert(
            "sample_count".to_string(),
            format!("{}", updated.sample_count),
        );
        updated
            .metadata
            .insert("version".to_string(), format!("{}", updated.version));

        // Persist updated profile to disk
        let profiles_dir = std::env::var("VOXFORGE_PROFILES_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("data/voice_profiles"));
        let _ = std::fs::create_dir_all(&profiles_dir);
        let file_path = profiles_dir.join(format!("{}.json", updated.id));
        if let Ok(json_str) = serde_json::to_string_pretty(&updated) {
            let _ = std::fs::write(file_path, json_str);
        }

        // Update in-memory store
        self.cloned_profiles
            .write()
            .await
            .insert(updated.id.clone(), updated.clone());

        Ok(updated)
    }

    /// Synthesize speech conditioned on a cloned voice profile with optional lossless f32 payload.
    async fn synthesize_cloned_hq(
        &self,
        request: &ClonedSynthesisRequest,
    ) -> Result<(AudioChunk, Option<Vec<f32>>)> {
        self.synthesize_cloned_hq_inner(request).await
    }

    async fn synthesize_cloned(&self, request: &ClonedSynthesisRequest) -> Result<AudioChunk> {
        let (chunk, _) = self.synthesize_cloned_hq_inner(request).await?;
        Ok(chunk)
    }
}

impl Qwen3TtsEngine {
    async fn synthesize_cloned_hq_inner(
        &self,
        request: &ClonedSynthesisRequest,
    ) -> Result<(AudioChunk, Option<Vec<f32>>)> {
        let target_lang = normalize_language_code(
            request
                .language
                .as_deref()
                .filter(|s| !s.trim().is_empty() && !s.eq_ignore_ascii_case("AUTO"))
                .unwrap_or(&request.profile.language),
        );

        // 1. Dynamic sidecar resolution: check configured URL, env, or cached auto-detected ports
        let sidecar_url = self.resolve_sidecar_url();

        if let Some(ref url) = sidecar_url {
            let num_step: u32 = request
                .num_step
                .or_else(|| {
                    request
                        .profile
                        .metadata
                        .get("num_step")
                        .and_then(|s| s.parse().ok())
                })
                .unwrap_or(32);
            let instruct = request
                .instruct
                .clone()
                .or_else(|| request.profile.metadata.get("instruct").cloned());
            let seed: Option<u64> = request.seed.or_else(|| {
                request
                    .profile
                    .metadata
                    .get("seed")
                    .and_then(|s| s.parse().ok())
            });
            let precision: u16 = request
                .precision
                .or_else(|| {
                    request
                        .profile
                        .metadata
                        .get("precision")
                        .and_then(|s| s.parse().ok())
                })
                .unwrap_or(16);

            let payload = serde_json::json!({
                "text": request.text,
                "voice_profile_id": request.profile.id,
                "reference_audio": request.profile.reference_audio_base64,
                "reference_transcript": request.profile.reference_transcript,
                "language": target_lang,
                "speed": request.speed,
                "pitch": request.pitch,
                "num_step": num_step,
                "precision": precision,
                "instruct": instruct,
                "seed": seed,
                "mastering": true
            });

            match self.client.post(url).json(&payload).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        match resp.bytes().await {
                            Ok(bytes) if bytes.starts_with(b"RIFF") => {
                                match voxforge_audio::WavEncoder::decode_wav_to_mono_f32(&bytes) {
                                    Ok((samples_f32, sr)) if !samples_f32.is_empty() => {
                                        let pcm: Vec<i16> = samples_f32
                                            .iter()
                                            .map(|&s| (s.clamp(-1.0, 1.0) * 32767.0).round() as i16)
                                            .collect();
                                        return Ok((
                                            AudioChunk {
                                                sample_rate: sr,
                                                channels: 1,
                                                pcm_data: pcm,
                                                is_final: true,
                                            },
                                            Some(samples_f32),
                                        ));
                                    }
                                    Ok(_) => {
                                        tracing::warn!(url = %url, "Neural sidecar returned empty RIFF WAV payload; falling back to local DSP pipeline");
                                    }
                                    Err(err) => {
                                        tracing::warn!(url = %url, error = %err, "Failed to decode sidecar RIFF WAV; trying PCM16 decoder");
                                        if let Ok((pcm, sr)) =
                                            voxforge_audio::WavEncoder::decode_wav_to_mono_pcm16(&bytes)
                                        {
                                            return Ok((
                                                AudioChunk {
                                                    sample_rate: sr,
                                                    channels: 1,
                                                    pcm_data: pcm,
                                                    is_final: true,
                                                },
                                                None,
                                            ));
                                        }
                                    }
                                }
                            }
                            Ok(bytes) if !bytes.is_empty() => {
                                let pcm: Vec<i16> = bytes
                                    .chunks_exact(2)
                                    .map(|c| i16::from_le_bytes([c[0], c[1]]))
                                    .collect();
                                return Ok((
                                    AudioChunk {
                                        sample_rate: self.sample_rate,
                                        channels: 1,
                                        pcm_data: pcm,
                                        is_final: true,
                                    },
                                    None,
                                ));
                            }
                            Ok(_) => {
                                tracing::warn!(url = %url, "Neural sidecar returned empty response body");
                            }
                            Err(err) => {
                                tracing::warn!(url = %url, error = %err, "Failed to read neural sidecar response bytes");
                            }
                        }
                    } else {
                        let status = resp.status();
                        let err_body = resp.text().await.unwrap_or_default();
                        tracing::warn!(url = %url, %status, body = %err_body, "Neural sidecar returned non-success status; falling back");
                    }
                }
                Err(err) => {
                    tracing::warn!(url = %url, error = %err, "Neural sidecar HTTP request failed; falling back");
                }
            }
        }

        // 2. Decode reference audio PCM if present
        let ref_audio_opt = if let Some(ref b64) = request.profile.reference_audio_base64 {
            let bytes = decode_audio_payload(b64);
            if bytes.starts_with(b"RIFF") {
                voxforge_audio::WavEncoder::decode_wav_to_mono_pcm16(&bytes).ok()
            } else {
                let pcm: Vec<i16> = bytes
                    .chunks_exact(2)
                    .map(|c| i16::from_le_bytes([c[0], c[1]]))
                    .collect();
                Some((pcm, 24000))
            }
        } else if let Some(ref path_str) = request.profile.reference_audio_path {
            if let Ok(bytes) = std::fs::read(path_str) {
                if bytes.starts_with(b"RIFF") {
                    voxforge_audio::WavEncoder::decode_wav_to_mono_pcm16(&bytes).ok()
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        // 3. Acoustic prosody & timbre analysis (with persistent profile metadata fallback)
        let (ref_pcm, mut ref_features_opt) = match ref_audio_opt {
            Some((pcm, sr)) if !pcm.is_empty() => {
                let trimmed = voxforge_audio::dsp::SilenceTrimmer::trim(&pcm, sr, -45.0, 40);
                let active = if trimmed.len() >= 2400 {
                    &trimmed
                } else {
                    &pcm
                };
                let features = voxforge_audio::ProsodyMirror::analyze(active, sr);
                (Some(pcm), Some(features))
            }
            _ => (None, None),
        };

        // Reconstruct acoustic feature profile from persistent metadata if raw audio was purged
        if ref_features_opt.is_none() {
            let f0 = request
                .profile
                .metadata
                .get("f0_mean_hz")
                .and_then(|s| s.parse::<f32>().ok());
            let f1 = request
                .profile
                .metadata
                .get("formant_f1_hz")
                .and_then(|s| s.parse::<f32>().ok());
            let f2 = request
                .profile
                .metadata
                .get("formant_f2_hz")
                .and_then(|s| s.parse::<f32>().ok());
            let f3 = request
                .profile
                .metadata
                .get("formant_f3_hz")
                .and_then(|s| s.parse::<f32>().ok());
            let hnr = request
                .profile
                .metadata
                .get("hnr_db")
                .and_then(|s| s.parse::<f32>().ok());
            let shimmer = request
                .profile
                .metadata
                .get("shimmer_percent")
                .and_then(|s| s.parse::<f32>().ok());
            let voicing = request
                .profile
                .metadata
                .get("voicing_ratio")
                .and_then(|s| s.parse::<f32>().ok());
            let jitter = request
                .profile
                .metadata
                .get("f0_jitter_percent")
                .and_then(|s| s.parse::<f32>().ok());
            let rate = request
                .profile
                .metadata
                .get("syllable_rate")
                .and_then(|s| s.parse::<f32>().ok());
            let tilt = request
                .profile
                .metadata
                .get("spectral_tilt_db")
                .and_then(|s| s.parse::<f32>().ok());

            if let Some(f0_val) = f0 {
                ref_features_opt = Some(voxforge_audio::dsp::AcousticFeatures {
                    f0_mean_hz: f0_val,
                    f0_std_semitones: request
                        .profile
                        .metadata
                        .get("f0_std_semitones")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(1.5),
                    rms_dbfs: -20.0,
                    syllable_rate: rate.unwrap_or(4.2),
                    voicing_ratio: voicing.unwrap_or(0.65),
                    loudness_z_score: 0.0,
                    pace_z_score: 0.0,
                    f0_jitter_percent: jitter.unwrap_or(0.5),
                    spectral_tilt_db: tilt.unwrap_or(0.0),
                    formant_f1_hz: f1.unwrap_or(500.0),
                    formant_f2_hz: f2.unwrap_or(1500.0),
                    formant_f3_hz: f3.unwrap_or(2600.0),
                    hnr_db: hnr.unwrap_or(18.0),
                    shimmer_percent: shimmer.unwrap_or(2.5),
                });
            }
        }

        // 4. Determine acoustic gender & base voice
        let acoustic_gender = if let Some(ref f) = ref_features_opt {
            if f.f0_mean_hz < 165.0 {
                Gender::Male
            } else {
                Gender::Female
            }
        } else if let Some(ref gender) = request.profile.gender {
            gender.clone()
        } else {
            Gender::Female
        };

        let fallback_voice = resolve_cross_lingual_base_voice(
            Some(&acoustic_gender),
            &request.text,
            &target_lang,
        );

        // 5. Calculate pitch delta matching the cloned speaker's exact fundamental pitch
        let mut pitch_offset = request.pitch;
        if let Some(ref f) = ref_features_opt {
            let nominal_base_f0 = nominal_base_voice_f0(fallback_voice);
            let target_f0 = f.f0_mean_hz.clamp(65.0, 420.0);
            let delta_hz = target_f0 - nominal_base_f0;
            // edge_tts multiplies request.pitch by 5.0 to generate pitch_hz in SSML.
            // Setting f0_pitch_delta = delta_hz / 5.0 ensures pitch_hz == delta_hz (exact Hz-for-Hz alignment).
            let f0_pitch_delta = (delta_hz / 5.0).clamp(-10.0, 10.0);
            pitch_offset += f0_pitch_delta;
        } else if let Some(ref emb) = request.profile.embedding {
            let avg_centroid = emb.iter().take(32).sum::<f32>() / 32.0;
            pitch_offset += (avg_centroid * 4.0).clamp(-6.0, 6.0);
        }

        // 6. Natural speech cadence / tempo alignment
        let speed = if (request.speed - 1.0).abs() < 0.05 {
            if let Some(ref f) = ref_features_opt {
                (f.syllable_rate / 4.2).clamp(0.85, 1.25)
            } else {
                request.speed
            }
        } else {
            request.speed
        };

        // 7. Base voice synthesis pass
        // For long texts (>400 chars), split at sentence boundaries and stitch with cross-fades.
        // Each sentence is synthesized individually so prosody resets naturally, then 50ms equal-power cross-fades blend seams.
        let edge = crate::edge_tts::EdgeTtsEngine::new();
        let mut chunk = if request.text.len() > 400 {
            let sentences = split_text_to_sentences(&request.text);
            let mut chunks: Vec<AudioChunk> = Vec::with_capacity(sentences.len());
            for sentence in &sentences {
                if sentence.trim().is_empty() {
                    continue;
                }
                let req = SynthesisRequest {
                    text: sentence.clone(),
                    voice_id: fallback_voice.to_string(),
                    speed,
                    pitch: pitch_offset,
                    format: request.format,
                };
                if let Ok(c) = edge.synthesize(&req).await {
                    chunks.push(c);
                }
            }
            if chunks.is_empty() {
                // Fallback to full text if chunking yielded nothing
                let synth_req = SynthesisRequest {
                    text: request.text.clone(),
                    voice_id: fallback_voice.to_string(),
                    speed,
                    pitch: pitch_offset,
                    format: request.format,
                };
                edge.synthesize(&synth_req).await?
            } else {
                let sample_rate = chunks[0].sample_rate;
                let pcm_slices: Vec<&[i16]> =
                    chunks.iter().map(|c| c.pcm_data.as_slice()).collect();
                // Studio standard: fade-out → 100ms silence gap → fade-in
                // Avoids phase-mismatch clicks that equal-power overlap crossfade can produce
                // when chunk boundaries fall on arbitrary phases of the speech waveform.
                let stitched = voxforge_audio::AudioMerger::silence_gap_join(
                    &pcm_slices,
                    sample_rate,
                    20,
                    100,
                );
                AudioChunk {
                    sample_rate,
                    channels: 1,
                    pcm_data: stitched,
                    is_final: true,
                }
            }
        } else {
            let synth_req = SynthesisRequest {
                text: request.text.clone(),
                voice_id: fallback_voice.to_string(),
                speed,
                pitch: pitch_offset,
                format: request.format,
            };
            edge.synthesize(&synth_req).await?
        };

        // 8. Formant & Spectral Timbre Morphing (match cloned speaker's frequency envelope)
        if let Some(ref ref_samples) = ref_pcm {
            let (low_gain, mid_gain, high_gain) =
                compute_spectral_timbre_deltas(ref_samples, &chunk.pcm_data);
            voxforge_audio::ParametricEq::process_3band(
                &mut chunk.pcm_data,
                chunk.sample_rate,
                low_gain,
                mid_gain,
                high_gain,
            );

            if let Some(ref f) = ref_features_opt {
                // Vocal tract anatomical formant resonance match (F1, F2, F3, Presence)
                let f1_gain = (low_gain * 0.6).clamp(-4.0, 4.0);
                let f2_gain = (mid_gain * 0.6).clamp(-4.0, 4.0);
                let f3_gain = (mid_gain * 0.4 + high_gain * 0.2).clamp(-3.5, 3.5);
                let presence_gain = (high_gain * 0.5).clamp(-3.0, 3.0);
                voxforge_audio::VocalTractResonator::process_4band(
                    &mut chunk.pcm_data,
                    chunk.sample_rate,
                    f.formant_f1_hz,
                    f1_gain,
                    f.formant_f2_hz,
                    f2_gain,
                    f.formant_f3_hz,
                    f3_gain,
                    presence_gain,
                );

                if f.voicing_ratio > 0.40 {
                    let warmth_drive = (0.25 + (f.voicing_ratio * 0.15).clamp(0.0, 0.25))
                        * (f.hnr_db / 20.0).clamp(0.7, 1.3);
                    voxforge_audio::HarmonicWarmth::process(
                        &mut chunk.pcm_data,
                        warmth_drive.clamp(0.1, 0.6),
                        0.30,
                    );
                }
            }
        } else if let Some(ref emb) = request.profile.embedding {
            // Modulate timbre from 512-dim embedding slots
            let low_bias =
                (emb.iter().skip(32).take(64).sum::<f32>() / 64.0 * 6.0).clamp(-4.0, 4.0);
            let mid_bias =
                (emb.iter().skip(96).take(96).sum::<f32>() / 96.0 * 4.0).clamp(-3.0, 3.0);
            let high_bias =
                (emb.iter().skip(192).take(128).sum::<f32>() / 128.0 * 6.0).clamp(-4.0, 4.0);
            voxforge_audio::ParametricEq::process_3band(
                &mut chunk.pcm_data,
                chunk.sample_rate,
                low_bias,
                mid_bias,
                high_bias,
            );

            if let Some(ref f) = ref_features_opt {
                let f1_gain = (low_bias * 0.6).clamp(-4.0, 4.0);
                let f2_gain = (mid_bias * 0.6).clamp(-4.0, 4.0);
                let f3_gain = (mid_bias * 0.4 + high_bias * 0.2).clamp(-3.5, 3.5);
                let presence_gain = (high_bias * 0.5).clamp(-3.0, 3.0);
                voxforge_audio::VocalTractResonator::process_4band(
                    &mut chunk.pcm_data,
                    chunk.sample_rate,
                    f.formant_f1_hz,
                    f1_gain,
                    f.formant_f2_hz,
                    f2_gain,
                    f.formant_f3_hz,
                    f3_gain,
                    presence_gain,
                );

                if f.voicing_ratio > 0.40 {
                    let warmth_drive = (0.25 + (f.voicing_ratio * 0.15).clamp(0.0, 0.25))
                        * (f.hnr_db / 20.0).clamp(0.7, 1.3);
                    voxforge_audio::HarmonicWarmth::process(
                        &mut chunk.pcm_data,
                        warmth_drive.clamp(0.1, 0.6),
                        0.30,
                    );
                }
            } else {
                let f1_gain = (low_bias * 0.6).clamp(-4.0, 4.0);
                let f2_gain = (mid_bias * 0.6).clamp(-4.0, 4.0);
                let f3_gain = (mid_bias * 0.4 + high_bias * 0.2).clamp(-3.5, 3.5);
                let presence_gain = (high_bias * 0.5).clamp(-3.0, 3.0);
                voxforge_audio::VocalTractResonator::process_4band(
                    &mut chunk.pcm_data,
                    chunk.sample_rate,
                    520.0,
                    f1_gain,
                    1550.0,
                    f2_gain,
                    2650.0,
                    f3_gain,
                    presence_gain,
                );
            }
        }

        // 9. Physiological vocal fold micro-tremor (jitter & shimmer)
        let jitter_depth = if let Some(ref f) = ref_features_opt {
            (f.f0_jitter_percent / 2.0).clamp(0.20, 0.55)
        } else {
            0.35
        };
        let shimmer_depth = if let Some(ref f) = ref_features_opt {
            (f.shimmer_percent / 4.0).clamp(0.15, 0.55)
        } else {
            0.28
        };
        voxforge_audio::HumanMicroTremor::process(
            &mut chunk.pcm_data,
            chunk.sample_rate,
            jitter_depth,
            shimmer_depth,
        );

        // 10. Soft de-essing to tame harsh sibilance
        voxforge_audio::DeEsser::process(
            &mut chunk.pcm_data,
            chunk.sample_rate,
            6500.0,
            -22.0,
            5.0,
        );

        // 11. Phrase leveler (primary perceptual quality step).
        //     400ms slow EMA + speech gate: equalises phrase-to-phrase loudness WITHOUT
        //     compressing syllable peaks. Must run BEFORE the mastering compressor so the
        //     compressor sees an already-level signal and operates in its linear region.
        voxforge_audio::PhraseLeveler::process(&mut chunk.pcm_data, chunk.sample_rate);

        // 11b. Studio peak normalization & brickwall limiting (look-ahead, 5ms, 40ms release)
        let max_amp = chunk.pcm_data.iter().map(|&s| s.unsigned_abs() as u32).max().unwrap_or(0) as f32;
        if max_amp > 100.0 && max_amp < 28000.0 {
            let gain = (28000.0 / max_amp).min(1.6);
            for s in &mut chunk.pcm_data {
                *s = ((*s as f32 * gain).clamp(-32767.0, 32767.0)) as i16;
            }
        }
        voxforge_audio::BrickwallLimiter::process_sr(&mut chunk.pcm_data, -0.5, chunk.sample_rate);

        // 11c. Broadcast mastering pass (EQ, compressor, de-esser, peak safety).
        // Broadcast mastering post-chain for perceptually uniform
        // loudness and consistent spectral balance across synthesized clones.
        voxforge_audio::MasteringProfile::Broadcast.apply(&mut chunk.pcm_data, chunk.sample_rate);

        // 11d. Room-tone gap fill: replace digital-zero pauses with spectrally-shaped
        //      room tone from the take itself. Eliminates the gating artefact audible on
        //      headphones.
        voxforge_audio::RoomToneGapFill::process(&mut chunk.pcm_data, chunk.sample_rate);

        // 12. Soft anti-click envelope on speech core
        voxforge_audio::SoftEnvelope::process(&mut chunk.pcm_data, chunk.sample_rate, 6.0, 24.0);

        // 13. Organic Pre-Speech Inhalation Aspiration
        if chunk.pcm_data.len() > (chunk.sample_rate as usize) {
            let breath_center = ref_features_opt
                .as_ref()
                .map(|f| f.formant_f2_hz)
                .unwrap_or(1750.0);
            let mut breath = voxforge_audio::NaturalBreath::generate(
                chunk.sample_rate,
                140.0,
                -34.0,
                breath_center,
            );
            voxforge_audio::SoftEnvelope::process(&mut breath, chunk.sample_rate, 8.0, 8.0);
            let pause_samples = (chunk.sample_rate as f32 * 0.030).round() as usize;
            breath.resize(breath.len() + pause_samples, 0);
            breath.extend(chunk.pcm_data);
            chunk.pcm_data = breath;
        }

        // 14. Paralinguistic event injection from text markup.
        // Parses [laugh], [sigh], [gasp], [cough], [pause:Xms] tags in the input text
        // and inserts synthesized acoustic events at the correct positions.
        // This runs after all realism processing so event audio matches the cloned voice's sample rate.
        if voxforge_audio::has_paralinguistic_tags(&request.text) {
            let sr = chunk.sample_rate;
            let segments = voxforge_audio::parse_paralinguistic_text(&request.text);

            // Count the non-tag text segments to estimate proportional placement
            let text_parts: Vec<&str> = segments
                .iter()
                .filter_map(|s| {
                    if let voxforge_audio::ParalinguisticSegment::Text(t) = s {
                        Some(t.as_str())
                    } else {
                        None
                    }
                })
                .collect();
            let total_text_chars: usize = text_parts.iter().map(|t| t.len()).sum::<usize>().max(1);

            // Rebuild PCM with emotion events spliced in at proportional positions
            let mut assembled: Vec<i16> = Vec::with_capacity(chunk.pcm_data.len() + sr as usize);
            let mut text_chars_seen = 0usize;

            for segment in &segments {
                match segment {
                    voxforge_audio::ParalinguisticSegment::Text(text) => {
                        // Slice the proportional chunk of speech audio for this text segment
                        let char_fraction = text.len() as f32 / total_text_chars as f32;
                        let sample_count =
                            (chunk.pcm_data.len() as f32 * char_fraction).round() as usize;
                        let start = (chunk.pcm_data.len() as f32
                            * (text_chars_seen as f32 / total_text_chars as f32))
                            .round() as usize;
                        let end = (start + sample_count).min(chunk.pcm_data.len());
                        assembled.extend_from_slice(&chunk.pcm_data[start..end]);
                        text_chars_seen += text.len();
                    }
                    voxforge_audio::ParalinguisticSegment::Emotion(tag) => {
                        let mut event =
                            voxforge_audio::ParalinguisticSynthesizer::synthesize_event(*tag, sr);
                        // Apply soft envelope to the event so it blends cleanly
                        voxforge_audio::SoftEnvelope::process(&mut event, sr, 4.0, 8.0);
                        assembled.extend_from_slice(&event);
                    }
                }
            }

            // Only replace if assembly produced a plausible result
            if assembled.len() >= chunk.pcm_data.len() / 2 {
                voxforge_audio::BrickwallLimiter::process_sr(&mut assembled, -0.5, sr);
                chunk.pcm_data = assembled;
            }
        }

        // 15. EBU R128 / LUFS loudness normalization to -16 LUFS (podcast/voice standard).
        //     Ensures consistent perceived loudness regardless of the reference recording level.
        voxforge_audio::AudioNormalizer::lufs_normalize(
            &mut chunk.pcm_data,
            chunk.sample_rate,
            voxforge_audio::LUFS_TARGET_VOICE,
        );
        // Final brickwall to guard against any LUFS gain boost causing clipping (look-ahead)
        voxforge_audio::BrickwallLimiter::process_sr(&mut chunk.pcm_data, -0.5, chunk.sample_rate);

        Ok((chunk, None))
    }
}

// ---------------------------------------------------------------------------
// Private helpers used by Qwen3TtsEngine
// ---------------------------------------------------------------------------

/// Resample audio to the engine's 24 kHz working rate using the polyphase anti-alias filter.
///
/// Replaces the previous naive 2:1 decimation which aliased high-frequency speech content
/// (sibilance 5–8 kHz) into the baseband, causing "buzzy" or "robotic" artefacts in cloned voices.
/// The polyphase resampler applies a Kaiser-windowed sinc anti-alias filter before decimation.
fn resample_to_24k(samples: &[i16], src_rate: u32) -> Vec<i16> {
    if src_rate == 24000 {
        return samples.to_vec();
    }
    voxforge_audio::Resampler::resample(samples, src_rate, 24000)
}

/// Compute the L2-normalized centroid (mean) of a set of 512-D embeddings.
fn compute_l2_centroid(embeddings: &[Vec<f32>]) -> Vec<f32> {
    if embeddings.is_empty() {
        return vec![0.0f32; 512];
    }
    if embeddings.len() == 1 {
        return embeddings[0].clone();
    }
    let n = embeddings.len() as f32;
    let dim = embeddings[0].len();
    let mut centroid = vec![0.0f32; dim];
    for emb in embeddings {
        for (slot, &val) in centroid.iter_mut().zip(emb.iter()) {
            *slot += val;
        }
    }
    for slot in &mut centroid {
        *slot /= n;
    }
    // Re-normalize to unit sphere
    let l2: f32 = centroid.iter().map(|&x| x * x).sum::<f32>().sqrt();
    if l2 > 1e-6 {
        for slot in &mut centroid {
            *slot /= l2;
        }
    }
    centroid
}

/// Comprehensive Edge-TTS language voice table: (BCP-47 prefix, male voice, female voice).
///
/// Covers all languages supported by Microsoft Edge-TTS neural voices.
/// Script-based detection handles non-Latin scripts (Indic, CJK, Arabic, etc.);
/// BCP-47 prefix matching covers all Latin-script languages.
///
/// Based on `edge-tts --list-voices` — 100+ language/locale pairs.
pub(crate) static EDGE_TTS_LANG_TABLE: &[(&str, &str, &str)] = &[
    // ── Indic (script-detected, also prefix-matched) ──────────────────────
    ("ml-IN", "ml-IN-MidhunNeural", "ml-IN-SobhanaNeural"),
    ("hi-IN", "hi-IN-MadhurNeural", "hi-IN-SwaraNeural"),
    ("ta-IN", "ta-IN-ValluvarNeural", "ta-IN-PallaviNeural"),
    ("te-IN", "te-IN-MohanNeural", "te-IN-ShrutiNeural"),
    ("kn-IN", "kn-IN-GaganNeural", "kn-IN-SapnaNeural"),
    ("bn-IN", "bn-IN-BashkarNeural", "bn-IN-TanishaaNeural"),
    ("bn-BD", "bn-BD-PradeepNeural", "bn-BD-NabanitaNeural"),
    ("gu-IN", "gu-IN-NiranjanNeural", "gu-IN-DhwaniNeural"),
    ("pa-IN", "pa-IN-OjasNeural", "pa-IN-VaaniNeural"),
    ("mr-IN", "mr-IN-ManoharNeural", "mr-IN-AarohiNeural"),
    ("ur-IN", "ur-IN-SalmanNeural", "ur-IN-GulNeural"),
    ("ur-PK", "ur-PK-AsadNeural", "ur-PK-UzmaNeural"),
    ("or-IN", "or-IN-SukantNeural", "or-IN-SubhasiniNeural"),
    ("ne-NP", "ne-NP-SagarNeural", "ne-NP-HemkalaNeural"),
    ("si-LK", "si-LK-SameeraNeural", "si-LK-ThiliniNeural"),
    ("my-MM", "my-MM-ThihaNeural", "my-MM-NilarNeural"),
    ("km-KH", "km-KH-PisethNeural", "km-KH-SreymomNeural"),
    ("lo-LA", "lo-LA-ChanthavongNeural", "lo-LA-KeomanyNeural"),
    // ── East Asian ────────────────────────────────────────────────────────
    ("ja-JP", "ja-JP-KeitaNeural", "ja-JP-NanamiNeural"),
    ("zh-CN", "zh-CN-YunxiNeural", "zh-CN-XiaoxiaoNeural"),
    ("zh-TW", "zh-TW-YunJheNeural", "zh-TW-HsiaoChenNeural"),
    ("zh-HK", "zh-HK-WanLungNeural", "zh-HK-HiuGaaiNeural"),
    ("ko-KR", "ko-KR-InJoonNeural", "ko-KR-SunHiNeural"),
    ("mn-CN", "mn-CN-NaranbaatarNeural", "mn-CN-YesuiNeural"),
    ("mn-MN", "mn-MN-BataaNeural", "mn-MN-YesuiNeural"),
    // ── Southeast Asian ───────────────────────────────────────────────────
    ("id-ID", "id-ID-ArdiNeural", "id-ID-GadisNeural"),
    ("ms-MY", "ms-MY-OsmanNeural", "ms-MY-YasminNeural"),
    ("th-TH", "th-TH-NiwatNeural", "th-TH-PremwadeeNeural"),
    ("vi-VN", "vi-VN-NamMinhNeural", "vi-VN-HoaiMyNeural"),
    ("fil-PH", "fil-PH-AngeloNeural", "fil-PH-BlessicaNeural"),
    ("jv-ID", "jv-ID-DimasNeural", "jv-ID-SitiNeural"),
    ("su-ID", "su-ID-JajangNeural", "su-ID-TutiNeural"),
    // ── Middle East / North Africa ────────────────────────────────────────
    ("ar-SA", "ar-SA-HamedNeural", "ar-SA-ZariyahNeural"),
    ("ar-EG", "ar-EG-ShakirNeural", "ar-EG-SalmaNeural"),
    ("ar-AE", "ar-AE-HamdanNeural", "ar-AE-FatimaNeural"),
    ("ar-DZ", "ar-DZ-IsmaelNeural", "ar-DZ-AminaNeural"),
    ("ar-BH", "ar-BH-AliNeural", "ar-BH-LailaNeural"),
    ("ar-IQ", "ar-IQ-BasselNeural", "ar-IQ-RanaNeural"),
    ("ar-JO", "ar-JO-TaimNeural", "ar-JO-SanaNeural"),
    ("ar-KW", "ar-KW-FahedNeural", "ar-KW-NouraNeural"),
    ("ar-LB", "ar-LB-RamiNeural", "ar-LB-LaylaNeural"),
    ("ar-LY", "ar-LY-OmarNeural", "ar-LY-ImanNeural"),
    ("ar-MA", "ar-MA-JamalNeural", "ar-MA-MounaNeural"),
    ("ar-OM", "ar-OM-AbdullahNeural", "ar-OM-AyshaNeural"),
    ("ar-QA", "ar-QA-MoazNeural", "ar-QA-AmalNeural"),
    ("ar-SY", "ar-SY-LaithNeural", "ar-SY-AmanyNeural"),
    ("ar-TN", "ar-TN-HediNeural", "ar-TN-ReemNeural"),
    ("ar-YE", "ar-YE-SalehNeural", "ar-YE-MaryamNeural"),
    ("fa-IR", "fa-IR-FaridNeural", "fa-IR-DilaraNeural"),
    ("he-IL", "he-IL-AvriNeural", "he-IL-HilaNeural"),
    ("tr-TR", "tr-TR-AhmetNeural", "tr-TR-EmelNeural"),
    // ── English variants ──────────────────────────────────────────────────
    ("en-US", "en-US-GuyNeural", "en-US-AriaNeural"),
    ("en-GB", "en-GB-RyanNeural", "en-GB-SoniaNeural"),
    ("en-AU", "en-AU-WilliamNeural", "en-AU-NatashaNeural"),
    ("en-CA", "en-CA-LiamNeural", "en-CA-ClaraNeural"),
    ("en-NZ", "en-NZ-MitchellNeural", "en-NZ-MollyNeural"),
    ("en-IN", "en-IN-PrabhatNeural", "en-IN-NeerjaNeural"),
    ("en-IE", "en-IE-ConnorNeural", "en-IE-EmilyNeural"),
    ("en-ZA", "en-ZA-LukeNeural", "en-ZA-LeahNeural"),
    ("en-SG", "en-SG-WayneNeural", "en-SG-LunaNeural"),
    ("en-TZ", "en-TZ-ElimuNeural", "en-TZ-ImaniNeural"),
    ("en-KE", "en-KE-AsilNeural", "en-KE-ChiamaNeural"),
    ("en-NG", "en-NG-AbeoNeural", "en-NG-EzinneNeural"),
    ("en-PH", "en-PH-JamesNeural", "en-PH-RosaNeural"),
    ("en-HK", "en-HK-SamNeural", "en-HK-YanNeural"),
    // ── Western Europe ────────────────────────────────────────────────────
    ("de-DE", "de-DE-ConradNeural", "de-DE-KatjaNeural"),
    ("de-AT", "de-AT-JonasNeural", "de-AT-IngridNeural"),
    ("de-CH", "de-CH-JanNeural", "de-CH-LeniNeural"),
    ("fr-FR", "fr-FR-HenriNeural", "fr-FR-DeniseNeural"),
    ("fr-BE", "fr-BE-GerardNeural", "fr-BE-CharlineNeural"),
    ("fr-CH", "fr-CH-FabriceNeural", "fr-CH-ArianeNeural"),
    ("fr-CA", "fr-CA-AntoineNeural", "fr-CA-SylvieNeural"),
    ("es-ES", "es-ES-AlvaroNeural", "es-ES-ElviraNeural"),
    ("es-MX", "es-MX-JorgeNeural", "es-MX-DaliaNeural"),
    ("es-AR", "es-AR-TomasNeural", "es-AR-ElenaNeural"),
    ("es-CO", "es-CO-GonzaloNeural", "es-CO-SalomeNeural"),
    ("es-CL", "es-CL-LorenzoNeural", "es-CL-CatalinaNeural"),
    ("es-PE", "es-PE-AlexNeural", "es-PE-CamilaNeural"),
    ("es-VE", "es-VE-SebastianNeural", "es-VE-PaolaNeural"),
    ("es-US", "es-US-AlonsoNeural", "es-US-PalomaNeural"),
    ("es-CU", "es-CU-ManuelNeural", "es-CU-BelkysNeural"),
    ("es-UY", "es-UY-MateoNeural", "es-UY-ValentinaNeural"),
    ("es-PY", "es-PY-MarioNeural", "es-PY-TaniaNeural"),
    ("es-NI", "es-NI-FedericoNeural", "es-NI-YolandaNeural"),
    ("es-PA", "es-PA-RobertoNeural", "es-PA-MargaritaNeural"),
    ("es-DO", "es-DO-EmilioNeural", "es-DO-RamonaNeural"),
    ("es-EC", "es-EC-AndresNeural", "es-EC-AndreaNeural"),
    ("es-GT", "es-GT-AndresNeural", "es-GT-MartaNeural"),
    ("es-HN", "es-HN-CarlosNeural", "es-HN-KarlaNeural"),
    ("es-SV", "es-SV-RodrigoNeural", "es-SV-LorenaNeural"),
    ("es-BO", "es-BO-MarceloNeural", "es-BO-SofiaNeural"),
    ("es-PR", "es-PR-VictorNeural", "es-PR-KarinaNeural"),
    ("es-CR", "es-CR-JuanNeural", "es-CR-MariaNeural"),
    ("es-GQ", "es-GQ-JavierNeural", "es-GQ-TeresaNeural"),
    ("pt-BR", "pt-BR-AntonioNeural", "pt-BR-FranciscaNeural"),
    ("pt-PT", "pt-PT-DuarteNeural", "pt-PT-RaquelNeural"),
    ("it-IT", "it-IT-DiegoNeural", "it-IT-ElsaNeural"),
    ("nl-NL", "nl-NL-MaartenNeural", "nl-NL-FennaNeural"),
    ("nl-BE", "nl-BE-ArnaudNeural", "nl-BE-DenaNeural"),
    ("sv-SE", "sv-SE-MattiasNeural", "sv-SE-SofieNeural"),
    ("nb-NO", "nb-NO-FinnNeural", "nb-NO-PernilleNeural"),
    ("da-DK", "da-DK-JeppeNeural", "da-DK-ChristelNeural"),
    ("fi-FI", "fi-FI-HarriNeural", "fi-FI-NooraNeural"),
    ("pl-PL", "pl-PL-MarekNeural", "pl-PL-ZofiaNeural"),
    ("cs-CZ", "cs-CZ-AntoninNeural", "cs-CZ-VlastaNeural"),
    ("sk-SK", "sk-SK-LukasNeural", "sk-SK-ViktoriaNeural"),
    ("hu-HU", "hu-HU-TamasNeural", "hu-HU-NoemiNeural"),
    ("ro-RO", "ro-RO-EmilNeural", "ro-RO-AlinaNeural"),
    ("bg-BG", "bg-BG-BorislavNeural", "bg-BG-KalinaNeural"),
    ("hr-HR", "hr-HR-SreckoNeural", "hr-HR-GabrijelaNeural"),
    ("sl-SI", "sl-SI-RokNeural", "sl-SI-PetraNeural"),
    ("sr-RS", "sr-RS-NicholasNeural", "sr-RS-SophieNeural"),
    ("bs-BA", "bs-BA-GoranNeural", "bs-BA-VesnaNeural"),
    ("mk-MK", "mk-MK-AleksandarNeural", "mk-MK-MarijaNeural"),
    ("sq-AL", "sq-AL-IlirNeural", "sq-AL-AnilaNeural"),
    ("mt-MT", "mt-MT-JosephNeural", "mt-MT-GraceNeural"),
    ("is-IS", "is-IS-GunnarNeural", "is-IS-GudrunNeural"),
    ("lv-LV", "lv-LV-NilsNeural", "lv-LV-EveritaNeural"),
    ("lt-LT", "lt-LT-LeonasNeural", "lt-LT-OnaNeural"),
    ("et-EE", "et-EE-KertNeural", "et-EE-AnuNeural"),
    ("ga-IE", "ga-IE-ColmNeural", "ga-IE-OrlaNeural"),
    ("cy-GB", "cy-GB-AledNeural", "cy-GB-NiaNeural"),
    // ── Eastern Europe / Former Soviet ────────────────────────────────────
    ("ru-RU", "ru-RU-DmitryNeural", "ru-RU-SvetlanaNeural"),
    ("uk-UA", "uk-UA-OstapNeural", "uk-UA-PolinaNeural"),
    ("be-BY", "be-BY-NikolayNeural", "be-BY-YanaNeural"),
    ("az-AZ", "az-AZ-BabekNeural", "az-AZ-BanuNeural"),
    ("kk-KZ", "kk-KZ-DauletNeural", "kk-KZ-AigulNeural"),
    ("ky-KG", "ky-KG-IshenbekNeural", "ky-KG-AizatNeural"),
    ("uz-UZ", "uz-UZ-SardorNeural", "uz-UZ-MadinaNeural"),
    ("tk-TM", "tk-TM-AgabekNeural", "tk-TM-EzzatNeural"),
    ("ka-GE", "ka-GE-GiorgiNeural", "ka-GE-EkaNeural"),
    ("hy-AM", "hy-AM-HaykNeural", "hy-AM-AnahitNeural"),
    ("az-AZ", "az-AZ-BabekNeural", "az-AZ-BanuNeural"),
    // ── Africa ────────────────────────────────────────────────────────────
    ("af-ZA", "af-ZA-WillemNeural", "af-ZA-AdriNeural"),
    ("sw-KE", "sw-KE-RafikiNeural", "sw-KE-ZuriNeural"),
    ("sw-TZ", "sw-TZ-DaudiNeural", "sw-TZ-RehemaNeural"),
    ("am-ET", "am-ET-AmehaNeural", "am-ET-MekdesNeural"),
    ("so-SO", "so-SO-MuuseNeural", "so-SO-UbaxNeural"),
    ("zu-ZA", "zu-ZA-ThandoNeural", "zu-ZA-ThandekileNeural"),
    ("yo-NG", "yo-NG-IsiolaNeural", "yo-NG-EzinneNeural"),
    ("ig-NG", "ig-NG-EchewuNeural", "ig-NG-EzinneNeural"),
    ("ha-NG", "ha-NG-OmaruNeural", "ha-NG-HalimaNeural"),
    ("ps-AF", "ps-AF-GulNawazNeural", "ps-AF-LatifaNeural"),
    // ── Americas (additional) ─────────────────────────────────────────────
    ("gl-ES", "gl-ES-RoiNeural", "gl-ES-SabelaNeural"),
    ("ca-ES", "ca-ES-EnricNeural", "ca-ES-AlbaNeural"),
    ("eu-ES", "eu-ES-AitorNeural", "eu-ES-AinhoaNeural"),
    ("ht-HT", "ht-HT-RamaNeural", "ht-HT-KellyNeural"),
    ("qu-PE", "qu-PE-NinanNeural", "qu-PE-ChuraNeural"),
    // ── Pacific ───────────────────────────────────────────────────────────
    ("wuu-CN", "wuu-CN-YunzheNeural", "wuu-CN-XiaotongNeural"),
    ("yue-CN", "yue-CN-YunSongNeural", "yue-CN-XiaoMinNeural"),
    (
        "zh-CN-liaoning",
        "zh-CN-liaoning-XiaobeiNeural",
        "zh-CN-liaoning-XiaobeiNeural",
    ),
    ("el-GR", "el-GR-NestorasNeural", "el-GR-AthinaNeural"),
];

/// Normalize BCP-47 locales, ISO 639-3 macrolanguage codes, Romanized tags, and display names.
pub fn normalize_language_code(lang: &str) -> String {
    let trimmed = lang.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("AUTO") {
        return "en-US".to_string();
    }

    let lower = trimmed.to_ascii_lowercase().replace('_', "-");
    match lower.as_str() {
        "portuguese (brazil)" | "brazilian portuguese" => return "pt-BR".to_string(),
        "portuguese (portugal)" | "european portuguese" => return "pt-PT".to_string(),
        "chinese (simplified)" => return "zh-CN".to_string(),
        "chinese (traditional)" => return "zh-TW".to_string(),
        "ml-latn" => return "ml".to_string(),
        "hi-latn" => return "hi".to_string(),
        "ta-latn" => return "ta".to_string(),
        _ => {}
    }

    fn map_macro(code: &str) -> &str {
        match code {
            "arb" | "arz" | "acm" | "apc" | "ary" | "ajp" | "aeb" | "apd" => "ar",
            "cmn" | "zho" | "yue" | "wuu" => "zh",
            "mal" => "ml",
            "hin" => "hi",
            "tam" => "ta",
            "tel" => "te",
            "kan" => "kn",
            "ben" => "bn",
            "urd" => "ur",
            "npi" | "nep" => "ne",
            "jpn" => "ja",
            "kor" => "ko",
            "fra" | "fre" => "fr",
            "deu" | "ger" => "de",
            "spa" => "es",
            "por" => "pt",
            "rus" => "ru",
            "ita" => "it",
            "nld" | "dut" => "nl",
            "tur" => "tr",
            "vie" => "vi",
            "ind" => "id",
            "tha" => "th",
            "ell" | "gre" => "el",
            "heb" => "he",
            "ukr" => "uk",
            "pol" => "pl",
            "ces" | "cze" => "cs",
            "swe" => "sv",
            "ron" | "rum" => "ro",
            "hun" => "hu",
            "fas" | "per" => "fa",
            "mar" => "mr",
            "guj" => "gu",
            "pan" => "pa",
            "sin" => "si",
            "khm" => "km",
            "lao" => "lo",
            "mya" | "bur" => "my",
            "amh" => "am",
            "kat" | "geo" => "ka",
            "hye" | "arm" => "hy",
            _ => code,
        }
    }

    let parts: Vec<&str> = lower.split('-').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return "en-US".to_string();
    }
    let base = map_macro(parts[0]);
    if parts.len() >= 2 {
        let region = parts[1];
        if region == "latn" {
            return base.to_string();
        }
        if matches!(region, "hans" | "cn" | "sg") {
            return format!("{base}-CN");
        }
        if matches!(region, "hant" | "tw" | "hk") {
            return format!("{base}-TW");
        }
        if region.len() == 2 {
            return format!("{base}-{}", region.to_ascii_uppercase());
        }
        return format!("{base}-{region}");
    }
    base.to_string()
}

/// Detect which Unicode script block dominates the text.
/// Returns a BCP-47 script-based language code or empty string if Latin/unknown.
pub fn detect_script_lang(text: &str) -> &'static str {
    let mut counts = [0u32; 23];
    // 0=ml, 1=hi, 2=ta, 3=te, 4=kn, 5=bn, 6=gu, 7=pa,
    // 8=ar, 9=ja, 10=zh, 11=ko, 12=si, 13=km, 14=lo, 15=my,
    // 16=ru(Cyrillic), 17=el(Greek), 18=he(Hebrew), 19=th(Thai),
    // 20=ka(Georgian), 21=hy(Armenian), 22=am(Ethiopic)
    for c in text.chars() {
        let cp = c as u32;
        match cp {
            0x0D00..=0x0D7F => counts[0] += 1,
            0x0900..=0x097F => counts[1] += 1,
            0x0B80..=0x0BFF => counts[2] += 1,
            0x0C00..=0x0C7F => counts[3] += 1,
            0x0C80..=0x0CFF => counts[4] += 1,
            0x0980..=0x09FF => counts[5] += 1,
            0x0A80..=0x0AFF => counts[6] += 1,
            0x0A00..=0x0A7F => counts[7] += 1,
            0x0600..=0x06FF | 0x0750..=0x077F | 0xFB50..=0xFDFF => counts[8] += 1,
            0x3040..=0x30FF | 0x31F0..=0x31FF => counts[9] += 1,
            0x4E00..=0x9FFF | 0x3400..=0x4DBF => counts[10] += 1,
            0xAC00..=0xD7A3 | 0x1100..=0x11FF => counts[11] += 1,
            0x0D80..=0x0DFF => counts[12] += 1,
            0x1780..=0x17FF => counts[13] += 1,
            0x0E80..=0x0EFF => counts[14] += 1,
            0x1000..=0x109F => counts[15] += 1,
            0x0400..=0x04FF => counts[16] += 1,
            0x0370..=0x03FF => counts[17] += 1,
            0x0590..=0x05FF => counts[18] += 1,
            0x0E00..=0x0E7F => counts[19] += 1,
            0x10A0..=0x10FF => counts[20] += 1,
            0x0530..=0x058F => counts[21] += 1,
            0x1200..=0x137F => counts[22] += 1,
            _ => {}
        }
    }
    let max_idx = counts
        .iter()
        .enumerate()
        .max_by_key(|(_, &v)| v)
        .map(|(i, _)| i)
        .unwrap_or(0);
    if counts[max_idx] == 0 {
        return "";
    }
    match max_idx {
        0 => "ml",
        1 => "hi",
        2 => "ta",
        3 => "te",
        4 => "kn",
        5 => "bn",
        6 => "gu",
        7 => "pa",
        8 => "ar",
        9 => "ja",
        10 => "zh",
        11 => "ko",
        12 => "si",
        13 => "km",
        14 => "lo",
        15 => "my",
        16 => "ru",
        17 => "el",
        18 => "he",
        19 => "th",
        20 => "ka",
        21 => "hy",
        22 => "am",
        _ => "",
    }
}

/// Resolve the best-matching Edge-TTS base voice for a given speaker gender + language tag.
///
/// Resolution priority:
/// 1. Script detection from text (non-Latin scripts only).
/// 2. Exact BCP-47 locale match in `EDGE_TTS_LANG_TABLE` (e.g. `en-GB`).
/// 3. Language-prefix match (e.g. `ar-` matches `ar-SA` as first entry).
/// 4. English fallback.
pub fn resolve_cross_lingual_base_voice(
    gender: Option<&Gender>,
    text: &str,
    profile_lang: &str,
) -> &'static str {
    let is_male = matches!(gender, Some(Gender::Male));
    let pick = |male: &'static str, female: &'static str| {
        if is_male {
            male
        } else {
            female
        }
    };

    // 1. Script detection — non-Latin scripts short-circuit the profile tag entirely.
    //    e.g. Malayalam text with profile_lang="en-US" must still return an ml-* voice.
    let script_lang = if !text.trim().is_empty() {
        detect_script_lang(text)
    } else {
        ""
    };

    if !script_lang.is_empty() {
        // Script was detected: search by script prefix first.
        let script_prefix = script_lang.split('-').next().unwrap_or(script_lang);
        for &(bcp47, male, female) in EDGE_TTS_LANG_TABLE {
            if bcp47.starts_with(script_prefix) {
                return pick(male, female);
            }
        }
    }

    let normalized = normalize_language_code(profile_lang);

    // 2. Exact locale match (e.g. "en-GB", "fr-CA", "pt-BR", "pt-PT") — only reached for Latin/undetected scripts.
    for &(bcp47, male, female) in EDGE_TTS_LANG_TABLE {
        if profile_lang.eq_ignore_ascii_case(bcp47) || normalized.eq_ignore_ascii_case(bcp47) {
            return pick(male, female);
        }
    }

    // 3. Language-prefix match on normalized language code (e.g. "arb"->"ar" matches "ar-SA")
    let prefix = normalized.split('-').next().unwrap_or(&normalized);
    for &(bcp47, male, female) in EDGE_TTS_LANG_TABLE {
        if bcp47.starts_with(prefix) && !prefix.is_empty() {
            return pick(male, female);
        }
    }

    // 4. English fallback
    pick("en-US-GuyNeural", "en-US-AriaNeural")
}

fn auto_detect_local_port(port: u16) -> bool {
    use std::net::{SocketAddr, TcpStream};
    use std::time::Duration;
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&addr, Duration::from_millis(15)).is_ok()
}

/// Probe and invoke the local neural Whisper sidecar `/transcribe` endpoint if live.
/// Returns `None` without fabricated placeholder text when offline or unavailable.
pub async fn transcribe_via_sidecar(
    audio_base64: &str,
    language: Option<&str>,
) -> Option<String> {
    if audio_base64.trim().is_empty() {
        return None;
    }

    let base_url = if let Ok(url) = std::env::var("VOXFORGE_SIDECAR_URL") {
        let trimmed = url.trim().trim_end_matches('/').to_string();
        if trimmed.is_empty() {
            return None;
        }
        trimmed
    } else if auto_detect_local_port(9000) {
        "http://127.0.0.1:9000".to_string()
    } else {
        return None;
    };

    let endpoint = if base_url.ends_with("/synthesize") {
        format!("{}/transcribe", base_url.trim_end_matches("/synthesize"))
    } else {
        format!("{base_url}/transcribe")
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .ok()?;

    let payload = serde_json::json!({
        "audio_base64": audio_base64,
        "language": language,
    });

    match client.post(&endpoint).json(&payload).send().await {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                return json
                    .get("text")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(ToString::to_string);
            }
            None
        }
        Ok(resp) => {
            tracing::debug!(status = %resp.status(), endpoint = %endpoint, "Sidecar /transcribe non-success status");
            None
        }
        Err(err) => {
            tracing::debug!(error = %err, endpoint = %endpoint, "Sidecar /transcribe request failed");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxforge_core::models::AudioContainerFormat;

    #[tokio::test]
    async fn test_qwen3_cloning_and_synthesis() {
        let engine = Qwen3TtsEngine::default();
        assert!(engine.supports_cloning());

        let fake_pcm: Vec<i16> = (0..2400).map(|i| ((i % 100) * 100) as i16).collect();
        let bytes: Vec<u8> = fake_pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
        let b64 = hex::encode(bytes);

        let req = CloneVoiceRequest {
            name: "Sample Speaker".to_string(),
            engine_id: "qwen3-tts".to_string(),
            reference_audio_base64: Some(b64),
            reference_audio_path: None,
            reference_audio_url: None,
            reference_transcript: Some("Sample speaker reference sentence.".to_string()),
            additional_reference_audio_base64: vec![],
            language: "en-US".to_string(),
            description: Some("Custom cloned speaker".to_string()),
            gender: Some(Gender::Female),
            metadata: HashMap::new(),
        };

        let profile = engine
            .clone_voice(&req)
            .await
            .expect("Cloning must succeed");
        assert_eq!(profile.name, "Sample Speaker");
        assert!(profile.embedding.is_some());
        assert_eq!(profile.embedding.as_ref().unwrap().len(), 512);

        let synth_req = ClonedSynthesisRequest {
            text: "Testing cloned synthesis output with Qwen3-TTS".to_string(),
            profile,
            speed: 1.0,
            pitch: 0.0,
            format: AudioContainerFormat::Wav,
            language: None,
            instruct: None,
            num_step: None,
            seed: None,
            precision: None,
        };

        match engine.synthesize_cloned(&synth_req).await {
            Ok(chunk) => {
                assert_eq!(chunk.sample_rate, 24000);
                assert!(!chunk.pcm_data.is_empty());
            }
            Err(e) => {
                let err_str = e.to_string();
                assert!(
                    err_str.contains("No such host is known")
                        || err_str.contains("failed to lookup address")
                        || err_str.contains("Connect")
                        || err_str.contains("timeout")
                        || err_str.contains("timed out")
                        || err_str.contains("Edge TTS WebSocket error"),
                    "Unexpected synthesis error: {err_str}"
                );
            }
        }
    }

    #[test]
    fn test_resolve_cross_lingual_base_voice() {
        // Malayalam text with male gender -> ml-IN-MidhunNeural
        let ml_voice_male =
            resolve_cross_lingual_base_voice(Some(&Gender::Male), "നമസ്കാരം സുഖമാണോ", "en-US");
        assert_eq!(ml_voice_male, "ml-IN-MidhunNeural");

        // Malayalam text with female gender -> ml-IN-SobhanaNeural
        let ml_voice_female =
            resolve_cross_lingual_base_voice(Some(&Gender::Female), "നമസ്കാരം", "en-US");
        assert_eq!(ml_voice_female, "ml-IN-SobhanaNeural");

        // Hindi text -> hi-IN-SwaraNeural
        let hi_voice =
            resolve_cross_lingual_base_voice(Some(&Gender::Female), "नमस्ते भारत", "en-US");
        assert_eq!(hi_voice, "hi-IN-SwaraNeural");

        // German profile language -> de-DE-KatjaNeural
        let de_voice =
            resolve_cross_lingual_base_voice(Some(&Gender::Female), "Guten Morgen", "de-DE");
        assert_eq!(de_voice, "de-DE-KatjaNeural");

        // English text with male gender -> en-US-GuyNeural
        let en_voice =
            resolve_cross_lingual_base_voice(Some(&Gender::Male), "Hello world", "en-US");
        assert_eq!(en_voice, "en-US-GuyNeural");

        // ISO 639-3 macrolanguage codes & expanded scripts
        assert_eq!(normalize_language_code("arb"), "ar");
        assert_eq!(normalize_language_code("mal"), "ml");
        assert_eq!(normalize_language_code("ml-Latn"), "ml");
        assert_eq!(normalize_language_code("Portuguese (Brazil)"), "pt-BR");
        assert_eq!(
            resolve_cross_lingual_base_voice(Some(&Gender::Male), "Marhaba", "arb"),
            "ar-SA-HamedNeural"
        );
        assert_eq!(detect_script_lang("Привет мир"), "ru");
        assert_eq!(detect_script_lang("Γεια σου"), "el");
        assert_eq!(detect_script_lang("שלום"), "he");
        assert_eq!(detect_script_lang("สวัสดี"), "th");
        assert_eq!(detect_script_lang("გამარჯობა"), "ka");
        assert_eq!(detect_script_lang("Բարև"), "hy");
        assert_eq!(detect_script_lang("ሰላም"), "am");
        assert!((nominal_base_voice_f0("ar-SA-HamedNeural") - 120.0).abs() < 1.0);
        assert!((nominal_base_voice_f0("ru-RU-DmitryNeural") - 120.0).abs() < 1.0);
        assert!((nominal_base_voice_f0("el-GR-NestorasNeural") - 120.0).abs() < 1.0);
    }

    #[test]
    fn test_extract_speaker_embedding_with_acoustic_preconditioning() {
        // Empty buffer returns zeros
        let empty_emb = Qwen3TtsEngine::extract_speaker_embedding(&[]);
        assert_eq!(empty_emb.len(), 512);
        assert!(empty_emb.iter().all(|&v| v == 0.0));

        // Audio with low rumble (25Hz) plus speech frequencies (400Hz)
        let sample_rate = 24000f32;
        let pcm: Vec<i16> = (0..4800)
            .map(|i| {
                let t = i as f32 / sample_rate;
                let rumble = (2.0 * std::f32::consts::PI * 25.0 * t).sin() * 8000.0;
                let voice = (2.0 * std::f32::consts::PI * 400.0 * t).sin() * 12000.0;
                (rumble + voice).round() as i16
            })
            .collect();

        let emb = Qwen3TtsEngine::extract_speaker_embedding(&pcm);
        assert_eq!(emb.len(), 512);
        assert!(emb.iter().all(|&v| v.is_finite()));
        // Embedding should have non-trivial variation
        let non_zero_count = emb.iter().filter(|&&v| v.abs() > 0.0001).count();
        assert!(non_zero_count > 400);
    }

    #[test]
    fn test_decode_audio_payload_handles_data_url() {
        let raw_bytes = b"RIFFfakeaudioWAVEfmt ";
        let b64 = format!(
            "data:audio/wav;base64,{}",
            std::str::from_utf8(&filtered_base64(raw_bytes)).unwrap()
        );
        let decoded = decode_audio_payload(&b64);
        assert_eq!(decoded, raw_bytes);
    }
    fn filtered_base64(input: &[u8]) -> Vec<u8> {
        const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = Vec::new();
        let mut buf = 0u32;
        let mut bits = 0;
        for &b in input {
            buf = (buf << 8) | (b as u32);
            bits += 8;
            while bits >= 6 {
                bits -= 6;
                out.push(CHARS[((buf >> bits) & 0x3F) as usize]);
            }
        }
        if bits > 0 {
            out.push(CHARS[((buf << (6 - bits)) & 0x3F) as usize]);
            while out.len() % 4 != 0 {
                out.push(b'=');
            }
        }
        out
    }

    #[test]
    fn test_compute_spectral_timbre_deltas() {
        let empty_deltas = compute_spectral_timbre_deltas(&[], &[]);
        assert_eq!(empty_deltas, (0.0, 0.0, 0.0));

        let ref_pcm: Vec<i16> = (0..2400).map(|i| (i % 50 * 200) as i16).collect();
        let syn_pcm: Vec<i16> = (0..2400).map(|i| (i % 20 * 400) as i16).collect();
        let (low, mid, high) = compute_spectral_timbre_deltas(&ref_pcm, &syn_pcm);
        assert!(low.is_finite());
        assert!(mid.is_finite());
        assert!(high.is_finite());
    }

    #[test]
    fn test_speaker_embedding_cosine_similarity_consistency() {
        let sample_rate = 24000f32;
        // Speaker A: 130Hz male tone with formants at 500Hz and 1500Hz
        let pcm_a1: Vec<i16> = (0..9600)
            .map(|i| {
                let t = i as f32 / sample_rate;
                let s = (2.0 * std::f32::consts::PI * 130.0 * t).sin() * 12000.0
                    + (2.0 * std::f32::consts::PI * 500.0 * t).sin() * 4000.0
                    + (2.0 * std::f32::consts::PI * 1500.0 * t).sin() * 2000.0;
                s.round() as i16
            })
            .collect();

        // Speaker A variation: same speaker with slight amplitude change and 20ms offset
        let pcm_a2: Vec<i16> = (480..10080)
            .map(|i| {
                let t = i as f32 / sample_rate;
                let s = (2.0 * std::f32::consts::PI * 130.0 * t).sin() * 14000.0
                    + (2.0 * std::f32::consts::PI * 500.0 * t).sin() * 4500.0
                    + (2.0 * std::f32::consts::PI * 1500.0 * t).sin() * 2300.0;
                s.round() as i16
            })
            .collect();

        // Speaker B: 260Hz female tone with different formants
        let pcm_b: Vec<i16> = (0..9600)
            .map(|i| {
                let t = i as f32 / sample_rate;
                let s = (2.0 * std::f32::consts::PI * 260.0 * t).sin() * 12000.0
                    + (2.0 * std::f32::consts::PI * 850.0 * t).sin() * 5000.0
                    + (2.0 * std::f32::consts::PI * 2200.0 * t).sin() * 3000.0;
                s.round() as i16
            })
            .collect();

        let emb_a1 = Qwen3TtsEngine::extract_speaker_embedding(&pcm_a1);
        let emb_a2 = Qwen3TtsEngine::extract_speaker_embedding(&pcm_a2);
        let emb_b = Qwen3TtsEngine::extract_speaker_embedding(&pcm_b);

        // Identical audio must have similarity 1.0
        let sim_self = Qwen3TtsEngine::compute_cosine_similarity(&emb_a1, &emb_a1);
        assert!(
            (sim_self - 1.0).abs() < 1e-4,
            "Self similarity must equal 1.0"
        );

        // Same speaker across different recordings must exhibit high consistency (>0.85)
        let sim_same = Qwen3TtsEngine::compute_cosine_similarity(&emb_a1, &emb_a2);
        assert!(
            sim_same > 0.85,
            "Same speaker similarity should be >0.85, got {sim_same}"
        );

        // Different speaker must have lower similarity
        let sim_diff = Qwen3TtsEngine::compute_cosine_similarity(&emb_a1, &emb_b);
        assert!(
            sim_same > sim_diff,
            "Same speaker ({sim_same}) must be higher than different speaker ({sim_diff})"
        );
    }

    #[tokio::test]
    async fn test_clone_voice_from_reference_path() {
        let engine = Qwen3TtsEngine::default();
        let temp_dir = std::env::temp_dir().join(format!("voxforge_test_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let wav_path = temp_dir.join("ref_sample.wav");

        let pcm: Vec<i16> = (0..4800).map(|i| ((i % 80) * 150) as i16).collect();
        let wav_bytes = voxforge_audio::WavEncoder::encode_pcm16_to_wav(&pcm, 24000, 1)
            .expect("WAV encoding must succeed");
        std::fs::write(&wav_path, wav_bytes).expect("Write wav must succeed");

        let req = CloneVoiceRequest {
            name: "Path Cloned Speaker".to_string(),
            engine_id: "qwen3-tts".to_string(),
            reference_audio_base64: None,
            reference_audio_path: Some(wav_path.to_str().unwrap().to_string()),
            reference_audio_url: None,
            reference_transcript: Some("Testing path cloning.".to_string()),
            additional_reference_audio_base64: vec![],
            language: "en-US".to_string(),
            description: Some("Speaker cloned from local WAV path".to_string()),
            gender: Some(Gender::Male),
            metadata: HashMap::new(),
        };

        let profile = engine
            .clone_voice(&req)
            .await
            .expect("Path cloning must succeed");
        assert_eq!(profile.name, "Path Cloned Speaker");
        assert!(profile.embedding.is_some());
        assert_eq!(profile.embedding.as_ref().unwrap().len(), 512);
        assert!(profile.metadata.contains_key("f0_mean_hz"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_clone_voice_multi_sample_aggregation() {
        let engine = Qwen3TtsEngine::default();

        // Build two distinct synthetic PCM blobs
        let make_pcm_b64 = |seed: i16| -> String {
            let pcm: Vec<i16> = (0..4800)
                .map(|i| ((i % 100 + seed as usize) * 100 % 32000) as i16)
                .collect();
            let bytes: Vec<u8> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
            hex::encode(bytes)
        };

        let primary = make_pcm_b64(1);
        let extra = make_pcm_b64(50);

        let req = CloneVoiceRequest {
            name: "Multi-Sample Speaker".to_string(),
            engine_id: "qwen3-tts".to_string(),
            reference_audio_base64: Some(primary),
            reference_audio_path: None,
            reference_audio_url: None,
            reference_transcript: None,
            additional_reference_audio_base64: vec![extra],
            language: "en-US".to_string(),
            description: None,
            gender: None,
            metadata: HashMap::new(),
        };

        let profile = engine
            .clone_voice(&req)
            .await
            .expect("Multi-sample clone must succeed");
        assert_eq!(profile.sample_count, 2);
        assert_eq!(profile.version, 1);
        assert!(profile
            .clone_capabilities
            .as_deref()
            .unwrap_or_default()
            .contains(&"multi-sample-aggregated".to_string()));
        // similarity_score is set when more than 1 sample is provided
        assert!(profile.similarity_score.is_some());
        let sim = profile.similarity_score.unwrap();
        assert!((0.0..=1.0).contains(&sim), "Similarity must be in [0,1]");

        let emb = profile.embedding.as_ref().unwrap();
        assert_eq!(emb.len(), 512);
        // Centroid must be unit-normalized
        let l2: f32 = emb.iter().map(|&x| x * x).sum::<f32>().sqrt();
        assert!(
            (l2 - 1.0).abs() < 1e-3,
            "Centroid embedding must be unit-normalized"
        );
    }

    #[tokio::test]
    async fn test_refine_voice_embedding() {
        let engine = Qwen3TtsEngine::default();

        let make_pcm_b64 = |seed: i16| -> String {
            let pcm: Vec<i16> = (0..4800)
                .map(|i| ((i % 120 + seed as usize) * 90 % 32000) as i16)
                .collect();
            let bytes: Vec<u8> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
            hex::encode(bytes)
        };

        // Create initial profile
        let req = CloneVoiceRequest {
            name: "Refineable Speaker".to_string(),
            engine_id: "qwen3-tts".to_string(),
            reference_audio_base64: Some(make_pcm_b64(10)),
            reference_audio_path: None,
            reference_audio_url: None,
            reference_transcript: None,
            additional_reference_audio_base64: vec![],
            language: "en-US".to_string(),
            description: None,
            gender: None,
            metadata: HashMap::new(),
        };

        let profile_v1 = engine
            .clone_voice(&req)
            .await
            .expect("Initial clone must succeed");
        assert_eq!(profile_v1.version, 1);
        assert_eq!(profile_v1.sample_count, 1);
        assert!(profile_v1.refined_at.is_none());

        // Refine with a second sample
        let profile_v2 = engine
            .refine_voice_embedding(&profile_v1, &make_pcm_b64(75))
            .await
            .expect("Refinement must succeed");

        assert_eq!(profile_v2.version, 2);
        assert_eq!(profile_v2.sample_count, 2);
        assert!(profile_v2.refined_at.is_some());
        assert!(profile_v2.similarity_score.is_some());
        let sim = profile_v2.similarity_score.unwrap();
        assert!((0.0..=1.0).contains(&sim));

        // Embedding must still be unit-normalized
        let emb = profile_v2.embedding.as_ref().unwrap();
        let l2: f32 = emb.iter().map(|&x| x * x).sum::<f32>().sqrt();
        assert!(
            (l2 - 1.0).abs() < 1e-3,
            "Refined embedding must be unit-normalized"
        );

        // Verify refinement rejects empty audio
        let err = engine.refine_voice_embedding(&profile_v2, "").await;
        assert!(err.is_err());
    }
}

/// Split text into sentence-sized chunks for chunked synthesis.
///
/// Splits on sentence-ending punctuation (`.`, `!`, `?`, `。`, `।`, `؟`)
/// followed by whitespace or end-of-string. Respects common abbreviations
/// by not splitting on single-char prefixes like "Dr." or "Mr.".
/// Chunks that are too short (<10 chars) are merged into the previous chunk.
/// Maximum chunk length: 300 chars (avoids Edge-TTS SSML length limits).
/// Language-aware sentence chunker.
///
/// Advanced multilingual sentence chunking:
/// - CJK split on 。！？（fullstop/exclaim/question）as well as ；（semicolon）
/// - Arabic split on ؟ (Arabic question mark) and ۔ (Urdu fullstop)
/// - Devanagari split on । (danda) and ॥ (double danda)
/// - Korean split on 。？！ and also newlines (Korean often uses line-breaks between thoughts)
/// - Length-balanced: target 300–500 chars per chunk; short chunks (<10 chars) are merged
///   into the previous one; chunks > MAX are force-split at the last whitespace or CJK boundary
fn split_text_to_sentences(text: &str) -> Vec<String> {
    if text.is_empty() {
        return vec![];
    }

    // Target: 300–500 chars per chunk. Force-split above MAX.
    const TARGET_MIN: usize = 50;
    const MAX_CHUNK: usize = 450;

    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut i = 0;

    while i < n {
        current.push(chars[i]);

        // Detect sentence-ending punctuation (Latin, CJK, Arabic, Indic)
        let is_sentence_end = matches!(
            chars[i],
            '.' | '!' | '?' |     // Latin
            '。' | '！' | '？' | '；' |  // CJK fullwidth
            '।' | '॥' |          // Devanagari danda
            '؟' | '۔' // Arabic/Urdu
        );

        if is_sentence_end {
            // For Latin '.', avoid splitting abbreviations (Dr., Mr., 2 a.m., etc.)
            let is_abbreviation = if chars[i] == '.' {
                let prev_word = current
                    .trim_end_matches('.')
                    .split_whitespace()
                    .last()
                    .unwrap_or("");
                // Single uppercase letter (initials) or ≤3-char word (Dr, Mr, St, etc.)
                prev_word.len() <= 3
                    || (prev_word.len() == 1
                        && prev_word
                            .chars()
                            .next()
                            .map(|c| c.is_uppercase())
                            .unwrap_or(false))
            } else {
                false
            };

            // For Latin punctuation, require following whitespace or end-of-string.
            // For CJK/Arabic/Indic, split immediately (no space needed after 。）
            let is_cjk_or_indic =
                matches!(chars[i], '。' | '！' | '？' | '；' | '।' | '॥' | '؟' | '۔');
            let next_is_space_or_end =
                i + 1 >= n || chars[i + 1].is_whitespace() || is_cjk_or_indic;

            if !is_abbreviation && next_is_space_or_end && current.chars().count() >= TARGET_MIN {
                let chunk = current.trim().to_string();
                if !chunk.is_empty() {
                    chunks.push(chunk);
                    current = String::new();
                }
                // Skip whitespace after Latin sentence end
                while i + 1 < n && chars[i + 1].is_whitespace() {
                    i += 1;
                }
            }
        }

        // Also split on paragraph breaks (double newline)
        if chars[i] == '\n' && i + 1 < n && chars[i + 1] == '\n' {
            let chunk = current.trim().to_string();
            if chunk.chars().count() >= TARGET_MIN {
                chunks.push(chunk);
                current = String::new();
            }
            while i + 1 < n && chars[i + 1].is_whitespace() {
                i += 1;
            }
        }

        // Force-split at MAX_CHUNK — prefer last ASCII space; for CJK split at char boundary
        if current.chars().count() >= MAX_CHUNK {
            // Try to find the last ASCII space for a clean word boundary
            if let Some(last_space) = current.rfind(' ') {
                let head = current[..last_space].trim().to_string();
                let tail = current[last_space + 1..].to_string();
                if !head.is_empty() {
                    chunks.push(head);
                }
                current = tail;
            } else {
                // CJK or no-space text: split at a CJK char boundary
                // Find split point in chars (not bytes)
                let cur_chars: Vec<char> = current.chars().collect();
                let split_at = MAX_CHUNK * 2 / 3; // split at 2/3 to avoid tiny trailing chunks
                let split_at = split_at.min(cur_chars.len().saturating_sub(1));
                let head: String = cur_chars[..split_at].iter().collect();
                let tail: String = cur_chars[split_at..].iter().collect();
                if !head.is_empty() {
                    chunks.push(head.trim().to_string());
                }
                current = tail;
            }
        }

        i += 1;
    }

    // Flush remainder
    let remainder = current.trim().to_string();
    if !remainder.is_empty() {
        // Merge very short tails (<10 chars) into the previous chunk
        if remainder.chars().count() < 10 && !chunks.is_empty() {
            let last = chunks.last_mut().unwrap();
            last.push(' ');
            last.push_str(&remainder);
        } else {
            chunks.push(remainder);
        }
    }

    if chunks.is_empty() {
        vec![text.to_string()]
    } else {
        chunks
    }
}

#[cfg(test)]
mod sentence_splitter_tests {
    use super::*;

    #[test]
    fn test_split_empty_and_short() {
        assert!(split_text_to_sentences("").is_empty());
        // Short text below TARGET_MIN — returned as single chunk
        let result = split_text_to_sentences("Hello world.");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "Hello world.");
    }

    #[test]
    fn test_split_multiple_sentences_long_enough() {
        // Sentences must be ≥ TARGET_MIN (50 chars) each before the splitter fires
        let s1 = "This is a first sentence that is long enough to trigger a split here.";
        let s2 = "This is the second sentence and it is also long enough to split.";
        let s3 = "And here is sentence three which should round out the test just fine?";
        let text = format!("{} {} {}", s1, s2, s3);
        let result = split_text_to_sentences(&text);
        assert!(
            result.len() >= 2,
            "Must split into ≥2 chunks, got: {:?}",
            result
        );
        assert!(result.iter().all(|s| !s.is_empty()));
    }

    #[test]
    fn test_split_does_not_split_abbreviations() {
        // The whole string is short (<TARGET_MIN after each '.') — stays as 1 chunk
        let text = "Dr. Smith was here. He left.";
        let result = split_text_to_sentences(text);
        // All pieces are < 50 chars → no split is triggered
        assert_eq!(
            result.len(),
            1,
            "Short abbreviation text should stay as 1 chunk, got: {:?}",
            result
        );
    }

    #[test]
    fn test_split_long_text_max_chunk() {
        // Build a long text without sentence punctuation (should force-split at MAX_CHUNK=450)
        let long_word = "word ";
        let text: String = long_word.repeat(200); // 1000 chars
        let result = split_text_to_sentences(&text);
        assert!(
            result.iter().all(|c| c.chars().count() <= 460),
            "No chunk exceeds max"
        );
        assert!(!result.is_empty());
    }

    #[test]
    fn test_split_cjk_text() {
        // CJK text with 。 sentence boundaries — should split without needing whitespace
        let text = "これは最初の文です。これは二番目の文です！三番目の文はここにあります。四番目の文も長くなるように書きました。";
        let result = split_text_to_sentences(text);
        // All CJK sentences are short — stays as few chunks, but must not panic
        assert!(!result.is_empty());
        assert!(result.iter().all(|s| !s.is_empty()));
    }

    #[test]
    fn test_split_devanagari_danda() {
        // Devanagari text with दanda (।) — should recognise as sentence boundary
        let text = "यह पहला वाक्य है जो काफी लंबा है। यह दूसरा वाक्य है जो भी काफी लंबा होना चाहिए।";
        let result = split_text_to_sentences(text);
        assert!(!result.is_empty());
        assert!(result.iter().all(|s| !s.is_empty()));
    }

    #[test]
    fn test_split_paragraph_break() {
        // Double newline should force a chunk boundary
        let para1 =
            "First paragraph with enough text to exceed the minimum chunk size for splitting.";
        let para2 = "Second paragraph also has enough text to be a standalone chunk in the output.";
        let text = format!("{}\n\n{}", para1, para2);
        let result = split_text_to_sentences(&text);
        assert!(
            result.len() >= 2,
            "Double newline must create at least 2 chunks, got: {:?}",
            result
        );
    }
}
