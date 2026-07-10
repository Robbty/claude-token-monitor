mod locate;
mod models;
mod plan;
mod proc;
mod protocol;
mod render;
mod state;
mod tail;

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Sender, channel};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use clap::{Parser, Subcommand};

use crate::locate::{Selector, session_id_from_path};
use crate::protocol::ClaudeEvent;
use crate::render::Format;
use crate::state::TokenState;

/// Stdout token/context monitor for the currently active Claude Code session.
#[derive(Parser, Debug)]
#[command(name = "claude-tokens", version)]
struct Cli {
    /// Subcommand selector. When absent, the binary runs in session-tail mode
    /// and the flags below apply.
    #[command(subcommand)]
    command: Option<Command>,

    /// Bind to a specific session by its UUID.
    #[arg(long)]
    thread: Option<String>,

    /// Bind to the session whose recorded cwd matches PATH (default: $PWD).
    #[arg(long, value_name = "PATH", num_args = 0..=1, default_missing_value = ".")]
    cwd: Option<PathBuf>,

    /// Override CLAUDE_HOME (default: $CLAUDE_HOME or ~/.claude).
    #[arg(long, value_name = "DIR")]
    claude_home: Option<PathBuf>,

    /// Follow the session file and emit a fresh snapshot on every token update.
    #[arg(long, short = 'f')]
    follow: bool,

    /// Output as a single-line JSON object instead of KEY=VALUE.
    #[arg(long)]
    json: bool,

    /// Print the resolved session file path and exit.
    #[arg(long)]
    locate: bool,

    /// Wait until a matching session file appears instead of erroring out
    /// immediately. Useful when starting the monitor before Claude Code.
    #[arg(long)]
    wait: bool,

    /// Maximum seconds to wait when --wait is set (default: no limit).
    #[arg(long, value_name = "SECS", requires = "wait")]
    wait_timeout: Option<u64>,

    /// Multi-session mode: follow ALL active matching sessions instead of just
    /// the newest one. With --cwd, scoped to that directory; without --cwd,
    /// scans system-wide. Each session's output gets a "=== session <uuid> ==="
    /// header.
    #[arg(long)]
    all: bool,

    /// Maximum age in minutes a session's mtime can have to count as "active".
    /// Only applied in --all mode (default: 5 minutes).
    #[arg(long, value_name = "MINUTES", default_value_t = 5, requires = "all")]
    max_age: u64,

    /// With --all --follow: periodically re-scan for new sessions and start
    /// watching them too (poll every 5 seconds).
    #[arg(long, requires_all = ["all", "follow"])]
    watch_new: bool,

    /// Only include sessions that are currently live, i.e. a running `claude`
    /// process is working in the session's project directory (or the file is
    /// held open for writing). Linux/WSL2 only.
    #[arg(long)]
    require_open: bool,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Query the account's plan-level rate-limit state via the Anthropic OAuth
    /// usage endpoint (uses the token under ~/.claude/.credentials.json). Emits
    /// the upstream JSON on stdout. Non-zero exit codes signal distinct failure
    /// modes: 2 = not logged in, 3 = token expired, 4 = network/backend error.
    Plan,
}

fn main() {
    let cli = Cli::parse();

    if matches!(cli.command, Some(Command::Plan)) {
        std::process::exit(plan::run(cli.claude_home));
    }

    match run_session_mode(cli) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("claude-tokens: {e}");
            std::process::exit(1);
        }
    }
}

fn run_session_mode(cli: Cli) -> Result<()> {
    let claude_home = match cli.claude_home.clone() {
        Some(p) => p,
        None => locate::claude_home()?,
    };

    let selector = build_selector(&cli)?;

    if cli.all {
        return run_multi_session(&cli, &selector, &claude_home);
    }

    run_single_session(&cli, &selector, &claude_home)
}

fn build_selector(cli: &Cli) -> Result<Selector> {
    if let Some(id) = cli.thread.clone() {
        Ok(Selector::ThreadId(id))
    } else if let Some(cwd) = cli.cwd.clone() {
        let abs = if cwd.as_os_str() == "." {
            std::env::current_dir()?
        } else {
            cwd
        };
        Ok(Selector::Cwd(abs))
    } else if cli.all {
        Ok(Selector::All)
    } else {
        Ok(Selector::MostRecent)
    }
}

// ─── Single-session path ──────────────────────────────────────────────────────

fn run_single_session(cli: &Cli, selector: &Selector, claude_home: &Path) -> Result<()> {
    let session_path = resolve_with_optional_wait(
        selector,
        claude_home,
        cli.wait,
        cli.wait_timeout,
        cli.require_open,
    )?;

    if cli.locate {
        println!("{}", session_path.display());
        return Ok(());
    }

    let format = if cli.json { Format::Json } else { Format::Kv };
    let mut state = new_state(&session_path);
    let mut tail = tail::Tail::open(&session_path, cli.follow)?;
    let mut printed_initial = false;

    loop {
        let item = match tail.next_item()? {
            Some(item) => item,
            None => break,
        };

        let is_token_event = matches!(&item, ClaudeEvent::Assistant(_));
        apply(&mut state, item);

        if tail.follow {
            if !printed_initial && tail.passed_initial()? {
                state.session_active = Some(proc::is_path_live_now(&session_path));
                print_snapshot(&state, &format, /*multi*/ false)?;
                printed_initial = true;
            } else if printed_initial && is_token_event {
                state.session_active = Some(proc::is_path_live_now(&session_path));
                print_snapshot(&state, &format, /*multi*/ false)?;
            }
        }
    }

    if !tail.follow {
        state.session_active = Some(proc::is_path_live_now(&session_path));
        print_snapshot(&state, &format, /*multi*/ false)?;
    }
    Ok(())
}

fn resolve_with_optional_wait(
    selector: &Selector,
    claude_home: &Path,
    wait: bool,
    timeout_secs: Option<u64>,
    require_open: bool,
) -> Result<PathBuf> {
    if !wait {
        return locate::resolve_filtered(selector, claude_home, require_open);
    }

    let deadline = timeout_secs.map(|s| Instant::now() + Duration::from_secs(s));
    let mut announced = false;

    loop {
        match locate::resolve_filtered(selector, claude_home, require_open) {
            Ok(path) => return Ok(path),
            Err(err) => {
                if let Some(d) = deadline
                    && Instant::now() >= d
                {
                    return Err(err.context("--wait timeout reached"));
                }
                if !announced {
                    eprintln!("claude-tokens: waiting for matching session file...");
                    announced = true;
                }
                thread::sleep(Duration::from_millis(500));
            }
        }
    }
}

// ─── Multi-session path ──────────────────────────────────────────────────────

/// Discovery / rescan interval for --watch-new.
const DISCOVERY_INTERVAL: Duration = Duration::from_secs(5);

enum TailMsg {
    Item(ClaudeEvent),
    InitialDrained,
    Closed(Option<String>),
}

type TailKey = PathBuf;

fn run_multi_session(cli: &Cli, selector: &Selector, claude_home: &Path) -> Result<()> {
    let max_age = Duration::from_secs(cli.max_age.saturating_mul(60));

    let paths = resolve_all_with_optional_wait(
        selector,
        claude_home,
        max_age,
        cli.wait,
        cli.wait_timeout,
        cli.require_open,
    )?;

    if cli.locate {
        for p in &paths {
            println!("{}", p.display());
        }
        return Ok(());
    }

    let format = if cli.json { Format::Json } else { Format::Kv };
    let (tx, rx) = channel::<(TailKey, TailMsg)>();

    let mut tracked: HashSet<TailKey> = HashSet::new();
    for path in paths {
        spawn_tail(path.clone(), cli.follow, tx.clone());
        tracked.insert(path);
    }

    if cli.watch_new {
        spawn_discovery(
            selector.clone(),
            claude_home.to_path_buf(),
            max_age,
            cli.require_open,
            tracked.clone(),
            tx.clone(),
        );
    }

    // Drop the original sender so the channel closes when all tails finish in
    // non-follow mode.
    drop(tx);

    let mut states: HashMap<TailKey, TokenState> = HashMap::new();
    let mut printed_initial: HashSet<TailKey> = HashSet::new();

    while let Ok((key, msg)) = rx.recv() {
        match msg {
            TailMsg::Item(item) => {
                let entry = states
                    .entry(key.clone())
                    .or_insert_with(|| new_state(&key));
                let is_token_event = matches!(&item, ClaudeEvent::Assistant(_));
                apply(entry, item);

                if cli.follow && printed_initial.contains(&key) && is_token_event {
                    entry.session_active = Some(proc::is_path_live_now(&key));
                    print_snapshot(entry, &format, /*multi*/ true)?;
                }
            }
            TailMsg::InitialDrained => {
                if cli.follow && !printed_initial.contains(&key) {
                    let entry = states.entry(key.clone()).or_insert_with(|| new_state(&key));
                    entry.session_active = Some(proc::is_path_live_now(&key));
                    print_snapshot(entry, &format, /*multi*/ true)?;
                    printed_initial.insert(key.clone());
                }
            }
            TailMsg::Closed(err) => {
                if let Some(msg) = err {
                    eprintln!("claude-tokens: tail for {} stopped: {}", key.display(), msg);
                }
                if !cli.follow {
                    let entry = states.entry(key.clone()).or_insert_with(|| new_state(&key));
                    entry.session_active = Some(proc::is_path_live_now(&key));
                    print_snapshot(entry, &format, /*multi*/ true)?;
                }
            }
        }
    }

    Ok(())
}

fn new_state(key: &Path) -> TokenState {
    TokenState {
        session_id: session_id_from_path(key),
        is_worker: locate::is_worker_path(key),
        parent_session_id: locate::worker_parent_session_id(key),
        ..Default::default()
    }
}

fn resolve_all_with_optional_wait(
    selector: &Selector,
    claude_home: &Path,
    max_age: Duration,
    wait: bool,
    timeout_secs: Option<u64>,
    require_open: bool,
) -> Result<Vec<PathBuf>> {
    if !wait {
        return locate::resolve_all(selector, claude_home, max_age, require_open);
    }

    let deadline = timeout_secs.map(|s| Instant::now() + Duration::from_secs(s));
    let mut announced = false;

    loop {
        match locate::resolve_all(selector, claude_home, max_age, require_open) {
            Ok(paths) if !paths.is_empty() => return Ok(paths),
            Ok(_) | Err(_) => {
                if let Some(d) = deadline
                    && Instant::now() >= d
                {
                    return Err(anyhow!("--wait timeout reached without matching session"));
                }
                if !announced {
                    eprintln!("claude-tokens: waiting for matching session file...");
                    announced = true;
                }
                thread::sleep(Duration::from_millis(500));
            }
        }
    }
}

fn spawn_tail(path: PathBuf, follow: bool, tx: Sender<(TailKey, TailMsg)>) {
    thread::spawn(move || {
        let key = path.clone();
        let mut tail = match tail::Tail::open(&path, follow) {
            Ok(t) => t,
            Err(e) => {
                let _ = tx.send((key, TailMsg::Closed(Some(e.to_string()))));
                return;
            }
        };

        let mut signaled_drain = false;
        loop {
            if follow
                && !signaled_drain
                && let Ok(true) = tail.passed_initial()
            {
                if tx.send((key.clone(), TailMsg::InitialDrained)).is_err() {
                    return;
                }
                signaled_drain = true;
            }

            match tail.next_item() {
                Ok(Some(item)) => {
                    if tx.send((key.clone(), TailMsg::Item(item))).is_err() {
                        return;
                    }
                }
                Ok(None) => {
                    if !follow && !signaled_drain {
                        let _ = tx.send((key.clone(), TailMsg::InitialDrained));
                    }
                    let _ = tx.send((key, TailMsg::Closed(None)));
                    return;
                }
                Err(e) => {
                    let _ = tx.send((key, TailMsg::Closed(Some(e.to_string()))));
                    return;
                }
            }
        }
    });
}

fn spawn_discovery(
    selector: Selector,
    claude_home: PathBuf,
    max_age: Duration,
    require_open: bool,
    initial: HashSet<TailKey>,
    tx: Sender<(TailKey, TailMsg)>,
) {
    thread::spawn(move || {
        let mut tracked = initial;
        loop {
            thread::sleep(DISCOVERY_INTERVAL);
            let paths = match locate::resolve_all(&selector, &claude_home, max_age, require_open) {
                Ok(p) => p,
                Err(_) => continue,
            };
            for path in paths {
                if !tracked.contains(&path) {
                    tracked.insert(path.clone());
                    spawn_tail(path, /*follow*/ true, tx.clone());
                }
            }
        }
    });
}

// ─── Shared helpers ──────────────────────────────────────────────────────────

fn apply(state: &mut TokenState, item: ClaudeEvent) {
    match item {
        ClaudeEvent::Assistant(ev) => state.apply_assistant(&ev),
        ClaudeEvent::System(ev) => {
            if ev.subtype.as_deref() == Some("compact_boundary") {
                state.compact_count += 1;
            }
            if let Some(cwd) = ev.cwd.as_deref() {
                state.note_cwd(cwd);
            }
        }
        ClaudeEvent::User(ev) => {
            if let Some(cwd) = ev.cwd.as_deref() {
                state.note_cwd(cwd);
            }
        }
        ClaudeEvent::Other => {}
    }
}

fn print_snapshot(state: &TokenState, format: &Format, multi: bool) -> Result<()> {
    // Build the full block first, then write it with a single write_all so
    // concurrent threads' output cannot interleave at the line level.
    let mut buf = render::render(state, format, multi).into_bytes();
    if matches!(format, Format::Json) {
        buf.push(b'\n');
    } else {
        buf.extend_from_slice(b"---\n");
    }
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(&buf)?;
    stdout.flush()?;
    Ok(())
}
