//! The sidebar builds and paints only the rows in view.

use super::*;
use gpui::{TestAppContext, VisualTestContext};

// `use gpui::*` in this crate re-exports a `test` attribute that is not
// Rust's `#[test]`. Pin the std one so these stay ordinary unit tests.
use ::core::prelude::v1::test as std_test;

fn big_workspace(folders: usize, files_per_folder: usize) -> WorkspaceDirectory {
    let root = PathBuf::from("/litemark-sidebar-test");
    // Built by a scan of a folder that doesn't exist (so it is empty) rather
    // than a struct literal, so fields added to WorkspaceDirectory later
    // (`scan_truncated` in #62) get their real defaults here.
    let mut ws = crate::services::fs_service::scan_workspace_dir(&root, true);
    ws.folder_name = "Notes".into();
    assert!(ws.folders.is_empty() && ws.files.is_empty());
    for d in 0..folders {
        let id: std::sync::Arc<str> = std::sync::Arc::from(format!("folder-{d}"));
        ws.folders.push(crate::model::WorkspaceFolder {
            id: id.clone(),
            name: format!("Folder {d}"),
            parent_id: None,
            path: root.join(format!("Folder {d}")),
            is_open: true,
        });
        for f in 0..files_per_folder {
            let name = format!("Note {d}-{f}.md");
            ws.files.push(WorkspaceFile {
                id: format!("file-{d}-{f}"),
                name: name.clone(),
                folder_id: Some(id.clone()),
                path: root.join(format!("Folder {d}")).join(name),
                file_type: crate::model::FileType::Markdown,
            });
        }
    }
    ws
}

fn redraw(cx: &mut VisualTestContext) {
    cx.update(|window, _cx| window.refresh());
    cx.run_until_parked();
}

#[std_test]
fn a_frame_with_thousands_of_notes_stays_fast() {
    // F12: every frame built and laid out all 5,050 rows (2.6 s per frame in
    // a debug build, 0.5 s in release). Only rows in view are built now
    // (about 20 ms debug, 3.5 ms release).
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| {
        let mut app = LiteMarkApp::test_new("", cx);
        app.workspace = big_workspace(50, 100).into();
        app.show_sidebar = true;
        app
    });
    let mut cx = cx.clone();
    view.update(&mut cx, |app, _| app.refresh_workspace_ids());
    redraw(&mut cx);
    let started = std::time::Instant::now();
    for _ in 0..3 {
        redraw(&mut cx);
    }
    let per_frame = started.elapsed() / 3;
    assert!(
        per_frame.as_millis() < 500,
        "sidebar frame took {per_frame:?}"
    );
}

#[std_test]
fn keyboard_selection_finds_its_row_to_scroll_to() {
    let ws = big_workspace(3, 4);
    let items = crate::ui::sidebar::build_sidebar_tree(&ws.folders, &ws.files, None);
    let file = ws.files.last().unwrap().id.clone();
    let ix =
        crate::ui::sidebar::selected_row_index(&items, &ws.folders, &ws.files, Some(&file), None)
            .unwrap();
    assert!(
        matches!(items[ix], crate::ui::sidebar::SidebarTreeItem::File { index, .. } if ws.files[index as usize].id == file)
    );
    let folder = ws.folders[1].id.clone();
    let ix = crate::ui::sidebar::selected_row_index(
        &items,
        &ws.folders,
        &ws.files,
        None,
        Some(folder.as_ref()),
    )
    .unwrap();
    assert!(
        matches!(items[ix], crate::ui::sidebar::SidebarTreeItem::Folder { index, .. } if ws.folders[index as usize].id == folder)
    );
    assert_eq!(
        crate::ui::sidebar::selected_row_index(&items, &ws.folders, &ws.files, Some("gone"), None),
        None
    );
}

#[std_test]
fn the_sidebar_tree_is_cached_until_the_workspace_changes() {
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| {
        let mut app = LiteMarkApp::test_new("", cx);
        app.workspace = big_workspace(3, 4).into();
        app
    });
    let mut cx = cx.clone();
    view.update(&mut cx, |app, _| {
        let first = app.sidebar_tree();
        assert!(std::rc::Rc::ptr_eq(&first, &app.sidebar_tree()));
        assert_eq!(first.len(), 3 + 3 * 4);

        // Any mutable access counts as a change: collapse a folder.
        app.workspace.folders[0].is_open = false;
        let collapsed = app.sidebar_tree();
        assert!(!std::rc::Rc::ptr_eq(&first, &collapsed));
        assert_eq!(collapsed.len(), 3 + 2 * 4);

        // The pending create row is part of the key too.
        app.create_draft = Some(crate::ui::sidebar::SidebarCreate {
            parent: None,
            is_folder: false,
        });
        assert_eq!(app.sidebar_tree().len(), collapsed.len() + 1);
    });
}
