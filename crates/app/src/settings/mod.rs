//! The settings window: a searchable, schema-driven editor (search bar,
//! category sidebar, one control per setting) over the settings.json
//! backing store. The file stays the source of truth — every control writes
//! one key through `crate::confwrite`, the live-reload watcher applies it,
//! and a modified setting shows an accent bar plus a per-row reset that
//! removes its key. `Edit in settings.json` opens the file itself for
//! anything the GUI doesn't cover.

mod schema;
mod ui;

use std::collections::{HashMap, HashSet};

use gpui::prelude::*;
use gpui::{
    bounds, point, px, size, App, Bounds, Context, FocusHandle, KeyDownEvent, Pixels,
    TitlebarOptions, Window, WindowBounds, WindowOptions,
};

use guise::TextEdit;
use schema::{Control, ListKind, Section, Setting};

const WIDTH: f32 = 920.0;
const HEIGHT: f32 = 780.0;

/// What the single active text editor is bound to.
#[derive(Clone, PartialEq)]
pub(crate) enum EditTarget {
    /// A free-text setting, by schema key.
    Field(&'static str),
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
    /// Keys the user's settings.json sets — the modified indicators.
    userkeys: HashSet<String>,
    /// The search box. Receives keys whenever no field editor is active.
    query: TextEdit,
    section: Section,
    editing: Option<(EditTarget, TextEdit)>,
    /// When true, the next key chord is captured as the edited keybind's
    /// trigger instead of being typed into the field.
    capturing: bool,
    /// The Choice setting whose variant list is expanded, if any.
    open_choice: Option<&'static str>,
    /// Saved macros, for the Macros section.
    macros: Vec<macros::Macro>,
    /// When set, the next key chord is captured as the shortcut for this macro.
    capture_macro: Option<String>,
    focus: FocusHandle,
    relay_running: bool,
    tool_tests: HashMap<&'static str, ToolTest>,
    /// Each slider track's window-space bounds, captured every frame so a
    /// mouse-down (which carries a position but not the element's bounds) can
    /// be mapped to a value. Keyed by the option's settings key.
    slider_bounds: HashMap<&'static str, Bounds<Pixels>>,
    /// Each choice button's window-space bounds, captured every frame so the
    /// open dropdown menu can anchor below its button. Keyed by settings key.
    choice_bounds: HashMap<&'static str, Bounds<Pixels>>,
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
            is_resizable: true,
            titlebar: Some(TitlebarOptions {
                title: Some("Settings".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(20.0), px(20.0))),
            }),
            ..Default::default()
        },
        |window, cx| {
            window.set_window_title("Settings");
            let view = cx.new(SettingsView::new);
            // Focus immediately so typing searches without a click.
            let focus = view.read(cx).focus.clone();
            window.focus(&focus, cx);
            view
        },
    );
}

/// Open settings.json in the system editor (creating it first if needed).
pub(crate) fn open_settings_file() {
    let Some(path) = crate::confwrite::ensure_settings_file() else {
        return;
    };
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("open");
        c.arg("-t").arg(&path);
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(&path);
        c
    };
    #[cfg(windows)]
    let mut cmd = {
        let mut c = std::process::Command::new("cmd");
        c.arg("/C").arg("start").arg("").arg(&path);
        c
    };
    if let Err(e) = cmd.spawn() {
        eprintln!("sinclair: could not open {}: {e}", path.display());
    }
}

impl SettingsView {
    fn new(cx: &mut Context<Self>) -> Self {
        let mut view = Self {
            opts: config::Options::default(),
            userkeys: HashSet::new(),
            query: TextEdit::new(""),
            section: Section::General,
            editing: None,
            capturing: false,
            open_choice: None,
            macros: Vec::new(),
            capture_macro: None,
            focus: cx.focus_handle(),
            // Probing the relay blocks (file read + TCP connect); start
            // pessimistic and let the first off-thread poll fill it in.
            relay_running: false,
            tool_tests: HashMap::new(),
            slider_bounds: HashMap::new(),
            choice_bounds: HashMap::new(),
        };
        view.reload();
        view.poll_relay_status(cx);
        view
    }

    /// The active search query (empty = no filter).
    fn search(&self) -> String {
        self.query.text().trim().to_string()
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

    /// Refresh the parsed options and modified-key set after a write — the
    /// hot path for every toggle and slider step. The workspace's settings
    /// watcher is the single *apply* path; this just keeps the display in
    /// sync. Macros are not settings-file state, so their directory scan
    /// stays out of here.
    fn reload_opts(&mut self) {
        let (opts, diagnostics) = config::load();
        for d in &diagnostics {
            eprintln!("sinclair: settings line {}: {} ({})", d.line, d.message, d.key);
        }
        self.opts = opts;
        self.userkeys = config::default_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|text| config::settings::user_keys(&text).into_iter().collect())
            .unwrap_or_default();
    }

    /// Whether the user's file overrides this key.
    fn modified(&self, key: &str) -> bool {
        self.userkeys.contains(key)
    }

    fn set_section(&mut self, section: Section, cx: &mut Context<Self>) {
        self.section = section;
        self.query = TextEdit::new("");
        self.editing = None;
        self.capturing = false;
        self.capture_macro = None;
        self.open_choice = None;
        cx.notify();
    }

    /// Remove a key from settings.json, restoring the built-in default.
    fn reset(&mut self, key: &'static str, cx: &mut Context<Self>) {
        if self.editing.as_ref().is_some_and(|(t, _)| *t == EditTarget::Field(key)) {
            self.editing = None;
        }
        crate::confwrite::remove(key);
        self.reload_opts();
        cx.notify();
    }

    fn toggle(&mut self, s: &'static Setting, cx: &mut Context<Self>) {
        if let Control::Toggle(get) = s.control {
            write_config(s.key, &(!get(&self.opts)).to_string());
            self.reload_opts();
            cx.notify();
        }
    }

    /// Persist the value at slider fraction `frac` for `s`, but only when the
    /// snapped result actually changes — so a drag rewrites the file once per
    /// step it crosses, not once per pointer tick.
    fn slide_to(&mut self, s: &'static Setting, frac: f32, cx: &mut Context<Self>) {
        let Control::Slider(n) = &s.control else {
            return;
        };
        let value = n.value_at_fraction(frac);
        if (value - (n.get)(&self.opts)).abs() >= f32::EPSILON {
            write_config(s.key, &n.fmt(value));
            self.reload_opts();
            cx.notify();
        }
    }

    /// Expand or collapse a Choice setting's variant list.
    fn toggle_choice(&mut self, key: &'static str, cx: &mut Context<Self>) {
        self.open_choice = if self.open_choice == Some(key) { None } else { Some(key) };
        cx.notify();
    }

    /// Pick one variant of a Choice setting. The unset pseudo-variant
    /// removes the key instead of writing a value.
    fn choose(&mut self, s: &'static Setting, value: String, unset: bool, cx: &mut Context<Self>) {
        if unset {
            crate::confwrite::remove(s.key);
        } else {
            write_config(s.key, &value);
        }
        self.open_choice = None;
        self.reload_opts();
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

    fn begin_edit(&mut self, target: EditTarget, window: &mut Window, cx: &mut Context<Self>) {
        // Save any in-progress edit before switching fields, so a value typed
        // (or pasted) into one field isn't lost by clicking another. Capturing a
        // keybind is transient, not a committed value, so skip it there.
        if !self.capturing && self.editing.as_ref().is_some_and(|(t, _)| t != &target) {
            self.commit_edit(cx);
        }
        match target {
            EditTarget::Field(key) => self.start_field(key, window, cx),
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

    fn start_field(&mut self, key: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        let value = schema::find(key)
            .map(|s| match &s.control {
                Control::Text { get, .. } => get(&self.opts),
                _ => String::new(),
            })
            .unwrap_or_default();
        self.editing = Some((EditTarget::Field(key), TextEdit::new(&value)));
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
            self.write_kind(kind, &entries);
        }
        self.editing = None;
        cx.notify();
    }

    fn commit_edit(&mut self, cx: &mut Context<Self>) {
        self.capturing = false;
        if let Some((target, edit)) = self.editing.take() {
            let text = edit.text();
            match target {
                // An emptied field removes its key (back to the default).
                EditTarget::Field(key) => write_config(key, text.trim()),
                EditTarget::Item(kind, idx) => {
                    let mut entries = kind.values(&self.opts);
                    if idx < entries.len() {
                        if text.trim().is_empty() {
                            entries.remove(idx);
                        } else {
                            entries[idx] = text.trim().to_string();
                        }
                        self.write_kind(kind, &entries);
                    }
                }
                EditTarget::NewItem(kind) => {
                    if !text.trim().is_empty() {
                        let mut entries = kind.values(&self.opts);
                        entries.push(text.trim().to_string());
                        self.write_kind(kind, &entries);
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

    fn write_kind(&self, kind: ListKind, entries: &[String]) {
        write_list(kind.key(), &kind.to_values(entries));
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
        // An open variant list closes on escape before anything else sees it.
        if ks.key == "escape" && self.open_choice.is_some() {
            self.open_choice = None;
            cx.notify();
            cx.stop_propagation();
            return;
        }
        // Clipboard for the active editor — a field being edited, else the
        // search box. Handled here since the clipboard needs `App` access.
        if ks.modifiers.platform
            && !ks.modifiers.alt
            && !ks.modifiers.control
            && matches!(ks.key.as_str(), "c" | "x" | "v")
        {
            let edit = match self.editing.as_mut() {
                Some((_, edit)) => edit,
                None => &mut self.query,
            };
            match ks.key.as_str() {
                "c" => {
                    if let Some(t) = edit.selected_text() {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(t));
                    }
                }
                "x" => {
                    if let Some(t) = edit.selected_text() {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(t));
                        edit.delete_selection();
                        cx.notify();
                    }
                }
                "v" => {
                    if let Some(t) = cx.read_from_clipboard().and_then(|i| i.text()) {
                        let edit = match self.editing.as_mut() {
                            Some((_, edit)) => edit,
                            None => &mut self.query,
                        };
                        edit.insert(&t.replace(['\n', '\r'], " "));
                        cx.notify();
                    }
                }
                _ => {}
            }
            cx.stop_propagation();
            return;
        }
        // Route everything else to the field editor when one is active,
        // otherwise to the search box (type anywhere to search).
        match self.editing.as_mut() {
            Some((_, edit)) => {
                let outcome = guise::apply_key(edit, ks);
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
            }
            None => {
                match guise::apply_key(&mut self.query, ks) {
                    // Enter has nothing to commit; escape clears the filter.
                    guise::KeyOutcome::Submit => {}
                    guise::KeyOutcome::Cancel => {
                        self.query = TextEdit::new("");
                        cx.notify();
                    }
                    guise::KeyOutcome::Edited => cx.notify(),
                    guise::KeyOutcome::Pass => return,
                }
            }
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

/// Write one key to settings.json (an empty value removes it). See
/// `crate::confwrite` for the read-check + atomic-replace rules.
fn write_config(key: &str, value: &str) {
    crate::confwrite::upsert(key, value);
}

/// Replace a repeated key's entries (empty removes the key).
fn write_list(key: &str, values: &[String]) {
    crate::confwrite::set_list(key, values);
}
