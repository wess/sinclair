//! The side-column designer: which sections live in the left dock, which in the
//! right, and in what order.
//!
//! This page holds no state of its own. Every render recomposes both docks from
//! the settings file through `root::compose_docks` — the same function the
//! workspace uses — and every edit mutates that composition and writes it
//! straight back. Keeping one source of truth is what stops the designer and
//! the real sidebar from ever disagreeing; the cost is a config parse per edit,
//! which is what `reload_opts` already does for every toggle on every page.

use super::super::schema::Section;
use super::super::SettingsView;
use super::*;
use crate::root::dock::{self, Dock, Docks, SidebarPanel, SidebarSide};
use gpui::{div, px, AnyElement, Context, MouseButton, SharedString};

/// Width bounds offered by the steppers. The floor matches the renderer's own
/// `MIN_PANEL_W`, so the designer cannot ask for a column the dock will refuse
/// to draw.
const MIN_WIDTH: u32 = 180;
const MAX_WIDTH: u32 = 520;
const WIDTH_STEP: u32 = 20;

impl SettingsView {
    /// Both docks as the workspace would build them right now.
    fn docks(&self) -> Docks {
        crate::root::compose_docks(&self.opts, &self.plugins)
    }

    fn label_of(&self, panel: SidebarPanel) -> String {
        crate::root::label_of(&self.plugins, panel)
    }

    /// Persist a whole composition: both orders and the collapsed set.
    ///
    /// Written as three whole-list replacements rather than incremental edits
    /// because order *is* the data here — there is no meaningful "append one
    /// section" write when the position carries the meaning.
    fn write_docks(&mut self, docks: &Docks, cx: &mut Context<Self>) {
        let token = |p: SidebarPanel| crate::root::token_of(&self.plugins, p);
        crate::confwrite::set_list(
            "sidebar-left",
            &dock::tokens_of(&docks[SidebarSide::Left.index()], token),
        );
        crate::confwrite::set_list(
            "sidebar-right",
            &dock::tokens_of(&docks[SidebarSide::Right.index()], token),
        );
        crate::confwrite::set_list("sidebar-collapsed", &dock::collapsed_tokens(docks, token));
        self.reload_opts();
        cx.notify();
    }

    /// Apply `edit` to the current composition and save the result.
    fn edit_docks(&mut self, edit: impl FnOnce(&mut Docks), cx: &mut Context<Self>) {
        let mut docks = self.docks();
        edit(&mut docks);
        self.write_docks(&docks, cx);
    }

    fn set_width(&mut self, side: SidebarSide, width: u32, cx: &mut Context<Self>) {
        let key = match side {
            SidebarSide::Left => "sidebar-left-width",
            SidebarSide::Right => "sidebar-right-width",
        };
        crate::confwrite::upsert(key, &width.clamp(MIN_WIDTH, MAX_WIDTH).to_string());
        self.reload_opts();
        cx.notify();
    }

    fn width_of(&self, side: SidebarSide) -> u32 {
        match side {
            SidebarSide::Left => self.opts.sidebar_left_width,
            SidebarSide::Right => self.opts.sidebar_right_width,
        }
    }

    /// The whole page: a group per dock, then the unplaced pool.
    pub(crate) fn sidebar_content(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let docks = self.docks();
        let mut out: Vec<AnyElement> = Vec::new();
        out.push(
            div()
                .pt(px(6.0))
                .text_size(px(13.0))
                .text_color(hsla(MUTED))
                .child(SharedString::from(
                    "Each side is its own column. Sections stack top to bottom and \
                     collapse independently.",
                ))
                .into_any_element(),
        );
        for side in [SidebarSide::Left, SidebarSide::Right] {
            out.push(self.dock_group(side, &docks, cx).into_any_element());
        }
        out.push(self.available_group(&docks, cx).into_any_element());
        out
    }

    /// One dock: its sections in order, then its width stepper.
    fn dock_group(
        &self,
        side: SidebarSide,
        docks: &Docks,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let dock: &Dock = &docks[side.index()];
        let title = match side {
            SidebarSide::Left => "Left Column",
            SidebarSide::Right => "Right Column",
        };
        let mut rows: Vec<AnyElement> = Vec::new();
        if dock.sections.is_empty() {
            rows.push(
                self.row(
                    self.icon("\u{25a1}", px(18.0)),
                    "Empty",
                    div()
                        .text_size(px(12.5))
                        .text_color(hsla(MUTED))
                        .child(SharedString::from("Add a section from Available below")),
                )
                .into_any_element(),
            );
        }
        for (i, section) in dock.sections.iter().enumerate() {
            rows.push(self.section_row(side, i, section.panel, dock.sections.len(), cx));
        }
        rows.push(self.width_row(side, cx));

        div()
            .flex()
            .flex_col()
            .child(self.heading(title))
            .child(self.list(rows))
    }

    /// One placed section: move up/down, send across, remove.
    fn section_row(
        &self,
        side: SidebarSide,
        index: usize,
        panel: SidebarPanel,
        count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Disabled ends read as muted rather than vanishing, so the row's
        // control block keeps a stable width down the list.
        let up = self
            .step_button("\u{2191}", index > 0, cx, move |docks| {
                dock::reorder(docks, side, index, -1)
            });
        let down = self
            .step_button("\u{2193}", index + 1 < count, cx, move |docks| {
                dock::reorder(docks, side, index, 1)
            });
        let across_glyph = match side {
            SidebarSide::Left => "\u{2192}",
            SidebarSide::Right => "\u{2190}",
        };
        let across = self.step_button(across_glyph, true, cx, move |docks| {
            let to = side.other();
            let at = docks[to.index()].sections.len();
            dock::move_to(docks, side, index, to, at);
        });
        let remove = self.step_button("\u{2715}", true, cx, move |docks| {
            if let Some(section) = docks[side.index()].sections.get(index).copied() {
                dock::remove(docks, section.panel);
            }
        });

        let control = div()
            .flex()
            .items_center()
            .gap_2()
            .child(up)
            .child(down)
            .child(across)
            .child(remove);

        self.row(
            self.icon(panel.icon(), px(18.0)),
            &self.label_of(panel),
            control,
        )
        .into_any_element()
    }

    /// A small button that mutates the composition when enabled.
    fn step_button(
        &self,
        glyph: &str,
        enabled: bool,
        cx: &mut Context<Self>,
        edit: impl Fn(&mut Docks) + 'static,
    ) -> impl IntoElement {
        let button = button_box(glyph.to_string())
            .text_color(hsla(if enabled { TEXT } else { LINE }));
        if !enabled {
            return button;
        }
        button.on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _ev, _window, cx| {
                this.edit_docks(&edit, cx);
                cx.stop_propagation();
            }),
        )
    }

    /// The dock's width, with steppers. Written in whole steps rather than as a
    /// free field because a column width is a coarse preference, and a stepper
    /// cannot produce the 3px column a typo can.
    fn width_row(&self, side: SidebarSide, cx: &mut Context<Self>) -> AnyElement {
        let width = self.width_of(side);
        let narrower = width > MIN_WIDTH;
        let wider = width < MAX_WIDTH;

        let minus = {
            let b = button_box("\u{2212}").text_color(hsla(if narrower { TEXT } else { LINE }));
            if narrower {
                b.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _ev, _window, cx| {
                        let next = this.width_of(side).saturating_sub(WIDTH_STEP);
                        this.set_width(side, next, cx);
                        cx.stop_propagation();
                    }),
                )
            } else {
                b
            }
        };
        let plus = {
            let b = button_box("\u{002b}").text_color(hsla(if wider { TEXT } else { LINE }));
            if wider {
                b.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _ev, _window, cx| {
                        let next = this.width_of(side) + WIDTH_STEP;
                        this.set_width(side, next, cx);
                        cx.stop_propagation();
                    }),
                )
            } else {
                b
            }
        };

        let control = div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .min_w(px(70.0))
                    .flex()
                    .justify_end()
                    .text_size(px(13.0))
                    .text_color(hsla(TEXT))
                    .child(SharedString::from(format!("{width} px"))),
            )
            .child(minus)
            .child(plus);

        self.row(self.icon("\u{2194}", px(18.0)), "Width", control)
            .into_any_element()
    }

    /// Sections in neither dock, each with a button per side.
    ///
    /// Built-ins only. A plugin's section is placed automatically when the
    /// plugin loads (`dock::place_unplaced`), so it is never in this pool —
    /// removing one here would just see it reappear on the next launch.
    fn available_group(&self, docks: &Docks, cx: &mut Context<Self>) -> impl IntoElement {
        let pool = dock::available(docks);
        let mut rows: Vec<AnyElement> = Vec::new();
        if pool.is_empty() {
            rows.push(
                self.row(
                    self.icon("\u{2713}", px(18.0)),
                    "Everything is placed",
                    div()
                        .text_size(px(12.5))
                        .text_color(hsla(MUTED))
                        .child(SharedString::from("Remove a section to park it here")),
                )
                .into_any_element(),
            );
        }
        for panel in pool {
            let to_left = self.step_button("\u{2190} Left", true, cx, move |docks| {
                dock::add(docks, SidebarSide::Left, panel)
            });
            let to_right = self.step_button("Right \u{2192}", true, cx, move |docks| {
                dock::add(docks, SidebarSide::Right, panel)
            });
            rows.push(
                self.row(
                    self.icon(panel.icon(), px(18.0)),
                    &self.label_of(panel),
                    div().flex().items_center().gap_2().child(to_left).child(to_right),
                )
                .into_any_element(),
            );
        }
        div()
            .flex()
            .flex_col()
            .child(self.heading("Available"))
            .child(self.list(rows))
    }
}

/// Whether a search query should surface this page. It has no schema entries —
/// its state is three list keys, not a row per setting — so, like Macros, it
/// matches by name.
pub(crate) fn matches_search(section: Section, query: &str) -> bool {
    section == Section::Sidebar
        && super::rows::word_match(query, "sidebar side column dock panel section width")
}
