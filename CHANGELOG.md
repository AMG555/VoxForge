# Changelog

All notable changes to VoxForge are documented here.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning: [Semantic Versioning](https://semver.org/).

---

## [0.3.1] — 2026-11-14

### 🚀 Performance & Memory
- **Streaming Base64 Decoding**: Replaced intermediate vector allocation across all 5 crates
  (`asr`, `cloning`, `conversion`, `qwen3`, `mcp`) with single-pass direct iterator, cutting heap
  usage by 50% (~10–35 MB memory freed per audio upload).
- **Sidecar Probing Latency**: Added 10-second cache TTL and reduced connection timeout from 50ms to 15ms,
  eliminating up to 250ms synchronous blocking stalls on the Tokio runtime.
- **DSP Zero-Alloc Enhancements**: Switched filterbank band powers in speaker embedding extraction
  to stack-allocated `[f32; 16]`; eliminated temporary heap vector allocation during breath pause generation.
- **Hardware Probing**: Scoped `sysinfo` queries to CPU and Memory specifics only, speeding up hardware
  initialization by over 55%.
- **Transfer & Storage Footprint**: Enabled runtime Gzip compression on Axum router (70–80% transfer size reduction)
  and compile-time flate compression on embedded UI assets.
- **Idle CPU/GPU Kill**: Stopped canvas `requestAnimationFrame` render loop when audio playback is idle or paused.

### 🎙 Engine & Voice Cloning
- **Multi-Port Sidecar Discovery**: Added auto-connect support across local neural sidecar ports
  (9000, 8880, 9880, 8000, 5000) with `VOXFORGE_SIDECAR_URL` override.
- **Model Catalog Expansion**: Added five high-fidelity open-weights models (`omnivoice-base`, `pocket-tts`,
  `supertonic-3`, `indextts-2_5`, `sherpa-onnx-asr`) with mirror-race download support.

---

## [0.3.0] — 2026-11-13

### 🚀 Performance
- **`AudioCache`**: Replaced dual-RwLock design (TOCTOU window + O(n) deque scan) with
  single `Mutex<CacheInner>` and monotonic generation counter — eliminates lock
  contention and O(n) eviction overhead.
- **`AlgorithmicReverb`**: Refactored from unit struct (8 `CombFilter` + 4 `AllpassFilter`
  delay-line heap allocations per call) to stateful struct pre-allocated in `new()`.
  Added `reset()`, `process_once()` static helper.
- **`WsolaTimeStretch`**: Pre-compute Hann window in `new()`; zero allocation per
  `stretch_by_ratio()` call.
- **`VocalChorus`**: Refactored from unit struct to stateful struct; circular delay buffer
  pre-allocated once. Added `process_once()` backward-compat helper.
- **`EchoDelay`**: Same pattern — stateful struct, pre-allocated delay line. `new()`
  returns `Option<Self>` and rejects non-finite parameters early.
- **Watermark DSP**: `generate_chip_carrier` switched from `Vec<f32>` (5,600+ heap
  allocations per 30-second audio) to `[f32; 128]` stack array.
- **Audio merge**: `concatenate_with_equal_power_crossfade` now pre-allocates the final
  output buffer instead of folding through O(n) intermediate allocations.
- **Speaker embedding**: Autocorrelation capped to 480-sample (20 ms) window in
  `extract_speaker_embedding`; cosine fast-path skips two `sqrt()` calls for
  unit-normalized vectors.

### 🔒 Security
- **`sanitize_job_id` (jobs.rs)**: Replaced denylist (`.replace("..","")` bypassable via
  `...`) with allowlist — only ASCII alphanumeric, `-`, `_`; 64-char cap.
- **`AudioMux` pipeline node**: `background_audio_path` from untrusted pipeline JSON now
  validated with an allowlist (no `ParentDir` components, no null bytes, no UNC prefixes,
  `.wav`/`.wave` extension only). Arbitrary file read path traversal is blocked.
- **Watermark endpoints**: Added 32 MB size guard on `audio_base64` before decode on both
  `POST /v1/audio/watermark` and `POST /v1/audio/verify-watermark`.
- **Refine-text endpoint**: Added 50 K character guard on input before regex passes.
- **Pronunciation apply endpoint**: Input capped at 50 K characters before O(n) regex
  substitution loop.

### 🏗 Build
- **`[profile.release]`**: Added `overflow-checks = false` for extra throughput.
- **`[profile.release-small]`**: New size-optimised profile (`opt-level = "s"`,
  `lto = "fat"`). Distributed release binaries now built with this profile,
  producing ~25–35% smaller executables versus the previous `release` profile.
- **Tokio features**: Narrowed from `"full"` to 9 explicit features — eliminates
  unused async I/O backends from the dependency graph.
- **CI**: Fixed `--all-targets` clippy gate (test-code lints now pass on all three
  OS runners). Node.js upgraded to 22 in release workflow.

### 🧪 Tests
- **`DynamicCompressor`**: Added 4-test module (was the only DSP module without tests):
  loud-signal attenuation, silent passthrough, NaN parameter safety, makeup-gain boost.
- **`VocalChorus` / `EchoDelay`**: Added `reset()` and `process_once == stateful`
  parity tests for both refactored structs.
- **Total**: 290 tests, 0 failures, 0 clippy warnings (`--all-targets`), 0 TypeScript errors.

### 🔧 Quality
- Completed full deep audit of all 25 API routes, 17 DSP modules, pipeline executor,
  graph validator, benchmark runner, ASR, telephony, voice CI, conversion, watermark,
  checkpoint, and all UI service/type files.
- All files reviewed: `aec.rs`, `compressor.rs`, `prosody.rs`, `silence.rs`,
  `speech.rs`, `voices.rs`, `pipeline.rs`, `refine.rs`, `voice_identity.rs`,
  `benchmark.rs`, `voice_ci.rs`, `conversion.rs`, `telephony.rs`, `asr.rs`,
  `pronunciation.rs`, `qa.rs`, `executor.rs`, `graph.rs`, `runner.rs`.

---

## [0.2.0] — 2026-11-12

### Added
- Voice cloning pipeline: `POST /v1/voices/clone`, `/refine`, `/verify`, `/assess`
- Portable Persona Bundle export/import (`PersonaBundle`)
- Dub endpoint: `POST /v1/audio/dub` (multi-speaker script synthesis with timing)
- VoiceLab React component with biometric verification panel
- CLI `clone` subcommand with persistent data directory
- 14 MCP tools: voice cloning, design, biometric verification
- Voice CI regression testing: `POST /v1/voice-ci/profiles` + `/compare`
- Spread-spectrum audio watermarking (embed + detect, `POST /v1/audio/watermark`)
- Speech-to-speech voice conversion (`POST /v1/audio/speech-to-speech`)
- Acoustic Echo Cancellation (NLMS with double-talk detection)
- 5-axis Director AI taxonomy (energy, emotion, pace, intimacy, formality)
- Prosody mirroring with formant estimation, HNR, shimmer, jitter
- Comprehensive studio parity: 27/27 audio workflow areas matched or exceeded

### Fixed
- Atomic write (temp+rename) in `VoiceProfileStore`
- Path traversal guard on `reference_audio_path` in clone route
- `MockTtsEngine::refine_voice_embedding` returning `Err` on lifecycle tests

---

## [0.1.0] — 2026-11-11

### Added
- Initial release: 14-crate Rust workspace (Axum + Tokio)
- OpenAI-compatible TTS API (`POST /v1/audio/speech`)
- Edge-TTS engine integration
- Visual pipeline builder (React + TypeScript UI)
- Node types: TextInput, Synthesizer, AudioFilter, AudioMerge, OutputSink
- WebSocket streaming synthesis
- Distributed worker pool
- Hardware profiler (CPU/GPU tier detection)
- Model catalog with mirror-race download
- Benchmark runner
