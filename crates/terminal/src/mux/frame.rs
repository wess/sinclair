//! The session server's wire format: a one-byte kind, a big-endian `u32`
//! length, then that many bytes of payload. Terminal output and keystrokes
//! travel as raw bytes; the few control messages are small JSON objects.
//!
//! The format is meant to stay put across releases. A GUI updated in place
//! reattaches to a server started by the version before it, so the hello
//! carries a protocol number and anything new is added as a new kind or a new
//! JSON field the other side may ignore, never as a change to an old one.

use std::io::{self, Read, Write};

/// Bumped only for a change an older peer cannot ignore.
pub const PROTOCOL: u32 = 1;

/// Largest payload accepted. A snapshot of a full scrollback is the biggest
/// legitimate frame; anything past this is a broken or hostile peer.
pub const MAX_PAYLOAD: usize = 256 * 1024 * 1024;

// Client to server.
/// `{"protocol": n}`: always the first frame.
pub const HELLO: u8 = b'H';
/// Bytes replayed into a new session's terminal before its child starts
/// (a restored buffer). Optional; precedes `OPEN`.
pub const PRELOAD: u8 = b'P';
/// Start a new session and attach to it (JSON: argv, login, env, cwd, size).
pub const OPEN: u8 = b'O';
/// Attach to a running session: `{"id": "..."}`.
pub const ATTACH: u8 = b'A';
/// Keystrokes and pastes for the child.
pub const INPUT: u8 = b'I';
/// `{"cols","rows","cell_width","cell_height"}`.
pub const RESIZE: u8 = b'R';
/// The colors OSC 4/10/11/12 queries are answered with (see `colors`).
pub const COLORS: u8 = b'C';
/// End the attached session (or, as a first command, `{"id"}`'s session).
pub const KILL: u8 = b'K';
/// List running sessions.
pub const LIST: u8 = b'L';

// Server to client.
/// `{"id","cols","rows","token","protocol"}`: the attached session.
pub const INFO: u8 = b'S';
/// The bytes that rebuild the session's terminal (`vt::Terminal::snapshot`).
pub const SNAPSHOT: u8 = b'D';
/// Child output.
pub const OUTPUT: u8 = b'B';
/// One byte: whether a foreground job other than the shell is running.
pub const BUSY: u8 = b'Y';
/// `{"code": n|null}`: the child exited; the session is gone.
pub const EXIT: u8 = b'X';
/// UTF-8 error text; the connection closes after it.
pub const ERROR: u8 = b'E';

/// One frame off the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
  pub kind: u8,
  pub payload: Vec<u8>,
}

impl Frame {
  pub fn json(&self) -> io::Result<serde_json::Value> {
    serde_json::from_slice(&self.payload).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
  }
}

pub fn write(w: &mut impl Write, kind: u8, payload: &[u8]) -> io::Result<()> {
  if payload.len() > MAX_PAYLOAD {
    return Err(io::Error::new(io::ErrorKind::InvalidInput, "frame too large"));
  }
  let mut head = [0u8; 5];
  head[0] = kind;
  head[1..].copy_from_slice(&(payload.len() as u32).to_be_bytes());
  w.write_all(&head)?;
  w.write_all(payload)?;
  w.flush()
}

pub fn write_json(w: &mut impl Write, kind: u8, value: &serde_json::Value) -> io::Result<()> {
  write(w, kind, value.to_string().as_bytes())
}

/// Read one frame. `UnexpectedEof` at a frame boundary means the peer hung up.
pub fn read(r: &mut impl Read) -> io::Result<Frame> {
  let mut head = [0u8; 5];
  r.read_exact(&mut head)?;
  let len = u32::from_be_bytes([head[1], head[2], head[3], head[4]]) as usize;
  if len > MAX_PAYLOAD {
    return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too large"));
  }
  let mut payload = vec![0u8; len];
  r.read_exact(&mut payload)?;
  Ok(Frame {
    kind: head[0],
    payload,
  })
}

/// Encode the colors a terminal answers OSC color queries with: foreground,
/// background, cursor, then the 256-entry palette, three bytes each.
pub fn colors(c: &vt::ReportColors) -> Vec<u8> {
  let mut out = Vec::with_capacity(3 * 259);
  for (r, g, b) in [c.foreground, c.background, c.cursor]
    .into_iter()
    .chain(c.palette.iter().copied())
  {
    out.extend_from_slice(&[r, g, b]);
  }
  out
}

/// Decode [`colors`]. `None` for a payload of the wrong size.
pub fn parse_colors(bytes: &[u8]) -> Option<vt::ReportColors> {
  if bytes.len() != 3 * 259 {
    return None;
  }
  let rgb = |i: usize| (bytes[3 * i], bytes[3 * i + 1], bytes[3 * i + 2]);
  let mut palette = [(0, 0, 0); 256];
  for (i, slot) in palette.iter_mut().enumerate() {
    *slot = rgb(3 + i);
  }
  Some(vt::ReportColors {
    foreground: rgb(0),
    background: rgb(1),
    cursor: rgb(2),
    palette,
  })
}

#[cfg(test)]
#[path = "../../tests/mux/frame.rs"]
mod tests;
