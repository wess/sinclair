//! OSC sequence dispatch: titles, palette overrides, cwd, cursor color,
//! dynamic-color queries (OSC 4/10/11/12) and clipboard (OSC 52).

use super::report::{base64_decode, format_rgb, Clipboard};
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
        // Window title (0 also sets the icon name; we only track the title).
        0 | 2 => {
            inner.title = rejoin(&params[1..]);
            inner.title_changed = true;
        }
        // Palette entries: 4;index;spec-or-? [;index;spec-or-? ...].
        // `?` queries; anything else sets.
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
                    // Recolors any cell using the index: full damage.
                    inner.full_damage = true;
                }
            }
        }
        // Working directory, as a file:// URL or plain path.
        7 => {
            let s = rejoin(&params[1..]);
            inner.cwd = (!s.is_empty()).then_some(s);
        }
        // Hyperlink: `8 ; params ; URI`. An empty URI closes the link.
        // The URI may contain `;`, so rejoin everything past the params.
        8 => {
            let uri = rejoin(&params[2..]);
            let hid = if uri.is_empty() {
                None
            } else {
                let id = link_id_param(params.get(1));
                inner.hyperlinks.intern(id, uri)
            };
            inner.screen_mut().cursor.pen.hyperlink = hid;
        }
        // Shell integration semantic prompts (FinalTerm/OSC 133). `A`
        // marks the start of a prompt; that row becomes a jump target.
        // `B`/`C`/`D` (command start / output / end) are accepted but not
        // yet acted on.
        133 => {
            if params.get(1).and_then(|p| p.first()) == Some(&b'A') {
                let row = inner.screen().cursor.row;
                inner.screen_mut().grid.row_mut(row).prompt = true;
            }
        }
        // Dynamic foreground / background: query only (the theme owns the
        // actual colors; dynamic set is not yet plumbed to the renderer).
        10 => dynamic_query(inner, params.get(1), bell_terminated, 10, report_fg),
        11 => dynamic_query(inner, params.get(1), bell_terminated, 11, report_bg),
        // Cursor color: set, or query (override beats the theme cursor).
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
        // Clipboard set (OSC 52). `52;<kind>;<base64>`; data `?` is a query
        // we cannot answer (no system clipboard read here), so it's ignored.
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
        // Reset palette entries (all when no indices given).
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
        // Reset cursor color.
        112 => {
            inner.cursor_color = None;
            inner.full_damage = true;
        }
        _ => {}
    }
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
