//! Pasting and dropping images into a note: files land in `assets/` next to
//! the note, the link is inserted at the caret, and an unsaved note is asked
//! to save first.

use super::*;
use gpui::{ClipboardItem, Entity, Image, ImageFormat, TestAppContext, VisualTestContext};

// `use gpui::*` in this crate re-exports a `test` attribute that is not
// Rust's `#[test]`. Pin the std one so these stay ordinary unit tests.
use ::core::prelude::v1::test as std_test;

const NOTE: &str = "before\nafter\n";
/// 2024-02-29 13:05:09 UTC.
const NOW: u64 = 1_709_211_909;
const NAME: &str = "image-20240229-130509.png";

fn png_bytes() -> Vec<u8> {
    let pixels = ::image::RgbaImage::from_pixel(2, 2, ::image::Rgba([10, 120, 200, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    pixels
        .write_to(&mut out, ::image::ImageFormat::Png)
        .unwrap();
    out.into_inner()
}

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "litemark-note-assets-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A window whose note is saved in `dir` (or unsaved when `None`), caret at
/// the end of the first line.
fn harness(dir: Option<&Path>) -> (Entity<LiteMarkApp>, VisualTestContext) {
    let mut cx = TestAppContext::single();
    let (view, cx) = cx.add_window_view(|_window, cx| {
        let mut app = LiteMarkApp::test_new(NOTE, cx);
        app.active_tab_mut().expect("tab").file_path = dir.map(|d| d.join("note.md"));
        app.active_tab_mut()
            .expect("tab")
            .place("before".len(), false);
        app
    });
    (view, cx.clone())
}

fn content(view: &Entity<LiteMarkApp>, cx: &VisualTestContext) -> String {
    view.read_with(cx, |app, _| app.active_tab().expect("tab").content.clone())
}

fn error(view: &Entity<LiteMarkApp>, cx: &VisualTestContext) -> Option<String> {
    view.read_with(cx, |app, _| app.error_message.clone())
}

fn toast(view: &Entity<LiteMarkApp>, cx: &VisualTestContext) -> Option<String> {
    view.read_with(cx, |app, _| app.saved_toast.clone())
}

fn clipboard_png() -> gpui::Image {
    Image::from_bytes(ImageFormat::Png, png_bytes())
}

#[std_test]
fn pasting_an_image_saves_it_in_assets_and_links_it() {
    let dir = folder("paste");
    let (view, mut cx) = harness(Some(&dir));
    let image = clipboard_png();
    view.update(&mut cx, |app, cx| {
        assert!(app.paste_clipboard_image(&image, NOW, cx));
    });
    // Converted and saved in the background, then linked.
    assert_eq!(content(&view, &cx), NOTE);
    cx.run_until_parked();
    // The caret was mid-line: the link gets a line of its own.
    assert_eq!(
        content(&view, &cx),
        format!("before\n![](assets/{NAME})\nafter\n")
    );
    assert_eq!(
        toast(&view, &cx).as_deref(),
        Some(format!("Pasted image saved to assets/{NAME}").as_str())
    );
    assert_eq!(error(&view, &cx), None);
    assert_eq!(
        std::fs::read(dir.join("assets").join(NAME)).unwrap(),
        png_bytes()
    );

    // The same second again must not overwrite the first picture.
    view.update(&mut cx, |app, cx| {
        assert!(app.paste_clipboard_image(&image, NOW, cx));
    });
    cx.run_until_parked();
    assert!(dir
        .join("assets")
        .join("image-20240229-130509-2.png")
        .is_file());
    assert!(content(&view, &cx).contains("![](assets/image-20240229-130509-2.png)"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[std_test]
fn handle_paste_takes_an_image_only_clipboard() {
    let dir = folder("handle-paste");
    let (view, mut cx) = harness(Some(&dir));
    cx.write_to_clipboard(ClipboardItem::new_image(&clipboard_png()));
    view.update(&mut cx, |app, cx| app.handle_paste(cx));
    cx.run_until_parked();
    let pasted = content(&view, &cx);
    assert!(pasted.starts_with("before\n![](assets/image-"), "{pasted}");
    let saved: Vec<_> = std::fs::read_dir(dir.join("assets")).unwrap().collect();
    assert_eq!(saved.len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[std_test]
fn text_paste_is_unchanged() {
    let dir = folder("text");
    let (view, mut cx) = harness(Some(&dir));
    cx.write_to_clipboard(ClipboardItem::new_string(" plus".into()));
    view.update(&mut cx, |app, cx| app.handle_paste(cx));
    assert_eq!(content(&view, &cx), "before plus\nafter\n");
    assert!(!dir.join("assets").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[std_test]
fn an_unsaved_note_asks_to_save_before_pasting_an_image() {
    let dir = folder("unsaved-paste");
    let (view, mut cx) = harness(None);
    cx.write_to_clipboard(ClipboardItem::new_image(&clipboard_png()));
    view.update(&mut cx, |app, cx| app.handle_paste(cx));
    assert_eq!(content(&view, &cx), NOTE, "nothing was inserted");
    let message = error(&view, &cx).expect("the error UI is shown");
    assert!(message.contains("Save this note first"), "{message}");
    assert_eq!(toast(&view, &cx), None);
    // Nothing was written anywhere near the temp folder either.
    assert!(!dir.join("assets").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[std_test]
fn an_unsaved_note_refuses_dropped_images_too() {
    let dir = folder("unsaved-drop");
    let photo = dir.join("photo.png");
    std::fs::write(&photo, png_bytes()).unwrap();
    let (view, mut cx) = harness(None);
    view.update(&mut cx, |app, cx| {
        assert!(app.drop_image_files(std::slice::from_ref(&photo), cx));
    });
    assert_eq!(content(&view, &cx), NOTE);
    assert!(error(&view, &cx).unwrap().contains("Save this note first"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[std_test]
fn dropped_images_are_copied_to_assets_and_each_gets_a_link() {
    let dir = folder("drop");
    let sources = folder("drop-sources");
    let (a, b, c) = (
        sources.join("a.png"),
        sources.join("My Photo.JPG"),
        sources.join("notes.txt"),
    );
    std::fs::write(&a, b"aaa").unwrap();
    std::fs::write(&b, b"bbb").unwrap();
    std::fs::write(&c, b"text").unwrap();
    // A name already in assets is kept, and the new copy is numbered.
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::write(dir.join("assets").join("a.png"), b"old").unwrap();

    let (view, mut cx) = harness(Some(&dir));
    view.update(&mut cx, |app, cx| {
        assert!(app.drop_image_files(&[a.clone(), b.clone(), c.clone()], cx));
    });
    // The copy runs in the background; nothing is linked until it is done.
    assert_eq!(content(&view, &cx), NOTE);
    cx.run_until_parked();
    assert_eq!(
        content(&view, &cx),
        "before\n![](assets/a-2.png)\n![](<assets/My Photo.JPG>)\nafter\n"
    );
    assert_eq!(
        std::fs::read(dir.join("assets").join("a.png")).unwrap(),
        b"old"
    );
    assert_eq!(
        std::fs::read(dir.join("assets").join("a-2.png")).unwrap(),
        b"aaa"
    );
    assert_eq!(
        std::fs::read(dir.join("assets").join("My Photo.JPG")).unwrap(),
        b"bbb"
    );
    assert!(!dir.join("assets").join("notes.txt").exists());
    assert_eq!(std::fs::read(&a).unwrap(), b"aaa", "originals stay put");
    assert_eq!(
        toast(&view, &cx).as_deref(),
        Some("2 images saved to assets/ (skipped 1 non-image file)")
    );

    // Files that are not images are not handled at all.
    view.update(&mut cx, |app, cx| {
        assert!(!app.drop_image_files(std::slice::from_ref(&c), cx));
    });
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&sources);
}

#[std_test]
fn image_extensions_are_matched_case_insensitively() {
    use super::note_assets::is_droppable_image;
    for name in ["a.png", "a.JPG", "a.jpeg", "a.gif", "a.WebP", "a.svg"] {
        assert!(is_droppable_image(Path::new(name)), "{name}");
    }
    for name in ["a.txt", "a.md", "png", "a.bmp", "a"] {
        assert!(!is_droppable_image(Path::new(name)), "{name}");
    }
}

#[std_test]
fn links_get_lines_of_their_own() {
    use super::note_assets::own_lines;
    let link = "![](assets/a.png)";
    // Mid-line: broken before and after.
    assert_eq!(own_lines("abcd", 2..2, link), format!("\n{link}\n"));
    // At the start or end of a line: no extra break on that side.
    assert_eq!(own_lines("ab\ncd", 3..3, link), format!("{link}\n"));
    assert_eq!(own_lines("ab\ncd", 2..2, link), format!("\n{link}"));
    // A whole line selected, or an empty note: just the link.
    assert_eq!(own_lines("ab\ncd\n", 3..5, link), link);
    assert_eq!(own_lines("", 0..0, link), link);
}

#[std_test]
fn an_image_dropped_from_the_notes_own_assets_is_linked_not_copied() {
    let dir = folder("drop-own-asset");
    let assets = dir.join("assets");
    std::fs::create_dir_all(&assets).unwrap();
    let existing = assets.join("pic.png");
    std::fs::write(&existing, b"pic").unwrap();

    let (view, mut cx) = harness(Some(&dir));
    view.update(&mut cx, |app, cx| {
        assert!(app.drop_image_files(std::slice::from_ref(&existing), cx));
    });
    cx.run_until_parked();
    assert!(content(&view, &cx).contains("![](assets/pic.png)"));
    let files: Vec<_> = std::fs::read_dir(&assets).unwrap().collect();
    assert_eq!(files.len(), 1, "no pic-2.png copy");
    let _ = std::fs::remove_dir_all(&dir);
}

#[std_test]
fn nothing_is_pasted_into_the_note_behind_the_quick_open_pop_up() {
    let dir = folder("paste-behind-popup");
    let (view, mut cx) = harness(Some(&dir));
    view.update(&mut cx, |app, cx| app.open_quick_open(QuickMode::Notes, cx));
    for item in [
        ClipboardItem::new_string(" plus".into()),
        ClipboardItem::new_image(&clipboard_png()),
    ] {
        cx.write_to_clipboard(item);
        view.update(&mut cx, |app, cx| app.handle_paste(cx));
        cx.run_until_parked();
        assert_eq!(content(&view, &cx), NOTE);
    }
    assert!(!dir.join("assets").exists(), "no image file was written");
    let _ = std::fs::remove_dir_all(&dir);
}

#[std_test]
fn own_lines_use_the_notes_line_break() {
    use super::note_assets::own_lines_with;
    assert_eq!(
        own_lines_with("abcd", 2..2, "L", "\r\n"),
        "\r\nL\r\n".to_string()
    );
}

#[std_test]
fn an_image_pasted_in_a_list_item_goes_after_the_item() {
    let dir = folder("list-paste");
    let photo = dir.join("p.png");
    std::fs::write(&photo, png_bytes()).unwrap();
    let (view, mut cx) = harness(Some(&dir));
    view.update(&mut cx, |app, _| {
        let tab = app.active_tab_mut().unwrap();
        tab.content = "- one two\n- three\n".into();
        tab.place("- one".len(), false);
    });
    view.update(&mut cx, |app, cx| {
        assert!(app.drop_image_files(std::slice::from_ref(&photo), cx));
    });
    cx.run_until_parked();
    assert_eq!(
        content(&view, &cx),
        "- one two\n\n![](assets/p.png)\n- three\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[std_test]
fn an_image_lands_where_the_caret_was_when_it_was_dropped() {
    let dir = folder("anchor");
    let photo = dir.join("p.png");
    std::fs::write(&photo, png_bytes()).unwrap();
    let (view, mut cx) = harness(Some(&dir));
    view.update(&mut cx, |app, cx| {
        assert!(app.drop_image_files(std::slice::from_ref(&photo), cx));
        // The caret moves while the copy runs.
        app.active_tab_mut().unwrap().place(NOTE.len(), false);
    });
    cx.run_until_parked();
    assert_eq!(content(&view, &cx), "before\n![](assets/p.png)\nafter\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[std_test]
fn an_image_pasted_in_a_quote_goes_after_the_quote_line() {
    let dir = folder("quote-paste");
    let photo = dir.join("p.png");
    std::fs::write(&photo, png_bytes()).unwrap();
    let (view, mut cx) = harness(Some(&dir));
    view.update(&mut cx, |app, _| {
        let tab = app.active_tab_mut().unwrap();
        tab.content = "> quoted\nafter\n".into();
        tab.place("> quo".len(), false);
    });
    view.update(&mut cx, |app, cx| {
        assert!(app.drop_image_files(std::slice::from_ref(&photo), cx));
    });
    cx.run_until_parked();
    assert_eq!(
        content(&view, &cx),
        "> quoted\n\n![](assets/p.png)\nafter\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[std_test]
fn dropping_other_files_with_images_says_they_were_skipped() {
    let dir = folder("skip");
    let (photo, text) = (dir.join("p.png"), dir.join("t.txt"));
    std::fs::write(&photo, png_bytes()).unwrap();
    std::fs::write(&text, b"x").unwrap();
    let (view, mut cx) = harness(Some(&dir));
    view.update(&mut cx, |app, cx| {
        assert!(app.drop_image_files(&[photo.clone(), text.clone()], cx));
    });
    cx.run_until_parked();
    let message = toast(&view, &cx).expect("a toast");
    assert!(message.contains("skipped 1 non-image file"), "{message}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[std_test]
fn a_percent_in_a_file_name_is_escaped_in_the_link() {
    use super::note_assets::asset_link;
    use crate::markdown::image::{resolve_image_target, ImageTarget};
    let dir = folder("percent");
    let (markdown, _) = asset_link(&dir.join("assets").join("a%41.png")).unwrap();
    assert_eq!(markdown, "![](assets/a%2541.png)");
    // Even when `aA.png` exists, the link finds the file it names.
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::write(dir.join("assets/aA.png"), b"x").unwrap();
    std::fs::write(dir.join("assets/a%41.png"), b"y").unwrap();
    assert_eq!(
        resolve_image_target("assets/a%2541.png", Some(&dir)),
        ImageTarget::Local(dir.join("assets/a%41.png"))
    );
    let _ = std::fs::remove_dir_all(&dir);
}
