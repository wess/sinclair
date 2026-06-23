//! Applies a single `key = value` pair to [`Options`].

use crate::options::{ClipboardAccess, CursorStyle, FontStyle, OptionAsAlt, Options};
use crate::value;

/// Apply one `key = value` pair to the options. An empty value resets the
/// key to its default. Returns an error message for unknown keys or
/// unparseable values.
pub fn apply(opts: &mut Options, key: &str, val: &str) -> Result<(), String> {
    let d = Options::default();
    let empty = val.is_empty();
    match key {
        "font-family" => {
            // Empty resets the chain; otherwise each entry appends a
            // fallback (the first becomes the primary font).
            if empty {
                opts.font_family = d.font_family;
            } else {
                opts.font_family.push(val.to_string());
            }
        }
        "font-size" => {
            opts.font_size = if empty {
                d.font_size
            } else {
                value::parse_f32(val).ok_or_else(|| bad("number", val))?
            };
        }
        "font-style" => {
            opts.font_style = if empty {
                d.font_style
            } else {
                FontStyle::parse(val).ok_or_else(|| bad("normal|bold|italic|bold-italic", val))?
            };
        }
        "font-feature" => {
            if empty {
                opts.font_feature = d.font_feature;
            } else {
                let feature = value::parse_fontfeature(val)
                    .ok_or_else(|| bad("feature tag like `-liga` or `+ss01`", val))?;
                opts.font_feature.push(feature);
            }
        }
        "adjust-cell-width" => {
            opts.adjust_cell_width = if empty {
                d.adjust_cell_width
            } else {
                value::parse_adjust(val).ok_or_else(|| bad("integer pixels", val))?
            };
        }
        "adjust-cell-height" => {
            opts.adjust_cell_height = if empty {
                d.adjust_cell_height
            } else {
                value::parse_adjust(val).ok_or_else(|| bad("integer pixels", val))?
            };
        }
        "theme" => {
            opts.theme = if empty { d.theme } else { val.to_string() };
        }
        "background" => {
            opts.background = if empty {
                d.background
            } else {
                Some(val.to_string())
            };
        }
        "foreground" => {
            opts.foreground = if empty {
                d.foreground
            } else {
                Some(val.to_string())
            };
        }
        "cursor-style" => {
            opts.cursor_style = if empty {
                d.cursor_style
            } else {
                CursorStyle::parse(val).ok_or_else(|| bad("block|bar|underline", val))?
            };
        }
        "cursor-style-blink" => {
            opts.cursor_style_blink = if empty {
                d.cursor_style_blink
            } else {
                value::parse_bool(val).ok_or_else(|| bad("boolean", val))?
            };
        }
        "cursor-color" => {
            opts.cursor_color = if empty {
                d.cursor_color
            } else {
                Some(color(val)?)
            };
        }
        "cursor-text" => {
            opts.cursor_text = if empty {
                d.cursor_text
            } else {
                Some(color(val)?)
            };
        }
        "selection-foreground" => {
            opts.selection_foreground = if empty {
                d.selection_foreground
            } else {
                Some(color(val)?)
            };
        }
        "selection-background" => {
            opts.selection_background = if empty {
                d.selection_background
            } else {
                Some(color(val)?)
            };
        }
        "bold-is-bright" => {
            opts.bold_is_bright = if empty {
                d.bold_is_bright
            } else {
                value::parse_bool(val).ok_or_else(|| bad("boolean", val))?
            };
        }
        "minimum-contrast" => {
            opts.minimum_contrast = if empty {
                d.minimum_contrast
            } else {
                value::parse_f32_range(val, 1.0, 21.0).ok_or_else(|| bad("number in 1..21", val))?
            };
        }
        "unfocused-split-opacity" => {
            opts.unfocused_split_opacity = if empty {
                d.unfocused_split_opacity
            } else {
                value::parse_f32_range(val, 0.15, 1.0)
                    .ok_or_else(|| bad("number in 0.15..1", val))?
            };
        }
        "split-divider-color" => {
            opts.split_divider_color = if empty {
                d.split_divider_color
            } else {
                Some(color(val)?)
            };
        }
        "mouse-scroll-multiplier" => {
            opts.mouse_scroll_multiplier = if empty {
                d.mouse_scroll_multiplier
            } else {
                value::parse_f32_range(val, 0.01, 10_000.0)
                    .ok_or_else(|| bad("number in 0.01..10000", val))?
            };
        }
        "macos-option-as-alt" => {
            opts.macos_option_as_alt = if empty {
                d.macos_option_as_alt
            } else {
                OptionAsAlt::parse(val).ok_or_else(|| bad("false|true|left|right", val))?
            };
        }
        "window-inherit-working-directory" => {
            opts.window_inherit_working_directory = if empty {
                d.window_inherit_working_directory
            } else {
                value::parse_bool(val).ok_or_else(|| bad("boolean", val))?
            };
        }
        "quit-after-last-window-closed" => {
            opts.quit_after_last_window_closed = if empty {
                d.quit_after_last_window_closed
            } else {
                value::parse_bool(val).ok_or_else(|| bad("boolean", val))?
            };
        }
        "title" => {
            opts.title = if empty {
                d.title
            } else {
                Some(val.to_string())
            };
        }
        "clipboard-read" => {
            opts.clipboard_read = if empty {
                d.clipboard_read
            } else {
                ClipboardAccess::parse(val).ok_or_else(|| bad("allow|ask|deny", val))?
            };
        }
        "clipboard-write" => {
            opts.clipboard_write = if empty {
                d.clipboard_write
            } else {
                ClipboardAccess::parse(val).ok_or_else(|| bad("allow|ask|deny", val))?
            };
        }
        "scrollback-limit" => {
            opts.scrollback_limit = if empty {
                d.scrollback_limit
            } else {
                value::parse_usize(val).ok_or_else(|| bad("non-negative integer", val))?
            };
        }
        "window-padding-x" => {
            opts.window_padding_x = if empty {
                d.window_padding_x
            } else {
                value::parse_u32(val).ok_or_else(|| bad("non-negative integer", val))?
            };
        }
        "window-padding-y" => {
            opts.window_padding_y = if empty {
                d.window_padding_y
            } else {
                value::parse_u32(val).ok_or_else(|| bad("non-negative integer", val))?
            };
        }
        "window-width" => {
            opts.window_width = if empty {
                d.window_width
            } else {
                value::parse_u32(val).ok_or_else(|| bad("non-negative integer", val))?
            };
        }
        "window-height" => {
            opts.window_height = if empty {
                d.window_height
            } else {
                value::parse_u32(val).ok_or_else(|| bad("non-negative integer", val))?
            };
        }
        "command" => {
            opts.shell = if empty {
                d.shell
            } else {
                Some(val.to_string())
            };
        }
        "working-directory" => {
            opts.working_directory = if empty {
                d.working_directory
            } else {
                Some(val.to_string())
            };
        }
        "copy-on-select" => {
            opts.copy_on_select = if empty {
                d.copy_on_select
            } else {
                value::parse_bool(val).ok_or_else(|| bad("boolean", val))?
            };
        }
        "confirm-close-surface" => {
            opts.confirm_close_surface = if empty {
                d.confirm_close_surface
            } else {
                value::parse_bool(val).ok_or_else(|| bad("boolean", val))?
            };
        }
        "mouse-hide-while-typing" => {
            opts.mouse_hide_while_typing = if empty {
                d.mouse_hide_while_typing
            } else {
                value::parse_bool(val).ok_or_else(|| bad("boolean", val))?
            };
        }
        "palette" => {
            if empty {
                opts.palette = d.palette;
            } else {
                let entry = value::parse_palette(val).ok_or_else(|| bad("N=#rrggbb", val))?;
                opts.palette.push(entry);
            }
        }
        "plugin" => {
            if empty {
                opts.plugin = d.plugin;
            } else {
                opts.plugin.push(val.to_string());
            }
        }
        "keybind" => {
            if empty {
                opts.keybind = d.keybind;
            } else {
                opts.keybind.push(val.to_string());
            }
        }
        _ => return Err(format!("unknown key `{key}`")),
    }
    Ok(())
}

fn color(val: &str) -> Result<String, String> {
    value::parse_color(val).ok_or_else(|| bad("hex color `#rrggbb`", val))
}

fn bad(expected: &str, got: &str) -> String {
    format!("invalid value `{got}`, expected {expected}")
}

#[cfg(test)]
#[path = "../tests/apply.rs"]
mod tests;
