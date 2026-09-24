//! What runs an OS Tab: an installed container engine, or Sinclair's own
//! microVM (the `vm` crate) when there is none, so an OS Tab always works.
//!
//! `container-engine` picks: `docker`/`podman` pin an engine, `builtin` pins
//! the VM, and `auto` (the default) prefers an installed engine and falls back
//! to the VM.

/// The `container-engine` value that selects the built-in VM.
pub const BUILTIN: &str = "builtin";

pub enum Runner {
  Engine(container::Engine),
  Builtin(String),
}

/// Resolve the runner for `pref`, or `None` when nothing can run an OS Tab.
pub fn runner(pref: Option<&str>) -> Option<Runner> {
  let builtin = || {
    let exe = std::env::current_exe().ok()?;
    vm::available(&exe).then(|| Runner::Builtin(exe.to_string_lossy().into_owned()))
  };
  if pref.is_some_and(|p| p.trim().eq_ignore_ascii_case(BUILTIN)) {
    return builtin();
  }
  container::Engine::resolve(pref)
    .map(Runner::Engine)
    .or_else(builtin)
}
