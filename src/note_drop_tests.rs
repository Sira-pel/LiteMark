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
        app.workspace = scan_workspace_dir(&root);
        app.refresh_workspace_ids();
        let file_id = app.workspace.files[0].id.clone();
        let folder_id = app.workspace.folders[0].id.clone();

        let outcome = app.move_or_copy_note(&file_id, Some(folder_id), true, true);
        assert!(matches!(outcome, DropOutcome::Done(_)));

        let tab = app.active_tab().expect("copy opened in a tab");
        assert_eq!(tab.file_path.as_deref(), Some(root.join("dest").join("note.md").as_path()));
        assert_eq!(tab.content, text);
    });

    let _ = std::fs::remove_dir_all(&root);
}
