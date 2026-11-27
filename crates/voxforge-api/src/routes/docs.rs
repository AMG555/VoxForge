use axum::{
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

const FAVICON_ICO: &[u8] = include_bytes!("../../../../assets/voxforge.ico");

pub async fn favicon_handler() -> impl IntoResponse {
    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_static("image/x-icon"),
        )
        .header(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=86400"),
        )
        .body(axum::body::Body::from(FAVICON_ICO))
        .unwrap()
}

pub async fn scalar_docs_html() -> impl IntoResponse {
    let html = r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>VoxForge API Reference | Neural Speech & Voice Cloning</title>
    <link rel="icon" type="image/x-icon" href="/favicon.ico" />
    <style>
      :root { color-scheme: dark; }
      body { margin: 0; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, monospace, sans-serif; background: #0b0e14; color: #f0f4f8; }
      .vf-shell { max-width: 1080px; margin: 0 auto; padding: 28px 20px 60px; }
      .vf-head { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 12px; border-bottom: 1px solid #242e3d; padding-bottom: 16px; margin-bottom: 20px; }
      .vf-title { font-size: 20px; font-weight: 700; color: #f59e0b; margin: 0; }
      .vf-sub { font-size: 12px; color: #94a3b8; margin: 4px 0 0; }
      .vf-btn { background: #1a222d; color: #cbd5e1; border: 1px solid #3b485c; padding: 7px 12px; border-radius: 6px; font-size: 12px; cursor: pointer; text-decoration: none; display: inline-flex; align-items: center; gap: 6px; }
      .vf-btn:hover { border-color: #f59e0b; color: #fff; }
      .vf-card { background: #121820; border: 1px solid #242e3d; border-radius: 10px; margin-bottom: 12px; overflow: hidden; }
      .vf-row { padding: 12px 16px; display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 10px; cursor: pointer; }
      .vf-method { font-size: 11px; font-weight: 700; padding: 3px 8px; border-radius: 4px; text-transform: uppercase; }
      .get { background: rgba(16,185,129,.15); color: #34d399; border: 1px solid rgba(16,185,129,.35); }
      .post { background: rgba(245,158,11,.15); color: #fbbf24; border: 1px solid rgba(245,158,11,.35); }
      .delete { background: rgba(244,63,94,.15); color: #fb7185; border: 1px solid rgba(244,63,94,.35); }
      .vf-path { font-size: 13px; font-weight: 600; color: #fff; font-family: monospace; }
      .vf-summary { font-size: 12px; color: #94a3b8; }
      .vf-body { border-top: 1px solid #1e2633; padding: 14px 16px; background: #0d1219; display: none; font-size: 12px; }
      .vf-card.open .vf-body { display: block; }
      textarea.vf-json { width: 100%; box-sizing: border-box; background: #080b10; color: #e2e8f0; border: 1px solid #242e3d; border-radius: 6px; padding: 10px; font-family: monospace; font-size: 12px; min-height: 96px; margin-top: 8px; }
      pre.vf-out { background: #080b10; border: 1px solid #242e3d; border-radius: 6px; padding: 10px; font-size: 11px; overflow: auto; max-height: 220px; color: #a7f3d0; }
    </style>
  </head>
  <body>
    <div id="vf-offline-docs" class="vf-shell">
      <div class="vf-head">
        <div>
          <h1 class="vf-title">VoxForge REST &amp; MCP API Reference</h1>
          <p class="vf-sub" id="vf-base-url">Loading OpenAPI 3.1 schema...</p>
        </div>
        <div style="display:flex;gap:8px;">
          <a class="vf-btn" href="/openapi.json" target="_blank">OpenAPI JSON</a>
          <button class="vf-btn" id="vf-scalar-toggle" type="button">Scalar UI</button>
        </div>
      </div>
      <div id="vf-endpoints"></div>
    </div>
    <div id="vf-scalar-mount" style="display:none;">
      <script id="api-reference" data-url="/openapi.json" data-configuration='{"theme":"purple","layout":"modern","darkMode":true}'></script>
    </div>
    <script>
      document.getElementById('vf-scalar-toggle').onclick = function() {
        document.getElementById('vf-offline-docs').style.display = 'none';
        document.getElementById('vf-scalar-mount').style.display = 'block';
        if (!document.getElementById('vf-scalar-cdn')) {
          const s = document.createElement('script');
          s.id = 'vf-scalar-cdn';
          s.src = 'https://cdn.jsdelivr.net/npm/@scalar/api-reference';
          document.body.appendChild(s);
        }
      };
      (async function() {
        const origin = window.location.origin;
        document.getElementById('vf-base-url').textContent = 'Live Server: ' + origin + ' · Interactive zero-dependency explorer';
        try {
          const res = await fetch('/openapi.json');
          const spec = await res.json();
          const root = document.getElementById('vf-endpoints');
          const entries = Object.entries(spec.paths || {});
          entries.forEach(([path, methods]) => {
            Object.entries(methods).forEach(([method, op]) => {
              const card = document.createElement('div');
              card.className = 'vf-card';
              const reqSchema = op.requestBody?.content?.['application/json']?.schema;
              const sample = reqSchema?.example ? JSON.stringify(reqSchema.example, null, 2) : '{}';
              card.innerHTML = `
                <div class="vf-row">
                  <div style="display:flex;align-items:center;gap:10px;flex-wrap:wrap;">
                    <span class="vf-method ${method}">${method}</span>
                    <span class="vf-path">${path}</span>
                  </div>
                  <span class="vf-summary">${op.summary || ''}</span>
                </div>
                <div class="vf-body">
                  <p style="margin:0 0 8px;color:#cbd5e1;">${op.description || ''}</p>
                  ${method === 'post' ? `<label style="color:#94a3b8;font-size:11px;">JSON Request Body</label><textarea class="vf-json">${sample}</textarea>` : ''}
                  <div style="margin-top:10px;display:flex;align-items:center;gap:8px;">
                    <button class="vf-btn vf-try" type="button">Try Request</button>
                    <span class="vf-status" style="color:#94a3b8;"></span>
                  </div>
                  <pre class="vf-out" style="display:none;"></pre>
                </div>`;
              card.querySelector('.vf-row').onclick = () => card.classList.toggle('open');
              card.querySelector('.vf-try').onclick = async () => {
                const statusEl = card.querySelector('.vf-status');
                const outEl = card.querySelector('.vf-out');
                statusEl.textContent = 'Sending...';
                try {
                  const opts = { method: method.toUpperCase(), headers: {} };
                  if (method === 'post') {
                    opts.headers['Content-Type'] = 'application/json';
                    opts.body = card.querySelector('textarea.vf-json').value;
                  }
                  const r = await fetch(origin + path, opts);
                  statusEl.textContent = r.status + ' ' + r.statusText;
                  outEl.style.display = 'block';
                  const ct = r.headers.get('content-type') || '';
                  if (ct.includes('audio/') || ct.includes('octet-stream')) {
                    const blob = await r.blob();
                    outEl.textContent = 'Binary response: ' + blob.size + ' bytes (' + ct + ')';
                  } else {
                    const txt = await r.text();
                    try { outEl.textContent = JSON.stringify(JSON.parse(txt), null, 2); }
                    catch { outEl.textContent = txt.slice(0, 4000); }
                  }
                } catch (e) {
                  statusEl.textContent = 'Error: ' + e.message;
                }
              };
              root.appendChild(card);
            });
          });
        } catch (err) {
          document.getElementById('vf-endpoints').innerHTML = '<p style="color:#f87171;">Failed loading /openapi.json: ' + err.message + '</p>';
        }
      })();
    </script>
  </body>
</html>"#;

    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/html; charset=utf-8"),
        )
        .body(axum::body::Body::from(html))
        .unwrap()
}

pub async fn openapi_spec() -> Json<serde_json::Value> {
    let spec = json!({
        "openapi": "3.1.0",
        "info": {
            "title": "VoxForge Neural Audio Engine API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Studio-grade neural speech synthesis, modular DSP audio graph pipelines, and automated A/B voice quality testing."
        },
        "servers": [
            {
                "url": "http://localhost:8080",
                "description": "Local VoxForge Server"
            }
        ],
        "tags": [
            { "name": "Speech", "description": "OpenAI-compatible speech synthesis" },
            { "name": "Pipeline", "description": "Modular DSP and audio graph processing" },
            { "name": "Quality Assurance", "description": "A/B voice testing and objective audio quality metrics" },
            { "name": "Models", "description": "Installed and supported neural TTS engines" },
            { "name": "Voices", "description": "Available voice profiles and acoustic properties" },
            { "name": "Health", "description": "System health and readiness diagnostics" }
        ],
        "paths": {
            "/health": {
                "get": {
                    "tags": ["Health"],
                    "summary": "Liveness probe",
                    "description": "Quick check to verify server process is alive.",
                    "responses": {
                        "200": {
                            "description": "Server is healthy",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "status": { "type": "string", "example": "healthy" },
                                            "version": { "type": "string", "example": env!("CARGO_PKG_VERSION") }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/health/ready": {
                "get": {
                    "tags": ["Health"],
                    "summary": "Readiness probe",
                    "description": "Verifies registered engines, hardware acceleration status, and storage readiness.",
                    "responses": {
                        "200": {
                            "description": "Server is ready to accept synthesis traffic",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "status": { "type": "string", "example": "ready" },
                                            "version": { "type": "string", "example": env!("CARGO_PKG_VERSION") },
                                            "registered_engines": {
                                                "type": "array",
                                                "items": { "type": "string" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/v1/models": {
                "get": {
                    "tags": ["Models"],
                    "summary": "List TTS models",
                    "description": "Lists all available neural TTS models and engines.",
                    "security": [{ "BearerAuth": [] }],
                    "responses": {
                        "200": {
                            "description": "List of available models",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/ModelListResponse" }
                                }
                            }
                        },
                        "401": { "$ref": "#/components/responses/Unauthorized" }
                    }
                }
            },
            "/v1/voices": {
                "get": {
                    "tags": ["Voices"],
                    "summary": "List voice profiles",
                    "description": "Retrieves available voices with optional filtering by language code and engine.",
                    "security": [{ "BearerAuth": [] }],
                    "parameters": [
                        {
                            "name": "language",
                            "in": "query",
                            "description": "Filter by language code (e.g., 'en-US')",
                            "required": false,
                            "schema": { "type": "string" }
                        },
                        {
                            "name": "engine",
                            "in": "query",
                            "description": "Filter by TTS engine ID",
                            "required": false,
                            "schema": { "type": "string" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Voice catalog",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/VoicesResponse" }
                                }
                            }
                        },
                        "401": { "$ref": "#/components/responses/Unauthorized" }
                    }
                }
            },
            "/v1/audio/speech": {
                "post": {
                    "tags": ["Speech"],
                    "summary": "Synthesize speech",
                    "description": "Generate high-fidelity audio from text using neural synthesis models (OpenAI-compatible).",
                    "security": [{ "BearerAuth": [] }],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/OpenAiSpeechRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Generated audio stream (RIFF WAV format)",
                            "content": {
                                "audio/wav": {
                                    "schema": {
                                        "type": "string",
                                        "format": "binary"
                                    }
                                }
                            }
                        },
                        "400": { "$ref": "#/components/responses/BadRequest" },
                        "401": { "$ref": "#/components/responses/Unauthorized" },
                        "404": { "$ref": "#/components/responses/NotFound" },
                        "500": { "$ref": "#/components/responses/ServerError" }
                    }
                }
            },
            "/v1/pipeline/execute": {
                "post": {
                    "tags": ["Pipeline"],
                    "summary": "Execute audio graph pipeline",
                    "description": "Executes a directed acyclic audio processing graph supporting synthesis, DSP filtering (silence trimming, 3-band parametric EQ, envelope dynamic compression, brickwall limiting), normalization, and multi-track audio merging.",
                    "security": [{ "BearerAuth": [] }],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/PipelineExecuteRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Mastered audio buffer (RIFF WAV format)",
                            "content": {
                                "audio/wav": {
                                    "schema": {
                                        "type": "string",
                                        "format": "binary"
                                    }
                                }
                            }
                        },
                        "400": { "$ref": "#/components/responses/BadRequest" },
                        "401": { "$ref": "#/components/responses/Unauthorized" },
                        "500": { "$ref": "#/components/responses/ServerError" }
                    }
                }
            },
            "/v1/qa/ab-test": {
                "post": {
                    "tags": ["Quality Assurance"],
                    "summary": "Run A/B voice model evaluation",
                    "description": "Synthesizes identical text across two candidate configurations and computes latency, SNR, PESQ proxy score, clipping ratios, and duration metrics with automated recommendation.",
                    "security": [{ "BearerAuth": [] }],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/AbTestScenario" }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "A/B comparison results and metrics",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/AbTestComparison" }
                                }
                            }
                        },
                        "400": { "$ref": "#/components/responses/BadRequest" },
                        "401": { "$ref": "#/components/responses/Unauthorized" },
                        "500": { "$ref": "#/components/responses/ServerError" }
                    }
                }
            },
            "/v1/voices/clone": {
                "post": {
                    "tags": ["Voice Cloning"],
                    "summary": "Clone voice from reference audio",
                    "description": "Zero-shot neural voice cloning from reference audio sample (Base64 payload or validated file path). Extracts 512-D phase-invariant acoustic speaker embedding, vocal tract formants, and prosodic DNA.",
                    "security": [{ "BearerAuth": [] }],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "required": ["name", "engine_id"],
                                    "properties": {
                                        "name": { "type": "string", "example": "Custom Voice Profile" },
                                        "engine_id": { "type": "string", "example": "qwen3-tts" },
                                        "reference_audio_base64": { "type": "string", "description": "Base64-encoded WAV/MP3 audio sample" },
                                        "reference_audio_path": { "type": "string", "description": "Local path to clean WAV reference audio" },
                                        "reference_transcript": { "type": "string", "description": "Optional exact text transcript of reference clip" },
                                        "language": { "type": "string", "default": "en-US" },
                                        "gender": { "type": "string", "enum": ["female", "male", "neutral"] }
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "201": {
                            "description": "Voice profile successfully cloned and persisted",
                            "content": { "application/json": { "schema": { "type": "object" } } }
                        },
                        "400": { "$ref": "#/components/responses/BadRequest" },
                        "401": { "$ref": "#/components/responses/Unauthorized" }
                    }
                }
            },
            "/v1/voices/verify": {
                "post": {
                    "tags": ["Voice Cloning"],
                    "summary": "Verify speaker identity and consistency",
                    "description": "Calculates cosine similarity between reference speaker embedding and test audio. Evaluates pitch alignment, vocal tract timbre match, and biometric consistency confidence rating.",
                    "security": [{ "BearerAuth": [] }],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "required": ["test_audio_base64"],
                                    "properties": {
                                        "test_audio_base64": { "type": "string", "description": "Base64-encoded test audio" },
                                        "reference_audio_base64": { "type": "string", "description": "Base64-encoded reference audio" },
                                        "voice_profile_id": { "type": "string", "description": "ID of stored voice profile" }
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Voice verification similarity score and diagnostic metrics",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "similarity_score": { "type": "number", "example": 0.94 },
                                            "is_match": { "type": "boolean", "example": true },
                                            "confidence_rating": { "type": "string", "example": "high" },
                                            "reference_f0_hz": { "type": "number", "example": 142.5 },
                                            "test_f0_hz": { "type": "number", "example": 144.1 },
                                            "pitch_match_percent": { "type": "number", "example": 98.8 },
                                            "timbre_match_percent": { "type": "number", "example": 94.0 },
                                            "recommendations": { "type": "array", "items": { "type": "string" } }
                                        }
                                    }
                                }
                            }
                        },
                        "400": { "$ref": "#/components/responses/BadRequest" },
                        "401": { "$ref": "#/components/responses/Unauthorized" },
                        "404": { "$ref": "#/components/responses/NotFound" }
                    }
                }
            },
            "/v1/audio/transcriptions": {
                "post": {
                    "tags": ["Speech"],
                    "summary": "Transcribe audio to text (Whisper / ASR)",
                    "description": "Transcribes base64-encoded WAV/PCM audio using connected Whisper neural sidecar or local ASR engine.",
                    "security": [{ "BearerAuth": [] }],
                    "responses": {
                        "200": { "description": "Transcription result" },
                        "400": { "$ref": "#/components/responses/BadRequest" }
                    }
                }
            },
            "/v1/audio/refine": {
                "post": {
                    "tags": ["Speech"],
                    "summary": "Strip verbal fillers & Whisper loops from text",
                    "description": "Cleans filler words (um, uh, like) and repetition artifacts from dictated transcripts.",
                    "security": [{ "BearerAuth": [] }],
                    "responses": { "200": { "description": "Refined transcript" } }
                }
            },
            "/v1/catalog/models": {
                "get": {
                    "tags": ["Models"],
                    "summary": "List catalog models with live hardware fit scores",
                    "description": "Returns downloadable models scored for local CPU/RAM/GPU capabilities.",
                    "responses": { "200": { "description": "Catalog entries" } }
                }
            },
            "/v1/voices/profiles": {
                "get": {
                    "tags": ["Voice Cloning"],
                    "summary": "List persisted zero-shot voice cloning profiles",
                    "description": "Returns all cloned speaker profiles stored on disk.",
                    "responses": { "200": { "description": "Cloned voice profiles" } }
                }
            },
            "/v1/system/runtimes": {
                "get": {
                    "tags": ["Health"],
                    "summary": "Inspect local Python, Piper, and sidecar status",
                    "description": "Reports detected Python executables, installed packages, native Piper CLI, and active sidecars.",
                    "responses": { "200": { "description": "Runtime diagnostics" } }
                }
            },
            "/mcp": {
                "post": {
                    "tags": ["Model Context Protocol"],
                    "summary": "Execute Model Context Protocol (MCP) JSON-RPC 2.0 requests",
                    "description": "Streamable HTTP endpoint for AI agents (Claude Desktop, Cursor, Antigravity, Windsurf) to execute tools, read resources, and inspect prompts over JSON-RPC 2.0.",
                    "security": [{ "BearerAuth": [] }, { "ApiKeyAuth": [] }],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "required": ["jsonrpc", "method"],
                                    "properties": {
                                        "jsonrpc": { "type": "string", "example": "2.0" },
                                        "id": { "oneOf": [{ "type": "integer" }, { "type": "string" }], "example": 1 },
                                        "method": { "type": "string", "example": "tools/call" },
                                        "params": { "type": "object" }
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "JSON-RPC 2.0 Response",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "jsonrpc": { "type": "string", "example": "2.0" },
                                            "id": { "oneOf": [{ "type": "integer" }, { "type": "string" }] },
                                            "result": { "type": "object" }
                                        }
                                    }
                                }
                            }
                        },
                        "204": { "description": "Notification acknowledged without response" },
                        "401": { "$ref": "#/components/responses/Unauthorized" }
                    }
                },
                "get": {
                    "tags": ["Model Context Protocol"],
                    "summary": "Model Context Protocol (MCP) Server-Sent Events (SSE) stream",
                    "description": "SSE subscription endpoint for real-time MCP agent connectivity.",
                    "security": [{ "BearerAuth": [] }, { "ApiKeyAuth": [] }],
                    "responses": {
                        "200": {
                            "description": "SSE event stream emitting MCP endpoint registration",
                            "content": { "text/event-stream": {} }
                        },
                        "401": { "$ref": "#/components/responses/Unauthorized" }
                    }
                }
            }
        },
        "components": {
            "securitySchemes": {
                "BearerAuth": {
                    "type": "http",
                    "scheme": "bearer",
                    "bearerFormat": "API Key",
                    "description": "API key passed as Bearer token in the Authorization header"
                },
                "ApiKeyAuth": {
                    "type": "apiKey",
                    "in": "header",
                    "name": "x-api-key",
                    "description": "API key passed in x-api-key header or ?api_key query parameter"
                }
            },
            "responses": {
                "BadRequest": {
                    "description": "Invalid parameter or payload (RFC 7807 Problem Details)",
                    "content": {
                        "application/json": {
                            "schema": { "$ref": "#/components/schemas/ProblemDetails" }
                        }
                    }
                },
                "Unauthorized": {
                    "description": "Authentication token missing or invalid",
                    "content": {
                        "application/json": {
                            "schema": { "$ref": "#/components/schemas/ProblemDetails" }
                        }
                    }
                },
                "NotFound": {
                    "description": "Requested resource not found",
                    "content": {
                        "application/json": {
                            "schema": { "$ref": "#/components/schemas/ProblemDetails" }
                        }
                    }
                },
                "ServerError": {
                    "description": "Internal server execution failure",
                    "content": {
                        "application/json": {
                            "schema": { "$ref": "#/components/schemas/ProblemDetails" }
                        }
                    }
                }
            },
            "schemas": {
                "OpenAiSpeechRequest": {
                    "type": "object",
                    "required": ["model", "input", "voice"],
                    "properties": {
                        "model": { "type": "string", "example": "kokoro" },
                        "input": { "type": "string", "example": "Welcome to VoxForge studio neural voice synthesis." },
                        "voice": { "type": "string", "example": "af_bella" },
                        "response_format": {
                            "type": "string",
                            "enum": ["wav", "mp3", "ogg", "flac"],
                            "default": "wav"
                        },
                        "speed": { "type": "number", "minimum": 0.25, "maximum": 4.0, "default": 1.0 },
                        "pitch": { "type": "number", "minimum": -50.0, "maximum": 50.0, "default": 0.0 }
                    }
                },
                "PipelineExecuteRequest": {
                    "type": "object",
                    "required": ["pipeline"],
                    "properties": {
                        "pipeline": { "$ref": "#/components/schemas/PipelineDefinition" },
                        "input_text": { "type": "string", "nullable": true }
                    }
                },
                "PipelineDefinition": {
                    "type": "object",
                    "required": ["id", "name", "nodes", "edges"],
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "name": { "type": "string", "example": "Broadcast Voice Mastering" },
                        "description": { "type": "string", "nullable": true },
                        "nodes": {
                            "type": "array",
                            "items": { "$ref": "#/components/schemas/PipelineNode" }
                        },
                        "edges": {
                            "type": "array",
                            "items": { "$ref": "#/components/schemas/PipelineEdge" }
                        }
                    }
                },
                "PipelineNode": {
                    "type": "object",
                    "required": ["id", "name", "node_type", "params"],
                    "properties": {
                        "id": { "type": "string", "example": "node_filter_1" },
                        "name": { "type": "string", "example": "Studio DSP Mastering" },
                        "node_type": {
                            "type": "string",
                            "enum": ["text_input", "synthesizer", "audio_filter", "audio_merger", "output_sink"]
                        },
                        "params": {
                            "type": "object",
                            "description": "Node parameters. For audio_filter: silence_trim, eq, compressor, limiter, normalize_peak, gain_db."
                        }
                    }
                },
                "PipelineEdge": {
                    "type": "object",
                    "required": ["id", "from_node", "to_node"],
                    "properties": {
                        "id": { "type": "string", "example": "edge_1" },
                        "from_node": { "type": "string", "example": "node_synth_1" },
                        "to_node": { "type": "string", "example": "node_filter_1" }
                    }
                },
                "AbTestScenario": {
                    "type": "object",
                    "required": ["name", "text", "variant_a", "variant_b"],
                    "properties": {
                        "name": { "type": "string", "example": "Bella vs Nicole Model Test" },
                        "text": { "type": "string", "example": "High quality neural speech sample for comparison." },
                        "variant_a": {
                            "type": "object",
                            "properties": {
                                "voice_id": { "type": "string" },
                                "speed": { "type": "number" },
                                "pitch": { "type": "number" }
                            }
                        },
                        "variant_b": {
                            "type": "object",
                            "properties": {
                                "voice_id": { "type": "string" },
                                "speed": { "type": "number" },
                                "pitch": { "type": "number" }
                            }
                        }
                    }
                },
                "AbTestComparison": {
                    "type": "object",
                    "properties": {
                        "scenario_name": { "type": "string" },
                        "variant_a": { "$ref": "#/components/schemas/VariantResult" },
                        "variant_b": { "$ref": "#/components/schemas/VariantResult" },
                        "recommended_variant": { "type": "string", "example": "Variant A" },
                        "recommendation_rationale": { "type": "string" }
                    }
                },
                "VariantResult": {
                    "type": "object",
                    "properties": {
                        "variant_name": { "type": "string" },
                        "latency_ms": { "type": "number" },
                        "rtf": { "type": "number" },
                        "audio_metrics": { "$ref": "#/components/schemas/AudioMetrics" }
                    }
                },
                "AudioMetrics": {
                    "type": "object",
                    "properties": {
                        "duration_seconds": { "type": "number" },
                        "sample_rate": { "type": "integer" },
                        "peak_amplitude": { "type": "number" },
                        "rms_db": { "type": "number" },
                        "estimated_snr_db": { "type": "number" },
                        "clipping_ratio": { "type": "number" },
                        "pesq_proxy_score": { "type": "number" }
                    }
                },
                "ModelListResponse": {
                    "type": "object",
                    "properties": {
                        "object": { "type": "string", "example": "list" },
                        "data": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "id": { "type": "string" },
                                    "object": { "type": "string" },
                                    "created": { "type": "integer" },
                                    "owned_by": { "type": "string" }
                                }
                            }
                        }
                    }
                },
                "VoicesResponse": {
                    "type": "object",
                    "properties": {
                        "voices": {
                            "type": "array",
                            "items": { "$ref": "#/components/schemas/Voice" }
                        },
                        "total": { "type": "integer" }
                    }
                },
                "Voice": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string" },
                        "name": { "type": "string" },
                        "engine_id": { "type": "string" },
                        "language": { "type": "string" },
                        "gender": { "type": "string" },
                        "sample_rate_hz": { "type": "integer" },
                        "tags": { "type": "array", "items": { "type": "string" } },
                        "description": { "type": "string", "nullable": true }
                    }
                },
                "ProblemDetails": {
                    "type": "object",
                    "required": ["type", "title", "status", "detail", "instance"],
                    "properties": {
                        "type": { "type": "string", "example": "https://voxforge.org/errors/invalid-parameter" },
                        "title": { "type": "string", "example": "Invalid Speed Parameter" },
                        "status": { "type": "integer", "example": 400 },
                        "detail": { "type": "string", "example": "The 'speed' parameter must be a finite number between 0.25 and 4.0" },
                        "instance": { "type": "string", "example": "/v1/audio/speech" }
                    }
                }
            }
        }
    });

    Json(spec)
}
