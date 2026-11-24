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
            .find(|r| r.byte_start() <= target)
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

#[std_test]
fn activate_tab_releases_outgoing_caches_before_switching() {
    let (view, mut cx) = harness("# First\n| a | b |\n| - | - |\n| 1 | 2 |\n");
    let first_id = view.read_with(&cx, |app, _| app.active_tab_id.clone());

    view.update(&mut cx, |app, _cx| {
        app.open_loaded_tab("second.md".into(), "# Second\n".into(), None, None);
    });
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();

    view.update(&mut cx, |app, _| {
        assert!(app.active_tab().unwrap().has_cached_parse());
        assert!(!app.editor_paint.borrow().rows.is_empty());
        assert!(app.outline_cache.is_some());

        app.activate_tab(first_id.clone());
        let outgoing = app.tabs.iter().find(|t| t.id != first_id).unwrap();
        assert!(
            !outgoing.has_cached_parse(),
            "outgoing tab parse cache must be freed on switch"
        );
        assert!(app.editor_paint.borrow().rows.is_empty());
        assert!(app.outline_cache.is_none());
    });
}

#[std_test]
fn expanded_outline_with_many_headings_renders_with_uniform_list() {
    let doc: String = (0..5_000).map(|i| format!("# Heading {i}\n")).collect();
    let (view, mut cx) = harness(&doc);
    view.update(&mut cx, |app, cx| {
        app.outline_open = true;
        cx.notify();
    });
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    view.read_with(&cx, |app, _| {
        assert!(app.outline_expanded());
        assert_eq!(
            app.outline_cache.as_ref().map(|c| c.items.len()),
            Some(5_000)
        );
    });
}

#[std_test]
fn raw_mode_outline_and_render_never_trigger_full_parse() {
    let (view, mut cx) = harness("# First\n\nbody\n\n## Second\n");
    view.update(&mut cx, |app, cx| {
        app.handle_set_view_mode(ViewMode::Raw);
        app.active_tab_mut().unwrap().full_parse_count = 0;
        cx.notify();
    });
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    view.update(&mut cx, |app, cx| {
        let tab = app.active_tab_mut().unwrap();
        assert_eq!(tab.full_parse_count, 0);
        assert!(!tab.has_cached_parse());
        let pos = tab.content.len();
        tab.edit(pos..pos, "\n### Third\n");
        cx.notify();
    });
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    view.read_with(&cx, |app, _| {
        let tab = app.active_tab().unwrap();
        assert_eq!(tab.full_parse_count, 0);
        assert!(!tab.has_cached_parse());
        let cache = app.outline_cache.as_ref().unwrap();
        assert!(cache.parse_gen.is_none());
        assert_eq!(cache.items.len(), 3);
        assert_eq!(cache.items[2].text, "Third");
    });
}

#[std_test]
fn wysiwyg_paragraph_edit_patches_outline_cache_incrementally() {
    let (view, mut cx) = harness("# First\n\nbody\n\n## Second\n");
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    view.update(&mut cx, |app, cx| {
        let before_gen = app.outline_cache.as_ref().and_then(|c| c.parse_gen);
        assert!(before_gen.is_some());
        let tab = app.active_tab_mut().unwrap();
        let pos = tab.content.find("body").unwrap();
        tab.edit(pos..pos, "more ");
        cx.notify();
    });
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    view.read_with(&cx, |app, _| {
        let cache = app.outline_cache.as_ref().unwrap();
        assert_eq!(cache.items.len(), 2);
        assert_eq!(
            cache.items[1].offset,
            app.active_tab().unwrap().content.find("## Second").unwrap()
        );
    });
}
