//! Per-pane config knobs shared as a gpui global, so adding a key doesn't
//! grow the pane constructor or `Appearance`. Loaded lazily on first use and
//! refreshed by `set_appearance`, which the root runs on every config
//! reload.

/// gpui global carrying pane options that live outside `Appearance`.
pub struct PaneOpts {
  /// Config `visual-bell`: flash the pane on BEL.
  pub visual_bell: bool,
  /// Config `word-chars`: word characters for selection.
  pub word_chars: String,
  /// Config `clipboard-read`: whether programs may read the clipboard.
  pub clipboard_read: config::ClipboardAccess,
  /// Config `vt-window-resize-allowed` / `vt-checksum-report`: the risky
  /// sequences a program may use only when the user opted in.
  pub vt_gates: (bool, bool),
}

impl gpui::Global for PaneOpts {}

fn install(opts: &config::Options, cx: &mut gpui::App) {
  cx.set_global(PaneOpts {
    visual_bell: opts.visual_bell,
    word_chars: opts.word_chars.clone(),
    clipboard_read: opts.clipboard_read,
    vt_gates: (opts.vt_window_resize_allowed, opts.vt_checksum_report),
  });
}

/// Re-read the config file and refresh the global. Diagnostics are the
/// loader's concern; this only wants the resolved values.
pub fn refresh(cx: &mut gpui::App) {
  let (opts, _) = config::load();
  install(&opts, cx);
}

fn ensure(cx: &mut gpui::App) {
  if cx.try_global::<PaneOpts>().is_none() {
    refresh(cx);
  }
}

pub fn visual_bell(cx: &mut gpui::App) -> bool {
  ensure(cx);
  cx.global::<PaneOpts>().visual_bell
}

pub fn word_chars(cx: &mut gpui::App) -> String {
  ensure(cx);
  cx.global::<PaneOpts>().word_chars.clone()
}

pub fn clipboard_read(cx: &mut gpui::App) -> config::ClipboardAccess {
  ensure(cx);
  cx.global::<PaneOpts>().clipboard_read
}

/// Apply the opt-in sequence gates to a pane's terminal.
pub fn apply_vt_gates(term: &mut vt::Terminal, cx: &mut gpui::App) {
  ensure(cx);
  let (resize, checksum) = cx.global::<PaneOpts>().vt_gates;
  term.set_window_resize_allowed(resize);
  term.set_checksum_report(checksum);
}
