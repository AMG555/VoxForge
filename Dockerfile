# Multi-stage Dockerfile for VoxForge Self-Contained Workstation
# Single container serves REST API, WebSocket streams, MCP HTTP/SSE, and embedded React UI.

# 1. UI Build Stage
FROM node:24-bookworm-slim AS ui-builder
WORKDIR /app/ui
COPY ui/package*.json ./
RUN npm ci --prefer-offline --no-audit
COPY ui/ ./
RUN npm run build

# 2. Rust Compile Stage
FROM rust:1-bookworm AS rust-builder
WORKDIR /usr/src/voxforge
COPY Cargo.toml Cargo.lock ./
COPY assets ./assets
COPY crates ./crates

# Copy freshly compiled UI assets into voxforge-api embedded location
COPY --from=ui-builder /app/ui/dist ./crates/voxforge-api/ui_dist

# Build unified standalone binary with locked dependency resolution
RUN cargo build --release --locked --bin voxforge

# 3. Minimal Non-Root Runtime Stage
FROM debian:bookworm-slim

LABEL org.opencontainers.image.title="VoxForge" \
      org.opencontainers.image.description="Hardware-adaptive voice synthesis, zero-shot voice cloning, and MCP server" \
      org.opencontainers.image.source="https://github.com/AMG555/VoxForge" \
      org.opencontainers.image.licenses="Apache-2.0 OR MIT"

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    libasound2 \
    ffmpeg \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 10001 voxforge \
    && useradd --system --uid 10001 --gid voxforge --create-home voxforge

WORKDIR /app

# Copy standalone binary
COPY --from=rust-builder /usr/src/voxforge/target/release/voxforge /usr/local/bin/voxforge

RUN mkdir -p /app/data /app/models \
    && chown -R voxforge:voxforge /app

USER voxforge

ENV VOXFORGE_PORT=8080
ENV VOXFORGE_HOST=0.0.0.0
ENV VOXFORGE_MODELS_DIR=/app/models
ENV VOXFORGE_PROFILES_DIR=/app/data/profiles
ENV VOXFORGE_RATE_LIMIT_RPM=600
ENV RUST_LOG=info

VOLUME ["/app/data", "/app/models"]

EXPOSE 8080

HEALTHCHECK --interval=15s --timeout=5s --start-period=10s --retries=3 \
  CMD curl -fsS http://localhost:8080/health || exit 1

ENTRYPOINT ["voxforge", "serve"]
CMD ["--port", "8080", "--host", "0.0.0.0", "--data-dir", "/app/data"]
