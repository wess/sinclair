//! Built-in OS Tabs: Linux microVMs that Sinclair runs itself, so an OS Tab
//! works with no Docker or Podman installed.
//!
//! Sinclair owns every part of it — the images it pulls, the machines it
//! clones from them, and the process the VM runs in. The only outside pieces
//! are libkrun and libkrunfw (the hypervisor glue and the guest kernel), built
//! from upstream and shipped inside Sinclair itself.
//!
//! - [`reference`] parses image names the way `docker pull` does,
//! - `registry` pulls them from any OCI registry,
//! - `layer` unpacks layers with guest ownership kept in xattrs,
//! - `store` keeps images and per-tab machines under one root,
//! - `krun` boots a machine,
//! - `run` is the hidden `sinclair _vm` process mode that ties them together.
//!
//! macOS only for now: Linux needs its own answer for file ownership.

mod krun;
mod layer;
pub mod reference;
mod registry;
mod run;
mod store;

use std::path::Path;

/// True when this build can run built-in OS Tabs on this host.
pub fn available(exe: &Path) -> bool {
  cfg!(target_os = "macos") && krun::libdir(exe).is_some()
}

/// The argv that runs `image` as a tab: Sinclair's own executable in its
/// hidden `_vm` mode. `name` identifies the machine, so a persistent one is
/// found again by the next tab that uses the same name.
pub fn argv(exe: &str, image: &str, command: &str, persist: bool, name: &str) -> Vec<String> {
  let mut argv = vec![exe.to_string(), "_vm".into(), "run".into()];
  if persist {
    argv.push("--persist".into());
  }
  argv.extend(["--name".into(), name.to_string(), image.to_string()]);
  argv.extend(command.split_whitespace().map(str::to_string));
  argv
}

/// Entry for `sinclair _vm …`. `root` is the directory Sinclair keeps VM
/// state in. Returns the process exit status.
pub fn main(args: &[String], root: &Path) -> i32 {
  match args.first().map(String::as_str) {
    Some("run") => run::run(&args[1..], root),
    Some("boot") => run::boot(&args[1..]),
    _ => {
      eprintln!("usage: sinclair _vm run [--persist] [--name N] IMAGE [COMMAND…]");
      2
    }
  }
}

#[cfg(test)]
#[path = "../tests/lib.rs"]
mod tests;
