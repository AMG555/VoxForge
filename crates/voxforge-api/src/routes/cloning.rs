//! REST endpoints for voice cloning and voice profile management.
//!
//! | Method | Path                         | Description                             |
//! |--------|------------------------------|-----------------------------------------|
//! | GET    | `/v1/voices/profiles`        | List all persistent voice profiles      |
//! | POST   | `/v1/voices/clone`           | Clone a voice from reference audio      |
//! | POST   | `/v1/voices/profiles/{id}/refine` | Refine an existing voice profile   |
//! | GET    | `/v1/voices/profiles/{id}`   | Retrieve a specific voice profile       |
//! | DELETE | `/v1/voices/profiles/{id}`   | Delete a voice profile                  |

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use voxforge_audio::AudioAnalyzer;
use voxforge_core::error::ProblemDetails;
use voxforge_core::models::{CloneVoiceRequest, VoiceProfile};

use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct VoiceProfilesListResponse {
    pub count: usize,
    pub profiles: Vec<VoiceProfile>,
}

/// `GET /v1/voices/profiles` — list all stored voice profiles.
pub async fn list_profiles(State(state): State<AppState>) -> Json<VoiceProfilesListResponse> {
    let profiles = state.voice_profiles.list().await;
    Json(VoiceProfilesListResponse {
        count: profiles.len(),
        profiles,
    })
}

/// `GET /v1/voices/profiles/{id}` — retrieve a specific voice profile.
pub async fn get_profile(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<VoiceProfile>, (StatusCode, Json<ProblemDetails>)> {
    match state.voice_profiles.get(&id).await {
        Some(profile) => Ok(Json(profile)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/profile-not-found".to_string(),
                title: "Voice Profile Not Found".to_string(),
                status: 404,
                detail: format!("Voice profile '{id}' not found"),
                instance: format!("/v1/voices/profiles/{id}"),
            }),
        )),
    }
}

/// `POST /v1/voices/clone` — clone a voice from reference audio.
pub async fn clone_voice(
    State(state): State<AppState>,
    Json(body): Json<CloneVoiceRequest>,
) -> Result<(StatusCode, Json<VoiceProfile>), (StatusCode, Json<ProblemDetails>)> {
    if body.name.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/invalid-parameter".to_string(),
                title: "Invalid Voice Name".to_string(),
                status: 400,
                detail: "The 'name' field cannot be empty".to_string(),
                instance: "/v1/voices/clone".to_string(),
            }),
        ));
    }

    if body.name.len() > 128 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/invalid-parameter".to_string(),
                title: "Voice Name Too Long".to_string(),
                status: 400,
                detail: "The 'name' field cannot exceed 128 characters".to_string(),
                instance: "/v1/voices/clone".to_string(),
            }),
        ));
    }

    if let Some(ref b64) = body.reference_audio_base64 {
        if b64.len() > 35 * 1024 * 1024 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ProblemDetails {
                    problem_type: "https://voxforge.org/errors/payload-too-large".to_string(),
                    title: "Audio Payload Too Large".to_string(),
                    status: 400,
                    detail: "Reference audio payload exceeds maximum permitted size (35MB)"
                        .to_string(),
                    instance: "/v1/voices/clone".to_string(),
                }),
            ));
        }
    }

    if let Some(ref path) = body.reference_audio_path {
        let trimmed = path.trim();
        let is_unc_or_device = trimmed.starts_with(r"\\")
            || trimmed.starts_with("//")
            || trimmed.starts_with(r"\??\")
            || trimmed.starts_with(r"\\.\");
        let p = std::path::Path::new(trimmed);
        let has_traversal = is_unc_or_device
            || trimmed.is_empty()
            || trimmed.contains('\0')
            || p.components().any(|c| match c {
                std::path::Component::ParentDir => true,
                std::path::Component::Prefix(prefix) => {
                    use std::path::Prefix;
                    matches!(prefix.kind(), Prefix::UNC(..) | Prefix::DeviceNS(..))
                }
                _ => false,
            });
        let is_audio = p
            .extension()
            .and_then(|e| e.to_str())
            .map(|ext| {
                matches!(
                    ext.to_lowercase().as_str(),
                    "wav" | "mp3" | "ogg" | "flac" | "m4a"
                )
            })
            .unwrap_or(false);

        if has_traversal || !is_audio {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ProblemDetails {
                    problem_type: "https://voxforge.org/errors/invalid-path".to_string(),
                    title: "Invalid Reference Audio Path".to_string(),
                    status: 400,
                    detail: "Path traversal, UNC network paths, and non-audio extensions are strictly prohibited".to_string(),
                    instance: "/v1/voices/clone".to_string(),
                }),
            ));
        }
    }

    if body.reference_audio_base64.is_none() && body.reference_audio_path.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/missing-audio".to_string(),
                title: "Missing Reference Audio".to_string(),
                status: 400,
                detail: "Either 'reference_audio_base64' or 'reference_audio_path' is required"
                    .to_string(),
                instance: "/v1/voices/clone".to_string(),
            }),
        ));
    }

    // Resolve the target cloning engine
    let engine = state
        .engine_registry
        .get(&body.engine_id)
        .await
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ProblemDetails {
                    problem_type: "https://voxforge.org/errors/engine-not-found".to_string(),
                    title: "Engine Not Found".to_string(),
                    status: 404,
                    detail: format!("Engine '{}' not found in registry", body.engine_id),
                    instance: "/v1/voices/clone".to_string(),
                }),
            )
        })?;

    if !engine.supports_cloning() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/cloning-unsupported".to_string(),
                title: "Cloning Unsupported".to_string(),
                status: 400,
                detail: format!(
                    "Engine '{}' does not support zero-shot voice cloning",
                    body.engine_id
                ),
                instance: "/v1/voices/clone".to_string(),
            }),
        ));
    }

    let profile = engine.clone_voice(&body).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/cloning-failed".to_string(),
                title: "Voice Cloning Failed".to_string(),
                status: 500,
                detail: e.to_string(),
                instance: "/v1/voices/clone".to_string(),
            }),
        )
    })?;

    state.voice_profiles.insert(profile.clone()).await;

    Ok((StatusCode::CREATED, Json(profile)))
}

/// `POST /v1/voices/profiles/{id}/refine` — add a new reference sample to an existing voice profile,
/// updating the speaker embedding centroid and incrementing the profile version.
#[derive(Debug, Deserialize)]
pub struct RefineVoiceRequest {
    /// Base64-encoded additional reference audio sample.
    pub audio_base64: String,
    /// Optional transcript of the additional sample.
    pub transcript: Option<String>,
}

pub async fn refine_voice_profile(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<RefineVoiceRequest>,
) -> Result<Json<VoiceProfile>, (StatusCode, Json<ProblemDetails>)> {
    if body.audio_base64.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/empty-payload".to_string(),
                title: "Missing Audio Payload".to_string(),
                status: 400,
                detail: "The 'audio_base64' field must not be empty".to_string(),
                instance: format!("/v1/voices/profiles/{id}/refine"),
            }),
        ));
    }

    if body.audio_base64.len() > 35 * 1024 * 1024 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/payload-too-large".to_string(),
                title: "Audio Payload Too Large".to_string(),
                status: 400,
                detail: "Refinement audio payload exceeds maximum permitted size (35MB)"
                    .to_string(),
                instance: format!("/v1/voices/profiles/{id}/refine"),
            }),
        ));
    }

    let profile = state.voice_profiles.get(&id).await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/profile-not-found".to_string(),
                title: "Voice Profile Not Found".to_string(),
                status: 404,
                detail: format!("Voice profile '{id}' not found"),
                instance: format!("/v1/voices/profiles/{id}/refine"),
            }),
        )
    })?;

    let engine = state
        .engine_registry
        .get(&profile.engine_id)
        .await
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ProblemDetails {
                    problem_type: "https://voxforge.org/errors/engine-not-found".to_string(),
                    title: "Engine Not Found".to_string(),
                    status: 400,
                    detail: format!("Engine '{}' not registered", profile.engine_id),
                    instance: format!("/v1/voices/profiles/{id}/refine"),
                }),
            )
        })?;

    let refined = engine
        .refine_voice_embedding(&profile, &body.audio_base64)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ProblemDetails {
                    problem_type: "https://voxforge.org/errors/refinement-failed".to_string(),
                    title: "Voice Refinement Failed".to_string(),
                    status: 500,
                    detail: e.to_string(),
                    instance: format!("/v1/voices/profiles/{id}/refine"),
                }),
            )
        })?;

    state.voice_profiles.insert(refined.clone()).await;

    Ok(Json(refined))
}

/// `DELETE /v1/voices/profiles/{id}` — remove a stored voice profile.
pub async fn delete_profile(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ProblemDetails>)> {
    match state.voice_profiles.remove(&id).await {
        Some(_) => Ok(StatusCode::NO_CONTENT),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/profile-not-found".to_string(),
                title: "Voice Profile Not Found".to_string(),
                status: 404,
                detail: format!("Voice profile '{id}' not found"),
                instance: format!("/v1/voices/profiles/{id}"),
            }),
        )),
    }
}

#[derive(Debug, Deserialize)]
pub struct AssessAudioRequest {
    pub audio_base64: String,
}

#[derive(Debug, Serialize)]
pub struct VoiceAudioAssessment {
    pub duration_seconds: f64,
    pub sample_rate: u32,
    pub channels: u16,
    pub peak_dbfs: f32,
    pub rms_dbfs: f32,
    pub snr_estimate_db: f32,
    pub clipping_detected: bool,
    pub is_silent: bool,
    pub clarity_rating: &'static str,
    pub recommendations: Vec<String>,
}

fn decode_b64_to_pcm(input: &str) -> Result<(Vec<i16>, u32, u16), String> {
    let clean = if let Some(idx) = input.find(',') {
        input[idx + 1..].trim()
    } else {
        input.trim()
    };

    let mut table = [0xFFu8; 256];
    for (i, &b) in b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
        .iter()
        .enumerate()
    {
        table[b as usize] = i as u8;
    }
    table[b'-' as usize] = 62;
    table[b'_' as usize] = 63;

    if clean.is_empty() {
        return Err("Empty audio payload".to_string());
    }

    let mut out = Vec::with_capacity(clean.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0;

    for b in clean.bytes() {
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

    if out.len() >= 12 && &out[0..4] == b"RIFF" && &out[8..12] == b"WAVE" {
        if let Ok((pcm, sr, ch)) = voxforge_audio::WavEncoder::decode_wav_to_pcm16(&out) {
            if ch > 1 {
                let channels = ch as usize;
                let mut mono = Vec::with_capacity(pcm.len() / channels);
                for frame in pcm.chunks_exact(channels) {
                    let sum: i32 = frame.iter().map(|&s| s as i32).sum();
                    mono.push((sum / channels as i32) as i16);
                }
                return Ok((mono, sr, 1));
            }
            return Ok((pcm, sr, ch));
        }
    }

    let samples = out
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    Ok((samples, 24000, 1))
}

/// `POST /v1/voices/assess` — evaluate reference audio quality for zero-shot voice cloning.
pub async fn assess_reference_audio(
    Json(body): Json<AssessAudioRequest>,
) -> Result<Json<VoiceAudioAssessment>, (StatusCode, Json<ProblemDetails>)> {
    if body.audio_base64.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/empty-payload".to_string(),
                title: "Missing Audio Payload".to_string(),
                status: 400,
                detail: "The 'audio_base64' field must not be empty".to_string(),
                instance: "/v1/voices/assess".to_string(),
            }),
        ));
    }

    if body.audio_base64.len() > 35 * 1024 * 1024 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/payload-too-large".to_string(),
                title: "Audio Payload Too Large".to_string(),
                status: 400,
                detail: "Reference audio payload exceeds maximum permitted size (35MB)".to_string(),
                instance: "/v1/voices/assess".to_string(),
            }),
        ));
    }

    let (samples, sample_rate, channels) =
        decode_b64_to_pcm(&body.audio_base64).map_err(|detail| {
            (
                StatusCode::BAD_REQUEST,
                Json(ProblemDetails {
                    problem_type: "https://voxforge.org/errors/invalid-audio".to_string(),
                    title: "Invalid Audio Input".to_string(),
                    status: 400,
                    detail,
                    instance: "/v1/voices/assess".to_string(),
                }),
            )
        })?;

    let metrics = AudioAnalyzer::analyze_pcm16(&samples, sample_rate, channels);
    let clipping_detected = metrics.clipping_samples_count > 0;

    let clarity_rating =
        if metrics.snr_estimate_db >= 25.0 && !clipping_detected && metrics.duration_seconds >= 3.0
        {
            "excellent"
        } else if metrics.snr_estimate_db >= 16.0 && metrics.clipping_samples_count < 10 {
            "good"
        } else if metrics.snr_estimate_db >= 10.0 {
            "fair"
        } else {
            "noisy"
        };

    let mut recommendations = Vec::new();
    if metrics.duration_seconds < 3.0 {
        recommendations.push(
            "Audio sample is short (<3s). Provide 5–15 seconds for optimal voice cloning."
                .to_string(),
        );
    } else if metrics.duration_seconds > 60.0 {
        recommendations.push(
            "Audio sample is long (>60s). Trim to a focused 10–20 second clean speech segment."
                .to_string(),
        );
    }

    if clipping_detected {
        recommendations.push(
            "Audio clipping detected. Lower your microphone recording gain to avoid harsh distortion."
                .to_string(),
        );
    }

    if metrics.snr_estimate_db < 15.0 && !metrics.is_silent {
        recommendations.push(
            "High room noise floor detected. Use studio high-pass cut and noise gating."
                .to_string(),
        );
    }

    if metrics.is_silent {
        recommendations.push(
            "Audio is completely silent or muted. Check microphone input permissions.".to_string(),
        );
    }

    if recommendations.is_empty() {
        recommendations.push(
            "Studio-grade reference audio ready for zero-shot neural voice cloning.".to_string(),
        );
    }

    Ok(Json(VoiceAudioAssessment {
        duration_seconds: metrics.duration_seconds,
        sample_rate: metrics.sample_rate,
        channels: metrics.channels,
        peak_dbfs: metrics.peak_dbfs,
        rms_dbfs: metrics.rms_dbfs,
        snr_estimate_db: metrics.snr_estimate_db,
        clipping_detected,
        is_silent: metrics.is_silent,
        clarity_rating,
        recommendations,
    }))
}

#[derive(Debug, Deserialize)]
pub struct VerifyVoiceRequest {
    /// Base64-encoded reference audio clip (or original sample).
    pub reference_audio_base64: Option<String>,
    /// Base64-encoded synthesized or test audio to compare against.
    pub test_audio_base64: String,
    /// Optional stored voice profile ID to compare against.
    pub voice_profile_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct VoiceVerificationResponse {
    pub similarity_score: f32,
    pub is_match: bool,
    pub confidence_rating: &'static str,
    pub reference_f0_hz: f32,
    pub test_f0_hz: f32,
    pub pitch_match_percent: f32,
    pub timbre_match_percent: f32,
    pub formant_match_percent: f32,
    pub cadence_match_percent: f32,
    pub overall_biometric_score: f32,
    pub recommendations: Vec<String>,
}

/// `POST /v1/voices/verify` — evaluate speaker acoustic identity and compute cosine similarity.
pub async fn verify_voice(
    State(state): State<AppState>,
    Json(body): Json<VerifyVoiceRequest>,
) -> Result<Json<VoiceVerificationResponse>, (StatusCode, Json<ProblemDetails>)> {
    if body.test_audio_base64.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/missing-audio".to_string(),
                title: "Missing Test Audio".to_string(),
                status: 400,
                detail: "'test_audio_base64' is required for voice verification".to_string(),
                instance: "/v1/voices/verify".to_string(),
            }),
        ));
    }

    let (test_samples, test_sr, _) =
        decode_b64_to_pcm(&body.test_audio_base64).map_err(|detail| {
            (
                StatusCode::BAD_REQUEST,
                Json(ProblemDetails {
                    problem_type: "https://voxforge.org/errors/invalid-audio".to_string(),
                    title: "Invalid Test Audio".to_string(),
                    status: 400,
                    detail,
                    instance: "/v1/voices/verify".to_string(),
                }),
            )
        })?;

    let test_emb = voxforge_engine::Qwen3TtsEngine::extract_speaker_embedding(&test_samples);
    let test_prosody = voxforge_audio::ProsodyMirror::analyze(&test_samples, test_sr);

    let (ref_emb, ref_f0, ref_f1, ref_f2, ref_pace) = if let Some(ref profile_id) =
        body.voice_profile_id
    {
        let profile_opt = if let Some(p) = state.voice_profiles.get(profile_id).await {
            Some(p)
        } else {
            let list = state.voice_profiles.list().await;
            list.into_iter().find(|p| {
                p.id == *profile_id
                    || p.name.eq_ignore_ascii_case(profile_id)
                    || profile_id.ends_with(&p.id)
                    || p.id.ends_with(profile_id)
            })
        };

        if let Some(profile) = profile_opt {
            let emb = profile.embedding.unwrap_or_else(|| {
                if let Some(ref b64) = profile.reference_audio_base64 {
                    if let Ok((samples, _, _)) = decode_b64_to_pcm(b64) {
                        return voxforge_engine::Qwen3TtsEngine::extract_speaker_embedding(
                            &samples,
                        );
                    }
                } else if let Some(ref path) = profile.reference_audio_path {
                    if let Ok(bytes) = std::fs::read(path) {
                        if let Ok((samples, _)) =
                            voxforge_audio::WavEncoder::decode_wav_to_mono_pcm16(&bytes)
                        {
                            return voxforge_engine::Qwen3TtsEngine::extract_speaker_embedding(
                                &samples,
                            );
                        }
                    }
                }
                vec![0.0; 512]
            });
            let f0 = profile
                .metadata
                .get("f0_mean_hz")
                .and_then(|s| s.parse().ok())
                .unwrap_or(150.0);
            let f1 = profile
                .metadata
                .get("formant_f1_hz")
                .and_then(|s| s.parse().ok())
                .unwrap_or(500.0);
            let f2 = profile
                .metadata
                .get("formant_f2_hz")
                .and_then(|s| s.parse().ok())
                .unwrap_or(1500.0);
            let pace = profile
                .metadata
                .get("syllable_rate")
                .and_then(|s| s.parse().ok())
                .unwrap_or(4.2);
            (emb, f0, f1, f2, pace)
        } else if let Some(ref ref_b64) = body.reference_audio_base64 {
            let (ref_samples, ref_sr, _) = decode_b64_to_pcm(ref_b64).map_err(|detail| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ProblemDetails {
                        problem_type: "https://voxforge.org/errors/invalid-audio".to_string(),
                        title: "Invalid Reference Audio".to_string(),
                        status: 400,
                        detail,
                        instance: "/v1/voices/verify".to_string(),
                    }),
                )
            })?;
            let emb = voxforge_engine::Qwen3TtsEngine::extract_speaker_embedding(&ref_samples);
            let prosody = voxforge_audio::ProsodyMirror::analyze(&ref_samples, ref_sr);
            (
                emb,
                prosody.f0_mean_hz,
                prosody.formant_f1_hz,
                prosody.formant_f2_hz,
                prosody.syllable_rate,
            )
        } else {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ProblemDetails {
                    problem_type: "https://voxforge.org/errors/profile-not-found".to_string(),
                    title: "Profile Not Found".to_string(),
                    status: 404,
                    detail: format!(
                        "Voice profile '{profile_id}' not found and no reference audio provided"
                    ),
                    instance: "/v1/voices/verify".to_string(),
                }),
            ));
        }
    } else if let Some(ref ref_b64) = body.reference_audio_base64 {
        let (ref_samples, ref_sr, _) = decode_b64_to_pcm(ref_b64).map_err(|detail| {
            (
                StatusCode::BAD_REQUEST,
                Json(ProblemDetails {
                    problem_type: "https://voxforge.org/errors/invalid-audio".to_string(),
                    title: "Invalid Reference Audio".to_string(),
                    status: 400,
                    detail,
                    instance: "/v1/voices/verify".to_string(),
                }),
            )
        })?;
        let emb = voxforge_engine::Qwen3TtsEngine::extract_speaker_embedding(&ref_samples);
        let prosody = voxforge_audio::ProsodyMirror::analyze(&ref_samples, ref_sr);
        (
            emb,
            prosody.f0_mean_hz,
            prosody.formant_f1_hz,
            prosody.formant_f2_hz,
            prosody.syllable_rate,
        )
    } else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/missing-reference".to_string(),
                title: "Missing Reference Audio or Profile".to_string(),
                status: 400,
                detail: "Provide either 'reference_audio_base64' or 'voice_profile_id'".to_string(),
                instance: "/v1/voices/verify".to_string(),
            }),
        ));
    };

    let similarity_score =
        voxforge_engine::Qwen3TtsEngine::compute_cosine_similarity(&ref_emb, &test_emb);

    let pitch_delta = (ref_f0 - test_prosody.f0_mean_hz).abs();
    let pitch_match_percent = ((1.0 - (pitch_delta / ref_f0.max(50.0))).clamp(0.0, 1.0)) * 100.0;
    let timbre_match_percent = ((similarity_score.clamp(0.0, 1.0)) * 100.0).round();

    let f1_delta = (ref_f1 - test_prosody.formant_f1_hz).abs();
    let f2_delta = (ref_f2 - test_prosody.formant_f2_hz).abs();
    let f1_match = (1.0 - (f1_delta / ref_f1.max(100.0))).clamp(0.0, 1.0);
    let f2_match = (1.0 - (f2_delta / ref_f2.max(200.0))).clamp(0.0, 1.0);
    let formant_match_percent = ((f1_match * 0.5 + f2_match * 0.5) * 100.0).round();

    let pace_delta = (ref_pace - test_prosody.syllable_rate).abs();
    let cadence_match_percent =
        ((1.0 - (pace_delta / ref_pace.max(1.0))).clamp(0.0, 1.0) * 100.0).round();

    let overall_biometric_score = ((timbre_match_percent * 0.50
        + pitch_match_percent * 0.25
        + formant_match_percent * 0.15
        + cadence_match_percent * 0.10)
        .clamp(0.0, 100.0)
        * 10.0)
        .round()
        / 10.0;

    let is_match = similarity_score >= 0.78 && overall_biometric_score >= 75.0;
    let confidence_rating = if overall_biometric_score >= 88.0 && similarity_score >= 0.85 {
        "high"
    } else if overall_biometric_score >= 75.0 && similarity_score >= 0.72 {
        "medium"
    } else {
        "low"
    };

    let mut recommendations = Vec::new();
    if is_match {
        recommendations.push(
            format!("Speaker biometric identity verified with {overall_biometric_score:.1}% aggregate precision."),
        );
    } else {
        recommendations.push(
            "Acoustic disparity detected between reference identity and generated speech."
                .to_string(),
        );
        if pitch_match_percent < 75.0 {
            recommendations.push(format!(
                "Pitch offset needed: target is {ref_f0:.1}Hz vs synthesized {test_f0:.1}Hz.",
                test_f0 = test_prosody.f0_mean_hz
            ));
        }
        if formant_match_percent < 70.0 {
            recommendations.push(
                "Formant envelope mismatch: adjust vocal tract resonance parameters.".to_string(),
            );
        }
    }

    Ok(Json(VoiceVerificationResponse {
        similarity_score,
        is_match,
        confidence_rating,
        reference_f0_hz: ref_f0,
        test_f0_hz: test_prosody.f0_mean_hz,
        pitch_match_percent,
        timbre_match_percent,
        formant_match_percent,
        cadence_match_percent,
        overall_biometric_score,
        recommendations,
    }))
}

// ---------------------------------------------------------------------------
// VoxPersona export via PersonaBundle
// ---------------------------------------------------------------------------

/// `GET /v1/voices/profiles/{id}/export` — export a stored voice profile as a downloadable
/// `.voxpersona` bundle (JSON envelope wrapping the full VoiceProfile + 512-D embedding).
/// Uses the canonical `PersonaBundle` format from `voxforge-engine`.
pub async fn export_profile_as_bundle(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<voxforge_engine::PersonaBundle>, (StatusCode, Json<ProblemDetails>)> {
    let profile = state.voice_profiles.get(&id).await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/profile-not-found".to_string(),
                title: "Voice Profile Not Found".to_string(),
                status: 404,
                detail: format!("Voice profile '{id}' not found"),
                instance: format!("/v1/voices/profiles/{id}/export"),
            }),
        )
    })?;

    Ok(Json(voxforge_engine::PersonaBundle::from_profile(profile)))
}

/// `POST /v1/voices/profiles/compare` — compute embedding cosine similarity between two profiles.
#[derive(Debug, Deserialize)]
pub struct CompareProfilesRequest {
    pub profile_id_a: String,
    pub profile_id_b: String,
}

#[derive(Debug, Serialize)]
pub struct ProfileComparisonResponse {
    pub profile_id_a: String,
    pub profile_id_b: String,
    pub cosine_similarity: f32,
    pub is_same_speaker: bool,
    pub confidence: &'static str,
}

/// `POST /v1/voices/profiles/compare` — compare two stored voice profiles by embedding cosine similarity.
pub async fn compare_profiles(
    State(state): State<AppState>,
    Json(body): Json<CompareProfilesRequest>,
) -> Result<Json<ProfileComparisonResponse>, (StatusCode, Json<ProblemDetails>)> {
    let profile_a = state
        .voice_profiles
        .get(&body.profile_id_a)
        .await
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ProblemDetails {
                    problem_type: "https://voxforge.org/errors/profile-not-found".to_string(),
                    title: "Profile A Not Found".to_string(),
                    status: 404,
                    detail: format!("Voice profile '{}' not found", body.profile_id_a),
                    instance: "/v1/voices/profiles/compare".to_string(),
                }),
            )
        })?;

    let profile_b = state
        .voice_profiles
        .get(&body.profile_id_b)
        .await
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ProblemDetails {
                    problem_type: "https://voxforge.org/errors/profile-not-found".to_string(),
                    title: "Profile B Not Found".to_string(),
                    status: 404,
                    detail: format!("Voice profile '{}' not found", body.profile_id_b),
                    instance: "/v1/voices/profiles/compare".to_string(),
                }),
            )
        })?;

    let emb_a = profile_a.embedding.as_deref().unwrap_or(&[]);
    let emb_b = profile_b.embedding.as_deref().unwrap_or(&[]);

    if emb_a.is_empty() || emb_b.is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/missing-embedding".to_string(),
                title: "Missing Speaker Embedding".to_string(),
                status: 422,
                detail: "One or both profiles lack a speaker embedding vector".to_string(),
                instance: "/v1/voices/profiles/compare".to_string(),
            }),
        ));
    }

    let cosine_similarity =
        voxforge_engine::Qwen3TtsEngine::compute_cosine_similarity(emb_a, emb_b);
    let is_same_speaker = cosine_similarity >= 0.78;
    let confidence = if cosine_similarity >= 0.90 {
        "high"
    } else if cosine_similarity >= 0.78 {
        "medium"
    } else {
        "low"
    };

    Ok(Json(ProfileComparisonResponse {
        profile_id_a: body.profile_id_a,
        profile_id_b: body.profile_id_b,
        cosine_similarity,
        is_same_speaker,
        confidence,
    }))
}

/// Request payload to blend two voice profiles into a new hybrid profile.
#[derive(Debug, Deserialize)]
pub struct BlendProfilesRequest {
    pub name: String,
    pub profile_id_a: String,
    pub profile_id_b: String,
    /// Blend weight for profile B (0.0 to 1.0).
    /// 0.0 = 100% profile A, 1.0 = 100% profile B, 0.5 = 50/50 balanced blend.
    #[serde(default = "default_blend_weight")]
    pub weight_b: f32,
    #[serde(default)]
    pub description: Option<String>,
}

fn default_blend_weight() -> f32 {
    0.5
}

#[derive(Debug, Serialize)]
pub struct BlendProfilesResponse {
    pub blended_profile: VoiceProfile,
    pub interpolation_weight_b: f32,
    pub cosine_similarity_to_a: f32,
    pub cosine_similarity_to_b: f32,
}

/// `POST /v1/voices/profiles/blend` — interpolate two stored voice profiles into a new hybrid persona.
pub async fn blend_profiles(
    State(state): State<AppState>,
    Json(body): Json<BlendProfilesRequest>,
) -> Result<Json<BlendProfilesResponse>, (StatusCode, Json<ProblemDetails>)> {
    if body.name.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/invalid-parameter".to_string(),
                title: "Missing Voice Name".to_string(),
                status: 400,
                detail: "The 'name' field cannot be empty".to_string(),
                instance: "/v1/voices/profiles/blend".to_string(),
            }),
        ));
    }

    if body.name.len() > 128 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/invalid-parameter".to_string(),
                title: "Voice Name Too Long".to_string(),
                status: 400,
                detail: "The 'name' field cannot exceed 128 characters".to_string(),
                instance: "/v1/voices/profiles/blend".to_string(),
            }),
        ));
    }

    let w_b = if body.weight_b.is_finite() {
        body.weight_b.clamp(0.0, 1.0)
    } else {
        0.5
    };
    let w_a = 1.0 - w_b;

    let profile_a = state
        .voice_profiles
        .get(&body.profile_id_a)
        .await
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ProblemDetails {
                    problem_type: "https://voxforge.org/errors/profile-not-found".to_string(),
                    title: "Profile A Not Found".to_string(),
                    status: 404,
                    detail: format!("Voice profile '{}' not found", body.profile_id_a),
                    instance: "/v1/voices/profiles/blend".to_string(),
                }),
            )
        })?;

    let profile_b = state
        .voice_profiles
        .get(&body.profile_id_b)
        .await
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ProblemDetails {
                    problem_type: "https://voxforge.org/errors/profile-not-found".to_string(),
                    title: "Profile B Not Found".to_string(),
                    status: 404,
                    detail: format!("Voice profile '{}' not found", body.profile_id_b),
                    instance: "/v1/voices/profiles/blend".to_string(),
                }),
            )
        })?;

    let emb_a = profile_a.embedding.as_deref().unwrap_or(&[]);
    let emb_b = profile_b.embedding.as_deref().unwrap_or(&[]);

    if emb_a.is_empty() || emb_b.is_empty() || emb_a.len() != emb_b.len() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/missing-embedding".to_string(),
                title: "Incompatible Speaker Embeddings".to_string(),
                status: 422,
                detail:
                    "Both profiles must contain non-empty, matching-dimension speaker embeddings"
                        .to_string(),
                instance: "/v1/voices/profiles/blend".to_string(),
            }),
        ));
    }

    // Spherical linear / normalized linear interpolation of speaker embeddings
    let mut blended_emb = Vec::with_capacity(emb_a.len());
    let mut sum_sq = 0.0f32;
    for i in 0..emb_a.len() {
        let val = w_a * emb_a[i] + w_b * emb_b[i];
        blended_emb.push(val);
        sum_sq += val * val;
    }
    let norm = sum_sq.sqrt().max(1e-12);
    for val in &mut blended_emb {
        *val /= norm;
    }

    let cosine_similarity_to_a =
        voxforge_engine::Qwen3TtsEngine::compute_cosine_similarity(&blended_emb, emb_a);
    let cosine_similarity_to_b =
        voxforge_engine::Qwen3TtsEngine::compute_cosine_similarity(&blended_emb, emb_b);

    // Interpolate acoustic metadata (F0, formants, syllable rate, jitter, shimmer)
    let mut metadata = HashMap::new();
    metadata.insert("engine".to_string(), "qwen3-tts".to_string());
    metadata.insert("zero_shot".to_string(), "true".to_string());
    metadata.insert("blended".to_string(), "true".to_string());
    metadata.insert(
        "blended_from".to_string(),
        format!("{}:{:.2},{}:{:.2}", profile_a.id, w_a, profile_b.id, w_b),
    );

    let interp_num = |key: &str, def: f32, prec: usize| -> String {
        let va = profile_a
            .metadata
            .get(key)
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(def);
        let vb = profile_b
            .metadata
            .get(key)
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(def);
        format!("{:.*}", prec, w_a * va + w_b * vb)
    };

    metadata.insert("f0_mean_hz".to_string(), interp_num("f0_mean_hz", 150.0, 1));
    metadata.insert(
        "f0_std_semitones".to_string(),
        interp_num("f0_std_semitones", 2.5, 2),
    );
    metadata.insert(
        "formant_f1_hz".to_string(),
        interp_num("formant_f1_hz", 500.0, 1),
    );
    metadata.insert(
        "formant_f2_hz".to_string(),
        interp_num("formant_f2_hz", 1500.0, 1),
    );
    metadata.insert(
        "formant_f3_hz".to_string(),
        interp_num("formant_f3_hz", 2500.0, 1),
    );
    metadata.insert(
        "syllable_rate".to_string(),
        interp_num("syllable_rate", 4.2, 2),
    );
    metadata.insert(
        "spectral_tilt_db".to_string(),
        interp_num("spectral_tilt_db", -6.0, 2),
    );
    metadata.insert(
        "voicing_ratio".to_string(),
        interp_num("voicing_ratio", 0.85, 3),
    );
    metadata.insert("hnr_db".to_string(), interp_num("hnr_db", 18.0, 2));

    let blended_profile = VoiceProfile {
        id: format!("qwen3-blended-{}", uuid::Uuid::new_v4()),
        name: body.name.trim().to_string(),
        engine_id: "qwen3-tts".to_string(),
        language: if w_a >= 0.5 {
            profile_a.language.clone()
        } else {
            profile_b.language.clone()
        },
        gender: if w_a >= 0.5 {
            profile_a.gender
        } else {
            profile_b.gender
        },
        description: body.description.or_else(|| {
            Some(format!(
                "Acoustic blend of '{}' ({:.0}%) and '{}' ({:.0}%)",
                profile_a.name,
                w_a * 100.0,
                profile_b.name,
                w_b * 100.0
            ))
        }),
        reference_audio_path: None,
        reference_audio_base64: profile_a.reference_audio_base64.clone(),
        reference_transcript: None,
        embedding: Some(blended_emb),
        clone_capabilities: Some(vec![
            "cross_lingual".to_string(),
            "zero_shot".to_string(),
            "blended".to_string(),
        ]),
        similarity_score: Some(cosine_similarity_to_a.min(cosine_similarity_to_b)),
        sample_count: (profile_a.sample_count + profile_b.sample_count).max(2),
        version: 1,
        metadata,
        created_at: chrono::Utc::now(),
        refined_at: None,
    };

    state.voice_profiles.insert(blended_profile.clone()).await;

    Ok(Json(BlendProfilesResponse {
        blended_profile,
        interpolation_weight_b: w_b,
        cosine_similarity_to_a,
        cosine_similarity_to_b,
    }))
}

#[derive(Debug, Deserialize)]
pub struct CheckAudioRequest {
    pub audio_base64: String,
}

/// `POST /v1/audio/check` — scan audio for signal anomalies (clipping, silence gaps, volume jumps, DC offset).
pub async fn check_audio_signal(
    Json(body): Json<CheckAudioRequest>,
) -> Result<Json<voxforge_audio::AudioSignalReport>, (StatusCode, Json<ProblemDetails>)> {
    if body.audio_base64.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/empty-payload".to_string(),
                title: "Missing Audio Payload".to_string(),
                status: 400,
                detail: "The 'audio_base64' field must not be empty".to_string(),
                instance: "/v1/audio/check".to_string(),
            }),
        ));
    }

    if body.audio_base64.len() > 35 * 1024 * 1024 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/payload-too-large".to_string(),
                title: "Audio Payload Too Large".to_string(),
                status: 400,
                detail: "Audio payload exceeds maximum permitted size (35MB)".to_string(),
                instance: "/v1/audio/check".to_string(),
            }),
        ));
    }

    let (samples, sample_rate, channels) =
        decode_b64_to_pcm(&body.audio_base64).map_err(|detail| {
            (
                StatusCode::BAD_REQUEST,
                Json(ProblemDetails {
                    problem_type: "https://voxforge.org/errors/invalid-audio".to_string(),
                    title: "Invalid Audio Input".to_string(),
                    status: 400,
                    detail,
                    instance: "/v1/audio/check".to_string(),
                }),
            )
        })?;

    let report =
        voxforge_audio::AudioAnalyzer::check_signal_anomalies(&samples, sample_rate, channels);
    Ok(Json(report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;

    fn b64_encode(data: &[u8]) -> String {
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut result = String::new();
        let mut i = 0;
        while i < data.len() {
            let b0 = data[i] as u32;
            let b1 = if i + 1 < data.len() {
                data[i + 1] as u32
            } else {
                0
            };
            let b2 = if i + 2 < data.len() {
                data[i + 2] as u32
            } else {
                0
            };
            let triple = (b0 << 16) | (b1 << 8) | b2;

            result.push(CHARSET[((triple >> 18) & 0x3F) as usize] as char);
            result.push(CHARSET[((triple >> 12) & 0x3F) as usize] as char);
            if i + 1 < data.len() {
                result.push(CHARSET[((triple >> 6) & 0x3F) as usize] as char);
            } else {
                result.push('=');
            }
            if i + 2 < data.len() {
                result.push(CHARSET[(triple & 0x3F) as usize] as char);
            } else {
                result.push('=');
            }
            i += 3;
        }
        result
    }

    fn test_state() -> AppState {
        let registry = std::sync::Arc::new(voxforge_engine::EngineRegistry::new());
        let store = std::sync::Arc::new(voxforge_core::store::memory::MemoryStore::new());
        let hardware = voxforge_hardware::HardwareProbe::probe();
        AppState::new(registry, store, hardware, None)
    }

    #[tokio::test]
    async fn test_verify_voice_empty_test_audio_rejected() {
        let state = test_state();
        let req = VerifyVoiceRequest {
            reference_audio_base64: Some("dGVzdA==".to_string()),
            test_audio_base64: "   ".to_string(),
            voice_profile_id: None,
        };
        let err = verify_voice(State(state), Json(req)).await.unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert_eq!(err.1 .0.title, "Missing Test Audio");
    }

    #[tokio::test]
    async fn test_verify_voice_missing_reference_rejected() {
        let state = test_state();
        let pcm: Vec<i16> = (0..4800).map(|i| ((i % 80) * 100) as i16).collect();
        let wav = voxforge_audio::WavEncoder::encode_pcm16_to_wav(&pcm, 24000, 1).unwrap();
        let b64 = b64_encode(&wav);

        let req = VerifyVoiceRequest {
            reference_audio_base64: None,
            test_audio_base64: b64,
            voice_profile_id: None,
        };
        let err = verify_voice(State(state), Json(req)).await.unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert_eq!(err.1 .0.title, "Missing Reference Audio or Profile");
    }

    #[tokio::test]
    async fn test_verify_voice_identical_audio_matches_with_high_confidence() {
        let state = test_state();
        // 140Hz tone with formants
        let pcm: Vec<i16> = (0..9600)
            .map(|i| {
                let t = i as f32 / 24000.0;
                let s = (2.0 * std::f32::consts::PI * 140.0 * t).sin() * 12000.0
                    + (2.0 * std::f32::consts::PI * 500.0 * t).sin() * 4000.0;
                s.round() as i16
            })
            .collect();
        let wav = voxforge_audio::WavEncoder::encode_pcm16_to_wav(&pcm, 24000, 1).unwrap();
        let b64 = b64_encode(&wav);

        let req = VerifyVoiceRequest {
            reference_audio_base64: Some(b64.clone()),
            test_audio_base64: b64,
            voice_profile_id: None,
        };
        let res = verify_voice(State(state), Json(req))
            .await
            .expect("Verification must succeed");
        assert!(res.0.is_match);
        assert_eq!(res.0.confidence_rating, "high");
        assert!(res.0.similarity_score > 0.95);
        assert!(res.0.overall_biometric_score > 90.0);
    }

    #[tokio::test]
    async fn test_blend_profiles_endpoint() {
        let state = test_state();
        let mut meta_a = HashMap::new();
        meta_a.insert("f0_mean_hz".to_string(), "130.0".to_string());
        meta_a.insert("formant_f1_hz".to_string(), "480.0".to_string());
        let profile_a = VoiceProfile {
            id: "speaker-a".to_string(),
            name: "Speaker A".to_string(),
            engine_id: "qwen3-tts".to_string(),
            language: "en-US".to_string(),
            gender: Some(voxforge_core::models::Gender::Male),
            description: None,
            reference_audio_path: None,
            reference_audio_base64: None,
            reference_transcript: None,
            embedding: Some(vec![1.0; 512]),
            clone_capabilities: None,
            similarity_score: None,
            sample_count: 1,
            version: 1,
            metadata: meta_a,
            created_at: chrono::Utc::now(),
            refined_at: None,
        };
        state.voice_profiles.insert(profile_a.clone()).await;

        let mut meta_b = HashMap::new();
        meta_b.insert("f0_mean_hz".to_string(), "210.0".to_string());
        meta_b.insert("formant_f1_hz".to_string(), "550.0".to_string());
        let profile_b = VoiceProfile {
            id: "speaker-b".to_string(),
            name: "Speaker B".to_string(),
            engine_id: "qwen3-tts".to_string(),
            language: "en-US".to_string(),
            gender: Some(voxforge_core::models::Gender::Female),
            description: None,
            reference_audio_path: None,
            reference_audio_base64: None,
            reference_transcript: None,
            embedding: Some(vec![0.8; 512]),
            clone_capabilities: None,
            similarity_score: None,
            sample_count: 1,
            version: 1,
            metadata: meta_b,
            created_at: chrono::Utc::now(),
            refined_at: None,
        };
        state.voice_profiles.insert(profile_b.clone()).await;

        let blend_req = BlendProfilesRequest {
            name: "Speaker A+B Blend".to_string(),
            profile_id_a: profile_a.id.clone(),
            profile_id_b: profile_b.id.clone(),
            weight_b: 0.5,
            description: Some("Hybrid test blend".to_string()),
        };
        let blend_res = blend_profiles(State(state.clone()), Json(blend_req))
            .await
            .unwrap()
            .0;
        assert_eq!(blend_res.blended_profile.name, "Speaker A+B Blend");
        assert_eq!(blend_res.interpolation_weight_b, 0.5);
        assert!(blend_res.cosine_similarity_to_a > 0.95);
        assert!(blend_res.cosine_similarity_to_b > 0.95);
        assert!(blend_res.blended_profile.embedding.is_some());
        assert_eq!(
            blend_res.blended_profile.metadata.get("blended").unwrap(),
            "true"
        );
        let blended_f0: f32 = blend_res
            .blended_profile
            .metadata
            .get("f0_mean_hz")
            .unwrap()
            .parse()
            .unwrap();
        assert!((blended_f0 - 170.0).abs() < 1.0);
    }
}
