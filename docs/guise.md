# guise migration

Prompt's chrome (everything that isn't the terminal grid) is migrating onto
[guise](https://github.com/wess/guise), our Mantine-inspired gpui component
library, vendored as a submodule at `vendor/guise` so we can co-evolve it.

## How it's wired

- `vendor/guise` is a git submodule (its own cargo workspace, `exclude`d from
  ours). `crates/app` depends on `guise-ui` by path with `default-features =
  false` (no `wry`/webview backend).
- guise tracks crates.io `gpui 0.2.2`; Prompt builds gpui from a pinned zed git
  rev. The root `[patch.crates-io] gpui = { git = …, rev = … }` redirects
  guise's gpui onto our exact rev, so the whole tree shares **one** gpui
  (verify with `cargo tree -d`). When the zed rev is bumped, guise is rebuilt
  against it on the `prompt-gpui-port` branch and re-pinned.
- `crates/app/src/guisetheme.rs` derives a `guise::Theme` from the active
  terminal palette (body/text/surface/border/dimmed/primary) and installs it as
  the gpui global at boot and on every live config reload, so guise components
  track the terminal theme. The terminal cell grid keeps its own renderer.

## Component mapping

| Prompt surface            | guise component                         |
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

## Status

Done:

- guise integrated with a single unified gpui; theme bridge with live reload.
- About panel ported to guise.
- IPC plugin side panels render entirely with guise components.

Remaining (incremental, surface by surface; keep the build green and behavior
identical at each step):

- Command palette → `Spotlight` (stateful entity; restructure `palette.rs`).
- Settings panel controls → guise inputs (`settings/ui/*`).
- Custom `textedit` widget → `TextInput` / `TextArea`.
- Dialogs (`rename`, save-layout, `newagent`) → `Modal`.
- Built-in sidebar panels' typography → guise `Text` / `Title`.
- Notifications → `ToastStack`; tab-overflow menu → `Menu`.

## Porting a surface

1. Build with guise components from `guise::prelude`; read colors from the
   installed `guise::Theme` (don't hardcode).
2. guise event handlers hand back `&mut App` (not the view `Context`); capture
   `cx.entity()` and call `entity.update(app, |this, cx| …)` to mutate the view
   (see `root/pluginpanel.rs`).
3. Keep behavior and keybindings identical; port presentation only.
4. `cargo check -p app`, then verify the surface in a running window.
