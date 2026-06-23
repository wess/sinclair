use super::*;

#[test]
fn label_prefers_nonblank_title() {
    assert_eq!(label(Some("vim"), "zsh"), "vim");
    assert_eq!(label(Some(""), "zsh"), "zsh");
    assert_eq!(label(Some("   "), "zsh"), "zsh");
    assert_eq!(label(None, "zsh"), "zsh");
}

const OPT: input::Mods = input::Mods {
    shift: false,
    alt: true,
    ctrl: false,
    cmd: false,
};

fn policy(
    p: config::OptionAsAlt,
    held: bool,
    key: &str,
    key_char: Option<&str>,
) -> (input::Mods, Option<String>) {
    let (mods, text) = option_policy(p, true, held, key, key_char, OPT);
    (mods, text.map(str::to_string))
}

#[test]
fn option_off_drops_alt_and_keeps_composed_glyph() {
    // Default policy: Option composes. Alt is cleared (so arrows stay
    // plain) and the glyph macOS produced is emitted verbatim.
    let (mods, text) = policy(config::OptionAsAlt::False, true, "b", Some("\u{222b}"));
    assert!(!mods.alt);
    assert_eq!(text.as_deref(), Some("\u{222b}"));
    // Arrow: no text, alt cleared -> encode_key emits the plain `ESC[D`.
    let (mods, text) = policy(config::OptionAsAlt::False, true, "left", None);
    assert!(!mods.alt);
    assert_eq!(text, None);
    assert_eq!(
        input::encode_key("left", text.as_deref(), mods, STATE),
        Some(b"\x1b[D".to_vec())
    );
}

#[test]
fn option_as_alt_meta_prefixes_base_key() {
    // Option = Alt/Meta: a letter ESC-prefixes its *base* key, not the
    // composed glyph, so Option+b -> `ESC b`.
    let (mods, text) = policy(config::OptionAsAlt::True, true, "b", Some("\u{222b}"));
    assert!(mods.alt);
    assert_eq!(text.as_deref(), Some("b"));
    assert_eq!(
        input::encode_key("b", text.as_deref(), mods, STATE),
        Some(b"\x1bb".to_vec())
    );
    // Arrow keeps alt -> `ESC[1;3D`.
    let (mods, text) = policy(config::OptionAsAlt::Left, true, "left", None);
    assert_eq!(
        input::encode_key("left", text.as_deref(), mods, STATE),
        Some(b"\x1b[1;3D".to_vec())
    );
}

#[test]
fn non_macos_and_no_option_pass_through() {
    // Off the macOS path, text passes through untouched even with the
    // compose policy.
    let (mods, text) =
        option_policy(config::OptionAsAlt::False, false, true, "b", Some("b"), OPT);
    assert!(mods.alt);
    assert_eq!(text, Some("b"));
    // Option not actually held: untouched regardless of policy.
    let (mods, text) =
        option_policy(config::OptionAsAlt::False, true, false, "b", Some("b"), OPT);
    assert!(mods.alt);
    assert_eq!(text, Some("b"));
}

const STATE: input::TermState = input::TermState {
    cursor_keys_app: false,
    keypad_app: false,
    bracketed_paste: false,
    kitty_flags: 0,
};
