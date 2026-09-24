//! The hidden `_vm` process mode behind a built-in OS Tab.
//!
//! ```text
//! sinclair _vm run [--persist] [--name N] IMAGE [COMMAND…]
//!   └─ sinclair _vm boot <machine dir>     becomes the VM (libkrun)
//! ```
//!
//! `run` is the tab's process: it pulls and prepares the rootfs with output
//! going to the tab, then starts `boot` on the same terminal and waits. `boot`
//! is a separate process because libkrun takes the process over and exits it
//! when the guest stops, which would leave nothing behind to clean up an
//! ephemeral machine.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};

use crate::krun;
use crate::store;

const DEFAULT_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
const MEMORY_MIB: u32 = 2048;
const MAX_CPUS: usize = 4;

/// Parsed `_vm run` arguments.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Args {
  pub image: String,
  pub command: Vec<String>,
  pub name: String,
  pub persist: bool,
}

pub(crate) fn parse(args: &[String]) -> Result<Args, String> {
  let mut name = None;
  let mut persist = false;
  let mut rest = args.iter();
  let image = loop {
    match rest.next().map(String::as_str) {
      Some("--persist") => persist = true,
      Some("--name") => name = rest.next().cloned(),
      Some(flag) if flag.starts_with("--") => return Err(format!("unknown option `{flag}`")),
      Some(image) => break image.to_string(),
      None => return Err("usage: _vm run [--persist] [--name N] IMAGE [COMMAND…]".into()),
    }
  };
  Ok(Args {
    name: name.unwrap_or_else(|| store::slug(&image)),
    image,
    command: rest.cloned().collect(),
    persist,
  })
}

/// `_vm run`: returns the guest's exit status.
pub fn run(args: &[String], root: &Path) -> i32 {
  let args = match parse(args) {
    Ok(args) => args,
    Err(e) => return fail(&e),
  };
  let Ok(exe) = std::env::current_exe() else {
    return fail("cannot locate the sinclair executable");
  };
  if krun::libdir(&exe).is_none() {
    return fail("this build of Sinclair does not include the VM runtime");
  }

  store::sweep(root);
  eprintln!("Preparing {}…", args.image);
  let image = match store::image(root, &args.image, arch()) {
    Ok(image) => image,
    Err(e) => return fail(&e),
  };
  let rootfs = match store::machine(root, &args.name, &image, args.persist) {
    Ok(rootfs) => rootfs,
    Err(e) => return fail(&e),
  };
  if let Err(e) = resolv(&rootfs) {
    eprintln!("sinclair: {e}; DNS may not work in the VM");
  }

  let dir = store::machine_dir(root, &args.name);
  let term = std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".into());
  let spec = guest(&image.config, &args.command, &args.name, &term);
  if let Err(e) = fs::write(dir.join("boot"), spec.to_string()) {
    return fail(&format!("write boot spec: {e}"));
  }

  let status = Command::new(&exe).arg("_vm").arg("boot").arg(&dir).status();
  // Closing the tab hangs up the terminal. Outlive that long enough to remove
  // an ephemeral machine once the VM itself has gone.
  #[cfg(unix)]
  unsafe {
    libc::signal(libc::SIGHUP, libc::SIG_IGN);
  }
  if !args.persist {
    store::remove(root, &args.name);
  }
  match status {
    Ok(status) => status.code().unwrap_or(1),
    Err(e) => fail(&format!("start the VM: {e}")),
  }
}

/// `_vm boot <machine dir>`: become the VM. Returns only on failure.
pub fn boot(args: &[String]) -> i32 {
  let Some(dir) = args.first().map(PathBuf::from) else {
    return fail("usage: _vm boot <machine dir>");
  };
  let spec: Value = match fs::read(dir.join("boot")).map(|b| serde_json::from_slice(&b)) {
    Ok(Ok(spec)) => spec,
    _ => return fail("unreadable boot spec"),
  };
  let Some(libdir) = std::env::current_exe().ok().and_then(|e| krun::libdir(&e)) else {
    return fail("the VM runtime is missing");
  };
  let strings = |key: &str| -> Vec<String> {
    spec[key]
      .as_array()
      .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
      .unwrap_or_default()
  };
  let argv = strings("argv");
  let env = strings("env");
  let cpus = std::thread::available_parallelism().map_or(2, |n| n.get().min(MAX_CPUS)) as u8;

  let err = krun::enter(
    &libdir,
    &krun::Spec {
      rootfs: &dir.join("rootfs"),
      workdir: spec["workdir"].as_str().unwrap_or("/"),
      exec: spec["exec"].as_str().unwrap_or("/bin/sh"),
      argv: &argv,
      env: &env,
      cpus,
      memory_mib: MEMORY_MIB,
    },
  );
  fail(&err)
}

/// The guest command line and environment: `command` when given, else the
/// image's entrypoint and cmd, else `/bin/sh`.
pub(crate) fn guest(config: &Value, command: &[String], name: &str, term: &str) -> Value {
  let config = &config["config"];
  let list = |key: &str| -> Vec<String> {
    config[key]
      .as_array()
      .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
      .unwrap_or_default()
  };

  let mut line: Vec<String> = command.to_vec();
  if line.is_empty() {
    line = list("Entrypoint");
    line.extend(list("Cmd"));
  }
  if line.is_empty() {
    line = vec!["/bin/sh".into()];
  }

  let mut env = list("Env");
  let mut set_default = |key: &str, value: &str| {
    let prefix = format!("{key}=");
    if !env.iter().any(|e| e.starts_with(&prefix)) {
      env.push(format!("{prefix}{value}"));
    }
  };
  set_default("PATH", DEFAULT_PATH);
  set_default("HOME", "/root");
  set_default("TERM", term);
  set_default("HOSTNAME", name);

  let workdir = config["WorkingDir"]
    .as_str()
    .filter(|w| !w.is_empty())
    .unwrap_or("/root");

  json!({
    "exec": line[0],
    "argv": line[1..],
    "env": env,
    "workdir": workdir,
  })
}

/// Point the guest at the host's nameservers. Traffic leaves through the
/// host's own sockets (libkrun's TSI), so whatever resolves for the host
/// resolves here; a public resolver covers a host with none listed.
fn resolv(rootfs: &Path) -> Result<(), String> {
  let host = fs::read_to_string("/etc/resolv.conf").unwrap_or_default();
  let mut servers: Vec<&str> = host
    .lines()
    .filter_map(|l| l.trim().strip_prefix("nameserver"))
    .map(str::trim)
    .filter(|s| !s.is_empty() && !s.starts_with("127."))
    .collect();
  if servers.is_empty() {
    servers = vec!["1.1.1.1", "8.8.8.8"];
  }
  let body: String = servers.iter().map(|s| format!("nameserver {s}\n")).collect();

  let etc = crate::layer::resolve(rootfs, Path::new("etc")).map_err(|e| e.to_string())?;
  fs::create_dir_all(&etc).map_err(|e| e.to_string())?;
  let path = etc.join("resolv.conf");
  // Often a symlink into /run in the image; replace it with a plain file.
  let _ = fs::remove_file(&path);
  fs::write(&path, body).map_err(|e| format!("write resolv.conf: {e}"))
}

/// The OCI architecture name for this host.
fn arch() -> &'static str {
  match std::env::consts::ARCH {
    "aarch64" => "arm64",
    "x86_64" => "amd64",
    other => other,
  }
}

fn fail(msg: &str) -> i32 {
  eprintln!("sinclair: {msg}");
  1
}

#[cfg(test)]
#[path = "../tests/run.rs"]
mod tests;
