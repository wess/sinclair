# WebAssembly plugin runtime (design)

Status: **foundation landed, engine in progress.** The manifest surface
(`[runtime] type = "wasm"`) and the capability vocabulary
(`capability = "…"`) ship today and validate; the execution engine described
here is the next build. This document is the spec it is built against.

## Why

The process `[runtime]` — a subprocess spoken to over JSON on stdin/stdout — has
three problems that block a trustworthy plugin ecosystem:

1. **Runtime dependency.** It needs whatever interpreter its `command` names
   (bun, node, python). That is fragile (a GUI launch's bare `PATH`) and a heavy
   ask for a shipped plugin.
2. **No sandbox.** A subprocess runs with the user's full privileges. You cannot
   safely install a stranger's plugin.
3. **Serverless latency.** A fresh process per event; no persistent state, no
   push.

A WebAssembly runtime fixes all three: modules run **in-process** (no
dependency), **sandboxed** to exactly the host functions they are granted (real
enforcement of the declared capabilities), in **any language** that targets
wasm (Rust, JS via `componentize-js`, Go, Zig), and can be **persistent**.

## Manifest

```toml
[runtime]
type = "wasm"          # vs the default "process"
wasm = "plugin.wasm"   # module path, relative to the plugin dir

capability = "screen"  # the host functions this module may import
capability = "network"
```

Declaring `type = "wasm"` and a `wasm` path is supported now (parsed, validated,
surfaced). Until the engine lands, invoking such a plugin returns a clear
"not yet executable" error rather than failing obscurely.

## Engine

Host: [`wasmtime`] with the **component model**. The host↔guest contract is a
WIT world; the guest is a component, the host provides the imports.

```wit
// prompt:plugin/host — the capability-gated imports the host offers.
interface host {
  run-command: func(text: string, target: string) -> result<string, string>;  // cap: commands
  read-screen: func(lines: u32) -> result<string, string>;                     // cap: screen
  fetch: func(url: string) -> result<string, string>;                          // cap: network
  read-file: func(path: string) -> result<string, string>;                     // cap: filesystem
  notify: func(title: string, body: string);                                   // cap: notify
  storage-get: func(key: string) -> option<string>;                            // (ungated)
  storage-set: func(key: string, value: string);
}

// prompt:plugin/guest — what a plugin exports.
interface guest {
  // Handle an MCP tool call; returns a JSON result string.
  handle-tool: func(name: string, params-json: string) -> result<string, string>;
  // Render a panel to the block-tree JSON the sidebar already consumes.
  render: func(request-json: string) -> string;
  // React to a subscribed terminal/agent event.
  on-event: func(event-json: string) -> string;
}

world plugin {
  import host;
  export guest;
}
```

### Capability enforcement

The linker only supplies an import if the plugin's manifest declares the
matching `capability`. A module without `capability = "network"` cannot import
`fetch` — the component fails to instantiate. That is the enforcement the
process runtime can't provide: capabilities stop being advisory and become the
actual boundary. `storage-*` is a host-provided per-plugin key/value store
(scoped to the plugin id), which also gives WASM plugins the persistent state
the serverless model lacks.

### Lifecycle

One `Store` + instantiated component per plugin, created lazily on first use and
kept resident (persistent state, no per-event spawn). Fuel/epoch interruption
bounds runaway guests (the analogue of the process runtime's 15s SIGKILL).
Host calls are plain function calls across the boundary — no IPC, no JSON on a
pipe — so a tool call is microseconds of overhead, not a process spawn.

## Authoring

- **Rust:** `cargo build --target wasm32-wasip2` with `wit-bindgen` for the
  `guest` bindings.
- **JS/TS:** author against the same WIT and run `componentize-js` to produce the
  component — so the current bun/JS plugin authors have a migration path that
  keeps their language.
- Ship the built `.wasm` in the plugin dir (like the prebuilt `web/dist` bundle
  the Notes editor uses), so installing a plugin needs no toolchain.

## Migration

The two runtimes coexist. `type = "process"` (the default) keeps every existing
plugin — git, docker, sysinfo, dashboard — working unchanged. New plugins target
`type = "wasm"` for zero-dependency, sandboxed distribution. Once the WASM path
is proven, the bundled plugins port over and the process runtime is deprecated
for untrusted/catalog plugins (kept only for explicitly-trusted local ones).

## Build order

1. **Foundation (done):** manifest `type`/`wasm`, capability vocabulary +
   validation + display, honest not-yet-executable error.
2. Host: instantiate a component, wire `handle-tool`, gate imports by capability
   (start with `run-command`, `notify`, `storage-*`).
3. A reference guest (Rust → `wasm32-wasip2`) porting `sysinfo` to prove the
   path end to end.
4. `render` + `on-event`, so WASM plugins can own panels and triggers.
5. The JS toolchain (`componentize-js`) and docs, then port the bundled plugins.
