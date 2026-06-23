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
