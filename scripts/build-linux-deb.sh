#!/usr/bin/env bash
# build-linux-deb.sh — produce a Linux .deb for Cloakwire with
# a postinst that grants sing-box the caps it needs for TUN mode.
#
# What this does:
#   1. Run `cargo tauri build --bundles deb` from /home/<user>/cloakwire
#      (the WSL2 source copy). This produces an unsigned, postinst-less
#      .deb at src-tauri/target/release/bundle/deb/Cloakwire_*_amd64.deb.
#   2. Extract that .deb, drop in scripts/deb-postinst.sh as
#      DEBIAN/postinst (chmod 0755), and re-pack in place.
#   3. Copy the result back to the Windows-side
#      dist-release/Cloakwire_<version>_amd64.deb.
#
# Why a post-build patch: Tauri 2's bundler doesn't expose a
# `postinst` field in tauri.conf.json (no first-class hook for
# .deb maintainer scripts). The cleanest way to ship one is to
# inject it after the bundle is built. dpkg-deb -R / -b is
# standard tooling and round-trips the binary content byte-for-byte.
#
# Usage (from WSL2):
#   cd /home/cloakwire
#   ./scripts/build-linux-deb.sh 1.3.0
#
# Requirements:
#   - WSL2 with the cloakwire user (see the setup notes in
#     README.md §"Linux development").
#   - cargo, npm, and the Tauri 2 build deps installed.
#   - dpkg / dpkg-deb (default on every Ubuntu/Debian).

set -euo pipefail

# Resolve the user this script is running as — usually
# `cloakwire` (the dedicated build user in our WSL2 setup)
# but fall back to the current login if invoked from a
# different account.
BUILD_USER="$(id -un)"
export PATH="/home/${BUILD_USER}/.cargo/bin:$PATH"

VERSION="${1:-1.3.0}"

# Resolve the WSL-side project root (this script lives at
# scripts/build-linux-deb.sh; the actual cargo project is one
# level up, mirrored under /home/cloakwire/cloakwire).
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

# 1) Standard Tauri build. We use `npm run tauri:build` (which
#    delegates to the @tauri-apps/cli npm package) rather than
#    `cargo tauri build` because the npm package is the source of
#    truth in our repo (see package.json > scripts.tauri:build) and
#    we don't have cargo-tauri.exe installed as a global cargo subcmd.
echo "==> npm run tauri:build -- --bundles deb (this takes ~3 min on a warm cache)..."
npm run tauri:build -- --bundles deb

# 2) Find the produced .deb.
BUNDLE_DIR="src-tauri/target/release/bundle/deb"
DEB_SRC="$(ls -1 "$BUNDLE_DIR"/Cloakwire_*_amd64.deb 2>/dev/null | head -1 || true)"
if [ -z "$DEB_SRC" ]; then
    echo "ERROR: no .deb produced in $BUNDLE_DIR" >&2
    exit 1
fi
echo "==> produced: $DEB_SRC"

# 3) Round-trip through dpkg-deb to inject the postinst.
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
dpkg-deb -R "$DEB_SRC" "$WORK/extracted"

cp scripts/deb-postinst.sh "$WORK/extracted/DEBIAN/postinst"
chmod 0755 "$WORK/extracted/DEBIAN/postinst"

# Re-pack in place. `dpkg-deb -b` rebuilds the data + control
# archives from scratch, so the result is byte-deterministic
# (modulo timestamps and member order) and validates cleanly
# with `dpkg-deb -I` / `--info`.
# `--root-owner-group` is required when this script runs as a non-root
# build user: it preserves Debian's root/root ownership for all control and
# data archive members instead of recording the local builder account.
dpkg-deb --root-owner-group -b "$WORK/extracted" "$DEB_SRC"
echo "==> injected postinst into $DEB_SRC"

# 4) Copy the result back to this checkout's dist-release so the CI /
#    release tooling (and the user) can find the artifact alongside the
#    Windows installers.
WINDOWS_DEST="$PROJECT_ROOT/dist-release/Cloakwire_${VERSION}_amd64.deb"
mkdir -p "$PROJECT_ROOT/dist-release"
cp "$DEB_SRC" "$WINDOWS_DEST"
echo "==> copied to $WINDOWS_DEST"

# 5) Sanity-check the result: confirm the postinst is present
#    and references setcap. If either check fails, abort loudly.
#    `dpkg-deb -I` shows control fields (not maintainer scripts),
#    so we extract with `-e` to a temp dir and read postinst directly.
VERIFY_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK" "$VERIFY_DIR"' EXIT
if ! dpkg-deb -e "$DEB_SRC" "$VERIFY_DIR" 2>/dev/null; then
    echo "ERROR: dpkg-deb -e failed on $DEB_SRC" >&2
    exit 1
fi
if [ ! -x "$VERIFY_DIR/postinst" ]; then
    echo "ERROR: postinst is missing or not executable in $DEB_SRC" >&2
    exit 1
fi
if ! grep -q "setcap" "$VERIFY_DIR/postinst"; then
    echo "ERROR: postinst doesn't contain 'setcap' — build is broken" >&2
    exit 1
fi
echo "==> OK: $DEB_SRC (postinst verified, setcap present)"
