# The shared sandbox

One container per project, shared by the human and every agent on a team.

A team of agents working in your checkout is a team working on your real
machine, with your real credentials, usually with permission prompts bypassed
(`relay-team-autonomy` is on by default, because a member stuck on a dialog in a
split nobody is watching does nothing). The sandbox turns that trade from a
promise into a boundary: only the project is mounted, only the credentials you
mount are present, and resources are capped.

It is **self-contained**. `docker` or `podman` is the only dependency — no VS
Code, no `devcontainer` CLI, no image to pull from a registry Sinclair
publishes. A project's `devcontainer.json` and an editor's running container are
both used when they exist, and nothing degrades when they do not.

- User guide: [`docs/sandbox.html`](sandbox.html).
- The mesh the sandbox holds: [`docs/relay.md`](relay.md).

## Enable it

**File ▸ Sandbox** (or **AI ▸ Sandbox**) ▸ *Use Sandbox for This Project*. It
writes a real setting:

```jsonc
// ~/.config/sinclair/settings.json
{ "sandbox-enabled": true }
```

"This project" is the **repository** containing the focused pane — resolved by
walking up to the nearest `.git`, so every pane in a checkout lands in the same
container rather than a pane three directories down getting one of its own. A
pane with no repository above it falls back to its own working directory.

Paths too broad to mount are refused outright: `/`, anything at depth 1
(`/Users`, `/home`), and the user's home directory. The sandbox identity-mounts
what it is given, and mounting `$HOME` would hand an agent running with its
prompts bypassed the whole machine — the exact thing the sandbox exists to
prevent.

The container name is derived from that path (`sinclair-sbx-<slug>-<hash>`), so
it is stable across sessions, unique per checkout, and reopening a project finds
the container it already had.

## The three decisions

Everything else follows from these.

### One container per project, not per agent

Agents on a team need to see each other's edits, share a build cache, and read
each other's test output. Isolation between members stays where it already
works: a git worktree each, *inside* the shared mount.

The corollary is that a pane is not what owns the container. Panes are counted;
the container outlives any single one, and is only retired when the last leaves
— and only if `sandbox-persist` is off, and only if Sinclair created it.

### The project is identity-mounted

`/Users/you/code/api` on the host is `/Users/you/code/api` inside, not
`/workspaces/api`.

Git records **absolute** paths in a worktree: `.git/worktrees/<n>/gitdir` in the
main repo and the `.git` file in the worktree point at each other. Under any
other mapping a worktree created on one side is broken on the other, and there
is no fixing it after the fact. Equal paths mean the worktree verbs
(`worktree_create` and friends) keep working unchanged from both sides, and no
path-translation layer is needed anywhere in the codebase.

Worktrees must live *under* the project (`.worktrees/<member>`), not beside it —
`../wt/x` is outside the mount and invisible to every agent.

### Relay stays on the host

Only the agent process runs inside. `build::worker` resolves the role, renders
the harness, and writes the MCP config exactly as it does for a host launch;
`relay launch --sandbox` changes only the final exec. That keeps one pipeline
for both planes, and means a wedged container cannot take the mesh down.

Two things are restated from the container's point of view:

- **The bus URL** — a loopback endpoint is rewritten to the engine's gateway
  host (`host.docker.internal`, or `host.containers.internal` under Podman),
  because inside a bridged container `127.0.0.1` is the container itself.
- **The MCP config path** — written to the host state dir as always, and handed
  to the agent as `/sandbox/relay/<name>.mcp.json`, where the sandbox mounts
  that directory read-only.

## The image

There is no Sinclair image to pull. A stock `debian:bookworm-slim` has no
`claude`, and a host binary cannot be borrowed (a macOS Mach-O will not run in a
Linux container), so the image has to provide the agent CLI.

`container::Recipe` generates a thin Dockerfile on top of whatever base is
configured and builds it locally, piping the Dockerfile in on stdin so there is
no build context on disk. The tag embeds a hash of the recipe: change the recipe
and it rebuilds, leave it alone and it is an instant cache hit.

```dockerfile
FROM debian:bookworm-slim
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl git openssh-client less procps nodejs npm ...
RUN npm install -g @anthropic-ai/claude-code
RUN mkdir -p /sandbox/home /sandbox/relay && chmod 0777 /sandbox/home
ENV HOME=/sandbox/home
```

The `chmod 0777` is load-bearing. A fresh named volume is initialised from the
image's contents at its mount point, permissions included, which is what lets
the container run as an arbitrary `--user` uid on Linux and still write its
credentials.

Set `sandbox-image` to skip generation entirely and use an image as-is. The
generated layer is `apt-get`-shaped, so a non-Debian base needs that.

## Credentials

The agent's `$HOME` is a named volume (`<container name>-home` at
`/sandbox/home`), not part of the image. A macOS keychain credential is not
readable from a Linux container, so the first agent launched inside will ask you
to sign in — once. The volume carries that login, folder-trust answers, and
shell history across image rebuilds and container recreation.

## Adopting what is already there

Resolution looks before it creates:

1. Anything labelled `devcontainer.local_folder=<project>` and running that
   Sinclair did **not** create is entered, and is never stopped or removed —
   the user's editor is very likely attached to it.
2. Otherwise Sinclair's own container for the project is started if stopped,
   created if missing.

Sinclair stamps `devcontainer.local_folder` on the containers it creates too, so
VS Code's *Reopen in Container* finds the one already running rather than
building a second beside it. `sinclair.owner=sinclair` is what separates the two
cases, and `Owner::may_remove` is checked before every stop and remove.

### What is read from `devcontainer.json`

`image` (becomes the recipe's base, so agents get the project's toolchain *plus*
the agent CLI), `containerEnv`/`remoteEnv` (settings win on conflicts),
`workspaceFolder`/`workspaceMount` (to detect identity mapping),
`remoteUser`, `mounts`, `runArgs`, `shutdownAction`, `postCreateCommand`.

A `shutdownAction` other than `none` is reported as an advisory: closing the
editor would stop a container with a team working in it. Two lines make a
project's devcontainer behave well with agents:

```jsonc
{
  "workspaceMount": "source=${localWorkspaceFolder},target=${localWorkspaceFolder},type=bind",
  "workspaceFolder": "${localWorkspaceFolder}",
  "shutdownAction": "none"
}
```

`sandbox-devcontainer: false` turns the whole behaviour off.

## Settings

| key | default | what it does |
|-----|---------|--------------|
| `sandbox-enabled` | `false` | Run this project's panes and agents in one container. |
| `sandbox-image` | — | Ready-made image, used as-is. Nothing is built. |
| `sandbox-base` | `debian:bookworm-slim` | Base the generated image layers on. |
| `sandbox-packages` | — | Extra apt packages in the generated image. |
| `sandbox-setup` | — | Extra commands baked in, one `RUN` layer each. |
| `sandbox-agents` | the default agent | Agent CLIs to install (`claude`, `codex`, `gemini`). |
| `sandbox-mount` | — | Extra mounts, `source:target[:ro]`. A bare path mounts at itself. |
| `sandbox-env` | — | Extra environment, `KEY=VALUE`. |
| `sandbox-user` | host uid on Linux | `--user` for the container. `host` is your own uid:gid. |
| `sandbox-network` | `bridge` | `bridge`, `host`, or `none`. |
| `sandbox-memory` | unlimited | `--memory` ceiling, e.g. `8g`. |
| `sandbox-cpus` | unlimited | `--cpus` ceiling, e.g. `4`. |
| `sandbox-persist` | `true` | Keep the container when the last pane closes. |
| `sandbox-devcontainer` | `true` | Read `devcontainer.json` when the project has one. |

Actions, bindable and in the command palette: `sandbox_shell`,
`toggle_sandbox`, `sandbox_start`, `sandbox_stop`, `sandbox_rebuild`,
`sandbox_status`.

## Networking

The bus lives on the host in every arrangement.

- **macOS** — `host.docker.internal` reaches host loopback through Docker
  Desktop, so relay stays bound to `127.0.0.1` and nothing is exposed.
- **Linux** — Sinclair adds `--add-host=<gateway>:host-gateway` itself, which
  resolves to the bridge IP. A relay bound strictly to `127.0.0.1` is not
  reachable there; use `sandbox-network: host` (loopback inside *is* host
  loopback, and nothing is exposed) or bind relay to the bridge gateway
  address. Never `0.0.0.0` — the bearer token is the only gate, with no
  transport encryption.
- **Podman** resolves `host.containers.internal` natively.

`sandbox-network: none` is the strictest sandbox and cuts agents off from the
bus entirely; the resolver says so as an advisory rather than failing.

## Safety

- Only the project is mounted. Nothing else of yours is there to damage.
- Only credentials you mount are present.
- A pid ceiling is always set (a supervisor can spawn eight workers into one
  container); `sandbox-memory` and `sandbox-cpus` bound the rest.
- **Never mount `/var/run/docker.sock`.** It is a routine devcontainer
  convenience and a one-line container escape — it hands an agent running with
  prompts bypassed root on the host, defeating the entire feature.

## Limits

- A worker started with the MCP `spawn` tool runs on the **host**: the relay
  daemon is not told which container a session belongs to. The identity mount
  means it sees the same files; its toolchain is the host's.
- The generated image assumes `apt-get`. Use `sandbox-image` otherwise.
- A `devcontainer.json` that builds from a Dockerfile is not built — point
  `sandbox-image` at the image it produces.
- Unix only, like the rest of the container support.

## Where the code is

- **`crates/container`** — pure argv construction, no I/O beyond a `$PATH`
  probe. `sandbox.rs` (the container model and its create/exec/stop/remove
  argvs), `image.rs` (the `Recipe`), `mount.rs`, `adopt.rs` (label discovery and
  ownership), `devcontainer.rs` (the JSONC reader, on `config`'s parser).
- **`crates/app/src/sandbox/`** — `spec.rs` resolves settings + project +
  devcontainer into a description (pure, tested); `ensure.rs` runs the engine
  calls in order.
- **`crates/app/src/root/sandbox.rs`** — the host layer: resolve once per
  window off the render path, count attached panes, retire on the last one.
- **`crates/relay/src/cli/sandbox.rs`** — the exec wrap and the endpoint
  rewrite for a sandboxed launch.
