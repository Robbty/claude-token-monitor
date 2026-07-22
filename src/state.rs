//! Aggregated token state assembled from the stream of Claude session events.
//!
//! Unlike Codex, Claude does not emit server-summed totals or a context-window
//! size — we accumulate per assistant turn and derive everything ourselves.

use crate::models::context_window_at_least;
use crate::protocol::{AssistantEvent, Usage};

#[derive(Default, Debug, Clone)]
pub struct TokenState {
    pub session_id: Option<String>,
    pub session_cwd: Option<String>,
    /// True for subagent ("worker") transcripts under
    /// `<session-uuid>/subagents/agent-*.jsonl` — background agents spawned by
    /// a main session, same record format but no interactive chat.
    pub is_worker: bool,
    /// UUID of the main session a worker belongs to (None for main sessions).
    pub parent_session_id: Option<String>,
    /// ISO-8601 timestamp of the earliest event seen — the session's start
    /// (events arrive in file order, so the first timestamped one wins).
    pub started_at: Option<String>,
    /// `Some(true)` if a process currently holds the session file open for
    /// writing; `Some(false)` if not; `None` if the check could not run.
    pub session_active: Option<bool>,
    /// Number of `compact_boundary` system events seen. Each is one auto-compact
    /// (context hit the limit) or a manual `/compact`.
    pub compact_count: i64,
    /// Last non-synthetic model id seen on an assistant turn.
    pub model: Option<String>,
    /// Last reasoning effort seen on an assistant turn (`low`/`medium`/`high`).
    pub effort: Option<String>,
    /// Number of assistant turns with usage data.
    pub turns: i64,

    // Accumulators across the whole session.
    pub sum_input: i64,
    pub sum_cache_creation: i64,
    pub sum_cache_read: i64,
    pub sum_output: i64,

    /// Usage of the most recent assistant turn (= what occupies the context now).
    pub last: Option<Usage>,

    /// Largest context occupancy observed on any turn. A prompt can never
    /// exceed the real context window, so this is a hard lower bound for it —
    /// used to self-correct an outdated model table (never show >100%).
    pub max_context_tokens: i64,
}

impl TokenState {
    pub fn apply_assistant(&mut self, ev: &AssistantEvent) {
        // `<synthetic>` events are placeholders (interrupts, API errors, …) that
        // carry zero usage. Counting them would reset `last` to 0 and skew the
        // context-occupancy reading, so we ignore them entirely.
        if ev.message.model.as_deref() == Some("<synthetic>") {
            return;
        }
        if let Some(model) = ev.message.model.as_deref() {
            self.model = Some(model.to_string());
        }
        if let Some(effort) = ev.effort.as_deref() {
            self.effort = Some(effort.to_string());
        }
        if let Some(u) = ev.message.usage.as_ref() {
            self.sum_input += u.input_tokens;
            self.sum_cache_creation += u.cache_creation_input_tokens;
            self.sum_cache_read += u.cache_read_input_tokens;
            self.sum_output += u.output_tokens;
            self.max_context_tokens = self.max_context_tokens.max(u.context_tokens());
            self.last = Some(u.clone());
            self.turns += 1;
        }
        if self.session_cwd.is_none()
            && let Some(cwd) = ev.cwd.as_ref()
        {
            self.session_cwd = Some(cwd.clone());
        }
    }

    pub fn note_cwd(&mut self, cwd: &str) {
        if self.session_cwd.is_none() {
            self.session_cwd = Some(cwd.to_string());
        }
    }

    pub fn note_timestamp(&mut self, ts: Option<&str>) {
        if self.started_at.is_none()
            && let Some(ts) = ts
        {
            self.started_at = Some(ts.to_string());
        }
    }

    pub fn context_window(&self) -> Option<i64> {
        self.model
            .as_deref()
            .map(|m| context_window_at_least(m, self.max_context_tokens))
    }

    /// Tokens currently occupying the context window (last turn's prompt + output).
    pub fn tokens_in_context(&self) -> Option<i64> {
        self.last.as_ref().map(|u| u.context_tokens())
    }

    pub fn percent_used(&self) -> Option<i64> {
        let window = self.context_window()?;
        let used = self.tokens_in_context()?;
        if window <= 0 {
            return Some(0);
        }
        Some(
            ((used as f64 / window as f64) * 100.0)
                .clamp(0.0, 100.0)
                .round() as i64,
        )
    }

    pub fn percent_left(&self) -> Option<i64> {
        self.percent_used().map(|u| 100 - u)
    }

    /// Total input tokens billed across the session (fresh + both cache fields).
    pub fn total_input_tokens(&self) -> i64 {
        self.sum_input + self.sum_cache_creation + self.sum_cache_read
    }

    pub fn total_cached_input_tokens(&self) -> i64 {
        self.sum_cache_creation + self.sum_cache_read
    }

    pub fn total_output_tokens(&self) -> i64 {
        self.sum_output
    }

    pub fn total_tokens(&self) -> i64 {
        self.total_input_tokens() + self.total_output_tokens()
    }

    /// Accumulated tokens across the whole chat (includes compaction turns).
    pub fn session_total_tokens(&self) -> i64 {
        self.total_tokens()
    }
}
