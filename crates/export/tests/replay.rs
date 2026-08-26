use super::*;
use crate::options::Options;
use std::io::Cursor;

fn cast_of(lines: &str) -> cast::Cast {
  cast::parse(Cursor::new(lines.to_owned())).unwrap()
}

#[test]
fn emits_declared_frame_count_at_fixed_size() {
  let cast = cast_of(
    "{\"version\":2,\"width\":20,\"height\":4}\n\
         [0.0, \"o\", \"hello\"]\n\
         [0.5, \"o\", \" world\"]\n",
  );
  let opts = Options {
    fps: 10,
    tail: 0.5,
    idle_cap: Some(2.0),
    ..Options::default()
  };
  let mut renderer = Renderer::software(&cast, &opts);
  let (w, h) = renderer.dimensions();
  let expected = renderer.frame_count();

  let mut count = 0;
  let mut sizes_ok = true;
  renderer.run(|img| {
    count += 1;
    if img.width != w || img.height != h {
      sizes_ok = false;
    }
  });

  assert_eq!(count, expected);
  assert!(count > 0);
  assert!(sizes_ok);
  assert!(w > 0 && h > 0);
}

#[test]
fn later_frames_reflect_fed_output() {
  // The first event anchors to t=0 (shown from frame 0); a second event one
  // second later adds more text, so an early frame and a late frame differ.
  let cast = cast_of(
    "{\"version\":2,\"width\":10,\"height\":2}\n\
         [0.0, \"o\", \"A\"]\n\
         [1.0, \"o\", \"B\"]\n",
  );
  let opts = Options {
    fps: 10,
    tail: 0.3,
    idle_cap: None,
    ..Options::default()
  };
  let mut renderer = Renderer::software(&cast, &opts);
  let mut frames: Vec<Vec<u8>> = Vec::new();
  renderer.run(|img| frames.push(img.data.clone()));
  assert!(frames.len() >= 2);
  assert_ne!(
    frames.first().unwrap(),
    frames.last().unwrap(),
    "output fed between frames should change the image"
  );
}
