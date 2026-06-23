use super::*;

fn sample() -> Scheme {
    Scheme {
        name: "sample",
        ansi: [Rgb::new(1, 2, 3); 16],
        foreground: Rgb::new(0xee, 0xee, 0xee),
        background: Rgb::new(0x10, 0x10, 0x10),
        cursor: Rgb::new(0xff, 0xff, 0xff),
        cursor_text: Rgb::new(0, 0, 0),
        selection_foreground: Rgb::new(0xff, 0xff, 0xff),
        selection_background: Rgb::new(0x33, 0x44, 0x55),
    }
}

#[test]
fn fields_accessible() {
    let s = sample();
    assert_eq!(s.name, "sample");
    assert_eq!(s.ansi[15], Rgb::new(1, 2, 3));
    assert_eq!(s.selection_background, Rgb::new(0x33, 0x44, 0x55));
}

#[test]
fn dark_detection() {
    let mut s = sample();
    assert!(s.is_dark());
    s.background = Rgb::new(0xff, 0xff, 0xff);
    assert!(!s.is_dark());
}
