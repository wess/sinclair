//! Prompt: a terminal emulator. Tabs of split panes, one shell per pane.

mod about;
mod agentpicker;
mod appkit;
mod boxdraw;
mod bridge;
mod catalog;
mod colors;
mod element;
mod exportcmd;
#[cfg(target_os = "macos")]
mod fidelity;
mod font;
mod guisetheme;
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
mod pointer;
mod quick;
mod redact;
mod trigger;
mod relay;
mod relaywatch;
mod reload;
mod rename;
mod root;
mod session;
mod sessionstate;
mod settings;
mod shellinteg;
mod splits;
mod tabbar;
mod tiles;
mod titlebar;
mod view;

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

    if args.first().map(String::as_str) == Some("export") {
        std::process::exit(exportcmd::run(&args[1..]));
    }

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
        open_default_window(opts, cx);
        cx.activate(true);
        quick::install_global_hotkey(cx);
        ipc::listen(cx);
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
    open_window(opts, colors, font, font_size, cell, pad, None, cx);
}

/// Open a fresh top-level window hosting its own `WorkspaceView`. Shared by
/// startup and the `new_window` action so both produce identically sized,
/// identically styled windows from the same appearance values.
pub fn open_window(
    opts: config::Options,
    colors: Rc<colors::Colors>,
    font: gpui::Font,
    font_size: gpui::Pixels,
    cell: metrics::CellSize,
    pad: metrics::Padding,
    cwd: Option<std::path::PathBuf>,
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
            title: Some("prompt".into()),
            appears_transparent: true,
            traffic_light_position: Some(point(px(9.0), px(9.0))),
        }),
        app_id: Some("prompt".into()),
        ..Default::default()
    };
    if opts.background_opacity < 1.0 {
        options.window_background = gpui::WindowBackgroundAppearance::Transparent;
    }
    #[cfg(target_os = "linux")]
    {
        options.window_decorations = Some(gpui::WindowDecorations::Client);
    }
    cx.open_window(
        options,
        move |window, cx| {
            cx.new(move |cx| {
                root::WorkspaceView::new(
                    opts, colors, font, font_size, cell, pad, cols, rows, cwd, window, cx,
                )
            })
        },
    )
    .expect("open window");
}
