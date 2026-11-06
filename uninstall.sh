#!/usr/bin/env bash
# Uninstall VoxForge binary, desktop launcher, and PATH configuration
set -e
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
bash "$DIR/scripts/uninstall.sh" "$@"

