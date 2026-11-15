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
        let id = format!("folder-{d}");
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
                updated_at: 0,
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
        app.workspace = big_workspace(50, 100);
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
    assert!(per_frame.as_millis() < 500, "sidebar frame took {per_frame:?}");
}

#[std_test]
fn keyboard_selection_finds_its_row_to_scroll_to() {
    let ws = big_workspace(3, 4);
    let items = crate::ui::sidebar::build_sidebar_tree(
        &ws.folders,
        &ws.files,
        None,
    );
    let file = ws.files.last().unwrap().id.clone();
    let ix = crate::ui::sidebar::selected_row_index(&items, Some(&file), None).unwrap();
    assert!(matches!(&items[ix], crate::ui::sidebar::SidebarTreeItem::File { file: f, .. } if f.id == file));
    let folder = ws.folders[1].id.clone();
    let ix = crate::ui::sidebar::selected_row_index(&items, None, Some(&folder)).unwrap();
    assert!(matches!(&items[ix], crate::ui::sidebar::SidebarTreeItem::Folder { folder: f, .. } if f.id == folder));
    assert_eq!(crate::ui::sidebar::selected_row_index(&items, Some("gone"), None), None);
}
