//! Prompt: a terminal emulator. Tabs of split panes, one shell per pane.

mod about;
mod agenthooks;
mod agentpicker;
mod agentstate;
mod appid;
mod appkit;
mod boxdraw;
mod bridge;
mod catalog;
mod colors;
mod element;
mod envpath;
mod notes;
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
mod metrics;
mod mouse;
mod ospicker;
mod pluginhost;
mod pluginmanager;
mod pluginwebview;
mod pluginwindow;
mod badge;
mod clipboard;
mod pointer;
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
mod worktree;
mod settings;
mod suggest;
mod teambuilder;
mod update;
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
use gpui::{point, px, size, App, Bounds, TitlebarOptions, WindowBounds, WindowOptions};

const DEFAULT_COLS: usize = 80;
const DEFAULT_ROWS: usize = 24;

/// Parse `notify` argv into `(title, body)`. `--title`/`-t` sets the title
/// (default "Prompt"); the remaining words join into the body.
fn notify_args(args: &[String]) -> (String, String) {
    let mut title = "Prompt".to_string();
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

    // Dev-only drive surface: `prompt ipc <op> [json-args]` sends one op over
    // the single-instance socket and prints the JSON reply, for scripted UI
    // testing. Compiled out of release builds so it never widens the shipped
    // automation surface.
    #[cfg(debug_assertions)]
    if args.first().map(String::as_str) == Some("ipc") {
        std::process::exit(ipc::run_cli(&args[1..]));
    }

    // GUI launch (Finder/Dock) inherits a bare PATH; adopt the login shell's so
    // spawned tools (bun/node, agent CLIs, git/docker plugins) are found.
    envpath::fix();

    let (opts, diagnostics) = config::load();
    for d in &diagnostics {
        eprintln!("prompt: config line {}: {} ({})", d.line, d.message, d.key);
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
        eprintln!("prompt: config line {}: {} ({})", d.line, d.message, d.key);
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
    open_window(opts, colors, font, font_size, cell, pad, None, None, cx);
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
    cx: &mut App,
) {
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
    let handle = cx
        .open_window(
            options,
            move |window, cx| {
                cx.new(move |cx| {
                    root::WorkspaceView::new(
                        opts, colors, font, font_size, cell, pad, cols, rows, cwd, adopt, window, cx,
                    )
                })
            },
        )
        .expect("open window");
    // Bring the new window to the front (a torn-off tab must not open behind).
    handle
        .update(cx, |_view, window, _cx| window.activate_window())
        .ok();
}
