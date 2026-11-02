use super::*;

#[test]
fn frames_round_trip() {
  let mut buf = Vec::new();
  write(&mut buf, OUTPUT, b"hello").unwrap();
  write_json(&mut buf, INFO, &serde_json::json!({ "id": "x" })).unwrap();
  let mut r = &buf[..];
  assert_eq!(read(&mut r).unwrap(), Frame { kind: OUTPUT, payload: b"hello".to_vec() });
  assert_eq!(read(&mut r).unwrap().json().unwrap()["id"], "x");
  assert_eq!(read(&mut r).unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
}

#[test]
fn oversized_lengths_are_refused() {
  let head = [OUTPUT, 0xff, 0xff, 0xff, 0xff];
  assert_eq!(read(&mut &head[..]).unwrap_err().kind(), io::ErrorKind::InvalidData);
}

#[test]
fn colors_round_trip() {
  let mut palette = [(0, 0, 0); 256];
  palette[200] = (1, 2, 3);
  let c = vt::ReportColors {
    foreground: (10, 20, 30),
    background: (40, 50, 60),
    cursor: (70, 80, 90),
    palette,
  };
  let back = parse_colors(&colors(&c)).unwrap();
  assert_eq!(back.foreground, c.foreground);
  assert_eq!(back.cursor, c.cursor);
  assert_eq!(back.palette[200], (1, 2, 3));
  assert!(parse_colors(&[0; 5]).is_none());
}
