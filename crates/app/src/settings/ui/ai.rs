use super::*;
use super::super::model::{Bool, Field, Section};
use super::super::{SettingsView, ToolTest};
use gpui::{div, px, AnyElement, Context, MouseButton, SharedString};

impl SettingsView {
    pub(crate) fn ai_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let a = Section::Ai.accent();
        let mut rows = vec![
            self.toggle_row(Bool::AiEnabled, "\u{2728}", a, cx),
            self.toggle_row(Bool::McpServer, "M", theme::Rgb::new(10, 132, 255), cx),
            self.toggle_row(Bool::RelayEnabled, "R", theme::Rgb::new(52, 199, 89), cx),
            self.toggle_row(
                Bool::RelayStartOnLaunch,
                "\u{21aa}",
                theme::Rgb::new(255, 159, 10),
                cx,
            ),
            self.field_row(Field::RelayAddress, "@", theme::Rgb::new(90, 200, 250), cx),
            self.field_row(
                Field::RelayDefaultAgent,
                "\u{2318}",
                theme::Rgb::new(94, 92, 230),
                cx,
            ),
        ];
        if self.opts.relay_enabled {
            rows.push(self.relay_status_row());
            rows.push(self.relay_log_row());
        }
        rows
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
    /// and an explicit-path field for the CLI tools (so a non-PATH install is
    /// found). Ollama is reached over its API port, so it has no path.
    pub(crate) fn tools_group(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let accent = Section::Ai.accent();
        div()
            .flex()
            .flex_col()
            .child(self.heading("Agent tools"))
            .child(self.list(vec![
                self.tool_row(Bool::ToolClaude, "claude", cx),
                self.field_row(Field::ClaudePath, "\u{2026}", accent, cx),
                self.tool_row(Bool::ToolCodex, "codex", cx),
                self.field_row(Field::CodexPath, "\u{2026}", accent, cx),
                self.tool_row(Bool::ToolOllama, "ollama", cx),
                self.tool_row(Bool::ToolGemini, "gemini", cx),
                self.field_row(Field::GeminiPath, "\u{2026}", accent, cx),
            ]))
    }

    fn tool_row(&self, b: Bool, tool: &'static str, cx: &mut Context<Self>) -> AnyElement {
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
            .child(self.switch(b, cx));
        self.row(self.icon("\u{25cb}", Section::Ai.accent(), px(22.0)), b.label(), control)
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
            .text_color(hsla(MUTED))
            .child(SharedString::from(path));
        self.row(self.icon("\u{2630}", theme::Rgb::new(142, 142, 147), px(22.0)), "Log", text)
            .into_any_element()
    }
}
