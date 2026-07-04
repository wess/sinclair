//! OSC sequence dispatch: titles, palette overrides, cwd, cursor color,
//! dynamic-color queries (OSC 4/10/11/12) and clipboard (OSC 52).

use super::report::{base64_decode, format_rgb, Clipboard, Notification};
use super::Inner;

/// Handle a complete OSC. `params` are the semicolon-split raw byte chunks
/// as provided by vte. `bell_terminated` says whether the sequence ended
/// with BEL (so replies echo the same terminator). Unknown commands are
/// ignored.
pub(crate) fn dispatch(inner: &mut Inner, params: &[&[u8]], bell_terminated: bool) {
    let Some(cmd) = params.first().and_then(|b| parse_number(b)) else {
        return;
    };
    match cmd {
        0 | 2 => {
            inner.title = sanitize_title(&rejoin(&params[1..]));
            inner.title_changed = true;
        }
        4 => {
            for pair in params[1..].chunks(2) {
                let [idx, spec] = pair else { continue };
                let Some(idx) = parse_number(idx).filter(|&i| i < 256) else {
                    continue;
                };
                if spec == b"?" {
                    if let Some(rgb) = palette_color(inner, idx as u8) {
                        reply(
                            inner,
                            bell_terminated,
                            &format!("4;{idx};{}", format_rgb(rgb)),
                        );
                    }
                } else if let Some(rgb) = parse_color_spec(&String::from_utf8_lossy(spec)) {
                    inner.palette[idx as usize] = Some(rgb);
                    inner.full_damage = true;
                }
            }
        }
        7 => {
            let s = rejoin(&params[1..]);
            let next = (!s.is_empty()).then_some(s);
            if next != inner.cwd {
                inner.cwd = next;
                inner.cwd_changed = true;
            }
        }
        8 => {
            let uri = rejoin(params.get(2..).unwrap_or(&[]));
            let hid = if uri.is_empty() {
                None
            } else {
                let id = link_id_param(params.get(1));
                inner.hyperlinks.intern(id, uri)
            };
            inner.screen_mut().cursor.pen.hyperlink = hid;
        }
        133 => match params.get(1).and_then(|p| p.first()) {
            // Prompt start: mark the row for jump-to-prompt.
            Some(&b'A') => {
                let row = inner.screen().cursor.row;
                inner.screen_mut().grid.row_mut(row).prompt = true;
            }
            // Command-line start: the cursor now sits where shell input begins.
            Some(&b'B') => {
                let c = &inner.screen().cursor;
                inner.input_start = Some((c.row, c.col));
            }
            // Command start (about to run): capture the typed line into history.
            Some(&b'C') => {
                if let Some((row, col)) = inner.input_start.take() {
                    let line = row_text_from(inner, row, col);
                    let line = line.trim();
                    if !line.is_empty() && inner.history.back().map(String::as_str) != Some(line) {
                        inner.history.push_back(line.to_string());
                        while inner.history.len() > 1000 {
                            inner.history.pop_front();
                        }
                    }
                }
            }
            // Command finished: `133;D` or `133;D;<exit-code>`.
            Some(&b'D') => {
                let code = params
                    .get(2)
                    .and_then(|p| std::str::from_utf8(p).ok())
                    .and_then(|s| s.trim().parse::<i32>().ok());
                inner.command_finished = Some(code);
                inner.input_start = None;
            }
            _ => {}
        },
        10 => dynamic_query(inner, params.get(1), bell_terminated, 10, report_fg),
        11 => dynamic_query(inner, params.get(1), bell_terminated, 11, report_bg),
        12 => {
            if params.get(1) == Some(&b"?".as_slice()) {
                if let Some(rgb) = inner.cursor_color.or_else(|| report_cursor(inner)) {
                    reply(inner, bell_terminated, &format!("12;{}", format_rgb(rgb)));
                }
            } else if let Some(spec) = params.get(1) {
                if let Some(rgb) = parse_color_spec(&String::from_utf8_lossy(spec)) {
                    inner.cursor_color = Some(rgb);
                    inner.full_damage = true;
                }
            }
        }
        52 => {
            let kind = params
                .get(1)
                .map(|b| String::from_utf8_lossy(b).into_owned());
            let data = params.get(2);
            if let (Some(kind), Some(data)) = (kind, data) {
                if data != b"?" {
                    if let Some(decoded) = base64_decode(data) {
                        let kind = if kind.is_empty() {
                            "c".to_string()
                        } else {
                            kind
                        };
                        inner.clipboard = Some(Clipboard {
                            kind,
                            data: decoded,
                        });
                    }
                }
            }
        }
        104 => {
            if params.len() <= 1 {
                inner.palette = [None; 256];
            } else {
                for idx in &params[1..] {
                    if let Some(idx) = parse_number(idx).filter(|&i| i < 256) {
                        inner.palette[idx as usize] = None;
                    }
                }
            }
            inner.full_damage = true;
        }
        112 => {
            inner.cursor_color = None;
            inner.full_damage = true;
        }
        9 => {
            let conemu = params.len() > 2
                && params
                    .get(1)
                    .is_some_and(|p| p.len() == 1 && p[0].is_ascii_digit());
            if !conemu {
                notify(inner, None, rejoin(&params[1..]));
            }
        }
        777 if params.get(1) == Some(&b"notify".as_slice()) => {
            let title = params
                .get(2)
                .map(|b| String::from_utf8_lossy(b).into_owned())
                .filter(|t| !t.is_empty());
            notify(inner, title, rejoin(params.get(3..).unwrap_or(&[])));
        }
        99 => {
            notify(inner, None, rejoin(params.get(2..).unwrap_or(&[])));
        }
        _ => {}
    }
}

/// Set the pending desktop notification (last write wins). Empty requests are
/// dropped so a bare sequence doesn't post a blank notification.
/// Text of active-grid row `row` from column `col` onward (right-trimmed).
/// Empty when the row is out of range.
fn row_text_from(inner: &Inner, row: usize, col: usize) -> String {
    let grid = &inner.screen().grid;
    if row >= grid.rows() {
        return String::new();
    }
    grid.row(row).text().chars().skip(col).collect::<String>().trim_end().to_string()
}

fn notify(inner: &mut Inner, title: Option<String>, body: String) {
    if body.is_empty() && title.is_none() {
        return;
    }
    inner.notification = Some(Notification { title, body });
}

/// Answer a `?` query for a single dynamic color (OSC 10/11), echoing the
/// command number. A non-`?` payload (a set) is ignored for now.
fn dynamic_query(
    inner: &mut Inner,
    arg: Option<&&[u8]>,
    bell_terminated: bool,
    cmd: u16,
    pick: fn(&Inner) -> Option<(u8, u8, u8)>,
) {
    if arg == Some(&b"?".as_slice()) {
        if let Some(rgb) = pick(inner) {
            reply(
                inner,
                bell_terminated,
                &format!("{cmd};{}", format_rgb(rgb)),
            );
        }
    }
}

/// Queue an OSC reply: `ESC ] <body> <terminator>`, where the terminator
/// matches the request (BEL or ST).
fn reply(inner: &mut Inner, bell_terminated: bool, body: &str) {
    inner.output.extend_from_slice(b"\x1b]");
    inner.output.extend_from_slice(body.as_bytes());
    inner
        .output
        .extend_from_slice(if bell_terminated { b"\x07" } else { b"\x1b\\" });
}

/// The reportable color for a palette index: an OSC 4 override wins,
/// otherwise the host-installed report palette (if any).
fn palette_color(inner: &Inner, index: u8) -> Option<(u8, u8, u8)> {
    inner.palette[index as usize].or_else(|| {
        inner
            .report_colors
            .as_ref()
            .map(|c| c.palette[index as usize])
    })
}

fn report_fg(inner: &Inner) -> Option<(u8, u8, u8)> {
    inner.report_colors.as_ref().map(|c| c.foreground)
}

fn report_bg(inner: &Inner) -> Option<(u8, u8, u8)> {
    inner.report_colors.as_ref().map(|c| c.background)
}

fn report_cursor(inner: &Inner) -> Option<(u8, u8, u8)> {
    inner.report_colors.as_ref().map(|c| c.cursor)
}

/// Extract the `id=` value from an OSC 8 params field (colon-separated
/// `key=value` pairs). Returns `None` when absent or empty.
fn link_id_param(field: Option<&&[u8]>) -> Option<String> {
    let field = String::from_utf8_lossy(field?);
    field.split(':').find_map(|kv| {
        let value = kv.strip_prefix("id=")?;
        (!value.is_empty()).then(|| value.to_string())
    })
}

/// Drop control characters from a window title before it reaches the host UI,
/// so a program can't smuggle newlines or escape bytes into the title bar.
fn sanitize_title(title: &str) -> String {
    title.chars().filter(|c| !c.is_control()).collect()
}

/// Rebuild a value that vte split on `;` (titles may legitimately contain it).
fn rejoin(params: &[&[u8]]) -> String {
    params
        .iter()
        .map(|b| String::from_utf8_lossy(b))
        .collect::<Vec<_>>()
        .join(";")
}

fn parse_number(bytes: &[u8]) -> Option<u16> {
    if bytes.is_empty() || bytes.len() > 5 {
        return None;
    }
    let mut n: u32 = 0;
    for &b in bytes {
        if !b.is_ascii_digit() {
            return None;
        }
        n = n * 10 + (b - b'0') as u32;
    }
    u16::try_from(n).ok()
}

/// Parse an X11-style color spec: `rgb:RR/GG/BB` (1-4 hex digits per
/// component) or `#RGB` / `#RRGGBB` / `#RRRRGGGGBBBB`.
pub(crate) fn parse_color_spec(spec: &str) -> Option<(u8, u8, u8)> {
    if let Some(rest) = spec.strip_prefix("rgb:") {
        let mut it = rest.split('/');
        let r = component(it.next()?)?;
        let g = component(it.next()?)?;
        let b = component(it.next()?)?;
        if it.next().is_some() {
            return None;
        }
        return Some((r, g, b));
    }
    if let Some(hex) = spec.strip_prefix('#') {
        if !hex.is_ascii() {
            return None;
        }
        let per = match hex.len() {
            3 => 1,
            6 => 2,
            12 => 4,
            _ => return None,
        };
        let r = component(&hex[0..per])?;
        let g = component(&hex[per..2 * per])?;
        let b = component(&hex[2 * per..3 * per])?;
        return Some((r, g, b));
    }
    None
}

/// Scale a 1-4 digit hex component to 8 bits.
fn component(s: &str) -> Option<u8> {
    if s.is_empty() || s.len() > 4 {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    let max = 16u32.pow(s.len() as u32) - 1;
    Some(((v * 255 + max / 2) / max) as u8)
}

#[cfg(test)]
#[path = "../../tests/term/osc.rs"]
mod tests;
