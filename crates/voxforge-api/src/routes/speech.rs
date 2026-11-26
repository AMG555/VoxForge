use std::sync::Arc;

use axum::{
    body::{Body, Bytes},
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{header, HeaderValue, StatusCode},
    response::Response,
    Json,
};
use futures::stream;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::warn;

use voxforge_audio::{
    has_paralinguistic_tags, parse_paralinguistic_text, AudioMerger, ParalinguisticSegment,
    ParalinguisticSynthesizer, WavEncoder,
};
use voxforge_core::error::ProblemDetails;
use voxforge_core::models::{AudioChunk, AudioContainerFormat, Voice};
use voxforge_engine::{SynthesisRequest, TtsEngine};
use voxforge_router::SynthesisPolicy;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiSpeechRequest {
    pub model: String,
    pub input: String,
    pub voice: String,
    #[serde(default)]
    pub response_format: AudioContainerFormat,
    #[serde(default = "default_speed")]
    pub speed: f32,
    #[serde(default)]
    pub pitch: Option<f32>,
    /// Optional natural language instruction / delivery style steering (e.g. "whisper", "urgent", "cheerful").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instruct: Option<String>,
    /// Optional diffusion / flow-matching refinement steps (e.g. 16 = preview, 32 = studio quality, 64 = ultra).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_step: Option<u32>,
    /// Optional bit depth precision (16 = standard PCM, 24 = studio master, 32 = 32-bit float).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub precision: Option<u16>,
    /// Optional random seed for deterministic generation and take reproducibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// Optional crossfade duration in ms when synthesizing multi-sentence long text. Default: 30ms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crossfade_ms: Option<u32>,
    /// Optional explicit output language / dialect code (e.g. "ml-IN", "es-MX", "arb", "ja-JP").
    /// Enables cross-lingual speech with any cloned voice profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Optional SLA-policy-based routing. When present the engine is chosen
    /// automatically by `VoiceRouter`; `model` and `voice` still select the
    /// voice *within* the chosen engine as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<SynthesisPolicy>,
}

fn default_speed() -> f32 {
    1.0
}

fn validate_speech_request(
    payload: &OpenAiSpeechRequest,
) -> Result<(), (StatusCode, Json<ProblemDetails>)> {
    if payload.input.trim().is_empty() {
        let err = ProblemDetails {
            problem_type: "https://voxforge.org/errors/empty-input".to_string(),
            title: "Empty Input Text".to_string(),
            status: StatusCode::BAD_REQUEST.as_u16(),
            detail: "The 'input' parameter must contain at least 1 non-whitespace character"
                .to_string(),
            instance: "/v1/audio/speech".to_string(),
        };
        return Err((StatusCode::BAD_REQUEST, Json(err)));
    }

    if payload.input.len() > 50_000 {
        let err = ProblemDetails {
            problem_type: "https://voxforge.org/errors/input-too-large".to_string(),
            title: "Input Text Exceeds Limit".to_string(),
            status: StatusCode::UNPROCESSABLE_ENTITY.as_u16(),
            detail: "The 'input' parameter cannot exceed 50,000 characters per request".to_string(),
            instance: "/v1/audio/speech".to_string(),
        };
        return Err((StatusCode::UNPROCESSABLE_ENTITY, Json(err)));
    }

    if !payload.speed.is_finite() || !(0.25..=4.0).contains(&payload.speed) {
        let err = ProblemDetails {
            problem_type: "https://voxforge.org/errors/invalid-parameter".to_string(),
            title: "Invalid Speed Parameter".to_string(),
            status: StatusCode::BAD_REQUEST.as_u16(),
            detail: "The 'speed' parameter must be a finite number between 0.25 and 4.0"
                .to_string(),
            instance: "/v1/audio/speech".to_string(),
        };
        return Err((StatusCode::BAD_REQUEST, Json(err)));
    }

    if let Some(p) = payload.pitch {
        if !p.is_finite() || !(-50.0..=50.0).contains(&p) {
            let err = ProblemDetails {
                problem_type: "https://voxforge.org/errors/invalid-parameter".to_string(),
                title: "Invalid Pitch Parameter".to_string(),
                status: StatusCode::BAD_REQUEST.as_u16(),
                detail:
                    "The 'pitch' parameter must be a finite number between -50.0 and 50.0 semitones"
                        .to_string(),
                instance: "/v1/audio/speech".to_string(),
            };
            return Err((StatusCode::BAD_REQUEST, Json(err)));
        }
    }

    if let Some(steps) = payload.num_step {
        if !(4..=128).contains(&steps) {
            let err = ProblemDetails {
                problem_type: "https://voxforge.org/errors/invalid-parameter".to_string(),
                title: "Invalid num_step Parameter".to_string(),
                status: StatusCode::BAD_REQUEST.as_u16(),
                detail: "The 'num_step' parameter must be between 4 and 128".to_string(),
                instance: "/v1/audio/speech".to_string(),
            };
            return Err((StatusCode::BAD_REQUEST, Json(err)));
        }
    }

    if let Some(prec) = payload.precision {
        if prec != 16 && prec != 24 && prec != 32 {
            let err = ProblemDetails {
                problem_type: "https://voxforge.org/errors/invalid-parameter".to_string(),
                title: "Invalid precision Parameter".to_string(),
                status: StatusCode::BAD_REQUEST.as_u16(),
                detail: "The 'precision' parameter must be 16, 24, or 32".to_string(),
                instance: "/v1/audio/speech".to_string(),
            };
            return Err((StatusCode::BAD_REQUEST, Json(err)));
        }
    }

    Ok(())
}

/// Resolve engine + voice, honouring `SynthesisPolicy` when provided.
/// When no policy is present the legacy `resolve_voice` → `model` fallback path
/// is used unchanged, so all existing callers continue to work.
/// Prevent Malayalam number-to-word expansion when synthesizing pure Latin/English sentences.
fn effective_pronunciation_language<'a>(
    text: &str,
    requested_lang: Option<&'a str>,
) -> Option<&'a str> {
    if let Some(lang) = requested_lang {
        let is_ml = lang.eq_ignore_ascii_case("ml")
            || lang.to_ascii_lowercase().starts_with("ml-")
            || lang.eq_ignore_ascii_case("mal");
        if is_ml {
            let has_ml = text.chars().any(|c| ('\u{0D00}'..='\u{0D7F}').contains(&c));
            let has_latin = text.chars().any(|c| c.is_ascii_alphabetic());
            if !has_ml && has_latin {
                return Some("en-IN");
            }
        }
    }
    requested_lang
}

async fn ensure_engine_runtime_ready(
    engine: &dyn TtsEngine,
    voice: &Voice,
    instance: &str,
) -> Result<(), (StatusCode, Json<ProblemDetails>)> {
    if engine.id() == "kokoro" {
        let probe = voxforge_engine::kokoro::KokoroTtsEngine::new();
        if !probe.has_live_sidecar().await {
            let err = ProblemDetails {
                problem_type: "https://voxforge.org/errors/runtime-not-installed".to_string(),
                title: "Kokoro Neural Runtime Not Active".to_string(),
                status: StatusCode::FAILED_DEPENDENCY.as_u16(),
                detail: format!(
                    "Kokoro-82M voice '{}' requires a running Kokoro inference server at http://127.0.0.1:8880 (VOXFORGE_KOKORO_URL). Install 'kokoro' & start the Kokoro sidecar from Model Catalog / Hardware & Engines, or select an Edge-TTS neural voice.",
                    voice.id
                ),
                instance: instance.to_string(),
            };
            return Err((StatusCode::FAILED_DEPENDENCY, Json(err)));
        }
    }
    Ok(())
}

async fn resolve_engine_and_voice(
    state: &AppState,
    payload: &OpenAiSpeechRequest,
) -> Result<(Arc<dyn TtsEngine>, Voice), (StatusCode, Json<ProblemDetails>)> {
    // ── Policy-based routing path ─────────────────────────────────────────
    if let Some(policy) = &payload.policy {
        return match state
            .voice_router
            .route(
                policy,
                &payload.input,
                &payload.voice,
                payload.response_format,
            )
            .await
        {
            Ok((decision, _synth_req)) => {
                // Build a synthetic Voice so the rest of the handler stays unchanged
                let voice = Voice {
                    id: payload.voice.clone(),
                    name: payload.voice.clone(),
                    engine_id: decision.engine_id.clone(),
                    language: policy
                        .language
                        .clone()
                        .unwrap_or_else(|| "en-US".to_string()),
                    gender: voxforge_core::models::Gender::Neutral,
                    sample_rate_hz: 24000,
                    tags: vec![],
                    description: None,
                };
                Ok((decision.engine, voice))
            }
            Err(e) => {
                let err = ProblemDetails {
                    problem_type: "https://voxforge.org/errors/no-engine-for-policy".to_string(),
                    title: "No Engine Satisfies Policy".to_string(),
                    status: StatusCode::UNPROCESSABLE_ENTITY.as_u16(),
                    detail: e.to_string(),
                    instance: "/v1/audio/speech".to_string(),
                };
                Err((StatusCode::UNPROCESSABLE_ENTITY, Json(err)))
            }
        };
    }

    // ── Voice Identity resolution path (portable voice abstraction) ─────
    if let Ok(res) = state
        .voice_identities
        .resolve(&payload.voice, &state.engine_registry)
        .await
    {
        return Ok(res);
    }

    // ── Legacy path (concrete voice_id / model fallback) ───────────────────
    match state.engine_registry.resolve_voice(&payload.voice).await {
        Ok(res) => Ok(res),
        Err(_) => {
            if let Some(engine) = state.engine_registry.get(&payload.model).await {
                let voices = engine.voices().await.unwrap_or_default();
                let chosen_voice = voices.into_iter().next().unwrap_or(Voice {
                    id: payload.voice.clone(),
                    name: payload.voice.clone(),
                    engine_id: engine.id().to_string(),
                    language: "en-US".to_string(),
                    gender: voxforge_core::models::Gender::Neutral,
                    sample_rate_hz: 24000,
                    tags: vec![],
                    description: None,
                });
                Ok((engine, chosen_voice))
            } else {
                let err = ProblemDetails {
                    problem_type: "https://voxforge.org/errors/voice-not-found".to_string(),
                    title: "Voice or Engine Not Found".to_string(),
                    status: StatusCode::NOT_FOUND.as_u16(),
                    detail: format!(
                        "Neither voice '{}' nor engine '{}' could be found",
                        payload.voice, payload.model
                    ),
                    instance: "/v1/audio/speech".to_string(),
                };
                Err((StatusCode::NOT_FOUND, Json(err)))
            }
        }
    }
}

fn encode_chunk_to_response(
    audio_chunk: voxforge_core::models::AudioChunk,
    format: AudioContainerFormat,
    precision: Option<u16>,
) -> Result<Response, (StatusCode, Json<ProblemDetails>)> {
    let (audio_bytes, content_type) = match format {
        AudioContainerFormat::Pcm => {
            let bytes: Vec<u8> = audio_chunk
                .pcm_data
                .iter()
                .flat_map(|s| s.to_le_bytes())
                .collect();
            (bytes, "audio/pcm")
        }
        AudioContainerFormat::Wav => {
            let wav_bytes = if let Some(prec) = precision {
                WavEncoder::encode_pcm_to_wav_with_precision(
                    &audio_chunk.pcm_data,
                    audio_chunk.sample_rate,
                    audio_chunk.channels,
                    prec,
                )
            } else {
                WavEncoder::encode_pcm16_to_wav(
                    &audio_chunk.pcm_data,
                    audio_chunk.sample_rate,
                    audio_chunk.channels,
                )
            }
            .map_err(|e| {
                let err = ProblemDetails {
                    problem_type: "https://voxforge.org/errors/encoding-failure".to_string(),
                    title: "Audio Encoding Error".to_string(),
                    status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                    detail: e.to_string(),
                    instance: "/v1/audio/speech".to_string(),
                };
                (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
            })?;
            (wav_bytes, "audio/wav")
        }
        // MP3, Opus, FLAC, AAC encoding is not yet implemented locally.
        // Return 406 with a clear contract error rather than silently sending
        // WAV bytes under the wrong MIME type (which breaks every decoder).
        other => {
            let err = ProblemDetails {
                problem_type: "https://voxforge.org/errors/format-not-supported".to_string(),
                title: "Output Format Not Supported".to_string(),
                status: StatusCode::NOT_ACCEPTABLE.as_u16(),
                detail: format!(
                    "Response format '{:?}' is not yet supported for local synthesis. \
                     Use 'wav' or 'pcm'. To get MP3/Opus output pass the request through \
                     an upstream router that supports transcoding (VOXFORGE_ROUTER_URL).",
                    other
                ),
                instance: "/v1/audio/speech".to_string(),
            };
            return Err((StatusCode::NOT_ACCEPTABLE, Json(err)));
        }
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, HeaderValue::from_static(content_type))
        .header(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"))
        .body(Body::from(audio_bytes))
        .map_err(|e| {
            let err = ProblemDetails {
                problem_type: "https://voxforge.org/errors/internal".to_string(),
                title: "Internal Error".to_string(),
                status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                detail: e.to_string(),
                instance: "/v1/audio/speech".to_string(),
            };
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        })
}

pub async fn synthesize_speech(
    State(state): State<AppState>,
    Json(payload): Json<OpenAiSpeechRequest>,
) -> Result<Response, (StatusCode, Json<ProblemDetails>)> {
    validate_speech_request(&payload)?;

    // ── Distributed worker dispatch path ──────────────────────────────────
    let is_explicit_worker = payload.model.starts_with("worker:");
    let worker_target = payload
        .model
        .strip_prefix("worker:")
        .unwrap_or(&payload.model);

    if is_explicit_worker || state.engine_registry.get(&payload.model).await.is_none() {
        if let Some(worker) = state.worker_pool.select_worker(worker_target).await {
            if let Err(e) = state.worker_pool.acquire_lease(&worker.worker_id).await {
                let err = ProblemDetails {
                    problem_type: "https://voxforge.org/errors/worker-busy".to_string(),
                    title: "Worker Node Busy".to_string(),
                    status: StatusCode::TOO_MANY_REQUESTS.as_u16(),
                    detail: format!("Worker '{}' at capacity: {}", worker.worker_id, e),
                    instance: "/v1/audio/speech".to_string(),
                };
                return Err((StatusCode::TOO_MANY_REQUESTS, Json(err)));
            }

            let mut worker_payload = match serde_json::to_value(&payload) {
                Ok(val) => val,
                Err(e) => {
                    let err = ProblemDetails {
                        problem_type: "https://voxforge.org/errors/internal".to_string(),
                        title: "Payload Serialization Error".to_string(),
                        status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                        detail: e.to_string(),
                        instance: "/v1/audio/speech".to_string(),
                    };
                    return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(err)));
                }
            };
            if let Some(obj) = worker_payload.as_object_mut() {
                obj.insert(
                    "model".to_string(),
                    serde_json::Value::String(worker_target.to_string()),
                );
            }

            let dispatch_res = state
                .worker_client
                .dispatch_raw(&worker, &worker_payload)
                .await;
            let _ = state.worker_pool.release_lease(&worker.worker_id).await;

            return match dispatch_res {
                Ok(audio_bytes) => {
                    let content_type = payload.response_format.mime_type();
                    let response = Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, content_type)
                        .header("x-voxforge-dispatched-worker", worker.worker_id.to_string())
                        .body(Body::from(audio_bytes))
                        .map_err(|e| {
                            let err = ProblemDetails {
                                problem_type: "https://voxforge.org/errors/internal".to_string(),
                                title: "Response Build Failed".to_string(),
                                status: 500,
                                detail: e.to_string(),
                                instance: "/v1/audio/speech".to_string(),
                            };
                            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
                        })?;
                    Ok(response)
                }
                Err(e) => {
                    let err = ProblemDetails {
                        problem_type: "https://voxforge.org/errors/worker-dispatch-failed"
                            .to_string(),
                        title: "Worker Dispatch Failed".to_string(),
                        status: StatusCode::BAD_GATEWAY.as_u16(),
                        detail: e.to_string(),
                        instance: "/v1/audio/speech".to_string(),
                    };
                    Err((StatusCode::BAD_GATEWAY, Json(err)))
                }
            };
        } else if is_explicit_worker {
            let err = ProblemDetails {
                problem_type: "https://voxforge.org/errors/no-worker-available".to_string(),
                title: "No Worker Available".to_string(),
                status: StatusCode::NOT_FOUND.as_u16(),
                detail: format!(
                    "No available cluster worker node supports model '{worker_target}'"
                ),
                instance: "/v1/audio/speech".to_string(),
            };
            return Err((StatusCode::NOT_FOUND, Json(err)));
        }
    }

    // ── Cloned voice profile synthesis path ──────────────────────────────
    if let Some(profile) = state.voice_profiles.find(&payload.voice).await {
        let engine_opt = state
            .engine_registry
            .get(&profile.engine_id)
            .await
            .or_else(|| {
                // If engine_id is not found directly, try qwen3-tts
                tokio::task::block_in_place(|| None)
            });
        let target_engine = match engine_opt {
            Some(e) => Some(e),
            None => state.engine_registry.get("qwen3-tts").await,
        };
        if let Some(engine) = target_engine {
            if engine.supports_cloning() {
                let effective_lang = payload
                    .language
                    .as_deref()
                    .filter(|s| !s.trim().is_empty() && !s.eq_ignore_ascii_case("AUTO"))
                    .map(voxforge_engine::qwen3::normalize_language_code);
                let chunks = split_text_into_chunks(&payload.input, 800);
                let crossfade_ms = payload.crossfade_ms.unwrap_or(30);
                let mut pcm_chunks: Vec<Vec<i16>> = Vec::with_capacity(chunks.len());
                let mut f32_chunks: Vec<Vec<f32>> = Vec::with_capacity(chunks.len());
                let mut all_chunks_have_f32 = true;
                let mut sample_rate = 24000;
                let mut channels = 1;

                for chunk_text in chunks {
                    let norm_lang = effective_pronunciation_language(
                        &chunk_text,
                        effective_lang.as_deref().or(Some(profile.language.as_str())),
                    );
                    let cloned_req = voxforge_core::models::ClonedSynthesisRequest {
                        text: state
                            .pronunciation
                            .process_with_language(&chunk_text, norm_lang),
                        profile: profile.clone(),
                        speed: payload.speed,
                        pitch: payload.pitch.unwrap_or(0.0),
                        format: payload.response_format,
                        language: effective_lang.clone(),
                        instruct: payload.instruct.clone(),
                        num_step: payload.num_step,
                        seed: payload.seed,
                        precision: payload.precision,
                    };
                    let (chunk, opt_f32) =
                        engine
                            .synthesize_cloned_hq(&cloned_req)
                            .await
                            .map_err(|e| {
                                let err = ProblemDetails {
                                    problem_type:
                                        "https://voxforge.org/errors/cloned-synthesis-failure"
                                            .to_string(),
                                    title: "Cloned Synthesis Error".to_string(),
                                    status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                                    detail: e.to_string(),
                                    instance: "/v1/audio/speech".to_string(),
                                };
                                (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
                            })?;
                    sample_rate = chunk.sample_rate;
                    channels = chunk.channels;
                    pcm_chunks.push(chunk.pcm_data);
                    if let Some(f32_buf) = opt_f32 {
                        f32_chunks.push(f32_buf);
                    } else {
                        all_chunks_have_f32 = false;
                    }
                }

                // Preserve lossless 24-bit/32-bit float fidelity when neural sidecar produced f32 output
                if all_chunks_have_f32
                    && !f32_chunks.is_empty()
                    && payload.response_format == AudioContainerFormat::Wav
                    && payload.precision.unwrap_or(16) > 16
                {
                    let final_f32 = if f32_chunks.len() == 1 {
                        f32_chunks.remove(0)
                    } else {
                        let slices: Vec<&[f32]> = f32_chunks.iter().map(|v| v.as_slice()).collect();
                        AudioMerger::concatenate_f32_with_equal_power_crossfade(
                            &slices,
                            sample_rate,
                            crossfade_ms,
                        )
                    };
                    let wav_bytes = WavEncoder::encode_f32_to_wav_with_precision(
                        &final_f32,
                        sample_rate,
                        channels,
                        payload.precision.unwrap_or(24),
                    )
                    .map_err(|e| {
                        let err = ProblemDetails {
                            problem_type: "https://voxforge.org/errors/encoding-failure"
                                .to_string(),
                            title: "Encoding Error".to_string(),
                            status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                            detail: e.to_string(),
                            instance: "/v1/audio/speech".to_string(),
                        };
                        (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
                    })?;

                    let mut response = Response::new(Body::from(wav_bytes));
                    response
                        .headers_mut()
                        .insert(header::CONTENT_TYPE, HeaderValue::from_static("audio/wav"));
                    return Ok(response);
                }

                let final_pcm = if pcm_chunks.len() == 1 {
                    pcm_chunks.remove(0)
                } else {
                    let slices: Vec<&[i16]> = pcm_chunks.iter().map(|v| v.as_slice()).collect();
                    AudioMerger::concatenate_with_equal_power_crossfade(
                        &slices,
                        sample_rate,
                        crossfade_ms,
                    )
                };

                let audio_chunk = AudioChunk {
                    pcm_data: final_pcm,
                    sample_rate,
                    channels,
                    is_final: true,
                };
                return encode_chunk_to_response(
                    audio_chunk,
                    payload.response_format,
                    payload.precision,
                );
            }
        }
    }

    let (engine, voice) = resolve_engine_and_voice(&state, &payload).await?;
    ensure_engine_runtime_ready(engine.as_ref(), &voice, "/v1/audio/speech").await?;
    let crossfade_ms = payload.crossfade_ms.unwrap_or(30);

    let start = std::time::Instant::now();
    let mut pcm_chunks = Vec::new();
    let mut sample_rate = voice.sample_rate_hz;
    let mut channels = 1;

    if has_paralinguistic_tags(&payload.input) {
        let segments = parse_paralinguistic_text(&payload.input);
        for seg in segments {
            match seg {
                ParalinguisticSegment::Text(text) => {
                    let chunks = split_text_into_chunks(&text, 800);
                    let eff_lang = payload
                        .language
                        .as_deref()
                        .filter(|s| !s.trim().is_empty() && !s.eq_ignore_ascii_case("AUTO"))
                        .or(Some(voice.language.as_str()));
                    for chunk_text in chunks {
                        let synth_lang = effective_pronunciation_language(&chunk_text, eff_lang);
                        let synth_req = SynthesisRequest {
                            text: state
                                .pronunciation
                                .process_with_language(&chunk_text, synth_lang),
                            voice_id: voice.id.clone(),
                            speed: payload.speed,
                            pitch: payload.pitch.unwrap_or(0.0),
                            format: payload.response_format,
                        };

                        let chunk = state
                            .engine_registry
                            .synthesize_cached(engine.clone(), &synth_req)
                            .await
                            .map_err(|e| {
                                let (status_code, detail) = e.classify_synthesis_error();
                                let err = ProblemDetails {
                                    problem_type: "https://voxforge.org/errors/synthesis-failure"
                                        .to_string(),
                                    title: "Synthesis Error".to_string(),
                                    status: status_code,
                                    detail,
                                    instance: "/v1/audio/speech".to_string(),
                                };
                                (
                                    StatusCode::from_u16(status_code)
                                        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                                    Json(err),
                                )
                            })?;
                        sample_rate = chunk.sample_rate;
                        channels = chunk.channels;
                        pcm_chunks.push(chunk.pcm_data);
                    }
                }
                ParalinguisticSegment::Emotion(tag) => {
                    let event_pcm = ParalinguisticSynthesizer::synthesize_event(tag, sample_rate);
                    if !event_pcm.is_empty() {
                        pcm_chunks.push(event_pcm);
                    }
                }
            }
        }
    } else {
        let chunks = split_text_into_chunks(&payload.input, 800);
        let eff_lang = payload
            .language
            .as_deref()
            .filter(|s| !s.trim().is_empty() && !s.eq_ignore_ascii_case("AUTO"))
            .or(Some(voice.language.as_str()));
        for chunk_text in chunks {
            let synth_lang = effective_pronunciation_language(&chunk_text, eff_lang);
            let synth_req = SynthesisRequest {
                text: state
                    .pronunciation
                    .process_with_language(&chunk_text, synth_lang),
                voice_id: voice.id.clone(),
                speed: payload.speed,
                pitch: payload.pitch.unwrap_or(0.0),
                format: payload.response_format,
            };

            let chunk = state
                .engine_registry
                .synthesize_cached(engine.clone(), &synth_req)
                .await
                .map_err(|e| {
                    let (status_code, detail) = e.classify_synthesis_error();
                    let err = ProblemDetails {
                        problem_type: "https://voxforge.org/errors/synthesis-failure".to_string(),
                        title: "Synthesis Error".to_string(),
                        status: status_code,
                        detail,
                        instance: "/v1/audio/speech".to_string(),
                    };
                    (
                        StatusCode::from_u16(status_code)
                            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                        Json(err),
                    )
                })?;
            sample_rate = chunk.sample_rate;
            channels = chunk.channels;
            pcm_chunks.push(chunk.pcm_data);
        }
    }

    let final_pcm = if pcm_chunks.len() == 1 {
        pcm_chunks.remove(0)
    } else if pcm_chunks.is_empty() {
        Vec::new()
    } else {
        let slices: Vec<&[i16]> = pcm_chunks.iter().map(|v| v.as_slice()).collect();
        AudioMerger::concatenate_with_equal_power_crossfade(&slices, sample_rate, crossfade_ms)
    };

    let elapsed = start.elapsed();
    state
        .metrics
        .record_synthesis(elapsed.as_millis() as u64, final_pcm.len());

    let audio_chunk = AudioChunk {
        pcm_data: final_pcm,
        sample_rate,
        channels,
        is_final: true,
    };
    encode_chunk_to_response(audio_chunk, payload.response_format, payload.precision)
}

fn split_text_into_chunks(text: &str, max_chunk_chars: usize) -> Vec<String> {
    voxforge_pronunciation::chunk_text(text, max_chunk_chars)
}

pub async fn synthesize_speech_stream(
    State(state): State<AppState>,
    Json(payload): Json<OpenAiSpeechRequest>,
) -> Result<Response, (StatusCode, Json<ProblemDetails>)> {
    validate_speech_request(&payload)?;
    let (engine, voice) = resolve_engine_and_voice(&state, &payload).await?;
    ensure_engine_runtime_ready(engine.as_ref(), &voice, "/v1/audio/speech/stream").await?;

    let eff_lang = payload
        .language
        .as_deref()
        .filter(|s| !s.trim().is_empty() && !s.eq_ignore_ascii_case("AUTO"))
        .or(Some(voice.language.as_str()));
    let synth_lang = effective_pronunciation_language(&payload.input, eff_lang);
    let synth_req = SynthesisRequest {
        text: state
            .pronunciation
            .process_with_language(&payload.input, synth_lang),
        voice_id: voice.id,
        speed: payload.speed,
        pitch: payload.pitch.unwrap_or(0.0),
        format: payload.response_format,
    };

    let rx = engine.synthesize_stream(&synth_req).await.map_err(|e| {
        let err = ProblemDetails {
            problem_type: "https://voxforge.org/errors/synthesis-failure".to_string(),
            title: "Streaming Synthesis Error".to_string(),
            status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
            detail: e.to_string(),
            instance: "/v1/audio/speech/stream".to_string(),
        };
        (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
    })?;

    let stream = stream::unfold(rx, |mut rx| async move {
        match rx.recv().await {
            Some(Ok(chunk)) => {
                let bytes: Vec<u8> = chunk
                    .pcm_data
                    .iter()
                    .flat_map(|s| s.to_le_bytes())
                    .collect();
                Some((Ok::<_, std::io::Error>(Bytes::from(bytes)), rx))
            }
            Some(Err(e)) => {
                warn!("Streaming synthesis chunk error: {e}");
                None
            }
            None => None,
        }
    });

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, HeaderValue::from_static("audio/pcm"))
        .header(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"))
        .header(
            header::TRANSFER_ENCODING,
            HeaderValue::from_static("chunked"),
        )
        .body(Body::from_stream(stream))
        .map_err(|e| {
            let err = ProblemDetails {
                problem_type: "https://voxforge.org/errors/internal".to_string(),
                title: "Internal Error".to_string(),
                status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                detail: e.to_string(),
                instance: "/v1/audio/speech/stream".to_string(),
            };
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        })?;

    Ok(response)
}

pub async fn speech_websocket(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(|socket| handle_speech_socket(socket, state))
}

async fn handle_speech_socket(mut socket: WebSocket, state: AppState) {
    while let Some(msg_res) = socket.recv().await {
        let msg = match msg_res {
            Ok(m) => m,
            Err(_) => break,
        };

        match msg {
            Message::Text(text) => {
                let req: OpenAiSpeechRequest = match serde_json::from_str(&text) {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = socket
                            .send(Message::Text(
                                json!({ "error": format!("Invalid JSON: {e}") }).to_string(),
                            ))
                            .await;
                        continue;
                    }
                };

                if let Err((_status, err_json)) = validate_speech_request(&req) {
                    let _ = socket
                        .send(Message::Text(
                            json!({ "error": err_json.detail }).to_string(),
                        ))
                        .await;
                    continue;
                }

                let (engine, voice) = match resolve_engine_and_voice(&state, &req).await {
                    Ok(res) => res,
                    Err((_status, err_json)) => {
                        let _ = socket
                            .send(Message::Text(
                                json!({ "error": err_json.detail }).to_string(),
                            ))
                            .await;
                        continue;
                    }
                };

                let eff_lang = req
                    .language
                    .as_deref()
                    .filter(|s| !s.trim().is_empty() && !s.eq_ignore_ascii_case("AUTO"))
                    .or(Some(voice.language.as_str()));
                let synth_lang = effective_pronunciation_language(&req.input, eff_lang);
                let synth_req = SynthesisRequest {
                    text: state
                        .pronunciation
                        .process_with_language(&req.input, synth_lang),
                    voice_id: voice.id,
                    speed: req.speed.clamp(0.25, 4.0),
                    pitch: req.pitch.unwrap_or(0.0).clamp(-50.0, 50.0),
                    format: req.response_format,
                };

                let mut rx = match engine.synthesize_stream(&synth_req).await {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = socket
                            .send(Message::Text(json!({ "error": e.to_string() }).to_string()))
                            .await;
                        continue;
                    }
                };

                let mut total_samples = 0;
                while let Some(chunk_res) = rx.recv().await {
                    match chunk_res {
                        Ok(chunk) => {
                            total_samples += chunk.pcm_data.len();
                            let bytes: Vec<u8> = chunk
                                .pcm_data
                                .iter()
                                .flat_map(|s| s.to_le_bytes())
                                .collect();
                            if socket.send(Message::Binary(bytes)).await.is_err() {
                                return;
                            }
                        }
                        Err(e) => {
                            let _ = socket
                                .send(Message::Text(json!({ "error": e.to_string() }).to_string()))
                                .await;
                            break;
                        }
                    }
                }

                let _ = socket
                    .send(Message::Text(
                        json!({ "event": "done", "total_samples": total_samples }).to_string(),
                    ))
                    .await;
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
}
