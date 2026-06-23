//! The 256-entry terminal palette built from a scheme.

use crate::rgb::Rgb;
use crate::scheme::Scheme;

/// Component values of the standard xterm 6x6x6 color cube.
pub const CUBE_STEPS: [u8; 6] = [0, 95, 135, 175, 215, 255];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    pub colors: [Rgb; 256],
}

impl Palette {
    pub fn get(&self, index: u8) -> Rgb {
        self.colors[index as usize]
    }

    pub fn set(&mut self, index: u8, color: Rgb) {
        self.colors[index as usize] = color;
    }
}

/// Standard xterm color for indices 16..=255 (cube + grayscale ramp).
/// Indices 0..=15 fall back to the cube formula's nearest definition
/// only via [`build`]; this function is defined for 16..=255 and
/// returns black for 0..=15.
fn xterm_extended(index: u8) -> Rgb {
    if index >= 232 {
        let v = 8 + 10 * (index - 232);
        Rgb::new(v, v, v)
    } else if index >= 16 {
        let i = index - 16;
        Rgb::new(
            CUBE_STEPS[(i / 36) as usize],
            CUBE_STEPS[((i / 6) % 6) as usize],
            CUBE_STEPS[(i % 6) as usize],
        )
    } else {
        Rgb::new(0, 0, 0)
    }
}

/// Build a palette: slots 0..=15 from the scheme's ANSI colors,
/// 16..=231 the 6x6x6 cube, 232..=255 the grayscale ramp, then
/// per-index `overrides` applied on top.
pub fn build(scheme: &Scheme, overrides: &[(u8, Rgb)]) -> Palette {
    let mut colors = [Rgb::new(0, 0, 0); 256];
    colors[..16].copy_from_slice(&scheme.ansi);
    for i in 16..=255u8 {
        colors[i as usize] = xterm_extended(i);
    }
    for &(index, color) in overrides {
        colors[index as usize] = color;
    }
    Palette { colors }
}

impl Palette {
    pub fn from_scheme(scheme: &Scheme) -> Palette {
        build(scheme, &[])
    }
}

#[cfg(test)]
#[path = "../tests/palette.rs"]
mod tests;
