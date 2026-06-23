use super::*;

#[test]
fn bools() {
    assert_eq!(parse_bool("true"), Some(true));
    assert_eq!(parse_bool("TRUE"), Some(true));
    assert_eq!(parse_bool("1"), Some(true));
    assert_eq!(parse_bool("yes"), Some(true));
    assert_eq!(parse_bool("false"), Some(false));
    assert_eq!(parse_bool("0"), Some(false));
    assert_eq!(parse_bool("no"), Some(false));
    assert_eq!(parse_bool("maybe"), None);
    assert_eq!(parse_bool(""), None);
}

#[test]
fn numbers() {
    assert_eq!(parse_f32("13.5"), Some(13.5));
    assert_eq!(parse_f32("13"), Some(13.0));
    assert_eq!(parse_f32("abc"), None);
    assert_eq!(parse_u32("42"), Some(42));
    assert_eq!(parse_u32("-1"), None);
    assert_eq!(parse_usize("10000"), Some(10000));
    assert_eq!(parse_usize("x"), None);
}

#[test]
fn f32_ranges() {
    assert_eq!(parse_f32_range("1.5", 1.0, 21.0), Some(1.5));
    assert_eq!(parse_f32_range("0.5", 1.0, 21.0), Some(1.0));
    assert_eq!(parse_f32_range("100", 1.0, 21.0), Some(21.0));
    assert_eq!(parse_f32_range("abc", 1.0, 21.0), None);
    assert_eq!(parse_f32_range("NaN", 1.0, 21.0), None);
    assert_eq!(parse_f32_range("inf", 1.0, 21.0), None);
}

#[test]
fn adjusts() {
    assert_eq!(parse_adjust("2"), Some(2));
    assert_eq!(parse_adjust("-1"), Some(-1));
    assert_eq!(parse_adjust("+3"), Some(3));
    assert_eq!(parse_adjust("4px"), Some(4));
    assert_eq!(parse_adjust("-2px"), Some(-2));
    assert_eq!(parse_adjust("2 px"), Some(2));
    assert_eq!(parse_adjust("10%"), None);
    assert_eq!(parse_adjust("abc"), None);
    assert_eq!(parse_adjust(""), None);
}

#[test]
fn colors() {
    assert_eq!(parse_color("#1d1f21"), Some("#1d1f21".to_string()));
    assert_eq!(parse_color("1d1f21"), Some("#1d1f21".to_string()));
    assert_eq!(parse_color("#FFAA00"), Some("#ffaa00".to_string()));
    assert_eq!(parse_color("#fff"), None);
    assert_eq!(parse_color("red"), None);
    assert_eq!(parse_color("#12345g"), None);
    assert_eq!(parse_color(""), None);
}

#[test]
fn fontfeatures() {
    assert_eq!(parse_fontfeature("liga"), Some("liga".to_string()));
    assert_eq!(parse_fontfeature("-liga"), Some("-liga".to_string()));
    assert_eq!(parse_fontfeature("+ss01"), Some("+ss01".to_string()));
    assert_eq!(parse_fontfeature("-"), None);
    assert_eq!(parse_fontfeature("no spaces"), None);
    assert_eq!(parse_fontfeature(""), None);
}

#[test]
fn unquoting() {
    assert_eq!(unquote("\"hello\""), "hello");
    assert_eq!(unquote("hello"), "hello");
    assert_eq!(unquote("  \"spaced value\"  "), "spaced value");
    assert_eq!(unquote("\""), "\"");
    assert_eq!(unquote(""), "");
    assert_eq!(unquote("\"\""), "");
}

#[test]
fn palette() {
    assert_eq!(parse_palette("0=#1d1f21"), Some((0, "#1d1f21".to_string())));
    assert_eq!(
        parse_palette(" 15 = #FFFFFF "),
        Some((15, "#FFFFFF".to_string()))
    );
    assert_eq!(parse_palette("256=#000000"), None); // index overflows u8
    assert_eq!(parse_palette("0=1d1f21"), None); // missing #
    assert_eq!(parse_palette("0=#zzz"), None); // bad hex
    assert_eq!(parse_palette("0=#12345"), None); // short hex
    assert_eq!(parse_palette("#1d1f21"), None); // missing index
}
