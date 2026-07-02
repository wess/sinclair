//! Pointer event glue: routes gpui mouse events to pty mouse reporting,
//! selection gestures, and display scrolling. Policy lives in
//! [`crate::mouse`]; this file only touches the session and the window.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    px, App, Bounds, ClipboardItem, Modifiers, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, ScrollDelta, ScrollWheelEvent, TouchPhase, Window,
};
use input::{MouseAction, MouseButton};
use terminal::Session;

use crate::metrics::{self, CellSize, Padding};
use crate::mouse::{self, MouseState, WheelRoute};

/// URL schemes Prompt will hand to the OS. OSC 8 lets a program aim a link
/// anywhere while showing innocuous text, so anything outside this set, most
/// dangerously `javascript:`, `data:`, and custom app-handler schemes, is
/// refused instead of being passed to `open_url`.
const OPENABLE_SCHEMES: &[&str] = &["http", "https", "ftp", "ftps", "file", "mailto", "tel"];

/// Whether `url` carries a scheme we are willing to open. A real scheme is
/// non-empty, holds no path separator (otherwise the `:` was inside a path),
/// and appears in [`OPENABLE_SCHEMES`].
fn openable(url: &str) -> bool {
    let Some((scheme, _)) = url.split_once(':') else {
        return false;
    };
    if scheme.is_empty() || scheme.contains('/') {
        return false;
    }
    OPENABLE_SCHEMES.contains(&scheme.to_ascii_lowercase().as_str())
}

/// Everything a pointer event needs, captured at paint time.
#[derive(Clone)]
pub struct Pointer {
    pub session: Arc<Session>,
    pub state: Rc<RefCell<MouseState>>,
    pub bounds: Bounds<Pixels>,
    pub pad: Padding,
    pub cell: CellSize,
    pub cols: usize,
    pub rows: usize,
    pub copy_on_select: bool,
    pub smart_select: bool,
    pub middle_click_paste: bool,
}

fn mods(m: &Modifiers) -> input::Mods {
    input::Mods {
        shift: m.shift,
        alt: m.alt,
        ctrl: m.control,
        cmd: m.platform,
    }
}

fn button(b: gpui::MouseButton) -> Option<MouseButton> {
    match b {
        gpui::MouseButton::Left => Some(MouseButton::Left),
        gpui::MouseButton::Middle => Some(MouseButton::Middle),
        gpui::MouseButton::Right => Some(MouseButton::Right),
        gpui::MouseButton::Navigate(_) => None,
    }
}

fn cell_at(p: &Pointer, pos: gpui::Point<Pixels>) -> (usize, usize) {
    metrics::cell_at(
        (f32::from(pos.x), f32::from(pos.y)),
        (f32::from(p.bounds.origin.x), f32::from(p.bounds.origin.y)),
        p.pad,
        p.cell,
        p.cols,
        p.rows,
    )
}

/// Send one encoded mouse report, 1-based coordinates.
fn report(
    p: &Pointer,
    action: MouseAction,
    btn: MouseButton,
    cell: (usize, usize),
    m: input::Mods,
    sgr: bool,
) {
    let (row, col) = cell;
    if let Some(bytes) = input::encode_mouse(action, btn, col as u32 + 1, row as u32 + 1, m, sgr) {
        let _ = p.session.write(&bytes);
    }
}

pub fn down(p: &Pointer, e: &MouseDownEvent, window: &mut Window, _cx: &mut App) {
    if !p.bounds.contains(&e.position) {
        return;
    }
    let m = mods(&e.modifiers);
    let (mode, sgr, offset) = p
        .session
        .with_term(|t| (t.mouse_mode(), t.mouse_sgr(), t.display_offset()));
    let cell = cell_at(p, e.position);

    if mouse::reports(mode, m.shift) {
        let Some(btn) = button(e.button) else { return };
        report(p, MouseAction::Press, btn, cell, m, sgr);
        let mut s = p.state.borrow_mut();
        s.report_button = Some(btn);
        s.last_motion = Some(cell);
        return;
    }

    // Middle-click paste (X-style): send the current selection to the pty.
    if e.button == gpui::MouseButton::Middle && p.middle_click_paste {
        let (text, bracketed) =
            p.session.with_term(|t| (t.selection_text(), t.bracketed_paste()));
        if let Some(text) = text.filter(|t| !t.is_empty()) {
            let _ = p.session.write(&input::encode_paste(&text, bracketed));
            window.refresh();
        }
        return;
    }

    if e.button != gpui::MouseButton::Left {
        return;
    }
    let select_mode = mouse::click_mode(e.click_count, p.smart_select);
    let point = metrics::selection_point(cell.0, cell.1, offset);
    p.session
        .with_term(|t| t.start_selection(select_mode, point));
    let mut s = p.state.borrow_mut();
    s.selecting = true;
    s.dragged = e.click_count > 1;
    s.pressed = Some(cell);
    drop(s);
    window.refresh();
}

pub fn moved(p: &Pointer, e: &MouseMoveEvent, window: &mut Window, _cx: &mut App) {
    let m = mods(&e.modifiers);

    if p.state.borrow().selecting && e.pressed_button == Some(gpui::MouseButton::Left) {
        let top = p.bounds.origin.y + px(p.pad.y);
        let bottom = p.bounds.origin.y + p.bounds.size.height - px(p.pad.y);
        let scroll: isize = if e.position.y < top {
            1
        } else if e.position.y > bottom {
            -1
        } else {
            0
        };
        let cell = cell_at(p, e.position);
        p.session.with_term(|t| {
            if scroll != 0 {
                t.scroll_display(scroll);
            }
            let point = metrics::selection_point(cell.0, cell.1, t.display_offset());
            t.update_selection(point);
        });
        let mut s = p.state.borrow_mut();
        if s.pressed != Some(cell) || scroll != 0 {
            s.dragged = true;
        }
        drop(s);
        window.refresh();
        return;
    }

    let (mode, sgr) = p.session.with_term(|t| (t.mouse_mode(), t.mouse_sgr()));
    if !mouse::reports(mode, m.shift) {
        return;
    }
    let held = p.state.borrow().report_button;
    if !mouse::reports_motion(mode, held) || !p.bounds.contains(&e.position) {
        return;
    }
    let cell = cell_at(p, e.position);
    if p.state.borrow().last_motion == Some(cell) {
        return;
    }
    report(
        p,
        MouseAction::Motion,
        held.unwrap_or(MouseButton::None),
        cell,
        m,
        sgr,
    );
    p.state.borrow_mut().last_motion = Some(cell);
}

pub fn up(p: &Pointer, e: &MouseUpEvent, window: &mut Window, cx: &mut App) {
    let m = mods(&e.modifiers);

    let held = p.state.borrow().report_button;
    if let Some(btn) = held {
        if button(e.button) == Some(btn) {
            let sgr = p.session.with_term(|t| t.mouse_sgr());
            report(p, MouseAction::Release, btn, cell_at(p, e.position), m, sgr);
            let mut s = p.state.borrow_mut();
            s.report_button = None;
            s.last_motion = None;
        }
        return;
    }

    if e.button == gpui::MouseButton::Left && m.cmd {
        let (row, col) = cell_at(p, e.position);
        let url = p.session.with_term(|t| {
            t.visible_row(row)
                .cells
                .get(col)
                .copied()
                .and_then(|c| t.cell_hyperlink(&c).map(str::to_string))
                .or_else(|| t.visible_url_at(row, col))
        });
        if let Some(url) = url {
            if openable(&url) {
                cx.open_url(&url);
            } else {
                eprintln!("prompt: refused to open link with disallowed scheme: {url}");
            }
            p.session.with_term(|t| t.clear_selection());
            let mut s = p.state.borrow_mut();
            s.selecting = false;
            s.pressed = None;
            s.dragged = false;
            drop(s);
            window.refresh();
            return;
        }
    }

    if e.button != gpui::MouseButton::Left || !p.state.borrow().selecting {
        return;
    }
    let dragged = {
        let mut s = p.state.borrow_mut();
        s.selecting = false;
        s.pressed = None;
        s.dragged
    };
    if !dragged {
        p.session.with_term(|t| t.clear_selection());
        window.refresh();
        return;
    }
    if p.copy_on_select {
        if let Some(text) = p.session.with_term(|t| t.selection_text()) {
            if !text.is_empty() {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
        }
    }
    window.refresh();
}

pub fn wheel(p: &Pointer, e: &ScrollWheelEvent, window: &mut Window, _cx: &mut App) {
    if !p.bounds.contains(&e.position) {
        return;
    }
    let m = mods(&e.modifiers);
    if matches!(e.touch_phase, TouchPhase::Started) {
        p.state.borrow_mut().wheel = 0.0;
    }
    let delta = match e.delta {
        ScrollDelta::Lines(l) => l.y,
        ScrollDelta::Pixels(d) => f32::from(d.y) / p.cell.height,
    };
    let lines = mouse::wheel_lines(&mut p.state.borrow_mut().wheel, delta);
    if lines == 0 {
        return;
    }

    let (mode, sgr, alt, alt_scroll, app_cursor) = p.session.with_term(|t| {
        (
            t.mouse_mode(),
            t.mouse_sgr(),
            t.is_alt_screen(),
            t.alternate_scroll(),
            t.cursor_keys_app(),
        )
    });
    match mouse::route_wheel(mode, m.shift, alt, alt_scroll) {
        WheelRoute::Report => {
            let cell = cell_at(p, e.position);
            let btn = if lines > 0 {
                MouseButton::WheelUp
            } else {
                MouseButton::WheelDown
            };
            for _ in 0..lines.unsigned_abs() {
                report(p, MouseAction::Press, btn, cell, m, sgr);
            }
        }
        WheelRoute::Arrows => {
            let bytes = input::encode_scroll_arrows(lines > 0, lines.unsigned_abs(), app_cursor);
            let _ = p.session.write(&bytes);
        }
        WheelRoute::Display => {
            p.session.with_term(|t| t.scroll_display(lines as isize));
            window.refresh();
        }
    }
}

#[cfg(test)]
#[path = "../tests/pointer.rs"]
mod tests;
