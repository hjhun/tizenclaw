#!/usr/bin/env bash
# Cross-compile TizenClaw for aarch64-unknown-linux-gnu on an x86_64 Ubuntu host.
#
# Usage:
#   ./scripts/cross-build-aarch64.sh                # build release
#   ./scripts/cross-build-aarch64.sh --install       # build + install toolchain/target
#   ./scripts/cross-build-aarch64.sh --test          # run tests (host target, no cross)
#   ./scripts/cross-build-aarch64.sh --test FILTER   # run tests matching FILTER
#
# Prerequisites (run once with --install, or manually):
#   sudo apt-get install gcc-aarch64-linux-gnu g++-aarch64-linux-gnu
#   rustup target add aarch64-unknown-linux-gnu
#
# Output binaries:
#   target/aarch64-unknown-linux-gnu/release/tizenclaw
#   target/aarch64-unknown-linux-gnu/release/tizenclaw-cli
#   target/aarch64-unknown-linux-gnu/release/tizenclaw-tool-executor
#   target/aarch64-unknown-linux-gnu/release/tizenclaw-web-dashboard
#
# Install on target (Jetson / aarch64 Linux):
#   scp target/aarch64-unknown-linux-gnu/release/tizenclaw* user@jetson:~/.tizenclaw/bin/

set -euo pipefail

cd "$(dirname "$0")/.."

TARGET="aarch64-unknown-linux-gnu"
LINKER="aarch64-linux-gnu-gcc"
PROFILE="release"
BINARIES=(tizenclaw tizenclaw-cli tizenclaw-tool-executor tizenclaw-web-dashboard)

# ── Install dependencies if requested ────────────────────────────────
if [[ "${1:-}" == "--install" ]]; then
    echo "==> Installing cross-compilation toolchain..."
    sudo apt-get update -qq
    sudo apt-get install -y gcc-aarch64-linux-gnu g++-aarch64-linux-gnu
    rustup target add "$TARGET"
    echo "==> Toolchain installed."
    shift
fi

# ── Test mode (runs on host, no cross-compilation) ──────────────────
if [[ "${1:-}" == "--test" ]]; then
    shift
    FILTER="${1:-}"
    echo "==> Running tests on host (no cross-compilation)..."
    if [[ -n "$FILTER" ]]; then
        echo "==> Test filter: $FILTER"
        cargo test "$FILTER" -- --nocapture 2>&1
    else
        cargo test 2>&1
    fi
    exit $?
fi

# ── Verify toolchain ─────────────────────────────────────────────────
if ! command -v "$LINKER" &>/dev/null; then
    echo "ERROR: $LINKER not found. Run: sudo apt-get install gcc-aarch64-linux-gnu" >&2
    echo "   or: $0 --install" >&2
    exit 1
fi

if ! rustup target list --installed | grep -q "$TARGET"; then
    echo "ERROR: Rust target $TARGET not installed. Run: rustup target add $TARGET" >&2
    echo "   or: $0 --install" >&2
    exit 1
fi

# ── Check Rust version ───────────────────────────────────────────────
MIN_RUSTC="1.87"
RUSTC_VER="$(rustc --version | grep -oP '\d+\.\d+\.\d+' | head -1)"
if [ "$(printf '%s\n' "$MIN_RUSTC" "$RUSTC_VER" | sort -V | head -1)" != "$MIN_RUSTC" ]; then
    echo "ERROR: rustc $RUSTC_VER is too old. TizenClaw requires >= $MIN_RUSTC" >&2
    echo "   Run: rustup update stable" >&2
    exit 1
fi
echo "==> Using rustc $RUSTC_VER (>= $MIN_RUSTC required)"

# ── Build ─────────────────────────────────────────────────────────────
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER="$LINKER"
# Required for C dependencies (openssl-src, etc.) that use cc/cmake.
export CC_aarch64_unknown_linux_gnu="aarch64-linux-gnu-gcc"
export CXX_aarch64_unknown_linux_gnu="aarch64-linux-gnu-g++"
export AR_aarch64_unknown_linux_gnu="aarch64-linux-gnu-ar"

echo "==> Cross-compiling TizenClaw for $TARGET ($PROFILE)..."
# Build only the main binaries — metadata plugin crates require Tizen-only
# native libraries (pkgmgr_installer, dlog) that are unavailable in a
# standard cross-compilation environment.
cargo build --release --target "$TARGET" \
    -p tizenclaw \
    -p tizenclaw-cli \
    -p tizenclaw-tool-executor \
    -p tizenclaw-web-dashboard

echo ""
echo "==> Build complete. Binaries:"
for bin in "${BINARIES[@]}"; do
    BINARY="target/${TARGET}/release/${bin}"
    if [ -f "$BINARY" ]; then
        echo "    $BINARY ($(ls -lh "$BINARY" | awk '{print $5}'))"
    else
        echo "    $BINARY [NOT FOUND]"
    fi
done

echo ""
echo "==> To install on aarch64 target:"
echo "    scp target/${TARGET}/release/tizenclaw* user@jetson:~/.tizenclaw/bin/"
echo "    Or use: ./scripts/install-remote.sh user@jetson"
