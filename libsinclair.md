# libsinclair: embedding the terminal

`crates/libsinclair` is Sinclair's terminal packaged as a library: the
session/emulation core plus the gpui renderer, without the app shell
(windows, tabs, settings, plugins, relay). The Sinclair app itself builds on
it — there is one renderer implementation, not a fork.

## What you get

- **Headless core** (always): `Session` (pty + child + reader thread),
  the full `vt` emulation, `input` key/mouse encoding, `theme` schemes,
  and the render policy modules (`colors`, `metrics`, `mouse`, `boxdraw`)
  that resolve what a frame should look like without touching a GPU.
- **`ui` feature** (default): the gpui layer — `element::TerminalElement`
  paints the grid; `termview::TermView` is a drop-in pane with the session,
  keyboard, mouse, selection, scrollback, links, and copy/paste wired up.

Deliberately not included: `config` (bring your own settings; the library
takes plain option structs), plugins, the relay mesh, auto-update, and all
window chrome.

## Consuming from another app

`libsinclair` is consumed as a git dependency (its crate names — `vt`,
`terminal`, `pty` — are not registry-publishable as-is):

```toml
[dependencies]
libsinclair = { git = "https://github.com/wess/sinclair" }

# Required: cargo patches do not propagate through git dependencies, so
# zed's gpui patches must be mirrored here (see docs/gpui.md for why).
[patch.crates-io]
async-process = { git = "https://github.com/zed-industries/async-process.git", rev = "0b6d6713570af61806e1e5cb40e0f757cb93fd9d" }
async-task = { git = "https://github.com/smol-rs/async-task.git", rev = "b4486cd71e4e94fbda54ce6302444de14f4d190e" }
```

Requires Rust stable >= 1.96. The `[patch.crates-io]` block is only needed
with the `ui` feature (it exists for gpui); a headless consumer
(`default-features = false`) skips it and gets a gpui-free tree.

If your app also depends on a crates.io `gpui` (e.g. via guise), add the
gpui redirect from `docs/gpui.md` so a single gpui resolves across the tree.

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

`TermOptions` covers appearance and behavior: colors (`Colors::from_scheme`
over any of the built-in `theme` schemes), font, padding, cursor shape,
copy-on-select, smart select, middle-click paste, macOS option-as-alt, and
OSC 52 clipboard policy. Every session `Event` (title, bell, exit, cwd,
notifications) is re-emitted by the view, so the host reacts with a normal
gpui subscription — see `crates/libsinclair/examples/embed.rs` for a complete
window that quits when the shell exits, runnable with:

```sh
cargo run -p libsinclair --example embed
```

A `TermView`'s focus listeners are subscriptions on the window it was built in,
so a host that moves one into a *different* window — tearing a pane off into
its own window, say — must call `TermView::rehome(window, cx)` as part of the
move. Until it does, the view reports the old window's focus, and can arrive in
its new home unfocused with no listener left that can mark it focused again
(the cursor stays hollow until the user clicks away and back).

Hosts that outgrow `TermView` (overlays, search UI, split dimming) keep
`TerminalElement` and build their own view around it — that is exactly what
the Sinclair app's `TerminalView` does. The element handles grid resize,
damage-aware frame reuse, selection/search/link overlays, sixel images, and
mouse reporting; the view supplies appearance, focus state, and a `CopyHook`
if it wants redaction or clipboard history layered over copy-on-select.

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
damage tracking (`term.take_damage()`) supports partial redraw. `bridge::
forward` (ui feature) adapts the blocking receiver to an async stream.
