#!/bin/sh
set -e

REPO="yonben/hudcc-rs"

# Detect OS
OS="$(uname -s)"
ARCH="$(uname -m)"

case "${OS}" in
    Linux)
        case "${ARCH}" in
            x86_64) ASSET="hudcc-rs-x86_64-unknown-linux-musl" ;;
            *) echo "Error: Unsupported architecture: ${ARCH}. Supported: x86_64" >&2; exit 1 ;;
        esac
        ;;
    Darwin)
        case "${ARCH}" in
            x86_64) ASSET="hudcc-rs-x86_64-apple-darwin" ;;
            arm64)  ASSET="hudcc-rs-aarch64-apple-darwin" ;;
            *) echo "Error: Unsupported architecture: ${ARCH}. Supported: x86_64, arm64" >&2; exit 1 ;;
        esac
        ;;
    *)
        echo "Error: Unsupported OS: ${OS}. Supported: Linux (x86_64), macOS (x86_64, arm64)" >&2
        exit 1
        ;;
esac

# Check Claude Code is installed
if [ ! -d "${HOME}/.claude" ]; then
    echo "Error: ~/.claude not found. Is Claude Code installed?" >&2
    exit 1
fi

# Get latest release tag
TAG="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')"

if [ -z "${TAG}" ]; then
    echo "Error: Could not determine latest release." >&2
    exit 1
fi

# Create target directory
mkdir -p "${HOME}/.claude/hud"

# Download
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${TAG}/${ASSET}"
HTTP_CODE="$(curl -sSL -w '%{http_code}' -o "${HOME}/.claude/hud/hudcc_rs" "${DOWNLOAD_URL}")"

if [ "${HTTP_CODE}" -lt 200 ] || [ "${HTTP_CODE}" -ge 300 ]; then
    rm -f "${HOME}/.claude/hud/hudcc_rs"
    echo "Error: Download failed with HTTP ${HTTP_CODE}" >&2
    exit 1
fi

chmod +x "${HOME}/.claude/hud/hudcc_rs"

VERSION="${TAG#v}"
echo "hudcc-rs ${VERSION} installed to ~/.claude/hud/hudcc_rs"
