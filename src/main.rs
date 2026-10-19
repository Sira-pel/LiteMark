#![recursion_limit = "512"]

use gpui::*;
use rust_embed::RustEmbed;
use std::env;
use std::path::PathBuf;

mod actions;
mod app;
mod markdown;
mod model;
mod services;
mod theme;
mod ui;

use actions::*;
use app::LiteMarkApp;

#[derive(RustEmbed)]
#[folder = "assets"]
struct Assets;

struct EmbeddedAssets;

impl AssetSource for EmbeddedAssets {
    fn load(&self, path: &str) -> Result<Option<std::borrow::Cow<'static, [u8]>>> {
        let clean_path = path.trim_start_matches('/');
        if let Some(file) = Assets::get(clean_path) {
            Ok(Some(file.data))
        } else {
            Ok(None)
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let clean_path = path.trim_start_matches('/');
        Ok(Assets::iter()
            .filter(|p| p.starts_with(clean_path))
            .map(|p| p.to_string().into())
            .collect())
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let initial_file = args.get(1).map(PathBuf::from).filter(|p| p.exists());

    Application::new()
        .with_assets(EmbeddedAssets)
        .run(move |cx: &mut App| {
            // Register bundled Ubuntu fonts
            let fonts: Vec<std::borrow::Cow<'static, [u8]>> = [
                "fonts/Ubuntu-R.ttf",
                "fonts/Ubuntu-RI.ttf",
                "fonts/Ubuntu-M.ttf",
                "fonts/Ubuntu-B.ttf",
                "fonts/Ubuntu-BI.ttf",
                "fonts/UbuntuMono-R.ttf",
                "fonts/UbuntuMono-B.ttf",
                "fonts/UbuntuMono-BI.ttf",
            ]
            .iter()
            .filter_map(|p| Assets::get(p).map(|f| f.data))
            .collect();
            let _ = cx.text_system().add_fonts(fonts);

            // Bind global key shortcuts
            cx.bind_keys([
                KeyBinding::new("ctrl-b", ToggleBold, None),
                KeyBinding::new("ctrl-i", ToggleItalic, None),
                KeyBinding::new("ctrl-u", ToggleUnderline, None),
                KeyBinding::new("ctrl-shift-x", ToggleStrikethrough, None),
                KeyBinding::new("ctrl-s", SaveFile, None),
                KeyBinding::new("ctrl-o", OpenFile, None),
                KeyBinding::new("ctrl-n", NewTab, None),
                KeyBinding::new("ctrl-w", CloseActiveTab, None),
                KeyBinding::new("ctrl-f", FindReplace, None),
                KeyBinding::new("ctrl-\\", ToggleSidebar, None),
                KeyBinding::new("f11", ToggleZenMode, None),
                KeyBinding::new("escape", CloseModal, None),
                KeyBinding::new("ctrl-=", ZoomIn, None),
                KeyBinding::new("ctrl-+", ZoomIn, None),
                KeyBinding::new("ctrl--", ZoomOut, None),
                KeyBinding::new("ctrl-0", ResetZoom, None),
                KeyBinding::new("ctrl-z", UndoAction, None),
                KeyBinding::new("ctrl-y", RedoAction, None),
                KeyBinding::new("ctrl-shift-z", RedoAction, None),
                KeyBinding::new("ctrl-1", Heading1, None),
                KeyBinding::new("ctrl-2", Heading2, None),
                KeyBinding::new("ctrl-3", Heading3, None),
                KeyBinding::new("ctrl-e", ToggleInlineCode, None),
                KeyBinding::new("ctrl-k", InsertLink, None),
                KeyBinding::new("ctrl-shift-c", InsertCodeBlock, None),
                KeyBinding::new("ctrl-shift-s", SaveFileAs, None),
                KeyBinding::new("ctrl-h", OpenReplace, None),
                KeyBinding::new("ctrl-shift-m", ToggleViewMode, None),
                KeyBinding::new("ctrl-shift-l", ToggleLineNumbers, None),
                KeyBinding::new("ctrl-x", Cut, None),
                KeyBinding::new("ctrl-c", Copy, None),
                KeyBinding::new("ctrl-v", Paste, None),
                KeyBinding::new("ctrl-a", SelectAll, None),
                KeyBinding::new("f1", OpenShortcuts, None),
                KeyBinding::new("alt-f", OpenFileMenu, None),
                KeyBinding::new("alt-e", OpenEditMenu, None),
                KeyBinding::new("alt-v", OpenViewMenu, None),
                KeyBinding::new("f10", FocusMenuBar, None),
                // Menu navigation
                KeyBinding::new("up", SelectPrev, Some("Menu")),
                KeyBinding::new("down", SelectNext, Some("Menu")),
                KeyBinding::new("home", SelectFirst, Some("Menu")),
                KeyBinding::new("end", SelectLast, Some("Menu")),
                KeyBinding::new("enter", Confirm, Some("Menu")),
                KeyBinding::new("space", Confirm, Some("Menu")),
                KeyBinding::new("escape", CancelMenu, Some("Menu")),
                KeyBinding::new("left", PrevMenu, Some("Menu")),
                KeyBinding::new("right", NextMenu, Some("Menu")),
                // Text editing. Menu arrows stay on the Menu context.
                KeyBinding::new("left", EditorLeft, Some("Editor")),
                KeyBinding::new("right", EditorRight, Some("Editor")),
                KeyBinding::new("up", EditorUp, Some("Editor")),
                KeyBinding::new("down", EditorDown, Some("Editor")),
                KeyBinding::new("shift-left", EditorSelectLeft, Some("Editor")),
                KeyBinding::new("shift-right", EditorSelectRight, Some("Editor")),
                KeyBinding::new("shift-up", EditorSelectUp, Some("Editor")),
                KeyBinding::new("shift-down", EditorSelectDown, Some("Editor")),
                KeyBinding::new("home", EditorHome, Some("Editor")),
                KeyBinding::new("end", EditorEnd, Some("Editor")),
                KeyBinding::new("shift-home", EditorSelectHome, Some("Editor")),
                KeyBinding::new("shift-end", EditorSelectEnd, Some("Editor")),
                KeyBinding::new("ctrl-left", EditorWordLeft, Some("Editor")),
                KeyBinding::new("ctrl-right", EditorWordRight, Some("Editor")),
                KeyBinding::new("ctrl-shift-left", EditorSelectWordLeft, Some("Editor")),
                KeyBinding::new("ctrl-shift-right", EditorSelectWordRight, Some("Editor")),
                KeyBinding::new("ctrl-shift-up", EditorSelectLineUp, Some("Editor")),
                KeyBinding::new("ctrl-shift-down", EditorSelectLineDown, Some("Editor")),
                KeyBinding::new("ctrl-home", EditorDocStart, Some("Editor")),
                KeyBinding::new("ctrl-end", EditorDocEnd, Some("Editor")),
                KeyBinding::new("ctrl-shift-home", EditorSelectDocStart, Some("Editor")),
                KeyBinding::new("ctrl-shift-end", EditorSelectDocEnd, Some("Editor")),
                KeyBinding::new("ctrl-backspace", EditorDeleteWordLeft, Some("Editor")),
                KeyBinding::new("ctrl-delete", EditorDeleteWordRight, Some("Editor")),
                KeyBinding::new("ctrl-shift-backspace", EditorDeleteLineStart, Some("Editor")),
                KeyBinding::new("ctrl-shift-delete", EditorDeleteLineEnd, Some("Editor")),
                KeyBinding::new("backspace", EditorBackspace, Some("Editor")),
                KeyBinding::new("delete", EditorDelete, Some("Editor")),
                KeyBinding::new("enter", EditorNewline, Some("Editor")),
                KeyBinding::new("shift-enter", EditorPlainNewline, Some("Editor")),
                KeyBinding::new("tab", IndentAction, Some("Editor")),
                KeyBinding::new("shift-tab", OutdentAction, Some("Editor")),
                KeyBinding::new("ctrl-up", EditorMoveLineUp, Some("Editor")),
                KeyBinding::new("ctrl-down", EditorMoveLineDown, Some("Editor")),
                KeyBinding::new("ctrl-b", ToggleBold, Some("Editor")),
                KeyBinding::new("ctrl-i", ToggleItalic, Some("Editor")),
                KeyBinding::new("ctrl-u", ToggleUnderline, Some("Editor")),
                KeyBinding::new("ctrl-shift-x", ToggleStrikethrough, Some("Editor")),
                KeyBinding::new("ctrl-1", Heading1, Some("Editor")),
                KeyBinding::new("ctrl-2", Heading2, Some("Editor")),
                KeyBinding::new("ctrl-3", Heading3, Some("Editor")),
                KeyBinding::new("ctrl-e", ToggleInlineCode, Some("Editor")),
                KeyBinding::new("ctrl-k", InsertLink, Some("Editor")),
                KeyBinding::new("ctrl-shift-c", InsertCodeBlock, Some("Editor")),
                KeyBinding::new("left", EditorLeft, Some("LineField")),
                KeyBinding::new("right", EditorRight, Some("LineField")),
                KeyBinding::new("shift-left", EditorSelectLeft, Some("LineField")),
                KeyBinding::new("shift-right", EditorSelectRight, Some("LineField")),
                KeyBinding::new("home", EditorHome, Some("LineField")),
                KeyBinding::new("end", EditorEnd, Some("LineField")),
                KeyBinding::new("shift-home", EditorSelectHome, Some("LineField")),
                KeyBinding::new("shift-end", EditorSelectEnd, Some("LineField")),
                KeyBinding::new("ctrl-home", EditorHome, Some("LineField")),
                KeyBinding::new("ctrl-end", EditorEnd, Some("LineField")),
                KeyBinding::new("ctrl-shift-home", EditorSelectHome, Some("LineField")),
                KeyBinding::new("ctrl-shift-end", EditorSelectEnd, Some("LineField")),
                KeyBinding::new("ctrl-shift-up", EditorSelectHome, Some("LineField")),
                KeyBinding::new("ctrl-shift-down", EditorSelectEnd, Some("LineField")),
                KeyBinding::new("ctrl-left", EditorWordLeft, Some("LineField")),
                KeyBinding::new("ctrl-right", EditorWordRight, Some("LineField")),
                KeyBinding::new("ctrl-shift-left", EditorSelectWordLeft, Some("LineField")),
                KeyBinding::new("ctrl-shift-right", EditorSelectWordRight, Some("LineField")),
                KeyBinding::new("ctrl-backspace", EditorDeleteWordLeft, Some("LineField")),
                KeyBinding::new("ctrl-delete", EditorDeleteWordRight, Some("LineField")),
                KeyBinding::new("ctrl-shift-backspace", EditorDeleteLineStart, Some("LineField")),
                KeyBinding::new("ctrl-shift-delete", EditorDeleteLineEnd, Some("LineField")),
                KeyBinding::new("backspace", EditorBackspace, Some("LineField")),
                KeyBinding::new("delete", EditorDelete, Some("LineField")),
                KeyBinding::new("enter", FindNextMatch, Some("LineField")),
                KeyBinding::new("shift-enter", FindPrevMatch, Some("LineField")),
            ]);

            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(960.0), px(640.0)),
                        cx,
                    ))),
                    window_decorations: Some(WindowDecorations::Client),
                    window_min_size: Some(size(px(480.0), px(360.0))),
                    app_id: Some("litemark".into()),
                    titlebar: None,
                    ..Default::default()
                },
                move |_, cx| cx.new(|cx| LiteMarkApp::new(initial_file.clone(), cx)),
            )
            .unwrap();
        });
}
