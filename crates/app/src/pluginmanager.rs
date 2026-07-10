//! The Plugin Manager: a standalone window (like JetBrains' plugin dialog) to
//! search installed plugins, read their descriptions, install from the catalog,
//! and uninstall. Self-contained — it loads plugins and the catalog itself, and
//! tells the main workspace to reload after any change so menus/panels update.

use gpui::prelude::*;
use gpui::{
    bounds, div, point, px, size, App, Context, Entity, FocusHandle, FontWeight, KeyDownEvent,
    MouseButton, Subscription, TitlebarOptions, Window, WindowBounds, WindowControlArea,
    WindowOptions,
};

use guise::{Button, Size, TextInput, TextInputEvent, Variant};

use crate::root::WorkspaceView;

const WIDTH: f32 = 720.0;
const HEIGHT: f32 = 560.0;

/// Open the Plugin Manager window, centered over `parent`.
pub fn open(parent: &Window, cx: &mut App) {
    let center = parent.bounds().center();
    let where_ = bounds(
        center - point(px(WIDTH / 2.0), px(HEIGHT / 2.0)),
        size(px(WIDTH), px(HEIGHT)),
    );
    let handle = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(where_)),
            is_resizable: true,
            titlebar: Some(TitlebarOptions {
                title: Some("Plugins".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(12.0), px(12.0))),
            }),
            ..Default::default()
        },
        |window, cx| {
            window.set_window_title("Plugins");
            cx.new(|cx| PluginManager::new(window, cx))
        },
    );
    if let Ok(handle) = handle {
        handle
            .update(cx, |view, window, cx| {
                window.activate_window();
                window.focus(&view.search.read(cx).focus_handle(), cx);
            })
            .ok();
    }
}

pub struct PluginManager {
    opts: config::Options,
    installed: Vec<plugin::Plugin>,
    /// Plugins present on disk but disabled in `installed.toml` — they don't
    /// load, so they come from a separate managed-dir scan.
    disabled: Vec<plugin::Plugin>,
    catalog: Option<Vec<String>>,
    status: Option<String>,
    loading: bool,
    /// Name currently being installed/removed, for a per-row busy state.
    busy: Option<String>,
    query: String,
    search: Entity<TextInput>,
    focus: FocusHandle,
    _subs: Vec<Subscription>,
}

impl PluginManager {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (opts, _) = config::load();
        let installed = plugin::load(&opts.plugin).0;
        let search = cx.new(|cx| TextInput::new(cx).placeholder("Search plugins\u{2026}"));
        let focus = cx.focus_handle();

        let search_focus = search.read(cx).focus_handle();
        window.on_next_frame(move |window, cx| window.focus(&search_focus, cx));

        let mut subs = Vec::new();
        subs.push(cx.subscribe(&search, |this, _src, event: &TextInputEvent, cx| {
            if let TextInputEvent::Change(text) = event {
                this.query = text.clone();
                cx.notify();
            }
        }));

        let mut this = Self {
            opts,
            installed,
            disabled: load_disabled(),
            catalog: None,
            status: None,
            loading: false,
            busy: None,
            query: String::new(),
            search,
            focus,
            _subs: subs,
        };
        this.fetch_catalog(cx);
        this
    }

    /// Reload the installed set from disk (after an install/uninstall/toggle).
    fn refresh_installed(&mut self) {
        self.installed = plugin::load(&self.opts.plugin).0;
        self.disabled = load_disabled();
    }

    /// Tell every workspace window to reload plugins so their menus, sidebars,
    /// and keybinds reflect the change — not just an arbitrary first window.
    fn reload_workspaces(&self, cx: &mut Context<Self>) {
        for handle in cx.windows() {
            if let Some(ws) = handle.downcast::<WorkspaceView>() {
                let _ = ws.update(cx, |ws, _window, cx| ws.reload_plugins(cx));
            }
        }
    }

    /// Enable or disable a plugin by id: persist the record, then reload the
    /// plugin set everywhere. A disabled plugin stays installed but inert.
    fn set_enabled(&mut self, id: String, enabled: bool, cx: &mut Context<Self>) {
        let mut installed = plugin::Installed::load();
        installed.set_enabled(&id, enabled);
        if let Err(e) = installed.save() {
            self.status = Some(format!("Could not save installed.toml: {e}"));
            cx.notify();
            return;
        }
        self.status = Some(format!(
            "{} {id}",
            if enabled { "Enabled" } else { "Disabled" }
        ));
        self.refresh_installed();
        self.reload_workspaces(cx);
        cx.notify();
    }

    /// Fetch the installable catalog off-thread (the GitHub call would block).
    fn fetch_catalog(&mut self, cx: &mut Context<Self>) {
        if self.loading {
            return;
        }
        self.loading = true;
        self.status = None;
        cx.notify();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor.spawn(async { crate::catalog::list() }).await;
            let _ = this.update(cx, |this, cx| {
                this.loading = false;
                match result {
                    Ok(names) => this.catalog = Some(names),
                    Err(e) => {
                        this.catalog = Some(Vec::new());
                        this.status = Some(format!("Catalog unavailable: {e}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn install(&mut self, name: String, cx: &mut Context<Self>) {
        self.busy = Some(name.clone());
        self.status = Some(format!("Installing {name}\u{2026}"));
        cx.notify();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let target = name.clone();
            let result = executor
                .spawn(async move { crate::catalog::install(&target) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = None;
                match result {
                    Ok(_) => {
                        this.status = Some(format!("Installed {name}"));
                        this.refresh_installed();
                        this.reload_workspaces(cx);
                    }
                    Err(e) => this.status = Some(format!("Install {name} failed: {e}")),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn uninstall(&mut self, name: String, cx: &mut Context<Self>) {
        self.busy = Some(name.clone());
        self.status = Some(format!("Removing {name}\u{2026}"));
        cx.notify();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let target = name.clone();
            let result = executor
                .spawn(async move { crate::catalog::uninstall(&target) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = None;
                match result {
                    Ok(()) => {
                        this.status = Some(format!("Removed {name}"));
                        this.refresh_installed();
                        this.reload_workspaces(cx);
                    }
                    Err(e) => this.status = Some(format!("Remove {name} failed: {e}")),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The folder name of a plugin *if* it lives in the managed plugin dir (so
    /// it can be uninstalled); `None` for plugins referenced from config.
    fn managed_name(&self, plugin: &plugin::Plugin) -> Option<String> {
        let dir = plugin::defaultdir()?;
        if plugin.path.parent() != Some(dir.as_path()) {
            return None;
        }
        Some(plugin.path.file_name()?.to_string_lossy().into_owned())
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, _cx: &mut Context<Self>) {
        if event.keystroke.key == "escape" {
            window.remove_window();
        }
    }
}

impl Render for PluginManager {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = guise::theme(cx);
        let bg = t.body().hsla();
        let surface = t.surface().hsla();
        let border = t.border().hsla();
        let text = t.text().hsla();
        let dim = t.dimmed().hsla();

        let q = self.query.trim().to_lowercase();
        let matches = |hay: &str| q.is_empty() || hay.to_lowercase().contains(&q);

        // Installed rows.
        let installed: Vec<gpui::AnyElement> = self
            .installed
            .iter()
            .filter(|p| {
                matches(&p.name)
                    || matches(&p.id)
                    || p.description.as_deref().is_some_and(matches)
            })
            .enumerate()
            .map(|(i, p)| {
                let uninstall = self.managed_name(p);
                let busy = uninstall.as_ref().is_some_and(|n| self.busy.as_deref() == Some(n));
                self.plugin_row(
                    i,
                    &p.name,
                    Some(&p.version),
                    p.description.as_deref(),
                    &p.capabilities,
                    surface,
                    border,
                    text,
                    dim,
                    Some((p.id.clone(), true)),
                    match uninstall {
                        Some(name) => RowAction::Uninstall { name, busy },
                        None => RowAction::External,
                    },
                    cx,
                )
            })
            .collect();

        // Disabled rows: on disk but inert; offer Enable (and Uninstall).
        let disabled: Vec<gpui::AnyElement> = self
            .disabled
            .iter()
            .filter(|p| {
                matches(&p.name)
                    || matches(&p.id)
                    || p.description.as_deref().is_some_and(matches)
            })
            .enumerate()
            .map(|(i, p)| {
                let uninstall = self.managed_name(p);
                let busy = uninstall.as_ref().is_some_and(|n| self.busy.as_deref() == Some(n));
                self.plugin_row(
                    20_000 + i,
                    &p.name,
                    Some(&p.version),
                    p.description.as_deref(),
                    &p.capabilities,
                    surface,
                    border,
                    dim,
                    dim,
                    Some((p.id.clone(), false)),
                    match uninstall {
                        Some(name) => RowAction::Uninstall { name, busy },
                        None => RowAction::External,
                    },
                    cx,
                )
            })
            .collect();

        // Available (catalog) rows: names not already installed (or disabled).
        let installed_ids: std::collections::HashSet<&str> = self
            .installed
            .iter()
            .chain(self.disabled.iter())
            .map(|p| p.id.as_str())
            .collect();
        let available: Vec<gpui::AnyElement> = self
            .catalog
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter(|n| !installed_ids.contains(n.as_str()) && matches(n))
            .enumerate()
            .map(|(i, name)| {
                let busy = self.busy.as_deref() == Some(name.as_str());
                self.plugin_row(
                    10_000 + i,
                    name,
                    None,
                    None,
                    &[],
                    surface,
                    border,
                    text,
                    dim,
                    None,
                    RowAction::Install {
                        name: name.clone(),
                        busy,
                    },
                    cx,
                )
            })
            .collect();

        let section = |label: &str, count: usize| {
            div()
                .flex()
                .items_center()
                .gap_2()
                .pt_2()
                .child(
                    div()
                        .text_size(px(11.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(dim)
                        .child(format!("{} ({count})", label.to_uppercase())),
                )
        };

        let mut list = div()
            .id("pm-list")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1()
            .child(section("Installed", installed.len()));
        if installed.is_empty() {
            list = list.child(note("No installed plugins match.", dim));
        }
        for row in installed {
            list = list.child(row);
        }
        if !disabled.is_empty() {
            list = list.child(section("Disabled", disabled.len()));
            for row in disabled {
                list = list.child(row);
            }
        }
        list = list.child(section("Marketplace", available.len()));
        if self.loading && self.catalog.is_none() {
            list = list.child(note("Loading catalog\u{2026}", dim));
        } else if available.is_empty() {
            list = list.child(note("Nothing to install.", dim));
        }
        for row in available {
            list = list.child(row);
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .bg(bg)
            .text_color(text)
            .pt(px(34.0)) // clear the transparent titlebar
            .px(px(16.0))
            .pb(px(12.0))
            .gap(px(10.0))
            .child(drag_strip())
            .child(
                div()
                    .text_size(px(15.0))
                    .font_weight(FontWeight::BOLD)
                    .child("Plugins"),
            )
            .child(self.search.clone())
            .child(list)
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(dim)
                    .child(self.status.clone().unwrap_or_else(|| {
                        "Install from the catalog, or uninstall a managed plugin.".to_string()
                    })),
            )
    }
}

/// What the trailing button on a plugin row does.
enum RowAction {
    Install { name: String, busy: bool },
    Uninstall { name: String, busy: bool },
    /// A plugin referenced from config (absolute path) — can't be uninstalled.
    External,
}

impl PluginManager {
    #[allow(clippy::too_many_arguments)]
    fn plugin_row(
        &self,
        idx: usize,
        name: &str,
        version: Option<&str>,
        description: Option<&str>,
        capabilities: &[String],
        surface: gpui::Hsla,
        border: gpui::Hsla,
        text: gpui::Hsla,
        dim: gpui::Hsla,
        // `(plugin id, currently enabled)` renders an Enable/Disable toggle.
        toggle: Option<(String, bool)>,
        action: RowAction,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let title = match version {
            Some(v) => format!("{name}  ·  {v}"),
            None => name.to_string(),
        };
        let toggle = toggle.map(|(id, enabled)| {
            let label = if enabled { "Disable" } else { "Enable" };
            Button::new(("pm-toggle", idx), label)
                .size(Size::Sm)
                .variant(Variant::Subtle)
                .on_click(cx.listener(move |this, _ev, _w, cx| {
                    this.set_enabled(id.clone(), !enabled, cx);
                }))
                .into_any_element()
        });
        let button = match action {
            RowAction::Install { name, busy } => {
                let label = if busy { "Installing…" } else { "Install" };
                Button::new(("pm-install", idx), label)
                    .size(Size::Sm)
                    .variant(Variant::Filled)
                    .on_click(cx.listener(move |this, _ev, _w, cx| {
                        this.install(name.clone(), cx);
                    }))
                    .into_any_element()
            }
            RowAction::Uninstall { name, busy } => {
                let label = if busy { "Removing…" } else { "Uninstall" };
                Button::new(("pm-remove", idx), label)
                    .size(Size::Sm)
                    .variant(Variant::Outline)
                    .on_click(cx.listener(move |this, _ev, _w, cx| {
                        this.uninstall(name.clone(), cx);
                    }))
                    .into_any_element()
            }
            RowAction::External => div()
                .text_size(px(11.0))
                .text_color(dim)
                .child("external")
                .into_any_element(),
        };

        let mut info = div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .flex_1()
            .min_w(px(0.0))
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::MEDIUM)
                    .child(title),
            );
        if let Some(desc) = description.filter(|d| !d.trim().is_empty()) {
            info = info.child(
                div()
                    .text_size(px(12.0))
                    .text_color(dim)
                    .child(desc.to_string()),
            );
        }
        if !capabilities.is_empty() {
            info = info.child(
                div()
                    .text_size(px(11.0))
                    .text_color(dim)
                    .child(format!("accesses: {}", capabilities.join(", "))),
            );
        }

        div()
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .rounded(px(7.0))
            .bg(surface)
            .border_1()
            .border_color(border)
            .text_color(text)
            .child(info)
            .children(toggle)
            .child(button)
            .into_any_element()
    }
}

/// Manifest-parse the managed plugin dir for plugins disabled in
/// `installed.toml` — they never come back from `plugin::load` (disabled means
/// not loaded), but the manager must still list them with an Enable button.
fn load_disabled() -> Vec<plugin::Plugin> {
    let installed = plugin::Installed::load();
    let Some(dir) = plugin::defaultdir() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let manifest = entry.path().join(plugin::MANIFEST);
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        if let Some(p) = plugin::parse(manifest, &text).0 {
            if !installed.is_enabled(&p.id) {
                out.push(p);
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn note(msg: &str, dim: gpui::Hsla) -> gpui::AnyElement {
    div()
        .px_3()
        .py_2()
        .text_size(px(12.0))
        .text_color(dim)
        .child(msg.to_string())
        .into_any_element()
}

/// Drag handle across the transparent titlebar so the window can be moved.
fn drag_strip() -> impl IntoElement {
    let lead = if cfg!(target_os = "macos") { 70.0 } else { 0.0 };
    div()
        .absolute()
        .top_0()
        .left(px(lead))
        .w(px(WIDTH - lead))
        .h(px(28.0))
        .window_control_area(WindowControlArea::Drag)
        .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
}
