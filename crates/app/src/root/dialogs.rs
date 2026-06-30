//! In-window dialog overlays (rename, new agent): opening them, and the shared
//! `close_modal` that dismisses whichever is active and restores focus to the
//! active pane.

use super::*;

impl WorkspaceView {
    /// Open the rename dialog over `target`, seeded with `initial`.
    pub(crate) fn open_rename(
        &mut self,
        target: crate::rename::Target,
        initial: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let root = cx.entity().downgrade();
        let view =
            cx.new(|cx| crate::rename::RenameDialog::new(root, target, &initial, window, cx));
        self.modal = Some(view.into());
        cx.notify();
    }

    /// Open the "New Agent" dialog with the given providers and role presets.
    pub(crate) fn open_new_agent(
        &mut self,
        providers: Vec<String>,
        roles: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let workspace = cx.entity().downgrade();
        let opts = self.opts.clone();
        let view = cx.new(|cx| {
            crate::newagent::NewAgentDialog::new(workspace, opts, providers, roles, window, cx)
        });
        self.modal = Some(view.into());
        cx.notify();
    }

    /// Dismiss the active in-window dialog and refocus the active pane.
    pub(crate) fn close_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal.take().is_some() {
            self.focusactive(window, cx);
            cx.notify();
        }
    }
}
