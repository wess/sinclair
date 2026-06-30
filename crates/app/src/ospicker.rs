//! In-window "New OS Tab" dialog: a centered card over a dimming backdrop,
//! listing OS images to run fresh as a container-backed tab plus a field for an
//! arbitrary image.
//!
//! Unlike guise's `Modal`, this is rendered as a plain (non-`deferred`) overlay
//! — the host adds it as the last child of the window root, so it paints on top
//! without relying on the deferred draw pass. Clicking a row launches that OS;
//! typing an image and pressing Return runs it directly; Esc / ⌘W / backdrop
//! click cancels.

use gpui::prelude::*;
use gpui::{
    div, px, ClickEvent, Context, Entity, FontWeight, Hsla, KeyDownEvent, MouseButton,
    Subscription, WeakEntity, Window,
};

use guise::{TextInput, TextInputEvent};

use crate::root::WorkspaceView;

pub struct OsPickerDialog {
    workspace: WeakEntity<WorkspaceView>,
    available: bool,
    profiles: Vec<container::Profile>,
    input: Entity<TextInput>,
    /// Foreground text and elevated surface, resolved from the active theme by
    /// the host so the card matches the terminal colors.
    text: Hsla,
    surface: Hsla,
    _submit: Subscription,
}

impl OsPickerDialog {
    pub fn new(
        workspace: WeakEntity<WorkspaceView>,
        available: bool,
        profiles: Vec<container::Profile>,
        text: Hsla,
        surface: Hsla,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| {
            TextInput::new(cx).placeholder("or type an image, e.g. debian:bookworm")
        });
        window.focus(&input.read(cx).focus_handle(), cx);

        let submit = {
            let me = cx.entity().downgrade();
            window.subscribe(&input, cx, move |_input, event, window, app| {
                if let TextInputEvent::Submit(text) = event {
                    let text = text.clone();
                    me.update(app, |this, cx| this.submit(&text, window, cx)).ok();
                }
            })
        };

        Self {
            workspace,
            available,
            profiles,
            input,
            text,
            surface,
            _submit: submit,
        }
    }

    /// Launch `profile` in a new tab, then dismiss.
    fn launch(&self, profile: container::Profile, window: &mut Window, cx: &mut Context<Self>) {
        self.workspace
            .update(cx, |ws, cx| {
                ws.launch_container(&profile, window, cx);
                ws.close_modal(window, cx);
            })
            .ok();
    }

    /// Return in the field: run the typed image (or the matching profile), or
    /// the first profile when the field is empty.
    fn submit(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let text = text.trim();
        let profile = if text.is_empty() {
            self.profiles.first().cloned()
        } else {
            Some(
                self.profiles
                    .iter()
                    .find(|p| {
                        p.label.eq_ignore_ascii_case(text) || p.image.eq_ignore_ascii_case(text)
                    })
                    .cloned()
                    .unwrap_or_else(|| container::Profile {
                        label: text.to_string(),
                        image: text.to_string(),
                        command: "bash".to_string(),
                        persist: None,
                    }),
            )
        };
        match profile {
            Some(p) => self.launch(p, window, cx),
            None => self.cancel(window, cx),
        }
    }

    fn cancel(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.workspace
            .update(cx, |ws, cx| ws.close_modal(window, cx))
            .ok();
    }

    fn on_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let ks = &event.keystroke;
        if ks.key == "escape" || (ks.modifiers.platform && ks.key == "w") {
            self.cancel(window, cx);
            cx.stop_propagation();
        }
    }

    /// Text color at a given alpha, for borders/hover/dimming.
    fn faded(&self, alpha: f32) -> Hsla {
        let mut c = self.text;
        c.a = alpha;
        c
    }
}

impl Render for OsPickerDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let scrim = gpui::hsla(0.0, 0.0, 0.0, 0.55);
        let border = self.faded(0.12);
        let hover = self.faded(0.08);
        let dim = self.faded(0.6);

        let mut card = div()
            .w(px(420.0))
            .max_h(px(520.0))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .bg(self.surface)
            .text_color(self.text)
            .text_size(px(13.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(border)
            .p(px(18.0))
            .shadow_lg()
            // Clicks inside the card must not fall through to the backdrop.
            .on_mouse_down(MouseButton::Left, |_ev, _window, cx| cx.stop_propagation())
            .child(
                div()
                    .text_size(px(16.0))
                    .font_weight(FontWeight::BOLD)
                    .child("New OS Tab"),
            );

        if self.available {
            let mut list = div().flex().flex_col().gap(px(2.0)).child(
                div()
                    .text_size(px(11.0))
                    .text_color(dim)
                    .pb(px(2.0))
                    .child("RUN FRESH"),
            );
            for (i, p) in self.profiles.iter().enumerate() {
                let profile = p.clone();
                let me = cx.entity().downgrade();
                list = list.child(
                    div()
                        .id(("os-row", i))
                        .flex()
                        .items_center()
                        .px(px(10.0))
                        .py(px(7.0))
                        .rounded(px(6.0))
                        .hover(move |s| s.bg(hover))
                        .on_click(move |_ev: &ClickEvent, window, app| {
                            let profile = profile.clone();
                            me.update(app, |this, cx| this.launch(profile, window, cx)).ok();
                        })
                        .child(format!("{}  \u{00b7}  {}", p.label, p.image)),
                );
            }
            card = card
                .child(list)
                .child(self.input.clone())
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(dim)
                        .child("Click an image or press Return \u{2022} Esc to cancel"),
                );
        } else {
            card = card.child(
                div()
                    .text_size(px(13.0))
                    .child("No container engine found. Install Docker or Podman to launch OS tabs."),
            );
        }

        div().on_key_down(cx.listener(Self::on_key)).child(
            div()
                .id("os-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(scrim)
                .on_click(cx.listener(|this, _ev: &ClickEvent, window, cx| {
                    this.cancel(window, cx);
                }))
                .child(card),
        )
    }
}
