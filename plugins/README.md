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

## Learn more

- Ideas worth building: [ideas.md](./ideas.md) — today's command model plus what
  the planned plugin API (event hooks, host capabilities, column UI) unlocks.
- Tutorial: <https://github.com/wess/prompt/blob/main/docs/plugins.html>
- Project docs: <https://github.com/wess/prompt>

## Example plugins in this catalog

| Plugin                       | What it does                                   | Requires         |
| ---------------------------- | ---------------------------------------------- | ---------------- |
| [lazygit](./lazygit/)        | Launch the lazygit TUI in a new tab            | `lazygit`        |
| [lazydocker](./lazydocker/)  | Launch the lazydocker TUI in a new tab         | `lazydocker`     |
| [k9s](./k9s/)                | Open the k9s Kubernetes UI in a new tab        | `k9s`            |
| [tig](./tig/)                | Browse git history with tig in a new tab       | `tig`            |
| [gh](./gh/)                  | GitHub PR list / status in a split             | `gh`             |
| [gitlog](./gitlog/)          | Pretty git commit graph in the focused pane    | `git`            |
| [serve](./serve/)            | Serve the current directory over HTTP          | `python3`        |
| [ports](./ports/)            | List processes listening on local ports        | `lsof`           |
| [repl](./repl/)              | Open a Python / Node / Bun REPL in a split     | runtime of choice |
| [sysmon](./sysmon/)          | Open a system monitor (htop / btop) in a split | `htop`/`btop`    |
| [weather](./weather/)        | Show the local weather in the focused pane     | `curl`           |
