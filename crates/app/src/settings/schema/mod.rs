//! The settings schema: one entry per option — its key, label, description,
//! category, and control — and the whole GUI renders from it. Adding a
//! setting is one entry in the right section file; current values come from
//! [`config::Options`], defaults from `Options::default()`, and writes go
//! through the settings.json editor. No UI lives here.

use config::Options;

mod ai;
mod appearance;
mod general;
mod lists;
mod terminal;

pub use ai::TOOL_KEYS;
pub use lists::ListKind;

/// Sidebar sections (the GUI's categories).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Section {
    General,
    Appearance,
    Terminal,
    Keyboard,
    Macros,
    Plugins,
    Ai,
}

impl Section {
    pub const ALL: [Section; 7] = [
        Section::General,
        Section::Appearance,
        Section::Terminal,
        Section::Keyboard,
        Section::Macros,
        Section::Plugins,
        Section::Ai,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Section::General => "General",
            Section::Appearance => "Appearance",
            Section::Terminal => "Terminal",
            Section::Keyboard => "Keyboard",
            Section::Macros => "Macros",
            Section::Plugins => "Plugins",
            Section::Ai => "AI",
        }
    }

    /// The monospace group label under the page title (Zed's subsection
    /// header style).
    pub fn subtitle(self) -> &'static str {
        match self {
            Section::General => "General Settings",
            Section::Appearance => "Appearance Settings",
            Section::Terminal => "Terminal Settings",
            Section::Keyboard => "Keymap",
            Section::Macros => "Macros",
            Section::Plugins => "Plugin Settings",
            Section::Ai => "AI Settings",
        }
    }
}

/// One settings entry.
pub struct Setting {
    /// The settings.json key.
    pub key: &'static str,
    pub label: &'static str,
    /// One short sentence shown under the label.
    pub desc: &'static str,
    pub section: Section,
    pub control: Control,
}

/// How a setting is rendered and read.
pub enum Control {
    /// A boolean switch.
    Toggle(fn(&Options) -> bool),
    /// A numeric slider.
    Slider(Slider),
    /// One value from a fixed (or theme-derived) list.
    Choice(Choice),
    /// A free-text field. Committing an empty value resets to default.
    Text {
        get: fn(&Options) -> String,
        placeholder: &'static str,
    },
    /// A repeated option, rendered as its own add/remove list group.
    List(ListKind),
}

/// A numeric option's range and formatting.
#[derive(Clone, Copy)]
pub struct Slider {
    pub get: fn(&Options) -> f32,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    /// Persist (and display) as a whole number.
    pub int: bool,
    /// Display zero as "auto" (unset window dimensions).
    pub auto_zero: bool,
}

impl Slider {
    /// The current value's position along the range, in `0..=1`.
    pub fn fraction(&self, o: &Options) -> f32 {
        if self.max <= self.min {
            0.0
        } else {
            (((self.get)(o) - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
        }
    }

    /// The value at slider fraction `frac`, snapped to the step and clamped.
    pub fn value_at_fraction(&self, frac: f32) -> f32 {
        let raw = self.min + frac.clamp(0.0, 1.0) * (self.max - self.min);
        ((raw / self.step).round() * self.step).clamp(self.min, self.max)
    }

    /// The current value formatted for display.
    pub fn display(&self, o: &Options) -> String {
        let v = (self.get)(o);
        if self.auto_zero && v == 0.0 {
            return "auto".to_string();
        }
        self.fmt(v)
    }

    /// Format a value for persistence.
    pub fn fmt(&self, v: f32) -> String {
        if self.int {
            format!("{}", v.round() as i64)
        } else {
            format!("{v}")
        }
    }
}

/// A pick-one option.
#[derive(Clone, Copy)]
pub struct Choice {
    /// The current value as displayed (`unset` when the key is unset).
    pub get: fn(&Options) -> String,
    pub variants: fn() -> Vec<String>,
    /// A leading pseudo-variant that removes the key (e.g. theme "default").
    pub unset: Option<&'static str>,
}

/// Every setting, in sidebar order. Section files each contribute a slice.
pub fn all() -> &'static [Setting] {
    static ALL: std::sync::OnceLock<Vec<Setting>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        let mut entries = general::settings();
        entries.extend(appearance::settings());
        entries.extend(terminal::settings());
        entries.extend(lists::keyboard_settings());
        entries.extend(lists::plugin_settings());
        entries.extend(ai::settings());
        entries
    })
}

/// Look a setting up by key.
pub fn find(key: &str) -> Option<&'static Setting> {
    all().iter().find(|s| s.key == key)
}

/// The settings of one section, in declaration order.
pub fn in_section(section: Section) -> impl Iterator<Item = &'static Setting> {
    all().iter().filter(move |s| s.section == section)
}

impl Setting {
    /// Case-insensitive search across key, label, and description.
    pub fn matches(&self, query: &str) -> bool {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return true;
        }
        q.split_whitespace().all(|w| {
            self.key.contains(w)
                || self.label.to_lowercase().contains(w)
                || self.desc.to_lowercase().contains(w)
        })
    }
}

// Terse constructors for the section tables.

pub(crate) fn toggle(
    key: &'static str,
    label: &'static str,
    desc: &'static str,
    section: Section,
    get: fn(&Options) -> bool,
) -> Setting {
    Setting { key, label, desc, section, control: Control::Toggle(get) }
}

pub(crate) fn slider(
    key: &'static str,
    label: &'static str,
    desc: &'static str,
    section: Section,
    get: fn(&Options) -> f32,
    range: (f32, f32, f32), // (min, max, step)
    int: bool,
) -> Setting {
    let (min, max, step) = range;
    Setting {
        key,
        label,
        desc,
        section,
        control: Control::Slider(Slider { get, min, max, step, int, auto_zero: false }),
    }
}

pub(crate) fn text(
    key: &'static str,
    label: &'static str,
    desc: &'static str,
    section: Section,
    get: fn(&Options) -> String,
    placeholder: &'static str,
) -> Setting {
    Setting { key, label, desc, section, control: Control::Text { get, placeholder } }
}

pub(crate) fn choice(
    key: &'static str,
    label: &'static str,
    desc: &'static str,
    section: Section,
    get: fn(&Options) -> String,
    variants: fn() -> Vec<String>,
    unset: Option<&'static str>,
) -> Setting {
    Setting { key, label, desc, section, control: Control::Choice(Choice { get, variants, unset }) }
}

pub(crate) fn list(kind: ListKind, desc: &'static str, section: Section) -> Setting {
    Setting { key: kind.key(), label: kind.label(), desc, section, control: Control::List(kind) }
}

/// Clone an `Option<String>` field for display.
pub(crate) fn opt(v: &Option<String>) -> String {
    v.clone().unwrap_or_default()
}

pub(crate) fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[cfg(test)]
#[path = "../../../tests/schema.rs"]
mod tests;
