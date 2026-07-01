# Prompt plugin catalog

A community catalog of plugins for [Prompt](https://github.com/wess/prompt), the
GPU-accelerated terminal. Each plugin lives in its own subfolder
`plugins/<your-plugin-name>/` containing a single `plugin.toml`. Browse the
folders here, install the ones you like, and send a pull request to add your own.

## What a plugin is

A Prompt plugin is a directory with one `plugin.toml`. The manifest contributes
named commands; each command runs a shell command in the terminal and can carry
a default keybinding. That is the whole surface — no scripting runtime, no
network access, just declarative commands.

## `plugin.toml` schema

The manifest is a small TOML subset: root key/values plus repeated
`[[command]]` tables.

Root keys:

| Key           | Required | Notes                                                       |
| ------------- | -------- | ----------------------------------------------------------- |
| `id`          | yes      | lowercase ASCII letters, digits, `.`, `-` only              |
| `name`        | no       | display name, defaults to `id`                              |
| `version`     | no       | defaults to `"0.0.0"`                                       |
| `description` | no       | one-line summary                                            |

Each `[[command]]` table:

| Key       | Required | Notes                                                                 |
| --------- | -------- | --------------------------------------------------------------------- |
| `id`      | yes      | same charset rule as the plugin `id`                                  |
| `title`   | no       | display name, defaults to the command `id`                            |
| `run`     | yes      | the shell command text to execute                                     |
| `mode`    | no       | one of `pane`, `tab`, `split-right`, `split-down` (default `pane`)    |
| `keybind` | no       | a default keybinding, e.g. `cmd+shift+l`                              |

Modes:

- `pane` (default) — types the command into the focused shell and presses enter.
- `tab` — opens a new tab and runs the command there.
- `split-right` — opens a split to the right and runs the command there.
- `split-down` — opens a split below and runs the command there.

Keybind syntax: `cmd`/`ctrl`/`alt`/`shift`/`super` plus a key, joined with `+`
(for example `cmd+ctrl+g`). A plugin keybind overrides any built-in binding with
the same trigger, so avoid Prompt's `cmd+shift+*` defaults (Compose, Broadcast,
the relay shortcuts, and so on). The examples here use the `cmd+ctrl+*` namespace
to stay clear of them.

### Annotated example

```toml
# Root: identifies the plugin. `id` must match the folder name.
id = "tools"
name = "Dev Tools"
version = "0.1.0"
description = "Handy developer shortcuts"

# A command. Becomes the action `plugin_command:tools/logs`.
[[command]]
id = "logs"                 # required
title = "Tail app logs"     # shown in the command palette
run = "tail -f ./app.log"   # required: the shell command
mode = "split-down"         # pane | tab | split-right | split-down
keybind = "cmd+ctrl+l"      # optional default binding

# A second command in the same plugin.
[[command]]
id = "test"
title = "Run tests"
run = "cargo test"
mode = "tab"
```

## Installing a catalog plugin

Pick whichever you prefer:

1. **Copy or symlink the folder** into your plugins directory:

   ```sh
   cp -R plugins/lazygit ~/.config/prompt/plugins/lazygit
   # or, to track the catalog:
   ln -s "$PWD/plugins/lazygit" ~/.config/prompt/plugins/lazygit
   ```

   (Use `$XDG_CONFIG_HOME/prompt/plugins/` if you have `XDG_CONFIG_HOME` set.)

2. **Point your config at it** by adding a line to `~/.config/prompt/config`:

   ```
   plugin = /path/to/prompt/plugins/lazygit
   ```

Config reloads live, so the plugin loads on the next launch (and on most config
saves without a restart).

## How commands and keybinds work

Every contributed command becomes an action named
`plugin_command:<plugin-id>/<command-id>` — for example `plugin_command:tools/logs`.
If a command declares a `keybind`, that binding is registered by default.

To override the binding, or to bind a command that ships without one, add a
`keybind` line to your config:

```
keybind = cmd+ctrl+l=plugin_command:tools/logs
```

To unbind a default keybind, use the `unbind` action (or rebind the key to
something else):

```
keybind = cmd+ctrl+l=unbind
```

You can always run any command from the command palette regardless of keybind.

## Contributing a plugin via PR

1. Fork <https://github.com/wess/prompt>.
2. Add your plugin under `plugins/<your-plugin-name>/plugin.toml`. The folder
   name is lowercase (kebab-case is fine) and the plugin `id` must match the
   folder name.
3. Include a short per-plugin `readme.md` describing what it does, any
   dependencies, an install hint, and the keybind(s).
4. Keep `run` commands portable and safe — no destructive defaults
   (no `rm -rf`, no force-push, nothing that mutates state without asking).
5. Open a pull request.

### Checklist

- [ ] Plugin `id` is unique and matches the folder name.
- [ ] Each command has a sensible `title`.
- [ ] Dependencies are documented in the `readme.md`.
- [ ] No secrets, tokens, or credentials baked into `run`.
- [ ] No destructive or irreversible commands.

## IPC plugins (live panels)

Beyond the declarative command model, a plugin can be an **IPC plugin**: it
contributes a side-drawer panel rendered from a block tree. Prompt invokes the
plugin's `[runtime]` once per event (serverless-style), passing a JSON request
on stdin and reading a JSON response on stdout:

```toml
id = "git"
name = "Git"

[runtime]
command = "bun run plugin.ts"   # any language; reads stdin, writes stdout

[panel]
id = "git"
title = "Git"
icon = "⎇"                       # activity-bar glyph
```

Request: `{ "kind": "render" | "action", "panel", "action"?, "cwd"? }`.
Response: `{ "title"?, "blocks": [...], "run"?: [{ "text", "target"? }] }`.

Block types: `section`, `text` (`dimmed?`), `divider`, `kv`, `badge` (`color?`),
`button` (`id`, `variant?`), and `row` (`children`). A `button` click sends an
`action` request with its `id`; `run` directives are executed in the focused
terminal (`pane` | `tab` | `split_right` | `split_down`). See
[`git/plugin.ts`](./git/plugin.ts) for a complete example.

## Webview plugins (HTML/JS surfaces)

For a full custom UI, a plugin can contribute a **web view** — a native OS web
view (WKWebView / WebView2 / WebKitGTK) hosting arbitrary HTML/JS. Add a
`[webview]` section:

```toml
[webview]
id = "dashboard"
title = "Dashboard"
icon = "◱"                 # activity-bar / tab glyph
placement = "panel"        # panel | window | tab
entry = "index.html"       # a file in the plugin dir (file://) …
# url = "https://…"        # … or a URL instead of `entry` (exactly one)
```

Open it from the command palette ("Open <title>"), the right sidebar (for
`placement = "panel"`), or bind the `open_webview:<id>` action. `placement =
"tab"` currently opens in a window until tab hosting lands.

**The `window.Prompt` bridge.** The page talks to Prompt through an injected
global:

- `Prompt.runCommand(text, target?)` — run a command in the focused terminal.
- `Prompt.readScreen(lines?)` — read the visible screen; resolves `{ text }`.
- `Prompt.invoke(method, params?)` — returns a Promise. Built-in methods (the
  same capabilities as `run` directives: `run_command`, `read_screen`,
  `send_input`, `new_tab`, `split`, `list_panes`, `list_tabs`, `focus_tab`,
  `run_macro`, …) are handled by the app; any other method is forwarded to the
  plugin's `[runtime]` as a `message` request, and its `result` resolves the
  promise.
- `Prompt.postMessage(data)` — fire-and-forget message to the runtime.
- `Prompt.onMessage(cb)` — receive pushes from the host.

A `message` request is `{ "kind": "message", "panel", "method", "params"?,
"cwd"? }`; reply with `{ "result": … }`. See [`dashboard/`](./dashboard/) for a
complete example (HTML + runtime).

## Trigger plugins (event hooks)

Plugins can also just **react to events** — no UI, no manual command. Add one or
more `[[trigger]]` tables:

```toml
[[trigger]]
on = "command_finished"    # bell | title_changed | notify | exit | command_finished | dir_changed
when = "nonzero"           # optional filter (see below)
notify = "A command failed"  # one action: notify | run | invoke
```

**Events.** `bell`, `title_changed`, `notify`, and `exit` fire out of the box.
`command_finished` (with exit code) and `dir_changed` need **shell integration**
enabled (OSC 133 / OSC 7).

**`when` filter (optional).** For `exit` / `command_finished`: `any`,
`zero`/`success`, or `nonzero`/`failure`. For `title_changed` / `notify` /
`dir_changed`: a substring the title / body / path must contain. Ignored for
`bell`.

**Action (exactly one).**

- `notify = "…"` — a desktop notification.
- `run = "…"` with optional `target` — `background` (default, detached), `pane`,
  `tab`, `split_right`, or `split_down`. Runs with the focused pane's cwd.
- `invoke = "method"` — call the plugin's `[runtime]` with the event payload
  (`{ event, … }`); any `run` directives it returns are executed.

See [`alert/`](./alert/) for a complete example.

## Learn more

- Ideas worth building: [ideas.md](./ideas.md) — today's command model plus what
  the plugin API (event hooks, host capabilities, column UI) unlocks.
- Tutorial: <https://github.com/wess/prompt/blob/main/docs/plugins.html>
- Project docs: <https://github.com/wess/prompt>

## Example plugins in this catalog

IPC panel plugins (live side-drawer panels):

| Plugin                              | What it does                                        | Requires        |
| ----------------------------------- | --------------------------------------------------- | --------------- |
| [git](./git/)                       | Live branch/changes panel with stage/fetch/log      | `bun`, `git`    |
| [sysinfo](./sysinfo/)               | Host load + disk panel with a monitor shortcut      | `bun`           |
| [docker](./docker/)                 | Running-containers panel with stats/prune actions   | `bun`, `docker` |
| [promptdesigner](./promptdesigner/) | Design your shell prompt and apply it to your shell | `bun`           |

Webview plugins (HTML/JS surfaces):

| Plugin                    | What it does                                          | Requires |
| ------------------------- | ---------------------------------------------------- | -------- |
| [notes](./notes/)         | A lightweight Obsidian: a markdown vault + editor tab | `bun`    |
| [dashboard](./dashboard/) | HTML panel that runs commands + calls the runtime    | `bun`    |

Trigger plugins (event hooks, no UI):

| Plugin                | What it does                                              | Requires |
| --------------------- | -------------------------------------------------------- | -------- |
| [alert](./alert/)     | Desktop-notify when a command exits non-zero             | —        |

Every example plugin here is "involved" — a live panel, a web view, or an event
hook. The declarative command model still exists (see the schema above); it's
just not something worth shipping as a catalog example on its own.
