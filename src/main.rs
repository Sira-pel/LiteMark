#![recursion_limit = "1024"]
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use gpui::*;
use rust_embed::RustEmbed;
use std::env;
use std::path::PathBuf;

mod actions;
mod app;
mod editor_input;
mod markdown;
mod model;
mod services;
mod single_instance;
mod theme;
mod ui;

use actions::*;
use app::LiteMarkApp;

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// Test builds count live heap bytes so memory benchmarks can report what a
/// note costs. One relaxed atomic per call; release builds never see it.
#[cfg(all(test, feature = "heap-count", not(feature = "mimalloc")))]
pub(crate) mod heap_counter {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::sync::atomic::{AtomicIsize, Ordering};

    static LIVE: AtomicIsize = AtomicIsize::new(0);
    static COUNT: AtomicIsize = AtomicIsize::new(0);

    pub struct Counting;

    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let ptr = System.alloc(layout);
            if !ptr.is_null() {
                LIVE.fetch_add(layout.size() as isize, Ordering::Relaxed);
                COUNT.fetch_add(1, Ordering::Relaxed);
            }
            ptr
        }
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            let ptr = System.alloc_zeroed(layout);
            if !ptr.is_null() {
                LIVE.fetch_add(layout.size() as isize, Ordering::Relaxed);
                COUNT.fetch_add(1, Ordering::Relaxed);
            }
            ptr
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            System.dealloc(ptr, layout);
            LIVE.fetch_sub(layout.size() as isize, Ordering::Relaxed);
            COUNT.fetch_sub(1, Ordering::Relaxed);
        }
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            let new = System.realloc(ptr, layout, new_size);
            if !new.is_null() {
                LIVE.fetch_add(
                    new_size as isize - layout.size() as isize,
                    Ordering::Relaxed,
                );
            }
            new
        }
    }

    /// Bytes currently allocated by the whole process (all threads).
    pub fn live_bytes() -> isize {
        LIVE.load(Ordering::Relaxed)
    }

    /// Live allocations; each costs a heap header on top of its bytes.
    pub fn live_allocs() -> isize {
        COUNT.load(Ordering::Relaxed)
    }
}

#[cfg(all(test, feature = "heap-count", not(feature = "mimalloc")))]
#[global_allocator]
static GLOBAL: heap_counter::Counting = heap_counter::Counting;

#[derive(RustEmbed)]
#[folder = "assets"]
#[exclude = "fonts/*"]
#[exclude = "icons/hicolor/*"]
#[exclude = "windows/*"]
struct Assets;

/// The bundled fonts, deflated by `build.rs`: 1.26 MB off the binary.
///
/// Unpacking them (about 2.6 MB) at launch costs nothing measurable. On
/// Windows, gpui hands the bytes to DirectWrite's in-memory loader with no
/// owner, so DirectWrite keeps its own copy whether the bytes were borrowed
/// or owned, and the unpacked buffer is freed right after. Release builds
/// measured with 6 interleaved launches each: time to window 358 vs 367 ms,
/// idle working set 187.0 vs 188.7 MB, private 161.7 vs 162.3 MB
/// (compressed vs plain `include_bytes!`), all within run-to-run noise.
const COMPRESSED_FONTS: &[&[u8]] = &[
    include_bytes!(concat!(env!("OUT_DIR"), "/Ubuntu-R.ttf.deflate")),
    include_bytes!(concat!(env!("OUT_DIR"), "/Ubuntu-RI.ttf.deflate")),
    include_bytes!(concat!(env!("OUT_DIR"), "/Ubuntu-M.ttf.deflate")),
    include_bytes!(concat!(env!("OUT_DIR"), "/Ubuntu-B.ttf.deflate")),
    include_bytes!(concat!(env!("OUT_DIR"), "/Ubuntu-BI.ttf.deflate")),
    include_bytes!(concat!(env!("OUT_DIR"), "/UbuntuMono-R.ttf.deflate")),
    include_bytes!(concat!(env!("OUT_DIR"), "/UbuntuMono-RI.ttf.deflate")),
    include_bytes!(concat!(env!("OUT_DIR"), "/UbuntuMono-B.ttf.deflate")),
    include_bytes!(concat!(env!("OUT_DIR"), "/UbuntuMono-BI.ttf.deflate")),
];

fn load_bundled_fonts() -> Vec<std::borrow::Cow<'static, [u8]>> {
    use std::io::Read;
    COMPRESSED_FONTS
        .iter()
        .filter_map(|compressed| {
            let mut decoder = flate2::read::DeflateDecoder::new(*compressed);
            let mut out = Vec::new();
            decoder
                .read_to_end(&mut out)
                .ok()
                .map(|_| std::borrow::Cow::Owned(out))
        })
        .collect()
}

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

/// Release builds abort on panic and, on Windows, have no console, so a
/// crash used to leave no trace. Append what panicked to
/// `<config>/litemark/crash.log` (kept under 256 KiB), then run the default
/// hook. Every step is best-effort: the hook itself must never panic.
fn install_crash_log() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(dir) = dirs::config_dir().map(|base| base.join("litemark")) {
            let _ = write_crash_report(&dir, &info.to_string());
        }
        default_hook(info);
    }));
}

fn write_crash_report(dir: &std::path::Path, report: &str) -> std::io::Result<()> {
    use std::io::Write;
    const MAX_LOG_BYTES: u64 = 256 * 1024;
    std::fs::create_dir_all(dir)?;
    let path = dir.join("crash.log");
    let too_big = std::fs::metadata(&path).is_ok_and(|meta| meta.len() > MAX_LOG_BYTES);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(!too_big)
        .write(true)
        .truncate(too_big)
        .open(&path)?;
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    writeln!(
        file,
        "[unix {secs}] LiteMark {} panicked: {report}",
        env!("CARGO_PKG_VERSION")
    )
}

fn main() {
    install_crash_log();
    let args: Vec<String> = env::args().collect();
    let initial_file = args.get(1).map(PathBuf::from).filter(|p| p.exists());
    // A file opened while LiteMark is already running goes to that window.
    let incoming = match single_instance::claim(initial_file.as_deref()) {
        single_instance::Claim::HandedOff => return,
        single_instance::Claim::Primary(rx) => rx,
    };

    Application::new()
        .with_assets(EmbeddedAssets)
        .run(move |cx: &mut App| {
            // Register bundled Ubuntu fonts
            let _ = cx.text_system().add_fonts(load_bundled_fonts());

            // Bind global key shortcuts
            cx.bind_keys([
                KeyBinding::new("secondary-b", ToggleBold, None),
                KeyBinding::new("secondary-i", ToggleItalic, None),
                KeyBinding::new("secondary-u", ToggleUnderline, None),
                KeyBinding::new("secondary-shift-x", ToggleStrikethrough, None),
                KeyBinding::new("secondary-s", SaveFile, None),
                KeyBinding::new("secondary-o", OpenFile, None),
                KeyBinding::new("secondary-n", NewNote, None),
                KeyBinding::new("secondary-shift-n", NewFolder, None),
                KeyBinding::new("secondary-t", NewTab, None),
                // Sidebar tree navigation
                KeyBinding::new("up", SelectPrev, Some("Sidebar")),
                KeyBinding::new("down", SelectNext, Some("Sidebar")),
                KeyBinding::new("shift-up", ExtendSelectPrev, Some("Sidebar")),
                KeyBinding::new("shift-down", ExtendSelectNext, Some("Sidebar")),
                KeyBinding::new("enter", Confirm, Some("Sidebar")),
                KeyBinding::new("space", Confirm, Some("Sidebar")),
                KeyBinding::new("delete", DeleteNote, Some("Sidebar")),
                KeyBinding::new("secondary-d", DuplicateNote, Some("Sidebar")),
                KeyBinding::new("left", SidebarCollapse, Some("Sidebar")),
                KeyBinding::new("right", SidebarExpand, Some("Sidebar")),
                KeyBinding::new("secondary-w", CloseActiveTab, None),
                KeyBinding::new("secondary-f", FindReplace, None),
                KeyBinding::new("secondary-p", QuickOpenNotes, None),
                KeyBinding::new("secondary-shift-f", QuickOpenContents, None),
                KeyBinding::new("secondary-\\", ToggleSidebar, None),
                KeyBinding::new("f11", ToggleZenMode, None),
                KeyBinding::new("escape", CloseModal, None),
                KeyBinding::new("secondary-=", ZoomIn, None),
                KeyBinding::new("secondary-+", ZoomIn, None),
                KeyBinding::new("secondary--", ZoomOut, None),
                KeyBinding::new("secondary-0", ResetZoom, None),
                KeyBinding::new("secondary-z", UndoAction, None),
                KeyBinding::new("secondary-y", RedoAction, None),
                KeyBinding::new("secondary-shift-z", RedoAction, None),
                KeyBinding::new("secondary-1", Heading1, None),
                KeyBinding::new("secondary-2", Heading2, None),
                KeyBinding::new("secondary-3", Heading3, None),
                KeyBinding::new("secondary-4", Heading4, None),
                KeyBinding::new("secondary-5", Heading5, None),
                KeyBinding::new("secondary-6", Heading6, None),
                KeyBinding::new("secondary-e", ToggleInlineCode, None),
                KeyBinding::new("secondary-k", InsertLink, None),
                KeyBinding::new("secondary-shift-c", InsertCodeBlock, None),
                KeyBinding::new("secondary-shift-s", SaveFileAs, None),
                // Cmd+H hides the app on macOS.
                KeyBinding::new(
                    if cfg!(target_os = "macos") {
                        "cmd-alt-f"
                    } else {
                        "ctrl-h"
                    },
                    OpenReplace,
                    None,
                ),
                KeyBinding::new("secondary-shift-m", ToggleViewMode, None),
                KeyBinding::new("secondary-shift-l", ToggleLineNumbers, None),
                KeyBinding::new("secondary-shift-o", ToggleOutline, None),
                KeyBinding::new("secondary-x", Cut, None),
                KeyBinding::new("secondary-c", Copy, None),
                KeyBinding::new("secondary-v", Paste, None),
                KeyBinding::new("secondary-a", SelectAll, None),
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
                // Prompt modal navigation (delete/confirm dialogs)
                KeyBinding::new("up", PromptUp, Some("PromptModal")),
                KeyBinding::new("down", PromptDown, Some("PromptModal")),
                KeyBinding::new("left", PromptUp, Some("PromptModal")),
                KeyBinding::new("right", PromptDown, Some("PromptModal")),
                KeyBinding::new("enter", PromptConfirm, Some("PromptModal")),
                KeyBinding::new("space", PromptConfirm, Some("PromptModal")),
                // Quick open (Ctrl+P) pop-up
                KeyBinding::new("up", QuickOpenUp, Some("QuickOpen")),
                KeyBinding::new("down", QuickOpenDown, Some("QuickOpen")),
                KeyBinding::new("tab", QuickOpenSwitch, Some("QuickOpen")),
                KeyBinding::new("shift-tab", QuickOpenSwitch, Some("QuickOpen")),
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
                KeyBinding::new(
                    "ctrl-shift-backspace",
                    EditorDeleteLineStart,
                    Some("Editor"),
                ),
                KeyBinding::new("ctrl-shift-delete", EditorDeleteLineEnd, Some("Editor")),
                KeyBinding::new("backspace", EditorBackspace, Some("Editor")),
                KeyBinding::new("delete", EditorDelete, Some("Editor")),
                KeyBinding::new("enter", EditorNewline, Some("Editor")),
                KeyBinding::new("shift-enter", EditorPlainNewline, Some("Editor")),
                KeyBinding::new("f2", EditorEditPicture, Some("Editor")),
                KeyBinding::new("f3", EditorCyclePictureSize, Some("Editor")),
                KeyBinding::new("secondary-enter", EditorTableRowBelow, Some("Editor")),
                KeyBinding::new("tab", IndentAction, Some("Editor")),
                KeyBinding::new("shift-tab", OutdentAction, Some("Editor")),
                // Alt+Up/Down (Ctrl+Up/Down is Mission Control on macOS).
                KeyBinding::new("alt-up", EditorMoveLineUp, Some("Editor")),
                KeyBinding::new("alt-down", EditorMoveLineDown, Some("Editor")),
                KeyBinding::new("ctrl-tab", NextTab, None),
                KeyBinding::new("ctrl-shift-tab", PrevTab, None),
                KeyBinding::new("secondary-b", ToggleBold, Some("Editor")),
                KeyBinding::new("secondary-i", ToggleItalic, Some("Editor")),
                KeyBinding::new("secondary-u", ToggleUnderline, Some("Editor")),
                KeyBinding::new("secondary-shift-x", ToggleStrikethrough, Some("Editor")),
                KeyBinding::new("secondary-1", Heading1, Some("Editor")),
                KeyBinding::new("secondary-2", Heading2, Some("Editor")),
                KeyBinding::new("secondary-3", Heading3, Some("Editor")),
                KeyBinding::new("secondary-4", Heading4, Some("Editor")),
                KeyBinding::new("secondary-5", Heading5, Some("Editor")),
                KeyBinding::new("secondary-6", Heading6, Some("Editor")),
                KeyBinding::new("secondary-e", ToggleInlineCode, Some("Editor")),
                KeyBinding::new("secondary-k", InsertLink, Some("Editor")),
                KeyBinding::new("secondary-shift-c", InsertCodeBlock, Some("Editor")),
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
                KeyBinding::new(
                    "ctrl-shift-backspace",
                    EditorDeleteLineStart,
                    Some("LineField"),
                ),
                KeyBinding::new("ctrl-shift-delete", EditorDeleteLineEnd, Some("LineField")),
                KeyBinding::new("backspace", EditorBackspace, Some("LineField")),
                KeyBinding::new("delete", EditorDelete, Some("LineField")),
                KeyBinding::new("enter", FindNextMatch, Some("LineField")),
                KeyBinding::new("shift-enter", FindPrevMatch, Some("LineField")),
            ]);

            // macOS text navigation (Option = word, Cmd = line/document).
            // Ctrl+Arrows are Mission Control there and Macs have no Home/End.
            if cfg!(target_os = "macos") {
                for ctx in ["Editor", "LineField"] {
                    let c = Some(ctx);
                    cx.bind_keys([
                        KeyBinding::new("alt-left", EditorWordLeft, c),
                        KeyBinding::new("alt-right", EditorWordRight, c),
                        KeyBinding::new("alt-shift-left", EditorSelectWordLeft, c),
                        KeyBinding::new("alt-shift-right", EditorSelectWordRight, c),
                        KeyBinding::new("cmd-left", EditorHome, c),
                        KeyBinding::new("cmd-right", EditorEnd, c),
                        KeyBinding::new("cmd-shift-left", EditorSelectHome, c),
                        KeyBinding::new("cmd-shift-right", EditorSelectEnd, c),
                        KeyBinding::new("alt-backspace", EditorDeleteWordLeft, c),
                        KeyBinding::new("alt-delete", EditorDeleteWordRight, c),
                        KeyBinding::new("cmd-backspace", EditorDeleteLineStart, c),
                        KeyBinding::new("cmd-delete", EditorDeleteLineEnd, c),
                    ]);
                }
                cx.bind_keys([
                    KeyBinding::new("cmd-up", EditorDocStart, Some("Editor")),
                    KeyBinding::new("cmd-down", EditorDocEnd, Some("Editor")),
                    KeyBinding::new("cmd-shift-up", EditorSelectDocStart, Some("Editor")),
                    KeyBinding::new("cmd-shift-down", EditorSelectDocEnd, Some("Editor")),
                    KeyBinding::new("cmd-up", EditorHome, Some("LineField")),
                    KeyBinding::new("cmd-down", EditorEnd, Some("LineField")),
                    KeyBinding::new("cmd-shift-up", EditorSelectHome, Some("LineField")),
                    KeyBinding::new("cmd-shift-down", EditorSelectEnd, Some("LineField")),
                ]);

                // Emacs-style Ctrl chords every native macOS text field supports.
                for ctx in ["Editor", "LineField"] {
                    let c = Some(ctx);
                    cx.bind_keys([
                        KeyBinding::new("ctrl-a", EditorHome, c),
                        KeyBinding::new("ctrl-e", EditorEnd, c),
                        KeyBinding::new("ctrl-k", EditorDeleteLineEnd, c),
                        KeyBinding::new("ctrl-f", EditorRight, c),
                        KeyBinding::new("ctrl-b", EditorLeft, c),
                    ]);
                }
                cx.bind_keys([
                    KeyBinding::new("ctrl-n", EditorDown, Some("Editor")),
                    KeyBinding::new("ctrl-p", EditorUp, Some("Editor")),
                    KeyBinding::new("cmd-,", OpenSettingsMenu, None),
                    KeyBinding::new("cmd-m", MinimizeWindow, None),
                ]);

                // Cmd+Q, and quit once the last window is gone (macOS keeps
                // the process alive otherwise).
                cx.bind_keys([KeyBinding::new("cmd-q", QuitApp, None)]);
                cx.on_window_closed(|cx| {
                    if cx.windows().is_empty() {
                        cx.quit();
                    }
                })
                .detach();
            }

            let opened = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(960.0), px(640.0)),
                        cx,
                    ))),
                    window_decorations: Some(WindowDecorations::Client),
                    window_min_size: Some(size(px(360.0), px(360.0))),
                    app_id: Some("litemark".into()),
                    titlebar: window_titlebar(),
                    ..Default::default()
                },
                move |_, cx| cx.new(|cx| LiteMarkApp::new(initial_file.clone(), cx)),
            );
            // No display server, or the window system refused the window:
            // say so instead of panicking with a raw unwrap message.
            let window = match opened {
                Ok(window) => window,
                Err(err) => {
                    eprintln!("LiteMark couldn't open its window: {err:#}");
                    std::process::exit(1);
                }
            };
            // Files other launches hand over open as tabs here.
            if let Some(incoming) = incoming {
                // A thread blocks on the handover channel and wakes the UI
                // only when a path arrives, instead of polling on a timer.
                let (tx, mut woken) = futures::channel::mpsc::unbounded::<PathBuf>();
                let forwarder = std::thread::Builder::new()
                    .name("incoming-files".into())
                    .spawn(move || {
                        while let Ok(path) = incoming.recv() {
                            if tx.unbounded_send(path).is_err() {
                                break;
                            }
                        }
                    });
                if forwarder.is_err() {
                    eprintln!("litemark: could not watch for files from other launches");
                }
                cx.spawn(async move |cx| {
                    use futures::StreamExt;
                    while let Some(path) = woken.next().await {
                        let updated = window.update(cx, |app, window, cx| {
                            app.open_path(path, cx);
                            // Redraw now, or the new tab only shows once the
                            // user next moves the mouse over the window.
                            cx.notify();
                            window.activate_window();
                        });
                        // The window is gone: stop waiting.
                        if updated.is_err() {
                            return;
                        }
                    }
                })
                .detach();
            }
        });
}

/// macOS only builds a resizable/zoomable window when a titlebar is given;
/// with `None` the style mask lacks the resizable and miniaturizable flags, so
/// `zoom_window()` and `minimize_window()` silently do nothing. Keep the
/// titlebar transparent (our header bar draws the controls) and park the
/// native traffic lights off-screen. Other platforms keep `None`.
fn window_titlebar() -> Option<TitlebarOptions> {
    if cfg!(target_os = "macos") {
        Some(TitlebarOptions {
            title: Some("LiteMark".into()),
            appears_transparent: true,
            traffic_light_position: Some(point(px(-100.0), px(-100.0))),
        })
    } else {
        None
    }
}

#[cfg(test)]
mod crash_log_tests {
    use super::write_crash_report;

    #[test]
    fn crash_reports_append_and_the_log_is_capped() {
        let dir = std::env::temp_dir().join(format!("litemark-crash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        write_crash_report(&dir, "first").unwrap();
        write_crash_report(&dir, "second").unwrap();
        let log = std::fs::read_to_string(dir.join("crash.log")).unwrap();
        assert!(log.contains("panicked: first") && log.contains("panicked: second"));

        // Past the cap the next report starts the file over.
        std::fs::write(dir.join("crash.log"), vec![b'x'; 300 * 1024]).unwrap();
        write_crash_report(&dir, "third").unwrap();
        let log = std::fs::read_to_string(dir.join("crash.log")).unwrap();
        assert!(log.starts_with("[unix ") && log.contains("panicked: third"));
        assert!(log.len() < 1024);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bundled_fonts_decompress_bit_for_bit() {
        let fonts = super::load_bundled_fonts();
        let names = [
            "Ubuntu-R.ttf",
            "Ubuntu-RI.ttf",
            "Ubuntu-M.ttf",
            "Ubuntu-B.ttf",
            "Ubuntu-BI.ttf",
            "UbuntuMono-R.ttf",
            "UbuntuMono-RI.ttf",
            "UbuntuMono-B.ttf",
            "UbuntuMono-BI.ttf",
        ];
        assert_eq!(fonts.len(), names.len());
        for (font, name) in fonts.iter().zip(names) {
            let raw = std::fs::read(std::path::Path::new("assets/fonts").join(name)).unwrap();
            assert_eq!(font.as_ref(), raw.as_slice(), "font mismatch for {name}");
        }
    }
}
