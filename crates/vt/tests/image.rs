use super::*;

fn img(w: usize, h: usize) -> Image {
  Image {
    width: w,
    height: h,
    rgba: vec![0u8; w * h * 4].into(),
  }
}

fn kitty(k: Kitty) -> Placement {
  Placement {
    id: 1,
    line: 0,
    col: 0,
    image: img(100, 50),
    kitty: Some(k),
  }
}

fn base() -> Kitty {
  Kitty {
    image_id: 1,
    placement_id: 0,
    src: Rect::default(),
    cell_x: 0,
    cell_y: 0,
    cols: 0,
    rows: 0,
    z: 0,
    virtual_placement: false,
    parent: None,
  }
}

#[test]
fn unset_rect_covers_the_whole_image() {
  let r = Rect::default();
  assert_eq!(r.clip(100, 50), (0, 0, 100, 50));
  assert!(r.is_whole(100, 50));
}

#[test]
fn rect_clips_to_the_image() {
  let r = Rect {
    x: 90,
    y: 40,
    w: 999,
    h: 999,
  };
  assert_eq!(r.clip(100, 50), (90, 40, 10, 10));
  assert!(!r.is_whole(100, 50));

  // An origin past the edge yields an empty rectangle, not a panic.
  let past = Rect {
    x: 500,
    y: 500,
    w: 10,
    h: 10,
  };
  assert_eq!(past.clip(100, 50), (100, 50, 0, 0));
}

#[test]
fn draw_size_defaults_to_the_source_pixels() {
  let p = kitty(base());
  assert_eq!(p.draw_size(10, 20), (100.0, 50.0));
}

#[test]
fn cell_box_scales_the_image() {
  // `c=`/`r=` ask for a cell box; the image stretches into it.
  let p = kitty(Kitty {
    cols: 4,
    rows: 3,
    ..base()
  });
  assert_eq!(p.draw_size(10, 20), (40.0, 60.0));
  assert_eq!(p.cell_cols(10, 20), 4);
  assert_eq!(p.cell_rows(10, 20), 3);
}

#[test]
fn source_crop_shrinks_the_drawn_size() {
  let p = kitty(Kitty {
    src: Rect {
      x: 0,
      y: 0,
      w: 20,
      h: 10,
    },
    ..base()
  });
  assert_eq!(p.draw_size(10, 20), (20.0, 10.0));
  assert_eq!(p.cell_cols(10, 20), 2);
  assert_eq!(p.cell_rows(10, 20), 1);
}

#[test]
fn cell_offset_can_push_the_image_onto_another_row() {
  // A 50px image starting 15px down a 20px cell spans four rows, not three.
  let p = kitty(Kitty {
    cell_y: 15,
    ..base()
  });
  assert_eq!(p.cell_rows(10, 20), 4);
}

#[test]
fn sixel_placements_report_natural_size_at_z_zero() {
  let p = Placement {
    id: 1,
    line: 0,
    col: 0,
    image: img(30, 40),
    kitty: None,
  };
  assert_eq!(p.draw_size(10, 20), (30.0, 40.0));
  assert_eq!(p.z(), 0);
  assert!(!p.is_virtual());
  assert_eq!(p.cell_rows(10, 20), 2);
}
