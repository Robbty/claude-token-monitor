#!/usr/bin/env python3
"""
claude-token-display — minimal HTTP/SSE bridge between claude-tokens and a
browser-based status window.

Architecture:
  - spawns `claude-tokens --all --follow --watch-new --require-open --wait
    --max-age 999999 --json` as a child process
  - reads its NDJSON stdout line by line
  - exposes:
        GET /                  → index.html
        GET /static/<file>     → app.js, app.css, …
        GET /events            → Server-Sent Events stream of session snapshots
        GET /plan              → normalized account rate-limit state (opt-in)
        GET /chats             → chats.html (recent-chats window)
        GET /chats-data        → recent chats across all projects (JSON)
        GET /chat-detail       → condensed conversation tail of one session
        GET/POST /settings     → plan-widget + chats settings
        POST /open-dir         → opens a directory in the system file manager
        POST /focus-terminal   → tries to focus the terminal/IDE window for a cwd
        POST /copy             → copy text to the clipboard
        POST /sticky           → pin a monitor window to all workspaces
        POST /chat-delete      → delete one session file (inactive sessions only)

Standard library only; no pip install needed.
"""

import http.server
import json
import os
import queue
import re
import shlex
import socketserver
import subprocess
import sys
import threading
import time
from datetime import datetime
from pathlib import Path
from urllib.parse import parse_qs, urlparse

HERE = Path(__file__).resolve().parent
DEFAULT_PORT = 8766
SSE_HEARTBEAT_SEC = 10

# Plan-widget settings live in a small XDG config file. The widget is opt-in
# because it reads the OAuth token from ~/.claude/.credentials.json and makes
# outbound HTTPS calls to api.anthropic.com.
CONFIG_DIR = Path(
    os.environ.get("XDG_CONFIG_HOME") or (Path.home() / ".config")
) / "claude-token-monitor"
CONFIG_PATH = CONFIG_DIR / "config.json"

DEFAULT_SETTINGS = {
    "plan_widget": {
        "enabled": False,
        # One-shot ack: stays True once the user accepted the consent modal.
        # Deliberately not reset when the widget is toggled off, so the dialog
        # only appears once per config file (delete the file to resurface it).
        "consent_acknowledged": False,
        "rows": {
            "main":    True,
            "credits": False,
            # Per-feature buckets (seven_day_opus, …) are discovered at runtime
            # and added here with a default of False.
        },
    },
    # Limits for the recent-chats window (🕘). Both apply combined: first the
    # N newest chats per project directory, then the M newest of those overall.
    # active_first: sort chats of currently running sessions (and, when
    # sorting by directory, their whole directory) above the rest.
    "chats": {
        "per_dir": 3,
        "total": 10,
        "active_first": False,
    },
    # Window behavior. sticky: show the monitor and its sub-windows (help,
    # recent chats) on all workspaces. Applied via wmctrl when a page loads;
    # set to false to keep windows on the workspace they were opened on.
    "windows": {
        "sticky": True,
    },
}

CHATS_PER_DIR_MAX = 20
CHATS_TOTAL_MAX = 100

# Human labels for the account's rate-limit buckets. Anthropic uses internal
# codenames for some of them; unknown keys get a prettified fallback.
BUCKET_LABELS = {
    "seven_day_opus":        "Opus · 7 Tage",
    "seven_day_sonnet":      "Sonnet · 7 Tage",
    "seven_day_cowork":      "Cowork · 7 Tage",
    "seven_day_oauth_apps":  "OAuth-Apps · 7 Tage",
    "seven_day_omelette":    "Omelette · 7 Tage",
    "omelette_promotional":  "Omelette (Aktion)",
    "tangelo":               "Tangelo",
    "iguana_necktie":        "Iguana",
    "cinder_cove":           "Cinder Cove",
}

# Keys that are handled explicitly, not as generic buckets.
_MAIN_KEYS = {"five_hour", "seven_day", "extra_usage"}

# Cache for the upstream /plan response. The numbers only change when the user
# actually makes a Claude request; refreshing more than once a minute just burns
# the rate-limit budget of our own call.
PLAN_CACHE_TTL_SEC = 60
_plan_cache_lock = threading.Lock()
_plan_cache: dict | None = None
_plan_cache_ts: float = 0.0
_plan_cache_exit: int = 0


def _claude_home() -> Path:
    return Path(os.environ.get("CLAUDE_HOME") or (Path.home() / ".claude"))


def _clamp_int(value, lo: int, hi: int, default: int) -> int:
    """Coerce value to an int within [lo, hi]; fall back to default."""
    try:
        n = int(value)
    except (TypeError, ValueError):
        return default
    return max(lo, min(hi, n))


def _load_settings() -> dict:
    """Read settings from disk, merge over defaults, tolerate corruption."""
    try:
        data = json.loads(CONFIG_PATH.read_text())
    except (FileNotFoundError, json.JSONDecodeError):
        return json.loads(json.dumps(DEFAULT_SETTINGS))  # deep copy
    merged = json.loads(json.dumps(DEFAULT_SETTINGS))
    pw_in = (data.get("plan_widget") or {})
    pw_out = merged["plan_widget"]
    if isinstance(pw_in.get("enabled"), bool):
        pw_out["enabled"] = pw_in["enabled"]
    if isinstance(pw_in.get("consent_acknowledged"), bool):
        pw_out["consent_acknowledged"] = pw_in["consent_acknowledged"]
    rows_in = pw_in.get("rows") or {}
    if isinstance(rows_in, dict):
        for k, v in rows_in.items():
            if isinstance(v, bool):
                pw_out["rows"][k] = v
    ch_in = data.get("chats") or {}
    ch_out = merged["chats"]
    if isinstance(ch_in, dict):
        ch_out["per_dir"] = _clamp_int(
            ch_in.get("per_dir"), 1, CHATS_PER_DIR_MAX, ch_out["per_dir"])
        ch_out["total"] = _clamp_int(
            ch_in.get("total"), 1, CHATS_TOTAL_MAX, ch_out["total"])
        if isinstance(ch_in.get("active_first"), bool):
            ch_out["active_first"] = ch_in["active_first"]
    win_in = data.get("windows") or {}
    if isinstance(win_in, dict) and isinstance(win_in.get("sticky"), bool):
        merged["windows"]["sticky"] = win_in["sticky"]
    return merged


def _save_settings(data: dict) -> None:
    CONFIG_DIR.mkdir(parents=True, exist_ok=True)
    tmp = CONFIG_PATH.with_suffix(".json.tmp")
    tmp.write_text(json.dumps(data, indent=2))
    tmp.replace(CONFIG_PATH)  # atomic on POSIX


# --- plan normalization ---------------------------------------------------

def _parse_reset(iso: str | None) -> int | None:
    """ISO-8601 (e.g. '2026-06-11T11:19:59.851048+00:00') → unix seconds."""
    if not iso:
        return None
    try:
        return int(datetime.fromisoformat(iso).timestamp())
    except (ValueError, TypeError):
        return None


def _plan_type_label() -> str | None:
    """Read the subscription type from the credentials file for a plan label."""
    try:
        raw = json.loads((_claude_home() / ".credentials.json").read_text())
        sub = (raw.get("claudeAiOauth") or {}).get("subscriptionType")
    except (FileNotFoundError, json.JSONDecodeError, OSError):
        return None
    if not isinstance(sub, str) or not sub:
        return None
    return {"pro": "Pro", "max": "Max", "free": "Free"}.get(sub.lower(), sub.title())


def _normalize_plan(raw: dict) -> dict:
    """Convert the api.anthropic.com/api/oauth/usage response into a stable
    shape the UI consumes: a main window pair (5h/7d), a list of non-null
    per-feature buckets, and the extra-usage credit balance.
    """
    def window(node: dict | None, seconds: int) -> dict | None:
        if not isinstance(node, dict):
            return None
        return {
            "used_percent": node.get("utilization") or 0,
            "reset_at": _parse_reset(node.get("resets_at")),
            "window_seconds": seconds,
        }

    # Newer responses (e.g. Max) carry a structured `limits` array which is the
    # canonical source; the legacy top-level `five_hour`/`seven_day` objects can
    # be null there. Prefer `limits`, fall back to the legacy fields.
    def from_limits(kind: str, seconds: int) -> dict | None:
        limits = raw.get("limits")
        if not isinstance(limits, list):
            return None
        for entry in limits:
            if isinstance(entry, dict) and entry.get("kind") == kind:
                return {
                    "used_percent": entry.get("percent") or 0,
                    "reset_at": _parse_reset(entry.get("resets_at")),
                    "window_seconds": seconds,
                }
        return None

    out: dict = {
        "plan_type": _plan_type_label(),
        "main": {
            "five_hour": from_limits("session", 18000) or window(raw.get("five_hour"), 18000),
            "seven_day": from_limits("weekly_all", 604800) or window(raw.get("seven_day"), 604800),
        },
        "buckets": [],
        "extra_usage": None,
    }

    for key, val in raw.items():
        if key in _MAIN_KEYS or not isinstance(val, dict):
            continue
        if val.get("utilization") is None and val.get("resets_at") is None:
            continue  # null/empty bucket — not applicable to this account
        seconds = 604800 if key.startswith("seven_day") else None
        out["buckets"].append({
            "key": key,
            "label": BUCKET_LABELS.get(key, key.replace("_", " ").title()),
            "used_percent": val.get("utilization") or 0,
            "reset_at": _parse_reset(val.get("resets_at")),
            "window_seconds": seconds,
        })

    # Extra usage (overage charged when plan limits are exceeded, up to a
    # user-set monthly cap). Prefer the structured `spend` object: it gives
    # amounts in *minor* units with an explicit exponent (2000 @ exp 2 = 20.00),
    # which is unambiguous. Fall back to the legacy `extra_usage`, whose
    # `monthly_limit` is also in minor units (scaled by `decimal_places`).
    def money(node: dict | None) -> float | None:
        if not isinstance(node, dict) or node.get("amount_minor") is None:
            return None
        return node["amount_minor"] / (10 ** (node.get("exponent") or 0))

    spend = raw.get("spend")
    eu = raw.get("extra_usage")
    if isinstance(spend, dict) and isinstance(spend.get("limit"), dict):
        out["extra_usage"] = {
            "is_enabled": bool(spend.get("enabled")),
            "limit": money(spend.get("limit")),
            "used": money(spend.get("used")),
            "currency": spend["limit"].get("currency"),
            "disabled_reason": spend.get("disabled_reason"),
        }
    elif isinstance(eu, dict):
        dp = eu.get("decimal_places") or 0
        lim = eu.get("monthly_limit")
        out["extra_usage"] = {
            "is_enabled": bool(eu.get("is_enabled")),
            "limit": (lim / (10 ** dp)) if isinstance(lim, (int, float)) else None,
            "used": eu.get("used_credits"),
            "currency": eu.get("currency"),
            "disabled_reason": eu.get("disabled_reason"),
        }
    return out


def _get_plan_cached(bin_path: str) -> tuple[dict, int]:
    """Return (data, exit_code). `data` is the normalized plan object on success
    or a `{"error","code"}` envelope on failure. Cached for PLAN_CACHE_TTL_SEC.
    """
    global _plan_cache, _plan_cache_ts, _plan_cache_exit
    now = time.time()
    with _plan_cache_lock:
        if _plan_cache is not None and (now - _plan_cache_ts) < PLAN_CACHE_TTL_SEC:
            return _plan_cache, _plan_cache_exit
    try:
        result = subprocess.run(
            [bin_path, "plan"], capture_output=True, text=True, timeout=15
        )
        stdout = result.stdout.strip()
        stderr = (result.stderr or "").strip()
        if not stdout:
            # The plan subcommand always emits JSON. Empty stdout means the
            # binary is too old (no `plan` subcommand) or crashed before
            # printing. Surface stderr so the UI shows the real cause.
            data = {
                "error": stderr[:240] or "binary returned no output",
                "code": "binary_outdated" if "unexpected argument" in stderr else "no_output",
            }
            exit_code = result.returncode or 4
        else:
            try:
                parsed = json.loads(stdout)
            except json.JSONDecodeError:
                parsed = {"error": f"unparseable response: {stdout[:200]}", "code": "bad_response"}
            if parsed.get("error") or parsed.get("code"):
                data = parsed  # CLI error envelope — forward as-is
            else:
                data = _normalize_plan(parsed)
            exit_code = result.returncode
    except FileNotFoundError:
        data = {"error": f"binary not found: {bin_path}", "code": "binary_missing"}
        exit_code = 4
    except subprocess.TimeoutExpired:
        data = {"error": "plan fetch timed out", "code": "timeout"}
        exit_code = 4
    with _plan_cache_lock:
        _plan_cache = data
        _plan_cache_ts = time.time()
        _plan_cache_exit = exit_code
    return data, exit_code


# --- shared state ---------------------------------------------------------

_scope: str | None = None

_state_lock = threading.Lock()
_sessions: dict[str, dict] = {}

_subscribers_lock = threading.Lock()
_subscribers: list[queue.Queue] = []

_path_cache: dict[str, str] = {}

_claude_tokens_bin: str = "claude-tokens"


def _enrich_snapshot(snap: dict) -> dict:
    """Add session_path (absolute) to the snapshot, with caching."""
    sid = snap.get("session_id")
    if not sid:
        return snap
    cached = _path_cache.get(sid)
    if cached is None:
        path = _find_session_for_sid(sid)
        if path is not None:
            try:
                cached = str(path.resolve())
            except OSError:
                cached = str(path)
            _path_cache[sid] = cached
    if cached:
        snap["session_path"] = cached
        # Real file mtime = when this session was last written. The client uses
        # it to show the true data age and to flag stale (not-yet-refreshed)
        # cards, rather than timing from when the snapshot happened to arrive.
        try:
            snap["last_modified"] = os.path.getmtime(cached)
        except OSError:
            pass
    return snap


def _broadcast(event: dict) -> None:
    with _subscribers_lock:
        for q in list(_subscribers):
            try:
                q.put_nowait(event)
            except queue.Full:
                pass


# --- claude-tokens reader thread -----------------------------------------

def _spawn_tokens(cwd_to_watch: str | None, claude_tokens_bin: str) -> subprocess.Popen:
    cmd = [claude_tokens_bin]
    if cwd_to_watch:
        cmd += ["--cwd", cwd_to_watch]
    cmd += [
        "--all", "--follow", "--watch-new", "--require-open",
        "--wait", "--max-age", "999999", "--json",
    ]
    sys.stderr.write(f"[server] spawn: {' '.join(shlex.quote(c) for c in cmd)}\n")
    return subprocess.Popen(
        cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, bufsize=1
    )


def _reader_thread(cwd_to_watch: str | None, claude_tokens_bin: str) -> None:
    """Run claude-tokens, parse NDJSON, update state, broadcast diffs."""
    while True:
        try:
            proc = _spawn_tokens(cwd_to_watch, claude_tokens_bin)
        except FileNotFoundError:
            sys.stderr.write(
                f"[server] claude-tokens not found at {claude_tokens_bin}. "
                "Install it or pass --claude-tokens-bin.\n"
            )
            time.sleep(5)
            continue

        for line in proc.stdout:
            line = line.strip()
            if not line:
                continue
            try:
                snap = json.loads(line)
            except json.JSONDecodeError:
                continue
            sid = snap.get("session_id")
            if not sid:
                continue
            snap = _enrich_snapshot(snap)
            with _state_lock:
                _sessions[sid] = snap
            _broadcast({"type": "snapshot", "data": snap})

        sys.stderr.write("[server] claude-tokens exited; restarting in 2 s\n")
        try:
            err = proc.stderr.read() if proc.stderr else ""
            if err:
                sys.stderr.write(f"[server] stderr: {err[:500]}\n")
        except Exception:
            pass
        time.sleep(2)


# --- active-status sweeper ------------------------------------------------

def _is_claude_proc(pid_dir: Path) -> bool:
    """A process is a Claude Code instance if its exe lives under
    `claude/versions/`. The `comm` name is unreliable (it can be "claude" or the
    bare version like "2.1.186", e.g. for `claude --resume`), so key off the exe
    path and fall back to comm. Our own binary is `claude-tokens` (no match)."""
    try:
        if "/claude/versions/" in os.readlink(pid_dir / "exe"):
            return True
    except OSError:
        pass
    try:
        return (pid_dir / "comm").read_text().strip() == "claude"
    except OSError:
        return False


def _claude_live_cwd_counts() -> dict[str, int]:
    """How many running Claude Code processes work in each cwd. The count
    matters: N instances in one directory keep the N newest session files live.
    Mirrors the Rust CLI so the sweeper can flip sessions to closed."""
    counts: dict[str, int] = {}
    proc_root = Path("/proc")
    if not proc_root.is_dir():
        return counts
    for pid_dir in proc_root.iterdir():
        if not pid_dir.name.isdigit() or not _is_claude_proc(pid_dir):
            continue
        try:
            cwd = os.readlink(pid_dir / "cwd")
        except OSError:
            continue
        counts[cwd] = counts.get(cwd, 0) + 1
    return counts


def _is_among_newest_k(path_str: str | None, k: int) -> bool:
    """True if path_str is among the k newest .jsonl files in its directory
    (mirrors the Rust liveness check: N running instances keep the N newest
    session files live; older finished ones drop out)."""
    if not path_str or k <= 0:
        return False
    path = Path(path_str)
    try:
        self_mtime = path.stat().st_mtime
        parent = path.parent
    except OSError:
        return True
    newer = 0
    try:
        for p in parent.glob("*.jsonl"):
            if p == path:
                continue
            try:
                if p.stat().st_mtime > self_mtime:
                    newer += 1
                    if newer >= k:
                        return False
            except OSError:
                continue
    except OSError:
        return True
    return True


def _sweeper_thread() -> None:
    """claude-tokens emits a snapshot per token update. When a session closes,
    no further events come, so we never see session_active flip to false. This
    thread periodically re-checks liveness and broadcasts changed flags.
    """
    while True:
        time.sleep(3)
        with _state_lock:
            items = list(_sessions.items())
        counts = _claude_live_cwd_counts()
        for sid, snap in items:
            now_active = _is_among_newest_k(
                snap.get("session_path"), counts.get(snap.get("session_cwd"), 0)
            )
            # Re-stat the file mtime every cycle so the client's age stays exact
            # even between assistant events (e.g. while tool results stream in);
            # rebroadcast whenever the freshness or the active flag changed.
            path = snap.get("session_path")
            new_mtime = snap.get("last_modified")
            if path:
                try:
                    new_mtime = os.path.getmtime(path)
                except OSError:
                    pass
            if snap.get("session_active") != now_active or new_mtime != snap.get("last_modified"):
                snap["session_active"] = now_active
                snap["last_modified"] = new_mtime
                with _state_lock:
                    _sessions[sid] = snap
                _broadcast({"type": "snapshot", "data": snap})


def _find_session_for_sid(sid: str) -> Path | None:
    projects = _claude_home() / "projects"
    if not projects.is_dir():
        return None
    for path in projects.rglob(f"{sid}.jsonl"):
        return path
    return None


# --- recent chats -----------------------------------------------------------
#
# Data source: the same ~/.claude/projects/<encoded-cwd>/<uuid>.jsonl files the
# CLI tails, read-only. Session files can reach tens of MB, so we never load
# one fully: the title (`ai-title` record) and cwd sit within the first few
# lines (head read), the latest `last-prompt` record and the conversation tail
# sit at the end (tail read). Per-file metadata is cached by (mtime, size).

CHATS_HEAD_BYTES = 131072
CHATS_TAIL_BYTES = 262144
CHATS_META_TAIL_BYTES = 65536
CHAT_DETAIL_MAX_MSGS = 20
CHAT_DETAIL_SNIPPET_CHARS = 400
CHAT_TITLE_CHARS = 120
CHAT_LAST_PROMPT_CHARS = 160

_SID_RE = re.compile(r"[0-9a-fA-F][0-9a-fA-F-]{7,63}")
# User records whose text is tooling noise, not something the user typed:
# slash-command expansions (<command-name>/<command-message>/…), interrupt
# markers, hook caveats, local-command output wrappers.
_NON_PROMPT_RE = re.compile(r"^(<command-|<local-command-|\[Request interrupted|Caveat:)")

_chats_meta_lock = threading.Lock()
_chats_meta_cache: dict[str, tuple[float, int, dict]] = {}


def _read_head_lines(path: Path, max_bytes: int) -> list[str]:
    """First complete lines of a file, reading at most max_bytes."""
    with path.open("rb") as f:
        chunk = f.read(max_bytes)
    complete = len(chunk) < max_bytes
    lines = chunk.decode("utf-8", errors="replace").splitlines()
    if not complete and lines:
        lines.pop()  # last line may be cut mid-record
    return lines


def _read_tail_lines(path: Path, max_bytes: int) -> tuple[list[str], bool]:
    """Last complete lines of a file. Returns (lines, truncated) where
    truncated means the read did not start at the beginning of the file."""
    size = path.stat().st_size
    offset = max(0, size - max_bytes)
    with path.open("rb") as f:
        f.seek(offset)
        chunk = f.read(max_bytes)
    lines = chunk.decode("utf-8", errors="replace").splitlines()
    if offset > 0 and lines:
        lines.pop(0)  # first line may be cut mid-record
    return lines, offset > 0


def _snippet(text: str, limit: int) -> tuple[str, bool]:
    text = text.strip()
    if len(text) <= limit:
        return text, False
    return text[:limit].rstrip() + " …", True


def _is_real_user_text(rec: dict) -> str | None:
    """The text the user actually typed, or None. Tool results arrive as user
    records with array content; meta/sidechain/command records are noise."""
    if rec.get("type") != "user" or rec.get("isMeta") or rec.get("isSidechain"):
        return None
    content = (rec.get("message") or {}).get("content")
    if not isinstance(content, str):
        return None
    text = content.strip()
    if not text or _NON_PROMPT_RE.match(text):
        return None
    return text


def _extract_chat_meta(path: Path) -> dict:
    """Title/cwd/last-prompt of one session file, cached by (mtime, size)."""
    try:
        st = path.stat()
    except OSError:
        return {}
    key = str(path)
    with _chats_meta_lock:
        hit = _chats_meta_cache.get(key)
        if hit and hit[0] == st.st_mtime and hit[1] == st.st_size:
            return hit[2]

    meta: dict = {
        "title": None, "title_source": None,
        "cwd": None, "first_ts": None, "last_prompt": None,
    }
    first_user_text = None
    try:
        for line in _read_head_lines(path, CHATS_HEAD_BYTES):
            try:
                rec = json.loads(line)
            except json.JSONDecodeError:
                continue
            if not isinstance(rec, dict):
                continue
            if meta["title"] is None and rec.get("type") == "ai-title":
                title = rec.get("aiTitle")
                if isinstance(title, str) and title.strip():
                    meta["title"], _ = _snippet(title, CHAT_TITLE_CHARS)
                    meta["title_source"] = "ai-title"
            if meta["cwd"] is None and isinstance(rec.get("cwd"), str):
                meta["cwd"] = rec["cwd"]
            if meta["first_ts"] is None and isinstance(rec.get("timestamp"), str):
                meta["first_ts"] = rec["timestamp"]
            if first_user_text is None:
                first_user_text = _is_real_user_text(rec)
            if meta["title"] and meta["cwd"] and meta["first_ts"] and first_user_text:
                break
        if meta["title"] is None and first_user_text:
            meta["title"], _ = _snippet(first_user_text, CHAT_TITLE_CHARS)
            meta["title_source"] = "first_prompt"

        tail_lines, _tail_truncated = _read_tail_lines(path, CHATS_META_TAIL_BYTES)
        for line in reversed(tail_lines):
            if '"last-prompt"' not in line:
                continue
            try:
                rec = json.loads(line)
            except json.JSONDecodeError:
                continue
            if isinstance(rec, dict) and rec.get("type") == "last-prompt":
                prompt = rec.get("lastPrompt")
                if isinstance(prompt, str) and prompt.strip():
                    meta["last_prompt"], _ = _snippet(prompt, CHAT_LAST_PROMPT_CHARS)
                break
    except OSError:
        return {}

    with _chats_meta_lock:
        _chats_meta_cache[key] = (st.st_mtime, st.st_size, meta)
    return meta


def _list_recent_chats(per_dir: int, total: int) -> list[dict]:
    """The newest chats across all projects: per_dir newest per directory
    first, then the total newest of those overall (both limits combined)."""
    projects = _claude_home() / "projects"
    if not projects.is_dir():
        return []

    candidates: list[tuple[float, int, Path]] = []
    for proj_dir in projects.iterdir():
        if not proj_dir.is_dir():
            continue
        files: list[tuple[float, int, Path]] = []
        for path in proj_dir.glob("*.jsonl"):
            try:
                st = path.stat()
            except OSError:
                continue
            if st.st_size == 0:
                continue
            files.append((st.st_mtime, st.st_size, path))
        files.sort(key=lambda t: t[0], reverse=True)
        candidates.extend(files[:per_dir])

    candidates.sort(key=lambda t: t[0], reverse=True)

    with _state_lock:
        active_sids = {
            sid for sid, snap in _sessions.items() if snap.get("session_active")
        }

    now = time.time()
    chats: list[dict] = []
    for mtime, size, path in candidates:
        if len(chats) >= total:
            break
        meta = _extract_chat_meta(path)
        if not meta or (not meta.get("title") and not meta.get("last_prompt")):
            continue  # empty warm-up session — let older ones move up
        sid = path.stem
        cwd = meta.get("cwd")
        chats.append({
            "session_id": sid,
            "cwd": cwd,
            "dir_name": Path(cwd).name if cwd else path.parent.name,
            "mtime": mtime,
            "age_seconds": max(0, int(now - mtime)),
            "title": meta.get("title"),
            "title_source": meta.get("title_source"),
            "last_prompt": meta.get("last_prompt"),
            "size_bytes": size,
            "active": sid in active_sids,
        })
    return chats


def _latest_ai_title(path: Path) -> str | None:
    """The most recent aiTitle in a session file. Claude Code keeps the
    terminal window title in sync with it (e.g. "✳ Review next prompt …"),
    so for a busy session it is the only reliable window-match candidate —
    the cwd is no longer part of the terminal title then."""
    try:
        lines, _ = _read_tail_lines(path, CHATS_META_TAIL_BYTES)
    except OSError:
        return None
    for line in reversed(lines):
        if '"ai-title"' not in line:
            continue
        try:
            rec = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(rec, dict) and rec.get("type") == "ai-title":
            aititle = rec.get("aiTitle")
            if isinstance(aititle, str) and aititle.strip():
                return aititle.strip()
    return None


def _extract_chat_detail(path: Path) -> dict:
    """Condensed conversation tail: the last CHAT_DETAIL_MAX_MSGS displayable
    user/assistant messages, each truncated to CHAT_DETAIL_SNIPPET_CHARS."""
    lines, partial = _read_tail_lines(path, CHATS_TAIL_BYTES)
    messages: list[dict] = []
    for line in reversed(lines):
        if len(messages) >= CHAT_DETAIL_MAX_MSGS:
            break
        try:
            rec = json.loads(line)
        except json.JSONDecodeError:
            continue
        if not isinstance(rec, dict):
            continue
        text = None
        role = None
        user_text = _is_real_user_text(rec)
        if user_text:
            role, text = "user", user_text
        elif rec.get("type") == "assistant" and not rec.get("isSidechain"):
            msg = rec.get("message") or {}
            if msg.get("model") == "<synthetic>":
                continue  # interrupt/error placeholder, no real content
            content = msg.get("content")
            if isinstance(content, list):
                parts = [
                    b.get("text", "") for b in content
                    if isinstance(b, dict) and b.get("type") == "text"
                ]
                joined = "\n".join(p for p in parts if p).strip()
                if joined:
                    role, text = "assistant", joined
        if not text:
            continue
        snippet, truncated = _snippet(text, CHAT_DETAIL_SNIPPET_CHARS)
        messages.append({
            "role": role,
            "ts": rec.get("timestamp"),
            "text": snippet,
            "truncated": truncated,
        })
    messages.reverse()  # chronological
    return {"messages": messages, "partial": partial}


# --- window management (wmctrl) --------------------------------------------
#
# Both the ⚡ (focus terminal) and 📁 (open directory) actions target the
# workspace a project lives on: `wmctrl -ia` activates a window AND switches
# to its desktop, so finding the right window is the whole job. While a
# session runs, Claude Code replaces the terminal title with the current chat
# title, so matching needs the ai-title in addition to the cwd.

_IDE_CLASSES = (
    "code", "cursor", "windsurf", "vscodium", "code-oss",
    "jetbrains", "intellij", "pycharm", "webstorm", "phpstorm",
    "goland", "rustrover", "rider", "datagrip", "android-studio",
    "sublime_text", "atom", "zed",
)
_TERM_CLASSES = (
    "terminal", "alacritty", "kitty", "konsole", "xterm",
    "wezterm", "tilix", "guake", "tabby", "rxvt", "urxvt",
    "gnome-terminal", "xfce4-terminal", "qterminal", "foot",
)
_FILE_MANAGER_CLASSES = (
    "nautilus", "dolphin", "thunar", "nemo", "pcmanfm",
    "caja", "spacefm", "krusader",
)


def _wmctrl_windows() -> list[tuple[str, str, str, str]] | None:
    """Parsed `wmctrl -lx` lines as (win_id, desktop, wm_class, title).
    Raises FileNotFoundError if wmctrl is missing; returns None on failure."""
    result = subprocess.run(
        ["wmctrl", "-lx"], capture_output=True, text=True, check=False
    )
    if result.returncode != 0:
        return None
    windows = []
    for line in result.stdout.splitlines():
        parts = line.split(None, 4)
        if len(parts) < 5:
            continue
        win_id, desktop, wm_class, _host, title = parts
        windows.append((win_id, desktop, wm_class, title))
    return windows


def _activate_window(win_id: str) -> None:
    """Raise + focus a window; wmctrl also switches to its desktop."""
    subprocess.run(
        ["wmctrl", "-ia", win_id],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False,
    )


def _find_session_window(
    windows: list[tuple[str, str, str, str]], cwd: str, chat_title: str | None
) -> tuple[str, str] | None:
    """Best (win_id, desktop) for a session's IDE/terminal window, or None."""
    cwd_basename = Path(cwd).name
    candidates: list[tuple[int, str, str]] = []
    for win_id, desktop, wm_class, title in windows:
        title_match = chat_title is not None and chat_title in title
        if not title_match and cwd not in title and cwd_basename not in title:
            continue
        cls_lower = wm_class.lower()
        if any(fm in cls_lower for fm in _FILE_MANAGER_CLASSES):
            continue
        score = 1
        if any(ide in cls_lower for ide in _IDE_CLASSES):
            score = 10
        elif any(t in cls_lower for t in _TERM_CLASSES):
            score = 5
        if title_match:
            # The chat title identifies THE session's window; a cwd match
            # may hit any terminal that happens to sit in that directory.
            score += 20
        candidates.append((score, win_id, desktop))
    if not candidates:
        return None
    candidates.sort(key=lambda c: c[0], reverse=True)
    _, win_id, desktop = candidates[0]
    return win_id, desktop


def _find_file_manager_window(
    windows: list[tuple[str, str, str, str]], path: str
) -> str | None:
    """A file-manager window already showing `path` (title = the shown
    directory, possibly via a differently-prefixed route, e.g. a symlink)."""
    basename = Path(path).name
    for win_id, _desktop, wm_class, title in windows:
        if not any(fm in wm_class.lower() for fm in _FILE_MANAGER_CLASSES):
            continue
        if path in title or title.rstrip("/").endswith("/" + basename):
            return win_id
    return None


# All monitor windows carry this title prefix (index.html, help.html,
# chats.html). The /sticky endpoint only touches windows matching it, so it
# cannot be used to modify arbitrary windows.
STICKY_TITLE_PREFIX = "Claude Token Monitor"


def _make_windows_sticky(title: str) -> int:
    """Pin every monitor window whose title contains `title` to all
    workspaces. Retries briefly — right after window.open the X window may
    not be mapped yet. Returns the number of windows pinned."""
    for _ in range(6):
        windows = _wmctrl_windows()
        if windows is None:
            return 0
        stuck = 0
        for win_id, _desktop, _cls, wtitle in windows:
            if title in wtitle:
                subprocess.run(
                    ["wmctrl", "-i", "-r", win_id, "-b", "add,sticky"],
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                    check=False,
                )
                stuck += 1
        if stuck:
            return stuck
        time.sleep(0.3)
    return 0


def _chat_title_for_sid(sid) -> str | None:
    """Latest ai-title of a session, given a client-supplied sid (validated)."""
    if not isinstance(sid, str) or not _SID_RE.fullmatch(sid):
        return None
    session_path = _find_session_for_sid(sid)
    if session_path is None:
        return None
    return _latest_ai_title(session_path)


# --- HTTP handler ---------------------------------------------------------

class Handler(http.server.BaseHTTPRequestHandler):
    server_version = "claude-token-display/0.1"

    def log_message(self, fmt: str, *args) -> None:
        sys.stderr.write(f"[http] {self.address_string()} - {fmt % args}\n")

    def do_GET(self) -> None:
        url = urlparse(self.path)
        if url.path == "/":
            self._serve_file(HERE / "index.html", "text/html; charset=utf-8")
        elif url.path.startswith("/static/"):
            self._serve_static(url.path[len("/static/"):])
        elif url.path == "/events":
            self._serve_events()
        elif url.path == "/scope":
            self._json_ok(extra={"scope": _scope})
        elif url.path == "/settings":
            self._serve_settings()
        elif url.path == "/plan":
            self._serve_plan()
        elif url.path == "/readme":
            self._serve_file(HERE / "README.md", "text/markdown; charset=utf-8")
        elif url.path == "/readme-main":
            self._serve_file(HERE.parent / "README.md", "text/markdown; charset=utf-8")
        elif url.path == "/help":
            self._serve_file(HERE / "help.html", "text/html; charset=utf-8")
        elif url.path == "/chats":
            self._serve_file(HERE / "chats.html", "text/html; charset=utf-8")
        elif url.path == "/chats-data":
            self._serve_chats_data(url)
        elif url.path == "/chat-detail":
            self._serve_chat_detail(url)
        elif url.path in ("/icon.png", "/favicon.ico"):
            self._serve_file(HERE / "icon.png", "image/png")
        else:
            self.send_error(404, "not found")

    def do_POST(self) -> None:
        length = int(self.headers.get("Content-Length", "0"))
        raw = self.rfile.read(length) if length else b"{}"
        try:
            body = json.loads(raw.decode() or "{}")
        except json.JSONDecodeError:
            self.send_error(400, "bad json")
            return

        if self.path == "/open-readme-dir":
            self._handle_open_dir({"path": str(HERE)})
        elif self.path == "/open-dir":
            self._handle_open_dir(body)
        elif self.path == "/focus-terminal":
            self._handle_focus_terminal(body)
        elif self.path == "/copy":
            self._handle_copy(body)
        elif self.path == "/settings":
            self._handle_update_settings(body)
        elif self.path == "/sticky":
            self._handle_sticky(body)
        elif self.path == "/chat-delete":
            self._handle_chat_delete(body)
        else:
            self.send_error(404, "not found")

    # --- helpers ---

    def _serve_file(self, path: Path, ctype: str) -> None:
        try:
            data = path.read_bytes()
        except FileNotFoundError:
            self.send_error(404)
            return
        self.send_response(200)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def _serve_static(self, fname: str) -> None:
        static_root = HERE / "static"
        safe = static_root / fname
        if not safe.resolve().is_relative_to(static_root):
            self.send_error(403)
            return
        ctype = "application/octet-stream"
        if fname.endswith(".js"):
            ctype = "application/javascript; charset=utf-8"
        elif fname.endswith(".css"):
            ctype = "text/css; charset=utf-8"
        elif fname.endswith(".html"):
            ctype = "text/html; charset=utf-8"
        elif fname.endswith(".md"):
            ctype = "text/markdown; charset=utf-8"
        self._serve_file(safe, ctype)

    def _serve_events(self) -> None:
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("Connection", "keep-alive")
        self.end_headers()

        q: queue.Queue = queue.Queue(maxsize=256)
        with _subscribers_lock:
            _subscribers.append(q)

        try:
            with _state_lock:
                snapshots = list(_sessions.values())
            for snap in snapshots:
                self._sse_send({"type": "snapshot", "data": snap})

            last_heartbeat = time.time()
            while True:
                try:
                    event = q.get(timeout=SSE_HEARTBEAT_SEC)
                    self._sse_send(event)
                except queue.Empty:
                    pass
                if time.time() - last_heartbeat > SSE_HEARTBEAT_SEC:
                    try:
                        self.wfile.write(b": ping\n\n")
                        self.wfile.flush()
                    except (BrokenPipeError, ConnectionResetError):
                        break
                    last_heartbeat = time.time()
        finally:
            with _subscribers_lock:
                if q in _subscribers:
                    _subscribers.remove(q)

    def _sse_send(self, event: dict) -> None:
        try:
            payload = json.dumps(event)
            self.wfile.write(f"data: {payload}\n\n".encode())
            self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError):
            raise

    def _handle_open_dir(self, body: dict) -> None:
        path = body.get("path", "")
        if not path or not Path(path).is_dir():
            self.send_error(400, "missing or invalid 'path'")
            return

        # Land on the project's workspace instead of dropping the file manager
        # onto whatever desktop happens to be active: focus an existing file-
        # manager window for the path if there is one, otherwise switch to the
        # desktop of the session's terminal before opening a new one. Without
        # wmctrl this gracefully degrades to a plain xdg-open.
        windows = None
        try:
            windows = _wmctrl_windows()
        except FileNotFoundError:
            pass
        if windows:
            existing = _find_file_manager_window(windows, path)
            if existing:
                _activate_window(existing)
                self._json_ok(extra={"focused": True})
                return
            hit = _find_session_window(windows, path, _chat_title_for_sid(body.get("sid")))
            if hit and hit[1].isdigit():
                subprocess.run(
                    ["wmctrl", "-s", hit[1]],
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False,
                )

        try:
            subprocess.Popen(
                ["xdg-open", path],
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            )
        except FileNotFoundError:
            self.send_error(500, "xdg-open not installed")
            return
        self._json_ok()

    def _handle_focus_terminal(self, body: dict) -> None:
        cwd = body.get("cwd", "")
        if not cwd:
            self.send_error(400, "missing 'cwd'")
            return
        try:
            windows = _wmctrl_windows()
        except FileNotFoundError:
            self.send_error(500, "wmctrl not installed")
            return
        if windows is None:
            self._json_ok(extra={"warning": "wmctrl -lx failed"})
            return
        hit = _find_session_window(windows, cwd, _chat_title_for_sid(body.get("sid")))
        if hit is None:
            self._json_ok(extra={"warning": "no matching IDE/terminal window found"})
            return
        _activate_window(hit[0])
        self._json_ok()

    def _handle_copy(self, body: dict) -> None:
        text = body.get("text", "")
        if not text:
            self.send_error(400, "missing 'text'")
            return
        for cmd in (["xclip", "-selection", "clipboard"], ["wl-copy"]):
            try:
                p = subprocess.Popen(cmd, stdin=subprocess.PIPE)
                p.communicate(input=text.encode())
                if p.returncode == 0:
                    self._json_ok()
                    return
            except FileNotFoundError:
                continue
        self.send_error(500, "no clipboard tool found (install xclip or wl-clipboard)")

    def _serve_settings(self) -> None:
        body = json.dumps(_load_settings()).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _handle_update_settings(self, body: dict) -> None:
        current = _load_settings()
        pw_in = (body.get("plan_widget") or {})
        if isinstance(pw_in.get("enabled"), bool):
            current["plan_widget"]["enabled"] = pw_in["enabled"]
        if isinstance(pw_in.get("consent_acknowledged"), bool):
            current["plan_widget"]["consent_acknowledged"] = pw_in["consent_acknowledged"]
        rows_in = pw_in.get("rows") or {}
        if isinstance(rows_in, dict):
            for k, v in rows_in.items():
                if isinstance(v, bool):
                    current["plan_widget"]["rows"][k] = v
        ch_in = body.get("chats") or {}
        if isinstance(ch_in, dict):
            if "per_dir" in ch_in:
                current["chats"]["per_dir"] = _clamp_int(
                    ch_in["per_dir"], 1, CHATS_PER_DIR_MAX, current["chats"]["per_dir"])
            if "total" in ch_in:
                current["chats"]["total"] = _clamp_int(
                    ch_in["total"], 1, CHATS_TOTAL_MAX, current["chats"]["total"])
            if isinstance(ch_in.get("active_first"), bool):
                current["chats"]["active_first"] = ch_in["active_first"]
        win_in = body.get("windows") or {}
        if isinstance(win_in, dict) and isinstance(win_in.get("sticky"), bool):
            current["windows"]["sticky"] = win_in["sticky"]
        try:
            _save_settings(current)
        except OSError as e:
            self.send_error(500, f"could not persist settings: {e}")
            return
        global _plan_cache, _plan_cache_ts
        with _plan_cache_lock:
            _plan_cache = None
            _plan_cache_ts = 0.0
        self._json_ok(extra={"settings": current})

    def _serve_plan(self) -> None:
        settings = _load_settings()
        if not settings["plan_widget"]["enabled"]:
            self.send_response(403)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(json.dumps({
                "error": "plan widget not opted in", "code": "disabled",
            }).encode())
            return

        data, _exit_code = _get_plan_cached(_claude_tokens_bin)
        body = json.dumps(data).encode()
        self.send_response(200)  # always 200; UI reads `code`
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def _serve_chats_data(self, url) -> None:
        settings = _load_settings()["chats"]
        params = parse_qs(url.query)
        per_dir = _clamp_int(
            (params.get("per_dir") or [None])[0],
            1, CHATS_PER_DIR_MAX, settings["per_dir"])
        total = _clamp_int(
            (params.get("total") or [None])[0],
            1, CHATS_TOTAL_MAX, settings["total"])
        body = json.dumps({
            "per_dir": per_dir,
            "total": total,
            "chats": _list_recent_chats(per_dir, total),
        }).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def _serve_chat_detail(self, url) -> None:
        sid = (parse_qs(url.query).get("sid") or [""])[0]
        # Strict sid validation; the file is then resolved exclusively via the
        # glob under ~/.claude/projects — never from a client-supplied path.
        if not _SID_RE.fullmatch(sid):
            self.send_error(400, "invalid 'sid'")
            return
        path = _find_session_for_sid(sid)
        if path is None:
            self.send_error(404, "session not found")
            return
        meta = _extract_chat_meta(path)
        try:
            detail = _extract_chat_detail(path)
        except OSError:
            self.send_error(500, "could not read session file")
            return
        body = json.dumps({
            "session_id": sid,
            "cwd": meta.get("cwd"),
            "title": meta.get("title"),
            "messages": detail["messages"],
            "partial": detail["partial"],
        }).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def _handle_chat_delete(self, body: dict) -> None:
        sid = body.get("sid", "")
        # Same strict sid validation as /chat-detail; the file is resolved
        # exclusively via the glob under ~/.claude/projects — never from a
        # client-supplied path.
        if not isinstance(sid, str) or not _SID_RE.fullmatch(sid):
            self.send_error(400, "invalid 'sid'")
            return
        path = _find_session_for_sid(sid)
        if path is None:
            self.send_error(404, "session not found")
            return
        with _state_lock:
            active = bool((_sessions.get(sid) or {}).get("session_active"))
        if active:
            self.send_error(409, "session is running")
            return
        try:
            path.unlink()
        except OSError:
            self.send_error(500, "could not delete session file")
            return
        with _chats_meta_lock:
            _chats_meta_cache.pop(str(path), None)
        with _state_lock:
            _sessions.pop(sid, None)
        self._json_ok(extra={"deleted": sid})

    def _handle_sticky(self, body: dict) -> None:
        title = body.get("title", "")
        if not isinstance(title, str) or not title.startswith(STICKY_TITLE_PREFIX):
            self.send_error(400, "invalid 'title'")
            return
        if not _load_settings()["windows"]["sticky"]:
            self._json_ok(extra={"skipped": "disabled"})
            return
        try:
            stuck = _make_windows_sticky(title)
        except FileNotFoundError:
            self._json_ok(extra={"warning": "wmctrl not installed"})
            return
        self._json_ok(extra={"stuck": stuck})

    def _json_ok(self, extra: dict | None = None) -> None:
        payload = {"ok": True}
        if extra:
            payload.update(extra)
        body = json.dumps(payload).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


class ThreadedHTTPServer(socketserver.ThreadingMixIn, http.server.HTTPServer):
    daemon_threads = True
    allow_reuse_address = True


def main() -> int:
    import argparse

    parser = argparse.ArgumentParser(description="claude-token-display HTTP/SSE bridge")
    parser.add_argument(
        "cwd", nargs="?", default=None,
        help="Project directory to watch (omit for system-wide view of all Claude sessions)",
    )
    parser.add_argument(
        "--port", type=int,
        default=int(os.environ.get("CLAUDE_TOKEN_DISPLAY_PORT") or DEFAULT_PORT),
    )
    parser.add_argument(
        "--claude-tokens-bin",
        default=os.environ.get("CLAUDE_TOKENS_BIN") or "claude-tokens",
    )
    args = parser.parse_args()

    if args.cwd is not None and not Path(args.cwd).is_dir():
        sys.stderr.write(f"[server] cwd does not exist: {args.cwd}\n")
        return 2

    global _scope, _claude_tokens_bin
    _scope = str(Path(args.cwd).resolve()) if args.cwd else None
    _claude_tokens_bin = args.claude_tokens_bin

    t1 = threading.Thread(
        target=_reader_thread, args=(args.cwd, args.claude_tokens_bin), daemon=True
    )
    t1.start()
    t2 = threading.Thread(target=_sweeper_thread, daemon=True)
    t2.start()

    server = ThreadedHTTPServer(("127.0.0.1", args.port), Handler)
    sys.stderr.write(f"[server] listening on http://127.0.0.1:{args.port}\n")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        sys.stderr.write("\n[server] shutting down\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
