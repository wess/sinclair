# Plugin system v2 — architecture

Status: **design (approved direction).** Supersedes `docs/plugins-wasm.md` (the
earlier WASM-only sketch), which is folded in here. This is the spec the v2 build
executes against.

Approved direction:

- **Full overhaul**, delivered as one arc (staged internally, build green at each
  stage).
- **Plugins are fully capable**: agent tools *and* UI extension are both
  first-class — a plugin can give an agent a new tool, react to terminal events,
  and own a panel or a webview, all from one module.
- **WASM-primary runtime** with a **trusted native tier** kept for power users /
  local development. Catalog (untrusted) plugins must be WASM; the five existing
  `bun`/TS plugins keep working on the native tier and port to WASM over time.
- **Notes moves back out of the core into a plugin** — behaving *exactly* as it
  does today. Notes is a markdown-vault editor, not terminal-core, so it belongs
  as a plugin. It ships the existing Rust `notes` sidecar binary (no `bun` — the
  dependency that forced it in-tree originally) and rides the new host-managed
  sidecar path. Notes is the driving proof that that path works: making it a
  plugin retires the bespoke `Boot::Server` fn-pointer and the hardcoded port in
  `app/src/notes.rs`.

## Why (the problems v2 fixes)

The v1 system is three half-built systems on two runtimes, one of which is dead.
The concrete debt, and how v2 removes it:

| v1 problem | v2 fix |
|---|---|
| Subprocess spawned **per event** (render/click/trigger/tool) — slow, cold-start every time | Resident in-process WASM instance; a call is microseconds, not a process spawn |
| **Stateless** — a plugin can't remember anything between a render and a click | Persistent instance + host `storage`; UI is **pushed**, not re-fetched |
| Needs `bun`/`node` on `$PATH` (the whole reason `envpath.rs` runs a login shell) | WASM has no runtime dependency; native tier is opt-in and explicit |
| **Full user privilege**; `capability =` enforces nothing | Host imports are **linked only if the matching capability is declared** |
| WASM runtime **declared but hard-errors** as "not executable" | The engine is built; WASM is the primary path |
| 684-line hand-rolled TOML parser; params as pipe-strings | `serde` + `toml` (already a dep); real tables/arrays |
| Block-tree is **pull-only, button-only**, one bad block nukes the panel, **stderr discarded** | Declarative UI **pushed** by the resident module, with inputs/forms; per-node resilience; a host `log` function |
| Webview `onMessage` (native→page) **defined but never called**; `boot` hack with a hardcoded port + token race; managed-sidecar path closed to plugins | Typed two-way bridge; a **host-managed sidecar** service open to plugins (port negotiation + token handshake + lifecycle) |
| Catalog: single repo, no signing, no enable/disable, self-attested capabilities | A registry with **signed manifests, capability consent at install, enable/disable, version pinning** — safe because the runtime is sandboxed |
| No SDK — every author copy-pastes the protocol | A `sinclair-plugin` Rust crate + a JS package (via `componentize-js`) |

## Principles

1. **One contract, many contributions.** A plugin declares *contributions* (tools,
   commands, panels, webviews, triggers) in its manifest and implements them
   against **one** versioned host↔guest interface (a WIT world), not five ad-hoc
   JSON shapes.
2. **Capabilities are the boundary.** What a plugin can reach is exactly what it
   declared and the user consented to — enforced by the runtime, not by
   convention.
3. **The host owns surfaces; the plugin owns logic.** Plugins never link gpui.
   They emit data/UI descriptions and call gated host functions; the host renders
   and applies effects. (Kept from v1 — it's the one good bone.)
4. **Push, don't poll.** A resident plugin updates its panel when *it* has news,
   instead of the host re-spawning it to ask.
5. **Sandbox first, native as an escape hatch.** The default and the catalog are
   WASM. Native is a labelled, trusted, locally-installed-only tier.

## Architecture overview

```
                       ┌─────────────────────────────────────────┐
   plugin.toml  ──────▶│ crates/plugin        (manifest, serde)   │  pure
   (manifest v2)       └─────────────────────────────────────────┘
                                        │  Manifest + Contributions
                                        ▼
                       ┌─────────────────────────────────────────┐
   plugin.wasm  ──────▶│ crates/pluginrt      (wasmtime engine)   │  owns wasmtime,
   (component)         │  · WIT world  · capability linker        │  WIT bindings,
                       │  · resident Store per plugin             │  Host trait
                       │  · native-tier adapter (subprocess)      │
                       │  defines `trait Host` (host functions)   │
                       └─────────────────────────────────────────┘
                                        │  implements Host
                                        ▼
                       ┌─────────────────────────────────────────┐
                       │ crates/app                               │
                       │  · impl Host  → mcp_dispatch, panels,    │
                       │    webviews, notifications, triggers     │
                       │  · command palette + `sinclair mcp` expose │
                       │    plugin tools                          │
                       └─────────────────────────────────────────┘
```

`pluginrt` isolates the heavy `wasmtime` dependency and the WIT bindings from
`app`. It defines a `Host` trait (the gated host functions); `app` implements it,
bridging to the same `mcp_dispatch`/panel/webview machinery it already owns. No
upward dependency: `plugin` (pure) ← `pluginrt` ← `app`.

## 1. Manifest v2 (`crates/plugin`, serde/toml)

Real TOML, parsed with `#[derive(Deserialize)]`. The 684-line `parse.rs` and the
`RawPlugin` bag are deleted; validation becomes a small post-deserialize pass
that produces the same friendly `Diagnostic`s.

```toml
id = "git"
name = "Git"
version = "0.2.0"
description = "Live git panel + agent tools"

[runtime]
type = "wasm"                 # "wasm" (default) | "native"
wasm = "plugin.wasm"          # for wasm
# command = "bun run plugin.ts"   # for native (trusted, local-only)

capabilities = ["commands", "filesystem", "network"]   # a real array

[[command]]
id = "status"
title = "Git status"
run = "git status"
mode = "split-right"
keybind = "cmd+ctrl+g"

[[tool]]                      # first-class: user (palette) + agent (MCP)
id = "blame"
description = "Blame a file and return structured JSON."
[[tool.param]]                # a real table, not "name | type | desc | required"
name = "path"
type = "string"
description = "File to blame."
required = true

[[panel]]
id = "git"
title = "Git"
icon = "⎇"

[[webview]]
id = "diff"
title = "Diff"
placement = "tab"             # panel | tab | window
entry = "web/index.html"      # entry (served) | url (remote) | service (host-managed)

[[trigger]]
on = "command_finished"
when = { exit = "nonzero" }   # typed filter, not a positional string
do = { invoke = "on_fail" }   # do = { run=… } | { notify=… } | { invoke=… }
```

Notable schema upgrades: capabilities as an array; tool params as tables; typed
trigger `when` (`{ exit = … }` / `{ contains = … }` / `{ matches = <regex> }`)
and explicit `do`; `[[panel]]`/`[[webview]]` repeatable (a plugin may contribute
several). `id` validity, capability vocabulary, and "tool needs a runtime" checks
survive as diagnostics.

## 2. Runtime

### 2a. WASM (primary) — the component model on `wasmtime`

- **Engine:** `wasmtime` with the component model. One `Engine`, one `Linker`
  per capability profile.
- **Instance lifecycle:** one `Store` + instantiated component **per plugin**,
  created lazily on first use and **kept resident** (persistent state; no
  per-event spawn). `init` runs once on instantiation.
- **Capability linking:** the linker adds a host import **only if** the plugin's
  manifest declares the matching capability. A module without `network` cannot
  import `fetch` — it fails to instantiate. This is the real enforcement the
  process runtime can't give.
- **Resource bounds:** epoch-interruption + fuel bound a runaway guest (the
  analogue of v1's 15s SIGKILL, but cooperative and per-call). Memory is capped
  per `Store`.
- **Concurrency:** host functions are `async` where they do I/O (`fetch`,
  `read-file`); guest calls run on the async executor, so a slow plugin call
  never blocks the UI thread (v1 pinned a worker thread for up to 15s).

### 2b. The WIT world (the contract)

```wit
package prompt:plugin@2.0.0;

// ── Host imports (capability-gated unless noted) ─────────────────────────────
interface host {
  // cap: commands
  run-command: func(text: string, target: target) -> result<_, string>;
  send-input:  func(bytes: list<u8>) -> result<_, string>;
  // cap: screen
  read-screen: func(lines: u32) -> result<string, string>;
  selection:   func() -> option<string>;
  // cap: network
  fetch: func(req: http-request) -> result<http-response, string>;
  // cap: filesystem  (scoped to the plugin dir + the focused cwd by default)
  read-file:  func(path: string) -> result<list<u8>, string>;
  write-file: func(path: string, data: list<u8>) -> result<_, string>;
  // cap: clipboard
  clipboard-read:  func() -> result<string, string>;
  clipboard-write: func(text: string) -> result<_, string>;
  // cap: notify
  notify: func(title: string, body: string);
  // ungated: per-plugin key/value store (persistent state)
  storage-get: func(key: string) -> option<string>;
  storage-set: func(key: string, value: string);
  // ungated: structured logging — surfaced in the Plugin Manager (fixes v1's
  // swallowed stderr)
  log: func(level: log-level, msg: string);
  // ungated: UI push — the host owns the surface, the plugin drives it
  render-panel: func(panel-id: string, tree: node);   // proactively update a panel
  open-webview: func(id: string);                      // request a webview open
}

// ── Guest exports (what a plugin implements) ─────────────────────────────────
interface guest {
  init: func();                                        // once, on instantiation
  // Tools — the spine, surfaced to the palette AND to `sinclair mcp`.
  call-tool: func(name: string, params-json: string) -> result<string, string>;
  // Panels — render on demand; react to events (then push updates via host).
  render:      func(req: render-request) -> node;
  on-ui-event: func(ev: ui-event);                     // button/input/select
  // Webview messages — a typed replacement for the JS `invoke()` catch-all.
  on-message:  func(method: string, params-json: string) -> result<string, string>;
  // Terminal/agent events (triggers with `invoke`).
  on-event:    func(ev: event);
}

world plugin { import host; export guest; }
```

`node` is a real UI tree — the v1 block set **plus inputs**: `section`, `text`,
`kv`, `badge`, `divider`, `button`, `row`/`column`, and new `text-input`,
`checkbox`, `select`, `progress`, `list`. Events (`on-ui-event`) carry the node
id + value, so a plugin can build a form and update it in place by calling
`render-panel` again — no host round-trip through a subprocess.

### 2c. Native tier (trusted, opt-in)

`[runtime] type = "native"` keeps v1's subprocess-over-stdio model, but:

- **Trust-gated:** allowed only for a plugin installed from a local path (a
  `plugin = <path>` config entry), never for a catalog install. The Plugin
  Manager labels it "native — runs with full access."
- **One adapter, same contract:** `pluginrt` wraps the subprocess behind the
  *same* `Host`/guest surface, so `app` calls plugins uniformly regardless of
  tier. The native adapter keeps a **warm process** (a long-lived stdio server,
  not spawn-per-event) so even native plugins stop paying cold-start.
- This is the compatibility bridge for git/docker/sysinfo/dashboard/
  promptdesigner until they're ported.

## 3. Contributions (both tiers surface identically)

- **Tools** — `call-tool`. Registered into the command palette (run with a JSON
  arg form) *and* merged into `sinclair mcp`'s tool list as `<plugin>_<tool>` (as
  today, but now backed by the resident instance). This is the differentiator:
  one implementation, callable by the human and by their agents.
- **Commands** — unchanged in spirit (shell command + mode + optional keybind),
  resolved through the existing keymap pipeline.
- **Panels** — declarative `node` tree, **pushed** by the plugin; supports
  inputs/forms; per-node render resilience (a bad node renders an inline error,
  never blanks the panel).
- **Webviews** — `panel` | `tab` | `window`, with a fixed bridge (below).
- **Triggers** — typed `when`, `do = run|notify|invoke`; `invoke` calls
  `on-event` on the resident instance.

## 4. Webview overhaul

- **Two-way typed bridge.** Keep `Sinclair.invoke(method, params) -> Promise` and
  `Sinclair.runCommand`/`readScreen`, but **implement the missing direction**:
  `Sinclair.onMessage(cb)` is wired to a host `post-to-webview(id, msg)` so a
  plugin (or the host) can push to the page. `invoke` calls route to built-in
  ops first, then to the guest's `on-message` (typed, not the v1 blind
  `kind:"message"` spawn).
- **Host-managed sidecar** (replaces the `boot` hack). A `[[webview]]` with
  `service = true` asks the **host** to run a bundled/plugin server: the host
  allocates a free port, mints a token, starts and health-checks the process,
  serves the page from that origin, and **reaps it on close**. This generalizes
  the compile-time `Boot::Server` (Notes) into a data-driven path any plugin can
  use — and fixes the hardcoded-4319 collision and the token-file race for Notes
  too.
- `Entry` (served over the `guise://` origin) stays for static pages; `Url` stays
  for remote.

## 5. SDK

- **Rust:** a `sinclair-plugin` crate — the generated `wit-bindgen` bindings, a
  typed host wrapper, and `ui` builders (`ui::section`, `ui::button`, …). A
  plugin is `#[prompt_plugin] impl Plugin { fn call_tool(...) ... }`.
- **JS/TS:** an `sinclair-plugin` package authored against the same WIT, built to
  a component with `componentize-js` — so today's `bun`/TS authors keep their
  language but ship a **self-contained `.wasm`** with no runtime dependency.
- Both replace the copy-pasted block union + stdin/stdout boilerplate every v1
  `plugin.ts` carries.

## 6. Distribution & trust

The sandbox is what makes a real ecosystem safe:

- **Signed manifest + capability consent.** Install shows exactly what the plugin
  declared (`network`, `filesystem`, …) and asks the user to grant it; the grant
  is recorded. A plugin can't widen its capabilities post-install without
  re-consent.
- **Enable/disable + version pinning.** An `installed.toml` records id → version,
  source, granted capabilities, and enabled state (replacing "a folder exists and
  parses"). Updates are explicit and diffed against the granted capabilities.
- **Registry.** Move off the single `wess/sinclair/plugins` monorepo folder to an
  index (name → source + version + signature). Keep a curated first-party set.
- The duplicated manager (standalone window + inline on `WorkspaceView`) collapses
  to one implementation.

## 7. Crate / module layout

- `crates/plugin` — manifest v2 (serde model + validation), the Contribution
  types, capability vocabulary. Pure, no wasmtime.
- `crates/pluginrt` — **new.** `wasmtime` engine, WIT bindings, capability
  linker, resident-store manager, the `Host` trait, and the native-tier adapter
  (warm subprocess). Depends on `plugin`.
- `crates/app` — `impl Host` (bridging host functions to `mcp_dispatch`, panel
  render, webview, notify, triggers), palette + `sinclair mcp` tool exposure,
  Plugin Manager, registry client. `pluginhost.rs` shrinks to the native adapter
  glue; `pluginwebview.rs` gains the two-way bridge + sidecar manager.

## 8. Migration

- **Back-compat:** v1 `plugin.toml` files parse under v2 (the schema is a superset;
  the pipe-string `param` form is accepted with a deprecation diagnostic and
  auto-mapped to `[[tool.param]]`).
- **Native tier keeps v1 plugins running** unchanged (they become `type =
  "native"` implicitly when they declare only `command`).
- **Bundled plugins mostly stay native** (refinement found while building Stage
  2). `git` / `docker` / `sysinfo` fundamentally spawn processes and read their
  output (`git status`, `docker ps`, `df`) — which a WASM sandbox cannot do. They
  are the *canonical* native-tier plugins. WASM is the tier for **sandboxed,
  host-function** plugins: ones that read the screen, drive the terminal, fetch
  URLs, read/write scoped files, or expose computed tools. The reference WASM
  plugin is therefore a purpose-built one (a screen/terminal utility), not a port
  of a process-spawning bundled plugin. `dashboard`/`promptdesigner` (webviews)
  are a separate axis — see the webview stage.
- **Notes** moves out of the core into a first-party **plugin**, behaving
  identically. It ships the bundled Rust `notes` binary and declares a
  `[[webview]]` with `service = true`; the host-managed sidecar runs it (port +
  token + lifecycle), so the page loads from the same `http://127.0.0.1/?token=…`
  origin as today. This retires `app/src/notes.rs` (the `ensure_server`
  fn-pointer), the `Boot::Server` variant, and the hardcoded port 4319. The
  `notes` crate stays (still bundled beside `sinclair`); only its *wiring* moves
  from hardcoded app code to a plugin manifest. File → Notes still opens it.

## 9. Build order (the one push, staged so the build stays green)

Progress: all 8 stages landed on `feat/pluginsv2` — but a post-merge audit
(§11) found several stage claims ahead of the code. Treat §11 as the accurate
status; the per-stage notes below describe the intent.
✅ = done. The runtime foundation (0–4) is complete and unit-tested end-to-end;
the surface/distribution stages (5–7) are complete — the webview sidecar path
(Notes migrated, `.service.json` discovery, both bridge directions), both SDKs
building loadable components (`componentize-js` validated), and the install-state
model with capability-consent enforcement and checksum verification are all in.

0. ✓ **Foundation & cleanup** — manifest → serde (delete `parse.rs`/`RawPlugin`);
   delete the dead WASM stub error; collapse the duplicated Plugin Manager;
   surface stderr + per-node panel resilience (immediate DX wins). *No behavior
   change for existing plugins.*
1. ✓ **`pluginrt` + WIT world + capability linker** — instantiate a component, wire
   `call-tool`, gate imports by capability. Prove it end-to-end with a Rust
   `sysinfo` port (tools only).
2. ✓ **Host functions** — implement the gated `Host` in `app` (`run-command`,
   `read-screen`, `fetch`, `storage`, `notify`, `log`), mapped onto existing
   machinery.
3. ✅ **Panels v2** — the `node` tree with inputs, `render` + `on-ui-event`,
   rendered in-process (`render_wasm_panel`/`wasm_ui_event` reusing the block-tree
   renderer). The `screentools` bundled WASM plugin drives it end-to-end.
4. ✓ **Native tier adapter** — a warm long-lived stdio server for `[runtime]
   persistent = true` plugins (spawned once, newline-JSON request/response loop),
   wired into the tool bridge; one-shot plugins keep the spawn-per-event path.
   (GUI-panel warm path reuses the same manager — follow-up.)
5. ✅ **Webview overhaul + Notes is now a real plugin** — the host-managed
   sidecar mechanism: a `[webview] service = "…"` (manifest
   `WebviewSource::Service`) maps to `Boot::Command`, which spawns the sidecar
   (detached, own process group) in a writable per-plugin data dir
   (`<config>/prompt/data/<id>`, not the possibly read-only plugin dir), waits on
   a liveness check, and reads its `.service.json` (`{port, token}`, 0600). The
   `notes` binary publishes its real bound port in `.service.json`, and
   `resolve_program` resolves the bundled sidecar as a sibling of the exe.
   **Bundled-plugin discovery** (`plugin::load`) loads first-party plugins shipped
   with the app (`Contents/Resources/plugins` on macOS,
   `share/sinclair/plugins` on Linux, the workspace `plugins/` in dev); a
   user-installed plugin of the same id overrides the bundled copy. Notes is now
   the bundled `plugins/notes` plugin — File → Notes resolves and opens it from
   its manifest (`Boot::Server` retired), with a direct-sidecar fallback for a
   bare binary run outside a bundle. The bridge is symmetric: page→host
   (`__sinclairResolve`) and host→page (`post_to_page` → `__sinclairDeliver`).
6. ✅ **SDK** — `sinclair-plugin` crate + `sinclair-plugin` (`componentize-js`) + docs;
   port `promptdesigner`.
7. ✅ **Registry & trust** — the install-state model: `installed.toml`
   (`plugin::Installed`) records each plugin's version, source, enabled flag, and
   the capabilities the user granted; `plugin::load` skips disabled plugins.
   Replaces "a folder exists = enabled, capabilities self-attested." Consent is
   *enforced*, not just recorded: `effective_capabilities` intersects declared
   with granted, and the WASM host links only the granted interfaces — an
   ungranted capability fails instantiation. (A checksum-verified registry
   module shipped here originally but was removed as dead code — catalog
   installs are unverified until a real pinned index lands.)

Each stage is independently shippable and leaves the tree green.

## 10. Post-v2 audit — spec vs. shipped (2026-07-09)

A full code survey against this spec. What §9 marks done but the tree doesn't
deliver, plus debt v2 didn't cover. This section is the v2.1 worklist.

**Gaps against explicit v2 promises:**

- **Sidecar lifecycle (§4 "reaps it on close").** `run_service` spawns
  detached and never tracks or kills anything — the comment says so
  (`pluginwebview.rs:388` "Lifecycle/reaping is a follow-up"). The `notes`
  sidecar compensates with a self-reaping 60s idle timer
  (`crates/notes/src/server.rs`), which let the server die under an open
  folder dialog (the 2026-07-09 Notes bug). Sidecars outlive the app.
- **Port allocation (§4 "the host allocates a free port").** The host passes
  no port; `notes` defaults to fixed 4319 (`crates/notes/src/main.rs:14`) and
  the host discovers it by polling `.service.json` 60×50ms plus a TCP probe
  (`pluginwebview.rs:425,445`). Bind races are "resolved" by the loser exiting
  silently (`server.rs:103`). Two descriptor files are maintained for one
  server (`server.json` + `.service.json`).
- **Consent (§9 stage 7 "Consent is enforced, not just recorded").**
  *Closed 2026-07-09:* catalog install records declared capabilities as
  granted, the manager has Enable/Disable wired to `set_enabled` + `save`,
  and `effective_capabilities` narrowing engages. The unwired `registry.rs`
  was deleted; catalog installs stay checksum-unverified until a pinned
  index ships.
- **Two-way bridge (§9 stage 5 "the bridge is symmetric").** `post_to_page` /
  `__sinclairDeliver` exist but nothing calls them (`pluginwebview.rs:355`).
- **Triggers `invoke` on wasm.** Routed through `pluginhost::invoke`, which
  hard-rejects wasm (`pluginhost.rs:138`, with a comment claiming the engine
  "is not built yet" — stale since stage 1 shipped). A wasm plugin cannot
  receive events.
- **Gated stubs.** `fetch` and `clipboard` return "not yet available" in both
  wasm hosts (`wasmhost.rs:161,175`; `guiwasm.rs:121,130`); panel
  `read_screen` returns empty (`guiwasm.rs:114`). The `network`/`clipboard`
  capabilities currently gate nothing real.
- **`docs/plugins-wasm.md`** still describes the pre-v2 single-interface WIT
  and says the engine is "in progress"; it was supposed to be superseded.

**Debt v2 didn't cover (found the hard way):**

- **Native dialogs from sidecars.** `notes` shells `osascript`/`zenity` for
  the vault folder picker (`server.rs:513`). macOS attributes the folder-
  permission (TCC) consent to the wrong process; the picker hung on the
  blocked stat until 2026-07-09's symptomatic fix. Dialogs must be a
  capability-gated **host service** (`pick-folder` in the WIT host interface
  and the native-tier stdio protocol), not a sidecar shell-out.
- **Namespace drift.** WIT package still `prompt:plugin`
  (`wit/plugin.wit:6`), `window.Prompt` bridge alias
  (`pluginwebview.rs:142`), legacy `prompt` config-dir fallbacks
  (`load.rs:12`, `paths.rs:14`), `<config>/prompt/data/<id>` naming in §9.
- **Dead code to collect:** `Runtime::evict` (`pluginrt/lib.rs:314`),
  `Placement::Tab` falling back to a window (`pluginpanel.rs:127`).
  (`WarmPlugins::evict` and the duplicated Notes fallback boot were removed
  2026-07-09.)

**v2.1 order (each independently shippable):**

1. **Host-owned lifecycle + handoff** — *shipped* (`app/src/sidecar.rs`):
   tracked, refcounted children; host-assigned port + token via env; reaped
   on last-surface-close and app quit; no descriptor files. Readiness is now
   verified with a token challenge (`GET /health?challenge=` →
   `x-sinclair-proof`), so a port squatter cannot be handed the session.
2. **Host services** — `pick-folder`/`notify`/`clipboard` as gated host calls
   on both tiers; move the Notes picker to it; fill or fence the
   `fetch`/`clipboard` stubs.
3. **Wire consent** — *mostly shipped:* `record` at catalog install and
   Enable/Disable in the manager landed 2026-07-09. Remaining: a consent
   sheet at install and broker-checking capabilities on native-tier
   dispatches (still display-only outside wasm).
4. **Bridge + triggers** — call `post_to_page` or delete it; route wasm
   `invoke` triggers to `on-event`; retire `docs/plugins-wasm.md`.
5. **Rename hygiene** — `sinclair:plugin@2` WIT package with a load-time
   alias for committed `prompt:plugin` components; drop `window.Prompt`;
   collapse legacy `prompt` path fallbacks.

## 11. Open questions

- **Signing scheme** — first-party key + a simple detached signature, or lean on
  an existing format (sigstore/minisign)?
- **Filesystem capability scope** — default to the plugin dir + focused cwd, with
  an explicit broader grant? (Proposed: yes.)
- **`wasmtime` build cost** — it's a large dependency; confirm it's acceptable in
  the shipped binary size / build time, and that it cross-compiles for the Linux
  targets. (Likely fine; verify early in stage 1.)
- **componentize-js maturity** — *resolved.* The JS→component path produces a
  loadable plugin (`sdk/js/`), with the engine's WASI http/fetch disabled so the
  network is reachable only through gated `host-net`. The component is ~12 MB (it
  embeds the JS engine); Rust modules are far smaller, so Rust stays the
  recommended path and JS is the escape hatch for existing `bun`/TS authors.
</content>
