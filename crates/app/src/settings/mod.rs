//! System Settings-style preferences window. Every option Sinclair reads from
//! its config file is shown and edited here; the window never asks the user
//! to go hand-edit the file. Writes go straight back to the config file and
//! the live-reload watcher applies them.

mod model;
mod ui;

use gpui::prelude::*;
use gpui::{
    bounds, point, px, size, App, Bounds, Context, FocusHandle, KeyDownEvent, Pixels,
    TitlebarOptions, Window, WindowBounds, WindowOptions,
};

use guise::TextEdit;
use model::{Bool, Choice, Field, ListKind, Num, Section};

const WIDTH: f32 = 725.0;
const HEIGHT: f32 = 810.0;

/// What the single active text editor is bound to.
#[derive(Clone, PartialEq)]
pub(crate) enum EditTarget {
    /// A scalar free-text option.
    Field(Field),
    /// An existing entry of a repeated option.
    Item(ListKind, usize),
    /// A new, not-yet-saved entry being typed for a repeated option.
    NewItem(ListKind),
    /// Renaming a saved macro; carries the old name.
    MacroName(String),
}

/// Result of probing a tool's reachability.
#[derive(Clone)]
enum ToolTest {
    Testing,
    Ok(String),
    Fail(String),
}

pub struct SettingsView {
    opts: config::Options,
    section: Section,
    editing: Option<(EditTarget, TextEdit)>,
    /// When true, the next key chord is captured as the edited keybind's
    /// trigger instead of being typed into the field.
    capturing: bool,
    /// Saved macros, for the Macros section.
    macros: Vec<macros::Macro>,
    /// When set, the next key chord is captured as the shortcut for this macro.
    capture_macro: Option<String>,
    focus: FocusHandle,
    relay_running: bool,
    tool_tests: std::collections::HashMap<&'static str, ToolTest>,
    /// Each slider track's window-space bounds, captured every frame so a
    /// mouse-down (which carries a position but not the element's bounds) can
    /// be mapped to a value. Keyed by the option's config key.
    slider_bounds: std::collections::HashMap<&'static str, Bounds<Pixels>>,
}

pub fn open(parent: &Window, cx: &mut App) {
    let center = parent.bounds().center();
    let bounds = bounds(
        center - point(px(WIDTH / 2.0), px(HEIGHT / 2.0)),
        size(px(WIDTH), px(HEIGHT)),
    );
    let _ = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            is_resizable: false,
            titlebar: Some(TitlebarOptions {
                title: Some("Settings".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(20.0), px(20.0))),
            }),
            ..Default::default()
        },
        |window, cx| {
            window.set_window_title("Settings");
            cx.new(SettingsView::new)
        },
    );
}

impl SettingsView {
    fn new(cx: &mut Context<Self>) -> Self {
        let mut view = Self {
            opts: config::Options::default(),
            section: Section::General,
            editing: None,
            capturing: false,
            macros: Vec::new(),
            capture_macro: None,
            focus: cx.focus_handle(),
            // Probing the relay blocks (file read + TCP connect); start
            // pessimistic and let the first off-thread poll fill it in.
            relay_running: false,
            tool_tests: std::collections::HashMap::new(),
            slider_bounds: std::collections::HashMap::new(),
        };
        view.reload();
        view.poll_relay_status(cx);
        view
    }

    /// Probe a tool off-thread and record the result for the row to show.
    fn test_tool(&mut self, tool: &'static str, cx: &mut Context<Self>) {
        self.tool_tests.insert(tool, ToolTest::Testing);
        cx.notify();
        let executor = cx.background_executor().clone();
        let path = tool_path(&self.opts, tool);
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move { crate::relay::test_tool(tool, path.as_deref()) })
                .await;
            let _ = this.update(cx, |view, cx| {
                let state = match result {
                    Ok(m) => ToolTest::Ok(m),
                    Err(e) => ToolTest::Fail(e),
                };
                view.tool_tests.insert(tool, state);
                cx.notify();
            });
        })
        .detach();
    }

    /// Re-probe the relay server on an interval so the status dot stays live.
    fn poll_relay_status(&self, cx: &mut Context<Self>) {
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| loop {
            let running = executor.spawn(async { crate::relay::running() }).await;
            let updated = this.update(cx, |view, cx| {
                if view.relay_running != running {
                    view.relay_running = running;
                    cx.notify();
                }
            });
            if updated.is_err() {
                break;
            }
            executor.timer(std::time::Duration::from_millis(1500)).await;
        })
        .detach();
    }

    fn reload(&mut self) {
        self.reload_opts();
        self.macros = macros::defaultdir().map(|d| macros::load(&d)).unwrap_or_default();
    }

    /// Refresh only the parsed options after a write — the hot path for every
    /// toggle and slider step. The workspace's config watcher is the single
    /// *apply* path; this just keeps the settings display in sync. Macros are
    /// not config-file state, so their directory scan stays out of here.
    fn reload_opts(&mut self) {
        let (opts, diagnostics) = config::load();
        for d in &diagnostics {
            eprintln!("sinclair: config line {}: {} ({})", d.line, d.message, d.key);
        }
        self.opts = opts;
    }

    fn set_section(&mut self, section: Section, cx: &mut Context<Self>) {
        self.section = section;
        self.editing = None;
        self.capturing = false;
        self.capture_macro = None;
        cx.notify();
    }

    /// The trigger currently bound to replay this macro, if any.
    fn macro_shortcut(&self, name: &str) -> Option<String> {
        let action = config::Action::MacroReplay(name.to_string());
        let (binds, _) = config::resolve(&self.opts.keybind);
        binds.iter().find(|kb| kb.action == action).map(|kb| kb.trigger())
    }

    /// Arm capture so the next chord becomes `name`'s replay shortcut.
    fn start_macro_capture(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        self.editing = None;
        self.capturing = false;
        self.capture_macro = Some(name);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Bind `trigger` to replay `name`, replacing any prior binding on that
    /// trigger and any other shortcut already pointing at this macro.
    fn assign_macro_shortcut(&mut self, name: &str, trigger: &str, cx: &mut Context<Self>) {
        let Ok((mods, key)) = config::keybind::parse_trigger(trigger) else {
            return;
        };
        let action = config::Action::MacroReplay(name.to_string());
        let (mut binds, _) = config::resolve(&self.opts.keybind);
        binds.retain(|kb| {
            !((kb.mods == mods && kb.key == key && kb.tail.is_empty()) || kb.action == action)
        });
        binds.push(config::Keybind {
            mods,
            key,
            tail: Vec::new(),
            action,
        });
        write_list("keybind", &config::diff_from_defaults(&binds));
        self.reload();
        cx.notify();
    }

    /// Remove any shortcut that replays `name`.
    fn clear_macro_shortcut(&mut self, name: &str, cx: &mut Context<Self>) {
        let action = config::Action::MacroReplay(name.to_string());
        let (mut binds, _) = config::resolve(&self.opts.keybind);
        let before = binds.len();
        binds.retain(|kb| kb.action != action);
        if binds.len() != before {
            write_list("keybind", &config::diff_from_defaults(&binds));
            self.reload();
            cx.notify();
        }
    }

    /// Delete the macro file and drop any shortcut bound to it.
    fn delete_macro(&mut self, name: &str, cx: &mut Context<Self>) {
        if let Some(dir) = macros::defaultdir() {
            let _ = macros::delete(&dir, name);
        }
        let action = config::Action::MacroReplay(name.to_string());
        let (mut binds, _) = config::resolve(&self.opts.keybind);
        let before = binds.len();
        binds.retain(|kb| kb.action != action);
        if binds.len() != before {
            write_list("keybind", &config::diff_from_defaults(&binds));
        }
        self.reload();
        cx.notify();
    }

    fn toggle(&mut self, b: Bool, cx: &mut Context<Self>) {
        write_config(b.key(), &(!b.get(&self.opts)).to_string());
        self.reload_opts();
        cx.notify();
    }

    /// Persist the value at slider fraction `frac` for `n`, but only when the
    /// snapped result actually changes — so a drag rewrites the config once per
    /// step it crosses, not once per pointer tick.
    fn slide_to(&mut self, n: Num, frac: f32, cx: &mut Context<Self>) {
        let value = n.value_at_fraction(frac);
        if (value - n.current(&self.opts)).abs() >= f32::EPSILON {
            write_config(n.key(), &n.fmt(value));
            self.reload_opts();
            cx.notify();
        }
    }

    fn cycle(&mut self, c: Choice, dir: i32, cx: &mut Context<Self>) {
        write_config(c.key(), &c.write_value(&self.opts, dir));
        self.reload_opts();
        cx.notify();
    }

    fn begin_edit(&mut self, target: EditTarget, window: &mut Window, cx: &mut Context<Self>) {
        // Save any in-progress edit before switching fields, so a value typed
        // (or pasted) into one field isn't lost by clicking another. Capturing a
        // keybind is transient, not a committed value, so skip it there.
        if !self.capturing && self.editing.as_ref().is_some_and(|(t, _)| t != &target) {
            self.commit_edit(cx);
        }
        match target {
            EditTarget::Field(f) => self.start_field(f, window, cx),
            EditTarget::Item(k, i) => self.start_item(k, i, window, cx),
            EditTarget::NewItem(k) => self.start_new_item(k, window, cx),
            EditTarget::MacroName(old) => self.start_macro_rename(old, window, cx),
        }
    }

    /// Begin renaming a saved macro inline (the Macros section's ✎ button).
    pub(crate) fn start_macro_rename(&mut self, old: String, window: &mut Window, cx: &mut Context<Self>) {
        self.editing = Some((EditTarget::MacroName(old.clone()), TextEdit::new(&old)));
        self.capturing = false;
        self.capture_macro = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Rename a saved macro: move its file, retarget any replay shortcut bound
    /// to the old name, and drop the old name entirely.
    fn rename_macro(&mut self, old: &str, new_raw: &str, cx: &mut Context<Self>) {
        let Some(new) = macros::sanitize_name(new_raw) else {
            eprintln!("sinclair: macro name `{new_raw}` has no usable characters");
            return;
        };
        if new == old {
            return;
        }
        let Some(dir) = macros::defaultdir() else {
            return;
        };
        if let Err(error) = macros::rename(&dir, old, &new) {
            eprintln!("sinclair: {error}");
            return;
        }
        // Keep any shortcut pointing at the macro working under its new name.
        let old_action = config::Action::MacroReplay(old.to_string());
        let (mut binds, _) = config::resolve(&self.opts.keybind);
        let mut changed = false;
        for kb in &mut binds {
            if kb.action == old_action {
                kb.action = config::Action::MacroReplay(new.clone());
                changed = true;
            }
        }
        if changed {
            write_list("keybind", &config::diff_from_defaults(&binds));
        }
        self.reload();
        cx.notify();
    }

    fn start_field(&mut self, field: Field, window: &mut Window, cx: &mut Context<Self>) {
        self.editing = Some((
            EditTarget::Field(field),
            TextEdit::new(&field.value(&self.opts)),
        ));
        self.capturing = false;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn start_item(&mut self, kind: ListKind, idx: usize, window: &mut Window, cx: &mut Context<Self>) {
        let current = kind.values(&self.opts).get(idx).cloned().unwrap_or_default();
        self.editing = Some((EditTarget::Item(kind, idx), TextEdit::new(&current)));
        self.capturing = false;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn start_new_item(&mut self, kind: ListKind, window: &mut Window, cx: &mut Context<Self>) {
        self.editing = Some((EditTarget::NewItem(kind), TextEdit::new("")));
        self.capturing = kind == ListKind::Keybind;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Arm trigger capture for the binding currently being edited, starting the
    /// edit first if needed. Bound to the per-row "record" button.
    fn record_item(&mut self, kind: ListKind, idx: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.start_item(kind, idx, window, cx);
        self.capturing = true;
        cx.notify();
    }

    fn remove_item(&mut self, kind: ListKind, idx: usize, cx: &mut Context<Self>) {
        let mut entries = kind.values(&self.opts);
        if idx < entries.len() {
            entries.remove(idx);
            self.write_list(kind, &entries);
        }
        self.editing = None;
        cx.notify();
    }

    fn commit_edit(&mut self, cx: &mut Context<Self>) {
        self.capturing = false;
        if let Some((target, edit)) = self.editing.take() {
            let text = edit.text();
            match target {
                EditTarget::Field(field) => write_config(field.key(), text.trim()),
                EditTarget::Item(kind, idx) => {
                    let mut entries = kind.values(&self.opts);
                    if idx < entries.len() {
                        if text.trim().is_empty() {
                            entries.remove(idx);
                        } else {
                            entries[idx] = text.trim().to_string();
                        }
                        self.write_list(kind, &entries);
                    }
                }
                EditTarget::NewItem(kind) => {
                    if !text.trim().is_empty() {
                        let mut entries = kind.values(&self.opts);
                        entries.push(text.trim().to_string());
                        self.write_list(kind, &entries);
                    }
                }
                EditTarget::MacroName(old) => {
                    if !text.trim().is_empty() {
                        self.rename_macro(&old, text.trim(), cx);
                    }
                }
            }
            self.reload();
        }
        cx.notify();
    }

    fn write_list(&self, kind: ListKind, entries: &[String]) {
        let (key, values) = kind.to_config(entries);
        write_list(key, &values);
    }

    /// Drop every user keybind override, restoring the built-in defaults.
    fn reset_keybinds(&mut self, cx: &mut Context<Self>) {
        self.editing = None;
        self.capturing = false;
        write_list("keybind", &[]);
        self.reload();
        cx.notify();
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let ks = &event.keystroke;
        if self.capturing {
            if ks.key == "escape" {
                self.capturing = false;
                cx.notify();
            } else {
                self.capture_key(ks, cx);
            }
            cx.stop_propagation();
            return;
        }
        if let Some(name) = self.capture_macro.clone() {
            if ks.key == "escape" {
                self.capture_macro = None;
                cx.notify();
            } else if let Some(trigger) = capture_trigger(ks) {
                self.assign_macro_shortcut(&name, &trigger, cx);
                self.capture_macro = None;
            }
            cx.stop_propagation();
            return;
        }
        if ks.modifiers.platform && ks.key == "w" {
            window.remove_window();
            cx.stop_propagation();
            return;
        }
        // Clipboard for the inline field editor (copy/cut/paste a value, e.g. a
        // binary path). Handled here since the clipboard needs `App` access.
        if ks.modifiers.platform
            && !ks.modifiers.alt
            && !ks.modifiers.control
            && self.editing.is_some()
            && matches!(ks.key.as_str(), "c" | "x" | "v")
        {
            match ks.key.as_str() {
                "c" => {
                    if let Some((_, edit)) = self.editing.as_ref() {
                        if let Some(t) = edit.selected_text() {
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(t));
                        }
                    }
                }
                "x" => {
                    if let Some((_, edit)) = self.editing.as_mut() {
                        if let Some(t) = edit.selected_text() {
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(t));
                            edit.delete_selection();
                            cx.notify();
                        }
                    }
                }
                "v" => {
                    let pasted = cx.read_from_clipboard().and_then(|i| i.text());
                    if let (Some(t), Some((_, edit))) = (pasted, self.editing.as_mut()) {
                        edit.insert(&t.replace(['\n', '\r'], " "));
                        cx.notify();
                    }
                }
                _ => {}
            }
            cx.stop_propagation();
            return;
        }
        let outcome = {
            let Some((_, edit)) = self.editing.as_mut() else {
                return;
            };
            guise::apply_key(edit, ks)
        };
        match outcome {
            guise::KeyOutcome::Submit => self.commit_edit(cx),
            guise::KeyOutcome::Cancel => {
                self.editing = None;
                self.capturing = false;
                cx.notify();
            }
            guise::KeyOutcome::Edited => cx.notify(),
            guise::KeyOutcome::Pass => return,
        }
        cx.stop_propagation();
    }

    /// Record one captured chord as the trigger of the binding being edited,
    /// preserving any already-typed action half. Bare modifier presses are
    /// ignored so capture waits for the real key.
    fn capture_key(&mut self, ks: &gpui::Keystroke, cx: &mut Context<Self>) {
        let Some(trigger) = capture_trigger(ks) else {
            return;
        };
        if let Some((_, edit)) = self.editing.as_mut() {
            let action = edit
                .text()
                .split_once('=')
                .map(|(_, a)| a.trim().to_string())
                .unwrap_or_default();
            *edit = TextEdit::new(&format!("{trigger}={action}"));
        }
        self.capturing = false;
        cx.notify();
    }
}

/// Convert a gpui keystroke into a config trigger string (`cmd+shift+t`), or
/// `None` for a bare modifier press that should not end capture.
fn capture_trigger(ks: &gpui::Keystroke) -> Option<String> {
    let key = ks.key.to_ascii_lowercase();
    if matches!(
        key.as_str(),
        "" | "cmd" | "command" | "super" | "ctrl" | "control" | "alt" | "option" | "shift"
            | "fn" | "function" | "capslock"
    ) {
        return None;
    }
    let m = &ks.modifiers;
    let mods = config::Mods {
        cmd: m.platform,
        ctrl: m.control,
        alt: m.alt,
        shift: m.shift,
    };
    Some(config::format_trigger(mods, &key))
}

/// The configured explicit path for a built-in tool, if any.
fn tool_path(opts: &config::Options, tool: &str) -> Option<String> {
    let p = match tool {
        "claude" => &opts.agent_claude_path,
        "codex" => &opts.agent_codex_path,
        "gemini" => &opts.agent_gemini_path,
        _ => &None,
    };
    p.clone().filter(|s| !s.trim().is_empty())
}

/// Write a single `key = value` line to the config file. See `crate::confwrite`
/// for the read-check + atomic-replace rules.
fn write_config(key: &str, value: &str) {
    crate::confwrite::upsert(key, value);
}

/// Replace every line for a repeated `key` with the given values.
fn write_list(key: &str, values: &[String]) {
    crate::confwrite::set_list(key, values);
}
