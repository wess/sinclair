use super::*;
use gpui::prelude::*;

impl Render for WorkspaceView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tree = self.tabs.active().tree.clone();
        let focused = self.tabs.focused();
        let multi = tree.panes().len() > 1;
        let children: Vec<(PaneId, AnyElement)> = tree
            .panes()
            .into_iter()
            .filter_map(|id| {
                self.panes
                    .get(&id)
                    .map(|pane| (id, pane.view.clone().into_any_element()))
            })
            .collect();
        let mut dividercolor = colors::hsla(self.colors.fg);
        dividercolor.a = 0.2;
        let mut dimcolor = colors::hsla(self.colors.bg);
        dimcolor.a = (1.0 - self.opts.unfocused_split_opacity).clamp(0.0, 1.0);
        let root: WeakEntity<Self> = cx.weak_entity();
        let splitselement = SplitsElement::new(
            tree,
            focused,
            children,
            dividercolor,
            dimcolor,
            self.drag.clone(),
            root,
        );

        let mut base = div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(colors::rgba(self.colors.bg))
            .key_context("Workspace")
            .on_action(cx.listener(Self::runbind))
            .on_action(cx.listener(Self::showdocs))
            .on_action(cx.listener(Self::showabout))
            .on_action(cx.listener(Self::menupick));

        let tab_infos = self.tab_infos(cx);
        let max_visible = self.tab_max_visible(window);
        let (_, overflow) =
            crate::tabbar::visible_split(tab_infos.len(), self.tabs.active_index(), max_visible);
        base = base.child(crate::titlebar::bar(
            &tab_infos,
            self.tabs.active_index(),
            max_visible,
            self.trailing_menu,
            &self.colors,
            &self.font,
            self.font_size,
            window,
            cx,
        ));
        if self.tab_overflow && !overflow.is_empty() {
            base = base.child(self.tab_overflow_menu(&tab_infos, &overflow, window, cx));
        } else if self.tab_overflow {
            self.tab_overflow = false;
        }
        if let Some(which) = self.trailing_menu {
            base = base.child(self.trailing_menu(which, window, cx));
        }

        let content: AnyElement = if self.zoomed && multi {
            match self.panes.get(&focused) {
                Some(pane) => pane.view.clone().into_any_element(),
                None => splitselement.into_any_element(),
            }
        } else {
            splitselement.into_any_element()
        };
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
            .panes
            .values()
            .any(|p| p.view.read(cx).is_recording())
        {
            base = base.child(recording_pill(&self.colors));
        }

        // The cmd+P quick-open overlay (renders nothing while closed).
        if let Some(spot) = self.spotlight.as_ref() {
            base = base.child(spot.clone());
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
