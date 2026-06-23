use super::*;

#[test]
fn default_is_default_variant() {
    assert_eq!(Color::default(), Color::Default);
}

#[test]
fn named_maps_to_indexed() {
    assert_eq!(Color::from(NamedColor::Black), Color::Indexed(0));
    assert_eq!(Color::from(NamedColor::Red), Color::Indexed(1));
    assert_eq!(Color::from(NamedColor::BrightWhite), Color::Indexed(15));
    assert_eq!(NamedColor::BrightBlack.index(), 8);
}
