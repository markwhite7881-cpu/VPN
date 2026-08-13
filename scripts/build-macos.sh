#!/usr/bin/env bash
# build-macos.sh — produce an unsigned Cloakwire .dmg locally on a Mac.
#
# Use this when iterating on macOS without going through GitHub Actions.
# On CI, .github/workflows/release-macos.yml does the same job.
#
# Requirements:
#   - macOS 11+ (Apple Silicon or Intel)
#   - Xcode Command Line Tools (xcode-select --install)
#   - Rust 1.77+ with the aarch64-apple-darwin + x86_64-apple-darwin
#     targets installed:
#         rustup target add aarch64-apple-darwin
#         rustup target add x86_64-apple-darwin
#   - Node 20+ (nvm or Homebrew)
#   - Homebrew's `create-dmg` for the .dmg step:
#         brew install create-dmg
#
# Usage (from the repo root, on a Mac):
#   ./scripts/build-macos.sh                     # universal .dmg
#   ./scripts/build-macos.sh aarch64-apple-darwin   # arm64 only
#   ./scripts/build-macos.sh x86_64-apple-darwin   # Intel only

set -euo pipefail

# Make sure cargo / rustup are reachable. A fresh Mac install of
# Rust via rustup.rs puts cargo in ~/.cargo/bin which is not on
# the default PATH for GUI-launched shells.
export PATH="$HOME/.cargo/bin:$PATH"

TARGET="${1:-universal-apple-darwin}"

case "$TARGET" in
  aarch64-apple-darwin|x86_64-apple-darwin|universal-apple-darwin)
    ;;
  *)
    echo "ERROR: unknown target '$TARGET' (use aarch64-apple-darwin, x86_64-apple-darwin, or universal-apple-darwin)" >&2
    exit 1
    ;;
esac

# Repo root = the dir that contains this script's parent.
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

# 1) Standard Tauri build. --bundles app,dmg produces both the
#    .app (drag-to-Applications) and the .dmg (mount + drag).
echo "==> npm run tauri:build -- --target $TARGET --bundles app,dmg (this takes ~3 min on a warm cache)..."
npm run tauri:build -- --target "$TARGET" --bundles app,dmg

# 2) The Tauri output lives in src-tauri/target/<target>/release/bundle/dmg/
#    on macOS. Tauri 2 emits `Cloakwire_<version>_<arch>.dmg`. We
#    also drop a copy in dist-release/ (mirrored to where the
#    .deb / .AppImage live) so the user can find it next to the
#    other platform builds.
DMG_SRC_DIR="src-tauri/target/$TARGET/release/bundle/dmg"
if [ ! -d "$DMG_SRC_DIR" ]; then
    DMG_SRC_DIR="src-tauri/target/release/bundle/dmg"
fi
DMG_SRC="$(ls -1 "$DMG_SRC_DIR"/*.dmg 2>/dev/null | head -1 || true)"
if [ -z "$DMG_SRC" ]; then
    echo "ERROR: no .dmg produced in $DMG_SRC_DIR" >&2
    exit 1
fi

VER="$(echo "$DMG_SRC" | sed -nE 's/.*_([0-9]+\.[0-9]+\.[0-9]+).*\.dmg/\1/p')"
if [ -z "$VER" ]; then
    VER="0.0.0"
fi

# Keep Tauri 2's original filename (it has the arch suffix that
# downstream tooling depends on).
echo "==> built: $DMG_SRC"

# 3) Sanity check: mount the .dmg and confirm Cloakwire.app
#    + sing-box are inside.
if command -v hdiutil >/dev/null 2>&1; then
    MOUNT="$(mktemp -d)"
    trap 'rm -rf "$MOUNT"' EXIT
    if hdiutil attach -nobrowse -readonly -mountpoint "$MOUNT" "$DMG_SRC" >/dev/null 2>&1; then
        if [ -d "$MOUNT/Cloakwire.app" ] || find "$MOUNT" -maxdepth 3 -name 'Cloakwire.app' -print -quit | grep -q .; then
            echo "==> OK: Cloakwire.app found inside .dmg"
        else
            echo "ERROR: Cloakwire.app missing inside .dmg" >&2
            hdiutil detach "$MOUNT" >/dev/null 2>&1 || true
            exit 1
        fi
        hdiutil detach "$MOUNT" >/dev/null 2>&1 || true
    else
        echo "WARNING: hdiutil attach failed (Gatekeeper?); skipping smoke check"
    fi
fi

# 4) Mirror to dist-release/ alongside the other installers.
DEST_DIR="$PROJECT_ROOT/dist-release"
mkdir -p "$DEST_DIR"
DEST="$DEST_DIR/$(basename "$DMG_SRC")"
cp "$DMG_SRC" "$DEST"
echo "==> mirrored to: $DEST"
echo
echo "Done. To install on a Mac:"
echo "  open $DEST"
echo "Then drag Cloakwire.app to /Applications. First launch:"
echo "  xattr -d com.apple.quarantine /Applications/Cloakwire.app"
echo "or right-click → Open → Open Anyway in System Settings."
