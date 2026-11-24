//! Regression tests for Milestone 8 (WYSIWYG editing fidelity).

use super::*;

fn doc(content: &str) -> DocumentTab {
    DocumentTab::new("m8".into(), "m8.md".into(), content.into(), None, None)
}

#[test]
fn w6_toolbar_not_lit_at_outer_span_edges() {
    let mut d = doc("**bold**");
    d.place(0, false);
    assert!(!d.active_formats.bold, "outer start must not highlight");
    d.place(8, false);
    assert!(!d.active_formats.bold, "outer end must not highlight");
    d.place(2, false);
    assert!(d.active_formats.bold, "inner start highlights");
    d.place(6, false);
    assert!(d.active_formats.bold, "inner end highlights");
    d.place(4, false);
    assert!(d.active_formats.bold);
}

#[test]
fn w5_ctrl_b_at_inner_start_moves_before_opener_not_past_closer() {
    let mut d = doc("**bold**");
    d.place(2, false);
    d.apply_format_in_mode("bold", ViewMode::Wysiwyg);
    assert_eq!(d.content, "**bold**");
    assert_eq!(d.cursor_offset(), 0);
    assert!(!d.pending_formats.bold);

    let mut i = doc("*it*");
    i.place(1, false);
    i.apply_format_in_mode("italic", ViewMode::Wysiwyg);
    assert_eq!(i.content, "*it*");
    assert_eq!(i.cursor_offset(), 0);
}
