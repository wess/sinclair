//! Catppuccin Mocha - official catppuccin palette (terminal mapping).

use crate::rgb::Rgb;
use crate::scheme::Scheme;

const fn c(r: u8, g: u8, b: u8) -> Rgb {
    Rgb::new(r, g, b)
}

pub static CATPPUCCIN_MOCHA: Scheme = Scheme {
    name: "Catppuccin Mocha",
    ansi: [
        c(0x45, 0x47, 0x5a), // 0 black (surface1)
        c(0xf3, 0x8b, 0xa8), // 1 red
        c(0xa6, 0xe3, 0xa1), // 2 green
        c(0xf9, 0xe2, 0xaf), // 3 yellow
        c(0x89, 0xb4, 0xfa), // 4 blue
        c(0xf5, 0xc2, 0xe7), // 5 magenta (pink)
        c(0x94, 0xe2, 0xd5), // 6 cyan (teal)
        c(0xba, 0xc2, 0xde), // 7 white (subtext1)
        c(0x58, 0x5b, 0x70), // 8 bright black (surface2)
        c(0xf3, 0x8b, 0xa8), // 9 bright red
        c(0xa6, 0xe3, 0xa1), // 10 bright green
        c(0xf9, 0xe2, 0xaf), // 11 bright yellow
        c(0x89, 0xb4, 0xfa), // 12 bright blue
        c(0xf5, 0xc2, 0xe7), // 13 bright magenta
        c(0x94, 0xe2, 0xd5), // 14 bright cyan
        c(0xa6, 0xad, 0xc8), // 15 bright white (subtext0)
    ],
    foreground: c(0xcd, 0xd6, 0xf4),
    background: c(0x1e, 0x1e, 0x2e),
    cursor: c(0xf5, 0xe0, 0xdc),      // rosewater
    cursor_text: c(0x1e, 0x1e, 0x2e), // base
    selection_foreground: c(0x1e, 0x1e, 0x2e),
    selection_background: c(0xf5, 0xe0, 0xdc), // rosewater
};
