use super::*;

#[test]
fn config_overrides_apply() {
  let opts = config::Options {
    foreground: Some("#102030".to_string()),
    background: Some("#abcdef".to_string()),
    palette: vec![(1, "#ff0000".to_string()), (200, "#00ff00".to_string())],
    ..Default::default()
  };
  let c = from_config(&opts, true);
  assert_eq!(c.fg, Rgb::new(0x10, 0x20, 0x30));
  assert_eq!(c.bg, Rgb::new(0xab, 0xcd, 0xef));
  assert_eq!(c.palette.get(1), Rgb::new(255, 0, 0));
  assert_eq!(c.palette.get(200), Rgb::new(0, 255, 0));
}

#[test]
fn cursor_overrides_apply() {
  let opts = config::Options {
    cursor_color: Some("#ff00ff".to_string()),
    cursor_text: Some("#001122".to_string()),
    ..Default::default()
  };
  let c = from_config(&opts, true);
  assert_eq!(c.cursor, Rgb::new(0xff, 0x00, 0xff));
  assert_eq!(c.cursor_text, Rgb::new(0x00, 0x11, 0x22));
  // Bad values fall back to the scheme.
  let opts = config::Options {
    cursor_color: Some("nonsense".to_string()),
    ..Default::default()
  };
  let c = from_config(&opts, true);
  assert_eq!(c.cursor, theme::default_scheme().cursor);
}

#[test]
fn selection_overrides_apply() {
  let opts = config::Options {
    selection_foreground: Some("#111111".to_string()),
    selection_background: Some("#eeeeee".to_string()),
    ..Default::default()
  };
  let c = from_config(&opts, true);
  assert_eq!(c.selection_fg, Rgb::new(0x11, 0x11, 0x11));
  assert_eq!(c.selection_bg, Rgb::new(0xee, 0xee, 0xee));
}

#[test]
fn bad_config_colors_fall_back() {
  let opts = config::Options {
    foreground: Some("nonsense".to_string()),
    palette: vec![(1, "alsobad".to_string())],
    ..Default::default()
  };
  let c = from_config(&opts, true);
  let scheme = theme::default_scheme();
  assert_eq!(c.fg, scheme.foreground);
  assert_eq!(c.palette.get(1), scheme.ansi[1]);
}

#[test]
fn defaults_match_the_scheme_constructor() {
  let a = from_config(&config::Options::default(), true);
  let b = Colors::from_scheme(theme::default_scheme());
  assert_eq!(a.fg, b.fg);
  assert_eq!(a.bg, b.bg);
  assert_eq!(a.cursor, b.cursor);
  assert_eq!(a.selection_bg, b.selection_bg);
  assert_eq!(a.palette.get(42), b.palette.get(42));
}
