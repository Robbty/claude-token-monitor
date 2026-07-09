#!/usr/bin/env bash
# Generate and install a .desktop launcher for the Claude Token Monitor
# display app, so it can be started from the application menu and/or a
# desktop icon.
#
# Usage:
#   ./display/install-desktop.sh                 # system-wide view
#   ./display/install-desktop.sh --cwd /pfad     # scoped to a project dir
#   ./display/install-desktop.sh --no-desktop    # menu entry only, no desktop icon
#
# Re-run any time (e.g. after moving the repo) to refresh the paths.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LAUNCHER="$HERE/launcher.sh"
ICON="$HERE/icon.png"

SCOPE_ARG=""
PUT_ON_DESKTOP=1
while [[ $# -gt 0 ]]; do
  case "$1" in
    --cwd) SCOPE_ARG="$2"; shift 2 ;;
    --no-desktop) PUT_ON_DESKTOP=0; shift ;;
    -h|--help) sed -n '2,12p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "Unknown arg: $1" >&2; exit 2 ;;
  esac
done

[[ -x "$LAUNCHER" ]] || { echo "launcher.sh not found/executable: $LAUNCHER" >&2; exit 1; }
[[ -f "$ICON" ]]      || { echo "icon.png not found: $ICON" >&2; exit 1; }

# Resolve claude-tokens so the launcher works even when started from a
# minimal desktop-session PATH (no nvm init etc.).
TOKENS_BIN=""
if command -v claude-tokens >/dev/null 2>&1; then
  TOKENS_BIN="$(command -v claude-tokens)"
elif [[ -x "$HERE/../target/release/claude-tokens" ]]; then
  TOKENS_BIN="$(cd "$HERE/.." && pwd)/target/release/claude-tokens"
fi

# Build the Exec line. Pin CLAUDE_TOKENS_BIN if we found one.
EXEC="$LAUNCHER"
[[ -n "$SCOPE_ARG" ]] && EXEC="$EXEC $(printf '%q' "$SCOPE_ARG")"
if [[ -n "$TOKENS_BIN" ]]; then
  EXEC="env CLAUDE_TOKENS_BIN=$(printf '%q' "$TOKENS_BIN") $EXEC"
fi

APP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
mkdir -p "$APP_DIR"
DESKTOP_FILE="$APP_DIR/claude-token-monitor.desktop"

write_desktop() {
  cat > "$1" <<EOF
[Desktop Entry]
Version=1.1
Type=Application
Name=Claude Token Monitor
Comment=Live-Anzeige der Token-/Kontext-Auslastung laufender Claude-Code-Sessions
Exec=$EXEC
Icon=$ICON
Terminal=false
Categories=Development;
Keywords=claude;token;context;ai;monitor;
StartupNotify=true
EOF
}

write_desktop "$DESKTOP_FILE"
chmod +x "$DESKTOP_FILE"
echo "[install] menu entry → $DESKTOP_FILE"

# Refresh the menu database if the tool is available.
command -v update-desktop-database >/dev/null 2>&1 \
  && update-desktop-database "$APP_DIR" >/dev/null 2>&1 || true

if [[ $PUT_ON_DESKTOP -eq 1 ]]; then
  DESKTOP_DIR="$(xdg-user-dir DESKTOP 2>/dev/null || echo "$HOME/Desktop")"
  if [[ -d "$DESKTOP_DIR" ]]; then
    DESK_COPY="$DESKTOP_DIR/claude-token-monitor.desktop"
    write_desktop "$DESK_COPY"
    chmod +x "$DESK_COPY"
    # XFCE/GNOME require the launcher to be marked trusted before it runs
    # on a double-click without warning.
    gio set "$DESK_COPY" metadata::trusted true 2>/dev/null || true
    echo "[install] desktop icon → $DESK_COPY"
  else
    echo "[install] no desktop directory found ($DESKTOP_DIR) — skipped icon"
  fi
fi

echo
echo "[done] Installed. If the desktop icon shows a warning on first click,"
echo "       right-click → 'Diese Datei ausführbar machen' / 'Allow Launching'."
