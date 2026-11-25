//! Speech-to-Speech (Voice Conversion / Neural Voice Changer) endpoints.
//!
//! | Method | Path                         | Description                             |
//! |--------|------------------------------|-----------------------------------------|
//! | POST   | `/v1/audio/speech-to-speech` | Convert source voice audio into target voice |
//! | POST   | `/v1/audio/voice-conversion` | Alias for speech-to-speech              |

use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderValue, StatusCode},
    response::Response,
    Json,
};
use serde::Deserialize;
use voxforge_asr::TranscriptionOptions;
use voxforge_audio::WavEncoder;
use voxforge_core::error::ProblemDetails;
use voxforge_core::models::{AudioContainerFormat, ClonedSynthesisRequest};
use voxforge_engine::SynthesisRequest;

use crate::routes::cloning::{decode_b64_bytes, decode_b64_to_pcm};
use crate::state::AppState;

#[derive(Debug, Clone, Deserialize)]
pub struct SpeechToSpeechRequest {
    /// Base64-encoded audio (WAV, WebM, MP3).
    pub audio_base64: String,
    /// Target voice profile or voice ID to convert into.
    pub target_voice: String,
    /// Synthesis speed factor (0.5 - 2.5). Default: 1.0.
    #[serde(default = "default_speed")]
    pub speed: f32,
    /// Whether to adjust tempo to match the source audio spoken duration.
    #[serde(default)]
    pub preserve_tempo: bool,
    /// Optional explicit output/source language code (e.g. ml-IN, en-US, arb).
    #[serde(default)]
    pub language: Option<String>,
    /// Optional emotion / prosody instruction.
    #[serde(default)]
    pub instruct: Option<String>,
    /// Optional deterministic seed.
    #[serde(default)]
    pub seed: Option<u64>,
    /// Optional bit depth (16, 24, 32).
    #[serde(default)]
    pub precision: Option<u16>,
    /// Output container format.
    #[serde(default)]
    pub response_format: AudioContainerFormat,
}

pub const MAX_AUDIO_PAYLOAD_CHARS: usize = 32 * 1024 * 1024;

fn default_speed() -> f32 {
    1.0
}

#[cfg(test)]
fn decode_b64(input: &str) -> Option<Vec<u8>> {
    decode_b64_bytes(input).ok()
}

fn decode_audio(payload: &str) -> Result<(Vec<i16>, u32), String> {
    let clean = if let Some(idx) = payload.find(',') {
        payload[idx + 1..].trim()
    } else {
        payload.trim()
    };

    let bytes = decode_b64_bytes(clean).or_else(|_| {
        hex::decode(clean)
            .map_err(|_| "Invalid audio payload: expected valid base64 or hex encoding".to_string())
    })?;

    let is_wav = bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE";
    let (samples, sr, _) = decode_b64_to_pcm(payload)?;
    Ok((samples, if is_wav { sr } else { 16000 }))
}

/// Convert spoken audio into target voice.
pub async fn speech_to_speech(
    State(state): State<AppState>,
    Json(payload): Json<SpeechToSpeechRequest>,
) -> Result<Response, (StatusCode, Json<ProblemDetails>)> {
    if payload.audio_base64.trim().is_empty() {
        let err = ProblemDetails {
            problem_type: "https://voxforge.org/errors/empty-payload".to_string(),
            title: "Missing Audio Payload".to_string(),
            status: StatusCode::BAD_REQUEST.as_u16(),
            detail: "The 'audio_base64' parameter must not be empty".to_string(),
            instance: "/v1/audio/speech-to-speech".to_string(),
        };
        return Err((StatusCode::BAD_REQUEST, Json(err)));
    }

    if payload.audio_base64.len() > MAX_AUDIO_PAYLOAD_CHARS {
        let err = ProblemDetails {
            problem_type: "https://voxforge.org/errors/payload-too-large".to_string(),
            title: "Audio Payload Too Large".to_string(),
            status: StatusCode::PAYLOAD_TOO_LARGE.as_u16(),
            detail: format!(
                "The 'audio_base64' payload exceeds the 32MB limit (was {} bytes)",
                payload.audio_base64.len()
            ),
            instance: "/v1/audio/speech-to-speech".to_string(),
        };
        return Err((StatusCode::PAYLOAD_TOO_LARGE, Json(err)));
    }

    let (pcm_samples, sample_rate) = decode_audio(&payload.audio_base64).map_err(|detail| {
        let err = ProblemDetails {
            problem_type: "https://voxforge.org/errors/invalid-audio-input".to_string(),
            title: "Invalid Audio Input".to_string(),
            status: StatusCode::BAD_REQUEST.as_u16(),
            detail,
            instance: "/v1/audio/speech-to-speech".to_string(),
        };
        (StatusCode::BAD_REQUEST, Json(err))
    })?;

    // 1. Transcribe source speech (live Whisper sidecar first, registry ASR fallback)
    let source_duration_sec = pcm_samples.len() as f64 / sample_rate.max(1) as f64;
    let (raw_text, measured_duration) = if let Some(neural_text) =
        crate::routes::asr::try_sidecar_transcribe(
            &payload.audio_base64,
            payload.language.as_deref(),
        )
        .await
    {
        (neural_text, source_duration_sec)
    } else {
        let asr_engine = state.asr_registry.default_engine().await.ok_or_else(|| {
            let err = ProblemDetails {
                problem_type: "https://voxforge.org/errors/no-asr-engine".to_string(),
                title: "No Default ASR Engine".to_string(),
                status: 500,
                detail: "No default speech recognition engine registered".to_string(),
                instance: "/v1/audio/speech-to-speech".to_string(),
            };
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        })?;

        let opts = TranscriptionOptions {
            language: payload.language.clone(),
            ..TranscriptionOptions::default()
        };
        let transcription = asr_engine
            .transcribe(&pcm_samples, sample_rate, &opts)
            .await
            .map_err(|e| {
                let err = ProblemDetails {
                    problem_type: "https://voxforge.org/errors/transcription-failure".to_string(),
                    title: "ASR Transcription Failed".to_string(),
                    status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                    detail: format!("Failed to transcribe input voice: {}", e),
                    instance: "/v1/audio/speech-to-speech".to_string(),
                };
                (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
            })?;
        (transcription.text, transcription.duration_seconds as f64)
    };

    let text = raw_text.trim();
    if text.is_empty() {
        let err = ProblemDetails {
            problem_type: "https://voxforge.org/errors/no-speech".to_string(),
            title: "No Speech Detected".to_string(),
            status: StatusCode::UNPROCESSABLE_ENTITY.as_u16(),
            detail: "No intelligible speech detected in source audio".to_string(),
            instance: "/v1/audio/speech-to-speech".to_string(),
        };
        return Err((StatusCode::UNPROCESSABLE_ENTITY, Json(err)));
    }

    // 2. Compute speed adjustment if preserve_tempo is requested
    let target_speed = if payload.preserve_tempo && measured_duration > 0.5 {
        let word_count = text.split_whitespace().count().max(1);
        let estimated_normal_dur = word_count as f64 / 2.8;
        let tempo_ratio = estimated_normal_dur / measured_duration;
        (tempo_ratio as f32 * payload.speed).clamp(0.6, 2.2)
    } else {
        payload.speed
    };

    let eff_lang_opt = payload
        .language
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("AUTO"))
        .map(voxforge_engine::qwen3::normalize_language_code);
    let prec = payload.precision.unwrap_or(16);

    // 3. Target cloned profile path OR standard registry voice
    let (target_voice_id, wav_bytes) = if let Some(profile) =
        state.voice_profiles.find(&payload.target_voice).await
    {
        let engine = match state.engine_registry.get(&profile.engine_id).await {
            Some(e) => Some(e),
            None => state.engine_registry.get("qwen3-tts").await,
        };
        if let Some(eng) = engine.filter(|e| e.supports_cloning()) {
            let norm_lang = eff_lang_opt.as_deref().or(Some(profile.language.as_str()));
            let cloned_req = ClonedSynthesisRequest {
                text: state.pronunciation.process_with_language(text, norm_lang),
                profile: profile.clone(),
                speed: target_speed,
                pitch: 0.0,
                format: payload.response_format,
                language: eff_lang_opt.clone(),
                instruct: payload.instruct.clone(),
                num_step: None,
                seed: payload.seed,
                precision: payload.precision,
            };
            let (chunk, opt_f32) = eng.synthesize_cloned_hq(&cloned_req).await.map_err(|e| {
                let err = ProblemDetails {
                    problem_type: "https://voxforge.org/errors/synthesis-failure".to_string(),
                    title: "Voice Conversion Synthesis Failed".to_string(),
                    status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                    detail: e.to_string(),
                    instance: "/v1/audio/speech-to-speech".to_string(),
                };
                (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
            })?;
            let bytes = if let (Some(f32_buf), true) = (opt_f32.as_ref(), prec > 16) {
                WavEncoder::encode_f32_to_wav_with_precision(
                    f32_buf,
                    chunk.sample_rate,
                    chunk.channels,
                    prec,
                )
            } else {
                WavEncoder::encode_pcm_to_wav_with_precision(
                    &chunk.pcm_data,
                    chunk.sample_rate,
                    chunk.channels,
                    prec,
                )
            }
            .map_err(|e| {
                let err = ProblemDetails {
                    problem_type: "https://voxforge.org/errors/encoding-failure".to_string(),
                    title: "Audio Encoding Error".to_string(),
                    status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                    detail: e.to_string(),
                    instance: "/v1/audio/speech-to-speech".to_string(),
                };
                (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
            })?;
            (profile.id, bytes)
        } else {
            synthesize_standard_sts(
                &state,
                &payload,
                text,
                target_speed,
                eff_lang_opt.as_deref(),
                prec,
            )
            .await?
        }
    } else {
        synthesize_standard_sts(
            &state,
            &payload,
            text,
            target_speed,
            eff_lang_opt.as_deref(),
            prec,
        )
        .await?
    };

    let mut response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "audio/wav")
        .body(Body::from(wav_bytes))
        .map_err(|e| {
            let err = ProblemDetails {
                problem_type: "https://voxforge.org/errors/internal".to_string(),
                title: "Response Build Failed".to_string(),
                status: 500,
                detail: e.to_string(),
                instance: "/v1/audio/speech-to-speech".to_string(),
            };
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        })?;

    response.headers_mut().insert(
        header::HeaderName::from_static("x-voxforge-source-duration-sec"),
        HeaderValue::from_str(&format!("{:.2}", measured_duration))
            .unwrap_or_else(|_| HeaderValue::from_static("0.00")),
    );
    response.headers_mut().insert(
        header::HeaderName::from_static("x-voxforge-target-voice"),
        HeaderValue::from_str(&target_voice_id)
            .unwrap_or_else(|_| HeaderValue::from_static("unknown")),
    );

    Ok(response)
}

async fn synthesize_standard_sts(
    state: &AppState,
    payload: &SpeechToSpeechRequest,
    text: &str,
    target_speed: f32,
    eff_lang: Option<&str>,
    prec: u16,
) -> Result<(String, Vec<u8>), (StatusCode, Json<ProblemDetails>)> {
    let (engine, voice) = state
        .engine_registry
        .resolve_voice(&payload.target_voice)
        .await
        .map_err(|e| {
            let err = ProblemDetails {
                problem_type: "https://voxforge.org/errors/voice-not-found".to_string(),
                title: "Target Voice Not Found".to_string(),
                status: StatusCode::NOT_FOUND.as_u16(),
                detail: e.to_string(),
                instance: "/v1/audio/speech-to-speech".to_string(),
            };
            (StatusCode::NOT_FOUND, Json(err))
        })?;

    let synth_req = SynthesisRequest {
        text: state
            .pronunciation
            .process_with_language(text, eff_lang.or(Some(voice.language.as_str()))),
        voice_id: voice.id.clone(),
        speed: target_speed,
        pitch: 0.0,
        format: payload.response_format,
    };

    let chunk = state
        .engine_registry
        .synthesize_cached(engine, &synth_req)
        .await
        .map_err(|e| {
            let err = ProblemDetails {
                problem_type: "https://voxforge.org/errors/synthesis-failure".to_string(),
                title: "Voice Conversion Synthesis Failed".to_string(),
                status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                detail: e.to_string(),
                instance: "/v1/audio/speech-to-speech".to_string(),
            };
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        })?;

    let wav_bytes = WavEncoder::encode_pcm_to_wav_with_precision(
        &chunk.pcm_data,
        chunk.sample_rate,
        chunk.channels,
        prec,
    )
    .map_err(|e| {
        let err = ProblemDetails {
            problem_type: "https://voxforge.org/errors/encoding-failure".to_string(),
            title: "Audio Encoding Error".to_string(),
            status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
            detail: e.to_string(),
            instance: "/v1/audio/speech-to-speech".to_string(),
        };
        (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
    })?;

    Ok((voice.id, wav_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_b64_standard() {
        let res = decode_b64("SGVsbG8gV29ybGQ=");
        assert_eq!(res, Some(b"Hello World".to_vec()));
    }

    #[test]
    fn test_decode_b64_url_safe() {
        // - and _ in place of + and /
        let res = decode_b64("-_==");
        assert!(res.is_some());
    }

    #[test]
    fn test_decode_b64_whitespace_and_padding() {
        let res = decode_b64("  SGVsbG8g\nV29ybGQ= \n");
        assert_eq!(res, Some(b"Hello World".to_vec()));
    }

    #[test]
    fn test_decode_b64_empty_and_invalid() {
        assert_eq!(decode_b64(""), None);
        assert_eq!(decode_b64("   "), None);
    }

    #[test]
    fn test_decode_audio_wav_payload() {
        let wav_bytes = WavEncoder::encode_pcm16_to_wav(&[100, 200, -200, -100], 16000, 1).unwrap();
        let b64 = format!("data:audio/wav;base64,{}", hex_sim_or_b64(&wav_bytes));
        let (samples, sr) = decode_audio(&b64).unwrap();
        assert_eq!(sr, 16000);
        assert_eq!(samples.len(), 4);
        assert_eq!(samples, vec![100, 200, -200, -100]);
    }

    #[test]
    fn test_decode_audio_raw_pcm_fallback() {
        // 2 raw 16-bit samples: 512, 1024
        let pcm_bytes = vec![0x00, 0x02, 0x00, 0x04];
        let b64 = hex_sim_or_b64(&pcm_bytes);
        let (samples, sr) = decode_audio(&b64).unwrap();
        assert_eq!(sr, 16000);
        assert_eq!(samples, vec![512, 1024]);
    }

    #[test]
    fn test_decode_audio_stereo_wav_downmix() {
        // Interleaved stereo samples: (100, 200), (-100, -200) -> mono: (150, -150)
        let wav_bytes = WavEncoder::encode_pcm16_to_wav(&[100, 200, -100, -200], 16000, 2).unwrap();
        let b64 = format!("data:audio/wav;base64,{}", hex_sim_or_b64(&wav_bytes));
        let (samples, sr) = decode_audio(&b64).unwrap();
        assert_eq!(sr, 16000);
        assert_eq!(samples.len(), 2);
        assert_eq!(samples, vec![150, -150]);
    }

    #[test]
    fn test_decode_audio_odd_raw_pcm_rejected() {
        // 3 bytes cannot form 16-bit PCM
        let odd_pcm = vec![0x00, 0x02, 0x00];
        let b64 = hex_sim_or_b64(&odd_pcm);
        assert!(decode_audio(&b64).is_err());
    }

    #[test]
    fn test_decode_audio_empty_err() {
        assert!(decode_audio("").is_err());
        assert!(decode_audio("   ").is_err());
    }

    fn hex_sim_or_b64(bytes: &[u8]) -> String {
        const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let b0 = chunk[0] as u32;
            let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
            let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
            let n = (b0 << 16) | (b1 << 8) | b2;
            out.push(CHARS[((n >> 18) & 63) as usize] as char);
            out.push(CHARS[((n >> 12) & 63) as usize] as char);
            if chunk.len() > 1 {
                out.push(CHARS[((n >> 6) & 63) as usize] as char);
            } else {
                out.push('=');
            }
            if chunk.len() > 2 {
                out.push(CHARS[(n & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
        out
    }
}
