//! Resolving `vt::Color` cells to concrete RGB, mirroring the app's renderer.

use theme::{Palette, Rgb, Scheme};

/// A resolved color context: the 256-slot palette plus the scheme defaults the
/// terminal's `Color::Default` and cursor map to.
#[derive(Debug, Clone)]
pub struct Colors {
  pub palette: Palette,
  pub fg: Rgb,
  pub bg: Rgb,
  pub cursor: Rgb,
  pub cursor_text: Rgb,
}

impl Colors {
  pub fn from_scheme(scheme: &Scheme) -> Colors {
    Colors {
      palette: Palette::from_scheme(scheme),
      fg: scheme.foreground,
      bg: scheme.background,
      cursor: scheme.cursor,
      cursor_text: scheme.cursor_text,
    }
  }

  /// Resolve one cell color. `default` is the fg or bg to use for
  /// `Color::Default`; `brighten` lifts indexed 0..8 to their bright variant
  /// (used for bold text), matching `app::colors::cell_rgb`.
  pub fn resolve(&self, color: vt::Color, default: Rgb, brighten: bool) -> Rgb {
    match color {
      vt::Color::Default => default,
      vt::Color::Indexed(index) => {
        let index = if brighten && index < 8 {
          index + 8
        } else {
          index
        };
        self.palette.get(index)
      }
      vt::Color::Rgb(r, g, b) => Rgb::new(r, g, b),
    }
  }
}
