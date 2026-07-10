//! Minimal serde mirrors of the Claude Code session JSONL shapes we consume.
//!
//! Claude Code writes one JSON object per line under
//! `~/.claude/projects/<encoded-cwd>/<uuid>.jsonl`. Each line is tagged by a
//! `type` field. We only declare the fields this tool needs; unknown fields and
//! unknown event types are ignored (`#[serde(other)]`), so additions on the
//! Claude side do not break us.

use serde::Deserialize;

/// One line of a Claude Code session file.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ClaudeEvent {
    /// A model turn. Carries `message.usage` (token counts) and `message.model`.
    Assistant(AssistantEvent),
    /// A user turn. The only place the working directory is reliably recorded.
    User(MetaEvent),
    /// System events; `subtype == "compact_boundary"` marks a context compaction.
    System(SystemEvent),
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssistantEvent {
    pub message: AssistantMessage,
    /// Present on most events; carries the cwd. Used as a fallback source.
    #[serde(default)]
    pub cwd: Option<String>,
    /// ISO-8601 wall-clock time of the event.
    #[serde(default)]
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssistantMessage {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub usage: Option<Usage>,
}

/// Token usage as reported by Claude on each assistant turn. Note that
/// `input_tokens` here is only the *fresh, non-cached* prompt tokens (typically
/// tiny); the bulk of the prompt is split across the two cache fields.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: i64,
    #[serde(default)]
    pub cache_creation_input_tokens: i64,
    #[serde(default)]
    pub cache_read_input_tokens: i64,
    #[serde(default)]
    pub output_tokens: i64,
}

impl Usage {
    /// Full prompt the model read for this turn (fresh + cache-create + cache-read).
    pub fn prompt_tokens(&self) -> i64 {
        self.input_tokens + self.cache_creation_input_tokens + self.cache_read_input_tokens
    }
    /// Everything occupying the context window after this turn (prompt + output).
    pub fn context_tokens(&self) -> i64 {
        self.prompt_tokens() + self.output_tokens
    }
    /// Tokens that were served from / written to cache (the "cached" portion).
    pub fn cached_tokens(&self) -> i64 {
        self.cache_creation_input_tokens + self.cache_read_input_tokens
    }
}

/// An event that carries a `cwd` we want (user turns, and others).
#[derive(Debug, Clone, Deserialize)]
pub struct MetaEvent {
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SystemEvent {
    #[serde(default)]
    pub subtype: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub timestamp: Option<String>,
}
