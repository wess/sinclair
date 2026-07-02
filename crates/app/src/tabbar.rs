//! Small color helper shared by the titlebar and sidebar chrome. (The
//! window-level tab strip is gone: tabs now live per-pane inside the
//! `guise::PaneGroup`.)

use theme::Rgb;

/// Linear mix of two colors: `t` 0 is `a`, 1 is `b`. Clamped.
pub fn blend(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Rgb::new(mix(a.r, b.r), mix(a.g, b.g), mix(a.b, b.b))
}

#[cfg(test)]
#[path = "../tests/tabbar.rs"]
mod tests;
