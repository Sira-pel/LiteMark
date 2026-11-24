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
fn big_notes_open_in_the_background_one_after_another() {
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
        // A second note arriving meanwhile waits its turn; it isn't dropped.
        app.open_path(other.clone(), cx);
        assert_eq!(app.read_queue.len(), 1);
    });
    cx.run_until_parked();
    view.update(cx, |app, _| {
        assert!(app.note_loading.is_none() && app.read_queue.is_empty());
        let open: Vec<_> = app
            .tabs
            .iter()
            .filter_map(|t| t.file_path.clone())
            .collect();
        assert!(open.contains(&path) && open.contains(&other), "{open:?}");
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn reopening_a_modified_note_does_not_read_its_file() {
    let root = fresh_root("readmod");
    let path = root.join("n.md");
    std::fs::write(&path, "disk\n").unwrap();
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_w, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, cx| {
        app.open_path(path.clone(), cx);
        app.active_tab_mut().unwrap().edit(0..0, "mine ");
        app.async_read_bytes = 0;
        app.open_path(path.clone(), cx);
        // No background read, and the unsaved edit is kept.
        assert!(app.note_loading.is_none());
        assert_eq!(app.active_tab().unwrap().content, "mine disk\n");
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn moving_or_trashing_a_note_that_is_still_saving_is_refused() {
    let root = fresh_root("busy");
    std::fs::create_dir_all(root.join("dest")).unwrap();
    let path = root.join("n.md");
    std::fs::write(&path, "a\n").unwrap();
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_w, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, cx| {
        open(app, &root, &path);
        app.refresh_workspace_ids();
        app.active_tab_mut().unwrap().edit(0..0, "x ");
        app.handle_save(cx);
        let file_id = app
            .workspace
            .files
            .iter()
            .find(|f| f.path == path)
            .unwrap()
            .id
            .clone();
        let dest_id = app.workspace.folders[0].id.clone();
        let outcome = app.move_or_copy_note(&file_id, Some(dest_id.to_string()), false, false);
        assert!(matches!(outcome, DropOutcome::Failed(_)), "move must wait");
        app.trash_file(&file_id, cx);
        // Still listed and open: the trash was refused, not started.
        assert!(app.workspace.files.iter().any(|f| f.path == path));
    });
    cx.run_until_parked();
    // The save landed at the original path and nothing recreated or moved it.
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "x a\n");
    assert!(!root.join("dest").join("n.md").exists());
    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn a_tab_closed_mid_save_closes_after_the_write_lands() {
    let root = fresh_root("closetab");
    let path = root.join("n.md");
    let other = root.join("o.md");
    std::fs::write(&path, "a\n").unwrap();
    std::fs::write(&other, "o\n").unwrap();
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_w, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, cx| {
        open(app, &root, &other);
        open(app, &root, &path);
        app.active_tab_mut().unwrap().edit(0..0, "x ");
        app.handle_save(cx);
        let id = app.active_tab_id.clone();
        app.handle_close_tab(id.clone());
        assert!(
            app.tabs.iter().any(|t| t.id == id),
            "kept until the write lands"
        );
    });
    cx.run_until_parked();
    view.update(cx, |app, _| {
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "x a\n");
        assert!(app
            .tabs
            .iter()
            .all(|t| t.file_path.as_deref() != Some(path.as_path())));
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn a_tab_closed_mid_save_still_closes_after_a_second_ctrl_s() {
    let root = fresh_root("closeresave");
    let path = root.join("n.md");
    let other = root.join("o.md");
    std::fs::write(&path, "a\n").unwrap();
    std::fs::write(&other, "o\n").unwrap();
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_w, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, cx| {
        open(app, &root, &other);
        open(app, &root, &path);
        app.active_tab_mut().unwrap().edit(0..0, "x ");
        app.handle_save(cx);
        let id = app.active_tab_id.clone();
        app.handle_close_tab(id);
        app.handle_save(cx);
    });
    cx.run_until_parked();
    view.update(cx, |app, _| {
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "x a\n");
        assert!(app
            .tabs
            .iter()
            .all(|t| t.file_path.as_deref() != Some(path.as_path())));
        assert!(app.close_after_save.is_empty());
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn a_tab_closed_mid_save_stays_open_when_the_save_conflicts() {
    let root = fresh_root("closeconflict");
    let path = root.join("n.md");
    let other = root.join("o.md");
    std::fs::write(&path, "a\n").unwrap();
    std::fs::write(&other, "o\n").unwrap();
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_w, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, cx| {
        open(app, &root, &other);
        open(app, &root, &path);
        app.active_tab_mut().unwrap().edit(0..0, "x ");
        app.handle_save(cx);
        let id = app.active_tab_id.clone();
        app.handle_close_tab(id);
        // Someone else changes the file before the background check runs.
        std::fs::write(&path, "theirs\n").unwrap();
    });
    cx.run_until_parked();
    view.update(cx, |app, _| {
        let tab = app
            .tabs
            .iter()
            .find(|t| t.file_path.as_deref() == Some(path.as_path()));
        let tab = tab.expect("the unsaved note must not be closed");
        assert!(tab.is_modified);
        assert_eq!(tab.content, "x a\n");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "theirs\n");
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn reopening_a_note_that_is_saving_keeps_the_text_being_saved() {
    let root = fresh_root("reopen");
    let path = root.join("n.md");
    std::fs::write(&path, "old\n").unwrap();
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_w, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, cx| {
        open(app, &root, &path);
        app.active_tab_mut().unwrap().edit(0..0, "new ");
        app.handle_save(cx);
        // A sidebar click while the write is in flight reads the old file.
        app.open_path(path.clone(), cx);
        assert_eq!(app.active_tab().unwrap().content, "new old\n");
    });
    cx.run_until_parked();
    view.update(cx, |app, _| {
        assert_eq!(app.active_tab().unwrap().content, "new old\n");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new old\n");
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn a_big_note_removed_while_loading_is_not_opened() {
    let root = fresh_root("readgone");
    let path = root.join("big.md");
    std::fs::write(&path, "big\n").unwrap();
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_w, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, cx| {
        app.async_read_bytes = 0;
        app.open_path(path.clone(), cx);
        // Renamed or trashed while the read is queued.
        std::fs::rename(&path, root.join("moved.md")).unwrap();
    });
    cx.run_until_parked();
    view.update(cx, |app, _| {
        assert!(app
            .tabs
            .iter()
            .all(|t| t.file_path.as_deref() != Some(path.as_path())));
        assert!(app.error_message.is_some());
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn trashing_a_note_drops_a_scan_that_started_before_it() {
    let root = fresh_root("trashscan");
    let path = root.join("n.md");
    std::fs::write(&path, "a\n").unwrap();
    std::fs::write(root.join("keep.md"), "k\n").unwrap();
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_w, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, cx| {
        app.workspace = scan_workspace_dir(&root, true).into();
        app.refresh_workspace_ids();
        app.request_rescan();
        app.start_requested_rescan(cx);
        let before = app.scan_generation;
        let id = app
            .workspace
            .files
            .iter()
            .find(|f| f.path == path)
            .unwrap()
            .id
            .clone();
        app.trash_file(&id, cx);
        assert!(app.scan_generation > before);
    });
    cx.run_until_parked();
    view.update(cx, |app, _| {
        // The older scan saw the note; it must not bring it back.
        assert!(app.workspace.files.iter().all(|f| f.path != path));
    });
    let _ = std::fs::remove_dir_all(&root);
}
