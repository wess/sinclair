//! Scrollback storage: a bounded ring of rows in two tiers.
//!
//! The newest rows sit uncompressed in `hot`; idle compaction
//! ([`Scrollback::compact_step`]) moves the oldest hot rows, one
//! [`BLOCK_ROWS`] block at a time, into `cold` as lz4-compressed blocks.
//! Every cold block holds exactly [`BLOCK_ROWS`] rows, so a logical index
//! maps to a block in O(1). Reads go through [`Scrollback::row`], which
//! keeps the last-touched block decoded in a one-slot cache - a sequential
//! scan decodes each block once.

use std::collections::VecDeque;

use crate::grid::codec::{self, Block};
use crate::grid::row::Row;

/// Default maximum number of scrollback rows.
pub const DEFAULT_SCROLLBACK: usize = 100_000;

/// Rows per compressed block; uniform so index math stays O(1).
pub(crate) const BLOCK_ROWS: usize = 512;

/// Compaction keeps at least this many newest rows uncompressed.
pub(crate) const HOT_TARGET: usize = 1024;

/// Ring buffer that rows scroll into off the top of the screen. The oldest
/// row is evicted once `limit` is reached. A limit of 0 disables storage
/// (used by the alternate screen).
#[derive(Debug, Clone)]
pub struct Scrollback {
    /// Newest rows, uncompressed.
    hot: VecDeque<Row>,
    /// Older rows, oldest first, [`BLOCK_ROWS`] per block.
    cold: VecDeque<Block>,
    /// Logical index 0 lands this far into the oldest block; eviction
    /// advances it and drops the block once fully consumed.
    front_skip: usize,
    /// Current grid width. Hot rows are kept at it eagerly
    /// ([`Scrollback::resize_rows`]); cold blocks resize lazily at decode.
    cols: usize,
    /// The last cold block touched, decoded: `(block index, rows)`.
    cache: Option<(usize, Vec<Row>)>,
    limit: usize,
    /// Count of rows committed to scrollback (survives eviction). Lets the
    /// host map a scrollback index to a stable line number for timestamps;
    /// it rolls back when a resize pulls rows out of the ring so the
    /// remaining rows keep their numbers.
    pushed: u64,
}

impl Scrollback {
    pub fn new(limit: usize) -> Scrollback {
        Scrollback {
            hot: VecDeque::new(),
            cold: VecDeque::new(),
            front_skip: 0,
            cols: 0,
            cache: None,
            limit,
            pushed: 0,
        }
    }

    pub fn limit(&self) -> usize {
        self.limit
    }

    /// Rows committed to scrollback (survives eviction); the stable line
    /// number of the next row to enter is `committed()`.
    pub fn committed(&self) -> u64 {
        self.pushed
    }

    fn cold_rows(&self) -> usize {
        self.cold.len() * BLOCK_ROWS
    }

    pub fn len(&self) -> usize {
        self.cold_rows() - self.front_skip + self.hot.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Append a row that scrolled off; evicts the oldest beyond the limit.
    pub fn push(&mut self, row: Row) {
        if self.limit == 0 {
            return;
        }
        if self.len() == self.limit {
            self.evict_oldest();
        }
        self.cols = row.len();
        self.hot.push_back(row);
        self.pushed += 1;
    }

    /// Append a copy of `row`, recycling the evicted front row's buffer when
    /// the eviction comes out of the hot tier. Instead of allocating a clone
    /// of `row` and freeing the dropped front row, the front row is popped
    /// and overwritten in place (reusing its `Vec<Cell>` capacity), then
    /// pushed to the back - no per-line alloc/free at steady state until
    /// compaction moves rows into cold blocks. Result is identical to
    /// `push(row.clone())`.
    pub(crate) fn push_recycled(&mut self, row: &Row) {
        if self.limit == 0 {
            return;
        }
        if self.len() == self.limit && self.cold.is_empty() {
            let mut recycled = self
                .hot
                .pop_front()
                .expect("len == limit > 0 with no cold rows implies a hot front");
            recycled.copy_from(row);
            self.cols = recycled.len();
            self.hot.push_back(recycled);
        } else {
            if self.len() == self.limit {
                self.evict_oldest();
            }
            self.cols = row.len();
            self.hot.push_back(row.clone());
        }
        self.pushed += 1;
    }

    /// Drop the oldest logical row: advance into the oldest cold block
    /// (dropping it once fully consumed), or pop the hot front when there
    /// are no cold rows.
    fn evict_oldest(&mut self) {
        if self.cold.is_empty() {
            self.hot.pop_front();
            return;
        }
        self.front_skip += 1;
        if self.front_skip == BLOCK_ROWS {
            self.cold.pop_front();
            self.front_skip = 0;
            // The cache indexes into `cold`; follow the shift.
            match &mut self.cache {
                Some((0, _)) => self.cache = None,
                Some((i, _)) => *i -= 1,
                None => {}
            }
        }
    }

    /// Remove and return the newest row (a resize pulling history back into
    /// the live grid). Rolls the pushed counter back so the stable line
    /// numbers of the remaining rows stay aligned.
    pub(crate) fn pop_newest(&mut self) -> Option<Row> {
        if self.hot.is_empty() {
            self.thaw_newest_block();
        }
        let row = self.hot.pop_back()?;
        self.pushed -= 1;
        Some(row)
    }

    /// Decompress the newest cold block back into the (empty) hot tier.
    fn thaw_newest_block(&mut self) {
        let Some(block) = self.cold.pop_back() else {
            return;
        };
        if self.cache.as_ref().is_some_and(|(i, _)| *i == self.cold.len()) {
            self.cache = None;
        }
        let mut rows = codec::decode(&block, self.cols);
        if self.cold.is_empty() && self.front_skip > 0 {
            // The thawed block was also the oldest: its first rows are
            // already evicted.
            rows.drain(..self.front_skip);
            self.front_skip = 0;
        }
        for row in rows.into_iter().rev() {
            self.hot.push_front(row);
        }
    }

    /// Move all rows out (cold blocks decompressed), leaving the ring empty;
    /// the pushed counter is untouched. Reflow drains the ring this way and
    /// rebuilds it.
    pub(crate) fn take_rows(&mut self) -> VecDeque<Row> {
        let mut out = VecDeque::with_capacity(self.len());
        for (i, block) in self.cold.iter().enumerate() {
            let skip = if i == 0 { self.front_skip } else { 0 };
            out.extend(codec::decode(block, self.cols).into_iter().skip(skip));
        }
        self.cold.clear();
        self.front_skip = 0;
        self.cache = None;
        out.append(&mut self.hot);
        out
    }

    /// Overwrite the pushed counter. Reflow re-pushes every row it drained,
    /// so it must restore the counter afterwards, adjusted only by the net
    /// rows that genuinely entered or left scrollback.
    pub(crate) fn set_committed(&mut self, pushed: u64) {
        self.pushed = pushed;
    }

    /// Row by age: index 0 is the oldest stored row. Takes `&mut self`
    /// because a row inside a cold block decodes into the block cache first.
    pub fn row(&mut self, index: usize) -> Option<&Row> {
        if index >= self.len() {
            return None;
        }
        let pos = index + self.front_skip;
        let cold_rows = self.cold_rows();
        if pos < cold_rows {
            let block = pos / BLOCK_ROWS;
            if self.cache.as_ref().is_none_or(|(i, _)| *i != block) {
                self.cache = Some((block, codec::decode(&self.cold[block], self.cols)));
            }
            let (_, rows) = self.cache.as_ref().expect("just filled");
            return rows.get(pos % BLOCK_ROWS);
        }
        self.hot.get(pos - cold_rows)
    }

    pub fn clear(&mut self) {
        self.hot.clear();
        self.cold.clear();
        self.front_skip = 0;
        self.cache = None;
    }

    /// Resize every stored row (simple truncate/pad; no reflow yet). Hot rows
    /// resize eagerly; cold blocks resize lazily when decoded.
    pub fn resize_rows(&mut self, cols: usize) {
        self.cols = cols;
        for row in &mut self.hot {
            row.resize(cols, crate::cell::Cell::default());
        }
        if let Some((_, rows)) = &mut self.cache {
            for row in rows {
                row.resize(cols, crate::cell::Cell::default());
            }
        }
    }

    /// Compress one block of the oldest hot rows into the cold tier, when
    /// at least [`HOT_TARGET`] rows would stay hot. Returns whether more
    /// work remains - the caller budgets, one block per call.
    pub fn compact_step(&mut self) -> bool {
        if self.hot.len() < HOT_TARGET + BLOCK_ROWS {
            return false;
        }
        let rows: Vec<Row> = self.hot.drain(..BLOCK_ROWS).collect();
        self.cold.push_back(codec::encode(&rows, self.cols));
        self.hot.len() >= HOT_TARGET + BLOCK_ROWS
    }

    /// Memory footprint estimate: `(resident_bytes, compressed_bytes)`.
    /// Resident counts uncompressed cells (hot rows plus the decode cache);
    /// compressed counts cold block payloads.
    pub fn memory(&self) -> (usize, usize) {
        let cell = std::mem::size_of::<crate::cell::Cell>();
        let cached = self
            .cache
            .as_ref()
            .map_or(0, |(_, rows)| rows.iter().map(|r| r.len()).sum());
        let resident: usize = self.hot.iter().map(|r| r.len()).sum::<usize>() + cached;
        let compressed = self.cold.iter().map(Block::compressed_len).sum();
        (resident * cell, compressed)
    }
}

#[cfg(test)]
#[path = "../../tests/grid/scrollback.rs"]
mod tests;
