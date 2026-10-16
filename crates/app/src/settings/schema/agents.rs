//! Agents section: the MCP server and the shared project sandbox.

use super::{choice, list, opt, strs, text, toggle, ListKind, Section, Setting};

pub(super) fn settings() -> Vec<Setting> {
  let s = Section::Agents;
  vec![
    toggle(
      "mcp-server-enabled",
      "MCP server",
      "Expose this terminal to agents over the Model Context Protocol.",
      s,
      |o| o.mcp_server_enabled,
    ),
    toggle(
      "sandbox-enabled",
      "Shared sandbox",
      "Run this project's panes and agents inside one container, so a team shares a \
             filesystem and a toolchain. Needs Docker or Podman.",
      s,
      |o| o.sandbox_enabled,
    ),
    text(
      "sandbox-image",
      "Sandbox image",
      "Ready-made image to use as-is. Blank builds one from the base below and installs \
             the agent CLI into it.",
      s,
      |o| opt(&o.sandbox_image),
      "build one",
    ),
    text(
      "sandbox-base",
      "Sandbox base image",
      "Base the generated image layers on. The generated layer uses apt, so a non-Debian \
             base needs a ready-made image instead.",
      s,
      |o| opt(&o.sandbox_base),
      container::DEFAULT_BASE,
    ),
    choice(
      "sandbox-network",
      "Sandbox network",
      "How the sandbox reaches the network. `none` is the strictest, and cuts agents off \
             from their model APIs.",
      s,
      |o| o.sandbox_network.clone().unwrap_or_default(),
      || strs(&["bridge", "host", "none"]),
      Some("bridge"),
    ),
    text(
      "sandbox-user",
      "Sandbox user",
      "User the container runs as. `host` uses your own uid, which keeps a Linux bind \
             mount from filling with root-owned files.",
      s,
      |o| opt(&o.sandbox_user),
      "host on Linux, image default on macOS",
    ),
    text(
      "sandbox-memory",
      "Sandbox memory limit",
      "Ceiling for the whole sandbox, e.g. 8g. Blank is unlimited.",
      s,
      |o| opt(&o.sandbox_memory),
      "unlimited",
    ),
    text(
      "sandbox-cpus",
      "Sandbox CPU limit",
      "Ceiling for the whole sandbox, e.g. 4. Blank is unlimited.",
      s,
      |o| opt(&o.sandbox_cpus),
      "unlimited",
    ),
    toggle(
      "sandbox-persist",
      "Keep the sandbox running",
      "Leave the container up when the last pane using it closes. Rebuilding a toolchain \
             every session is slow, and an idle container costs nothing.",
      s,
      |o| o.sandbox_persist,
    ),
    toggle(
      "sandbox-devcontainer",
      "Use devcontainer.json",
      "Read a project's .devcontainer/devcontainer.json when it has one, and enter a \
             container an editor already started for it.",
      s,
      |o| o.sandbox_devcontainer,
    ),
    list(
      ListKind::SandboxAgents,
      "Agent CLIs installed into the generated sandbox image. Empty installs \
             Claude Code.",
      s,
    ),
    list(
      ListKind::SandboxPackages,
      "Extra apt packages baked into the generated sandbox image.",
      s,
    ),
    list(
      ListKind::SandboxSetup,
      "Extra commands baked into the generated sandbox image, one layer each.",
      s,
    ),
    list(
      ListKind::SandboxMount,
      "Extra sandbox mounts: source:target[:ro]. A bare path mounts at itself.",
      s,
    ),
    list(
      ListKind::SandboxEnv,
      "Extra environment inside the sandbox: KEY=VALUE.",
      s,
    ),
  ]
}
