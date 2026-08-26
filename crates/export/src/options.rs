//! Export options and the output format, inferred from the file extension.

use std::path::Path;

/// Knobs that shape a render. Defaults produce a watchable 30fps clip with
/// long idle gaps collapsed to two seconds.
#[derive(Debug, Clone)]
pub struct Options {
  /// Output frames per second.
  pub fps: u32,
  /// Playback speed multiplier (2.0 = twice as fast).
  pub speed: f64,
  /// Collapse any idle gap longer than this many seconds down to it. `None`
  /// preserves the original timing exactly.
  pub idle_cap: Option<f64>,
  /// Hold the final frame this many seconds so the ending is readable.
  pub tail: f64,
  /// Override the recording's column count.
  pub cols: Option<usize>,
  /// Override the recording's row count.
  pub rows: Option<usize>,
  /// Font pixel size; also sets the cell grid, and thus the pixel dimensions.
  pub font_px: f32,
  /// Built-in color scheme name; `None` uses the default dark scheme.
  pub theme: Option<String>,
}

impl Default for Options {
  fn default() -> Self {
    Options {
      fps: 30,
      speed: 1.0,
      idle_cap: Some(2.0),
      tail: 1.0,
      cols: None,
      rows: None,
      font_px: 16.0,
      theme: None,
    }
  }
}

/// A supported output container, chosen by the output file's extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
  /// Animated GIF, encoded in-process (no external tools).
  Gif,
  /// H.264 in an MP4 container (via ffmpeg).
  Mp4,
  /// H.264 in a QuickTime container (via ffmpeg).
  Mov,
  /// VP9 in a WebM container (via ffmpeg).
  Webm,
}

impl Format {
  /// Infer the format from a path's extension, case-insensitively.
  pub fn from_path(path: &Path) -> Option<Format> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
      "gif" => Some(Format::Gif),
      "mp4" => Some(Format::Mp4),
      "mov" => Some(Format::Mov),
      "webm" => Some(Format::Webm),
      _ => None,
    }
  }
}
