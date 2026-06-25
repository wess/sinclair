//! Events delivered from the session reader thread to the embedder.

/// What a [`crate::Session`] reports on its event channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// New child output was applied to the terminal; redraw when convenient.
    ///
    /// Coalesced: at most one unconsumed `Wakeup` sits in the channel. The
    /// next one is sent only after the embedder re-arms the flag by locking
    /// the terminal ([`crate::Session::with_term`]) or calling
    /// [`crate::Session::clear_wakeup`].
    Wakeup,
    /// The child set a new window title (OSC 0/2 or title-stack pop).
    TitleChanged(String),
    /// BEL was received.
    Bell,
    /// The child requested a clipboard write via OSC 52. `kind` is the raw
    /// selection field (`c` = clipboard, `p` = primary); `data` is the
    /// already-decoded bytes to place on the clipboard.
    Clipboard { kind: String, data: Vec<u8> },
    /// The child requested a desktop notification (OSC 9 / 777 / 99).
    Notify {
        title: Option<String>,
        body: String,
    },
    /// The child exited; carries the unix exit code when available
    /// (`None` when it was killed by a signal).
    Exit(Option<i32>),
}
