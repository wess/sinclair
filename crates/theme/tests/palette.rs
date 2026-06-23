use super::*;
use crate::builtin;

fn palette() -> Palette {
    Palette::from_scheme(builtin::builtin("dark").unwrap())
}

#[test]
fn ansi_slots_come_from_scheme() {
    let scheme = builtin::builtin("dark").unwrap();
    let p = palette();
    for i in 0..16 {
        assert_eq!(p.get(i as u8), scheme.ansi[i]);
    }
}

#[test]
fn cube_known_values() {
    let p = palette();
    assert_eq!(p.get(16), "#000000".parse().unwrap());
    assert_eq!(p.get(21), "#0000ff".parse().unwrap());
    assert_eq!(p.get(46), "#00ff00".parse().unwrap());
    assert_eq!(p.get(196), "#ff0000".parse().unwrap());
    assert_eq!(p.get(231), "#ffffff".parse().unwrap());
    // 16 + 36*1 + 6*2 + 3 = 67 -> (95, 135, 175)
    assert_eq!(p.get(67), Rgb::new(95, 135, 175));
}

#[test]
fn cube_uses_standard_steps() {
    let p = palette();
    for i in 16..=231u8 {
        let c = p.get(i);
        for v in [c.r, c.g, c.b] {
            assert!(CUBE_STEPS.contains(&v), "index {i} component {v}");
        }
    }
}

#[test]
fn grayscale_ramp() {
    let p = palette();
    assert_eq!(p.get(232), "#080808".parse().unwrap());
    assert_eq!(p.get(244), "#808080".parse().unwrap());
    assert_eq!(p.get(255), "#eeeeee".parse().unwrap());
    for i in 232..=255u8 {
        let c = p.get(i);
        let v = 8 + 10 * (i - 232);
        assert_eq!(c, Rgb::new(v, v, v));
    }
}

#[test]
fn overrides_apply_on_top() {
    let scheme = builtin::builtin("dark").unwrap();
    let red = Rgb::new(0xff, 0x00, 0x00);
    let teal = Rgb::new(0x00, 0x80, 0x80);
    let p = build(scheme, &[(0, red), (231, teal), (255, red)]);
    assert_eq!(p.get(0), red);
    assert_eq!(p.get(231), teal);
    assert_eq!(p.get(255), red);
    // Untouched indices keep their computed values.
    assert_eq!(p.get(1), scheme.ansi[1]);
    assert_eq!(p.get(232), "#080808".parse().unwrap());
}

#[test]
fn later_overrides_win() {
    let scheme = builtin::builtin("dark").unwrap();
    let a = Rgb::new(1, 1, 1);
    let b = Rgb::new(2, 2, 2);
    let p = build(scheme, &[(42, a), (42, b)]);
    assert_eq!(p.get(42), b);
}

#[test]
fn set_mutates() {
    let mut p = palette();
    let c = Rgb::new(9, 9, 9);
    p.set(100, c);
    assert_eq!(p.get(100), c);
}
