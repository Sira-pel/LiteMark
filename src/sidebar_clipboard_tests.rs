//! Ctrl+C / Ctrl+V / Ctrl+D on notes and folders selected in the sidebar.

use super::*;
use gpui::TestAppContext;

// `use gpui::*` in this crate re-exports a `test` attribute that is not
// Rust's `#[test]`. Pin the std one so these stay ordinary unit tests.
use ::core::prelude::v1::test as std_test;

/// A fresh temp workspace with `note.md`, `dest/` and `dest/inner.md`.
fn workspace(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "litemark-sidebar-clip-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("dest")).unwrap();
    std::fs::write(root.join("note.md"), "# Note\n").unwrap();
    std::fs::write(root.join("dest").join("inner.md"), "inner").unwrap();
    root
}

fn load(app: &mut LiteMarkApp, root: &Path) {
    app.workspace = scan_workspace_dir(root, true).into();
    app.refresh_workspace_ids();
}

fn file_target(app: &LiteMarkApp, path: &Path) -> SidebarTarget {
    let file = app.workspace.files.iter().find(|f| f.path == path);
    SidebarTarget::File(file.expect("listed note").id.clone())
}

fn folder_target(app: &LiteMarkApp, path: &Path) -> SidebarTarget {
    let folder = app.workspace.folders.iter().find(|f| f.path == path);
    SidebarTarget::Folder(folder.expect("listed folder").id.to_string())
}

/// Select `target` in the sidebar with the keyboard there, as a click does.
fn select(app: &mut LiteMarkApp, target: SidebarTarget) {
    app.multi_selection = None;
    app.selected_target = Some(target);
    app.sidebar_keyboard = true;
}

#[std_test]
fn copy_then_paste_into_a_folder_keeps_the_name_and_selects_the_copy() {
    let root = workspace("paste");
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, _cx| {
        load(app, &root);
        let note = file_target(app, &root.join("note.md"));
        select(app, note);
        assert!(app.sidebar_owns_file_keys());
        app.sidebar_copy();

        let dest = folder_target(app, &root.join("dest"));
        select(app, dest);
        app.sidebar_paste();

        let pasted = root.join("dest").join("note.md");
        assert_eq!(std::fs::read_to_string(&pasted).unwrap(), "# Note\n");
        assert!(root.join("note.md").exists(), "the original stays");
        assert_eq!(app.selected_target, Some(file_target(app, &pasted)));
        assert!(app.error_message.is_none());
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn pasting_beside_the_original_adds_copy_suffixes() {
    let root = workspace("suffix");
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, _cx| {
        load(app, &root);
        let note = file_target(app, &root.join("note.md"));
        select(app, note.clone());
        app.sidebar_copy();

        // The selected note's own folder is the destination.
        app.sidebar_paste();
        assert!(root.join("note copy.md").exists());
        // The new copy is now selected; pasting again goes beside it too.
        app.sidebar_paste();
        assert!(root.join("note copy 2.md").exists());
        assert_eq!(
            app.selected_target,
            Some(file_target(app, &root.join("note copy 2.md")))
        );
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn duplicate_copies_the_selected_note_next_to_it() {
    let root = workspace("dup");
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, _cx| {
        load(app, &root);
        let inner = root.join("dest").join("inner.md");
        let target = file_target(app, &inner);
        select(app, target);
        app.sidebar_duplicate();

        let copy = root.join("dest").join("inner copy.md");
        assert_eq!(std::fs::read_to_string(&copy).unwrap(), "inner");
        assert_eq!(app.selected_target, Some(file_target(app, &copy)));
        assert!(
            app.sidebar_clipboard.is_empty(),
            "duplicate leaves the clipboard alone"
        );
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn multi_selection_copies_and_pastes_every_row() {
    let root = workspace("multi");
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, _cx| {
        std::fs::write(root.join("second.md"), "two").unwrap();
        std::fs::create_dir_all(root.join("into")).unwrap();
        load(app, &root);
        let dest = folder_target(app, &root.join("dest"));
        let note = file_target(app, &root.join("note.md"));
        let second = file_target(app, &root.join("second.md"));
        select(app, note.clone());
        app.toggle_selection(second.clone());
        app.toggle_selection(dest);
        assert_eq!(app.active_multi().map(|m| m.items.len()), Some(3));
        app.sidebar_copy();
        assert_eq!(app.sidebar_clipboard.len(), 3);

        let target = folder_target(app, &root.join("into"));
        select(app, target);
        app.sidebar_paste();

        let into = root.join("into");
        assert!(into.join("note.md").exists());
        assert!(into.join("second.md").exists());
        assert!(into.join("dest").join("inner.md").exists());
        assert_eq!(app.active_multi().map(|m| m.items.len()), Some(3));
        assert!(app.error_message.is_none());
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn a_folder_pasted_onto_itself_lands_beside_it() {
    let root = workspace("self");
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, _cx| {
        load(app, &root);
        let dest = folder_target(app, &root.join("dest"));
        select(app, dest);
        app.sidebar_copy();
        app.sidebar_paste();
        assert!(root.join("dest copy").join("inner.md").exists());
        assert!(!root.join("dest").join("dest").exists());
        assert!(app.error_message.is_none());

        // Into a folder inside it is refused and reported.
        std::fs::create_dir_all(root.join("dest").join("sub")).unwrap();
        app.rescan_workspace();
        let sub = folder_target(app, &root.join("dest").join("sub"));
        select(app, sub);
        app.sidebar_paste();
        assert!(!root.join("dest").join("sub").join("dest").exists());
        assert!(app.error_message.is_some());
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn a_vanished_source_is_reported() {
    let root = workspace("gone");
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, _cx| {
        load(app, &root);
        let note = file_target(app, &root.join("note.md"));
        select(app, note);
        app.sidebar_copy();
        std::fs::remove_file(root.join("note.md")).unwrap();
        app.sidebar_paste();
        let error = app.error_message.clone().expect("error shown");
        assert!(error.contains("no longer exists"), "{error}");
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn the_keys_belong_to_the_editor_and_fields_unless_the_sidebar_has_focus() {
    let root = workspace("focus");
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, cx| {
        load(app, &root);
        let note = file_target(app, &root.join("note.md"));
        select(app, note.clone());
        assert!(app.sidebar_owns_file_keys());

        // Clicking into the editor or a field hands the keys back.
        app.sidebar_keyboard = false;
        assert!(!app.sidebar_owns_file_keys());
        app.sidebar_keyboard = true;
        app.claim_field(TextInput::Search, cx);
        assert!(!app.sidebar_owns_file_keys());

        // An inline rename in the sidebar keeps them for its text.
        select(app, note.clone());
        app.rename_target = Some(note);
        assert!(!app.sidebar_owns_file_keys());
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn cut_then_paste_moves_the_note_and_its_open_tab() {
    let root = workspace("cut");
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, _cx| {
        load(app, &root);
        let original = root.join("note.md");
        let note = file_target(app, &original);
        let SidebarTarget::File(note_id) = note.clone() else {
            unreachable!()
        };
        app.open_loaded_tab(
            "note.md".into(),
            "# Note\n".into(),
            Some(original.clone()),
            Some(note_id),
        );
        select(app, note);
        app.sidebar_cut();
        assert!(original.exists(), "nothing moves before the paste");

        let dest = folder_target(app, &root.join("dest"));
        select(app, dest);
        app.sidebar_paste();

        let moved = root.join("dest").join("note.md");
        assert!(!original.exists());
        assert_eq!(std::fs::read_to_string(&moved).unwrap(), "# Note\n");
        assert_eq!(
            app.active_tab().unwrap().file_path.as_deref(),
            Some(moved.as_path())
        );
        assert_eq!(app.selected_target, Some(file_target(app, &moved)));
        assert!(app.sidebar_clipboard.is_empty(), "a cut pastes once");
        assert!(app.error_message.is_none());
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn a_cut_folder_pasted_onto_itself_stays_put() {
    let root = workspace("cutself");
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, _cx| {
        load(app, &root);
        let dest = folder_target(app, &root.join("dest"));
        select(app, dest);
        app.sidebar_cut();
        app.sidebar_paste();
        assert!(root.join("dest").join("inner.md").exists());
        assert!(!root.join("dest copy").exists());
        assert!(app.error_message.is_none());
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn a_cut_that_fails_to_move_stays_cut() {
    let root = workspace("cutfail");
    std::fs::create_dir_all(root.join("dest").join("sub")).unwrap();
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, _cx| {
        load(app, &root);
        let dest = root.join("dest");
        select(app, folder_target(app, &dest));
        app.sidebar_cut();
        // A folder can't move into its own subfolder.
        select(app, folder_target(app, &dest.join("sub")));
        app.sidebar_paste();
        assert!(app.error_message.is_some());
        assert!(dest.join("inner.md").exists());
        assert_eq!(app.sidebar_clipboard, vec![dest.clone()]);
        assert!(
            app.sidebar_clipboard_cut,
            "it can still be pasted elsewhere"
        );

        // Pasted somewhere valid, it moves and the clipboard empties.
        app.selected_target = None;
        app.sidebar_keyboard = true;
        let elsewhere = root.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        load(app, &root);
        select(app, folder_target(app, &elsewhere));
        app.sidebar_paste();
        assert!(elsewhere.join("dest").join("inner.md").exists());
        assert!(app.sidebar_clipboard.is_empty());
        assert!(!app.sidebar_clipboard_cut);
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn copy_after_cut_pastes_a_copy() {
    let root = workspace("cutcopy");
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, _cx| {
        load(app, &root);
        let note = file_target(app, &root.join("note.md"));
        select(app, note);
        app.sidebar_cut();
        app.sidebar_copy();
        let dest = folder_target(app, &root.join("dest"));
        select(app, dest);
        app.sidebar_paste();
        assert!(root.join("note.md").exists(), "Ctrl+C replaced the cut");
        assert!(root.join("dest").join("note.md").exists());
    });
    let _ = std::fs::remove_dir_all(&root);
}
