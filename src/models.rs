//! Map a Claude model id to its context-window size.
//!
//! Claude Code never records the context window in the session file, so we
//! derive it from `message.model`. The table is prefix-matched and easy to
//! extend.
//!
//! Note: Claude Code runs Opus and Sonnet 4.x with a **1M** context window.
//! This was verified empirically from real session files — an Opus-4-7 session
//! reached `cache_read_input_tokens` of 643k and a Sonnet-4-6 session 320k, both
//! without an auto-compaction; since the cached prompt can never exceed the
//! window, the window must be ≥1M. (The handoff's assumed 200k was outdated.)
//! Haiku 4.x keeps its documented 200k. Unknown models default to 1M so a new
//! large-context model never renders an impossible ">100%" bar.

pub const DEFAULT_CONTEXT_WINDOW: i64 = 1_000_000;

const TABLE: &[(&str, i64)] = &[
    ("claude-opus-4", 1_000_000),
    ("claude-sonnet-4", 1_000_000),
    ("claude-haiku-4", 200_000),
];

/// Returns the context-window size (in tokens) for a given model id.
pub fn context_window_for(model: &str) -> i64 {
    for (prefix, window) in TABLE {
        if model.starts_with(prefix) {
            return *window;
        }
    }
    DEFAULT_CONTEXT_WINDOW
}
