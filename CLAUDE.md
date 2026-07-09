<!-- Keep in sync with AGENTS.md. Same content; this file is the Claude Code
     copy, AGENTS.md is the tool-neutral one. Edit both when either changes. -->

# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with
code in this repository. Other AI agents read `AGENTS.md`, which carries the
same guidance.

## What this is

Sinclair is a GPU-accelerated terminal emulator for macOS and Linux, written in
Rust as a Cargo workspace. The GUI is built on [gpui](https://github.com/zed-industries/zed)
(pulled as a git dependency from the zed repo). The GUI is the `app` crate,
whose bin target is `sinclairdev`: a dev build (`cargo run -p app`, debug or
`--release`) is named `sinclairdev` so it never collides with an installed
`sinclair` — it gets its own window title, app id, and single-instance socket and
runs side by side. The release scripts install the same binary as the shipped
`sinclair` command. The app derives this name from its own executable at runtime
(see `crates/app/src/appid.rs`).

## Commands

```sh
cargo run -p app --release        # build and launch the terminal
cargo build --release             # build the workspace
cargo test                        # run all tests (workspace)
cargo test -p vt                  # test one crate
cargo test -p vt screen           # run tests matching "screen" in one crate
cargo clippy --all-targets        # lint

scripts/bundle.sh                 # cargo build --release + assemble dist/Sinclair.app
scripts/dmg.sh                    # package dist/Sinclair.dmg (needs bundle first)
scripts/linux.sh [x86_64|aarch64] # build + package .tar.gz/.deb/.AppImage (Linux)
```

Each crate keeps its tests in a sibling `tests/` directory (e.g.
`crates/vt/tests/`), mirroring the `src/` layout. These are **not** ordinary
integration tests: every crate sets `autotests = false`, and each source file
pulls its test file back in as a private module so unit tests keep access to
private items and the `app` binary can be tested:

```rust
// at the bottom of src/foo.rs
#[cfg(test)]
#[path = "../tests/foo.rs"]
mod tests;
```

Add a new test file the same way (and create the `#[path]` stub in the source
file). Genuine integration tests that exercise only the public API are declared
explicitly as `[[test]]` targets (see `crates/vt` and `crates/terminal`). The
`vt`, `config`, and `workspace` crates carry the bulk of the coverage and are
pure logic — prefer adding there.

## gpui dependency

gpui and `gpui_platform` come from a pinned zed git rev. Because cargo
`[patch.crates-io]` entries do not propagate through git dependencies, the root
`Cargo.toml` must mirror zed's own patches (`async-process`, `async-task`).
Requires Rust stable >= 1.96. If you bump the zed rev, re-check zed's root
`Cargo.toml` patch section and update ours to match. See `docs/gpui.md`.

## Architecture

The workspace is layered bottom-up; each crate depends only on those below it.

- **`vt`** — the terminal emulation core. Pure, no I/O: a `vte`-driven parser
  feeds a `Grid`/`Screen` with cursor, modes, scrollback, selection,
  hyperlinks, search, and SGR/charset state. `term/` holds CSI/OSC/DCS dispatch
  and reports. Everything here is testable in isolation.
- **`pty`** — Unix pty allocation and child-process spawn (`rustix`). Unix-only.
- **`terminal`** — runtime glue: `Session::spawn` runs a child on a pty, feeds
  its bytes into a `vt::Terminal` on a reader thread, and emits `Event`s
  (wakeup, title, bell, exit) over a std channel.
- **`cast`** — asciinema v2 `.cast` recording: a `Recorder` writes a header line
  plus timestamped output events as bytes arrive (output only; UTF-8 split
  across reads is carried over). Used by `terminal` for session capture.
- **`workspace`** — pure pane-tree + tab model: recursive splits, ratios,
  focus, directional navigation. Generic over content via opaque `PaneId`s; the
  host maps ids to terminals.
- **`input`** — keyboard/mouse encoding to terminal byte sequences (CSI, kitty
  keyboard protocol, mouse reporting, bracketed paste).
- **`config`** — `key = value` config: parse → diagnostics →
  `Options` + keybindings, with live file watching. Bad lines become friendly
  diagnostics and never abort the load. Default path
  `$XDG_CONFIG_HOME/sinclair/config` or `~/.config/sinclair/config`.
- **`theme`** — 22 built-in color schemes (`builtin/`) plus per-color overrides.
- **`plugin`** — parses `plugin.toml` manifests contributing: `[[command]]`
  actions (+ default keybindings), `[runtime]`/`[panel]` IPC block-tree panels,
  `[webview]` HTML/JS surfaces (panel/window/tab), `[[trigger]]` event hooks,
  and `[[tool]]` MCP tools exposed to agents (`mcpbridge` merges them into
  `sinclair mcp`'s tool list, routing calls to the runtime). Plugins declare
  `capability = "…"` (advisory under the process runtime; the vocabulary the
  WASM runtime enforces) and a `[runtime] type` of `process` (default) or `wasm`
  (declaration + design only so far — see `docs/plugins-wasm.md`). Pure
  parsing/validation; the host (`app`) drives the runtime, renders
  panels/webviews (via the `WebviewSurface`-based `PluginWebView`, shared with
  built-ins like Notes), and dispatches triggers.
- **`macros`** — record/replay of typed command sequences, stored as plain text.
- **`mcp`** — a minimal Model Context Protocol server (JSON-RPC over stdio).
  Transport/framing only; the caller supplies the tool list and handler. Knows
  nothing about terminals.
- **`assist`** — local, offline terminal assistance (command ranking,
  suggestions, paste-risk safety checks). Optional `candle` feature (on by
  default) for the candle-core backend; code is gated with
  `#[cfg(feature = "candle")]` and has a non-candle fallback.
- **`updater`** — self-update mechanics, gpui-free (Zed's `auto_update`
  design): GitHub release check, install detection, and **in-place** installs —
  macOS mounts the `.dmg` and rsyncs the new bundle's contents onto the
  installed `.app` (never swap the bundle directory: LaunchServices' stale
  registration relaunches the bare Mach-O inside Terminal.app), Linux renames
  the new AppImage over the running one, staged on the same filesystem. Returns
  a `Relaunch` decision that `app`'s `updateui.rs` hands to gpui's restart —
  `Relaunch::Current` restarts with *no* explicit path so gpui reopens the
  running bundle via `NSBundle`.
- **`relay`** — the agent mesh, shipped as a standalone sidecar binary
  (`relay`), **not** part of the terminal. Lets independent coding-agent
  sessions (Claude Code, Codex, …) coordinate over a shared SQLite bus: agents
  `register`, message each other / channels, and `wait` (a single blocking SSE
  call) to park for free between tasks. Built on tokio + axum + sqlx; MCP
  transport is Streamable HTTP so many sessions share one server. Submodules:
  `protocol/` (wire types), `db/` (SQLite bus), `state/` (in-memory app +
  wake signal), `bus.rs` (core park/deliver shared by both planes), `tools/`
  (MCP tool impls), `mcp/` + `transport/` (MCP dispatch over HTTP), `control/`
  (plain-HTTP control plane for the CLI and non-MCP bridges), `spawn/`
  (background workers), and `cli/` (the `relay` subcommands). The app never runs
  the mesh in-process — `app/src/relay.rs` starts/stops the bundled binary as a
  detached daemon and launches agents into splits. Reads no env vars; every
  parameter comes from settings, passed explicitly. See `docs/relay.md`.
- **`app`** — the gpui application that wires everything together. Owns windows,
  rendering, splits, settings UI, the About panel, font handling, and the
  process-entry dispatch in `main.rs`. The window opens with a transparent
  native title bar; `titlebar.rs` draws the chrome itself — a themed strip with
  the tabs folded in (`tabbar.rs`), window dragging, and, on Linux, custom
  minimize/maximize/close controls plus resize edges.

### Process modes (`app/src/main.rs`)

The `sinclair` binary dispatches on argv before starting the GUI:

- `sinclair --toggle-quick` — signals a running instance to summon the quick
  terminal (used by Wayland compositor keybinds), then exits.
- `sinclair mcp` — runs the MCP stdio server (`mcpbridge`), bridging tool calls
  into a running GUI instance.
- `sinclair notify [--title T] <message>` — posts a desktop notification, for
  agent hooks that can't emit an OSC 9/777/99 escape themselves.
- otherwise — loads config and launches the gpui app.

### Single-instance IPC (`app/src/ipc.rs`)

A per-user unix socket carries one newline-terminated JSON request →
response per connection. Both `--toggle-quick` and `sinclair mcp` are clients;
the live GUI window is the server and does the real work. This is how the MCP
bridge and quick-terminal summon reach the running terminal.

### Event flow

`terminal::Session` emits events on a blocking std channel; `app/src/bridge.rs`
forwards them into an async `futures` stream that a gpui foreground task polls.
Keep the vt/terminal layers free of gpui types — the boundary is the bridge.

## Working in this repo

- In this project Claude has full authority to run git and everything else —
  branching, committing, pushing, tagging, cutting releases, and any other
  operation needed to move the work forward. The owner's usual "I handle git"
  rule does not apply here; act directly.
- Commit messages, PRs, and release notes must never mention Claude, Claude
  Code, or Anthropic, and carry no AI/co-author trailer.
- Releases ship straight from `main`: a workspace version bump committed and
  pushed to `main` is the release (see the version convention below). Run the
  full gate first — `cargo test`, `cargo clippy --all-targets`, and a release
  build — before pushing a version bump.

## Conventions

- Crates are layered; do not introduce upward dependencies (e.g. `vt` must not
  depend on `app`). Keep terminal emulation logic in `vt` and gpui concerns in
  `app`.
- The workspace version in the root `Cargo.toml` drives releases: pushing a
  `Cargo.toml` version bump to `main` tags and publishes a GitHub release with
  the macOS `.dmg` and Linux `.tar.gz`/`.deb`/`.AppImage` (x86_64 + aarch64),
  and updates the Homebrew cask (see `.github/workflows/release.yml` and
  `docs/release.md`).
- Linux-only code (`linux.rs`, the `#[cfg(target_os = "linux")]` blocks in
  `titlebar.rs`/`main.rs`) is not compiled on the macOS dev host; validate it
  with the `Linux Build` workflow (`.github/workflows/linux.yml`, runs on PRs).

## Docs

- `docs/roadmap.md` — built vs. planned.
- `docs/parity.md` — terminal feature coverage and known gaps.
- `docs/release.md` — signing, notarization, release cutting.
- `docs/gpui.md` — the gpui/zed dependency recipe.
- `docs/guise.md` — the guise component-library migration: how `vendor/guise` is
  wired (the single-gpui patch), the theme bridge, and the surface-by-surface
  port status.
- `docs/relay.md` — the agent mesh: roles, teams/tiles, the `relay` CLI, and the
  MCP coordination tools.
