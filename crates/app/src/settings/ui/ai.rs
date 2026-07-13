//! The AI section: schema rows for the switches and relay options, plus the
//! hand-built Agent-tools group (Test buttons need live probe state) and the
//! relay status rows.

use super::super::schema::{self, Control, Section, Setting, TOOL_KEYS};
use super::super::{SettingsView, ToolTest};
use super::*;
use gpui::{div, px, AnyElement, Context, MouseButton, SharedString};

impl SettingsView {
    /// The whole AI section: generic rows (minus the tool keys the tools
    /// group lays out itself), relay status when running, then the tools.
    pub(crate) fn ai_content(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut rows: Vec<AnyElement> = Vec::new();
        let mut groups: Vec<AnyElement> = Vec::new();
        for s in schema::in_section(Section::Ai) {
            if TOOL_KEYS.contains(&s.key) {
                continue;
            }
            // Token optimization only matters with AI on; hide it otherwise.
            if s.key == "ai-optimize-tokens" && !self.opts.ai_enabled {
                continue;
            }
            match &s.control {
                Control::List(kind) => groups.push(self.list_group(s, *kind, cx).into_any_element()),
                _ => rows.push(self.setting_row(s, cx)),
            }
        }
        if self.opts.relay_enabled {
            rows.push(self.relay_status_row());
            rows.push(self.relay_log_row());
        }
        let mut out: Vec<AnyElement> = vec![self.list(rows).into_any_element()];
        out.push(self.tools_group(cx).into_any_element());
        out.extend(groups);
        out
    }

    /// A live green/red dot for whether the relay server is listening.
    fn relay_status_row(&self) -> AnyElement {
        let running = self.relay_running;
        let color = if running {
            theme::Rgb::new(52, 199, 89)
        } else {
            theme::Rgb::new(255, 69, 58)
        };
        let dot = div()
            .w(px(22.0))
            .h(px(22.0))
            .flex()
            .items_center()
            .justify_center()
            .child(div().w(px(10.0)).h(px(10.0)).rounded(px(5.0)).bg(hsla(color)));
        let status = div()
            .text_color(hsla(color))
            .child(SharedString::from(if running { "Running" } else { "Stopped" }));
        self.row(dot, "Relay server", status).into_any_element()
    }

    /// The "Agent tools" group: each known tool with a Test button + toggle,
    /// and the explicit path/flags fields for the CLI tools (so a non-PATH
    /// install is found). Ollama is reached over its API port, so it has no
    /// path.
    fn tools_group(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut rows: Vec<AnyElement> = Vec::new();
        for &key in TOOL_KEYS {
            let Some(s) = schema::find(key) else {
                continue;
            };
            match &s.control {
                Control::Toggle(get) => {
                    let tool: &'static str = key.strip_prefix("agent-").unwrap_or(key);
                    rows.push(self.tool_row(s, *get, tool, cx));
                }
                _ => rows.push(self.setting_row(s, cx)),
            }
        }
        div()
            .flex()
            .flex_col()
            .child(self.heading("Agent tools"))
            .child(self.list(rows))
    }

    fn tool_row(
        &self,
        s: &'static Setting,
        get: fn(&config::Options) -> bool,
        tool: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let test = button_box("Test").on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _ev, _window, cx| {
                this.test_tool(tool, cx);
                cx.stop_propagation();
            }),
        );
        let control = div()
            .flex()
            .items_center()
            .gap_2()
            .child(self.test_result(tool))
            .child(test)
            .child(self.switch(s, get, cx));
        self.row(self.icon("\u{25cb}", px(18.0)), s.label, control)
            .into_any_element()
    }

    fn test_result(&self, tool: &str) -> AnyElement {
        match self.tool_tests.get(tool) {
            Some(ToolTest::Testing) => div()
                .text_color(hsla(MUTED))
                .child(SharedString::from("testing…"))
                .into_any_element(),
            Some(ToolTest::Ok(m)) => div()
                .text_color(hsla(theme::Rgb::new(52, 199, 89)))
                .child(SharedString::from(format!("\u{2713} {}", trunc(m, 22))))
                .into_any_element(),
            Some(ToolTest::Fail(e)) => div()
                .text_color(hsla(theme::Rgb::new(255, 69, 58)))
                .child(SharedString::from(format!("\u{2717} {}", trunc(e, 30))))
                .into_any_element(),
            None => div().into_any_element(),
        }
    }

    fn relay_log_row(&self) -> AnyElement {
        let path = crate::relay::log_path().display().to_string();
        let text = div()
            .text_size(px(12.5))
            .text_color(hsla(MUTED))
            .child(SharedString::from(path));
        self.row(self.icon("\u{2630}", px(18.0)), "Log", text)
            .into_any_element()
    }
}
