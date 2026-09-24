# guise migration

Sinclair's chrome (everything that isn't the terminal grid) is migrating onto
[guise](https://github.com/wess/guise), our gpui component library, vendored as
a submodule at `vendor/guise` so we can co-evolve it.

## Why the submodule tracks a compatibility branch

The submodule tracks **`sinclair-v1.5.3`**, based directly on guise v1.5.3.
That release targets crates.io `gpui 0.2.2`, while Sinclair still needs zed rev
`96285fc1` because `gpui_platform` is not published separately. The versions
match, but the APIs do not: focus, scrolling, text painting, async updates, and
style refinement differ between those snapshots.

The branch keeps the 1.5.3 component surface and carries only what Sinclair
needs on top:

- compatibility shims for the pinned gpui API, including runnable guise tests
  and examples against that same revision;
- `SplitRequested { pane, axis, first }` and structured
  `TearOff { item, drop: Option<TearDrop> }` events;
- Sinclair's tab sizing, overflow, reorder motion, drop affordances, tear-off
  preview, per-pane split controls, and context-menu behavior.

It also preserves the v1.5.3 layout snapshot/restore API and devtools probe,
including the text-overflow fixes for inputs, labels, picker values, table
cells, and bounded text areas.
This is a released Guise baseline with a pinned-gpui compatibility commit, not
the old divergent `sinclair-panegroup` line.

## How it's wired

- `vendor/guise` is a git submodule (its own cargo workspace, `exclude`d from
  ours). `crates/app` depends on `guise-ui` by path with `features =
  ["webview"]`, so the `wry` backend is built in.
- guise declares `gpui 0.2.2`; Sinclair's root `[patch.crates-io]` redirects it
  to the pinned zed rev, so the whole tree shares **one** gpui (verify with
  `cargo tree -d`). The compatibility branch mirrors that patch so its own
  tests and examples exercise the same API. When the zed rev moves, update
  `sinclair-v1.5.3`, run the guise gate, push it, and re-pin this submodule.
- `crates/app/src/guisetheme.rs` derives a `guise::Theme` from the active
  terminal palette (body/text/surface/border/dimmed/primary) and installs it as
  the gpui global at boot and on every live config reload, so guise components
  track the terminal theme. The terminal cell grid keeps its own renderer.

## Component mapping

| Sinclair surface            | guise component                         |
| ------------------------- | --------------------------------------- |
| About panel               | `Title` / `Text` / `Anchor`             |
| Plugin side panels        | `Title` / `Text` / `Badge` / `Button` / `Divider` |
| Command palette           | `Spotlight` (+ `Kbd`)                   |
| Side drawers              | `Drawer` / `List` / `Table`             |
| Settings controls         | `TextInput` / `Select` / `Switch` / `Slider` / `Field` |
| Text input widget         | `TextInput` / `TextArea`               |
| Notification banners      | `Notification` / `ToastStack`           |
| Tab-overflow dropdown     | `Menu` / `Popover`                      |
| Rename / save-layout / new-agent dialogs | `Modal`                  |
| Title/status strip        | `StatusBar`                             |
| App menu bar (where used) | `MenuBar`                               |

## Status

Done:

- guise integrated with a single unified gpui; theme bridge with live reload.
- About panel ported to guise.
- IPC plugin side panels render entirely with guise components.
- Side drawers' typography (section headers, empty-state notes) on guise `Text`.
- cmd+P quick-open is a guise `Spotlight` (commands + plugin commands + panels);
  see `root/quickopen.rs`.
- cmd+shift+P command palette now opens the same in-window `Spotlight`; the
  legacy standalone-window `palette.rs` is deleted (`root/quickopen.rs` shares
  one `open_spotlight` helper).
- The bespoke `textedit` widget and its `textkeys` dispatcher are **gone**.
  guise owns the single-line text model and keyboard handling now:
  `guise::TextEdit` (the model), `guise::apply_key` + `guise::KeyOutcome` (the
  macOS/Linux key map — Option=word, Cmd=line, Ctrl+A/E/K). `guise::TextInput`
  routes through `apply_key`; inline fields (terminal search, assist, help
  search, settings) drive `guise::TextEdit` + `apply_key` directly.
- The `rename` / save-layout and `newagent` dialogs are in-window guise `Modal`
  overlays (no longer separate OS windows): `rename.rs` (`Modal` + `TextInput`)
  and `newagent.rs` (`Modal` + `Select` / `TextInput` / `SegmentedControl` /
  `Button`). `WorkspaceView` hosts them via a `modal: Option<AnyView>` slot;
  opening/closing lives in `root/dialogs.rs`.

Remaining (incremental, surface by surface; keep the build green and behavior
identical at each step):

- Settings panel *controls* → guise inputs (`settings/ui/*`). The text editing
  is already on guise; the rows/toggles/steppers are still bespoke.
- Notifications → `ToastStack` (net-new in-window toasts; today notifications
  are OS-level via OSC 9/777/99).
- Tab-overflow / trailing `+` / split menus → guise `Menu` (currently bound to
  the custom titlebar state).
- An app menu bar where one is wanted → guise `MenuBar` (e.g. the Linux
  titlebar, which has no native menu).

## Porting a surface

1. Build with guise components from `guise::prelude`; read colors from the
   installed `guise::Theme` (don't hardcode).
2. guise event handlers hand back `&mut App` (not the view `Context`); capture
   `cx.entity()` and call `entity.update(app, |this, cx| …)` to mutate the view
   (see `root/pluginpanel.rs`).
3. Keep behavior and keybindings identical; port presentation only.
4. `cargo check -p app`, then verify the surface in a running window.
