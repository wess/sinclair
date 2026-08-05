//! The custom gpui element that paints the terminal grid.
//!
//! Painting happens in three layers: per-cell background rects
//! (merged into horizontal runs), shaped text runs grouped by style, then
//! the cursor. Cell data is snapshotted quickly under the terminal lock;
//! shaping happens after the lock is released.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    fill, point, px, relative, size, App, Bounds, ContentMask, Corners, Element, ElementId,
    GlobalElementId, Hsla, InspectorElementId, IntoElement, LayoutId, Pixels, Point, RenderImage,
    ShapedLine, Style, TextAlign, Window,
};
use terminal::Session;

use crate::colors::{self, Colors};
use crate::metrics::{self, CellSize, Padding};
use crate::mouse::MouseState;
use crate::pointer::CopyHook;

#[cfg(test)]
use theme::Rgb;
#[cfg(test)]
use vt::CellFlags;

mod draw;
mod snapshot;

pub(crate) use snapshot::*;
pub use snapshot::{ImageCache, ImageCachePool, ImageCacheStats, RenderStats, SnapCache};

/// Wait for a resize gesture to settle before doing the primary screen's
/// potentially expensive full-history reflow.
const RESIZE_SETTLE: Duration = Duration::from_millis(80);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResizeRequest {
    cols: usize,
    rows: usize,
    cell_width: u16,
    cell_height: u16,
}

#[derive(Default)]
pub(crate) struct ResizeState {
    desired: Option<ResizeRequest>,
    generation: u64,
    armed: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum ResizeAction {
    None,
    Immediate,
    Arm,
}

impl ResizeState {
    fn request(&mut self, desired: ResizeRequest, current: (usize, usize)) -> ResizeAction {
        let first = self.desired.is_none();
        if self.desired != Some(desired) {
            self.desired = Some(desired);
            self.generation = self.generation.wrapping_add(1);
        }
        if current == (desired.cols, desired.rows) {
            return ResizeAction::None;
        }
        if first {
            return ResizeAction::Immediate;
        }
        if self.armed {
            ResizeAction::None
        } else {
            self.armed = true;
            ResizeAction::Arm
        }
    }

    fn settle(&mut self, generation: u64) -> Option<ResizeRequest> {
        if self.generation != generation {
            return None;
        }
        self.armed = false;
        self.desired
    }
}

/// How the cursor is drawn when the program leaves the power-on default
/// (DECSCUSR blinking block). Hosts map their configured style onto this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorShape {
    #[default]
    Block,
    Bar,
    Underline,
}

/// Active search overlay query, recomputed against the live buffer each
/// frame so highlights never go stale.
#[derive(Clone)]
pub struct SearchQuery {
    pub query: String,
    pub current: usize,
    /// Precomputed (and view-cached) matches, so the renderer never re-scans
    /// the buffer itself. Shared by `Rc`: cloning a query is O(1).
    pub matches: Rc<Vec<vt::Match>>,
}

pub struct TerminalElement {
    session: Arc<Session>,
    colors: Rc<Colors>,
    font: gpui::Font,
    font_size: Pixels,
    cell: CellSize,
    pad: Padding,
    cursor_default: CursorShape,
    mouse: Rc<RefCell<MouseState>>,
    copy_on_select: bool,
    /// What to do with text captured by copy-on-select; hosts can layer
    /// redaction or clipboard history on top of the plain clipboard write.
    copy: Rc<CopyHook>,
    smart_select: bool,
    middle_click_paste: bool,
    /// Whether this pane holds keyboard focus; an unfocused pane paints a
    /// hollow cursor, so the active pane is unambiguous in a split.
    focused: bool,
    search: Option<SearchQuery>,
    /// Autosuggestion ghost suffix drawn dimmed at the cursor, if any.
    ghost: Option<String>,
    /// GPU textures for decoded sixel images, keyed by placement id and shared
    /// with the view so they survive across frames.
    image_cache: Rc<RefCell<ImageCache>>,
    /// Previous frame's snapshot plus the inputs it was built from, shared
    /// with the view; an undamaged, input-identical frame reuses it.
    snap_cache: Rc<RefCell<SnapCache>>,
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
        cursor_default: CursorShape,
        mouse: Rc<RefCell<MouseState>>,
        copy_on_select: bool,
        copy: Rc<CopyHook>,
        smart_select: bool,
        middle_click_paste: bool,
        focused: bool,
        search: Option<SearchQuery>,
        ghost: Option<String>,
        image_cache: Rc<RefCell<ImageCache>>,
        snap_cache: Rc<RefCell<SnapCache>>,
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
            copy,
            smart_select,
            middle_click_paste,
            focused,
            search,
            ghost,
            image_cache,
            snap_cache,
        }
    }

    fn resize(&self, desired: ResizeRequest, current: (usize, usize), cx: &mut App) {
        let action = self
            .snap_cache
            .borrow_mut()
            .resize
            .request(desired, current);
        match action {
            ResizeAction::None => {}
            ResizeAction::Immediate => {
                let _ = self.session.resize_px(
                    desired.cols,
                    desired.rows,
                    desired.cell_width,
                    desired.cell_height,
                );
            }
            ResizeAction::Arm => {
                let session = self.session.clone();
                let cache = self.snap_cache.clone();
                let executor = cx.background_executor().clone();
                cx.spawn(async move |cx| loop {
                    let generation = cache.borrow().resize.generation;
                    executor.timer(RESIZE_SETTLE).await;
                    let request = cache.borrow_mut().resize.settle(generation);
                    let Some(request) = request else {
                        continue;
                    };
                    let current = session.with_term(|term| (term.cols(), term.rows()));
                    if current != (request.cols, request.rows) {
                        let _ = session.resize_px(
                            request.cols,
                            request.rows,
                            request.cell_width,
                            request.cell_height,
                        );
                    }
                    cx.update(|cx| cx.refresh_windows());
                    break;
                })
                .detach();
            }
        }
    }
}

pub struct Frame {
    bg_quads: Vec<(Bounds<Pixels>, Hsla)>,
    box_quads: Vec<(Bounds<Pixels>, Hsla)>,
    lines: Vec<(Point<Pixels>, ShapedLine)>,
    cursor: Option<CursorFrame>,
    /// Dimmed autosuggestion ghost text at the cursor.
    ghost: Option<(Point<Pixels>, ShapedLine)>,
    indicator: Option<Bounds<Pixels>>,
    /// Sixel images, as positioned pixel bounds plus their texture.
    images: Vec<(Bounds<Pixels>, Arc<RenderImage>)>,
    /// Grid size at prepaint, for pointer hit testing.
    grid: (usize, usize),
}

pub(crate) struct CursorFrame {
    bounds: Bounds<Pixels>,
    color: Hsla,
    glyph: Option<(Point<Pixels>, ShapedLine)>,
    /// Outline only (unfocused pane): the glyph beneath shows through.
    hollow: bool,
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
        cx: &mut App,
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

        let current = self.session.with_term(|term| (term.cols(), term.rows()));
        // Carry the cell box so TIOCSWINSZ reports real pixel sizes to
        // pixel-addressing programs (kitty graphics, sixel). The first layout
        // applies immediately; subsequent resize-drag churn is trailing-edge
        // debounced so scrollback is reflowed once at the settled width.
        self.resize(
            ResizeRequest {
                cols,
                rows,
                cell_width: self.cell.width.round() as u16,
                cell_height: self.cell.height.round() as u16,
            },
            current,
            cx,
        );

        let hover_link = self.mouse.borrow().hover_link;
        let snap = {
            let mut images = self.image_cache.borrow_mut();
            let mut cache = self.snap_cache.borrow_mut();
            self.session.with_term(|term| {
                snapshot_reuse(
                    term,
                    &mut cache,
                    &self.colors,
                    self.search.as_ref(),
                    self.cell,
                    &mut images,
                    hover_link,
                )
            })
        };

        let cell_w = px(self.cell.width);
        let cell_h = px(self.cell.height);

        let images = snap
            .images
            .iter()
            .filter_map(|img| {
                let row = img.line + snap.offset as isize;
                if row < 0 || row as usize >= rows {
                    return None;
                }
                let pos = point(
                    origin.x + cell_w * img.col as f32,
                    origin.y + cell_h * row as f32,
                );
                let bounds = Bounds::new(pos, size(px(img.width), px(img.height)));
                Some((bounds, img.image.clone()))
            })
            .collect();
        let bg_quads = bg_quads(&snap.rows, origin, self.cell);

        let (box_w, box_h) = (self.cell.width, self.cell.height);
        let box_quads = snap
            .rows
            .iter()
            .enumerate()
            .flat_map(|(row_i, row)| row.boxes.iter().map(move |b| (row_i, b)))
            .flat_map(|(row_i, b)| {
                let glyph = crate::boxdraw::rects(b.ch, box_w, box_h);
                let cell_origin = point(
                    origin.x + cell_w * b.col as f32,
                    origin.y + cell_h * row_i as f32,
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

        let lines = {
            let mut cache = self.snap_cache.borrow_mut();
            let font_size = f32::from(self.font_size).to_bits();
            let cell_width = self.cell.width.to_bits();
            if cache.shape_font.as_ref() != Some(&self.font)
                || cache.shape_font_size != font_size
                || cache.shape_cell_width != cell_width
            {
                cache.shape_font = Some(self.font.clone());
                cache.shape_font_size = font_size;
                cache.shape_cell_width = cell_width;
                cache.shaped_rows.clear();
            }
            // Re-key the cache by row-snapshot identity rather than viewport
            // index. A terminal scroll rotates most rows; preserving their
            // shaped lines avoids re-shaping the whole screen just because
            // each unchanged row moved up by one slot.
            let mut prior: HashMap<*const RowSnapshot, ShapedRow> =
                std::mem::take(&mut cache.shaped_rows)
                    .into_iter()
                    .flatten()
                    .map(|shaped| (Rc::as_ptr(&shaped.source), shaped))
                    .collect();
            cache.shaped_rows.resize_with(snap.rows.len(), || None);
            let mut shaped_rows = 0;
            for (row_i, row) in snap.rows.iter().enumerate() {
                if let Some(shaped) = prior.remove(&Rc::as_ptr(row)) {
                    cache.shaped_rows[row_i] = Some(shaped);
                    continue;
                }
                let lines = row
                    .spans
                    .iter()
                    .map(|span| {
                        let run = self.text_run(span);
                        window.text_system().shape_line(
                            span.text.clone().into(),
                            self.font_size,
                            &[run],
                            Some(cell_w),
                        )
                    })
                    .collect();
                cache.shaped_rows[row_i] = Some(ShapedRow {
                    source: row.clone(),
                    lines,
                });
                shaped_rows += 1;
            }
            cache.set_last_shaped_rows(shaped_rows);
            snap.rows
                .iter()
                .enumerate()
                .flat_map(|(row_i, row)| {
                    let shaped = cache.shaped_rows[row_i].as_ref().unwrap();
                    row.spans
                        .iter()
                        .zip(shaped.lines.iter())
                        .map(|(span, line)| {
                            let pos = point(
                                origin.x + cell_w * span.col as f32,
                                origin.y + cell_h * row_i as f32,
                            );
                            (pos, line.clone())
                        })
                        .collect::<Vec<_>>()
                })
                .collect()
        };

        // The focused pane draws a filled cursor; an unfocused pane keeps a
        // hollow outline so the cursor stays findable after a focus switch.
        let cursor = snap
            .cursor
            .as_ref()
            .filter(|c| c.row < rows && c.col < cols)
            .map(|c| self.cursor_frame(c, origin, window));

        // Ghost text: dimmed suggestion suffix starting at the cursor cell.
        let ghost = self.ghost.as_ref().filter(|g| !g.is_empty()).and_then(|g| {
            let c = snap
                .cursor
                .as_ref()
                .filter(|c| c.row < rows && c.col < cols)?;
            let mut color = colors::hsla(self.colors.fg);
            color.a *= 0.4;
            let run = gpui::TextRun {
                len: g.len(),
                font: self.font.clone(),
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let line =
                window
                    .text_system()
                    .shape_line(g.clone().into(), self.font_size, &[run], None);
            let pos = point(
                origin.x + cell_w * c.col as f32,
                origin.y + cell_h * c.row as f32,
            );
            Some((pos, line))
        });

        Frame {
            bg_quads,
            box_quads,
            lines,
            cursor,
            ghost,
            indicator: scroll_indicator(&bounds, rows, snap.offset, snap.scrollback),
            images,
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
            for (bounds, image) in &frame.images {
                window
                    .paint_image(*bounds, Corners::default(), image.clone(), 0, false)
                    .ok();
            }
            for (pos, line) in &frame.lines {
                line.paint(*pos, line_height, TextAlign::Left, None, window, cx)
                    .ok();
            }
            if let Some((pos, line)) = &frame.ghost {
                line.paint(*pos, line_height, TextAlign::Left, None, window, cx)
                    .ok();
            }
            if let Some(cursor) = &frame.cursor {
                if cursor.hollow {
                    window.paint_quad(gpui::quad(
                        cursor.bounds,
                        Corners::default(),
                        gpui::transparent_black(),
                        gpui::Edges::all(px(1.5)),
                        cursor.color,
                        gpui::BorderStyle::default(),
                    ));
                } else {
                    window.paint_quad(fill(cursor.bounds, cursor.color));
                    if let Some((pos, glyph)) = &cursor.glyph {
                        glyph
                            .paint(*pos, line_height, TextAlign::Left, None, window, cx)
                            .ok();
                    }
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

/// Place each row's background runs for paint.
///
/// A row's y is its slot in `rows` and nothing else — see [`RowSnapshot`] for
/// why a row cannot be trusted to know where it lives. The text and
/// box-drawing layers in [`TerminalElement::prepaint`] index the same way;
/// this one is split out because it is the layer that can be checked without
/// a window.
pub(crate) fn bg_quads(
    rows: &[Rc<RowSnapshot>],
    origin: Point<Pixels>,
    cell: CellSize,
) -> Vec<(Bounds<Pixels>, Hsla)> {
    let cell_w = px(cell.width);
    let cell_h = px(cell.height);
    rows.iter()
        .enumerate()
        .flat_map(|(row_i, row)| {
            row.bg_runs.iter().map(move |run| {
                let pos = point(
                    origin.x + cell_w * run.col as f32,
                    origin.y + cell_h * row_i as f32,
                );
                (
                    Bounds::new(pos, size(cell_w * run.len as f32, cell_h)),
                    colors::hsla(run.color),
                )
            })
        })
        .collect()
}

impl IntoElement for TerminalElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

#[cfg(test)]
#[path = "../../tests/element.rs"]
mod tests;
