use crate::graphics::AnimState;
use crate::term::Terminal;

/// Standard base64 (no line breaks) — the graphics payload encoding.
fn b64(data: &[u8]) -> String {
  const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  let mut out = String::new();
  for chunk in data.chunks(3) {
    let b0 = chunk[0] as u32;
    let b1 = *chunk.get(1).unwrap_or(&0) as u32;
    let b2 = *chunk.get(2).unwrap_or(&0) as u32;
    let n = (b0 << 16) | (b1 << 8) | b2;
    out.push(A[(n >> 18 & 63) as usize] as char);
    out.push(A[(n >> 12 & 63) as usize] as char);
    out.push(if chunk.len() > 1 {
      A[(n >> 6 & 63) as usize] as char
    } else {
      '='
    });
    out.push(if chunk.len() > 2 {
      A[(n & 63) as usize] as char
    } else {
      '='
    });
  }
  out
}

fn term() -> Terminal {
  let mut t = Terminal::new(20, 6, 100);
  t.set_cell_pixels(8, 16);
  t
}

/// Feed one graphics command with a raw RGBA payload.
fn gfx(t: &mut Terminal, control: &str, payload: &[u8]) {
  t.feed(format!("\x1b_G{control};{}\x1b\\", b64(payload)).as_bytes());
}

/// Feed one graphics command with no payload.
fn cmd(t: &mut Terminal, control: &str) {
  t.feed(format!("\x1b_G{control}\x1b\\").as_bytes());
}

fn reply(t: &mut Terminal) -> String {
  String::from_utf8_lossy(&t.take_output()).into_owned()
}

/// A `w`x`h` opaque image of one color.
fn solid(w: usize, h: usize, px: [u8; 4]) -> Vec<u8> {
  px.repeat(w * h)
}

// transmission

#[test]
fn transmit_stores_without_placing() {
  let mut t = term();
  gfx(&mut t, "a=t,f=32,s=2,v=2,i=1", &solid(2, 2, [1, 2, 3, 255]));
  assert!(t.images().is_empty());
  assert!(t.graphics_image(1).is_some());
  assert_eq!(reply(&mut t), "\x1b_Gi=1;OK\x1b\\");
}

#[test]
fn an_image_number_gets_an_id_the_terminal_reports_back() {
  let mut t = term();
  gfx(&mut t, "a=t,f=32,s=1,v=1,I=7", &solid(1, 1, [0, 0, 0, 255]));
  let out = reply(&mut t);
  // The response names both spellings so the client can learn its id.
  assert!(out.starts_with("\x1b_Gi="), "{out:?}");
  assert!(out.contains(",I=7;OK"), "{out:?}");
}

#[test]
fn a_number_resolves_to_the_newest_image_that_claimed_it() {
  let mut t = term();
  gfx(&mut t, "a=t,f=32,s=1,v=1,I=3", &solid(1, 1, [9, 9, 9, 255]));
  let _ = reply(&mut t);
  cmd(&mut t, "a=p,I=3");
  assert_eq!(t.images().len(), 1, "display by number found the image");
}

#[test]
fn chunked_transfers_reassemble() {
  let mut t = term();
  let rgba = solid(2, 1, [7, 7, 7, 255]); // 8 bytes
  let half = b64(&rgba[..3]);
  let rest = b64(&rgba[3..]);
  t.feed(format!("\x1b_Ga=T,f=32,s=2,v=1,i=5,m=1;{half}\x1b\\").as_bytes());
  assert!(t.images().is_empty(), "nothing placed until the last chunk");
  t.feed(format!("\x1b_Gm=0;{rest}\x1b\\").as_bytes());
  assert_eq!(t.images().len(), 1);
  assert_eq!(t.images()[0].image.width, 2);
}

#[test]
fn a_query_validates_without_storing() {
  let mut t = term();
  gfx(&mut t, "a=q,f=24,s=1,v=1,i=31", &[1, 2, 3]);
  assert_eq!(reply(&mut t), "\x1b_Gi=31;OK\x1b\\");
  assert!(t.graphics_image(31).is_none(), "a query stores nothing");
}

#[test]
fn a_bad_query_reports_the_error_code() {
  let mut t = term();
  gfx(&mut t, "a=q,f=32,s=9,v=9,i=2", &[0; 4]); // payload far too short
  assert_eq!(reply(&mut t), "\x1b_Gi=2;ESIZE\x1b\\");
}

#[test]
fn the_quiet_level_gates_responses() {
  let mut t = term();
  gfx(&mut t, "a=t,f=32,s=1,v=1,i=1,q=1", &solid(1, 1, [0; 4]));
  assert_eq!(reply(&mut t), "", "q=1 suppresses OK");

  gfx(&mut t, "a=t,f=32,s=9,v=9,i=1,q=1", &[0; 4]);
  assert_eq!(reply(&mut t), "\x1b_Gi=1;ESIZE\x1b\\", "q=1 keeps errors");

  gfx(&mut t, "a=t,f=32,s=9,v=9,i=1,q=2", &[0; 4]);
  assert_eq!(reply(&mut t), "", "q=2 suppresses everything");
}

#[test]
fn an_anonymous_command_gets_no_response() {
  // With no id and no number there is nothing to name a reply by.
  let mut t = term();
  gfx(&mut t, "a=T,f=32,s=1,v=1", &solid(1, 1, [0; 4]));
  assert_eq!(reply(&mut t), "");
}

#[test]
fn a_file_medium_transmission_reads_the_file() {
  let mut t = term();
  let path = std::env::temp_dir().join(format!("vt-gfx-file-{}", std::process::id()));
  std::fs::write(&path, solid(2, 2, [4, 5, 6, 255])).unwrap();
  gfx(
    &mut t,
    "a=T,f=32,s=2,v=2,i=1,t=f",
    path.to_str().unwrap().as_bytes(),
  );
  assert_eq!(reply(&mut t), "\x1b_Gi=1;OK\x1b\\");
  assert_eq!(t.images().len(), 1);
  assert_eq!(&t.images()[0].image.rgba[..4], &[4, 5, 6, 255]);
  std::fs::remove_file(&path).unwrap();
}

// display

#[test]
fn display_geometry_is_carried_onto_the_placement() {
  let mut t = term();
  gfx(
    &mut t,
    "a=T,f=32,s=4,v=4,i=1,p=2,x=1,y=1,w=2,h=2,X=3,Y=4,c=5,r=6,z=-7",
    &solid(4, 4, [0, 0, 0, 255]),
  );
  let k = t.images()[0].kitty.as_ref().unwrap();
  assert_eq!(k.image_id, 1);
  assert_eq!(k.placement_id, 2);
  assert_eq!((k.src.x, k.src.y, k.src.w, k.src.h), (1, 1, 2, 2));
  assert_eq!((k.cell_x, k.cell_y), (3, 4));
  assert_eq!((k.cols, k.rows), (5, 6));
  assert_eq!(k.z, -7);
}

#[test]
fn a_placement_id_replaces_the_one_it_names() {
  let mut t = term();
  gfx(&mut t, "a=t,f=32,s=1,v=1,i=1", &solid(1, 1, [0; 4]));
  let _ = reply(&mut t);
  cmd(&mut t, "a=p,i=1,p=9,C=1");
  cmd(&mut t, "a=p,i=1,p=9,C=1");
  assert_eq!(t.images().len(), 1, "the second p=9 replaced the first");

  // A different placement id stacks instead of replacing.
  cmd(&mut t, "a=p,i=1,p=10,C=1");
  assert_eq!(t.images().len(), 2);
}

#[test]
fn displaying_leaves_the_cursor_past_the_image() {
  let mut t = term();
  // 16x32 px at 8x16 cells: two columns, two rows, anchored at the origin.
  gfx(&mut t, "a=T,f=32,s=16,v=32,i=1", &solid(16, 32, [0; 4]));
  assert_eq!(t.cursor_pos(), (1, 2), "one row down, just right of the image");
}

#[test]
fn c_equals_one_leaves_the_cursor_alone() {
  let mut t = term();
  t.feed(b"\x1b[3;5H");
  let before = t.cursor_pos();
  gfx(&mut t, "a=T,f=32,s=16,v=32,i=1,C=1", &solid(16, 32, [0; 4]));
  assert_eq!(t.cursor_pos(), before);
}

#[test]
fn a_display_of_an_unknown_image_reports_enoent() {
  let mut t = term();
  cmd(&mut t, "a=p,i=42");
  assert_eq!(reply(&mut t), "\x1b_Gi=42;ENOENT\x1b\\");
}

#[test]
fn a_virtual_placement_stays_off_the_grid() {
  let mut t = term();
  gfx(&mut t, "a=T,f=32,s=2,v=2,i=1,U=1", &solid(2, 2, [0; 4]));
  assert!(t.images().is_empty(), "U=1 is not anchored at the cursor");
  assert_eq!(t.virtual_placements().len(), 1);
  assert!(t.virtual_placements()[0].is_virtual());
  assert_eq!(t.cursor_pos(), (0, 0), "and never moves the cursor");
}

// delete

/// Place three images: ids 1..=3 at rows 0, 2 and 4, one cell wide.
fn three_placed() -> Terminal {
  let mut t = term();
  for (i, row) in [(1u32, 1), (2, 3), (3, 5)] {
    t.feed(format!("\x1b[{row};1H").as_bytes());
    gfx(
      &mut t,
      &format!("a=T,f=32,s=8,v=16,i={i},C=1"),
      &solid(8, 16, [0; 4]),
    );
  }
  let _ = t.take_output();
  t
}

#[test]
fn delete_all_clears_every_placement() {
  let mut t = three_placed();
  cmd(&mut t, "a=d,d=a");
  assert!(t.images().is_empty());
}

#[test]
fn delete_by_id_keeps_the_others() {
  let mut t = three_placed();
  cmd(&mut t, "a=d,d=i,i=2");
  let left: Vec<u32> = t
    .images()
    .iter()
    .map(|p| p.kitty.as_ref().unwrap().image_id)
    .collect();
  assert_eq!(left, vec![1, 3]);
}

#[test]
fn a_lowercase_delete_keeps_the_pixels_for_a_redisplay() {
  let mut t = three_placed();
  cmd(&mut t, "a=d,d=i,i=2");
  assert!(t.graphics_image(2).is_some(), "lowercase frees no data");
  cmd(&mut t, "a=d,d=I,i=2");
  assert!(t.graphics_image(2).is_none(), "uppercase frees it");
}

#[test]
fn an_uppercase_delete_spares_an_image_another_placement_still_draws() {
  let mut t = term();
  gfx(&mut t, "a=t,f=32,s=1,v=1,i=1", &solid(1, 1, [0; 4]));
  let _ = reply(&mut t);
  cmd(&mut t, "a=p,i=1,p=1,C=1");
  cmd(&mut t, "a=p,i=1,p=2,C=1");
  // Deleting one placement by id leaves the other, so the data must stay.
  cmd(&mut t, "a=d,d=I,i=1,p=1");
  assert_eq!(t.images().len(), 1);
  assert!(t.graphics_image(1).is_some());
}

#[test]
fn delete_by_row_column_and_cursor() {
  let mut t = three_placed();
  // Row 3 (1-based) holds image 2 only.
  cmd(&mut t, "a=d,d=y,y=3");
  assert_eq!(t.images().len(), 2);

  let mut t = three_placed();
  cmd(&mut t, "a=d,d=x,x=1"); // every image sits in column 1
  assert!(t.images().is_empty());

  let mut t = three_placed();
  t.feed(b"\x1b[1;1H");
  cmd(&mut t, "a=d,d=c");
  assert_eq!(t.images().len(), 2, "only the image under the cursor went");
}

#[test]
fn delete_by_row_range_and_point() {
  let mut t = three_placed();
  cmd(&mut t, "a=d,d=r,x=1,y=3"); // rows 1..=3 hold images 1 and 2
  assert_eq!(t.images().len(), 1);

  let mut t = three_placed();
  cmd(&mut t, "a=d,d=p,x=1,y=5"); // the cell holding image 3
  assert_eq!(t.images().len(), 2);
}

#[test]
fn delete_by_z_index() {
  let mut t = term();
  for (i, z) in [(1u32, 0i32), (2, -5), (3, -5)] {
    gfx(
      &mut t,
      &format!("a=T,f=32,s=8,v=16,i={i},z={z},C=1"),
      &solid(8, 16, [0; 4]),
    );
  }
  let _ = t.take_output();
  cmd(&mut t, "a=d,d=z,z=-5");
  assert_eq!(t.images().len(), 1);
  assert_eq!(t.images()[0].kitty.as_ref().unwrap().image_id, 1);
}

#[test]
fn delete_by_cell_and_z_together() {
  let mut t = term();
  gfx(&mut t, "a=T,f=32,s=8,v=16,i=1,z=3,C=1", &solid(8, 16, [0; 4]));
  gfx(&mut t, "a=T,f=32,s=8,v=16,i=2,z=4,C=1", &solid(8, 16, [0; 4]));
  let _ = t.take_output();
  cmd(&mut t, "a=d,d=q,x=1,y=1,z=4");
  assert_eq!(t.images().len(), 1);
  assert_eq!(t.images()[0].kitty.as_ref().unwrap().image_id, 1);
}

#[test]
fn delete_by_number_finds_the_image_behind_it() {
  let mut t = term();
  gfx(&mut t, "a=T,f=32,s=8,v=16,I=12,C=1", &solid(8, 16, [0; 4]));
  let _ = t.take_output();
  assert_eq!(t.images().len(), 1);
  cmd(&mut t, "a=d,d=n,I=12");
  assert!(t.images().is_empty());
}

#[test]
fn deleting_everything_also_takes_sixel_placements() {
  let mut t = term();
  t.inner.place_sixel(crate::image::Image {
    width: 8,
    height: 16,
    rgba: vec![0; 8 * 16 * 4].into(),
  });
  assert_eq!(t.images().len(), 1);
  cmd(&mut t, "a=d,d=i,i=1");
  assert_eq!(t.images().len(), 1, "a sixel has no image id to match");
  cmd(&mut t, "a=d,d=a");
  assert!(t.images().is_empty());
}

// animation

/// A 2x1 image with one extra frame.
fn animated() -> Terminal {
  let mut t = term();
  gfx(&mut t, "a=t,f=32,s=2,v=1,i=1", &solid(2, 1, [1, 1, 1, 255]));
  gfx(
    &mut t,
    "a=f,f=32,s=2,v=1,i=1,c=1,z=100",
    &solid(2, 1, [2, 2, 2, 255]),
  );
  let _ = t.take_output();
  t
}

#[test]
fn transmitting_a_frame_extends_the_image() {
  let t = animated();
  let frames = t.graphics_frames(1).expect("two frames");
  assert_eq!(frames.len(), 2);
  assert_eq!(frames[1].gap, 100);
  assert_eq!(&frames[1].image.rgba[..4], &[2, 2, 2, 255]);
  assert_eq!(&frames[0].image.rgba[..4], &[1, 1, 1, 255]);
}

#[test]
fn a_still_image_reports_no_frames() {
  let mut t = term();
  gfx(&mut t, "a=t,f=32,s=1,v=1,i=1", &solid(1, 1, [0; 4]));
  assert!(t.graphics_frames(1).is_none());
}

#[test]
fn animation_control_sets_playback_state() {
  let mut t = animated();
  cmd(&mut t, "a=a,i=1,s=3,v=4,c=2");
  let play = t.graphics_playback(1).unwrap();
  assert!(play.running && play.looping);
  assert_eq!(play.loops, Some(3), "v=n means n-1 repeats");
  assert_eq!(play.current, 1, "c= is 1-based");

  cmd(&mut t, "a=a,i=1,s=1");
  assert!(!t.graphics_playback(1).unwrap().running);

  cmd(&mut t, "a=a,i=1,s=3,v=1");
  assert_eq!(t.graphics_playback(1).unwrap().loops, None, "v=1 is forever");
}

#[test]
fn animation_control_can_retime_one_frame() {
  let mut t = animated();
  cmd(&mut t, "a=a,i=1,r=2,z=250");
  assert_eq!(t.graphics_frames(1).unwrap()[1].gap, 250);
}

#[test]
fn animation_control_rejects_a_frame_that_is_not_there() {
  let mut t = animated();
  cmd(&mut t, "a=a,i=1,c=9");
  assert_eq!(reply(&mut t), "\x1b_Gi=1;ENOENT\x1b\\");
}

#[test]
fn composing_frames_edits_stored_pixels() {
  let mut t = animated();
  cmd(&mut t, "a=c,i=1,r=1,c=2,x=0,y=0,w=1,h=1,X=1,Y=0,C=1");
  let frames = t.graphics_frames(1).unwrap();
  assert_eq!(&frames[0].image.rgba[4..8], &[2, 2, 2, 255]);
  assert_eq!(&frames[0].image.rgba[0..4], &[1, 1, 1, 255]);
}

#[test]
fn deleting_frames_leaves_the_still_image() {
  let mut t = animated();
  cmd(&mut t, "a=d,d=f,i=1");
  assert!(t.graphics_frames(1).is_none());
  assert!(t.graphics_image(1).is_some());
  assert_eq!(t.graphics_playback(1).unwrap().current, 0);
}

#[test]
fn a_frame_edit_bumps_the_serial_so_caches_notice() {
  let mut t = animated();
  let before = t.graphics_playback(1).unwrap().serial;
  cmd(&mut t, "a=a,i=1,s=3");
  assert!(t.graphics_playback(1).unwrap().serial > before);
}

#[test]
fn playback_starts_stopped_on_the_first_frame() {
  let t = animated();
  let play = t.graphics_playback(1).unwrap();
  assert_eq!(play.current, 0);
  assert!(!play.running);
  assert_eq!(t.inner.gfx_store.get(1).unwrap().state, AnimState::Stopped);
}

// end-to-end shapes real clients emit

#[test]
fn a_placeholder_run_resolves_to_the_virtual_placement_it_names() {
  // What a tmux-style passthrough emits: transmit with a virtual placement,
  // then write placeholder cells that say which piece goes where.
  let mut t = term();
  gfx(&mut t, "a=T,f=32,s=4,v=2,i=1,U=1,c=2,r=1", &solid(4, 2, [1, 2, 3, 255]));
  let _ = reply(&mut t);
  assert_eq!(t.virtual_placements().len(), 1);

  // Foreground = image id 1; two cells, row 0 columns 0 and 1.
  let d = |v: u16| crate::placeholder::diacritic(v).unwrap();
  let cell = format!(
    "\x1b[38;2;0;0;1m\u{10EEEE}{}{}\u{10EEEE}{}{}",
    d(0),
    d(0),
    d(0),
    d(1)
  );
  t.feed(cell.as_bytes());

  let runs = t.placeholder_draws();
  assert_eq!(runs.len(), 1, "adjacent cells collapse into one run");
  assert_eq!(runs[0].image_id, 1);
  assert_eq!((runs[0].row, runs[0].col, runs[0].len), (0, 0, 2));
  assert!(t
    .virtual_placement(runs[0].image_id, runs[0].placement_id)
    .is_some());
}

#[test]
fn placeholder_scanning_costs_nothing_without_a_virtual_placement() {
  // The scan is per-frame, so the overwhelmingly common case — no image
  // placed this way — must not touch a cell.
  let mut t = term();
  t.feed("just some text".as_bytes());
  assert!(t.placeholder_draws().is_empty());
}

#[test]
fn a_scrolled_image_rides_its_text_into_history() {
  let mut t = term();
  gfx(&mut t, "a=T,f=32,s=8,v=16,i=1,C=1", &solid(8, 16, [0; 4]));
  let _ = reply(&mut t);
  assert_eq!(t.images()[0].line, 0);
  // Six newlines on a six-row grid pushes the anchor into scrollback.
  t.feed(b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n");
  assert!(t.images()[0].line < 0, "the anchor followed the text up");
}

#[test]
fn images_survive_an_alt_screen_round_trip_per_screen() {
  let mut t = term();
  gfx(&mut t, "a=T,f=32,s=8,v=16,i=1,C=1", &solid(8, 16, [0; 4]));
  let _ = reply(&mut t);
  t.feed(b"\x1b[?1049h");
  assert!(t.images().is_empty(), "the alt screen starts clean");
  gfx(&mut t, "a=T,f=32,s=8,v=16,i=2,C=1", &solid(8, 16, [0; 4]));
  let _ = reply(&mut t);
  assert_eq!(t.images().len(), 1);
  t.feed(b"\x1b[?1049l");
  assert_eq!(t.images().len(), 1, "the primary screen kept its own");
  assert_eq!(t.images()[0].kitty.as_ref().unwrap().image_id, 1);
}

#[test]
fn an_unrelated_command_mid_transfer_is_not_swallowed_by_it() {
  // Only a command carrying `m=` continues a chunked transfer. Joining
  // blindly would run this delete with the transmit's control block — the
  // image would appear and the delete would vanish.
  let mut t = term();
  gfx(&mut t, "a=T,f=32,s=8,v=16,i=9,C=1", &solid(8, 16, [0; 4]));
  let _ = reply(&mut t);
  assert_eq!(t.images().len(), 1);

  t.feed(b"\x1b_Ga=T,f=32,s=1,v=1,i=1,m=1;AAAA\x1b\\");
  assert!(t.inner.gfx_pending.is_some(), "the transfer is in flight");

  cmd(&mut t, "a=d,d=a");
  assert!(t.images().is_empty(), "the delete ran as a delete");
  assert!(
    t.inner.gfx_pending.is_none(),
    "and the interrupted transfer was abandoned, not left to poison the next"
  );
}

#[test]
fn a_chunked_transfer_survives_being_split_many_ways() {
  let mut t = term();
  let rgba = solid(4, 1, [3, 3, 3, 255]); // 16 bytes
  t.feed(format!("\x1b_Ga=T,f=32,s=4,v=1,i=2,m=1;{}\x1b\\", b64(&rgba[..3])).as_bytes());
  t.feed(format!("\x1b_Gm=1;{}\x1b\\", b64(&rgba[3..9])).as_bytes());
  t.feed(format!("\x1b_Gm=1;{}\x1b\\", b64(&rgba[9..12])).as_bytes());
  t.feed(format!("\x1b_Gm=0;{}\x1b\\", b64(&rgba[12..])).as_bytes());
  assert_eq!(t.images().len(), 1);
  assert_eq!(t.images()[0].image.rgba.as_ref(), rgba.as_slice());
}

// resource bounds

#[test]
fn virtual_placements_are_charged_to_the_pane_budget() {
  // They have no grid anchor, so nothing scrolls them off. Without the
  // budget a client could ask for them in a loop and grow the pane forever.
  let mut t = term();
  gfx(&mut t, "a=t,f=32,s=1,v=1,i=1,q=2", &solid(1, 1, [0; 4]));
  for p in 1..=(crate::term::ops::MAX_GRAPHICS_ITEMS + 50) {
    cmd(&mut t, &format!("a=p,i=1,U=1,p={p},q=2"));
  }
  assert!(
    t.virtual_placements().len() <= crate::term::ops::MAX_GRAPHICS_ITEMS,
    "virtual placements grew to {}",
    t.virtual_placements().len()
  );
}

#[test]
fn animation_frames_are_capped() {
  // Each frame is a whole canvas, so an unbounded count is an unbounded
  // allocation driven entirely by the stream.
  let mut t = term();
  gfx(&mut t, "a=t,f=32,s=2,v=2,i=1,q=2", &solid(2, 2, [0; 4]));
  for _ in 0..(crate::graphics::MAX_FRAMES + 20) {
    gfx(&mut t, "a=f,f=32,s=2,v=2,i=1,c=1,q=2", &solid(2, 2, [1; 4]));
  }
  let frames = t.graphics_frames(1).expect("animated").len();
  assert_eq!(frames, crate::graphics::MAX_FRAMES);

  // And the refusal is reported, not silent.
  gfx(&mut t, "a=f,f=32,s=2,v=2,i=1,c=1", &solid(2, 2, [1; 4]));
  assert_eq!(reply(&mut t), "\x1b_Gi=1;ENOSPC\x1b\\");
}

#[test]
fn a_flood_of_small_images_stays_inside_the_item_cap() {
  // The eviction pass used to rescan every placement per victim, which made
  // exactly this stream quadratic in what it retained.
  let mut t = term();
  for i in 1..=(crate::term::ops::MAX_GRAPHICS_ITEMS + 200) {
    gfx(
      &mut t,
      &format!("a=t,f=32,s=1,v=1,i={i},q=2"),
      &solid(1, 1, [0; 4]),
    );
  }
  assert!(t.inner.gfx_store.len() <= crate::term::ops::MAX_GRAPHICS_ITEMS);
  assert!(t.graphics_memory() <= crate::term::ops::MAX_GRAPHICS_BYTES);
}

#[test]
fn placements_of_a_live_image_are_evicted_before_the_pixels() {
  // Evicting the image out from under a placement would blank it on screen;
  // the oldest placement goes first and the image follows once unused.
  let mut t = term();
  gfx(&mut t, "a=t,f=32,s=1,v=1,i=1,q=2", &solid(1, 1, [0; 4]));
  for p in 1..=(crate::term::ops::MAX_GRAPHICS_ITEMS + 10) {
    cmd(&mut t, &format!("a=p,i=1,p={p},C=1,q=2"));
  }
  assert!(t.images().len() <= crate::term::ops::MAX_GRAPHICS_ITEMS);
  assert!(t.graphics_image(1).is_some(), "the image is still drawn");
}
