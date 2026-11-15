//! Copying a note into another folder from the sidebar.

use super::*;
use gpui::TestAppContext;

// `use gpui::*` in this crate re-exports a `test` attribute that is not
// Rust's `#[test]`. Pin the std one so these stay ordinary unit tests.
use ::core::prelude::v1::test as std_test;

#[std_test]
fn opening_a_copied_note_shows_its_content() {
    let root = std::env::temp_dir().join(format!("litemark-copy-open-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("dest")).unwrap();
    let text = "# Copied\n\nBody that must survive.\n";
    std::fs::write(root.join("note.md"), text).unwrap();

    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, _cx| {
        app.workspace = scan_workspace_dir(&root, true);
        app.refresh_workspace_ids();
        let file_id = app.workspace.files[0].id.clone();
        let folder_id = app.workspace.folders[0].id.clone();

        let outcome = app.move_or_copy_note(&file_id, Some(folder_id), true, true);
        assert!(matches!(outcome, DropOutcome::Done(_)));

        let tab = app.active_tab().expect("copy opened in a tab");
        assert_eq!(
            tab.file_path.as_deref(),
            Some(root.join("dest").join("note.md").as_path())
        );
        assert_eq!(tab.content, text);
    });

    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn resaving_a_listed_note_does_not_rescan_the_workspace() {
    // F17: Ctrl+S on a note the sidebar already lists used to rescan the
    // whole tree on the UI thread. A file added behind the app's back is the
    // tell: it shows up only if a rescan ran.
    let root = std::env::temp_dir().join(format!("litemark-resave-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let note = root.join("note.md");
    std::fs::write(&note, "old").unwrap();

    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, _cx| {
        app.workspace = scan_workspace_dir(&root, true);
        app.refresh_workspace_ids();
        let file_id = app.workspace.files[0].id.clone();
        app.open_loaded_tab(
            "note.md".into(),
            "old".into(),
            Some(note.clone()),
            Some(file_id.clone()),
        );
        std::fs::write(root.join("outside.md"), "added elsewhere").unwrap();

        let id = app.active_tab_id.clone();
        app.active_tab_mut().unwrap().content = "new".into();
        app.save_tab(&id).expect("save");
        assert_eq!(std::fs::read_to_string(&note).unwrap(), "new");
        assert_eq!(app.workspace.files.len(), 1, "re-save must not rescan");
        assert_eq!(
            app.active_tab().unwrap().workspace_file_id.as_deref(),
            Some(file_id.as_str())
        );

        // A save that creates a file still rescans, so the new note is listed.
        app.open_loaded_tab("fresh.md".into(), "fresh".into(), None, None);
        let id = app.active_tab_id.clone();
        app.save_tab(&id).expect("save new");
        let mut names: Vec<_> = app.workspace.files.iter().map(|f| f.name.clone()).collect();
        names.sort();
        assert_eq!(names, ["fresh.md", "note.md", "outside.md"]);
    });

    let _ = std::fs::remove_dir_all(&root);
}
