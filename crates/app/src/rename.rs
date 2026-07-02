//! In-window "rename" dialog: a guise `Modal` with a single text field. Enter
//! applies the new title back to the workspace, Escape (or clicking the
//! backdrop / close button) cancels. Hosted by `WorkspaceView` as an overlay
//! (see `root/render.rs`); opened via `WorkspaceView::open_rename`.

use gpui::prelude::*;
use gpui::{div, App, Context, Entity, KeyDownEvent, Subscription, WeakEntity, Window};
use guise::panegroup::ItemId;

use guise::{Modal, Size, Text, TextInput, TextInputEvent};

use crate::root::WorkspaceView;

/// What a rename targets: an item's title, naming a freshly recorded macro
/// (carrying its commands), or saving a layout.
#[derive(Clone)]
pub enum Target {
    Item(ItemId),
    Macro(Vec<String>),
    Layout(crate::tiles::Layout),
    /// Annotate the current line of an item with the entered note.
    Annotate(ItemId),
}

impl Target {
    fn title(&self) -> &'static str {
        match self {
            Target::Item(_) => "Change Terminal Title",
            Target::Macro(_) => "Name Macro",
            Target::Layout(_) => "Save Layout",
            Target::Annotate(_) => "Annotate Line",
        }
    }
}

/// Apply the entered `text` to `target`, then close the dialog. Runs with a
/// live `Window` so focus returns to the active pane.
fn commit(
    root: &WeakEntity<WorkspaceView>,
    target: &Target,
    text: &str,
    window: &mut Window,
    app: &mut App,
) {
    root.update(app, |ws, cx| {
        match target {
            Target::Item(id) => ws.rename_item(*id, text, cx),
            Target::Macro(commands) => ws.save_macro(text, commands.clone(), cx),
            Target::Layout(layout) => ws.save_layout(text, layout.clone(), cx),
            Target::Annotate(id) => ws.annotate_item(*id, text, cx),
        }
        ws.close_modal(window, cx);
    })
    .ok();
}

pub struct RenameDialog {
    root: WeakEntity<WorkspaceView>,
    title: &'static str,
    input: Entity<TextInput>,
    _submit: Subscription,
}

impl RenameDialog {
    pub fn new(
        root: WeakEntity<WorkspaceView>,
        target: Target,
        initial: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let title = target.title();
        let input = cx.new(|cx| TextInput::new(cx).value(initial).placeholder("Title"));
        window.focus(&input.read(cx).focus_handle(), cx);
        let submit = {
            let root = root.clone();
            window.subscribe(&input, cx, move |_input, event, window, app| {
                if let TextInputEvent::Submit(text) = event {
                    commit(&root, &target, text, window, app);
                }
            })
        };
        Self {
            root,
            title,
            input,
            _submit: submit,
        }
    }

    fn cancel(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.root.update(cx, |ws, cx| ws.close_modal(window, cx)).ok();
    }
}

impl Render for RenameDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let root = self.root.clone();
        div()
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                let ks = &ev.keystroke;
                if ks.key == "escape" || (ks.modifiers.platform && ks.key == "w") {
                    this.cancel(window, cx);
                    cx.stop_propagation();
                }
            }))
            .child(
                Modal::new()
                    .title(self.title)
                    .width(380.0)
                    .on_close(move |_ev, window, app| {
                        root.update(app, |ws, cx| ws.close_modal(window, cx)).ok();
                    })
                    .child(self.input.clone())
                    .child(
                        Text::new("Return to apply \u{2022} Esc to cancel")
                            .dimmed()
                            .size(Size::Sm),
                    ),
            )
    }
}
