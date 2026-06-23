use super::*;

#[test]
fn default_is_80x24() {
    let ws = Winsize::default();
    assert_eq!(ws.cols, 80);
    assert_eq!(ws.rows, 24);
    assert_eq!(ws.cell_width, 0);
    assert_eq!(ws.cell_height, 0);
}

#[test]
fn converts_grid_to_termios() {
    let ws = Winsize::new(120, 40).to_termios();
    assert_eq!(ws.ws_col, 120);
    assert_eq!(ws.ws_row, 40);
    assert_eq!(ws.ws_xpixel, 0);
    assert_eq!(ws.ws_ypixel, 0);
}

#[test]
fn converts_pixels_as_total_window_size() {
    let ws = Winsize::with_cell_size(100, 50, 8, 16).to_termios();
    assert_eq!(ws.ws_xpixel, 800);
    assert_eq!(ws.ws_ypixel, 800);
}

#[test]
fn pixel_conversion_saturates() {
    let ws = Winsize::with_cell_size(u16::MAX, u16::MAX, u16::MAX, u16::MAX).to_termios();
    assert_eq!(ws.ws_xpixel, u16::MAX);
    assert_eq!(ws.ws_ypixel, u16::MAX);
}
