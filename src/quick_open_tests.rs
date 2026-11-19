//! The Ctrl+P pop-up: opening a note from the Notes list, and a line from
//! the Contents list.

use super::*;
use gpui::TestAppContext;

// `use gpui::*` in this crate re-exports a `test` attribute that is not
// the std one. Pin the std one so these stay ordinary unit tests.
use ::core::prelude::v1::test as std_test;

/// A temp workspace with a few notes, removed on drop.
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(tag: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("litemark-quick-open-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("projects")).unwrap();
        std::fs::write(root.join("alpha.md"), "# Alpha\n\nnothing here\n").unwrap();
        std::fs::write(
            root.join("projects").join("beta plan.md"),
            "# Beta\n\nfirst line\nthe Needle is here, and a needle again\nlast\n",
        )
        .unwrap();
        Self { root }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn app_in<'a>(
    fixture: &Fixture,
    cx: &'a mut TestAppContext,
) -> (Entity<LiteMarkApp>, &'a mut gpui::VisualTestContext) {
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, _cx| {
        app.workspace = scan_workspace_dir(&fixture.root, true).into();
        app.refresh_workspace_ids();
    });
    (view, cx)
}

#[std_test]
fn enter_on_a_notes_result_opens_that_note() {
    let fixture = Fixture::new("notes");
    let mut cx = TestAppContext::single();
    let (view, cx) = app_in(&fixture, &mut cx);
    view.update(cx, |app, cx| {
        app.open_quick_open(QuickMode::Notes, cx);
        assert!(app.input_blocked(), "the editor must not take keys");
        app.quick_open_query_changed("bplan".into(), cx);
        let open = app.quick_open.as_ref().unwrap();
        assert_eq!(open.notes.len(), 1);
        assert_eq!(open.notes[0].name, "beta plan.md");
        assert_eq!(open.notes[0].folder, "projects");

        app.quick_open_activate(None, cx);
        assert!(app.quick_open.is_none());
        assert!(!app.input_blocked());
        let tab = app.active_tab().unwrap();
        assert_eq!(
            tab.file_path.as_deref(),
            Some(fixture.root.join("projects").join("beta plan.md").as_path())
        );
        assert!(tab.content.contains("Needle"));
    });
}

#[std_test]
fn empty_query_lists_the_recent_notes() {
    let fixture = Fixture::new("recents");
    let mut cx = TestAppContext::single();
    let (view, cx) = app_in(&fixture, &mut cx);
    view.update(cx, |app, cx| {
        app.settings.recent_notes = vec![
            fixture.root.join("projects").join("beta plan.md"),
            fixture.root.join("alpha.md"),
            fixture.root.join("gone.md"),
        ];
        app.open_quick_open(QuickMode::Notes, cx);
        let names: Vec<_> = app
            .quick_open
            .as_ref()
            .unwrap()
            .notes
            .iter()
            .map(|hit| hit.name.as_str())
            .collect();
        // A recent note that no longer exists is not offered.
        assert_eq!(names, vec!["beta plan.md", "alpha.md"]);
    });
}

#[std_test]
fn switching_modes_keeps_the_query() {
    let fixture = Fixture::new("switch");
    let mut cx = TestAppContext::single();
    let (view, cx) = app_in(&fixture, &mut cx);
    view.update(cx, |app, cx| {
        app.open_quick_open(QuickMode::Notes, cx);
        app.quick_open_query_changed("alpha".into(), cx);
        app.quick_open_switch_mode(cx);
        let open = app.quick_open.as_ref().unwrap();
        assert_eq!(open.mode, QuickMode::Contents);
        assert_eq!(open.query, "alpha");
        app.quick_open_switch_mode(cx);
        assert_eq!(app.quick_open.as_ref().unwrap().mode, QuickMode::Notes);
        assert_eq!(app.quick_open.as_ref().unwrap().notes.len(), 1);
    });
}

#[std_test]
fn contents_search_runs_in_the_background_and_lists_lines() {
    let fixture = Fixture::new("contents");
    let mut cx = TestAppContext::single();
    let (view, cx) = app_in(&fixture, &mut cx);
    view.update(cx, |app, cx| {
        app.open_quick_open(QuickMode::Contents, cx);
        app.quick_open_query_changed("needle".into(), cx);
        assert!(app.quick_open.as_ref().unwrap().searching);
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(400));
    cx.run_until_parked();
    view.read_with(cx, |app, _| {
        let open = app.quick_open.as_ref().unwrap();
        assert!(!open.searching);
        assert_eq!(open.contents.len(), 1);
        assert_eq!(open.contents[0].name, "beta plan.md");
        assert_eq!(open.contents[0].line, 4);
    });
}

#[std_test]
fn a_stale_search_is_dropped_when_the_query_changes() {
    let fixture = Fixture::new("stale");
    let mut cx = TestAppContext::single();
    let (view, cx) = app_in(&fixture, &mut cx);
    view.update(cx, |app, cx| {
        app.open_quick_open(QuickMode::Contents, cx);
        app.quick_open_query_changed("needle".into(), cx);
        app.quick_open_query_changed("nothing".into(), cx);
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(400));
    cx.run_until_parked();
    view.read_with(cx, |app, _| {
        let open = app.quick_open.as_ref().unwrap();
        assert_eq!(open.contents.len(), 1);
        assert_eq!(open.contents[0].name, "alpha.md");
    });
}

#[std_test]
fn enter_on_a_contents_hit_selects_the_match_and_fills_the_find_bar() {
    let fixture = Fixture::new("hit");
    let mut cx = TestAppContext::single();
    let (view, cx) = app_in(&fixture, &mut cx);
    view.update(cx, |app, cx| {
        app.open_quick_open(QuickMode::Contents, cx);
        let path = fixture.root.join("projects").join("beta plan.md");
        let hits = search_contents(
            std::slice::from_ref(&path),
            &fixture.root,
            "needle",
            |p| std::fs::read_to_string(p).ok(),
            || false,
            3,
            10,
        );
        assert_eq!(hits.len(), 1);
        let open = app.quick_open.as_mut().unwrap();
        open.query = "needle".into();
        open.contents = hits;

        app.quick_open_activate(None, cx);

        assert!(app.quick_open.is_none());
        assert_eq!(app.find_term, "needle");
        assert!(app.show_find_replace);
        let tab = app.active_tab().unwrap();
        assert_eq!(tab.file_path.as_deref(), Some(path.as_path()));
        let start = tab.content.find("Needle").unwrap();
        assert_eq!(tab.selection_range(), start..start + "needle".len());
        assert_eq!(app.find_index, Some(0));
        assert_eq!(app.find_hits.len(), 2);

        // Enter in the find bar goes on to the next match.
        app.find_next();
        let tab = app.active_tab().unwrap();
        let second = tab.content.rfind("needle").unwrap();
        assert_eq!(tab.selection_range(), second..second + "needle".len());
        assert_eq!(app.find_index, Some(1));
    });
}

#[std_test]
fn escape_closes_the_pop_up() {
    let fixture = Fixture::new("escape");
    let mut cx = TestAppContext::single();
    let (view, cx) = app_in(&fixture, &mut cx);
    view.update(cx, |app, cx| app.open_quick_open(QuickMode::Notes, cx));
    cx.update(|window, cx| {
        view.update(cx, |app, cx| app.handle_close_modal(window, cx));
    });
    view.read_with(cx, |app, _| {
        assert!(app.quick_open.is_none());
        assert_eq!(app.field_focus, TextInput::Editor);
    });
}
