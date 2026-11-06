#!/usr/bin/env sh
# VoxForge Turnkey Uninstaller for Linux & macOS
# Usage: curl -fsSL https://raw.githubusercontent.com/AMG555/VoxForge/main/scripts/uninstall.sh | sh
# Or:    ./scripts/uninstall.sh [--purge-data]

set -e

INSTALL_DIR="${VOXFORGE_INSTALL_DIR:-$HOME/.local/bin}"
TARGET_FILE="$INSTALL_DIR/voxforge"
DESKTOP_ENTRY="$HOME/.local/share/applications/voxforge.desktop"
DATA_DIR="$HOME/.voxforge"

printf "\033[1;33m"
cat << 'EOF'
  _    _            ______                 
 | |  | |          |  ____|                
 | |  | | _____  __| |__ ___  _ __ __ _    
 | |  | |/ _ \ \/ /|  __/ _ \| '__/ _` |   
  \ \/ / (_) >  < | | | (_) | | | (_| |   
   \__/ \___/_/\_\|_|  \___/|_|  \__, |   
                                  __/ |   
                                 |___/    
EOF
printf "\033[0m"
printf "\033[1;37mVoxForge Uninstaller\033[0m\n\n"

# 1. Terminate running voxforge instances
printf "Stopping any running VoxForge processes...\n"
pkill -f voxforge 2>/dev/null || true

# 2. Remove binary
if [ -f "$TARGET_FILE" ]; then
    rm -f "$TARGET_FILE"
    printf "\033[1;32mRemoved binary: %s\033[0m\n" "$TARGET_FILE"
fi

# 3. Remove Linux desktop entry
if [ -f "$DESKTOP_ENTRY" ]; then
    rm -f "$DESKTOP_ENTRY"
    printf "\033[1;32mRemoved desktop launcher: %s\033[0m\n" "$DESKTOP_ENTRY"
fi

# 4. Clean PATH entries in shell rc files
for RC in "$HOME/.zprofile" "$HOME/.zshrc" "$HOME/.bashrc"; do
    if [ -f "$RC" ]; then
        if grep -q "voxforge" "$RC" 2>/dev/null || grep -q "$INSTALL_DIR" "$RC" 2>/dev/null; then
            # Clean voxforge specific PATH export line if present
            sed -i.bak "/export PATH=.*$INSTALL_DIR/d" "$RC" 2>/dev/null || true
            rm -f "${RC}.bak" 2>/dev/null || true
            printf "\033[1;32mCleaned PATH configuration in %s\033[0m\n" "$RC"
        fi
    fi
done

# 5. Handle data directory
PURGE=0
for arg in "$@"; do
    if [ "$arg" = "--purge-data" ]; then
        PURGE=1
    fi
done

if [ "$PURGE" -eq 1 ]; then
    if [ -d "$DATA_DIR" ]; then
        rm -rf "$DATA_DIR"
        printf "\033[1;32mPurged data directory: %s\033[0m\n" "$DATA_DIR"
    fi
else
    if [ -d "$DATA_DIR" ]; then
        printf "\n\033[1;33mNote: Downloaded neural weights and voice profiles kept at: %s\033[0m\n" "$DATA_DIR"
        printf "To completely delete all data as well, run: %s --purge-data\n" "$0"
    fi
fi

printf "\n\033[1;32m=== VoxForge successfully uninstalled ===\033[0m\n\n"
