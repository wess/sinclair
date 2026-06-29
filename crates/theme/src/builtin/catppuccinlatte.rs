//! Catppuccin Latte - official catppuccin palette (terminal mapping).

use crate::rgb::Rgb;
use crate::scheme::Scheme;

const fn c(r: u8, g: u8, b: u8) -> Rgb {
    Rgb::new(r, g, b)
}

pub static CATPPUCCIN_LATTE: Scheme = Scheme {
    name: "Catppuccin Latte",
    ansi: [
        c(0x5c, 0x5f, 0x77), // 0 black (subtext1)
        c(0xd2, 0x0f, 0x39), // 1 red
        c(0x40, 0xa0, 0x2b), // 2 green
        c(0xdf, 0x8e, 0x1d), // 3 yellow
        c(0x1e, 0x66, 0xf5), // 4 blue
        c(0xea, 0x76, 0xcb), // 5 magenta (pink)
        c(0x17, 0x92, 0x99), // 6 cyan (teal)
        c(0xac, 0xb0, 0xbe), // 7 white (surface2)
        c(0x6c, 0x6f, 0x85), // 8 bright black (subtext0)
        c(0xd2, 0x0f, 0x39), // 9 bright red
        c(0x40, 0xa0, 0x2b), // 10 bright green
        c(0xdf, 0x8e, 0x1d), // 11 bright yellow
        c(0x1e, 0x66, 0xf5), // 12 bright blue
        c(0xea, 0x76, 0xcb), // 13 bright magenta
        c(0x17, 0x92, 0x99), // 14 bright cyan
        c(0xbc, 0xc0, 0xcc), // 15 bright white (surface1)
    ],
    foreground: c(0x4c, 0x4f, 0x69),
    background: c(0xef, 0xf1, 0xf5),
    cursor: c(0xdc, 0x8a, 0x78),      // rosewater
    cursor_text: c(0xef, 0xf1, 0xf5), // base
    selection_foreground: c(0xef, 0xf1, 0xf5),
    selection_background: c(0xdc, 0x8a, 0x78), // rosewater
};
