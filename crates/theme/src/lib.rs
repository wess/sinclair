//! Color schemes and palettes for the Sinclair terminal emulator.

pub mod builtin;
pub mod overrides;
pub mod palette;
pub mod rgb;
pub mod scheme;

pub use builtin::{builtin, default_scheme, names};
pub use overrides::{apply_overrides, OverrideError};
pub use palette::{build, Palette, CUBE_STEPS};
pub use rgb::{ParseRgbError, Rgb};
pub use scheme::Scheme;
