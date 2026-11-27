# VoxForge TTS Engines & Open-Weight Model Guide

VoxForge routes speech synthesis, voice cloning, transcription, and voice activity detection across pure-Rust native engines and optional Python neural sidecars. Every curated model and engine links directly to its official upstream documentation and exposes pros, cons, download size, RAM/VRAM targets, and live compatibility checks in **Model Catalog**, **Hardware & Engines**, and **Voice Lab**.

---

## Core Synthesis Engines (Hardware & Engines / Voice Lab)

| Engine ID | Engine Name | Footprint & Runtime | Min RAM / VRAM | Best For | Pros | Cons | Official Docs |
|---|---|---|---|---|---|---|---|
| `edge-tts` | Microsoft Edge Cloud Relay | `0 MB` local · Zero Python (Rust WebSocket) | `128 MB` RAM · CPU Only | Zero-setup instant multilingual speech (Malayalam, Hindi, Arabic, Cantonese, English) | Instant boot, 130+ languages, studio prosody on any laptop | Needs internet; not air-gapped offline | [Microsoft Neural TTS Docs](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/language-support?tabs=tts) |
| `piper-tts` | Piper Local Neural ONNX | `~22 MB` CLI + `~60 MB`/voice · Zero Python | `512 MB` RAM · CPU Only | Air-gapped offline narration & low-latency CPU voice agents | 100% offline deterministic VITS ONNX; faster than real-time on CPU | Single-speaker preset per weight; no zero-shot cloning | [Piper Official Repository](https://github.com/rhasspy/piper) |
| `kokoro` | Kokoro-82M Neural Engine | `310 MB` + Optional Kokoro Python sidecar (`:8880`) | `2.0 GB` RAM · CPU or Entry GPU | Audiobooks & podcasts needing permissive Apache-2.0 quality | Top naturalness at just 82M params; fast on CPU & 4 GB GPUs | Uses curated voicepacks rather than open reference cloning | [Kokoro-82M Model Card](https://huggingface.co/hexgrad/Kokoro-82M) |
| `qwen3-tts` | Zero-Shot Voice Cloning & Neural Sidecar | `0 MB` native Rust · `0.5–3.3 GB` optional PyTorch sidecars (`:9000`) | `1–4 GB` RAM · `4 GB` VRAM ideal | Custom 3s cloned voices, 646-lang cross-lingual dubbing & Director controls | Native Rust mode works zero-Python; upgrades automatically to OmniVoice/Chatterbox/F5-TTS when sidecar runs | True PyTorch neural diffusion sidecars need ~4 GB+ CUDA/MPS VRAM for real-time speed | [OmniVoice Official Page](https://huggingface.co/k2-fsa/OmniVoice) |
| `openai-router` | OpenAI & OpenRouter Cloud Gateway | `0 MB` local (BYO API Key) | `128 MB` RAM · Cloud | Hybrid production fallback & OpenAI commercial voices | Zero local CPU/GPU load | Paid metered API; no local cloning | [OpenAI TTS Docs](https://platform.openai.com/docs/guides/text-to-speech) |
| `mock-tts` | In-Memory Synthesis Test Harness | `0 MB` built-in Rust | `64 MB` RAM | Automated CI & DSP pipeline diagnostics only | Sub-ms deterministic WAV output | Emits 440 Hz test tone, not human speech | [Architecture Docs](architecture.md) |

---

## Curated Downloadable Catalog Models (`GET /v1/catalog/models`)

| Catalog ID | Model Name | Format · Size | Min RAM · Rec VRAM | System Tier Fit | Pros | Cons | Best For | Official Model Card |
|---|---|---|---|---|---|---|---|---|
| `piper-en-lessac-medium` (+6 voices) | Piper ONNX Voices (`en-US`, `en-GB`, `es-ES`, `fr-FR`, `de-DE`) | ONNX · `27–60 MB` | `0.5 GB` · CPU | Every machine (incl. Tier 1 Minimal) | Zero Python via native Piper CLI; tiny size; real-time CPU | Preset single speakers only | Offline desktop/embedded assistants | [rhasspy/piper-voices](https://huggingface.co/rhasspy/piper-voices) |
| `kokoro-v0_19` | Kokoro 82M Multilingual ONNX | ONNX · `310 MB` | `2.0 GB` · CPU/GPU | 4 GB+ RAM CPU or entry GPU | Strong Apache-2.0 prosody & multi-voice blending | No reference cloning; needs Kokoro package | Commercial narration & audiobooks | [onnx-community/Kokoro-82M-v1.0-ONNX](https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX) |
| `pocket-tts` | PocketTTS Real-Time CPU Voice | Safetensors · `225 MB` | `0.5 GB` · CPU | CPU-only laptops | Very fast CPU time-to-first-audio; 225 MB | Excludes cloning in this checkpoint; European langs only | Live CPU dictation & readers | [kyutai/pocket-tts-without-voice-cloning](https://huggingface.co/kyutai/pocket-tts-without-voice-cloning) |
| `kitten-tts-2` | KittenTTS 2 Lightweight Engine | PyTorch · `510 MB` | `1.0 GB` · CPU | CPU-only or 8 GB laptops | Compact in-context cloning + tags without CUDA | English-focused; less nuance than 500M+ GPU models | Private CPU English synthesis | [KittenML/kitten-tts-2](https://huggingface.co/KittenML/kitten-tts-2) |
| `canto-tts-nano` | Canto-TTS Nano (HK Cantonese) | ONNX · `421 MB` | `1.0 GB` · CPU | CPU-only machines | ~84% Cantonese tone accuracy + mixed HK English on CPU | Cantonese & English specialized | Offline Hong Kong Cantonese speech | [typangaa/canto-tts-nano](https://huggingface.co/typangaa/canto-tts-nano) |
| `supertonic-3` | Supertonic High-Fidelity Predictor | ONNX · `2 MB` | `1.0 GB` · CPU | Any CPU host | Very fast ONNX duration prediction | Component weight only; OpenRail-M terms | Fast ONNX timing pipelines | [Supertone/supertonic](https://huggingface.co/Supertone/supertonic) |
| `chatterbox-turbo` | Chatterbox-Turbo 350M | Safetensors · `1.8 GB` | `4.0 GB` · `4.0 GB` VRAM | GPU >= 4 GB CUDA/MPS | Sub-150ms TTFB, 5s cloning, 19 tags (`[laugh]`, `[sigh]`), MIT license | English-only; slow on pure CPU | Real-time English voice agents | [ResembleAI/chatterbox-turbo](https://huggingface.co/ResembleAI/chatterbox-turbo) |
| `chatterbox-multilingual` | Chatterbox 500M Multilingual | Safetensors · `2.0 GB` | `4.0 GB` · `4.0 GB` VRAM | GPU >= 4 GB CUDA/MPS | 23 languages, emotion exaggeration control, PerTh watermark | Heavier than Turbo; prefers CUDA/MPS | Multilingual expressive cloning & dubbing | [ResembleAI/chatterbox](https://huggingface.co/ResembleAI/chatterbox) |
| `qwen3-tts-base` | Qwen3-TTS 12Hz 0.6B Base | Safetensors · `1.7 GB` | `4.0 GB` · `4.0 GB` VRAM | GPU >= 4 GB CUDA/MPS | Accurate 3s English/Chinese cloning & style steering | English/Mandarin primary; large checkpoint | High-fidelity EN/ZH cloning | [Qwen/Qwen3-TTS-12Hz-0.6B-Base](https://huggingface.co/Qwen/Qwen3-TTS-12Hz-0.6B-Base) |
| `omnivoice-base` | OmniVoice 600+ Multilingual | Safetensors · `2.3 GB` | `4.0 GB` · `4.0 GB` VRAM | GPU >= 4 GB CUDA/MPS | Covers 600+ langs & cross-lingual cloning | 2.3 GB size; slow on CPU-only hosts | Indic, Arabic, European & global cloning | [k2-fsa/OmniVoice](https://huggingface.co/k2-fsa/OmniVoice) |
| `f5-tts-v1` | F5-TTS v1 Base Flow-Matching | Safetensors · `1.3 GB` | `4.0 GB` · `4.0 GB` VRAM | GPU >= 4 GB CUDA/MPS | Very smooth non-robotic cadence across paragraphs | CC-BY-NC-4.0 weight license; needs CUDA for speed | Research & non-commercial narration | [SWivid/F5-TTS](https://huggingface.co/SWivid/F5-TTS) |
| `indextts-2_5` | IndexTTS 2 Expressive Emotion | PyTorch · `3.3 GB` | `4.0 GB` · `6.0 GB` VRAM | Workstation GPU (6+ GB VRAM) | Top-tier emotion separation & dubbing duration control | Largest 3.3 GB footprint; batch-slow on 4 GB / CPU | Dramatic character acting & film dubbing | [IndexTeam/IndexTTS-2](https://huggingface.co/IndexTeam/IndexTTS-2) |
| `whisper-base` / `whisper-small` / `sherpa-onnx-asr` | Whisper Base · Small · Sherpa Int8 | ONNX · `79` / `336` / `13 MB` | `0.5–2 GB` · CPU | Every machine | Fast offline transcription for clone prep | Small slower on weak CPUs; Sherpa English-only | Reference clip Auto-Transcribe & dictation | [whisper-base](https://huggingface.co/onnx-community/whisper-base) · [whisper-small](https://huggingface.co/onnx-community/whisper-small) · [sherpa-onnx](https://huggingface.co/csukuangfj/sherpa-onnx-whisper-tiny.en) |
| `silero-vad` | Silero Voice Activity Detector | ONNX · `2 MB` | `256 MB` · CPU | Every machine | 2 MB, <1ms speech/silence segmentation | VAD filter only | Cleaning noisy reference recordings | [snakers4/silero-vad](https://github.com/snakers4/silero-vad) |

---

## How "System Fit" Works

Every catalog item (`GET /v1/catalog/models`) and engine card calls VoxForge's live hardware profiler (`HardwareProbe::probe()`):
- **Best for Your System (Score 94–96):** Meets or exceeds RAM headroom + CPU SIMD (`AVX2`/`AVX-512`/`NEON`) for CPU/ONNX models, or meets recommended CUDA/MPS VRAM for neural diffusion models.
- **Runs Well / Tight VRAM (Score 76–82):** Compatible RAM and mid-range GPU (e.g., 4 GB GTX 1650 / M-series Mac) — works smoothly with FP16 or short segments.
- **CPU Slow / Batch Only (Score 52):** RAM fits, but CUDA/MPS VRAM is below recommended target; falls back to CPU for offline rendering slower than real-time.
- **Low RAM / GPU Required (Score 15–20):** Host RAM falls under model minimum requirement (`min_ram_mb`).
