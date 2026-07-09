//! Read a session JSONL file: scan existing lines, then optionally poll for new
//! ones. No `notify` dependency — a small size-poll is plenty for a status
//! display.

use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;

use crate::protocol::ClaudeEvent;

const POLL_INTERVAL: Duration = Duration::from_millis(250);

pub struct Tail {
    reader: BufReader<File>,
    path: PathBuf,
    pub follow: bool,
    /// File size at open time. Used to mark the "initial drain" boundary.
    initial_len: u64,
}

impl Tail {
    pub fn open(path: &Path, follow: bool) -> Result<Self> {
        let file = File::open(path)?;
        let initial_len = file.metadata()?.len();
        Ok(Self {
            reader: BufReader::new(file),
            path: path.to_path_buf(),
            follow,
            initial_len,
        })
    }

    /// True once the reader has passed the byte length that the file had at
    /// `open`. Use this to print a snapshot exactly once after the initial drain.
    pub fn passed_initial(&mut self) -> Result<bool> {
        Ok(self.reader.stream_position()? >= self.initial_len)
    }

    /// Returns the next parsed event.
    ///
    /// In non-follow mode returns `Ok(None)` at EOF.
    /// In follow mode blocks (polling) until another line arrives.
    pub fn next_item(&mut self) -> Result<Option<ClaudeEvent>> {
        loop {
            let mut line = String::new();
            let n = self.reader.read_line(&mut line)?;
            if n == 0 {
                if !self.follow {
                    return Ok(None);
                }
                std::thread::sleep(POLL_INTERVAL);
                self.maybe_resync()?;
                continue;
            }
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                continue;
            }
            match serde_json::from_str::<ClaudeEvent>(trimmed) {
                Ok(item) => return Ok(Some(item)),
                Err(_) => continue, // ignore malformed / unknown shapes
            }
        }
    }

    fn maybe_resync(&mut self) -> Result<()> {
        // If the file vanished between polls, fail clearly so the caller can
        // exit the tail thread (multi-session mode then drops this session).
        let len = match std::fs::metadata(&self.path) {
            Ok(m) => m.len(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(anyhow::anyhow!(
                    "session file disappeared: {}",
                    self.path.display()
                ));
            }
            Err(e) => return Err(e.into()),
        };
        let pos = self.reader.stream_position()?;
        if len < pos {
            // file truncated/rotated — rewind to start to be safe
            self.reader.seek(SeekFrom::Start(0))?;
            self.initial_len = len;
        }
        Ok(())
    }
}
