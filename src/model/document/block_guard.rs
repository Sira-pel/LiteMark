//! Dividers (`---`) and code fences are hidden markers in the WYSIWYG view.
//! Typing on one opens a paragraph line instead, Enter opens a line after
//! it, and Backspace or Delete never merge prose into the hidden marker.
//! Pictures are guarded the same way in `picture.rs`.

use super::*;

/// What kind of hidden marker line the caret is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MarkerKind {
    Rule,
    ClosingFence,
    OpeningFence,
}

struct MarkerLine {
    /// Source range without the line ending.
    source: Range<usize>,
    kind: MarkerKind,
}

impl DocumentTab {
    fn marker_line_at(&mut self, offset: usize) -> Option<MarkerLine> {
        if self.view_mode != ViewMode::Wysiwyg || self.file_type != FileType::Markdown {
            return None;
        }
        let offset = offset.min(self.content.len());
        let parsed = self.parsed();
        let line = parsed.line_at_offset(offset)?;
        marker_of(line)
    }

    /// The marker line right above the line that starts at `line_start`.
    fn marker_line_before(&mut self, line_start: usize) -> Option<MarkerLine> {
        if self.view_mode != ViewMode::Wysiwyg
            || self.file_type != FileType::Markdown
            || line_start == 0
        {
            return None;
        }
        let parsed = self.parsed();
        let line = parsed.line_at_offset(line_start)?;
        if line.source.start != line_start {
            return None;
        }
        let above = parsed
            .lines
            .get((line.line_index as usize).checked_sub(1)?)?;
        marker_of(above)
    }

    /// The marker line right below the line that ends at `line_end_at`.
    fn marker_line_after(&mut self, line_end_at: usize) -> Option<MarkerLine> {
        if self.view_mode != ViewMode::Wysiwyg || self.file_type != FileType::Markdown {
            return None;
        }
        let parsed = self.parsed();
        let line = parsed.line_at_offset(line_end_at)?;
        if line.source.end != line_end_at {
            return None;
        }
        marker_of(parsed.lines.get(line.line_index as usize + 1)?)
    }

    /// Remove a whole marker line with one of its line endings (the one
    /// after it, or on the last line the one before). The caret lands where
    /// the line was; one undo step.
    fn remove_marker_line(&mut self, line: &Range<usize>, caret: usize) {
        let ending = line_ending_len(&self.content, line.end);
        let (start, end) = if ending > 0 {
            (line.start, line.end + ending)
        } else {
            let before = line_ending_len_before(&self.content, line.start);
            (line.start - before, line.end)
        };
        let undo_caret = self.cursor_offset();
        self.history.seal();
        self.next_undo_caret = Some(undo_caret);
        let caret = caret.min(start);
        self.next_redo_caret = Some(caret);
        self.edit(start..end, "");
        self.caret.move_to(floor_boundary(&self.content, caret));
        self.refresh_cursor();
        self.history.seal();
    }

    /// Typing on a divider or closing fence goes on a line of its own: after
    /// the marker line, or before a divider when the caret is at its start.
    pub(super) fn block_marker_insert(&mut self, range: Range<usize>, text: &str) -> bool {
        let Some(marker) = self.marker_line_at(range.start) else {
            return false;
        };
        if marker.kind == MarkerKind::OpeningFence
            || range.end > marker.source.end
            || text.is_empty()
        {
            return false;
        }
        let line = marker.source;
        let nl = self.line_break(line.start);
        self.history.seal();
        // Bold, italic and the like armed at the marker apply to the text:
        // open the empty line first, then type into it as usual.
        if self.has_pending_inline_formats() {
            self.next_edit_opens_span_group = true;
            self.edit(line.end..line.end, nl);
            let new_line = line.end + nl.len();
            self.caret.move_to(floor_boundary(&self.content, new_line));
            self.refresh_cursor();
            self.insert_text_in_mode(new_line..new_line, text, ViewMode::Wysiwyg);
            return true;
        }
        self.edit(line.end..line.end, &format!("{nl}{text}"));
        self.caret.move_to(floor_boundary(
            &self.content,
            line.end + nl.len() + text.len(),
        ));
        self.refresh_cursor();
        true
    }

    /// True when `range` lies on a divider or closing fence line that the
    /// WYSIWYG view hides, so typed text must not go into it.
    pub fn range_on_block_marker(&mut self, range: &Range<usize>) -> bool {
        self.block_marker_target(range).is_some()
    }

    /// Where text meant for a divider or closing fence goes instead: an
    /// empty spot right after its line.
    pub(super) fn block_marker_target(&mut self, range: &Range<usize>) -> Option<Range<usize>> {
        let marker = self.marker_line_at(range.start)?;
        if marker.kind == MarkerKind::OpeningFence || range.end > marker.source.end {
            return None;
        }
        Some(marker.source.end..marker.source.end)
    }

    /// Enter on a divider or closing fence opens a paragraph line after it;
    /// the marker stays whole.
    pub(super) fn block_marker_newline(&mut self) -> bool {
        let range = self.selection_range();
        let Some(target) = self.block_marker_target(&range) else {
            return false;
        };
        let nl = self.line_break(target.start);
        self.edit(target.clone(), nl);
        self.caret
            .move_to(floor_boundary(&self.content, target.start + nl.len()));
        self.refresh_cursor();
        true
    }

    /// Backspace next to a hidden marker. A divider is removed whole; a code
    /// fence is never joined onto, the caret steps back instead.
    pub(super) fn block_marker_backspace(&mut self) -> bool {
        let sel = self.selection_range();
        if !sel.is_empty() || sel.start == 0 {
            return false;
        }
        let at = sel.start;
        if let Some(marker) = self.marker_line_at(at) {
            let line = marker.source;
            match marker.kind {
                MarkerKind::Rule if at > line.start => {
                    self.remove_marker_line(&line, line.start);
                    return true;
                }
                MarkerKind::Rule | MarkerKind::ClosingFence | MarkerKind::OpeningFence => {
                    let before = line_ending_len_before(&self.content, line.start);
                    let prev_end = line.start - before;
                    let prev_blank =
                        before == 0 || home_offset(&self.content, prev_end) == prev_end;
                    if at == line.start && prev_blank {
                        return false;
                    }
                    let target = {
                        let parsed = self.parsed();
                        skip_hidden_left(&parsed, prev_end)
                    };
                    self.place(target, false);
                    return true;
                }
            }
        }
        // The first character of a text line right below a marker line.
        if home_offset(&self.content, at) != at || line_end(&self.content, at) == at {
            return false;
        }
        let Some(marker) = self.marker_line_before(at) else {
            return false;
        };
        let line = marker.source;
        match marker.kind {
            MarkerKind::Rule => {
                self.remove_marker_line(&line, line.start);
            }
            MarkerKind::ClosingFence | MarkerKind::OpeningFence => {
                let before = line_ending_len_before(&self.content, line.start);
                let prev_end = line.start - before;
                let target = {
                    let parsed = self.parsed();
                    skip_hidden_left(&parsed, prev_end)
                };
                self.place(target, false);
            }
        }
        true
    }

    /// Delete next to a hidden marker: a divider below the caret (or under
    /// it) is removed whole, and nothing is joined onto a code fence.
    pub(super) fn block_marker_delete(&mut self) -> bool {
        let sel = self.selection_range();
        if !sel.is_empty() {
            return false;
        }
        let at = sel.start;
        if let Some(marker) = self.marker_line_at(at) {
            let line = marker.source;
            if marker.kind == MarkerKind::Rule {
                self.remove_marker_line(&line, line.start);
            }
            return true;
        }
        if line_end(&self.content, at) != at || line_ending_len(&self.content, at) == 0 {
            return false;
        }
        let Some(marker) = self.marker_line_after(at) else {
            return false;
        };
        match marker.kind {
            MarkerKind::Rule => {
                let line = marker.source;
                self.remove_marker_line(&line, at);
                true
            }
            // An empty line before a fence may still be joined away.
            MarkerKind::ClosingFence | MarkerKind::OpeningFence => {
                home_offset(&self.content, at) != at
            }
        }
    }
}

fn marker_of(line: &ParsedLine) -> Option<MarkerLine> {
    let kind = match line.kind {
        LineKind::HorizontalRule => MarkerKind::Rule,
        LineKind::CodeFence { opening: false, .. } => MarkerKind::ClosingFence,
        LineKind::CodeFence { opening: true, .. } => MarkerKind::OpeningFence,
        _ => return None,
    };
    Some(MarkerLine {
        source: line.source.clone(),
        kind,
    })
}
