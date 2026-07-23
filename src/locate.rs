//! Locate the session JSONL file(s) that belong to Claude Code sessions.
//!
//! Layout: `~/.claude/projects/<encoded-cwd>/<uuid>.jsonl`. The directory name
//! is the session's working directory with every non-alphanumeric character
//! replaced by `-` (e.g. `/home/peter/nc_peter` -> `-home-peter-nc-peter`).
//! The file stem is the session UUID.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, anyhow};

#[derive(Clone)]
pub enum Selector {
    /// Bind to a specific session by UUID (the file stem).
    ThreadId(String),
    /// Bind to the session(s) whose cwd encodes to this path's project dir.
    Cwd(PathBuf),
    /// Single newest session across all projects.
    MostRecent,
    /// System-wide: every session, no cwd filter.
    All,
}

pub fn claude_home() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("CLAUDE_HOME") {
        return Ok(PathBuf::from(p));
    }
    let home = dirs::home_dir().ok_or_else(|| anyhow!("could not determine home directory"))?;
    Ok(home.join(".claude"))
}

/// Encode an absolute cwd to its Claude `projects/` directory name. Claude
/// Code replaces every non-alphanumeric character with `-` (verified: `_`
/// becomes `-` too — `/home/peter/nc_peter/…` maps to `-home-peter-nc-peter-…`,
/// so replacing only `/` and `.` would miss such projects entirely).
pub fn encode_cwd(path: &Path) -> String {
    path.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

fn projects_dir(claude_home: &Path) -> Result<PathBuf> {
    let projects = claude_home.join("projects");
    if !projects.is_dir() {
        return Err(anyhow!(
            "projects directory not found: {}",
            projects.display()
        ));
    }
    Ok(projects)
}

pub fn resolve_filtered(sel: &Selector, claude_home: &Path, require_open: bool) -> Result<PathBuf> {
    let projects = projects_dir(claude_home)?;
    let files = list_sessions(&projects)?;
    if files.is_empty() {
        return Err(anyhow!("no session files under {}", projects.display()));
    }

    match sel {
        Selector::ThreadId(id) => {
            let needle = format!("{id}.jsonl");
            files
                .into_iter()
                .find(|(p, _)| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n == needle)
                        && (!require_open || crate::proc::is_path_live_now(p))
                })
                .map(|(p, _)| p)
                .ok_or_else(|| anyhow!("no session for id {id}"))
        }
        Selector::Cwd(target) => {
            let slug = encode_target(target);
            files
                .into_iter()
                .find(|(p, _)| {
                    parent_name_is(p, &slug) && (!require_open || crate::proc::is_path_live_now(p))
                })
                .map(|(p, _)| p)
                .ok_or_else(|| anyhow!("no session whose cwd matches {}", target.display()))
        }
        Selector::MostRecent | Selector::All => files
            .into_iter()
            .find(|(p, _)| !require_open || crate::proc::is_path_live_now(p))
            .map(|(p, _)| p)
            .ok_or_else(|| anyhow!("no open session files")),
    }
}

/// Returns ALL session files matching the selector that are newer than `max_age`,
/// sorted newest first by mtime.
pub fn resolve_all(
    sel: &Selector,
    claude_home: &Path,
    max_age: Duration,
    require_open: bool,
) -> Result<Vec<PathBuf>> {
    let projects = projects_dir(claude_home)?;
    let all_files = list_sessions(&projects)?;
    let cutoff = SystemTime::now()
        .checked_sub(max_age)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let fresh: Vec<_> = all_files
        .into_iter()
        .filter(|(_, mtime)| *mtime >= cutoff)
        .collect();

    match sel {
        Selector::ThreadId(_) | Selector::MostRecent => {
            // These ask for a specific session; reuse single-session resolve so
            // behavior stays consistent (max_age intentionally not applied).
            Ok(vec![resolve_filtered(sel, claude_home, require_open)?])
        }
        Selector::Cwd(target) => {
            let slug = encode_target(target);
            let matches: Vec<PathBuf> = fresh
                .into_iter()
                .filter(|(p, _)| {
                    parent_name_is(p, &slug) && (!require_open || crate::proc::is_path_live_now(p))
                })
                .map(|(p, _)| p)
                .collect();
            if matches.is_empty() {
                Err(anyhow!(
                    "no active session whose cwd matches {} (within max-age window)",
                    target.display()
                ))
            } else {
                Ok(matches)
            }
        }
        Selector::All => {
            let matches: Vec<PathBuf> = fresh
                .into_iter()
                .filter(|(p, _)| !require_open || crate::proc::is_path_live_now(p))
                .map(|(p, _)| p)
                .collect();
            if matches.is_empty() {
                Err(anyhow!(
                    "no active session files under {} (within max-age window)",
                    projects.display()
                ))
            } else {
                Ok(matches)
            }
        }
    }
}

/// True if `path` is a subagent ("worker") transcript. Claude Code stores
/// them under `projects/<slug>/<session-uuid>/subagents/agent-<id>.jsonl` —
/// same record format as a main session, but written by a background agent
/// (Agent tool / workflows) instead of the interactive chat.
pub fn is_worker_path(path: &Path) -> bool {
    path.parent()
        .and_then(|d| d.file_name())
        .and_then(|n| n.to_str())
        == Some("subagents")
}

/// The encoded-cwd project slug a session file belongs to: the directory
/// directly under `projects/`. For main sessions that is the parent; for
/// worker transcripts it sits three levels up.
pub fn project_slug_of(path: &Path) -> Option<&str> {
    let dir = if is_worker_path(path) {
        path.parent()?.parent()?.parent()?
    } else {
        path.parent()?
    };
    dir.file_name()?.to_str()
}

/// For a worker transcript, the main-session file it belongs to
/// (`projects/<slug>/<session-uuid>.jsonl`, sibling of the `<session-uuid>/`
/// directory). None for non-worker paths.
pub fn worker_parent_session(path: &Path) -> Option<PathBuf> {
    if !is_worker_path(path) {
        return None;
    }
    let session_dir = path.parent()?.parent()?;
    Some(session_dir.with_extension("jsonl"))
}

/// The parent session UUID of a worker transcript, None for main sessions.
pub fn worker_parent_session_id(path: &Path) -> Option<String> {
    worker_parent_session(path)
        .as_deref()
        .and_then(session_id_from_path)
}

fn encode_target(target: &Path) -> String {
    let abs = target.canonicalize().unwrap_or_else(|_| target.to_path_buf());
    encode_cwd(&abs)
}

fn parent_name_is(path: &Path, slug: &str) -> bool {
    // Named for the common case; worker transcripts resolve their slug from
    // three levels up, so a --cwd scope includes a project's workers too.
    project_slug_of(path) == Some(slug)
}

/// Returns session files (`projects/*/*.jsonl`) sorted newest first by mtime.
fn list_sessions(projects_dir: &Path) -> Result<Vec<(PathBuf, SystemTime)>> {
    let mut out: Vec<(PathBuf, SystemTime)> = Vec::new();
    walk(projects_dir, &mut out)?;
    out.sort_by(|a, b| b.1.cmp(&a.1));
    Ok(out)
}

fn walk(dir: &Path, out: &mut Vec<(PathBuf, SystemTime)>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("read_dir {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            walk(&path, out)?;
        } else if file_type.is_file()
            && path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".jsonl"))
        {
            let mtime = entry.metadata()?.modified()?;
            out.push((path, mtime));
        }
    }
    Ok(())
}

/// Extract the session UUID (file stem) from a session path.
pub fn session_id_from_path(path: &Path) -> Option<String> {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
}
