use super::*;

#[test]
fn blend_endpoints_and_midpoint() {
    let a = Rgb::new(0, 0, 0);
    let b = Rgb::new(255, 255, 255);
    assert_eq!(blend(a, b, 0.0), a);
    assert_eq!(blend(a, b, 1.0), b);
    assert_eq!(blend(a, b, 0.5), Rgb::new(128, 128, 128));
    // Out-of-range t clamps.
    assert_eq!(blend(a, b, -1.0), a);
    assert_eq!(blend(a, b, 2.0), b);
}

#[test]
fn blend_mixes_channels_independently() {
    let a = Rgb::new(10, 200, 0);
    let b = Rgb::new(20, 100, 255);
    let m = blend(a, b, 0.1);
    assert_eq!(m, Rgb::new(11, 190, 26));
}
