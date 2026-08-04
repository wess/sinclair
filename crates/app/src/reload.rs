//! Bridge the config-file watcher's background callback into an async
//! stream the gpui foreground can poll, so edits trigger a live reload.

use std::time::Duration;

use futures::StreamExt;

struct Installed(#[allow(dead_code)] config::WatchHandle);
impl gpui::Global for Installed {}

/// Quiet period used to coalesce an editor's multi-write save.
const INTERVAL: Duration = Duration::from_millis(250);

/// Start the process-wide config watcher and fan each change out to every
/// workspace window. No-op when already installed or no config path exists.
pub fn install(cx: &mut gpui::App) {
    if cx.try_global::<Installed>().is_some() {
        return;
    }
    let Some(path) = config::default_path() else {
        return;
    };
    let (tx, rx) = futures::channel::mpsc::unbounded();
    let handle = config::watch(path, INTERVAL, move || {
        let _ = tx.unbounded_send(());
    });
    cx.set_global(Installed(handle));

    let mut changes = rx;
    cx.spawn(async move |cx| {
        while changes.next().await.is_some() {
            cx.update(|cx| {
                for window in cx.windows() {
                    if let Some(workspace) = window.downcast::<crate::root::WorkspaceView>() {
                        workspace
                            .update(cx, |view, _window, cx| view.reload(cx))
                            .ok();
                    }
                }
            });
        }
    })
    .detach();
}
