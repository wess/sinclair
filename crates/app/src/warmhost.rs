//! Warm native-tier plugin processes. A `[runtime] persistent = true` plugin is
//! a long-lived stdio server: spawned once, it reads newline-delimited JSON
//! requests and writes newline-delimited JSON responses in a loop, so panels /
//! triggers / tool calls cost a round-trip instead of a process spawn. A
//! one-shot plugin (the default) keeps the spawn-per-event path in `pluginhost`.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Kill a warm plugin whose reply takes longer than this — the same deadline
/// the one-shot path enforces, so a persistent plugin that never answers can't
/// hang the MCP bridge forever. The killed process respawns on the next call.
const TIMEOUT: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(100);

/// One resident plugin process with its stdio kept open.
struct WarmProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl WarmProcess {
    fn spawn(program: &str, args: &[String], cwd: &Path) -> Result<Self, String> {
        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        // Own process group, so the timeout kill also reaps grandchildren.
        crate::pluginhost::spawn_in_own_group(&mut cmd);
        let mut child = cmd.spawn().map_err(|e| format!("spawn `{program}`: {e}"))?;
        let stdin = child.stdin.take().ok_or("no stdin")?;
        let stdout = BufReader::new(child.stdout.take().ok_or("no stdout")?);
        Ok(Self { child, stdin, stdout })
    }

    /// Send one JSON request line, read one JSON response line. The read is
    /// guarded by a watchdog: past [`TIMEOUT`] the process (group) is killed,
    /// which unblocks the read with EOF and surfaces an error — the caller
    /// then drops this process and the next request respawns it.
    fn request(&mut self, json: &str) -> Result<String, String> {
        writeln!(self.stdin, "{json}").map_err(|e| e.to_string())?;
        self.stdin.flush().map_err(|e| e.to_string())?;
        let pid = self.child.id();
        let done = Arc::new(AtomicBool::new(false));
        let flag = done.clone();
        let watchdog = std::thread::spawn(move || {
            let mut waited = Duration::ZERO;
            while waited < TIMEOUT {
                if flag.load(Ordering::Relaxed) {
                    return false;
                }
                std::thread::sleep(POLL);
                waited += POLL;
            }
            if flag.load(Ordering::Relaxed) {
                return false;
            }
            crate::pluginhost::force_kill(pid);
            true
        });
        let mut line = String::new();
        let read = self.stdout.read_line(&mut line);
        done.store(true, Ordering::Relaxed);
        let timed_out = watchdog.join().unwrap_or(false);
        if timed_out {
            return Err(format!("plugin did not answer within {}s", TIMEOUT.as_secs()));
        }
        let n = read.map_err(|e| e.to_string())?;
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
        crate::pluginhost::force_kill(self.child.id());
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
                // A broken pipe, EOF, or timeout kill means the process is done
                // for; drop it so the next call respawns.
                self.procs.remove(id);
                Err(e)
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/warmhost.rs"]
mod tests;
