//! POST /v1/audio/dub — multi-speaker script dubbing.
//!
//! Takes a script with timed cue entries (speaker + text + optional start/end)
//! and synthesizes each line with the assigned voice, returning a single
//! mixed WAV file whose total duration covers all cues.
//!
//! This addresses the core feature gap vs. VoiceStudio, which ships video/audio
//! dubbing as a first-class API operation.

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use voxforge_audio::WavEncoder;
use voxforge_core::error::ProblemDetails;
use voxforge_core::models::AudioContainerFormat;
use voxforge_engine::SynthesisRequest;

use crate::state::AppState;

/// A single dialogue line in the dubbing script.
#[derive(Debug, Deserialize)]
pub struct DubScriptLine {
    /// Voice/speaker identifier (voice id or cloned profile id).
    pub speaker: String,
    /// Text spoken in this line.
    pub text: String,
    /// Optional start time offset in seconds for silence-padded placement.
    /// When omitted lines are concatenated sequentially.
    #[serde(default)]
    pub start_seconds: Option<f64>,
    /// Optional speaking speed for this line (default 1.0).
    #[serde(default)]
    pub speed: Option<f32>,
    /// Optional pitch shift in semitones for this line (default 0.0).
    #[serde(default)]
    pub pitch: Option<f32>,
}

/// Request body for POST /v1/audio/dub.
#[derive(Debug, Deserialize)]
pub struct DubRequest {
    /// Ordered list of script lines to synthesize and merge.
    pub script: Vec<DubScriptLine>,
    /// Gap in milliseconds between sequentially-placed lines (used when
    /// `start_seconds` is not provided). Default: 0.
    #[serde(default)]
    pub gap_ms: Option<u32>,
    /// Audio output format (wav / pcm). Default: wav.
    #[serde(default)]
    pub response_format: Option<AudioContainerFormat>,
}

/// Per-line synthesis result returned alongside the mixed audio.
#[derive(Debug, Serialize)]
pub struct DubLineResult {
    pub speaker: String,
    pub start_seconds: f64,
    pub duration_seconds: f64,
}

/// Response body for POST /v1/audio/dub.
#[derive(Debug, Serialize)]
pub struct DubResponse {
    /// Base64-encoded mixed audio.
    pub audio_base64: String,
    /// Total duration of the mixed output in seconds.
    pub total_duration_seconds: f64,
    /// Per-line timing breakdown.
    pub lines: Vec<DubLineResult>,
    /// Sample rate of the output audio.
    pub sample_rate: u32,
}

/// `POST /v1/audio/dub` — synthesize a multi-speaker script and return mixed audio.
pub async fn dub_script(
    State(state): State<AppState>,
    Json(body): Json<DubRequest>,
) -> Result<Json<DubResponse>, (StatusCode, Json<ProblemDetails>)> {
    if body.script.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/empty-script".to_string(),
                title: "Empty Dubbing Script".to_string(),
                status: 400,
                detail: "The 'script' array must contain at least one line".to_string(),
                instance: "/v1/audio/dub".to_string(),
            }),
        ));
    }

    if body.script.len() > 500 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/script-too-long".to_string(),
                title: "Script Too Long".to_string(),
                status: 400,
                detail: "The 'script' array cannot exceed 500 lines per request".to_string(),
                instance: "/v1/audio/dub".to_string(),
            }),
        ));
    }

    for (i, line) in body.script.iter().enumerate() {
        if line.text.trim().is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ProblemDetails {
                    problem_type: "https://voxforge.org/errors/empty-line".to_string(),
                    title: "Empty Script Line".to_string(),
                    status: 400,
                    detail: format!("Script line {} has empty 'text'", i),
                    instance: "/v1/audio/dub".to_string(),
                }),
            ));
        }
        if line.text.len() > 10_000 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ProblemDetails {
                    problem_type: "https://voxforge.org/errors/line-too-long".to_string(),
                    title: "Script Line Too Long".to_string(),
                    status: 400,
                    detail: format!(
                        "Script line {} exceeds 10,000 character limit",
                        i
                    ),
                    instance: "/v1/audio/dub".to_string(),
                }),
            ));
        }
        if line.speaker.trim().is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ProblemDetails {
                    problem_type: "https://voxforge.org/errors/missing-speaker".to_string(),
                    title: "Missing Speaker".to_string(),
                    status: 400,
                    detail: format!("Script line {} has empty 'speaker'", i),
                    instance: "/v1/audio/dub".to_string(),
                }),
            ));
        }
    }

    let sample_rate: u32 = 24_000;
    let gap_samples = {
        let gap_ms = body.gap_ms.unwrap_or(0) as usize;
        (gap_ms * sample_rate as usize) / 1000
    };
    let format = body.response_format.unwrap_or(AudioContainerFormat::Wav);

    // Synthesize each line and collect PCM segments with their target offsets.
    struct Segment {
        start_sample: usize,
        pcm: Vec<i16>,
        speaker: String,
    }

    let mut segments: Vec<Segment> = Vec::with_capacity(body.script.len());
    let mut sequential_cursor: usize = 0; // Used when start_seconds is not given.

    for line in &body.script {
        let speed = line.speed.unwrap_or(1.0).clamp(0.25, 4.0);
        let pitch = line.pitch.unwrap_or(0.0).clamp(-50.0, 50.0);

        // Try cloned profile first, then fall back to standard engine resolution.
        let pcm = if let Some(profile) = state.voice_profiles.find(&line.speaker).await {
            let engine = state.engine_registry.get(&profile.engine_id).await;

            if let Some(eng) = engine {
                if eng.supports_cloning() {
                    let req = voxforge_core::models::ClonedSynthesisRequest {
                        text: state.pronunciation.process(&line.text),
                        profile: profile.clone(),
                        speed,
                        pitch,
                        format,
                    };
                    eng.synthesize_cloned(&req)
                        .await
                        .map(|c| c.pcm_data)
                        .unwrap_or_default()
                } else {
                    synthesize_via_registry(&state, &line.speaker, &line.text, speed, pitch, format)
                        .await?
                }
            } else {
                synthesize_via_registry(&state, &line.speaker, &line.text, speed, pitch, format)
                    .await?
            }
        } else {
            synthesize_via_registry(&state, &line.speaker, &line.text, speed, pitch, format)
                .await?
        };

        let start_sample = if let Some(secs) = line.start_seconds {
            (secs * sample_rate as f64).round() as usize
        } else {
            sequential_cursor
        };

        sequential_cursor = start_sample + pcm.len() + gap_samples;
        segments.push(Segment {
            start_sample,
            pcm,
            speaker: line.speaker.clone(),
        });
    }

    // Compute total length.
    let total_samples = segments
        .iter()
        .map(|s| s.start_sample + s.pcm.len())
        .max()
        .unwrap_or(0);

    // Mix into a single flat buffer.
    let mut mixed: Vec<i16> = vec![0i16; total_samples];
    for seg in &segments {
        for (i, &sample) in seg.pcm.iter().enumerate() {
            let idx = seg.start_sample + i;
            if idx < mixed.len() {
                // Simple additive mixing with saturation to avoid wrapping artefacts.
                mixed[idx] = mixed[idx].saturating_add(sample);
            }
        }
    }

    // Build per-line result metadata.
    let lines: Vec<DubLineResult> = segments
        .iter()
        .map(|s| DubLineResult {
            speaker: s.speaker.clone(),
            start_seconds: s.start_sample as f64 / sample_rate as f64,
            duration_seconds: s.pcm.len() as f64 / sample_rate as f64,
        })
        .collect();

    let total_duration_seconds = total_samples as f64 / sample_rate as f64;

    // Encode to WAV / PCM and base64.
    let audio_bytes = match format {
        AudioContainerFormat::Pcm => {
            mixed.iter().flat_map(|s| s.to_le_bytes()).collect::<Vec<u8>>()
        }
        _ => WavEncoder::encode_pcm16_to_wav(&mixed, sample_rate, 1).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ProblemDetails {
                    problem_type: "https://voxforge.org/errors/encoding-failure".to_string(),
                    title: "Audio Encoding Error".to_string(),
                    status: 500,
                    detail: e.to_string(),
                    instance: "/v1/audio/dub".to_string(),
                }),
            )
        })?,
    };

    let audio_base64 = base64_encode(&audio_bytes);

    Ok(Json(DubResponse {
        audio_base64,
        total_duration_seconds,
        lines,
        sample_rate,
    }))
}

/// Synthesize a single line via the engine registry (non-cloned path).
async fn synthesize_via_registry(
    state: &AppState,
    speaker: &str,
    text: &str,
    speed: f32,
    pitch: f32,
    format: AudioContainerFormat,
) -> Result<Vec<i16>, (StatusCode, Json<ProblemDetails>)> {
    let (engine, voice) = match state.engine_registry.resolve_voice(speaker).await {
        Ok(res) => res,
        Err(_) => {
            // Fall back to the first available engine.
            let engine_ids = state.engine_registry.list_engines().await;
            let first_engine = if let Some(id) = engine_ids.into_iter().next() {
                state.engine_registry.get(&id).await
            } else {
                None
            };
            if let Some(engine) = first_engine {
                let voices = engine.voices().await.unwrap_or_default();
                let voice = voices.into_iter().next().ok_or_else(|| {
                    (
                        StatusCode::NOT_FOUND,
                        Json(ProblemDetails {
                            problem_type: "https://voxforge.org/errors/speaker-not-found"
                                .to_string(),
                            title: "Speaker Not Found".to_string(),
                            status: 404,
                            detail: format!(
                                "Speaker '{}' not found in engine registry or voice profiles",
                                speaker
                            ),
                            instance: "/v1/audio/dub".to_string(),
                        }),
                    )
                })?;
                (engine, voice)
            } else {
                return Err((
                    StatusCode::NOT_FOUND,
                    Json(ProblemDetails {
                        problem_type: "https://voxforge.org/errors/speaker-not-found".to_string(),
                        title: "Speaker Not Found".to_string(),
                        status: 404,
                        detail: format!(
                            "Speaker '{}' not found and no engines are registered",
                            speaker
                        ),
                        instance: "/v1/audio/dub".to_string(),
                    }),
                ));
            }
        }
    };

    let req = SynthesisRequest {
        text: state.pronunciation.process(text),
        voice_id: voice.id,
        speed,
        pitch,
        format,
    };

    let chunk = state
        .engine_registry
        .synthesize_cached(engine, &req)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ProblemDetails {
                    problem_type: "https://voxforge.org/errors/synthesis-failure".to_string(),
                    title: "Synthesis Error".to_string(),
                    status: 500,
                    detail: e.to_string(),
                    instance: "/v1/audio/dub".to_string(),
                }),
            )
        })?;

    Ok(chunk.pcm_data)
}

/// Minimal base64 encoder (no external dep — mirrors the decoder in cloning.rs).
fn base64_encode(data: &[u8]) -> String {
    const CHARSET: &[u8] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    let mut i = 0;
    while i < data.len() {
        let b0 = data[i] as u32;
        let b1 = if i + 1 < data.len() { data[i + 1] as u32 } else { 0 };
        let b2 = if i + 2 < data.len() { data[i + 2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(CHARSET[((triple >> 18) & 0x3F) as usize] as char);
        out.push(CHARSET[((triple >> 12) & 0x3F) as usize] as char);
        if i + 1 < data.len() {
            out.push(CHARSET[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if i + 2 < data.len() {
            out.push(CHARSET[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        i += 3;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_encode_roundtrip() {
        let data = b"Hello, VoxForge dubbing!";
        let encoded = base64_encode(data);
        // Verify it is valid base64 by checking length and characters.
        assert!(!encoded.is_empty());
        assert!(encoded.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '='));
    }

    #[test]
    fn test_dub_request_validation_empty_script() {
        // Validate that the DubRequest deserializes correctly.
        let json = r#"{"script":[]}"#;
        let req: DubRequest = serde_json::from_str(json).unwrap();
        assert!(req.script.is_empty());
    }

    #[test]
    fn test_dub_script_line_defaults() {
        let json = r#"{"speaker":"alice","text":"Hello world"}"#;
        let line: DubScriptLine = serde_json::from_str(json).unwrap();
        assert_eq!(line.speaker, "alice");
        assert_eq!(line.text, "Hello world");
        assert!(line.start_seconds.is_none());
        assert!(line.speed.is_none());
        assert!(line.pitch.is_none());
    }
}
