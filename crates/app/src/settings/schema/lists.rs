//! Repeated options: each renders as an add/remove list group.

use super::{list, Section, Setting};
use config::Options;

/// A repeated option rendered as an editable list with add/remove.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ListKind {
    FontFamily,
    FontFeature,
    Palette,
    Plugin,
    Keybind,
    AgentTool,
    Redact,
    Trigger,
    Snippet,
    Profile,
    Container,
    SandboxMount,
    SandboxEnv,
    SandboxPackages,
    SandboxSetup,
    SandboxAgents,
}

impl ListKind {
    /// The settings.json key.
    pub fn key(self) -> &'static str {
        match self {
            ListKind::FontFamily => "font-family",
            ListKind::FontFeature => "font-feature",
            ListKind::Palette => "palette",
            ListKind::Plugin => "plugin",
            ListKind::Keybind => "keybind",
            ListKind::AgentTool => "agent-custom",
            ListKind::Redact => "redact",
            ListKind::Trigger => "trigger",
            ListKind::Snippet => "snippet",
            ListKind::Profile => "profile",
            ListKind::Container => "container",
            ListKind::SandboxMount => "sandbox-mount",
            ListKind::SandboxEnv => "sandbox-env",
            ListKind::SandboxPackages => "sandbox-packages",
            ListKind::SandboxSetup => "sandbox-setup",
            ListKind::SandboxAgents => "sandbox-agents",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ListKind::FontFamily => "Font family",
            ListKind::FontFeature => "Font features",
            ListKind::Palette => "Color palette",
            ListKind::Plugin => "Plugin directories",
            ListKind::Keybind => "Keybindings",
            ListKind::AgentTool => "Custom tools",
            ListKind::Redact => "Redact secrets on copy",
            ListKind::Trigger => "Output triggers",
            ListKind::Snippet => "Snippets",
            ListKind::Profile => "Profiles",
            ListKind::Container => "OS profiles",
            ListKind::SandboxMount => "Sandbox mounts",
            ListKind::SandboxEnv => "Sandbox environment",
            ListKind::SandboxPackages => "Sandbox packages",
            ListKind::SandboxSetup => "Sandbox setup commands",
            ListKind::SandboxAgents => "Sandbox agent CLIs",
        }
    }

    pub fn add_label(self) -> &'static str {
        match self {
            ListKind::FontFamily => "Add font",
            ListKind::FontFeature => "Add feature",
            ListKind::Palette => "Add color",
            ListKind::Plugin => "Add plugin",
            ListKind::Keybind => "Add binding",
            ListKind::AgentTool => "Add tool",
            ListKind::Redact => "Add pattern",
            ListKind::Trigger => "Add trigger",
            ListKind::Snippet => "Add snippet",
            ListKind::Profile => "Add profile",
            ListKind::Container => "Add profile",
            ListKind::SandboxMount => "Add mount",
            ListKind::SandboxEnv => "Add variable",
            ListKind::SandboxPackages => "Add package",
            ListKind::SandboxSetup => "Add command",
            ListKind::SandboxAgents => "Add agent",
        }
    }

    pub fn placeholder(self) -> &'static str {
        match self {
            ListKind::FontFamily => "Font name",
            ListKind::FontFeature => "-liga or +ss01",
            ListKind::Palette => "0=#1d1f21",
            ListKind::Plugin => "~/.config/sinclair/plugins/name",
            ListKind::Keybind => "cmd+shift+t=new_tab",
            ListKind::AgentTool => "mytool|/path/to/bin {prompt} --mcp {mcp}",
            ListKind::Redact => "regex, e.g. sk-[A-Za-z0-9]{20,}",
            ListKind::Trigger => "error|Build failed",
            ListKind::Snippet => "deploy | git push origin main",
            ListKind::Profile => "prod | ssh prod.example.com",
            ListKind::Container => "ubuntu | ubuntu:24.04 | bash",
            ListKind::SandboxMount => "~/.ssh:/sandbox/home/.ssh:ro",
            ListKind::SandboxEnv => "DATABASE_URL=postgres://localhost/dev",
            ListKind::SandboxPackages => "ripgrep",
            ListKind::SandboxSetup => "cargo install just",
            ListKind::SandboxAgents => "claude",
        }
    }

    /// The current entries, as the strings the user edits.
    pub fn values(self, o: &Options) -> Vec<String> {
        match self {
            ListKind::FontFamily => o.font_family.clone(),
            ListKind::FontFeature => o.font_feature.clone(),
            ListKind::Palette => o.palette.iter().map(|(n, c)| format!("{n}={c}")).collect(),
            ListKind::Plugin => o.plugin.clone(),
            ListKind::Keybind => {
                let (binds, _) = config::resolve(&o.keybind);
                binds.iter().map(|kb| kb.config_line()).collect()
            }
            ListKind::AgentTool => o.agent_custom.clone(),
            ListKind::Redact => o.redact.clone(),
            ListKind::Trigger => o.trigger.clone(),
            ListKind::Snippet => o.snippet.clone(),
            ListKind::Profile => o.profile.clone(),
            ListKind::Container => o.container.clone(),
            ListKind::SandboxMount => o.sandbox_mount.clone(),
            ListKind::SandboxEnv => o.sandbox_env.clone(),
            ListKind::SandboxPackages => o.sandbox_packages.clone(),
            ListKind::SandboxSetup => o.sandbox_setup.clone(),
            ListKind::SandboxAgents => o.sandbox_agents.clone(),
        }
    }

    /// Translate the edited entries into the values to persist. Keybinds
    /// collapse to the minimal diff against the defaults; every other list
    /// is written verbatim.
    pub fn to_values(self, entries: &[String]) -> Vec<String> {
        match self {
            ListKind::Keybind => {
                let desired: Vec<config::Keybind> = entries
                    .iter()
                    .filter_map(|e| config::parse_keybind(e.trim()).ok())
                    .collect();
                config::diff_from_defaults(&desired)
            }
            _ => clean(entries),
        }
    }
}

/// Drop blank entries and trim surrounding whitespace.
fn clean(entries: &[String]) -> Vec<String> {
    entries
        .iter()
        .map(|e| e.trim().to_string())
        .filter(|e| !e.is_empty())
        .collect()
}

pub(super) fn keyboard_settings() -> Vec<Setting> {
    vec![list(
        ListKind::Keybind,
        "trigger=action overrides; chain keys with > for a chord.",
        Section::Keyboard,
    )]
}

pub(super) fn plugin_settings() -> Vec<Setting> {
    vec![list(
        ListKind::Plugin,
        "Directories or manifest paths Sinclair loads plugins from.",
        Section::Plugins,
    )]
}
