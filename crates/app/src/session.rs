//! Session spawning policy: config to options, cwd inheritance, titles.

use std::path::PathBuf;

use terminal::SessionOptions;

/// Map configuration onto session options. `inherit` is the previous
/// pane's working directory (from OSC 7); it wins over the configured
/// `working_directory`, which wins over the default (home).
pub fn options(
    opts: &config::Options,
    cols: usize,
    rows: usize,
    inherit: Option<PathBuf>,
) -> SessionOptions {
    let mut session = SessionOptions {
        cols,
        rows,
        scrollback_limit: opts.scrollback_limit,
        ..SessionOptions::default()
    };
    if let Some(command) = &opts.shell {
        let argv: Vec<String> = command.split_whitespace().map(str::to_string).collect();
        if !argv.is_empty() {
            session.spawn = pty::SpawnOptions::command(argv);
        }
    }
    session.spawn.cwd = inherit
        .or_else(|| opts.working_directory.as_ref().map(PathBuf::from))
        .or_else(home);
    session
}

/// The user's home directory, the default working directory when no pane cwd
/// is inherited and the config sets none. Without it the child would inherit
/// the launcher's cwd — e.g. `/` when Prompt is opened from Finder.
fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

/// Shell program basename, used as a pane-title fallback.
pub fn shellname(shell: Option<&str>) -> String {
    let argv0 = shell
        .and_then(|s| s.split_whitespace().next())
        .map(str::to_string)
        .unwrap_or_else(pty::default_shell);
    std::path::Path::new(&argv0)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or(argv0)
}

/// Parse an OSC 7 working-directory report into a path. Accepts a
/// `file://host/path` URL (host ignored, percent-encoding decoded) or a
/// plain absolute path. Anything else is `None`.
pub fn cwdpath(osc: &str) -> Option<PathBuf> {
    if let Some(rest) = osc.strip_prefix("file://") {
        let path = &rest[rest.find('/')?..];
        return Some(PathBuf::from(percentdecode(path)));
    }
    osc.starts_with('/').then(|| PathBuf::from(osc))
}

/// Decode `%XX` escapes; malformed escapes pass through verbatim.
fn percentdecode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let decoded = (bytes[i] == b'%'
            && i + 2 < bytes.len()
            && bytes[i + 1].is_ascii_hexdigit()
            && bytes[i + 2].is_ascii_hexdigit())
        .then(|| u8::from_str_radix(&s[i + 1..i + 3], 16).ok())
        .flatten();
        match decoded {
            Some(byte) => {
                out.push(byte);
                i += 3;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
#[path = "../tests/session.rs"]
mod tests;
