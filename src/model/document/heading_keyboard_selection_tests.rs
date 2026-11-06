use super::*;

#[test]
fn shift_arrow_selection_in_a_heading_bolds_only_the_selection() {
    let mode = ViewMode::Wysiwyg;
    let src = "# Welcome to LiteMark\n\nBody\n";
    let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.to_string(), None, None);
    doc.view_mode = mode;
    doc.caret.range = 0..0;
    doc.move_home_in_mode(false, mode);
    for _ in 0..3 {
        doc.move_right_in_mode(false, mode);
    }
    for _ in 0..4 {
        doc.move_right_in_mode(true, mode);
    }
    doc.apply_format_in_mode("bold", mode);
    assert_eq!(doc.content, "# Wel**come** to LiteMark\n\nBody\n");
}
