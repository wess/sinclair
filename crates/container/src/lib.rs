//! Container-backed terminals: throwaway OS tabs, and the shared project
//! sandbox that a human and a team of agents work inside together.
//!
//! This crate is pure logic with no I/O beyond a `$PATH` probe for the engine
//! binary. It knows how to:
//!
//! - pick a container [`Engine`] (Docker or Podman),
//! - merge built-in OS [`Profile`]s with user-configured ones,
//! - turn a chosen profile into a [`Target`] and an `argv` that launches the
//!   container as the backing process of a tab,
//! - describe a project [`Sandbox`] — one long-lived container several panes
//!   share — and build the `argv`s that create, enter, and retire it,
//! - generate the image [`Recipe`] that puts the agent CLIs inside it,
//! - find a container a project already has ([`adopt`]) and read its
//!   [`devcontainer`] config when one exists.
//!
//! The heavy lifting (the actual VM/container) lives entirely inside the
//! engine's child process, so the host only ever spawns an argv on a pty —
//! the same seam every other tab uses. A later "Depth 2" can swap the argv for
//! a native engine-socket transport without touching the models here.
//!
//! ```
//! let engine = container::Engine::Docker;
//! let profile = &container::builtin()[0]; // Debian
//! let target = container::Target::from_profile(engine, profile, false, None);
//! assert_eq!(target.argv()[0], "docker");
//! ```
//!
//! ```
//! // The sandbox identity-mounts the project, so a path means the same thing
//! // inside and out and git worktrees stay valid from both sides.
//! let sandbox = container::Sandbox::for_project(
//!     container::Engine::Docker,
//!     "/Users/wess/code/api",
//!     "sinclair-sandbox:abc",
//! );
//! assert!(sandbox.mounts[0].is_identity());
//! ```

pub mod adopt;
pub mod devcontainer;
mod engine;
mod hash;
mod image;
mod list;
mod mount;
mod profile;
mod sandbox;
mod target;

pub use adopt::{Found, Owner};
pub use devcontainer::DevContainer;
pub use engine::Engine;
pub use image::{exists_argv, Recipe, DEFAULT_BASE};
pub use list::{attach_argv, parse_ps, ps_argv, Running};
pub use mount::Mount;
pub use profile::{builtin, parse_profile, profiles, Profile};
pub use sandbox::{
    home_volume_for, name_for, parse_state, state_argv, Limits, Network, Sandbox, State, HOME_DIR,
    LABEL_OWNER, LABEL_PROJECT, LABEL_SANDBOX, OWNER_SINCLAIR, RELAY_DIR,
};
pub use target::Target;
