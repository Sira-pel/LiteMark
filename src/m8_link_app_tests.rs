//! Ctrl+K and Ctrl+V with a URL on the clipboard (Milestone 8, W7).

use super::*;
use gpui::{ClipboardItem, Entity, TestAppContext, VisualTestContext};

// `use gpui::*` in this crate re-exports a `test` attribute that is not
// Rust's `#[test]`. Pin the std one so these stay ordinary unit tests.
use ::core::prelude::v1::test as std_test;

const NOTE: &str = "see docs now\n";

fn harness() -> (Entity<LiteMarkApp>, VisualTestContext) {
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| {
        let mut app = LiteMarkApp::test_new(NOTE, cx);
        let tab = app.active_tab_mut().expect("tab");
        tab.caret.range = 4..8;
        tab.caret.reversed = false;
        app
    });
    (view, cx.clone())
}

fn content(view: &Entity<LiteMarkApp>, cx: &VisualTestContext) -> String {
    view.read_with(cx, |app, _| app.active_tab().expect("tab").content.clone())
}

#[std_test]
fn ctrl_k_fills_the_clipboard_url_as_the_destination() {
    let (view, mut cx) = harness();
    cx.write_to_clipboard(ClipboardItem::new_string("https://example.com/a".into()));
    view.update(&mut cx, |app, cx| app.handle_insert_link(cx));
    assert_eq!(
        content(&view, &cx),
        "see [docs](https://example.com/a) now\n"
    );
}

#[std_test]
fn ctrl_k_without_a_url_keeps_the_placeholder() {
    let (view, mut cx) = harness();
    cx.write_to_clipboard(ClipboardItem::new_string("just words".into()));
    view.update(&mut cx, |app, cx| app.handle_insert_link(cx));
    assert_eq!(content(&view, &cx), "see [docs](https://) now\n");
}

#[std_test]
fn ctrl_v_of_a_url_over_a_selection_links_it() {
    let (view, mut cx) = harness();
    cx.write_to_clipboard(ClipboardItem::new_string("https://example.com/a".into()));
    view.update(&mut cx, |app, cx| app.handle_paste(cx));
    assert_eq!(
        content(&view, &cx),
        "see [docs](https://example.com/a) now\n"
    );
}
