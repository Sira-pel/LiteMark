//! Caret movement and selection.

use super::*;

impl DocumentTab {
    pub fn move_left(&mut self, extend: bool) {
        self.pending_formats = ActiveFormats::default();
        let target = if !extend && !self.caret.range.is_empty() {
            self.selection_range().start
        } else {
            let pos = self.cursor_offset();
            let ending = line_ending_len_before(&self.content, pos);
            if ending > 0 {
                pos - ending
            } else {
                previous_grapheme(&self.content, pos)
            }
        };
        self.place(target, extend);
    }

    pub fn move_right(&mut self, extend: bool) {
        self.pending_formats = ActiveFormats::default();
        let target = if !extend && !self.caret.range.is_empty() {
            self.selection_range().end
        } else {
            let pos = self.cursor_offset();
            let ending = line_ending_len(&self.content, pos);
            if ending > 0 {
                pos + ending
            } else {
                next_grapheme(&self.content, pos)
            }
        };
        self.place(target, extend);
    }

    pub fn move_left_in_mode(&mut self, extend: bool, mode: ViewMode) {
        self.pending_formats = ActiveFormats::default();
        if mode == ViewMode::Raw {
            self.move_left(extend);
            return;
        }
        if !extend && !self.caret.range.is_empty() {
            let start = self.selection_range().start;
            self.place(start, false);
            return;
        }
        let parsed = self.parsed();
        let mut pos = self.cursor_offset();
        if pos == 0 {
            return;
        }
        if let Some(pline) = parsed.line_at_offset(pos) {
            if extend && pos > visible_line_end(pline, self.view_mode) && pos <= pline.source.end {
                return self.move_left(true);
            }
            if let LineKind::TableRow { ref body, .. } = pline.kind {
                let TableRowBody {
                    cells,
                    raw_cells,
                    table_source,
                    ..
                } = &**body;
                if let Some(c_idx) = raw_cells
                    .iter()
                    .position(|rc| pos >= rc.start && pos <= rc.end)
                {
                    if pos <= cells[c_idx].start {
                        if c_idx > 0 {
                            let target = skip_hidden_left(&parsed, cells[c_idx - 1].end);
                            self.place(target, extend);
                        } else {
                            let prev_row_cell_end =
                                parsed.lines[..pline.line_index].iter().rev().find_map(|l| {
                                    if let LineKind::TableRow { ref body, .. } = l.kind {
                                        let TableRowBody {
                                            cells: pcells,
                                            table_source: pts,
                                            ..
                                        } = &**body;
                                        if pts == table_source {
                                            return pcells.last().map(|c| c.end);
                                        }
                                    }
                                    None
                                });
                            if let Some(prev_end) = prev_row_cell_end {
                                let target = skip_hidden_left(&parsed, prev_end);
                                self.place(target, extend);
                            } else if table_source.start > 0 {
                                let target = skip_hidden_left(&parsed, table_source.start - 1);
                                self.place(target, extend);
                            } else {
                                self.place(0, extend);
                            }
                        }
                        return;
                    }
                }
            }
            if pos <= pline.prefix_range.end && pos >= pline.source.start {
                if pline.source.start > 0 {
                    let prev = pline.source.start - 1;
                    let snapped = skip_hidden_left(&parsed, prev);
                    self.place(if extend { prev } else { snapped }, extend);
                } else {
                    self.place(0, extend);
                }
                return;
            }
        }
        pos = skip_hidden_left(&parsed, pos);
        let ending = line_ending_len_before(&self.content, pos);
        pos = if ending > 0 {
            pos - ending
        } else {
            previous_grapheme(&self.content, pos)
        };
        pos = skip_hidden_interior_left(&parsed, pos);
        if let Some(pline) = parsed.line_at_offset(pos) {
            if pos < pline.prefix_range.end {
                pos = pline.prefix_range.end;
            }
        }
        self.place(pos, extend);
    }

    pub fn move_right_in_mode(&mut self, extend: bool, mode: ViewMode) {
        self.pending_formats = ActiveFormats::default();
        if mode == ViewMode::Raw {
            self.move_right(extend);
            return;
        }
        if !extend && !self.caret.range.is_empty() {
            let end = self.selection_range().end;
            self.place(end, false);
            return;
        }
        let parsed = self.parsed();
        let mut pos = self.cursor_offset();
        if pos >= self.content.len() {
            return;
        }
        if let Some(pline) = parsed.line_at_offset(pos) {
            if let LineKind::TableRow { ref body, .. } = pline.kind {
                let TableRowBody {
                    cells,
                    raw_cells,
                    table_source,
                    ..
                } = &**body;
                if let Some(c_idx) = raw_cells
                    .iter()
                    .position(|rc| pos >= rc.start && pos <= rc.end)
                {
                    if pos >= cells[c_idx].end {
                        if c_idx + 1 < cells.len() {
                            let target = skip_hidden_right(&parsed, cells[c_idx + 1].start);
                            self.place(target, extend);
                        } else {
                            let next_row_cell_start =
                                parsed.lines.get(pline.line_index + 1..).and_then(|rest| {
                                    rest.iter().find_map(|l| {
                                        if let LineKind::TableRow { ref body, .. } = l.kind {
                                            let TableRowBody {
                                                cells: ncells,
                                                table_source: nts,
                                                ..
                                            } = &**body;
                                            if nts == table_source {
                                                return ncells.first().map(|c| c.start);
                                            }
                                        }
                                        None
                                    })
                                });
                            if let Some(next_start) = next_row_cell_start {
                                let target = skip_hidden_right(&parsed, next_start);
                                self.place(target, extend);
                            } else {
                                let after = (table_source.end + 1).min(self.content.len());
                                self.place(after, extend);
                            }
                        }
                        return;
                    }
                }
            }
            if pos < pline.prefix_range.end {
                pos = pline.prefix_range.end;
            }
            let vend = visible_line_end(pline, self.view_mode);
            let source_end = pline.source.end;
            if extend && pos >= vend && pos < source_end {
                return self.move_right(true);
            }
            if pos >= vend && vend < source_end {
                let break_len = line_ending_len(&self.content, source_end);
                if break_len > 0 {
                    let next_line_start = source_end + break_len;
                    let prefix_end = parsed
                        .line_at_offset(next_line_start)
                        .map(|nline| nline.prefix_range.end);
                    if let Some(prefix_end) = prefix_end {
                        let target = skip_hidden_right(&parsed, prefix_end);
                        self.place(target, extend);
                        return;
                    }
                }
                self.place(vend, extend);
                return;
            }
        }
        pos = skip_hidden_right(&parsed, pos);
        if pos >= self.content.len() {
            self.place(self.content.len(), extend);
            return;
        }
        let break_len = line_ending_len(&self.content, pos);
        if break_len > 0 {
            let next_line_start = pos + break_len;
            if let Some(nline) = parsed.line_at_offset(next_line_start) {
                let target = skip_hidden_right(&parsed, nline.prefix_range.end);
                self.place(target, extend);
                return;
            }
        }
        pos = next_grapheme(&self.content, pos);
        pos = skip_hidden_right(&parsed, pos);
        self.place(pos, extend);
    }

    pub fn move_up(&mut self, extend: bool) {
        self.pending_formats = ActiveFormats::default();
        self.history.seal();
        move_vertical_collapsed(&self.content, &mut self.caret, -1, extend);
        self.snap_caret_to_visible_line_end();
        self.refresh_cursor();
    }

    pub fn move_down(&mut self, extend: bool) {
        self.pending_formats = ActiveFormats::default();
        self.history.seal();
        move_vertical_collapsed(&self.content, &mut self.caret, 1, extend);
        self.snap_caret_to_visible_line_end();
        self.refresh_cursor();
    }

    pub fn set_view_mode(&mut self, mode: ViewMode) {
        self.view_mode = mode;
        self.snap_caret_to_visible_line_end();
        self.refresh_cursor();
    }

    /// Up/Down and a mode switch land on a source column. A collapsed caret
    /// that stopped inside a hidden closing-hash run moves to the visible
    /// end. A selection keeps both endpoints.
    pub(super) fn snap_caret_to_visible_line_end(&mut self) {
        if !self.caret.range.is_empty() || !self.file_type.supports_markdown_features() {
            return;
        }
        let offset = self.cursor_offset();
        let parsed = self.parsed();
        let Some(line) = parsed.line_at_offset(offset) else {
            return;
        };
        let vend = visible_line_end(line, self.view_mode);
        if offset > vend && offset <= line.source.end {
            let prefer = self.caret.preferred_col;
            self.caret.range = vend..vend;
            self.caret.reversed = false;
            self.caret.preferred_col = prefer;
        }
    }

    pub fn move_home(&mut self, extend: bool) {
        self.pending_formats = ActiveFormats::default();
        let target = crate::model::text_ops::home_offset(&self.content, self.cursor_offset());
        self.place(target, extend);
    }

    pub fn move_home_in_mode(&mut self, extend: bool, mode: ViewMode) {
        self.pending_formats = ActiveFormats::default();
        if mode == ViewMode::Raw {
            self.move_home(extend);
            return;
        }
        let cursor = snap_off_ending_interior(&self.content, self.cursor_offset());
        let parsed = self.parsed();
        if let Some(pline) = parsed.line_at_offset(cursor) {
            if let LineKind::TableRow { ref body, .. } = pline.kind {
                let TableRowBody {
                    cells, raw_cells, ..
                } = &**body;
                if let Some((_, cell)) = raw_cells
                    .iter()
                    .zip(cells.iter())
                    .find(|(rc, _)| cursor >= rc.start && cursor <= rc.end)
                {
                    let vis_start = skip_hidden_right(&parsed, cell.start).min(cell.end);
                    self.place(vis_start, extend);
                    return;
                }
            }
            let vis_start =
                skip_hidden_right(&parsed, pline.prefix_range.end).min(pline.source.end);
            self.place(vis_start, extend);
        } else {
            self.move_home(extend);
        }
    }

    pub fn move_end(&mut self, extend: bool) {
        self.pending_formats = ActiveFormats::default();
        let cursor = self.cursor_offset();
        let start = crate::model::text_ops::home_offset(&self.content, cursor);
        let nl = self.content[start..]
            .find('\n')
            .map_or(self.content.len(), |i| start + i);
        let target = if extend {
            nl
        } else {
            self.parsed()
                .line_at_offset(cursor)
                .map_or(nl, |l| visible_line_end(l, self.view_mode))
        };
        self.place(target, extend);
    }

    pub fn move_end_in_mode(&mut self, extend: bool, mode: ViewMode) {
        self.pending_formats = ActiveFormats::default();
        if mode == ViewMode::Raw {
            self.move_end(extend);
            return;
        }
        let cursor = self.cursor_offset();
        let parsed = self.parsed();
        if let Some(pline) = parsed.line_at_offset(cursor) {
            if let LineKind::TableRow { ref body, .. } = pline.kind {
                let TableRowBody {
                    cells, raw_cells, ..
                } = &**body;
                if let Some((_, cell)) = raw_cells
                    .iter()
                    .zip(cells.iter())
                    .find(|(rc, _)| cursor >= rc.start && cursor <= rc.end)
                {
                    self.place(cell.end, extend);
                    return;
                }
            }
        }
        self.move_end(extend);
    }

    pub fn select_line_up(&mut self) {
        let cursor = self.cursor_offset();
        let current_line_start = crate::model::text_ops::home_offset(&self.content, cursor);
        let target = if cursor > current_line_start {
            current_line_start
        } else if current_line_start > 0 {
            crate::model::text_ops::home_offset(&self.content, current_line_start - 1)
        } else {
            0
        };
        self.place(target, true);
    }

    pub fn select_line_down(&mut self) {
        let cursor = self.cursor_offset();
        let parsed = self.parsed();
        let source_end = parsed
            .line_at_offset(cursor)
            .map(|line| line.source.end)
            .unwrap_or_else(|| crate::model::text_ops::line_end(&self.content, cursor));
        let target = if cursor < source_end {
            source_end + line_ending_len(&self.content, source_end).saturating_sub(1)
        } else if source_end < self.content.len() {
            let next = source_end + line_ending_len(&self.content, source_end);
            let next_end = parsed
                .line_at_offset(next)
                .map(|line| line.source.end)
                .unwrap_or_else(|| crate::model::text_ops::line_end(&self.content, next));
            next_end + line_ending_len(&self.content, next_end).saturating_sub(1)
        } else {
            self.content.len()
        };
        self.place(target, true);
    }

    pub fn move_doc_start(&mut self, extend: bool) {
        self.place(0, extend);
    }

    pub fn move_doc_end(&mut self, extend: bool) {
        let len = self.content.len();
        self.place(len, extend);
    }

    pub fn move_word_left(&mut self, extend: bool) {
        let target = if !extend && !self.caret.range.is_empty() {
            crate::model::text_ops::word_left(&self.content, self.selection_range().start)
        } else {
            crate::model::text_ops::word_left(&self.content, self.cursor_offset())
        };
        self.place(target, extend);
    }

    pub fn move_word_right(&mut self, extend: bool) {
        let origin = if !extend && !self.caret.range.is_empty() {
            self.selection_range().end
        } else {
            self.cursor_offset()
        };
        let mut target = crate::model::text_ops::word_right(&self.content, origin);
        if let Some(line) = self.parsed().line_at_offset(origin) {
            let vend = visible_line_end(line, self.view_mode);
            if vend < line.source.end
                && origin <= vend
                && target > vend
                && target <= line.source.end
            {
                target = line.source.end;
            }
        }
        self.place(target, extend);
    }

    pub fn select_all(&mut self) {
        self.caret.range = 0..self.content.len();
        self.caret.reversed = false;
        self.caret.preferred_col = None;
        self.refresh_cursor();
    }

    pub fn select_word(&mut self) {
        let range = crate::model::text_ops::word_extent(&self.content, self.cursor_offset());
        self.caret.range = range;
        self.caret.reversed = false;
        self.caret.preferred_col = None;
        self.refresh_cursor();
    }

    pub fn select_line(&mut self) {
        let range = crate::model::text_ops::line_extent(&self.content, self.cursor_offset());
        self.caret.range = range;
        self.caret.reversed = false;
        self.caret.preferred_col = None;
        self.refresh_cursor();
    }

    pub fn place(&mut self, offset: usize, extend: bool) {
        self.pending_formats = ActiveFormats::default();
        let mut offset = floor_boundary(&self.content, offset);
        if !extend {
            offset = snap_off_ending_interior(&self.content, offset);
            if let Some(line) = self.parsed().line_at_offset(offset) {
                let vend = visible_line_end(line, self.view_mode);
                if offset > vend && offset <= line.source.end {
                    offset = vend;
                }
            }
        }
        if extend {
            self.caret.select_to(offset);
        } else {
            self.caret.move_to(offset);
        }
        self.refresh_cursor();
    }
}

pub(super) fn skip_hidden_left(parsed: &ParsedDocument, mut pos: usize) -> usize {
    while let Some(h) = hidden_range_at(parsed, pos, |r| pos > r.start && pos <= r.end) {
        pos = h.start;
    }
    pos
}

pub(super) fn skip_hidden_interior_left(parsed: &ParsedDocument, mut pos: usize) -> usize {
    while let Some(h) = hidden_range_at(parsed, pos, |r| pos > r.start && pos < r.end) {
        pos = h.start;
    }
    pos
}

/// End of the text the caret can stand on. A rendered closing-hash heading
/// hides the closer, so this is the click, End, and Right target. Raw paints
/// the source, and every other line keeps the source end.
pub(crate) fn visible_line_end(line: &ParsedLine, mode: ViewMode) -> usize {
    if mode != ViewMode::Raw && matches!(line.kind, LineKind::Heading { .. }) {
        line.offset_map.display_to_source(line.display_text.len())
    } else {
        line.source.end
    }
}

pub(super) fn skip_hidden_right(parsed: &ParsedDocument, mut pos: usize) -> usize {
    if let Some(line) = parsed.line_at_offset(pos) {
        let vend = visible_line_end(line, ViewMode::Wysiwyg);
        if pos == vend && vend < line.source.end {
            return pos;
        }
    }
    while let Some(h) = hidden_range_at(parsed, pos, |r| pos >= r.start && pos < r.end) {
        pos = h.end;
    }
    pos
}

pub(super) fn floor_boundary(content: &str, offset: usize) -> usize {
    let offset = offset.min(content.len());
    if content.is_char_boundary(offset) {
        offset
    } else {
        crate::model::text_ops::floor_char_boundary(content, offset)
    }
}
