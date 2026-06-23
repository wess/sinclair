use super::*;

fn colors() -> Colors {
    from_config(&config::Options::default())
}

fn no_override(_: u8) -> Option<(u8, u8, u8)> {
    None
}

#[test]
fn default_maps_to_given_default() {
    let c = colors();
    let fg = cell_rgb(vt::Color::Default, c.fg, false, &c.palette, no_override);
    assert_eq!(fg, c.fg);
    let bg = cell_rgb(vt::Color::Default, c.bg, false, &c.palette, no_override);
    assert_eq!(bg, c.bg);
}

#[test]
fn ansi_indices_come_from_scheme() {
    let c = colors();
    let scheme = theme::default_scheme();
    for i in 0..16u8 {
        let got = cell_rgb(vt::Color::Indexed(i), c.fg, false, &c.palette, no_override);
        assert_eq!(got, scheme.ansi[i as usize], "index {i}");
    }
}

#[test]
fn cube_and_grayscale_resolve() {
    let c = colors();
    // 16 + 36*5 + 6*0 + 0 = 196 -> pure red in the xterm cube.
    let red = cell_rgb(
        vt::Color::Indexed(196),
        c.fg,
        false,
        &c.palette,
        no_override,
    );
    assert_eq!(red, Rgb::new(255, 0, 0));
    // Grayscale ramp: 232 -> #080808, 255 -> #eeeeee.
    let lo = cell_rgb(
        vt::Color::Indexed(232),
        c.fg,
        false,
        &c.palette,
        no_override,
    );
    assert_eq!(lo, Rgb::new(8, 8, 8));
    let hi = cell_rgb(
        vt::Color::Indexed(255),
        c.fg,
        false,
        &c.palette,
        no_override,
    );
    assert_eq!(hi, Rgb::new(0xee, 0xee, 0xee));
}

#[test]
fn bold_brightens_only_low_ansi() {
    let c = colors();
    let scheme = theme::default_scheme();
    let bright = cell_rgb(vt::Color::Indexed(1), c.fg, true, &c.palette, no_override);
    assert_eq!(bright, scheme.ansi[9]);
    // Already-bright and extended indices are untouched.
    let same = cell_rgb(vt::Color::Indexed(9), c.fg, true, &c.palette, no_override);
    assert_eq!(same, scheme.ansi[9]);
    let cube = cell_rgb(vt::Color::Indexed(196), c.fg, true, &c.palette, no_override);
    assert_eq!(cube, Rgb::new(255, 0, 0));
}

#[test]
fn osc4_override_wins_after_brightening() {
    let c = colors();
    let ovr = |i: u8| (i == 9).then_some((1u8, 2u8, 3u8));
    let got = cell_rgb(vt::Color::Indexed(1), c.fg, true, &c.palette, ovr);
    assert_eq!(got, Rgb::new(1, 2, 3));
    // Index 1 itself is not overridden, so unbrightened stays themed.
    let plain = cell_rgb(vt::Color::Indexed(1), c.fg, false, &c.palette, ovr);
    assert_eq!(plain, theme::default_scheme().ansi[1]);
}

#[test]
fn truecolor_passes_through() {
    let c = colors();
    let got = cell_rgb(
        vt::Color::Rgb(12, 34, 56),
        c.fg,
        true,
        &c.palette,
        no_override,
    );
    assert_eq!(got, Rgb::new(12, 34, 56));
}

#[test]
fn config_overrides_apply() {
    let mut opts = config::Options::default();
    opts.foreground = Some("#102030".to_string());
    opts.background = Some("#abcdef".to_string());
    opts.palette = vec![(1, "#ff0000".to_string()), (200, "#00ff00".to_string())];
    let c = from_config(&opts);
    assert_eq!(c.fg, Rgb::new(0x10, 0x20, 0x30));
    assert_eq!(c.bg, Rgb::new(0xab, 0xcd, 0xef));
    assert_eq!(c.palette.get(1), Rgb::new(255, 0, 0));
    assert_eq!(c.palette.get(200), Rgb::new(0, 255, 0));
}

#[test]
fn selection_colors_come_from_scheme() {
    let c = colors();
    let scheme = theme::default_scheme();
    assert_eq!(c.selection_bg, scheme.selection_background);
    assert_eq!(c.selection_fg, scheme.selection_foreground);
}

#[test]
fn bad_config_colors_fall_back() {
    let mut opts = config::Options::default();
    opts.foreground = Some("nonsense".to_string());
    opts.palette = vec![(1, "alsobad".to_string())];
    let c = from_config(&opts);
    let scheme = theme::default_scheme();
    assert_eq!(c.fg, scheme.foreground);
    assert_eq!(c.palette.get(1), scheme.ansi[1]);
}

#[test]
fn rgba_conversion_is_opaque_unit_range() {
    let v = rgba(Rgb::new(255, 0, 128));
    assert_eq!(v.r, 1.0);
    assert_eq!(v.g, 0.0);
    assert!((v.b - 128.0 / 255.0).abs() < 1e-6);
    assert_eq!(v.a, 1.0);
}
