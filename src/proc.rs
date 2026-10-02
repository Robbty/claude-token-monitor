//! Detect whether a session JSONL file belongs to a currently running Claude
//! Code session.
//!
//! Linux-only: walks `/proc`. Claude Code does **not** keep the session file
//! open with a persistent write handle — it opens, appends and closes per
//! record — so the open-handle check is only a last-resort fallback. The
//! primary liveness signal is process-based: a session counts as live when a
//! running `claude` process's current working directory sits **at or below
//! the session's launch directory** (the first cwd recorded in its
//! transcript, see [`launch_cwd`]). The process chdirs around inside the
//! project as the session `cd`s and uses worktrees — and the per-record cwd
//! is not in lockstep with the process cwd — but both stay under the launch
//! dir, so the subtree match is stable. Matching the process cwd against the
//! transcript's *location* (the encoded launch-cwd slug) is not: after `cd`
//! into a subdirectory the process cwd no longer encodes to the slug the
//! file lives under, and the session flickers between live (poll caught
//! mid-append) and closed.
//!
//! That subtree match is only a heuristic, though — it cannot tell *which*
//! transcript a process writes. Newer Claude Code versions keep an exact
//! registry, `~/.claude/sessions/<pid>.json` (pid, sessionId, procStart; the
//! sessionId is rewritten on `/resume` and `/clear`). Where a running process
//! has such an entry it is bound to exactly that session: the session is live,
//! and the process lends no liveness to any other transcript — e.g. a fresh,
//! still transcript-less session in a subdirectory no longer revives a
//! finished session of the parent project. The heuristic only covers the
//! processes the registry does not account for (older versions).
//!
//! Returns `false` on non-Linux or if `/proc` cannot be read. Reads only the
//! entries the current user owns — no elevated privileges needed.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::SystemTime;

use serde::Deserialize;

/// Snapshot of the running Claude Code processes, split by whether the
/// session registry binds them to a session.
#[derive(Default)]
pub struct ProcSnapshot {
    /// Session ids the registry binds to a running process — exactly live.
    registered: HashSet<String>,
    /// Per current working directory, **how many** processes run there that
    /// the registry does not account for. The count matters: each running
    /// instance writes its own session file, so N such processes in one
    /// directory mean the N newest unregistered transcripts claiming that cwd
    /// are live (older claimants are finished sessions).
    unclaimed: HashMap<PathBuf, usize>,
}

#[derive(Deserialize)]
struct RegistryEntry {
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    #[serde(rename = "procStart")]
    proc_start: Option<String>,
}

/// Scan `/proc` for running Claude Code processes and match them against the
/// session registry in `claude_home/sessions/`.
pub fn proc_snapshot(claude_home: Option<&Path>) -> ProcSnapshot {
    let mut snap = ProcSnapshot::default();
    let proc_dir = match fs::read_dir("/proc") {
        Ok(d) => d,
        Err(_) => return snap,
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
        if let Some(sid) = claude_home.and_then(|home| registered_session(home, name_str, &pid_path))
        {
            snap.registered.insert(sid);
        } else if let Ok(cwd) = fs::read_link(pid_path.join("cwd")) {
            *snap.unclaimed.entry(cwd).or_insert(0) += 1;
        }
    }
    snap
}

/// The session id `sessions/<pid>.json` binds this running process to. A
/// leftover file of a dead process whose pid got reused is told apart by
/// `procStart` (= the kernel start time, field 22 of `/proc/<pid>/stat`).
fn registered_session(claude_home: &Path, pid: &str, pid_path: &Path) -> Option<String> {
    let raw = fs::read_to_string(claude_home.join("sessions").join(format!("{pid}.json"))).ok()?;
    let entry: RegistryEntry = serde_json::from_str(&raw).ok()?;
    if let Some(expected) = entry.proc_start
        && proc_start_time(pid_path).is_some_and(|actual| actual != expected)
    {
        return None;
    }
    entry.session_id
}

/// Field 22 (starttime) of `/proc/<pid>/stat`. `comm` (field 2) may contain
/// spaces and parentheses, so count from the last `)`.
fn proc_start_time(pid_path: &Path) -> Option<String> {
    let stat = fs::read_to_string(pid_path.join("stat")).ok()?;
    let rest = &stat[stat.rfind(')')? + 1..];
    rest.split_whitespace().nth(19).map(str::to_owned)
}

/// A process is a Claude Code instance if its executable lives under a
/// `claude/versions/` directory (native installer) or the npm package dir
/// `@anthropic-ai/claude-code/` (layout since the 2026-07 update). The `comm`
/// name is unreliable: depending on how the session was launched (e.g.
/// `claude --resume`, which re-execs the versioned binary directly), `comm`
/// is either "claude" or the bare version string like "2.1.186" — so we key
/// off the exe path and fall back to comm.
/// Our own binary is `claude-tokens`, which matches neither.
fn is_claude_process(pid_path: &Path) -> bool {
    if let Ok(exe) = fs::read_link(pid_path.join("exe"))
        && let Some(s) = exe.to_str()
        && (s.contains("/claude/versions/") || s.contains("@anthropic-ai/claude-code/"))
    {
        return true;
    }
    matches!(fs::read_to_string(pid_path.join("comm")), Ok(c) if c.trim_end() == "claude")
}

/// True if the session at `path` belongs to a currently live Claude session.
/// Exact when the session registry binds a running process to this session
/// id. Otherwise heuristic over the processes the registry does not account
/// for: such a `claude` process runs whose cwd sits at or below this session's
/// launch directory, and among all unregistered transcripts that could claim
/// such a process this file is one of the `k` newest (`k` = number of such
/// processes; older claimants are finished sessions). Transcripts without any
/// recorded cwd yet (just created) fall back to the encoded-slug match against
/// the directory the file lives under; holding the file open for writing is
/// the final fallback. `procs` should be a snapshot from [`proc_snapshot`].
pub fn is_path_live(path: &Path, procs: &ProcSnapshot) -> bool {
    if crate::locate::is_worker_path(path) {
        return is_worker_live(path, procs) || is_held_open(path);
    }
    if session_id_of(path).is_some_and(|sid| procs.registered.contains(sid)) {
        return true;
    }
    let counts = &procs.unclaimed;
    match launch_cwd(path) {
        Some(root) => {
            let k: usize = counts
                .iter()
                .filter(|(cwd, _)| cwd.starts_with(&root))
                .map(|(_, n)| *n)
                .sum();
            if k > 0 && is_among_newest_k_claimants(path, &root, k, &procs.registered) {
                return true;
            }
        }
        // No cwd record yet: the launch cwd encodes exactly to the slug
        // directory the file lives under, so the slug match is equivalent —
        // rank against siblings in the same project dir.
        None => {
            if let Some(slug) = crate::locate::project_slug_of(path) {
                let k: usize = counts
                    .iter()
                    .filter(|(cwd, _)| crate::locate::encode_cwd(cwd) == slug)
                    .map(|(_, n)| *n)
                    .sum();
                if k > 0 && is_among_newest_k(path, k, &procs.registered) {
                    return true;
                }
            }
        }
    }
    is_held_open(path)
}

/// The session id of a main-session transcript = its file stem.
fn session_id_of(path: &Path) -> Option<&str> {
    path.file_stem().and_then(|s| s.to_str())
}

/// Transcripts the registry binds to a process of their own never compete for
/// the unclaimed processes.
fn is_registered(path: &Path, registered: &HashSet<String>) -> bool {
    session_id_of(path).is_some_and(|sid| registered.contains(sid))
}

/// Leading chunk to scan for the first `cwd` record. The launch cwd sits in
/// the first few lines of every transcript; the bound only guards against
/// degenerate files.
const CWD_SCAN_BYTES: u64 = 1024 * 1024;

/// `launch_cwd` results keyed by file. A found value is final (the file is
/// append-only, its first cwd record never changes); a miss is cached by
/// mtime so a still-empty file is re-read once it grows.
static CWD_CACHE: LazyLock<Mutex<HashMap<PathBuf, (SystemTime, Option<PathBuf>)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Deserialize)]
struct CwdRecord {
    cwd: Option<String>,
}

/// The working directory this session was launched in — the first cwd its
/// transcript recorded. Stable for the whole session: in-session `cd`s and
/// worktrees move the process around *below* it, but the transcript stays in
/// the launch dir's slug. None if the file has no parseable cwd record yet.
fn launch_cwd(path: &Path) -> Option<PathBuf> {
    let mtime = fs::metadata(path).and_then(|m| m.modified()).ok()?;
    if let Some((cached_mtime, cached)) = CWD_CACHE.lock().unwrap().get(path)
        && (cached.is_some() || *cached_mtime == mtime)
    {
        return cached.clone();
    }
    let value = read_first_cwd(path);
    CWD_CACHE
        .lock()
        .unwrap()
        .insert(path.to_path_buf(), (mtime, value.clone()));
    value
}

fn read_first_cwd(path: &Path) -> Option<PathBuf> {
    let mut file = fs::File::open(path).ok()?;
    let mut buf = vec![0u8; CWD_SCAN_BYTES as usize];
    let mut filled = 0usize;
    while filled < buf.len() {
        match file.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(_) => return None,
        }
    }
    let text = String::from_utf8_lossy(&buf[..filled]);
    // Oldest record wins; a partial last line (chunk end mid-record) simply
    // fails to parse and is skipped.
    for line in text.lines() {
        if !line.contains("\"cwd\"") {
            continue;
        }
        if let Ok(rec) = serde_json::from_str::<CwdRecord>(line)
            && let Some(cwd) = rec.cwd
        {
            return Some(PathBuf::from(cwd));
        }
    }
    None
}

/// A finished worker's transcript never changes again, so freshness is the
/// only "still running" signal a subagent file has.
const WORKER_FRESH_SECS: u64 = 120;

/// A worker (subagent) transcript has no process of its own: it counts as
/// live while its parent session is live AND the file was written recently.
fn is_worker_live(path: &Path, procs: &ProcSnapshot) -> bool {
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
        .is_some_and(|parent| is_path_live(&parent, procs))
}

/// True if fewer than `k` other main-session transcripts are newer than
/// `path` AND could claim the same processes — i.e. their launch cwd is an
/// ancestor or descendant of `root` (a process below the deeper of the two
/// roots matches both sessions). Claimants live in *different* project dirs,
/// so this scans all of `projects/`. Cheap in practice: content is only read
/// (cached, launch cwds are final) for files newer than `path` — a handful of
/// concurrently active sessions — everything else is a stat. Ties and errors
/// resolve to true (better a false "live" than dropping the file that is
/// actually being written). Transcripts in `registered` own a process of
/// their own and do not compete.
fn is_among_newest_k_claimants(
    path: &Path,
    root: &Path,
    k: usize,
    registered: &HashSet<String>,
) -> bool {
    if k == 0 {
        return false;
    }
    let Ok(self_mtime) = path.metadata().and_then(|m| m.modified()) else {
        return true;
    };
    // `projects/<slug>/<uuid>.jsonl` → two levels up is the projects root.
    let Some(projects) = path.parent().and_then(|p| p.parent()) else {
        return true;
    };
    let Ok(project_dirs) = fs::read_dir(projects) else {
        return true;
    };
    let mut newer = 0usize;
    for project in project_dirs.flatten() {
        let dir = project.path();
        if !dir.is_dir() {
            continue;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.as_path() == path
                || !entry.file_type().is_ok_and(|t| t.is_file())
                || p.extension().and_then(|e| e.to_str()) != Some("jsonl")
                || is_registered(&p, registered)
            {
                continue;
            }
            let Ok(mtime) = entry.metadata().and_then(|m| m.modified()) else {
                continue;
            };
            if mtime <= self_mtime {
                continue;
            }
            if let Some(other_root) = launch_cwd(&p)
                && (other_root.starts_with(root) || root.starts_with(&other_root))
            {
                newer += 1;
                if newer >= k {
                    return false;
                }
            }
        }
    }
    true
}

/// True if `path` is among the `k` `.jsonl` files with the most recent mtime in
/// its parent directory — i.e. fewer than `k` siblings are newer than it. Only
/// used for transcripts without a recorded cwd; ties and errors resolve to
/// true. Siblings in `registered` do not compete.
fn is_among_newest_k(path: &Path, k: usize, registered: &HashSet<String>) -> bool {
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
        if p.extension().and_then(|e| e.to_str()) != Some("jsonl") || is_registered(&p, registered)
        {
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
    is_path_live(path, &proc_snapshot(claude_home_of(path).as_deref()))
}

/// The Claude home a transcript lives under — derived from the path rather
/// than the environment so a `--claude-home` override carries over:
/// `<home>/projects/<slug>/<uuid>.jsonl`, workers sit three levels deeper
/// (`<slug>/<uuid>/subagents/agent-<hex>.jsonl`).
fn claude_home_of(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|p| p.file_name().is_some_and(|n| n == "projects"))
        .and_then(Path::parent)
        .map(Path::to_path_buf)
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
