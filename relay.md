# Relay — the agent mesh

Relay lets independent coding-agent sessions (Claude Code, Codex, …) talk to each
other through Sinclair: a supervisor coordinating a team. Agents share one bus,
message each other directly or over channels, and **park for free** between tasks
— so an idle agent costs nothing while it waits for work.

It ships as a small sidecar binary (`relay`) bundled inside Sinclair and managed
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
- **Relay agent mesh** — run the mesh. When on, Sinclair starts the bundled `relay`
  daemon and a **Relay** menu appears.
- **Start Relay on launch** — bring the mesh up automatically when Sinclair opens.
- **Relay address** — bind address for the server (default `127.0.0.1:7777`).
  If the port is already taken, the server walks forward to the next free port
  (up to nine ahead) and records the address it actually bound in
  `server.json`; the CLI and the app follow the record, so nothing else needs
  to change.
- **Default agent** — the agent CLI used by *Launch Agent…* (default `claude`).
- **Team members run unattended** — on by default. Opening a team fills every
  split at once and you can only watch one, so members launch with their
  permission prompts bypassed; without this a member that stops to ask for
  approval sits idle until you notice its pane. Turn it off to approve each
  action yourself, pane by pane.
- **Open teams in their own window** — on by default. A team gets a window to
  itself: the first member opens as the window's first pane and the rest split
  off it, one member per pane, with a draggable divider between every pair. The
  layout you were working in is left untouched. Turn it off to open the team
  into the focused pane instead, the way it used to work.

When Relay is enabled the section also shows a live **status dot** (green = the
server is listening, red = stopped, re-probed every ~1.5s) and the **log path**.

Everything maps to plain config keys, so it round-trips through the file too:

```jsonc
// ~/.config/sinclair/settings.json
{
  "ai-enabled": true,
  "mcp-server-enabled": true,
  "relay-enabled": true,
  "relay-start-on-launch": true,
  "relay-address": "127.0.0.1:7777",
  "relay-default-agent": "claude",
  "relay-team-autonomy": true,
  "relay-team-window": true
}
```

Toggling **Relay agent mesh** starts or stops the daemon immediately;
**Start Relay on launch** only affects what happens at startup.

## Use it from Sinclair

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

The same binary backs the menu and works on its own. Sinclair points every call at
one shared state directory via `--home` (under `~/.config/sinclair/relay`); used
standalone it defaults to `./.relay` in the current directory.

```
relay start            # start the server (background daemon)
relay stop             # stop it (and its workers)
relay pause            # stop the daemon; the bus + worker roster persist on disk
relay resume           # start it again, rehydrating workers (resumes claude sessions)
relay restart
relay status

relay launch <name>    # launch an agent under relay (foreground, this terminal)
relay launch <name> --background   # ...as a server-monitored worker instead
relay launch <name> --skip-permissions   # ...unattended: bypass its prompts
relay ps               # registered agents + background workers
relay kill <name>      # stop a background worker
relay feed [--follow]  # print the message bus
relay role <list|info|create|edit|delete>   # manage role templates
relay team <list|info|create|edit|delete>   # manage teams (layout + roster)
```

`launch` flags: `--agent claude|codex|gemini`, `--role`, `--task`, `--channel`
(repeatable), `--model`, `--cwd`, `--cmd <template>`, `--background`, `--lead`
(launch interactively as the human-driven lead), `--allow-tool <rule>`
(repeatable, pre-grants a tool via `claude --allowedTools`), `--strict-mcp`
(load only relay's MCP server), `--skip-permissions` (run unattended — see
below).

`--skip-permissions` bypasses the agent's own approval prompts using whatever
flag that agent uses: `--dangerously-skip-permissions` for claude,
`approval_policy="never"` for codex, `--yolo` for gemini. It resolves *after*
the role picks the agent, so it works without naming one. `--background`
implies it (a monitored worker has no terminal to prompt in); pass it
explicitly for a pane nobody is watching, which is what the app does for team
members when `relay-team-autonomy` is on. A `--cmd` template and the ollama
bridge are left alone — relay won't guess a flag for a command it didn't
build.

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
# tools = ["Read", "Edit", "Bash(git:*)"]   # pre-granted tools (claude --allowedTools)
description = """
You own the frontend. Follow the existing component conventions and report
blockers to the supervisor.
"""
```

Then `relay launch alice --role frontend` joins `#frontend`, uses claude, and
opens with that brief.

## Teams & tiles

A **team** is a layout plus a roster: open one and Sinclair arranges a set of panes
and launches the right agent in each. Teams are Relay files (`relay team …`,
layered project → user → built-in, like roles); **tiles** (the layouts) are a
Sinclair feature, so they're useful on their own too.

From Sinclair, with AI + Relay on, the **AI menu** lists teams under **Teams ▸** —
click one and it opens in a window of its own, titled after the team (`web`).
Every pane in that window is a member, with a draggable divider between each
pair, so the whole roster reads as one divided workspace and the layout you were
already working in is untouched. Each pane's tab bar carries that member's name
and its agent-state dot, so you can see at a glance which one is working and
which is blocked. Set `relay-team-window` to `false` to open teams into the
focused pane instead. Standalone, manage teams with the CLI:

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

**Tiles** live in Sinclair under the **Workspace** menu: built-in presets (Two/Three
Columns, Two Rows, Grid, Main + Bottom Row, Main + Right Stack) open that
arrangement of shells in a new tab. **Save Current Layout…** captures the focused
tab's split structure, asks for a name, and adds it to the menu (stored as JSON
under `~/.config/sinclair/layouts/`). Layout shapes scale to any pane count, which
is how a team of N members maps onto one shape.

## How agents connect

Every launched agent receives an opening **harness** in one of two shapes:

- **Parked worker** (the default) — register under its name, join its channels,
  then enter a `wait`-loop: do work, report back, call `wait` again to stay
  reachable. Idle is nearly free.
- **Driver** — the human-driven lead. It registers and joins, then hands control
  back to the human in its terminal and only calls `wait` to gather replies
  *after* it has delegated, so the human can actually type to it. A team's first
  member (its main pane) launches as the driver, as does any role marked
  `driver = true` (the built-in `supervisor`) or a `relay launch … --lead`.

Both shapes are told explicitly that an empty *or failed* `wait` is routine and
to call it again. That line is load-bearing: without it an agent reads a park
that timed out as a failure, writes an explanation instead of another tool call,
and a headless `claude -p` worker exits the moment the model ends its turn.

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

A supervisor grows its team on demand with `spawn` (pick the provider with
`agent`, default `claude`), inspects it with `workers`, and tears a worker down
with `stop_worker`. `spawn` is bounded by a concurrent-worker cap (8) so an
over-eager supervisor can't start runaway agents; past the cap it must
`stop_worker` one first.

`wait` is the key to near-zero idle cost: it's a blocking tool call held open as
an SSE response, not a poll loop, so a parked agent burns no tokens while it
waits.

A park lasts up to four minutes and then returns an empty list, which the agent
answers by calling `wait` again — so an idle agent costs one tool call every four
minutes, not a spin loop. The deadline is deliberately short: MCP clients abort a
tool call that goes quiet, and an aborted call reaches the agent as an *error*
rather than an empty result, which is what makes an agent stop looping. Two
things keep a park alive inside that window:

- The server emits `notifications/progress` every 30s while a call is running.
  SSE keepalives alone are transport-level comments the JSON-RPC layer never
  sees, so they don't reset a client's idle timer.
- The generated `<name>.mcp.json` pins an explicit `timeout` (10 minutes) rather
  than inheriting whatever the client defaults to.

If you raise the park deadline, keep it under the shortest idle window any client
applies — `tools::WAIT_MAX_SECS` versus `CLIENT_IDLE_FLOOR_SECS`, which a unit
test enforces.

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
or per team member. Anything nobody is watching — a background worker, or a team
member in its own pane — runs with that agent's permission bypass, since there's
no one there to answer a prompt.

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
- **Sessions.** Every `/mcp` tool call must carry the `mcp-session-id` header
  returned by `initialize`; a call without one is refused. The header is what
  binds a connection to a registered name, so treating an absent one as valid
  would let two agents share a binding — the second to `register` would take over
  the first's identity, and its `wait` would drain *and ack* the other's inbox.
- **Codex credentials.** Codex reads no MCP config file, so it takes the bearer
  token by env-var *name* (`bearer_token_env_var`); relay passes the value to the
  child as `RELAY_TOKEN`. A rehydrated worker rebuilds this from the current
  token, since the token is regenerated every daemon run and cannot be persisted.
- **Ordering.** A *direct* message addressed to an agent that hasn't registered
  yet is now held and delivered the moment it does — the server pre-creates the
  recipient, so a task assigned the instant a team opens is never dropped.
  *Broadcast* and *channel* history is still not replayed: a new agent only sees
  those sent after it registers, so start workers before broadcasting, or
  re-register (the same name keeps its read cursor).
- **Delivery.** At-least-once: `wait` returns messages without consuming them,
  and the read cursor advances only once delivery is acknowledged — the MCP
  plane acks after the response is written to the stream; a non-MCP bridge
  using `/control/wait` **must** carry `ack: <last message id>` in its next
  call or it will re-receive the same batch. Rare duplicates after a dropped
  connection are possible; silent loss is not. The feed keeps the last 10k
  messages, but a registered reader's unread backlog is preserved up to a 50k
  hard cap (past that its cursor is force-bumped and the gap logged in
  `server.log`). When `wait` capacity is saturated, calls queue briefly and
  then return an explicit backoff error instead of an instant empty result.
- **Worker restarts.** A spawned worker that exits is relaunched with `--resume`,
  backing off from 3s to a 60s ceiling. The failure budget (20) counts only
  *consecutive rapid* exits: a run lasting a minute or more clears it, so a
  long-lived agent is never retired for accumulating restarts across a shift.
  Only a genuine crash-loop exhausts the budget and stops for good, which is
  reported in `workers`.
- **Feed streaming.** `relay feed --follow` consumes a server-sent event
  stream (`/control/feed/live`) instead of polling; one-shot `relay feed`
  still reads `/control/feed`.
- **MCP servers.** A launched agent loads your project `.mcp.json` and your
  global MCP servers *alongside* the relay server — relay's `--mcp-config` is
  additive, not exclusive. For a hermetic worker that sees only relay, launch it
  with `relay launch … --strict-mcp` (or set `agent-claude-args =
  --strict-mcp-config`).
- **Permissions.** Interactive agents inherit your project and user
  `.claude/settings.json` permission allow/deny lists automatically once you
  accept the workspace-trust prompt — you don't re-grant them per agent. An
  agent nobody is watching can't answer a prompt at all, so it runs with the
  bypass: background workers always, and team members unless you turn
  `relay-team-autonomy` off. Pass extra flags to every launched agent with
  `agent-claude-args` (e.g. `--permission-mode acceptEdits`).
- **Folder trust is separate.** `--dangerously-skip-permissions` covers
  per-action prompts, *not* Claude Code's first-run "do you trust this folder"
  dialog. Open a team in a directory Claude hasn't seen before and every pane
  stops on that dialog regardless of the flag. Answer it once per pane, or open
  the folder in Claude Code once beforehand, and it won't come back.
- **Context.** A long-running agent holds one growing context for its whole
  shift. You can pause and resume the mesh (`relay pause` / `relay resume`, or
  AI → Relay ▸ **Pause/Resume Mesh**): the bus and the background-worker roster
  persist on disk, and each background claude worker resumes its own session on
  restart, so its work continues rather than starting cold. Foreground
  (human-driven) agents restart fresh. See `docs/pauseresume.md`.
- **Codex/Gemini MCP** wiring is unverified — see above.

## Internals

State lives under `~/.config/sinclair/relay/` (or `$XDG_CONFIG_HOME`): the SQLite
bus (`relay.db`), the server record (`server.json`), logs, and per-agent MCP
configs. The CLI talks to the server over a small plain-HTTP control plane
(`/control/state`, `/control/feed`, `/control/spawn`, `/control/stop`), separate
from the MCP bus on `/mcp`. The crate is `crates/relay`; it's bundled into the
`.app` beside the `sinclair` executable by [`scripts/bundle.sh`](../scripts/bundle.sh).
