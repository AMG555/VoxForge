use futures::{SinkExt, StreamExt};
use std::io::Cursor;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::traits::SynthesisRequest;
use voxforge_core::error::{Result, VoxForgeError};
use voxforge_core::models::AudioChunk;

pub const TRUSTED_CLIENT_TOKEN: &str = "6A5AA1D4EAFF4E9FB37E23D68491D6F4";
pub const SEC_MS_GEC_VERSION: &str = "2-143.0.3650.75";
pub const EDGE_WSS_HOST: &str = "speech.platform.bing.com";
pub const SAMPLE_RATE: u32 = 24000;
const WIN_EPOCH: u64 = 11644473600;

static CLOCK_SKEW_SECONDS: AtomicI64 = AtomicI64::new(0);

const SHA256_INITIAL: [u32; 8] = [
    0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A, 0x510E527F, 0x9B05688C, 0x1F83D9AB, 0x5BE0CD19,
];
const SHA256_STATE: [u32; 8] = [
    0xFD9FBEA2, 0x91243CE7, 0xBE499094, 0xCD2E1E0A, 0x6C006F49, 0x04BD35AB, 0xFEB0A7E4, 0x03EFFBD2,
];
const SHA256_SCHEDULE: [u32; 14] = [
    0xEB24BF9E, 0x1558691F, 0xA0C09EE2, 0x0CFA32BA, 0xA622EE8C, 0xD1DD9491, 0xC0DD25A0, 0xF9E70AB6,
    0xF0C31573, 0x7058CE4E, 0x7188E606, 0x00E5AB33, 0xD258B060, 0x72766564,
];
const SHA256_ROUND_CONSTANTS: [u32; 64] = [
    0x428A2F98, 0x71374491, 0xB5C0FBCF, 0xE9B5DBA5, 0x3956C25B, 0x59F111F1, 0x923F82A4, 0xAB1C5ED5,
    0xD807AA98, 0x12835B01, 0x243185BE, 0x550C7DC3, 0x72BE5D74, 0x80DEB1FE, 0x9BDC06A7, 0xC19BF174,
    0xE49B69C1, 0xEFBE4786, 0x0FC19DC6, 0x240CA1CC, 0x2DE92C6F, 0x4A7484AA, 0x5CB0A9DC, 0x76F988DA,
    0x983E5152, 0xA831C66D, 0xB00327C8, 0xBF597FC7, 0xC6E00BF3, 0xD5A79147, 0x06CA6351, 0x14292967,
    0x27B70A85, 0x2E1B2138, 0x4D2C6DFC, 0x53380D13, 0x650A7354, 0x766A0ABB, 0x81C2C92E, 0x92722C85,
    0xA2BFE8A1, 0xA81A664B, 0xC24B8B70, 0xC76C51A3, 0xD192E819, 0xD6990624, 0xF40E3585, 0x106AA070,
    0x19A4C116, 0x1E376C08, 0x2748774C, 0x34B0BCB5, 0x391C0CB3, 0x4ED8AA4A, 0x5B9CCA4F, 0x682E6FF3,
    0x748F82EE, 0x78A5636F, 0x84C87814, 0x8CC70208, 0x90BEFFFA, 0xA4506CEB, 0xBEF9A3F7, 0xC67178F2,
];
const SHA256_ADJUSTMENT: u32 = 0xC9DC06A7;
const SHA256_SIGMA0_ADJUSTMENT: u32 = 0x059C0B80;
const SHA256_SIGMA1_ADJUSTMENT: u32 = 0x000B92C0;

const CS64_COEFFICIENTS: [(u32, u32); 14] = [
    (0xE82E91B1, 0xB32700D0),
    (0x44DE24C6, 0x88DFF51D),
    (0x3BEB65BC, 0x29514038),
    (0xC89C5933, 0xB328CBDF),
    (0x8D1EC211, 0xD405B4D5),
    (0x314EF5D5, 0x26E55F3C),
    (0xE92491B1, 0x607801C6),
    (0xF0A5B9D5, 0x82B2C1FB),
    (0x06064CF3, 0xFA51A3DA),
    (0xDA79FA60, 0x207CC464),
    (0xBED20560, 0x3C3C74F3),
    (0x89F9C9F8, 0xD2D14B61),
    (0x35FA38C6, 0xBB04A2C1),
    (0xCB49E3C6, 0xC5626167),
];
const CS64_FACTOR: u32 = 0x82893933;
const CS64_ADJUSTMENT0: u32 = 0xF2F90B1A;
const CS64_ADJUSTMENT1: u32 = 0x0B1A0000;

#[inline]
fn rol16(value: u32) -> u32 {
    value.rotate_left(16)
}

fn words_le4_even(data: &[u8]) -> Vec<u32> {
    let rem = (4 - (data.len() % 4)) % 4;
    let mut padded = Vec::with_capacity(data.len() + rem + 4);
    padded.extend_from_slice(data);
    padded.resize(data.len() + rem, 0);

    let mut words = Vec::with_capacity(padded.len() / 4 + 1);
    for chunk in padded.chunks_exact(4) {
        words.push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    if words.len() % 2 != 0 {
        words.push(0);
    }
    words
}

#[inline]
fn sigma0(value: u32) -> u32 {
    value.rotate_right(7) ^ value.rotate_right(18) ^ (value >> 3)
}

#[inline]
fn sigma1(value: u32) -> u32 {
    value.rotate_right(17) ^ value.rotate_right(19) ^ (value >> 10)
}

fn expand_sha256_schedule(words: &mut Vec<u32>, first_word: usize) {
    for index in first_word..64 {
        let x = words[index - 2];
        let y = words[index - 15];
        let s1 = sigma1(x);
        let s0 = sigma0(y);
        words.push(
            s1.wrapping_add(words[index - 7])
                .wrapping_add(s0)
                .wrapping_add(words[index - 16]),
        );
    }
}

fn sha256_rounds(state: [u32; 8], words: &[u32], first_round: usize) -> [u32; 8] {
    let mut a = state[0];
    let mut b = state[1];
    let mut c = state[2];
    let mut d = state[3];
    let mut e = state[4];
    let mut f = state[5];
    let mut g = state[6];
    let mut h = state[7];

    for index in first_round..64 {
        let k = if first_round == 14 && index == 14 {
            SHA256_ADJUSTMENT
        } else {
            SHA256_ROUND_CONSTANTS[index]
        };
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ (!e & g);
        let t1 = h
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(k)
            .wrapping_add(words[index]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(maj);

        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }
    [a, b, c, d, e, f, g, h]
}

fn sha256_init(data: &[u8]) -> [u32; 8] {
    assert!(
        data.len() >= 7,
        "Token salt must contain at least seven bytes"
    );
    let mut words = vec![0u32; 14];
    words.push(u32::from_be_bytes([0, data[0], data[1], data[2]]));
    words.push(u32::from_be_bytes([data[3], data[4], data[5], data[6]]));

    for index in 16..30 {
        let mut val = SHA256_SCHEDULE[index - 16];
        if index - 2 >= 14 {
            val = val.wrapping_add(if index == 16 {
                sigma1(words[index - 2]) ^ SHA256_SIGMA1_ADJUSTMENT
            } else {
                sigma1(words[index - 2])
            });
        }
        if index - 7 >= 14 {
            val = val.wrapping_add(words[index - 7]);
        }
        if index - 15 >= 14 {
            val = val.wrapping_add(if index == 29 {
                sigma0(words[index - 15]) ^ SHA256_SIGMA0_ADJUSTMENT
            } else {
                sigma0(words[index - 15])
            });
        }
        words.push(val);
    }

    let val30 = sigma1(words[28])
        .wrapping_add(words[23])
        .wrapping_add(sigma0(words[15]))
        .wrapping_add(words[14])
        .wrapping_add(SHA256_ADJUSTMENT)
        .wrapping_sub(SHA256_ROUND_CONSTANTS[14]);
    words.push(val30);

    expand_sha256_schedule(&mut words, 31);
    let working = sha256_rounds(SHA256_STATE, &words, 14);
    let mut out = [0u32; 8];
    for i in 0..8 {
        out[i] = SHA256_INITIAL[i].wrapping_add(working[i]);
    }
    out
}

fn sha256_digest(data: &[u8]) -> [u8; 32] {
    let length = 57 + data.len();
    let mut padded = Vec::with_capacity(data.len() + 64);
    padded.extend_from_slice(&data[7..]);
    padded.push(0x80);

    let rem = (55 + 64 - (length % 64)) % 64;
    padded.resize(padded.len() + rem, 0);

    let bit_len = (length as u64) * 8;
    padded.extend_from_slice(&bit_len.to_be_bytes());

    let mut state = sha256_init(&data[..7]);
    for chunk in padded.chunks_exact(64) {
        let mut words = Vec::with_capacity(64);
        for w in chunk.chunks_exact(4) {
            words.push(u32::from_be_bytes([w[0], w[1], w[2], w[3]]));
        }
        expand_sha256_schedule(&mut words, 16);
        let working = sha256_rounds(state, &words, 0);
        for i in 0..8 {
            state[i] = state[i].wrapping_add(working[i]);
        }
    }

    let mut out = [0u8; 32];
    for (i, word) in state.iter().enumerate() {
        out[i * 4..(i + 1) * 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

fn cs64_digest(data: &[u8], digest: &[u8; 32]) -> (u32, u32) {
    let key_a = u32::from_le_bytes([digest[0], digest[1], digest[2], digest[3]]) | 1;
    let key_b = u32::from_le_bytes([digest[4], digest[5], digest[6], digest[7]]) | 1;
    let factors = [
        key_a.wrapping_mul(CS64_FACTOR),
        key_b.wrapping_mul(CS64_FACTOR),
    ];

    let mut terms = Vec::with_capacity(CS64_COEFFICIENTS.len() + data.len() / 2 + 10);
    let mut offsets = Vec::with_capacity(CS64_COEFFICIENTS.len() + data.len() / 2 + 10);

    for (index, &(coef, off)) in CS64_COEFFICIENTS.iter().enumerate() {
        terms.push(coef.wrapping_mul(factors[index % 2]));
        offsets.push(off);
    }

    let mut padded_data = Vec::with_capacity(data.len() + 1);
    padded_data.push(0);
    padded_data.extend_from_slice(data);

    let words = words_le4_even(&padded_data);
    for (index, word) in words.into_iter().enumerate() {
        let mut term = word.wrapping_mul(if index % 2 == 0 { key_a } else { key_b });
        let mut offset = rol16(word).wrapping_mul(if index % 2 == 0 {
            0xEF0569FB
        } else {
            0xC31713DB
        });
        if index == 0 {
            term = term.wrapping_add(CS64_ADJUSTMENT0.wrapping_mul(factors[0]));
            offset = offset.wrapping_add(CS64_ADJUSTMENT1);
        }
        terms.push(term);
        offsets.push(offset);
    }

    let mut swap_acc: u32 = 0;
    let mut swap_last: u32 = 0;
    let mut state: u32 = 0;
    let mut acc: u32 = 0;

    for i in (0..terms.len()).step_by(2) {
        let x = terms[i];
        let y = terms[i + 1];

        let mut t = rol16(offsets[i].wrapping_add(x));
        let mut q = t.wrapping_mul(0x76F50F34);
        swap_acc = swap_acc.wrapping_add(rol16(q).wrapping_mul(0xC3EFEA97).wrapping_add(q));

        t = rol16(offsets[i + 1].wrapping_add(y));
        q = t.wrapping_mul(0x1881CE3C);
        swap_last = rol16(q).wrapping_mul(0x35BD1EC9).wrapping_add(q);
        swap_acc = swap_acc.wrapping_add(swap_last);

        state = state.wrapping_mul(key_a).wrapping_add(x);
        state = rol16(state).wrapping_mul(0xCF98B111);
        state = rol16(state).wrapping_mul(0x87085B9F);
        state = rol16(state).wrapping_mul(0x12CEB96D);
        state = rol16(state).wrapping_mul(0x257E1D83);
        acc = acc.wrapping_add(state);

        t = state.wrapping_mul(key_b).wrapping_add(y);
        t = rol16(t).wrapping_mul(0xA27416F5);
        t = rol16(t).wrapping_mul(0xD38396FF);
        state = rol16(rol16(t).wrapping_mul(0x7C932B89)).wrapping_mul(0xBFA49F69);
        acc = acc.wrapping_add(state);
    }

    (swap_last ^ state, swap_acc ^ acc)
}

/// Generate time-windowed Sec-MS-GEC v2 token at specific unix timestamp
pub fn generate_sec_ms_gec_at(unix_timestamp: f64) -> String {
    let mut ticks = unix_timestamp + WIN_EPOCH as f64;
    ticks -= ticks % 300.0;
    ticks *= 10_000_000.0;

    let str_to_hash = format!("{:.0}{}", ticks, TRUSTED_CLIENT_TOKEN);
    let digest = sha256_digest(str_to_hash.as_bytes());
    let (cs0, cs1) = cs64_digest(str_to_hash.as_bytes(), &digest);

    let mut cs64_bytes = [0u8; 8];
    cs64_bytes[0..4].copy_from_slice(&cs0.to_le_bytes());
    cs64_bytes[4..8].copy_from_slice(&cs1.to_le_bytes());

    let cs64_hex = hex::encode(cs64_bytes).to_uppercase();
    let sha256_hex = hex::encode(digest).to_uppercase();
    format!("{}-{}", cs64_hex, sha256_hex)
}

/// Adjust local clock skew in seconds relative to server Date header
pub fn adjust_clock_skew(skew_seconds: i64) {
    CLOCK_SKEW_SECONDS.fetch_add(skew_seconds, Ordering::Relaxed);
}

/// Generate time-windowed Sec-MS-GEC v2 token required by Microsoft Edge TTS WebSocket
pub fn generate_sec_ms_gec() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    let skew = CLOCK_SKEW_SECONDS.load(Ordering::Relaxed) as f64;
    generate_sec_ms_gec_at(now + skew)
}

pub fn generate_muid() -> String {
    let id = Uuid::new_v4().simple().to_string();
    id.to_uppercase()
}

pub struct EdgeTtsClient;

impl EdgeTtsClient {
    /// Build WebSocket connection URL with valid anti-abuse headers
    fn build_url(connection_id: &str) -> String {
        let sec_ms_gec = generate_sec_ms_gec();
        format!(
            "wss://{}/consumer/speech/synthesize/readaloud/edge/v1?TrustedClientToken={}&ConnectionId={}&Sec-MS-GEC={}&Sec-MS-GEC-Version={}",
            EDGE_WSS_HOST,
            TRUSTED_CLIENT_TOKEN,
            connection_id,
            sec_ms_gec,
            SEC_MS_GEC_VERSION
        )
    }

    /// Synthesize speech and stream AudioChunk instances through channel
    pub async fn stream_synthesis(
        request: &SynthesisRequest,
        ssml: &str,
        tx: mpsc::Sender<Result<AudioChunk>>,
    ) -> Result<()> {
        let chunk = Self::synthesize(request, ssml).await?;
        let chunk_size = 4800; // 200ms at 24kHz
        let mut offset = 0;
        let total = chunk.pcm_data.len();

        while offset < total {
            let end = (offset + chunk_size).min(total);
            let is_final = end >= total;
            let slice = chunk.pcm_data[offset..end].to_vec();

            let sub_chunk = AudioChunk {
                sample_rate: chunk.sample_rate,
                channels: chunk.channels,
                pcm_data: slice,
                is_final,
            };

            if tx.send(Ok(sub_chunk)).await.is_err() {
                break;
            }
            offset = end;
        }

        Ok(())
    }

    /// Direct synthesis returning single AudioChunk, retrying transient WebSocket resets.
    pub async fn synthesize(request: &SynthesisRequest, ssml: &str) -> Result<AudioChunk> {
        const MAX_ATTEMPTS: usize = 3;
        let mut last_err = None;

        for attempt in 1..=MAX_ATTEMPTS {
            match Self::synthesize_once(request, ssml).await {
                Ok(mp3_bytes) => {
                    let (sample_rate, channels, pcm_data) = decode_mp3_to_pcm(&mp3_bytes)?;
                    return Ok(AudioChunk {
                        sample_rate,
                        channels,
                        pcm_data,
                        is_final: true,
                    });
                }
                Err(err) if attempt < MAX_ATTEMPTS => {
                    let delay_ms = (attempt as u64) * 250;
                    warn!(
                        attempt,
                        max_attempts = MAX_ATTEMPTS,
                        delay_ms,
                        "Transient Edge-TTS synthesis failure ({}); retrying with fresh WebSocket",
                        err
                    );
                    last_err = Some(err);
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
                Err(err) => {
                    last_err = Some(err);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| {
            VoxForgeError::AudioProcessing("Edge TTS synthesis failed after retries".to_string())
        }))
    }

    async fn synthesize_once(_request: &SynthesisRequest, ssml: &str) -> Result<Vec<u8>> {
        let connection_id = Uuid::new_v4().simple().to_string();
        let request_id = Uuid::new_v4().simple().to_string();
        let url = Self::build_url(&connection_id);
        debug!("Connecting to Edge TTS WebSocket: {}", url);

        let mut req = url.into_client_request().map_err(|e| {
            VoxForgeError::AudioProcessing(format!("Failed to build WebSocket request: {}", e))
        })?;

        let headers = req.headers_mut();
        headers.insert(
            "User-Agent",
            HeaderValue::from_static(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36 Edg/143.0.0.0",
            ),
        );
        headers.insert(
            "Origin",
            HeaderValue::from_static("chrome-extension://jdiccldimpdaibmpdkjnbmckianbfold"),
        );
        headers.insert("Pragma", HeaderValue::from_static("no-cache"));
        headers.insert("Cache-Control", HeaderValue::from_static("no-cache"));
        headers.insert(
            "Accept-Encoding",
            HeaderValue::from_static("gzip, deflate, br, zstd"),
        );
        headers.insert(
            "Accept-Language",
            HeaderValue::from_static("en-US,en;q=0.9"),
        );

        let cookie_val = format!("muid={};", generate_muid());
        if let Ok(hv) = HeaderValue::from_str(&cookie_val) {
            headers.insert("Cookie", hv);
        }

        let (ws_stream, _) = tokio::time::timeout(Duration::from_secs(8), connect_async(req))
            .await
            .map_err(|_| {
                VoxForgeError::AudioProcessing("Edge TTS connection timed out".to_string())
            })?
            .map_err(|e| {
                VoxForgeError::AudioProcessing(format!("Edge TTS WebSocket error: {}", e))
            })?;

        let (mut write, mut read) = ws_stream.split();

        // 1. Send speech.config message requesting 24kHz MP3
        let js_date = chrono::Utc::now()
            .format("%a %b %d %Y %H:%M:%S GMT+0000 (Coordinated Universal Time)")
            .to_string();

        let config_json = serde_json::json!({
            "context": {
                "synthesis": {
                    "audio": {
                        "metadataoptions": {
                            "sentenceBoundaryEnabled": "true",
                            "wordBoundaryEnabled": "false"
                        },
                        "outputFormat": "audio-24khz-48kbitrate-mono-mp3"
                    }
                }
            }
        });

        let config_message = format!(
            "X-Timestamp:{}\r\nContent-Type:application/json; charset=utf-8\r\nPath:speech.config\r\n\r\n{}\r\n",
            js_date, config_json
        );

        debug!("Sending Edge TTS config message");
        write
            .send(Message::Text(config_message))
            .await
            .map_err(|e| {
                VoxForgeError::AudioProcessing(format!("Failed to send speech.config: {}", e))
            })?;

        // 2. Send SSML request (note trailing 'Z' as required by Microsoft Edge speech parser)
        let ssml_message = format!(
            "X-RequestId:{}\r\nContent-Type:application/ssml+xml\r\nX-Timestamp:{}Z\r\nPath:ssml\r\n\r\n{}",
            request_id, js_date, ssml
        );

        debug!("Sending Edge TTS SSML request");
        write.send(Message::Text(ssml_message)).await.map_err(|e| {
            VoxForgeError::AudioProcessing(format!("Failed to send SSML message: {}", e))
        })?;

        // 3. Collect incoming MP3 bytes
        let mut mp3_bytes = Vec::new();

        while let Some(msg_result) = read.next().await {
            let msg = match msg_result {
                Ok(m) => m,
                Err(e) => {
                    warn!("WebSocket read error: {}", e);
                    break;
                }
            };

            match &msg {
                Message::Text(text) => {
                    debug!("Received text message: {}", text);
                    if text.contains("Path:turn.end") {
                        break;
                    }
                }
                Message::Binary(bytes) => {
                    debug!("Received binary frame (len {})", bytes.len());
                    if let Some(audio_data) = parse_binary_audio_payload(bytes) {
                        mp3_bytes.extend_from_slice(audio_data);
                    }
                }
                Message::Close(c) => {
                    if let Some(ref cf) = c {
                        debug!(
                            "WebSocket closed with code: {} ({:?}), reason: {}",
                            u16::from(cf.code),
                            cf.code,
                            cf.reason
                        );
                    }
                    break;
                }
                _ => {}
            }
        }

        if mp3_bytes.is_empty() {
            return Err(VoxForgeError::AudioProcessing(
                "Edge TTS returned empty audio stream".to_string(),
            ));
        }

        Ok(mp3_bytes)
    }
}

/// Parse raw binary WebSocket frame from Edge TTS and extract audio payload
pub fn parse_binary_audio_payload(bytes: &[u8]) -> Option<&[u8]> {
    if bytes.len() < 2 {
        return None;
    }

    let header_len = u16::from_be_bytes([bytes[0], bytes[1]]) as usize;
    let header_end = 2 + header_len;
    if bytes.len() < header_end {
        return None;
    }

    let header_str = std::str::from_utf8(&bytes[2..header_end]).ok()?;
    if !header_str.contains("Path:audio") {
        return None;
    }

    Some(&bytes[header_end..])
}

/// Decode in-memory MP3 bytes into 16-bit PCM samples using pure-Rust Symphonia
pub fn decode_mp3_to_pcm(mp3_bytes: &[u8]) -> Result<(u32, u16, Vec<i16>)> {
    let cursor = Cursor::new(mp3_bytes.to_vec());
    let mss = MediaSourceStream::new(Box::new(cursor), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("mp3");

    let meta_opts = MetadataOptions::default();
    let fmt_opts = FormatOptions::default();

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &fmt_opts, &meta_opts)
        .map_err(|e| VoxForgeError::AudioProcessing(format!("Symphonia probe error: {}", e)))?;

    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| {
            VoxForgeError::AudioProcessing("No valid audio track in MP3 stream".to_string())
        })?;

    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| VoxForgeError::AudioProcessing(format!("Symphonia decoder error: {}", e)))?;

    let mut pcm_samples = Vec::new();
    let mut sample_rate = SAMPLE_RATE;
    let mut channels = 1;

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(ref err))
                if err.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break
            }
            Err(SymphoniaError::ResetRequired) => break,
            Err(e) => {
                return Err(VoxForgeError::AudioProcessing(format!(
                    "Packet read error: {}",
                    e
                )))
            }
        };

        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(audio_buf) => {
                let spec = *audio_buf.spec();
                sample_rate = spec.rate;
                channels = spec.channels.count() as u16;

                let mut sample_buf = SampleBuffer::<i16>::new(audio_buf.capacity() as u64, spec);
                sample_buf.copy_interleaved_ref(audio_buf);
                pcm_samples.extend_from_slice(sample_buf.samples());
            }
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(SymphoniaError::IoError(ref err))
                if err.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break
            }
            Err(SymphoniaError::ResetRequired) => break,
            Err(e) => {
                return Err(VoxForgeError::AudioProcessing(format!(
                    "Decode frame error: {}",
                    e
                )))
            }
        }
    }

    // If decoded audio is multi-channel/stereo, downmix with phase-cancellation guard
    if channels > 1 {
        pcm_samples = voxforge_audio::WavEncoder::downmix_interleaved_to_mono(
            &pcm_samples,
            channels as usize,
        );
        channels = 1;
    }

    Ok((sample_rate, channels, pcm_samples))
}
