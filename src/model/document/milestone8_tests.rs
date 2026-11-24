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
