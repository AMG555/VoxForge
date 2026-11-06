# VoxForge - AI Agent Context & Guide

This file provides system context, build commands, and execution patterns for AI coding assistants (Claude Code, Cursor, Windsurf, Antigravity, Roo Code, Aider, Codex).

## Project Overview

VoxForge is an open-source, modular, hardware-adaptive speech synthesis server, voice cloning suite, visual DAG pipeline builder, and Model Context Protocol (MCP) server written in Rust (backend) and React 19 + TypeScript (frontend).

- **Architecture**: 14 Rust crates in `crates/` workspace (`voxforge-core`, `voxforge-hardware`, `voxforge-audio`, `voxforge-engine`, `voxforge-router`, `voxforge-pronunciation`, `voxforge-pipeline`, `voxforge-benchmark`, `voxforge-worker`, `voxforge-asr`, `voxforge-catalog`, `voxforge-mcp`, `voxforge-api`, `voxforge-cli`).
- **Frontend**: React 19 SPA in `ui/`, pre-compiled and embedded directly into `voxforge-api` via `rust-embed` at compile time (`crates/voxforge-api/ui_dist/`).
- **Zero-Dependency Running**: Launching `voxforge serve` or `cargo run --bin voxforge` serves both the JSON/REST/WebSocket API and the embedded React workstation UI simultaneously on port `8080`.

## Quick Run Commands

| Task | Command | Notes |
| :--- | :--- | :--- |
| **Start Server & UI** | `cargo run --bin voxforge` | Defaults to `serve --open` on `http://127.0.0.1:8080` |
| **Windows 1-Click** | `.\run.bat` | Dynamic MSVC auto-detection wrapper |
| **macOS / Linux 1-Click** | `./run.sh` | Shell launcher |
| **NPM Launcher** | `npm start` | Uses `packages/voxforge/bin/voxforge.js` |
| **MCP Server (Stdio)** | `cargo run -p voxforge-cli -- mcp` | Starts JSON-RPC 2.0 stdio server |
| **Hardware Probe** | `cargo run --bin voxforge -- hardware` | Displays CPU SIMD & GPU compute tiers |
| **Audio Synthesis** | `cargo run --bin voxforge -- synth "Hello" --voice en-US-AriaNeural --out test.wav` | Direct CLI synthesis |
| **A/B Quality Test** | `cargo run --bin voxforge -- ab-test --iterations 3` | Measures RTF, SNR, loudness |
| **Uninstall (Windows)** | `.\uninstall.bat` | Removes shortcuts, cleans PATH, removes binary |
| **Uninstall (Unix)** | `./uninstall.sh` | Removes binary, desktop launcher, and cleans PATH |

## Development & Test Commands

| Task | Command |
| :--- | :--- |
| **Run All Tests** | `cargo test --workspace` |
| **Run Single Crate Tests** | `cargo test -p voxforge-mcp` |
| **Lint & Static Analysis** | `cargo clippy --workspace --all-targets` |
| **Typecheck Frontend** | `npm --prefix ui run lint` (`tsc --noEmit`) |
| **Rebuild UI Distribution** | `npm run ui:build` |
| **Full Release Build** | `cargo build --release --bin voxforge` |

## Core APIs & Endpoints

- **Webstation UI**: `GET http://localhost:8080/`
- **Health Check**: `GET http://localhost:8080/health` (returns `{"status":"ok", ...}`)
- **OpenAI-Compatible Speech**: `POST http://localhost:8080/v1/audio/speech`
- **Whisper Transcription**: `POST http://localhost:8080/v1/audio/transcriptions`
- **Voice Cloning**: `POST http://localhost:8080/v1/voices/clone`
- **Voice Profiles CRUD**: `GET|POST /v1/voices`
- **DAG Pipeline Execution**: `POST http://localhost:8080/v1/pipeline/execute`
- **MCP HTTP Bridge**: `POST http://localhost:8080/v1/mcp`
- **Model Catalog**: `GET /v1/catalog/models`

## AI Agent Integration (MCP)

Add to agent `mcpServers` config:
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
Exposes 14 tools (`synthesize_speech`, `clone_voice`, `transcribe_audio`, `execute_pipeline`, `list_voices`, etc.) and 4 resource streams (`voxforge://voices`, `voxforge://models`, etc.).

