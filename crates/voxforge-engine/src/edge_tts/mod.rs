pub mod client;

use async_trait::async_trait;
use tokio::sync::mpsc;
use tracing::warn;
use voxforge_audio::AudioMerger;
use voxforge_core::error::{Result, VoxForgeError};
use voxforge_core::models::{AudioChunk, Gender, Voice};

use crate::traits::{EngineCapabilities, SynthesisRequest, TtsEngine};
pub use client::{
    decode_mp3_to_pcm, generate_sec_ms_gec, parse_binary_audio_payload, EdgeTtsClient,
};

pub struct EdgeTtsEngine {
    _client: reqwest::Client,
}

impl EdgeTtsEngine {
    pub fn new() -> Self {
        Self {
            _client: reqwest::Client::builder()
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0")
                .build()
                .unwrap_or_default(),
        }
    }

    pub fn build_ssml(request: &SynthesisRequest) -> String {
        let speed = if request.speed.is_finite() {
            request.speed.clamp(0.25, 4.0)
        } else {
            1.0
        };
        let rate_pct = ((speed - 1.0) * 100.0).round() as i32;
        let rate_str = if rate_pct >= 0 {
            format!("+{}%", rate_pct)
        } else {
            format!("{}%", rate_pct)
        };

        let pitch = if request.pitch.is_finite() {
            request.pitch.clamp(-50.0, 50.0)
        } else {
            0.0
        };
        let pitch_hz = (pitch * 5.0).round() as i32;
        let pitch_str = if pitch_hz >= 0 {
            format!("+{}Hz", pitch_hz)
        } else {
            format!("{}Hz", pitch_hz)
        };

        let norm_voice = normalize_edge_voice_id(&request.voice_id);
        let formatted_voice = format_edge_voice_name(&norm_voice);
        let xml_lang = extract_locale_from_voice_id(&norm_voice);

        format!(
            r#"<speak version='1.0' xmlns='http://www.w3.org/2001/10/synthesis' xml:lang='{}'><voice name='{}'><prosody pitch='{}' rate='{}' volume='+0%'>{}</prosody></voice></speak>"#,
            quick_xml_escape(&xml_lang),
            quick_xml_escape(&formatted_voice),
            pitch_str,
            rate_str,
            quick_xml_escape(&request.text)
        )
    }
}

/// Map retired or renamed Microsoft Edge-TTS voice identifiers to verified live equivalents.
pub fn normalize_edge_voice_id(voice_id: &str) -> String {
    match voice_id {
        "de-DE-KatjaNeural" => "de-DE-TanjaNeural".to_string(),
        "de-DE-ConradNeural" => "de-DE-ChristophNeural".to_string(),
        "en-AU-WilliamNeural" => "en-AU-WilliamMultilingualNeural".to_string(),
        "en-KE-AsilNeural" => "en-KE-ChilembaNeural".to_string(),
        "en-KE-ChiamaNeural" => "en-KE-AsiliaNeural".to_string(),
        "es-EC-AndresNeural" => "es-EC-LuisNeural".to_string(),
        "ca-ES-AlbaNeural" => "ca-ES-JoanaNeural".to_string(),
        "zu-ZA-ThandekileNeural" => "zu-ZA-ThembaNeural".to_string(),
        // Edge-TTS does not publish pa-IN neural voices; fall back to closest Indic Hindi neural voices.
        "pa-IN-VaaniNeural" => "hi-IN-SwaraNeural".to_string(),
        "pa-IN-OjasNeural" => "hi-IN-MadhurNeural".to_string(),
        other => other.to_string(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScriptRunKind {
    Malayalam,
    Latin,
}

fn is_malayalam_char(c: char) -> bool {
    ('\u{0D00}'..='\u{0D7F}').contains(&c)
}

/// Segment text sent to a Malayalam Edge-TTS voice so Latin/English spans route to matching
/// Indian-English voices (`en-IN-PrabhatNeural` / `en-IN-NeerjaExpressiveNeural`) instead of
/// triggering Edge-TTS's Malayalam voice gibberish bug on Latin characters.
pub fn segment_malayalam_latin_runs(voice_id: &str, text: &str) -> Vec<(String, String)> {
    let norm_voice = normalize_edge_voice_id(voice_id);
    let en_in_voice = match norm_voice.as_str() {
        "ml-IN-MidhunNeural" => "en-IN-PrabhatNeural",
        "ml-IN-SobhanaNeural" => "en-IN-NeerjaExpressiveNeural",
        _ => return vec![(norm_voice, text.to_string())],
    };

    let has_ml = text.chars().any(is_malayalam_char);
    let has_latin = text.chars().any(|c| c.is_ascii_alphabetic());

    if !has_ml && has_latin {
        return vec![(en_in_voice.to_string(), text.to_string())];
    }
    if !has_latin {
        return vec![(norm_voice, text.to_string())];
    }

    // Mixed Malayalam + Latin text: group whitespace-delimited tokens into contiguous script runs.
    let mut runs: Vec<(ScriptRunKind, String)> = Vec::new();
    for token in text.split_whitespace() {
        let token_kind = if token.chars().any(is_malayalam_char) {
            Some(ScriptRunKind::Malayalam)
        } else if token.chars().any(|c| c.is_ascii_alphabetic()) {
            Some(ScriptRunKind::Latin)
        } else {
            None
        };

        let effective_kind = token_kind.unwrap_or_else(|| {
            runs.last()
                .map(|(k, _)| *k)
                .unwrap_or(ScriptRunKind::Malayalam)
        });

        if let Some((last_kind, last_buf)) = runs.last_mut() {
            if *last_kind == effective_kind {
                if !last_buf.is_empty() {
                    last_buf.push(' ');
                }
                last_buf.push_str(token);
                continue;
            }
        }
        runs.push((effective_kind, token.to_string()));
    }

    if runs.is_empty() {
        return vec![(norm_voice, text.to_string())];
    }

    runs.into_iter()
        .map(|(kind, chunk)| {
            let v = match kind {
                ScriptRunKind::Malayalam => norm_voice.clone(),
                ScriptRunKind::Latin => en_in_voice.to_string(),
            };
            (v, chunk)
        })
        .collect()
}

fn infer_edge_voice_gender(norm_voice: &str) -> Gender {
    let lower = norm_voice.to_ascii_lowercase();
    if [
        "guy",
        "christopher",
        "andrew",
        "brian",
        "ryan",
        "prabhat",
        "william",
        "midhun",
        "madhur",
        "valluvar",
        "mohan",
        "gagan",
        "bashkar",
        "dhwani_male",
        "niranjan",
        "henri",
        "christoph",
        "alvaro",
        "diego",
        "yunxi",
        "keita",
        "injoon",
        "antonio",
        "hamed",
        "wanlung",
    ]
    .iter()
    .any(|tok| lower.contains(tok))
    {
        Gender::Male
    } else {
        Gender::Female
    }
}

/// Segment text across incompatible Unicode scripts for any Edge-TTS voice so selecting e.g.
/// `en-US-AriaNeural` with Malayalam text routes non-Latin tokens cleanly to a matching neural
/// voice of the same gender instead of returning an empty audio stream from Edge-TTS.
pub fn segment_edge_tts_script_runs(voice_id: &str, text: &str) -> Vec<(String, String)> {
    let norm_voice = normalize_edge_voice_id(voice_id);
    if matches!(
        norm_voice.as_str(),
        "ml-IN-MidhunNeural" | "ml-IN-SobhanaNeural"
    ) {
        return segment_malayalam_latin_runs(&norm_voice, text);
    }

    let voice_locale = extract_locale_from_voice_id(&norm_voice);
    let voice_prefix = voice_locale
        .split('-')
        .next()
        .unwrap_or("en")
        .to_ascii_lowercase();
    let gender = infer_edge_voice_gender(&norm_voice);
    let has_kana = text
        .chars()
        .any(|c| matches!(c as u32, 0x3040..=0x30FF | 0x31F0..=0x31FF));

    let mut runs: Vec<(String, String)> = Vec::new();
    for token in text.split_whitespace() {
        let tok_script = crate::qwen3::detect_script_lang(token);
        let has_latin = token.chars().any(|c| c.is_ascii_alphabetic());

        let target_voice: Option<String> = if !tok_script.is_empty() {
            if tok_script == voice_prefix
                || (tok_script == "zh" && (voice_prefix == "ja" || has_kana))
            {
                Some(norm_voice.clone())
            } else {
                Some(
                    crate::qwen3::resolve_cross_lingual_base_voice(
                        Some(&gender),
                        token,
                        tok_script,
                    )
                    .to_string(),
                )
            }
        } else if has_latin {
            Some(norm_voice.clone())
        } else {
            None
        };

        let resolved = target_voice.unwrap_or_else(|| {
            runs.last()
                .map(|(v, _)| v.clone())
                .unwrap_or_else(|| norm_voice.clone())
        });

        if let Some((last_voice, last_buf)) = runs.last_mut() {
            if *last_voice == resolved {
                if !last_buf.is_empty() {
                    last_buf.push(' ');
                }
                last_buf.push_str(token);
                continue;
            }
        }
        runs.push((resolved, token.to_string()));
    }

    if runs.is_empty() {
        return vec![(norm_voice, text.to_string())];
    }
    runs
}

pub fn format_edge_voice_name(voice_id: &str) -> String {
    if voice_id.starts_with("Microsoft Server Speech") {
        return voice_id.to_string();
    }
    let norm_voice = normalize_edge_voice_id(voice_id);
    let parts: Vec<&str> = norm_voice.split('-').collect();
    if parts.len() >= 3 {
        let lang = parts[0];
        let region = parts[1];
        let name = parts[2..].join("-");
        format!(
            "Microsoft Server Speech Text to Speech Voice ({}-{}, {})",
            lang, region, name
        )
    } else {
        norm_voice
    }
}

pub fn extract_locale_from_voice_id(voice_id: &str) -> String {
    if voice_id.contains('(') && voice_id.contains(',') {
        if let Some(start) = voice_id.find('(') {
            if let Some(comma) = voice_id.find(',') {
                if comma > start {
                    return voice_id[start + 1..comma].trim().to_string();
                }
            }
        }
    }
    let norm_voice = normalize_edge_voice_id(voice_id);
    let parts: Vec<&str> = norm_voice.split('-').collect();
    if parts.len() >= 2 && parts[0].len() >= 2 && parts[1].len() >= 2 {
        format!("{}-{}", parts[0], parts[1])
    } else {
        "en-US".to_string()
    }
}

fn quick_xml_escape(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

impl Default for EdgeTtsEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TtsEngine for EdgeTtsEngine {
    fn id(&self) -> &'static str {
        "edge-tts"
    }

    fn name(&self) -> &'static str {
        "Microsoft Edge TTS Cloud Relay"
    }

    fn is_local(&self) -> bool {
        false
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            cost_per_1k_chars: 0.0, // free tier via browser relay
            avg_latency_ms: 300,
            quality_score: 0.85,
            languages: vec![
                "en-US".to_string(),
                "en-GB".to_string(),
                "en-IN".to_string(),
                "ml-IN".to_string(),
                "hi-IN".to_string(),
                "ta-IN".to_string(),
                "te-IN".to_string(),
                "kn-IN".to_string(),
                "bn-IN".to_string(),
                "mr-IN".to_string(),
                "gu-IN".to_string(),
                "ur-IN".to_string(),
                "de-DE".to_string(),
                "fr-FR".to_string(),
                "es-ES".to_string(),
                "zh-CN".to_string(),
                "ja-JP".to_string(),
            ],
            is_local: false,
            supports_cloning: false,
            supports_streaming: true,
        }
    }

    async fn voices(&self) -> Result<Vec<Voice>> {
        Ok(vec![
            Voice {
                id: "en-US-AriaNeural".to_string(),
                name: "Aria (Neural)".to_string(),
                engine_id: self.id().to_string(),
                language: "en-US".to_string(),
                gender: Gender::Female,
                sample_rate_hz: 24000,
                tags: vec!["conversational".to_string(), "news".to_string()],
                description: Some("Clear, natural American female voice".to_string()),
            },
            Voice {
                id: "en-US-GuyNeural".to_string(),
                name: "Guy (Neural)".to_string(),
                engine_id: self.id().to_string(),
                language: "en-US".to_string(),
                gender: Gender::Male,
                sample_rate_hz: 24000,
                tags: vec!["conversational".to_string(), "friendly".to_string()],
                description: Some("Warm, friendly American male voice".to_string()),
            },
            Voice {
                id: "en-US-JennyNeural".to_string(),
                name: "Jenny (Neural)".to_string(),
                engine_id: self.id().to_string(),
                language: "en-US".to_string(),
                gender: Gender::Female,
                sample_rate_hz: 24000,
                tags: vec!["conversational".to_string(), "expressive".to_string()],
                description: Some("Natural, conversational American female voice".to_string()),
            },
            Voice {
                id: "en-GB-SoniaNeural".to_string(),
                name: "Sonia (Neural)".to_string(),
                engine_id: self.id().to_string(),
                language: "en-GB".to_string(),
                gender: Gender::Female,
                sample_rate_hz: 24000,
                tags: vec!["british".to_string(), "narration".to_string()],
                description: Some("Crisp British English female voice".to_string()),
            },
            Voice {
                id: "en-GB-RyanNeural".to_string(),
                name: "Ryan (Neural)".to_string(),
                engine_id: self.id().to_string(),
                language: "en-GB".to_string(),
                gender: Gender::Male,
                sample_rate_hz: 24000,
                tags: vec!["british".to_string(), "storytelling".to_string()],
                description: Some("Polished British English male voice".to_string()),
            },
            Voice {
                id: "en-IN-NeerjaExpressiveNeural".to_string(),
                name: "Neerja (Indian English Neural)".to_string(),
                engine_id: self.id().to_string(),
                language: "en-IN".to_string(),
                gender: Gender::Female,
                sample_rate_hz: 24000,
                tags: vec![
                    "english".to_string(),
                    "indian-english".to_string(),
                    "expressive".to_string(),
                ],
                description: Some("Natural Indian-English expressive female voice".to_string()),
            },
            Voice {
                id: "en-IN-PrabhatNeural".to_string(),
                name: "Prabhat (Indian English Neural)".to_string(),
                engine_id: self.id().to_string(),
                language: "en-IN".to_string(),
                gender: Gender::Male,
                sample_rate_hz: 24000,
                tags: vec![
                    "english".to_string(),
                    "indian-english".to_string(),
                    "narration".to_string(),
                ],
                description: Some(
                    "Clear Indian-English male voice for narration and tech".to_string(),
                ),
            },
            // --- Indian Languages Neural Voices ---
            Voice {
                id: "ml-IN-SobhanaNeural".to_string(),
                name: "Sobhana (Neural - മലയാളം)".to_string(),
                engine_id: self.id().to_string(),
                language: "ml-IN".to_string(),
                gender: Gender::Female,
                sample_rate_hz: 24000,
                tags: vec![
                    "malayalam".to_string(),
                    "kerala".to_string(),
                    "expressive".to_string(),
                ],
                description: Some(
                    "Warm, expressive Malayalam female voice with natural native cadence"
                        .to_string(),
                ),
            },
            Voice {
                id: "ml-IN-MidhunNeural".to_string(),
                name: "Midhun (Neural - മലയാളം)".to_string(),
                engine_id: self.id().to_string(),
                language: "ml-IN".to_string(),
                gender: Gender::Male,
                sample_rate_hz: 24000,
                tags: vec![
                    "malayalam".to_string(),
                    "kerala".to_string(),
                    "broadcast".to_string(),
                ],
                description: Some(
                    "Crisp, authoritative Malayalam male voice for news and narration".to_string(),
                ),
            },
            Voice {
                id: "hi-IN-SwaraNeural".to_string(),
                name: "Swara (Neural - हिन्दी)".to_string(),
                engine_id: self.id().to_string(),
                language: "hi-IN".to_string(),
                gender: Gender::Female,
                sample_rate_hz: 24000,
                tags: vec!["hindi".to_string(), "conversational".to_string()],
                description: Some(
                    "Natural Hindi female voice with authentic conversational rhythm".to_string(),
                ),
            },
            Voice {
                id: "hi-IN-MadhurNeural".to_string(),
                name: "Madhur (Neural - हिन्दी)".to_string(),
                engine_id: self.id().to_string(),
                language: "hi-IN".to_string(),
                gender: Gender::Male,
                sample_rate_hz: 24000,
                tags: vec!["hindi".to_string(), "storytelling".to_string()],
                description: Some("Deep, engaging Hindi male voice".to_string()),
            },
            Voice {
                id: "ta-IN-PallaviNeural".to_string(),
                name: "Pallavi (Neural - தமிழ்)".to_string(),
                engine_id: self.id().to_string(),
                language: "ta-IN".to_string(),
                gender: Gender::Female,
                sample_rate_hz: 24000,
                tags: vec!["tamil".to_string(), "expressive".to_string()],
                description: Some("Fluent Tamil female voice with crisp articulation".to_string()),
            },
            Voice {
                id: "ta-IN-ValluvarNeural".to_string(),
                name: "Valluvar (Neural - தமிழ்)".to_string(),
                engine_id: self.id().to_string(),
                language: "ta-IN".to_string(),
                gender: Gender::Male,
                sample_rate_hz: 24000,
                tags: vec!["tamil".to_string(), "narration".to_string()],
                description: Some(
                    "Resonant Tamil male voice for stories and broadcast".to_string(),
                ),
            },
            Voice {
                id: "te-IN-ShrutiNeural".to_string(),
                name: "Shruti (Neural - తెలుగు)".to_string(),
                engine_id: self.id().to_string(),
                language: "te-IN".to_string(),
                gender: Gender::Female,
                sample_rate_hz: 24000,
                tags: vec!["telugu".to_string(), "melodic".to_string()],
                description: Some("Melodic Telugu female voice with natural cadence".to_string()),
            },
            Voice {
                id: "te-IN-MohanNeural".to_string(),
                name: "Mohan (Neural - తెలుగు)".to_string(),
                engine_id: self.id().to_string(),
                language: "te-IN".to_string(),
                gender: Gender::Male,
                sample_rate_hz: 24000,
                tags: vec!["telugu".to_string(), "professional".to_string()],
                description: Some("Clear, articulate Telugu male voice".to_string()),
            },
            Voice {
                id: "kn-IN-SapnaNeural".to_string(),
                name: "Sapna (Neural - ಕನ್ನಡ)".to_string(),
                engine_id: self.id().to_string(),
                language: "kn-IN".to_string(),
                gender: Gender::Female,
                sample_rate_hz: 24000,
                tags: vec!["kannada".to_string(), "conversational".to_string()],
                description: Some("Engaging Kannada female voice with smooth flow".to_string()),
            },
            Voice {
                id: "kn-IN-GaganNeural".to_string(),
                name: "Gagan (Neural - ಕನ್ನಡ)".to_string(),
                engine_id: self.id().to_string(),
                language: "kn-IN".to_string(),
                gender: Gender::Male,
                sample_rate_hz: 24000,
                tags: vec!["kannada".to_string(), "broadcast".to_string()],
                description: Some("Warm Kannada male voice".to_string()),
            },
            Voice {
                id: "bn-IN-TanishaaNeural".to_string(),
                name: "Tanishaa (Neural - বাংলা)".to_string(),
                engine_id: self.id().to_string(),
                language: "bn-IN".to_string(),
                gender: Gender::Female,
                sample_rate_hz: 24000,
                tags: vec!["bengali".to_string(), "expressive".to_string()],
                description: Some("Sweet, expressive Bengali female voice".to_string()),
            },
            Voice {
                id: "bn-IN-BashkarNeural".to_string(),
                name: "Bashkar (Neural - বাংলা)".to_string(),
                engine_id: self.id().to_string(),
                language: "bn-IN".to_string(),
                gender: Gender::Male,
                sample_rate_hz: 24000,
                tags: vec!["bengali".to_string(), "narration".to_string()],
                description: Some("Articulate Bengali male voice".to_string()),
            },
            Voice {
                id: "mr-IN-AarohiNeural".to_string(),
                name: "Aarohi (Neural - मराठी)".to_string(),
                engine_id: self.id().to_string(),
                language: "mr-IN".to_string(),
                gender: Gender::Female,
                sample_rate_hz: 24000,
                tags: vec!["marathi".to_string(), "conversational".to_string()],
                description: Some("Polished Marathi female voice".to_string()),
            },
            Voice {
                id: "mr-IN-ManoharNeural".to_string(),
                name: "Manohar (Neural - मराठी)".to_string(),
                engine_id: self.id().to_string(),
                language: "mr-IN".to_string(),
                gender: Gender::Male,
                sample_rate_hz: 24000,
                tags: vec!["marathi".to_string(), "storytelling".to_string()],
                description: Some("Deep Marathi male voice".to_string()),
            },
            Voice {
                id: "gu-IN-DhwaniNeural".to_string(),
                name: "Dhwani (Neural - ગુજરાતી)".to_string(),
                engine_id: self.id().to_string(),
                language: "gu-IN".to_string(),
                gender: Gender::Female,
                sample_rate_hz: 24000,
                tags: vec!["gujarati".to_string(), "expressive".to_string()],
                description: Some("Clear Gujarati female voice".to_string()),
            },
            Voice {
                id: "gu-IN-NiranjanNeural".to_string(),
                name: "Niranjan (Neural - ગુજરાતી)".to_string(),
                engine_id: self.id().to_string(),
                language: "gu-IN".to_string(),
                gender: Gender::Male,
                sample_rate_hz: 24000,
                tags: vec!["gujarati".to_string(), "friendly".to_string()],
                description: Some("Warm Gujarati male voice".to_string()),
            },
            Voice {
                id: "ur-IN-GulNeural".to_string(),
                name: "Gul (Neural - اردو)".to_string(),
                engine_id: self.id().to_string(),
                language: "ur-IN".to_string(),
                gender: Gender::Female,
                sample_rate_hz: 24000,
                tags: vec!["urdu".to_string(), "poetic".to_string()],
                description: Some("Eloquent Urdu female voice".to_string()),
            },
            Voice {
                id: "ur-IN-SalmanNeural".to_string(),
                name: "Salman (Neural - اردو)".to_string(),
                engine_id: self.id().to_string(),
                language: "ur-IN".to_string(),
                gender: Gender::Male,
                sample_rate_hz: 24000,
                tags: vec!["urdu".to_string(), "narration".to_string()],
                description: Some("Poetic and resonant Urdu male voice".to_string()),
            },
            // --- Global Languages ---
            Voice {
                id: "de-DE-TanjaNeural".to_string(),
                name: "Tanja (Neural)".to_string(),
                engine_id: self.id().to_string(),
                language: "de-DE".to_string(),
                gender: Gender::Female,
                sample_rate_hz: 24000,
                tags: vec!["german".to_string(), "broadcast".to_string()],
                description: Some("Standard German female voice".to_string()),
            },
            Voice {
                id: "fr-FR-DeniseNeural".to_string(),
                name: "Denise (Neural)".to_string(),
                engine_id: self.id().to_string(),
                language: "fr-FR".to_string(),
                gender: Gender::Female,
                sample_rate_hz: 24000,
                tags: vec!["french".to_string(), "expressive".to_string()],
                description: Some("Articulate French female voice".to_string()),
            },
            Voice {
                id: "es-ES-AlvaroNeural".to_string(),
                name: "Alvaro (Neural)".to_string(),
                engine_id: self.id().to_string(),
                language: "es-ES".to_string(),
                gender: Gender::Male,
                sample_rate_hz: 24000,
                tags: vec!["spanish".to_string(), "clear".to_string()],
                description: Some("Standard Castilian Spanish male voice".to_string()),
            },
            Voice {
                id: "ja-JP-NanamiNeural".to_string(),
                name: "Nanami (Neural)".to_string(),
                engine_id: self.id().to_string(),
                language: "ja-JP".to_string(),
                gender: Gender::Female,
                sample_rate_hz: 24000,
                tags: vec!["japanese".to_string(), "anime".to_string()],
                description: Some("Pleasant, fluent Japanese female voice".to_string()),
            },
            Voice {
                id: "zh-CN-XiaoxiaoNeural".to_string(),
                name: "Xiaoxiao (Neural)".to_string(),
                engine_id: self.id().to_string(),
                language: "zh-CN".to_string(),
                gender: Gender::Female,
                sample_rate_hz: 24000,
                tags: vec!["chinese".to_string(), "audiobook".to_string()],
                description: Some("Warm, natural Mandarin Chinese female voice".to_string()),
            },
            Voice {
                id: "zh-HK-HiuGaaiNeural".to_string(),
                name: "HiuGaai (Cantonese HK - 粵語)".to_string(),
                engine_id: self.id().to_string(),
                language: "zh-HK".to_string(),
                gender: Gender::Female,
                sample_rate_hz: 24000,
                tags: vec![
                    "cantonese".to_string(),
                    "hong-kong".to_string(),
                    "conversational".to_string(),
                ],
                description: Some("Native Hong Kong Cantonese female neural voice".to_string()),
            },
            Voice {
                id: "zh-HK-WanLungNeural".to_string(),
                name: "WanLung (Cantonese HK - 粵語)".to_string(),
                engine_id: self.id().to_string(),
                language: "zh-HK".to_string(),
                gender: Gender::Male,
                sample_rate_hz: 24000,
                tags: vec![
                    "cantonese".to_string(),
                    "hong-kong".to_string(),
                    "narration".to_string(),
                ],
                description: Some("Native Hong Kong Cantonese male neural voice".to_string()),
            },
            Voice {
                id: "ko-KR-SunHiNeural".to_string(),
                name: "SunHi (Korean - 한국어)".to_string(),
                engine_id: self.id().to_string(),
                language: "ko-KR".to_string(),
                gender: Gender::Female,
                sample_rate_hz: 24000,
                tags: vec!["korean".to_string(), "expressive".to_string()],
                description: Some("Natural Seoul Korean female neural voice".to_string()),
            },
            Voice {
                id: "pt-BR-FranciscaNeural".to_string(),
                name: "Francisca (Brazilian Portuguese)".to_string(),
                engine_id: self.id().to_string(),
                language: "pt-BR".to_string(),
                gender: Gender::Female,
                sample_rate_hz: 24000,
                tags: vec!["portuguese".to_string(), "brazil".to_string()],
                description: Some("Warm Brazilian Portuguese female neural voice".to_string()),
            },
            Voice {
                id: "ar-EG-SalmaNeural".to_string(),
                name: "Salma (Arabic - العربية)".to_string(),
                engine_id: self.id().to_string(),
                language: "ar-EG".to_string(),
                gender: Gender::Female,
                sample_rate_hz: 24000,
                tags: vec!["arabic".to_string(), "narration".to_string()],
                description: Some("Clear Arabic female neural voice".to_string()),
            },
        ])
    }

    async fn synthesize(&self, request: &SynthesisRequest) -> Result<AudioChunk> {
        let runs = segment_edge_tts_script_runs(&request.voice_id, &request.text);
        if runs.len() <= 1 {
            let (seg_voice, seg_text) = runs.into_iter().next().unwrap_or_else(|| {
                (
                    normalize_edge_voice_id(&request.voice_id),
                    request.text.clone(),
                )
            });
            let mut seg_req = request.clone();
            seg_req.voice_id = seg_voice;
            seg_req.text = seg_text;
            let ssml = Self::build_ssml(&seg_req);
            return EdgeTtsClient::synthesize(&seg_req, &ssml)
                .await
                .map_err(|e| {
                    VoxForgeError::Engine(format!("Edge-TTS neural synthesis failed: {}", e))
                });
        }

        let mut segments: Vec<Vec<i16>> = Vec::with_capacity(runs.len());
        let mut sample_rate = 24000u32;
        let mut channels = 1u16;
        for (seg_voice, seg_text) in runs {
            let mut seg_req = request.clone();
            seg_req.voice_id = seg_voice;
            seg_req.text = seg_text;
            let ssml = Self::build_ssml(&seg_req);
            let chunk = EdgeTtsClient::synthesize(&seg_req, &ssml)
                .await
                .map_err(|e| {
                    VoxForgeError::Engine(format!("Edge-TTS neural synthesis failed: {}", e))
                })?;
            sample_rate = chunk.sample_rate;
            channels = chunk.channels;
            if !chunk.pcm_data.is_empty() {
                segments.push(chunk.pcm_data);
            }
        }

        let refs: Vec<&[i16]> = segments.iter().map(|v| v.as_slice()).collect();
        let joined = AudioMerger::silence_gap_join(&refs, sample_rate, 15, 35);
        Ok(AudioChunk {
            sample_rate,
            channels,
            pcm_data: joined,
            is_final: true,
        })
    }

    async fn synthesize_stream(
        &self,
        request: &SynthesisRequest,
    ) -> Result<mpsc::Receiver<Result<AudioChunk>>> {
        let (tx, rx) = mpsc::channel(16);
        let request_clone = request.clone();

        tokio::spawn(async move {
            let engine = EdgeTtsEngine::new();
            match engine.synthesize(&request_clone).await {
                Ok(chunk) => {
                    let chunk_size = 4800; // 200ms at 24kHz
                    let mut offset = 0;
                    let total = chunk.pcm_data.len();
                    while offset < total {
                        let end = (offset + chunk_size).min(total);
                        let sub = AudioChunk {
                            sample_rate: chunk.sample_rate,
                            channels: chunk.channels,
                            pcm_data: chunk.pcm_data[offset..end].to_vec(),
                            is_final: end >= total,
                        };
                        if tx.send(Ok(sub)).await.is_err() {
                            break;
                        }
                        offset = end;
                    }
                }
                Err(e) => {
                    warn!("Live Edge-TTS streaming error: {}", e);
                    let _ = tx.send(Err(e)).await;
                }
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

    #[test]
    fn test_normalize_retired_edge_voice_ids() {
        assert_eq!(
            normalize_edge_voice_id("de-DE-KatjaNeural"),
            "de-DE-TanjaNeural"
        );
        assert_eq!(
            normalize_edge_voice_id("de-DE-ConradNeural"),
            "de-DE-ChristophNeural"
        );
        assert_eq!(
            normalize_edge_voice_id("en-AU-WilliamNeural"),
            "en-AU-WilliamMultilingualNeural"
        );
        assert_eq!(
            normalize_edge_voice_id("pa-IN-VaaniNeural"),
            "hi-IN-SwaraNeural"
        );
        assert_eq!(
            normalize_edge_voice_id("en-US-AriaNeural"),
            "en-US-AriaNeural"
        );
    }

    #[test]
    fn test_segment_malayalam_latin_runs() {
        let en_midhun =
            segment_malayalam_latin_runs("ml-IN-MidhunNeural", "What are you doing for me?");
        assert_eq!(
            en_midhun,
            vec![(
                "en-IN-PrabhatNeural".to_string(),
                "What are you doing for me?".to_string()
            )]
        );

        let en_sobhana = segment_malayalam_latin_runs("ml-IN-SobhanaNeural", "Hello world");
        assert_eq!(
            en_sobhana,
            vec![(
                "en-IN-NeerjaExpressiveNeural".to_string(),
                "Hello world".to_string()
            )]
        );

        let ml_native = segment_malayalam_latin_runs("ml-IN-SobhanaNeural", "നമസ്കാരം സുഖമാണോ");
        assert_eq!(
            ml_native,
            vec![(
                "ml-IN-SobhanaNeural".to_string(),
                "നമസ്കാരം സുഖമാണോ".to_string()
            )]
        );

        let mixed = segment_malayalam_latin_runs("ml-IN-MidhunNeural", "ഇന്ന് weather വളരെ cold ആണ്");
        assert_eq!(
            mixed,
            vec![
                ("ml-IN-MidhunNeural".to_string(), "ഇന്ന്".to_string()),
                ("en-IN-PrabhatNeural".to_string(), "weather".to_string()),
                ("ml-IN-MidhunNeural".to_string(), "വളരെ".to_string()),
                ("en-IN-PrabhatNeural".to_string(), "cold".to_string()),
                ("ml-IN-MidhunNeural".to_string(), "ആണ്".to_string()),
            ]
        );
    }

    #[test]
    fn test_segment_edge_tts_script_runs_cross_script_fallback() {
        let aria_ml = segment_edge_tts_script_runs("en-US-AriaNeural", "ഇപ്പോൾ നിങ്ങൾ സുഖമാണോ");
        assert_eq!(
            aria_ml,
            vec![(
                "ml-IN-SobhanaNeural".to_string(),
                "ഇപ്പോൾ നിങ്ങൾ സുഖമാണോ".to_string()
            )]
        );

        let guy_mixed = segment_edge_tts_script_runs("en-US-GuyNeural", "Hello ഇന്ന് weather നല്ലതാണ്");
        assert_eq!(
            guy_mixed,
            vec![
                ("en-US-GuyNeural".to_string(), "Hello".to_string()),
                ("ml-IN-MidhunNeural".to_string(), "ഇന്ന്".to_string()),
                ("en-US-GuyNeural".to_string(), "weather".to_string()),
                ("ml-IN-MidhunNeural".to_string(), "നല്ലതാണ്".to_string()),
            ]
        );
    }
}
