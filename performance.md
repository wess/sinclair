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
  placements on both screens, virtual (unicode-placeholder) placements
  included. Pixel buffers shared by storage and placement are counted once;
  4096 total stored/placed items also cap tiny-image metadata, and one image
  holds at most 512 animation frames. The store keeps a running byte total
  rather than recounting: the budget is checked on every transmit, and walking
  every frame of every image to answer it made a stream of images quadratic in
  what it retained.
- Render-image cache: 128 MiB per pane and 512 MiB across Sinclair's panes,
  evicted least-recently-used. Embedders can select another global limit with
  `ImageCachePool::new`.
- Hot, uncompressed scrollback: 4096 rows during sustained output; idle
  compaction reduces it toward 1024 rows.
- OSC retained data: 4 KiB titles, 16 KiB working directories and notification
  bodies, 512-byte notification titles, 8 MiB clipboard writes, 8 KiB link
  targets, and a 4 MiB hyperlink registry.
- Saved session buffers: `session-restore-lines` rows per pane (1000 by
  default, 50k ceiling), and 2 MiB per window shared evenly between its panes
  with 512 KiB the most any one pane may take. `Terminal::buffer_dump`
  serializes newest-row-first and stops at whichever bound comes first, so
  nothing bigger than the budget is built in memory and a tight budget keeps
  recent history rather than ancient history. A session file over 32 MiB is
  not one Sinclair wrote and is ignored rather than parsed.

## Quit and launch

Session save and restore are the only work that scales with how much you had
on screen, and both stay inside a frame's worth of time per pane. Measured on
a release build, per pane:

| Pane | Dump | Size | Replay |
|---|---|---|---|
| Dense 200x50, every run styled | 7 ms | 512 KiB (at the ceiling) | 7 ms |
| Ordinary shell, 1000 lines | 1 ms | 40 KiB | 1 ms |

Saving walks history newest-first and borrows one row at a time, so a dump
never clones the scrollback, and compressed history decodes one block per 512
rows rather than once per row. Replay happens inside `Session::spawn`, before
the reader thread starts, which is what puts restored history above the new
shell's first prompt instead of racing it — and it is wrapped in
`catch_unwind`, so a buffer the parser chokes on costs that pane's history
rather than the launch.

None of this runs unless `session-restore` is on; it is off by default.

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
- An on-screen kitty animation sleeps until its next frame is actually due
  rather than asking for a repaint every vsync, and asks for nothing once it
  has stopped. Repainting a whole pane 60 times a second to advance a frame
  every half second is the difference between an idle window and a busy core.
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
