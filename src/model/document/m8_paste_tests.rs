//! Regression tests for Milestone 8 paste and link items (W7, W13, W15a).

use super::*;

fn doc(content: &str) -> DocumentTab {
    DocumentTab::new("m8p".into(), "m8p.md".into(), content.into(), None, None)
}

fn lone_lf(s: &str) -> bool {
    s.match_indices('\n')
        .any(|(i, _)| i == 0 || s.as_bytes()[i - 1] != b'\r')
}

#[test]
fn w15a_table_row_in_crlf_note_uses_crlf() {
    let mut d = doc("| A | B |\r\n| --- | --- |\r\n| 1 | 2 |\r\n");
    d.place(2, false);
    d.apply_format_in_mode("table", ViewMode::Wysiwyg);
    assert!(d.content.len() > 40, "a row was added");
    assert!(!lone_lf(&d.content), "mixed endings: {:?}", d.content);
}

#[test]
fn w15a_new_table_in_crlf_note_uses_crlf() {
    let mut d = doc("intro\r\nmore text\r\n");
    d.place(5, false);
    d.apply_format_in_mode("table", ViewMode::Wysiwyg);
    assert!(d.content.contains("Column 1"));
    assert!(!lone_lf(&d.content), "mixed endings: {:?}", d.content);
}
