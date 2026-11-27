use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use voxforge_core::error::{Result, VoxForgeError};
use voxforge_hardware::HardwareProfile;

use crate::models::{assess_system_fit, CatalogItem, ModelFormat, ModelStatus, ModelType};

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
                    docs_url: "https://huggingface.co/rhasspy/piper-voices".to_string(),
                    license: "MIT / Speaker Voice License".to_string(),
                    pros: vec![
                        "Zero Python required via native Piper C++ CLI".to_string(),
                        "100% offline ONNX inference faster than real-time on CPU".to_string(),
                        "Tiny disk & RAM footprint (~60 MB)".to_string(),
                    ],
                    cons: vec![
                        "Single-speaker preset (no zero-shot voice cloning)".to_string(),
                        "Fixed intonation with limited emotional steering".to_string(),
                    ],
                    best_for: vec![
                        "Low-latency offline CPU voice agents".to_string(),
                        "Privacy-strict air-gapped narration".to_string(),
                    ],
                    min_ram_mb: 512,
                    min_vram_mb: 0,
                    requires_gpu: false,
                    runtime_requirement: "native_piper_cli".to_string(),
                    supported_languages: vec![lang.to_string()],
                    status: ModelStatus::Available,
                    installed_at: None,
                    local_path: None,
                    system_fit: None,
                }
            };

        let mut models = vec![
            piper_voice_entry(
                "piper-en-lessac-medium",
                "Piper Lessac Medium (en-US)",
                "Balanced studio-grade offline English (US) neural voice trained on VITS ONNX with crisp articulation and zero cloud dependency.",
                "en-US",
                "en/en_US/lessac/medium/en_US-lessac-medium.onnx",
                60,
            ),
            piper_voice_entry(
                "en_US-amy-medium",
                "Piper Amy Medium (en-US)",
                "Warm, conversational English (US) female Piper ONNX voice ideal for assistants and UI feedback with zero Python dependency.",
                "en-US",
                "en/en_US/amy/medium/en_US-amy-medium.onnx",
                60,
            ),
            piper_voice_entry(
                "en_US-danny-low",
                "Piper Danny Low (en-US)",
                "Ultra-low-latency compact English (US) male Piper ONNX voice tuned for Raspberry Pi and constrained CPU hardware.",
                "en-US",
                "en/en_US/danny/low/en_US-danny-low.onnx",
                60,
            ),
            piper_voice_entry(
                "en_GB-alan-medium",
                "Piper Alan Medium (en-GB)",
                "Articulate British English male Piper ONNX voice delivering clean documentary and technical narration fully offline.",
                "en-GB",
                "en/en_GB/alan/medium/en_GB-alan-medium.onnx",
                60,
            ),
            piper_voice_entry(
                "es_ES-carlfm-x_low",
                "Piper Carlfm X-Low (es-ES)",
                "Ultra-compact 27 MB European Spanish male Piper ONNX voice built for instant offline edge responses.",
                "es-ES",
                "es/es_ES/carlfm/x_low/es_ES-carlfm-x_low.onnx",
                27,
            ),
            piper_voice_entry(
                "fr_FR-siwis-medium",
                "Piper Siwis Medium (fr-FR)",
                "Expressive French female Piper VITS ONNX voice trained on native conversational corpora for natural French cadence.",
                "fr-FR",
                "fr/fr_FR/siwis/medium/fr_FR-siwis-medium.onnx",
                60,
            ),
            piper_voice_entry(
                "de_DE-thorsten-medium",
                "Piper Thorsten Medium (de-DE)",
                "Clean German male Piper ONNX voice from the Thorsten-Voice open dataset, strong consonant clarity on CPU.",
                "de-DE",
                "de/de_DE/thorsten/medium/de_DE-thorsten-medium.onnx",
                60,
            ),
            CatalogItem {
                id: "kokoro-v0_19".to_string(),
                name: "Kokoro 82M Multilingual (v1.0 ONNX)".to_string(),
                description: "82M-parameter open-weights StyleTTS2-inspired synthesis engine delivering studio-grade prosody across English, Japanese, Mandarin, French, and Spanish at tiny footprint (onnx-community/Kokoro-82M-v1.0-ONNX).".to_string(),
                model_type: ModelType::Tts,
                format: ModelFormat::Onnx,
                size_bytes: 310 * 1024 * 1024,
                sha256: String::new(),
                download_url: "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/main/onnx/model.onnx".to_string(),
                docs_url: "https://huggingface.co/hexgrad/Kokoro-82M".to_string(),
                license: "Apache-2.0".to_string(),
                pros: vec![
                    "Exceptional naturalness for an 82M parameter Apache-2.0 model".to_string(),
                    "Fast on both CPU and modest GPUs (~310 MB download)".to_string(),
                    "Rich built-in multi-voice style blending".to_string(),
                ],
                cons: vec![
                    "No arbitrary zero-shot reference cloning (uses curated voicepacks)".to_string(),
                    "Requires optional Python Kokoro sidecar runtime".to_string(),
                ],
                best_for: vec![
                    "Long-form audiobooks & podcasts on CPU/mid-range GPUs".to_string(),
                    "Commercial projects needing Apache-2.0 quality".to_string(),
                ],
                min_ram_mb: 2048,
                min_vram_mb: 0,
                requires_gpu: false,
                runtime_requirement: "python_kokoro".to_string(),
                supported_languages: vec!["en".to_string(), "es".to_string(), "fr".to_string(), "ja".to_string(), "zh".to_string()],
                status: ModelStatus::Available,
                installed_at: None,
                local_path: None,
                system_fit: None,
            },
            CatalogItem {
                id: "qwen3-tts-base".to_string(),
                name: "Qwen3-TTS 12Hz 0.6B Base".to_string(),
                description: "Zero-shot voice cloning speech transformer using discrete 12 Hz acoustic tokens and speaker conditioning for natural English and Mandarin cloning (Qwen/Qwen3-TTS-12Hz-0.6B-Base).".to_string(),
                model_type: ModelType::Tts,
                format: ModelFormat::Safetensors,
                size_bytes: 1745 * 1024 * 1024,
                sha256: String::new(),
                download_url: "https://huggingface.co/Qwen/Qwen3-TTS-12Hz-0.6B-Base/resolve/main/model.safetensors".to_string(),
                docs_url: "https://huggingface.co/Qwen/Qwen3-TTS-12Hz-0.6B-Base".to_string(),
                license: "Apache-2.0".to_string(),
                pros: vec![
                    "Strong 3-second zero-shot timbre fidelity in English & Chinese".to_string(),
                    "Supports prosody & style instruction steering".to_string(),
                ],
                cons: vec![
                    "1.7 GB weight size and slower generation without CUDA GPU".to_string(),
                    "Focused primarily on English and Mandarin".to_string(),
                ],
                best_for: vec![
                    "High-fidelity English/Chinese zero-shot cloning".to_string(),
                    "Director style-controlled dialogue".to_string(),
                ],
                min_ram_mb: 4096,
                min_vram_mb: 4096,
                requires_gpu: false,
                runtime_requirement: "python_sidecar".to_string(),
                supported_languages: vec!["en".to_string(), "zh".to_string()],
                status: ModelStatus::Available,
                installed_at: None,
                local_path: None,
                system_fit: None,
            },
            CatalogItem {
                id: "whisper-base".to_string(),
                name: "Whisper Base Encoder (ONNX)".to_string(),
                description: "Balanced 79 MB ONNX multilingual automatic speech recognition encoder ideal for fast Auto-Transcribe reference clip captioning (onnx-community/whisper-base).".to_string(),
                model_type: ModelType::Asr,
                format: ModelFormat::Onnx,
                size_bytes: 79 * 1024 * 1024,
                sha256: String::new(),
                download_url: "https://huggingface.co/onnx-community/whisper-base/resolve/main/onnx/encoder_model.onnx".to_string(),
                docs_url: "https://huggingface.co/onnx-community/whisper-base".to_string(),
                license: "Apache-2.0 / MIT".to_string(),
                pros: vec![
                    "Compact 79 MB ONNX weights running fast on CPU".to_string(),
                    "Solid multilingual accuracy for short voice-clone clips".to_string(),
                ],
                cons: vec![
                    "Lower accuracy than Whisper Small/Medium on heavy accents or noisy rooms".to_string(),
                ],
                best_for: vec![
                    "Fast voice-clone clip auto-transcription on laptops".to_string(),
                    "Real-time dictation drafts".to_string(),
                ],
                min_ram_mb: 1024,
                min_vram_mb: 0,
                requires_gpu: false,
                runtime_requirement: "python_sidecar".to_string(),
                supported_languages: vec!["en".to_string(), "es".to_string(), "fr".to_string(), "de".to_string(), "zh".to_string()],
                status: ModelStatus::Available,
                installed_at: None,
                local_path: None,
                system_fit: None,
            },
            CatalogItem {
                id: "whisper-small".to_string(),
                name: "Whisper Small Encoder (ONNX)".to_string(),
                description: "Higher-accuracy 336 MB ONNX multilingual transcription encoder for noisy audio, punctuation stability, and studio dubbing scripts (onnx-community/whisper-small).".to_string(),
                model_type: ModelType::Asr,
                format: ModelFormat::Onnx,
                size_bytes: 336 * 1024 * 1024,
                sha256: String::new(),
                download_url: "https://huggingface.co/onnx-community/whisper-small/resolve/main/onnx/encoder_model.onnx".to_string(),
                docs_url: "https://huggingface.co/onnx-community/whisper-small".to_string(),
                license: "Apache-2.0 / MIT".to_string(),
                pros: vec![
                    "Significantly fewer word errors than Base on accented speech".to_string(),
                    "Clean punctuation for multi-speaker script alignment".to_string(),
                ],
                cons: vec![
                    "~4x larger download and slower on low-power CPUs than Base".to_string(),
                ],
                best_for: vec![
                    "Accurate dubbing & reference voice transcript prep".to_string(),
                    "Longer podcast/interview ingestion".to_string(),
                ],
                min_ram_mb: 2048,
                min_vram_mb: 0,
                requires_gpu: false,
                runtime_requirement: "python_sidecar".to_string(),
                supported_languages: vec!["en".to_string(), "es".to_string(), "fr".to_string(), "de".to_string()],
                status: ModelStatus::Available,
                installed_at: None,
                local_path: None,
                system_fit: None,
            },
            CatalogItem {
                id: "silero-vad".to_string(),
                name: "Silero Voice Activity Detector".to_string(),
                description: "Ultra-light 2 MB enterprise ONNX speech-presence and pause detector filtering silence, breaths, and background noise before cloning or ASR (snakers4/silero-vad).".to_string(),
                model_type: ModelType::Vad,
                format: ModelFormat::Onnx,
                size_bytes: 2 * 1024 * 1024,
                sha256: String::new(),
                download_url: "https://github.com/snakers4/silero-vad/raw/master/src/silero_vad/data/silero_vad.onnx".to_string(),
                docs_url: "https://github.com/snakers4/silero-vad".to_string(),
                license: "MIT".to_string(),
                pros: vec![
                    "2 MB footprint processes 30ms audio chunks in <1ms on CPU".to_string(),
                    "Dramatically improves cloning reference cleanup".to_string(),
                ],
                cons: vec![
                    "Utility detector only (does not synthesize speech)".to_string(),
                ],
                best_for: vec![
                    "Trimming silence from mic recordings & telephony streams".to_string(),
                    "Pre-filtering noisy voice cloning references".to_string(),
                ],
                min_ram_mb: 256,
                min_vram_mb: 0,
                requires_gpu: false,
                runtime_requirement: "onnx_runtime".to_string(),
                supported_languages: vec!["*".to_string()],
                status: ModelStatus::Available,
                installed_at: None,
                local_path: None,
                system_fit: None,
            },
            CatalogItem {
                id: "omnivoice-base".to_string(),
                name: "OmniVoice 600+ Multilingual Zero-Shot".to_string(),
                description: "Massively multilingual 600+ language zero-shot voice cloning diffusion/LM architecture supporting cross-lingual cloning across Indic, CJK, Arabic, and European languages (k2-fsa/OmniVoice).".to_string(),
                model_type: ModelType::Tts,
                format: ModelFormat::Safetensors,
                size_bytes: 2337 * 1024 * 1024,
                sha256: String::new(),
                download_url: "https://huggingface.co/k2-fsa/OmniVoice/resolve/main/model.safetensors".to_string(),
                docs_url: "https://huggingface.co/k2-fsa/OmniVoice".to_string(),
                license: "Apache-2.0".to_string(),
                pros: vec![
                    "Covers 600+ languages & cross-lingual voice preservation".to_string(),
                    "High similarity cloning from short 3–10s samples".to_string(),
                ],
                cons: vec![
                    "Large 2.3 GB weights; slow without >=4 GB CUDA/MPS GPU".to_string(),
                    "Requires Python Neural Sidecar running on port 9000".to_string(),
                ],
                best_for: vec![
                    "Multilingual & Indic voice cloning (Malayalam, Hindi, Tamil, Arabic)".to_string(),
                    "Cross-lingual dubbing keeping original speaker identity".to_string(),
                ],
                min_ram_mb: 4096,
                min_vram_mb: 4096,
                requires_gpu: false,
                runtime_requirement: "python_sidecar".to_string(),
                supported_languages: vec!["*".to_string()],
                status: ModelStatus::Available,
                installed_at: None,
                local_path: None,
                system_fit: None,
            },
            CatalogItem {
                id: "pocket-tts".to_string(),
                name: "PocketTTS Real-Time CPU Voice".to_string(),
                description: "Kyutai Labs compact ~100M parameter continuous audio LM engine engineered specifically to stream natural speech faster than real-time on standard laptop CPUs (kyutai/pocket-tts-without-voice-cloning).".to_string(),
                model_type: ModelType::Tts,
                format: ModelFormat::Safetensors,
                size_bytes: 225 * 1024 * 1024,
                sha256: String::new(),
                download_url: "https://huggingface.co/kyutai/pocket-tts-without-voice-cloning/resolve/main/tts_b6369a24.safetensors".to_string(),
                docs_url: "https://huggingface.co/kyutai/pocket-tts-without-voice-cloning".to_string(),
                license: "CC-BY-4.0".to_string(),
                pros: vec![
                    "Tiny 225 MB safetensors package runs smoothly on CPU-only hosts".to_string(),
                    "Very fast time-to-first-audio for interactive dictation".to_string(),
                ],
                cons: vec![
                    "This release excludes zero-shot voice cloning weights".to_string(),
                    "Limited to core European languages (en/fr/de/es/it/pt)".to_string(),
                ],
                best_for: vec![
                    "CPU-only laptops needing fast conversational speech".to_string(),
                    "Live voice assistants and screen readers".to_string(),
                ],
                min_ram_mb: 512,
                min_vram_mb: 0,
                requires_gpu: false,
                runtime_requirement: "python_sidecar".to_string(),
                supported_languages: vec!["en".to_string(), "fr".to_string(), "de".to_string(), "es".to_string(), "it".to_string(), "pt".to_string()],
                status: ModelStatus::Available,
                installed_at: None,
                local_path: None,
                system_fit: None,
            },
            CatalogItem {
                id: "supertonic-3".to_string(),
                name: "Supertonic High-Fidelity Predictor (ONNX)".to_string(),
                description: "Supertone ultra-fast flow-matching ONNX speech predictor designed for crisp 44.1 kHz studio cadence across English, Spanish, French, German, Japanese, Korean, and Chinese (Supertone/supertonic).".to_string(),
                model_type: ModelType::Tts,
                format: ModelFormat::Onnx,
                size_bytes: 2 * 1024 * 1024,
                sha256: String::new(),
                download_url: "https://huggingface.co/Supertone/supertonic/resolve/main/onnx/duration_predictor.onnx".to_string(),
                docs_url: "https://huggingface.co/Supertone/supertonic".to_string(),
                license: "OpenRail-M".to_string(),
                pros: vec![
                    "Extremely fast ONNX duration prediction suitable for low-end CPUs".to_string(),
                    "Supports East Asian + European languages cleanly".to_string(),
                ],
                cons: vec![
                    "Component ONNX file only; pairs with Supertone full ONNX runtime stack".to_string(),
                    "Check OpenRail-M license terms before commercial deployment".to_string(),
                ],
                best_for: vec![
                    "Fast CPU duration modeling experiments".to_string(),
                    "Low-overhead ONNX timing pipelines".to_string(),
                ],
                min_ram_mb: 1024,
                min_vram_mb: 0,
                requires_gpu: false,
                runtime_requirement: "onnx_runtime".to_string(),
                supported_languages: vec!["en".to_string(), "es".to_string(), "fr".to_string(), "de".to_string(), "ja".to_string(), "ko".to_string(), "zh".to_string()],
                status: ModelStatus::Available,
                installed_at: None,
                local_path: None,
                system_fit: None,
            },
            CatalogItem {
                id: "indextts-2_5".to_string(),
                name: "IndexTTS 2 Expressive Emotion Voice".to_string(),
                description: "Bilibili IndexTeam autoregressive + flow-matching expressive TTS model separating timbre cloning from explicit emotion & duration control (IndexTeam/IndexTTS-2).".to_string(),
                model_type: ModelType::Tts,
                format: ModelFormat::PyTorch,
                size_bytes: 3323 * 1024 * 1024,
                sha256: String::new(),
                download_url: "https://huggingface.co/IndexTeam/IndexTTS-2/resolve/main/gpt.pth".to_string(),
                docs_url: "https://huggingface.co/IndexTeam/IndexTTS-2".to_string(),
                license: "Apache-2.0 / Check Repo Terms".to_string(),
                pros: vec![
                    "Top-tier emotional expressiveness and disentangled emotion vectors".to_string(),
                    "Precise duration alignment for video dubbing".to_string(),
                ],
                cons: vec![
                    "Heaviest download (3.3 GB GPT weights) and VRAM-hungry (~6 GB recommended)".to_string(),
                    "Slow on CPU-only or 4 GB VRAM GPUs".to_string(),
                ],
                best_for: vec![
                    "Dramatic game character dialogue & emotional acting".to_string(),
                    "Synced multi-emotion film/video dubbing on workstation GPUs".to_string(),
                ],
                min_ram_mb: 4096,
                min_vram_mb: 6144,
                requires_gpu: false,
                runtime_requirement: "python_sidecar".to_string(),
                supported_languages: vec!["en".to_string(), "zh".to_string(), "ja".to_string(), "es".to_string(), "ar".to_string()],
                status: ModelStatus::Available,
                installed_at: None,
                local_path: None,
                system_fit: None,
            },
            CatalogItem {
                id: "sherpa-onnx-asr".to_string(),
                name: "Sherpa-ONNX Whisper Tiny Encoder (Int8)".to_string(),
                description: "Next-gen Kaldi / Sherpa int8 quantized Whisper Tiny ONNX encoder (13 MB) built for instant offline English ASR on minimal RAM devices (csukuangfj/sherpa-onnx-whisper-tiny.en).".to_string(),
                model_type: ModelType::Asr,
                format: ModelFormat::Onnx,
                size_bytes: 13 * 1024 * 1024,
                sha256: String::new(),
                download_url: "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-tiny.en/resolve/main/tiny.en-encoder.int8.onnx".to_string(),
                docs_url: "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-tiny.en".to_string(),
                license: "MIT / Apache-2.0".to_string(),
                pros: vec![
                    "Microscopic 13 MB download runs effortlessly on any CPU".to_string(),
                    "Int8 quantization cuts RAM below 512 MB".to_string(),
                ],
                cons: vec![
                    "English-only and lower robustness on background noise than Whisper Small".to_string(),
                ],
                best_for: vec![
                    "Low-RAM laptops & quick English reference clip captioning".to_string(),
                    "Embedded push-to-talk commands".to_string(),
                ],
                min_ram_mb: 512,
                min_vram_mb: 0,
                requires_gpu: false,
                runtime_requirement: "onnx_runtime".to_string(),
                supported_languages: vec!["en".to_string()],
                status: ModelStatus::Available,
                installed_at: None,
                local_path: None,
                system_fit: None,
            },
            CatalogItem {
                id: "chatterbox-turbo".to_string(),
                name: "Chatterbox-Turbo 350M (Sub-150ms Cloning)".to_string(),
                description: "Resemble AI distilled MIT-licensed 350M GPT-2/GPT mel-decoder engineered for <150ms streaming latency, 5-second zero-shot cloning, and inline tags like [laugh], [chuckle], and [sigh] (ResembleAI/chatterbox-turbo).".to_string(),
                model_type: ModelType::Tts,
                format: ModelFormat::Safetensors,
                size_bytes: 1827 * 1024 * 1024,
                sha256: String::new(),
                download_url: "https://huggingface.co/ResembleAI/chatterbox-turbo/resolve/main/t3_turbo_v1.safetensors".to_string(),
                docs_url: "https://huggingface.co/ResembleAI/chatterbox-turbo".to_string(),
                license: "MIT".to_string(),
                pros: vec![
                    "Sub-150ms time-to-first-audio on CUDA GPUs with 1-step mel decoder".to_string(),
                    "Native 5-second zero-shot cloning + realistic [laugh]/[chuckle]/[cough] tags".to_string(),
                    "Permissive MIT license".to_string(),
                ],
                cons: vec![
                    "English-only (use Chatterbox 500M Multilingual for other languages)".to_string(),
                    "1.8 GB checkpoint + requires PyTorch sidecar & ~4 GB VRAM for real-time speed".to_string(),
                ],
                best_for: vec![
                    "Real-time expressive English AI voice agents".to_string(),
                    "Natural storytelling with paralinguistic reactions".to_string(),
                ],
                min_ram_mb: 4096,
                min_vram_mb: 4096,
                requires_gpu: false,
                runtime_requirement: "python_sidecar".to_string(),
                supported_languages: vec!["en".to_string()],
                status: ModelStatus::Available,
                installed_at: None,
                local_path: None,
                system_fit: None,
            },
            CatalogItem {
                id: "chatterbox-multilingual".to_string(),
                name: "Chatterbox 500M Multilingual (23 Langs)".to_string(),
                description: "Resemble AI flagship 500M zero-shot voice cloning foundation model covering 23 major languages with exaggerated emotion controls and PerTh neural watermarking (ResembleAI/chatterbox).".to_string(),
                model_type: ModelType::Tts,
                format: ModelFormat::Safetensors,
                size_bytes: 2044 * 1024 * 1024,
                sha256: String::new(),
                download_url: "https://huggingface.co/ResembleAI/chatterbox/resolve/main/t3_23lang.safetensors".to_string(),
                docs_url: "https://huggingface.co/ResembleAI/chatterbox".to_string(),
                license: "MIT".to_string(),
                pros: vec![
                    "Covers 23 languages with strong zero-shot speaker similarity".to_string(),
                    "Exaggeration/CFG steering from calm narration to dramatic acting".to_string(),
                ],
                cons: vec![
                    "Larger 2.0 GB footprint & slower than Turbo (prefers >=4–6 GB VRAM)".to_string(),
                    "Requires Python sidecar environment".to_string(),
                ],
                best_for: vec![
                    "Expressive Hindi, Spanish, French, German, Arabic, Japanese & Chinese cloning".to_string(),
                    "Studio-grade audiobook & dubbing rendering".to_string(),
                ],
                min_ram_mb: 4096,
                min_vram_mb: 4096,
                requires_gpu: false,
                runtime_requirement: "python_sidecar".to_string(),
                supported_languages: vec!["en".to_string(), "es".to_string(), "fr".to_string(), "de".to_string(), "hi".to_string(), "ja".to_string(), "zh".to_string(), "ar".to_string()],
                status: ModelStatus::Available,
                installed_at: None,
                local_path: None,
                system_fit: None,
            },
            CatalogItem {
                id: "kitten-tts-2".to_string(),
                name: "KittenTTS 2 Lightweight CPU Engine".to_string(),
                description: "KittenML ultra-compact CPU-friendly expressive synthesis & short-sample cloning checkpoint (~510 MB) built for private laptops without heavyweight GPUs (KittenML/kitten-tts-2).".to_string(),
                model_type: ModelType::Tts,
                format: ModelFormat::PyTorch,
                size_bytes: 510 * 1024 * 1024,
                sha256: String::new(),
                download_url: "https://huggingface.co/KittenML/kitten-tts-2/resolve/main/cpp/default/decoder.pt".to_string(),
                docs_url: "https://huggingface.co/KittenML/kitten-tts-2".to_string(),
                license: "Apache-2.0".to_string(),
                pros: vec![
                    "~510 MB total footprint runs smoothly on 1 GB RAM CPU hosts".to_string(),
                    "Supports in-context voice cloning and expression tags without CUDA".to_string(),
                ],
                cons: vec![
                    "English-focused voice coverage".to_string(),
                    "Lower speaker cloning nuance than 500M+ GPU foundation models".to_string(),
                ],
                best_for: vec![
                    "Private CPU-only English synthesis & lightweight cloning".to_string(),
                    "Developer workstations conserving VRAM".to_string(),
                ],
                min_ram_mb: 1024,
                min_vram_mb: 0,
                requires_gpu: false,
                runtime_requirement: "python_sidecar".to_string(),
                supported_languages: vec!["en".to_string()],
                status: ModelStatus::Available,
                installed_at: None,
                local_path: None,
                system_fit: None,
            },
            CatalogItem {
                id: "canto-tts-nano".to_string(),
                name: "Canto-TTS Nano (HK Cantonese ONNX)".to_string(),
                description: "0.1B parameter MOSS-TTS-Nano ONNX model fine-tuned specifically for natural Hong Kong Cantonese tone accuracy and seamless inline Cantonese-English code-switching on CPU (typangaa/canto-tts-nano).".to_string(),
                model_type: ModelType::Tts,
                format: ModelFormat::Onnx,
                size_bytes: 421 * 1024 * 1024,
                sha256: String::new(),
                download_url: "https://huggingface.co/typangaa/canto-tts-nano/resolve/main/MOSS-TTS-Nano-cantophon-ONNX/moss_tts_global_shared.data".to_string(),
                docs_url: "https://huggingface.co/typangaa/canto-tts-nano".to_string(),
                license: "Apache-2.0".to_string(),
                pros: vec![
                    "CPU-first ONNX (~421 MB) with ~84% Cantonese tone accuracy".to_string(),
                    "Handles mixed Hong Kong Cantonese + English sentences cleanly offline".to_string(),
                ],
                cons: vec![
                    "Specialized purely for Cantonese/English — use Edge-TTS or OmniVoice for other languages".to_string(),
                    "Requires optional Python ONNX runner package for local execution".to_string(),
                ],
                best_for: vec![
                    "Offline Hong Kong Cantonese voiceovers & bilingual Cantonese-English UI".to_string(),
                    "CPU-only machines needing local Cantonese speech".to_string(),
                ],
                min_ram_mb: 1024,
                min_vram_mb: 0,
                requires_gpu: false,
                runtime_requirement: "python_sidecar".to_string(),
                supported_languages: vec!["zh-HK".to_string(), "en".to_string()],
                status: ModelStatus::Available,
                installed_at: None,
                local_path: None,
                system_fit: None,
            },
            CatalogItem {
                id: "f5-tts-v1".to_string(),
                name: "F5-TTS v1 Base Flow-Matching".to_string(),
                description: "Non-autoregressive flow-matching diffusion transformer (DiT + ConvNeXt V2) known for smooth natural prosody and zero-shot English/Chinese cloning (SWivid/F5-TTS).".to_string(),
                model_type: ModelType::Tts,
                format: ModelFormat::Safetensors,
                size_bytes: 1286 * 1024 * 1024,
                sha256: String::new(),
                download_url: "https://huggingface.co/SWivid/F5-TTS/resolve/main/F5TTS_v1_Base/model_1250000.safetensors".to_string(),
                docs_url: "https://huggingface.co/SWivid/F5-TTS".to_string(),
                license: "CC-BY-NC-4.0 (Check Repo)".to_string(),
                pros: vec![
                    "Very natural non-robotic cadence via continuous flow-matching".to_string(),
                    "Strong speaker cloning consistency across medium/long paragraphs".to_string(),
                ],
                cons: vec![
                    "Non-commercial CC-BY-NC weight license restrictions may apply".to_string(),
                    "Iterative NFE diffusion steps need CUDA GPU for real-time speed".to_string(),
                ],
                best_for: vec![
                    "High-naturalness research, personal narration & podcast prototyping".to_string(),
                    "English & Mandarin cloning with clean reference audio".to_string(),
                ],
                min_ram_mb: 4096,
                min_vram_mb: 4096,
                requires_gpu: false,
                runtime_requirement: "python_sidecar".to_string(),
                supported_languages: vec!["en".to_string(), "zh".to_string()],
                status: ModelStatus::Available,
                installed_at: None,
                local_path: None,
                system_fit: None,
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

    /// List all catalog items enriched with live host compatibility scoring.
    pub async fn list_for_hardware(
        &self,
        model_type: Option<ModelType>,
        installed_only: bool,
        hardware: &HardwareProfile,
    ) -> Vec<CatalogItem> {
        let mut items: Vec<CatalogItem> = self
            .list(model_type, installed_only)
            .await
            .into_iter()
            .map(|mut item| {
                item.system_fit = Some(assess_system_fit(&item, hardware));
                item
            })
            .collect();
        items.sort_by(|a, b| {
            let score_a = a.system_fit.as_ref().map_or(0, |s| s.score);
            let score_b = b.system_fit.as_ref().map_or(0, |s| s.score);
            score_b.cmp(&score_a).then_with(|| a.name.cmp(&b.name))
        });
        items
    }

    /// Normalize engine-facing voice IDs (e.g. `en_US-lessac-medium`) to catalog IDs.
    pub fn canonical_model_id(id: &str) -> &str {
        match id {
            "en_US-lessac-medium" => "piper-en-lessac-medium",
            other => other,
        }
    }

    /// Look up a single catalog item by ID.
    pub async fn get(&self, id: &str) -> Option<CatalogItem> {
        let canonical = Self::canonical_model_id(id);
        let map = self.items.read().await;
        map.get(canonical).cloned()
    }

    /// Look up a single catalog item with live host system-fit assessment.
    pub async fn get_for_hardware(
        &self,
        id: &str,
        hardware: &HardwareProfile,
    ) -> Option<CatalogItem> {
        self.get(id).await.map(|mut item| {
            item.system_fit = Some(assess_system_fit(&item, hardware));
            item
        })
    }

    /// Install/download a catalog model, validating host hardware constraints first.
    pub async fn install(&self, id: &str, hardware: &HardwareProfile) -> Result<CatalogItem> {
        let canonical = Self::canonical_model_id(id).to_string();
        let mut map = self.items.write().await;
        let item = map
            .get_mut(&canonical)
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
        let filename = Self::target_filename_for_model(&canonical, item.format);
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
        let canonical = Self::canonical_model_id(id);
        let mut map = self.items.write().await;
        let item = map
            .get_mut(canonical)
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

    #[tokio::test]
    async fn test_catalog_metadata_docs_and_system_fit() {
        let store = ModelCatalogStore::with_curated_models();
        let cpu_only = make_test_hardware(8192, false);
        let items = store.list_for_hardware(None, false, &cpu_only).await;
        assert!(!items.is_empty());

        for item in &items {
            assert!(
                item.docs_url.starts_with("https://"),
                "missing docs_url for {}",
                item.id
            );
            assert!(!item.license.is_empty(), "missing license for {}", item.id);
            assert!(!item.pros.is_empty(), "missing pros for {}", item.id);
            assert!(!item.cons.is_empty(), "missing cons for {}", item.id);
            assert!(
                !item.best_for.is_empty(),
                "missing best_for for {}",
                item.id
            );
            assert!(
                !item.runtime_requirement.is_empty(),
                "missing runtime_requirement for {}",
                item.id
            );
            assert!(
                item.system_fit.is_some(),
                "missing system_fit for {}",
                item.id
            );
        }

        let piper = store
            .get_for_hardware("piper-en-lessac-medium", &cpu_only)
            .await
            .unwrap();
        assert_eq!(
            piper.system_fit.unwrap().tier,
            crate::models::SystemFitTier::Optimal
        );

        let indextts = store
            .get_for_hardware("indextts-2_5", &cpu_only)
            .await
            .unwrap();
        assert_eq!(
            indextts.system_fit.unwrap().tier,
            crate::models::SystemFitTier::CpuSlow
        );

        let workstation_gpu = make_test_hardware(32768, true);
        let chatterbox = store
            .get_for_hardware("chatterbox-turbo", &workstation_gpu)
            .await
            .unwrap();
        assert_eq!(
            chatterbox.system_fit.unwrap().tier,
            crate::models::SystemFitTier::Optimal
        );
    }

    #[tokio::test]
    async fn test_piper_lessac_alias_resolution() {
        let store = ModelCatalogStore::with_curated_models();
        let via_engine_id = store.get("en_US-lessac-medium").await.unwrap();
        let via_catalog_id = store.get("piper-en-lessac-medium").await.unwrap();
        assert_eq!(via_engine_id.id, "piper-en-lessac-medium");
        assert_eq!(via_engine_id.id, via_catalog_id.id);
    }
}
