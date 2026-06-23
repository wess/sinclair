use super::*;
use crate::builtin;

fn base() -> &'static Scheme {
    builtin::builtin("dracula").unwrap()
}

fn hex(s: &str) -> Rgb {
    s.parse().unwrap()
}

#[test]
fn no_overrides_is_identity() {
    let out = apply_overrides(base(), None, None, None, None, None, None, &[]).unwrap();
    assert_eq!(out, *base());
}

#[test]
fn field_overrides_apply() {
    let out = apply_overrides(
        base(),
        Some("#000000"),
        Some("#ffffff"),
        Some("#ff0000"),
        Some("00ff00"),
        Some("#abc"),
        Some("#123456"),
        &[],
    )
    .unwrap();
    assert_eq!(out.background, hex("#000000"));
    assert_eq!(out.foreground, hex("#ffffff"));
    assert_eq!(out.cursor, hex("#ff0000"));
    assert_eq!(out.cursor_text, hex("#00ff00"));
    assert_eq!(out.selection_foreground, hex("#aabbcc"));
    assert_eq!(out.selection_background, hex("#123456"));
    // Untouched fields keep base values.
    assert_eq!(out.ansi, base().ansi);
    assert_eq!(out.name, base().name);
}

#[test]
fn partial_override_leaves_rest() {
    let out =
        apply_overrides(base(), Some("#101010"), None, None, None, None, None, &[]).unwrap();
    assert_eq!(out.background, hex("#101010"));
    assert_eq!(out.foreground, base().foreground);
    assert_eq!(out.cursor, base().cursor);
}

#[test]
fn ansi_overrides_apply() {
    let ansi = [(1u8, "#000001".to_string()), (15u8, "#0f0f0f".to_string())];
    let out = apply_overrides(base(), None, None, None, None, None, None, &ansi).unwrap();
    assert_eq!(out.ansi[1], hex("#000001"));
    assert_eq!(out.ansi[15], hex("#0f0f0f"));
    assert_eq!(out.ansi[0], base().ansi[0]);
    assert_eq!(out.ansi[14], base().ansi[14]);
}

#[test]
fn bad_hex_is_reported() {
    let err =
        apply_overrides(base(), Some("nope"), None, None, None, None, None, &[]).unwrap_err();
    match err {
        OverrideError::Hex { field, value, .. } => {
            assert_eq!(field, "background");
            assert_eq!(value, "nope");
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn bad_ansi_hex_is_reported() {
    let ansi = [(3u8, "#zzz".to_string())];
    let err = apply_overrides(base(), None, None, None, None, None, None, &ansi).unwrap_err();
    match err {
        OverrideError::Hex { field, .. } => assert_eq!(field, "ansi[3]"),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn out_of_range_index_is_reported() {
    let ansi = [(16u8, "#ffffff".to_string())];
    let err = apply_overrides(base(), None, None, None, None, None, None, &ansi).unwrap_err();
    assert_eq!(err, OverrideError::Index(16));
}

#[test]
fn errors_display() {
    assert!(OverrideError::Index(200).to_string().contains("200"));
    let err =
        apply_overrides(base(), None, Some("xx"), None, None, None, None, &[]).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("foreground") && msg.contains("xx"));
}
