//! A line that holds only a picture (`![alt](path)`) is one block in the
//! WYSIWYG view. The caret rests only before or after it, never inside its
//! Markdown. Clicking selects the whole line; typing then goes on a new line,
//! and Backspace or Delete removes the line. F2 or a double-click shows the
//! raw Markdown until the caret leaves the line.

use super::*;
use crate::markdown::image::parse_image_line;

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
        if offset > src.end || parse_image_line(self.content.get(src.clone())?).is_none() {
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
        self.selected_picture().is_some() && self.picture_remove(false)
    }

    /// Enter on a selected picture opens a new line after it.
    pub(super) fn picture_newline(&mut self) -> bool {
        let Some(block) = self.selected_picture() else {
            return false;
        };
        let nl = self.line_break(block.start);
        self.edit(block.end..block.end, nl);
        self.caret
            .move_to(floor_boundary(&self.content, block.end + nl.len()));
        self.refresh_cursor();
        true
    }

    /// Backspace/Delete: remove the whole picture line when it is selected,
    /// or when the caret is just after it (Backspace) or before it (Delete).
    pub(super) fn picture_remove(&mut self, forward: bool) -> bool {
        let sel = self.selection_range();
        let Some(block) = self.picture_block_at(sel.start) else {
            return false;
        };
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
        self.history.seal();
        self.edit(start..end, "");
        self.caret.move_to(floor_boundary(&self.content, start));
        self.refresh_cursor();
        true
    }
}
