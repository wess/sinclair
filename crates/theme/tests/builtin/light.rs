use super::*;

#[test]
fn background_is_white() {
    assert_eq!(LIGHT.background, Rgb::new(0xff, 0xff, 0xff));
}

#[test]
fn is_not_dark() {
    assert!(!LIGHT.is_dark());
}

#[test]
fn foreground_contrasts_background() {
    assert!(LIGHT.background.luminance() - LIGHT.foreground.luminance() > 0.5);
}
