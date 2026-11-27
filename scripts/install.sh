#!/usr/bin/env sh
# VoxForge Turnkey One-Line Installer for Linux & macOS
# Usage: curl -fsSL https://raw.githubusercontent.com/AMG555/VoxForge/main/scripts/install.sh | sh

set -e

REPO="AMG555/VoxForge"
INSTALL_DIR="${VOXFORGE_INSTALL_DIR:-$HOME/.local/bin}"

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
printf "\033[1;37mVoxForge Standalone Workstation Installer\033[0m\n\n"

# 1. Detect OS and Architecture
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"

case "$OS" in
    linux*)
        PLATFORM="linux"
        ;;
    darwin*)
        PLATFORM="macos"
        ;;
    *)
        printf "\033[1;31mError: Unsupported operating system: %s\033[0m\n" "$OS"
        exit 1
        ;;
esac

case "$ARCH" in
    x86_64|amd64)
        TARGET_ARCH="x86_64"
        ;;
    arm64|aarch64)
        TARGET_ARCH="aarch64"
        ;;
    *)
        printf "\033[1;31mError: Unsupported CPU architecture: %s\033[0m\n" "$ARCH"
        exit 1
        ;;
esac

BINARY_NAME="voxforge-${PLATFORM}-${TARGET_ARCH}"
if [ "$PLATFORM" = "windows" ]; then
    BINARY_NAME="${BINARY_NAME}.exe"
fi

printf "Detected Platform: \033[1;32m%s\033[0m (%s)\n" "$PLATFORM" "$TARGET_ARCH"

# 2. Determine target install directory
mkdir -p "$INSTALL_DIR"
TARGET_FILE="$INSTALL_DIR/voxforge"

# 3. Fetch latest release version
LATEST_TAG=$(curl -s "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/' || true)
if [ -z "$LATEST_TAG" ]; then
    LATEST_TAG="v0.7.2"
fi

# 4. Resolve candidate asset filenames for platform
if [ "$PLATFORM" = "macos" ]; then
    # Universal FAT binary preferred, followed by arch-specific fallbacks
    CANDIDATES="voxforge-macos-universal voxforge-macos-${TARGET_ARCH} voxforge-macos-arm64 voxforge-macos-x86_64"
else
    CANDIDATES="voxforge-linux-${TARGET_ARCH}"
fi

printf "Fetching release \033[1;36m%s\033[0m from GitHub...\n" "$LATEST_TAG"

CHECKSUMS_FILE="$(mktemp 2>/dev/null || echo "/tmp/voxforge-checksums-$$.txt")"
CHECKSUMS_URL="https://github.com/${REPO}/releases/download/${LATEST_TAG}/checksums.txt"
if ! curl -fsSL "$CHECKSUMS_URL" -o "$CHECKSUMS_FILE" 2>/dev/null; then
    rm -f "$CHECKSUMS_FILE"
    CHECKSUMS_FILE=""
fi

verify_sha256() {
    local_file="$1"
    asset_name="$2"
    [ -z "$CHECKSUMS_FILE" ] && return 0
    expected=$(grep -E "[[:space:]](\*|\./)?${asset_name}\$" "$CHECKSUMS_FILE" | awk '{print $1}' | head -n 1 || true)
    [ -z "$expected" ] && return 0
    if command -v sha256sum >/dev/null 2>&1; then
        actual=$(sha256sum "$local_file" | awk '{print $1}')
    elif command -v shasum >/dev/null 2>&1; then
        actual=$(shasum -a 256 "$local_file" | awk '{print $1}')
    else
        return 0
    fi
    if [ "$actual" != "$expected" ]; then
        printf "\033[1;31mSHA-256 verification failed for %s\033[0m\n" "$asset_name"
        return 1
    fi
    printf "\033[1;32mSHA-256 verified (%s)\033[0m\n" "$actual"
    return 0
}

TMP_FILE="${TARGET_FILE}.tmp.$$"
DOWNLOAD_SUCCESS=0
for CANDIDATE in $CANDIDATES; do
    DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${LATEST_TAG}/${CANDIDATE}"
    printf "Attempting download: \033[1;34m%s\033[0m\n" "$CANDIDATE"
    if curl -fL --progress-bar "$DOWNLOAD_URL" -o "$TMP_FILE" 2>/dev/null; then
        if verify_sha256 "$TMP_FILE" "$CANDIDATE"; then
            chmod +x "$TMP_FILE"
            mv -f "$TMP_FILE" "$TARGET_FILE"
            if [ "$PLATFORM" = "macos" ]; then
                # Strip macOS quarantine attribute to prevent Gatekeeper popup
                xattr -d com.apple.quarantine "$TARGET_FILE" 2>/dev/null || true
            fi
            printf "\033[1;32mSuccessfully installed voxforge binary to %s\033[0m\n" "$TARGET_FILE"
            DOWNLOAD_SUCCESS=1
            break
        fi
    fi
    rm -f "$TMP_FILE"
done
[ -n "$CHECKSUMS_FILE" ] && rm -f "$CHECKSUMS_FILE"

if [ "$DOWNLOAD_SUCCESS" -eq 0 ]; then
    # Fallback to local source compilation if binary release asset is unavailable
    printf "\033[1;33mRelease binary not directly downloadable; attempting local cargo build...\033[0m\n"
    if command -v cargo >/dev/null 2>&1; then
        cargo install --git "https://github.com/${REPO}.git" voxforge-cli --root "$HOME/.local"
        printf "\033[1;32mSuccessfully compiled and installed via cargo.\033[0m\n"
    else
        printf "\033[1;31mDownload failed and cargo was not found. Please install Rust or check repository releases.\033[0m\n"
        exit 1
    fi
fi

# 5. PATH verification & persistence
case ":$PATH:" in
    *":$INSTALL_DIR:"*)
        ;;
    *)
        printf "\n\033[1;33mNote: %s is not in your current PATH.\033[0m\n" "$INSTALL_DIR"
        SHELL_RC=""
        if [ "$PLATFORM" = "macos" ]; then
            [ -f "$HOME/.zprofile" ] && SHELL_RC="$HOME/.zprofile"
            [ -z "$SHELL_RC" ] && [ -f "$HOME/.zshrc" ] && SHELL_RC="$HOME/.zshrc"
        elif [ -f "$HOME/.bashrc" ]; then
            SHELL_RC="$HOME/.bashrc"
        elif [ -f "$HOME/.zshrc" ]; then
            SHELL_RC="$HOME/.zshrc"
        fi

        if [ -n "$SHELL_RC" ] && [ -f "$SHELL_RC" ]; then
            if ! grep -q "$INSTALL_DIR" "$SHELL_RC" 2>/dev/null; then
                printf "\nexport PATH=\"%s:\$PATH\"\n" "$INSTALL_DIR" >> "$SHELL_RC"
                printf "\033[1;32mAdded %s to %s for future terminal sessions.\033[0m\n" "$INSTALL_DIR" "$SHELL_RC"
            fi
        else
            printf "Add it to your environment by running:\n"
            printf "  export PATH=\"%s:\$PATH\"\n\n" "$INSTALL_DIR"
        fi
        ;;
esac

# 6. Create Desktop application launcher entry for Linux users
if [ "$PLATFORM" = "linux" ]; then
    APPS_DIR="$HOME/.local/share/applications"
    mkdir -p "$APPS_DIR"
    DESKTOP_ENTRY="$APPS_DIR/voxforge.desktop"
    cat <<EOF > "$DESKTOP_ENTRY"
[Desktop Entry]
Name=VoxForge Studio
Comment=Neural Speech Synthesis & Voice Workstation
Exec=$TARGET_FILE serve --open
Icon=audio-card
Terminal=false
Type=Application
Categories=AudioVideo;Audio;Development;
EOF
    chmod +x "$DESKTOP_ENTRY"
    printf "Created desktop application entry: \033[1;36m%s\033[0m\n" "$DESKTOP_ENTRY"
fi

# 6. Verify system hardware profile
printf "\nValidating installation...\n"
"$TARGET_FILE" hardware || true

printf "\n\033[1;32m=== VoxForge installation ready! ===\033[0m\n"
printf "Run visual workstation:  \033[1;37mvoxforge serve --open\033[0m\n"
printf "Start MCP server:        \033[1;37mvoxforge mcp\033[0m\n"
printf "Self-update anytime:     \033[1;37mvoxforge update --yes\033[0m\n"
printf "Run speech synthesis:    \033[1;37mvoxforge synth \"Hello from VoxForge\"\033[0m\n"
printf "Documentation & API:     \033[1;37mhttp://localhost:8080/docs\033[0m\n\n"
