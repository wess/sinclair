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
    /// Scan output lines newly completed since the last wakeup against the
    /// configured regex triggers, firing a desktop notification per match.
    /// `trigger_hwm == usize::MAX` marks the first scan, which only records the
    /// high-water mark so pre-existing scrollback doesn't fire.
    pub(crate) fn scan_triggers(&mut self, cx: &mut Context<Self>) {
        let Some(triggers) = crate::trigger::current(cx) else {
            self.trigger_hwm = usize::MAX;
            return;
        };
        let start = self.trigger_hwm;
        let (fires, total) = self.session.with_term(|t| {
            let lines = t.text_lines();
            let total = lines.len();
            // Skip the final line: it may still be mid-write.
            let end = total.saturating_sub(1);
            let mut fires = Vec::new();
            if start != usize::MAX {
                for (idx, text, _) in &lines {
                    if *idx >= start && *idx < end {
                        if let Some(hit) = triggers.check(text) {
                            fires.push(hit);
                        }
                    }
                }
            }
            (fires, total)
        });
        self.trigger_hwm = total.saturating_sub(1);
        for (title, body) in fires {
            post_os_notification(&title, &body);
        }
    }

    /// A faint watermark badge in the pane corner, when configured.
    pub(crate) fn badge_overlay(&self, cx: &gpui::App) -> Option<gpui::AnyElement> {
        let template = crate::badge::template(cx)?;
        let text = crate::badge::render(&template, self.cwd().as_deref(), &crate::badge::hostname());
        if text.trim().is_empty() {
            return None;
        }
        let mut color = crate::colors::hsla(self.colors.fg);
        color.a = 0.14;
        Some(
            gpui::div()
                .absolute()
                .top(gpui::px(self.pad.y + 4.0))
                .right(gpui::px(self.pad.x + 8.0))
                .text_color(color)
                .text_size(gpui::px(11.0))
                .child(gpui::SharedString::from(text))
                .into_any_element(),
        )
    }

    /// The most recent non-blank output lines (newest first), for global search.
    pub(crate) fn recent_lines(&self, max: usize) -> Vec<String> {
        self.session.with_term(|t| {
            t.text_lines()
                .into_iter()
                .rev()
                .filter(|(_, s, _)| !s.trim().is_empty())
                .take(max)
                .map(|(_, s, _)| s.trim_end().to_string())
                .collect()
        })
    }

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

    /// Export the most recent `.cast` recording to `format` (a file extension
    /// like `gif` or `mp4`), off the UI thread.
    ///
    /// Spawns `sinclair export --fidelity <cast> <cast>.<format>` as a background
    /// subprocess (so a long render never blocks the UI) and posts a desktop
    /// notification when it finishes. GIF needs no external tools; the video
    /// formats need ffmpeg.
    pub fn export_recording(&mut self, format: &str, cx: &mut Context<Self>) {
        let Some(cast) = latest_recording() else {
            self.assist = Some(Assist::Message {
                title: "No recording to export".to_string(),
                body: "Record a session first, then export it.".to_string(),
            });
            cx.notify();
            return;
        };
        let out = cast.with_extension(format);
        let exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("prompt"));
        let out_display = out.display().to_string();
        let name = cast
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let label = format.to_ascii_uppercase();
        std::thread::spawn(move || {
            let status = std::process::Command::new(exe)
                .arg("export")
                .arg("--fidelity")
                .arg(&cast)
                .arg(&out)
                .status();
            match status {
                Ok(s) if s.success() => post_os_notification("Recording exported", &out_display),
                _ => post_os_notification("Recording export failed", &out_display),
            }
        });
        self.assist = Some(Assist::Message {
            title: "Exporting recording".to_string(),
            body: format!("{name} \u{2192} {label}; you'll be notified when it's ready."),
        });
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
/// display notification`; Linux uses `notify-send`. Used by `sinclair notify`,
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

/// Path for a new recording under `~/.config/sinclair/recordings/`, plus the
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

/// The newest `.cast` under the recordings directory, if any.
fn latest_recording() -> Option<std::path::PathBuf> {
    let dir = config::default_path()?.parent()?.join("recordings");
    let mut newest: Option<(std::time::SystemTime, std::path::PathBuf)> = None;
    for entry in std::fs::read_dir(&dir).ok()?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("cast") {
            continue;
        }
        let Ok(modified) = entry.metadata().and_then(|m| m.modified()) else {
            continue;
        };
        if newest.as_ref().is_none_or(|(t, _)| modified > *t) {
            newest = Some((modified, path));
        }
    }
    newest.map(|(_, path)| path)
}
