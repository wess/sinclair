use super::*;
use super::super::model::{Bool, Choice, Field, Num, Section};
use super::super::{EditTarget, SettingsView};
use gpui::{px, AnyElement, Context};

impl SettingsView {
    pub(crate) fn toggle_row(&self, b: Bool, glyph: &str, color: theme::Rgb, cx: &mut Context<Self>) -> AnyElement {
        self.row(self.icon(glyph, color, px(22.0)), b.label(), self.switch(b, cx))
            .into_any_element()
    }

    fn stepper_row(&self, n: Num, glyph: &str, color: theme::Rgb, cx: &mut Context<Self>) -> AnyElement {
        self.row(self.icon(glyph, color, px(22.0)), n.label(), self.stepper(n, cx))
            .into_any_element()
    }

    fn cycle_row(&self, c: Choice, glyph: &str, color: theme::Rgb, cx: &mut Context<Self>) -> AnyElement {
        self.row(self.icon(glyph, color, px(22.0)), c.label(), self.cycle_control(c, cx))
            .into_any_element()
    }

    pub(crate) fn field_row(&self, f: Field, glyph: &str, color: theme::Rgb, cx: &mut Context<Self>) -> AnyElement {
        let input = self.text_input(
            EditTarget::Field(f),
            f.value(&self.opts),
            f.placeholder(),
            220.0,
            cx,
        );
        self.row(self.icon(glyph, color, px(22.0)), f.label(), input)
            .into_any_element()
    }

    pub(crate) fn general_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let g = Section::General.accent();
        vec![
            self.field_row(Field::Shell, "\u{2318}", g, cx),
            self.field_row(Field::WorkingDirectory, "\u{1f4c1}", g, cx),
            self.field_row(Field::Title, "\u{24c9}", g, cx),
            self.toggle_row(Bool::InheritCwd, "\u{21aa}", theme::Rgb::new(10, 132, 255), cx),
            self.toggle_row(Bool::QuitLast, "Q", theme::Rgb::new(255, 69, 58), cx),
            self.toggle_row(Bool::ConfirmClose, "!", theme::Rgb::new(255, 159, 10), cx),
            self.toggle_row(Bool::ConfirmQuit, "\u{23fb}", theme::Rgb::new(255, 69, 58), cx),
            self.toggle_row(Bool::PasteProtection, "\u{2335}", theme::Rgb::new(255, 214, 10), cx),
            self.toggle_row(Bool::ShellIntegration, "\u{276f}", theme::Rgb::new(48, 209, 88), cx),
            self.toggle_row(Bool::SessionRestore, "\u{21ba}", theme::Rgb::new(94, 92, 230), cx),
            self.toggle_row(Bool::TabTitleShowHost, "@", theme::Rgb::new(100, 210, 255), cx),
            self.toggle_row(Bool::CopyOnSelect, "\u{2713}", theme::Rgb::new(52, 199, 89), cx),
            self.cycle_row(Choice::OptionAsAlt, "\u{2325}", theme::Rgb::new(88, 86, 214), cx),
            self.cycle_row(Choice::ClipboardRead, "R", theme::Rgb::new(90, 200, 250), cx),
            self.cycle_row(Choice::ClipboardWrite, "W", theme::Rgb::new(94, 92, 230), cx),
        ]
    }

    pub(crate) fn appearance_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        vec![
            self.cycle_row(Choice::Theme, "\u{25d0}", Section::Appearance.accent(), cx),
            self.field_row(Field::ThemeLight, "\u{2600}", theme::Rgb::new(255, 214, 10), cx),
            self.field_row(Field::ThemeDark, "\u{263e}", theme::Rgb::new(94, 92, 230), cx),
            self.cycle_row(Choice::FontStyle, "B", theme::Rgb::new(255, 159, 10), cx),
            self.cycle_row(Choice::CursorStyle, "C", theme::Rgb::new(255, 69, 58), cx),
            self.toggle_row(Bool::CursorBlink, "\u{2737}", theme::Rgb::new(255, 214, 10), cx),
            self.field_row(Field::Foreground, "\u{25a0}", theme::Rgb::new(94, 92, 230), cx),
            self.field_row(Field::Background, "\u{25a1}", theme::Rgb::new(99, 99, 102), cx),
            self.field_row(Field::CursorColor, "I", theme::Rgb::new(255, 69, 58), cx),
            self.field_row(Field::CursorText, "T", theme::Rgb::new(255, 149, 0), cx),
            self.field_row(Field::SelectionForeground, "S", theme::Rgb::new(10, 132, 255), cx),
            self.field_row(Field::SelectionBackground, "S", theme::Rgb::new(48, 209, 88), cx),
            self.toggle_row(Bool::BoldIsBright, "\u{2600}", theme::Rgb::new(255, 214, 10), cx),
            self.stepper_row(Num::MinContrast, "\u{25d1}", theme::Rgb::new(142, 142, 147), cx),
        ]
    }

    pub(crate) fn terminal_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let t = Section::Terminal.accent();
        let blue = theme::Rgb::new(90, 200, 250);
        vec![
            self.stepper_row(Num::FontSize, "T", t, cx),
            self.stepper_row(Num::CellWidth, "W", blue, cx),
            self.stepper_row(Num::CellHeight, "H", blue, cx),
            self.stepper_row(Num::PaddingX, "X", blue, cx),
            self.stepper_row(Num::PaddingY, "Y", blue, cx),
            self.stepper_row(Num::WindowWidth, "\u{2194}", theme::Rgb::new(88, 86, 214), cx),
            self.stepper_row(Num::WindowHeight, "\u{2195}", theme::Rgb::new(88, 86, 214), cx),
            self.stepper_row(Num::Scrollback, "\u{2630}", theme::Rgb::new(142, 142, 147), cx),
            self.stepper_row(Num::ScrollMultiplier, "\u{2207}", theme::Rgb::new(255, 159, 10), cx),
            self.toggle_row(Bool::MouseHide, "\u{2196}", theme::Rgb::new(170, 170, 170), cx),
            self.toggle_row(Bool::SmartSelect, "\u{2318}", theme::Rgb::new(52, 199, 89), cx),
            self.toggle_row(Bool::MiddleClickPaste, "\u{2504}", theme::Rgb::new(90, 200, 250), cx),
            self.toggle_row(Bool::FocusFollowsMouse, "\u{2192}", theme::Rgb::new(255, 159, 10), cx),
            self.stepper_row(Num::SplitOpacity, "\u{25d0}", theme::Rgb::new(94, 92, 230), cx),
            self.stepper_row(Num::BgOpacity, "\u{25d1}", theme::Rgb::new(94, 92, 230), cx),
            self.field_row(Field::SplitDivider, "\u{2503}", theme::Rgb::new(99, 99, 102), cx),
        ]
    }
}
