//! Opening and closing the find panel.

use super::*;
use gpui::TestAppContext;

// gpui's prelude brings its own `test` attribute into scope, which shadows
// Rust's `#[test]`. Pin the std one so these stay ordinary unit tests.
use ::core::prelude::v1::test as std_test;

#[std_test]
fn ctrl_f_opens_find_without_the_replace_row() {
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("text", cx));
    view.update(cx, |app, _cx| {
        // Ctrl+H showed the replace row, then the panel was closed.
        app.show_find_replace = true;
        app.show_replace_row = true;
        app.toggle_find_panel();
        assert!(!app.show_find_replace);

        app.toggle_find_panel();
        assert!(app.show_find_replace);
        assert!(!app.show_replace_row, "Ctrl+F shows only the find row");
        assert!(app.pending_find_focus);
    });
}
