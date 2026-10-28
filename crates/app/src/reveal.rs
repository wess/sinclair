//! Resolving a clicked path candidate and showing it in the desktop file
//! manager.
//!
//! The scanner in `vt::path` reports anything path-shaped, so this is where a
//! candidate becomes a decision: resolved against the pane's working directory,
//! then checked against the filesystem. Text that merely looked like a path
//! fails that check and the click does nothing, which is the only reason a
//! pattern that loose is safe to click on.

use std::path::{Path, PathBuf};

/// Resolve a candidate against `cwd` and return it only if it exists.
///
/// `~` expands from `$HOME`; a relative path is taken against the pane's
/// working directory, which is what makes the paths in build output clickable
/// — they are printed relative to wherever the build ran.
pub fn resolve(candidate: &str, cwd: Option<&Path>) -> Option<PathBuf> {
  let expanded = if candidate == "~" {
    home()?
  } else if let Some(rest) = candidate.strip_prefix("~/") {
    home()?.join(rest)
  } else {
    PathBuf::from(candidate)
  };
  let full = if expanded.is_absolute() {
    expanded
  } else {
    cwd?.join(expanded)
  };
  // Resolves `..` and symlinks, and is the existence check in one call.
  // A path that does not resolve is not a path, whatever it looked like.
  std::fs::canonicalize(full).ok()
}

fn home() -> Option<PathBuf> {
  std::env::var_os("HOME").map(PathBuf::from)
}

/// Show `path` in the desktop file manager, selected where the platform can
/// select it.
pub fn reveal(path: &Path) {
  spawn(path, command);
}

/// Hand `path` to whatever the desktop opens that kind of file with — what an
/// open-modifier click on a path does, matching what the same click does to a
/// URL. A directory opens in the file manager.
///
/// The path is passed as an argument rather than built into a `file://` URL,
/// so spaces and other characters that would need percent-encoding cannot be
/// mangled on the way.
///
/// Anything that would *run* rather than open — an app bundle, a `.command`
/// script, an executable — is revealed instead. The text being clicked came
/// from whatever printed it, so a click must never be a way to execute code.
pub fn open(path: &Path) {
  if launches(path) {
    reveal(path);
  } else {
    spawn(path, open_command);
  }
}

/// Extensions the desktop treats as something to run or install, compared
/// case-insensitively. Directories with these are bundles, not folders.
const LAUNCHABLE: &[&str] = &[
  "app",
  "appex",
  "applescript",
  "bundle",
  "command",
  "desktop",
  "fileloc",
  "inetloc",
  "jar",
  "kext",
  "mpkg",
  "pkg",
  "plugin",
  "prefpane",
  "saver",
  "scpt",
  "scptd",
  "terminal",
  "tool",
  "url",
  "webloc",
  "workflow",
  "appimage",
  "sh",
  "bash",
  "zsh",
  "fish",
  "py",
  "rb",
  "pl",
];

/// Whether handing `path` to the desktop would execute something: a bundle
/// or script by extension, or a regular file with any execute bit set (macOS
/// runs an extensionless executable in a terminal on `open`).
///
/// Judged on the resolved path too, so a harmless-looking symlink to an app
/// bundle cannot slip through on its own name.
pub fn launches(path: &Path) -> bool {
  let real = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
  launchable_name(path) || launchable_name(&real) || executable(&real)
}

fn launchable_name(path: &Path) -> bool {
  path
    .extension()
    .and_then(|e| e.to_str())
    .is_some_and(|e| LAUNCHABLE.iter().any(|l| l.eq_ignore_ascii_case(e)))
}

fn executable(path: &Path) -> bool {
  let Ok(meta) = std::fs::metadata(path) else {
    return false;
  };
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    meta.is_file() && meta.permissions().mode() & 0o111 != 0
  }
  #[cfg(not(unix))]
  {
    let _ = meta;
    false
  }
}

/// Run a launcher off the render thread: a cold app launch can block for
/// seconds, and every caller here is a click handler.
fn spawn(path: &Path, build: fn(&Path) -> Option<std::process::Command>) {
  let path = path.to_path_buf();
  std::thread::spawn(move || {
    let _ = build(&path).map(|mut c| c.spawn());
  });
}

#[cfg(target_os = "macos")]
fn open_command(path: &Path) -> Option<std::process::Command> {
  let mut c = std::process::Command::new("open");
  c.arg(path);
  Some(c)
}

#[cfg(target_os = "linux")]
fn open_command(path: &Path) -> Option<std::process::Command> {
  let mut c = std::process::Command::new("xdg-open");
  c.arg(path);
  Some(c)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn open_command(_path: &Path) -> Option<std::process::Command> {
  None
}

#[cfg(target_os = "macos")]
fn command(path: &Path) -> Option<std::process::Command> {
  let mut c = std::process::Command::new("open");
  c.arg("-R").arg(path);
  Some(c)
}

/// Linux has no portable "reveal this file": the freedesktop file-manager
/// interface is the closest thing, and `xdg-open` on the parent directory is
/// what is left when no one answers it. A directory opens as itself either way.
#[cfg(target_os = "linux")]
fn command(path: &Path) -> Option<std::process::Command> {
  let uri = format!("file://{}", path.display());
  // `dbus-send` fails when nothing implements the interface, and whether
  // anything does is not knowable up front — so the fallback is chained in
  // the shell rather than chosen here.
  let dir = if path.is_dir() {
    path.to_path_buf()
  } else {
    path.parent()?.to_path_buf()
  };
  let mut sh = std::process::Command::new("sh");
  sh.arg("-c").arg(format!(
    "dbus-send --session --print-reply --dest=org.freedesktop.FileManager1 \
         /org/freedesktop/FileManager1 org.freedesktop.FileManager1.ShowItems \
         array:string:{} string: >/dev/null 2>&1 || xdg-open {}",
    shell_quote(&uri),
    shell_quote(&dir.display().to_string()),
  ));
  Some(sh)
}

#[cfg(target_os = "linux")]
fn shell_quote(s: &str) -> String {
  format!("'{}'", s.replace('\'', r"'\''"))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn command(_path: &Path) -> Option<std::process::Command> {
  None
}

#[cfg(test)]
#[path = "../tests/reveal.rs"]
mod tests;
