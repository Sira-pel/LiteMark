//! The editor outline: which tabs show it, and jumping to a heading.

use super::*;
use gpui::{Entity, TestAppContext, VisualTestContext};

// `use gpui::*` in this crate re-exports a `test` attribute that is not
// Rust's `#[test]`. Pin the std one so these stay ordinary unit tests.
use ::core::prelude::v1::test as std_test;

const NOTE: &str = "# Title\n\nintro\n\n## Part\n\nmore text\n";

fn harness(content: &str) -> (Entity<LiteMarkApp>, VisualTestContext) {
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| {
        let mut app = LiteMarkApp::test_new(content, cx);
        // Never touch the real settings file from a test.
        app.settings.save_blocked = Some("test".into());
        app
    });
    (view, cx.clone())
}

#[std_test]
fn jumping_to_a_heading_moves_the_caret_there() {
    let (view, mut cx) = harness(NOTE);
    let target = NOTE.find("## Part").unwrap();
    view.update(&mut cx, |app, _cx| {
        assert!(app.outline_visible());
        assert_eq!(app.active_tab().unwrap().cursor_offset(), 0);
        app.field_focus = TextInput::Find;
        app.jump_to_heading(target);
        let tab = app.active_tab().unwrap();
        assert_eq!(tab.cursor_offset(), target);
        assert!(tab.selection_range().is_empty());
        assert_eq!(app.field_focus, TextInput::Editor, "keys go to the editor");
    });
}

#[std_test]
fn a_jump_scrolls_the_heading_near_the_top() {
    let mut text = String::new();
    for i in 0..60 {
        text.push_str(&format!("# Section {i}\n\n"));
        for _ in 0..12 {
            text.push_str("body line of text\n\n");
        }
    }
    let (view, mut cx) = harness(&text);
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let target = text.find("# Section 40").unwrap();
    view.update(&mut cx, |app, _cx| app.jump_to_heading(target));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    view.read_with(&cx, |app, _| {
        assert!(app.editor_scroll.offset().y < px(0.0), "the view scrolled");
        let paint = app.editor_paint.borrow();
        let row = paint
            .rows
            .iter()
            .rev()
            .find(|r| r.byte_start <= target)
            .unwrap();
        let view_top = app.editor_scroll.bounds().top();
        let from_top = row.origin.y - view_top;
        assert!(
            from_top >= px(0.0) && from_top < px(100.0),
            "heading sits near the top: {from_top:?}"
        );
    });
}

#[std_test]
fn the_outline_is_hidden_for_plain_text_and_without_headings() {
    let (view, mut cx) = harness(NOTE);
    view.update(&mut cx, |app, _cx| {
        assert!(app.outline_visible(), "a Markdown note with headings");

        app.open_loaded_tab("notes.txt".into(), NOTE.into(), None, None);
        assert!(!app.outline_visible(), "plain text never shows it");

        app.open_loaded_tab("plain.md".into(), "no headings\n".into(), None, None);
        assert!(!app.outline_visible(), "no headings, no outline");

        app.open_loaded_tab("main.rs".into(), "# not a heading\n".into(), None, None);
        assert!(!app.outline_visible(), "code is not Markdown");
    });
}

#[std_test]
fn zen_mode_hides_the_outline() {
    let (view, mut cx) = harness(NOTE);
    view.update(&mut cx, |app, _cx| {
        assert!(app.outline_visible());
        app.zen_mode = true;
        assert!(!app.outline_visible());
    });
}

#[std_test]
fn the_outline_follows_edits() {
    let (view, mut cx) = harness("text\n");
    view.update(&mut cx, |app, _cx| {
        assert!(!app.outline_visible());
        let tab = app.active_tab_mut().unwrap();
        let end = tab.content.len();
        tab.edit(end..end, "# New\n");
        assert!(app.outline_visible());
    });
}

#[std_test]
fn pinning_toggles_and_the_card_opens_on_hover() {
    let (view, mut cx) = harness(NOTE);
    view.update(&mut cx, |app, _cx| {
        assert!(!app.outline_expanded());
        app.outline_hover = true;
        assert!(app.outline_expanded());
        app.outline_hover = false;
        app.handle_toggle_outline();
        assert!(app.settings.outline_pinned && app.outline_expanded());
        app.handle_toggle_outline();
        assert!(!app.settings.outline_pinned);
    });
}
