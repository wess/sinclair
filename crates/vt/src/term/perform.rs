//! `vte::Perform` for the terminal state: print, C0, ESC, and delegation
//! to the CSI/OSC handlers.

use crate::charset;
use crate::mode::Modes;

use super::{csi, dcs, osc, Inner};

impl vte::Perform for Inner {
    fn print(&mut self, c: char) {
        let mapped = self.charsets.map(c);
        self.write_char(mapped);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x07 => self.bell = true,
            0x08 => self.cursor_left(1),
            0x09 => self.tab_forward(1),
            0x0a | 0x0b | 0x0c => self.linefeed(),
            0x0d => self.carriage_return(),
            0x0e => self.charsets.shifted = true,
            0x0f => self.charsets.shifted = false,
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        match (intermediates, byte) {
            ([], b'7') => self.save_cursor(),
            ([], b'8') => self.restore_cursor(),
            ([], b'D') => self.linefeed(),
            ([], b'E') => {
                self.carriage_return();
                self.linefeed();
            }
            ([], b'H') => {
                let col = self.screen().cursor.col;
                self.screen_mut().set_tab(col);
            }
            ([], b'M') => self.reverse_index(),
            ([], b'c') => self.full_reset(),
            ([], b'=') => self.modes.insert(Modes::APP_KEYPAD),
            ([], b'>') => self.modes.remove(Modes::APP_KEYPAD),
            ([b'('], f) => self.charsets.g0 = charset::from_final(f),
            ([b')'], f) => self.charsets.g1 = charset::from_final(f),
            ([b'#'], b'8') => self.screen_alignment_test(),
            _ => {}
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        csi::dispatch(self, params, intermediates, action);
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        osc::dispatch(self, params, bell_terminated);
    }

    fn hook(&mut self, _params: &vte::Params, intermediates: &[u8], _ignore: bool, action: char) {
        dcs::hook(self, intermediates, action);
    }

    fn put(&mut self, byte: u8) {
        dcs::put(self, byte);
    }

    fn unhook(&mut self) {
        dcs::unhook(self);
    }
}

#[cfg(test)]
#[path = "../../tests/term/perform.rs"]
mod tests;
