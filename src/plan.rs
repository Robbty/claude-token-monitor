//! `claude-tokens plan` — query the account-level rate-limit endpoint.
//!
//! Reads the OAuth access token that Claude Code persists under
//! `~/.claude/.credentials.json` (key `claudeAiOauth.accessToken`) and calls
//! `https://api.anthropic.com/api/oauth/usage`. The response carries the
//! account's rolling 5h / 7d utilisation windows plus per-feature buckets and
//! the extra-usage credit balance.
//!
//! On success the raw upstream JSON is forwarded verbatim to stdout. On failure
//! a small `{"error": "...", "code": "..."}` envelope is emitted and a non-zero
//! exit code is returned so a caller can render a fallback state without parsing
//! prose.

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

const ENDPOINT: &str = "https://api.anthropic.com/api/oauth/usage";
const OAUTH_BETA: &str = "oauth-2025-04-20";
const API_VERSION: &str = "2023-06-01";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Exit codes. A caller distinguishes states purely by exit code.
pub mod exit {
    pub const OK: i32 = 0;
    pub const AUTH_MISSING: i32 = 2;
    pub const TOKEN_EXPIRED: i32 = 3;
    pub const NETWORK: i32 = 4;
}

#[derive(Debug)]
enum PlanError {
    AuthMissing(String),
    TokenExpired,
    Network(String),
}

impl PlanError {
    fn code(&self) -> &'static str {
        match self {
            PlanError::AuthMissing(_) => "auth_missing",
            PlanError::TokenExpired => "token_expired",
            PlanError::Network(_) => "network",
        }
    }
    fn exit_code(&self) -> i32 {
        match self {
            PlanError::AuthMissing(_) => exit::AUTH_MISSING,
            PlanError::TokenExpired => exit::TOKEN_EXPIRED,
            PlanError::Network(_) => exit::NETWORK,
        }
    }
    fn message(&self) -> String {
        match self {
            PlanError::AuthMissing(detail) => format!("not logged in: {detail}"),
            PlanError::TokenExpired => {
                "access token rejected — use Claude Code to refresh the login".into()
            }
            PlanError::Network(detail) => format!("upstream call failed: {detail}"),
        }
    }
}

/// Entry point for `claude-tokens plan`. Returns the process exit code.
pub fn run(claude_home: Option<PathBuf>) -> i32 {
    let home = match claude_home.or_else(|| crate::locate::claude_home().ok()) {
        Some(p) => p,
        None => return emit_error(&PlanError::AuthMissing("CLAUDE_HOME not resolvable".into())),
    };

    let token = match read_access_token(&home) {
        Ok(t) => t,
        Err(e) => return emit_error(&e),
    };

    match fetch_usage(&token) {
        Ok(value) => {
            // Forward the upstream JSON verbatim. The shape is `five_hour`,
            // `seven_day`, per-feature buckets and `extra_usage`.
            println!("{}", value);
            exit::OK
        }
        Err(e) => emit_error(&e),
    }
}

fn emit_error(err: &PlanError) -> i32 {
    let envelope = json!({ "error": err.message(), "code": err.code() });
    println!("{}", envelope);
    err.exit_code()
}

fn read_access_token(claude_home: &std::path::Path) -> Result<String, PlanError> {
    let auth_path = claude_home.join(".credentials.json");
    let raw = std::fs::read_to_string(&auth_path)
        .map_err(|e| PlanError::AuthMissing(format!("{}: {e}", auth_path.display())))?;
    let parsed: Value = serde_json::from_str(&raw)
        .map_err(|e| PlanError::AuthMissing(format!(".credentials.json malformed: {e}")))?;
    let oauth = parsed
        .get("claudeAiOauth")
        .ok_or_else(|| PlanError::AuthMissing("claudeAiOauth missing (API-key login?)".into()))?;

    // Proactively report expiry so the UI doesn't need a round-trip to learn it.
    if let Some(expires_at) = oauth.get("expiresAt").and_then(Value::as_i64) {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        if now_ms >= expires_at {
            return Err(PlanError::TokenExpired);
        }
    }

    let token = oauth
        .get("accessToken")
        .and_then(Value::as_str)
        .ok_or_else(|| PlanError::AuthMissing("claudeAiOauth.accessToken missing".into()))?;
    Ok(token.to_string())
}

fn fetch_usage(access_token: &str) -> Result<Value, PlanError> {
    let agent = ureq::AgentBuilder::new()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(concat!("claude-token-monitor/", env!("CARGO_PKG_VERSION")))
        .build();

    let resp = agent
        .get(ENDPOINT)
        .set("Authorization", &format!("Bearer {access_token}"))
        .set("anthropic-beta", OAUTH_BETA)
        .set("anthropic-version", API_VERSION)
        .set("Accept", "application/json")
        .call();

    match resp {
        Ok(r) => r
            .into_json::<Value>()
            .map_err(|e| PlanError::Network(format!("response body parse: {e}"))),
        Err(ureq::Error::Status(401, _)) | Err(ureq::Error::Status(403, _)) => {
            Err(PlanError::TokenExpired)
        }
        Err(ureq::Error::Status(code, resp)) => {
            let body = resp.into_string().unwrap_or_default();
            Err(PlanError::Network(format!(
                "HTTP {code}: {}",
                body.chars().take(200).collect::<String>()
            )))
        }
        Err(ureq::Error::Transport(t)) => Err(PlanError::Network(t.to_string())),
    }
}
