use super::*;
use gpui::prelude::*;

impl WorkspaceView {
    /// Reset every divider in the group to an even split.
    pub(crate) fn equalizesplits(&mut self, cx: &mut Context<Self>) {
        self.group.update(cx, |g, cx| g.equalize(cx));
    }

    /// Nudge the divider adjacent to the focused pane in a direction.
    pub(crate) fn resizesplit(&mut self, dir: ResizeDir, cx: &mut Context<Self>) {
        let dir = match dir {
            ResizeDir::Left => Direction::Left,
            ResizeDir::Right => Direction::Right,
            ResizeDir::Up => Direction::Up,
            ResizeDir::Down => Direction::Down,
        };
        self.group.update(cx, |g, cx| g.resize_focused(dir, RESIZE_STEP, cx));
    }

    /// Resize the window back to the configured default cell grid.
    pub(crate) fn returntodefaultsize(&self, window: &mut Window) {
        let cols = if self.opts.window_width > 0 {
            self.opts.window_width as usize
        } else {
            SPAWN_COLS
        };
        let rows = if self.opts.window_height > 0 {
            self.opts.window_height as usize
        } else {
            SPAWN_ROWS
        };
        let (width, height) = crate::metrics::pixel_size(cols, rows, self.pad, self.cell);
        window.resize(size(px(width), px(height)));
    }

    /// Persist the focused item's current cell grid as the default size.
    pub(crate) fn useasdefault(&mut self, cx: &mut Context<Self>) {
        let Some((cols, rows)) = self.focused_terminal(cx).map(|v| v.read(cx).grid_size()) else {
            return;
        };
        write_config("window-width", &cols.to_string());
        write_config("window-height", &rows.to_string());
    }

    /// Split the focused pane. `first` places the new pane before the
    /// existing one (left/up) instead of after it (right/down).
    pub(crate) fn split(
        &mut self,
        axis: SplitAxis,
        first: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(item) = self.spawn_default(window, cx) else {
            return;
        };
        let pane = self.group.read(cx).focused_pane();
        self.group.update(cx, |g, cx| {
            g.split(pane, axis, first, item, cx);
        });
        self.focusactive(window, cx);
        cx.notify();
    }

    pub(crate) fn focusdir(&mut self, direction: Direction, window: &mut Window, cx: &mut Context<Self>) {
        self.group.update(cx, |g, cx| g.focus_direction(direction, cx));
        self.focusactive(window, cx);
        cx.notify();
    }

    /// Cycle focus to the previous/next pane in the group's layout order.
    pub(crate) fn cyclesplit(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let (panes, focused) = {
            let g = self.group.read(cx);
            (g.tree().panes(), g.focused_pane())
        };
        if panes.len() < 2 {
            return;
        }
        let i = panes.iter().position(|p| *p == focused).unwrap_or(0);
        let next = if forward {
            panes[(i + 1) % panes.len()]
        } else {
            panes[(i + panes.len() - 1) % panes.len()]
        };
        // Activate the target pane through its *current* active tab, so cycling
        // focus never resets a multi-tab pane back to its first item.
        let item = self
            .group
            .read(cx)
            .panes_with_items()
            .into_iter()
            .find(|(p, _, _)| *p == next)
            .map(|(_, _, active)| active);
        if let Some(item) = item {
            self.group.update(cx, |g, cx| g.activate(next, item, cx));
            self.focusactive(window, cx);
            cx.notify();
        }
    }
}
