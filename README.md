<div align="center">

# VoxForge

**Enterprise-Grade, Self-Hostable, Multi-Engine Text-to-Speech Platform & Visual Pipeline Builder**

[![Status](https://img.shields.io/badge/Status-Production_Ready_v0.3.0-brightgreen.svg)](docs/architecture.md)
[![Release](https://img.shields.io/badge/Release-v0.3.0-brightgreen.svg)](https://github.com/AMG555/VoxForge/releases)
[![License](https://img.shields.io/badge/License-Apache_2.0_OR_MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.80%2B-orange.svg)](https://www.rust-lang.org)
[![React](https://img.shields.io/badge/Frontend-React_19_%2B_TypeScript-61dafb.svg)](https://react.dev)
[![API](https://img.shields.io/badge/API-OpenAI_TTS_Compatible-green.svg)](docs/api.md)
[![Platform](https://img.shields.io/badge/Platform-Linux_%7C_macOS_%7C_Windows-lightgrey.svg)](docs/architecture.md)

*An open-source, modular speech synthesis workstation. Self-host high-fidelity neural voices, chain complex multi-speaker audio graphs, and run locally or scale horizontally.*

[Download .exe](#direct-downloads-desktop--standalone) • [Features](#key-features) • [Installation Guide](docs/installation.md) • [User Manual](docs/user-guide.md) • [Architecture](docs/architecture.md) • [API Spec](docs/api.md) • [Wireframes](docs/wireframes.md) • [Hardware Tiers](#hardware-profiler)

</div>

---

> [!NOTE]
> **Project Status: Feature-Complete (v0.2.0 Public Beta)**  
> All core features built, tested, and operational: multi-engine neural TTS (Kokoro, Piper, Qwen3, Edge-TTS), zero-shot voice cloning, Whisper ASR, infinite visual DAG pipeline builder, 14-tool Model Context Protocol (MCP) server, double-blind A/B QA lab, audio watermarking, and OpenAI-compatible API. All 14 crates pass automated unit & integration tests; currently in open public beta for community hardware & driver soak testing prior to v1.0 LTS.

> [!TIP]
> **100% Local & Privacy Guaranteed**  
> All neural voice synthesis, cloning, and transcription execute directly on your machine. Zero telemetry, zero audio streaming to external clouds, zero tracking. Fully functional offline.

---

## ⚡ Quick Start — Run in 1 Step

### 🤖 1-Prompt Setup for Any AI Agent (Cursor, Claude Code, Windsurf, Antigravity)

Running an AI coding assistant? Copy-paste this single prompt to have your agent clone, build, launch, and verify VoxForge automatically:

```text
Clone https://github.com/AMG555/VoxForge.git, navigate into VoxForge, launch the speech synthesis server and visual workstation (run `cargo run --bin voxforge` or use `./run.bat` on Windows / `./run.sh` on macOS/Linux; fallback to `npx voxforge` or `docker compose up -d` if no Rust compiler is installed), and verify that http://localhost:8080/health returns status ok.
```

---

### 🚀 Instant Run for Humans

| Method | Command | What It Does |
| :--- | :--- | :--- |
| **Instant (Zero Install)** | `npx voxforge` | Auto-detects OS, downloads pre-built binary, launches server & opens browser |
| **Clone & Run (Cargo)** | `git clone https://github.com/AMG555/VoxForge.git`<br>`cd VoxForge && cargo run --bin voxforge` | Builds Rust daemon + embedded React 19 UI, opens `http://localhost:8080` |
| **Windows 1-Click** | Double-click `run.bat` | Auto-detects MSVC compiler, builds & opens workstation |
| **Linux / macOS 1-Click** | `./run.sh` | Shell launcher for workstation daemon |
| **Docker Container** | `docker run -d -p 8080:8080 -v voxforge-data:/app/data ghcr.io/amg555/voxforge` | Standalone container with backend & embedded UI |

---

## 📦 System Install & 🗑️ Clean Uninstall

Install standalone binary into system PATH or cleanly remove it with 1 command:

| Platform | 1-Line Terminal Install | 1-Line Clean Uninstall |
| :--- | :--- | :--- |
| **Windows (PowerShell)** | `irm https://raw.githubusercontent.com/AMG555/VoxForge/main/scripts/install.ps1 \| iex` | `irm https://raw.githubusercontent.com/AMG555/VoxForge/main/scripts/uninstall.ps1 \| iex` |
| **Linux & macOS** | `curl -fsSL https://raw.githubusercontent.com/AMG555/VoxForge/main/scripts/install.sh \| sh` | `curl -fsSL https://raw.githubusercontent.com/AMG555/VoxForge/main/scripts/uninstall.sh \| sh` |
| **Cloned Repo** | Run `./run.bat` (Win) or `./run.sh` (Unix) | Run `./uninstall.bat` (Win) or `./uninstall.sh` (Unix) |

> [!TIP]
> **Need to delete models and voice profiles too?** Pass `-PurgeData` (Windows PowerShell: `.\scripts\uninstall.ps1 -PurgeData`) or `--purge-data` (Linux/macOS: `./scripts/uninstall.sh --purge-data`).

---

## ✨ Key Capabilities

- **🎙️ Unified Multi-Engine Speech Synthesis**: Abstract 10+ local and cloud speech engines (Piper ONNX, Kokoro-82M, KittenTTS, Qwen3-TTS, ChatTTS, StyleTTS2, and Edge-TTS) behind a single OpenAI-compatible API (`/v1/audio/speech`).
- **🧬 Zero-Shot Voice Cloning & Voice Lab**: Extract 512-D neural speaker identity embeddings from 3-second reference audio (`POST /v1/voices/clone`). Includes 80Hz Butterworth filtering, adaptive noise gating, peak normalization (-0.5 dBFS), 5-axis Director AI controls, and `.voxpersona` bundle sharing.
- **🎛️ Visual DAG Pipeline Studio**: Infinite node canvas with focal zoom (`0.25x` to `2.5x`), Kahn's topological execution, magnetic snapping, multi-node clipboard copying (`Ctrl+C`), video dubbing time-stretching (WSOLA), and audiobook resumption checkpointing (`resume.json`).
- **🤖 Model Context Protocol (MCP) Server**: Built-in JSON-RPC 2.0 stdio server (`voxforge mcp`) providing 14 speech tools and 4 resource streams directly to Claude Desktop, Cursor, Antigravity, and Windsurf.
- **🛡️ Enterprise Hardware & Audio DSP**: Automated hardware profiler detecting AVX2/AVX-512, NVIDIA CUDA, and Apple MPS; real-time Acoustic Echo Cancellation (NLMS); and spread-spectrum acoustic watermarking (`0x5658`).

---

## Direct Downloads (Desktop & Standalone)

For creators and desktop users who prefer pre-compiled release binaries:

| Platform | Download | Package Format | Details |
| :--- | :--- | :--- | :--- |
| **Windows 10 / 11 (Installer)** | [**Download Setup (.exe)**](https://github.com/AMG555/VoxForge/releases/latest/download/VoxForge-Setup-v0.1.0-windows-x64.exe) | Setup Wizard (`.exe`) | Professional installer with desktop shortcut & uninstaller |
| **Windows 10 / 11 (Portable)** | [**Download Standalone .exe**](https://github.com/AMG555/VoxForge/releases/latest/download/voxforge-windows-x86_64.exe) | Portable Executable | Single zero-install binary with embedded studio UI |
| **Windows 10 / 11 (Zip Bundle)** | [**Download .zip**](https://github.com/AMG555/VoxForge/releases/latest/download/VoxForge-Windows-x64.zip) | Portable Bundle (`.zip`) | Includes `voxforge.exe`, 1-click launcher, and desktop installer |
| **macOS (Universal)** | [**Download Universal**](https://github.com/AMG555/VoxForge/releases/latest/download/voxforge-macos-universal) | Standalone Binary | Universal binary with embedded studio UI (Apple Silicon & Intel) |
| **Linux (x86_64)** | [**Download .tar.gz**](https://github.com/AMG555/VoxForge/releases/latest/download/voxforge-linux-x86_64.tar.gz) | Tarball Archive | Zero-dependency standalone executable |

---

## System Architecture

VoxForge is built as a high-performance modular workspace in Rust:

```
                          ┌────────────────────────┐
                          │   Client Application   │
                          │ (OpenAI SDK / Web / UI)│
                          └───────────┬────────────┘
                                      │ HTTP / WebSocket
                                      ▼
┌────────────────────────────────────────────────────────────────────────┐
│                              VOXFORGE DAEMON                            │
│                                                                        │
│  ┌───────────────────────┐             ┌────────────────────────────┐  │
│  │     voxforge-api       │             │      voxforge-hardware      │  │
│  │ (Axum 0.8 / Security) │             │ (CUDA/MPS/AVX Auto-Detect) │  │
│  └───────────┬───────────┘             └─────────────┬──────────────┘  │
│              │                                       │                 │
│              ▼                                       ▼                 │
│  ┌───────────────────────┐             ┌────────────────────────────┐  │
│  │   voxforge-pipeline    │◄────────────┤      Engine Scheduler      │  │
│  │  (DAG Graph Runner)   │             │   (Tiered Auto-Fallback)   │  │
│  └───────────┬───────────┘             └─────────────┬──────────────┘  │
│              │                                       │                 │
│              ▼                                       ▼                 │
│  ┌───────────────────────┐             ┌────────────────────────────┐  │
│  │     voxforge-audio     │             │       voxforge-engine       │  │
│  │ (Normalizer/WAV/MP3)  │◄────────────┤  (Piper/Kokoro/Edge/Qwen)  │  │
│  └───────────┬───────────┘             └────────────────────────────┘  │
│              │                                                         │
│              ▼                                                         │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │                           voxforge-core                           │  │
│  │       (DataStore Trait: SQLite / PostgreSQL / Supabase)          │  │
│  └──────────────────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────────────┘
```

For complete technical specifications, sequence diagrams, and lifecycle documentation, refer to:
- [Installation Guide](docs/installation.md)
- [User Manual & CLI Reference](docs/user-guide.md)
- [Architecture Deep-Dive](docs/architecture.md)
- [Model Context Protocol (MCP) Integration](docs/mcp.md)
- [UI/UX Wireframes & Component Specs](docs/wireframes.md)
- [REST & WebSocket API Reference](docs/api.md)
- [Architecture Decision Records (ADRs)](docs/decisions/)

---

## 🛠️ Integrations & Developer Tools

### 1. AI Agent Integration via Model Context Protocol (MCP)

Connect Claude Desktop, Cursor, Google Antigravity, Windsurf, or Roo Code directly to VoxForge:

```json
{
  "mcpServers": {
    "voxforge": {
      "command": "cargo",
      "args": ["run", "-p", "voxforge-cli", "--", "mcp"]
    }
  }
}
```
*Equips AI agents with 14 speech tools (synthesis, cloning, Whisper transcription, voice conversion, DAG pipelines, watermark embedding/verification), 4 resource streams, and direct file artifact delivery.* See the [MCP Integration Guide](docs/mcp.md).

---

### 2. Docker Containers (All-in-One & Neural Stack)

Launch the unified container (backend + embedded visual UI):

```bash
docker run -d -p 8080:8080 -v voxforge-data:/app/data ghcr.io/amg555/voxforge
```
Or start multi-service stack with persistent database and neural microservices:
```bash
docker compose -f docker-compose.neural.yml up -d
```

---

### 3. Run Single Binary (CLI)

Download the release binary for your platform or build from source:

```bash
# Build binary
cargo build --release --bin voxforge

# Verify hardware environment
./target/release/voxforge hardware

# Run automated QA A/B benchmark between voices
./target/release/voxforge ab-test --iterations 3

# Start Model Context Protocol (MCP) server for LLM agents
./target/release/voxforge mcp

# Launch HTTP/WS server
./target/release/voxforge serve --port 8080 --data-dir ./data
```

### 4. OpenAI Drop-In Replacement

VoxForge provides a drop-in replacement for the OpenAI Audio Speech API (`/v1/audio/speech`).

Using `curl`:

```bash
curl http://localhost:8080/v1/audio/speech \
  -H "Authorization: Bearer dev-key-123" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "edge-tts",
    "input": "VoxForge is now operational with hardware-accelerated speech synthesis.",
    "voice": "en-US-ChristopherNeural",
    "response_format": "wav",
    "speed": 1.0
  }' \
  --output test.wav
```

Using the official OpenAI Python SDK:

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://localhost:8080/v1",
    api_key="dev-key-123"
)

response = client.audio.speech.create(
    model="edge-tts",
    voice="en-US-AriaNeural",
    input="Streaming low-latency audio via VoxForge engine."
)

response.stream_to_file("speech.wav")
```

---

### 5. n8n Workflow Automation Integration

Connect VoxForge to [n8n](https://n8n.io) to automate speech pipelines, customer voice replies, and multi-speaker podcast generation:

- **OpenAI Node (Drop-in)**: Point your n8n OpenAI node's Base URL to `http://localhost:8080/v1` to generate speech without changing your existing workflow logic.
- **HTTP Request Node**: Invoke `POST http://localhost:8080/v1/audio/speech` with response format set to `File (Binary)` to dispatch audio to Telegram, WhatsApp, Slack, or Discord bots.
- **Webhook DAG Execution**: Trigger full multi-speaker DAG pipelines with `POST http://localhost:8080/v1/pipeline/execute` passing exported pipeline JSON.
- **Automated Voice Cloning**: Conditionally clone caller voices via `POST http://localhost:8080/v1/voices/clone`.

---

## Hardware Profiler

VoxForge inspects your host hardware on startup to tier model selection automatically:

| Tier | Minimum Specifications | Recommended Engines | Latency (TTFB) |
|---|---|---|---|
| **Tier 1: Minimal** | 2 CPU Cores, 2GB RAM | `edge-tts` (Cloud Relay), `espeak-ng` | ~120ms |
| **Tier 2: Standard** | 4 CPU Cores (AVX2), 4GB RAM | `piper` (medium), `kokoro-82m` (ONNX CPU) | ~85ms |
| **Tier 3: Pro** | 8 CPU Cores, 16GB RAM / 4GB VRAM | `kokoro-82m` (GPU), `kittentts`, `piper` (high) | ~35ms |
| **Tier 4: Enterprise** | NVIDIA GPU >= 12GB VRAM (CUDA) | `qwen3-tts`, `chat-tts`, `styletts2` | ~45ms |

To inspect system capability:

```bash
voxforge hardware probe
```

Output:
```
[INFO] Hardware Analysis:
  - CPU: AMD Ryzen 9 5900X (24 threads) [AVX2: YES, AVX-512: NO]
  - Memory: 32768 MB total (24120 MB free)
  - Accelerators Detected:
    * NVIDIA GeForce RTX 3080 (10240 MB VRAM) [CUDA 12.4]
  - Assigned Profile: TIER_4_ENTERPRISE
  - Default Local Engine: kokoro-82m-cuda
```

---

## Visual Pipeline Nodes

The visual workflow editor (`ui/`) supports real-time DAG composition:

- **Text Input / Ingestion**: Raw text, Markdown parser, SSML injector, Document chunker.
- **Voice Allocation**: Regex-based speaker detection (`Alice: ...`, `Bob: ...`), dynamic voice assigner.
- **Audio Processing**: Pitch shift, tempo scale, stereo panning, dynamic range compressor.
- **Assembly**: Concatenation with configurable crossfades, background music ducking, stem export.

---

## Project Structure

```
voxforge/
├── Cargo.toml                  # Virtual workspace manifest
├── README.md                   # Project overview & documentation index
├── docker-compose.yml          # Container configuration
├── docs/
│   ├── architecture.md         # Full architecture diagrams & specifications
│   ├── wireframes.md           # Visual UI specifications & layout blueprints
│   ├── api.md                  # REST & WebSocket API documentation
│   └── decisions/              # Architecture Decision Records (ADR-001 - 005)
├── crates/
│   ├── voxforge-core/           # Domain models, DataStore trait, error taxonomy
│   ├── voxforge-hardware/       # CPU/GPU hardware detection & tier assignment
│   ├── voxforge-engine/         # TTS Engine traits, registry, Edge-TTS, Qwen3 zero-shot cloning
│   ├── voxforge-router/         # SLA policy routing and multi-objective scoring
│   ├── voxforge-pronunciation/  # Text normalization and custom pronunciation dictionary
│   ├── voxforge-audio/          # Audio manipulation, transcoding, DSP & metrics
│   ├── voxforge-pipeline/       # DAG execution engine, time stretch, dubbing/audiobook templates
│   ├── voxforge-benchmark/      # Engine benchmarking sentence suite & scorecard store
│   ├── voxforge-worker/         # Distributed cluster worker pool & lease management
│   ├── voxforge-asr/            # Automated Speech Recognition abstraction, Whisper, VAD
│   ├── voxforge-catalog/        # Open-weights model catalog & hardware validation
│   ├── voxforge-mcp/            # Model Context Protocol JSON-RPC 2.0 stdio server
│   ├── voxforge-api/            # Axum REST & WebSocket server, OpenAPI 3.1, metrics
│   └── voxforge-cli/            # Unified CLI binary (serve, synth, hardware, bench, ab-test, mcp)
└── ui/                         # React 19 + TypeScript + Tailwind UI
```

---

## Development & Testing

### Prerequisites

- Rust 1.80+ (`cargo`, `rustc`)
- Node.js 20+ & pnpm (for Web UI development)
- SQLite3 / PostgreSQL (optional for local database verification)

### Running Rust Workspace Tests

The workspace features a comprehensive 120+ automated test suite covering acoustic quality metrics, DAG scheduling, topological sorting, storage isolation, hardware tiering, voice cloning, ASR transcriptions, model catalogs, MCP protocol, and REST/WebSocket API endpoints:

```bash
# Run all unit and integration tests across crates (120+ tests)
cargo test --workspace

# Run dedicated audio metrics and QA benchmark tests
cargo test -p voxforge-audio --lib metrics
cargo test -p voxforge-engine --lib ab_test
cargo test -p voxforge-api --lib tests

# Run security linting
cargo clippy --workspace -- -D warnings
```

---

## Security Policy

VoxForge treats audio generation security with enterprise rigor:
- **No Client Secrets**: API keys, storage bucket credentials, and master configurations are strictly server-side.
- **Input Sanitization**: Text and SSML input is validated against injection attacks and buffer overrun limits.
- **Path Traversal Protection**: Export directories, custom audio models, and voice assets are strictly scoped to designated storage roots.
- **Rate Limiting**: Configurable token bucket rate limiting prevents denial-of-service on local inference engines.

To report security vulnerabilities, please contact `amgt503@gmail.com`.

---

## License

This project is dual-licensed under:
- **Apache License, Version 2.0** ([LICENSE-APACHE](LICENSE-APACHE))
- **MIT License** ([LICENSE-MIT](LICENSE-MIT))

You may choose either license at your option.
