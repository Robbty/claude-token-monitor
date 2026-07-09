#!/usr/bin/env bash
# claude-token-display launcher
#
# Usage:
#   ./launcher.sh                                 # system-wide view (all Claude sessions)
#   ./launcher.sh /pfad/zum/projekt               # nur ein Projekt-Verzeichnis
#   ./launcher.sh /pfad/zum/projekt --port 8888
#   ./launcher.sh --port 8888                     # system-wide auf eigenem Port
#
# Starts the Python helper-server in the background, then opens a
# Chromium/Chrome window in --app mode pointing at it. Cleans up the
# server when the window closes (Strg+C in the launcher terminal).

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# First positional arg, if it is not a flag, is the project directory.
PROJECT_DIR=""
if [[ $# -gt 0 && "$1" != --* ]]; then
  PROJECT_DIR="$1"
  shift
fi

if [[ -n "$PROJECT_DIR" && ! -d "$PROJECT_DIR" ]]; then
  echo "Error: $PROJECT_DIR is not a directory" >&2
  exit 2
fi

PORT="${CLAUDE_TOKEN_DISPLAY_PORT:-8766}"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --port) PORT="$2"; shift 2 ;;
    *) echo "Unknown arg: $1" >&2; exit 2 ;;
  esac
done

# Locate a Chromium-based browser that supports --app=.
find_browser() {
  for c in chromium chromium-browser google-chrome google-chrome-stable brave-browser microsoft-edge; do
    if command -v "$c" >/dev/null 2>&1; then
      echo "$c"
      return 0
    fi
  done
  return 1
}

BROWSER="$(find_browser || true)"
if [[ -z "$BROWSER" ]]; then
  echo "Error: no Chromium-based browser found (need chromium / google-chrome / brave / edge)." >&2
  echo "Install one, or open http://127.0.0.1:$PORT in any browser after starting just server.py." >&2
  exit 3
fi

# Resolve claude-tokens.
TOKENS_BIN="${CLAUDE_TOKENS_BIN:-}"
if [[ -z "$TOKENS_BIN" ]]; then
  if command -v claude-tokens >/dev/null 2>&1; then
    TOKENS_BIN="$(command -v claude-tokens)"
  elif [[ -x "$HERE/../target/release/claude-tokens" ]]; then
    TOKENS_BIN="$HERE/../target/release/claude-tokens"
  else
    echo "Error: claude-tokens not found in PATH or ../target/release." >&2
    echo "Install it (~/.local/bin) or set CLAUDE_TOKENS_BIN=/pfad/zu/claude-tokens." >&2
    exit 4
  fi
fi

if [[ -n "$PROJECT_DIR" ]]; then
  PROJECT_ABS="$(cd "$PROJECT_DIR" && pwd)"
  SCOPE_LABEL="$PROJECT_ABS"
else
  PROJECT_ABS=""
  SCOPE_LABEL="system-wide (alle Projekte)"
fi

echo "[launcher] scope        : $SCOPE_LABEL"
echo "[launcher] port         : $PORT"
echo "[launcher] claude-tokens: $TOKENS_BIN"
echo "[launcher] browser      : $BROWSER"
echo

# Start server in background and trap its PID for cleanup.
if [[ -n "$PROJECT_ABS" ]]; then
  python3 "$HERE/server.py" "$PROJECT_ABS" --port "$PORT" --claude-tokens-bin "$TOKENS_BIN" &
else
  python3 "$HERE/server.py" --port "$PORT" --claude-tokens-bin "$TOKENS_BIN" &
fi
SERVER_PID=$!

cleanup() {
  if kill -0 "$SERVER_PID" 2>/dev/null; then
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

# Wait briefly for the server to bind.
for _ in 1 2 3 4 5 6 7 8 9 10; do
  if curl -sf "http://127.0.0.1:$PORT/" >/dev/null 2>&1; then
    break
  fi
  sleep 0.2
done

# Temporary user-data-dir so the app window is independent of the regular profile.
TMP_PROFILE="$(mktemp -d -t claude-token-display.XXXXXX)"

"$BROWSER" \
  --app="http://127.0.0.1:$PORT/" \
  --user-data-dir="$TMP_PROFILE" \
  --no-first-run \
  --no-default-browser-check \
  --window-size=520,640 \
  >/dev/null 2>&1 || true

rm -rf "$TMP_PROFILE"
