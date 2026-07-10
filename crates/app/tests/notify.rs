use super::*;

#[test]
fn scanrange_covers_only_rows_above_the_mark() {
    // 10 committed rows all still in scrollback, 5 live rows, mark at 6:
    // scan 6..14 (sequence 14 is the final live row, possibly mid-write).
    assert_eq!(scanrange(6, 10, 10, 5), (6, 14));
    // Fully caught up: empty range.
    assert_eq!(scanrange(14, 10, 10, 5), (14, 14));
}

#[test]
fn scanrange_skips_evicted_rows() {
    // committed = 100k, capped scrollback of 10k: the oldest retained
    // sequence is 90k. A stale mark clamps forward instead of walking
    // rows that no longer exist.
    let (from, end) = scanrange(50_000, 100_000, 10_000, 40);
    assert_eq!(from, 90_000);
    assert_eq!(end, 100_039);
}

#[test]
fn scanrange_survives_a_shrink_without_going_backwards() {
    // Rows shrank so the end sits below the mark: nothing to scan.
    let (from, end) = scanrange(30, 10, 5, 3);
    assert!(from >= end, "{from}..{end} must be empty");
}

#[test]
fn seqrow_resolves_scrollback_and_live_rows() {
    let mut term = vt::Terminal::new(10, 3, 100);
    for i in 0..10 {
        term.feed(format!("line{i}\r\n").as_bytes());
    }
    let committed = term.committed_lines();
    let sb_len = term.grid().scrollback().len();
    assert_eq!(committed, 8);
    // Scrollback sequence 2 is "line2"; live row 0 (sequence 8) is "line8".
    assert_eq!(seqrow(&mut term, 2, committed, sb_len).as_deref(), Some("line2"));
    assert_eq!(seqrow(&mut term, 8, committed, sb_len).as_deref(), Some("line8"));
    // Past the live grid: nothing.
    assert_eq!(seqrow(&mut term, committed + 3, committed, sb_len), None);
}

#[test]
fn seqrow_returns_none_for_evicted_rows() {
    let mut term = vt::Terminal::new(10, 3, 4);
    for i in 0..10 {
        term.feed(format!("line{i}\r\n").as_bytes());
    }
    let committed = term.committed_lines();
    let sb_len = term.grid().scrollback().len();
    assert_eq!(committed, 8);
    assert_eq!(sb_len, 4);
    // Sequences 0..4 fell off the ring.
    assert_eq!(seqrow(&mut term, 0, committed, sb_len), None);
    assert_eq!(seqrow(&mut term, 3, committed, sb_len), None);
    assert_eq!(seqrow(&mut term, 4, committed, sb_len).as_deref(), Some("line4"));
}

#[test]
fn notify_limit_drops_bursts_and_reports_them() {
    let mut limit = NotifyLimit::default();
    let t0 = Instant::now();
    assert_eq!(limit.admit(t0, "first").as_deref(), Some("first"));
    // Within the gap: suppressed.
    assert_eq!(limit.admit(t0 + Duration::from_millis(100), "second"), None);
    assert_eq!(limit.admit(t0 + Duration::from_millis(200), "third"), None);
    // Past the gap: delivered, carrying the drop count.
    assert_eq!(
        limit.admit(t0 + Duration::from_secs(2), "fourth").as_deref(),
        Some("fourth (+2 dropped)")
    );
    // Counter reset after reporting.
    assert_eq!(
        limit.admit(t0 + Duration::from_secs(4), "fifth").as_deref(),
        Some("fifth")
    );
}
