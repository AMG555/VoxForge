use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use voxforge_core::error::{Result, VoxForgeError};
use voxforge_hardware::HardwareProfile;

use crate::models::{CatalogItem, ModelFormat, ModelStatus, ModelType};

/// In-memory and disk-backed manager for model weights packages.
pub struct ModelCatalogStore {
    items: Arc<RwLock<HashMap<String, CatalogItem>>>,
    allow_simulated_fallback: bool,
}

impl Default for ModelCatalogStore {
    fn default() -> Self {
        Self::with_curated_models()
    }
}

impl ModelCatalogStore {
    /// Create an empty catalog store.
    pub fn new() -> Self {
        Self {
            items: Arc::new(RwLock::new(HashMap::new())),
            allow_simulated_fallback: false,
        }
    }

    /// Enable or disable simulated fallback when HTTP downloads are skipped (for offline unit tests).
    pub fn with_simulated_downloads(mut self, allow: bool) -> Self {
        self.allow_simulated_fallback = allow;
        self
    }

    fn models_dir() -> std::path::PathBuf {
        std::env::var("VOXFORGE_MODELS_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("models"))
    }

    fn target_filename_for_model(id: &str, format: ModelFormat) -> String {
        if id == "silero-vad" {
            return "silero_vad.onnx".to_string();
        }
        let ext = match format {
            ModelFormat::Onnx => "onnx",
            ModelFormat::Safetensors => "safetensors",
            ModelFormat::PyTorch => "pt",
            ModelFormat::Ggml => "bin",
        };
        format!("{id}.{ext}")
    }

    /// Create store seeded with standard open-weights speech models.
    pub fn with_curated_models() -> Self {
        let models_dir = Self::models_dir();

        let piper_voice_entry =
            |id: &str, name: &str, desc: &str, lang: &str, repo_path: &str, size_mb: u64| {
                CatalogItem {
                    id: id.to_string(),
                    name: name.to_string(),
                    description: desc.to_string(),
                    model_type: ModelType::Tts,
                    format: ModelFormat::Onnx,
                    size_bytes: size_mb * 1024 * 1024,
                    sha256: String::new(),
                    download_url: format!(
                        "https://huggingface.co/rhasspy/piper-voices/resolve/main/{repo_path}"
                    ),
                    min_ram_mb: 512,
                    requires_gpu: false,
                    supported_languages: vec![lang.to_string()],
                    status: ModelStatus::Available,
                    installed_at: None,
                    local_path: None,
                }
            };

        let mut models = vec![
            piper_voice_entry(
                "piper-en-lessac-medium",
                "Piper Lessac Medium (en-US)",
                "Lightweight, ultra-fast offline English (US) neural voice model",
                "en-US",
                "en/en_US/lessac/medium/en_US-lessac-medium.onnx",
                60,
            ),
            piper_voice_entry(
                "en_US-amy-medium",
                "Piper Amy Medium (en-US)",
                "Warm, clear English (US) female Piper ONNX voice",
                "en-US",
                "en/en_US/amy/medium/en_US-amy-medium.onnx",
                60,
            ),
            piper_voice_entry(
                "en_US-danny-low",
                "Piper Danny Low (en-US)",
                "Ultra-low-latency English (US) male Piper ONNX voice",
                "en-US",
                "en/en_US/danny/low/en_US-danny-low.onnx",
                60,
            ),
            piper_voice_entry(
                "en_GB-alan-medium",
                "Piper Alan Medium (en-GB)",
                "Articulate British English male Piper ONNX voice",
                "en-GB",
                "en/en_GB/alan/medium/en_GB-alan-medium.onnx",
                60,
            ),
            piper_voice_entry(
                "es_ES-carlfm-x_low",
                "Piper Carlfm X-Low (es-ES)",
                "Fast European Spanish male Piper ONNX voice",
                "es-ES",
                "es/es_ES/carlfm/x_low/es_ES-carlfm-x_low.onnx",
                27,
            ),
            piper_voice_entry(
                "fr_FR-siwis-medium",
                "Piper Siwis Medium (fr-FR)",
                "Expressive French female Piper ONNX voice",
                "fr-FR",
                "fr/fr_FR/siwis/medium/fr_FR-siwis-medium.onnx",
                60,
            ),
            piper_voice_entry(
                "de_DE-thorsten-medium",
                "Piper Thorsten Medium (de-DE)",
                "Natural German male Piper ONNX voice",
                "de-DE",
                "de/de_DE/thorsten/medium/de_DE-thorsten-medium.onnx",
                60,
            ),
            CatalogItem {
                id: "kokoro-v0_19".to_string(),
                name: "Kokoro 82M Multilingual (v1.0 ONNX)".to_string(),
                description: "State-of-the-art 82M parameter lightweight text-to-speech ONNX weights (onnx-community/Kokoro-82M-v1.0-ONNX)".to_string(),
                model_type: ModelType::Tts,
                format: ModelFormat::Onnx,
                size_bytes: 310 * 1024 * 1024,
                sha256: String::new(),
                download_url: "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/main/onnx/model.onnx".to_string(),
                min_ram_mb: 2048,
                requires_gpu: false,
                supported_languages: vec!["en".to_string(), "es".to_string(), "fr".to_string(), "ja".to_string(), "zh".to_string()],
                status: ModelStatus::Available,
                installed_at: None,
                local_path: None,
            },
            CatalogItem {
                id: "qwen3-tts-base".to_string(),
                name: "Qwen3-TTS 12Hz 0.6B Base".to_string(),
                description: "Zero-shot voice cloning transformer with speaker embedding conditioning (Qwen/Qwen3-TTS-12Hz-0.6B-Base)".to_string(),
                model_type: ModelType::Tts,
                format: ModelFormat::Safetensors,
                size_bytes: 1745 * 1024 * 1024,
                sha256: String::new(),
                download_url: "https://huggingface.co/Qwen/Qwen3-TTS-12Hz-0.6B-Base/resolve/main/model.safetensors".to_string(),
                min_ram_mb: 4096,
                requires_gpu: false,
                supported_languages: vec!["en".to_string(), "zh".to_string()],
                status: ModelStatus::Available,
                installed_at: None,
                local_path: None,
            },
            CatalogItem {
                id: "whisper-base".to_string(),
                name: "Whisper Base Encoder (ONNX)".to_string(),
                description: "Robust general-purpose multilingual speech recognition encoder weights (onnx-community/whisper-base)".to_string(),
                model_type: ModelType::Asr,
                format: ModelFormat::Onnx,
                size_bytes: 79 * 1024 * 1024,
                sha256: String::new(),
                download_url: "https://huggingface.co/onnx-community/whisper-base/resolve/main/onnx/encoder_model.onnx".to_string(),
                min_ram_mb: 1024,
                requires_gpu: false,
                supported_languages: vec!["en".to_string(), "es".to_string(), "fr".to_string(), "de".to_string(), "zh".to_string()],
                status: ModelStatus::Available,
                installed_at: None,
                local_path: None,
            },
            CatalogItem {
                id: "whisper-small".to_string(),
                name: "Whisper Small Encoder (ONNX)".to_string(),
                description: "High accuracy multilingual speech transcription weights (onnx-community/whisper-small)".to_string(),
                model_type: ModelType::Asr,
                format: ModelFormat::Onnx,
                size_bytes: 336 * 1024 * 1024,
                sha256: String::new(),
                download_url: "https://huggingface.co/onnx-community/whisper-small/resolve/main/onnx/encoder_model.onnx".to_string(),
                min_ram_mb: 2048,
                requires_gpu: false,
                supported_languages: vec!["en".to_string(), "es".to_string(), "fr".to_string(), "de".to_string()],
                status: ModelStatus::Available,
                installed_at: None,
                local_path: None,
            },
            CatalogItem {
                id: "silero-vad".to_string(),
                name: "Silero Voice Activity Detector".to_string(),
                description: "Enterprise-grade speech presence and pause detection neural network".to_string(),
                model_type: ModelType::Vad,
                format: ModelFormat::Onnx,
                size_bytes: 2 * 1024 * 1024,
                sha256: String::new(),
                download_url: "https://github.com/snakers4/silero-vad/raw/master/src/silero_vad/data/silero_vad.onnx".to_string(),
                min_ram_mb: 256,
                requires_gpu: false,
                supported_languages: vec!["*".to_string()],
                status: ModelStatus::Available,
                installed_at: None,
                local_path: None,
            },
            CatalogItem {
                id: "omnivoice-base".to_string(),
                name: "OmniVoice 600+ Multilingual Zero-Shot".to_string(),
                description: "Multilingual zero-shot voice cloning foundation model covering 600+ languages and regional dialects (k2-fsa/OmniVoice)".to_string(),
                model_type: ModelType::Tts,
                format: ModelFormat::Safetensors,
                size_bytes: 2337 * 1024 * 1024,
                sha256: String::new(),
                download_url: "https://huggingface.co/k2-fsa/OmniVoice/resolve/main/model.safetensors".to_string(),
                min_ram_mb: 4096,
                requires_gpu: false,
                supported_languages: vec!["*".to_string()],
                status: ModelStatus::Available,
                installed_at: None,
                local_path: None,
            },
            CatalogItem {
                id: "pocket-tts".to_string(),
                name: "PocketTTS Real-Time CPU Voice".to_string(),
                description: "Ultra-fast ~100M parameter low-latency speech synthesis engine tuned for local CPU dictation and short utterances (Kyutai Labs)".to_string(),
                model_type: ModelType::Tts,
                format: ModelFormat::Safetensors,
                size_bytes: 225 * 1024 * 1024,
                sha256: String::new(),
                download_url: "https://huggingface.co/kyutai/pocket-tts-without-voice-cloning/resolve/main/tts_b6369a24.safetensors".to_string(),
                min_ram_mb: 512,
                requires_gpu: false,
                supported_languages: vec!["en".to_string(), "fr".to_string(), "de".to_string(), "es".to_string(), "it".to_string(), "pt".to_string()],
                status: ModelStatus::Available,
                installed_at: None,
                local_path: None,
            },
            CatalogItem {
                id: "supertonic-3".to_string(),
                name: "Supertonic High-Fidelity Predictor (ONNX)".to_string(),
                description: "Fast ONNX duration & speech predictor component for 44.1 kHz synthesis (Supertone/supertonic)".to_string(),
                model_type: ModelType::Tts,
                format: ModelFormat::Onnx,
                size_bytes: 2 * 1024 * 1024,
                sha256: String::new(),
                download_url: "https://huggingface.co/Supertone/supertonic/resolve/main/onnx/duration_predictor.onnx".to_string(),
                min_ram_mb: 1024,
                requires_gpu: false,
                supported_languages: vec!["en".to_string(), "es".to_string(), "fr".to_string(), "de".to_string(), "ja".to_string(), "ko".to_string(), "zh".to_string()],
                status: ModelStatus::Available,
                installed_at: None,
                local_path: None,
            },
            CatalogItem {
                id: "indextts-2_5".to_string(),
                name: "IndexTTS 2 Expressive Emotion Voice".to_string(),
                description: "Multilingual expressive voice cloning with emotional prosody and pitch inflection conditioning (IndexTeam/IndexTTS-2)".to_string(),
                model_type: ModelType::Tts,
                format: ModelFormat::PyTorch,
                size_bytes: 3323 * 1024 * 1024,
                sha256: String::new(),
                download_url: "https://huggingface.co/IndexTeam/IndexTTS-2/resolve/main/gpt.pth".to_string(),
                min_ram_mb: 4096,
                requires_gpu: false,
                supported_languages: vec!["en".to_string(), "zh".to_string(), "ja".to_string(), "es".to_string(), "ar".to_string()],
                status: ModelStatus::Available,
                installed_at: None,
                local_path: None,
            },
            CatalogItem {
                id: "sherpa-onnx-asr".to_string(),
                name: "Sherpa-ONNX Whisper Tiny Encoder (Int8)".to_string(),
                description: "Ultra-low-resource int8 quantized offline speech recognition encoder (csukuangfj/sherpa-onnx-whisper-tiny.en)".to_string(),
                model_type: ModelType::Asr,
                format: ModelFormat::Onnx,
                size_bytes: 13 * 1024 * 1024,
                sha256: String::new(),
                download_url: "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-tiny.en/resolve/main/tiny.en-encoder.int8.onnx".to_string(),
                min_ram_mb: 512,
                requires_gpu: false,
                supported_languages: vec!["en".to_string()],
                status: ModelStatus::Available,
                installed_at: None,
                local_path: None,
            },
        ];

        for item in &mut models {
            let local_file =
                models_dir.join(Self::target_filename_for_model(&item.id, item.format));
            if local_file.exists() {
                item.status = ModelStatus::Installed;
                item.installed_at = Some(Utc::now());
                item.local_path = Some(local_file.to_string_lossy().to_string());
            }
        }

        let mut map = HashMap::new();
        for m in models {
            map.insert(m.id.clone(), m);
        }

        Self {
            items: Arc::new(RwLock::new(map)),
            allow_simulated_fallback: cfg!(test),
        }
    }

    /// List all catalog items with optional filtering by type or installation state.
    pub async fn list(
        &self,
        model_type: Option<ModelType>,
        installed_only: bool,
    ) -> Vec<CatalogItem> {
        let map = self.items.read().await;
        map.values()
            .filter(|item| {
                model_type.is_none_or(|t| item.model_type == t)
                    && (!installed_only || item.status == ModelStatus::Installed)
            })
            .cloned()
            .collect()
    }

    /// Look up a single catalog item by ID.
    pub async fn get(&self, id: &str) -> Option<CatalogItem> {
        let map = self.items.read().await;
        map.get(id).cloned()
    }

    /// Install/download a catalog model, validating host hardware constraints first.
    pub async fn install(&self, id: &str, hardware: &HardwareProfile) -> Result<CatalogItem> {
        let mut map = self.items.write().await;
        let item = map
            .get_mut(id)
            .ok_or_else(|| VoxForgeError::Engine(format!("Model '{id}' not found in catalog")))?;

        // Validate hardware RAM
        if hardware.total_memory_mb > 0 && hardware.total_memory_mb < item.min_ram_mb {
            return Err(VoxForgeError::HardwareUnsupported(format!(
                "Insufficient host RAM: model '{}' requires at least {}MB, but system only has {}MB",
                id, item.min_ram_mb, hardware.total_memory_mb
            )));
        }

        // Validate GPU requirement
        if item.requires_gpu && hardware.gpus.is_empty() {
            return Err(VoxForgeError::HardwareUnsupported(format!(
                "Dedicated GPU required: model '{}' requires a GPU accelerator, but none was detected",
                id
            )));
        }

        let models_dir = Self::models_dir();
        let filename = Self::target_filename_for_model(id, item.format);
        let target_path = models_dir.join(&filename);

        // Attempt live download if URL is HTTP/HTTPS unless explicitly disabled (or test simulated mode)
        let skip_download = self.allow_simulated_fallback
            || std::env::var("VOXFORGE_SKIP_DOWNLOAD")
                .map(|v| v == "1" || v == "true")
                .unwrap_or(false);

        if !skip_download
            && (item.download_url.starts_with("http://")
                || item.download_url.starts_with("https://"))
        {
            Self::download_file(&item.download_url, &target_path, &item.sha256)
                .await
                .map_err(|e| {
                    VoxForgeError::Engine(format!(
                        "Failed to download model '{}' weights from {}: {}",
                        id, item.download_url, e
                    ))
                })?;

            // Companion `.onnx.json` config required by Piper ONNX runtime
            if item.download_url.contains("piper-voices") && item.download_url.ends_with(".onnx") {
                let json_url = format!("{}.json", item.download_url);
                let json_dest = models_dir.join(format!("{filename}.json"));
                if let Err(e) = Self::download_file(&json_url, &json_dest, "").await {
                    tracing::warn!(error = %e, url = %json_url, "Companion Piper JSON config download failed");
                }
            }
        }

        item.status = ModelStatus::Installed;
        item.installed_at = Some(Utc::now());
        item.local_path = Some(target_path.to_string_lossy().to_string());

        Ok(item.clone())
    }

    /// Stream download weights from remote URL, computing SHA-256 and saving to disk.
    pub async fn download_file(
        url: &str,
        dest: &std::path::Path,
        expected_sha256: &str,
    ) -> Result<()> {
        use sha2::{Digest, Sha256};
        use tokio::io::AsyncWriteExt;

        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                VoxForgeError::AudioProcessing(format!("Failed to create models dir: {e}"))
            })?;
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| VoxForgeError::Engine(format!("Failed to build HTTP client: {e}")))?;

        let mut res =
            client.get(url).send().await.map_err(|e| {
                VoxForgeError::Engine(format!("Failed to connect to model host: {e}"))
            })?;

        if !res.status().is_success() {
            return Err(VoxForgeError::Engine(format!(
                "Model download HTTP error: status {}",
                res.status()
            )));
        }

        let mut file = tokio::fs::File::create(dest).await.map_err(|e| {
            VoxForgeError::AudioProcessing(format!("Failed to create model file: {e}"))
        })?;

        let mut hasher = Sha256::new();
        let mut total_bytes: u64 = 0;
        while let Some(chunk) = res
            .chunk()
            .await
            .map_err(|e| VoxForgeError::Engine(format!("Error streaming model weights: {e}")))?
        {
            total_bytes = total_bytes.saturating_add(chunk.len() as u64);
            hasher.update(&chunk);
            file.write_all(&chunk).await.map_err(|e| {
                VoxForgeError::AudioProcessing(format!("Failed writing model file: {e}"))
            })?;
        }

        file.flush().await.map_err(|e| {
            VoxForgeError::AudioProcessing(format!("Failed to flush model file: {e}"))
        })?;

        if total_bytes < 64 {
            let _ = tokio::fs::remove_file(dest).await;
            return Err(VoxForgeError::Engine(format!(
                "Downloaded file was unexpectedly small ({total_bytes} bytes)"
            )));
        }

        let hash = hex::encode(hasher.finalize());
        if !expected_sha256.is_empty()
            && !expected_sha256.starts_with("placeholder")
            && hash != expected_sha256
        {
            // Remove corrupted file
            let _ = tokio::fs::remove_file(dest).await;
            return Err(VoxForgeError::Engine(format!(
                "SHA-256 verification failed for downloaded model: expected {expected_sha256}, got {hash}"
            )));
        }

        Ok(())
    }

    /// Uninstall/remove local model weights.
    pub async fn uninstall(&self, id: &str) -> Result<CatalogItem> {
        let mut map = self.items.write().await;
        let item = map
            .get_mut(id)
            .ok_or_else(|| VoxForgeError::Engine(format!("Model '{id}' not found in catalog")))?;

        if item.status != ModelStatus::Installed {
            return Err(VoxForgeError::Engine(format!(
                "Model '{}' is not currently installed",
                id
            )));
        }

        if let Some(ref path_str) = item.local_path {
            let path = std::path::Path::new(path_str);
            if path.exists() {
                let _ = tokio::fs::remove_file(path).await;
            }
            let json_sidecar = std::path::PathBuf::from(format!("{path_str}.json"));
            if json_sidecar.exists() {
                let _ = tokio::fs::remove_file(json_sidecar).await;
            }
        }

        item.status = ModelStatus::Available;
        item.installed_at = None;
        item.local_path = None;

        Ok(item.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxforge_hardware::{GpuAccelerator, HardwareTier, SimdSupport};

    fn make_test_hardware(memory_mb: u64, has_gpu: bool) -> HardwareProfile {
        HardwareProfile {
            os: "windows".to_string(),
            arch: "x86_64".to_string(),
            cpu_brand: "Test CPU".to_string(),
            cpu_physical_cores: 8,
            cpu_logical_threads: 16,
            total_memory_mb: memory_mb,
            available_memory_mb: memory_mb,
            simd: SimdSupport {
                avx: true,
                avx2: true,
                avx512: false,
                neon: false,
            },
            gpus: if has_gpu {
                vec![GpuAccelerator {
                    name: "Test GPU".to_string(),
                    vram_mb: 8192,
                    has_cuda: true,
                    has_mps: false,
                    has_directml: true,
                }]
            } else {
                Vec::new()
            },
            assigned_tier: HardwareTier::Tier3Pro,
            recommended_engines: vec!["mock-tts".to_string()],
        }
    }

    #[tokio::test]
    async fn test_catalog_listing_and_filtering() {
        let store = ModelCatalogStore::with_curated_models();

        let all = store.list(None, false).await;
        assert!(all.len() >= 6);

        let tts_only = store.list(Some(ModelType::Tts), false).await;
        assert!(tts_only.iter().all(|m| m.model_type == ModelType::Tts));

        let installed = store.list(None, true).await;
        assert!(installed.iter().all(|m| m.status == ModelStatus::Installed));
    }

    #[tokio::test]
    async fn test_catalog_install_and_uninstall() {
        let store = ModelCatalogStore::with_curated_models();
        let hw = make_test_hardware(16384, false);

        // Install kokoro-v0_19
        let installed = store.install("kokoro-v0_19", &hw).await.unwrap();
        assert_eq!(installed.status, ModelStatus::Installed);
        assert!(installed.local_path.is_some());

        // Re-read
        let item = store.get("kokoro-v0_19").await.unwrap();
        assert_eq!(item.status, ModelStatus::Installed);

        // Uninstall
        let uninstalled = store.uninstall("kokoro-v0_19").await.unwrap();
        assert_eq!(uninstalled.status, ModelStatus::Available);
        assert!(uninstalled.local_path.is_none());
    }

    #[tokio::test]
    async fn test_hardware_constraint_rejection() {
        let store = ModelCatalogStore::with_curated_models();
        let hw = make_test_hardware(256, false); // Only 256MB RAM

        // Try installing qwen3-tts-base which needs 4096MB
        let res = store.install("qwen3-tts-base", &hw).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_strict_install_fails_on_download_error() {
        let store = ModelCatalogStore::with_curated_models().with_simulated_downloads(false);
        {
            let mut map = store.items.write().await;
            if let Some(item) = map.get_mut("kokoro-v0_19") {
                item.download_url = "http://127.0.0.1:1/nonexistent.onnx".to_string();
            }
        }
        let hw = make_test_hardware(16384, false);
        let res = store.install("kokoro-v0_19", &hw).await;
        assert!(res.is_err());
        let after = store.get("kokoro-v0_19").await.unwrap();
        assert_eq!(after.status, ModelStatus::Available);
    }
}
