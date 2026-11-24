#!/usr/bin/env sh
set -eu

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
echo "[VoxForge] Starting Local Neural Cloning Sidecar (OmniVoice 646-Lang)..."

if ! command -v python3 >/dev/null 2>&1; then
  echo "[ERROR] python3 not found on PATH." >&2
  exit 1
fi

if ! python3 -c "import omnivoice" >/dev/null 2>&1; then
  echo "[INFO] Installing OmniVoice neural runtime..."
  python3 -m pip install --upgrade omnivoice || true
fi

exec python3 "$SCRIPT_DIR/neural_sidecar.py" --port 9000 "$@"
