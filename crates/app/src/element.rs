//! The custom gpui element that paints the terminal grid.
//!
//! Painting happens in three layers, zed-style: per-cell background rects
//! (merged into horizontal runs), shaped text runs grouped by style, then
//! the cursor. Cell data is snapshotted quickly under the terminal lock;
//! shaping happens after the lock is released.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    fill, point, px, relative, size, App, Bounds, ContentMask, DispatchPhase, Element, ElementId,
    FontStyle, FontWeight, GlobalElementId, Hsla, InspectorElementId, IntoElement, LayoutId,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, ScrollWheelEvent, ShapedLine,
    StrikethroughStyle, Style, TextAlign, TextRun, UnderlineStyle, Window,
};
use terminal::Session;
use theme::Rgb;
use vt::CellFlags;

use crate::colors::{self, Colors};
use crate::metrics::{self, CellSize, Padding};
use crate::mouse::MouseState;
use crate::pointer::{self, Pointer};

/// Style bits that affect how a span of text is shaped/decorated. Spans
/// merge only when these (and the foreground color) match.
const STYLE_FLAGS: CellFlags = CellFlags::BOLD
    .union(CellFlags::DIM)
    .union(CellFlags::ITALIC)
    .union(CellFlags::ANY_UNDERLINE)
    .union(CellFlags::STRIKETHROUGH);

/// Active search overlay query, recomputed against the live buffer each
/// frame so highlights never go stale.
#[derive(Clone)]
pub struct SearchQuery {
    pub query: String,
    pub current: usize,
}

pub struct TerminalElement {
    session: Arc<Session>,
    colors: Rc<Colors>,
    font: gpui::Font,
    font_size: Pixels,
    cell: CellSize,
    pad: Padding,
    cursor_default: config::CursorStyle,
    mouse: Rc<RefCell<MouseState>>,
    copy_on_select: bool,
    search: Option<SearchQuery>,
}

impl TerminalElement {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session: Arc<Session>,
        colors: Rc<Colors>,
        font: gpui::Font,
        font_size: Pixels,
        cell: CellSize,
        pad: Padding,
        cursor_default: config::CursorStyle,
        mouse: Rc<RefCell<MouseState>>,
        copy_on_select: bool,
        search: Option<SearchQuery>,
    ) -> Self {
        Self {
            session,
            colors,
            font,
            font_size,
            cell,
            pad,
            cursor_default,
            mouse,
            copy_on_select,
            search,
        }
    }
}

/// A horizontal run of equal non-default background color, in cells.
#[derive(Debug, PartialEq)]
struct BgRun {
    row: usize,
    col: usize,
    len: usize,
    color: Rgb,
}

/// A contiguous run of glyphs sharing one style, ready for shaping.
#[derive(Debug, PartialEq)]
struct Span {
    row: usize,
    col: usize,
    text: String,
    /// Columns covered (wide characters cover two).
    width: usize,
    fg: Rgb,
    flags: CellFlags,
}

#[derive(Debug, PartialEq)]
struct CursorSnap {
    row: usize,
    col: usize,
    style: vt::CursorStyle,
    wide: bool,
    ch: char,
    color: Rgb,
    text_color: Rgb,
}

/// A cell rendered with custom box-drawing/block geometry instead of a
/// font glyph.
struct BoxCell {
    row: usize,
    col: usize,
    ch: char,
    fg: Rgb,
}

/// Everything render needs, captured under the terminal lock.
struct Snapshot {
    bg_runs: Vec<BgRun>,
    spans: Vec<Span>,
    boxes: Vec<BoxCell>,
    cursor: Option<CursorSnap>,
    /// Display offset and scrollback length, for the scrollback indicator.
    offset: usize,
    scrollback: usize,
}

/// Capture visible rows as background runs and styled spans. Resolves all
/// colors (theme palette + OSC 4 overrides + inverse + bold brightening +
/// selection) so nothing after this needs the lock.
fn snapshot(term: &mut vt::Terminal, colors: &Colors, search: Option<&SearchQuery>) -> Snapshot {
    // TODO(perf): use the damage to clip painting; for now drain it so the
    // accumulator does not grow without bound.
    let _ = term.take_damage();

    // Pre-size to the visible grid so a frame's worth of runs rarely
    // reallocates (one span per row is a good lower bound).
    let rows = term.rows();
    let mut bg_runs: Vec<BgRun> = Vec::with_capacity(rows);
    let mut spans: Vec<Span> = Vec::with_capacity(rows * 2);
    let mut boxes: Vec<BoxCell> = Vec::with_capacity(rows);
    let ovr = |i: u8| term.palette_override(i);
    let offset = term.display_offset();
    let selection = term.selection().copied();

    // Search matches, recomputed live against the buffer. Keyed by global
    // line; each entry is (start_col, end_col, is_current).
    let scrollback_len = term.grid().scrollback().len();
    let mut search_map: std::collections::HashMap<usize, Vec<(usize, usize, bool)>> =
        std::collections::HashMap::new();
    if let Some(sq) = search.filter(|s| !s.query.is_empty()) {
        let hits = term.search(&sq.query, false);
        for (i, m) in hits.iter().enumerate() {
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
            // Selection overrides both colors, after inverse.
            if selection
                .as_ref()
                .is_some_and(|sel| sel.contains(metrics::selection_point(row_i, col, offset)))
            {
                bg = colors.selection_bg;
                fg = colors.selection_fg;
            }
            // Search highlight wins over selection. The current match
            // inverts the selection colors so it stands out from the rest.
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
            // Box-drawing/block glyphs render with custom geometry so lines
            // join and blocks tile exactly; they bypass text shaping.
            if crate::boxdraw::covers(cell.ch) {
                boxes.push(BoxCell {
                    row: row_i,
                    col,
                    ch: cell.ch,
                    fg,
                });
                continue;
            }
            // OSC 8 hyperlinks render underlined so they are discoverable.
            let mut style = flags & STYLE_FLAGS;
            if cell.hyperlink.is_some() {
                style.insert(CellFlags::UNDERLINE);
            }
            // Skip plain spaces; they only matter for decorations.
            if cell.ch == ' '
                && !style.intersects(CellFlags::ANY_UNDERLINE | CellFlags::STRIKETHROUGH)
            {
                continue;
            }

            match spans.last_mut() {
                // Wide glyphs get their own span so forced cell-width
                // shaping cannot misplace the following glyphs.
                Some(span)
                    if width == 1
                        && span.width == span.text.chars().count()
                        && span.row == row_i
                        && span.col + span.width == col
                        && span.fg == fg
                        && span.flags == style =>
                {
                    span.text.push(cell.ch);
                    span.width += 1;
                }
                _ => spans.push(Span {
                    row: row_i,
                    col,
                    text: cell.ch.to_string(),
                    width,
                    fg,
                    flags: style,
                }),
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

    Snapshot {
        bg_runs,
        spans,
        boxes,
        cursor,
        offset,
        scrollback: term.grid().scrollback().len(),
    }
}

/// Width of the scrollback indicator bar, in pixels.
const INDICATOR_WIDTH: f32 = 3.0;

/// Minimum indicator thumb height, in pixels.
const INDICATOR_MIN: f32 = 20.0;

/// A thin right-edge thumb showing where the viewport sits in history.
/// `None` at the live bottom (offset 0) so the indicator only appears
/// while scrolled back.
fn scroll_indicator(
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
    // Fraction of history above the viewport: 0 at the very top.
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
fn cursor_shape(style: vt::CursorStyle, fallback: config::CursorStyle) -> config::CursorStyle {
    match style {
        vt::CursorStyle::BlinkingBlock => fallback,
        vt::CursorStyle::SteadyBlock => config::CursorStyle::Block,
        vt::CursorStyle::BlinkingUnderline | vt::CursorStyle::SteadyUnderline => {
            config::CursorStyle::Underline
        }
        vt::CursorStyle::BlinkingBar | vt::CursorStyle::SteadyBar => config::CursorStyle::Bar,
    }
}

pub struct Frame {
    bg_quads: Vec<(Bounds<Pixels>, Hsla)>,
    box_quads: Vec<(Bounds<Pixels>, Hsla)>,
    lines: Vec<(Point<Pixels>, ShapedLine)>,
    cursor: Option<CursorFrame>,
    indicator: Option<Bounds<Pixels>>,
    /// Grid size at prepaint, for pointer hit testing.
    grid: (usize, usize),
}

struct CursorFrame {
    bounds: Bounds<Pixels>,
    color: Hsla,
    glyph: Option<(Point<Pixels>, ShapedLine)>,
}

impl TerminalElement {
    fn text_run(&self, span: &Span) -> TextRun {
        let mut font = self.font.clone();
        if span.flags.contains(CellFlags::BOLD) {
            font.weight = FontWeight::BOLD;
        }
        if span.flags.contains(CellFlags::ITALIC) {
            font.style = FontStyle::Italic;
        }
        let mut color = colors::hsla(span.fg);
        if span.flags.contains(CellFlags::DIM) {
            color.a *= 0.7;
        }
        let underline = span
            .flags
            .intersects(CellFlags::ANY_UNDERLINE)
            .then(|| UnderlineStyle {
                thickness: px(1.0),
                color: Some(color),
                wavy: span.flags.contains(CellFlags::CURLY_UNDERLINE),
            });
        let strikethrough =
            span.flags
                .contains(CellFlags::STRIKETHROUGH)
                .then(|| StrikethroughStyle {
                    thickness: px(1.0),
                    color: Some(color),
                });
        TextRun {
            len: span.text.len(),
            font,
            color,
            background_color: None,
            underline,
            strikethrough,
        }
    }

    fn cursor_frame(
        &self,
        snap: &CursorSnap,
        origin: Point<Pixels>,
        window: &mut Window,
    ) -> CursorFrame {
        let cell_w = px(self.cell.width);
        let cell_h = px(self.cell.height);
        let cell_origin = point(
            origin.x + cell_w * snap.col as f32,
            origin.y + cell_h * snap.row as f32,
        );
        let width = if snap.wide { cell_w * 2.0 } else { cell_w };
        let color = colors::hsla(snap.color);
        match cursor_shape(snap.style, self.cursor_default) {
            config::CursorStyle::Bar => CursorFrame {
                bounds: Bounds::new(cell_origin, size(px(2.0), cell_h)),
                color,
                glyph: None,
            },
            config::CursorStyle::Underline => CursorFrame {
                bounds: Bounds::new(
                    point(cell_origin.x, cell_origin.y + cell_h - px(2.0)),
                    size(width, px(2.0)),
                ),
                color,
                glyph: None,
            },
            config::CursorStyle::Block => {
                // Re-shape the glyph under the cursor in the cursor text
                // color so it stays legible on the block.
                let glyph = (snap.ch != ' ').then(|| {
                    let text = snap.ch.to_string();
                    let run = TextRun {
                        len: text.len(),
                        font: self.font.clone(),
                        color: colors::hsla(snap.text_color),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    let line = window.text_system().shape_line(
                        text.into(),
                        self.font_size,
                        &[run],
                        Some(cell_w),
                    );
                    (cell_origin, line)
                });
                CursorFrame {
                    bounds: Bounds::new(cell_origin, size(width, cell_h)),
                    color,
                    glyph,
                }
            }
        }
    }

    /// Register this frame's mouse listeners. Down/wheel events are gated
    /// on the bounds; move/up follow drags anywhere in the window.
    fn register_pointer(&self, bounds: Bounds<Pixels>, grid: (usize, usize), window: &mut Window) {
        let pointer = Pointer {
            session: self.session.clone(),
            state: self.mouse.clone(),
            bounds,
            pad: self.pad,
            cell: self.cell,
            cols: grid.0,
            rows: grid.1,
            copy_on_select: self.copy_on_select,
        };
        window.on_mouse_event({
            let p = pointer.clone();
            move |e: &MouseDownEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    pointer::down(&p, e, window, cx);
                }
            }
        });
        window.on_mouse_event({
            let p = pointer.clone();
            move |e: &MouseMoveEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    pointer::moved(&p, e, window, cx);
                }
            }
        });
        window.on_mouse_event({
            let p = pointer.clone();
            move |e: &MouseUpEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    pointer::up(&p, e, window, cx);
                }
            }
        });
        window.on_mouse_event({
            move |e: &ScrollWheelEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    pointer::wheel(&pointer, e, window, cx);
                }
            }
        });
    }
}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = Frame;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = relative(1.0).into();
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        window: &mut Window,
        _cx: &mut App,
    ) -> Frame {
        let origin = point(
            bounds.origin.x + px(self.pad.x),
            bounds.origin.y + px(self.pad.y),
        );
        let (cols, rows) = metrics::grid_size(
            f32::from(bounds.size.width),
            f32::from(bounds.size.height),
            self.pad,
            self.cell,
        );

        // Resize only when the cell grid actually changed. Done outside
        // with_term because Session::resize takes the terminal lock itself.
        let current = self.session.with_term(|term| (term.cols(), term.rows()));
        if current != (cols, rows) {
            let _ = self.session.resize(cols, rows);
        }

        let snap = self
            .session
            .with_term(|term| snapshot(term, &self.colors, self.search.as_ref()));

        let cell_w = px(self.cell.width);
        let cell_h = px(self.cell.height);
        let bg_quads = snap
            .bg_runs
            .iter()
            .map(|run| {
                let pos = point(
                    origin.x + cell_w * run.col as f32,
                    origin.y + cell_h * run.row as f32,
                );
                (
                    Bounds::new(pos, size(cell_w * run.len as f32, cell_h)),
                    colors::hsla(run.color),
                )
            })
            .collect();

        let box_quads = snap
            .boxes
            .iter()
            .flat_map(|b| {
                let glyph = crate::boxdraw::rects(b.ch, self.cell.width, self.cell.height);
                let cell_origin = point(
                    origin.x + cell_w * b.col as f32,
                    origin.y + cell_h * b.row as f32,
                );
                let mut color = colors::hsla(b.fg);
                glyph
                    .map(move |g| {
                        color.a *= g.alpha;
                        g.rects.into_iter().map(move |(x, y, w, h)| {
                            (
                                Bounds::new(
                                    point(cell_origin.x + px(x), cell_origin.y + px(y)),
                                    size(px(w), px(h)),
                                ),
                                color,
                            )
                        })
                    })
                    .into_iter()
                    .flatten()
            })
            .collect();

        let lines = snap
            .spans
            .iter()
            .map(|span| {
                let run = self.text_run(span);
                let line = window.text_system().shape_line(
                    span.text.clone().into(),
                    self.font_size,
                    &[run],
                    Some(cell_w),
                );
                let pos = point(
                    origin.x + cell_w * span.col as f32,
                    origin.y + cell_h * span.row as f32,
                );
                (pos, line)
            })
            .collect();

        let cursor = snap
            .cursor
            .as_ref()
            .filter(|c| c.row < rows && c.col < cols)
            .map(|c| self.cursor_frame(c, origin, window));

        Frame {
            bg_quads,
            box_quads,
            lines,
            cursor,
            indicator: scroll_indicator(&bounds, rows, snap.offset, snap.scrollback),
            grid: (cols, rows),
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        frame: &mut Frame,
        window: &mut Window,
        cx: &mut App,
    ) {
        let line_height = px(self.cell.height);
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            window.paint_quad(fill(bounds, colors::hsla(self.colors.bg)));
            for (quad, color) in &frame.bg_quads {
                window.paint_quad(fill(*quad, *color));
            }
            for (quad, color) in &frame.box_quads {
                window.paint_quad(fill(*quad, *color));
            }
            for (pos, line) in &frame.lines {
                line.paint(*pos, line_height, TextAlign::Left, None, window, cx)
                    .ok();
            }
            if let Some(cursor) = &frame.cursor {
                window.paint_quad(fill(cursor.bounds, cursor.color));
                if let Some((pos, glyph)) = &cursor.glyph {
                    glyph
                        .paint(*pos, line_height, TextAlign::Left, None, window, cx)
                        .ok();
                }
            }
            if let Some(indicator) = frame.indicator {
                let mut color = colors::hsla(self.colors.fg);
                color.a = 0.4;
                window.paint_quad(fill(indicator, color));
            }
        });

        self.register_pointer(bounds, frame.grid, window);
    }
}

impl IntoElement for TerminalElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

#[cfg(test)]
#[path = "../tests/element.rs"]
mod tests;
