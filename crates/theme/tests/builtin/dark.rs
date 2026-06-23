use super::*;

#[test]
fn background_matches_spec() {
    assert_eq!(DARK.background.to_hex(), "#282c34");
}

#[test]
fn is_dark() {
    assert!(DARK.is_dark());
}

#[test]
fn bright_white_is_white() {
    assert_eq!(DARK.ansi[15], Rgb::new(0xff, 0xff, 0xff));
}
