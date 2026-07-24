use crate::term::Terminal;

/// Standard base64 (no line breaks) — the graphics payload encoding. Small
/// helper so the tests can build `_G…;<payload>` sequences inline.
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

#[test]
fn transmit_and_display_anchors_an_image() {
    let mut t = term();
    let rgba = vec![255u8; 2 * 2 * 4]; // 2x2 white
    t.feed(format!("\x1b_Gf=32,s=2,v=2,a=T;{}\x1b\\", b64(&rgba)).as_bytes());
    let images = t.images();
    assert_eq!(images.len(), 1);
    assert_eq!((images[0].image.width, images[0].image.height), (2, 2));
    assert_eq!(images[0].line, 0);
}

#[test]
fn apc_does_not_disturb_surrounding_text_or_csi() {
    let mut t = term();
    // A red SGR (a CSI, not an APC), "hi", a transmit-only graphics APC, then
    // "!" — the ESC-not-underscore must not be mistaken for an APC start.
    let seq = format!("\x1b[31mhi\x1b_Gf=32,s=1,v=1,a=t,i=1;{}\x1b\\!", b64(&[0; 4]));
    t.feed(seq.as_bytes());
    assert!(t.images().is_empty()); // a=t stores only, no placement
    assert!(t.row_text(0).starts_with("hi!"));
}

#[test]
fn chunked_transfer_reassembles() {
    let mut t = term();
    let rgba = vec![7u8; 2 * 2 * 4];
    let (a, b) = rgba.split_at(8);
    t.feed(format!("\x1b_Gf=32,s=2,v=2,a=T,m=1;{}\x1b\\", b64(a)).as_bytes());
    assert!(t.images().is_empty()); // still waiting for the final chunk
    t.feed(format!("\x1b_Gm=0;{}\x1b\\", b64(b)).as_bytes());
    assert_eq!(t.images().len(), 1);
    assert_eq!(t.images()[0].image.rgba, rgba);
}

#[test]
fn apc_split_across_feed_calls() {
    let mut t = term();
    let seq = format!("\x1b_Gf=32,s=1,v=1,a=T;{}\x1b\\", b64(&[255; 4]));
    // One byte per feed — the scanner must carry its state across calls.
    for byte in seq.as_bytes() {
        t.feed(&[*byte]);
    }
    assert_eq!(t.images().len(), 1);
}

#[test]
fn esc_held_across_feed_is_not_lost() {
    // A feed ending on a bare ESC must carry it into the next feed. Here that
    // ESC opens an SGR (not an APC), so the halves rejoin as `ESC[31m` and are
    // consumed — not printed literally as `[31m`.
    let mut t = term();
    t.feed(b"X\x1b");
    t.feed(b"[31mY");
    assert!(t.images().is_empty());
    assert!(t.row_text(0).starts_with("XY"));
}

#[test]
fn apc_body_split_after_leading_text() {
    // Leading text, then an APC whose body is cut by the feed boundary: the text
    // must print and the image appear, with no body bytes leaking to the grid.
    let mut t = term();
    let payload = b64(&[255u8; 4]); // 1x1 white
    t.feed(format!("AB\x1b_Gf=32,s=1,v=1,a=T;{}", &payload[..2]).as_bytes());
    assert!(t.images().is_empty()); // body not yet terminated
    t.feed(format!("{}\x1b\\", &payload[2..]).as_bytes());
    assert_eq!(t.images().len(), 1);
    assert!(t.row_text(0).starts_with("AB"));
}

#[test]
fn bel_terminated_apc() {
    let mut t = term();
    // Some emitters end the APC with BEL instead of ST.
    t.feed(format!("\x1b_Gf=32,s=1,v=1,a=T;{}\x07", b64(&[1; 4])).as_bytes());
    assert_eq!(t.images().len(), 1);
}

#[test]
fn transmit_sends_ok_response_honoring_quiet() {
    let mut t = term();
    // Default quiet 0 → OK response echoing the image id.
    t.feed(format!("\x1b_Gf=32,s=1,v=1,a=t,i=5;{}\x1b\\", b64(&[0; 4])).as_bytes());
    assert_eq!(t.take_output(), b"\x1b_Gi=5;OK\x1b\\");
    // q=2 → suppress all responses.
    t.feed(format!("\x1b_Gf=32,s=1,v=1,a=t,i=6,q=2;{}\x1b\\", b64(&[0; 4])).as_bytes());
    assert!(t.take_output().is_empty());
}

#[test]
fn decode_error_reports_error_code() {
    let mut t = term();
    // Declares 2x2 (16 bytes) but sends 4 → size error, reported since q=0.
    t.feed(format!("\x1b_Gf=32,s=2,v=2,a=t,i=3;{}\x1b\\", b64(&[0; 4])).as_bytes());
    let out = t.take_output();
    assert!(out.starts_with(b"\x1b_Gi=3;E"), "got {out:?}");
    assert!(t.images().is_empty());
}

#[test]
fn delete_clears_placements() {
    let mut t = term();
    t.feed(format!("\x1b_Gf=32,s=1,v=1,a=T,i=1;{}\x1b\\", b64(&[255; 4])).as_bytes());
    assert_eq!(t.images().len(), 1);
    t.feed(b"\x1b_Ga=d,d=a\x1b\\");
    assert!(t.images().is_empty());
}

#[test]
fn store_then_display_by_id() {
    let mut t = term();
    // Transmit-and-store under id 9, then display it by reference.
    t.feed(format!("\x1b_Gf=32,s=2,v=1,a=t,i=9;{}\x1b\\", b64(&[1; 8])).as_bytes());
    assert!(t.images().is_empty());
    t.feed(b"\x1b_Ga=p,i=9\x1b\\");
    assert_eq!(t.images().len(), 1);
    assert_eq!(t.images()[0].kitty_id, Some(9));
}

#[test]
fn display_unknown_id_errors() {
    let mut t = term();
    t.feed(b"\x1b_Ga=p,i=404\x1b\\");
    assert!(t.images().is_empty());
    assert_eq!(t.take_output(), b"\x1b_Gi=404;ENOENT\x1b\\");
}

/// An APC block is captured here and never handed to vte, so a run forwarded to
/// vte must not end on a dangling ESC — vte would park mid-escape and eat the
/// first byte *after* the block as that escape's final byte. `ESC ESC _…ST H`
/// used to print "G" and silently lose the "H" to an `ESC H` (HTS).
#[test]
fn dangling_esc_before_apc_does_not_eat_the_next_byte() {
    let mut t = Terminal::new(20, 3, 0);
    t.feed(b"G\x1b\x1b_Xz\x1b\\H");
    assert_eq!(t.row_text(0).trim_end(), "GH");
}

/// Same shape with a real graphics APC rather than an ignored one.
#[test]
fn dangling_esc_before_graphics_apc_does_not_eat_the_next_byte() {
    let mut t = Terminal::new(20, 3, 0);
    t.feed(b"S\x1b\x1b_Gf=100,a=q;AAAA\x1b\\T");
    assert_eq!(t.row_text(0).trim_end(), "ST");
}

/// Any number of superseded ESCs must be dropped, not just one.
#[test]
fn several_dangling_escs_before_apc_are_all_dropped() {
    let mut t = Terminal::new(20, 3, 0);
    t.feed(b"A\x1b\x1b\x1b\x1b_Xz\x1b\\B");
    assert_eq!(t.row_text(0).trim_end(), "AB");
}

/// The same hazard across a feed boundary: an ESC held from the previous read,
/// resolved against a following ESC that turns out to start the APC.
#[test]
fn dangling_esc_held_across_feeds_does_not_eat_the_next_byte() {
    let mut t = Terminal::new(20, 3, 0);
    t.feed(b"C\x1b"); // read ends on a bare ESC
    t.feed(b"\x1b_Xz\x1b\\D");
    assert_eq!(t.row_text(0).trim_end(), "CD");
}

/// A *complete* escape before an APC must still reach vte — the trim may only
/// drop ESCs, never a finished sequence.
#[test]
fn completed_escape_before_apc_is_still_applied() {
    let mut t = Terminal::new(20, 3, 0);
    // ESC [ 7 m sets reverse video; it must survive and apply to the "L".
    t.feed(b"K\x1b[7m\x1b_Xz\x1b\\L");
    assert_eq!(t.row_text(0).trim_end(), "KL");
    assert!(t.cell(0, 1).flags.contains(crate::cell::CellFlags::INVERSE));
}

/// A held ESC that introduces something ordinary is still forwarded intact.
#[test]
fn held_esc_introducing_a_normal_sequence_still_works() {
    let mut t = Terminal::new(20, 3, 0);
    t.feed(b"M\x1b");
    t.feed(b"[7mN");
    assert_eq!(t.row_text(0).trim_end(), "MN");
    assert!(t.cell(0, 1).flags.contains(crate::cell::CellFlags::INVERSE));
}
