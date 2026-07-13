//! Map a Claude model id to its context-window size.
//!
//! Claude Code never records the context window in the session file
//! (verified across all local sessions: no structured `context_window` /
//! `max_input_tokens` field anywhere), so we derive it from `message.model`.
//! The table is prefix-matched and easy to extend.
//!
//! Note: Claude Code runs Opus, Sonnet **and Haiku** 4.x with a **1M**
//! context window. Verified empirically from real session files — an
//! Opus-4-7 session reached `cache_read_input_tokens` of 643k and a
//! Sonnet-4-6 session 320k without auto-compaction, and a Haiku-4-5 session
//! with 188.8k context showed 18–19% in Claude Code itself (188 788 / 1M),
//! not the 94% a 200k window would imply. Unknown models default to 1M so a
//! new large-context model never renders an impossible ">100%" bar.
//!
//! As a safety net against future window changes, `TokenState` additionally
//! bumps the table value whenever a session's *observed* context exceeds it
//! (see `context_window_at_least`) — the prompt can never exceed the real
//! window, so the observation is a hard lower bound.

pub const DEFAULT_CONTEXT_WINDOW: i64 = 1_000_000;

const TABLE: &[(&str, i64)] = &[
    ("claude-opus-4", 1_000_000),
    ("claude-sonnet-4", 1_000_000),
    ("claude-haiku-4", 1_000_000),
];

/// Known context-window sizes, ascending — used to round an observed
/// lower bound up to the next plausible window.
const KNOWN_WINDOWS: &[i64] = &[200_000, 500_000, 1_000_000];

/// Returns the context-window size (in tokens) for a given model id.
pub fn context_window_for(model: &str) -> i64 {
    for (prefix, window) in TABLE {
        if model.starts_with(prefix) {
            return *window;
        }
    }
    DEFAULT_CONTEXT_WINDOW
}

/// Returns the smallest plausible context window that is ≥ both the table
/// value for `model` and the observed context size `observed_tokens`.
///
/// A session's prompt can never exceed the real window, so an observation
/// above the table value proves the table is outdated for this model; we
/// then round up to the next known window size (or the next 100k step
/// beyond the largest known one) instead of showing >100%.
pub fn context_window_at_least(model: &str, observed_tokens: i64) -> i64 {
    let table = context_window_for(model);
    if observed_tokens <= table {
        return table;
    }
    for w in KNOWN_WINDOWS {
        if *w >= observed_tokens {
            return *w;
        }
    }
    // Beyond every known window: round up to the next 100k.
    ((observed_tokens + 99_999) / 100_000) * 100_000
}
