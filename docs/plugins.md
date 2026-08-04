# The plugin system

One runtime, one contract. A plugin is a WASM component that declares what it
contributes and what it needs; the host instantiates it with exactly the host
functions its granted capabilities allow, keeps it resident, and calls it.

For writing one, see `docs/pluginauthoring.md`. This describes how the machinery
fits together.

## Shape

```
  plugin.toml ─────▶ crates/plugin        manifest, install state, discovery
                     (pure, no wasmtime)
                            │  Plugin + contributions
                            ▼
  plugin.wasm ─────▶ crates/pluginrt      wasmtime engine, WIT world,
                     defines `AppHost`    capability linker, resident stores
                            │  implements AppHost
                            ▼
                     crates/app           panels, tools, triggers, the
                                          host functions themselves
```

`pluginrt` keeps `wasmtime` out of `app`'s dependency surface, and the layering
holds the usual direction: `plugin` ← `pluginrt` ← `app`.

## Contributions

A manifest may declare:

- **`[[tool]]`** — the spine. One implementation reachable three ways: the
  command palette, an agent over `sinclair mcp` (merged in as
  `<plugin>_<tool>`), and a `[[trigger]]` with `do = { invoke = "…" }`, which
  calls the tool of that name with the event payload. An author writes one
  handler, not three.
- **`[[command]]`** — a shell command with a mode and an optional default
  keybinding, resolved through the normal keymap pipeline.
- **`[panel]`** — a side-drawer panel. `render()` returns a JSON node tree the
  host paints with guise components; `on_ui_event` receives button clicks. A bad
  node renders an inline notice instead of blanking the panel.
- **`[[trigger]]`** — a terminal event hook with a typed `when` filter and a
  `do` of `run`, `notify`, or `invoke`.

## Capabilities

Capabilities are enforced by the linker, not by convention. The host adds a host
interface to the linker only if the plugin declared it **and** the user granted
it at install; a component that imports something ungranted fails to
instantiate. `effective_capabilities` intersects declared with granted, so a
plugin cannot widen its reach in an update without re-consent.

| interface | capability | gives |
|---|---|---|
| `host-core` | (always) | `log`, per-plugin `storage` |
| `host-commands` | `commands` | run a command / send input to the terminal |
| `host-screen` | `screen` | read the visible screen, the selection |
| `host-net` | `network` | `fetch` |
| `host-fs` | `filesystem` | read/write files, scoped to the plugin dir |
| `host-clipboard` | `clipboard` | read/write the clipboard |
| `host-notify` | `notify` | desktop notification |
| `host-process` | `process` | run a program and read its output (`exec`) |

The vocabulary is deliberately fine-grained so a plugin can ask for the narrow
thing. `network` exists so a plugin that only reads a URL need not take
`process`; `process` exists so a plugin that shells out need not take a broad
filesystem grant.

`exec` is the one that lets a sandboxed plugin do what the old subprocess tier
existed for. The guest passes a program and its arguments as separate values and
the **host** spawns it — never through a shell, in the focused pane's directory
unless told otherwise, killed at 15s, output truncated at 4 MiB. So the plugin
describes the work; the host holds the privilege and the limits.

## The host functions, and the two hosts

`AppHost` is implemented twice, because there are two processes:

- `app/src/wasmhost.rs` — for `sinclair mcp`, which has no window. Terminal
  operations route to the running GUI over the single-instance socket.
- `app/src/guiwasm.rs` — for a workspace window, which does.

Context-free operations (`exec`, `fetch`, storage, path scoping) live in
`wasmhost.rs` and both hosts share them.

The in-window host cannot hold the gpui context across a guest call, so anything
needing it is deposited before and drained after: the caller sets the focused
cwd and the current clipboard, and collects queued `run-command` directives and
clipboard writes once the guest returns. That is why a panel's button handler
calls `run-command` rather than returning a directive — the queue is the seam.

## Bounds

A guest gets a fuel budget per call, refilled each time, so an infinite loop
traps instead of freezing the caller. Fuel doesn't help while the guest is
parked inside a host call, which is why `exec` and `fetch` carry their own
deadlines.

## Installation

`installed.toml` records id → version, source, enabled flag, and granted
capabilities. `plugin::load` skips disabled plugins. First-party plugins bundled
beside the binary are discovered from the `.app` bundle, a Linux prefix, a
portable directory, and — in debug builds — the workspace `plugins/` directory,
so `cargo run` picks them up without a bundle. A user-installed plugin of the
same id overrides a bundled one.

Catalog installs are not yet signature- or checksum-verified; that waits on a
real pinned index.

## Web views

Plugins do not contribute web views. Notes is a first-party surface backed by
the bundled `notes` sidecar: the host reserves a loopback port, mints a token,
spawns the binary with both in its environment, verifies readiness with a token
challenge, and reaps the child when the last tab using it closes
(`app/src/sidecar.rs`). The page talks to its own server over that
authenticated origin, so it needs no privileged channel into the app.

## Open

- **Filesystem scope.** `filesystem` is scoped to the plugin's own directory,
  which is too narrow for anything that manages a config file elsewhere. A
  manifest-declared path grant, consented to at install, is the obvious answer
  and is not built.
- **Signing.** A first-party key with a detached signature, or an existing
  format (sigstore/minisign)? Undecided; catalog installs are unverified until
  it lands.
- **JS components.** `componentize-js` works and `sdk/js/` builds a loadable
  plugin, but the component embeds a JS engine and lands around 12 MB. Rust
  stays the recommended path.
