use super::*;

#[test]
fn intern_dedupes_identical_links() {
    let mut h = Hyperlinks::default();
    let a = h.intern(Some("x".into()), "https://a".into()).unwrap();
    let b = h.intern(Some("x".into()), "https://a".into()).unwrap();
    assert_eq!(a, b);
    assert_eq!(h.get(a).unwrap().uri, "https://a");
    assert_eq!(h.get(a).unwrap().id.as_deref(), Some("x"));
}

#[test]
fn distinct_links_get_distinct_ids() {
    let mut h = Hyperlinks::default();
    let a = h.intern(None, "https://a".into()).unwrap();
    let b = h.intern(None, "https://b".into()).unwrap();
    // Same uri but different id parameter is also distinct.
    let c = h.intern(Some("g".into()), "https://a".into()).unwrap();
    assert_ne!(a, b);
    assert_ne!(a, c);
    assert_eq!(h.get(b).unwrap().uri, "https://b");
}

#[test]
fn clear_empties_the_registry() {
    let mut h = Hyperlinks::default();
    let a = h.intern(None, "https://a".into()).unwrap();
    h.clear();
    assert!(h.get(a).is_none());
    // Ids restart after a clear.
    let b = h.intern(None, "https://b".into()).unwrap();
    assert_eq!(a, b);
}

#[test]
fn oversized_fields_are_rejected() {
    let mut h = Hyperlinks::default();
    assert!(h.intern(None, "x".repeat(MAX_URI_BYTES + 1)).is_none());
    assert!(h
        .intern(Some("x".repeat(MAX_ID_BYTES + 1)), "https://a".into())
        .is_none());
}

#[test]
fn registry_has_a_byte_budget() {
    let mut h = Hyperlinks::default();
    let mut accepted = 0;
    for i in 0..1000 {
        let uri = format!("https://example/{i}/{}", "x".repeat(8000));
        if h.intern(None, uri).is_some() {
            accepted += 1;
        } else {
            break;
        }
    }
    assert!(accepted > 0);
    assert!(accepted < 1000);
    assert!(h.bytes <= MAX_REGISTRY_BYTES);
}
