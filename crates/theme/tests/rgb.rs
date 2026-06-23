use super::*;

#[test]
fn parses_long_form_with_hash() {
    assert_eq!(
        "#1a2b3c".parse::<Rgb>().unwrap(),
        Rgb::new(0x1a, 0x2b, 0x3c)
    );
}

#[test]
fn parses_long_form_without_hash() {
    assert_eq!("ffcc00".parse::<Rgb>().unwrap(), Rgb::new(0xff, 0xcc, 0x00));
}

#[test]
fn parses_short_form() {
    assert_eq!("#abc".parse::<Rgb>().unwrap(), Rgb::new(0xaa, 0xbb, 0xcc));
    assert_eq!("#fff".parse::<Rgb>().unwrap(), Rgb::new(0xff, 0xff, 0xff));
    assert_eq!("#000".parse::<Rgb>().unwrap(), Rgb::new(0, 0, 0));
}

#[test]
fn parses_uppercase() {
    assert_eq!(
        "#1A2B3C".parse::<Rgb>().unwrap(),
        Rgb::new(0x1a, 0x2b, 0x3c)
    );
}

#[test]
fn rejects_bad_length() {
    assert_eq!("".parse::<Rgb>(), Err(ParseRgbError::Length));
    assert_eq!("#".parse::<Rgb>(), Err(ParseRgbError::Length));
    assert_eq!("#12345".parse::<Rgb>(), Err(ParseRgbError::Length));
    assert_eq!("#1234567".parse::<Rgb>(), Err(ParseRgbError::Length));
}

#[test]
fn rejects_bad_digits() {
    assert_eq!("#gggggg".parse::<Rgb>(), Err(ParseRgbError::Digit));
    assert_eq!("xyz".parse::<Rgb>(), Err(ParseRgbError::Digit));
    assert_eq!("#12 45 ".parse::<Rgb>(), Err(ParseRgbError::Digit));
}

#[test]
fn rejects_non_ascii() {
    // 3 bytes, but not hex digits; must not panic on char boundaries.
    assert_eq!("ä1".parse::<Rgb>(), Err(ParseRgbError::Digit));
}

#[test]
fn to_hex_roundtrip() {
    let c = Rgb::new(0x28, 0x2c, 0x34);
    assert_eq!(c.to_hex(), "#282c34");
    assert_eq!(c.to_hex().parse::<Rgb>().unwrap(), c);
    assert_eq!(format!("{c}"), "#282c34");
}

#[test]
fn luminance_extremes() {
    assert_eq!(Rgb::new(0, 0, 0).luminance(), 0.0);
    assert!((Rgb::new(255, 255, 255).luminance() - 1.0).abs() < 1e-9);
    let mid = Rgb::new(128, 128, 128).luminance();
    assert!(mid > 0.4 && mid < 0.6);
}
