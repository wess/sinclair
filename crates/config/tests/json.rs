use super::*;

#[test]
fn scalars() {
    assert_eq!(parse("true").unwrap(), Value::Bool(true));
    assert_eq!(parse("false").unwrap(), Value::Bool(false));
    assert_eq!(parse("null").unwrap(), Value::Null);
    assert_eq!(parse("14").unwrap(), Value::Num(14.0));
    assert_eq!(parse("-0.5").unwrap(), Value::Num(-0.5));
    assert_eq!(parse("\"hi\"").unwrap(), Value::Str("hi".into()));
}

#[test]
fn escapes() {
    assert_eq!(
        parse(r#""a\"b\\c\ndA""#).unwrap(),
        Value::Str("a\"b\\c\ndA".into())
    );
    // Surrogate pair and raw multibyte.
    assert_eq!(parse(r#""😀""#).unwrap(), Value::Str("\u{1f600}".into()));
    assert_eq!(parse("\"é\"").unwrap(), Value::Str("é".into()));
    // \u escapes, including a surrogate pair.
    assert_eq!(parse("\"\\u0041\"").unwrap(), Value::Str("A".into()));
    assert_eq!(
        parse("\"\\ud83d\\ude00\"").unwrap(),
        Value::Str("\u{1f600}".into())
    );
}

#[test]
fn objects_and_arrays() {
    let v = parse(r#"{ "a": 1, "b": [true, "x"], "c": { "d": null } }"#).unwrap();
    let Value::Obj(members) = v else {
        panic!("expected object")
    };
    assert_eq!(members.len(), 3);
    assert_eq!(members[0].key, "a");
    assert_eq!(
        members[1].value,
        Value::Arr(vec![Value::Bool(true), Value::Str("x".into())])
    );
}

#[test]
fn comments_and_trailing_commas() {
    let text = "// header\n{\n  // per-key note\n  \"a\": 1, /* inline */\n  \"b\": [1, 2,],\n}\n";
    let members = root(text).unwrap();
    assert_eq!(members.len(), 2);
    assert_eq!(members[0].line, 4);
    assert_eq!(
        members[1].value,
        Value::Arr(vec![Value::Num(1.0), Value::Num(2.0)])
    );
}

#[test]
fn blank_root_is_empty() {
    assert!(root("").unwrap().is_empty());
    assert!(root("  // just a comment\n").unwrap().is_empty());
}

#[test]
fn non_object_root_is_an_error() {
    assert!(root("[1, 2]").is_err());
}

#[test]
fn errors_carry_lines() {
    let e = parse("{\n  \"a\": nope\n}").unwrap_err();
    assert_eq!(e.line, 2);
    assert!(parse("{\"a\": 1").is_err());
    assert!(parse("{\"a\": \"unterminated}").is_err());
    assert!(parse("{\"a\": 1} extra").is_err());
}

#[test]
fn quote_escapes() {
    assert_eq!(quote("a\"b\\c\nd"), r#""a\"b\\c\nd""#);
    assert_eq!(quote("plain"), "\"plain\"");
}

#[test]
fn num_formats_integers_bare() {
    assert_eq!(num(14.0), "14");
    assert_eq!(num(-3.0), "-3");
    assert_eq!(num(0.7), "0.7");
}
