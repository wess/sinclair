//! Builtin color schemes and lookup by name.

mod ayudark;
mod catppuccinlatte;
mod catppuccinmocha;
mod dark;
mod dracula;
mod everforest;
mod githubdark;
mod githublight;
mod gruvboxdark;
mod gruvboxlight;
mod kanagawa;
mod light;
mod materialdark;
mod monokai;
mod nord;
mod onedark;
mod palenight;
mod rosepine;
mod solarizeddark;
mod solarizedlight;
mod tokyonight;
mod zenburn;

pub use ayudark::AYU_DARK;
pub use catppuccinlatte::CATPPUCCIN_LATTE;
pub use catppuccinmocha::CATPPUCCIN_MOCHA;
pub use dark::DARK;
pub use dracula::DRACULA;
pub use everforest::EVERFOREST;
pub use githubdark::GITHUB_DARK;
pub use githublight::GITHUB_LIGHT;
pub use gruvboxdark::GRUVBOX_DARK;
pub use gruvboxlight::GRUVBOX_LIGHT;
pub use kanagawa::KANAGAWA;
pub use light::LIGHT;
pub use materialdark::MATERIAL_DARK;
pub use monokai::MONOKAI;
pub use nord::NORD;
pub use onedark::ONE_DARK;
pub use palenight::PALENIGHT;
pub use rosepine::ROSE_PINE;
pub use solarizeddark::SOLARIZED_DARK;
pub use solarizedlight::SOLARIZED_LIGHT;
pub use tokyonight::TOKYO_NIGHT;
pub use zenburn::ZENBURN;

use crate::scheme::Scheme;

/// Every builtin scheme.
pub static ALL: &[&Scheme] = &[
    &DARK,
    &LIGHT,
    &AYU_DARK,
    &CATPPUCCIN_LATTE,
    &CATPPUCCIN_MOCHA,
    &DRACULA,
    &EVERFOREST,
    &GITHUB_DARK,
    &GITHUB_LIGHT,
    &GRUVBOX_DARK,
    &GRUVBOX_LIGHT,
    &KANAGAWA,
    &MATERIAL_DARK,
    &MONOKAI,
    &NORD,
    &ONE_DARK,
    &PALENIGHT,
    &ROSE_PINE,
    &SOLARIZED_DARK,
    &SOLARIZED_LIGHT,
    &TOKYO_NIGHT,
    &ZENBURN,
];

/// Lowercase a name and drop spaces, dashes, and underscores so
/// "Catppuccin Mocha", "catppuccin-mocha", and "catppuccin_mocha"
/// all normalize identically.
fn normalize(name: &str) -> String {
    name.chars()
        .filter(|ch| !matches!(ch, ' ' | '-' | '_'))
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

/// Look up a builtin scheme by name, case-insensitively and ignoring
/// spaces, dashes, and underscores.
pub fn builtin(name: &str) -> Option<&'static Scheme> {
    let want = normalize(name);
    ALL.iter().copied().find(|s| normalize(s.name) == want)
}

/// Sorted display names of every builtin scheme, for diagnostics.
pub fn names() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = ALL.iter().map(|s| s.name).collect();
    out.sort_unstable();
    out
}

/// The scheme used when nothing is configured.
pub fn default_scheme() -> &'static Scheme {
    &DARK
}

#[cfg(test)]
#[path = "../../tests/builtin/mod.rs"]
mod tests;
