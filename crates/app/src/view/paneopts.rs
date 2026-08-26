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
}

impl gpui::Global for PaneOpts {}

fn install(opts: &config::Options, cx: &mut gpui::App) {
  cx.set_global(PaneOpts {
    visual_bell: opts.visual_bell,
    word_chars: opts.word_chars.clone(),
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
