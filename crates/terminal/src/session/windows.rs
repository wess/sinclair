//! Windows session backend: blocking ConPTY pipe IO on a reader thread.

use std::fs::File;
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::{event_channel, Counters, EventSender, SessionStats};
use crate::event::Event;
use crate::options::SessionOptions;
use crate::EventReceiver;

/// A running child attached to a [`vt::Terminal`].
///
/// A reader thread pumps ConPTY output into the terminal and reports
/// [`Event`]s on the channel returned by [`Session::spawn`]. Dropping the
/// session (or calling [`Session::shutdown`]) kills the child, which closes
/// the console and unblocks the reader at EOF; the reader reaps the child
/// and sends [`Event::Exit`] before ending.
pub struct Session {
    pty: Arc<Mutex<pty::Pty>>,
    /// Clone of the console input pipe for user input; writes need no lock.
    writer: File,
    term: Arc<Mutex<vt::Terminal>>,
    /// `true` while an unconsumed [`Event::Wakeup`] sits in the channel.
    wakeup_pending: Arc<AtomicBool>,
    output_generation: Arc<AtomicU64>,
    stats: Arc<Counters>,
    /// Active asciinema recording; the reader thread writes output into it.
    recorder: Arc<Mutex<Option<cast::Recorder>>>,
    reader: Option<JoinHandle<()>>,
}

/// Everything the reader thread borrows while pumping output.
struct Reader {
    pty: Arc<Mutex<pty::Pty>>,
    term: Arc<Mutex<vt::Terminal>>,
    pending: Arc<AtomicBool>,
    generation: Arc<AtomicU64>,
    stats: Arc<Counters>,
    recorder: Arc<Mutex<Option<cast::Recorder>>>,
    events: EventSender,
    scrollback_limit: usize,
}

impl Session {
    /// Spawn the child on a ConPTY and start the reader thread.
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
        let output_generation = Arc::new(AtomicU64::new(0));
        let stats = Arc::new(Counters::default());
        let recorder: Arc<Mutex<Option<cast::Recorder>>> = Arc::new(Mutex::new(None));
        let (events, receiver) = event_channel(Arc::clone(&stats));

        let reader = thread::Builder::new()
            .name("ptyreader".to_string())
            .spawn({
                let reader = Reader {
                    pty: Arc::clone(&pty),
                    term: Arc::clone(&term),
                    pending: Arc::clone(&wakeup_pending),
                    generation: Arc::clone(&output_generation),
                    stats: Arc::clone(&stats),
                    recorder: Arc::clone(&recorder),
                    events,
                    scrollback_limit,
                };
                move || read_loop(output, replies, reader)
            })?;

        Ok((
            Session {
                pty,
                writer,
                term,
                wakeup_pending,
                output_generation,
                stats,
                recorder,
                reader: Some(reader),
            },
            receiver,
        ))
    }

    /// Send user input to the child. Loops until every byte is written.
    pub fn write(&self, bytes: &[u8]) -> io::Result<()> {
        (&self.writer).write_all(bytes)?;
        self.stats
            .input_bytes
            .fetch_add(bytes.len() as u64, Ordering::Relaxed);
        Ok(())
    }

    /// ConPTY writes directly rather than retaining a userspace queue.
    pub fn pending_input_bytes(&self) -> usize {
        0
    }

    /// Current resource/performance counters for this session.
    pub fn stats(&self) -> SessionStats {
        self.stats.snapshot(0)
    }

    /// Resize both the emulation grid and the pseudoconsole.
    pub fn resize(&self, cols: usize, rows: usize) -> io::Result<()> {
        self.resize_px(cols, rows, 0, 0)
    }

    /// [`Session::resize`] with per-cell pixel dimensions. ConPTY has no
    /// pixel reporting, so the cell sizes only feed the emulation grid's
    /// winsize bookkeeping.
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
        self.stats.resize_commits.fetch_add(1, Ordering::Relaxed);
        let size = pty::Winsize::with_cell_size(cols as u16, rows as u16, cell_width, cell_height);
        self.pty
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .resize(size)
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

    /// Whether a foreground process other than the shell is running on the
    /// console (e.g. an editor or a long-running command), for quit/close
    /// warnings.
    pub fn foreground_running(&self) -> bool {
        self.pty
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .foreground_running()
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
    /// should be scheduled immediately.
    pub fn acknowledge_wakeup(&self, observed_generation: u64) -> bool {
        self.stats
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

    /// Kill the child; closing the console unblocks the reader at EOF, so
    /// the join is bounded.
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

/// Pump child output into the terminal until EOF, then reap the child and
/// report its exit.
fn read_loop(mut output: File, mut replies: File, r: Reader) {
    let mut buf = [0u8; 65536];
    loop {
        match output.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if let Ok(mut rec) = r.recorder.lock() {
                    if let Some(rec) = rec.as_mut() {
                        let _ = rec.output(&buf[..n]);
                    }
                }
                apply_chunk(&buf[..n], &mut replies, &r);
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => break, // pipe gone once the console is fully closed
        }
    }
    let _ = r.events.send(Event::Exit(reap(&r.pty)));
}

/// Feed one chunk into the terminal and emit the resulting events. Replies
/// the terminal generates (DSR, DA, ...) are written straight back to the
/// console input pipe.
fn apply_chunk(chunk: &[u8], replies: &mut File, r: &Reader) {
    r.stats
        .output_bytes
        .fetch_add(chunk.len() as u64, Ordering::Relaxed);
    r.stats.output_chunks.fetch_add(1, Ordering::Relaxed);
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
        let _ = replies.write_all(&reply);
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
    r.generation.fetch_add(1, Ordering::Release);
    if !r.pending.swap(true, Ordering::SeqCst) {
        if r.events.send(Event::Wakeup).is_ok() {
            r.stats.wakeups.fetch_add(1, Ordering::Relaxed);
        } else {
            r.pending.store(false, Ordering::Release);
        }
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
