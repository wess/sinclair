//! Kitty unicode placeholders: images positioned by the text itself.
//!
//! A *virtual* placement (`U=1`) has no anchor of its own. Instead the client
//! writes cells holding U+10EEEE, and each such cell says which image it
//! belongs to and which of its rows and columns it stands for. That makes an
//! image scroll, reflow, and clip exactly like text, and lets a multiplexer
//! pass images through without understanding them.
//!
//! One cell carries:
//!
//! - the **image id** in its foreground color — the low 24 bits directly, and
//!   a third diacritic supplying the high byte when the id needs one;
//! - the **placement id** in its underline color, when it names one;
//! - the **row** and **column** within the image, as the first and second
//!   combining diacritics from the table below.
//!
//! Both diacritics may be omitted, in which case the cell continues the run to
//! its left — the common case, and why a decoded cell keeps "unset" distinct
//! from "zero".

/// The placeholder character. A cell holding this draws part of an image.
pub const PLACEHOLDER: char = '\u{10EEEE}';

/// The diacritics that spell a row or column number, in value order: the
/// character at index `n` means `n`. This is kitty's `rowcolumn-diacritics`
/// table, derived from the Unicode 6.0.0 combining-class-230 marks, and it is
/// sorted by codepoint — so the value of a diacritic is a binary search.
static DIACRITICS: [u32; 297] = [
  0x00305, 0x0030D, 0x0030E, 0x00310, 0x00312, 0x0033D, 0x0033E, 0x0033F,
  0x00346, 0x0034A, 0x0034B, 0x0034C, 0x00350, 0x00351, 0x00352, 0x00357,
  0x0035B, 0x00363, 0x00364, 0x00365, 0x00366, 0x00367, 0x00368, 0x00369,
  0x0036A, 0x0036B, 0x0036C, 0x0036D, 0x0036E, 0x0036F, 0x00483, 0x00484,
  0x00485, 0x00486, 0x00487, 0x00592, 0x00593, 0x00594, 0x00595, 0x00597,
  0x00598, 0x00599, 0x0059C, 0x0059D, 0x0059E, 0x0059F, 0x005A0, 0x005A1,
  0x005A8, 0x005A9, 0x005AB, 0x005AC, 0x005AF, 0x005C4, 0x00610, 0x00611,
  0x00612, 0x00613, 0x00614, 0x00615, 0x00616, 0x00617, 0x00657, 0x00658,
  0x00659, 0x0065A, 0x0065B, 0x0065D, 0x0065E, 0x006D6, 0x006D7, 0x006D8,
  0x006D9, 0x006DA, 0x006DB, 0x006DC, 0x006DF, 0x006E0, 0x006E1, 0x006E2,
  0x006E4, 0x006E7, 0x006E8, 0x006EB, 0x006EC, 0x00730, 0x00732, 0x00733,
  0x00735, 0x00736, 0x0073A, 0x0073D, 0x0073F, 0x00740, 0x00741, 0x00743,
  0x00745, 0x00747, 0x00749, 0x0074A, 0x007EB, 0x007EC, 0x007ED, 0x007EE,
  0x007EF, 0x007F0, 0x007F1, 0x007F3, 0x00816, 0x00817, 0x00818, 0x00819,
  0x0081B, 0x0081C, 0x0081D, 0x0081E, 0x0081F, 0x00820, 0x00821, 0x00822,
  0x00823, 0x00825, 0x00826, 0x00827, 0x00829, 0x0082A, 0x0082B, 0x0082C,
  0x0082D, 0x00951, 0x00953, 0x00954, 0x00F82, 0x00F83, 0x00F86, 0x00F87,
  0x0135D, 0x0135E, 0x0135F, 0x017DD, 0x0193A, 0x01A17, 0x01A75, 0x01A76,
  0x01A77, 0x01A78, 0x01A79, 0x01A7A, 0x01A7B, 0x01A7C, 0x01B6B, 0x01B6D,
  0x01B6E, 0x01B6F, 0x01B70, 0x01B71, 0x01B72, 0x01B73, 0x01CD0, 0x01CD1,
  0x01CD2, 0x01CDA, 0x01CDB, 0x01CE0, 0x01DC0, 0x01DC1, 0x01DC3, 0x01DC4,
  0x01DC5, 0x01DC6, 0x01DC7, 0x01DC8, 0x01DC9, 0x01DCB, 0x01DCC, 0x01DD1,
  0x01DD2, 0x01DD3, 0x01DD4, 0x01DD5, 0x01DD6, 0x01DD7, 0x01DD8, 0x01DD9,
  0x01DDA, 0x01DDB, 0x01DDC, 0x01DDD, 0x01DDE, 0x01DDF, 0x01DE0, 0x01DE1,
  0x01DE2, 0x01DE3, 0x01DE4, 0x01DE5, 0x01DE6, 0x01DFE, 0x020D0, 0x020D1,
  0x020D4, 0x020D5, 0x020D6, 0x020D7, 0x020DB, 0x020DC, 0x020E1, 0x020E7,
  0x020E9, 0x020F0, 0x02CEF, 0x02CF0, 0x02CF1, 0x02DE0, 0x02DE1, 0x02DE2,
  0x02DE3, 0x02DE4, 0x02DE5, 0x02DE6, 0x02DE7, 0x02DE8, 0x02DE9, 0x02DEA,
  0x02DEB, 0x02DEC, 0x02DED, 0x02DEE, 0x02DEF, 0x02DF0, 0x02DF1, 0x02DF2,
  0x02DF3, 0x02DF4, 0x02DF5, 0x02DF6, 0x02DF7, 0x02DF8, 0x02DF9, 0x02DFA,
  0x02DFB, 0x02DFC, 0x02DFD, 0x02DFE, 0x02DFF, 0x0A66F, 0x0A67C, 0x0A67D,
  0x0A6F0, 0x0A6F1, 0x0A8E0, 0x0A8E1, 0x0A8E2, 0x0A8E3, 0x0A8E4, 0x0A8E5,
  0x0A8E6, 0x0A8E7, 0x0A8E8, 0x0A8E9, 0x0A8EA, 0x0A8EB, 0x0A8EC, 0x0A8ED,
  0x0A8EE, 0x0A8EF, 0x0A8F0, 0x0A8F1, 0x0AAB0, 0x0AAB2, 0x0AAB3, 0x0AAB7,
  0x0AAB8, 0x0AABE, 0x0AABF, 0x0AAC1, 0x0FE20, 0x0FE21, 0x0FE22, 0x0FE23,
  0x0FE24, 0x0FE25, 0x0FE26, 0x10A0F, 0x10A38, 0x1D185, 0x1D186, 0x1D187,
  0x1D188, 0x1D189, 0x1D1AA, 0x1D1AB, 0x1D1AC, 0x1D1AD, 0x1D242, 0x1D243,
  0x1D244,];

/// The row or column number a diacritic stands for, or `None` if the
/// character is not one of the encoding diacritics.
pub fn diacritic_value(c: char) -> Option<u16> {
  DIACRITICS
    .binary_search(&(c as u32))
    .ok()
    .map(|index| index as u16)
}

/// The diacritic that spells `value`, for tests and for anything that needs
/// to emit placeholders rather than read them.
pub fn diacritic(value: u16) -> Option<char> {
  DIACRITICS
    .get(value as usize)
    .and_then(|&c| char::from_u32(c))
}

/// What a placeholder cell says about itself. Every field is optional: a cell
/// that omits one continues the run to its left.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Placeholder {
  /// Row within the image, from the first diacritic.
  pub row: Option<u16>,
  /// Column within the image, from the second diacritic.
  pub col: Option<u16>,
  /// The image id's high byte, from the third diacritic. The other three
  /// bytes come from the cell's foreground color.
  pub id_high: Option<u8>,
}

impl Placeholder {
  /// Fold in one more diacritic, filling row, then column, then the id's
  /// high byte. A fourth is ignored, as the spec defines only three.
  pub(crate) fn push(&mut self, c: char) {
    let Some(value) = diacritic_value(c) else {
      return;
    };
    if self.row.is_none() {
      self.row = Some(value);
    } else if self.col.is_none() {
      self.col = Some(value);
    } else if self.id_high.is_none() {
      self.id_high = Some(value as u8);
    }
  }

  /// The image id this cell names, given its foreground color's 24 bits.
  pub fn image_id(&self, fg_rgb: (u8, u8, u8)) -> u32 {
    let (r, g, b) = fg_rgb;
    let low = u32::from_be_bytes([0, r, g, b]);
    match self.id_high {
      Some(high) => low | (high as u32) << 24,
      None => low,
    }
  }
}

/// One horizontal run of placeholder cells resolved into something to draw:
/// a slice of an image, and the viewport cells it covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Draw {
  /// Viewport row and starting column of the run.
  pub row: usize,
  pub col: usize,
  /// How many cells wide the run is. It is always one cell tall — a taller
  /// image is a run per row.
  pub len: usize,
  /// The image the run draws, and the placement of it, when the cells named
  /// one through their underline color.
  pub image_id: u32,
  pub placement_id: u32,
  /// Which row of the image's cell grid this run is, and which column it
  /// starts at.
  pub image_row: u16,
  pub image_col: u16,
}

/// Resolve one grid row's placeholder cells into draw runs, appending to
/// `out`. `viewport_row` is the row's position on screen.
///
/// Cells may leave the row or column off, which means "continue the run to my
/// left" — that is the compact form a client emits for a whole image, so a
/// run only breaks when the image, the placement, or the coordinates jump.
pub fn scan_row(cells: &[crate::cell::Cell], viewport_row: usize, out: &mut Vec<Draw>) {
  let mut run: Option<Draw> = None;
  for (col, cell) in cells.iter().enumerate() {
    let Some(p) = cell.placeholder() else {
      out.extend(run.take());
      continue;
    };
    let image_id = p.image_id(color_rgb(cell.fg));
    let placement_id = match cell.underline_color {
      crate::color::Color::Default => 0,
      other => {
        let (r, g, b) = color_rgb(other);
        u32::from_be_bytes([0, r, g, b])
      }
    };
    // An omitted coordinate continues the run to the left; with nothing to
    // the left it starts at the image's own origin.
    let (image_row, image_col) = match &run {
      Some(prev) if prev.image_id == image_id && prev.placement_id == placement_id => (
        p.row.unwrap_or(prev.image_row),
        p.col.unwrap_or(prev.image_col + prev.len as u16),
      ),
      _ => (p.row.unwrap_or(0), p.col.unwrap_or(0)),
    };
    // Extend the current run when this cell is exactly its continuation.
    match &mut run {
      Some(r)
        if r.image_id == image_id
          && r.placement_id == placement_id
          && r.image_row == image_row
          && r.col + r.len == col
          && r.image_col + r.len as u16 == image_col =>
      {
        r.len += 1;
        continue;
      }
      _ => {}
    }
    out.extend(run.replace(Draw {
      row: viewport_row,
      col,
      len: 1,
      image_id,
      placement_id,
      image_row,
      image_col,
    }));
  }
  out.extend(run);
}

/// A cell color as 24-bit RGB. An indexed color is its own index, which is how
/// a client with only 256 colors names an image id.
fn color_rgb(color: crate::color::Color) -> (u8, u8, u8) {
  match color {
    crate::color::Color::Rgb(r, g, b) => (r, g, b),
    crate::color::Color::Indexed(i) => (0, 0, i),
    crate::color::Color::Default => (0, 0, 0),
  }
}

#[cfg(test)]
#[path = "../tests/placeholder.rs"]
mod tests;
