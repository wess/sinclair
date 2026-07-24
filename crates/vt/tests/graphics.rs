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
    assert!(!c.move_cursor); // C=1 suppresses the cursor move

    // Unspecified keys fall back to the spec defaults.
    let d = ctrl("s=1,v=1");
    assert_eq!(d.action, Action::Transmit);
    assert_eq!(d.format, 32);
    assert!(d.move_cursor);
    assert_eq!(d.delete, b'a');
}

#[test]
fn decodes_raw_rgba() {
    let c = ctrl("f=32,s=2,v=1");
    let raw = vec![1, 2, 3, 4, 5, 6, 7, 8]; // two RGBA pixels
    let img = decode(&c, &raw).unwrap();
    assert_eq!((img.width, img.height), (2, 1));
    assert_eq!(img.rgba, raw);
}

#[test]
fn decodes_raw_rgb_expanding_alpha() {
    let c = ctrl("f=24,s=1,v=1");
    let img = decode(&c, &[10, 20, 30]).unwrap();
    assert_eq!(img.rgba, vec![10, 20, 30, 255]);
}

#[test]
fn rejects_short_raw_payload() {
    let c = ctrl("f=32,s=2,v=2"); // needs 16 bytes
    assert!(decode(&c, &[0; 8]).is_err());
}

#[test]
fn rejects_absurd_dimensions() {
    let c = ctrl("f=32,s=100000,v=100000");
    assert!(decode(&c, &[0; 16]).is_err());
}

#[test]
fn inflates_zlib_payload() {
    use std::io::Write;
    let raw = vec![9u8, 8, 7, 6]; // one RGBA pixel
    let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(&raw).unwrap();
    let comp = enc.finish().unwrap();
    let c = ctrl("f=32,s=1,v=1,o=z");
    assert_eq!(decode(&c, &comp).unwrap().rgba, raw);
}

#[test]
fn rejects_decompression_bomb() {
    // A tiny zlib stream that inflates past the decode cap must be refused
    // before the whole expansion is allocated — not decoded into an OOM.
    use std::io::Write;
    let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    let chunk = vec![0u8; 1024 * 1024];
    let mut written: u64 = 0;
    while written <= MAX_DECODED_BYTES {
        enc.write_all(&chunk).unwrap();
        written += chunk.len() as u64;
    }
    let bomb = enc.finish().unwrap();
    assert!(bomb.len() < 1024 * 1024, "bomb should stay tiny compressed");
    let c = ctrl("f=32,s=1,v=1,o=z");
    assert!(decode(&c, &bomb).is_err());
}

#[test]
fn decodes_png_to_rgba() {
    // Encode a 1x1 opaque-blue RGBA PNG, then decode it back.
    let mut bytes = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut bytes, 1, 1);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().unwrap();
        w.write_image_data(&[0, 0, 255, 255]).unwrap();
    }
    let img = decode(&ctrl("f=100"), &bytes).unwrap();
    assert_eq!((img.width, img.height), (1, 1));
    assert_eq!(img.rgba, vec![0, 0, 255, 255]);
}

#[test]
fn decodes_rgb_png_expanding_alpha() {
    let mut bytes = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut bytes, 1, 1);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().unwrap();
        w.write_image_data(&[10, 20, 30]).unwrap();
    }
    let img = decode(&ctrl("f=100"), &bytes).unwrap();
    assert_eq!(img.rgba, vec![10, 20, 30, 255]);
}

#[test]
fn unsupported_medium_errors() {
    // File-based transmission is out of scope; decode refuses it.
    assert!(decode(&ctrl("f=32,s=1,v=1,t=f"), &[0; 4]).is_err());
}

#[test]
fn unsupported_format_errors() {
    assert!(decode(&ctrl("f=8,s=1,v=1"), &[0; 4]).is_err());
}
