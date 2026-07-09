use super::*;
use std::io::Cursor;

fn roundtrip(cols: usize, rows: usize, chunks: &[&[u8]]) -> Cast {
    let dir = std::env::temp_dir();
    let path = dir.join("sinclair-cast-reader.cast");
    let mut rec = crate::Recorder::create(path.clone(), cols, rows, Some("t"), Some(42)).unwrap();
    for c in chunks {
        rec.output(c).unwrap();
    }
    let saved = rec.finish().unwrap();
    let cast = Cast::read(&saved).unwrap();
    let _ = std::fs::remove_file(&saved);
    cast
}

#[test]
fn reads_header_and_output() {
    let cast = roundtrip(80, 24, &[b"hello", b" world"]);
    assert_eq!(cast.header.width, 80);
    assert_eq!(cast.header.height, 24);
    assert_eq!(cast.header.title.as_deref(), Some("t"));
    assert_eq!(cast.header.timestamp, Some(42));
    let out: Vec<&str> = cast.output().map(|e| e.data.as_str()).collect();
    assert_eq!(out, vec!["hello", " world"]);
}

#[test]
fn classifies_event_codes() {
    let text = "{\"version\":2,\"width\":10,\"height\":2}\n\
                [0.1, \"o\", \"out\"]\n\
                [0.2, \"i\", \"in\"]\n\
                [0.3, \"r\", \"20x5\"]\n\
                [0.4, \"m\", \"mark\"]\n";
    let cast = parse(Cursor::new(text)).unwrap();
    let kinds: Vec<EventKind> = cast.events.iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        vec![
            EventKind::Output,
            EventKind::Input,
            EventKind::Resize,
            EventKind::Marker
        ]
    );
    assert_eq!(cast.duration(), 0.4);
    // output() filters to just the "o" event.
    assert_eq!(cast.output().count(), 1);
}

#[test]
fn skips_malformed_lines_but_keeps_good_ones() {
    let text = "{\"version\":2,\"width\":8,\"height\":3}\n\
                [0.1, \"o\", \"good\"]\n\
                not json at all\n\
                [0.2, \"o\"]\n\
                [0.3, \"o\", \"also good\"]\n";
    let cast = parse(Cursor::new(text)).unwrap();
    let out: Vec<&str> = cast.output().map(|e| e.data.as_str()).collect();
    assert_eq!(out, vec!["good", "also good"]);
}

#[test]
fn empty_file_is_an_error() {
    assert!(parse(Cursor::new("")).is_err());
}

#[test]
fn missing_geometry_defaults_are_sane() {
    let cast = parse(Cursor::new("{\"version\":2}\n")).unwrap();
    assert_eq!(cast.header.width, 80);
    assert_eq!(cast.header.height, 24);
    assert!(cast.events.is_empty());
    assert_eq!(cast.duration(), 0.0);
}
