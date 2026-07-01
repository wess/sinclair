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
    /// Active asciinema recording; the reader thread writes output into it.
    recorder: Arc<Mutex<Option<cast::Recorder>>>,
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
        let recorder: Arc<Mutex<Option<cast::Recorder>>> = Arc::new(Mutex::new(None));
        let (events, receiver) = mpsc::channel();

        let reader = thread::Builder::new()
            .name("ptyreader".to_string())
            .spawn({
                let pty = Arc::clone(&pty);
                let term = Arc::clone(&term);
                let pending = Arc::clone(&wakeup_pending);
                let recorder = Arc::clone(&recorder);
                move || read_loop(output, replies, pty, term, pending, recorder, events)
            })?;

        Ok((
            Session {
                pty,
                writer,
                term,
                wakeup_pending,
                recorder,
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
        self.term.lock().unwrap_or_else(|e| e.into_inner()).resize(cols, rows);
        let size = pty::Winsize::new(cols as u16, rows as u16);
        self.pty.lock().unwrap_or_else(|e| e.into_inner()).resize(size)
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
        self.pty
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .foreground_running()
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
            let _ = pty.kill();
        }
        let _ = handle.join();
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
    recorder: Arc<Mutex<Option<cast::Recorder>>>,
    events: Sender<Event>,
) {
    let mut buf = [0u8; 65536];
    loop {
        match output.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if let Ok(mut rec) = recorder.lock() {
                    if let Some(rec) = rec.as_mut() {
                        let _ = rec.output(&buf[..n]);
                    }
                }
                apply_chunk(&buf[..n], &mut replies, &term, &pending, &events);
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
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
    let (reply, title, bell, clipboard, notification, command_finished, cwd_changed) = {
        let mut term = term.lock().unwrap_or_else(|e| e.into_inner());
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| term.feed(chunk)));
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
    if let Some(note) = notification {
        let _ = events.send(Event::Notify {
            title: note.title,
            body: note.body,
        });
    }
    if let Some(code) = command_finished {
        let _ = events.send(Event::CommandFinished(code));
    }
    if let Some(dir) = cwd_changed {
        let _ = events.send(Event::DirChanged(dir));
    }
    if !pending.swap(true, Ordering::SeqCst) {
        let _ = events.send(Event::Wakeup);
    }
}

/// Wait for the child without holding the pty lock while sleeping, so a
/// concurrent [`Session::shutdown`] can still take the lock to kill.
fn reap(pty: &Mutex<pty::Pty>) -> Option<i32> {
    loop {
        match pty.lock().unwrap_or_else(|e| e.into_inner()).try_wait() {
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
