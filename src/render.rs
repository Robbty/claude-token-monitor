//! Stdout rendering for the token state. Two formats: `kv` (KEY=VALUE, default,
//! shell-friendly) and `json` (single JSON object per snapshot).

use serde_json::json;

use crate::protocol::Usage;
use crate::state::TokenState;

pub enum Format {
    Kv,
    Json,
}

pub fn render(state: &TokenState, format: &Format, multi: bool) -> String {
    match format {
        Format::Kv => render_kv(state, multi),
        Format::Json => render_json(state),
    }
}

fn render_kv(s: &TokenState, multi: bool) -> String {
    let mut out = String::new();

    // In multi-session mode prepend a visible block header so a human can spot
    // session boundaries. Scripts keep using `session_id=...` as the anchor.
    if multi && let Some(id) = &s.session_id {
        out.push_str("=== session ");
        out.push_str(id);
        out.push_str(" ===\n");
    }

    if let Some(id) = &s.session_id {
        push_kv(&mut out, "session_id", id);
    }
    if let Some(cwd) = &s.session_cwd {
        push_kv(&mut out, "session_cwd", cwd);
    }
    if let Some(active) = s.session_active {
        push_kv(&mut out, "session_active", if active { "true" } else { "false" });
    }
    if let Some(model) = &s.model {
        push_kv(&mut out, "model", model);
    }
    push_kv(&mut out, "compact_count", &s.compact_count.to_string());
    push_kv(&mut out, "turns", &s.turns.to_string());

    if let Some(w) = s.context_window() {
        push_kv(&mut out, "context_window", &w.to_string());
    }
    if let Some(p) = s.percent_left() {
        push_kv(&mut out, "percent_left", &p.to_string());
    }
    if let Some(p) = s.percent_used() {
        push_kv(&mut out, "percent_used", &p.to_string());
    }
    if let Some(u) = s.tokens_in_context() {
        push_kv(&mut out, "tokens_in_context", &u.to_string());
    }
    push_kv(&mut out, "session_total_tokens", &s.session_total_tokens().to_string());

    push_kv(&mut out, "total_input_tokens", &s.total_input_tokens().to_string());
    push_kv(&mut out, "total_cached_input_tokens", &s.total_cached_input_tokens().to_string());
    push_kv(&mut out, "total_output_tokens", &s.total_output_tokens().to_string());
    push_kv(&mut out, "total_tokens", &s.total_tokens().to_string());

    if let Some(u) = &s.last {
        push_kv(&mut out, "last_input_tokens", &u.prompt_tokens().to_string());
        push_kv(&mut out, "last_cached_input_tokens", &u.cached_tokens().to_string());
        push_kv(&mut out, "last_output_tokens", &u.output_tokens.to_string());
        push_kv(&mut out, "last_total_tokens", &u.context_tokens().to_string());
    }

    out
}

fn push_kv(out: &mut String, key: &str, value: &str) {
    out.push_str(key);
    out.push('=');
    out.push_str(value);
    out.push('\n');
}

fn last_json(u: &Usage) -> serde_json::Value {
    json!({
        "input_tokens": u.prompt_tokens(),
        "cached_input_tokens": u.cached_tokens(),
        "output_tokens": u.output_tokens,
        "total_tokens": u.context_tokens(),
    })
}

fn render_json(s: &TokenState) -> String {
    let v = json!({
        "session_id": s.session_id,
        "session_cwd": s.session_cwd,
        "session_active": s.session_active,
        "model": s.model,
        "compact_count": s.compact_count,
        "turns": s.turns,
        "context_window": s.context_window(),
        "percent_left": s.percent_left(),
        "percent_used": s.percent_used(),
        "tokens_in_context": s.tokens_in_context(),
        "session_total_tokens": s.session_total_tokens(),
        "total": {
            "input_tokens": s.total_input_tokens(),
            "cached_input_tokens": s.total_cached_input_tokens(),
            "output_tokens": s.total_output_tokens(),
            "total_tokens": s.total_tokens(),
        },
        "last": s.last.as_ref().map(last_json),
    });
    serde_json::to_string(&v).unwrap_or_default()
}
