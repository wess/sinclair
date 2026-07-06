# Authoring a WASM plugin

A WASM plugin is a sandboxed capability module: it can expose tools to the user
and to agents, react to events, and own a panel — reaching only the host
functions its declared capabilities grant. It has no runtime dependency (no
`bun`/`node`) and runs in-process.

Use the WASM tier for anything that works through the host functions (read the
screen, drive the terminal, fetch, scoped files, computed tools). Use the
**native tier** (`[runtime] type = "native"`, a subprocess) for plugins that must
spawn processes and read their output — `git`, `docker`, `sysinfo` are native by
nature. See `docs/pluginsv2.md`.

## Rust (recommended)

1. Copy `sdk/rust/` and rename it.
2. Edit `src/lib.rs` — implement `call_tool` (and `render` for a panel).
3. Build a component:

   ```sh
   rustup target add wasm32-wasip2      # once
   cargo build --target wasm32-wasip2 --release
   ```

   The `wasm32-wasip2` target's linker emits a component directly — no external
   tooling.

4. Ship `target/wasm32-wasip2/release/<name>.wasm` as `plugin.wasm` next to a
   `plugin.toml`.

### Capabilities and the world

Your plugin's WIT *world* imports only the interfaces it uses; that is what makes
gating precise. The template's world is `screentools` (core + screen). To use
more, add the interface to a world in `crates/pluginrt/wit/plugin.wit` (or your
own copy) and declare the matching `capability` in the manifest:

| host interface | capability | gives |
|---|---|---|
| `host-core` | (always) | `log`, `storage` |
| `host-commands` | `commands` | run a command / send input to the terminal |
| `host-screen` | `screen` | read the visible screen, the selection |
| `host-net` | `network` | `fetch` |
| `host-fs` | `filesystem` | read/write files (scoped to the plugin dir) |
| `host-clipboard` | `clipboard` | read/write the clipboard |
| `host-notify` | `notify` | desktop notification |

A plugin that imports an interface it was not granted **fails to instantiate** —
the boundary is enforced by the runtime, not by convention.

## Manifest

```toml
id = "screentools"
name = "Screen Tools"
version = "0.1.0"
capabilities = ["screen"]

[runtime]
type = "wasm"
wasm = "plugin.wasm"

[[tool]]                 # callable from the palette and by agents over MCP
id = "grep"
description = "Search the visible screen."
[[tool.param]]
name = "query"
type = "string"

[panel]                  # optional: a side-drawer panel your render() draws
id = "screentools"
title = "Screen Tools"
```

## JavaScript / TypeScript

Authoring in JS against the same WIT via `componentize-js` — so today's `bun`/TS
authors keep their language but ship a self-contained `.wasm` — is the next
addition; it needs the `componentize-js` toolchain wired into the build. Until
then, use the Rust template or the native tier.
