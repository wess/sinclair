//! Unix session backend: poll-pumped pty IO with queued input.

use std::collections::VecDeque;
use std::io;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::{event_channel, Counters, EventSender, SessionStats};
use crate::event::Event;
use crate::options::SessionOptions;
use crate::EventReceiver;

#[cfg(test)]
use std::sync::mpsc;

/// How long the group SIGHUP gets before teardown escalates to SIGKILL.
const HANGUP_GRACE: Duration = Duration::from_millis(200);

/// Queue capacity kept after a large paste has drained.
const INPUT_SLACK: usize = 64 * 1024;
/// Maximum pending user input per session. A child that stops reading must not
/// turn repeated pastes or agent writes into unbounded process memory.
const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;

/// A running child attached to a [`vt::Terminal`].
///
/// A reader thread pumps pty output into the terminal, drains queued input
/// when the pty can take it, and reports [`Event`]s on the channel returned
/// by [`Session::spawn`]. Dropping the session (or calling
/// [`Session::shutdown`]) hangs up the child's process group, escalating to
/// SIGKILL, and joins the reader thread; the join is bounded because the
/// waker always breaks the reader out of its poll.
pub struct Session {
  /// What the session is attached to: a pty this process owns, or a session
  /// the session server owns (see [`crate::mux`]).
  link: Link,
  /// Pending input bytes; the reader thread writes them out in order.
  input: Arc<Mutex<VecDeque<u8>>>,
  stop: Arc<AtomicBool>,
  term: Arc<Mutex<vt::Terminal>>,
  /// `true` while an unconsumed [`Event::Wakeup`] sits in the channel.
  wakeup_pending: Arc<AtomicBool>,
  /// Increments after each output chunk has been applied to the terminal.
  output_generation: Arc<AtomicU64>,
  stats: Arc<Counters>,
  /// Active asciinema recording; the reader thread writes output into it.
  recorder: Arc<Mutex<Option<cast::Recorder>>>,
  /// Sees every output chunk, under the terminal lock (see [`Tap`]).
  tap: Arc<Mutex<Option<Tap>>>,
  reader: Option<JoinHandle<()>>,
  /// The remote link's socket writer.
  writer: Option<JoinHandle<()>>,
}

/// Called with every chunk of child output, while the terminal lock is held
/// and right after the chunk was parsed. Holding the lock is the point: a
/// caller that snapshots the terminal under the same lock and then starts
/// listening can never miss or double a byte.
pub type Tap = Arc<dyn Fn(&[u8]) + Send + Sync>;

enum Link {
  Pty {
    control: pty::Control,
    waker: pty::Waker,
  },
  Remote(Arc<Remote>),
}

/// One frame for the server: its kind and payload.
type Outgoing = (u8, Vec<u8>);

/// The client side of a session the server owns.
struct Remote {
  /// Frames for the writer thread: input, resizes, colors, kill. Taken (and
  /// so dropped) on teardown, which is what ends the writer.
  out: Mutex<Option<flume::Sender<Outgoing>>>,
  /// Whether a foreground job is running, as the server last reported.
  busy: Arc<AtomicBool>,
  socket: std::os::unix::net::UnixStream,
}

impl Remote {
  /// Queue a frame for the server. `false` when the queue is full or gone.
  fn send(&self, kind: u8, payload: Vec<u8>) -> bool {
    self
      .out
      .lock()
      .unwrap_or_else(|e| e.into_inner())
      .as_ref()
      .is_some_and(|tx| tx.try_send((kind, payload)).is_ok())
  }

  /// Drop the sender so the writer drains what is queued and ends.
  fn close(&self) {
    self.out.lock().unwrap_or_else(|e| e.into_inner()).take();
  }
}

/// The shared state an output chunk is applied to, whichever link it came
/// from.
struct Sink {
  term: Arc<Mutex<vt::Terminal>>,
  pending: Arc<AtomicBool>,
  generation: Arc<AtomicU64>,
  stats: Arc<Counters>,
  recorder: Arc<Mutex<Option<cast::Recorder>>>,
  tap: Arc<Mutex<Option<Tap>>>,
  events: EventSender,
  scrollback_limit: usize,
}

/// Everything the reader thread owns, including the child handle so it can
/// block in `wait` without holding any lock the embedder needs.
struct Reader {
  pty: pty::Pty,
  pump: pty::Pump,
  input: Arc<Mutex<VecDeque<u8>>>,
  stop: Arc<AtomicBool>,
  sink: Sink,
}

impl Session {
  /// Spawn the child on a pty and start the reader thread.
  ///
  /// Wakeup contract: `Event::Wakeup` is coalesced until the embedder calls
  /// [`Session::acknowledge_wakeup`] after processing the observed output
  /// generation. Spurious wakeups are possible; missed ones are not.
  pub fn spawn(options: SessionOptions) -> io::Result<(Session, EventReceiver)> {
    let SessionOptions {
      cols,
      rows,
      scrollback_limit,
      mut spawn,
      preload,
    } = options;
    spawn.winsize.cols = cols as u16;
    spawn.winsize.rows = rows as u16;

    let pty = pty::Pty::spawn(&spawn)?;
    let control = pty.control()?;
    let (pump, waker) = pty.pump()?;

    let term = Arc::new(Mutex::new(vt::Terminal::new(cols, rows, scrollback_limit)));
    super::preload(&term, &preload, scrollback_limit);
    let input = Arc::new(Mutex::new(VecDeque::new()));
    let stop = Arc::new(AtomicBool::new(false));
    let wakeup_pending = Arc::new(AtomicBool::new(false));
    let output_generation = Arc::new(AtomicU64::new(0));
    let stats = Arc::new(Counters::default());
    let recorder: Arc<Mutex<Option<cast::Recorder>>> = Arc::new(Mutex::new(None));
    let tap: Arc<Mutex<Option<Tap>>> = Arc::new(Mutex::new(None));
    let (events, receiver) = event_channel(Arc::clone(&stats));

    // On spawn failure the closure is dropped, and with it the pty,
    // whose Drop kills and reaps the just-started child.
    let reader = thread::Builder::new()
      .name("ptyreader".to_string())
      .spawn({
        let reader = Reader {
          pty,
          pump,
          input: Arc::clone(&input),
          stop: Arc::clone(&stop),
          sink: Sink {
            term: Arc::clone(&term),
            pending: Arc::clone(&wakeup_pending),
            generation: Arc::clone(&output_generation),
            stats: Arc::clone(&stats),
            recorder: Arc::clone(&recorder),
            tap: Arc::clone(&tap),
            events,
            scrollback_limit,
          },
        };
        move || read_loop(reader)
      })?;

    Ok((
      Session {
        link: Link::Pty { control, waker },
        input,
        stop,
        term,
        wakeup_pending,
        output_generation,
        stats,
        recorder,
        tap,
        reader: Some(reader),
        writer: None,
      },
      receiver,
    ))
  }

  /// Attach to a session the session server owns, starting it first when
  /// `how` is [`crate::mux::Attach::Open`]. The terminal is rebuilt from the
  /// server's snapshot, then follows its output. `cols`/`rows` is the size
  /// the caller wants; the server's session is resized to it.
  ///
  /// The terminal here answers nothing: the server's own terminal already
  /// replied to the child, so replies generated locally are dropped.
  pub fn attach(
    path: &std::path::Path,
    how: crate::mux::Attach,
    cols: usize,
    rows: usize,
    scrollback_limit: usize,
  ) -> io::Result<(Session, EventReceiver, crate::mux::Attached)> {
    let (socket, attached, snapshot) = crate::mux::client::handshake(path, how, cols, rows)?;
    let term = Arc::new(Mutex::new(vt::Terminal::new(
      attached.cols,
      attached.rows,
      scrollback_limit,
    )));
    super::preload(&term, &snapshot, scrollback_limit);
    let input = Arc::new(Mutex::new(VecDeque::new()));
    let stop = Arc::new(AtomicBool::new(false));
    let wakeup_pending = Arc::new(AtomicBool::new(false));
    let output_generation = Arc::new(AtomicU64::new(0));
    let stats = Arc::new(Counters::default());
    let recorder: Arc<Mutex<Option<cast::Recorder>>> = Arc::new(Mutex::new(None));
    let tap: Arc<Mutex<Option<Tap>>> = Arc::new(Mutex::new(None));
    let (events, receiver) = event_channel(Arc::clone(&stats));
    let busy = Arc::new(AtomicBool::new(attached.busy));
    let (out, queue) = flume::bounded::<(u8, Vec<u8>)>(REMOTE_QUEUE);

    let mut write_half = socket.try_clone()?;
    let writer = thread::Builder::new()
      .name("muxwriter".to_string())
      .spawn(move || {
        for (kind, payload) in queue.iter() {
          if crate::mux::frame::write(&mut write_half, kind, &payload).is_err() {
            break;
          }
        }
      })?;
    let read_half = socket.try_clone()?;
    let sink = Sink {
      term: Arc::clone(&term),
      pending: Arc::clone(&wakeup_pending),
      generation: Arc::clone(&output_generation),
      stats: Arc::clone(&stats),
      recorder: Arc::clone(&recorder),
      tap: Arc::clone(&tap),
      events,
      scrollback_limit,
    };
    let reader = thread::Builder::new()
      .name("muxreader".to_string())
      .spawn({
        let busy = Arc::clone(&busy);
        move || remote_loop(read_half, sink, busy)
      })?;
    let session = Session {
      link: Link::Remote(Arc::new(Remote {
        out: Mutex::new(Some(out)),
        busy,
        socket,
      })),
      input,
      stop,
      term,
      wakeup_pending,
      output_generation,
      stats,
      recorder,
      tap,
      reader: Some(reader),
      writer: Some(writer),
    };
    if (attached.cols, attached.rows) != (cols, rows) {
      let _ = session.resize(cols, rows);
    }
    Ok((session, receiver, attached))
  }

  /// Whether this session lives in the session server rather than here.
  pub fn is_remote(&self) -> bool {
    matches!(self.link, Link::Remote(_))
  }

  /// Watch every output chunk (see [`Tap`]). Replaces any earlier tap.
  pub fn set_tap(&self, tap: Option<Tap>) {
    *self.tap.lock().unwrap_or_else(|e| e.into_inner()) = tap;
  }

  /// Tell the server which colors to answer OSC color queries with. Locally
  /// owned sessions answer from their own terminal, so this does nothing
  /// for them.
  pub fn push_report_colors(&self, colors: &vt::ReportColors) {
    if let Link::Remote(remote) = &self.link {
      remote.send(crate::mux::frame::COLORS, crate::mux::frame::colors(colors));
    }
  }

  /// Let go of a server-owned session without ending it, so another client
  /// (this app, relaunched) can attach to it again. For a local session this
  /// is [`Session::shutdown`].
  pub fn detach(mut self) {
    self.join();
  }

  /// Queue user input for the child and return immediately. The reader
  /// thread drains the queue as the pty accepts bytes, so a stopped or
  /// slow child can never block the caller.
  pub fn write(&self, bytes: &[u8]) -> io::Result<()> {
    if bytes.is_empty() {
      return Ok(());
    }
    let waker = match &self.link {
      Link::Pty { waker, .. } => waker,
      Link::Remote(remote) => {
        if !remote.send(crate::mux::frame::INPUT, bytes.to_vec()) {
          return Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "terminal input queue is full",
          ));
        }
        self
          .stats
          .input_bytes
          .fetch_add(bytes.len() as u64, Ordering::Relaxed);
        return Ok(());
      }
    };
    let mut input = self.input.lock().unwrap_or_else(|e| e.into_inner());
    if input.len().saturating_add(bytes.len()) > MAX_INPUT_BYTES {
      return Err(io::Error::new(
        io::ErrorKind::WouldBlock,
        "terminal input queue is full",
      ));
    }
    input.extend(bytes);
    drop(input);
    self
      .stats
      .input_bytes
      .fetch_add(bytes.len() as u64, Ordering::Relaxed);
    waker.wake();
    Ok(())
  }

  /// Bytes waiting for the child to accept them.
  pub fn pending_input_bytes(&self) -> usize {
    self.input.lock().unwrap_or_else(|e| e.into_inner()).len()
  }

  /// Current resource/performance counters for this session.
  pub fn stats(&self) -> SessionStats {
    self.stats.snapshot(self.pending_input_bytes())
  }

  /// Resize both the emulation grid and the kernel pty winsize. The
  /// kernel delivers SIGWINCH to the child's process group.
  pub fn resize(&self, cols: usize, rows: usize) -> io::Result<()> {
    self.resize_px(cols, rows, 0, 0)
  }

  /// [`Session::resize`] with per-cell pixel dimensions, so the winsize
  /// reports `ws_xpixel`/`ws_ypixel` to pixel-addressing programs (kitty
  /// graphics, sixel).
  pub fn resize_px(
    &self,
    cols: usize,
    rows: usize,
    cell_width: u16,
    cell_height: u16,
  ) -> io::Result<()> {
    self
      .term
      .lock()
      .unwrap_or_else(|e| e.into_inner())
      .resize(cols, rows);
    self.stats.resize_commits.fetch_add(1, Ordering::Relaxed);
    match &self.link {
      Link::Pty { control, .. } => {
        let size = pty::Winsize::with_cell_size(cols as u16, rows as u16, cell_width, cell_height);
        control.resize(size)
      }
      Link::Remote(remote) => {
        let msg = serde_json::json!({
          "cols": cols, "rows": rows, "cell_width": cell_width, "cell_height": cell_height,
        });
        remote.send(crate::mux::frame::RESIZE, msg.to_string().into_bytes());
        Ok(())
      }
    }
  }

  /// Whether an asciinema recording is currently capturing this session.
  pub fn is_recording(&self) -> bool {
    self.recorder.lock().map(|r| r.is_some()).unwrap_or(false)
  }

  /// Begin recording output to `path` as an asciinema v2 cast. Replaces any
  /// recording already in progress.
  pub fn start_recording(
    &self,
    path: std::path::PathBuf,
    title: Option<&str>,
    timestamp: Option<u64>,
  ) -> io::Result<()> {
    let (cols, rows) = {
      let term = self.term.lock().unwrap_or_else(|e| e.into_inner());
      (term.cols(), term.rows())
    };
    let recorder = cast::Recorder::create(path, cols, rows, title, timestamp)?;
    *self.recorder.lock().unwrap_or_else(|e| e.into_inner()) = Some(recorder);
    Ok(())
  }

  /// Stop recording, flushing the file. Returns the saved path, if any.
  pub fn stop_recording(&self) -> Option<std::path::PathBuf> {
    let recorder = self.recorder.lock().ok()?.take()?;
    recorder.finish().ok()
  }

  /// Whether a foreground process other than the shell is running on the pty
  /// (e.g. an editor or a long-running command), for quit/close warnings.
  pub fn foreground_running(&self) -> bool {
    match &self.link {
      Link::Pty { control, .. } => control.foreground_running(),
      Link::Remote(remote) => remote.busy.load(Ordering::Relaxed),
    }
  }

  /// Run `f` with the terminal locked, for rendering or inspection.
  pub fn with_term<R>(&self, f: impl FnOnce(&mut vt::Terminal) -> R) -> R {
    let mut term = self.term.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut term)
  }

  /// Current generation of output applied to the terminal.
  pub fn output_generation(&self) -> u64 {
    self.output_generation.load(Ordering::Acquire)
  }

  /// Acknowledge the pending wakeup after processing `observed_generation`.
  /// Returns `true` when output raced with that processing and another pass
  /// should be scheduled immediately. Output arriving after the pending flag
  /// is cleared will enqueue its own wakeup.
  pub fn acknowledge_wakeup(&self, observed_generation: u64) -> bool {
    self
      .stats
      .wakeup_acknowledgements
      .fetch_add(1, Ordering::Relaxed);
    self.wakeup_pending.store(false, Ordering::Release);
    self.output_generation.load(Ordering::Acquire) != observed_generation
  }

  /// End the child and join the reader thread. The reader reaps the child
  /// and sends [`Event::Exit`] before ending.
  pub fn shutdown(mut self) {
    self.teardown();
  }

  /// SIGHUP the child's process group, escalate to a group SIGKILL after a
  /// grace period, then break the reader out of its poll and join it. The
  /// child itself cannot survive the SIGKILL, so the reader's blocking
  /// `wait` — and therefore the join — always returns, even if some
  /// descendant keeps the pty slave open.
  ///
  /// A server-owned session is ended by asking the server to kill it —
  /// unless the app is quitting with [`crate::mux::detach_on_drop`] set, in
  /// which case it is only let go.
  fn teardown(&mut self) {
    if let Link::Remote(remote) = &self.link {
      if !crate::mux::detaching() {
        remote.send(crate::mux::frame::KILL, Vec::new());
      }
      self.join();
      return;
    }
    let Some(handle) = self.reader.take() else {
      return;
    };
    let Link::Pty { control, waker } = &self.link else {
      return;
    };
    if !handle.is_finished() {
      let _ = control.hangup();
      let deadline = Instant::now() + HANGUP_GRACE;
      while !handle.is_finished() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
      }
      if !handle.is_finished() {
        let _ = control.kill();
      }
    }
    self.stop.store(true, Ordering::SeqCst);
    waker.wake();
    let _ = handle.join();
  }

  /// Wind down a remote link: close the queue so the writer delivers what
  /// is in it (a kill, say) and ends, then shut the socket so the reader
  /// ends.
  fn join(&mut self) {
    if let Link::Remote(remote) = &self.link {
      remote.close();
      if let Some(writer) = self.writer.take() {
        let _ = writer.join();
      }
      let _ = remote.socket.shutdown(std::net::Shutdown::Both);
    }
    if let Some(reader) = self.reader.take() {
      let _ = reader.join();
    }
  }
}

impl Drop for Session {
  /// Best-effort [`Session::shutdown`].
  fn drop(&mut self) {
    self.teardown();
  }
}

/// Pump child output into the terminal and queued input into the pty until
/// EOF/EIO or a stop request, then reap the child and report its exit.
fn read_loop(mut r: Reader) {
  let mut buf = [0u8; 65536];
  loop {
    let want_write = !r.input.lock().unwrap_or_else(|e| e.into_inner()).is_empty();
    let Ok(ready) = r.pump.wait(want_write) else {
      break;
    };
    if r.stop.load(Ordering::SeqCst) {
      break;
    }
    if ready.writable {
      drain_input(&r.pump, &r.input);
    }
    if ready.readable {
      match fill(&r.pump, &mut buf) {
        Ok(0) => break,
        Ok(n) => {
          let reply = apply_chunk(&buf[..n], &r.sink);
          if !reply.is_empty() {
            r.input
              .lock()
              .unwrap_or_else(|e| e.into_inner())
              .extend(&reply);
          }
        }
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
        Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
        Err(_) => break, // EIO once the child side is fully closed
      }
    }
  }
  let code = r.pty.wait().ok().and_then(|status| status.code());
  super::end_status(&r.sink.term, &r.sink.events);
  let _ = r.sink.events.send(Event::Exit(code));
}

/// Remote frames queued for the writer before `write` reports the queue
/// full. Each is one keystroke burst or paste chunk.
const REMOTE_QUEUE: usize = 4096;

/// Follow a server-owned session: apply its output, track its busy flag,
/// and report its exit. A dropped connection reads as an exit with no code —
/// the pane can't show what it can no longer see.
fn remote_loop(mut socket: std::os::unix::net::UnixStream, sink: Sink, busy: Arc<AtomicBool>) {
  use crate::mux::frame;
  let mut code = None;
  while let Ok(f) = frame::read(&mut socket) {
    match f.kind {
      frame::OUTPUT => {
        // The server's terminal answered the child already.
        let _ = apply_chunk(&f.payload, &sink);
      }
      frame::BUSY => busy.store(f.payload.first() == Some(&1), Ordering::Relaxed),
      frame::EXIT => {
        code = f
          .json()
          .ok()
          .and_then(|v| v.get("code").and_then(serde_json::Value::as_i64))
          .map(|c| c as i32);
        break;
      }
      _ => {}
    }
  }
  super::end_status(&sink.term, &sink.events);
  let _ = sink.events.send(Event::Exit(code));
}

/// Read what the pty has, then keep reading until it is empty or `buf` is
/// full, so one parse (one terminal lock, one round of events) covers a burst.
/// A pty hands output over in whatever pieces the child wrote — `yes | head`
/// arrives ~100 bytes at a time — and paying the per-chunk cost for each
/// piece made the reader, not the parser, the bottleneck. Returns 0 only at
/// EOF on the first read; later errors end the fill and keep what was read.
fn fill(pump: &pty::Pump, buf: &mut [u8]) -> io::Result<usize> {
  let mut n = pump.read(buf)?;
  if n == 0 {
    return Ok(0);
  }
  while n < buf.len() {
    match pump.read(&mut buf[n..]) {
      Ok(0) => break,
      Ok(more) => n += more,
      Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
      Err(_) => break, // WouldBlock (drained) or EIO; the next wait sees it
    }
  }
  Ok(n)
}

/// Write queued input until the pty stops taking it, preserving order. On a
/// dead pty the queue is discarded; the read side reports the exit.
fn drain_input(pump: &pty::Pump, input: &Mutex<VecDeque<u8>>) {
  let mut queue = input.lock().unwrap_or_else(|e| e.into_inner());
  while !queue.is_empty() {
    let (front, _) = queue.as_slices();
    match pump.write(front) {
      Ok(0) => break,
      Ok(n) => {
        queue.drain(..n);
      }
      Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
      Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
      Err(_) => {
        queue.clear();
        break;
      }
    }
  }
  if queue.is_empty() && queue.capacity() > INPUT_SLACK {
    queue.shrink_to(INPUT_SLACK);
  }
}

/// Feed one chunk into the terminal and emit the resulting events. Returns
/// the replies the terminal generated (DSR, DA, ...) for the caller to route:
/// a pty session queues them as input so they leave in order with user
/// keystrokes; a remote one drops them, the server having answered already.
fn apply_chunk(chunk: &[u8], r: &Sink) -> Vec<u8> {
  if let Ok(mut rec) = r.recorder.lock() {
    if let Some(rec) = rec.as_mut() {
      let _ = rec.output(chunk);
    }
  }
  r.stats
    .output_bytes
    .fetch_add(chunk.len() as u64, Ordering::Relaxed);
  r.stats.output_chunks.fetch_add(1, Ordering::Relaxed);
  let (
    reply,
    title,
    bell,
    clipboard,
    clipboard_requests,
    resize_request,
    notification,
    command_finished,
    cwd_changed,
    status_changed,
  ) = {
    let mut term = r.term.lock().unwrap_or_else(|e| e.into_inner());
    // The reader thread must survive parser bugs, but a panicking feed
    // leaves the terminal half-mutated: log it and start over from a
    // fresh terminal at the current size rather than resume blind.
    let fed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| term.feed(chunk)));
    if fed.is_err() {
      eprintln!(
        "vt: parser panicked on a {}-byte chunk; resetting terminal state",
        chunk.len()
      );
      *term = vt::Terminal::new(term.cols(), term.rows(), r.scrollback_limit);
    }
    // Still under the terminal lock: see `Tap`.
    if let Some(tap) = r.tap.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
      tap(chunk);
    }
    (
      term.take_output(),
      term.take_title_changed(),
      term.take_bell(),
      term.take_clipboard(),
      term.take_clipboard_requests(),
      term.take_resize_request(),
      term.take_notification(),
      term.take_command_finished(),
      term.take_cwd_changed(),
      term.take_status_changed(),
    )
  };
  if let Some(title) = title {
    let _ = r.events.send(Event::TitleChanged(title));
  }
  if bell {
    let _ = r.events.send(Event::Bell);
  }
  if let Some(clip) = clipboard {
    let _ = r.events.send(Event::Clipboard {
      kind: clip.kind,
      data: clip.data,
    });
  }
  if let Some((rows, cols)) = resize_request {
    let _ = r.events.send(Event::ResizeRequest { rows, cols });
  }
  for request in clipboard_requests {
    let _ = r.events.send(Event::ClipboardRequest(Box::new(request)));
  }
  if let Some(note) = notification {
    let _ = r.events.send(Event::Notify {
      title: note.title,
      body: note.body,
    });
  }
  if let Some(code) = command_finished {
    let _ = r.events.send(Event::CommandFinished(code));
  }
  if let Some(dir) = cwd_changed {
    let _ = r.events.send(Event::DirChanged(dir));
  }
  if status_changed {
    let _ = r.events.send(Event::StatusChanged);
  }
  r.generation.fetch_add(1, Ordering::Release);
  if !r.pending.swap(true, Ordering::SeqCst) {
    if r.events.send(Event::Wakeup).is_ok() {
      r.stats.wakeups.fetch_add(1, Ordering::Relaxed);
    } else {
      r.pending.store(false, Ordering::Release);
    }
  }
  reply
}

#[cfg(all(test, unix))]
#[path = "../../tests/session.rs"]
mod tests;
