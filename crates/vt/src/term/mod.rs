//! The terminal: owns both screens, modes, and the escape-sequence parser.

mod csi;
mod dcs;
mod ops;
mod osc;
mod perform;
pub mod report;
mod select;

pub use report::{Clipboard, Notification, ReportColors};

use crate::cell::Cell;
use crate::charset::Charsets;
use crate::cursor::CursorStyle;
use crate::grid::damage::Damage;
use crate::grid::row::Row;
use crate::grid::Grid;
use crate::hyperlink::{Hyperlink, HyperlinkId, Hyperlinks};
use crate::mode::{Modes, MouseMode};
use crate::screen::Screen;
use crate::selection::Selection;

/// A link found under a viewport cell: its target and the inclusive
/// cell-column span it occupies on that row. See [`Term::link_at`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkHit {
    pub url: String,
    pub start_col: usize,
    pub end_col: usize,
}

/// Full terminal state. Feed pty bytes with [`Terminal::feed`], drain
/// responses for the pty with [`Terminal::take_output`], and read cells via
/// the grid/row accessors when rendering.
pub struct Terminal {
    parser: vte::Parser,
    inner: Inner,
}

/// All mutable terminal state; the `vte::Perform` target. Lives apart from
/// the parser because `Parser::advance` borrows both mutably.
pub(crate) struct Inner {
    pub(crate) primary: Screen,
    pub(crate) alt: Screen,
    pub(crate) modes: Modes,
    pub(crate) charsets: Charsets,
    pub(crate) title: String,
    pub(crate) title_stack: Vec<String>,
    pub(crate) cwd: Option<String>,
    /// OSC 4 palette overrides; `None` means "use the theme".
    pub(crate) palette: [Option<(u8, u8, u8)>; 256],
    pub(crate) cursor_color: Option<(u8, u8, u8)>,
    pub(crate) cursor_style: CursorStyle,
    /// Bytes the host must write back to the pty (DSR replies, DA, ...).
    pub(crate) output: Vec<u8>,
    pub(crate) bell: bool,
    /// Set by whole-terminal render events (alt switch, RIS, palette OSC,
    /// resize, scroll-offset changes); overrides per-row grid damage.
    pub(crate) full_damage: bool,
    /// Set when the title changes (OSC 0/2 or XTWINOPS title pop).
    pub(crate) title_changed: bool,
    /// Set on a shell-integration command-finished mark (OSC 133 `D`); the
    /// inner value is the reported exit code (`None` when the mark omits it).
    pub(crate) command_finished: Option<Option<i32>>,
    /// Cursor `(row, col)` where shell input begins on the current prompt line,
    /// set by OSC 133 `B` (command-line start). Powers autosuggestion's read of
    /// the line being typed; cleared when the command runs (`133;C`).
    pub(crate) input_start: Option<(usize, usize)>,
    /// Executed command lines captured on `133;C`, newest last, capped. Feeds
    /// history-based autosuggestion.
    pub(crate) history: std::collections::VecDeque<String>,
    /// Set when OSC 7 reports a working directory different from the last one.
    pub(crate) cwd_changed: bool,
    pub(crate) last_printed: Option<char>,
    /// Lines scrolled back into history for display; 0 = bottom.
    pub(crate) display_offset: usize,
    pub(crate) scrollback_limit: usize,
    /// Active selection, in content-anchored absolute coordinates
    /// (see [`crate::selection`]).
    pub(crate) selection: Option<Selection>,
    /// Extra characters (beyond alphanumerics) word selection treats as
    /// word constituents.
    pub(crate) word_chars: Vec<char>,
    /// Pending OSC 52 clipboard write for the host to act on.
    pub(crate) clipboard: Option<report::Clipboard>,
    /// Pending OSC 9/777/99 desktop notification for the host to post.
    pub(crate) notification: Option<report::Notification>,
    /// Colors the host installed for answering OSC color queries.
    pub(crate) report_colors: Option<Box<report::ReportColors>>,
    /// Interned OSC 8 hyperlinks referenced by cells.
    pub(crate) hyperlinks: Hyperlinks,
    /// In-progress device control string (XTGETTCAP or sixel), if any.
    pub(crate) dcs: dcs::Dcs,
    /// Monotonic id for the next image placement.
    pub(crate) image_seq: u64,
    /// Cell size in pixels `(w, h)`, set by the host; sixel uses it to reserve
    /// rows. A sane default until the host reports real metrics.
    pub(crate) cell_px: (u16, u16),
}

impl Inner {
    pub(crate) fn screen(&self) -> &Screen {
        if self.modes.contains(Modes::ALT_SCREEN) {
            &self.alt
        } else {
            &self.primary
        }
    }

    pub(crate) fn screen_mut(&mut self) -> &mut Screen {
        if self.modes.contains(Modes::ALT_SCREEN) {
            &mut self.alt
        } else {
            &mut self.primary
        }
    }
}

impl Terminal {
    /// A `cols` x `rows` terminal whose primary screen keeps up to
    /// `scrollback_limit` history rows (see [`crate::DEFAULT_SCROLLBACK`]).
    pub fn new(cols: usize, rows: usize, scrollback_limit: usize) -> Terminal {
        Terminal {
            parser: vte::Parser::new(),
            inner: Inner {
                primary: Screen::new(cols, rows, scrollback_limit),
                alt: Screen::new(cols, rows, 0),
                modes: Modes::default(),
                charsets: Charsets::default(),
                title: String::new(),
                title_stack: Vec::new(),
                cwd: None,
                palette: [None; 256],
                cursor_color: None,
                cursor_style: CursorStyle::default(),
                output: Vec::new(),
                bell: false,
                command_finished: None,
                input_start: None,
                history: std::collections::VecDeque::new(),
                cwd_changed: false,
                full_damage: true,
                title_changed: false,
                last_printed: None,
                display_offset: 0,
                scrollback_limit,
                selection: None,
                word_chars: vec!['/', '-', '_', '.', '~'],
                clipboard: None,
                notification: None,
                report_colors: None,
                hyperlinks: Hyperlinks::default(),
                dcs: dcs::Dcs::None,
                image_seq: 0,
                cell_px: (8, 16),
            },
        }
    }

    /// Drive the parser with bytes read from the pty. vte batch-scans
    /// plain-text runs, so this is not a per-byte state machine walk.
    pub fn feed(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.inner, bytes);
    }

    /// Simple resize; clamps cursors, resets scroll regions, and drops any
    /// selection (no reflow yet, so old coordinates would lie).
    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.inner.primary.resize(cols, rows);
        self.inner.alt.resize(cols, rows);
        self.inner.display_offset = self
            .inner
            .display_offset
            .min(self.inner.primary.grid.scrollback().len());
        self.inner.selection = None;
        self.inner.full_damage = true;
    }

    pub fn cols(&self) -> usize {
        self.inner.screen().grid.cols()
    }

    pub fn rows(&self) -> usize {
        self.inner.screen().grid.rows()
    }

    /// The active screen (alternate when it is enabled).
    pub fn screen(&self) -> &Screen {
        self.inner.screen()
    }

    /// The active screen's grid.
    pub fn grid(&self) -> &Grid {
        &self.inner.screen().grid
    }

    /// The active screen's grid, mutably. Scrollback row reads need it:
    /// compressed history decodes through a cache on the ring.
    pub fn grid_mut(&mut self) -> &mut Grid {
        &mut self.inner.screen_mut().grid
    }

    /// Compress one block of idle scrollback history (primary screen; the
    /// alternate screen keeps none). Returns whether more work remains, so
    /// the host can budget one block per tick.
    pub fn compact_scrollback(&mut self) -> bool {
        self.inner.primary.grid.scrollback_mut().compact_step()
    }

    /// Primary-screen scrollback footprint:
    /// `(resident_bytes_estimate, compressed_bytes)`.
    pub fn scrollback_memory(&self) -> (usize, usize) {
        self.inner.primary.grid.scrollback().memory()
    }

    /// Sixel images anchored to the active screen's buffer, oldest first.
    /// Lines follow the [`crate::selection`] scheme: 0 is the top live row,
    /// negative scrollback.
    pub fn images(&self) -> &[crate::sixel::Placement] {
        &self.inner.screen().images
    }

    /// Tell the emulator the cell size in pixels so sixel can reserve rows.
    pub fn set_cell_pixels(&mut self, w: u16, h: u16) {
        self.inner.cell_px = (w.max(1), h.max(1));
    }

    /// Cell accessor on the active grid (no scrollback offset applied).
    pub fn cell(&self, row: usize, col: usize) -> &Cell {
        self.inner.screen().grid.cell(row, col)
    }

    /// Text of a visible row (offset-aware), trimmed; for tests/debugging.
    pub fn row_text(&mut self, row: usize) -> String {
        self.visible_row(row).text()
    }

    /// Viewport row `i` (0 = top) honoring the display offset into
    /// scrollback. The alternate screen has no scrollback, so the offset
    /// only matters on primary. `&mut` because a scrolled-back row may
    /// decode out of a compressed history block.
    pub fn visible_row(&mut self, i: usize) -> &Row {
        let offset = self.inner.display_offset;
        let grid = &mut self.inner.screen_mut().grid;
        let sb_len = grid.scrollback().len();
        let global = sb_len - offset.min(sb_len) + i;
        if global < sb_len {
            grid.scrollback_mut().row(global).expect("in range")
        } else {
            grid.row(global - sb_len)
        }
    }

    /// The whole buffer as plain text: every scrollback row followed by the
    /// live screen, one right-trimmed row per line, with trailing blank lines
    /// dropped and a single trailing newline when non-empty. Independent of the
    /// current selection and scroll position. Dumps whichever screen is active,
    /// so on the alternate screen only its rows appear (it has no scrollback).
    pub fn buffer_text(&mut self) -> String {
        let grid = &mut self.inner.screen_mut().grid;
        let sb_len = grid.scrollback().len();
        let mut lines: Vec<String> = Vec::with_capacity(sb_len + grid.rows());
        for i in 0..sb_len {
            if let Some(row) = grid.scrollback_mut().row(i) {
                lines.push(row.text());
            }
        }
        for r in 0..grid.rows() {
            lines.push(grid.row(r).text());
        }
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        if lines.is_empty() {
            return String::new();
        }
        lines.push(String::new()); // trailing newline
        lines.join("\n")
    }

    /// Rows committed to scrollback (survives eviction; only moves for rows
    /// genuinely entering or leaving history, so resizes don't inflate it).
    /// Used by the host to stamp scrollback lines with a time.
    pub fn committed_lines(&self) -> u64 {
        self.inner.screen().grid.scrollback().committed()
    }

    /// How far the view is scrolled back into history (0 = live bottom).
    pub fn display_offset(&self) -> usize {
        self.inner.display_offset
    }

    /// Scroll the view; clamped to available scrollback. Changing the
    /// offset shifts every visible row, so it escalates to full damage.
    /// vt never resets the offset on output by itself (the app decides);
    /// it only keeps it stable as new lines enter scrollback, and resets
    /// it when the alternate screen is entered (no scrollback there).
    pub fn set_display_offset(&mut self, offset: usize) {
        let max = self.inner.screen().grid.scrollback().len();
        let offset = offset.min(max);
        if offset != self.inner.display_offset {
            self.inner.display_offset = offset;
            self.inner.full_damage = true;
        }
    }

    /// Scroll the view by `delta` lines: positive scrolls back into
    /// history, negative toward the live bottom. Clamped to
    /// `[0, scrollback len]`.
    pub fn scroll_display(&mut self, delta: isize) {
        let max = self.inner.screen().grid.scrollback().len() as isize;
        let next = (self.inner.display_offset as isize + delta).clamp(0, max);
        self.set_display_offset(next as usize);
    }

    pub fn title(&self) -> &str {
        &self.inner.title
    }

    pub fn cwd(&self) -> Option<&str> {
        self.inner.cwd.as_deref()
    }

    /// Drain bytes that must be written back to the pty.
    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.inner.output)
    }

    /// `true` once if a BEL was seen since the last call.
    pub fn take_bell(&mut self) -> bool {
        std::mem::take(&mut self.inner.bell)
    }

    /// Accumulated render damage; returns it and resets to clean. A fresh
    /// terminal reports [`Damage::Full`]. Whole-terminal events (resize,
    /// alt-screen switch, RIS, ED 2/3, palette OSCs, display-offset
    /// changes) escalate to full damage.
    pub fn take_damage(&mut self) -> Damage {
        if std::mem::take(&mut self.inner.full_damage) {
            self.inner.primary.grid.take_damage();
            self.inner.alt.grid.take_damage();
            return Damage::Full;
        }
        self.inner.screen_mut().grid.take_damage()
    }

    /// The new title once after it changed (OSC 0/2 or title-stack pop);
    /// `None` until the next change. [`Terminal::title`] always works.
    pub fn take_title_changed(&mut self) -> Option<String> {
        std::mem::take(&mut self.inner.title_changed).then(|| self.inner.title.clone())
    }

    /// The exit code of a command that just finished (OSC 133 `D`), once per
    /// mark. The outer `Option` is "did a command finish since the last call";
    /// the inner is the exit code (`None` when the mark carried none).
    pub fn take_command_finished(&mut self) -> Option<Option<i32>> {
        std::mem::take(&mut self.inner.command_finished)
    }

    /// The shell input line currently being typed, from the OSC 133 `B` mark to
    /// the end of that row (right-trimmed). `None` when no input mark is active
    /// (e.g. no shell integration, or a command is running). Backs autosuggest.
    pub fn current_input(&self) -> Option<String> {
        let (srow, scol) = self.inner.input_start?;
        let grid = &self.inner.screen().grid;
        if srow >= grid.rows() {
            return None;
        }
        Some(grid.row(srow).text().chars().skip(scol).collect())
    }

    /// `(row, col)` one past the end of the current input line, where ghost text
    /// should begin. `None` when there's no active input.
    pub fn input_end(&self) -> Option<(usize, usize)> {
        let (srow, scol) = self.inner.input_start?;
        let text = self.current_input()?;
        Some((srow, scol + text.chars().count()))
    }

    /// Captured command history, newest first. Empty without shell integration.
    pub fn command_history(&self) -> Vec<String> {
        self.inner.history.iter().rev().cloned().collect()
    }

    /// The new working directory once after OSC 7 reported a change; `None`
    /// until it changes again. [`Terminal::cwd`] always works.
    pub fn take_cwd_changed(&mut self) -> Option<String> {
        std::mem::take(&mut self.inner.cwd_changed)
            .then(|| self.inner.cwd.clone())
            .flatten()
    }

    pub fn is_alt_screen(&self) -> bool {
        self.inner.modes.contains(Modes::ALT_SCREEN)
    }

    pub fn cursor_visible(&self) -> bool {
        self.inner.modes.contains(Modes::CURSOR_VISIBLE)
    }

    /// Cursor `(row, col)`, 0-based, in active-grid coordinates.
    pub fn cursor_pos(&self) -> (usize, usize) {
        let c = &self.inner.screen().cursor;
        (c.row, c.col)
    }

    pub fn cursor_style(&self) -> CursorStyle {
        self.inner.cursor_style
    }

    pub fn modes(&self) -> Modes {
        self.inner.modes
    }

    /// DECCKM (CSI ? 1 h/l): application cursor keys.
    pub fn cursor_keys_app(&self) -> bool {
        self.inner.modes.contains(Modes::APP_CURSOR)
    }

    /// DECKPAM/DECKPNM (ESC = / ESC >): application keypad.
    pub fn keypad_app(&self) -> bool {
        self.inner.modes.contains(Modes::APP_KEYPAD)
    }

    /// Bracketed paste (CSI ? 2004 h/l).
    pub fn bracketed_paste(&self) -> bool {
        self.inner.modes.contains(Modes::BRACKETED_PASTE)
    }

    /// Strongest enabled mouse reporting mode (?1000/?1002/?1003).
    pub fn mouse_mode(&self) -> MouseMode {
        MouseMode::from_modes(self.inner.modes)
    }

    /// SGR mouse encoding (?1006).
    pub fn mouse_sgr(&self) -> bool {
        self.inner.modes.contains(Modes::MOUSE_SGR)
    }

    /// Alternate scroll (?1007): wheel sends arrow keys on the alternate
    /// screen. Defaults on.
    pub fn alternate_scroll(&self) -> bool {
        self.inner.modes.contains(Modes::ALT_SCROLL)
    }

    /// OSC 4 palette override for an index, if any.
    pub fn palette_override(&self, index: u8) -> Option<(u8, u8, u8)> {
        self.inner.palette[index as usize]
    }

    /// OSC 12 cursor color, if set (OSC 112 clears it).
    pub fn cursor_color(&self) -> Option<(u8, u8, u8)> {
        self.inner.cursor_color
    }

    /// Focus reporting (?1004): the program wants CSI I / CSI O on focus
    /// changes. The host calls [`Terminal::report_focus`] on window events.
    pub fn focus_reporting(&self) -> bool {
        self.inner.modes.contains(Modes::FOCUS_REPORT)
    }

    /// Emit a focus-in (CSI I) or focus-out (CSI O) report if the program
    /// enabled focus reporting; otherwise a no-op.
    pub fn report_focus(&mut self, focused: bool) {
        if self.focus_reporting() {
            self.inner
                .output
                .extend_from_slice(if focused { b"\x1b[I" } else { b"\x1b[O" });
        }
    }

    /// Synchronized output (?2026): while set, the host should hold off
    /// presenting frames so the program's update lands atomically.
    pub fn synchronized_output(&self) -> bool {
        self.inner.modes.contains(Modes::SYNC_OUTPUT)
    }

    /// Install the colors used to answer OSC 4/10/11/12 queries. Call this
    /// from the theme and refresh it on config reload.
    pub fn set_report_colors(&mut self, colors: report::ReportColors) {
        self.inner.report_colors = Some(Box::new(colors));
    }

    /// Take a pending OSC 9/777/99 desktop notification, if one was requested.
    pub fn take_notification(&mut self) -> Option<report::Notification> {
        self.inner.notification.take()
    }

    /// Take a pending OSC 52 clipboard write, if the program requested one.
    pub fn take_clipboard(&mut self) -> Option<report::Clipboard> {
        self.inner.clipboard.take()
    }

    /// Resolve an OSC 8 hyperlink id (from a [`Cell`]) to its target.
    pub fn hyperlink(&self, id: HyperlinkId) -> Option<&Hyperlink> {
        self.inner.hyperlinks.get(id)
    }

    /// The hyperlink URI a cell belongs to, if any.
    pub fn cell_hyperlink(&self, cell: &Cell) -> Option<&str> {
        cell.hyperlink
            .and_then(|id| self.inner.hyperlinks.get(id))
            .map(|link| link.uri.as_str())
    }

    /// Active kitty keyboard enhancement flags on the current screen (0 in
    /// legacy mode). Feed this into the input encoder.
    pub fn kitty_keyboard_flags(&self) -> u8 {
        self.inner.screen().kitty.current()
    }

    /// Search the whole buffer (scrollback + grid) for `needle`, returning
    /// matches in global-row order. `case_sensitive` false folds ASCII case.
    /// Matches do not span row breaks.
    pub fn search(&mut self, needle: &str, case_sensitive: bool) -> Vec<crate::search::Match> {
        let needle: Vec<char> = needle.chars().collect();
        if needle.is_empty() {
            return Vec::new();
        }
        let grid = &mut self.inner.screen_mut().grid;
        let sb_len = grid.scrollback().len();
        let mut out = Vec::new();
        let mut chars = Vec::new();
        let mut col_of = Vec::new();
        let mut row_matches = |row: &Row, line: usize, out: &mut Vec<crate::search::Match>| {
            row_chars(&row.cells, &mut chars, &mut col_of);
            out.extend(crate::search::in_row(
                &needle,
                &chars,
                &col_of,
                line,
                !case_sensitive,
                |c| row.cells.get(c).is_some_and(|cell| cell.is_wide()),
            ));
        };
        for i in 0..sb_len {
            if let Some(row) = grid.scrollback_mut().row(i) {
                row_matches(row, i, &mut out);
            }
        }
        for r in 0..grid.rows() {
            row_matches(grid.row(r), sb_len + r, &mut out);
        }
        out
    }

    /// Text rows across scrollback + live grid in global-row order. Each
    /// tuple is `(line, text, prompt_marked)`, using the same line index
    /// space as [`Terminal::prompt_lines`].
    pub fn text_lines(&mut self) -> Vec<(usize, String, bool)> {
        let grid = &mut self.inner.screen_mut().grid;
        let sb_len = grid.scrollback().len();
        let mut out = Vec::with_capacity(sb_len + grid.rows());
        for i in 0..sb_len {
            if let Some(row) = grid.scrollback_mut().row(i) {
                out.push((i, row.text(), row.prompt));
            }
        }
        for r in 0..grid.rows() {
            let row = grid.row(r);
            out.push((sb_len + r, row.text(), row.prompt));
        }
        out
    }

    /// The link under viewport `row`/`col` — an OSC 8 hyperlink if the cell
    /// carries one (expanded to the contiguous same-id run), else a detectable
    /// URL in the row's text. Returns the target plus the inclusive cell-column
    /// span, so callers can open it, underline it, or select it uniformly.
    pub fn link_at(&mut self, row: usize, col: usize) -> Option<LinkHit> {
        if row >= self.rows() {
            return None;
        }
        // Own the cells: the row reference pins the terminal (it may live in
        // the scrollback decode cache) but the hyperlink registry is needed
        // below. Cells are `Copy`, so this is one flat buffer.
        let cells = self.visible_row(row).cells.clone();
        // OSC 8 hyperlink: expand over the run of cells sharing the same id.
        if let Some(cell) = cells.get(col) {
            if let Some(hid) = cell.hyperlink {
                let url = self.hyperlink(hid)?.uri.clone();
                let mut start = col;
                while start > 0 && cells[start - 1].hyperlink == Some(hid) {
                    start -= 1;
                }
                let mut end = col;
                while end + 1 < cells.len() && cells[end + 1].hyperlink == Some(hid) {
                    end += 1;
                }
                return Some(LinkHit { url, start_col: start, end_col: end });
            }
        }
        // Auto-detected URL in the row text (skip wide-cell spacers).
        let mut chars = Vec::new();
        let mut col_of = Vec::new();
        row_chars(&cells, &mut chars, &mut col_of);
        for (start, end) in crate::url::find(&chars) {
            let start_col = col_of[start];
            let last = col_of[end - 1];
            let end_col = last + usize::from(cells[last].is_wide());
            if col >= start_col && col <= end_col {
                return Some(LinkHit {
                    url: chars[start..end].iter().collect(),
                    start_col,
                    end_col,
                });
            }
        }
        None
    }

    /// Every detectable URL in the visible viewport, as `(row, start_col,
    /// end_col_inclusive, text)`, for hint-mode labelling. Wide cells extend
    /// the end column by their spacer.
    pub fn visible_links(&mut self) -> Vec<(usize, usize, usize, String)> {
        let mut out = Vec::new();
        let mut chars = Vec::new();
        let mut col_of = Vec::new();
        for row in 0..self.rows() {
            let cells = &self.visible_row(row).cells;
            row_chars(cells, &mut chars, &mut col_of);
            for (start, end) in crate::url::find(&chars) {
                let start_col = col_of[start];
                let last = col_of[end - 1];
                let end_col = last + usize::from(cells[last].is_wide());
                out.push((row, start_col, end_col, chars[start..end].iter().collect()));
            }
        }
        out
    }

    /// Global indices of rows marked as shell prompts (OSC 133;A), sorted
    /// oldest first. Index space matches the viewport: `0..scrollback.len()`
    /// are history rows, `scrollback.len()..` are live-grid rows - so the
    /// top viewport row is `scrollback.len() - display_offset`. Used for
    /// jump-to-prompt.
    pub fn prompt_lines(&mut self) -> Vec<usize> {
        let grid = &mut self.inner.screen_mut().grid;
        let sb_len = grid.scrollback().len();
        let mut lines = Vec::new();
        for i in 0..sb_len {
            if grid.scrollback_mut().row(i).is_some_and(|r| r.prompt) {
                lines.push(i);
            }
        }
        for r in 0..grid.rows() {
            if grid.row(r).prompt {
                lines.push(sb_len + r);
            }
        }
        lines
    }
}

/// Fill `chars`/`col_of` with a row's characters and their cell columns,
/// skipping wide spacers, reusing the buffers across rows.
fn row_chars(cells: &[Cell], chars: &mut Vec<char>, col_of: &mut Vec<usize>) {
    chars.clear();
    col_of.clear();
    for (c, cell) in cells.iter().enumerate() {
        if cell.is_wide_spacer() {
            continue;
        }
        chars.push(cell.ch);
        col_of.push(c);
    }
}

#[cfg(test)]
#[path = "../../tests/term/mod.rs"]
mod tests;
