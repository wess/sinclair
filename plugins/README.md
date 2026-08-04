# Sinclair plugin catalog

A community catalog of plugins for [Sinclair](https://github.com/wess/sinclair), the
GPU-accelerated terminal. Each plugin lives in its own subfolder
`plugins/<your-plugin-name>/` containing a single `plugin.toml`. Browse the
folders here, install the ones you like, and send a pull request to add your own.

## What a plugin is

A Sinclair plugin is a directory with one `plugin.toml`. The manifest contributes
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
the same trigger, so avoid Sinclair's `cmd+shift+*` defaults (Compose, Broadcast,
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
   cp -R plugins/lazygit ~/.config/sinclair/plugins/lazygit
   # or, to track the catalog:
   ln -s "$PWD/plugins/lazygit" ~/.config/sinclair/plugins/lazygit
   ```

   (Use `$XDG_CONFIG_HOME/sinclair/plugins/` if you have `XDG_CONFIG_HOME` set.)

2. **Point your config at it** by adding a line to `~/.config/sinclair/config`:

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

1. Fork <https://github.com/wess/sinclair>.
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

## Panel plugins (live panels)

Beyond the declarative command model, a plugin can contribute a **side-drawer
panel** rendered from a block tree. A panel plugin is a WASM component — one
runtime, no `bun`/`node`, and the instance stays resident so a render or a click
costs microseconds:

```toml
id = "git"
name = "Git"
capabilities = ["process", "commands"]

[runtime]
type = "wasm"
wasm = "plugin.wasm"

[panel]
id = "git"
title = "Git"
icon = "⎇"                       # activity-bar glyph
```

Your component exports `render()`, which returns the panel as a JSON node tree,
and `on_ui_event()`, which receives button clicks by `id` (the host re-renders
afterwards).

Block types: `section`, `text` (`dimmed?`, `color?`, `mono?`), `divider`, `kv`,
`badge` (`color?`), `button` (`id`, `variant?`), and `row` (`children`). An
unrecognized block renders an inline notice rather than blanking the panel.

To have the terminal do something, call the `run-command` host function; it is
queued during your call and dispatched when you return. To read a program's
output, declare `capabilities = ["process"]` and call `exec` — the host spawns
it (never through a shell, in the focused pane's directory, bounded by a
timeout) and hands you the output. See [`git/src/lib.rs`](./git/src/lib.rs) for
a complete example, and `docs/pluginauthoring.md` for the full guide.

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
- `invoke = "tool"` — call the plugin's `[[tool]]` of that name with the event
  payload (`{ event, … }`) as its parameters. One handler serves the palette,
  agents and triggers alike.

See [`alert/`](./alert/) for a complete example.

## Learn more

- Ideas worth building: [ideas.md](./ideas.md) — today's command model plus what
  the plugin API (event hooks, host capabilities, column UI) unlocks.
- Tutorial: <https://github.com/wess/sinclair/blob/main/docs/plugins.html>
- Project docs: <https://github.com/wess/sinclair>

## Example plugins in this catalog

Panel plugins (live side-drawer panels):

| Plugin                              | What it does                                        | Requires |
| ----------------------------------- | --------------------------------------------------- | -------- |
| [git](./git/)                       | Live branch/changes panel with stage/fetch/log      | `git`    |
| [sysinfo](./sysinfo/)               | Host load + disk panel with a monitor shortcut      | —        |
| [docker](./docker/)                 | Running-containers panel with stats/prune actions   | `docker` |
| [promptdesigner](./promptdesigner/) | Design your shell prompt and apply it to your shell | —        |

Tool plugins (no UI, callable from the palette and by agents):

| Plugin                        | What it does                          | Requires |
| ----------------------------- | ------------------------------------- | -------- |
| [screentools](./screentools/) | Grep the visible terminal screen      | —        |

Trigger plugins (event hooks, no UI):

| Plugin            | What it does                                  | Requires |
| ----------------- | --------------------------------------------- | -------- |
| [alert](./alert/) | Desktop-notify when a command exits non-zero  | —        |

None of these need a language runtime installed: a plugin ships as a single
`plugin.wasm`. The "Requires" column is the program the plugin asks the host to
run on its behalf.
