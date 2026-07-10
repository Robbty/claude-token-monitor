//! Detect whether a session JSONL file is currently held open by any process.
//!
//! Linux-only: walks `/proc/<pid>/fd/` and compares symlink targets. Claude Code
//! keeps the session file open with a write handle for the duration of a
//! session, so "any process holds this file for writing" is a precise
//! active/closed indicator — far better than the mtime heuristic, which mistakes
//! long idle pauses for closed sessions.
//!
//! Returns `false` on non-Linux or if `/proc` cannot be read. Reads only the
//! entries the current user owns — no elevated privileges needed.
//!
//! Note: unlike Codex, Claude Code does **not** keep the session file open with
//! a persistent write handle — it appends and closes. So the open-handle check
//! almost always reports "closed" for Claude. The primary liveness signal is
//! therefore process-based: count the running `claude` processes whose working
//! directory encodes to a project slug (see [`live_project_slug_counts`]).

use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Scan `/proc` for running Claude Code processes and return, per project slug
/// (encoded cwd), **how many** of them are working there. The count matters:
/// each running instance writes its own session file, so N concurrent processes
/// in one directory means the N newest `.jsonl` files there are live (older ones
/// are finished sessions).
pub fn live_project_slug_counts() -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    let proc_dir = match fs::read_dir("/proc") {
        Ok(d) => d,
        Err(_) => return counts,
    };
    for entry in proc_dir.flatten() {
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else { continue };
        if !name_str.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let pid_path = entry.path();
        if !is_claude_process(&pid_path) {
            continue;
        }
        if let Ok(cwd) = fs::read_link(pid_path.join("cwd")) {
            *counts.entry(crate::locate::encode_cwd(&cwd)).or_insert(0) += 1;
        }
    }
    counts
}

/// A process is a Claude Code instance if its executable lives under a
/// `claude/versions/` directory. The `comm` name is unreliable: depending on how
/// the session was launched (e.g. `claude --resume`, which re-execs the
/// versioned binary directly), `comm` is either "claude" or the bare version
/// string like "2.1.186" — so we key off the exe path and fall back to comm.
/// Our own binary is `claude-tokens`, which matches neither.
fn is_claude_process(pid_path: &Path) -> bool {
    if let Ok(exe) = fs::read_link(pid_path.join("exe"))
        && let Some(s) = exe.to_str()
        && s.contains("/claude/versions/")
    {
        return true;
    }
    matches!(fs::read_to_string(pid_path.join("comm")), Ok(c) if c.trim_end() == "claude")
}

/// True if the session at `path` belongs to a currently live Claude session:
/// there are `k` `claude` processes working in its project **and** this file is
/// among the `k` newest `.jsonl` files there (each running instance owns one of
/// the newest files; older files are finished sessions). Holding the file open
/// for writing is an additional fallback. `counts` should be a snapshot from
/// [`live_project_slug_counts`].
pub fn is_path_live(path: &Path, counts: &HashMap<String, usize>) -> bool {
    if crate::locate::is_worker_path(path) {
        return is_worker_live(path, counts) || is_held_open(path);
    }
    if let Some(slug) = crate::locate::project_slug_of(path)
        && let Some(&k) = counts.get(slug)
        && k > 0
        && is_among_newest_k(path, k)
    {
        return true;
    }
    is_held_open(path)
}

/// A finished worker's transcript never changes again, so freshness is the
/// only "still running" signal a subagent file has.
const WORKER_FRESH_SECS: u64 = 120;

/// A worker (subagent) transcript has no process of its own: it counts as
/// live while its parent session is live AND the file was written recently.
fn is_worker_live(path: &Path, counts: &HashMap<String, usize>) -> bool {
    let fresh = path
        .metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|mtime| mtime.elapsed().ok())
        .is_some_and(|age| age.as_secs() <= WORKER_FRESH_SECS);
    if !fresh {
        return false;
    }
    crate::locate::worker_parent_session(path)
        .is_some_and(|parent| is_path_live(&parent, counts))
}

/// True if `path` is among the `k` `.jsonl` files with the most recent mtime in
/// its parent directory — i.e. fewer than `k` siblings are newer than it. Ties
/// and errors resolve to true (better a false "live" than dropping the file that
/// is actually being written).
fn is_among_newest_k(path: &Path, k: usize) -> bool {
    if k == 0 {
        return false;
    }
    let Some(dir) = path.parent() else {
        return true;
    };
    let Ok(self_mtime) = path.metadata().and_then(|m| m.modified()) else {
        return true;
    };
    let Ok(entries) = fs::read_dir(dir) else {
        return true;
    };
    let mut newer = 0usize;
    for entry in entries.flatten() {
        let p = entry.path();
        if p == path {
            continue;
        }
        if p.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        if let Ok(mtime) = entry.metadata().and_then(|m| m.modified())
            && mtime > self_mtime
        {
            newer += 1;
            if newer >= k {
                return false;
            }
        }
    }
    true
}

/// Convenience wrapper that takes a fresh `/proc` snapshot for a single check.
pub fn is_path_live_now(path: &Path) -> bool {
    is_path_live(path, &live_project_slug_counts())
}

/// Returns true iff some process currently holds the session file open **for
/// writing**. Read-only handles (e.g. our own tailing process) are ignored.
pub fn is_held_open(session: &Path) -> bool {
    let target = match fs::canonicalize(session) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let proc_dir = match fs::read_dir("/proc") {
        Ok(d) => d,
        Err(_) => return false,
    };

    for entry in proc_dir.flatten() {
        let name = entry.file_name();
        let name_str = match name.to_str() {
            Some(s) => s,
            None => continue,
        };
        if !name_str.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }

        let pid_path = entry.path();
        let fd_dir = pid_path.join("fd");
        let fds = match fs::read_dir(&fd_dir) {
            Ok(d) => d,
            Err(_) => continue,
        };

        for fd in fds.flatten() {
            let fd_path = fd.path();
            if let Ok(link) = fs::read_link(&fd_path)
                && link == target
            {
                let fd_name = fd.file_name();
                let fdinfo_path = pid_path.join("fdinfo").join(&fd_name);
                if fd_opened_for_write(&fdinfo_path) {
                    return true;
                }
            }
        }
    }

    false
}

/// Parse `/proc/<pid>/fdinfo/<fd>` and check the `flags:` line for write access.
/// O_WRONLY = 0o1, O_RDWR = 0o2 — either makes the lower two bits non-zero.
fn fd_opened_for_write(fdinfo_path: &Path) -> bool {
    let content = match fs::read_to_string(fdinfo_path) {
        Ok(s) => s,
        Err(_) => return false,
    };
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("flags:") {
            let flags_str = rest.trim();
            // Linux writes flags in octal.
            if let Ok(flags) = u32::from_str_radix(flags_str, 8) {
                return flags & 0o3 != 0;
            }
            return false;
        }
    }
    false
}
