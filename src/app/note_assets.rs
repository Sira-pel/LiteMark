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

/// Where an image goes: the selection when it was pasted or dropped, and the
/// text revision that selection belongs to.
struct InsertAnchor {
    range: std::ops::Range<usize>,
    revision: u64,
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
    // A literal `%` is written `%25`, or `a%41.png` would be read as `aA.png`.
    let link = format!("{ASSETS_DIR}/{}", name.replace('%', "%25"));
    Some((image_markdown("", &link), relative))
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

    /// Put image `links` in at the caret of tab `tab_id`, replacing the
    /// selection, one per line. A picture only shows when its link is alone
    /// on its line, so a line break is added before and after when the
    /// caret is mid-line. Nothing happens if the tab was closed meanwhile.
    fn insert_image_links(
        &mut self,
        tab_id: &str,
        note_dir: &Path,
        anchor: &InsertAnchor,
        links: &[String],
    ) -> bool {
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == tab_id) else {
            return false;
        };
        // Saved elsewhere meanwhile: `assets/..` would point at the wrong folder.
        if tab.file_path.as_ref().and_then(|p| p.parent()) != Some(note_dir) {
            return false;
        }
        // Where it was pasted, if the text has not changed since. Otherwise at
        // the caret now, never over a selection the user made meanwhile.
        let range = if tab.revision == anchor.revision {
            anchor.range.clone()
        } else {
            let at = tab.cursor_offset();
            at..at
        };
        let nl = tab.line_break(range.start);
        // Inside a list item or quote the link goes after the whole item, set
        // off by a blank line: split in two, the rest would lose its marker.
        let mut range = range;
        let mut gap = "";
        let in_block = {
            let line_start = crate::model::text_ops::home_offset(&tab.content, range.start);
            crate::model::document::block_prefix_in(&tab.content, line_start) > 0
        };
        // Pasting over selected text there: the selection goes first, then the
        // link follows the item as it would from a bare caret.
        if in_block && !range.is_empty() {
            let content = tab.content.clone();
            let kept_end = crate::model::text_ops::line_end(&content, range.end);
            let after = format!("{}{}", &content[..range.start], &content[range.end..]);
            let end = range.start + (kept_end - range.end);
            let text = own_lines_with(&after, end..end, &links.join(nl), nl);
            let replacement = format!("{}{nl}{text}", &after[range.start..end]);
            tab.edit(range.start..kept_end, &replacement);
            self.refresh_find();
            return true;
        }
        if range.is_empty() {
            let line_start = crate::model::text_ops::home_offset(&tab.content, range.start);
            if crate::model::document::block_prefix_in(&tab.content, line_start) > 0 {
                let end = crate::model::text_ops::line_end(&tab.content, range.start);
                range = end..end;
                gap = nl;
            }
        }
        let text = format!(
            "{gap}{}",
            own_lines_with(&tab.content, range.clone(), &links.join(nl), nl)
        );
        tab.edit(range, &text);
        self.refresh_find();
        true
    }

    fn insert_anchor(&self) -> Option<InsertAnchor> {
        let tab = self.active_tab()?;
        Some(InsertAnchor {
            range: tab.selection_range(),
            revision: tab.revision,
        })
    }

    /// The assets folder appears in the sidebar once it holds files.
    fn refresh_after_asset_write(&mut self, note_dir: &Path) {
        if note_dir.starts_with(&self.workspace.folder_path) {
            self.rescan_workspace();
        }
    }

    /// Paste an image from the clipboard into the active note. The image is
    /// converted and saved in the background (a large screenshot takes a
    /// moment to encode), then linked in the tab it was pasted into. Returns
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
        let tab_id = self.active_tab_id.clone();
        let anchor = self.insert_anchor();
        let task = cx.background_executor().spawn({
            let image = image.clone();
            let note_dir = note_dir.clone();
            async move {
                let png = clipboard_image_png(&image)
                    .map_err(|err| format!("Couldn't paste the image: {err}"))?;
                save_note_asset(&note_dir, &pasted_image_name(unix_secs), &png)
                    .map_err(|err| format!("Couldn't save the pasted image: {err}"))
            }
        });
        cx.spawn(async move |this, cx| {
            let saved = task.await;
            this.update(cx, |this, cx| {
                this.finish_image_paste(&tab_id, &note_dir, anchor, saved, cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
        true
    }

    fn finish_image_paste(
        &mut self,
        tab_id: &str,
        note_dir: &Path,
        anchor: Option<InsertAnchor>,
        saved: Result<PathBuf, String>,
        cx: &mut Context<Self>,
    ) {
        match saved {
            Ok(saved) => {
                if let Some((markdown, relative)) = asset_link(&saved) {
                    let placed = anchor.is_some_and(|a| {
                        self.insert_image_links(tab_id, note_dir, &a, &[markdown])
                    });
                    if placed {
                        self.show_toast(format!("Pasted image saved to {relative}"), cx);
                    } else {
                        self.show_toast(format!("Image saved to {relative}, not linked"), cx);
                    }
                }
                self.refresh_after_asset_write(note_dir);
            }
            Err(err) => self.show_file_error(err),
        }
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
        let skipped = paths.len() - images.len();
        let note_dir = match self.asset_target() {
            AssetTarget::NotMarkdown => return false,
            AssetTarget::Unsaved => {
                self.show_error(SAVE_FIRST_MESSAGE);
                return true;
            }
            AssetTarget::Folder(dir) => dir,
        };
        let tab_id = self.active_tab_id.clone();
        let anchor = self.insert_anchor();
        let task = cx.background_executor().spawn({
            let note_dir = note_dir.clone();
            async move {
                let assets = note_dir.join(ASSETS_DIR).canonicalize().ok();
                images
                    .iter()
                    .map(|src| {
                        // A picture already in this note's assets folder is
                        // linked as it is, not copied a second time.
                        let parent = src.parent().and_then(|p| p.canonicalize().ok());
                        if assets.is_some() && parent == assets && src.is_file() {
                            return Ok(src.clone());
                        }
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
                this.finish_image_drop(&tab_id, &note_dir, anchor, skipped, copied, cx);
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
        anchor: Option<InsertAnchor>,
        skipped: usize,
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
            let placed =
                anchor.is_some_and(|a| self.insert_image_links(tab_id, note_dir, &a, &links));
            let message = if !placed {
                format!("Saved to {ASSETS_DIR}/, not linked: the note was closed or moved")
            } else if links.len() == 1 {
                format!("Image saved to {last_relative}")
            } else {
                format!("{} images saved to {ASSETS_DIR}/", links.len())
            };
            let message = match skipped {
                0 => message,
                1 => format!("{message} (skipped 1 non-image file)"),
                n => format!("{message} (skipped {n} non-image files)"),
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

/// `text` as it should replace `range` of `content` so that it sits on lines
/// of its own: a line break is added before it unless the range starts a
/// line, and after it unless the range ends one.
#[cfg(test)]
pub(super) fn own_lines(content: &str, range: std::ops::Range<usize>, text: &str) -> String {
    own_lines_with(content, range, text, "\n")
}

/// [`own_lines`] with the note's own line break, so a CRLF note stays CRLF.
pub(super) fn own_lines_with(
    content: &str,
    range: std::ops::Range<usize>,
    text: &str,
    nl: &str,
) -> String {
    let starts_line = range.start == 0 || content[..range.start].ends_with('\n');
    let ends_line = range.end >= content.len() || content[range.end..].starts_with(['\n', '\r']);
    let mut out = String::with_capacity(text.len() + 2);
    if !starts_line {
        out.push_str(nl);
    }
    out.push_str(text);
    if !ends_line {
        out.push_str(nl);
    }
    out
}

/// Seconds since the Unix epoch, for naming a pasted image.
pub(super) fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
