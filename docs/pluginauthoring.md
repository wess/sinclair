# Authoring a WASM plugin

Every plugin is a WASM component: a sandboxed capability module that can expose
tools to the user and to agents, react to events, and own a panel — reaching
only the host functions its declared capabilities grant. There is one runtime
and no runtime dependency (no `bun`/`node`); a plugin's instance stays resident,
so a call costs microseconds rather than a process spawn.

A plugin that needs to read a program's output — `git status`, `docker ps`,
`df` — declares the `process` capability and calls `exec`. The **host** does the
spawning and hands back the output, so the plugin never holds the privilege to
run anything itself, and the host is the one enforcing the timeout and the
output cap. The bundled `git`, `docker` and `sysinfo` plugins are built this
way; read them for a working example.

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
| `host-process` | `process` | run a program and read its output (`exec`) |

A plugin that imports an interface it was not granted **fails to instantiate** —
the boundary is enforced by the runtime, not by convention.

Ask for the narrowest capability that does the job. `network` exists so a plugin
that only reads a URL doesn't have to take `process` (which would let it run
anything); `process` exists so a plugin that shells out doesn't need a broad
filesystem grant. The pre-built worlds in `crates/pluginrt/wit/plugin.wit` —
`screentools`, `inspector`, `designer` — are the common combinations.

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

## Panels

`render()` returns a JSON node tree the host paints with guise components:
`section`, `text`, `divider`, `kv`, `badge`, `button`, and `row` to group them.
A `text` node takes an optional palette `color` and a `mono` flag for content
that is literally terminal output (a prompt, a path, a command). An unrecognized
node renders an inline notice rather than blanking the panel, so a newer block
type degrades instead of failing.

A button click arrives at `on_ui_event` with the button's `id`; update your state
and the host re-renders. To have the terminal do something — open a monitor,
show a log — call `run-command` rather than describing it: the call is queued
during your event and dispatched once you return.

State persists in the ungated per-plugin key/value store (`storage-get` /
`storage-set`), which is what makes a resident panel able to remember anything
between a render and a click. Treat what comes back out of it as untrusted —
re-validate before interpolating it anywhere.

## JavaScript

Author against the same WIT and build to a component with `componentize-js` — so
`bun`/TS authors keep their language but ship a self-contained `.wasm` with no
runtime dependency.

1. Copy `sdk/js/` and `npm install`.
2. Edit `plugin.js` — export a `guest` object (`init`, `callTool`, `render`,
   `onUiEvent`). Import host functions from their versioned interface, e.g.
   `import { readScreen } from 'prompt:plugin/host-screen@0.1.0'`.
3. `npm run build` → `plugin.wasm`.

The build **disables the engine's WASI http/fetch** (`disableFeatures: ['http',
'fetch-event']` in `build.mjs`) so a JS plugin reaches the network only through
the gated `host-net`, keeping the capability boundary. The component is ~12 MB
(it embeds the JS engine); the Rust path produces far smaller modules.
