//! Live miniatures of a terminal screen.
//!
//! A preview paints the same grid at a size where no glyph would survive
//! shaping: every run of inked cells becomes one quad in its foreground
//! color, every run of colored background another. What it shows is the
//! *shape* of a session — where the prompt sits, how far a build got, whether
//! something is painting a full-screen UI — not its text.
//!
//! It deliberately shares nothing with [`crate::element::TerminalElement`]'s
//! snapshot path. That one consumes vt's damage and republishes the cell pixel
//! size to the pty, both of which belong to the pane actually being rendered:
//! a preview that went through it would eat the damage its own tab needs to
//! repaint with, and tell the program in it that the screen is three pixels
//! wide. This path only ever reads.

use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    fill, point, px, relative, size, App, Bounds, ContentMask, Element, ElementId, GlobalElementId,
    Hsla, InspectorElementId, IntoElement, LayoutId, Pixels, Point, Style, Window,
};
use terminal::Session;
use theme::Rgb;
use vt::CellFlags;

use crate::colors::{self, Colors};
use crate::metrics::CellSize;

/// Fraction of a preview cell's height that inked text fills. Text drawn to
/// the full cell closes the gaps between lines and the miniature turns into a
/// solid block; leaving air above and below is what keeps lines countable.
const INK_HEIGHT: f32 = 0.62;

/// Where that ink starts inside the cell, as a fraction of cell height.
const INK_TOP: f32 = 0.19;

/// Real text at this size is antialiased down to a fraction of its color.
/// Solid quads are not, so they are toned down to land in the same place.
const INK_ALPHA: f32 = 0.85;

/// A horizontal run of cells sharing one color, in cell units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Run {
    pub col: usize,
    pub len: usize,
    pub color: Rgb,
}

/// One visible row reduced to the runs a miniature can show.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MiniRow {
    /// Runs of non-default background color.
    pub bg: Vec<Run>,
    /// Runs of inked (non-blank, visible) cells, in their foreground color.
    pub ink: Vec<Run>,
}

/// A terminal's visible screen, reduced for drawing at thumbnail size.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mini {
    pub cols: usize,
    pub rows: Vec<MiniRow>,
    /// The cursor cell and its color, when the cursor is visible and the
    /// screen is not scrolled back.
    pub cursor: Option<(usize, usize, Rgb)>,
}

/// Reduce the visible screen to color runs. Runs under the terminal lock, so
/// it resolves every color here and leaves nothing for the caller to look up.
pub fn capture(term: &mut vt::Terminal, colors: &Colors) -> Mini {
    let cursor = (term.cursor_visible() && term.display_offset() == 0).then(|| {
        let (row, col) = term.cursor_pos();
        let color = term
            .cursor_color()
            .map(|(r, g, b)| Rgb::new(r, g, b))
            .unwrap_or(colors.cursor);
        (
            row,
            col,
            colors::enforce_contrast(color, colors.bg, colors::CURSOR_MIN_CONTRAST),
        )
    });
    // Copied out: the row loop below borrows the terminal mutably.
    let palette: [Option<(u8, u8, u8)>; 256] =
        std::array::from_fn(|i| term.palette_override(i as u8));
    let ovr = |i: u8| palette[i as usize];

    let cols = term.cols();
    let rows = (0..term.rows())
        .map(|row_i| {
            let mut out = MiniRow::default();
            for (col, cell) in term.visible_row(row_i).cells.iter().enumerate() {
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
                let width = if cell.is_wide() { 2 } else { 1 };
                if bg != colors.bg {
                    push(&mut out.bg, col, width, bg);
                }
                if flags.contains(CellFlags::INVISIBLE) || cell.ch == ' ' || cell.ch == '\0' {
                    continue;
                }
                push(&mut out.ink, col, width, fg);
            }
            out
        })
        .collect();
    Mini { cols, rows, cursor }
}

/// Extend the last run, or start a new one. Both runs are the same row's, so
/// adjacency is a column test.
fn push(runs: &mut Vec<Run>, col: usize, width: usize, color: Rgb) {
    match runs.last_mut() {
        Some(run) if run.col + run.len == col && run.color == color => run.len += width,
        _ => runs.push(Run {
            col,
            len: width,
            color,
        }),
    }
}

/// Where a grid of `cols` x `rows` lands inside `bounds`: the top-left of the
/// miniature and its cell box, scaled to fit while keeping the terminal's own
/// cell aspect (letterboxed, so a card is never lying about the shape of the
/// screen it shows).
pub(crate) fn fit(
    bounds: Bounds<Pixels>,
    cols: usize,
    rows: usize,
    cell: CellSize,
) -> (Point<Pixels>, f32, f32) {
    let (cols, rows) = (cols.max(1) as f32, rows.max(1) as f32);
    let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    let scale = (w / (cols * cell.width))
        .min(h / (rows * cell.height))
        .max(0.0);
    let (cw, ch) = (cell.width * scale, cell.height * scale);
    let origin = point(
        bounds.origin.x + px((w - cols * cw) / 2.0),
        bounds.origin.y + px((h - rows * ch) / 2.0),
    );
    (origin, cw, ch)
}

/// Place a captured screen for paint: background runs as full cells, ink as a
/// shorter bar inside the cell, then the cursor over both.
pub(crate) fn quads(
    mini: &Mini,
    bounds: Bounds<Pixels>,
    cell: CellSize,
    opacity: f32,
) -> Vec<(Bounds<Pixels>, Hsla)> {
    let (origin, cw, ch) = fit(bounds, mini.cols, mini.rows.len(), cell);
    if cw <= 0.0 || ch <= 0.0 {
        return Vec::new();
    }
    let ink_h = (ch * INK_HEIGHT).max(1.0);
    let ink_y = ch * INK_TOP;
    let mut out = Vec::new();
    for (row_i, row) in mini.rows.iter().enumerate() {
        let y = origin.y + px(ch * row_i as f32);
        for run in &row.bg {
            let mut color = colors::hsla(run.color);
            color.a *= opacity;
            out.push((
                Bounds::new(
                    point(origin.x + px(cw * run.col as f32), y),
                    size(px(cw * run.len as f32), px(ch)),
                ),
                color,
            ));
        }
        for run in &row.ink {
            let mut color = colors::hsla(run.color);
            color.a *= INK_ALPHA * opacity;
            out.push((
                Bounds::new(
                    point(origin.x + px(cw * run.col as f32), y + px(ink_y)),
                    size(px(cw * run.len as f32), px(ink_h)),
                ),
                color,
            ));
        }
    }
    if let Some((row, col, color)) = mini.cursor {
        if row < mini.rows.len() && col < mini.cols {
            let mut color = colors::hsla(color);
            color.a *= opacity;
            out.push((
                Bounds::new(
                    point(
                        origin.x + px(cw * col as f32),
                        origin.y + px(ch * row as f32),
                    ),
                    size(px(cw.max(1.0)), px(ch)),
                ),
                color,
            ));
        }
    }
    out
}

/// The width-to-height ratio of a session's screen, for sizing the card a
/// preview is drawn in so the miniature fills it.
pub fn aspect(session: &Session, cell: CellSize) -> f32 {
    let (cols, rows) = session.with_term(|term| (term.cols().max(1), term.rows().max(1)));
    (cols as f32 * cell.width) / (rows as f32 * cell.height)
}

/// A read-only miniature of a live terminal. Paints whatever the session's
/// screen holds at the time of the frame; never resizes it, never touches its
/// damage.
pub struct Preview {
    session: Arc<Session>,
    colors: Rc<Colors>,
    /// The session's real cell box. Only its aspect is used — the miniature
    /// scales itself to the space it is given.
    cell: CellSize,
    opacity: f32,
}

impl Preview {
    pub fn new(session: Arc<Session>, colors: Rc<Colors>, cell: CellSize) -> Self {
        Self {
            session,
            colors,
            cell,
            opacity: 1.0,
        }
    }

    /// Fade the whole miniature (an inactive card, a strip on its way in).
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }
}

impl Element for Preview {
    type RequestLayoutState = ();
    type PrepaintState = Vec<(Bounds<Pixels>, Hsla)>;

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
        _window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        let mini = self
            .session
            .with_term(|term| capture(term, self.colors.as_ref()));
        quads(&mini, bounds, self.cell, self.opacity)
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        quads: &mut Self::PrepaintState,
        window: &mut Window,
        _cx: &mut App,
    ) {
        let mut bg = colors::hsla(self.colors.bg);
        bg.a *= self.opacity;
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            window.paint_quad(fill(bounds, bg));
            for (quad, color) in quads.iter() {
                window.paint_quad(fill(*quad, *color));
            }
        });
    }
}

impl IntoElement for Preview {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

#[cfg(test)]
#[path = "../tests/preview.rs"]
mod tests;
