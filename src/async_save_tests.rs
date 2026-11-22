//! Ctrl+S writes on a background thread without losing or reordering edits.

use super::*;
use gpui::TestAppContext;

use ::core::prelude::v1::test as std_test;

fn fresh_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("litemark-asave-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn open(app: &mut LiteMarkApp, root: &Path, path: &Path) {
    app.workspace = scan_workspace_dir(root, true).into();
    let file = app
        .workspace
        .files
        .iter()
        .find(|file| file.path == path)
        .cloned()
        .unwrap();
    app.open_loaded_tab(
        file.name.clone(),
        read_note(&file.path).unwrap(),
        Some(file.path.clone()),
        Some(file.id.clone()),
    );
}

#[std_test]
fn ctrl_s_returns_before_the_file_is_written_and_then_saves_it() {
    let root = fresh_root("basic");
    let path = root.join("n.md");
    std::fs::write(&path, "old\n").unwrap();
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_w, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, cx| {
        open(app, &root, &path);
        app.active_tab_mut().unwrap().edit(0..0, "new ");
        app.handle_save(cx);
        // The write hasn't happened yet; the note already shows as saved.
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "old\n");
        assert!(!app.active_tab().unwrap().is_modified);
    });
    cx.run_until_parked();
    view.update(cx, |app, _| {
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new old\n");
        let id = app.active_tab_id.clone();
        assert_eq!(app.disk_change_for(&id), DiskChange::Unchanged);
        assert!(app.saves_in_flight.is_empty());
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn typing_while_saving_stays_modified_and_the_disk_version_is_the_snapshot() {
    let root = fresh_root("typing");
    let path = root.join("n.md");
    std::fs::write(&path, "a\n").unwrap();
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_w, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, cx| {
        open(app, &root, &path);
        app.active_tab_mut().unwrap().edit(0..0, "one ");
        app.handle_save(cx);
        // Typed after the snapshot was taken.
        app.active_tab_mut().unwrap().edit(0..0, "two ");
    });
    cx.run_until_parked();
    view.update(cx, |app, cx| {
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "one a\n");
        assert!(
            app.active_tab().unwrap().is_modified,
            "later edit is unsaved"
        );
        // The recorded version is the file as written, so the next save
        // sees no outside change.
        let id = app.active_tab_id.clone();
        assert_eq!(app.disk_change_for(&id), DiskChange::Unchanged);
        app.handle_save(cx);
    });
    cx.run_until_parked();
    view.update(cx, |_, _| {
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "two one a\n");
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn a_second_ctrl_s_while_saving_writes_after_the_first() {
    let root = fresh_root("twice");
    let path = root.join("n.md");
    std::fs::write(&path, "a\n").unwrap();
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_w, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, cx| {
        open(app, &root, &path);
        app.active_tab_mut().unwrap().edit(0..0, "1 ");
        app.handle_save(cx);
        app.active_tab_mut().unwrap().edit(0..0, "2 ");
        app.handle_save(cx); // queued behind the first
        let id = app.active_tab_id.clone();
        // A synchronous save of the same note refuses rather than race it.
        assert!(app.save_tab(&id).is_err());
    });
    cx.run_until_parked();
    view.update(cx, |app, _| {
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "2 1 a\n");
        assert!(!app.active_tab().unwrap().is_modified);
        assert!(app.saves_in_flight.is_empty() && app.resave_after.is_empty());
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn closing_the_window_waits_for_the_write() {
    let root = fresh_root("close");
    let path = root.join("n.md");
    std::fs::write(&path, "a\n").unwrap();
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_w, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, cx| {
        open(app, &root, &path);
        app.active_tab_mut().unwrap().edit(0..0, "x ");
        app.handle_save(cx);
        app.request_close_window();
        assert!(!app.allow_window_close, "must not close mid-write");
    });
    cx.run_until_parked();
    view.update(cx, |app, _| {
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "x a\n");
        assert!(app.allow_window_close, "closes once the write is done");
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn a_small_note_opens_at_once() {
    let root = fresh_root("readsmall");
    let path = root.join("n.md");
    std::fs::write(&path, "small\n").unwrap();
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_w, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, cx| {
        app.open_path(path.clone(), cx);
        assert_eq!(app.active_tab().unwrap().content, "small\n");
        assert!(app.note_loading.is_none());
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn a_big_note_opens_in_the_background_and_a_second_click_is_ignored() {
    let root = fresh_root("readbig");
    let path = root.join("big.md");
    let other = root.join("other.md");
    std::fs::write(&path, "big note\n").unwrap();
    std::fs::write(&other, "other\n").unwrap();
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_w, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, cx| {
        app.async_read_bytes = 0; // every note counts as big
        let before = app.tabs.len();
        app.open_path(path.clone(), cx);
        assert_eq!(app.tabs.len(), before, "not opened yet");
        assert_eq!(app.note_loading.as_deref(), Some(path.as_path()));
        // Clicking another while one loads is refused, not queued.
        app.open_path(other.clone(), cx);
        assert_eq!(app.note_loading.as_deref(), Some(path.as_path()));
    });
    cx.run_until_parked();
    view.update(cx, |app, _| {
        assert!(app.note_loading.is_none());
        assert_eq!(app.active_tab().unwrap().content, "big note\n");
        assert!(app
            .tabs
            .iter()
            .all(|t| t.file_path.as_deref() != Some(other.as_path())));
    });
    let _ = std::fs::remove_dir_all(&root);
}
