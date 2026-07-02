//! In-window dialog overlays (rename): opening them, and the shared
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

    /// Open the annotate dialog for the focused item.
    pub(crate) fn open_annotate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.group.read(cx).active_item();
        if self.focused_terminal(cx).is_some() {
            self.open_rename(crate::rename::Target::Annotate(id), String::new(), window, cx);
        }
    }

    /// Attach `note` to the current line of item `id`.
    pub(crate) fn annotate_item(&mut self, id: ItemId, note: &str, cx: &mut Context<Self>) {
        let view = self
            .items
            .borrow()
            .get(&id)
            .and_then(|it| it.content.as_terminal().cloned());
        if let Some(v) = view {
            let note = note.to_string();
            v.update(cx, |view, cx| view.annotate(note, cx));
        }
    }

    /// Dismiss the active in-window dialog and refocus the active pane.
    pub(crate) fn close_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal.take().is_some() {
            self.focusactive(window, cx);
            cx.notify();
        }
    }
}
