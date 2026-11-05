//! Typed characters as the window delivers them.

use super::*;
use gpui::TestAppContext;

use ::core::prelude::v1::test as std_test;

fn typed(input: &str) -> String {
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    let mut cx = cx.clone();
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            for ch in input.chars() {
                app.replace_text_in_range(None, &ch.to_string(), window, cx);
            }
        });
    });
    view.read_with(&cx, |app, _| app.active_tab().unwrap().content.clone())
}

#[std_test]
fn three_backticks_start_a_fence_without_a_stray_closer() {
    assert_eq!(typed("```"), "```");
}

#[std_test]
fn inline_code_still_auto_closes() {
    assert_eq!(typed("`a`"), "`a`");
    assert_eq!(typed("`"), "``");
}
