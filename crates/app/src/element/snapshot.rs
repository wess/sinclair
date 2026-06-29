use super::*;

use theme::Rgb;
use vt::CellFlags;

/// Style bits that affect how a span of text is shaped/decorated. Spans
/// merge only when these (and the foreground color) match.
const STYLE_FLAGS: CellFlags = CellFlags::BOLD
    .union(CellFlags::DIM)
    .union(CellFlags::ITALIC)
    .union(CellFlags::ANY_UNDERLINE)
    .union(CellFlags::STRIKETHROUGH);

/// A decoded image positioned for drawing: its absolute content line, column,
/// pixel size, and shared GPU texture.
pub(crate) struct ImageDraw {
    pub(crate) line: isize,
    pub(crate) col: usize,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) image: Arc<RenderImage>,
}

/// Build a GPU texture from a decoded sixel image (RGBA -> the BGRA gpui wants).
fn render_image(img: &vt::Image) -> Arc<RenderImage> {
    let mut bgra = img.rgba.clone();
    for px in bgra.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    let buf = image::RgbaImage::from_raw(img.width as u32, img.height as u32, bgra)
        .unwrap_or_else(|| image::RgbaImage::new(1, 1));
    Arc::new(RenderImage::new(smallvec::SmallVec::from_buf([
        image::Frame::new(buf),
    ])))
}

/// A horizontal run of equal non-default background color, in cells.
#[derive(Debug, PartialEq)]
pub(crate) struct BgRun {
    pub(crate) row: usize,
    pub(crate) col: usize,
    pub(crate) len: usize,
    pub(crate) color: Rgb,
}

/// A contiguous run of glyphs sharing one style, ready for shaping.
#[derive(Debug, PartialEq)]
pub(crate) struct Span {
    pub(crate) row: usize,
    pub(crate) col: usize,
    pub(crate) text: String,
    /// Columns covered (wide characters cover two).
    pub(crate) width: usize,
    /// Set once a wide (2-column) glyph joins the span; such spans never take
    /// further cells, so forced cell-width shaping can't misplace glyphs.
    pub(crate) has_wide: bool,
    pub(crate) fg: Rgb,
    pub(crate) flags: CellFlags,
}

#[derive(Debug, PartialEq)]
pub(crate) struct CursorSnap {
    pub(crate) row: usize,
    pub(crate) col: usize,
    pub(crate) style: vt::CursorStyle,
    pub(crate) wide: bool,
    pub(crate) ch: char,
    pub(crate) color: Rgb,
    pub(crate) text_color: Rgb,
}

/// A cell rendered with custom box-drawing/block geometry instead of a
/// font glyph.
pub(crate) struct BoxCell {
    pub(crate) row: usize,
    pub(crate) col: usize,
    pub(crate) ch: char,
    pub(crate) fg: Rgb,
}

/// Everything render needs, captured under the terminal lock.
pub(crate) struct Snapshot {
    pub(crate) bg_runs: Vec<BgRun>,
    pub(crate) spans: Vec<Span>,
    pub(crate) boxes: Vec<BoxCell>,
    pub(crate) cursor: Option<CursorSnap>,
    /// Display offset and scrollback length, for the scrollback indicator.
    pub(crate) offset: usize,
    pub(crate) scrollback: usize,
    pub(crate) images: Vec<ImageDraw>,
}

/// Capture visible rows as background runs and styled spans. Resolves all
/// colors (theme palette + OSC 4 overrides + inverse + bold brightening +
/// selection) so nothing after this needs the lock.
pub(crate) fn snapshot(
    term: &mut vt::Terminal,
    colors: &Colors,
    search: Option<&SearchQuery>,
    cell: CellSize,
    image_cache: &mut HashMap<u64, Arc<RenderImage>>,
) -> Snapshot {
    term.set_cell_pixels(cell.width.round() as u16, cell.height.round() as u16);
    // TODO(perf): use the damage to clip painting; for now drain it so the
    // accumulator does not grow without bound.
    let _ = term.take_damage();

    let rows = term.rows();
    let mut bg_runs: Vec<BgRun> = Vec::with_capacity(rows);
    let mut spans: Vec<Span> = Vec::with_capacity(rows * 2);
    let mut boxes: Vec<BoxCell> = Vec::with_capacity(rows);
    let ovr = |i: u8| term.palette_override(i);
    let offset = term.display_offset();
    let selection = term.selection().copied();

    let scrollback_len = term.grid().scrollback().len();
    let mut search_map: std::collections::HashMap<usize, Vec<(usize, usize, bool)>> =
        std::collections::HashMap::new();
    if let Some(sq) = search.filter(|s| !s.query.is_empty()) {
        for (i, m) in sq.matches.iter().enumerate() {
            search_map
                .entry(m.line)
                .or_default()
                .push((m.start_col, m.end_col, i == sq.current));
        }
    }

    for (row_i, row) in term.visible_rows().enumerate() {
        for (col, cell) in row.cells.iter().enumerate() {
            if cell.is_wide_spacer() {
                continue;
            }
            let flags = cell.flags;
            let bold = flags.contains(CellFlags::BOLD);
            let mut fg = colors::cell_rgb(cell.fg, colors.fg, bold, &colors.palette, ovr);
            let mut bg = colors::cell_rgb(cell.bg, colors.bg, false, &colors.palette, ovr);
            if flags.contains(CellFlags::INVERSE) {
                std::mem::swap(&mut fg, &mut bg);
            }
            if selection
                .as_ref()
                .is_some_and(|sel| sel.contains(metrics::selection_point(row_i, col, offset)))
            {
                bg = colors.selection_bg;
                fg = colors.selection_fg;
            }
            if !search_map.is_empty() {
                let global = scrollback_len + row_i - offset.min(scrollback_len);
                if let Some(ranges) = search_map.get(&global) {
                    if let Some(&(_, _, current)) =
                        ranges.iter().find(|(s, e, _)| col >= *s && col <= *e)
                    {
                        if current {
                            bg = colors.selection_fg;
                            fg = colors.selection_bg;
                        } else {
                            bg = colors.selection_bg;
                            fg = colors.selection_fg;
                        }
                    }
                }
            }
            let width = if cell.is_wide() { 2 } else { 1 };

            if bg != colors.bg {
                match bg_runs.last_mut() {
                    Some(run)
                        if run.row == row_i && run.col + run.len == col && run.color == bg =>
                    {
                        run.len += width;
                    }
                    _ => bg_runs.push(BgRun {
                        row: row_i,
                        col,
                        len: width,
                        color: bg,
                    }),
                }
            }

            if flags.contains(CellFlags::INVISIBLE) {
                continue;
            }
            if crate::boxdraw::covers(cell.ch) {
                boxes.push(BoxCell {
                    row: row_i,
                    col,
                    ch: cell.ch,
                    fg,
                });
                continue;
            }
            let mut style = flags & STYLE_FLAGS;
            if cell.hyperlink.is_some() {
                style.insert(CellFlags::UNDERLINE);
            }
            if cell.ch == ' '
                && !style.intersects(CellFlags::ANY_UNDERLINE | CellFlags::STRIKETHROUGH)
            {
                continue;
            }

            match spans.last_mut() {
                Some(span)
                    if width == 1
                        && !span.has_wide
                        && span.row == row_i
                        && span.col + span.width == col
                        && span.fg == fg
                        && span.flags == style =>
                {
                    cell.write_grapheme(&mut span.text);
                    span.width += 1;
                }
                _ => {
                    let mut text = String::new();
                    cell.write_grapheme(&mut text);
                    spans.push(Span {
                        row: row_i,
                        col,
                        text,
                        width,
                        has_wide: width == 2,
                        fg,
                        flags: style,
                    });
                }
            }
        }
    }

    let cursor = (term.cursor_visible() && term.display_offset() == 0).then(|| {
        let (row, col) = term.cursor_pos();
        let cell = term.cell(row, col);
        CursorSnap {
            row,
            col,
            style: term.cursor_style(),
            wide: cell.is_wide(),
            ch: cell.ch,
            color: term
                .cursor_color()
                .map(|(r, g, b)| Rgb::new(r, g, b))
                .unwrap_or(colors.cursor),
            text_color: colors.cursor_text,
        }
    });

    let placements = term.images();
    let live: std::collections::HashSet<u64> = placements.iter().map(|p| p.id).collect();
    image_cache.retain(|id, _| live.contains(id));
    let images = placements
        .iter()
        .map(|p| {
            let image = image_cache
                .entry(p.id)
                .or_insert_with(|| render_image(&p.image))
                .clone();
            ImageDraw {
                line: p.line,
                col: p.col,
                width: p.image.width as f32,
                height: p.image.height as f32,
                image,
            }
        })
        .collect();

    Snapshot {
        bg_runs,
        spans,
        boxes,
        cursor,
        offset,
        scrollback: term.grid().scrollback().len(),
        images,
    }
}

/// Width of the scrollback indicator bar, in pixels.
pub(crate) const INDICATOR_WIDTH: f32 = 3.0;

/// Minimum indicator thumb height, in pixels.
pub(crate) const INDICATOR_MIN: f32 = 20.0;

/// A thin right-edge thumb showing where the viewport sits in history.
/// `None` at the live bottom (offset 0) so the indicator only appears
/// while scrolled back.
pub(crate) fn scroll_indicator(
    bounds: &Bounds<Pixels>,
    rows: usize,
    offset: usize,
    scrollback: usize,
) -> Option<Bounds<Pixels>> {
    if offset == 0 || scrollback == 0 {
        return None;
    }
    let height = f32::from(bounds.size.height);
    let total = (scrollback + rows) as f32;
    let thumb = (height * rows as f32 / total)
        .max(INDICATOR_MIN)
        .min(height);
    let above = (scrollback - offset) as f32 / scrollback as f32;
    let y = (height - thumb) * above;
    Some(Bounds::new(
        point(
            bounds.origin.x + bounds.size.width - px(INDICATOR_WIDTH),
            bounds.origin.y + px(y),
        ),
        size(px(INDICATOR_WIDTH), px(thumb)),
    ))
}

/// The cursor shape to draw: vt DECSCUSR wins, except that the power-on
/// default (blinking block) defers to the configured style.
pub(crate) fn cursor_shape(
    style: vt::CursorStyle,
    fallback: config::CursorStyle,
) -> config::CursorStyle {
    match style {
        vt::CursorStyle::BlinkingBlock => fallback,
        vt::CursorStyle::SteadyBlock => config::CursorStyle::Block,
        vt::CursorStyle::BlinkingUnderline | vt::CursorStyle::SteadyUnderline => {
            config::CursorStyle::Underline
        }
        vt::CursorStyle::BlinkingBar | vt::CursorStyle::SteadyBar => config::CursorStyle::Bar,
    }
}
