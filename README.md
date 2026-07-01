# Prompt

A fast, modern terminal that gets out of your way.

Prompt is a GPU-accelerated terminal emulator built for people who live in the
command line. It pairs a meticulous, standards-complete terminal core with a
clean tabbed-and-split workspace, live-reloading config, and a library of
beautiful themes — so your terminal feels instant, looks great, and bends to
exactly how you work.

## Why Prompt

- **Quick.** GPU rendering and a tight event loop keep scrolling and heavy
  output buttery, even under a firehose of logs.
- **Comfortable.** Tabs and recursive splits, true-color and ligature-aware
  text, emoji, crisp box-drawing, and 22 hand-tuned themes out of the box.
- **Yours.** A single readable config file, reloaded the instant you save —
  no restart. Rebind any key, set fonts, pick a theme, tune behavior.
- **Capable.** Deep terminal support: hyperlinks, the clipboard protocol,
  bracketed paste, mouse reporting, the kitty keyboard protocol, focus and
  synchronized-output handling, and shell-integration prompt marking with
  jump-to-prompt.
- **Searchable.** Find anything in your scrollback with a live, highlighted
  in-place search.

## Highlights

- **Tabs & splits** — open tabs, split panes any direction, drag the dividers,
  and move focus by direction. Each pane is its own shell.
- **Selection & clipboard** — mouse selection by cell, word, or line;
  copy-on-select; paste with bracketing; OSC 52 clipboard support.
- **Hyperlinks & URLs** — OSC 8 links are underlined and open on ⌘-click, and
  plain URLs in output are clickable too.
- **Shell integration** — prompts are marked, the working directory follows
  into new splits and tabs, and you can jump between prompts.
- **Fonts** — primary font plus a fallback chain, emoji, programming
  ligatures, and OpenType feature controls.
- **Search** — ⌘F opens an incremental search across scrollback with live
  match highlighting and next/previous navigation.
- **Themes** — 22 built-in schemes with full per-color overrides.
- **Plugins** — `plugin.toml` manifests that add command actions, live
  side-drawer panels, HTML/JS webview surfaces, and event triggers that react to
  terminal events. No build step; install from a shared catalog. See the
  [plugin tutorial](https://wess.github.io/prompt/plugintutorial.html).
- **Macros** — record the commands you type, name them, and replay them with a
  keybinding; replay paces itself off shell-integration prompt marks.
- **Recording & export** — capture a pane to an asciinema `.cast` (⌘⇧R), then
  export it to a GIF or MP4/MOV/WebM from the File menu or with `prompt export`;
  on macOS it can render through the app's own text system for the same
  ligatures, fonts, and box-drawing you see on screen.
- **Save buffer** — write the focused terminal's whole buffer (scrollback and
  screen) to a text file from **File → Save Buffer…** (⌘S).
- **MCP server** — `prompt mcp` exposes the running terminal to Model Context
  Protocol clients (Claude Desktop, Claude Code) so an agent can run commands,
  read the screen, replay macros, and switch tabs.
- **Relay** — run a team of coding agents (Claude Code, Codex, …) that share a
  bus and message each other, launched into splits and managed from Settings →
  AI. See [`docs/relay.md`](docs/relay.md).

## Install

### macOS

Install with Homebrew:

```sh
brew install --cask wess/packages/prompt
```

Or grab the latest `Prompt.dmg` from the
[releases page](https://github.com/wess/prompt/releases) and drag it to
Applications.

### Linux

Builds are published for **x86_64** and **aarch64** on the
[releases page](https://github.com/wess/prompt/releases) in three formats:

```sh
# AppImage — self-contained, no install
chmod +x Prompt-*-x86_64.AppImage
./Prompt-*-x86_64.AppImage

# Debian / Ubuntu
sudo apt install ./prompt_*_amd64.deb

# Tarball — extract and run, or copy usr/ into /usr/local
tar xzf prompt-*-linux-x86_64.tar.gz
./prompt-*-linux-x86_64/usr/bin/prompt
```

Prompt draws its own window controls on Linux, so it needs a compositor with
client-side decoration support (Wayland or X11).

## Get started

Build and launch from source:

```sh
# Launch Prompt
cargo run -p app --release
```

That's it — Prompt opens with sensible defaults. On first run it looks for a
config file (see below); if there isn't one, it uses built-in defaults.

To build a distributable package yourself:

```sh
# macOS .app + .dmg
scripts/bundle.sh   # cargo build --release + assemble dist/Prompt.app
scripts/dmg.sh      # package dist/Prompt.dmg

# Linux .tar.gz + .deb + .AppImage (into dist/linux)
scripts/linux.sh
```

See [`docs/release.md`](docs/release.md) for signing, notarization, and how
tagged releases are cut.

## Configure

Prefer a UI? Press **⌘,** for an in-app settings panel — flip themes, font
size and style, cursor, padding, scrollback, and copy-on-select with a click,
and type directly into fields for your font family, shell, and foreground /
background colors. Changes are written straight back to your config file, so
the file stays the single source of truth.

Under the hood it's a simple `key = value` file at `~/.config/prompt/config`
(or `$XDG_CONFIG_HOME/prompt/config`) that **reloads the moment you save** —
fonts, theme, padding, cursor, and keybindings all update live.

```ini
# Fonts — repeat font-family to add fallbacks (the first is primary)
font-family = JetBrains Mono
font-family = Apple Color Emoji
font-size = 14
font-feature = +liga
font-feature = +ss01

# Look
theme = catppuccin-mocha
background = #1e1e2e
cursor-style = bar
window-padding-x = 8
window-padding-y = 8

# Behavior
shell = /bin/zsh
scrollback-limit = 10000
copy-on-select = true
# Confirm before sending a risky paste (off by default)
clipboard-paste-protection = false
# Warn on quit if a process is still running in a pane
confirm-quit = true
# OSC 133/7 hooks for jump-to-prompt + cwd inheritance (zsh/bash/fish)
shell-integration = true
# Reopen the previous window's tabs/splits/cwds on launch (off by default)
session-restore = false
# Keep the leading user@host: in tab titles (off by default; tabs show just the path)
tab-title-show-host = false

# AI — opt-in (also editable in Settings → AI); see docs/relay.md
ai-enabled = true
relay-enabled = true
relay-address = 127.0.0.1:7777
relay-default-agent = claude

# Keybindings — trigger = action[:param]; use `unbind` to remove a default
keybind = cmd+shift+c=copy_to_clipboard
keybind = ctrl+shift+page_up=scroll_page_up
```

Mistakes are reported as friendly diagnostics on launch — a bad line never
stops the rest of your config from loading.

## Plugins

Prompt loads plugins from `~/.config/prompt/plugins/*/plugin.toml` (or
`$XDG_CONFIG_HOME/prompt/plugins/*/plugin.toml`). You can also point at a
plugin directory or manifest directly:

```ini
plugin = ~/dev/prompttools
keybind = cmd+ctrl+l=plugin_command:tools/logs
```

A plugin manifest contributes commands:

```toml
id = "tools"
name = "Tools"
version = "0.1.0"

[[command]]
id = "logs"
title = "Tail logs"
run = "tail -f /tmp/app.log"
mode = "split-right"
keybind = "cmd+ctrl+l"
```

Command modes are `pane`, `tab`, `split-right`, and `split-down`. A plugin
keybinding is just a default; your config can override it or unbind it. A plugin
binding overrides a built-in with the same trigger, so prefer the `cmd+ctrl+*`
namespace to stay clear of the `cmd+shift+*` defaults.

Beyond commands, a plugin can contribute:

- **Live panels** — a `[runtime]` (any program that speaks JSON over stdio) plus
  a `[panel]`, rendered as a side-drawer UI from a block tree with clickable
  actions (see `plugins/git`).
- **Webview surfaces** — a `[webview]` hosting your own HTML/JS in a panel,
  window, or tab, wired to the terminal through a `window.Prompt` bridge (see
  `plugins/dashboard`).
- **Event triggers** — `[[trigger]]` tables that run an action (notify, run a
  command, or call the runtime) when a terminal event fires: command finished,
  directory changed, bell, exit, and more (see `plugins/alert`).

A ready-made catalog of plugins lives in [`plugins/`](plugins/), and the full
build-it-yourself guide is the
[plugin development tutorial](https://wess.github.io/prompt/plugintutorial.html).

## Macros

Record a sequence of commands and replay it later. Bind `macro_record` to a
key, trigger it to start recording, type your commands at the shell, then
trigger it again to stop — a small window asks you to name the macro. A
floating pill (red ● REC while recording, blue ▶ REPLAY while replaying) shows
the current state. Replay it by binding the `macro:<name>` action:

```ini
keybind = cmd+shift+r=macro_record
keybind = cmd+shift+1=macro:deploy
```

Macros are stored as plain text under `~/.config/prompt/macros/<name>.macro`
(one command per line, `#` comments allowed), so you can edit, rename, or
version-control them by hand. Names use lowercase letters, digits, `.`, or
`-`. Replay sends one command per line and, when your shell emits OSC 133
prompt marks (shell integration), waits for each command to finish before
sending the next; without shell integration it uses a short fixed delay.

## MCP server

`prompt mcp` runs a [Model Context Protocol](https://modelcontextprotocol.io)
server over stdio that bridges to the already-running Prompt instance. Point an
MCP client at it — for Claude Desktop, in `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "prompt": { "command": "prompt", "args": ["mcp"] }
  }
}
```

Tools exposed: `run_command` (into the focused pane, a new tab, or a split),
`send_input` (raw keystrokes), `read_screen`, `new_tab`, `split`, `list_tabs`,
`list_panes`, `focus_tab`, `list_macros`, `run_macro`, and `notify` (post a
desktop alert). The `prompt mcp` process is a thin stdio bridge; the live
terminal window does the work, reached over the same per-user socket used for
`--toggle-quick`.

**Agent attention.** A program (or agent hook) can raise a desktop notification
with an `OSC 9` / `OSC 777` / `OSC 99` escape, or by running `prompt notify
"message"`. Prompt posts a native banner and — if the pane is in the
background — lights up its tab until you look at it. Each tab also shows the
focused pane's **git branch and working directory**, so a row of agents is
legible at a glance.

## Relay

Relay runs a team of coding agents that coordinate through Prompt — a supervisor
delegating to workers — sharing one bus so they message each other and loop on
work. It's a bundled sidecar (`relay`), managed from **Settings → AI**, not run
inside the terminal process.

Turn it on under **Settings → AI**: enable AI features, enable the Relay mesh,
and optionally start it on launch. An **AI** menu then appears:

- **Agents ▸ Define Agent…** — opens a small window to pick a provider, name the
  agent, and choose a role preset or a custom brief, then runs it in a split
  wired to the bus and a register → `wait`-loop harness. Agents you define
  reappear in the same submenu for one-click relaunch.
- **Open Feed** — streams every message on the bus in a split.
- **Relay ▸** — server controls: shows whether the server is running, then
  Start / Stop / Restart it and **View Logs** (tails the server log in a split).
- **Teams ▸** — open a whole **team** at once: Prompt arranges a tile layout and
  launches the right agent in each pane.

The same `relay` binary works on its own (`relay start`, `relay launch <name>`,
`relay feed --follow`, `relay ps`, `relay stop`). **Claude** and **Codex** join
the mesh natively over MCP; **Ollama** is supported via a tool-using bridge relay
drives; Gemini/anything else run via `--cmd`. Enable and **Test** each tool in
Settings → AI.

Agents launch with a **role** — a reusable brief (and optional channels/agent)
that shapes what they do. Built-ins (`supervisor`, `frontend`, `backend`,
`reviewer`, …) ship in the box; manage your own with `relay role list|create|edit`
(an `$EDITOR` drop-in, layered project → user → built-in). **Teams** bundle a
roster with a layout (`relay team …`), and the **Workspace** menu offers layout
presets plus *Save Current Layout* for any tab.

Full details — config keys, the CLI, the MCP tools agents call, and supported
agents — are in [`docs/relay.md`](docs/relay.md).

## Default keys

| Keys | Action |
|------|--------|
| ⌘N / ⌘T | New window / new tab |
| ⌘W | Close pane |
| ⌘⌥W / ⌘⇧W / ⌘⌥⇧W | Close tab / window / all windows |
| ⌘S | Save the focused terminal's buffer to a text file |
| ⌘1…⌘9 | Go to tab |
| ⌘⇧[ / ⌘⇧] | Previous / next tab |
| ⌘D / ⌘⇧D | Split right / down |
| ⌘⌥ arrows | Move focus between splits |
| ⌘C / ⌘V | Copy / paste |
| ⌘A † | Select all (scrollback + screen) |
| ⇧ arrows | Extend the selection (falls through to the app when none) |
| ⌥⇧ ← / → | Extend the selection by a word (starts at the cursor) |
| ⌘⇧ ← / → | Extend the selection to the line start / end (starts at the cursor) |
| ⌘← / ⌘→ † | Jump to start / end of line |
| ⌥← / ⌥→ † | Jump back / forward a word |
| ⌘⌫ / ⌥⌫ † | Delete to line start / delete previous word |
| ⌘F | Search scrollback |
| ⌘⇧P | Command palette |
| ⌘⇧B | Broadcast input to all panes in the tab |
| ⌘⇧R | Record session to an asciinema `.cast` |
| ⌘↑ / ⌘↓ | Jump to previous / next prompt |
| ⌘+ / ⌘− / ⌘0 | Font size up / down / reset |
| ⌘K | Clear screen |
| ⌘, | Open settings |
| ⌘Q | Quit |

⌘ is **Command on macOS** and **Ctrl on Linux & Windows** — the same config
binding works everywhere. Every binding is a config default; override or
unbind any of them.

† macOS only — these readline navigation defaults are not registered on Linux
or Windows, where ⌘ maps to Ctrl and would shadow the shell's own
Ctrl-A/Ctrl-E/Ctrl-U/Ctrl-W bindings. Bind them yourself if you want them.

## Themes

22 built-in schemes, matched loosely (`Tokyo Night`, `tokyo-night`, and
`tokyonight` all work):

`dark`, `light`, `dracula`, `nord`, `gruvbox dark`, `gruvbox light`,
`solarized dark`, `solarized light`, `catppuccin latte`, `catppuccin mocha`,
`tokyo night`, `one dark`, `monokai`, `ayu dark`, `rose pine`, `kanagawa`,
`everforest`, `github dark`, `github light`, `material dark`, `palenight`,
`zenburn`.

Override any color in config (`background`, `foreground`,
`palette = N=#rrggbb`, …).

## Documentation

- [Full documentation site](https://wess.github.io/prompt/) — install, configuration,
  keybindings, themes, plugins, and the [plugin development tutorial](https://wess.github.io/prompt/plugintutorial.html).
- [`docs/relay.md`](docs/relay.md) — the Relay agent mesh: setup, CLI, and tools.
- [`docs/roadmap.md`](docs/roadmap.md) — what's built and what's planned.
- [`docs/parity.md`](docs/parity.md) — feature coverage and known gaps.
- [`docs/release.md`](docs/release.md) — how releases are built and shipped.

## License

Licensed under the [Apache License, Version 2.0](LICENSE).
