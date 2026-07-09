use super::*;
use std::io::Read;

fn read(path: &Path) -> String {
    let mut s = String::new();
    File::open(path).unwrap().read_to_string(&mut s).unwrap();
    s
}

fn tmp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("sinclair-cast-{name}.cast"))
}

#[test]
fn header_then_output_events() {
    let path = tmp("basic");
    let mut rec = Recorder::create(path.clone(), 80, 24, Some("demo"), Some(1000)).unwrap();
    rec.output(b"hello").unwrap();
    let saved = rec.finish().unwrap();
    let text = read(&saved);
    let mut lines = text.lines();

    let header: serde_json::Value = serde_json::from_str(lines.next().unwrap()).unwrap();
    assert_eq!(header["version"], 2);
    assert_eq!(header["width"], 80);
    assert_eq!(header["height"], 24);
    assert_eq!(header["title"], "demo");
    assert_eq!(header["timestamp"], 1000);

    let event: serde_json::Value = serde_json::from_str(lines.next().unwrap()).unwrap();
    assert_eq!(event[1], "o");
    assert_eq!(event[2], "hello");
    let _ = std::fs::remove_file(&saved);
}

#[test]
fn split_utf8_is_not_corrupted() {
    let path = tmp("utf8");
    let mut rec = Recorder::create(path.clone(), 10, 2, None, None).unwrap();
    // "é" is two bytes (0xC3 0xA9); feed them in separate chunks.
    let bytes = "é".as_bytes();
    rec.output(&bytes[..1]).unwrap(); // incomplete: should emit nothing
    rec.output(&bytes[1..]).unwrap(); // completes the char
    let saved = rec.finish().unwrap();
    let text = read(&saved);
    let events: Vec<&str> = text.lines().skip(1).collect();
    // Exactly one event carrying the whole grapheme, no replacement chars.
    assert_eq!(events.len(), 1);
    let event: serde_json::Value = serde_json::from_str(events[0]).unwrap();
    assert_eq!(event[2], "é");
    let _ = std::fs::remove_file(&saved);
}

#[test]
fn invalid_bytes_flush_lossily() {
    let path = tmp("invalid");
    let mut rec = Recorder::create(path.clone(), 10, 2, None, None).unwrap();
    rec.output(&[0xff, b'x']).unwrap(); // 0xff is never valid UTF-8
    let saved = rec.finish().unwrap();
    let text = read(&saved);
    let event: serde_json::Value =
        serde_json::from_str(text.lines().nth(1).unwrap()).unwrap();
    // The invalid byte becomes U+FFFD; the valid byte survives.
    assert!(event[2].as_str().unwrap().ends_with('x'));
    let _ = std::fs::remove_file(&saved);
}
