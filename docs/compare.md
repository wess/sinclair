# Sinclair vs. other terminals

An honest comparison with kitty, Alacritty, Ghostty, and WezTerm — four
excellent terminals, all of which have been at this longer than Sinclair has.
If one of them already fits how you work, keep it. This page is for people
deciding where Sinclair sits, what it does differently, and what it doesn't do
yet. Feature notes on other terminals reflect their documentation at the time
of writing; check their sites for the current state.

The factual base for Sinclair's side is [`parity.md`](parity.md), the
feature-by-feature coverage map we keep as we build.

## The short version

- **kitty** is the feature powerhouse with its own protocol extensions
  (graphics, keyboard) and a scripting ecosystem (kittens).
- **Alacritty** is deliberately minimal: a very fast emulator that leaves
  tabs, splits, and everything else to tmux or your window manager.
- **Ghostty** pairs a fast core with platform-native UI (SwiftUI on macOS,
  GTK on Linux) and strong defaults.
- **WezTerm** is the everything-terminal: built-in multiplexing, SSH domains,
  Lua config, runs everywhere including Windows.
- **Sinclair** covers the modern-terminal baseline (GPU rendering,
  tabs/splits, ligatures, themes, live-reload config) and then focuses on
  something the others don't: being a workspace for coding agents — an agent
  mesh, a built-in MCP server, per-pane agent status, plus a no-build plugin
  system and built-in session recording.

## Core emulation

All five handle the essentials: truecolor, the common DEC/CSI set, alt
screen, scrollback, bracketed paste, mouse reporting, OSC 8 hyperlinks, and
OSC 52 clipboard. Differences show up at the edges:

| | Sinclair | kitty | Alacritty | Ghostty | WezTerm |
|---|---|---|---|---|---|
| GPU rendering | yes (gpui) | yes | yes | yes | yes |
| Sixel images | yes | no (own protocol) | no | no | yes |
| Kitty graphics protocol | **not yet** | yes (origin) | no | yes | yes |
| Kitty keyboard protocol | partial (press-only) | yes (origin) | partial | yes | yes |
| Synchronized output | yes | yes | yes | yes | yes |
| Shell integration (OSC 133) | yes, auto-injected | yes | no | yes, auto-injected | yes (sourced script) |
| Reflow on resize | yes | yes | yes | yes | yes |

Being specific about the gaps: Sinclair speaks sixel but not the kitty
graphics protocol yet (blocked on a parser dependency; see
[`parity.md`](parity.md)), and its kitty-keyboard support negotiates and
disambiguates but doesn't encode key-release/repeat events. Stacked combining
marks beyond the first per cell are dropped, so complex grapheme clusters
aren't fully there yet. Heavy/double/dashed box-drawing characters fall back
to the font instead of being drawn custom.

## Workspace

| | Sinclair | kitty | Alacritty | Ghostty | WezTerm |
|---|---|---|---|---|---|
| Tabs + splits | yes | yes | no (by design) | yes | yes |
| Command palette | yes | no | no | yes | yes |
| Scrollback search | yes | yes (pager) | yes | no built-in UI | yes |
| Copy/vi mode | yes | yes | yes | no | yes |
| Session restore on launch | yes | startup sessions | no | limited | via plugin |
| Detach/reattach (live mux) | **no** | no | no | no | yes |
| SSH/remote domains | **no** | ssh kitten | no | no | yes |

If you need a terminal that survives its own window closing — a real
detachable multiplexer — WezTerm has that built in, and tmux inside any of
these does it too. Sinclair restores your tab/split/cwd layout on relaunch
(and relaunches agent panes *resumed*), but it is not a live mux server.

## Configuration and extensibility

- **Sinclair**: a plain `key = value` file, reloaded live on save, with a GUI
  settings panel that writes back to the same file. Plugins are `plugin.toml`
  manifests — commands, side panels, webview surfaces, event triggers, and
  MCP tools — with no build step.
- **kitty**: `kitty.conf` plus kittens (Python) and remote control — the
  deepest scripting story of the group.
- **Alacritty**: a TOML file and intentionally no plugin system.
- **Ghostty**: a simple config file with strong defaults; extensibility is
  mostly upstream feature work rather than user plugins.
- **WezTerm**: full Lua configuration — the config *is* a program, which is
  either the best or worst part depending on taste.

## What Sinclair does that the others don't

This is the actual reason Sinclair exists, so weigh it by whether you run
coding agents at all. If you don't, this section is mostly noise to you.

- **MCP server built in.** `sinclair mcp` exposes the running terminal to any
  Model Context Protocol client: run commands into panes/tabs/splits, read
  the screen, replay macros, switch tabs, manage git worktrees.
- **Relay, an agent mesh.** A bundled sidecar that lets independent agent
  sessions (Claude Code, Codex, others) register on a shared bus, message
  each other, and park cheaply between tasks — launched into splits and
  arranged as tiled teams from the terminal.
- **Agent-legible UI.** Every pane self-reports a state (working, blocked,
  done, idle) shown on its tab and rolled up in an activity panel; tabs show
  git branch + cwd; OSC 9/777/99 raise native notifications and light up
  background tabs.
- **Git worktrees as first-class verbs.** Create/open/remove worktrees as
  keybinds or MCP calls, one isolated branch per agent, with lifecycle
  triggers for setup/teardown.
- **Recording + export in the box.** Record any pane to an asciinema v2
  `.cast`, then export GIF/MP4/WebM — on macOS through the app's own text
  renderer, so exports match what you saw on screen.
- **Typed-command macros.** Record the commands you type, replay them paced
  by shell-integration prompt marks; stored as plain text files.

Parts of this exist elsewhere in other forms — kitty's remote control and
WezTerm's Lua can script panes, and anything can run agents in plain shells.
The difference is that Sinclair treats "a grid of agents plus the human
supervising them" as the primary workload rather than something you wire up
yourself.

## What Sinclair doesn't do (yet)

Straight from [`parity.md`](parity.md) and the roadmap:

- No kitty graphics protocol (sixel only).
- No detachable sessions / live multiplexing, no SSH domains.
- Kitty keyboard protocol is press-only.
- Grapheme clusters beyond one combining mark per cell.
- Heavy/double/dashed/rounded box-drawing falls back to the font.
- No macOS status-bar (tray) item.
- macOS and Linux are the primary targets; Windows packages are published
  with each release but get far less testing.
- It's young. kitty, Alacritty, and WezTerm have a decade-plus of combined
  edge-case fixes that Sinclair simply hasn't accumulated yet. If you hit
  one of those edges, [file an issue](https://github.com/wess/sinclair/issues).

## Choosing

- You want maximum protocol features and scriptability: **kitty**.
- You want the smallest, most focused emulator and you already live in
  tmux/i3: **Alacritty**.
- You want the most platform-native feel with great defaults: **Ghostty**.
- You want built-in multiplexing, SSH domains, Lua, or Windows: **WezTerm**.
- You spend your day supervising coding agents, or you want the
  tabs-splits-themes baseline plus MCP/plugins/recording without extra
  tooling: **Sinclair**.
