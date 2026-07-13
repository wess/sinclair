# libsinclair

[Sinclair](https://github.com/wess/sinclair)'s terminal as an embeddable
library: the session/emulation core plus the gpui renderer, without the app
shell (windows, tabs, settings, plugins). The Sinclair app itself builds on
this crate — there is one renderer implementation, not a fork.

## What you get

- **Headless core** (always): `Session` (pty + child process + reader
  thread), the full `vt` emulation (grid, screens, modes, scrollback,
  selection, search, hyperlinks, sixel), `input` key/mouse encoding (legacy +
  kitty keyboard protocol), `theme` with 22 built-in schemes, and the render
  policy modules (`colors`, `metrics`, `mouse`, `boxdraw`) that resolve what
  a frame should look like without touching a GPU.
- **`ui` feature** (default): the gpui layer — `element::TerminalElement`
  paints the grid (damage-aware frame reuse, selection/search/link overlays,
  sixel images, custom box-drawing geometry), and `termview::TermView` is a
  drop-in terminal pane with the session, keyboard, mouse, scrollback,
  selection, and copy/paste wired up.

Deliberately not included: settings/config loading (the library takes plain
option structs), plugins, the relay agent mesh, auto-update, window chrome.

## Install

Consumed as a git dependency:

```toml
[dependencies]
libsinclair = { git = "https://github.com/wess/sinclair" }

# Required with the `ui` feature: cargo patches do not propagate through
# git dependencies, so zed's gpui patches must be mirrored here.
[patch.crates-io]
async-process = { git = "https://github.com/zed-industries/async-process.git", rev = "0b6d6713570af61806e1e5cb40e0f757cb93fd9d" }
async-task = { git = "https://github.com/smol-rs/async-task.git", rev = "b4486cd71e4e94fbda54ce6302444de14f4d190e" }
```

Requires Rust stable >= 1.96. A headless consumer
(`default-features = false`) gets a gpui-free tree and skips the patch block.

## A terminal pane in a gpui app

```rust
use gpui::AppContext as _;
use libsinclair::terminal::{Event, SessionOptions};
use libsinclair::termview::{TermOptions, TermView};

let window = cx.open_window(options, |window, cx| {
    cx.new(|cx| {
        TermView::spawn(SessionOptions::default(), TermOptions::default(), window, cx)
            .expect("spawn shell")
    })
})?;
```

`TermOptions` covers appearance and behavior: colors
(`Colors::from_scheme` over any `theme` scheme), font, padding, cursor
shape, copy-on-select, smart select, middle-click paste, macOS
option-as-alt, and OSC 52 clipboard policy. The view re-emits every session
`Event` (title, bell, exit, cwd, notifications), so the host reacts with a
normal gpui subscription.

A complete runnable window lives in [`examples/embed.rs`](examples/embed.rs):

```sh
cargo run -p libsinclair --example embed
```

Hosts that outgrow `TermView` (overlays, search UI, split dimming) keep
`TerminalElement` and build their own view around it — exactly what the
Sinclair app does. The element handles grid resize, frame reuse, selection
and search overlays, and mouse reporting; the view supplies appearance,
focus state, and a `CopyHook` to layer redaction or clipboard history over
copy-on-select.

## Headless embedding

```rust
use libsinclair::{Session, SessionOptions};

let (session, events) = Session::spawn(SessionOptions::default())?;
// Event::Wakeup on the receiver -> read the grid under the terminal lock:
session.with_term(|term| { /* term.visible_row(..), term.cursor_pos(), .. */ });
session.write(b"ls\r")?;
```

The embedder contract: `with_term` locks the terminal; `Wakeup` events are
coalesced (at most one in flight until the terminal is next locked); vt's
damage tracking (`term.take_damage()`) supports partial redraw.

## More

See [`docs/libsinclair.md`](../../docs/libsinclair.md) in the repository
root for the full embedding guide, and [`llm.txt`](llm.txt) for a compact
API reference aimed at coding agents.

Licensed under Apache-2.0, like the rest of the workspace.
