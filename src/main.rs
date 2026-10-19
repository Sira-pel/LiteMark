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
            // Bind global key shortcuts
            cx.bind_keys([
                KeyBinding::new("ctrl-b", ToggleBold, None),
                KeyBinding::new("ctrl-i", ToggleItalic, None),
                KeyBinding::new("ctrl-u", ToggleUnderline, None),
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
            ]);

            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(960.0), px(640.0)),
                        cx,
                    ))),
                    titlebar: Some(TitlebarOptions {
                        title: Some("LiteMark".into()),
                        appears_transparent: true,
                        traffic_light_position: None,
                    }),
                    ..Default::default()
                },
                move |_, cx| cx.new(|_| LiteMarkApp::new(initial_file.clone())),
            )
            .unwrap();
        });
}
