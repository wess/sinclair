//! Events delivered from the session reader thread to the embedder.

/// What a [`crate::Session`] reports on its event channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
  /// New child output was applied to the terminal; redraw when convenient.
  ///
  /// Coalesced: at most one unacknowledged `Wakeup` sits in the channel. The
  /// embedder acknowledges it explicitly after processing a generation via
  /// [`crate::Session::acknowledge_wakeup`].
  Wakeup,
  /// The child set a new window title (OSC 0/2 or title-stack pop).
  TitleChanged(String),
  /// BEL was received.
  Bell,
  /// The child requested a clipboard write via OSC 52. `kind` is the raw
  /// selection field (`c` = clipboard, `p` = primary); `data` is the
  /// already-decoded bytes to place on the clipboard.
  Clipboard { kind: String, data: Vec<u8> },
  /// The child made an OSC 5522 clipboard request (any MIME type, read or
  /// write). The embedder vets it and answers with the `vt::term::kclip`
  /// reply encoders written back through the session.
  ClipboardRequest(Box<vt::ClipboardRequest>),
  /// The child asked for the window to fit `rows` x `cols` of this terminal
  /// (`CSI 8 t`). Only sent when the terminal allows it; see
  /// `vt::Terminal::set_window_resize_allowed`.
  ResizeRequest { rows: u16, cols: u16 },
  /// The child requested a desktop notification (OSC 9 / 777 / 99).
  Notify { title: Option<String>, body: String },
  /// The child exited; carries the unix exit code when available
  /// (`None` when it was killed by a signal).
  Exit(Option<i32>),
  /// A shell-integration command finished (OSC 133 `D`). Carries the reported
  /// exit code (`None` when the mark omitted it). Needs shell integration.
  CommandFinished(Option<i32>),
  /// The working directory changed (OSC 7). Carries the new directory as the
  /// raw OSC 7 value (typically a `file://` URL).
  DirChanged(String),
}
