//! Finishing a background PDF open.

use super::commands::PdfOpenStart;
use super::*;
use gpui::TestAppContext;

// gpui's prelude brings its own `test` attribute into scope, which shadows
// Rust's `#[test]`. Pin the std one so these stay ordinary unit tests.
use ::core::prelude::v1::test as std_test;

fn extracted() -> Result<(String, usize), String> {
    Ok(("## Page 1\n\ntext".into(), 1))
}

#[std_test]
fn finished_pdf_takes_over_only_the_tab_it_was_opened_from() {
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| LiteMarkApp::test_new("note", cx));
    view.update(cx, |app, cx| {
        let note = app.active_tab_id.clone();

        // Still on the same tab, nothing else toasted: the PDF opens.
        app.show_toast("Opening a.pdf…", cx);
        let started = PdfOpenStart {
            tab_id: note.clone(),
            toast_epoch: app.toast_epoch,
        };
        app.finish_pdf_open("a.pdf".into(), "/x/a.pdf".into(), extracted(), &started, cx);
        assert_ne!(app.active_tab_id, note);
        assert!(app.show_pdf_modal);
        assert_eq!(app.saved_toast, None);

        // The user moved to another tab and saved meanwhile.
        app.show_pdf_modal = false;
        app.active_tab_id = note.clone();
        app.show_toast("Opening b.pdf…", cx);
        let started = PdfOpenStart {
            tab_id: "some-other-tab".into(),
            toast_epoch: app.toast_epoch,
        };
        let pdf_tab = app.tabs.len();
        app.finish_pdf_open("b.pdf".into(), "/x/b.pdf".into(), extracted(), &started, cx);
        assert_eq!(app.active_tab_id, note, "focus stays where the user is");
        assert!(!app.show_pdf_modal);
        assert_eq!(
            app.tabs[pdf_tab].title, "b.pdf",
            "the PDF still opens, in the background"
        );
        assert_eq!(
            app.saved_toast.as_deref(),
            Some("Opened b.pdf in a new tab")
        );

        // Already open in a tab: no new tab, so no "new tab" toast.
        app.show_toast("Opening b.pdf…", cx);
        let started = PdfOpenStart {
            tab_id: "some-other-tab".into(),
            toast_epoch: app.toast_epoch,
        };
        app.finish_pdf_open("b.pdf".into(), "/x/b.pdf".into(), extracted(), &started, cx);
        assert_eq!(app.saved_toast, None);

        // A newer toast (here "Saved") is kept when the extraction ends.
        app.show_toast("Opening c.pdf…", cx);
        let started = PdfOpenStart {
            tab_id: note.clone(),
            toast_epoch: app.toast_epoch,
        };
        app.show_toast("Saved note", cx);
        app.finish_pdf_open(
            "c.pdf".into(),
            "/x/c.pdf".into(),
            Err("bad".into()),
            &started,
            cx,
        );
        assert_eq!(app.saved_toast.as_deref(), Some("Saved note"));
    });
}
