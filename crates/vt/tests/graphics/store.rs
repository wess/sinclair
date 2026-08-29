use super::*;
use crate::graphics::parse_control;

fn ctrl(s: &str) -> Control {
  parse_control(s.as_bytes())
}

fn solid(w: usize, h: usize, px: [u8; 4]) -> Image {
  Image {
    width: w,
    height: h,
    rgba: px.repeat(w * h).into(),
  }
}

#[test]
fn client_ids_and_terminal_numbers_share_one_space() {
  let mut store = Store::default();
  let a = store.insert(&ctrl("i=7"), solid(1, 1, [1, 1, 1, 255]));
  assert_eq!(a, 7);

  // An `I=` transmission has the terminal pick an id, and the number then
  // resolves to it.
  let b = store.insert(&ctrl("I=3"), solid(1, 1, [2, 2, 2, 255]));
  assert_ne!(b, 7);
  assert_eq!(store.resolve(&ctrl("I=3")), Some(b));
  assert_eq!(store.resolve(&ctrl("i=7")), Some(7));
  assert_eq!(store.resolve(&ctrl("i=99")), None);
  assert_eq!(store.resolve(&ctrl("I=99")), None);
}

#[test]
fn retransmitting_a_number_reuses_its_id() {
  // Otherwise a client animating by number leaks one id per frame.
  let mut store = Store::default();
  let first = store.insert(&ctrl("I=5"), solid(1, 1, [0, 0, 0, 255]));
  let second = store.insert(&ctrl("I=5"), solid(1, 1, [9, 9, 9, 255]));
  assert_eq!(first, second);
  assert_eq!(store.len(), 1);
}

#[test]
fn removing_an_image_forgets_its_number() {
  let mut store = Store::default();
  let id = store.insert(&ctrl("I=4"), solid(1, 1, [0, 0, 0, 255]));
  store.remove(id);
  assert_eq!(store.resolve(&ctrl("I=4")), None);
}

#[test]
fn allocation_skips_ids_a_client_already_claimed() {
  let mut store = Store::default();
  store.insert(&ctrl("i=1"), solid(1, 1, [0, 0, 0, 255]));
  store.insert(&ctrl("i=2"), solid(1, 1, [0, 0, 0, 255]));
  let auto = store.insert(&ctrl("I=1"), solid(1, 1, [0, 0, 0, 255]));
  assert!(auto > 2, "allocated {auto} over an occupied id");
}

#[test]
fn a_new_frame_composites_onto_a_solid_background() {
  let mut image = GfxImage::new(0, solid(2, 2, [0, 0, 0, 255]));
  // Y is a 32-bit RGBA background: opaque red.
  let red: u32 = u32::from_be_bytes([255, 0, 0, 255]);
  let rect = solid(1, 1, [0, 0, 255, 255]);
  add_frame(&mut image, &ctrl(&format!("a=f,s=1,v=1,x=1,y=1,Y={red}")), &rect).unwrap();

  assert_eq!(image.frames.len(), 2);
  let px = &image.frames[1].image.rgba;
  assert_eq!(&px[0..4], &[255, 0, 0, 255], "background fills the canvas");
  assert_eq!(&px[12..16], &[0, 0, 255, 255], "the rect lands at 1,1");
  assert_eq!(image.frames[1].gap, DEFAULT_GAP_MS);
}

#[test]
fn a_new_frame_can_start_from_an_earlier_one() {
  let mut image = GfxImage::new(0, solid(2, 1, [10, 20, 30, 255]));
  let rect = solid(1, 1, [40, 50, 60, 255]);
  add_frame(&mut image, &ctrl("a=f,s=1,v=1,x=0,y=0,c=1,z=90"), &rect).unwrap();

  let px = &image.frames[1].image.rgba;
  assert_eq!(&px[0..4], &[40, 50, 60, 255], "the rect overwrote pixel 0");
  assert_eq!(&px[4..8], &[10, 20, 30, 255], "the base showed through");
  assert_eq!(image.frames[1].gap, 90);
}

#[test]
fn a_negative_gap_marks_a_frame_unrendered() {
  let mut image = GfxImage::new(0, solid(1, 1, [0, 0, 0, 255]));
  add_frame(
    &mut image,
    &ctrl("a=f,s=1,v=1,z=-5"),
    &solid(1, 1, [1, 1, 1, 255]),
  )
  .unwrap();
  assert_eq!(image.frames[1].gap, 0);
}

#[test]
fn editing_a_frame_composites_in_place() {
  let mut image = GfxImage::new(0, solid(2, 1, [0, 0, 0, 255]));
  add_frame(
    &mut image,
    &ctrl("a=f,s=2,v=1,c=1"),
    &solid(2, 1, [1, 1, 1, 255]),
  )
  .unwrap();
  let before = image.frames.len();

  // `r=2` edits frame 2 rather than appending a third.
  add_frame(
    &mut image,
    &ctrl("a=f,s=1,v=1,x=1,r=2,X=1"),
    &solid(1, 1, [7, 7, 7, 255]),
  )
  .unwrap();
  assert_eq!(image.frames.len(), before);
  assert_eq!(&image.frames[1].image.rgba[4..8], &[7, 7, 7, 255]);
}

#[test]
fn editing_a_frame_that_does_not_exist_errors() {
  let mut image = GfxImage::new(0, solid(1, 1, [0, 0, 0, 255]));
  let err = add_frame(
    &mut image,
    &ctrl("a=f,s=1,v=1,r=9"),
    &solid(1, 1, [0, 0, 0, 255]),
  );
  assert_eq!(err, Err(GfxError("ENOENT")));
}

#[test]
fn composing_copies_a_rect_between_frames() {
  let mut image = GfxImage::new(0, solid(2, 2, [0, 0, 0, 255]));
  add_frame(
    &mut image,
    &ctrl("a=f,s=2,v=2,c=1"),
    &solid(2, 2, [9, 9, 9, 255]),
  )
  .unwrap();
  // Copy frame 2's top-left pixel into frame 1 at (1,1), overwriting.
  compose_frames(&mut image, &ctrl("a=c,r=1,c=2,x=0,y=0,w=1,h=1,X=1,Y=1,C=1")).unwrap();
  assert_eq!(&image.frames[0].image.rgba[12..16], &[9, 9, 9, 255]);
  assert_eq!(&image.frames[0].image.rgba[0..4], &[0, 0, 0, 255]);
}

#[test]
fn composing_a_frame_onto_itself_is_rejected() {
  let mut image = GfxImage::new(0, solid(1, 1, [0, 0, 0, 255]));
  assert!(compose_frames(&mut image, &ctrl("a=c,r=1,c=1")).is_err());
  assert!(compose_frames(&mut image, &ctrl("a=c,r=1")).is_err());
}

#[test]
fn alpha_blending_mixes_rather_than_replacing() {
  let mut image = GfxImage::new(0, solid(1, 1, [0, 0, 0, 255]));
  // Half-transparent white over opaque black, alpha-blended (X unset = 0).
  add_frame(
    &mut image,
    &ctrl("a=f,s=1,v=1,c=1"),
    &solid(1, 1, [255, 255, 255, 128]),
  )
  .unwrap();
  let px = &image.frames[1].image.rgba;
  assert!(
    (100..=155).contains(&px[0]),
    "expected a mid grey, got {}",
    px[0]
  );
  assert_eq!(px[3], 255);
}

#[test]
fn crop_clips_to_the_source() {
  let src = Image {
    width: 2,
    height: 2,
    rgba: vec![
      1, 1, 1, 255, 2, 2, 2, 255, //
      3, 3, 3, 255, 4, 4, 4, 255,
    ]
    .into(),
  };
  let out = crop(&src, 1, 0, 5, 5);
  assert_eq!((out.width, out.height), (1, 2));
  assert_eq!(out.rgba.as_ref(), &[2, 2, 2, 255, 4, 4, 4, 255]);

  assert_eq!(crop(&src, 9, 9, 1, 1).width, 0);
}
