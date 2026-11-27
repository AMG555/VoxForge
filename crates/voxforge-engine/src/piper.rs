use async_trait::async_trait;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use voxforge_audio::wav::WavEncoder;
use voxforge_core::error::{Result, VoxForgeError};
use voxforge_core::models::{AudioChunk, Gender, Voice};

use crate::traits::{EngineCapabilities, SynthesisRequest, TtsEngine};

/// Piper local neural text-to-speech engine running ONNX models.
/// Supports both local process invocation via `piper` binary and HTTP sidecars.
pub struct PiperTtsEngine {
    sample_rate: u32,
    piper_path: Option<String>,
    sidecar_url: Option<String>,
    models_dir: PathBuf,
    http_client: reqwest::Client,
}

impl Default for PiperTtsEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PiperTtsEngine {
    pub fn new() -> Self {
        let piper_path = std::env::var("VOXFORGE_PIPER_PATH").ok().or_else(|| {
            // Check standalone native piper CLI first, then managed/system `python -m piper` module.
            if let Some(bin) = which_piper() {
                Some(bin)
            } else {
                which_python_piper().map(|py| format!("python-module:{py}"))
            }
        });

        let sidecar_url = std::env::var("VOXFORGE_PIPER_URL")
            .ok()
            .or_else(|| std::env::var("VOXFORGE_KOKORO_URL").ok());

        let models_dir = std::env::var("VOXFORGE_MODELS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("models"));

        Self {
            sample_rate: 22050,
            piper_path,
            sidecar_url,
            models_dir,
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
        }
    }

    pub fn with_sidecar_url(mut self, url: impl Into<String>) -> Self {
        self.sidecar_url = Some(url.into());
        self
    }

    pub fn with_piper_path(mut self, path: impl Into<String>) -> Self {
        self.piper_path = Some(path.into());
        self
    }

    pub fn with_models_dir(mut self, dir: PathBuf) -> Self {
        self.models_dir = dir;
        self
    }

    /// Map engine voice IDs to exact Model Catalog IDs when naming differs.
    fn catalog_id_for_voice(voice_id: &str) -> &str {
        if voice_id == "en_US-lessac-medium" {
            "piper-en-lessac-medium"
        } else {
            voice_id
        }
    }

    /// Resolve onnx model path for the given voice_id (strictly per-voice so German/French voices never silently speak English Lessac).
    fn resolve_model_path(&self, voice_id: &str) -> Option<PathBuf> {
        let mut candidates = vec![
            self.models_dir.join(format!("{voice_id}.onnx")),
            self.models_dir.join(format!("piper-{voice_id}.onnx")),
        ];
        if voice_id == "en_US-lessac-medium" || voice_id == "piper-en-lessac-medium" {
            candidates.push(self.models_dir.join("piper-en-lessac-medium.onnx"));
            candidates.push(self.models_dir.join("en_US-lessac-medium.onnx"));
        }

        for c in &candidates {
            if c.exists() {
                return Some(c.clone());
            }
        }

        None
    }

    /// Synthesize via remote HTTP sidecar (Piper or Kokoro FastAPI)
    async fn synthesize_via_http(&self, url: &str, req: &SynthesisRequest) -> Result<AudioChunk> {
        let endpoint = if url.ends_with("/v1/audio/speech") || url.ends_with("/speech") {
            url.to_string()
        } else {
            format!("{}/v1/audio/speech", url.trim_end_matches('/'))
        };

        let body = serde_json::json!({
            "model": "piper",
            "input": req.text,
            "voice": req.voice_id,
            "response_format": "wav",
            "speed": req.speed
        });

        debug!(endpoint = %endpoint, voice = %req.voice_id, "Dispatching Piper synthesis to HTTP sidecar");

        let res = self
            .http_client
            .post(&endpoint)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                VoxForgeError::Engine(format!("Piper HTTP sidecar request failed: {e}"))
            })?;

        if !res.status().is_success() {
            let status = res.status();
            let err_text = res.text().await.unwrap_or_default();
            return Err(VoxForgeError::Engine(format!(
                "Piper HTTP sidecar returned status {status}: {err_text}"
            )));
        }

        let bytes = res.bytes().await.map_err(|e| {
            VoxForgeError::Engine(format!("Failed to read sidecar audio response: {e}"))
        })?;

        // Decode WAV response
        let (pcm_data, sample_rate, channels) = WavEncoder::decode_wav_to_pcm16(&bytes)?;

        Ok(AudioChunk {
            pcm_data,
            sample_rate,
            channels,
            is_final: true,
        })
    }

    /// Synthesize via local CLI subprocess (`piper` binary or `python -m piper`).
    fn synthesize_via_cli(
        &self,
        binary: &str,
        model_path: &Path,
        text: &str,
    ) -> Result<AudioChunk> {
        info!(binary = %binary, model = %model_path.display(), "Executing local Piper subprocess");

        let mut cmd = if let Some(py_exe) = binary.strip_prefix("python-module:") {
            let py = if py_exe == "piper" || py_exe.is_empty() {
                "python"
            } else {
                py_exe
            };
            let mut c = Command::new(py);
            c.arg("-m").arg("piper");
            c
        } else {
            Command::new(binary)
        };

        let mut child = cmd
            .arg("--model")
            .arg(model_path)
            .arg("--output-raw")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                VoxForgeError::Engine(format!("Failed to spawn piper process '{binary}': {e}"))
            })?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes()).map_err(|e| {
                VoxForgeError::Engine(format!("Failed to write text to piper stdin: {e}"))
            })?;
        }

        let output = child
            .wait_with_output()
            .map_err(|e| VoxForgeError::Engine(format!("Piper process failed: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(stderr = %stderr, "Piper execution exited with error");
            return Err(VoxForgeError::Engine(format!(
                "Piper execution failed: {stderr}"
            )));
        }

        let raw_bytes = output.stdout;
        if raw_bytes.len() < 2 {
            return Err(VoxForgeError::Engine(
                "Piper produced empty audio output".to_string(),
            ));
        }

        // Convert raw 16-bit little-endian bytes to Vec<i16>
        let pcm_data: Vec<i16> = raw_bytes
            .chunks_exact(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();

        Ok(AudioChunk {
            pcm_data,
            sample_rate: self.sample_rate,
            channels: 1,
            is_final: true,
        })
    }
}

/// Locate bundled standalone native `piper` binary inside VoxForge data directory (`<data_dir>/bin/piper/`).
pub fn managed_piper_binary_path() -> Option<PathBuf> {
    let mut bases = Vec::new();
    if let Ok(dir) = std::env::var("VOXFORGE_DATA_DIR") {
        let p = PathBuf::from(dir.trim());
        if !p.as_os_str().is_empty() {
            bases.push(p);
        }
    }
    #[cfg(target_os = "windows")]
    if let Ok(appdata) = std::env::var("APPDATA") {
        bases.push(PathBuf::from(appdata).join("VoxForge"));
    }
    if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        bases.push(PathBuf::from(home).join(".voxforge"));
    }

    let exe_name = if cfg!(windows) { "piper.exe" } else { "piper" };
    for base in bases {
        for cand in [
            base.join("bin").join("piper").join(exe_name),
            base.join("bin").join(exe_name),
        ] {
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    None
}

fn which_piper() -> Option<String> {
    if let Some(managed) = managed_piper_binary_path() {
        return Some(managed.to_string_lossy().to_string());
    }

    if Command::new("piper")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return Some("piper".to_string());
    }

    let cmd = if cfg!(windows) { "where" } else { "which" };
    let ok = Command::new(cmd)
        .arg("piper")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok {
        Some("piper".to_string())
    } else {
        None
    }
}

fn resolve_python_executable() -> String {
    if let Ok(managed) = std::env::var("VOXFORGE_MANAGED_PYTHON") {
        let trimmed = managed.trim();
        if !trimmed.is_empty() && Path::new(trimmed).exists() {
            return trimmed.to_string();
        }
    }
    if cfg!(windows) {
        "python".to_string()
    } else {
        "python3".to_string()
    }
}

fn which_python_piper() -> Option<String> {
    let py = resolve_python_executable();
    let ok = Command::new(&py)
        .args(["-m", "piper", "--help"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok {
        Some(py)
    } else {
        None
    }
}

pub fn resolve_runtime_piper_binary() -> Option<String> {
    if let Ok(path) = std::env::var("VOXFORGE_PIPER_PATH") {
        if !path.trim().is_empty() {
            return Some(path);
        }
    }
    if let Some(bin) = which_piper() {
        Some(bin)
    } else {
        which_python_piper().map(|py| format!("python-module:{py}"))
    }
}

#[async_trait]
impl TtsEngine for PiperTtsEngine {
    fn id(&self) -> &'static str {
        "piper-tts"
    }

    fn name(&self) -> &'static str {
        "Piper Neural TTS (ONNX)"
    }

    fn is_local(&self) -> bool {
        true
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            cost_per_1k_chars: 0.0,
            avg_latency_ms: 60,
            quality_score: 0.88,
            languages: vec![
                "en-US".to_string(),
                "en-GB".to_string(),
                "es-ES".to_string(),
                "fr-FR".to_string(),
                "de-DE".to_string(),
            ],
            is_local: true,
            supports_cloning: false,
            supports_streaming: true,
        }
    }

    async fn voices(&self) -> Result<Vec<Voice>> {
        Ok(vec![
            Voice {
                id: "en_US-lessac-medium".to_string(),
                name: "Lessac (US Medium Neural)".to_string(),
                engine_id: self.id().to_string(),
                language: "en-US".to_string(),
                gender: Gender::Female,
                sample_rate_hz: self.sample_rate,
                tags: vec![
                    "neural".to_string(),
                    "onnx".to_string(),
                    "piper".to_string(),
                    "offline".to_string(),
                ],
                description: Some(
                    "Clear natural English narration voice powered by Piper ONNX".to_string(),
                ),
            },
            Voice {
                id: "en_US-amy-medium".to_string(),
                name: "Amy (US Medium Neural)".to_string(),
                engine_id: self.id().to_string(),
                language: "en-US".to_string(),
                gender: Gender::Female,
                sample_rate_hz: self.sample_rate,
                tags: vec![
                    "neural".to_string(),
                    "onnx".to_string(),
                    "piper".to_string(),
                ],
                description: Some("Warm conversational US female voice".to_string()),
            },
            Voice {
                id: "en_US-danny-low".to_string(),
                name: "Danny (US Low Latency)".to_string(),
                engine_id: self.id().to_string(),
                language: "en-US".to_string(),
                gender: Gender::Male,
                sample_rate_hz: self.sample_rate,
                tags: vec![
                    "neural".to_string(),
                    "onnx".to_string(),
                    "piper".to_string(),
                    "fast".to_string(),
                ],
                description: Some(
                    "Ultra-low latency male voice for real-time applications".to_string(),
                ),
            },
            Voice {
                id: "en_GB-alan-medium".to_string(),
                name: "Alan (British English Neural)".to_string(),
                engine_id: self.id().to_string(),
                language: "en-GB".to_string(),
                gender: Gender::Male,
                sample_rate_hz: self.sample_rate,
                tags: vec![
                    "neural".to_string(),
                    "onnx".to_string(),
                    "piper".to_string(),
                ],
                description: Some("British English received pronunciation voice".to_string()),
            },
            Voice {
                id: "es_ES-carlfm-x_low".to_string(),
                name: "Carlfm (Spanish Neural)".to_string(),
                engine_id: self.id().to_string(),
                language: "es-ES".to_string(),
                gender: Gender::Male,
                sample_rate_hz: self.sample_rate,
                tags: vec![
                    "neural".to_string(),
                    "onnx".to_string(),
                    "piper".to_string(),
                ],
                description: Some("European Spanish natural voice".to_string()),
            },
            Voice {
                id: "fr_FR-siwis-medium".to_string(),
                name: "Siwis (French Neural)".to_string(),
                engine_id: self.id().to_string(),
                language: "fr-FR".to_string(),
                gender: Gender::Female,
                sample_rate_hz: self.sample_rate,
                tags: vec![
                    "neural".to_string(),
                    "onnx".to_string(),
                    "piper".to_string(),
                ],
                description: Some("French expressive neural voice".to_string()),
            },
            Voice {
                id: "de_DE-thorsten-medium".to_string(),
                name: "Thorsten (German Neural)".to_string(),
                engine_id: self.id().to_string(),
                language: "de-DE".to_string(),
                gender: Gender::Male,
                sample_rate_hz: self.sample_rate,
                tags: vec![
                    "neural".to_string(),
                    "onnx".to_string(),
                    "piper".to_string(),
                ],
                description: Some("German standard conversational voice".to_string()),
            },
        ])
    }

    async fn synthesize(&self, req: &SynthesisRequest) -> Result<AudioChunk> {
        if let Some(ref url) = self.sidecar_url {
            match self.synthesize_via_http(url, req).await {
                Ok(chunk) => return Ok(chunk),
                Err(e) => {
                    warn!(error = %e, "Piper HTTP sidecar failed, trying local CLI or fallback");
                }
            }
        }

        let resolved_bin = self
            .piper_path
            .clone()
            .or_else(resolve_runtime_piper_binary);
        if let Some(ref bin) = resolved_bin {
            if let Some(model_path) = self.resolve_model_path(&req.voice_id) {
                match self.synthesize_via_cli(bin, &model_path, &req.text) {
                    Ok(chunk) => return Ok(chunk),
                    Err(e) => {
                        warn!(error = %e, "Piper CLI invocation failed");
                        return Err(e);
                    }
                }
            }
            let catalog_id = Self::catalog_id_for_voice(&req.voice_id);
            return Err(VoxForgeError::Engine(format!(
                "Piper model weights for '{}' are not installed locally. Download '{}' in Model Catalog.",
                req.voice_id, catalog_id
            )));
        }

        let has_model = self.resolve_model_path(&req.voice_id).is_some();
        let catalog_id = Self::catalog_id_for_voice(&req.voice_id);
        if has_model {
            Err(VoxForgeError::Engine(format!(
                "Piper runtime ('piper' CLI) is not installed locally for '{}'. Install Native Piper from Model Catalog / Hardware & Engines or start VOXFORGE_PIPER_URL.",
                req.voice_id
            )))
        } else {
            Err(VoxForgeError::Engine(format!(
                "Piper runtime and model weights for '{}' are not installed locally. Install Native Piper and download '{}' in Model Catalog.",
                req.voice_id, catalog_id
            )))
        }
    }

    async fn synthesize_stream(
        &self,
        request: &SynthesisRequest,
    ) -> Result<mpsc::Receiver<Result<AudioChunk>>> {
        let (tx, rx) = mpsc::channel(4);
        let full_chunk = self.synthesize(request).await?;

        tokio::spawn(async move {
            let chunk_size = 2205; // 100ms chunks at 22.05kHz
            let mut offset = 0;
            let total = full_chunk.pcm_data.len();

            while offset < total {
                let end = (offset + chunk_size).min(total);
                let is_final = end >= total;
                let slice = full_chunk.pcm_data[offset..end].to_vec();

                let chunk = AudioChunk {
                    sample_rate: full_chunk.sample_rate,
                    channels: full_chunk.channels,
                    pcm_data: slice,
                    is_final,
                };

                if tx.send(Ok(chunk)).await.is_err() {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_piper_engine_voices_and_capabilities() {
        let engine = PiperTtsEngine::new();
        assert_eq!(engine.id(), "piper-tts");
        assert!(engine.is_local());

        let caps = engine.capabilities();
        assert_eq!(caps.cost_per_1k_chars, 0.0);
        assert!(caps.quality_score >= 0.8);
        assert!(!caps.languages.is_empty());

        let voices = engine.voices().await.unwrap();
        assert!(voices.len() >= 5);
        assert!(voices.iter().any(|v| v.id == "en_US-lessac-medium"));
    }

    #[tokio::test]
    async fn test_piper_missing_model_error() {
        let engine = PiperTtsEngine::new();
        let req = SynthesisRequest {
            text: "Hello world from Piper ONNX engine.".to_string(),
            voice_id: "en_US-lessac-medium".to_string(),
            speed: 1.0,
            pitch: 0.0,
            format: voxforge_core::models::AudioContainerFormat::Wav,
        };

        let res = engine.synthesize(&req).await;
        assert!(
            res.is_err(),
            "Must return error when model is not installed"
        );
        let err_msg = res.unwrap_err().to_string();
        assert!(err_msg.contains("not installed locally"));
        assert!(err_msg.contains("piper-en-lessac-medium"));
    }

    #[test]
    fn test_piper_builder_methods() {
        let engine = PiperTtsEngine::new()
            .with_sidecar_url("http://127.0.0.1:8888")
            .with_piper_path("/usr/local/bin/piper")
            .with_models_dir(PathBuf::from("/opt/piper/models"));

        assert_eq!(engine.sidecar_url.as_deref(), Some("http://127.0.0.1:8888"));
        assert_eq!(engine.piper_path.as_deref(), Some("/usr/local/bin/piper"));
        assert_eq!(engine.models_dir, PathBuf::from("/opt/piper/models"));
    }
}
