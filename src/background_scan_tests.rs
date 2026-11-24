//! A workspace rescan that runs in the background applies only if nothing
//! newer has happened since it started.

use super::*;
use gpui::TestAppContext;

// See sidebar_render_tests.rs: pin the std `#[test]`.
use ::core::prelude::v1::test as std_test;

fn setup(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("litemark-bgscan-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("sub")).unwrap();
    std::fs::write(root.join("a.md"), "a").unwrap();
    root
}

#[std_test]
fn a_requested_rescan_is_applied_when_it_finishes() {
    let root = setup("apply");
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, cx| {
        app.workspace = scan_workspace_dir(&root, true).into();
        app.refresh_workspace_ids();
        // A file added behind the app's back appears once the scan lands.
        std::fs::write(root.join("b.md"), "b").unwrap();
        assert_eq!(app.workspace.files.len(), 1);
        app.request_rescan();
        app.start_requested_rescan(cx);
        assert_eq!(app.workspace.files.len(), 1, "scan must not block");
    });
    cx.run_until_parked();
    view.update(cx, |app, _| assert_eq!(app.workspace.files.len(), 2));
    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn a_newer_synchronous_rescan_drops_the_one_in_flight() {
    let root = setup("stale");
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, cx| {
        app.workspace = scan_workspace_dir(&root, true).into();
        app.refresh_workspace_ids();
        app.request_rescan();
        app.start_requested_rescan(cx);
        // A synchronous rescan lands first; empty the list so a stale result
        // that gets applied afterwards is visible.
        app.rescan_workspace();
        app.workspace.files.clear();
    });
    cx.run_until_parked();
    view.update(cx, |app, _| {
        assert!(app.workspace.files.is_empty(), "stale scan was applied");
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn of_two_overlapping_background_scans_only_the_newer_applies() {
    let root = setup("overlap");
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, cx| {
        app.workspace = scan_workspace_dir(&root, true).into();
        app.refresh_workspace_ids();
        app.request_rescan();
        app.start_requested_rescan(cx);
        let first = app.scan_generation;
        app.request_rescan();
        app.start_requested_rescan(cx);
        assert!(app.scan_generation > first);
        app.workspace.files.clear();
    });
    cx.run_until_parked();
    // Exactly one result (the newer) refilled the list.
    view.update(cx, |app, _| assert_eq!(app.workspace.files.len(), 1));
    let _ = std::fs::remove_dir_all(&root);
}

#[std_test]
fn folders_toggled_while_a_scan_runs_keep_their_state() {
    let root = setup("open");
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("", cx));
    view.update(cx, |app, cx| {
        app.workspace = scan_workspace_dir(&root, true).into();
        app.refresh_workspace_ids();
        app.request_rescan();
        app.start_requested_rescan(cx);
        // The user closes the folder after the scan started.
        for folder in app.workspace.folders.iter_mut() {
            folder.is_open = false;
        }
    });
    cx.run_until_parked();
    view.update(cx, |app, _| {
        assert!(!app.workspace.folders.is_empty());
        assert!(app.workspace.folders.iter().all(|f| !f.is_open));
    });
    let _ = std::fs::remove_dir_all(&root);
}
