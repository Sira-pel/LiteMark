//! Reopening last session's tabs at startup, and the recent-notes list.

use super::*;
use gpui::TestAppContext;

// `use gpui::*` in this crate re-exports a `test` attribute that is not
// Rust's `#[test]`. Pin the std one so these stay ordinary unit tests.
use ::core::prelude::v1::test as std_test;

fn temp_workspace(tag: &str, notes: &[&str]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("litemark-restore-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    for name in notes {
        std::fs::write(root.join(name), format!("# {name}\n")).unwrap();
    }
    root
}

fn use_workspace(app: &mut LiteMarkApp, root: &Path) {
    app.workspace = scan_workspace_dir(root, true).into();
    app.refresh_workspace_ids();
}

fn tab_paths(app: &LiteMarkApp) -> Vec<PathBuf> {
    app.tabs
        .iter()
        .filter_map(|t| t.file_path.clone())
        .collect()
}

#[std_test]
fn restore_skips_a_deleted_note_and_restores_the_active_tab() {
    let root = temp_workspace("skip", &["a.md", "b.md", "c.md"]);
    let (a, b, c) = (root.join("a.md"), root.join("b.md"), root.join("c.md"));
    std::fs::remove_file(&b).unwrap();

    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, _cx| {
        use_workspace(app, &root);
        app.settings.open_tabs = vec![a.clone(), b.clone(), c.clone()];
        app.settings.active_tab = Some(c.clone());

        assert_eq!(app.restore_saved_tabs(false), 2);
        assert_eq!(tab_paths(app), [a.clone(), c.clone()]);
        let active = app.active_tab().unwrap();
        assert_eq!(active.file_path.as_deref(), Some(c.as_path()));
        assert_eq!(active.content, "# c.md\n");
        // The sidebar selects the active note, as opening it would.
        let c_id = app
            .workspace
            .files
            .iter()
            .find(|f| f.path == c)
            .unwrap()
            .id
            .clone();
        assert_eq!(app.selected_target, Some(SidebarTarget::File(c_id)));
    });

    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn nothing_to_restore_leaves_the_startup_tab_alone() {
    let root = temp_workspace("none", &[]);
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("hello", cx));
    view.update(cx, |app, _cx| {
        use_workspace(app, &root);
        app.settings.open_tabs = vec![root.join("gone.md")];
        let before = app.active_tab_id.clone();
        assert_eq!(app.restore_saved_tabs(false), 0);
        assert_eq!(app.tabs.len(), 1);
        assert_eq!(app.active_tab_id, before);
        assert_eq!(app.active_tab().unwrap().content, "hello");
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn setting_off_restores_nothing() {
    let root = temp_workspace("off", &["a.md"]);
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, _cx| {
        use_workspace(app, &root);
        app.settings.reopen_tabs_on_startup = false;
        app.settings.open_tabs = vec![root.join("a.md")];
        app.settings.active_tab = Some(root.join("a.md"));
        assert_eq!(app.restore_saved_tabs(false), 0);
        assert_eq!(app.tabs.len(), 1);
        assert_eq!(app.active_tab().unwrap().file_path, None);
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn command_line_file_stays_active_after_the_restored_tabs() {
    let root = temp_workspace("cmdline", &["a.md", "b.md"]);
    let (a, b) = (root.join("a.md"), root.join("b.md"));
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, _cx| {
        use_workspace(app, &root);
        // Stand-in for the tab `new` opens from the command line.
        app.tabs[0].file_path = Some(b.clone());
        let cmdline = app.active_tab_id.clone();
        app.settings.open_tabs = vec![b.clone(), a.clone()];
        app.settings.active_tab = Some(a.clone());

        assert_eq!(app.restore_saved_tabs(true), 1, "b is already open");
        assert_eq!(tab_paths(app), [a.clone(), b.clone()]);
        assert_eq!(app.active_tab_id, cmdline);
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn switching_tabs_updates_the_saved_list_and_recent_notes() {
    let root = temp_workspace("track", &["a.md", "b.md"]);
    let (a, b) = (root.join("a.md"), root.join("b.md"));
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, _cx| {
        use_workspace(app, &root);
        app.open_loaded_tab("a.md".into(), "# a.md\n".into(), Some(a.clone()), None);
        app.sync_tab_settings();
        app.open_loaded_tab("b.md".into(), "# b.md\n".into(), Some(b.clone()), None);
        app.sync_tab_settings();
        // The untitled test tab is not saved.
        assert_eq!(app.settings.open_tabs, [a.clone(), b.clone()]);
        assert_eq!(app.settings.active_tab, Some(b.clone()));
        assert_eq!(app.settings.recent_notes, [b.clone(), a.clone()]);

        app.cycle_tab(1); // wraps to the untitled tab
        app.cycle_tab(1); // then a
        app.sync_tab_settings();
        assert_eq!(app.settings.active_tab, Some(a.clone()));
        assert_eq!(app.settings.recent_notes, [a.clone(), b.clone()]);

        // An edit doesn't count as a change.
        app.active_tab_mut().unwrap().content.push('x');
        assert!(!app.sync_tab_settings());

        // Clearing the list isn't undone by the next frame.
        app.clear_recent_notes();
        app.sync_tab_settings();
        assert!(app.settings.recent_notes.is_empty());
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn a_rename_updates_open_tabs_and_recent_notes() {
    let root = temp_workspace("rename", &["a.md", "b.md"]);
    let (a, b) = (root.join("a.md"), root.join("b.md"));
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, _cx| {
        use_workspace(app, &root);
        app.open_loaded_tab("b.md".into(), "# b.md\n".into(), Some(b.clone()), None);
        app.sync_tab_settings();
        app.open_loaded_tab("a.md".into(), "# a.md\n".into(), Some(a.clone()), None);
        app.sync_tab_settings();
        assert_eq!(app.settings.recent_notes, [a.clone(), b.clone()]);

        let renamed = root.join("renamed.md");
        std::fs::rename(&a, &renamed).unwrap();
        app.retarget_open_tabs(&a, &renamed);

        assert_eq!(app.settings.open_tabs, [b.clone(), renamed.clone()]);
        assert_eq!(app.settings.active_tab, Some(renamed.clone()));
        assert_eq!(app.settings.recent_notes, [renamed.clone(), b.clone()]);
        // Following the rename doesn't count as opening a new note.
        assert!(!app.sync_tab_settings());
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn the_recent_menu_drops_missing_notes() {
    let root = temp_workspace("missing", &["a.md", "b.md"]);
    let (a, b) = (root.join("a.md"), root.join("b.md"));
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, _cx| {
        use_workspace(app, &root);
        app.settings.recent_notes = vec![a.clone(), b.clone()];
        std::fs::remove_file(&a).unwrap();
        assert_eq!(app.recent_notes_for_menu(), std::slice::from_ref(&b));
        assert_eq!(app.settings.recent_notes, std::slice::from_ref(&b));
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn tab_switches_are_written_once_after_a_pause_or_on_close() {
    let root = temp_workspace("batch", &["a.md", "b.md"]);
    let (a, b) = (root.join("a.md"), root.join("b.md"));
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, cx| {
        use_workspace(app, &root);
        app.open_loaded_tab("a.md".into(), "# a.md\n".into(), Some(a.clone()), None);
        app.sync_tab_settings();
        assert!(app.tab_settings_dirty, "marked, not written yet");
        app.schedule_tab_settings_save(cx);
        assert!(app.tab_settings_save_pending);
        app.open_loaded_tab("b.md".into(), "# b.md\n".into(), Some(b.clone()), None);
        app.sync_tab_settings();
        // A second switch joins the write already scheduled.
        app.schedule_tab_settings_save(cx);
        assert!(app.tab_settings_dirty);
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(2));
    cx.run_until_parked();
    view.update(cx, |app, _cx| {
        assert!(!app.tab_settings_dirty, "written after the pause");
        assert!(!app.tab_settings_save_pending);
        assert_eq!(app.settings.open_tabs, [a.clone(), b.clone()]);

        // Closing the window writes what is pending straight away.
        app.open_loaded_tab("a.md".into(), "# a.md\n".into(), Some(a.clone()), None);
        app.flush_tab_settings();
        assert!(!app.tab_settings_dirty);
        assert_eq!(app.settings.active_tab, Some(a.clone()));
    });
    let _ = std::fs::remove_dir_all(&root);
}
