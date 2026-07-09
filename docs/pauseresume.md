# Pause & resume with work persistence

Status: **complete — steps 1–3 shipped (1.21.3), step 4 shipped (1.23.0).** This is the response to
issue #4 (*feat: Pause and resume support with work persistence*). The literal
ask — pause the manager and its agents mid-task, serialize the in-flight work,
and on resume have each agent pick up from its last checkpoint — cannot be met by
Sinclair *alone*: the in-flight reasoning lives inside the agent CLI's own context,
which Sinclair doesn't own. What Sinclair *can* do — and now does — is pause/resume
the mesh, persist the worker roster, and ask the agent CLI to reload its own
session. This doc records what survives a restart, names the one hard blocker,
and tracks the build order (steps 1–3 done, step 4 next).

## Why

Long agent runs are expensive and interruptible. A user wants to close the app
(or the machine sleeps, or the daemon is killed) and later come back to the same
mesh, with each agent resuming the task it was on rather than starting cold. The
value is concentrated in one place: not losing an agent's accumulated reasoning
when the session is torn down.

## What exists today

Two persistence stories already exist, and they are very different in quality.

**Session restore** (`session-restore`) saves layout only.
`SessionState`/`TabState` (`crates/app/src/sessionstate.rs`) is
`{ layout, cwds, title }` — the split tree, each pane's working directory (in
pre-order leaf order), and the tab title. On restore
(`crates/app/src/root/persist.rs`) it spawns **fresh shells** at those cwds. It
captures no running process and no agent state, so a pane that was running an
agent restores as a bare shell. Saving is skipped entirely for a tab that holds
a webview.

**The relay bus** is the one thing that genuinely survives a kill/restart.
`relay.db` is SQLite in WAL mode (`crates/relay/src/db/mod.rs`) and persists
`agents(name, role, caps, cursor, online, last_seen)`, `subs`, and the last
`MESSAGE_RETENTION` (10,000) `messages`. The daemon is spawned detached
(`setsid`) and outlives an app restart — the app's quit path never stops it
(`crates/app/src/relay/mod.rs`).

**Same-name re-registration resumes a read cursor.** `upsert_agent`
(`crates/relay/src/db/mod.rs`) preserves `cursor` on conflict, so an agent that
re-registers under the same name picks up the messages it missed, bounded by the
10k-message retention window. A brand-new name starts its cursor at the current
message tip (`max_message_id`) and sees no history. This is already noted in
`docs/relay.md` ("re-register — the same name keeps its read cursor").

**The live worker registry is in-memory only.** `App.workers`
(`crates/relay/src/state/mod.rs`) is an `Arc<Mutex<HashMap<String, Worker>>>`,
populated by `spawn::launch` (`crates/relay/src/spawn/mod.rs`) and **not**
rehydrated from the DB on daemon startup. Restarting the daemon therefore loses
every live background worker; only their `agents` rows (name + cursor) remain in
the bus.

## The blocker

An agent's actual in-progress work — its conversation transcript, the tool calls
it has made, its partial plan — lives **inside the Claude Code / Codex CLI
process's own context**. Relay carries the message bus and nothing else; it has
no window into that transcript and no way to serialize it. The launch builders
(`crates/relay/src/cli/agent.rs`) always start a **fresh** agent, with no
`--resume`/`--continue`, so a relaunched agent begins from an empty context even
when its `agents` row and bus cursor survive.

So "pause mid-turn and resume with work intact" cannot be done by Sinclair on its
own: that state is opaque and owned by the agent CLI. `docs/relay.md` already
states the shape of this — "a long-running agent holds one growing context for
its whole shift; restart it for a fresh one" — and `docs/parity.md` tracks the
general version as **Persistent, detachable sessions** ("a multi-week
subsystem"). Checkpointing agent in-context work is the agent CLI's job; the
most Sinclair can do is ask the CLI to reload its own checkpoint (step 3).

## Scoped plan

Build order, smallest first. Each step stands on its own and is honest about
what it does *not* do.

### 1. Pause / Resume mesh control — ✓ shipped (1.21.3)

A thin surface over what already works. **Pause** = stop the daemon (SIGTERM +
`stop_all` workers) while the WAL bus persists on disk; **resume** = start it
again. Shipped as `relay pause` / `relay resume` CLI verbs and the AI → Relay ▸
**Pause Mesh** / **Resume Mesh** menu items (reusing the existing
`RelayStop`/`RelayStart` actions).

*Honest caveat:* this stops the *daemon*, not any agent's in-flight turn. With
steps 2–3 also shipped, resume now brings the workers back *and* resumes their
sessions; bus messages and read cursors survive throughout.

### 2. Persist + reload the worker registry — ✓ shipped (1.21.3)

"Resume the daemon" now brings the workers back. `spawn::launch`
(`crates/relay/src/spawn/mod.rs`) persists each background worker to a `workers`
table alongside `agents`/`subs`/`messages` (`crates/relay/src/db/mod.rs`); the
daemon rehydrates and respawns them on startup (`cli/server.rs`). A worker is
forgotten only on an explicit `stop_worker`, a one-shot completion, or terminal
failure — a graceful shutdown keeps it so `resume` restores it. This also fixes
the crash-recovery gap where a daemon restart silently dropped live workers.

### 3. Resume `claude` sessions — ✓ shipped (1.21.3)

The route to real work-intact resume. A background claude worker is assigned a
fixed session id (`claude --session-id <uuid>`) on its first launch, persisted
with its worker row (step 2). On any respawn — a crash within a run, or a daemon
resume — it relaunches with `claude --resume <uuid>` (`crates/relay/src/spawn/
mod.rs` picks the flag per attempt; the id is minted in `tools/mod.rs` /
`cli/launch.rs`). So a resumed worker re-registers under the same name
(reclaiming its bus cursor via `upsert_agent`) **and** reloads its own claude
transcript.

*Honest caveat:* correctness is bounded entirely by the agent CLI — Sinclair only
threads the id through. It applies to `claude` (session-id/resume); other
providers keep fresh context until they expose an equivalent. Foreground
(human-driven) agents aren't resumed this way — they `exec` over the shell and
own their own `/resume`.

### 4. Make session-restore agent-aware — ✓ shipped (1.23.0)

The loop is closed on the app side. An agent pane now persists its launch
command and native session id in `TabState` (`crates/app/src/sessionstate.rs`,
`crates/app/src/resume.rs`) instead of being dropped to a plain shell on restore
(`crates/app/src/root/persist.rs`). On restore it relaunches the agent
*resumed* — threading the provider's `--resume`/session id through where the
provider supports it (claude today, per step 3) — rather than spawning a bare
shell at its cwd. Plain shells restore exactly as before.

*Honest caveat:* the "with work intact" half is still bounded by the agent CLI —
Sinclair reloads the session id, the provider reloads the transcript. Providers
without a resume flag relaunch fresh at the right cwd. Foreground human-driven
agents own their own `/resume`.

## Status & what's left

All four steps have shipped. Steps 1–3 (1.21.3): you can pause and resume the
mesh (`relay pause`/`resume`, or AI → Relay ▸ Pause/Resume Mesh), the
background-worker roster survives a restart, and each claude worker resumes its
own session so its work continues rather than starting cold.

Step 4 (1.23.0): app-level `session-restore` is now agent-aware. A pane that was
running an agent is remembered as one (launch command + native session id in
`crates/app/src/sessionstate.rs` / `crates/app/src/resume.rs`) and relaunches
*resumed* on restore instead of dropping to a bare shell — closing the loop
between the app's window restore and the mesh's durable workers.

What remains is the one genuine blocker, unchanged: "pause mid-turn and resume
with the exact in-flight reasoning intact" is owned by the agent CLI, not Sinclair.
Sinclair threads the session id; the provider reloads the transcript. Real,
detachable mux-style sessions (`docs/parity.md`) remain the larger follow-up.
