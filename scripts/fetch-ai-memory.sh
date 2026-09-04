#!/usr/bin/env bash
# Fetch the pinned ai-memory kernel and place it as a Tauri sidecar.
#
# The checksum check is not a formality: this binary is redistributed inside the .app and is
# spawned as a child process, so a silent substitution would be arbitrary code execution under the
# user's identity. A mismatch aborts and leaves nothing behind.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOCK="$ROOT/scripts/ai-memory.lock"
DEST_DIR="$ROOT/src-tauri/binaries"

[[ -f "$LOCK" ]] || { echo "error: missing $LOCK" >&2; exit 1; }
# shellcheck disable=SC1090
source "$LOCK"

case "$(uname -m)" in
  arm64|aarch64) ARCH_TAG="aarch64"; TRIPLE="aarch64-apple-darwin"; EXPECTED="$AI_MEMORY_SHA256_AARCH64" ;;
  x86_64)        ARCH_TAG="x86_64";  TRIPLE="x86_64-apple-darwin";  EXPECTED="$AI_MEMORY_SHA256_X86_64" ;;
  *) echo "error: unsupported architecture $(uname -m)" >&2; exit 1 ;;
esac

TARGET="$DEST_DIR/ai-memory-$TRIPLE"

# Idempotent: a correct binary already in place is left alone, so this is cheap to chain into a build.
if [[ -x "$TARGET" ]]; then
  have="$("$TARGET" --version 2>/dev/null | awk '{print $2}' || true)"
  if [[ "v${have}" == "$AI_MEMORY_VERSION" ]]; then
    echo "ai-memory $AI_MEMORY_VERSION already present at $TARGET"
    exit 0
  fi
  echo "replacing $TARGET (found v${have:-unknown}, want $AI_MEMORY_VERSION)"
fi

ASSET="ai-memory-macos-${ARCH_TAG}.tar.gz"
URL="https://github.com/akitaonrails/ai-memory/releases/download/${AI_MEMORY_VERSION}/${ASSET}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "downloading $URL"
curl -fsSL -o "$TMP/$ASSET" "$URL"

actual="$(shasum -a 256 "$TMP/$ASSET" | awk '{print $1}')"
if [[ "$actual" != "$EXPECTED" ]]; then
  echo "error: checksum mismatch for $ASSET" >&2
  echo "  expected $EXPECTED" >&2
  echo "  actual   $actual" >&2
  exit 1
fi

tar -xzf "$TMP/$ASSET" -C "$TMP"
[[ -f "$TMP/ai-memory" ]] || { echo "error: archive did not contain ai-memory" >&2; exit 1; }

mkdir -p "$DEST_DIR"
# Tauri requires the target-triple suffix on disk and references the path without it.
install -m 0755 "$TMP/ai-memory" "$TARGET"

# MIT obliges us to redistribute the copyright notice with the binary.
LICENSE_DIR="$ROOT/src-tauri/resources/third-party"
mkdir -p "$LICENSE_DIR"
if [[ -f "$TMP/LICENSE" ]]; then
  install -m 0644 "$TMP/LICENSE" "$LICENSE_DIR/ai-memory-LICENSE.txt"
else
  echo "warning: archive had no LICENSE; MIT attribution must be added by hand" >&2
fi

echo "placed $TARGET ($("$TARGET" --version))"
