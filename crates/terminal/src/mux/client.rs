//! The client half of the session server protocol: the handshake that ends
//! in an attached session (the streaming itself lives with
//! [`crate::Session::attach`]), plus the one-shot `list` and `kill` calls.

use std::io;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use serde_json::{json, Value};

use super::frame::{self, Frame};
use super::{Attach, Attached, Listed};

/// How long a handshake may take before the server counts as unreachable.
const HANDSHAKE: Duration = Duration::from_secs(5);

fn connect(path: &Path) -> io::Result<UnixStream> {
  let mut s = UnixStream::connect(path)?;
  s.set_read_timeout(Some(HANDSHAKE))?;
  s.set_write_timeout(Some(HANDSHAKE))?;
  frame::write_json(&mut s, frame::HELLO, &json!({ "protocol": frame::PROTOCOL }))?;
  Ok(s)
}

fn server_error(f: &Frame) -> io::Error {
  io::Error::other(String::from_utf8_lossy(&f.payload).into_owned())
}

/// Connect, start or pick a session, and read back its info and snapshot.
/// The returned socket has no timeouts left on it: from here on it streams.
pub fn handshake(
  path: &Path,
  how: Attach,
  cols: usize,
  rows: usize,
) -> io::Result<(UnixStream, Attached, Vec<u8>)> {
  let mut s = connect(path)?;
  match how {
    Attach::Open { options, token } => {
      if !options.preload.is_empty() {
        frame::write(&mut s, frame::PRELOAD, &options.preload)?;
      }
      let spawn = &options.spawn;
      frame::write_json(
        &mut s,
        frame::OPEN,
        &json!({
          "argv": spawn.argv,
          "login": spawn.login,
          "env": spawn.env,
          "cwd": spawn.cwd,
          "cols": cols,
          "rows": rows,
          "cell_width": spawn.winsize.cell_width,
          "cell_height": spawn.winsize.cell_height,
          "scrollback": options.scrollback_limit,
          "token": token,
        }),
      )?;
    }
    Attach::Existing(id) => frame::write_json(&mut s, frame::ATTACH, &json!({ "id": id }))?,
  }
  let info = frame::read(&mut s)?;
  if info.kind != frame::INFO {
    return Err(server_error(&info));
  }
  let v = info.json()?;
  let snapshot = frame::read(&mut s)?;
  if snapshot.kind != frame::SNAPSHOT {
    return Err(server_error(&snapshot));
  }
  let num = |k: &str| v.get(k).and_then(Value::as_u64).unwrap_or(0);
  let attached = Attached {
    id: v.get("id").and_then(Value::as_str).unwrap_or_default().to_string(),
    token: num("token"),
    cols: num("cols").max(1) as usize,
    rows: num("rows").max(1) as usize,
    busy: v.get("busy").and_then(Value::as_bool).unwrap_or(false),
  };
  s.set_read_timeout(None)?;
  s.set_write_timeout(None)?;
  Ok((s, attached, snapshot.payload))
}

/// The server's running sessions.
pub fn list(path: &Path) -> io::Result<Vec<Listed>> {
  let mut s = connect(path)?;
  frame::write(&mut s, frame::LIST, &[])?;
  let f = frame::read(&mut s)?;
  if f.kind != frame::LIST {
    return Err(server_error(&f));
  }
  let v = f.json()?;
  Ok(
    v.get("sessions")
      .and_then(Value::as_array)
      .map(|a| a.as_slice())
      .unwrap_or_default()
      .iter()
      .map(|e| Listed {
        id: e["id"].as_str().unwrap_or_default().to_string(),
        token: e["token"].as_u64().unwrap_or(0),
        title: e["title"].as_str().unwrap_or_default().to_string(),
        clients: e["clients"].as_u64().unwrap_or(0) as usize,
      })
      .collect(),
  )
}

/// End a session by id.
pub fn kill(path: &Path, id: &str) -> io::Result<()> {
  let mut s = connect(path)?;
  frame::write_json(&mut s, frame::KILL, &json!({ "id": id }))?;
  match frame::read(&mut s) {
    Ok(f) if f.kind == frame::ERROR => Err(server_error(&f)),
    _ => Ok(()),
  }
}

/// Whether a server is answering at `path`.
pub fn alive(path: &Path) -> bool {
  list(path).is_ok()
}
