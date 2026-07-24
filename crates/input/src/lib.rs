//! Pure keystroke/paste -> pty-byte encoding: standard legacy
//! encoding plus the kitty keyboard protocol's disambiguation forms. No
//! deps, no IO.

mod csi;
mod encode;
mod kitty;
mod mouse;
mod paste;

pub use encode::encode_key;
pub use mouse::{encode_mouse, encode_scroll_arrows, MouseAction, MouseButton};
pub use paste::encode_paste;

/// Kitty keyboard enhancement flag bits. Mirrors `vt::kitty::flags`; kept
/// here so this crate stays dependency-free. The host reads the active
/// flags from the terminal and passes them in [`TermState::kitty_flags`].
pub mod kitty_flags {
    pub const DISAMBIGUATE: u8 = 0b0_0001;
    pub const REPORT_EVENT_TYPES: u8 = 0b0_0010;
    pub const REPORT_ALTERNATE_KEYS: u8 = 0b0_0100;
    pub const REPORT_ALL_KEYS_AS_ESCAPE_CODES: u8 = 0b0_1000;
    pub const REPORT_ASSOCIATED_TEXT: u8 = 0b1_0000;
}

/// Which phase of a keystroke this is. Only the kitty keyboard protocol's
/// `report_event_types` mode encodes it; legacy encoding treats `Repeat` like
/// `Press` and emits nothing for `Release`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum KeyEvent {
    #[default]
    Press,
    Repeat,
    Release,
}

/// Modifier keys held during a keystroke.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mods {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
    pub cmd: bool,
}

/// Terminal modes that affect key/paste encoding.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TermState {
    /// DECCKM: application cursor keys (SS3 arrows/home/end).
    pub cursor_keys_app: bool,
    /// DECKPAM: application keypad (reserved; keypad keys arrive as their
    /// plain names from gpui, so legacy encoding does not branch on this).
    pub keypad_app: bool,
    /// Whether the program requested bracketed paste (?2004).
    pub bracketed_paste: bool,
    /// Active kitty keyboard enhancement flags (0 = legacy encoding).
    pub kitty_flags: u8,
}
