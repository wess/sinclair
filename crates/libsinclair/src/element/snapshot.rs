use super::*;

use std::collections::{HashMap, HashSet};

use theme::Rgb;
use vt::CellFlags;

const PANE_IMAGE_CACHE_BYTES: usize = 128 * 1024 * 1024;
const DEFAULT_IMAGE_CACHE_BYTES: usize = 512 * 1024 * 1024;

struct CachedImage {
    image: Arc<RenderImage>,
    bytes: usize,
    used: u64,
}

/// Shared decoded-texture cache. Multiple panes can use one pool so their
/// combined GPU-facing image data has a single LRU budget.
pub struct ImageCachePool {
    entries: HashMap<(u64, u64), CachedImage>,
    bytes: usize,
    limit: usize,
    clock: u64,
    next_pane: u64,
    evictions: u64,
}

/// Current shared texture-cache footprint and lifetime eviction count.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImageCacheStats {
    pub bytes: usize,
    pub entries: usize,
    pub evictions: u64,
}

impl Default for ImageCachePool {
    fn default() -> Self {
        Self::new(DEFAULT_IMAGE_CACHE_BYTES)
    }
}

impl ImageCachePool {
    pub fn new(limit: usize) -> Self {
        Self {
            entries: HashMap::new(),
            bytes: 0,
            limit: limit.max(1),
            clock: 0,
            next_pane: 1,
            evictions: 0,
        }
    }

    pub fn stats(&self) -> ImageCacheStats {
        ImageCacheStats {
            bytes: self.bytes,
            entries: self.entries.len(),
            evictions: self.evictions,
        }
    }

    fn remove(&mut self, key: (u64, u64)) {
        if let Some(entry) = self.entries.remove(&key) {
            self.bytes = self.bytes.saturating_sub(entry.bytes);
            self.evictions = self.evictions.saturating_add(1);
        }
    }

    fn enforce(&mut self, pane: u64) {
        loop {
            let pane_bytes: usize = self
                .entries
                .iter()
                .filter(|((owner, _), _)| *owner == pane)
                .map(|(_, entry)| entry.bytes)
                .sum();
            if pane_bytes <= PANE_IMAGE_CACHE_BYTES {
                break;
            }
            let victim = self
                .entries
                .iter()
                .filter(|((owner, _), _)| *owner == pane)
                .min_by_key(|(_, entry)| entry.used)
                .map(|(key, _)| *key);
            let Some(victim) = victim else { break };
            self.remove(victim);
        }
        while self.bytes > self.limit {
            let victim = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.used)
                .map(|(key, _)| *key);
            let Some(victim) = victim else { break };
            self.remove(victim);
        }
    }
}

/// One pane's namespace within an [`ImageCachePool`].
pub struct ImageCache {
    pane: u64,
    pool: Rc<RefCell<ImageCachePool>>,
}

impl Default for ImageCache {
    fn default() -> Self {
        Self::new(Rc::new(RefCell::new(ImageCachePool::default())))
    }
}

impl ImageCache {
    pub fn new(pool: Rc<RefCell<ImageCachePool>>) -> Self {
        let pane = {
            let mut pool = pool.borrow_mut();
            let pane = pool.next_pane;
            pool.next_pane = pool.next_pane.wrapping_add(1).max(1);
            pane
        };
        Self { pane, pool }
    }

    pub fn stats(&self) -> ImageCacheStats {
        self.pool.borrow().stats()
    }

    fn retain(&mut self, live: &HashSet<u64>) {
        let victims: Vec<_> = self
            .pool
            .borrow()
            .entries
            .keys()
            .filter(|(pane, id)| *pane == self.pane && !live.contains(id))
            .copied()
            .collect();
        let mut pool = self.pool.borrow_mut();
        for victim in victims {
            pool.remove(victim);
        }
    }

    pub(super) fn texture(&mut self, id: u64, source: &vt::Image) -> Arc<RenderImage> {
        let key = (self.pane, id);
        {
            let mut pool = self.pool.borrow_mut();
            pool.clock = pool.clock.wrapping_add(1);
            let used = pool.clock;
            if let Some(entry) = pool.entries.get_mut(&key) {
                entry.used = used;
                return entry.image.clone();
            }
        }

        let image = render_image(source);
        let bytes = source.rgba.len();
        let mut pool = self.pool.borrow_mut();
        pool.clock = pool.clock.wrapping_add(1);
        let used = pool.clock;
        pool.bytes = pool.bytes.saturating_add(bytes);
        pool.entries.insert(
            key,
            CachedImage {
                image: image.clone(),
                bytes,
                used,
            },
        );
        pool.enforce(self.pane);
        image
    }
}

impl Drop for ImageCache {
    fn drop(&mut self) {
        let Ok(mut pool) = self.pool.try_borrow_mut() else {
            return;
        };
        let victims: Vec<_> = pool
            .entries
            .keys()
            .filter(|(pane, _)| *pane == self.pane)
            .copied()
            .collect();
        for victim in victims {
            pool.remove(victim);
        }
    }
}

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
    let mut bgra = img.rgba.to_vec();
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
///
/// Column-only: a run's row is the viewport slot of the [`RowSnapshot`]
/// holding it. See that type for why no row index is stored here.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BgRun {
    pub(crate) col: usize,
    pub(crate) len: usize,
    pub(crate) color: Rgb,
}

/// A contiguous run of glyphs sharing one style, ready for shaping.
///
/// Column-only, for the reason given on [`RowSnapshot`].
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Span {
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
///
/// Column-only, for the reason given on [`RowSnapshot`].
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BoxCell {
    pub(crate) col: usize,
    pub(crate) ch: char,
    pub(crate) fg: Rgb,
}

/// One visible row's resolved primitives. Reference counting lets partial
/// damage carry unchanged rows across frames without cloning their strings.
///
/// Deliberately holds no row index, and neither do the primitives inside it:
/// a row's position is its index in [`Snapshot::rows`], nothing else. A
/// scroll re-homes these objects to new viewport slots ([`Reuse::Scrolled`]),
/// so any row index stored in here would be stale the moment the screen
/// scrolls — the row would keep painting at the slot it was first built at
/// and collide with whatever legitimately owns that slot.
pub(crate) struct RowSnapshot {
    pub(crate) source_revision: u64,
    pub(crate) bg_runs: Vec<BgRun>,
    pub(crate) spans: Vec<Span>,
    pub(crate) boxes: Vec<BoxCell>,
}

/// Everything render needs, captured under the terminal lock.
pub(crate) struct Snapshot {
    pub(crate) rows: Vec<Rc<RowSnapshot>>,
    // Keep the direct unit-test surface without flattened duplicates in
    // production builds.
    #[cfg(test)]
    pub(crate) bg_runs: Vec<BgRun>,
    #[cfg(test)]
    pub(crate) spans: Vec<Span>,
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) boxes: Vec<BoxCell>,
    pub(crate) cursor: Option<CursorSnap>,
    /// Display offset and scrollback length, for the scrollback indicator.
    pub(crate) offset: usize,
    pub(crate) scrollback: usize,
    pub(crate) images: Vec<ImageDraw>,
}

/// The app-side inputs a [`Snapshot`] was built from, beyond grid content
/// (which vt's damage tracks). [`snapshot_reuse`] compares these — every one
/// of them changes what the snapshot resolves — before reusing a frame.
pub(crate) struct SnapKey {
    offset: usize,
    scrollback: usize,
    committed: u64,
    cols: usize,
    rows: usize,
    cell: CellSize,
    /// Theme identity: a config/theme reload swaps the view's `Rc<Colors>`,
    /// so pointer equality is exact.
    colors: Rc<Colors>,
    selection: Option<vt::Selection>,
    /// Query text, focused index, and match-list identity (the view caches
    /// matches behind an `Rc` and replaces it on rescan).
    search: Option<(String, usize, Rc<Vec<vt::Match>>)>,
    hover_link: Option<(usize, usize, usize)>,
    /// Image placement identity (id + anchor); decoded textures are cached
    /// separately by id.
    images: Vec<(u64, isize, usize)>,
}

/// Capture the current snapshot inputs. Runs under the terminal lock.
pub(crate) fn snapkey(
    term: &vt::Terminal,
    colors: &Rc<Colors>,
    search: Option<&SearchQuery>,
    cell: CellSize,
    hover_link: Option<(usize, usize, usize)>,
) -> SnapKey {
    SnapKey {
        offset: term.display_offset(),
        scrollback: term.grid().scrollback().len(),
        committed: term.committed_lines(),
        cols: term.cols(),
        rows: term.rows(),
        cell,
        colors: colors.clone(),
        selection: term.selection().copied(),
        search: search.map(|s| (s.query.clone(), s.current, s.matches.clone())),
        hover_link,
        images: term
            .images()
            .iter()
            .map(|p| (p.id, p.line, p.col))
            .collect(),
    }
}

/// Whether two input keys are equivalent for snapshot reuse.
pub(crate) fn keyeq(a: &SnapKey, b: &SnapKey) -> bool {
    a.offset == b.offset
        && a.scrollback == b.scrollback
        && a.committed == b.committed
        && a.cols == b.cols
        && a.rows == b.rows
        && a.cell == b.cell
        && Rc::ptr_eq(&a.colors, &b.colors)
        && a.selection == b.selection
        && a.hover_link == b.hover_link
        && a.images == b.images
        && match (&a.search, &b.search) {
            (None, None) => true,
            (Some((qa, ca, ma)), Some((qb, cb, mb))) => qa == qb && ca == cb && Rc::ptr_eq(ma, mb),
            _ => false,
        }
}

/// The previous frame's snapshot and the inputs it was built from.
#[derive(Default)]
pub struct SnapCache {
    snap: Option<Rc<Snapshot>>,
    key: Option<SnapKey>,
    last_snapshot_rows: usize,
    pub(crate) shape_font: Option<gpui::Font>,
    pub(crate) shape_font_size: u32,
    pub(crate) shape_cell_width: u32,
    pub(crate) shaped_rows: Vec<Option<ShapedRow>>,
    last_shaped_rows: usize,
    pub(super) resize: super::ResizeState,
    stats: RenderStats,
}

/// Monotonic renderer work counters, useful for profiling and regression tests.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderStats {
    pub frames: u64,
    pub snapshot_reuses: u64,
    pub snapshot_rows: u64,
    pub shaped_rows: u64,
}

pub(crate) struct ShapedRow {
    pub(crate) source: Rc<RowSnapshot>,
    /// Refcounted, not owned: a `ShapedLine` is ~3KB (its `decoration_runs`
    /// SmallVec is inline), and every frame copies one per span into the
    /// frame's draw list. Sharing makes that a refcount bump instead of a
    /// multi-hundred-kilobyte memcpy on every frame, including idle ones.
    pub(crate) lines: Vec<Rc<ShapedLine>>,
}

/// Line up the shaped-line cache with this frame's rows, and report the slots
/// that still need shaping.
///
/// Rows are identified by the address of their [`RowSnapshot`], which is
/// stable while a `ShapedRow` holds it: a slot whose row object is unchanged
/// keeps its lines for one pointer compare, and only rows that actually moved
/// go through the identity map. That matters because the settled case is the
/// common one -- an idle frame reuses the entire snapshot, and partial damage
/// replaces only the rows that changed -- and it must not allocate. A scroll
/// is the case that does move every row, and re-homing them is what stops a
/// one-line scroll from re-shaping the whole screen.
pub(crate) fn realign_shaped_rows(
    cache: &mut Vec<Option<ShapedRow>>,
    rows: &[Rc<RowSnapshot>],
) -> Vec<usize> {
    cache.resize_with(rows.len(), || None);
    cache.truncate(rows.len());
    let mut misplaced: Vec<usize> = Vec::new();
    for (row_i, row) in rows.iter().enumerate() {
        let settled = cache[row_i]
            .as_ref()
            .is_some_and(|shaped| Rc::ptr_eq(&shaped.source, row));
        if !settled {
            misplaced.push(row_i);
        }
    }
    if !misplaced.is_empty() {
        let mut pool: HashMap<*const RowSnapshot, ShapedRow> =
            HashMap::with_capacity(misplaced.len());
        for &row_i in &misplaced {
            if let Some(shaped) = cache[row_i].take() {
                pool.insert(Rc::as_ptr(&shaped.source), shaped);
            }
        }
        for &row_i in &misplaced {
            if let Some(shaped) = pool.remove(&Rc::as_ptr(&rows[row_i])) {
                cache[row_i] = Some(shaped);
            }
        }
    }
    misplaced.retain(|&row_i| cache[row_i].is_none());
    misplaced
}

enum Reuse<'a> {
    /// Reuse rows by viewport index, rebuilding only explicit dirty rows.
    Dirty(&'a Snapshot, &'a [usize]),
    /// A full-screen scroll rotated row objects. Match their stable revisions
    /// so only newly exposed rows are resolved and shaped.
    Scrolled(&'a Snapshot),
}

impl SnapCache {
    pub fn stats(&self) -> RenderStats {
        self.stats
    }

    /// Terminal rows resolved during the most recent snapshot pass.
    pub fn last_snapshot_rows(&self) -> usize {
        self.last_snapshot_rows
    }

    /// Rows sent through the text shaper during the most recent prepaint.
    pub fn last_shaped_rows(&self) -> usize {
        self.last_shaped_rows
    }

    pub(crate) fn set_last_shaped_rows(&mut self, rows: usize) {
        self.last_shaped_rows = rows;
        self.stats.shaped_rows = self.stats.shaped_rows.saturating_add(rows as u64);
    }
}

/// [`snapshot`] with cross-frame reuse: when vt reports no damage and every
/// [`SnapKey`] input plus the resolved cursor state is unchanged, the
/// previous frame's snapshot is returned untouched instead of re-resolving
/// every visible cell.
pub(crate) fn snapshot_reuse(
    term: &mut vt::Terminal,
    cache: &mut SnapCache,
    colors: &Rc<Colors>,
    search: Option<&SearchQuery>,
    cell: CellSize,
    image_cache: &mut ImageCache,
    hover_link: Option<(usize, usize, usize)>,
) -> Rc<Snapshot> {
    cache.stats.frames = cache.stats.frames.saturating_add(1);
    let key = snapkey(term, colors, search, cell, hover_link);
    let damage = term.take_damage();
    let cursor = cursor_snap(term, colors);
    if damage.is_empty() {
        if let (Some(prev), Some(snap)) = (&cache.key, &cache.snap) {
            // Cursor motion is not damage-tracked; compare its resolved state.
            if keyeq(prev, &key) && snap.cursor == cursor {
                cache.last_snapshot_rows = 0;
                cache.stats.snapshot_reuses = cache.stats.snapshot_reuses.saturating_add(1);
                return snap.clone();
            }
        }
    }
    let previous = cache.snap.clone();
    let reuse = match (&damage, cache.key.as_ref(), previous.as_ref()) {
        (vt::Damage::Rows(rows), Some(prev_key), Some(prev)) if keyeq(prev_key, &key) => {
            Some(Reuse::Dirty(prev.as_ref(), rows.as_slice()))
        }
        (vt::Damage::Full, Some(prev_key), Some(prev))
            if prev_key.committed != key.committed
                && prev_key.offset == key.offset
                && prev_key.cols == key.cols
                && prev_key.rows == key.rows
                && prev_key.cell == key.cell
                && Rc::ptr_eq(&prev_key.colors, &key.colors)
                && prev_key.selection == key.selection
                && prev_key.hover_link == key.hover_link
                && prev_key.search.is_none()
                && key.search.is_none() =>
        {
            Some(Reuse::Scrolled(prev.as_ref()))
        }
        _ => None,
    };
    let (snap, built) = build_snapshot(
        term,
        colors,
        search,
        cell,
        image_cache,
        hover_link,
        cursor,
        reuse,
    );
    let snap = Rc::new(snap);
    cache.last_snapshot_rows = built;
    cache.stats.snapshot_rows = cache.stats.snapshot_rows.saturating_add(built as u64);
    cache.key = Some(key);
    cache.snap = Some(snap.clone());
    snap
}

/// Capture visible rows as background runs and styled spans. Resolves all
/// colors (theme palette + OSC 4 overrides + inverse + bold brightening +
/// selection) so nothing after this needs the lock.
#[cfg(test)]
pub(crate) fn snapshot(
    term: &mut vt::Terminal,
    colors: &Colors,
    search: Option<&SearchQuery>,
    cell: CellSize,
    image_cache: &mut ImageCache,
    hover_link: Option<(usize, usize, usize)>,
) -> Snapshot {
    let _ = term.take_damage();
    let cursor = cursor_snap(term, colors);
    build_snapshot(
        term,
        colors,
        search,
        cell,
        image_cache,
        hover_link,
        cursor,
        None,
    )
    .0
}

#[allow(clippy::too_many_arguments)]
fn build_snapshot(
    term: &mut vt::Terminal,
    colors: &Colors,
    search: Option<&SearchQuery>,
    cell: CellSize,
    image_cache: &mut ImageCache,
    hover_link: Option<(usize, usize, usize)>,
    cursor: Option<CursorSnap>,
    reuse: Option<Reuse<'_>>,
) -> (Snapshot, usize) {
    term.set_cell_pixels(cell.width.round() as u16, cell.height.round() as u16);

    let rows = term.rows();
    // Copied out: the row loop below borrows the terminal mutably
    // (scrolled-back rows may decode out of compressed history).
    let palette: [Option<(u8, u8, u8)>; 256] =
        std::array::from_fn(|i| term.palette_override(i as u8));
    let ovr = |i: u8| palette[i as usize];
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

    let (mut row_snaps, rebuild) = match reuse {
        Some(Reuse::Dirty(previous, dirty)) => (
            previous.rows.clone(),
            dirty
                .iter()
                .copied()
                .filter(|row| *row < rows)
                .collect::<Vec<_>>(),
        ),
        Some(Reuse::Scrolled(previous)) => {
            let prior: std::collections::HashMap<u64, Rc<RowSnapshot>> = previous
                .rows
                .iter()
                .map(|row| (row.source_revision, row.clone()))
                .collect();
            let mut next = Vec::with_capacity(rows);
            let mut dirty = Vec::new();
            for row_i in 0..rows {
                let revision = term.visible_row(row_i).revision();
                if let Some(row) = prior.get(&revision) {
                    next.push(row.clone());
                } else {
                    next.push(Rc::new(RowSnapshot {
                        source_revision: 0,
                        bg_runs: Vec::new(),
                        spans: Vec::new(),
                        boxes: Vec::new(),
                    }));
                    dirty.push(row_i);
                }
            }
            (next, dirty)
        }
        None => (Vec::new(), (0..rows).collect()),
    };
    row_snaps.resize_with(rows, || {
        Rc::new(RowSnapshot {
            source_revision: 0,
            bg_runs: Vec::new(),
            spans: Vec::new(),
            boxes: Vec::new(),
        })
    });

    for row_i in rebuild.iter().copied() {
        let mut bg_runs: Vec<BgRun> = Vec::new();
        let mut spans: Vec<Span> = Vec::new();
        let mut boxes: Vec<BoxCell> = Vec::new();
        let row = term.visible_row(row_i);
        let source_revision = row.revision();
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
            if colors.min_contrast > 1.0 {
                fg = colors::enforce_contrast(fg, bg, colors.min_contrast);
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
                // Both runs are this row's, so adjacency is a column test.
                match bg_runs.last_mut() {
                    Some(run) if run.col + run.len == col && run.color == bg => {
                        run.len += width;
                    }
                    _ => bg_runs.push(BgRun {
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
            // Underline the link currently hovered with the open-modifier held,
            // so auto-detected URLs (which carry no OSC 8 id) read as clickable.
            if let Some((hr, hs, he)) = hover_link {
                if row_i == hr && col >= hs && col <= he {
                    style.insert(CellFlags::UNDERLINE);
                }
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
        row_snaps[row_i] = Rc::new(RowSnapshot {
            source_revision,
            bg_runs,
            spans,
            boxes,
        });
    }

    let placements = term.images();
    let live: HashSet<u64> = placements.iter().map(|p| p.id).collect();
    image_cache.retain(&live);
    let images = placements
        .iter()
        .map(|p| {
            let image = image_cache.texture(p.id, &p.image);
            ImageDraw {
                line: p.line,
                col: p.col,
                width: p.image.width as f32,
                height: p.image.height as f32,
                image,
            }
        })
        .collect();

    let snapshot = Snapshot {
        #[cfg(test)]
        bg_runs: row_snaps
            .iter()
            .flat_map(|row| row.bg_runs.iter().cloned())
            .collect(),
        #[cfg(test)]
        spans: row_snaps
            .iter()
            .flat_map(|row| row.spans.iter().cloned())
            .collect(),
        #[cfg(test)]
        boxes: row_snaps
            .iter()
            .flat_map(|row| row.boxes.iter().cloned())
            .collect(),
        rows: row_snaps,
        cursor,
        offset,
        scrollback: term.grid().scrollback().len(),
        images,
    };
    (snapshot, rebuild.len())
}

/// Resolve the cursor's drawable state, or `None` while hidden or scrolled
/// back. The effective background under the cursor (inverse and selection
/// included) matters: full-screen programs paint their own cell backgrounds,
/// so the cursor must keep contrast against those, not just the theme
/// background.
pub(crate) fn cursor_snap(term: &vt::Terminal, colors: &Colors) -> Option<CursorSnap> {
    if !term.cursor_visible() || term.display_offset() != 0 {
        return None;
    }
    let ovr = |i: u8| term.palette_override(i);
    let (row, col) = term.cursor_pos();
    let cell = term.cell(row, col);
    let mut cell_bg = if cell.flags.contains(CellFlags::INVERSE) {
        let bold = cell.flags.contains(CellFlags::BOLD);
        colors::cell_rgb(cell.fg, colors.fg, bold, &colors.palette, ovr)
    } else {
        colors::cell_rgb(cell.bg, colors.bg, false, &colors.palette, ovr)
    };
    if term
        .selection()
        .is_some_and(|sel| sel.contains(metrics::selection_point(row, col, term.display_offset())))
    {
        cell_bg = colors.selection_bg;
    }
    let color = term
        .cursor_color()
        .map(|(r, g, b)| Rgb::new(r, g, b))
        .unwrap_or(colors.cursor);
    let color = colors::enforce_contrast(color, cell_bg, colors::CURSOR_MIN_CONTRAST);
    let text_color =
        colors::enforce_contrast(colors.cursor_text, color, colors::CURSOR_MIN_CONTRAST);
    Some(CursorSnap {
        row,
        col,
        style: term.cursor_style(),
        wide: cell.is_wide(),
        ch: cell.ch,
        color,
        text_color,
    })
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
/// default (blinking block) defers to the host's configured shape.
pub(crate) fn cursor_shape(style: vt::CursorStyle, fallback: CursorShape) -> CursorShape {
    match style {
        vt::CursorStyle::BlinkingBlock => fallback,
        vt::CursorStyle::SteadyBlock => CursorShape::Block,
        vt::CursorStyle::BlinkingUnderline | vt::CursorStyle::SteadyUnderline => {
            CursorShape::Underline
        }
        vt::CursorStyle::BlinkingBar | vt::CursorStyle::SteadyBar => CursorShape::Bar,
    }
}
