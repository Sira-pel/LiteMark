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
                "fonts/UbuntuMono-R.ttf",
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
