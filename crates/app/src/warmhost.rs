//! Warm native-tier plugin processes. A `[runtime] persistent = true` plugin is
//! a long-lived stdio server: spawned once, it reads newline-delimited JSON
//! requests and writes newline-delimited JSON responses in a loop, so panels /
//! triggers / tool calls cost a round-trip instead of a process spawn. A
//! one-shot plugin (the default) keeps the spawn-per-event path in `pluginhost`.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

/// One resident plugin process with its stdio kept open.
struct WarmProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl WarmProcess {
    fn spawn(program: &str, args: &[String], cwd: &Path) -> Result<Self, String> {
        let mut child = Command::new(program)
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("spawn `{program}`: {e}"))?;
        let stdin = child.stdin.take().ok_or("no stdin")?;
        let stdout = BufReader::new(child.stdout.take().ok_or("no stdout")?);
        Ok(Self { child, stdin, stdout })
    }

    /// Send one JSON request line, read one JSON response line.
    fn request(&mut self, json: &str) -> Result<String, String> {
        writeln!(self.stdin, "{json}").map_err(|e| e.to_string())?;
        self.stdin.flush().map_err(|e| e.to_string())?;
        let mut line = String::new();
        let n = self.stdout.read_line(&mut line).map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("plugin process closed its output".into());
        }
        Ok(line.trim_end().to_string())
    }

    fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

impl Drop for WarmProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Resident warm processes keyed by plugin id, respawned on death.
#[derive(Default)]
pub struct WarmPlugins {
    procs: HashMap<String, WarmProcess>,
}

impl WarmPlugins {
    pub fn new() -> Self {
        Self::default()
    }

    /// Send `json` to plugin `id`, spawning (or respawning) its process first.
    pub fn request(
        &mut self,
        id: &str,
        program: &str,
        args: &[String],
        cwd: &Path,
        json: &str,
    ) -> Result<String, String> {
        let stale = self.procs.get_mut(id).map(|p| !p.alive()).unwrap_or(true);
        if stale {
            self.procs
                .insert(id.to_string(), WarmProcess::spawn(program, args, cwd)?);
        }
        let proc = self.procs.get_mut(id).expect("just ensured");
        match proc.request(json) {
            Ok(response) => Ok(response),
            Err(e) => {
                // A broken pipe means the process died mid-request; drop it so the
                // next call respawns.
                self.procs.remove(id);
                Err(e)
            }
        }
    }

    /// Drop a resident process (e.g. on plugin disable/reload). Exercised by
    /// tests; wired to plugin lifecycle in the registry stage's UI work.
    #[allow(dead_code)]
    pub fn evict(&mut self, id: &str) {
        self.procs.remove(id);
    }
}

#[cfg(test)]
#[path = "../tests/warmhost.rs"]
mod tests;
