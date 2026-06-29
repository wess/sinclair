# Relay — the agent mesh

Relay lets independent coding-agent sessions (Claude Code, Codex, …) talk to each
other through Prompt: a supervisor coordinating a team. Agents share one bus,
message each other directly or over channels, and **park for free** between tasks
— so an idle agent costs nothing while it waits for work.

It ships as a small sidecar binary (`relay`) bundled inside Prompt and managed
from **Settings → AI**. Relay is its own process, not part of the terminal: a
crash or hang in the mesh can never take down your terminal, and the same binary
works standalone (over SSH, in CI, with any terminal).

- Built with Rust + tokio + axum; SQLite (sqlx) is the shared bus.
- MCP transport is Streamable HTTP, so many agent sessions share one server.
- Every parameter comes from settings and is passed explicitly — Relay reads no
  environment variables.

## Enable it

**Settings → AI** (⌘,):

- **Enable AI features** — master switch. Off means no MCP server, no mesh, no
  network activity.
- **MCP server** — expose this terminal to agents (see [the MCP server](../README.md#mcp-server)).
- **Relay agent mesh** — run the mesh. When on, Prompt starts the bundled `relay`
  daemon and a **Relay** menu appears.
- **Start Relay on launch** — bring the mesh up automatically when Prompt opens.
- **Relay address** — bind address for the server (default `127.0.0.1:7777`).
- **Default agent** — the agent CLI used by *Launch Agent…* (default `claude`).

When Relay is enabled the section also shows a live **status dot** (green = the
server is listening, red = stopped, re-probed every ~1.5s) and the **log path**.

Everything maps to plain config keys, so it round-trips through the file too:

```ini
ai-enabled = true
mcp-server-enabled = true
relay-enabled = true
relay-start-on-launch = true
relay-address = 127.0.0.1:7777
relay-default-agent = claude
```

Toggling **Relay agent mesh** starts or stops the daemon immediately;
**Start Relay on launch** only affects what happens at startup.

## Use it from Prompt

With AI enabled, an **AI** menu appears; with Relay on it offers:

- **Agents ▸** — **Define Agent…** opens the New Agent dialog: pick a provider
  (from the enabled tools), name it, and either choose a role preset (DevOps,
  frontend, …) or toggle Custom to describe it. Create adds it to the current
  workspace as a split and **saves the definition**, so it reappears in this
  submenu — click a saved agent to relaunch it.
- **Open Feed** — opens a split streaming every message on the bus (who said what
  to whom), the one view of cross-agent traffic.
- **Relay ▸** — server controls: a live running/stopped status line, then **Start
  Server**, **Stop Server**, **Restart Server**, and **View Logs** (tails the relay
  server log, `server.log`, in a split).
- **Teams ▸** — open a whole team at once (see [Teams & tiles](#teams--tiles)).

A typical session: launch a `supervisor`, launch a couple of workers
(`frontend`, `backend`), and drive the supervisor — it delegates over the bus
while you watch the feed.

## The `relay` CLI

The same binary backs the menu and works on its own. Prompt points every call at
one shared state directory via `--home` (under `~/.config/prompt/relay`); used
standalone it defaults to `./.relay` in the current directory.

```
relay start            # start the server (background daemon)
relay stop             # stop it (and its workers)
relay restart
relay status

relay launch <name>    # launch an agent under relay (foreground, this terminal)
relay launch <name> --background   # ...as a server-monitored worker instead
relay ps               # registered agents + background workers
relay kill <name>      # stop a background worker
relay feed [--follow]  # print the message bus
relay role <list|info|create|edit|delete>   # manage role templates
relay team <list|info|create|edit|delete>   # manage teams (layout + roster)
```

`launch` flags: `--agent claude|codex|gemini`, `--role`, `--task`, `--channel`
(repeatable), `--model`, `--cwd`, `--cmd <template>`, `--background`, `--lead`
(launch interactively as the human-driven lead).

- **Foreground** `launch` replaces the calling shell with the agent (you see and
  steer it).
- **`--background`** hands it to the server, which monitors it (respawns on
  crash) and logs its output under the state directory.

## Roles

A **role** is a reusable identity an agent launches with — a brief (what it owns,
how it coordinates) plus optional defaults. It's distinct from `--task`: the role
is durable ("the frontend owner"), the task is the one-off assignment ("build the
login page"). At launch the brief is injected into the harness; explicit flags
override the role's defaults, and `--channel` flags merge with the role's.

Roles resolve highest-priority-first:

1. **project** — `./.relay/roles/<name>.toml` (travels with the repo)
2. **user** — `~/.config/relay/roles/<name>.toml` (or `$XDG_CONFIG_HOME`)
3. **built-in** — embedded: `supervisor`, `worker`, `frontend`, `backend`,
   `reviewer`, `devops`, `qa`

Manage them like `git` config — CRUD with an `$EDITOR` drop-in:

```
relay role list                 # all roles + their source
relay role info <name>          # show the resolved role
relay role create <name>        # new role in $EDITOR (--user for the user dir)
relay role edit <name>          # edit in $EDITOR; copies a built-in/lower layer first
relay role delete <name>        # remove a project (or --user) role file
```

`create`/`edit` open `$VISUAL`/`$EDITOR` (default `vi`) on a seeded file and only
save once it parses. Editing a built-in copies it into your project (or `--user`)
dir first, so built-ins stay pristine. Files are TOML:

```toml
name = "frontend"
channels = ["frontend"]   # auto-joined at launch
agent = "claude"          # default agent CLI (optional)
# model = "claude-..."    # default model (optional)
# driver = true           # human-driven lead: stay interactive, don't park on wait
description = """
You own the frontend. Follow the existing component conventions and report
blockers to the supervisor.
"""
```

Then `relay launch alice --role frontend` joins `#frontend`, uses claude, and
opens with that brief.

## Teams & tiles

A **team** is a layout plus a roster: open one and Prompt arranges a set of panes
and launches the right agent in each. Teams are Relay files (`relay team …`,
layered project → user → built-in, like roles); **tiles** (the layouts) are a
Prompt feature, so they're useful on their own too.

From Prompt, with AI + Relay on, the **AI menu** lists teams under **Teams ▸** —
click one to open it in a fresh tab — the agent panes live under that one tab,
which is titled after the team (`web`). Standalone, manage teams with the CLI:

```
relay team list                 # all teams + source
relay team info <name>          # roster + layout (add --json for tooling)
relay team create <name>        # new team in $EDITOR (--user for the user dir)
relay team edit <name>          # edit; copies a built-in/lower layer first
relay team delete <name>
```

A team is TOML — a `layout` shape and ordered members (first = the main pane):

```toml
name = "web"
layout = "main-bottom"   # columns | rows | grid | main-bottom | main-right

[[member]]
name = "lead"
role = "supervisor"

[[member]]
name = "frontend"
role = "frontend"
```

Built-ins: `web` (lead + frontend + backend + reviewer, main-bottom) and `pair`
(driver + reviewer, columns).

**Tiles** live in Prompt under the **Workspace** menu: built-in presets (Two/Three
Columns, Two Rows, Grid, Main + Bottom Row, Main + Right Stack) open that
arrangement of shells in a new tab. **Save Current Layout…** captures the focused
tab's split structure, asks for a name, and adds it to the menu (stored as JSON
under `~/.config/prompt/layouts/`). Layout shapes scale to any pane count, which
is how a team of N members maps onto one shape.

## How agents connect

Every launched agent receives an opening **harness** in one of two shapes:

- **Parked worker** (the default) — register under its name, join its channels,
  then enter a `wait`-loop: do work, report back, call `wait` again to stay
  reachable. Idle costs nothing.
- **Driver** — the human-driven lead. It registers and joins, then hands control
  back to the human in its terminal and only calls `wait` to gather replies
  *after* it has delegated, so the human can actually type to it. A team's first
  member (its main pane) launches as the driver, as does any role marked
  `driver = true` (the built-in `supervisor`) or a `relay launch … --lead`.

Coordination happens through MCP tools the agent calls:

| tool | purpose |
|------|---------|
| `register` | join under a unique name (first, once) |
| `send` / `post` / `broadcast` | message an agent / channel / everyone |
| `join` / `leave` | subscribe / unsubscribe from a channel |
| `wait` | block (free) until messages arrive, then return them |
| `inbox` | pending messages now, without blocking |
| `agents` / `channels` / `whoami` | introspection |
| `spawn` / `workers` / `stop_worker` | a supervisor agent grows/manages its own team |

`wait` is the key to zero idle cost: it's a single blocking tool call (held open
as an SSE response with keepalives), not a poll loop, so a parked agent burns no
tokens until a message actually arrives.

## Agents

Tools join the mesh at one of three integration tiers:

- **MCP-native** — the CLI speaks streamable-HTTP MCP and runs its own agentic
  loop, doing the work itself. **claude** (`--mcp-config`) and **codex**
  (`-c mcp_servers.relay.url=…`) are both first-class here.
- **Bridged** — for backends that aren't agents. **ollama** has no MCP client, so
  relay runs its own loop (`relay agent ollama`): it registers, waits, runs a
  tool-using turn against the model (`localhost:11434/api/chat` — the model can
  call `send`/`post`/`broadcast`), and sends its reply back. Requires
  `ollama serve` running and a pulled model.
- **Terminal** — any other CLI via `--cmd 'tool --mcp {mcp} -- {prompt}'`; runs in
  a pane without bus coordination. Placeholders: `{prompt}` (harness), `{mcp}`
  (config path), `{url}` (bus URL), `{name}`. **gemini** ships as a template.

Pick the agent per launch with `--agent claude|codex|ollama|gemini` (or `--cmd`),
or per team member. CLI background agents run with `--dangerously-skip-permissions`
since they can't answer prompts.

In **Settings → AI → Agent tools**, each tool has an enable toggle and a **Test**
button that checks it's reachable (CLI `--version`, or the Ollama API port).

## Notes & limits

- **Authentication.** Every request to `/mcp` and `/control/*` must carry the
  server's bearer token (`Authorization: Bearer …`); only `/health` is open. The
  token is generated at startup and stored in `server.json`, which — along with
  the bus DB and logs — is written owner-only (0600) inside a 0700 state dir, so
  only the same user can read it. The CLI, the app, and launched agents pick it
  up automatically (the latter via the generated `*.mcp.json`).
- **Localhost trust.** The token gates cross-user access on a shared host, but
  the bus still trusts the self-asserted `from`/`name` within a single user's
  mesh. Binding `relay-address` to a public interface is still discouraged: the
  token is the only gate, with no transport encryption.
- **Ordering.** A new agent only sees messages sent after it registers. Start
  workers before dispatching, or re-register (the same name keeps its read
  cursor).
- **Context.** A long-running agent holds one growing context for its whole
  shift; restart it for a fresh one.
- **Codex/Gemini MCP** wiring is unverified — see above.

## Internals

State lives under `~/.config/prompt/relay/` (or `$XDG_CONFIG_HOME`): the SQLite
bus (`relay.db`), the server record (`server.json`), logs, and per-agent MCP
configs. The CLI talks to the server over a small plain-HTTP control plane
(`/control/state`, `/control/feed`, `/control/spawn`, `/control/stop`), separate
from the MCP bus on `/mcp`. The crate is `crates/relay`; it's bundled into the
`.app` beside the `prompt` executable by [`scripts/bundle.sh`](../scripts/bundle.sh).
