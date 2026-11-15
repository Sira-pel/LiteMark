//! A note changed or deleted by another program while it is open, and the
//! name typed when saving an untitled note.

use super::*;
use gpui::TestAppContext;

// `use gpui::*` in this crate re-exports a `test` attribute that is not
// Rust's `#[test]`. Pin the std one so these stay ordinary unit tests.
use ::core::prelude::v1::test as std_test;

fn fresh_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("litemark-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

/// Open `path` the way a sidebar click does.
fn click_open(app: &mut LiteMarkApp, path: &Path) {
    let file = app
        .workspace
        .files
        .iter()
        .find(|file| file.path == path)
        .cloned()
        .expect("file in workspace");
    let content = read_note(&file.path).unwrap();
    app.open_loaded_tab(
        file.name.clone(),
        content,
        Some(file.path.clone()),
        Some(file.id.clone()),
    );
}

#[std_test]
fn reclicking_an_unedited_note_shows_the_newer_file() {
    let root = fresh_root("reclick");
    let path = root.join("journal.md");
    std::fs::write(&path, "line written in LiteMark\n").unwrap();

    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, _cx| {
        app.workspace = scan_workspace_dir(&root, true);
        click_open(app, &path);
        let tabs = app.tabs.len();

        std::fs::write(
            &path,
            "line written in LiteMark\nTEN PARAGRAPHS ADDED ELSEWHERE\n",
        )
        .unwrap();
        click_open(app, &path);

        assert_eq!(app.tabs.len(), tabs, "the open tab is reused");
        let tab = app.active_tab().unwrap();
        assert_eq!(
            tab.content,
            "line written in LiteMark\nTEN PARAGRAPHS ADDED ELSEWHERE\n"
        );
        assert!(!tab.is_modified);
        // The reloaded text is now what the tab knows, so saving is allowed.
        let id = app.active_tab_id.clone();
        assert_eq!(app.disk_change_for(&id), DiskChange::Unchanged);
        assert!(app.save_tab(&id).is_ok());
    });

    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn save_refuses_to_overwrite_a_file_changed_elsewhere() {
    let root = fresh_root("conflict");
    let path = root.join("journal.md");
    std::fs::write(&path, "mine\n").unwrap();

    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, cx| {
        app.workspace = scan_workspace_dir(&root, true);
        click_open(app, &path);
        let id = app.active_tab_id.clone();
        let tab = app.active_tab_mut().unwrap();
        tab.edit(0..0, "edited ");
        tab.is_modified = true;

        std::fs::write(&path, "theirs, written by a sync client\n").unwrap();
        // A re-click keeps the unsaved edits.
        click_open(app, &path);
        assert_eq!(app.active_tab().unwrap().content, "edited mine\n");

        assert_eq!(app.disk_change_for(&id), DiskChange::Changed);
        let err = app.save_tab(&id).unwrap_err();
        assert!(err.contains("changed it on disk"), "{err}");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "theirs, written by a sync client\n"
        );

        // Ctrl+S asks instead of saving.
        app.handle_save(cx);
        assert!(matches!(
            app.prompt,
            Some(Prompt::ExternalChange { deleted: false, .. })
        ));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "theirs, written by a sync client\n"
        );

        // Overwrite keeps the user's version.
        app.answer_prompt("Overwrite", cx);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "edited mine\n");
        assert!(!app.active_tab().unwrap().is_modified);
        assert_eq!(app.disk_change_for(&id), DiskChange::Unchanged);
    });

    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn reload_takes_the_file_from_disk() {
    let root = fresh_root("reload");
    let path = root.join("journal.md");
    std::fs::write(&path, "mine\n").unwrap();

    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, cx| {
        app.workspace = scan_workspace_dir(&root, true);
        click_open(app, &path);
        let tab = app.active_tab_mut().unwrap();
        tab.edit(0..0, "edited ");
        tab.is_modified = true;
        std::fs::write(&path, "theirs\n").unwrap();

        app.handle_save(cx);
        app.answer_prompt("Reload", cx);
        let tab = app.active_tab().unwrap();
        assert_eq!(tab.content, "theirs\n");
        assert!(!tab.is_modified);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "theirs\n");

        // Cancel leaves both alone.
        let tab = app.active_tab_mut().unwrap();
        tab.edit(0..0, "again ");
        tab.is_modified = true;
        std::fs::write(&path, "theirs, longer now\n").unwrap();
        app.handle_save(cx);
        app.answer_prompt("Cancel", cx);
        assert_eq!(app.active_tab().unwrap().content, "again theirs\n");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "theirs, longer now\n"
        );
    });

    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn a_note_deleted_elsewhere_is_not_silently_recreated() {
    let root = fresh_root("deleted");
    let path = root.join("journal.md");
    std::fs::write(&path, "mine\n").unwrap();

    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, cx| {
        app.workspace = scan_workspace_dir(&root, true);
        click_open(app, &path);
        let id = app.active_tab_id.clone();
        std::fs::remove_file(&path).unwrap();

        assert_eq!(app.disk_change_for(&id), DiskChange::Deleted);
        assert!(app.save_tab(&id).is_err());
        assert!(!path.exists());

        app.handle_save(cx);
        assert!(matches!(
            app.prompt,
            Some(Prompt::ExternalChange { deleted: true, .. })
        ));
        app.answer_prompt("Save", cx);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "mine\n");
    });

    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn touching_a_file_without_changing_it_is_not_a_conflict() {
    let root = fresh_root("touch");
    let path = root.join("journal.md");
    std::fs::write(&path, "mine\n").unwrap();

    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, _cx| {
        app.workspace = scan_workspace_dir(&root, true);
        click_open(app, &path);
        let id = app.active_tab_id.clone();
        let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.set_modified(std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000))
            .unwrap();
        drop(file);
        assert_eq!(app.disk_change_for(&id), DiskChange::Unchanged);
        assert!(app.save_tab(&id).is_ok());
    });

    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn save_name_box_keeps_the_note_inside_the_workspace() {
    let root = fresh_root("savename");
    let ws = root.join("workspace");
    std::fs::create_dir_all(&ws).unwrap();

    let cases = [
        // Leading dots stay (dotfiles), but the separator is gone.
        ("../escaped", ".._escaped"),
        (".env", ".env"),
        ("sub/dir/note", "sub_dir_note.md"),
        ("CON", "CON_.md"),
        ("a:b?", "a_b_.md"),
    ];
    for (typed, expected) in cases {
        let mut cx = TestAppContext::single();
        let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("text", cx));
        view.update(cx, |app, cx| {
            app.workspace = scan_workspace_dir(&ws, true);
            let id = app.active_tab_id.clone();
            app.begin_save_naming(&id, cx);
            app.commit_save_naming(Some(typed.to_string()), cx);
            assert_eq!(app.error_message, None, "typed {typed:?}");
            let saved = app.active_tab().unwrap().file_path.clone().unwrap();
            assert_eq!(saved, ws.join(expected), "typed {typed:?}");
        });
    }
    assert!(!root.join("escaped").exists());
    assert!(!ws.join("sub").exists());

    // Only dots and separators: nothing left, so ask for a name.
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("text", cx));
    view.update(cx, |app, cx| {
        app.workspace = scan_workspace_dir(&ws, true);
        let id = app.active_tab_id.clone();
        app.begin_save_naming(&id, cx);
        app.commit_save_naming(Some("..".to_string()), cx);
        assert!(app.error_message.is_some());
        assert!(app.active_tab().unwrap().file_path.is_none());
    });

    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn closing_the_window_asks_about_a_note_changed_on_disk() {
    let root = fresh_root("closewindow");
    let path = root.join("journal.md");
    std::fs::write(&path, "mine\n").unwrap();

    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, cx| {
        app.workspace = scan_workspace_dir(&root, true);
        click_open(app, &path);
        let tab = app.active_tab_mut().unwrap();
        tab.edit(0..0, "edited ");
        tab.is_modified = true;
        std::fs::write(&path, "theirs\n").unwrap();

        app.request_close_window();
        app.answer_prompt("Save", cx);
        assert!(
            matches!(app.prompt, Some(Prompt::ExternalChange { deleted: false, .. })),
            "the changed note gets Overwrite or Reload, not the close prompt again"
        );
        assert!(!app.allow_window_close);

        // Cancel there stops the close and keeps both versions.
        app.answer_prompt("Cancel", cx);
        assert!(app.prompt.is_none());
        assert!(!app.allow_window_close);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "theirs\n");

        // Overwrite settles it and the window closes.
        app.request_close_window();
        app.answer_prompt("Save", cx);
        app.answer_prompt("Overwrite", cx);
        assert!(app.allow_window_close);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "edited mine\n");
    });

    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn naming_an_untitled_note_on_close_still_asks_about_changed_files() {
    let root = fresh_root("closenamed");
    let path = root.join("journal.md");
    std::fs::write(&path, "mine\n").unwrap();

    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("untitled", cx));
    view.update(cx, |app, cx| {
        app.workspace = scan_workspace_dir(&root, true);
        app.active_tab_mut().unwrap().is_modified = true;
        click_open(app, &path);
        let tab = app.active_tab_mut().unwrap();
        tab.edit(0..0, "edited ");
        tab.is_modified = true;
        std::fs::write(&path, "theirs\n").unwrap();

        app.request_close_window();
        app.answer_prompt("Save", cx);
        app.commit_save_naming(Some("named".to_string()), cx);
        assert!(matches!(app.prompt, Some(Prompt::ExternalChange { .. })));
        assert!(!app.allow_window_close);
    });

    let _ = std::fs::remove_dir_all(&root);
}
