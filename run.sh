#!/usr/bin/env bash
# Start VoxForge Speech Synthesis Workstation and launch browser
set -e
cargo run --bin voxforge -- serve --open "$@"

