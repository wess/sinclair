//! Terminal runtime: ties a pty session to the vt emulation core.
//!
//! [`Session::spawn`] starts a child process on a pty, feeds its output into
//! a [`vt::Terminal`] on a dedicated reader thread, and reports [`Event`]s
//! (wakeups, title changes, bell, exit) to the embedder over a channel.

pub mod event;
pub mod options;
pub mod session;

pub use event::Event;
pub use options::SessionOptions;
pub use session::{Session, SessionStats};

/// Event stream returned by [`Session::spawn`]. It supports both blocking
/// receives for headless embedders and async streaming for UI hosts.
pub type EventReceiver = flume::Receiver<Event>;
