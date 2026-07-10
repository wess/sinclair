//! asciinema v2 `.cast` recording - a terminal session captured as a stream of
//! timestamped output events, replayable with `asciinema play` and embeddable.
//!
//! The format is one JSON object header line followed by one JSON array per
//! event: `[time_seconds, "o", "utf8 output"]`. We record output only (what you
//! see); the user's keystrokes show up as the shell echoes them.

pub mod reader;

pub use reader::{parse, Cast, Event, EventKind, Header};

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::time::Instant;

/// Writes an asciinema v2 cast file as output arrives.
pub struct Recorder {
    writer: BufWriter<File>,
    start: Instant,
    path: PathBuf,
    /// Trailing bytes of an incomplete UTF-8 sequence, carried to the next
    /// chunk so a multi-byte char split across reads is never corrupted.
    pending: Vec<u8>,
}

impl Recorder {
    /// Create `path` and write the header for a `cols`x`rows` recording.
    pub fn create(
        path: PathBuf,
        cols: usize,
        rows: usize,
        title: Option<&str>,
        timestamp: Option<u64>,
    ) -> io::Result<Self> {
        let file = File::create(&path)?;
        let mut writer = BufWriter::new(file);
        let mut header = serde_json::json!({
            "version": 2,
            "width": cols,
            "height": rows,
            "env": { "TERM": "xterm-256color" },
        });
        if let Some(t) = title {
            header["title"] = t.into();
        }
        if let Some(ts) = timestamp {
            header["timestamp"] = ts.into();
        }
        writeln!(writer, "{header}")?;
        Ok(Self {
            writer,
            start: Instant::now(),
            path,
            pending: Vec::new(),
        })
    }

    /// Record an output chunk at the current elapsed time. Bytes that form an
    /// incomplete UTF-8 tail are held back until the next call completes them;
    /// genuinely invalid bytes are written through as replacement characters.
    pub fn output(&mut self, bytes: &[u8]) -> io::Result<()> {
        let t = self.start.elapsed().as_secs_f64();
        self.pending.extend_from_slice(bytes);
        let emit = emit_boundary(&self.pending);
        if emit == 0 {
            return Ok(());
        }
        let text = String::from_utf8_lossy(&self.pending[..emit]).into_owned();
        self.pending.drain(..emit);
        let line = serde_json::to_string(&(t, "o", text))?;
        writeln!(self.writer, "{line}")
    }

    /// Flush any held tail and the buffer, returning the file path.
    pub fn finish(mut self) -> io::Result<PathBuf> {
        if !self.pending.is_empty() {
            let t = self.start.elapsed().as_secs_f64();
            let text = String::from_utf8_lossy(&self.pending).into_owned();
            let line = serde_json::to_string(&(t, "o", text))?;
            writeln!(self.writer, "{line}")?;
        }
        self.writer.flush()?;
        Ok(self.path)
    }
}

/// How much of `bytes` is safe to emit now: everything except a trailing
/// UTF-8 sequence that is incomplete but still valid — the next chunk may
/// finish it. Invalid regions anywhere earlier are stepped over so an
/// invalid byte never flushes a completable tail with it.
fn emit_boundary(mut bytes: &[u8]) -> usize {
    let mut emitted = 0;
    loop {
        match std::str::from_utf8(bytes) {
            Ok(_) => return emitted + bytes.len(),
            Err(e) if e.error_len().is_none() => return emitted + e.valid_up_to(),
            Err(e) => {
                let skip = e.valid_up_to() + e.error_len().unwrap_or(1);
                emitted += skip;
                bytes = &bytes[skip..];
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/cast.rs"]
mod tests;
