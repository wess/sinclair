//! `relay watch` — stream live status as newline-delimited JSON. Connects to the
//! server's `/control/events` SSE endpoint and prints each `{agents, workers}`
//! snapshot as one line. Reconnects on its own so callers (the app) can spawn it
//! once and let it ride across server restarts; a `null` line marks a dropped
//! connection. Exits when stdout closes (its parent went away).

use super::{http, paths};
use anyhow::Result;
use std::io::{BufRead, Write};
use std::time::Duration;

pub fn watch() -> Result<()> {
    let mut out = std::io::stdout();
    loop {
        let _ = stream_once(&mut out);
        // Connection failed or dropped: tell the consumer, then back off. If the
        // consumer is gone the pipe is broken and we exit.
        if writeln!(out, "null").is_err() || out.flush().is_err() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// One connection's lifetime: forward `data:` payloads until it closes. Returns
/// `Ok` on a clean close and `Err` if the server is unreachable (caller retries
/// either way).
fn stream_once(out: &mut impl Write) -> Result<()> {
    let info = paths::read_info()?;
    let mut reader = http::open_stream(&info.addr, "/control/events")?;
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }
        if let Some(rest) = line.strip_prefix("data:") {
            let payload = rest.trim();
            if !payload.is_empty() {
                writeln!(out, "{payload}")?;
                out.flush()?;
            }
        }
    }
}
