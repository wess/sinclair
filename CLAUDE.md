# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Prompt is a GPU-accelerated terminal emulator for macOS and Linux, written in
Rust as a Cargo workspace. The GUI is built on [gpui](https://github.com/zed-industries/zed)
(pulled as a git dependency from the zed repo). The binary is `app`; the
shipped command is `prompt`.

## Commands

```sh
cargo run -p app --release        # build and launch the terminal
cargo build --release             # build the workspace
cargo test                        # run all tests (workspace)
cargo test -p vt                  # test one crate
cargo test -p vt screen           # run tests matching "screen" in one crate
cargo clippy --all-targets        # lint

scripts/bundle.sh                 # cargo build --release + assemble dist/Prompt.app
scripts/dmg.sh                    # package dist/Prompt.dmg (needs bundle first)
```

Tests live in `#[cfg(test)]` modules next to the code (no separate `tests/`
dirs). The `vt`, `config`, and `workspace` crates carry the bulk of them and
are pure logic — prefer adding coverage there.

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
- **`workspace`** — pure pane-tree + tab model: recursive splits, ratios,
  focus, directional navigation. Generic over content via opaque `PaneId`s; the
  host maps ids to terminals.
- **`input`** — keyboard/mouse encoding to terminal byte sequences (CSI, kitty
  keyboard protocol, mouse reporting, bracketed paste).
- **`config`** — Ghostty-style `key = value` config: parse → diagnostics →
  `Options` + keybindings, with live file watching. Bad lines become friendly
  diagnostics and never abort the load. Default path
  `$XDG_CONFIG_HOME/prompt/config` or `~/.config/prompt/config`.
- **`theme`** — 22 built-in color schemes (`builtin/`) plus per-color overrides.
- **`plugin`** — loads `plugin.toml` manifests contributing command actions and
  default keybindings.
- **`macros`** — record/replay of typed command sequences, stored as plain text.
- **`mcp`** — a minimal Model Context Protocol server (JSON-RPC over stdio).
  Transport/framing only; the caller supplies the tool list and handler. Knows
  nothing about terminals.
- **`assist`** — local, offline terminal assistance (command ranking,
  suggestions, paste-risk safety checks). Optional `candle` feature (on by
  default) for the candle-core backend; code is gated with
  `#[cfg(feature = "candle")]` and has a non-candle fallback.
- **`app`** — the gpui application that wires everything together. Owns windows,
  rendering, the tab bar, splits, settings UI, font handling, and the
  process-entry dispatch in `main.rs`.

### Process modes (`app/src/main.rs`)

The `prompt` binary dispatches on argv before starting the GUI:

- `prompt --toggle-quick` — signals a running instance to summon the quick
  terminal (used by Wayland compositor keybinds), then exits.
- `prompt mcp` — runs the MCP stdio server (`mcpbridge`), bridging tool calls
  into a running GUI instance.
- otherwise — loads config and launches the gpui app.

### Single-instance IPC (`app/src/ipc.rs`)

A per-user unix socket carries one newline-terminated JSON request →
response per connection. Both `--toggle-quick` and `prompt mcp` are clients;
the live GUI window is the server and does the real work. This is how the MCP
bridge and quick-terminal summon reach the running terminal.

### Event flow

`terminal::Session` emits events on a blocking std channel; `app/src/bridge.rs`
forwards them into an async `futures` stream that a gpui foreground task polls.
Keep the vt/terminal layers free of gpui types — the boundary is the bridge.

## Conventions

- Crates are layered; do not introduce upward dependencies (e.g. `vt` must not
  depend on `app`). Keep terminal emulation logic in `vt` and gpui concerns in
  `app`.
- The workspace version in the root `Cargo.toml` drives releases: pushing a
  `Cargo.toml` version bump to `main` tags and publishes a GitHub release (see
  `.github/workflows/release.yml` and `docs/release.md`).

## Docs

- `docs/roadmap.md` — built vs. planned.
- `docs/parity.md` — terminal feature coverage and known gaps.
- `docs/release.md` — signing, notarization, release cutting.
- `docs/gpui.md` — the gpui/zed dependency recipe.
