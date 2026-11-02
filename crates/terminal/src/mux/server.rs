//! The session server: owns ptys on behalf of GUIs that come and go.
//!
//! Each connection starts with a hello, then one command. `OPEN` starts a
//! session and attaches; `ATTACH` attaches to a running one; `LIST` and
//! `KILL` answer and hang up. An attached connection gets the session's info
//! and snapshot, then every output chunk, its busy flag as it changes, and
//! finally its exit; it sends keystrokes, resizes, colors, or a kill.
//!
//! Registering a client and taking its snapshot happen under the session's
//! terminal lock, and output reaches clients from a tap that runs under the
//! same lock, so a client sees each byte exactly once, after its snapshot.
//!
//! A dropped connection is a detach: the session keeps running. The server
//! exits on its own once it has had no sessions and no connections for a
//! while, so a server is never left behind with nothing to do.

use std::collections::HashMap;
use std::io;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::frame::{self, Frame};
use crate::{Event, Session, SessionOptions};

/// How often busy flags are checked and idleness is judged.
const TICK: Duration = Duration::from_millis(500);
/// A client this far behind on output is cut off rather than buffered for.
const MAX_BACKLOG: usize = 64 * 1024 * 1024;

type Out = (u8, Vec<u8>);

struct Client {
  id: u64,
  tx: flume::Sender<Out>,
  backlog: Arc<AtomicUsize>,
}

struct Hosted {
  id: String,
  token: u64,
  session: Mutex<Option<Session>>,
  clients: Mutex<Vec<Client>>,
  busy: AtomicBool,
}

impl Hosted {
  /// Queue a frame for every attached client, cutting off any that has
  /// fallen too far behind.
  fn broadcast(&self, kind: u8, payload: &[u8]) {
    let mut clients = self.clients.lock().unwrap_or_else(|e| e.into_inner());
    clients.retain(|c| {
      if c.backlog.load(Ordering::Relaxed) > MAX_BACKLOG {
        return false;
      }
      c.backlog.fetch_add(payload.len(), Ordering::Relaxed);
      c.tx.send((kind, payload.to_vec())).is_ok()
    });
  }

  fn with_session<R>(&self, f: impl FnOnce(&Session) -> R) -> Option<R> {
    self
      .session
      .lock()
      .unwrap_or_else(|e| e.into_inner())
      .as_ref()
      .map(f)
  }

  /// End the session: dropping it hangs up the child, whose exit then
  /// reaches the clients through the event pump.
  fn kill(&self) {
    let session = self.session.lock().unwrap_or_else(|e| e.into_inner()).take();
    drop(session);
  }
}

#[derive(Default)]
struct Server {
  sessions: Mutex<HashMap<String, Arc<Hosted>>>,
  connections: AtomicUsize,
  next_client: AtomicU64,
}

/// Serve on `path` until idle for `idle` with no sessions and no
/// connections; then return.
pub fn serve(path: &Path, idle: Duration) -> io::Result<()> {
  let listener = bind(path)?;
  let server = Arc::new(Server::default());
  let done = Arc::new(AtomicBool::new(false));
  thread::Builder::new().name("muxtick".into()).spawn({
    let server = Arc::clone(&server);
    let done = Arc::clone(&done);
    let path = path.to_path_buf();
    move || tick(&server, idle, &done, &path)
  })?;
  for stream in listener.incoming() {
    if done.load(Ordering::SeqCst) {
      break;
    }
    let Ok(stream) = stream else { continue };
    let server = Arc::clone(&server);
    server.connections.fetch_add(1, Ordering::SeqCst);
    let _ = thread::Builder::new().name("muxconn".into()).spawn(move || {
      let _ = connection(&server, stream);
      server.connections.fetch_sub(1, Ordering::SeqCst);
    });
  }
  let _ = std::fs::remove_file(path);
  Ok(())
}

/// Bind, clearing a stale socket a dead server left, and lock it to the
/// owner.
fn bind(path: &Path) -> io::Result<UnixListener> {
  use std::os::unix::fs::PermissionsExt;
  let listener = match UnixListener::bind(path) {
    Ok(l) => l,
    Err(_) if UnixStream::connect(path).is_err() => {
      let _ = std::fs::remove_file(path);
      UnixListener::bind(path)?
    }
    Err(e) => return Err(io::Error::new(io::ErrorKind::AddrInUse, e)),
  };
  std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
  Ok(listener)
}

/// Busy flags and the idle clock.
fn tick(server: &Server, idle: Duration, done: &AtomicBool, path: &Path) {
  let mut idle_since: Option<Instant> = None;
  loop {
    thread::sleep(TICK);
    let hosted: Vec<Arc<Hosted>> = server
      .sessions
      .lock()
      .unwrap_or_else(|e| e.into_inner())
      .values()
      .cloned()
      .collect();
    for h in &hosted {
      let busy = h.with_session(|s| s.foreground_running()).unwrap_or(false);
      if h.busy.swap(busy, Ordering::Relaxed) != busy {
        h.broadcast(frame::BUSY, &[busy as u8]);
      }
    }
    let quiet = hosted.is_empty() && server.connections.load(Ordering::SeqCst) == 0;
    match (quiet, idle_since) {
      (false, _) => idle_since = None,
      (true, None) => idle_since = Some(Instant::now()),
      (true, Some(t)) if t.elapsed() >= idle => {
        done.store(true, Ordering::SeqCst);
        // Wake the accept loop so it sees `done`.
        let _ = UnixStream::connect(path);
        return;
      }
      _ => {}
    }
  }
}

fn connection(server: &Arc<Server>, mut stream: UnixStream) -> io::Result<()> {
  if !same_user(&stream) {
    return Ok(());
  }
  stream.set_read_timeout(Some(Duration::from_secs(5)))?;
  let hello = frame::read(&mut stream)?;
  let protocol = hello.json().ok().and_then(|v| v.get("protocol")?.as_u64());
  if hello.kind != frame::HELLO || protocol != Some(frame::PROTOCOL as u64) {
    return refuse(&mut stream, "unsupported protocol");
  }
  let mut command = frame::read(&mut stream)?;
  let mut preload = Vec::new();
  if command.kind == frame::PRELOAD {
    preload = command.payload;
    command = frame::read(&mut stream)?;
  }
  match command.kind {
    frame::LIST => {
      let sessions: Vec<Value> = server
        .sessions
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .values()
        .map(|h| {
          json!({
            "id": h.id,
            "token": h.token,
            "title": h.with_session(|s| s.with_term(|t| t.title().to_string())).unwrap_or_default(),
            "clients": h.clients.lock().map(|c| c.len()).unwrap_or(0),
          })
        })
        .collect();
      frame::write_json(&mut stream, frame::LIST, &json!({ "sessions": sessions }))
    }
    frame::KILL => {
      let id = command.json()?.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
      let hosted = server.sessions.lock().unwrap_or_else(|e| e.into_inner()).get(&id).cloned();
      match hosted {
        Some(h) => {
          h.kill();
          frame::write(&mut stream, frame::EXIT, b"{}")
        }
        None => refuse(&mut stream, "no such session"),
      }
    }
    frame::OPEN => match open(server, &command, preload) {
      Ok(hosted) => attach(server, &hosted, stream),
      Err(e) => refuse(&mut stream, &format!("could not start the session: {e}")),
    },
    frame::ATTACH => {
      let id = command.json()?.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
      let hosted = server.sessions.lock().unwrap_or_else(|e| e.into_inner()).get(&id).cloned();
      match hosted {
        Some(h) => attach(server, &h, stream),
        None => refuse(&mut stream, "no such session"),
      }
    }
    _ => refuse(&mut stream, "unknown command"),
  }
}

fn refuse(stream: &mut UnixStream, why: &str) -> io::Result<()> {
  frame::write(stream, frame::ERROR, why.as_bytes())
}

/// Start a session from an `OPEN` request.
fn open(server: &Arc<Server>, command: &Frame, preload: Vec<u8>) -> io::Result<Arc<Hosted>> {
  let v = command.json()?;
  let num = |k: &str| v.get(k).and_then(Value::as_u64).unwrap_or(0);
  let argv: Vec<String> = v
    .get("argv")
    .and_then(Value::as_array)
    .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
    .unwrap_or_default();
  let env: Vec<(String, String)> = v
    .get("env")
    .and_then(Value::as_array)
    .map(|a| {
      a.iter()
        .filter_map(|p| Some((p.get(0)?.as_str()?.to_string(), p.get(1)?.as_str()?.to_string())))
        .collect()
    })
    .unwrap_or_default();
  let mut spawn = pty::SpawnOptions {
    argv,
    login: v.get("login").and_then(Value::as_bool).unwrap_or(false),
    env,
    cwd: v.get("cwd").and_then(Value::as_str).map(std::path::PathBuf::from),
    ..Default::default()
  };
  spawn.winsize.cell_width = num("cell_width") as u16;
  spawn.winsize.cell_height = num("cell_height") as u16;
  let options = SessionOptions {
    cols: num("cols").max(1) as usize,
    rows: num("rows").max(1) as usize,
    scrollback_limit: v
      .get("scrollback")
      .and_then(Value::as_u64)
      .map_or(vt::DEFAULT_SCROLLBACK, |n| n as usize),
    spawn,
    preload,
  };
  let (session, events) = Session::spawn(options)?;
  let hosted = Arc::new(Hosted {
    id: new_id(server),
    token: num("token"),
    session: Mutex::new(None),
    clients: Mutex::new(Vec::new()),
    busy: AtomicBool::new(false),
  });
  let weak: Weak<Hosted> = Arc::downgrade(&hosted);
  session.set_tap(Some(Arc::new(move |bytes: &[u8]| {
    if let Some(h) = weak.upgrade() {
      h.broadcast(frame::OUTPUT, bytes);
    }
  })));
  *hosted.session.lock().unwrap_or_else(|e| e.into_inner()) = Some(session);
  server
    .sessions
    .lock()
    .unwrap_or_else(|e| e.into_inner())
    .insert(hosted.id.clone(), Arc::clone(&hosted));
  thread::Builder::new().name("muxpump".into()).spawn({
    let server = Arc::clone(server);
    let hosted = Arc::clone(&hosted);
    move || pump(&server, &hosted, events)
  })?;
  Ok(hosted)
}

/// Drain a session's events: keep its wakeups acknowledged (the server
/// renders nothing, but the contract wants it) and turn its exit into the
/// clients' last frame.
fn pump(server: &Server, hosted: &Arc<Hosted>, events: crate::EventReceiver) {
  let mut code = None;
  for event in events.iter() {
    match event {
      Event::Wakeup => {
        hosted.with_session(|s| s.acknowledge_wakeup(s.output_generation()));
      }
      Event::Exit(c) => {
        code = c;
        break;
      }
      _ => {}
    }
  }
  server
    .sessions
    .lock()
    .unwrap_or_else(|e| e.into_inner())
    .remove(&hosted.id);
  hosted.broadcast(frame::EXIT, json!({ "code": code }).to_string().as_bytes());
  // Dropping the senders ends each client's writer, which hangs up.
  hosted.clients.lock().unwrap_or_else(|e| e.into_inner()).clear();
  hosted.kill();
}

/// Serve an attached client until it hangs up or the session ends.
fn attach(server: &Server, hosted: &Arc<Hosted>, stream: UnixStream) -> io::Result<()> {
  stream.set_read_timeout(None)?;
  let (tx, rx) = flume::unbounded::<Out>();
  let backlog = Arc::new(AtomicUsize::new(0));
  let id = server.next_client.fetch_add(1, Ordering::Relaxed);
  // Under the terminal lock: info and snapshot go first, then the client
  // joins the tap's list, so the next byte of output it sees is the first
  // one after the snapshot.
  let joined = hosted.with_session(|s| {
    s.with_term(|t| {
      let info = json!({
        "id": hosted.id,
        "token": hosted.token,
        "cols": t.cols(),
        "rows": t.rows(),
        "busy": hosted.busy.load(Ordering::Relaxed),
        "protocol": frame::PROTOCOL,
      });
      let _ = tx.send((frame::INFO, info.to_string().into_bytes()));
      let _ = tx.send((frame::SNAPSHOT, t.snapshot()));
      hosted
        .clients
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(Client {
          id,
          tx: tx.clone(),
          backlog: Arc::clone(&backlog),
        });
    })
  });
  drop(tx);
  if joined.is_none() {
    let mut stream = stream;
    return refuse(&mut stream, "the session has ended");
  }
  let mut out = stream.try_clone()?;
  let writer = thread::Builder::new().name("muxsend".into()).spawn(move || {
    for (kind, payload) in rx.iter() {
      backlog.fetch_sub(payload.len().min(backlog.load(Ordering::Relaxed)), Ordering::Relaxed);
      if frame::write(&mut out, kind, &payload).is_err() {
        break;
      }
    }
    let _ = out.shutdown(std::net::Shutdown::Both);
  })?;
  let mut input = stream;
  while let Ok(f) = frame::read(&mut input) {
    match f.kind {
      frame::INPUT => {
        hosted.with_session(|s| s.write(&f.payload));
      }
      frame::RESIZE => {
        if let Ok(v) = f.json() {
          let n = |k: &str| v.get(k).and_then(Value::as_u64).unwrap_or(0);
          let (cols, rows) = (n("cols").max(1) as usize, n("rows").max(1) as usize);
          hosted.with_session(|s| {
            s.resize_px(cols, rows, n("cell_width") as u16, n("cell_height") as u16)
          });
        }
      }
      frame::COLORS => {
        if let Some(colors) = frame::parse_colors(&f.payload) {
          hosted.with_session(|s| s.with_term(|t| t.set_report_colors(colors)));
        }
      }
      frame::KILL => hosted.kill(),
      _ => {}
    }
  }
  // Hung up: a detach. Leave the session running.
  hosted
    .clients
    .lock()
    .unwrap_or_else(|e| e.into_inner())
    .retain(|c| c.id != id);
  let _ = writer.join();
  Ok(())
}

fn new_id(server: &Server) -> String {
  use std::hash::{BuildHasher, Hasher};
  let mut h = std::collections::hash_map::RandomState::new().build_hasher();
  h.write_u64(server.next_client.fetch_add(1, Ordering::Relaxed));
  h.write_u128(
    std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .map(|d| d.as_nanos())
      .unwrap_or(0),
  );
  format!("{:012x}", h.finish() & 0xffff_ffff_ffff)
}

/// Only the user who owns the server may talk to it.
fn same_user(stream: &UnixStream) -> bool {
  use std::os::unix::io::AsRawFd;
  let fd = stream.as_raw_fd();
  // SAFETY: plain syscalls reading the peer's credentials from our socket.
  unsafe {
    let me = libc::getuid();
    #[cfg(target_os = "macos")]
    {
      let mut uid: libc::uid_t = 0;
      let mut gid: libc::gid_t = 0;
      libc::getpeereid(fd, &mut uid, &mut gid) == 0 && uid == me
    }
    #[cfg(not(target_os = "macos"))]
    {
      let mut cred = libc::ucred { pid: 0, uid: 0, gid: 0 };
      let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
      libc::getsockopt(
        fd,
        libc::SOL_SOCKET,
        libc::SO_PEERCRED,
        (&mut cred as *mut libc::ucred).cast(),
        &mut len,
      ) == 0
        && cred.uid == me
    }
  }
}

#[cfg(test)]
#[path = "../../tests/mux/server.rs"]
mod tests;
