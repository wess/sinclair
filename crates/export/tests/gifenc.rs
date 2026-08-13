use std::io::Cursor;

use super::*;
use crate::colors::Colors;
use crate::options::Options;
use crate::raster::{Raster, Rgba};
use crate::replay::Renderer;

/// A raster whose single cell already overflows a GIF's u16 width.
struct HugeCells;

impl Raster for HugeCells {
    fn cell_size(&self) -> (usize, usize) {
        (70_000, 10)
    }

    fn frame(&mut self, _term: &vt::Terminal, _colors: &Colors, _out: &mut Rgba) {}
}

#[test]
fn oversized_frames_error_instead_of_wrapping() {
    let cast = cast::parse(Cursor::new(
        "{\"version\":2,\"width\":1,\"height\":1}\n[0.0, \"o\", \"x\"]\n".to_owned(),
    ))
    .unwrap();
    let opts = Options {
        cols: Some(1),
        rows: Some(1),
        ..Options::default()
    };
    let mut renderer = Renderer::new(&cast, &opts, HugeCells);
    let out = std::env::temp_dir().join("sinclairgiftoolarge.gif");
    let _ = std::fs::remove_file(&out);

    let e = encode(&mut renderer, &out).unwrap_err();
    assert!(matches!(e, Error::TooLarge(70_000, 10)), "{e}");
    assert!(!out.exists());
}
