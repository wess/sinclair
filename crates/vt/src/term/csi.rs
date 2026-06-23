//! CSI sequence dispatch.

use crate::cursor::CursorStyle;
use crate::mode::Modes;
use crate::sgr;

use super::Inner;

/// Handle a complete CSI sequence. Unknown sequences are ignored.
pub(crate) fn dispatch(
    inner: &mut Inner,
    params: &vte::Params,
    intermediates: &[u8],
    action: char,
) {
    let private = intermediates.contains(&b'?');
    // First subparameter of each parameter; enough for everything but SGR.
    let p: Vec<u16> = params
        .iter()
        .map(|s| s.first().copied().unwrap_or(0))
        .collect();

    match (action, intermediates) {
        ('A', []) => inner.cursor_up(count(&p, 0)),
        ('B', []) | ('e', []) => inner.cursor_down(count(&p, 0)),
        ('C', []) | ('a', []) => inner.cursor_right(count(&p, 0)),
        ('D', []) => inner.cursor_left(count(&p, 0)),
        ('E', []) => {
            inner.cursor_down(count(&p, 0));
            inner.carriage_return();
        }
        ('F', []) => {
            inner.cursor_up(count(&p, 0));
            inner.carriage_return();
        }
        ('G', []) | ('`', []) => inner.set_column(count(&p, 0) - 1),
        ('H', []) | ('f', []) => inner.cursor_to(count(&p, 0) - 1, count(&p, 1) - 1),
        ('I', []) => inner.tab_forward(count(&p, 0)),
        ('J', _) => inner.erase_display(arg(&p, 0, 0)),
        ('K', _) => inner.erase_line(arg(&p, 0, 0)),
        ('L', []) => inner.insert_lines(count(&p, 0)),
        ('M', []) => inner.delete_lines(count(&p, 0)),
        ('P', []) => inner.delete_chars(count(&p, 0)),
        ('@', []) => inner.insert_blank(count(&p, 0)),
        ('S', []) => inner.scroll_up_region(count(&p, 0)),
        ('T', []) => inner.scroll_down_region(count(&p, 0)),
        ('X', []) => inner.erase_chars(count(&p, 0)),
        ('Z', []) => inner.tab_backward(count(&p, 0)),
        ('b', []) => inner.repeat_last(count(&p, 0)),
        ('c', []) if arg(&p, 0, 0) == 0 => {
            // DA1: report VT220-class with sixel-less feature set.
            inner.output.extend_from_slice(b"\x1b[?62;22c");
        }
        ('c', [b'>']) => {
            // DA2: terminal type 0 ("VT100"), firmware version, ROM 0.
            inner.output.extend_from_slice(b"\x1b[>0;276;0c");
        }
        ('d', []) => inner.set_row(count(&p, 0) - 1),
        ('g', []) => match arg(&p, 0, 0) {
            0 => {
                let col = inner.screen().cursor.col;
                inner.screen_mut().clear_tab(col);
            }
            3 => inner.screen_mut().clear_all_tabs(),
            _ => {}
        },
        ('h', _) => set_modes(inner, &p, private, true),
        ('l', _) => set_modes(inner, &p, private, false),
        ('m', []) => {
            let mut pen = inner.screen().cursor.pen;
            sgr::apply(&mut pen, params.iter());
            inner.screen_mut().cursor.pen = pen;
        }
        ('n', []) => match arg(&p, 0, 0) {
            5 => inner.output.extend_from_slice(b"\x1b[0n"),
            6 => inner.report_cursor(),
            _ => {}
        },
        ('q', [b' ']) => {
            if let Some(style) = CursorStyle::from_decscusr(arg(&p, 0, 0)) {
                inner.cursor_style = style;
            }
        }
        ('r', []) => inner.set_scroll_region(arg(&p, 0, 0), arg(&p, 1, 0)),
        ('s', []) => inner.save_cursor(),
        ('u', []) => inner.restore_cursor(),
        // Kitty keyboard protocol negotiation.
        ('u', [b'?']) => {
            let flags = inner.screen().kitty.current();
            inner
                .output
                .extend_from_slice(format!("\x1b[?{flags}u").as_bytes());
        }
        ('u', [b'>']) => inner.screen_mut().kitty.push(arg(&p, 0, 0) as u8),
        ('u', [b'<']) => inner.screen_mut().kitty.pop(count(&p, 0)),
        ('u', [b'=']) => {
            let flags = arg(&p, 0, 0) as u8;
            let mode = arg(&p, 1, 1) as u8;
            inner.screen_mut().kitty.set(flags, mode);
        }
        ('t', []) => match arg(&p, 0, 0) {
            // XTWINOPS title stack.
            22 => inner.title_stack.push(inner.title.clone()),
            23 => {
                if let Some(title) = inner.title_stack.pop() {
                    inner.title = title;
                    inner.title_changed = true;
                }
            }
            _ => {}
        },
        _ => {}
    }
}

/// Parameter `i` with a default for missing-or-zero.
fn arg(p: &[u16], i: usize, default: u16) -> u16 {
    match p.get(i) {
        Some(&0) | None => default,
        Some(&v) => v,
    }
}

/// Count-style parameter: missing or 0 means 1.
fn count(p: &[u16], i: usize) -> usize {
    arg(p, i, 1) as usize
}

fn set_modes(inner: &mut Inner, p: &[u16], private: bool, enable: bool) {
    for &param in p {
        if private {
            set_private_mode(inner, param, enable);
        } else {
            set_ansi_mode(inner, param, enable);
        }
    }
}

fn set_ansi_mode(inner: &mut Inner, param: u16, enable: bool) {
    if param == 4 {
        inner.modes.set(Modes::INSERT, enable);
    }
}

fn set_private_mode(inner: &mut Inner, param: u16, enable: bool) {
    match param {
        1 => inner.modes.set(Modes::APP_CURSOR, enable),
        6 => {
            inner.modes.set(Modes::ORIGIN, enable);
            inner.cursor_to(0, 0);
        }
        7 => {
            inner.modes.set(Modes::AUTOWRAP, enable);
            if !enable {
                inner.screen_mut().cursor.pending_wrap = false;
            }
        }
        25 => inner.modes.set(Modes::CURSOR_VISIBLE, enable),
        47 => {
            if enable {
                inner.enter_alt(false);
            } else {
                inner.exit_alt();
            }
        }
        1000 => inner.modes.set(Modes::MOUSE_CLICK, enable),
        1002 => inner.modes.set(Modes::MOUSE_DRAG, enable),
        1003 => inner.modes.set(Modes::MOUSE_MOTION, enable),
        1004 => inner.modes.set(Modes::FOCUS_REPORT, enable),
        1006 => inner.modes.set(Modes::MOUSE_SGR, enable),
        1007 => inner.modes.set(Modes::ALT_SCROLL, enable),
        1047 => {
            if enable {
                inner.enter_alt(true);
            } else {
                if inner.modes.contains(Modes::ALT_SCREEN) {
                    inner.erase_display(2);
                }
                inner.exit_alt();
            }
        }
        1048 => {
            if enable {
                inner.save_cursor();
            } else {
                inner.restore_cursor();
            }
        }
        1049 => {
            if enable {
                inner.save_cursor();
                inner.enter_alt(true);
            } else {
                inner.exit_alt();
                inner.restore_cursor();
            }
        }
        2004 => inner.modes.set(Modes::BRACKETED_PASTE, enable),
        2026 => inner.modes.set(Modes::SYNC_OUTPUT, enable),
        _ => {}
    }
}

#[cfg(test)]
#[path = "../../tests/term/csi.rs"]
mod tests;
