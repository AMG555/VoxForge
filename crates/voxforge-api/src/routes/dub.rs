//! POST /v1/audio/dub — multi-speaker script dubbing.
//!
//! Takes a script with timed cue entries (speaker + text + optional start/end)
//! and synthesizes each line with the assigned voice, returning a single
//! mixed WAV file whose total duration covers all cues.
//!
//! Ships multi-speaker video/audio dialogue dubbing as a first-class API operation.

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
    /// Optional explicit output language / dialect for cross-lingual synthesis & number normalization.
    #[serde(default)]
    pub language: Option<String>,
    /// Optional natural-language delivery instruction.
    #[serde(default)]
    pub instruct: Option<String>,
    /// Optional deterministic seed.
    #[serde(default)]
    pub seed: Option<u64>,
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
    /// Optional bit depth precision (16 = standard PCM, 24 = studio master, 32 = 32-bit float).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub precision: Option<u16>,
    /// Optional studio mastering pass (EQ + 3:1 broadcast compressor + de-esser + peak safety).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mastering: Option<bool>,
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
                    detail: format!("Script line {} exceeds 10,000 character limit", i),
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
                    let eff_lang = line
                        .language
                        .as_deref()
                        .filter(|s| !s.trim().is_empty() && !s.eq_ignore_ascii_case("AUTO"))
                        .map(voxforge_engine::qwen3::normalize_language_code);
                    let norm_lang = eff_lang.as_deref().or(Some(profile.language.as_str()));
                    let req = voxforge_core::models::ClonedSynthesisRequest {
                        text: state
                            .pronunciation
                            .process_with_language(&line.text, norm_lang),
                        profile: profile.clone(),
                        speed,
                        pitch,
                        format,
                        language: eff_lang,
                        instruct: line.instruct.clone(),
                        num_step: None,
                        seed: line.seed,
                        precision: body.precision,
                    };
                    eng.synthesize_cloned_hq(&req)
                        .await
                        .map(|(c, _)| c.pcm_data)
                        .unwrap_or_default()
                } else {
                    synthesize_via_registry(
                        &state,
                        &line.speaker,
                        &line.text,
                        line.language.as_deref(),
                        speed,
                        pitch,
                        format,
                    )
                    .await?
                }
            } else {
                synthesize_via_registry(
                    &state,
                    &line.speaker,
                    &line.text,
                    line.language.as_deref(),
                    speed,
                    pitch,
                    format,
                )
                .await?
            }
        } else {
            synthesize_via_registry(
                &state,
                &line.speaker,
                &line.text,
                line.language.as_deref(),
                speed,
                pitch,
                format,
            )
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

    // Mix into a 32-bit float buffer to eliminate clipping and saturation distortion
    // when multiple speakers overlap or cue timestamps coincide.
    let mut mixed_f32: Vec<f32> = vec![0.0f32; total_samples];
    for seg in &segments {
        for (i, &sample) in seg.pcm.iter().enumerate() {
            let idx = seg.start_sample + i;
            if idx < mixed_f32.len() {
                mixed_f32[idx] += sample as f32;
            }
        }
    }

    // Peak-normalize headroom if cumulative amplitude exceeds 32000.0
    let max_amp = mixed_f32.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    let scale = if max_amp > 32000.0 {
        32000.0 / max_amp
    } else {
        1.0
    };
    let mut mixed: Vec<i16> = mixed_f32
        .iter()
        .map(|&s| (s * scale).clamp(-32767.0, 32767.0) as i16)
        .collect();

    // Studio look-ahead brickwall limiter guarantees -0.5 dBFS true-peak safety
    voxforge_audio::BrickwallLimiter::process_sr(&mut mixed, -0.5, sample_rate);

    // Optional mastering profile pass (Broadcast EQ, 3:1 compressor, de-esser)
    if body.mastering.unwrap_or(false) {
        voxforge_audio::MasteringProfile::Broadcast.apply(&mut mixed, sample_rate);
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
    let precision = body.precision.unwrap_or(16);

    // Encode to WAV / PCM and base64 with requested bit depth precision (16/24/32-bit).
    let audio_bytes = match format {
        AudioContainerFormat::Pcm => mixed
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect::<Vec<u8>>(),
        _ => WavEncoder::encode_pcm_to_wav_with_precision(&mixed, sample_rate, 1, precision)
            .map_err(|e| {
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

    let audio_base64 = crate::routes::cloning::base64_encode(&audio_bytes);

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
    language: Option<&str>,
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

    let eff_lang = language
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("AUTO"))
        .or(Some(voice.language.as_str()));
    let req = SynthesisRequest {
        text: state.pronunciation.process_with_language(text, eff_lang),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_encode_roundtrip() {
        let data = b"Hello, VoxForge dubbing!";
        let encoded = crate::routes::cloning::base64_encode(data);
        // Verify it is valid base64 by checking length and characters.
        assert!(!encoded.is_empty());
        assert!(encoded
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '='));
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

    #[test]
    fn test_dub_request_precision_and_mastering() {
        let json = r#"{"script":[{"speaker":"bob","text":"Testing mastering and 24-bit audio."}],"precision":24,"mastering":true}"#;
        let req: DubRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.precision, Some(24));
        assert_eq!(req.mastering, Some(true));
        assert_eq!(req.script.len(), 1);
    }
}
