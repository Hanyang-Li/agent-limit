#!/bin/sh
# agent-limit installer — downloads the latest release binary (macOS / Apple
# Silicon) and installs it onto your PATH.
#
#   curl -fsSL https://raw.githubusercontent.com/Hanyang-Li/agent-limit/main/install.sh | sh
#
# Overrides (env):
#   AGENT_LIMIT_VERSION       tag to install (default: latest release)
#   AGENT_LIMIT_INSTALL_DIR   install directory (default: /usr/local/bin)
set -eu

REPO="Hanyang-Li/agent-limit"
BIN="agent-limit"
TARGET="aarch64-apple-darwin"
INSTALL_DIR="${AGENT_LIMIT_INSTALL_DIR:-/usr/local/bin}"

fail() { echo "agent-limit install: $*" >&2; exit 1; }

os="$(uname -s)"
arch="$(uname -m)"
[ "$os" = "Darwin" ] || fail "release binaries are macOS only (detected $os); install with: cargo install --git https://github.com/$REPO --locked"
[ "$arch" = "arm64" ] || fail "release binaries are Apple Silicon (arm64) only (detected $arch); install with: cargo install --git https://github.com/$REPO --locked"
command -v curl >/dev/null 2>&1 || fail "curl is required"
command -v tar >/dev/null 2>&1 || fail "tar is required"

VERSION="${AGENT_LIMIT_VERSION:-}"
if [ -z "$VERSION" ]; then
  VERSION="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
    | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -1)"
fi
[ -n "$VERSION" ] || fail "could not determine the latest version"

ASSET="$BIN-$VERSION-$TARGET.tar.gz"
URL="https://github.com/$REPO/releases/download/$VERSION/$ASSET"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "Downloading $ASSET ..."
curl -fsSL "$URL" -o "$tmp/$ASSET" || fail "download failed: $URL"

if curl -fsSL "$URL.sha256" -o "$tmp/$ASSET.sha256" 2>/dev/null; then
  if (cd "$tmp" && shasum -a 256 -c "$ASSET.sha256" >/dev/null 2>&1); then
    echo "Checksum OK"
  else
    fail "checksum verification failed"
  fi
fi

tar -xzf "$tmp/$ASSET" -C "$tmp"
[ -f "$tmp/$BIN" ] || fail "archive did not contain $BIN"

if [ -d "$INSTALL_DIR" ] && [ -w "$INSTALL_DIR" ]; then
  install -m 0755 "$tmp/$BIN" "$INSTALL_DIR/$BIN"
else
  echo "Installing to $INSTALL_DIR (may prompt for sudo) ..."
  sudo mkdir -p "$INSTALL_DIR"
  sudo install -m 0755 "$tmp/$BIN" "$INSTALL_DIR/$BIN"
fi

echo "Installed $BIN $VERSION to $INSTALL_DIR/$BIN"
case ":$PATH:" in
  *":$INSTALL_DIR:"*) echo "Run: $BIN" ;;
  *) echo "Note: $INSTALL_DIR is not on your PATH; add it or run $INSTALL_DIR/$BIN" ;;
esac
