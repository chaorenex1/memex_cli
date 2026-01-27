#!/bin/bash
set -e

# Configuration
REPO="chaorenex1/memex-cli"
NAME="memex-cli"
INSTALL_DIR="$HOME/.local/bin"

# Colors
R='\033[0;31m' G='\033[0;32m' Y='\033[1;33m' B='\033[0;34m' N='\033[0m'
info() { echo -e "${B}[INFO]${N} $1"; }
ok() { echo -e "${G}[OK]${N} $1"; }
warn() { echo -e "${Y}[WARN]${N} $1"; }
err() { echo -e "${R}[ERROR]${N} $1"; exit 1; }

# Detect OS and arch
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

# Map to release naming convention
case "$OS" in
    darwin) TARGET_OS="apple-darwin" ;;
    linux)  TARGET_OS="unknown-linux-gnu" ;;
    *) err "Unsupported OS: $OS" ;;
esac

case "$ARCH" in
    x86_64|amd64) TARGET_ARCH="x86_64" ;;
    aarch64|arm64) TARGET_ARCH="aarch64" ;;
    *) err "Unsupported arch: $ARCH" ;;
esac

# Build filename: memex-cli-{arch}-{os}.tar.gz
FILENAME="memex-cli-${TARGET_ARCH}-${TARGET_OS}.tar.gz"

echo -e "\n${G}=== $NAME Installer ===${N}\n"
info "System: $OS/$ARCH"
info "Target: $FILENAME"

# Create temp dir
TMP=$(mktemp -d)
trap "rm -rf $TMP" EXIT

# Get latest version
info "Fetching latest release..."
API="https://api.github.com/repos/$REPO/releases/latest"
VERSION=$(curl -sL "$API" 2>/dev/null | grep -o '"tag_name"[^,]*' | head -1 | cut -d'"' -f4)

if [ -z "$VERSION" ]; then
    err "Cannot fetch release info"
fi

info "Version: $VERSION"

# Download
URL="https://github.com/$REPO/releases/download/$VERSION/$FILENAME"
info "Downloading: $URL"

if ! curl -fsSL "$URL" -o "$TMP/$FILENAME" 2>/dev/null; then
    err "Download failed: $URL"
fi
ok "Download complete"

# Extract
info "Extracting..."
cd "$TMP"
tar -xzf "$FILENAME"
ok "Extraction complete"

# Find binary (it should be named memex-cli or memex)
BIN=$(find . -type f \( -name "memex-cli" -o -name "memex" \) ! -name "*.tar.gz" 2>/dev/null | head -1)

if [ -z "$BIN" ]; then
    # List extracted files for debugging
    warn "Extracted files:"
    ls -la "$TMP"
    err "Binary not found"
fi

info "Found: $BIN"

# Install
mkdir -p "$INSTALL_DIR"
[ -f "$INSTALL_DIR/$NAME" ] && warn "Overwriting existing version"
cp "$BIN" "$INSTALL_DIR/$NAME"
chmod +x "$INSTALL_DIR/$NAME"
ok "Installed: $INSTALL_DIR/$NAME"

# Install memex-env scripts (optional)
echo ""
info "Installing memex-env scripts..."
SCRIPTS_URL="https://github.com/$REPO/releases/download/$VERSION/memex-env-scripts.tar.gz"
SCRIPTS_ARCHIVE="$TMP/memex-env-scripts.tar.gz"

if curl -fsSL "$SCRIPTS_URL" -o "$SCRIPTS_ARCHIVE" 2>/dev/null; then
    cd "$TMP"
    if tar -xzf "$SCRIPTS_ARCHIVE" 2>/dev/null; then
        INSTALLED_SCRIPTS=0
        for script in scripts/memex-env.*; do
            if [ -f "$script" ]; then
                cp "$script" "$INSTALL_DIR/$(basename $script)"
                if [[ "$script" == *.sh ]]; then
                    chmod +x "$INSTALL_DIR/$(basename $script)"
                fi
                ok "Installed: $INSTALL_DIR/$(basename $script)"
                ((INSTALLED_SCRIPTS++))
            fi
        done
        if [ $INSTALLED_SCRIPTS -eq 0 ]; then
            warn "No memex-env scripts found in archive"
        fi
    else
        warn "Failed to extract memex-env scripts"
        info "Continuing without memex-env scripts..."
    fi
else
    info "memex-env scripts not available in this release"
    info "Continuing with main installation..."
fi

# Update PATH if needed
if ! echo "$PATH" | grep -q "$INSTALL_DIR"; then
    RC="$HOME/.bashrc"
    [ -n "$ZSH_VERSION" ] || [[ "$SHELL" == *zsh* ]] && RC="$HOME/.zshrc"
    echo -e "\nexport PATH=\"$INSTALL_DIR:\$PATH\"" >> "$RC"
    warn "Added to $RC - restart terminal or: source $RC"
else
    ok "$INSTALL_DIR already in PATH"
fi

# Install via npm as additional installation method
echo ""
info "Installing via npm (additional method)..."
if command -v npm &>/dev/null; then
    if npm install -g "$NAME" 2>/dev/null; then
        ok "npm installation complete"
    else
        warn "npm installation failed"
    fi
else
    info "npm not available, skipping..."
fi

# Verify
echo -e "\n${G}=== Installation Complete ===${N}\n"
echo "Run: $NAME --help"
echo ""

# Try to show version
"$INSTALL_DIR/$NAME" --help 2>/dev/null || true
