use super::*;
use gpui::prelude::*;

impl Render for WorkspaceView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Note where the window is, for the session. Every variant carries the
        // *restore* bounds, so a maximized or full-screen window still records
        // the size it would return to — which is what we want to reopen at.
        self.last_bounds = Some(match window.window_bounds() {
            gpui::WindowBounds::Windowed(b)
            | gpui::WindowBounds::Maximized(b)
            | gpui::WindowBounds::Fullscreen(b) => b,
        });
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
        // The order the tab switcher walks. Recorded here because every path
        // that changes the active item ends in a repaint, and a repaint is the
        // only point they all pass through — hooking the activation paths
        // one by one is how a switcher ends up quietly missing one of them.
        // `touch_recent` ignores this while the switcher is open, so cycling
        // cannot reorder the list it is walking.
        let active = self.group.read(cx).active_item();
        if self.recent.first() != Some(&active) {
            self.touch_recent(active);
        }

        let switching = self.switcher.is_some();
        // Only a pinned peek takes keys: one opened by hovering the tab bar
        // must leave the shell below exactly as reachable as it was.
        let peeking = self.peek.as_ref().is_some_and(|p| p.pinned);
        let mut base = div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .key_context("Workspace")
            .on_action(cx.listener(Self::runbind))
            .on_action(cx.listener(Self::showdocs))
            .on_action(cx.listener(Self::showabout))
            .on_action(cx.listener(Self::menupick))
            // Only while the switcher is up: this fires on every modifier press
            // and release in the window, and there is nothing else to spend
            // that on.
            .when(switching, |d| {
                d.on_modifiers_changed(cx.listener(
                    |this, ev: &gpui::ModifiersChangedEvent, window, cx| {
                        this.switcher_modifiers(ev.modifiers, window, cx);
                    },
                ))
                // Captured, because the focused terminal would otherwise eat
                // the escape and send it to the shell.
                .capture_key_down(cx.listener(
                    |this, ev: &gpui::KeyDownEvent, _w, cx| {
                        if ev.keystroke.key == "escape" {
                            this.cancel_switcher(cx);
                            cx.stop_propagation();
                        }
                    },
                ))
            })
            // Same reason, for the keys that drive an open peek. Everything
            // else still reaches the terminal, so a peek left open does not
            // put the window in a mode.
            .when(peeking, |d| {
                d.capture_key_down(cx.listener(|this, ev: &gpui::KeyDownEvent, window, cx| {
                    match ev.keystroke.key.as_str() {
                        "escape" => this.close_peek(cx),
                        "left" => this.step_peek(-1, cx),
                        "right" => this.step_peek(1, cx),
                        "enter" => this.commit_peek(window, cx),
                        _ => return,
                    }
                    cx.stop_propagation();
                }))
            });
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
        base = base.child(div().absolute().top_0().left_0().size_full().bg(winbg));

        // No separate titlebar: the pane group's top-row tab bar *is* the
        // titlebar (it reserves the traffic-light inset and drags the window).
        // macOS uses native traffic lights and Windows native caption controls;
        // only Linux (client-side decorations) overlays its own at the top-right.
        #[cfg(target_os = "linux")]
        {
            base = base.child(crate::titlebar::window_controls_overlay(&self.colors));
        }

        // The group renders the whole tree of tabbed splits (per-pane tab bars,
        // dividers, drag/drop) itself. A window with no live items at all (the
        // startup shell failed to spawn, e.g. a bad `shell =`) shows the error
        // instead — exiting here would kill every window on NewWindow.
        let content: AnyElement = if self.items.borrow().is_empty() {
            match self.spawn_error.as_deref() {
                Some(error) => spawn_error_view(error, &self.colors),
                None => self.group.clone().into_any_element(),
            }
        } else {
            self.group.clone().into_any_element()
        };
        // Content row: [left dock?] [splits] [right dock?]. Each dock is a
        // stack of collapsible sections at its own configured width, and is
        // hidden entirely while closed.
        let left = self.docks[SidebarSide::Left.index()]
            .open
            .then(|| self.dock_column(SidebarSide::Left, cx));
        let right = self.docks[SidebarSide::Right.index()]
            .open
            .then(|| self.dock_column(SidebarSide::Right, cx));
        // The peek hangs inside the splits column rather than the window, so
        // its strip spans the tabs it previews and stops at a dock's edge.
        let strip = self.peek_strip(window, cx);
        let band = self.peek_hover_band(cx);
        base = base.child(
            div()
                .w_full()
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .flex_row()
                .children(left)
                .child(
                    div()
                        .relative()
                        .flex_1()
                        .min_w(px(0.0))
                        .h_full()
                        .child(content)
                        .children(strip)
                        .children(band),
                )
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

        if let Some(overlay) = self.switcher_overlay(cx) {
            base = base.child(overlay);
        }

        // The active in-window dialog (rename), if any.
        // The context menu renders nothing while closed, so it can stay in the
        // tree until the next open replaces it.
        if let Some(menu) = self.tab_menu.as_ref() {
            base = base.child(menu.clone());
        }
        if let Some(modal) = self.modal.as_ref() {
            base = base.child(modal.clone());
        }

        #[cfg(target_os = "linux")]
        if matches!(
            window.window_decorations(),
            gpui::Decorations::Client { .. }
        ) {
            base = base.child(crate::titlebar::resize_handles());
        }

        base
    }
}

/// Full-window notice shown when the startup shell could not be spawned:
/// the error itself plus a hint at the config key that usually causes it.
fn spawn_error_view(error: &str, palette: &Colors) -> AnyElement {
    let fg = colors::hsla(palette.fg);
    let mut dim = fg;
    dim.a = 0.65;
    let accent = colors::hsla(theme::Rgb::new(230, 80, 80));
    div()
        .flex_1()
        .min_h(px(0.0))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_2()
        .px_8()
        .text_color(fg)
        .child(
            div()
                .text_size(px(15.0))
                .text_color(accent)
                .child(SharedString::from("The shell could not be started")),
        )
        .child(div().text_size(px(13.0)).child(SharedString::from(error.to_string())))
        .child(
            div()
                .text_size(px(12.0))
                .text_color(dim)
                .child(SharedString::from(
                    "Check the `shell =` line in ~/.config/sinclair/config, then open a new tab or window.",
                )),
        )
        .into_any_element()
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
