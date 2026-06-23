//! A live terminal session: pty child + vt emulation + reader thread.

use std::fs::File;
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::event::Event;
use crate::options::SessionOptions;

/// A running child attached to a [`vt::Terminal`].
///
/// A reader thread pumps pty output into the terminal and reports
/// [`Event`]s on the channel returned by [`Session::spawn`]. Dropping the
/// session (or calling [`Session::shutdown`]) kills the child and joins
/// the reader thread.
pub struct Session {
    pty: Arc<Mutex<pty::Pty>>,
    /// Dup of the master fd for user input; writes need no lock.
    writer: File,
    term: Arc<Mutex<vt::Terminal>>,
    /// `true` while an unconsumed [`Event::Wakeup`] sits in the channel.
    wakeup_pending: Arc<AtomicBool>,
    reader: Option<JoinHandle<()>>,
}

impl Session {
    /// Spawn the child on a pty and start the reader thread.
    ///
    /// Wakeup contract: `Event::Wakeup` is coalesced. The reader sends one
    /// only when none is pending, then sets the pending flag. The embedder
    /// re-arms it by locking the terminal via [`Session::with_term`] (the
    /// natural render path) or with [`Session::clear_wakeup`]. Spurious
    /// wakeups are possible; missed ones are not.
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
        let output = pty.try_clone_reader()?;
        let replies = pty.try_clone_writer()?;
        let writer = pty.try_clone_writer()?;

        let term = Arc::new(Mutex::new(vt::Terminal::new(cols, rows, scrollback_limit)));
        let pty = Arc::new(Mutex::new(pty));
        let wakeup_pending = Arc::new(AtomicBool::new(false));
        let (events, receiver) = mpsc::channel();

        let reader = thread::Builder::new()
            .name("ptyreader".to_string())
            .spawn({
                let pty = Arc::clone(&pty);
                let term = Arc::clone(&term);
                let pending = Arc::clone(&wakeup_pending);
                move || read_loop(output, replies, pty, term, pending, events)
            })?;

        Ok((
            Session {
                pty,
                writer,
                term,
                wakeup_pending,
                reader: Some(reader),
            },
            receiver,
        ))
    }

    /// Send user input to the child. Loops until every byte is written.
    pub fn write(&self, bytes: &[u8]) -> io::Result<()> {
        (&self.writer).write_all(bytes)
    }

    /// Resize both the emulation grid and the kernel pty winsize. The
    /// kernel delivers SIGWINCH to the child's process group.
    pub fn resize(&self, cols: usize, rows: usize) -> io::Result<()> {
        self.term.lock().expect("terminal lock").resize(cols, rows);
        let size = pty::Winsize::new(cols as u16, rows as u16);
        self.pty.lock().expect("pty lock").resize(size)
    }

    /// Run `f` with the terminal locked, for rendering or inspection.
    ///
    /// Clears the wakeup-pending flag *before* taking the lock, so output
    /// applied while (or after) `f` runs raises a fresh [`Event::Wakeup`].
    pub fn with_term<R>(&self, f: impl FnOnce(&mut vt::Terminal) -> R) -> R {
        self.wakeup_pending.store(false, Ordering::SeqCst);
        let mut term = self.term.lock().expect("terminal lock");
        f(&mut term)
    }

    /// Re-arm [`Event::Wakeup`] without touching the terminal; for
    /// embedders that consume the event but defer rendering.
    pub fn clear_wakeup(&self) {
        self.wakeup_pending.store(false, Ordering::SeqCst);
    }

    /// Kill the child and join the reader thread. The kill closes the
    /// child's side of the pty, which unblocks the reader at EOF; the
    /// reader reaps the child and sends [`Event::Exit`] before ending.
    pub fn shutdown(mut self) {
        self.teardown();
    }

    fn teardown(&mut self) {
        let Some(handle) = self.reader.take() else {
            return;
        };
        if let Ok(mut pty) = self.pty.lock() {
            // Best effort: fails harmlessly when the child already exited.
            let _ = pty.kill();
        }
        let _ = handle.join();
        // Dropping the Session afterwards drops the last master fds.
    }
}

impl Drop for Session {
    /// Best-effort [`Session::shutdown`].
    fn drop(&mut self) {
        self.teardown();
    }
}

/// Pump child output into the terminal until EOF/EIO, then report exit.
fn read_loop(
    mut output: File,
    mut replies: File,
    pty: Arc<Mutex<pty::Pty>>,
    term: Arc<Mutex<vt::Terminal>>,
    pending: Arc<AtomicBool>,
    events: Sender<Event>,
) {
    let mut buf = [0u8; 8192];
    loop {
        match output.read(&mut buf) {
            // EOF: every slave-side fd is closed (macOS reports this).
            Ok(0) => break,
            Ok(n) => apply_chunk(&buf[..n], &mut replies, &term, &pending, &events),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            // Linux reports EIO instead of EOF once the child is gone.
            Err(_) => break,
        }
    }
    let _ = events.send(Event::Exit(reap(&pty)));
}

/// Feed one chunk into the terminal and emit the resulting events.
fn apply_chunk(
    chunk: &[u8],
    replies: &mut File,
    term: &Mutex<vt::Terminal>,
    pending: &AtomicBool,
    events: &Sender<Event>,
) {
    let (reply, title, bell, clipboard) = {
        let mut term = term.lock().expect("terminal lock");
        term.feed(chunk);
        (
            term.take_output(),
            term.take_title_changed(),
            term.take_bell(),
            term.take_clipboard(),
        )
    };
    if !reply.is_empty() {
        // DSR/DA/query answers back to the child; ignore a dying pty.
        let _ = replies.write_all(&reply);
    }
    if let Some(title) = title {
        let _ = events.send(Event::TitleChanged(title));
    }
    if bell {
        let _ = events.send(Event::Bell);
    }
    if let Some(clip) = clipboard {
        let _ = events.send(Event::Clipboard {
            kind: clip.kind,
            data: clip.data,
        });
    }
    // Coalesce: queue a Wakeup only when none is pending; the embedder
    // re-arms via with_term/clear_wakeup.
    if !pending.swap(true, Ordering::SeqCst) {
        let _ = events.send(Event::Wakeup);
    }
}

/// Wait for the child without holding the pty lock while sleeping, so a
/// concurrent [`Session::shutdown`] can still take the lock to kill.
fn reap(pty: &Mutex<pty::Pty>) -> Option<i32> {
    loop {
        match pty.lock().expect("pty lock").try_wait() {
            Ok(Some(status)) => return status.code(),
            Ok(None) => {}
            Err(_) => return None,
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(all(test, unix))]
#[path = "../tests/session.rs"]
mod tests;
