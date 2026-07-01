//! File > Save Buffer: write the focused terminal's whole buffer (scrollback +
//! screen) to a text file chosen through the native save panel. A native panel
//! (rather than an in-app dialog) matches the OS "pick a file" flow and gets
//! folder navigation for free.

use std::path::PathBuf;

use super::*;

impl WorkspaceView {
    /// Save the focused terminal's buffer to a text file. Prompts for the path
    /// with the native save dialog, then writes off the UI thread.
    pub(crate) fn save_buffer(&mut self, cx: &mut Context<Self>) {
        let (text, dir, suggested) = {
            let Some(view) = self
                .panes
                .get(&self.tabs.focused())
                .and_then(|p| p.content.as_terminal())
            else {
                return;
            };
            let view = view.read(cx);
            let dir = view
                .cwd_path()
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_else(|| PathBuf::from("."));
            (view.buffer_text(), dir, suggested_name(view.title()))
        };
        if text.is_empty() {
            crate::view::post_os_notification("Nothing to save", "The terminal buffer is empty.");
            return;
        }
        let rx = cx.prompt_for_new_path(&dir, Some(&suggested));
        cx.spawn(async move |_this, _cx| {
            if let Ok(Ok(Some(path))) = rx.await {
                std::thread::spawn(move || match std::fs::write(&path, text) {
                    Ok(()) => {
                        crate::view::post_os_notification("Buffer saved", &path.display().to_string())
                    }
                    Err(e) => crate::view::post_os_notification("Save failed", &e.to_string()),
                });
            }
        })
        .detach();
    }
}

/// Turn a terminal title into a safe `.txt` filename, defaulting to `buffer`.
fn suggested_name(title: &str) -> String {
    let mut base: String = title.split_whitespace().collect::<Vec<_>>().join("-");
    base.retain(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.');
    let base = base.trim_matches(|c| c == '-' || c == '.');
    if base.is_empty() {
        "buffer.txt".into()
    } else {
        format!("{base}.txt")
    }
}
