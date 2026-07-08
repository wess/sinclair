use super::*;
use gpui::prelude::*;

impl Render for WorkspaceView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Sync the platform window's opacity to the setting. At opacity 1.0 the
        // window is marked opaque so the compositor ignores the framebuffer's
        // alpha entirely (a residual-alpha frame from a prior translucent state
        // would otherwise keep bleeding through); below 1.0 it's transparent so
        // the root fill's alpha shows the desktop. Flip only on change.
        let want_transparent = self.opts.background_opacity < 1.0;
        if want_transparent != self.bg_transparent {
            self.bg_transparent = want_transparent;
            window.set_background_appearance(if want_transparent {
                gpui::WindowBackgroundAppearance::Transparent
            } else {
                gpui::WindowBackgroundAppearance::Opaque
            });
        }
        // Root fill; its alpha is the window background opacity. Default-bg cells
        // aren't painted by the element, so they show this (and the desktop when
        // the window is transparent); colored cells stay opaque.
        let has_image = self.opts.background_image.is_some();
        let mut winbg = colors::hsla(self.colors.bg);
        winbg.a = self.opts.background_opacity.clamp(0.0, 1.0);
        // A background image needs the tint translucent to show through.
        if has_image {
            winbg.a = winbg.a.min(0.85);
        }
        let mut base = div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .key_context("Workspace")
            .on_action(cx.listener(Self::runbind))
            .on_action(cx.listener(Self::showdocs))
            .on_action(cx.listener(Self::showabout))
            .on_action(cx.listener(Self::menupick));
        // Background layers (painted first, behind the chrome): the image, then a
        // translucent tint. Without an image the tint is just the window fill.
        if let Some(path) = self.opts.background_image.clone() {
            base = base.child(
                gpui::img(std::path::PathBuf::from(path))
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .object_fit(gpui::ObjectFit::Cover),
            );
        }
        base = base.child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .bg(winbg),
        );

        // No separate titlebar: the pane group's top-row tab bar *is* the
        // titlebar (it reserves the traffic-light inset and drags the window).
        // macOS uses native traffic lights and Windows native caption controls;
        // only Linux (client-side decorations) overlays its own at the top-right.
        #[cfg(target_os = "linux")]
        {
            base = base.child(crate::titlebar::window_controls_overlay(&self.colors));
        }

        // The group renders the whole tree of tabbed splits (per-pane tab bars,
        // dividers, drag/drop) itself.
        let content: AnyElement = self.group.clone().into_any_element();
        // Content row: [left drawer?] [splits] [right drawer?]. Drawers are
        // fixed-width and hidden unless a panel is active on that side.
        let left = self
            .left_panel
            .map(|panel| self.drawer(SidebarSide::Left, panel, cx));
        let right = self
            .right_panel
            .map(|panel| self.drawer(SidebarSide::Right, panel, cx));
        base = base.child(
            div()
                .w_full()
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .flex_row()
                .children(left)
                .child(div().flex_1().min_w(px(0.0)).h_full().child(content))
                .children(right),
        );

        let recording = cx
            .try_global::<MacroRecorder>()
            .is_some_and(|rec| rec.0.is_active());
        let replaying = cx.try_global::<MacroReplays>().is_some_and(|r| r.0 > 0);
        if let Some(pill) = macro_pill(recording, replaying, &self.colors) {
            base = base.child(pill);
        }

        if cx.try_global::<Broadcast>().is_some_and(|b| b.0) {
            base = base.child(broadcast_pill(&self.colors));
        }

        if self
            .items
            .borrow()
            .values()
            .any(|it| it.content.is_recording(cx))
        {
            base = base.child(recording_pill(&self.colors));
        }

        // The cmd+P quick-open overlay (renders nothing while closed).
        if let Some(spot) = self.spotlight.as_ref() {
            base = base.child(spot.clone());
        }

        // The active in-window dialog (rename), if any.
        if let Some(modal) = self.modal.as_ref() {
            base = base.child(modal.clone());
        }

        #[cfg(target_os = "linux")]
        if matches!(window.window_decorations(), gpui::Decorations::Client { .. }) {
            base = base.child(crate::titlebar::resize_handles());
        }

        base
    }
}

/// A floating pill shown while a cast recording is capturing, stacked below
/// the macro/broadcast pills so the three never collide.
fn recording_pill(palette: &Colors) -> AnyElement {
    let accent = theme::Rgb::new(255, 69, 58);
    let mut bg = colors::hsla(palette.bg);
    bg.a = 0.9;
    let mut border = colors::hsla(accent);
    border.a = 0.5;
    div()
        .absolute()
        .top(px(56.0))
        .right(px(8.0))
        .flex()
        .items_center()
        .gap_1()
        .px_2()
        .py(px(2.0))
        .rounded(px(6.0))
        .bg(bg)
        .border_1()
        .border_color(border)
        .text_size(px(11.0))
        .text_color(colors::hsla(accent))
        .child(SharedString::from("\u{25cf}"))
        .child(SharedString::from("REC"))
        .into_any_element()
}

/// A floating pill warning that broadcast input is active, placed beside the
/// macro pill (one notch lower so they never collide).
fn broadcast_pill(palette: &Colors) -> AnyElement {
    let accent = theme::Rgb::new(255, 196, 0);
    let mut bg = colors::hsla(palette.bg);
    bg.a = 0.9;
    let mut border = colors::hsla(accent);
    border.a = 0.5;
    div()
        .absolute()
        .top(px(32.0))
        .right(px(8.0))
        .flex()
        .items_center()
        .gap_1()
        .px_2()
        .py(px(2.0))
        .rounded(px(6.0))
        .bg(bg)
        .border_1()
        .border_color(border)
        .text_size(px(11.0))
        .text_color(colors::hsla(accent))
        .child(SharedString::from("\u{1f4e1}"))
        .child(SharedString::from("BROADCAST"))
        .into_any_element()
}

fn macro_pill(recording: bool, replaying: bool, palette: &Colors) -> Option<AnyElement> {
    if !recording && !replaying {
        return None;
    }
    let (glyph, label, accent) = if recording {
        ("\u{25cf}", "REC", theme::Rgb::new(230, 80, 80))
    } else {
        ("\u{25b6}", "REPLAY", theme::Rgb::new(120, 190, 250))
    };
    let mut bg = colors::hsla(palette.bg);
    bg.a = 0.9;
    let mut border = colors::hsla(palette.fg);
    border.a = 0.18;
    Some(
        div()
            .absolute()
            .top(px(8.0))
            .right(px(8.0))
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .py(px(2.0))
            .rounded(px(6.0))
            .bg(bg)
            .border_1()
            .border_color(border)
            .text_size(px(11.0))
            .text_color(colors::hsla(accent))
            .child(SharedString::from(glyph))
            .child(SharedString::from(label))
            .into_any_element(),
    )
}
