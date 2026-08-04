# Performance and resource budgets

Sinclair's steady state is event-driven: a quiet terminal has no repaint loop,
no config polling loop, and no per-window bridge thread. PTY output is parsed on
the session reader thread, while UI work is coalesced to one pass per 16 ms.

## Built-in counters

`terminal::Session::stats()` returns cumulative output bytes/chunks, emitted and
acknowledged wakeups, committed resizes, accepted input bytes, dropped metadata
events, and the current queued-input gauge.
`libsinclair::element::SnapCache::stats()` reports frames, whole-snapshot reuse,
resolved rows, and shaped rows. The related memory gauges are
`vt::Terminal::scrollback_memory()`, `vt::Terminal::graphics_memory()`, and
`ImageCachePool::stats()`.

These counters make the useful ratios explicit:

- `wakeups / output_chunks` measures event coalescing.
- `snapshot_rows / frames` and `shaped_rows / frames` measure renderer reuse.
- `resize_commits` should rise once, not once per pixel, for a window-resize
  gesture.

## Hard budgets

- Pending Unix PTY input: 16 MiB per session; larger queued writes return
  `WouldBlock` without partially enqueueing data.
- Session events: eight metadata messages, plus reserved slots for the one
  coalesced redraw wakeup and eventual exit. With the OSC 52 payload limit,
  queued metadata retains at most 64 MiB.
- Decoded terminal graphics: 128 MiB per pane across kitty storage and image
  placements on both screens. Pixel buffers shared by storage and placement are
  counted once; 4096 total stored/placed items also cap tiny-image metadata.
- Render-image cache: 128 MiB per pane and 512 MiB across Sinclair's panes,
  evicted least-recently-used. Embedders can select another global limit with
  `ImageCachePool::new`.
- Hot, uncompressed scrollback: 4096 rows during sustained output; idle
  compaction reduces it toward 1024 rows.
- OSC retained data: 4 KiB titles, 16 KiB working directories and notification
  bodies, 512-byte notification titles, 8 MiB clipboard writes, 8 KiB link
  targets, and a 4 MiB hyperlink registry.

## CPU controls

- Unchanged split panes use gpui's cached view path.
- VT row revisions let snapshots and shaped text survive both small row damage
  and full-screen scroll rotation. Only new or dirty rows are rebuilt/shaped.
- Output wakeups use one async-capable flume receiver directly; there is no
  forwarding thread or duplicate queue. A generation handshake prevents lost
  output while still limiting presentation to 60 Hz.
- Search refresh is capped at 10 Hz during streaming output. Filesystem/history
  suggestion ranking runs on the background executor and reuses unchanged
  candidates.
- Resize gestures settle for 80 ms before one full-history reflow. Initial
  layout remains immediate.
- Config reload uses the OS filesystem event backend with trailing-edge
  debounce and only falls back to a sleeping mtime poll when no watch can be
  installed.
- Parking Quick Terminal retains its shell/view but removes the native window,
  releasing compositor and GPU surface resources.

## Repeatable checks

Run the timing scenarios in release mode:

```sh
cargo test -p vt --release --test throughput -- --ignored --nocapture
```

The command reports parser throughput, one settled large-history reflow, and
scrollback memory before/after idle compaction. Functional resource regressions
remain in the normal test suite, including hot-history high water, graphics and
hyperlink budgets, wakeup coalescing, resize coalescing, texture LRU eviction,
and stalled-input bounds.

For an end-to-end macOS sample, launch the release app, record its PID, and use
`ps -o rss=,%cpu= -p PID` after one minute idle and during a fixed output command
(for example `yes | head -n 500000`). Compare the built-in counters alongside
RSS/CPU so a lower repaint rate is not mistaken for lower parser throughput.
