use super::*;
use gpui::prelude::*;
use gpui::Focusable as _;

impl WorkspaceView {
    /// Set a divider's ratio in the active tab (divider drag).
    pub fn setratio(&mut self, split: SplitId, ratio: f32, cx: &mut Context<Self>) {
        if self.tabs.active_mut().tree.set_ratio(split, ratio) {
            cx.notify();
        }
    }

    /// Reset every divider in the active tab to an even split.
    pub(crate) fn equalizesplits(&mut self, cx: &mut Context<Self>) {
        let dividers = self.tabs.active().tree.list_dividers();
        if dividers.is_empty() {
            return;
        }
        let tree = &mut self.tabs.active_mut().tree;
        for (split, _) in dividers {
            tree.set_ratio(split, 0.5);
        }
        cx.notify();
    }

    /// Nudge the divider adjacent to the focused pane in a direction.
    pub(crate) fn resizesplit(&mut self, dir: ResizeDir, cx: &mut Context<Self>) {
        let (axis, delta) = match dir {
            ResizeDir::Left => (Axis::Horizontal, -RESIZE_STEP),
            ResizeDir::Right => (Axis::Horizontal, RESIZE_STEP),
            ResizeDir::Up => (Axis::Vertical, -RESIZE_STEP),
            ResizeDir::Down => (Axis::Vertical, RESIZE_STEP),
        };
        let focused = self.tabs.focused();
        let tree = &mut self.tabs.active_mut().tree;
        let Some(split) = tree.nearest_split(focused, axis) else {
            return;
        };
        if let Some(current) = tree.ratio(split) {
            tree.set_ratio(split, current + delta);
            cx.notify();
        }
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

    /// Persist the focused pane's current cell grid as the default size.
    pub(crate) fn useasdefault(&mut self, cx: &mut Context<Self>) {
        let Some((cols, rows)) = self
            .panes
            .get(&self.tabs.focused())
            .map(|p| p.view.read(cx).grid_size())
        else {
            return;
        };
        write_config("window-width", &cols.to_string());
        write_config("window-height", &rows.to_string());
    }

    /// Split the focused pane. `first` places the new pane before the
    /// existing one (left/up) instead of after it (right/down).
    pub(crate) fn split(&mut self, axis: Axis, first: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.zoomed = false;
        let target = self.tabs.focused();
        let Some(id) = self.spawnpane(window, cx) else {
            return;
        };
        if self
            .tabs
            .active_mut()
            .tree
            .split(target, axis, id, first)
            .is_none()
        {
            self.panes.remove(&id);
            return;
        }
        self.tabs.focus(id);
        self.focusactive(window, cx);
        cx.notify();
    }

    pub(crate) fn focusdir(&mut self, direction: Direction, window: &mut Window, cx: &mut Context<Self>) {
        let viewport = window.viewport_size();
        let rect = Rect::new(
            0.0,
            0.0,
            f32::from(viewport.width).max(1.0),
            f32::from(viewport.height).max(1.0),
        );
        let layout = workspace::compute_layout(&self.tabs.active().tree, rect, splits::DIVIDER);
        if let Some(next) = workspace::neighbor(&layout, self.tabs.focused(), direction) {
            self.focuspane(next, window, cx);
        }
    }

    /// Move window focus to the active tab's focused pane and retitle.
    pub(crate) fn focusactive(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(pane) = self.panes.get(&self.tabs.focused()) {
            window.focus(&pane.view.focus_handle(cx), cx);
        }
        self.settitle(window, cx);
    }

    pub(crate) fn settitle(&self, window: &mut Window, cx: &App) {
        let title = self
            .panes
            .get(&self.tabs.focused())
            .map(|pane| pane.view.read(cx).title().to_string())
            .unwrap_or_else(|| "prompt".to_string());
        window.set_window_title(&title);
    }

    /// Cycle focus to the previous/next pane in the active tab's layout.
    pub(crate) fn cyclesplit(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let focused = self.tabs.focused();
        let tree = &self.tabs.active().tree;
        let next = if forward {
            workspace::next(tree, focused)
        } else {
            workspace::prev(tree, focused)
        };
        if let Some(next) = next {
            self.focuspane(next, window, cx);
        }
    }
}
