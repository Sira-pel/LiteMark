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

#[std_test]
fn wysiwyg_prefix_guard_uses_parsed_context_not_isolated_line() {
    let mut cx = TestAppContext::single();
    let fenced = "```\n# comment\n```";
    let (view, cx) = cx.add_window_view(|_window, cx| {
        let mut app = LiteMarkApp::test_new(fenced, cx);
        app.settings.view_mode = ViewMode::Wysiwyg;
        app.active_tab_mut().unwrap().place(4, false);
        app
    });
    let mut cx = cx.clone();
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.replace_text_in_range(None, "x", window, cx);
        });
    });
    let after = view.read_with(&cx, |app, _| app.active_tab().unwrap().content.clone());
    assert_eq!(after, "```\nx# comment\n```");

    let tab_heading = "#\tHeading";
    let (view2, cx2) = cx_new_wysiwyg(tab_heading, 0);
    let mut cx2 = cx2;
    cx2.update(|window, cx| {
        view2.update(cx, |app, cx| {
            app.replace_text_in_range(None, "x", window, cx);
        });
    });
    let after2 = view2.read_with(&cx2, |app, _| app.active_tab().unwrap().content.clone());
    assert_eq!(after2, "#\txHeading");
}

fn cx_new_wysiwyg(content: &str, cursor: usize) -> (Entity<LiteMarkApp>, gpui::VisualTestContext) {
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| {
        let mut app = LiteMarkApp::test_new(content, cx);
        app.settings.view_mode = ViewMode::Wysiwyg;
        app.active_tab_mut().unwrap().place(cursor, false);
        app
    });
    (view, cx.clone())
}

#[std_test]
fn wysiwyg_overtype_and_autowrap_do_not_corrupt_hidden_delimiters() {
    // Typing `]` at `[hello|](url)` must not step into hidden `](url)`.
    let link = "[hello](url)";
    let (view, mut cx) = cx_new_wysiwyg(link, 6);
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            app.replace_text_in_range(None, "]", window, cx);
        });
    });
    view.read_with(&cx, |app, _| {
        let tab = app.active_tab().unwrap();
        assert_eq!(tab.content, "[hello]](url)");
        assert_eq!(tab.cursor_offset(), 7);
    });

    // Typing `` ` `` at `` ``foo|`` `` must re-fence rather than split `` `` ``.
    let code = "``foo``";
    let (view2, mut cx2) = cx_new_wysiwyg(code, 5);
    cx2.update(|window, cx| {
        view2.update(cx, |app, cx| {
            app.replace_text_in_range(None, "`", window, cx);
        });
    });
    view2.read_with(&cx2, |app, _| {
        let tab = app.active_tab().unwrap();
        assert_ne!(tab.content, "``foo``");
        assert_eq!(
            crate::markdown::parse_document(&tab.content).lines[0].display_text,
            "foo`"
        );
    });

    // Auto-wrapping a selection spanning `**` widens to cover the whole bold
    // span and never deletes selected text.
    let md = "**Hello** world";
    let mut cx3 = TestAppContext::single();
    let (view3, cx3) = cx3.add_window_view(|_window, cx| {
        let mut app = LiteMarkApp::test_new(md, cx);
        app.settings.view_mode = ViewMode::Wysiwyg;
        let tab = app.active_tab_mut().unwrap();
        tab.caret.range = 4..12; // "llo** wo"
        app
    });
    let mut cx3 = cx3.clone();
    cx3.update(|window, cx| {
        view3.update(cx, |app, cx| {
            app.replace_text_in_range(None, "(", window, cx);
        });
    });
    let after3 = view3.read_with(&cx3, |app, _| app.active_tab().unwrap().content.clone());
    assert_eq!(after3, "(**Hello** wo)rld");

    // Auto-wrapping across a newline and bold text also wraps without deleting.
    let multi = "**Hello**\nworld";
    let mut cx4 = TestAppContext::single();
    let (view4, cx4) = cx4.add_window_view(|_window, cx| {
        let mut app = LiteMarkApp::test_new(multi, cx);
        app.settings.view_mode = ViewMode::Wysiwyg;
        let tab = app.active_tab_mut().unwrap();
        tab.caret.range = 4..multi.len();
        app
    });
    let mut cx4 = cx4.clone();
    cx4.update(|window, cx| {
        view4.update(cx, |app, cx| {
            app.replace_text_in_range(None, "\"", window, cx);
        });
    });
    let after4 = view4.read_with(&cx4, |app, _| app.active_tab().unwrap().content.clone());
    assert_eq!(after4, "\"**Hello**\nworld\"");

    // `(` at the end of a bold word (`**bold|**`) still auto-pairs `()`.
    let (view5, mut cx5) = cx_new_wysiwyg("**bold**", 6);
    cx5.update(|window, cx| {
        view5.update(cx, |app, cx| {
            app.replace_text_in_range(None, "(", window, cx);
        });
    });
    let after5 = view5.read_with(&cx5, |app, _| app.active_tab().unwrap().content.clone());
    assert_eq!(after5, "**bold()**");
}

#[std_test]
#[allow(clippy::reversed_empty_ranges)]
fn entity_input_handler_handles_reversed_utf16_ranges() {
    let (view, mut cx) = cx_new_wysiwyg("hello world", 0);
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            let mut adjusted = None;
            let got = app.text_for_range(5..1, &mut adjusted, window, cx);
            assert_eq!(got.as_deref(), Some("ello"));
            assert_eq!(adjusted, Some(1..5));
            let _ = app.bounds_for_range(5..1, Bounds::default(), window, cx);
            app.replace_text_in_range(Some(5..1), "X", window, cx);
        });
    });
    let after = view.read_with(&cx, |app, _| app.active_tab().unwrap().content.clone());
    assert_eq!(after, "hX world");
}

#[std_test]
fn task_checkbox_click_handles_tab_checkbox() {
    assert_eq!(
        crate::markdown::parser::checkbox_click_replacement("[\t]"),
        "[x]"
    );
    assert_eq!(
        crate::markdown::parser::checkbox_click_replacement("[x]"),
        "[ ]"
    );
}

#[std_test]
fn wrapped_wysiwyg_sub_maps_include_leading_trailing_and_split_delimiters() {
    let md = "# **AlphaBetaGammaDelta** **EpsilonZetaEtaThetaIotaKappaLambda**";
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|window, cx| {
        window.resize(size(px(280.0), px(400.0)));
        let mut app = LiteMarkApp::test_new(md, cx);
        app.settings.view_mode = ViewMode::Wysiwyg;
        app.show_sidebar = false;
        app
    });
    let mut cx = cx.clone();
    cx.update(|window, _cx| window.refresh());
    cx.run_until_parked();

    view.read_with(&cx, |app, _| {
        let paint = app.editor_paint.borrow();
        let rows: Vec<_> = paint.rows.iter().filter(|r| r.line_index == 0).collect();
        assert!(rows.len() >= 2, "expected wrapped heading rows");
        assert_eq!(rows.first().unwrap().byte_start, 0);
        assert_eq!(rows.last().unwrap().byte_end, md.len());
        for pair in rows.windows(2) {
            assert_eq!(pair[0].byte_end, pair[1].byte_start);
        }
    });
}

#[std_test]
fn raw_mode_ime_marked_range_does_not_leak_onto_earlier_tab_line() {
    let content = "\tfoo\nbar";
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| {
        let mut app = LiteMarkApp::test_new(content, cx);
        app.settings.view_mode = ViewMode::Raw;
        app.active_tab_mut().unwrap().marked_range = Some(5..8);
        app
    });
    let mut cx = cx.clone();
    cx.update(|window, _cx| window.refresh());
    cx.run_until_parked();

    view.read_with(&cx, |app, _| {
        let paint = app.editor_paint.borrow();
        assert!(paint.rows.len() >= 2);
        let row0 = &paint.rows[0];
        // Tab-expanded row 0 has 1 unsplit run when no IME underline leaks onto it.
        assert_eq!(row0.shaped.runs.len(), 1);
    });
}
