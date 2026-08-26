//! Pane badge: a faint watermark string (host / cwd / custom) drawn in the
//! corner of every terminal pane. The template is app-wide config, shared as a
//! gpui global; each pane substitutes its own `{cwd}` / `{host}` at render.

/// gpui global holding the badge template, or `None` when unset.
pub struct Badge(pub Option<String>);

impl gpui::Global for Badge {}

/// The current badge template, if any.
pub fn template(cx: &gpui::App) -> Option<String> {
  cx.try_global::<Badge>().and_then(|b| b.0.clone())
}

/// Install (or clear) the global badge template.
pub fn install(text: &Option<String>, cx: &mut gpui::App) {
  cx.set_global(Badge(text.clone().filter(|s| !s.trim().is_empty())));
}

/// Substitute `{cwd}` and `{host}` in a badge template.
pub fn render(template: &str, cwd: Option<&str>, host: &str) -> String {
  template
    .replace("{cwd}", cwd.unwrap_or(""))
    .replace("{host}", host)
}

/// The machine's short hostname, computed once.
pub fn hostname() -> String {
  use std::sync::OnceLock;
  static HOST: OnceLock<String> = OnceLock::new();
  HOST
    .get_or_init(|| {
      std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().split('.').next().unwrap_or("").to_string())
        .unwrap_or_default()
    })
    .clone()
}
