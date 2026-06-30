//! Container-backed terminal tabs: spin up a fresh OS userland (Debian,
//! Ubuntu, …) in a tab by exec-ing a shell inside a Docker/Podman container.
//!
//! This crate is pure logic with no I/O beyond a `$PATH` probe for the engine
//! binary. It knows how to:
//!
//! - pick a container [`Engine`] (Docker or Podman),
//! - merge built-in OS [`Profile`]s with user-configured ones,
//! - turn a chosen profile into a [`Target`] and an `argv` that launches the
//!   container as the backing process of a tab.
//!
//! The heavy lifting (the actual VM/container) lives entirely inside the
//! engine's child process, so the host only ever spawns an argv on a pty —
//! the same seam every other tab uses. A later "Depth 2" can swap the argv for
//! a native engine-socket transport without touching the profile/picker model.
//!
//! ```
//! let engine = container::Engine::Docker;
//! let profile = &container::builtin()[0]; // Debian
//! let target = container::Target::from_profile(engine, profile, false, None);
//! assert_eq!(target.argv()[0], "docker");
//! ```

mod engine;
mod list;
mod profile;
mod target;

pub use engine::Engine;
pub use list::{attach_argv, exec_argv, parse_ps, ps_argv, Running};
pub use profile::{builtin, parse_profile, profiles, Profile};
pub use target::Target;
