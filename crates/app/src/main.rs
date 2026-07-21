//! Sinclair: a terminal emulator. Tabs of split panes, one shell per pane.

mod about;
mod agenthooks;
mod agentpicker;
mod agentstate;
mod appid;
mod appkit;
mod catalog;
mod colors;
mod confwrite;
mod envpath;
mod notes;
mod notify;
mod exportcmd;
#[cfg(target_os = "macos")]
mod fidelity;
mod font;
mod guisetheme;
mod guiwasm;
mod help;
mod ipc;
mod keys;
#[cfg(target_os = "linux")]
mod linux;
mod mcpbridge;
mod ospicker;
mod paths;
mod pluginhost;
mod pluginmanager;
mod pluginwebview;
mod pluginwindow;
mod badge;
mod clipboard;
mod quick;
mod redact;
mod trigger;
mod relay;
mod relaywatch;
mod reload;
mod rename;
mod resume;
mod root;
mod session;
mod sessionstate;
mod sidecar;
mod worktree;
mod settings;
mod suggest;
mod teambuilder;
mod updateui;
mod shellinteg;
mod tabbar;
mod tiles;
mod titlebar;
mod view;
mod warmhost;
mod wasmhost;

use std::rc::Rc;

use gpui::AppContext as _;
use libsinclair::metrics;
use gpui::{point, px, size, App, Bounds, Pixels, TitlebarOptions, WindowBounds, WindowOptions};

const DEFAULT_COLS: usize = 80;
const DEFAULT_ROWS: usize = 24;

/// How far a torn-off window is nudged from the window it came from.
const CASCADE_STEP: f32 = 36.0;

/// Parse `notify` argv into `(title, body)`. `--title`/`-t` sets the title
/// (default "Sinclair"); the remaining words join into the body.
fn notify_args(args: &[String]) -> (String, String) {
    let mut title = "Sinclair".to_string();
    let mut body: Vec<&str> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--title" | "-t" => {
                if let Some(t) = it.next() {
                    title = t.clone();
                }
            }
            other => body.push(other),
        }
    }
    (title, body.join(" "))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "--toggle-quick") {
        ipc::send_toggle();
        return;
    }

    if args.first().map(String::as_str) == Some("mcp") {
        mcpbridge::run_stdio();
        return;
    }

    if args.first().map(String::as_str) == Some("notify") {
        let (title, body) = notify_args(&args[1..]);
        view::notify_command(&title, &body);
        return;
    }

    // An agent (or its lifecycle hook) reporting its semantic state back to the
    // running GUI over the single-instance socket. Best-effort; never blocks the
    // GUI path.
    if args.first().map(String::as_str) == Some("agent-status") {
        std::process::exit(agenthooks::report(&args[1..]));
    }

    // Install / remove the Claude Code lifecycle hooks that drive the status dots.
    if args.first().map(String::as_str) == Some("agent-hooks") {
        std::process::exit(agenthooks::hooks(&args[1..]));
    }

    if args.first().map(String::as_str) == Some("export") {
        std::process::exit(exportcmd::run(&args[1..]));
    }

    // Dev-only drive surface: `sinclair ipc <op> [json-args]` sends one op over
    // the single-instance socket and prints the JSON reply, for scripted UI
    // testing. Compiled out of release builds so it never widens the shipped
    // automation surface.
    #[cfg(debug_assertions)]
    if args.first().map(String::as_str) == Some("ipc") {
        std::process::exit(ipc::run_cli(&args[1..]));
    }

    // A pre-rename `Prompt.app` whose contents already self-updated to Sinclair
    // renames itself once and relaunches at the new path; this process then
    // exits, before it can bind the single-instance socket. Only the GUI launch
    // migrates — a subcommand above must never move the bundle out from under a
    // running instance.
    #[cfg(target_os = "macos")]
    if updater::migrate_bundle() {
        return;
    }

    // GUI launch (Finder/Dock) inherits a bare PATH; adopt the login shell's so
    // spawned tools (bun/node, agent CLIs, git/docker plugins) are found.
    envpath::fix();

    // One-time upgrade of a pre-JSON `key = value` config into settings.json,
    // before the first load and before the watcher points at the new file.
    confwrite::migrate();

    let (opts, diagnostics) = config::load();
    for d in &diagnostics {
        eprintln!("sinclair: settings line {}: {} ({})", d.line, d.message, d.key);
    }

    let app = gpui_platform::application();
    app.on_reopen(|cx| {
        if cx.windows().is_empty() {
            spawn_window(cx);
        }
    });
    app.run(move |cx: &mut App| {
        let auto_update = opts.auto_update;
        open_default_window(opts, cx);
        cx.activate(true);
        quick::install_global_hotkey(cx);
        ipc::listen(cx);
        // Plugin sidecars are host-owned children; never leave one behind.
        cx.on_app_quit(|_cx| async { sidecar::shutdown() }).detach();
        if auto_update {
            updateui::start_from_config(cx);
        }
    });
}

/// Load config fresh and open one default window. Used for dock-reopen, where
/// there is no surviving window to copy appearance from.
fn spawn_window(cx: &mut App) {
    let (opts, diagnostics) = config::load();
    for d in &diagnostics {
        eprintln!("sinclair: settings line {}: {} ({})", d.line, d.message, d.key);
    }
    open_default_window(opts, cx);
}

/// Derive appearance from `opts` and open one default-sized window.
fn open_default_window(opts: config::Options, cx: &mut App) {
    redact::install(&opts.redact, cx);
    badge::install(&opts.badge, cx);
    view::install_timestamps(opts.timestamps, cx);
    trigger::install(&opts.trigger, cx);
    let colors = Rc::new(colors::from_config(&opts, root::is_dark(cx.window_appearance())));
    guisetheme::install(&colors, cx);
    let font = font::build(&opts);
    let font_size = px(opts.font_size.max(1.0));
    let cell = metrics::measure(cx.text_system(), &font, font_size);
    let pad = metrics::Padding {
        x: opts.window_padding_x as f32,
        y: opts.window_padding_y as f32,
    };
    open_window(opts, colors, font, font_size, cell, pad, None, None, None, cx);
}

/// Where a torn-off window goes: the source window's size, stepped down and to
/// the right so it reads as a separate window instead of hiding the one it was
/// dragged out of. Steps back to the display's top-left once the nudge would
/// push the window past the bottom or right edge.
pub(crate) fn cascade(source: Bounds<Pixels>, display: Bounds<Pixels>) -> Bounds<Pixels> {
    let step = px(CASCADE_STEP);
    let limit = point(
        display.origin.x + (display.size.width - source.size.width).max(px(0.0)),
        display.origin.y + (display.size.height - source.size.height).max(px(0.0)),
    );
    let stepped = point(source.origin.x + step, source.origin.y + step);
    let origin = if stepped.x > limit.x || stepped.y > limit.y {
        point(display.origin.x + step, display.origin.y + step)
    } else {
        stepped
    };
    Bounds::new(origin, source.size)
}

/// Open a fresh top-level window hosting its own `WorkspaceView`. Shared by
/// startup and the `new_window` action so both produce identically sized,
/// identically styled windows from the same appearance values.
#[allow(clippy::too_many_arguments)]
pub(crate) fn open_window(
    opts: config::Options,
    colors: Rc<colors::Colors>,
    font: gpui::Font,
    font_size: gpui::Pixels,
    cell: metrics::CellSize,
    pad: metrics::Padding,
    cwd: Option<std::path::PathBuf>,
    // A torn-off tab re-homed here as the first item (else spawn a fresh shell).
    adopt: Option<root::PaneContent>,
    // Exact screen bounds to open at (a torn-off window cascades off its
    // source); `None` centers a default-sized window on the active display.
    place: Option<Bounds<Pixels>>,
    cx: &mut App,
) {
    let (bounds, cols, rows) = match place {
        Some(bounds) => {
            let (cols, rows) =
                metrics::grid_size(bounds.size.width.into(), bounds.size.height.into(), pad, cell);
            (bounds, cols, rows)
        }
        None => {
            let cols = if opts.window_width > 0 {
                opts.window_width as usize
            } else {
                DEFAULT_COLS
            };
            let rows = if opts.window_height > 0 {
                opts.window_height as usize
            } else {
                DEFAULT_ROWS
            };
            let (width, height) = metrics::pixel_size(cols, rows, pad, cell);
            let bounds = Bounds::centered(None, size(px(width), px(height)), cx);
            (bounds, cols, rows)
        }
    };
    #[cfg_attr(not(target_os = "linux"), allow(unused_mut))]
    let mut options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(size(px(200.0), px(200.0))),
        titlebar: Some(TitlebarOptions {
            title: Some(appid::id().into()),
            appears_transparent: true,
            traffic_light_position: Some(point(px(9.0), px(9.0))),
        }),
        app_id: Some(appid::id().into()),
        ..Default::default()
    };
    // Always open transparent-capable so `background-opacity` can be lowered
    // live: a macOS window created opaque can't be flipped back to transparent
    // at runtime. At opacity 1.0 the root fill is fully opaque, so it still
    // looks solid — the window backing just permits alpha when it's lowered.
    options.window_background = gpui::WindowBackgroundAppearance::Transparent;
    #[cfg(target_os = "linux")]
    {
        options.window_decorations = Some(gpui::WindowDecorations::Client);
    }
    // Windows draws its own caption and frame (native decorations); our tab bar
    // sits in the client area below it.
    #[cfg(target_os = "windows")]
    {
        options.window_decorations = Some(gpui::WindowDecorations::Server);
    }
    let handle = match cx.open_window(options, move |window, cx| {
        cx.new(move |cx| {
            root::WorkspaceView::new(
                opts, colors, font, font_size, cell, pad, cols, rows, cwd, adopt, window, cx,
            )
        })
    }) {
        Ok(handle) => handle,
        Err(error) => {
            eprintln!("sinclair: could not open a window: {error}");
            return;
        }
    };
    // Bring the new window to the front. A tear-off opens from inside the
    // source window's mouse-up, and the key-window change AppKit makes while
    // that event is still on the stack loses to the source window's own
    // ordering — so raise the app and re-key the new window once the current
    // effect cycle has drained, when nothing is left to override it.
    cx.activate(true);
    cx.defer(move |cx| {
        handle
            .update(cx, |view, window, cx| {
                window.activate_window();
                view.focusactive(window, cx);
            })
            .ok();
    });
}

#[cfg(test)]
#[path = "../tests/main.rs"]
mod tests;
