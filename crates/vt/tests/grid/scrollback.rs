use super::*;

use crate::cell::{Cell, CellFlags};
use crate::color::Color;
use crate::hyperlink::HyperlinkId;

fn marked(ch: char) -> Row {
    let mut r = Row::new(2);
    r.cells[0].ch = ch;
    r
}

/// A row whose content encodes `n`, over `cols` columns, exercising the
/// codec fields as `n` varies.
fn numbered(n: usize, cols: usize) -> Row {
    let mut row = Row::new(cols);
    let text = format!("row {n}");
    for (i, ch) in text.chars().enumerate().take(cols) {
        row.cells[i].ch = ch;
    }
    match n % 4 {
        1 => {
            row.cells[0].fg = Color::Indexed((n % 256) as u8);
            row.cells[0].flags = CellFlags::BOLD;
        }
        2 => {
            row.cells[1].bg = Color::Rgb((n % 256) as u8, 7, 9);
            row.wrapped = true;
        }
        3 => {
            row.cells[2].hyperlink = HyperlinkId::from_u16((n % 100 + 1) as u16);
            row.prompt = true;
        }
        _ => {}
    }
    row
}

/// Drive compaction to completion.
fn compact_fully(sb: &mut Scrollback) {
    while sb.compact_step() {}
}

#[test]
fn pushes_in_order() {
    let mut sb = Scrollback::new(10);
    sb.push(marked('a'));
    sb.push(marked('b'));
    assert_eq!(sb.len(), 2);
    assert_eq!(sb.row(0).unwrap().text(), "a");
    assert_eq!(sb.row(1).unwrap().text(), "b");
}

#[test]
fn evicts_oldest_at_limit() {
    let mut sb = Scrollback::new(2);
    sb.push(marked('a'));
    sb.push(marked('b'));
    sb.push(marked('c'));
    assert_eq!(sb.len(), 2);
    assert_eq!(sb.row(0).unwrap().text(), "b");
    assert_eq!(sb.row(1).unwrap().text(), "c");
}

#[test]
fn zero_limit_stores_nothing() {
    let mut sb = Scrollback::new(0);
    sb.push(marked('a'));
    assert!(sb.is_empty());
}

#[test]
fn clear_empties() {
    let mut sb = Scrollback::new(5);
    sb.push(marked('a'));
    sb.clear();
    assert!(sb.is_empty());
    assert_eq!(sb.limit(), 5);
}

#[test]
fn resize_rows_changes_width() {
    let mut sb = Scrollback::new(5);
    sb.push(marked('a'));
    sb.resize_rows(7);
    assert_eq!(sb.row(0).unwrap().len(), 7);
}

#[test]
fn compact_step_reports_no_work_when_hot_is_small() {
    let mut sb = Scrollback::new(10_000);
    for n in 0..HOT_TARGET + BLOCK_ROWS - 1 {
        sb.push(numbered(n, 20));
    }
    assert!(!sb.compact_step());
    assert!(sb.cold.is_empty());
}

#[test]
fn compaction_keeps_hot_floor_and_uniform_blocks() {
    let mut sb = Scrollback::new(10_000);
    let total = HOT_TARGET + 5 * BLOCK_ROWS + 37;
    for n in 0..total {
        sb.push(numbered(n, 20));
    }
    compact_fully(&mut sb);
    assert!(sb.hot.len() >= HOT_TARGET);
    assert!(sb.hot.len() < HOT_TARGET + BLOCK_ROWS);
    assert_eq!(sb.hot.len() + sb.cold.len() * BLOCK_ROWS, total);
    assert_eq!(sb.len(), total);
}

#[test]
fn compacted_rows_round_trip_exactly() {
    let mut sb = Scrollback::new(100_000);
    let total = HOT_TARGET + 6 * BLOCK_ROWS;
    let mut originals = Vec::with_capacity(total);
    for n in 0..total {
        // Mix in wide chars, combining marks, and long runs.
        let mut row = numbered(n, 30);
        if n % 5 == 0 {
            row.cells[10].ch = '漢';
            row.cells[10].flags |= CellFlags::WIDE;
            row.cells[11] = Cell {
                flags: CellFlags::WIDE_SPACER,
                ..Cell::default()
            };
        }
        if n % 7 == 0 {
            row.cells[12].ch = 'e';
            row.cells[12].zw = '\u{0301}';
        }
        if n % 11 == 0 {
            for c in &mut row.cells[13..25] {
                c.ch = '=';
                c.fg = Color::Rgb(9, 9, 9);
            }
        }
        originals.push(row.clone());
        sb.push(row);
    }
    compact_fully(&mut sb);
    assert!(!sb.cold.is_empty());
    for (i, original) in originals.iter().enumerate() {
        assert_eq!(sb.row(i).unwrap(), original, "row {i}");
    }
    // Reverse order too, so the block cache thrashes both directions.
    for (i, original) in originals.iter().enumerate().rev() {
        assert_eq!(sb.row(i).unwrap(), original, "row {i} (reverse)");
    }
}

#[test]
fn eviction_at_limit_spans_block_boundaries() {
    let limit = HOT_TARGET + 2 * BLOCK_ROWS;
    let mut sb = Scrollback::new(limit);
    for n in 0..limit {
        sb.push(numbered(n, 10));
    }
    compact_fully(&mut sb);
    assert_eq!(sb.len(), limit);
    // Push enough to consume the whole first cold block and reach into the
    // second; every push evicts exactly one row.
    let extra = BLOCK_ROWS + 100;
    for n in limit..limit + extra {
        sb.push(numbered(n, 10));
        assert_eq!(sb.len(), limit);
    }
    assert_eq!(sb.row(0).unwrap().text(), format!("row {extra}"));
    let last = limit + extra - 1;
    assert_eq!(sb.row(limit - 1).unwrap().text(), format!("row {last}"));
    assert_eq!(sb.committed(), (limit + extra) as u64);
}

#[test]
fn push_recycled_matches_push_across_tiers() {
    let limit = HOT_TARGET + BLOCK_ROWS;
    let mut a = Scrollback::new(limit);
    let mut b = Scrollback::new(limit);
    for n in 0..limit + 200 {
        let row = numbered(n, 8);
        a.push(row.clone());
        b.push_recycled(&row);
        if n == limit / 2 {
            compact_fully(&mut a);
            compact_fully(&mut b);
        }
    }
    assert_eq!(a.len(), b.len());
    assert_eq!(a.committed(), b.committed());
    for i in 0..a.len() {
        let row = a.row(i).unwrap().clone();
        assert_eq!(&row, b.row(i).unwrap(), "row {i}");
    }
}

#[test]
fn committed_is_stable_across_compaction_and_eviction() {
    let limit = HOT_TARGET + BLOCK_ROWS;
    let mut sb = Scrollback::new(limit);
    for n in 0..limit {
        sb.push(numbered(n, 8));
    }
    assert_eq!(sb.committed(), limit as u64);
    compact_fully(&mut sb);
    assert_eq!(sb.committed(), limit as u64);
    sb.push(numbered(limit, 8));
    assert_eq!(sb.committed(), limit as u64 + 1);
    let popped = sb.pop_newest().unwrap();
    assert_eq!(popped.text(), format!("row {limit}"));
    assert_eq!(sb.committed(), limit as u64);
}

#[test]
fn pop_newest_thaws_cold_blocks() {
    let mut sb = Scrollback::new(10_000);
    let total = HOT_TARGET + BLOCK_ROWS;
    for n in 0..total {
        sb.push(numbered(n, 8));
    }
    compact_fully(&mut sb);
    // Drain everything through pop_newest; cold must thaw transparently.
    for n in (0..total).rev() {
        let row = sb.pop_newest().unwrap();
        assert_eq!(row.text(), format!("row {n}"));
        assert_eq!(sb.committed(), n as u64);
    }
    assert!(sb.pop_newest().is_none());
    assert!(sb.is_empty());
}

#[test]
fn pop_newest_respects_partially_evicted_only_block() {
    let limit = HOT_TARGET + BLOCK_ROWS;
    let mut sb = Scrollback::new(limit);
    for n in 0..limit {
        sb.push(numbered(n, 8));
    }
    compact_fully(&mut sb);
    // Evict 10 rows out of the front (and only) cold block...
    for n in limit..limit + 10 {
        sb.push(numbered(n, 8));
    }
    // ...then drain: the evicted rows must not resurrect.
    let mut seen = Vec::new();
    while let Some(row) = sb.pop_newest() {
        seen.push(row.text());
    }
    assert_eq!(seen.len(), limit);
    assert_eq!(seen.last().unwrap(), "row 10");
    assert_eq!(seen.first().unwrap(), &format!("row {}", limit + 9));
}

#[test]
fn take_rows_drains_cold_and_hot_preserving_committed() {
    let limit = HOT_TARGET + 2 * BLOCK_ROWS;
    let mut sb = Scrollback::new(limit);
    for n in 0..limit + 5 {
        sb.push(numbered(n, 8));
    }
    compact_fully(&mut sb);
    let committed = sb.committed();
    let rows = sb.take_rows();
    assert_eq!(rows.len(), limit);
    assert_eq!(rows[0].text(), "row 5");
    assert_eq!(rows[limit - 1].text(), format!("row {}", limit + 4));
    assert!(sb.is_empty());
    assert_eq!(sb.committed(), committed);
}

#[test]
fn resize_after_compaction_decodes_lazily() {
    let mut sb = Scrollback::new(10_000);
    let total = HOT_TARGET + 2 * BLOCK_ROWS;
    for n in 0..total {
        let mut row = numbered(n, 12);
        // Wide pair in the last two columns; a narrower resize slices it.
        row.cells[10].ch = '漢';
        row.cells[10].flags |= CellFlags::WIDE;
        row.cells[11] = Cell {
            flags: CellFlags::WIDE_SPACER,
            ..Cell::default()
        };
        sb.push(row);
    }
    compact_fully(&mut sb);
    sb.resize_rows(16);
    assert_eq!(sb.row(0).unwrap().len(), 16);
    assert_eq!(sb.row(0).unwrap().cells[10].ch, '漢');
    assert_eq!(sb.row(total - 1).unwrap().len(), 16);
    sb.resize_rows(11);
    for &i in &[0, BLOCK_ROWS, total - 1] {
        let row = sb.row(i).unwrap();
        assert_eq!(row.len(), 11, "row {i}");
        // The stranded wide head is blanked, as Row::resize does live.
        assert_eq!(row.cells[10], Cell::default(), "row {i}");
    }
}

#[test]
fn compaction_compresses_realistic_history_at_least_5x() {
    let cols = 100;
    let limit = 100_000;
    let mut sb = Scrollback::new(limit);
    for n in 0..limit {
        let mut row = Row::new(cols);
        // Shell-ish lines: mixed widths, paths, some color.
        let text = match n % 4 {
            0 => format!("$ cargo build --release # step {n}"),
            1 => format!("   Compiling crate-{} v0.{}.{}", n % 97, n % 10, n % 7),
            2 => format!("/usr/local/share/project/src/module{}/file{}.rs:{}: note", n % 13, n % 29, n % 500),
            _ => format!("{n:>8}  drwxr-xr-x  wess  staff  {} bytes", n * 37 % 100_000),
        };
        for (i, ch) in text.chars().enumerate().take(cols) {
            row.cells[i].ch = ch;
        }
        if n % 6 == 0 {
            row.cells[0].fg = Color::Indexed(2);
        }
        sb.push(row);
        // Mimic the idle driver: compact as history accumulates so the hot
        // tier (and the test's memory use) stays bounded.
        if n % (4 * BLOCK_ROWS) == 0 {
            compact_fully(&mut sb);
        }
    }
    compact_fully(&mut sb);
    assert_eq!(sb.len(), limit);
    assert!(sb.hot.len() <= HOT_TARGET + BLOCK_ROWS);
    let (resident, compressed) = sb.memory();
    let raw_estimate = limit * cols * std::mem::size_of::<Cell>();
    assert!(
        compressed * 5 <= raw_estimate,
        "compressed {compressed} vs raw {raw_estimate}"
    );
    assert!(resident <= (HOT_TARGET + 2 * BLOCK_ROWS) * cols * std::mem::size_of::<Cell>());
    // Spot-check fidelity at the extremes and across a block seam.
    assert!(sb.row(0).unwrap().text().starts_with("$ cargo build"));
    assert_eq!(
        sb.row(limit - 1).unwrap().text(),
        format!("{:>8}  drwxr-xr-x  wess  staff  {} bytes", limit - 1, (limit - 1) * 37 % 100_000)
    );
}
