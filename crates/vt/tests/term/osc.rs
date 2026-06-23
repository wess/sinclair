use super::*;
use crate::term::Terminal;

#[test]
fn title_via_osc0_and_osc2() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"\x1b]0;hello\x07");
    assert_eq!(t.title(), "hello");
    t.feed(b"\x1b]2;a;b\x1b\\");
    assert_eq!(t.title(), "a;b");
}

#[test]
fn palette_set_and_reset() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"\x1b]4;1;rgb:ff/00/00\x07");
    assert_eq!(t.palette_override(1), Some((255, 0, 0)));
    t.feed(b"\x1b]4;2;#00ff00;3;#0000ff\x07");
    assert_eq!(t.palette_override(2), Some((0, 255, 0)));
    assert_eq!(t.palette_override(3), Some((0, 0, 255)));
    t.feed(b"\x1b]104;2\x07");
    assert_eq!(t.palette_override(2), None);
    assert_eq!(t.palette_override(3), Some((0, 0, 255)));
    t.feed(b"\x1b]104\x07");
    assert_eq!(t.palette_override(1), None);
    assert_eq!(t.palette_override(3), None);
}

#[test]
fn cwd_stored() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"\x1b]7;file://host/Users/me\x07");
    assert_eq!(t.cwd(), Some("file://host/Users/me"));
}

#[test]
fn cursor_color_set_and_reset() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"\x1b]12;#102030\x07");
    assert_eq!(t.cursor_color(), Some((16, 32, 48)));
    t.feed(b"\x1b]112\x07");
    assert_eq!(t.cursor_color(), None);
}

#[test]
fn unknown_osc_ignored() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"\x1b]777;whatever\x07ok");
    assert_eq!(t.row_text(0), "ok");
}

fn report_colors() -> crate::term::ReportColors {
    crate::term::ReportColors {
        foreground: (0xc0, 0xc0, 0xc0),
        background: (0x10, 0x10, 0x10),
        cursor: (0xff, 0xff, 0x00),
        palette: [(1, 2, 3); 256],
    }
}

#[test]
fn osc10_11_queries_report_theme_colors() {
    let mut t = Terminal::new(10, 3, 0);
    t.set_report_colors(report_colors());
    t.feed(b"\x1b]10;?\x07");
    assert_eq!(t.take_output(), b"\x1b]10;rgb:c0c0/c0c0/c0c0\x07");
    t.feed(b"\x1b]11;?\x1b\\");
    // ST request gets an ST-terminated reply.
    assert_eq!(t.take_output(), b"\x1b]11;rgb:1010/1010/1010\x1b\\");
}

#[test]
fn osc_color_query_ignored_without_report_colors() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"\x1b]11;?\x07");
    assert!(t.take_output().is_empty());
}

#[test]
fn osc4_query_prefers_override_then_report_palette() {
    let mut t = Terminal::new(10, 3, 0);
    t.set_report_colors(report_colors());
    // Unoverridden index answers from the report palette.
    t.feed(b"\x1b]4;5;?\x07");
    assert_eq!(t.take_output(), b"\x1b]4;5;rgb:0101/0202/0303\x07");
    // An OSC 4 override wins over the report palette.
    t.feed(b"\x1b]4;5;rgb:ff/00/00\x07");
    t.feed(b"\x1b]4;5;?\x07");
    assert_eq!(t.take_output(), b"\x1b]4;5;rgb:ffff/0000/0000\x07");
}

#[test]
fn osc12_query_uses_override_then_report_cursor() {
    let mut t = Terminal::new(10, 3, 0);
    t.set_report_colors(report_colors());
    t.feed(b"\x1b]12;?\x07");
    assert_eq!(t.take_output(), b"\x1b]12;rgb:ffff/ffff/0000\x07");
    t.feed(b"\x1b]12;#010203\x07");
    t.feed(b"\x1b]12;?\x07");
    assert_eq!(t.take_output(), b"\x1b]12;rgb:0101/0202/0303\x07");
}

#[test]
fn osc52_set_decodes_base64() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"\x1b]52;c;aGVsbG8=\x07");
    let clip = t.take_clipboard().expect("clipboard write");
    assert_eq!(clip.kind, "c");
    assert_eq!(clip.data, b"hello");
    // Taken once.
    assert!(t.take_clipboard().is_none());
}

#[test]
fn osc52_empty_kind_defaults_to_clipboard() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"\x1b]52;;Zm9vYmFy\x07");
    let clip = t.take_clipboard().expect("clipboard write");
    assert_eq!(clip.kind, "c");
    assert_eq!(clip.data, b"foobar");
}

#[test]
fn osc52_query_and_garbage_ignored() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"\x1b]52;c;?\x07");
    assert!(t.take_clipboard().is_none());
    t.feed(b"\x1b]52;c;@@notbase64@@\x07");
    assert!(t.take_clipboard().is_none());
}

#[test]
fn osc8_links_cells_until_closed() {
    let mut t = Terminal::new(20, 3, 0);
    t.feed(b"\x1b]8;;https://example.com\x07ab\x1b]8;;\x07cd");
    // 'a' and 'b' carry the link; 'c' and 'd' do not.
    assert_eq!(t.cell_hyperlink(t.cell(0, 0)), Some("https://example.com"));
    assert_eq!(t.cell_hyperlink(t.cell(0, 1)), Some("https://example.com"));
    assert_eq!(t.cell_hyperlink(t.cell(0, 2)), None);
    assert_eq!(t.cell_hyperlink(t.cell(0, 3)), None);
}

#[test]
fn osc8_uri_with_semicolons_is_preserved() {
    let mut t = Terminal::new(20, 3, 0);
    t.feed(b"\x1b]8;;https://x/a;b;c\x07z\x1b]8;;\x07");
    assert_eq!(t.cell_hyperlink(t.cell(0, 0)), Some("https://x/a;b;c"));
}

#[test]
fn osc8_id_param_groups_links() {
    let mut t = Terminal::new(20, 3, 0);
    t.feed(b"\x1b]8;id=foo;https://a\x07x\x1b]8;;\x07");
    let id = t.cell(0, 0).hyperlink.expect("linked");
    let link = t.hyperlink(id).expect("resolves");
    assert_eq!(link.id.as_deref(), Some("foo"));
    assert_eq!(link.uri, "https://a");
}

#[test]
fn osc133_marks_prompt_rows() {
    let mut t = Terminal::new(10, 4, 10);
    // Prompt on row 0, then output pushes a second prompt down a line.
    t.feed(b"\x1b]133;A\x07$ \r\nout\r\n\x1b]133;A\x07$ ");
    let prompts = t.prompt_lines();
    // Two prompt rows recorded (no scrollback yet, so global == grid row).
    assert_eq!(prompts, vec![0, 2]);
}

#[test]
fn osc133_prompts_follow_into_scrollback() {
    let mut t = Terminal::new(10, 2, 10);
    // Mark a prompt, then scroll it into history.
    t.feed(b"\x1b]133;A\x07top\r\nb\r\nc\r\nd");
    // The marked row is now the oldest scrollback line: global index 0.
    assert_eq!(t.prompt_lines().first(), Some(&0));
    assert_eq!(t.visible_row(0).text(), "c");
}

#[test]
fn osc8_links_cleared_by_ris() {
    let mut t = Terminal::new(20, 3, 0);
    t.feed(b"\x1b]8;;https://a\x07x");
    let id = t.cell(0, 0).hyperlink.expect("linked");
    t.feed(b"\x1bc");
    // After RIS the registry is empty and the cell is blank.
    assert!(t.hyperlink(id).is_none());
    assert_eq!(t.cell_hyperlink(t.cell(0, 0)), None);
}

#[test]
fn color_spec_forms() {
    assert_eq!(parse_color_spec("rgb:ff/00/80"), Some((255, 0, 128)));
    assert_eq!(parse_color_spec("rgb:f/0/8"), Some((255, 0, 136)));
    assert_eq!(parse_color_spec("rgb:ffff/0000/8000"), Some((255, 0, 128)));
    assert_eq!(parse_color_spec("#ff0080"), Some((255, 0, 128)));
    assert_eq!(parse_color_spec("#f08"), Some((255, 0, 136)));
    assert_eq!(parse_color_spec("#ffff00008000"), Some((255, 0, 128)));
    assert_eq!(parse_color_spec("nonsense"), None);
    assert_eq!(parse_color_spec("#12345"), None);
    assert_eq!(parse_color_spec("rgb:gg/00/00"), None);
}

#[test]
fn number_parsing() {
    assert_eq!(parse_number(b"0"), Some(0));
    assert_eq!(parse_number(b"104"), Some(104));
    assert_eq!(parse_number(b""), None);
    assert_eq!(parse_number(b"12a"), None);
    assert_eq!(parse_number(b"999999"), None);
}
