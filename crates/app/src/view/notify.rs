use super::*;

/// Macro replay: how often the prompt-mark count is polled while waiting for
/// a replayed command to finish.
const REPLAY_POLL: Duration = Duration::from_millis(40);
/// Macro replay: give up waiting for a command's next prompt after this and
/// send the following command anyway.
const REPLAY_TIMEOUT: Duration = Duration::from_secs(20);
/// Macro replay gap used when the shell emits no OSC 133 prompt marks, so
/// pacing cannot key off a fresh prompt.
const REPLAY_FALLBACK_GAP: Duration = Duration::from_millis(150);

impl TerminalView {
    /// Whether this pane is recording an asciinema cast.
    pub fn is_recording(&self) -> bool {
        self.session.is_recording()
    }

    /// Start or stop recording this pane to an asciinema `.cast` file. On stop,
    /// surfaces the saved path in a dismissable message.
    pub fn toggle_recording(&mut self, cx: &mut Context<Self>) {
        if self.session.is_recording() {
            let body = match self.session.stop_recording() {
                Some(path) => path.display().to_string(),
                None => "(no file written)".to_string(),
            };
            self.assist = Some(Assist::Message {
                title: "Recording saved".to_string(),
                body,
            });
        } else if let Some((path, ts)) = recording_target() {
            let title = self.title().to_string();
            if self.session.start_recording(path, Some(&title), Some(ts)).is_err() {
                self.assist = Some(Assist::Message {
                    title: "Recording failed".to_string(),
                    body: "could not create the cast file".to_string(),
                });
            }
        }
        cx.notify();
    }

    /// Write raw bytes to the pty, snapping the view to the live bottom.
    /// Backs the macOS readline navigation keybinds (`text:`/`esc:`).
    pub fn send_text(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        if self.read_only || bytes.is_empty() {
            return;
        }
        self.scroll_to_bottom(cx);
        let _ = self.session.write(bytes);
    }

    /// Run a trusted plugin command in the focused shell.
    pub fn run_command(&mut self, command: &str, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        let command = command.trim();
        if command.is_empty() {
            return;
        }
        self.scroll_to_bottom(cx);
        let mut input = command.as_bytes().to_vec();
        input.push(b'\n');
        let _ = self.session.write(&input);
    }

    /// Replay a macro's commands into this pane, one submitted line each.
    ///
    /// Pacing keys off OSC 133 prompt marks: after sending a command we wait
    /// for the prompt-mark count to grow (the shell printed a fresh prompt,
    /// i.e. the command finished) before sending the next, bounded by
    /// [`REPLAY_TIMEOUT`]. Shells without shell integration emit no marks, so
    /// we fall back to a fixed gap between commands.
    pub fn run_macro(&self, commands: Vec<String>, cx: &mut Context<Self>) {
        if self.read_only || commands.is_empty() {
            return;
        }
        let session = self.session.clone();
        let executor = cx.background_executor().clone();
        crate::root::replays_changed(cx, 1);
        cx.spawn(async move |view, cx| {
            let paced = session.with_term(|t| !t.prompt_lines().is_empty());
            for command in commands {
                let before = session.with_term(|t| {
                    t.set_display_offset(0);
                    t.prompt_lines().len()
                });
                let mut bytes = command.into_bytes();
                bytes.push(b'\n');
                if session.write(&bytes).is_err() {
                    break;
                }
                if !paced {
                    executor.timer(REPLAY_FALLBACK_GAP).await;
                    continue;
                }
                let start = Instant::now();
                loop {
                    executor.timer(REPLAY_POLL).await;
                    if session.with_term(|t| t.prompt_lines().len()) > before {
                        break;
                    }
                    if start.elapsed() >= REPLAY_TIMEOUT {
                        break;
                    }
                }
            }
            let _ = view.update(cx, |view, cx| view.scroll_to_bottom(cx));
            cx.update(|cx| crate::root::replays_changed(cx, -1));
        })
        .detach();
    }

    /// Up to `lines` of the most recent screen text (scrollback + live grid),
    /// defaulting to the visible row count. Backs the MCP `read_screen` tool.
    pub fn screen_text(&self, lines: Option<usize>) -> String {
        self.session.with_term(|term| {
            let all = term.text_lines();
            let want = lines.unwrap_or_else(|| term.rows());
            let start = all.len().saturating_sub(want);
            let text = all[start..]
                .iter()
                .map(|(_, line, _)| line.trim_end())
                .collect::<Vec<_>>()
                .join("\n");
            text.trim_end().to_string()
        })
    }
}

/// Post a native desktop notification without blocking the UI (spawns a thread
/// that runs [`notify_command`]). Best-effort: missing tools and errors are
/// ignored; the in-app indicator is the reliable signal.
pub fn post_os_notification(title: &str, body: &str) {
    let (title, body) = (title.to_string(), body.to_string());
    std::thread::spawn(move || notify_command(&title, &body));
}

/// Post a native desktop notification synchronously. macOS uses `osascript
/// display notification`; Linux uses `notify-send`. Used by `prompt notify`,
/// which must wait for the helper before the process exits.
pub fn notify_command(title: &str, body: &str) {
    #[cfg(target_os = "macos")]
    {
        let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
        let script = format!(
            "display notification \"{}\" with title \"{}\"",
            esc(body),
            esc(title)
        );
        let _ = std::process::Command::new("osascript")
            .args(["-e", &script])
            .output();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("notify-send")
            .args([title, body])
            .output();
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let _ = (title, body);
}

/// Path for a new recording under `~/.config/prompt/recordings/`, plus the
/// unix timestamp for its header. `None` if the directory can't be made.
fn recording_target() -> Option<(std::path::PathBuf, u64)> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    let dir = config::default_path()?.parent()?.join("recordings");
    std::fs::create_dir_all(&dir).ok()?;
    Some((dir.join(format!("prompt-{ts}.cast")), ts))
}
