//! The `sinclair export` process mode: render a `.cast` recording to a GIF or
//! video file. Output format is chosen by the destination extension.

use std::path::PathBuf;

use export::{export, Options};

const USAGE: &str = "\
usage: sinclair export <input.cast> <output.(gif|mp4|mov|webm)> [options]

Render a recorded terminal session to a shareable file. GIF needs no external
tools; mp4/mov/webm are encoded with ffmpeg (must be on PATH).

options:
  --fps <n>          frames per second (default 30)
  --speed <x>        playback speed multiplier (default 1.0)
  --idle-cap <s>     collapse idle gaps longer than s seconds (default 2)
  --no-idle-cap      keep original timing, do not collapse idle gaps
  --tail <s>         hold the final frame for s seconds (default 1)
  --font-px <n>      font/cell pixel size (default 16)
  --cols <n>         override recorded column count
  --rows <n>         override recorded row count
  --theme <name>     built-in color scheme (default: the default dark scheme)
  --fidelity         render with the app's gpui text system (ligatures, exact
                     fonts); uses the configured font. macOS only.
  --scale <n>        device pixel scale for --fidelity (default 2)
";

/// Run the export command with the args following `export`. Returns a process
/// exit code.
pub fn run(args: &[String]) -> i32 {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{USAGE}");
        return 0;
    }

    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut opts = Options::default();
    let mut fidelity = false;
    let mut scale = 2.0f32;

    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = || it.next().cloned();
        match arg.as_str() {
            "--fidelity" => fidelity = true,
            "--scale" => match value().and_then(|v| v.parse().ok()) {
                Some(s) => scale = s,
                None => return fail("--scale needs a number"),
            },
            "--fps" => match value().and_then(|v| v.parse().ok()) {
                Some(n) => opts.fps = n,
                None => return fail("--fps needs a number"),
            },
            "--speed" => match value().and_then(|v| v.parse().ok()) {
                Some(x) => opts.speed = x,
                None => return fail("--speed needs a number"),
            },
            "--idle-cap" => match value().and_then(|v| v.parse().ok()) {
                Some(s) => opts.idle_cap = Some(s),
                None => return fail("--idle-cap needs a number"),
            },
            "--no-idle-cap" => opts.idle_cap = None,
            "--tail" => match value().and_then(|v| v.parse().ok()) {
                Some(s) => opts.tail = s,
                None => return fail("--tail needs a number"),
            },
            "--font-px" => match value().and_then(|v| v.parse().ok()) {
                Some(n) => opts.font_px = n,
                None => return fail("--font-px needs a number"),
            },
            "--cols" => match value().and_then(|v| v.parse().ok()) {
                Some(n) => opts.cols = Some(n),
                None => return fail("--cols needs a number"),
            },
            "--rows" => match value().and_then(|v| v.parse().ok()) {
                Some(n) => opts.rows = Some(n),
                None => return fail("--rows needs a number"),
            },
            "--theme" => match value() {
                Some(name) => opts.theme = Some(name),
                None => return fail("--theme needs a name"),
            },
            other if other.starts_with('-') => {
                return fail(&format!("unknown option: {other}"));
            }
            other if input.is_none() => input = Some(PathBuf::from(other)),
            other if output.is_none() => output = Some(PathBuf::from(other)),
            other => return fail(&format!("unexpected argument: {other}")),
        }
    }

    let (input, output) = match (input, output) {
        (Some(i), Some(o)) => (i, o),
        _ => {
            eprint!("{USAGE}");
            return 2;
        }
    };

    let result = if fidelity {
        render_fidelity(&input, &output, &opts, scale)
    } else {
        export(&input, &output, &opts)
    };
    match result {
        Ok(()) => {
            let size = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
            println!("wrote {} ({} bytes)", output.display(), size);
            0
        }
        Err(e) => fail(&e.to_string()),
    }
}

/// Render with the gpui/CoreText fidelity rasterizer (macOS). Elsewhere, warn
/// and fall back to the software renderer.
#[cfg(target_os = "macos")]
fn render_fidelity(
    input: &std::path::Path,
    output: &std::path::Path,
    opts: &Options,
    scale: f32,
) -> Result<(), export::Error> {
    let (cfg, _diagnostics) = config::load();
    let base = crate::font::build(&cfg);
    let raster = crate::fidelity::GpuiRaster::new(base, cfg.font_size, scale);
    export::render_file(input, output, opts, raster)
}

#[cfg(not(target_os = "macos"))]
fn render_fidelity(
    input: &std::path::Path,
    output: &std::path::Path,
    opts: &Options,
    _scale: f32,
) -> Result<(), export::Error> {
    eprintln!("sinclair export: --fidelity is macOS-only; using the software renderer");
    export(input, output, opts)
}

fn fail(message: &str) -> i32 {
    eprintln!("sinclair export: {message}");
    1
}
