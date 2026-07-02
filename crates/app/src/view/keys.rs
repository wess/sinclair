use super::*;

/// The key name as text only when it is a single character (a literal key
/// such as `b` or `/`), never a named key like `left` or `enter`.
fn single_char(key: &str) -> Option<&str> {
    let mut chars = key.chars();
    match (chars.next(), chars.next()) {
        (Some(_), None) => Some(key),
        _ => None,
    }
}

/// The `macos-option-as-alt` decision for one keystroke: the effective
/// modifiers and the text `encode_key` should use. Pure so it can be tested
/// off the macOS event path. On non-macOS, or when Option isn't held, the
/// platform's composed text passes straight through.
pub(crate) fn option_policy<'k>(
    policy: config::OptionAsAlt,
    is_macos: bool,
    option_held: bool,
    key: &'k str,
    key_char: Option<&'k str>,
    mut mods: input::Mods,
) -> (input::Mods, Option<&'k str>) {
    if !is_macos || !option_held {
        return (mods, key_char);
    }
    match policy {
        config::OptionAsAlt::False | config::OptionAsAlt::Auto => {
            mods.alt = false;
            (mods, key_char)
        }
        _ => (mods, single_char(key)),
    }
}

/// Resolve `macos-option-as-alt = auto` to a concrete True/False using the
/// active keyboard layout. US-style ASCII layouts want Option to act as Alt
/// (so `option+b` sends `ESC b`); other layouts keep it as a composer.
fn resolve_auto(policy: config::OptionAsAlt) -> config::OptionAsAlt {
    if policy != config::OptionAsAlt::Auto {
        return policy;
    }
    let wants = crate::appkit::keyboard_layout_id()
        .as_deref()
        .map(layout_wants_alt)
        .unwrap_or(false);
    if wants {
        config::OptionAsAlt::True
    } else {
        config::OptionAsAlt::False
    }
}

/// Whether a keyboard layout id (e.g. `com.apple.keylayout.US`) is one where
/// Option should default to Alt. Pure so it can be tested off the macOS path.
pub(crate) fn layout_wants_alt(id: &str) -> bool {
    let layout = id.rsplit('.').next().unwrap_or(id);
    matches!(
        layout,
        "US" | "ABC" | "USExtended" | "US-PC" | "USInternational-PC"
    )
}

impl TerminalView {
    /// Capture phase: intercept Tab / Shift+Tab before gpui's built-in focus
    /// traversal consumes them, so they reach the shell (zsh completion, etc.).
    /// Every other key is left to the normal bubble-phase [`Self::key_down`]
    /// so keybinding precedence is unchanged. Overlays and read-only panes are
    /// deferred to the bubble handler, which already routes those cases.
    pub(crate) fn capture_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key != "tab" {
            return;
        }
        if self.context_menu.is_some()
            || self.search.is_some()
            || self.hints_active()
            || self.copy_mode_active()
            || self.assist.is_some()
            || self.read_only
        {
            return;
        }
        self.key_down(event, window, cx);
    }

    pub(crate) fn key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let mods = input::Mods {
            shift: keystroke.modifiers.shift,
            alt: keystroke.modifiers.alt,
            ctrl: keystroke.modifiers.control,
            cmd: keystroke.modifiers.platform,
        };
        if self.context_menu.is_some() {
            if keystroke.key == "escape" {
                self.context_menu = None;
                cx.notify();
                cx.stop_propagation();
            }
            return;
        }
        if self.copy_mode_active() {
            self.copy_mode_key(keystroke, cx);
            cx.stop_propagation();
            return;
        }
        if self.hints_active() {
            self.hints_key(keystroke, cx);
            cx.stop_propagation();
            return;
        }
        if self.search.is_some() {
            self.search_key(keystroke, mods, cx);
            cx.stop_propagation();
            return;
        }
        if self.assist.is_some() {
            self.assist_key(keystroke, mods, cx);
            cx.stop_propagation();
            return;
        }
        if self.read_only {
            cx.stop_propagation();
            return;
        }
        if cx.try_global::<crate::root::MacroRecorder>().is_some() {
            cx.update_global::<crate::root::MacroRecorder, _>(|rec, _| {
                rec.0.key(&keystroke.key, keystroke.key_char.as_deref());
            });
        }
        let state = self.term_state();
        let (mods, text) = self.resolve_option(keystroke, mods);
        if let Some(bytes) = input::encode_key(&keystroke.key, text, mods, state) {
            self.scroll_to_bottom(cx);
            let _ = self.session.write(&bytes);
            if cx
                .try_global::<crate::root::Broadcast>()
                .is_some_and(|b| b.0)
            {
                cx.emit(ViewEvent::Input(bytes));
            }
            cx.stop_propagation();
        }
    }

    /// Snapshot the terminal modes `encode_key` needs to spell a keystroke.
    pub(crate) fn term_state(&self) -> input::TermState {
        self.session.with_term(|term| input::TermState {
            cursor_keys_app: term.cursor_keys_app(),
            keypad_app: term.keypad_app(),
            bracketed_paste: term.bracketed_paste(),
            kitty_flags: term.kitty_keyboard_flags(),
        })
    }

    /// Apply the `macos-option-as-alt` policy, returning the effective
    /// modifiers and the text source for `encode_key`. On non-macOS, or when
    /// Option isn't held, it passes the platform's composed text straight
    /// through.
    fn resolve_option<'k>(
        &self,
        keystroke: &'k gpui::Keystroke,
        mods: input::Mods,
    ) -> (input::Mods, Option<&'k str>) {
        let option_held = keystroke.modifiers.alt && !keystroke.modifiers.platform;
        option_policy(
            resolve_auto(self.option_as_alt),
            cfg!(target_os = "macos"),
            option_held,
            &keystroke.key,
            keystroke.key_char.as_deref(),
            mods,
        )
    }
}
