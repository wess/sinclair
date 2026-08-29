use super::*;

fn ctrl(s: &str) -> Control {
  parse_control(s.as_bytes())
}

#[test]
fn parses_control_keys_and_defaults() {
  let c = ctrl("a=T,f=24,s=3,v=2,i=7,o=z,m=1,q=2,C=1");
  assert_eq!(c.action, Action::TransmitAndDisplay);
  assert_eq!(c.format, 24);
  assert_eq!((c.width, c.height), (3, 2));
  assert_eq!(c.image_id, 7);
  assert!(c.compressed);
  assert!(c.more);
  assert_eq!(c.quiet, 2);
  assert!(!c.cursor_move); // C=1 suppresses the cursor move

  // Unspecified keys fall back to the spec defaults.
  let d = ctrl("s=1,v=1");
  assert_eq!(d.action, Action::Transmit);
  assert_eq!(d.format, 32);
  assert_eq!(d.medium, b'd');
  assert!(d.cursor_move);
  assert_eq!(d.delete, b'a');
  assert_eq!(d.z, 0);
}

#[test]
fn parses_every_action() {
  for (spelling, action) in [
    ("a=t", Action::Transmit),
    ("a=T", Action::TransmitAndDisplay),
    ("a=p", Action::Display),
    ("a=d", Action::Delete),
    ("a=q", Action::Query),
    ("a=f", Action::Frame),
    ("a=a", Action::Animate),
    ("a=c", Action::Compose),
  ] {
    assert_eq!(ctrl(spelling).action, action, "{spelling}");
  }
}

#[test]
fn parses_placement_geometry() {
  let c = ctrl("a=p,i=3,p=9,x=10,y=20,w=30,h=40,X=5,Y=6,c=7,r=8,z=-9,U=1,P=1,Q=2,H=-3,V=4");
  assert_eq!(c.image_id, 3);
  assert_eq!(c.placement_id, 9);
  assert_eq!((c.src_x, c.src_y, c.src_w, c.src_h), (10, 20, 30, 40));
  assert_eq!((c.cell_x, c.cell_y), (5, 6));
  assert_eq!((c.cols, c.rows), (7, 8));
  assert_eq!(c.z, -9);
  assert!(c.unicode);
  assert_eq!((c.parent_id, c.parent_placement), (1, 2));
  assert_eq!((c.h_offset, c.v_offset), (-3, 4));
}

#[test]
fn parses_transmission_medium_and_span() {
  let c = ctrl("t=s,S=1024,O=64,I=42");
  assert_eq!(c.medium, b's');
  assert_eq!(c.size, 1024);
  assert_eq!(c.offset, 64);
  assert_eq!(c.image_number, 42);
}

#[test]
fn signed_fields_saturate_instead_of_wrapping() {
  // A z-index past i32 must still sort to the far end, not wrap to the other.
  assert_eq!(ctrl("z=99999999999").z, i32::MAX);
  assert_eq!(ctrl("z=-99999999999").z, i32::MIN);
  assert_eq!(ctrl("z=nonsense").z, 0);
}

#[test]
fn unknown_keys_are_ignored() {
  // Forward compatibility: a key from a newer spec must not derail the rest.
  let c = ctrl("a=T,zz=5,i=4,\u{fffd}=1,s=2,v=2");
  assert_eq!(c.action, Action::TransmitAndDisplay);
  assert_eq!(c.image_id, 4);
  assert_eq!((c.width, c.height), (2, 2));
}

#[test]
fn overloaded_keys_read_back_per_action() {
  // `r`/`c`/`z`/`X`/`Y` mean one thing for a display and another for a frame.
  let f = ctrl("a=f,i=1,r=3,c=2,z=120,X=1,Y=4278190335");
  assert_eq!(f.frame(), 3);
  assert_eq!(f.base_frame(), 2);
  assert_eq!(f.gap(), 120);
  assert!(f.compose_overwrite());
  assert_eq!(f.frame_background(), [255, 0, 0, 255]);

  // `a=c` spells the composition mode with `C` instead.
  let c = ctrl("a=c,i=1,r=2,c=1,X=8,Y=9,C=1");
  assert_eq!(c.dest_origin(), (8, 9));
  assert!(c.compose_overwrite());
  assert!(!ctrl("a=c,i=1,r=2,c=1").compose_overwrite());

  let a = ctrl("a=a,i=1,s=3,v=5");
  assert_eq!(a.anim_state(), 3);
  assert_eq!(a.anim_loops(), 5);
}

#[test]
fn names_image_tracks_both_spellings() {
  assert!(ctrl("i=1").names_image());
  assert!(ctrl("I=1").names_image());
  assert!(!ctrl("a=T,s=1,v=1").names_image());
}
