//! A live terminal session: pty child + vt emulation + reader thread.

use std::collections::VecDeque;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::event::Event;
use crate::options::SessionOptions;

/// How long the group SIGHUP gets before teardown escalates to SIGKILL.
const HANGUP_GRACE: Duration = Duration::from_millis(200);

/// Queue capacity kept after a large paste has drained.
const INPUT_SLACK: usize = 64 * 1024;

/// A running child attached to a [`vt::Terminal`].
///
/// A reader thread pumps pty output into the terminal, drains queued input
/// when the pty can take it, and reports [`Event`]s on the channel returned
/// by [`Session::spawn`]. Dropping the session (or calling
/// [`Session::shutdown`]) hangs up the child's process group, escalating to
/// SIGKILL, and joins the reader thread; the join is bounded because the
/// waker always breaks the reader out of its poll.
pub struct Session {
    control: pty::Control,
    /// Pending input bytes; the reader thread writes them out in order.
    input: Arc<Mutex<VecDeque<u8>>>,
    waker: pty::Waker,
    stop: Arc<AtomicBool>,
    term: Arc<Mutex<vt::Terminal>>,
    /// `true` while an unconsumed [`Event::Wakeup`] sits in the channel.
    wakeup_pending: Arc<AtomicBool>,
    /// Active asciinema recording; the reader thread writes output into it.
    recorder: Arc<Mutex<Option<cast::Recorder>>>,
    reader: Option<JoinHandle<()>>,
}

/// Everything the reader thread owns, including the child handle so it can
/// block in `wait` without holding any lock the embedder needs.
struct Reader {
    pty: pty::Pty,
    pump: pty::Pump,
    input: Arc<Mutex<VecDeque<u8>>>,
    stop: Arc<AtomicBool>,
    term: Arc<Mutex<vt::Terminal>>,
    pending: Arc<AtomicBool>,
    recorder: Arc<Mutex<Option<cast::Recorder>>>,
    events: Sender<Event>,
    scrollback_limit: usize,
}

impl Session {
    /// Spawn the child on a pty and start the reader thread.
    ///
    /// Wakeup contract: `Event::Wakeup` is coalesced. The reader sends one
    /// only when none is pending, then sets the pending flag. The embedder
    /// re-arms it by locking the terminal via [`Session::with_term`] (the
    /// natural render path). Spurious wakeups are possible; missed ones are
    /// not.
    pub fn spawn(options: SessionOptions) -> io::Result<(Session, Receiver<Event>)> {
        let SessionOptions {
            cols,
            rows,
            scrollback_limit,
            mut spawn,
        } = options;
        spawn.winsize.cols = cols as u16;
        spawn.winsize.rows = rows as u16;

        let pty = pty::Pty::spawn(&spawn)?;
        let control = pty.control()?;
        let (pump, waker) = pty.pump()?;

        let term = Arc::new(Mutex::new(vt::Terminal::new(cols, rows, scrollback_limit)));
        let input = Arc::new(Mutex::new(VecDeque::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let wakeup_pending = Arc::new(AtomicBool::new(false));
        let recorder: Arc<Mutex<Option<cast::Recorder>>> = Arc::new(Mutex::new(None));
        let (events, receiver) = mpsc::channel();

        // On spawn failure the closure is dropped, and with it the pty,
        // whose Drop kills and reaps the just-started child.
        let reader = thread::Builder::new().name("ptyreader".to_string()).spawn({
            let reader = Reader {
                pty,
                pump,
                input: Arc::clone(&input),
                stop: Arc::clone(&stop),
                term: Arc::clone(&term),
                pending: Arc::clone(&wakeup_pending),
                recorder: Arc::clone(&recorder),
                events,
                scrollback_limit,
            };
            move || read_loop(reader)
        })?;

        Ok((
            Session {
                control,
                input,
                waker,
                stop,
                term,
                wakeup_pending,
                recorder,
                reader: Some(reader),
            },
            receiver,
        ))
    }

    /// Queue user input for the child and return immediately. The reader
    /// thread drains the queue as the pty accepts bytes, so a stopped or
    /// slow child can never block the caller.
    pub fn write(&self, bytes: &[u8]) -> io::Result<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        self.input
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .extend(bytes);
        self.waker.wake();
        Ok(())
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
        self.term
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .resize(cols, rows);
        let size = pty::Winsize::with_cell_size(cols as u16, rows as u16, cell_width, cell_height);
        self.control.resize(size)
    }

    /// Whether an asciinema recording is currently capturing this session.
    pub fn is_recording(&self) -> bool {
        self.recorder
            .lock()
            .map(|r| r.is_some())
            .unwrap_or(false)
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
        self.control.foreground_running()
    }

    /// Run `f` with the terminal locked, for rendering or inspection.
    ///
    /// Clears the wakeup-pending flag *before* taking the lock, so output
    /// applied while (or after) `f` runs raises a fresh [`Event::Wakeup`].
    pub fn with_term<R>(&self, f: impl FnOnce(&mut vt::Terminal) -> R) -> R {
        self.wakeup_pending.store(false, Ordering::SeqCst);
        let mut term = self.term.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut term)
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
    fn teardown(&mut self) {
        let Some(handle) = self.reader.take() else {
            return;
        };
        if !handle.is_finished() {
            let _ = self.control.hangup();
            let deadline = Instant::now() + HANGUP_GRACE;
            while !handle.is_finished() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
            if !handle.is_finished() {
                let _ = self.control.kill();
            }
        }
        self.stop.store(true, Ordering::SeqCst);
        self.waker.wake();
        let _ = handle.join();
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
        let want_write = !r
            .input
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_empty();
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
            match r.pump.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if let Ok(mut rec) = r.recorder.lock() {
                        if let Some(rec) = rec.as_mut() {
                            let _ = rec.output(&buf[..n]);
                        }
                    }
                    apply_chunk(&buf[..n], &r);
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => break, // EIO once the child side is fully closed
            }
        }
    }
    let code = r.pty.wait().ok().and_then(|status| status.code());
    let _ = r.events.send(Event::Exit(code));
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

/// Feed one chunk into the terminal and emit the resulting events. Replies
/// the terminal generates (DSR, DA, ...) are queued as input so they leave
/// in order with user keystrokes.
fn apply_chunk(chunk: &[u8], r: &Reader) {
    let (reply, title, bell, clipboard, notification, command_finished, cwd_changed) = {
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
        (
            term.take_output(),
            term.take_title_changed(),
            term.take_bell(),
            term.take_clipboard(),
            term.take_notification(),
            term.take_command_finished(),
            term.take_cwd_changed(),
        )
    };
    if !reply.is_empty() {
        r.input
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .extend(&reply);
    }
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
    if !r.pending.swap(true, Ordering::SeqCst) {
        let _ = r.events.send(Event::Wakeup);
    }
}

#[cfg(all(test, unix))]
#[path = "../tests/session.rs"]
mod tests;
