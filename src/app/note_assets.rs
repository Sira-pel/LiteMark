//! Pasting and dropping images into a saved Markdown note: the image is saved
//! under `assets/` next to the note and a link to it goes in at the caret.

use super::*;
use crate::markdown::image::image_markdown;
use crate::model::clipboard::clipboard_image_png;

/// Shown instead of pasting when the note has no folder yet.
pub(super) const SAVE_FIRST_MESSAGE: &str =
    "Save this note first: pasted and dropped images are kept in an assets folder next to it.";

/// Image file types accepted when files are dropped on the editor.
const DROPPABLE_EXTENSIONS: [&str; 6] = ["png", "jpg", "jpeg", "gif", "webp", "svg"];

/// Where images for the active tab can go.
enum AssetTarget {
    /// Not a Markdown note: nothing to do.
    NotMarkdown,
    /// A Markdown note that has not been saved: no folder yet.
    Unsaved,
    /// The folder the note lives in.
    Folder(PathBuf),
}

/// Whether `path` names an image type the editor can show.
pub(super) fn is_droppable_image(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            DROPPABLE_EXTENSIONS
                .iter()
                .any(|ok| ext.eq_ignore_ascii_case(ok))
        })
}

/// The Markdown and the `assets/<name>` path for a file saved at `saved`.
pub(super) fn asset_link(saved: &Path) -> Option<(String, String)> {
    let name = saved.file_name()?.to_str()?;
    let relative = format!("{ASSETS_DIR}/{name}");
    Some((image_markdown("", &relative), relative))
}

impl LiteMarkApp {
    fn asset_target(&self) -> AssetTarget {
        let Some(tab) = self.active_tab() else {
            return AssetTarget::NotMarkdown;
        };
        if tab.file_type != FileType::Markdown {
            return AssetTarget::NotMarkdown;
        }
        match tab.file_path.as_ref().and_then(|path| path.parent()) {
            Some(dir) => AssetTarget::Folder(dir.to_path_buf()),
            None => AssetTarget::Unsaved,
        }
    }

    /// Put `text` in at the caret, replacing the selection.
    fn insert_at_caret(&mut self, text: &str) {
        if let Some(tab) = self.active_tab_mut() {
            let range = tab.selection_range();
            tab.edit(range, text);
        }
        self.refresh_find();
    }

    /// The assets folder appears in the sidebar once it holds files.
    fn refresh_after_asset_write(&mut self, note_dir: &Path) {
        if note_dir.starts_with(&self.workspace.folder_path) {
            self.rescan_workspace();
        }
    }

    /// Paste an image from the clipboard into the active note. Returns
    /// `false` when the active tab is not a Markdown note, so the caller can
    /// carry on with an ordinary paste.
    pub(super) fn paste_clipboard_image(
        &mut self,
        image: &gpui::Image,
        unix_secs: u64,
        cx: &mut Context<Self>,
    ) -> bool {
        let note_dir = match self.asset_target() {
            AssetTarget::NotMarkdown => return false,
            AssetTarget::Unsaved => {
                self.show_error(SAVE_FIRST_MESSAGE);
                return true;
            }
            AssetTarget::Folder(dir) => dir,
        };
        let png = match clipboard_image_png(image) {
            Ok(png) => png,
            Err(err) => {
                self.show_error(format!("Couldn't paste the image: {err}"));
                return true;
            }
        };
        match save_note_asset(&note_dir, &pasted_image_name(unix_secs), &png) {
            Ok(saved) => {
                if let Some((markdown, relative)) = asset_link(&saved) {
                    self.insert_at_caret(&markdown);
                    self.show_toast(format!("Pasted image saved to {relative}"), cx);
                }
                self.refresh_after_asset_write(&note_dir);
            }
            Err(err) => self.show_file_error(format!("Couldn't save the pasted image: {err}")),
        }
        true
    }

    /// Copy dropped image files into the active note's `assets` folder and
    /// link each one at the caret. The files are copied in the background
    /// (an image can be large), and the links go into the tab the files were
    /// dropped on once copying is done. Returns `false` when nothing dropped
    /// is an image, or the tab is not a Markdown note.
    pub(super) fn drop_image_files(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) -> bool {
        let images: Vec<PathBuf> = paths
            .iter()
            .filter(|p| is_droppable_image(p))
            .cloned()
            .collect();
        if images.is_empty() {
            return false;
        }
        let note_dir = match self.asset_target() {
            AssetTarget::NotMarkdown => return false,
            AssetTarget::Unsaved => {
                self.show_error(SAVE_FIRST_MESSAGE);
                return true;
            }
            AssetTarget::Folder(dir) => dir,
        };
        let tab_id = self.active_tab_id.clone();
        let task = cx.background_executor().spawn({
            let note_dir = note_dir.clone();
            async move {
                images
                    .iter()
                    .map(|src| {
                        copy_into_note_assets(&note_dir, src).map_err(|err| {
                            let name = src.file_name().and_then(|n| n.to_str()).unwrap_or("image");
                            format!("Couldn't add {name}: {err}")
                        })
                    })
                    .collect::<Vec<_>>()
            }
        });
        cx.spawn(async move |this, cx| {
            let copied = task.await;
            this.update(cx, |this, cx| {
                this.finish_image_drop(&tab_id, &note_dir, copied, cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
        true
    }

    /// Link the copied images in tab `tab_id`, at its caret, and report.
    fn finish_image_drop(
        &mut self,
        tab_id: &str,
        note_dir: &Path,
        copied: Vec<Result<PathBuf, String>>,
        cx: &mut Context<Self>,
    ) {
        let mut links = Vec::new();
        let mut last_relative = String::new();
        let mut first_error = None;
        for result in copied {
            match result {
                Ok(saved) => {
                    if let Some((markdown, relative)) = asset_link(&saved) {
                        links.push(markdown);
                        last_relative = relative;
                    }
                }
                Err(err) => {
                    first_error.get_or_insert(err);
                }
            }
        }
        if !links.is_empty() {
            // The tab may have been closed meanwhile: the files stay saved.
            if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == tab_id) {
                let range = tab.selection_range();
                tab.edit(range, &links.join("\n"));
                self.refresh_find();
            }
            let message = if links.len() == 1 {
                format!("Image saved to {last_relative}")
            } else {
                format!("{} images saved to {ASSETS_DIR}/", links.len())
            };
            self.show_toast(message, cx);
            self.refresh_after_asset_write(note_dir);
        }
        if let Some(err) = first_error {
            self.show_file_error(err);
        }
    }

    /// Files dropped on the editor. Only images are handled for now.
    pub(super) fn handle_dropped_paths(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        self.drop_image_files(paths, cx);
    }
}

/// Seconds since the Unix epoch, for naming a pasted image.
pub(super) fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
