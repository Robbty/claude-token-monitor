#!/usr/bin/env bash
# Build claude-tokens binaries and stage a self-contained bundle under
#   target/release/bundle/
#
# Bundle layout:
#   target/release/bundle/
#     claude-tokens                ← the chosen binary (default: musl static)
#     display/                     ← display app (HTML/CSS/JS/Python/launcher)
#     README.md                    ← project README
#     SHA256SUMS                   ← checksum of the bundled binary
#
# Optionally, with --tarball, also produces
#   target/release/claude-token-monitor-v<version>-x86_64-linux-<variant>.tar.gz
#
# Usage:
#   ./scripts/build.sh                       # build musl + gnu, stage musl in bundle
#   ./scripts/build.sh --tarball             # ... plus produce a release tarball
#   ./scripts/build.sh --variant gnu         # use gnu binary in bundle
#   ./scripts/build.sh --skip-build          # skip cargo, just re-stage

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$HERE"

VARIANT="musl"
MAKE_TARBALL=0
SKIP_BUILD=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --variant) VARIANT="${2:-musl}"; shift 2 ;;
    --tarball) MAKE_TARBALL=1; shift ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    -h|--help)
      sed -n '2,15p' "${BASH_SOURCE[0]}"
      exit 0 ;;
    *) echo "Unknown arg: $1" >&2; exit 2 ;;
  esac
done

# Resolve version from Cargo.toml (first version line).
VERSION="$(awk -F\" '/^version *=/{print $2; exit}' Cargo.toml)"
if [[ -z "$VERSION" ]]; then
  echo "Could not read version from Cargo.toml" >&2
  exit 1
fi

case "$VARIANT" in
  musl) TARGET_TRIPLE="x86_64-unknown-linux-musl"; BIN_REL="target/$TARGET_TRIPLE/release/claude-tokens" ;;
  gnu)  TARGET_TRIPLE="";                          BIN_REL="target/release/claude-tokens" ;;
  *)    echo "--variant must be musl or gnu" >&2; exit 2 ;;
esac

# Path-remap so embedded panic-paths in the binary don't leak /home/<user>.
REMAP="--remap-path-prefix $HOME/.cargo=/cargo"
REMAP+=" --remap-path-prefix $HERE=/build"
if command -v rustc >/dev/null 2>&1; then
  REMAP+=" --remap-path-prefix $(rustc --print sysroot)=/rustc"
fi

if [[ $SKIP_BUILD -eq 0 ]]; then
  echo "[build] cargo build --release (gnu)"
  RUSTFLAGS="$REMAP" cargo build --release
  if [[ -n "$TARGET_TRIPLE" ]]; then
    echo "[build] cargo build --release --target $TARGET_TRIPLE"
    RUSTFLAGS="$REMAP" cargo build --release --target "$TARGET_TRIPLE"
  fi
fi

if [[ ! -x "$BIN_REL" ]]; then
  echo "Binary not found: $BIN_REL" >&2
  echo "Hint: re-run without --skip-build, or check the target triple" >&2
  echo "      (musl needs 'rustup target add x86_64-unknown-linux-musl' + 'musl-tools')." >&2
  exit 1
fi

BUNDLE="target/release/bundle"
rm -rf "$BUNDLE"
mkdir -p "$BUNDLE"

echo "[stage] copying $BIN_REL → $BUNDLE/claude-tokens"
install -m 755 "$BIN_REL" "$BUNDLE/claude-tokens"

echo "[stage] copying display/ → $BUNDLE/display/"
cp -r display "$BUNDLE/"
# Strip caches that may have collected locally.
rm -rf "$BUNDLE/display/__pycache__" "$BUNDLE/display/static/__pycache__"

echo "[stage] copying README.md → $BUNDLE/README.md"
cp README.md "$BUNDLE/README.md"

echo "[stage] computing SHA256"
(cd "$BUNDLE" && sha256sum claude-tokens > SHA256SUMS)

# The bundled launcher prefers claude-tokens in PATH, then falls back to
# '../target/release/claude-tokens'. Inside the bundle, claude-tokens lives one
# level up from display/, so that fallback resolves correctly — no edit needed.

if [[ $MAKE_TARBALL -eq 1 ]]; then
  TARBALL="target/release/claude-token-monitor-v${VERSION}-x86_64-linux-${VARIANT}.tar.gz"
  echo "[tar]   $TARBALL"
  TOPDIR="claude-token-monitor-v${VERSION}"
  STAGE_PARENT="$(mktemp -d)"
  cp -r "$BUNDLE" "$STAGE_PARENT/$TOPDIR"
  tar -C "$STAGE_PARENT" -czf "$TARBALL" "$TOPDIR"
  rm -rf "$STAGE_PARENT"
fi

echo
echo "[done] Bundle ready under: $BUNDLE/"
ls -la "$BUNDLE/"
if [[ $MAKE_TARBALL -eq 1 ]]; then
  echo
  echo "[done] Tarball: $TARBALL ($(du -h "$TARBALL" | cut -f1))"
fi
