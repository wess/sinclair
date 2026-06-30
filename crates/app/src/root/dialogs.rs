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

    /// Open the "New OS Tab" dialog: an in-window modal listing OS images to
    /// run fresh. Bad config profiles are reported to stderr and skipped.
    pub(crate) fn open_os_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let available = self.container_engine().is_some();
        let (profiles, errors) = container::profiles(&self.opts.container);
        for error in &errors {
            eprintln!("prompt: container profile {error}");
        }
        let text = colors::hsla(self.colors.fg);
        let surface = colors::hsla(crate::tabbar::blend(self.colors.bg, self.colors.fg, 0.08));
        let workspace = cx.entity().downgrade();
        let view = cx.new(|cx| {
            crate::ospicker::OsPickerDialog::new(
                workspace, available, profiles, text, surface, window, cx,
            )
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
