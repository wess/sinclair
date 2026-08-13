//! Sinclair's terminal, embeddable in other apps.
//!
//! The core is headless: [`Session`] runs a child process on a pty and feeds
//! a [`vt::Terminal`], and the modules here resolve what a frame should look
//! like ([`colors`], [`metrics`], [`mouse`], [`boxdraw`]) without touching a
//! GPU. The `ui` feature (on by default) adds the gpui layer on top:
//! [`element::TerminalElement`] paints the grid, and [`termview::TermView`]
//! is a drop-in pane — session, keyboard, mouse, scrollback, and selection
//! wired up — for any gpui application.
//!
//! Headless embedding (`default-features = false`):
//!
//! ```no_run
//! use libsinclair::{Session, SessionOptions};
//!
//! let (session, events) = Session::spawn(SessionOptions::default()).unwrap();
//! // on Event::Wakeup, read the grid under the terminal lock:
//! session.with_term(|term| term.visible_row(0).cells.len());
//! session.write(b"ls\r").unwrap();
//! # drop(events);
//! ```
//!
//! gpui embedding: build a [`termview::TermOptions`], create a
//! [`termview::TermView`] entity, and put it in your layout. See
//! `examples/embed.rs` for a complete window.

pub mod boxdraw;
pub mod colors;
pub mod metrics;
pub mod mouse;

#[cfg(feature = "ui")]
pub mod bridge;
#[cfg(feature = "ui")]
pub mod element;
#[cfg(feature = "ui")]
pub mod pointer;
#[cfg(feature = "ui")]
pub mod preview;
#[cfg(feature = "ui")]
pub mod termview;

// The full stack, so embedders need only this crate in their Cargo.toml.
pub use cast;
pub use input;
pub use pty;
pub use terminal;
pub use theme;
pub use vt;

pub use colors::Colors;
pub use metrics::{CellSize, Padding};
pub use terminal::{Event, Session, SessionOptions};

#[cfg(feature = "ui")]
pub use element::{CursorShape, SearchQuery, SnapCache, TerminalElement};
#[cfg(feature = "ui")]
pub use preview::Preview;
#[cfg(feature = "ui")]
pub use termview::{TermOptions, TermView};
