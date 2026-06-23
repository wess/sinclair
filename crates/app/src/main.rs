//! Prompt: a terminal emulator. Tabs of split panes, one shell per pane.

mod about;
mod appkit;
mod boxdraw;
mod bridge;
mod colors;
mod element;
mod font;
mod help;
mod ipc;
mod keys;
#[cfg(target_os = "linux")]
mod linux;
mod mcpbridge;
mod metrics;
mod mouse;
mod pointer;
mod quick;
mod reload;
mod rename;
mod root;
mod session;
mod settings;
mod splits;
mod tabbar;
mod textedit;
mod titlebar;
mod view;

use std::rc::Rc;

use gpui::AppContext as _;
use gpui::{point, px, size, App, Bounds, TitlebarOptions, WindowBounds, WindowOptions};

const DEFAULT_COLS: usize = 80;
const DEFAULT_ROWS: usize = 24;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // `prompt --toggle-quick` just signals the running instance (used by a
    // Wayland compositor keybind to summon the quick terminal) and exits.
    if args.iter().any(|a| a == "--toggle-quick") {
        ipc::send_toggle();
        return;
    }

    // `prompt mcp` runs a Model Context Protocol server on stdio, bridging
    // tool calls into the running instance over the single-instance socket.
    if args.first().map(String::as_str) == Some("mcp") {
        mcpbridge::run_stdio();
        return;
    }

    let (opts, diagnostics) = config::load();
    for d in &diagnostics {
        eprintln!("prompt: config line {}: {} ({})", d.line, d.message, d.key);
    }

    gpui_platform::application().run(move |cx: &mut App| {
        let colors = Rc::new(colors::from_config(&opts));
        let font = font::build(&opts);
        let font_size = px(opts.font_size.max(1.0));
        let cell = metrics::measure(cx.text_system(), &font, font_size);
        let pad = metrics::Padding {
            x: opts.window_padding_x as f32,
            y: opts.window_padding_y as f32,
        };

        // Keybindings come from config (defaults + user overrides) and are
        // bound by the workspace view, which owns the resolved table.
        // Startup has no pane to inherit from; defaults to home.
        open_window(opts, colors, font, font_size, cell, pad, None, cx);
        cx.activate(true);
        // Two summon paths for the quick terminal: an in-process global
        // hotkey (macOS/X11) and a socket the compositor can poke (Wayland).
        quick::install_global_hotkey(cx);
        ipc::listen(cx);
    });
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
    // Transparent native title bar so our own `titlebar` strip is the whole
    // chrome (tabs folded in). The macOS traffic lights stay, repositioned;
    // Linux gets client-side decorations so we can draw controls + resize.
    // `mut` is used only on Linux (client-side decorations) below.
    #[cfg_attr(not(target_os = "linux"), allow(unused_mut))]
    let mut options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("prompt".into()),
            appears_transparent: true,
            traffic_light_position: Some(point(px(9.0), px(9.0))),
        }),
        // Matches the `.desktop` StartupWMClass so Linux associates the window
        // with our icon in the taskbar/dock.
        app_id: Some("prompt".into()),
        ..Default::default()
    };
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
