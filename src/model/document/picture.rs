//! A line that holds only a picture (`![alt](path)`) is one block in the
//! WYSIWYG view. The caret rests only before or after it, never inside its
//! Markdown. Clicking selects the whole line; typing then goes on a new line,
//! and Backspace or Delete removes the line. F2 or a double-click shows the
//! raw Markdown until the caret leaves the line.

use super::*;
use crate::markdown::image::{
    is_image_block, parse_image_line, parse_image_row, set_image_size, set_image_width,
};

/// Widths in pixels that `cycle_picture_size` steps through: small, medium, large.
pub const PICTURE_SIZES: [u32; 3] = [200, 400, 600];

impl DocumentTab {
    /// The source range (without line ending) of the picture block at
    /// `offset`, or `None` when the line is not a picture, the view is raw,
    /// or its Markdown is being edited.
    pub fn picture_block_at(&mut self, offset: usize) -> Option<Range<usize>> {
        if self.view_mode != ViewMode::Wysiwyg || self.file_type != FileType::Markdown {
            return None;
        }
        let offset = offset.min(self.content.len());
        let parsed = self.parsed();
        let line = parsed.line_at_offset(offset)?;
        if !matches!(line.kind, LineKind::Paragraph) {
            return None;
        }
        let src = line.source.clone();
        if offset > src.end || !is_image_block(self.content.get(src.clone())?) {
            return None;
        }
        if self.picture_raw_line == Some(src.start) && self.caret_within(&src) {
            return None;
        }
        Some(src)
    }

    fn caret_within(&self, range: &Range<usize>) -> bool {
        self.caret.range.start >= range.start && self.caret.range.end <= range.end
    }

    /// The picture block that is selected as a whole (outlined).
    pub fn selected_picture(&mut self) -> Option<Range<usize>> {
        let sel = self.selection_range();
        if sel.is_empty() {
            return None;
        }
        let block = self.picture_block_at(sel.start)?;
        (block == sel).then_some(block)
    }

    /// The one image of a thumbnail row that is selected by itself.
    pub fn selected_image(&mut self) -> Option<Range<usize>> {
        let sel = self.selection_range();
        if sel.is_empty() {
            return None;
        }
        let block = self.picture_block_at(sel.start)?;
        if sel == block || sel.end > block.end {
            return None;
        }
        let images = parse_image_row(self.content.get(block.clone())?)?;
        images
            .iter()
            .any(|(range, _)| {
                range.start + block.start == sel.start && range.end + block.start == sel.end
            })
            .then_some(sel)
    }

    /// Select a picture block as a whole.
    pub fn select_picture(&mut self, block: Range<usize>) {
        self.pending_formats = ActiveFormats::default();
        self.caret.range = block;
        self.caret.reversed = false;
        self.caret.preferred_col = None;
        self.refresh_cursor();
    }

    /// Show the raw Markdown of the picture the caret is on or has selected.
    pub fn show_picture_source(&mut self) -> bool {
        let at = self.selection_range().start;
        let Some(block) = self.picture_block_at(at) else {
            return false;
        };
        self.picture_raw_line = Some(block.start);
        self.caret.move_to(block.end);
        self.refresh_cursor();
        true
    }

    /// Set the width of the picture at the caret or selection (`None` removes
    /// it, back to fitting the column) as one undo step; the picture stays
    /// selected.
    pub fn set_picture_width(&mut self, width: Option<u32>) -> bool {
        if self.is_read_only() {
            return false;
        }
        let at = self.selection_range().start;
        let Some(block) = self.picture_block_at(at) else {
            return false;
        };
        // In a row only a selected image is resized, and only that one.
        let block = self.selected_image().unwrap_or(block);
        let Some(line) = self.content.get(block.clone()) else {
            return false;
        };
        let Some(new) = set_image_width(line, width) else {
            return false;
        };
        if new == line {
            return true;
        }
        let undo_sel = self.selection_snap();
        let undo_caret = self.cursor_offset();
        self.history.seal();
        let end = block.start + new.len();
        self.next_undo_caret = Some(undo_caret);
        self.next_undo_sel = undo_sel;
        self.next_redo_caret = Some(end);
        self.next_redo_sel = Some(SelectionSnap {
            start: block.start,
            end,
            reversed: false,
        });
        self.edit(block.clone(), &new);
        self.select_picture(block.start..end);
        self.history.seal();
        true
    }

    /// Set the width of the picture at `range` (a lone picture, or one image
    /// of a row), whatever the selection is. One undo step; the picture is
    /// selected afterwards. False, and nothing changed, when `range` is not a
    /// picture or the width is already `width`.
    #[cfg(test)]
    pub fn set_picture_width_at(&mut self, range: Range<usize>, width: Option<u32>) -> bool {
        self.set_picture_size_at(range, width, None)
    }

    /// [`Self::set_picture_width_at`] with a height as well (`|200x100`).
    pub fn set_picture_size_at(
        &mut self,
        range: Range<usize>,
        width: Option<u32>,
        height: Option<u32>,
    ) -> bool {
        if self.is_read_only() {
            return false;
        }
        let Some(block) = self.picture_block_at(range.start) else {
            return false;
        };
        if range.end > block.end {
            return false;
        }
        let Some(line) = self.content.get(range.clone()) else {
            return false;
        };
        let Some(new) = set_image_size(line, width, height) else {
            return false;
        };
        if new == line {
            return false;
        }
        let undo_sel = self.selection_snap();
        let undo_caret = self.cursor_offset();
        self.history.seal();
        let end = range.start + new.len();
        self.next_undo_caret = Some(undo_caret);
        self.next_undo_sel = undo_sel;
        self.next_redo_caret = Some(end);
        self.next_redo_sel = Some(SelectionSnap {
            start: range.start,
            end,
            reversed: false,
        });
        self.edit(range.clone(), &new);
        self.select_picture(range.start..end);
        self.history.seal();
        true
    }

    /// Step the picture through Small, Medium, Large and back to its natural
    /// fit. A width that is not a preset starts again at Small.
    pub fn cycle_picture_size(&mut self) -> bool {
        let at = self.selection_range().start;
        let Some(block) = self.picture_block_at(at) else {
            return false;
        };
        let block = self.selected_image().unwrap_or(block);
        let current = self
            .content
            .get(block)
            .and_then(parse_image_line)
            .and_then(|image| image.width);
        let next = match current {
            None => Some(PICTURE_SIZES[0]),
            Some(w) => PICTURE_SIZES
                .iter()
                .position(|&size| size == w)
                .map_or(Some(PICTURE_SIZES[0]), |i| {
                    PICTURE_SIZES.get(i + 1).copied()
                }),
        };
        self.set_picture_width(next)
    }

    /// End the current undo group (input-method text has been committed).
    pub fn seal_history(&mut self) {
        self.history.seal();
    }

    /// Drop the raw-editing flag once the caret has left that line.
    pub(super) fn expire_picture_raw(&mut self) {
        if let Some(start) = self.picture_raw_line {
            let end = line_end(&self.content, start.min(self.content.len()));
            if !self.caret_within(&(start..end)) {
                self.picture_raw_line = None;
            }
        }
    }

    /// Typing at a picture block goes on its own line: after the picture when
    /// it is selected or the caret is at its end, before it at its start.
    pub(super) fn picture_insert(&mut self, range: Range<usize>, text: &str) -> bool {
        let Some(block) = self.picture_block_at(range.start) else {
            return false;
        };
        if range.end > block.end || text.is_empty() {
            return false;
        }
        let nl = self.line_break(block.start);
        self.history.seal();
        // Bold, italic and the like armed beside the picture apply to the text:
        // open the empty line first, then type into it as usual.
        if self.has_pending_inline_formats() {
            let before = range.is_empty() && range.start == block.start;
            let at = if before { block.start } else { block.end };
            // The line break and the text typed into it are one undo step.
            self.next_edit_opens_span_group = true;
            self.edit(at..at, nl);
            let line = if before { at } else { at + nl.len() };
            self.caret.move_to(floor_boundary(&self.content, line));
            self.refresh_cursor();
            self.insert_text_in_mode(line..line, text, ViewMode::Wysiwyg);
            return true;
        }
        if range.is_empty() && range.start == block.start {
            self.edit(block.start..block.start, &format!("{text}{nl}"));
            self.caret
                .move_to(floor_boundary(&self.content, block.start + text.len()));
        } else {
            self.edit(block.end..block.end, &format!("{nl}{text}"));
            self.caret.move_to(floor_boundary(
                &self.content,
                block.end + nl.len() + text.len(),
            ));
        }
        self.refresh_cursor();
        true
    }

    /// True when `range` lies on a picture block shown as a picture.
    pub fn range_on_picture(&mut self, range: &Range<usize>) -> bool {
        self.picture_block_at(range.start)
            .is_some_and(|block| range.end <= block.end)
    }

    /// Where text meant for a picture block goes instead: before the picture
    /// when the caret is at its start, otherwise right after it.
    pub fn picture_target(&mut self, range: &Range<usize>) -> Option<Range<usize>> {
        let block = self.picture_block_at(range.start)?;
        if range.end > block.end {
            return None;
        }
        Some(if range.is_empty() && range.start == block.start {
            block.start..block.start
        } else {
            block.end..block.end
        })
    }

    /// Open an empty line next to the picture for text that is about to be
    /// written (input methods edit the text directly). Returns the empty
    /// range to write into.
    pub fn picture_open_line(&mut self, range: &Range<usize>) -> Option<Range<usize>> {
        let target = self.picture_target(range)?;
        let nl = self.line_break(target.start);
        self.history.seal();
        // The line break and the text written into it are one undo step; if
        // the input is cancelled the step vanishes.
        self.next_edit_opens_span_group = true;
        self.edit(target.clone(), nl);
        let at = if target.start == 0 || self.content[..target.start].ends_with(['\n', '\r']) {
            // Before the picture: the new empty line is where the picture was.
            target.start
        } else {
            target.start + nl.len()
        };
        let before = at == target.start;
        self.ime_opened_line = Some((target.start..target.start + nl.len(), before));
        self.caret.move_to(floor_boundary(&self.content, at));
        self.refresh_cursor();
        Some(at..at)
    }

    /// Input-method text was cancelled: take back the line opened for it.
    pub fn cancel_picture_ime_line(&mut self) {
        let Some((range, before)) = self.ime_opened_line.take() else {
            return;
        };
        let nl_ok = self
            .content
            .get(range.clone())
            .is_some_and(|s| s == "\n" || s == "\r\n");
        let empty = if before {
            home_offset(&self.content, range.start) == range.start
        } else {
            line_end(&self.content, range.end) == range.end
        };
        if nl_ok && empty {
            self.edit(range.clone(), "");
            self.caret
                .move_to(floor_boundary(&self.content, range.start));
            self.refresh_cursor();
        }
    }

    /// Commits any active IME composition (preserving composed text and sealing
    /// its undo group) or cancels an empty IME-opened picture line.
    pub fn finish_ime_composition(&mut self) {
        if let Some(range) = self.marked_range.take() {
            self.ime_opened_line = None;
            let end = floor_boundary(&self.content, range.end.min(self.content.len()));
            self.caret.move_to(end);
            self.history.seal();
            self.refresh_cursor();
        } else {
            self.cancel_picture_ime_line();
        }
    }

    /// Backspace at the start of the text below a picture selects the
    /// picture instead of joining the text onto its Markdown.
    pub(super) fn picture_guard_join_back(&mut self) -> bool {
        let sel = self.selection_range();
        if !sel.is_empty() || sel.start == 0 {
            return false;
        }
        let at = sel.start;
        if home_offset(&self.content, at) != at || line_end(&self.content, at) == at {
            return false;
        }
        let prev_end = at - line_ending_len_before(&self.content, at);
        match self.picture_block_at(prev_end) {
            Some(block) if block.end == prev_end => {
                self.select_picture(block);
                true
            }
            _ => false,
        }
    }

    /// Delete at the end of the line above a picture selects the picture
    /// instead of joining the picture's Markdown onto that line.
    pub(super) fn picture_guard_join(&mut self) -> bool {
        let sel = self.selection_range();
        if !sel.is_empty() {
            return false;
        }
        let ending = line_ending_len(&self.content, sel.start);
        if ending == 0 || home_offset(&self.content, sel.start) == sel.start {
            return false;
        }
        let next = sel.start + ending;
        match self.picture_block_at(next) {
            Some(block) if block.start == next => {
                self.select_picture(block);
                true
            }
            _ => false,
        }
    }

    /// Cut: a selected picture goes with its whole line. Returns false when
    /// the selection is not a picture, so the caller cuts as usual.
    pub fn cut_selected_picture(&mut self) -> bool {
        (self.selected_picture().is_some() || self.selected_image().is_some())
            && self.picture_remove(false)
    }

    /// Enter on a selected picture opens a new line after it.
    pub(super) fn picture_newline(&mut self) -> bool {
        // One image of a row selected opens the line after the whole row.
        let block = match self.selected_picture() {
            Some(block) => block,
            None => {
                let at = self.selection_range().start;
                match self
                    .selected_image()
                    .and_then(|_| self.picture_block_at(at))
                {
                    Some(block) => block,
                    None => return false,
                }
            }
        };
        let nl = self.line_break(block.start);
        self.edit(block.end..block.end, nl);
        self.caret
            .move_to(floor_boundary(&self.content, block.end + nl.len()));
        self.refresh_cursor();
        true
    }

    /// Remove the selected image `image` from the row `block`, with the
    /// blanks that separated it from its neighbour. The neighbour that takes
    /// its place becomes the selection; with one image left the line is an
    /// ordinary picture again.
    fn remove_row_image(&mut self, block: Range<usize>, image: Range<usize>) -> bool {
        let Some(images) = self.content.get(block.clone()).and_then(parse_image_row) else {
            return false;
        };
        let spans: Vec<Range<usize>> = images
            .iter()
            .map(|(r, _)| r.start + block.start..r.end + block.start)
            .collect();
        let Some(i) = spans.iter().position(|r| *r == image) else {
            return false;
        };
        let (cut, keep) = match spans.get(i + 1) {
            Some(next) => (image.start..next.start, i),
            None => (spans[i - 1].end..image.end, i - 1),
        };
        let undo_sel = self.selection_snap().or(Some(SelectionSnap {
            start: image.start,
            end: image.end,
            reversed: false,
        }));
        let undo_caret = self.cursor_offset();
        let redo_range = if spans.len() > 2 {
            let survivor_idx = if keep < i { keep } else { keep + 1 };
            let r = &spans[survivor_idx];
            if survivor_idx > i {
                (r.start - cut.len())..(r.end - cut.len())
            } else {
                r.clone()
            }
        } else {
            block.start..(block.end - cut.len())
        };
        self.history.seal();
        self.next_undo_caret = Some(undo_caret);
        self.next_undo_sel = undo_sel;
        self.next_redo_caret = Some(redo_range.end);
        self.next_redo_sel = Some(SelectionSnap {
            start: redo_range.start,
            end: redo_range.end,
            reversed: false,
        });
        self.edit(cut.clone(), "");
        let end = block.end - cut.len();
        let line = block.start..end;
        // `keep` indexes the survivors: the next image moved into this slot,
        // or the previous one is last.
        let survivors = self.content.get(line.clone()).and_then(parse_image_row);
        match survivors {
            Some(rest) => {
                let r = &rest[keep.min(rest.len() - 1)].0;
                self.select_picture(r.start + block.start..r.end + block.start);
            }
            None => self.select_picture(line),
        }
        self.history.seal();
        true
    }

    /// Backspace/Delete: remove the whole picture line when it is selected,
    /// or when the caret is just after it (Backspace) or before it (Delete).
    pub(super) fn picture_remove(&mut self, forward: bool) -> bool {
        let sel = self.selection_range();
        let Some(block) = self.picture_block_at(sel.start) else {
            return false;
        };
        if self.selected_image().is_some() {
            return self.remove_row_image(block, sel);
        }
        // Backspace at the start of a picture would join its Markdown onto
        // the line above: move the caret up there instead.
        if !forward && sel.is_empty() && sel.start == block.start && block.start > 0 {
            let before = line_ending_len_before(&self.content, block.start);
            let prev_end = block.start - before;
            if home_offset(&self.content, prev_end) < prev_end {
                self.place(prev_end, false);
                return true;
            }
            return false;
        }
        // Delete just after a picture would join the next line's text onto
        // its Markdown: step to that line instead.
        if forward && sel.is_empty() && sel.start == block.end {
            let ending = line_ending_len(&self.content, block.end);
            let next = block.end + ending;
            if ending > 0 && line_end(&self.content, next) > next {
                self.place(next, false);
                return true;
            }
            return false;
        }
        // Backspace right after a row, or Delete right before it, selects the
        // image next to the caret first; the next press removes just that one.
        if sel.is_empty()
            && (forward && sel.start == block.start || !forward && sel.start == block.end)
        {
            if let Some(images) = self.content.get(block.clone()).and_then(parse_image_row) {
                let pick = if forward {
                    images.first()
                } else {
                    images.last()
                };
                if let Some((range, _)) = pick {
                    self.select_picture(range.start + block.start..range.end + block.start);
                    return true;
                }
            }
        }
        let hit = if sel.is_empty() {
            if forward {
                sel.start == block.start
            } else {
                sel.start == block.end
            }
        } else {
            sel == block
        };
        if !hit {
            return false;
        }
        let ending = line_ending_len(&self.content, block.end);
        let (start, end) = if ending > 0 {
            (block.start, block.end + ending)
        } else {
            // Last line: take the line break before it instead.
            let before = line_ending_len_before(&self.content, block.start);
            (block.start - before, block.end)
        };
        let undo_sel = self.selection_snap();
        let undo_caret = self.cursor_offset();
        self.history.seal();
        self.next_undo_caret = Some(undo_caret);
        self.next_undo_sel = undo_sel;
        self.next_redo_caret = Some(start);
        self.next_redo_sel = Some(SelectionSnap {
            start,
            end: start,
            reversed: false,
        });
        self.edit(start..end, "");
        self.caret.move_to(floor_boundary(&self.content, start));
        self.refresh_cursor();
        true
    }
}
