use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::models::{AudioContainerFormat, Gender};

/// Persistent voice profile with reference audio and optional acoustic embeddings
/// used for zero-shot voice cloning across supported engines.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VoiceProfile {
    pub id: String,
    pub name: String,
    pub engine_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gender: Option<Gender>,
    /// Relative or absolute path to reference audio sample file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_audio_path: Option<String>,
    /// Base64-encoded reference audio bytes (for API transport).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_audio_base64: Option<String>,
    /// Exact text transcription of the reference audio clip (greatly improves cloning fidelity).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_transcript: Option<String>,
    /// Pre-extracted acoustic speaker embedding vector.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    /// Supported cloning capabilities for this profile (e.g. "cross-lingual", "emotion-transfer").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clone_capabilities: Option<Vec<String>>,
    /// Cosine similarity score (0.0–1.0) of this embedding against its own reference audio.
    /// Populated after multi-sample aggregation; higher scores indicate better cloning fidelity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub similarity_score: Option<f32>,
    /// Number of reference audio samples averaged into this embedding.
    #[serde(default)]
    pub sample_count: u32,
    /// Monotonically increasing version counter, incremented on each embedding refinement.
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
    /// Timestamp of the most recent embedding refinement (None if never refined after creation).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refined_at: Option<DateTime<Utc>>,
}

fn default_language() -> String {
    "en-US".to_string()
}

/// Request payload to clone a voice from reference audio.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloneVoiceRequest {
    pub name: String,
    pub engine_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_audio_base64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_audio_path: Option<String>,
    /// Optional exact transcript of the reference audio for conditioning.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_transcript: Option<String>,
    /// Additional reference audio samples (base64-encoded). When provided, their
    /// acoustic embeddings are averaged with the primary reference to produce a
    /// more stable, noise-robust speaker identity vector.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_reference_audio_base64: Vec<String>,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gender: Option<Gender>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Request payload to synthesize speech using a cloned voice profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClonedSynthesisRequest {
    pub text: String,
    pub profile: VoiceProfile,
    #[serde(default = "default_speed")]
    pub speed: f32,
    #[serde(default)]
    pub pitch: f32,
    #[serde(default)]
    pub format: AudioContainerFormat,
}

fn default_speed() -> f32 {
    1.0
}
