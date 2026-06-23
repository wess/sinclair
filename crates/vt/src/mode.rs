//! Terminal mode flags (SM/RM and private DECSET/DECRST).

bitflags::bitflags! {
    /// Active terminal modes.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Modes: u32 {
        /// DECAWM (?7): wrap at the right margin. Default on.
        const AUTOWRAP        = 1 << 0;
        /// DECTCEM (?25): cursor visible. Default on.
        const CURSOR_VISIBLE  = 1 << 1;
        /// DECOM (?6): cursor addressing relative to the scroll region.
        const ORIGIN          = 1 << 2;
        /// IRM (4): printed characters shift existing cells right.
        const INSERT          = 1 << 3;
        /// Alternate screen active (?47/?1047/?1049).
        const ALT_SCREEN      = 1 << 4;
        /// Bracketed paste (?2004).
        const BRACKETED_PASTE = 1 << 5;
        /// DECCKM (?1): application cursor keys.
        const APP_CURSOR      = 1 << 6;
        /// DECKPAM/DECKPNM (ESC = / ESC >): application keypad.
        const APP_KEYPAD      = 1 << 7;
        /// X11 mouse click reporting (?1000). Stored for later phases.
        const MOUSE_CLICK     = 1 << 8;
        /// Mouse drag reporting (?1002).
        const MOUSE_DRAG      = 1 << 9;
        /// Mouse any-motion reporting (?1003).
        const MOUSE_MOTION    = 1 << 10;
        /// SGR mouse encoding (?1006).
        const MOUSE_SGR       = 1 << 11;
        /// Alternate scroll (?1007): wheel sends arrow keys on the alt
        /// screen. Default off, matching xterm.
        const ALT_SCROLL      = 1 << 12;
        /// Focus reporting (?1004): emit CSI I / CSI O on focus changes.
        const FOCUS_REPORT    = 1 << 13;
        /// Synchronized output (?2026): hold rendering until cleared so a
        /// frame is presented atomically.
        const SYNC_OUTPUT     = 1 << 14;
    }
}

/// The strongest mouse reporting mode currently enabled (motion implies
/// drag implies click in every emulator that matters).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseMode {
    /// No mouse reporting.
    None,
    /// Button press/release only (?1000).
    Click,
    /// Clicks plus motion while a button is held (?1002).
    Drag,
    /// All motion (?1003).
    Motion,
}

impl MouseMode {
    /// Derive the effective mouse mode from the mode flags.
    pub fn from_modes(modes: Modes) -> MouseMode {
        if modes.contains(Modes::MOUSE_MOTION) {
            MouseMode::Motion
        } else if modes.contains(Modes::MOUSE_DRAG) {
            MouseMode::Drag
        } else if modes.contains(Modes::MOUSE_CLICK) {
            MouseMode::Click
        } else {
            MouseMode::None
        }
    }
}

impl Default for Modes {
    fn default() -> Self {
        Modes::AUTOWRAP | Modes::CURSOR_VISIBLE
    }
}

#[cfg(test)]
#[path = "../tests/mode.rs"]
mod tests;
