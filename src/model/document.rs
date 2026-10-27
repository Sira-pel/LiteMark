use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

use crate::markdown::{
    accepted_list_marker, parse_document, split_table_cells,
    unescaped_pipe_offsets, InlinePair, InlineTagKind, LineKind, ParsedDocument, ParsedLine,
    StyledSegment, TableColumnAlign,
};

use super::text_ops::{
    find_matches, find_next, find_prev, format_replacement, home_offset, line_col, line_end,
    line_ending_len, line_ending_len_before, line_start_flags, move_vertical_collapsed,
    next_grapheme, previous_grapheme, replace_all, snap_off_ending_interior, Caret, DiffEdit,
    UndoHistory,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FileType {
    #[default]
    Markdown,
    PlainText,
    Pdf,
}

impl FileType {
    /// `.md` and `.markdown` are Markdown. `.pdf` is PDF. Every other name is plain text.
    pub fn from_filename(name: &str) -> Self {
        let lower = name.to_lowercase();
        if lower.ends_with(".pdf") {
            FileType::Pdf
        } else if lower.ends_with(".md") || lower.ends_with(".markdown") {
            FileType::Markdown
        } else {
            FileType::PlainText
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ViewMode {
    #[default]
    Wysiwyg,
    Raw,
}

impl ViewMode {
    pub fn toggle(&self) -> Self {
        match self {
            ViewMode::Wysiwyg => ViewMode::Raw,
            ViewMode::Raw => ViewMode::Wysiwyg,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ViewMode::Wysiwyg => "WYSIWYG",
            ViewMode::Raw => "Raw",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ActiveFormats {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub h1: bool,
    pub h2: bool,
    pub h3: bool,
    pub ul: bool,
    pub ol: bool,
    pub checklist: bool,
    pub quote: bool,
    pub code: bool,
    pub link: bool,
    pub table: bool,
}

#[derive(Debug, Clone)]
pub struct DocumentStats {
    pub words: usize,
    pub chars: usize,
    pub lines: usize,
    pub reading_time_min: usize,
}

pub fn compute_stats(content: &str) -> DocumentStats {
    let chars = content.chars().count();
    let words = content.split_whitespace().count();
    let lines = if content.is_empty() {
        1
    } else {
        content.lines().count().max(1)
    };
    let reading_time_min = (words / 200).max(1);

    DocumentStats {
        words,
        chars,
        lines,
        reading_time_min,
    }
}

#[derive(Debug, Clone)]
pub struct DocumentTab {
    pub id: String,
    pub title: String,
    pub content: String,
    pub file_type: FileType,
    pub is_modified: bool,
    pub file_path: Option<PathBuf>,
    pub workspace_file_id: Option<String>,
    pub word_count: usize,
    pub char_count: usize,
    pub line_count: usize,
    pub reading_time_min: usize,
    pub pdf_page_count: Option<usize>,
    pub cursor_line: usize,
    pub cursor_col: usize,
    pub active_formats: ActiveFormats,
    pub pending_formats: ActiveFormats,
    pub caret: Caret,
    pub view_mode: ViewMode,
    pub marked_range: Option<Range<usize>>,
    pub revision: u64,
    cached_parse: Option<(u64, Arc<ParsedDocument>)>,
    stats_revision: Option<u64>,
    history: UndoHistory,
    /// Set right before the edit that wraps freshly typed text into a new
    /// formatted span, so that edit and the typing after it share one undo
    /// step.
    next_edit_opens_span_group: bool,
    /// When set, the next `edit` records this caret on its undo entry instead
    /// of the end of the deleted range (Enter rewrites a whole span).
    next_undo_caret: Option<usize>,
}

impl DocumentTab {
    pub fn new(
        id: String,
        title: String,
        content: String,
        file_path: Option<PathBuf>,
        workspace_file_id: Option<String>,
    ) -> Self {
        let file_type = FileType::from_filename(&title);
        let stats = compute_stats(&content);

        let mut tab = Self {
            id,
            title,
            content,
            file_type,
            is_modified: false,
            file_path,
            workspace_file_id,
            word_count: stats.words,
            char_count: stats.chars,
            line_count: stats.lines,
            reading_time_min: stats.reading_time_min,
            pdf_page_count: None,
            cursor_line: 1,
            cursor_col: 1,
            active_formats: ActiveFormats::default(),
            pending_formats: ActiveFormats::default(),
            caret: Caret::default(),
            view_mode: ViewMode::Wysiwyg,
            marked_range: None,
            revision: 1,
            cached_parse: None,
            stats_revision: Some(1),
            history: UndoHistory::default(),
            next_edit_opens_span_group: false,
            next_undo_caret: None,
        };
        tab.refresh_cursor();
        tab
    }

    pub fn parsed(&mut self) -> Arc<ParsedDocument> {
        if let Some((rev, ref doc)) = self.cached_parse {
            if rev == self.revision {
                return doc.clone();
            }
        }
        let doc = Arc::new(parse_document(&self.content));
        self.cached_parse = Some((self.revision, doc.clone()));
        doc
    }

    #[allow(dead_code)]
    pub fn parsed_snapshot(&self) -> Arc<ParsedDocument> {
        if let Some((rev, ref doc)) = self.cached_parse {
            if rev == self.revision {
                return doc.clone();
            }
        }
        Arc::new(parse_document(&self.content))
    }

    pub fn is_read_only(&self) -> bool {
        self.file_type == FileType::Pdf
    }

    pub fn cursor_offset(&self) -> usize {
        floor_boundary(&self.content, self.caret.cursor())
    }

    pub fn selection_range(&self) -> Range<usize> {
        let start = floor_boundary(&self.content, self.caret.range.start);
        let end = floor_boundary(&self.content, self.caret.range.end);
        if end < start {
            end..start
        } else {
            start..end
        }
    }

    #[allow(dead_code)]
    pub fn undo_stored_bytes(&self) -> usize {
        self.history.stored_bytes()
    }

    pub fn set_content(&mut self, new_content: String) {
        if self.content != new_content {
            let end = self.content.len();
            self.edit(0..end, &new_content);
        }
    }

    /// Replace `range` and record a diff. Read-only documents ignore the edit.
    pub fn edit(&mut self, range: Range<usize>, new_text: &str) {
        if self.is_read_only() {
            return;
        }
        let opens_span_group = std::mem::take(&mut self.next_edit_opens_span_group);
        let range = self.clamp_range(range);
        if !self.content.is_char_boundary(range.start) || !self.content.is_char_boundary(range.end)
        {
            return;
        }
        let deleted = self.content[range.clone()].to_string();
        if deleted == new_text {
            let cursor = range.start + new_text.len();
            self.caret.move_to(cursor.min(self.content.len()));
            self.marked_range = None;
            self.refresh_cursor();
            return;
        }
        let undo_caret = self.next_undo_caret.take().unwrap_or(range.end);
        let mut edit = DiffEdit::new(range.start, deleted.clone(), new_text.to_string());
        edit.undo_caret = undo_caret;
        if opens_span_group {
            self.history.push_span_opening(edit);
        } else {
            self.history.push(edit);
        }
        let cursor_at = range.start + new_text.len();
        self.content.replace_range(range, new_text);
        let cursor = snap_off_ending_interior(&self.content, cursor_at);
        self.revision = self.revision.wrapping_add(1);
        self.caret.move_to(cursor.min(self.content.len()));
        self.marked_range = None;
        self.is_modified = true;
        self.refresh_cursor();
    }

    pub fn move_line_up(&mut self) {
        if self.is_read_only() {
            return;
        }
        let cursor = self.cursor_offset();
        let current_line_start = super::text_ops::home_offset(&self.content, cursor);
        if current_line_start == 0 {
            return;
        }
        let col = cursor - current_line_start;
        let prev_line_start = super::text_ops::home_offset(&self.content, current_line_start - 1);
        let current_line_end = super::text_ops::line_end(&self.content, current_line_start);
        let break_len = line_ending_len_before(&self.content, current_line_start);
        if break_len == 0 || current_line_start < break_len {
            return;
        }
        let break_at = current_line_start - break_len;
        let ending = self.content[break_at..current_line_start].to_string();
        let prev_line = self.content[prev_line_start..break_at].to_string();
        let curr_line = self.content[current_line_start..current_line_end].to_string();
        let curr_len = curr_line.len();

        let new_text = format!("{curr_line}{ending}{prev_line}");
        self.edit(prev_line_start..current_line_end, &new_text);

        let new_cursor = (prev_line_start + col).min(prev_line_start + curr_len);
        self.caret.move_to(new_cursor);
        self.snap_caret_to_visible_line_end();
        self.refresh_cursor();
    }

    pub fn move_line_down(&mut self) {
        if self.is_read_only() {
            return;
        }
        let cursor = self.cursor_offset();
        let current_line_start = super::text_ops::home_offset(&self.content, cursor);
        let current_line_end = super::text_ops::line_end(&self.content, current_line_start);
        if current_line_end >= self.content.len() {
            return;
        }
        let col = cursor - current_line_start;
        let ending_len = line_ending_len(&self.content, current_line_end);
        if ending_len == 0 {
            return;
        }
        let next_line_start = current_line_end + ending_len;
        let next_line_end = super::text_ops::line_end(&self.content, next_line_start);

        let curr_line = self.content[current_line_start..current_line_end].to_string();
        let ending = self.content[current_line_end..next_line_start].to_string();
        let next_line = self.content[next_line_start..next_line_end].to_string();
        let curr_len = curr_line.len();
        let next_len = next_line.len();

        let new_text = format!("{next_line}{ending}{curr_line}");
        self.edit(current_line_start..next_line_end, &new_text);

        let new_line_start = current_line_start + next_len + ending_len;
        let new_cursor = (new_line_start + col).min(new_line_start + curr_len);
        self.caret.move_to(new_cursor);
        self.snap_caret_to_visible_line_end();
        self.refresh_cursor();
    }

    /// Enter at the visible end of a closing-hash heading inserts after the
    /// closer, so the heading stays one line.
    fn heading_newline_at(&mut self, range: &Range<usize>) -> Option<usize> {
        if !range.is_empty() {
            return None;
        }
        let parsed = self.parsed();
        let line = parsed.line_at_offset(range.start)?;
        let vend = visible_line_end(line, self.view_mode);
        if range.start == vend && vend < line.source.end {
            Some(line.source.end)
        } else {
            None
        }
    }

    /// `Foo\r` + `\n` + `Bar` is one CRLF ending. Enter at the start of the
    /// bare-CR line has to insert two breaks to add a line.
    fn bare_cr_needs_extra_break(&self, range: &Range<usize>) -> bool {
        if !range.is_empty() {
            return false;
        }
        let cursor = range.start;
        let line_start = home_offset(&self.content, cursor);
        cursor == line_start
            && cursor > 0
            && self.content.as_bytes().get(cursor - 1) == Some(&b'\r')
    }

    /// The table whose block contains `pos`, with its body rows in order.
    fn table_at(&mut self, pos: usize) -> Option<TableInfo> {
        let parsed = self.parsed();
        let mut info: Option<TableInfo> = None;
        for line in &parsed.lines {
            if let LineKind::TableRow {
                ref cells,
                is_header,
                ref table_source,
                ..
            } = line.kind
            {
                if pos < table_source.start || pos > table_source.end {
                    continue;
                }
                let info = info.get_or_insert_with(|| TableInfo {
                    source: table_source.clone(),
                    rows: Vec::new(),
                    delimiter_end: None,
                });
                info.rows.push(TableRowInfo {
                    source: line.source.clone(),
                    cells: cells.clone(),
                    is_header,
                });
            }
        }
        let info = info.as_mut()?;
        for line in &parsed.lines {
            if matches!(line.kind, LineKind::TableDelimiter)
                && line.source.start >= info.source.start
                && line.source.end <= info.source.end
            {
                info.delimiter_end = Some(line.source.end);
            }
        }
        info.rows.sort_by_key(|r| r.source.start);
        info.clone().into()
    }

    /// Replace `remove_start..remove_end` (an optional trailing empty row,
    /// otherwise empty at the table end) and put the caret on a paragraph line
    /// below the table, with exactly one blank line between them. One undo step.
    fn apply_table_exit(&mut self, remove_start: usize, remove_end: usize) {
        let rest = &self.content[remove_end..];
        let (insert_at, insert_text, caret_rel) = blank_gap_after_table(rest);
        let end = remove_end + insert_at;
        let replacement = format!("{}{}", &self.content[remove_end..end], insert_text);
        if remove_start != end || !replacement.is_empty() {
            self.edit(remove_start..end, &replacement);
        }
        let target = floor_boundary(&self.content, remove_start + caret_rel);
        self.caret.move_to(target);
        self.refresh_cursor();
    }

    /// Ctrl+Enter: add an empty row below the current row and put the caret in
    /// the same column. Returns false outside a table.
    pub fn table_insert_row_below(&mut self) -> bool {
        if self.is_read_only() {
            return false;
        }
        let cursor = self.selection_range().end;
        let Some(table) = self.table_at(cursor) else {
            return false;
        };
        let Some(row_pos) = table
            .rows
            .iter()
            .position(|r| cursor >= r.source.start && cursor <= r.source.end)
        else {
            // On the delimiter row: add the first body row
            let at = table.delimiter_end.unwrap_or(table.source.end);
            let cols = table.rows.first().map(|r| r.cells.len()).unwrap_or(2).max(2);
            let row = format!("\n|{}", "  |".repeat(cols));
            self.edit(at..at, &row);
            self.caret.move_to(floor_boundary(&self.content, at + 3));
            self.refresh_cursor();
            return true;
        };
        let row = &table.rows[row_pos];
        let col = row
            .cells
            .iter()
            .position(|c| cursor >= c.start.saturating_sub(1) && cursor <= c.end + 1)
            .unwrap_or_else(|| row.cells.iter().rposition(|c| c.start <= cursor).unwrap_or(0));
        let cols = row.cells.len().max(2);
        let at = if row.is_header {
            table.delimiter_end.unwrap_or(row.source.end)
        } else {
            row.source.end
        };
        let new_row = format!("\n|{}", "  |".repeat(cols));
        self.edit(at..at, &new_row);
        let target = at + 3 + 3 * col.min(cols - 1);
        self.caret.move_to(floor_boundary(&self.content, target));
        self.refresh_cursor();
        true
    }

    /// Up on the first row or Down on the last row of a table leaves the table.
    /// Returns false when the caret is not on that edge row.
    pub fn table_edge_exit(&mut self, delta: i32) -> bool {
        if self.is_read_only() {
            return false;
        }
        let cursor = self.cursor_offset();
        let Some(table) = self.table_at(cursor) else {
            return false;
        };
        let Some(row_pos) = table
            .rows
            .iter()
            .position(|r| cursor >= r.source.start && cursor <= r.source.end)
        else {
            return false;
        };
        if delta < 0 {
            if row_pos != 0 {
                return false;
            }
            let start = table.source.start;
            if start == 0 {
                self.edit(0..0, "\n\n");
                self.caret.move_to(0);
            } else {
                let ending = line_ending_len_before(&self.content, start);
                self.caret
                    .move_to(floor_boundary(&self.content, start - ending.max(1)));
            }
            self.pending_formats = ActiveFormats::default();
            self.refresh_cursor();
            true
        } else {
            if row_pos + 1 != table.rows.len() {
                return false;
            }
            let end = table.source.end;
            let ending = line_ending_len(&self.content, end);
            if end + ending >= self.content.len() {
                // The table ends the note: make room below it
                self.apply_table_exit(end, end);
            } else {
                self.caret.move_to(end + ending);
                self.refresh_cursor();
            }
            self.pending_formats = ActiveFormats::default();
            true
        }
    }

    /// Enter with the caret inside an inline formatting span: close the span
    /// before the newline and reopen it after `line_ins` on the new line, so
    /// no markers straddle the break. A caret at the end of the span closes
    /// it and keeps the formats armed so the next letter reopens them on the
    /// new line. `line_ins` is the line-prefix text the caller would insert
    /// at the caret (for example `"\n> "` inside a blockquote).
    fn split_inline_span_newline(&mut self, range: &Range<usize>, line_ins: &str) -> bool {
        if !range.is_empty() {
            return false;
        }
        let cursor = range.start;
        let parsed = self.parsed();
        let mut spanning: Vec<InlinePair> = line_pair_refs(&parsed, cursor)
            .iter()
            .filter(|p| {
                p.kind != InlineTagKind::Link
                    && cursor > p.open_delim.start
                    && cursor < p.close_delim.end
                    && cursor >= p.inner.start
                    && cursor <= p.inner.end
            })
            .cloned()
            .collect();
        if spanning.is_empty() {
            return false;
        }
        spanning.sort_by_key(|p| p.open_delim.start);

        let innermost = spanning.last().cloned().unwrap();
        if cursor == innermost.inner.end {
            // End of the span: leave it, keeping the formats armed for the
            // new line.
            let pending = pending_formats_at(&parsed, cursor);
            let at = spanning
                .iter()
                .map(|p| p.close_delim.end)
                .max()
                .unwrap_or(innermost.close_delim.end);
            self.edit(at..at, line_ins);
            self.pending_formats = pending;
            self.caret
                .move_to(floor_boundary(&self.content, at + line_ins.len()));
            self.refresh_cursor();
            return true;
        }

        let run_start = spanning
            .iter()
            .map(|p| p.open_delim.start)
            .min()
            .unwrap_or(innermost.open_delim.start);
        let run_end = spanning
            .iter()
            .map(|p| p.close_delim.end)
            .max()
            .unwrap_or(innermost.close_delim.end);

        let open_ranges =
            merge_adjacent_ranges(spanning.iter().map(|p| p.open_delim.clone()).collect());
        let close_ranges =
            merge_adjacent_ranges(spanning.iter().map(|p| p.close_delim.clone()).collect());
        let openers: String = open_ranges
            .iter()
            .map(|r| &self.content[r.clone()])
            .collect();
        let closers: String = close_ranges
            .iter()
            .map(|r| &self.content[r.clone()])
            .collect();

        let prefix = self.content[run_start..cursor].to_string();
        let suffix = self.content[cursor..run_end].to_string();
        let before_core_len = prefix.trim_end().len();
        let before_tail = &prefix[before_core_len..];
        let after_head_len = suffix.len() - suffix.trim_start().len();
        let after_head = &suffix[..after_head_len];
        let after_core = &suffix[after_head_len..];
        let mut repl = String::new();
        if before_core_len > 0 {
            repl.push_str(&prefix[..before_core_len]);
            repl.push_str(&closers);
        }
        repl.push_str(before_tail);
        repl.push_str(line_ins);
        if !after_core.is_empty() {
            repl.push_str(after_head);
            repl.push_str(&openers);
            repl.push_str(after_core);
        } else {
            repl.push_str(after_head);
        }
        let caret = if after_core.is_empty() {
            run_start + repl.len()
        } else {
            run_start + repl.len() - after_core.len()
        };
        self.next_undo_caret = Some(cursor);
        self.edit(run_start..run_end, &repl);
        self.caret.move_to(floor_boundary(&self.content, caret));
        self.refresh_cursor();
        true
    }

    pub fn smart_newline(&mut self, is_shift: bool) {
        if self.is_read_only() {
            return;
        }
        self.history.seal();
        let range = self.selection_range();
        if is_shift {
            // Shift+Enter in a table cell leaves the table without touching it.
            if let Some(table) = self.table_at(range.start) {
                self.apply_table_exit(table.source.end, table.source.end);
                return;
            }
            if let Some(at) = self.heading_newline_at(&range) {
                self.edit(at..at, "\n");
                return;
            }
            if self.bare_cr_needs_extra_break(&range) {
                self.edit(range.start..range.start, "\n\n");
                return;
            }
            if self.split_inline_span_newline(&range, "\n") {
                return;
            }
            self.edit(range, "\n");
            return;
        }

        let cursor = range.start;
        let line_start = home_offset(&self.content, cursor);
        let line_end_pos = line_end(&self.content, cursor);

        // 0. Table rows: continuation or exit
        let parsed = self.parsed();
        if let Some(pline) = parsed.line_at_offset(cursor) {
            if let LineKind::TableRow {
                ref cells,
                is_header,
                ..
            } = pline.kind
            {
                let col_count = cells.len().max(2);
                let is_empty_row = cells.iter().all(|c| {
                    if c.end <= self.content.len() && c.start <= c.end {
                        self.content[c.clone()].trim().is_empty()
                    } else {
                        true
                    }
                });

                if is_header {
                    // Header row: find delimiter line and jump to / create first data row
                    if let Some(delim_line) = parsed.lines.get(pline.line_index + 1) {
                        let after_delim = delim_line.source.end;
                        // Check if there is already a body row after delimiter
                        if let Some(first_body) = parsed.lines.get(pline.line_index + 2) {
                            if let LineKind::TableRow {
                                ref cells,
                                is_header: false,
                                ..
                            } = first_body.kind
                            {
                                if let Some(first_cell) = cells.first() {
                                    self.caret.move_to(first_cell.start);
                                    self.refresh_cursor();
                                    return;
                                }
                            }
                        }
                        // Otherwise insert first body row after delimiter
                        let new_row = format!("\n|{}", "  |".repeat(col_count));
                        self.edit(after_delim..after_delim, &new_row);
                        let target = (after_delim + 3).min(self.content.len());
                        self.caret.move_to(floor_boundary(&self.content, target));
                        self.refresh_cursor();
                        return;
                    }
                }

                if is_empty_row {
                    let table = self.table_at(cursor);
                    let row_pos = table
                        .as_ref()
                        .and_then(|t| t.rows.iter().position(|r| r.source.start == pline.source.start));
                    if let (Some(table), Some(row_pos)) = (table, row_pos) {
                        if row_pos + 1 < table.rows.len() {
                            // Not the last row: move to the same column in the next row
                            let col = table.rows[row_pos]
                                .cells
                                .iter()
                                .position(|c| cursor >= c.start.saturating_sub(1) && cursor <= c.end + 1)
                                .unwrap_or(0);
                            let next = &table.rows[row_pos + 1];
                            let target = next
                                .cells
                                .get(col)
                                .or(next.cells.last())
                                .map(|c| c.start)
                                .unwrap_or(next.source.start);
                            self.caret.move_to(floor_boundary(&self.content, target));
                            self.refresh_cursor();
                            return;
                        }
                    }
                    // Last empty row: remove it, keeping the header, and exit below the table
                    let remove_start =
                        if line_start > 0 && self.content.as_bytes()[line_start - 1] == b'\n' {
                            line_start - 1
                        } else {
                            line_start
                        };
                    self.apply_table_exit(remove_start, line_end_pos);
                    return;
                } else {
                    // Non-empty row: check if cursor is inside a cell with content to split
                    let row_text = &self.content[line_start..line_end_pos];
                    let pipe_indices: Vec<usize> = unescaped_pipe_offsets(row_text)
                        .into_iter()
                        .map(|idx| line_start + idx)
                        .collect();

                    let mut split_cell_info: Option<(usize, usize, usize, String, String)> = None;
                    if pipe_indices.len() >= 2 {
                        for k in 0..pipe_indices.len() - 1 {
                            let cell_start = pipe_indices[k] + 1;
                            let cell_end = pipe_indices[k + 1];
                            if cursor >= cell_start && cursor <= cell_end {
                                // Only split when the whole selection sits inside this cell;
                                // otherwise the start/end slices below would be out of order.
                                if range.start < cell_start || range.end > cell_end {
                                    break;
                                }
                                let before = &self.content[cell_start..range.start];
                                let after = &self.content[range.end..cell_end];
                                if !after.trim().is_empty() {
                                    split_cell_info = Some((
                                        k,
                                        cell_start,
                                        cell_end,
                                        before.to_string(),
                                        after.trim().to_string(),
                                    ));
                                }
                                break;
                            }
                        }
                    }

                    if let Some((col_idx, _c_start, c_end, before_str, after_str)) = split_cell_info
                    {
                        // Split the cell into current row and a new row below in a single undo step.
                        // Close spans in the current cell and reopen them on the new row so
                        // `| **first half** |` becomes `| **first** |` / `| **half** |`.
                        let (span_open, span_close) =
                            spanning_marker_runs(&self.content, &parsed, cursor)
                                .unwrap_or_default();
                        let after_str = format!("{span_open}{after_str}");
                        let tail_len = before_str.len() - before_str.trim_end().len();
                        let edit_start = range.start - tail_len;
                        let tail = &self.content[edit_start..range.start];
                        let rest_of_line = &self.content[c_end..line_end_pos];
                        let n_cols = cells.len().max(1);
                        let mut new_cells = Vec::new();
                        for i in 0..n_cols {
                            if i == col_idx {
                                new_cells.push(format!(" {} ", after_str));
                            } else {
                                new_cells.push("  ".to_string());
                            }
                        }
                        let new_row = format!("\n|{}|", new_cells.join("|"));
                        let combined =
                            format!("{}{}{}{}", span_close, tail, rest_of_line, new_row);
                        let new_row_start = edit_start
                            + span_close.len()
                            + tail.len()
                            + rest_of_line.len()
                            + 1;
                        self.edit(edit_start..line_end_pos, &combined);

                        let mut target = new_row_start + 1;
                        for cell in new_cells.iter().take(col_idx) {
                            target += cell.len() + 1;
                        }
                        target += 1;
                        target += span_open.len();
                        let caret_pos = floor_boundary(&self.content, target);
                        self.caret.move_to(caret_pos);
                        self.refresh_cursor();
                        return;
                    }

                    let new_row = format!("\n|{}", "  |".repeat(col_count));
                    self.edit(line_end_pos..line_end_pos, &new_row);
                    let target = (line_end_pos + 3).min(self.content.len());
                    self.caret.move_to(floor_boundary(&self.content, target));
                    self.refresh_cursor();
                    return;
                }
            }
        }

        if let Some(at) = self.heading_newline_at(&range) {
            self.edit(at..at, "\n");
            return;
        }
        if self.bare_cr_needs_extra_break(&range) {
            self.edit(range.start..range.start, "\n\n");
            return;
        }

        let line = &self.content[line_start..line_end_pos];
        let trimmed_start = line.trim_start();
        let indent = &line[..line.len() - trimmed_start.len()];

        if let Some(item) = accepted_list_marker(&self.content, line_start) {
            let body = &line[item.prefix_end()..];
            // The caret stops before a CR line ending, so the CR is not in
            // `body`. An ordered task or a paren marker still does not exit
            // when that ending is a CR. A plain `1. ` or `- [ ] ` does.
            let ending_is_cr = self.content.as_bytes().get(line_end_pos) == Some(&b'\r');
            let cr_keeps_item =
                ending_is_cr && (item.punct == b')' || (item.ordered && item.task.is_some()));
            let is_empty = body.trim().is_empty() && !cr_keeps_item;
            if is_empty {
                if !range.is_empty() {
                    self.edit(range, "\n");
                    return;
                }
                if item.indent_bytes > 0 {
                    let indent = &line[..item.indent_bytes];
                    let outdent_len = if indent.ends_with('\t') || indent.len() < 2 {
                        1
                    } else {
                        2
                    };
                    self.edit(line_start..line_start + outdent_len, "");
                } else {
                    // A CRLF ending loses the CR with the marker, leaving LF.
                    // The caret never stood on that CR.
                    let remove_end = if line_ending_len(&self.content, line_end_pos) == 2 {
                        line_end_pos + 1
                    } else {
                        line_end_pos
                    };
                    self.edit(line_start..remove_end, "");
                }
            } else {
                let kept = &line[..item.indent_bytes];
                let cont = item.continuation(kept);
                if self.split_inline_span_newline(&range, &cont) {
                    return;
                }
                self.edit(range, &cont);
            }
            return;
        }

        // Blockquotes: "> "
        if trimmed_start.starts_with("> ") || trimmed_start == ">" {
            let is_empty_quote = trimmed_start == ">"
                || trimmed_start == "> "
                || trimmed_start[1..].trim().is_empty();
            if is_empty_quote {
                if !range.is_empty() {
                    self.edit(range, "\n");
                } else {
                    self.edit(line_start..line_end_pos, "");
                }
            } else {
                let ins = format!("\n{}> ", indent);
                if self.split_inline_span_newline(&range, &ins) {
                    return;
                }
                self.edit(range, &ins);
            }
            return;
        }

        // 5. Headings: "# ", "## ", etc. -> normal newline
        let is_heading = indent.len() <= 3
            && ["###### ", "##### ", "#### ", "### ", "## ", "# "]
                .iter()
                .any(|p| trimmed_start.starts_with(p));
        if is_heading {
            if self.split_inline_span_newline(&range, "\n") {
                return;
            }
            self.edit(range, "\n");
            return;
        }

        // 6. Indented code/text: maintain indentation
        if !indent.is_empty() && !trimmed_start.is_empty() {
            let ins = format!("\n{}", indent);
            if self.split_inline_span_newline(&range, &ins) {
                return;
            }
            self.edit(range, &ins);
            return;
        }

        // 7. Default newline
        if self.split_inline_span_newline(&range, "\n") {
            return;
        }
        self.edit(range, "\n");
    }

    pub fn smart_backspace(&mut self) -> bool {
        if self.is_read_only() {
            return false;
        }
        let range = self.selection_range();
        if !range.is_empty() {
            self.edit(range, "");
            return true;
        }

        let cursor = self.cursor_offset();
        if cursor == 0 {
            return false;
        }

        // 1. Auto-pairing bracket/quote deletion: (|), [|], {|}, "|", '|', `|`
        if cursor < self.content.len() {
            let prev_char = self.content[..cursor].chars().next_back().unwrap();
            let next_char = self.content[cursor..].chars().next().unwrap();
            let is_paired = matches!(
                (prev_char, next_char),
                ('(', ')') | ('[', ']') | ('{', '}') | ('"', '"') | ('\'', '\'') | ('`', '`')
            );
            if is_paired {
                let p_len = prev_char.len_utf8();
                let n_len = next_char.len_utf8();
                self.edit(cursor - p_len..cursor + n_len, "");
                return true;
            }
        }

        let line_start = home_offset(&self.content, cursor);
        let line_before = &self.content[line_start..cursor];

        // 2. Heading prefix at cursor boundary
        for (prefix, demoted) in [
            ("###### ", "##### "),
            ("##### ", "#### "),
            ("#### ", "### "),
            ("### ", "## "),
            ("## ", "# "),
            ("# ", ""),
        ] {
            if line_before == prefix {
                self.edit(line_start..cursor, demoted);
                return true;
            }
        }

        if let Some(item) = accepted_list_marker(&self.content, line_start) {
            if item.prefix_end() == line_before.len() {
                if item.task.is_some() {
                    let start = line_start + item.marker_end;
                    self.edit(start..cursor, "");
                    return true;
                }
                if item.indent_bytes > 0 {
                    let indent = &line_before[..item.indent_bytes];
                    let outdent_len = if indent.ends_with('\t') || indent.len() < 2 {
                        1
                    } else {
                        2
                    };
                    self.edit(line_start..line_start + outdent_len, "");
                } else {
                    self.edit(line_start..cursor, "");
                }
                return true;
            }
        }

        // Blockquote prefix at cursor boundary
        if line_before == "> " {
            self.edit(line_start..cursor, "");
            return true;
        }

        // 7. Indentation spaces deletion (outdent 2 spaces if only spaces before cursor on this line)
        if cursor >= line_start + 2 && line_before.chars().all(|c| c == ' ') {
            self.edit(cursor - 2..cursor, "");
            return true;
        }

        // 8. Normal backspace: previous grapheme
        let prev = previous_grapheme(&self.content, cursor);
        self.edit(prev..cursor, "");
        true
    }

    pub fn indent(&mut self) {
        if self.is_read_only() {
            return;
        }
        let range = self.selection_range();
        let start_line = home_offset(&self.content, range.start);
        let end_line = line_end(&self.content, range.end);

        let lines_str = self.content[start_line..end_line].to_string();
        let mut modified = Vec::new();
        for line in lines_str.split('\n') {
            modified.push(format!("  {}", line));
        }
        let replacement = modified.join("\n");
        let added_len = replacement.len() - lines_str.len();
        self.edit(start_line..end_line, &replacement);
        if range.is_empty() {
            self.caret.move_to(range.start + 2);
        } else {
            self.caret.range = (range.start + 2)..(range.end + added_len);
        }
        self.snap_caret_to_visible_line_end();
        self.refresh_cursor();
    }

    pub fn outdent(&mut self) {
        if self.is_read_only() {
            return;
        }
        let range = self.selection_range();
        let start_line = home_offset(&self.content, range.start);
        let end_line = line_end(&self.content, range.end);

        let lines_str = self.content[start_line..end_line].to_string();
        let mut modified = Vec::new();
        let mut first_line_stripped = 0;
        // Indent removed from lines at or before the selection end, so the end
        // shifts only by what was removed in front of it.
        let mut removed_before_end = 0;
        let mut line_start_pos = start_line;
        for (i, line) in lines_str.split('\n').enumerate() {
            let stripped = if let Some(s) = line.strip_prefix("  ") {
                s
            } else if let Some(s) = line.strip_prefix(' ') {
                s
            } else if let Some(s) = line.strip_prefix('\t') {
                s
            } else {
                line
            };
            if i == 0 {
                first_line_stripped = line.len() - stripped.len();
            }
            let removed = line.len() - stripped.len();
            removed_before_end += removed.min(range.end.saturating_sub(line_start_pos));
            line_start_pos += line.len() + 1;
            modified.push(stripped.to_string());
        }
        let replacement = modified.join("\n");
        self.edit(start_line..end_line, &replacement);
        if range.is_empty() {
            let new_cursor = range
                .start
                .saturating_sub(first_line_stripped)
                .max(start_line);
            self.caret.move_to(new_cursor);
        } else {
            let new_start = range
                .start
                .saturating_sub(first_line_stripped)
                .max(start_line);
            let new_end = range.end.saturating_sub(removed_before_end).max(new_start);
            let new_end = floor_boundary(&self.content, new_end).max(new_start);
            self.caret.range = new_start..new_end;
        }
        self.snap_caret_to_visible_line_end();
        self.refresh_cursor();
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn apply_format(&mut self, cmd: &str) {
        self.apply_format_in_mode(cmd, ViewMode::Raw);
    }

    pub fn apply_format_in_mode(&mut self, cmd: &str, mode: ViewMode) {
        if self.is_read_only() {
            return;
        }
        // A format toggle is an undo boundary: pending span typing groups end
        // here so text typed after the toggle starts a new step.
        self.history.seal();

        // 1. Block-level formats: h1, h2, h3, ul, ol, checklist, quote
        let block_target = match cmd {
            "h1" => Some(BlockFormatTarget::Heading(1)),
            "h2" => Some(BlockFormatTarget::Heading(2)),
            "h3" => Some(BlockFormatTarget::Heading(3)),
            "ul" => Some(BlockFormatTarget::BulletList),
            "ol" => Some(BlockFormatTarget::OrderedList),
            "checklist" => Some(BlockFormatTarget::TaskList),
            "quote" => Some(BlockFormatTarget::Blockquote),
            _ => None,
        };
        if let Some(target) = block_target {
            self.toggle_block_lines(target);
            return;
        }

        // 2. Code block toggle (Ctrl+Shift+C)
        if cmd == "codeblock" {
            self.toggle_code_block();
            return;
        }

        // 3. Inline styling & Link: bold, italic, underline, strike, code, link
        let inline_spec = match cmd {
            "bold" => Some((InlineTagKind::Bold, "**", "**", "bold text")),
            "italic" => Some((InlineTagKind::Italic, "*", "*", "italic text")),
            "underline" => Some((InlineTagKind::Underline, "<u>", "</u>", "underlined")),
            "strike" => Some((InlineTagKind::Strike, "~~", "~~", "strikethrough")),
            "code" => Some((InlineTagKind::Code, "`", "`", "code")),
            "link" => Some((InlineTagKind::Link, "[", "](https://)", "link")),
            _ => None,
        };

        if let Some((tag_kind, prefix, suffix, placeholder)) = inline_spec {
            let mut range = self.selection_range();
            let parsed = self.parsed();

            // If selection is empty, check if cursor is on/inside a word or enclosed by raw delimiters
            if range.is_empty() {
                let cursor = range.start;

                let word_start = super::text_ops::word_left(&self.content, cursor);
                let word_end = super::text_ops::word_right(&self.content, cursor);
                let line_s = home_offset(&self.content, cursor);
                let line_e = parsed
                    .line_at_offset(cursor)
                    .map(|line| visible_line_end(line, self.view_mode))
                    .unwrap_or_else(|| line_end(&self.content, cursor));
                let w_start = word_start.max(line_s);
                let w_end = word_end.min(line_e);
                let touching_word = cursor > w_start
                    && cursor < w_end
                    && self.content[..cursor]
                        .chars()
                        .next_back()
                        .is_some_and(|c| c.is_alphanumeric() || c == '_')
                    && self.content[cursor..]
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphanumeric() || c == '_');
                if touching_word && w_start < w_end {
                    let extent = super::text_ops::word_extent(&self.content, cursor);
                    if self.content[extent.clone()]
                        .chars()
                        .any(|c| c.is_alphanumeric() || c == '_')
                    {
                        range = extent;
                    }
                }

                if range.is_empty() {
                    // Check if cursor is strictly inside an existing InlinePair of `tag_kind`
                    if let Some(pair) = line_pair_refs(&parsed, range.start)
                        .iter()
                        .rev()
                        .find(|p| {
                            p.kind == tag_kind
                                && range.start > p.open_delim.start
                                && range.end < span_close_end(&parsed, p)
                        })
                        .cloned()
                    {
                        let inner_text = self.content[pair.inner.clone()].to_string();
                        let edit_range = pair.open_delim.start..pair.close_delim.end;
                        // If the inner text is just the default unmodified placeholder and the user immediately toggled off,
                        // remove the placeholder too so pressing Ctrl+B twice on an empty spot leaves it empty!
                        if inner_text == placeholder && range == pair.inner {
                            let pos = pair.open_delim.start;
                            self.edit(edit_range, "");
                            self.caret.move_to(pos);
                            self.pending_formats = ActiveFormats::default();
                            self.refresh_cursor();
                            return;
                        }
                        if mode == ViewMode::Wysiwyg && tag_kind != InlineTagKind::Link {
                            // Word-style exit: the span stays formatted and the
                            // caret moves past the whole closing run so further
                            // typing is plain in the toggled format. Other
                            // formats stacked around the caret stay armed for
                            // the next letter, and a second press of the same
                            // toggle then disarms them. Links unwrap instead.
                            let mut pending = pending_formats_at(&parsed, range.start);
                            match tag_kind {
                                InlineTagKind::Bold => pending.bold = false,
                                InlineTagKind::Italic => pending.italic = false,
                                InlineTagKind::Underline => pending.underline = false,
                                InlineTagKind::Strike => pending.strike = false,
                                InlineTagKind::Code => pending.code = false,
                                InlineTagKind::Link => {}
                            }
                            if range.start > pair.open_delim.start {
                                let group_end = span_close_end(&parsed, &pair);
                                self.caret.move_to(floor_boundary(&self.content, group_end));
                            }
                            self.pending_formats = pending;
                            self.refresh_cursor();
                            return;
                        }
                        let new_start = pair.open_delim.start;
                        self.edit(edit_range, &inner_text);
                        let rel = range
                            .start
                            .saturating_sub(pair.inner.start)
                            .min(inner_text.len());
                        self.caret.move_to(new_start + rel);
                        self.pending_formats = ActiveFormats::default();
                        self.refresh_cursor();
                        return;
                    }

                    // Check empty delimiters right around cursor, e.g. `**|**`
                    if outer_is_delimited(&self.content, cursor, cursor, prefix, suffix) {
                        let edit_range = (cursor - prefix.len())..(cursor + suffix.len());
                        let pos = cursor - prefix.len();
                        self.edit(edit_range, "");
                        self.caret.move_to(pos);
                        self.pending_formats = ActiveFormats::default();
                        self.refresh_cursor();
                        return;
                    }

                    if outer_is_delimited(&self.content, word_start, word_end, prefix, suffix) {
                        let core = self.content[word_start..word_end].to_string();
                        let edit_range = (word_start - prefix.len())..(word_end + suffix.len());
                        let new_start = word_start - prefix.len();
                        let new_end = new_start + core.len();
                        self.edit(edit_range, &core);
                        self.caret.range = new_start..new_end;
                        self.caret.reversed = false;
                        self.pending_formats = ActiveFormats::default();
                        self.refresh_cursor();
                        return;
                    }
                }
            }

            if range.is_empty() {
                // Caret on the opener: unwrap the span (Left to offset 0 of
                // `**Hello**` then Ctrl+B must drop the markers).
                if let Some(pair) = line_pair_refs(&parsed, range.start)
                    .iter()
                    .rev()
                    .find(|p| p.kind == tag_kind && range.start == p.open_delim.start)
                    .cloned()
                {
                    let inner_text = self.content[pair.inner.clone()].to_string();
                    let edit_range = pair.open_delim.start..pair.close_delim.end;
                    self.edit(edit_range, &inner_text);
                    self.caret.move_to(pair.open_delim.start);
                    self.pending_formats = ActiveFormats::default();
                    self.refresh_cursor();
                    return;
                }

                // In WYSIWYG mode, toggling bold/italic/underline/strike/code at an empty cursor toggles pending_formats!
                if mode == ViewMode::Wysiwyg && tag_kind != InlineTagKind::Link {
                    match tag_kind {
                        InlineTagKind::Bold => {
                            self.pending_formats.bold = !self.pending_formats.bold
                        }
                        InlineTagKind::Italic => {
                            self.pending_formats.italic = !self.pending_formats.italic
                        }
                        InlineTagKind::Underline => {
                            self.pending_formats.underline = !self.pending_formats.underline
                        }
                        InlineTagKind::Strike => {
                            self.pending_formats.strike = !self.pending_formats.strike
                        }
                        InlineTagKind::Code => {
                            self.pending_formats.code = !self.pending_formats.code
                        }
                        InlineTagKind::Link => {}
                    }
                    self.refresh_cursor();
                    return;
                }

                // In Raw mode (or for Link), insert placeholder and select inner placeholder so a second press unwraps/removes it
                self.edit(
                    range.clone(),
                    &format!("{}{}{}", prefix, placeholder, suffix),
                );
                let start = floor_boundary(&self.content, range.start + prefix.len());
                let end = floor_boundary(&self.content, start + placeholder.len());
                self.caret.range = start..end;
                self.caret.reversed = false;
                self.refresh_cursor();
                return;
            }

            // Non-empty selection: collect parts across table cells or lines separately
            let orig_cursor = if self.caret.range.is_empty() {
                Some(self.caret.range.start)
            } else {
                None
            };
            let orig_sel = range.clone();
            struct InlinePartInfo {
                range: Range<usize>,
                has_format: bool,
                edit_range: Range<usize>,
                unwrapped_text: String,
                wrapped_text: String,
                /// For a partial unformat, the new range of the selection's
                /// text after the surrounding span was split.
                split_inner: Option<Range<usize>>,
            }

            let mut candidate_ranges: Vec<Range<usize>> = Vec::new();

            for pline in parsed.lines_overlapping(range.clone()) {
                let line_start = range.start.max(pline.source.start);
                let line_end = range.end.min(pline.source.end);
                if line_start >= line_end {
                    continue;
                }
                match &pline.kind {
                    LineKind::TableDelimiter
                    | LineKind::CodeFence { .. }
                    | LineKind::HorizontalRule => {
                        continue;
                    }
                    LineKind::TableRow {
                        cells, raw_cells, ..
                    } => {
                        let cell_ranges = if raw_cells.is_empty() {
                            cells
                        } else {
                            raw_cells
                        };
                        for c_range in cell_ranges {
                            let c_start = range.start.max(c_range.start);
                            let c_end = range.end.min(c_range.end);
                            if c_start >= c_end {
                                continue;
                            }
                            let slice = &self.content[c_start..c_end];
                            let trimmed = slice.trim();
                            if trimmed.is_empty() {
                                continue;
                            }
                            let lead = slice.len() - slice.trim_start().len();
                            let trail = slice.len() - slice.trim_end().len();
                            let part_start = c_start + lead;
                            let part_end = c_end - trail;
                            candidate_ranges.push(part_start..part_end);
                        }
                    }
                    _ => {
                        let content_start = pline.prefix_range.end.max(pline.source.start);
                        let l_start = range.start.max(content_start);
                        let l_end = range.end.min(pline.source.end);
                        if l_start >= l_end {
                            continue;
                        }
                        let slice = &self.content[l_start..l_end];
                        let trimmed = slice.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        let lead = slice.len() - slice.trim_start().len();
                        let trail = slice.len() - slice.trim_end().len();
                        let part_start = l_start + lead;
                        let part_end = l_end - trail;
                        candidate_ranges.push(part_start..part_end);
                    }
                }
            }

            if candidate_ranges.is_empty() {
                return;
            }

            if tag_kind != InlineTagKind::Link {
                let global_all_have_format = !candidate_ranges.is_empty()
                    && candidate_ranges.iter().all(|p_range| {
                        let pline = match parsed.line_at_offset(p_range.start) {
                            Some(l) => l,
                            None => return false,
                        };
                        let overlapping_segs: Vec<&StyledSegment> = pline
                            .styled_segments
                            .iter()
                            .filter(|s| s.source.start < p_range.end && s.source.end > p_range.start)
                            .collect();
                        !overlapping_segs.is_empty()
                            && overlapping_segs.iter().all(|s| match tag_kind {
                                InlineTagKind::Bold => s.bold,
                                InlineTagKind::Italic => s.italic,
                                InlineTagKind::Underline => s.underline,
                                InlineTagKind::Strike => s.strike,
                                InlineTagKind::Code => s.code,
                                InlineTagKind::Link => false,
                            })
                    });
                let should_turn_on = !global_all_have_format;

                let mut candidate_results = Vec::new();
                let in_table = candidate_ranges.iter().any(|r| {
                    parsed.line_at_offset(r.start).is_some_and(|l| {
                        matches!(l.kind, LineKind::TableRow { .. })
                    })
                });
                let rewrite_ranges: Vec<Range<usize>> = if !in_table && candidate_ranges.len() > 1 {
                    let combined = candidate_ranges[0].start
                        ..candidate_ranges
                            .last()
                            .map(|r| r.end)
                            .unwrap_or(candidate_ranges[0].end);
                    let has_pairs = range_has_non_link_pairs(&parsed, combined.clone());
                    // Combine across lines only when rewriting existing spans
                    // (m5). Plain select-all still wraps each line separately.
                    if has_pairs {
                        vec![combined]
                    } else {
                        candidate_ranges.clone()
                    }
                } else {
                    candidate_ranges.clone()
                };

                for p_range in &rewrite_ranges {
                    let p_start = p_range.start;
                    let p_end = p_range.end;
                    let pline = match parsed.line_at_offset(p_start) {
                        Some(l) => l,
                        None => continue,
                    };

                    let mut group_pairs = collect_group_pairs(&parsed, p_start..p_end);

                    let (replace_start, replace_end) = if group_pairs.is_empty() {
                        (p_start, p_end)
                    } else {
                        let r_start = group_pairs
                            .iter()
                            .map(|p| p.open_delim.start)
                            .min()
                            .unwrap()
                            .min(p_start);
                        let r_end = group_pairs
                            .iter()
                            .map(|p| span_close_end(&parsed, p))
                            .max()
                            .unwrap()
                            .max(p_end);
                        (r_start, r_end)
                    };
                    let (replace_start, replace_end) = expand_replace_to_matching_neighbors(
                        &self.content,
                        &parsed,
                        pline,
                        p_start..p_end,
                        replace_start..replace_end,
                        tag_kind,
                        should_turn_on,
                    );
                    // Neighbor expansion may pull in spans that did not overlap
                    // the selection (`aa` next to `**bb cc**`). Skip those
                    // delimiters so they are rewritten instead of kept as text.
                    add_contained_pairs(&parsed, replace_start..replace_end, &mut group_pairs);

                    let simple_wrap = group_pairs.is_empty()
                        && replace_start == p_start
                        && replace_end == p_end;
                    let slices: Vec<StyledSlice> = if simple_wrap {
                        let mut flags = FormatFlags::default();
                        apply_toggle(&mut flags, tag_kind, should_turn_on);
                        vec![StyledSlice {
                            text: self.content[p_start..p_end].to_string(),
                            flags,
                            is_selected: true,
                        }]
                    } else {
                        let skip: Vec<Range<usize>> = group_pairs
                            .iter()
                            .flat_map(|p| [p.open_delim.clone(), p.close_delim.clone()])
                            .collect();
                        let segs: Vec<&StyledSegment> = parsed
                            .lines_overlapping(replace_start..replace_end)
                            .iter()
                            .flat_map(|l| l.styled_segments.iter())
                            .collect();
                        let local_pairs = nearby_pair_refs(&parsed, replace_start..replace_end);
                        collect_rewrite_slices(
                            &self.content,
                            &local_pairs,
                            &segs,
                            replace_start..replace_end,
                            p_start..p_end,
                            &skip,
                            Some((tag_kind, should_turn_on)),
                        )
                    };
                    let glued_left = replace_start > 0
                        && self.content[..replace_start]
                            .chars()
                            .next_back()
                            .is_some_and(|c| !c.is_whitespace());
                    let include_wraps = self.content[p_start..p_end].contains('\n');
                    let (rendered_str, sel_in_rendered) =
                        render_styled_slices_with_selection_opts(
                            &slices,
                            glued_left,
                            include_wraps,
                        );
                    candidate_results.push((replace_start..replace_end, rendered_str, sel_in_rendered, p_start));
                }

                if rewrite_ranges.len() == 1 && !candidate_results.is_empty() {
                    let (rep_range, rendered_str, sel_in_rendered, p_start) = candidate_results.remove(0);
                    self.edit(rep_range.clone(), &rendered_str);

                    if let Some(cur) = orig_cursor {
                        let cursor_in_sel = cur.saturating_sub(p_start);
                        let target_pos = rep_range.start + sel_in_rendered.start + cursor_in_sel.min(sel_in_rendered.len());
                        self.caret.move_to(floor_boundary(&self.content, target_pos));
                    } else {
                        let new_start = rep_range.start + sel_in_rendered.start;
                        let new_end = rep_range.start + sel_in_rendered.end;
                        self.caret.range = new_start..new_end;
                        self.caret.reversed = false;
                    }
                    self.pending_formats = ActiveFormats::default();
                    self.refresh_cursor();
                    return;
                }

                let edits: Vec<(Range<usize>, String)> = candidate_results
                    .into_iter()
                    .filter(|(r, repl, _, _)| self.content[r.clone()] != *repl)
                    .map(|(r, repl, _, _)| (r, repl))
                    .collect();

                if edits.is_empty() {
                    return;
                }

                let min_start = edits.first().unwrap().0.start;
                let max_end = edits.last().unwrap().0.end;
                let span_range = min_start..max_end;

                let mut replacement = String::with_capacity(
                    max_end - min_start + edits.len() * (prefix.len() + suffix.len()),
                );
                let mut cursor = min_start;
                for (r, repl) in &edits {
                    if r.start > cursor {
                        replacement.push_str(&self.content[cursor..r.start]);
                    }
                    replacement.push_str(repl);
                    cursor = r.end;
                }
                if max_end > cursor {
                    replacement.push_str(&self.content[cursor..max_end]);
                }

                self.edit(span_range, &replacement);

                let map_offset = |offset: usize| -> usize {
                    if offset <= min_start {
                        return offset;
                    }
                    let mut mapped = min_start;
                    let mut orig_cursor = min_start;
                    for (r, repl) in &edits {
                        if offset <= r.start {
                            mapped += offset - orig_cursor;
                            return mapped;
                        }
                        mapped += r.start - orig_cursor;
                        if offset <= r.end {
                            mapped += repl.len();
                            return mapped;
                        }
                        mapped += repl.len();
                        orig_cursor = r.end;
                    }
                    if offset > orig_cursor {
                        mapped += offset - orig_cursor;
                    }
                    mapped
                };
                let new_start = map_offset(orig_sel.start);
                let new_end = map_offset(orig_sel.end);
                let new_start = floor_boundary(&self.content, new_start);
                let new_end = floor_boundary(&self.content, new_end);
                self.caret.range = new_start..new_end;
                self.caret.reversed = false;
                self.pending_formats = ActiveFormats::default();
                self.refresh_cursor();
                return;
            }

            let mut parts: Vec<InlinePartInfo> = Vec::with_capacity(candidate_ranges.len());

            for p_range in candidate_ranges {
                let p_start = p_range.start;
                let p_end = p_range.end;
                let part_text = &self.content[p_start..p_end];

                let (has_format, edit_range, unwrapped_text, split_inner) = if tag_kind
                    == InlineTagKind::Link
                {
                    if let Some(inner) = is_single_link(part_text) {
                        (true, p_start..p_end, inner.to_string(), None)
                    } else if let Some(pair) = line_pair_refs(&parsed, p_start).iter().find(|p| {
                        p.kind == InlineTagKind::Link
                            && (p.inner == (p_start..p_end)
                                || (p.open_delim.start == p_start && p.close_delim.end == p_end))
                    }) {
                        (
                            true,
                            pair.open_delim.start..pair.close_delim.end,
                            self.content[pair.inner.clone()].to_string(),
                            None,
                        )
                    } else if let Some((outer_range, inner)) =
                        unwrap_outer_link(&self.content, p_start, p_end)
                    {
                        (true, outer_range, inner, None)
                    } else {
                        (false, p_start..p_end, String::new(), None)
                    }
                } else {
                    (false, p_start..p_end, String::new(), None)
                };

                let wrapped_text = format!("{}{}{}", prefix, part_text, suffix);
                parts.push(InlinePartInfo {
                    range: p_range,
                    has_format,
                    edit_range,
                    unwrapped_text,
                    wrapped_text,
                    split_inner,
                });
            }

            let every_part_has_format = parts.iter().all(|p| p.has_format);
            let edits: Vec<(Range<usize>, String)> = if every_part_has_format {
                parts
                    .iter()
                    .map(|p| (p.edit_range.clone(), p.unwrapped_text.clone()))
                    .collect()
            } else {
                parts
                    .iter()
                    .filter(|p| !p.has_format)
                    .map(|p| (p.edit_range.clone(), p.wrapped_text.clone()))
                    .collect()
            };

            if edits.is_empty() {
                return;
            }

            let min_start = edits.first().unwrap().0.start;
            let max_end = edits.last().unwrap().0.end;
            let span_range = min_start..max_end;

            let mut replacement = String::with_capacity(
                max_end - min_start + edits.len() * (prefix.len() + suffix.len()),
            );
            let mut cursor = min_start;
            for (r, repl) in &edits {
                if r.start > cursor {
                    replacement.push_str(&self.content[cursor..r.start]);
                }
                replacement.push_str(repl);
                cursor = r.end;
            }
            if max_end > cursor {
                replacement.push_str(&self.content[cursor..max_end]);
            }

            self.edit(span_range, &replacement);

            if parts.len() == 1
                && (orig_sel == (parts[0].range.start..parts[0].range.end)
                    || orig_sel == (parts[0].edit_range.start..parts[0].edit_range.end))
            {
                if let (Some(cur), true) = (orig_cursor, every_part_has_format) {
                    let unwrapped_len = edits[0].1.len();
                    let rel = cur
                        .saturating_sub(parts[0].range.start)
                        .min(unwrapped_len);
                    self.caret.move_to(min_start + rel);
                    self.caret.reversed = false;
                    self.pending_formats = ActiveFormats::default();
                    self.refresh_cursor();
                    return;
                }
                if let Some(split) = parts[0].split_inner.clone() {
                    self.caret.range = split;
                } else if every_part_has_format {
                    let unwrapped_len = edits[0].1.len();
                    self.caret.range = min_start..min_start + unwrapped_len;
                } else {
                    let inner_start = min_start + prefix.len();
                    let inner_end = inner_start + (parts[0].range.end - parts[0].range.start);
                    self.caret.range = inner_start..inner_end;
                }
                self.caret.reversed = false;
            } else {
                let map_offset = |offset: usize| -> usize {
                    if offset <= min_start {
                        return offset;
                    }
                    let mut mapped = min_start;
                    let mut orig_cursor = min_start;
                    for (r, repl) in &edits {
                        if offset <= r.start {
                            mapped += offset - orig_cursor;
                            return mapped;
                        }
                        mapped += r.start - orig_cursor;
                        if offset <= r.end {
                            mapped += repl.len();
                            return mapped;
                        }
                        mapped += repl.len();
                        orig_cursor = r.end;
                    }
                    if offset > orig_cursor {
                        mapped += offset - orig_cursor;
                    }
                    mapped
                };
                let new_start = map_offset(orig_sel.start);
                let new_end = map_offset(orig_sel.end);
                let new_start = floor_boundary(&self.content, new_start);
                let new_end = floor_boundary(&self.content, new_end).max(new_start);
                self.caret.range = new_start..new_end;
                self.caret.reversed = false;
            }
            self.pending_formats = ActiveFormats::default();
            self.refresh_cursor();
            return;
        }

        if cmd == "table" {
            let range = self.selection_range();
            let parsed = self.parsed();

            // Check if cursor is currently inside an existing table
            let in_table = parsed.lines.iter().find(|l| {
                if let LineKind::TableRow {
                    ref table_source, ..
                } = l.kind
                {
                    range.start >= table_source.start && range.start <= table_source.end
                } else {
                    false
                }
            });

            if let Some(table_line) = in_table {
                if let LineKind::TableRow {
                    ref cells,
                    ref table_source,
                    ..
                } = table_line.kind
                {
                    let col_count = cells.len().max(2);
                    let row_str = format!("\n|{}", "  |".repeat(col_count));
                    let insert_pos = table_source.end;
                    self.edit(insert_pos..insert_pos, &row_str);
                    let target_pos = (insert_pos + 3).min(self.content.len());
                    self.caret
                        .move_to(floor_boundary(&self.content, target_pos));
                    self.refresh_cursor();
                    return;
                }
            }

            // Outside table: insert clean standard 2-column table template
            let mut prefix = String::new();
            if range.start > 0 {
                let prev_char = self.content[..range.start].chars().next_back();
                if let Some(c) = prev_char {
                    if c != '\n' {
                        prefix.push_str("\n\n");
                    } else if !self.content[..range.start].ends_with("\n\n") {
                        prefix.push('\n');
                    }
                }
            }
            let mut suffix = String::new();
            if range.end < self.content.len() {
                let next_char = self.content[range.end..].chars().next();
                if let Some(c) = next_char {
                    if c != '\n' {
                        suffix.push_str("\n\n");
                    } else if !self.content[range.end..].starts_with("\n\n") {
                        suffix.push('\n');
                    }
                }
            }
            let table_template = "| Column 1 | Column 2 |\n| :--- | :--- |\n| Item 1 | Item 2 |";
            let full_insert = format!("{}{}{}", prefix, table_template, suffix);
            self.edit(range.clone(), &full_insert);
            let col1_start = range.start + prefix.len() + 2;
            let col1_end = col1_start + "Column 1".len();
            self.caret.range = col1_start..col1_end;
            self.caret.reversed = false;
            self.refresh_cursor();
            return;
        }

        // 4. Fallback for clear, hr, table
        let range = self.selection_range();
        let selected = self.content[range.clone()].to_string();
        let flags = line_start_flags(&self.content, range.start, &selected);
        let Some(replacement) = format_replacement(cmd, &selected, &flags) else {
            return;
        };
        let cursor_after_placeholder = cmd_selects_placeholder(cmd) && selected.is_empty();
        let prefix_len = placeholder_prefix_len(cmd);
        self.edit(range.clone(), &replacement);
        if cursor_after_placeholder {
            let start = floor_boundary(&self.content, range.start + prefix_len);
            let end = floor_boundary(&self.content, start + placeholder_len(cmd));
            self.caret.range = start..end;
            self.caret.reversed = false;
            self.refresh_cursor();
        }
    }

    fn toggle_block_lines(&mut self, target: BlockFormatTarget) {
        let range = self.selection_range();
        let start_line = home_offset(&self.content, range.start);
        let end_line = line_end(&self.content, range.end);
        let lines_owned = self.content[start_line..end_line].to_string();
        let split_lines: Vec<&str> = lines_owned.split('\n').collect();

        // Check if all non-empty lines (or the single line) already match `target`.
        // An ordered task is a checklist too: the button is on, and turning it
        // off must keep the ordered marker instead of writing `- [ ] `.
        let mut line_at = Vec::with_capacity(split_lines.len());
        let mut abs = start_line;
        for (idx, line) in split_lines.iter().enumerate() {
            line_at.push(abs);
            abs += line.len();
            if idx + 1 < split_lines.len() {
                abs += 1;
            }
        }

        let parsed = self.parsed();
        let is_table_line = |at: usize| -> bool {
            parsed.line_at_offset(at).is_some_and(|l| {
                matches!(l.kind, LineKind::TableRow { .. } | LineKind::TableDelimiter)
            })
        };

        let non_table_indices: Vec<usize> = (0..split_lines.len())
            .filter(|&i| !is_table_line(line_at[i]))
            .collect();

        if non_table_indices.is_empty() {
            // All selected lines are table lines: skip so the table never breaks!
            return;
        }

        let all_match = non_table_indices
            .iter()
            .copied()
            .filter(|&idx| !split_lines[idx].trim().is_empty() || non_table_indices.len() == 1)
            .all(|idx| {
                let at = line_at[idx];
                if target == BlockFormatTarget::TaskList {
                    accepted_list_marker(&self.content, at).is_some_and(|item| item.task.is_some())
                } else {
                    let (_, existing, _) = parse_line_block_prefix(&self.content, at);
                    existing == Some(target)
                }
            });

        let mut transformed = Vec::with_capacity(split_lines.len());
        let mut first_delta: isize = 0;
        let mut total_delta: isize = 0;

        for (idx, line) in split_lines.iter().enumerate() {
            if is_table_line(line_at[idx]) {
                transformed.push(line.to_string());
                continue;
            }
            if line.trim().is_empty() && split_lines.len() > 1 {
                transformed.push(line.to_string());
                continue;
            }
            let (indent, _existing, prefix_end) =
                parse_line_block_prefix(&self.content, line_at[idx]);
            let leading_spaces = if matches!(target, BlockFormatTarget::Heading(_)) {
                &line[..indent.min(3)]
            } else {
                &line[..indent]
            };
            let raw_body = &line[prefix_end..];
            let has_hidden_closer =
                self.parsed()
                    .line_at_offset(line_at[idx])
                    .is_some_and(|parsed_line| {
                        visible_line_end(parsed_line, self.view_mode) < parsed_line.source.end
                    });
            let strip_closer = has_hidden_closer
                && (all_match || !matches!(target, BlockFormatTarget::Heading(_)));
            let body = if strip_closer {
                crate::markdown::parser::atx_content_range(line)
                    .map(|content| &line[content])
                    .unwrap_or(raw_body)
            } else {
                raw_body
            };
            let new_line = if let Some(rewritten) =
                ordered_checklist_line(&self.content, line_at[idx], target, all_match)
            {
                rewritten
            } else if all_match {
                format!("{}{}", leading_spaces, body)
            } else {
                let new_prefix = match target {
                    BlockFormatTarget::Heading(1) => "# ".to_string(),
                    BlockFormatTarget::Heading(2) => "## ".to_string(),
                    BlockFormatTarget::Heading(_) => "### ".to_string(),
                    BlockFormatTarget::BulletList => "- ".to_string(),
                    BlockFormatTarget::OrderedList => format!("{}. ", idx + 1),
                    BlockFormatTarget::TaskList => "- [ ] ".to_string(),
                    BlockFormatTarget::Blockquote => "> ".to_string(),
                };
                format!("{}{}{}", leading_spaces, new_prefix, body)
            };
            let delta = new_line.len() as isize - line.len() as isize;
            if idx == 0 {
                first_delta = delta;
            }
            total_delta += delta;
            transformed.push(new_line);
        }

        let replacement = transformed.join("\n");
        self.edit(start_line..end_line, &replacement);
        if range.is_empty() {
            let new_cursor = (range.start as isize + first_delta).max(start_line as isize) as usize;
            self.caret
                .move_to(floor_boundary(&self.content, new_cursor));
        } else {
            let new_start = (range.start as isize + first_delta).max(start_line as isize) as usize;
            let new_end = (range.end as isize + total_delta).max(new_start as isize) as usize;
            self.caret.range =
                floor_boundary(&self.content, new_start)..floor_boundary(&self.content, new_end);
            self.caret.reversed = false;
        }
        self.refresh_cursor();
    }

    fn toggle_code_block(&mut self) {
        let range = self.selection_range();
        let parsed = self.parsed();

        // Check if selection/cursor is inside a fenced code block
        if let Some((open_fence, close_fence)) = parsed
            .code_blocks
            .iter()
            .find(|(open_r, close_r)| range.start >= open_r.start && range.end <= close_r.end)
            .cloned()
        {
            let inner_start = open_fence.end.min(close_fence.start);
            let inner_end = close_fence.start;
            let mut inner = self.content[inner_start..inner_end].to_string();
            // Drop the newline that separated the body from the closer. An
            // unclosed fence has an empty close range, and the document's own
            // trailing newline is added back below, so the same pop applies.
            if inner.ends_with('\n') {
                inner.pop();
            }
            let full_end = close_fence.end;
            let trailing_nl =
                if full_end <= self.content.len() && self.content[..full_end].ends_with('\n') {
                    "\n"
                } else {
                    ""
                };
            let replacement = format!("{}{}", inner, trailing_nl);
            let unwrapped_len = inner.len();
            self.edit(open_fence.start..full_end, &replacement);
            self.caret.range = floor_boundary(&self.content, open_fence.start)
                ..floor_boundary(&self.content, open_fence.start + unwrapped_len);
            self.caret.reversed = false;
            self.refresh_cursor();
            return;
        }

        // Also check if `selected` itself starts and ends with ```
        let selected = self.content[range.clone()].to_string();
        if selected.starts_with("```\n") && selected.ends_with("\n```") && selected.len() >= 8 {
            let unwrapped = selected[4..selected.len() - 4].to_string();
            let start = range.start;
            let end = start + unwrapped.len();
            self.edit(range, &unwrapped);
            self.caret.range = start..end;
            self.caret.reversed = false;
            self.refresh_cursor();
            return;
        }

        let is_table_line = |at: usize| -> bool {
            parsed.line_at_offset(at).is_some_and(|l| {
                matches!(l.kind, LineKind::TableRow { .. } | LineKind::TableDelimiter)
            })
        };

        if selected.is_empty() {
            if is_table_line(range.start) {
                // Inside a table: skip so table never breaks!
                return;
            }
            let line_s = home_offset(&self.content, range.start);
            let line_e = line_end(&self.content, range.start);
            let line_text = self.content[line_s..line_e].to_string();
            if line_text.is_empty() {
                self.edit(range.clone(), "```\ncode\n```");
                self.caret.range = (range.start + 4)..(range.start + 8);
                self.caret.reversed = false;
            } else {
                let wrapped = format!("```\n{}\n```", line_text);
                self.edit(line_s..line_e, &wrapped);
                self.caret.range = (line_s + 4)..(line_s + 4 + line_text.len());
                self.caret.reversed = false;
            }
            self.refresh_cursor();
        } else {
            let start_line = home_offset(&self.content, range.start);
            let end_line = line_end(&self.content, range.end);
            let lines_owned = self.content[start_line..end_line].to_string();
            let split_lines: Vec<&str> = lines_owned.split('\n').collect();

            let mut line_at = Vec::with_capacity(split_lines.len());
            let mut abs = start_line;
            for (idx, line) in split_lines.iter().enumerate() {
                line_at.push(abs);
                abs += line.len();
                if idx + 1 < split_lines.len() {
                    abs += 1;
                }
            }

            let non_table_count = (0..split_lines.len())
                .filter(|&i| !is_table_line(line_at[i]))
                .count();
            if non_table_count == 0 {
                // All selected lines are table lines: skip so table never breaks!
                return;
            }

            let has_table_lines = non_table_count < split_lines.len();
            if !has_table_lines {
                // Pure non-table selection: wrap the selection directly
                let wrapped = format!("```\n{}\n```", selected);
                let inner_start = range.start + 4;
                let inner_end = inner_start + selected.len();
                self.edit(range, &wrapped);
                self.caret.range = inner_start..inner_end;
                self.caret.reversed = false;
                self.refresh_cursor();
            } else {
                // Mixed selection: wrap non-table chunks, keep table lines intact
                let mut transformed = Vec::new();
                let mut chunk: Vec<&str> = Vec::new();
                for (i, line) in split_lines.iter().enumerate() {
                    if is_table_line(line_at[i]) {
                        if !chunk.is_empty() {
                            transformed.push(format!("```\n{}\n```", chunk.join("\n")));
                            chunk.clear();
                        }
                        transformed.push(line.to_string());
                    } else {
                        chunk.push(line);
                    }
                }
                if !chunk.is_empty() {
                    transformed.push(format!("```\n{}\n```", chunk.join("\n")));
                }
                let replacement = transformed.join("\n");
                let new_len = replacement.len();
                self.edit(start_line..end_line, &replacement);
                self.caret.range = start_line..(start_line + new_len);
                self.caret.reversed = false;
                self.refresh_cursor();
            }
        }
    }

    pub fn has_pending_inline_formats(&self) -> bool {
        let pf = self.pending_formats;
        pf.bold || pf.italic || pf.underline || pf.strike || pf.code
    }

    pub fn insert_text_in_mode(&mut self, range: Range<usize>, text: &str, mode: ViewMode) {
        if self.is_read_only() {
            return;
        }
        if mode == ViewMode::Wysiwyg
            && !range.is_empty()
            && !text.is_empty()
            && text != "\n"
            && self.replace_wysiwyg_selection(range.clone(), text)
        {
            return;
        }
        if text.trim().is_empty() {
            if mode == ViewMode::Wysiwyg && range.is_empty() && !text.is_empty() {
                let parsed = self.parsed();
                // A Markdown closer cannot sit right after whitespace, so whitespace
                // typed at the end of a `*`/`**`/`~~` span goes after the whole
                // closing run; the span's formats stay armed for what is typed
                // next. Extra whitespace keeps stacking outside the closer.
                let line_pairs = line_pair_refs(&parsed, range.start);
                let pair = line_pairs
                    .iter()
                    .rev()
                    .find(|p| {
                        matches!(
                            p.kind,
                            InlineTagKind::Bold
                                | InlineTagKind::Italic
                                | InlineTagKind::Strike
                                | InlineTagKind::Underline
                                | InlineTagKind::Code
                        ) && range.start == p.close_delim.start
                            && !line_pairs.iter().any(|o| {
                                o.kind != InlineTagKind::Link
                                    && o.inner.start <= p.inner.start
                                    && o.inner.end >= p.inner.end
                                    && o.inner != p.inner
                            })
                    })
                    .cloned();
                if let Some(pair) = pair {
                    let pending = if self.has_pending_inline_formats() {
                        self.pending_formats
                    } else {
                        pending_formats_at(&parsed, range.start)
                    };
                    let at = span_close_end(&parsed, &pair);
                    self.edit(at..at, text);
                    self.pending_formats = pending;
                    self.caret.move_to(floor_boundary(&self.content, at + text.len()));
                    self.refresh_cursor();
                    return;
                }
            }
            let pf = self.pending_formats;
            self.edit(range, text);
            self.pending_formats = pf;
            return;
        }
        if mode == ViewMode::Wysiwyg
            && range.is_empty()
            && !text.is_empty()
            && text != "\n"
        {
            let pf = self.pending_formats;
            if self.has_pending_inline_formats() {
                let parsed = self.parsed();
                // The armed formats extend a span ending right at the caret
                // (with or without a trailing space) or continue a subset of its formats.
                if let Some((run_range, repl, caret)) =
                    extend_or_splice_span(&self.content, &parsed, &pf, range.start, text)
                {
                    self.pending_formats = ActiveFormats::default();
                    self.edit(run_range, &repl);
                    self.caret.move_to(floor_boundary(&self.content, caret));
                    self.refresh_cursor();
                    return;
                }
            }
            if pf.bold || pf.italic || pf.underline || pf.strike || pf.code {
                let flags = FormatFlags {
                    bold: pf.bold,
                    italic: pf.italic,
                    underline: pf.underline,
                    strike: pf.strike,
                    code: pf.code,
                };
                let lead_len = text.len() - text.trim_start().len();
                let trail_len = text.len() - text.trim_end().len();
                let lead_ws = &text[..lead_len];
                let trail_ws = &text[text.len() - trail_len..];
                let core = &text[lead_len..text.len() - trail_len];
                // The span's own delimiter characters inside the typed text
                // would end the span early; escape them so it stays intact.
                let core_owned;
                let core = if pf.strike && core.contains('~') {
                    core_owned = core.replace('~', "\\~");
                    &core_owned
                } else if (pf.bold || pf.italic) && core.contains('*') {
                    core_owned = core.replace('*', "\\*");
                    &core_owned
                } else {
                    core
                };

                let (open, close) = wrap_delimiters(&flags);
                let wrapped = format!("{}{}{}{}{}", lead_ws, open, core, close, trail_ws);
                let close_start = wrapped.len() - trail_ws.len() - close.len();
                let cursor_after = if trail_len > 0 {
                    range.start + wrapped.len()
                } else {
                    range.start + close_start
                };
                if trail_len == 0 {
                    self.pending_formats = ActiveFormats::default();
                }
                self.next_edit_opens_span_group = true;
                self.edit(range, &wrapped);
                self.caret
                    .move_to(floor_boundary(&self.content, cursor_after));
                self.refresh_cursor();
                return;
            }
        }
        // Typing a span's own delimiter characters inside it would end the
        // span early: escape them so the span stays intact. Backticks cannot
        // be escaped inside a code span, so the span is re-fenced with a
        // longer backtick run instead.
        let mut escaped: Option<String> = None;
        if mode == ViewMode::Wysiwyg && range.is_empty() && !text.is_empty() {
            let parsed = self.parsed();
            if let Some(pair) = line_pair_refs(&parsed, range.start).iter().rev().find(|p| {
                range.start >= p.inner.start && range.start <= p.inner.end
            }) {
                match pair.kind {
                    InlineTagKind::Bold | InlineTagKind::Italic => {
                        if text.contains('*') {
                            escaped = Some(text.replace('*', "\\*"));
                        }
                    }
                    InlineTagKind::Strike => {
                        if text.contains('~') {
                            escaped = Some(text.replace('~', "\\~"));
                        }
                    }
                    InlineTagKind::Code if text.contains('`') => {
                        // The backtick is literal content: re-fence the span
                        // with a longer run so it stays intact.
                        self.refence_code_span(pair, range.start, text);
                        return;
                    }
                    _ => {}
                }
            }
        }
        let text = escaped.as_deref().unwrap_or(text);

        // A literally typed `<` must not start an HTML tag (`<u>a<b>c</u>`);
        // escape it so it renders as text. Skipped inside code spans, whose
        // content is literal.
        if mode == ViewMode::Wysiwyg && range.is_empty() && text.chars().count() == 1 {
            let parsed = self.parsed();
            let in_code = line_pair_refs(&parsed, range.start).iter().any(|p| {
                p.kind == InlineTagKind::Code
                    && range.start > p.inner.start
                    && range.start < p.inner.end
            });
            if !in_code {
                let next_char = self.content[range.start..].chars().next();
                let starts_tag =
                    next_char.is_some_and(|c| c.is_ascii_alphabetic() || c == '/');
                let prev_is_lt = range.start > 0
                    && self.content[..range.start].ends_with('<')
                    && !self.content[..range.start].ends_with("\\<");
                let typed_tag_cont = text
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '/');
                if text == "<" && starts_tag {
                    let escaped = format!("\\{text}");
                    let start = range.start;
                    self.pending_formats = ActiveFormats::default();
                    self.edit(start..start, &escaped);
                    self.caret
                        .move_to(floor_boundary(&self.content, start + escaped.len()));
                    self.refresh_cursor();
                    return;
                }
                if typed_tag_cont && prev_is_lt {
                    // The `<` right before the caret becomes `\<` and the
                    // typed character follows it, in one edit.
                    let lt = range.start - 1;
                    let repl = format!("\\<{text}");
                    let caret = lt + repl.len();
                    self.pending_formats = ActiveFormats::default();
                    self.edit(lt..range.start, &repl);
                    self.caret.move_to(floor_boundary(&self.content, caret));
                    self.refresh_cursor();
                    return;
                }
            }
        }
        self.pending_formats = ActiveFormats::default();
        self.edit(range, text);
    }

    /// Replace a WYSIWYG selection that overlaps inline markers by rewriting
    /// the surrounding spans so unselected text keeps its formats and the
    /// typed text is inserted plain (`**He**Xld` not `**He*X*rld`).
    fn replace_wysiwyg_selection(&mut self, range: Range<usize>, text: &str) -> bool {
        let parsed = self.parsed();
        let overlapping = nearby_pair_refs(&parsed, range.clone()).iter().any(|p| {
            p.kind != InlineTagKind::Link
                && p.open_delim.start < range.end
                && p.close_delim.end > range.start
        });
        if !overlapping {
            return false;
        }
        let group_pairs = collect_group_pairs(&parsed, range.clone());
        let replace_start = group_pairs
            .iter()
            .map(|p| p.open_delim.start)
            .min()
            .unwrap_or(range.start)
            .min(range.start);
        let replace_end = group_pairs
            .iter()
            .map(|p| span_close_end(&parsed, p))
            .max()
            .unwrap_or(range.end)
            .max(range.end);
        let skip: Vec<Range<usize>> = group_pairs
            .iter()
            .flat_map(|p| [p.open_delim.clone(), p.close_delim.clone()])
            .collect();
        let segs: Vec<&StyledSegment> = parsed
            .lines_overlapping(replace_start..replace_end)
            .iter()
            .flat_map(|l| l.styled_segments.iter())
            .collect();
        let local_pairs = nearby_pair_refs(&parsed, replace_start..replace_end);
        let slices = collect_rewrite_slices(
            &self.content,
            &local_pairs,
            &segs,
            replace_start..replace_end,
            range.clone(),
            &skip,
            None,
        );
        let selected: Vec<&StyledSlice> = slices.iter().filter(|s| s.is_selected).collect();
        let typed_flags = if !selected.is_empty()
            && selected.iter().all(|s| s.flags == selected[0].flags)
        {
            selected[0].flags
        } else {
            FormatFlags::default()
        };
        let mut out = Vec::new();
        let mut replaced = false;
        for s in slices {
            if s.is_selected {
                if !replaced {
                    out.push(StyledSlice {
                        text: text.to_string(),
                        flags: typed_flags,
                        is_selected: true,
                    });
                    replaced = true;
                }
            } else {
                out.push(s);
            }
        }
        if !replaced {
            return false;
        }
        let glued_left = replace_start > 0
            && self.content[..replace_start]
                .chars()
                .next_back()
                .is_some_and(|c| !c.is_whitespace());
        let (rendered, sel) = render_styled_slices_with_selection_glued(&out, glued_left);
        self.edit(replace_start..replace_end, &rendered);
        self.caret
            .move_to(floor_boundary(&self.content, replace_start + sel.end));
        self.pending_formats = ActiveFormats::default();
        self.refresh_cursor();
        true
    }

    /// Typing a backtick inside a code span cannot be escaped (backslash is
    /// literal inside code); re-fence the whole span with a longer backtick
    /// run so the span stays intact.
    fn refence_code_span(&mut self, pair: &InlinePair, caret: usize, text: &str) {
        let inner_start = pair.inner.start;
        let inner_end = pair.inner.end;
        let mut new_inner = String::with_capacity(inner_end - inner_start + text.len());
        new_inner.push_str(&self.content[inner_start..caret]);
        new_inner.push_str(text);
        new_inner.push_str(&self.content[caret..inner_end]);
        let mut max_run = 0usize;
        let mut run = 0usize;
        for c in new_inner.chars() {
            if c == '`' {
                run += 1;
                max_run = max_run.max(run);
            } else {
                run = 0;
            }
        }
        let fence = "`".repeat(max_run.max(pair.open_delim.len() - 1) + 1);
        let repl = format!("{}{}{}", fence, new_inner, fence);
        let caret_after = pair.open_delim.start + fence.len() + (caret - inner_start) + text.len();
        self.edit(pair.open_delim.start..pair.close_delim.end, &repl);
        self.caret
            .move_to(floor_boundary(&self.content, caret_after));
        self.refresh_cursor();
    }

    #[allow(dead_code)]
    pub fn insert_snippet(&mut self, snippet: &str) {
        let cursor = self.cursor_offset();
        self.edit(
            self.selection_range().start..self.selection_range().end,
            snippet,
        );
        let _ = cursor;
    }

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
            if let LineKind::TableRow {
                ref cells,
                ref raw_cells,
                ref table_source,
                ..
            } = pline.kind
            {
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
                                    if let LineKind::TableRow {
                                        cells: ref pcells,
                                        table_source: ref pts,
                                        ..
                                    } = l.kind
                                    {
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
            if let LineKind::TableRow {
                ref cells,
                ref raw_cells,
                ref table_source,
                ..
            } = pline.kind
            {
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
                                        if let LineKind::TableRow {
                                            cells: ref ncells,
                                            table_source: ref nts,
                                            ..
                                        } = l.kind
                                        {
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

    pub fn smart_backspace_in_mode(&mut self, mode: ViewMode) -> bool {
        self.pending_formats = ActiveFormats::default();
        if mode == ViewMode::Raw {
            return self.smart_backspace();
        }
        if self.is_read_only() {
            return false;
        }
        if !self.selection_range().is_empty() {
            let range = self.selection_range();
            self.edit(range, "");
            return true;
        }
        let cursor = self.cursor_offset();
        if cursor == 0 {
            return false;
        }
        let parsed = self.parsed();

        // Guard table pipe delimiters in WYSIWYG mode
        if let Some(pline) = parsed.line_at_offset(cursor) {
            if let LineKind::TableRow {
                ref cells,
                ref raw_cells,
                ..
            } = pline.kind
            {
                let at_cell_start_or_pipe = raw_cells
                    .iter()
                    .zip(cells.iter())
                    .find(|(rc, _)| cursor >= rc.start && cursor <= rc.end)
                    .map(|(_, c)| cursor <= c.start)
                    .unwrap_or(true);
                if at_cell_start_or_pipe {
                    return false;
                }
            }
        }

        // 1. Check if cursor is inside or immediately after an InlinePair
        for pair in line_pair_refs(&parsed, cursor).iter().rev() {
            let group_end = span_close_end(&parsed, pair);
            let prev_vis = if cursor >= pair.inner.end && cursor <= group_end {
                pair.inner.end
            } else {
                cursor
            };
            if prev_vis > pair.inner.start && prev_vis <= pair.inner.end {
                let prev_g = previous_grapheme(&self.content, prev_vis);
                if prev_g <= pair.inner.start && prev_vis == pair.inner.end {
                    // Only 1 visible grapheme inside this formatted span! Delete the whole span + delimiters!
                    let start = span_open_start(&parsed, pair);
                    self.edit(start..group_end, "");
                    self.caret.move_to(start);
                    self.refresh_cursor();
                    return true;
                } else if cursor >= pair.inner.end && cursor <= group_end {
                    // Cursor was right outside the closing run -> delete the last
                    // visible grapheme inside `word`; never split a delimiter.
                    let deleted = prev_vis - prev_g;
                    self.edit(prev_g..prev_vis, "");
                    self.caret
                        .move_to(floor_boundary(&self.content, group_end - deleted));
                    self.refresh_cursor();
                    return true;
                }
            }
        }

        self.smart_backspace()
    }

    pub fn delete_forward_in_mode(&mut self, mode: ViewMode) {
        if self.is_read_only() {
            return;
        }
        if !self.selection_range().is_empty() {
            let range = self.selection_range();
            self.edit(range, "");
            return;
        }
        let cursor = self.cursor_offset();
        if cursor >= self.content.len() {
            return;
        }
        if mode == ViewMode::Wysiwyg {
            let parsed = self.parsed();
            if let Some(pline) = parsed.line_at_offset(cursor) {
                if let LineKind::TableRow {
                    ref cells,
                    ref raw_cells,
                    ..
                } = pline.kind
                {
                    let at_cell_end_or_pipe = raw_cells
                        .iter()
                        .zip(cells.iter())
                        .find(|(rc, _)| cursor >= rc.start && cursor <= rc.end)
                        .map(|(_, c)| cursor >= c.end)
                        .unwrap_or(true);
                    if at_cell_end_or_pipe {
                        return;
                    }
                }
                let vend = visible_line_end(pline, self.view_mode);
                if cursor == vend && vend < pline.source.end {
                    let after = pline.source.end;
                    let break_len = line_ending_len(&self.content, after);
                    if break_len > 0 {
                        self.edit(after..after + break_len, "");
                    }
                    return;
                }

                // Delete skips hidden delimiters like Backspace does, and
                // drops a span whose last visible grapheme it removes
                // instead of leaving lone markers behind.
                let next_vis = next_visible_pos(&parsed, cursor);
                if next_vis >= self.content.len() {
                    return;
                }
                for pair in line_pair_refs(&parsed, next_vis).iter().rev() {
                    if next_vis < pair.inner.start || next_vis >= pair.inner.end {
                        continue;
                    }
                    let only =
                        previous_grapheme(&self.content, pair.inner.end) <= pair.inner.start;
                    if only {
                        let start = span_open_start(&parsed, pair);
                        let end = span_close_end(&parsed, pair);
                        self.edit(start..end, "");
                        self.caret.move_to(floor_boundary(&self.content, start));
                        self.refresh_cursor();
                        return;
                    }
                    let g_end = next_grapheme(&self.content, next_vis);
                    self.edit(next_vis..g_end, "");
                    self.caret
                        .move_to(floor_boundary(&self.content, cursor.min(next_vis)));
                    self.refresh_cursor();
                    return;
                }
                let g_end = next_grapheme(&self.content, next_vis);
                self.edit(next_vis..g_end, "");
                self.caret.move_to(floor_boundary(&self.content, cursor.min(next_vis)));
                self.refresh_cursor();
                return;
            }
        }
        let next = next_grapheme(&self.content, cursor);
        self.edit(cursor..next, "");
    }

    #[allow(dead_code)]
    pub fn is_in_table(&mut self) -> bool {
        let cursor = self.cursor_offset();
        let parsed = self.parsed();
        parsed.lines.iter().any(|l| match l.kind {
            LineKind::TableRow {
                ref table_source, ..
            } => cursor >= table_source.start && cursor <= table_source.end,
            _ => false,
        })
    }

    fn extract_active_table(&mut self) -> Option<TableEditContext> {
        let cursor = self.cursor_offset();
        let parsed = self.parsed();
        let table_source = parsed.lines.iter().find_map(|l| {
            if let LineKind::TableRow {
                ref table_source, ..
            } = l.kind
            {
                if cursor >= table_source.start && cursor <= table_source.end {
                    return Some(table_source.clone());
                }
            }
            None
        })?;

        let mut num_cols = 1usize;
        let mut alignments = Vec::new();
        for l in &parsed.lines {
            if l.source.start >= table_source.start && l.source.end <= table_source.end {
                if let LineKind::TableRow {
                    alignments: ref al, ..
                } = l.kind
                {
                    num_cols = num_cols.max(al.len());
                    if alignments.is_empty() {
                        alignments = al.clone();
                    }
                }
            }
        }
        while alignments.len() < num_cols {
            alignments.push(TableColumnAlign::None);
        }

        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut active_row = 0usize;
        let mut active_col = 0usize;

        for l in &parsed.lines {
            if l.source.start >= table_source.start && l.source.end <= table_source.end {
                if let LineKind::TableRow { ref raw_cells, .. } = l.kind {
                    let r_idx = rows.len();
                    let line_text = self.content.get(l.source.clone()).unwrap_or("");
                    let split_cells = split_table_cells(line_text);
                    let mut row_vals = Vec::with_capacity(num_cols);
                    for c_idx in 0..num_cols {
                        row_vals.push(split_cells.get(c_idx).cloned().unwrap_or_default());
                    }
                    if cursor >= l.source.start && cursor <= l.source.end {
                        active_row = r_idx;
                        if let Some(c_idx) = raw_cells
                            .iter()
                            .position(|rc| cursor >= rc.start && cursor <= rc.end)
                        {
                            active_col = c_idx.min(num_cols - 1);
                        } else if let Some(c_idx) =
                            raw_cells.iter().rposition(|rc| rc.start <= cursor)
                        {
                            active_col = c_idx.min(num_cols - 1);
                        }
                    }
                    rows.push(row_vals);
                }
            }
        }

        if rows.is_empty() {
            return None;
        }

        Some(TableEditContext {
            table_source,
            rows,
            alignments,
            active_row,
            active_col,
        })
    }

    pub fn insert_table_with_size(&mut self, cols: usize, rows: usize) {
        if self.is_read_only() {
            return;
        }
        let cols = cols.clamp(1, 10);
        let rows = rows.clamp(1, 20);

        let mut range = self.selection_range();
        let parsed = self.parsed();
        if let Some(table_end) = parsed.lines.iter().find_map(|l| {
            if let LineKind::TableRow {
                ref table_source, ..
            } = l.kind
            {
                if range.start >= table_source.start && range.start <= table_source.end {
                    return Some(table_source.end);
                }
            }
            None
        }) {
            range = table_end..table_end;
        }

        let mut prefix = String::new();
        if range.start > 0 {
            let prev_char = self.content[..range.start].chars().next_back();
            if let Some(c) = prev_char {
                if c != '\n' {
                    prefix.push_str("\n\n");
                } else if !self.content[..range.start].ends_with("\n\n") {
                    prefix.push('\n');
                }
            }
        }
        let mut suffix = String::new();
        if range.end < self.content.len() {
            let next_char = self.content[range.end..].chars().next();
            if let Some(c) = next_char {
                if c != '\n' {
                    suffix.push_str("\n\n");
                } else if !self.content[range.end..].starts_with("\n\n") {
                    suffix.push('\n');
                }
            }
        }

        let mut table_rows: Vec<Vec<String>> = Vec::with_capacity(rows);
        for _ in 0..rows {
            table_rows.push(vec![String::new(); cols]);
        }
        let alignments = vec![TableColumnAlign::Left; cols];
        let (table_str, first_cell_rel) = format_markdown_table(&table_rows, &alignments, 0, 0);
        let full_insert = format!("{}{}{}", prefix, table_str, suffix);
        let base = range.start + prefix.len();
        self.edit(range, &full_insert);
        self.caret.range = (base + first_cell_rel.start)..(base + first_cell_rel.end);
        self.caret.reversed = false;
        self.refresh_cursor();
    }

    pub fn table_add_row_above(&mut self) {
        if self.is_read_only() {
            return;
        }
        let Some(mut ctx) = self.extract_active_table() else {
            return;
        };
        let num_cols = ctx.alignments.len().max(1);
        let insert_idx = ctx.active_row.min(ctx.rows.len());
        ctx.rows.insert(insert_idx, vec![String::new(); num_cols]);
        let (new_table, cell_rel) =
            format_markdown_table(&ctx.rows, &ctx.alignments, insert_idx, ctx.active_col);
        let base = ctx.table_source.start;
        self.edit(ctx.table_source, &new_table);
        self.caret
            .move_to(floor_boundary(&self.content, base + cell_rel.start));
        self.refresh_cursor();
    }

    pub fn table_add_row_below(&mut self) {
        if self.is_read_only() {
            return;
        }
        let Some(mut ctx) = self.extract_active_table() else {
            return;
        };
        let num_cols = ctx.alignments.len().max(1);
        let insert_idx = (ctx.active_row + 1).min(ctx.rows.len());
        ctx.rows.insert(insert_idx, vec![String::new(); num_cols]);
        let (new_table, cell_rel) =
            format_markdown_table(&ctx.rows, &ctx.alignments, insert_idx, ctx.active_col);
        let base = ctx.table_source.start;
        self.edit(ctx.table_source, &new_table);
        self.caret
            .move_to(floor_boundary(&self.content, base + cell_rel.start));
        self.refresh_cursor();
    }

    pub fn table_add_col_left(&mut self) {
        if self.is_read_only() {
            return;
        }
        let Some(mut ctx) = self.extract_active_table() else {
            return;
        };
        let insert_col = ctx.active_col.min(ctx.alignments.len());
        ctx.alignments.insert(insert_col, TableColumnAlign::None);
        for row in ctx.rows.iter_mut() {
            row.insert(insert_col.min(row.len()), String::new());
        }
        let (new_table, cell_rel) =
            format_markdown_table(&ctx.rows, &ctx.alignments, ctx.active_row, insert_col);
        let base = ctx.table_source.start;
        self.edit(ctx.table_source, &new_table);
        self.caret.range = (base + cell_rel.start)..(base + cell_rel.end);
        self.caret.reversed = false;
        self.refresh_cursor();
    }

    pub fn table_add_col_right(&mut self) {
        if self.is_read_only() {
            return;
        }
        let Some(mut ctx) = self.extract_active_table() else {
            return;
        };
        let insert_col = (ctx.active_col + 1).min(ctx.alignments.len());
        ctx.alignments.insert(insert_col, TableColumnAlign::None);
        for row in ctx.rows.iter_mut() {
            row.insert(insert_col.min(row.len()), String::new());
        }
        let (new_table, cell_rel) =
            format_markdown_table(&ctx.rows, &ctx.alignments, ctx.active_row, insert_col);
        let base = ctx.table_source.start;
        self.edit(ctx.table_source, &new_table);
        self.caret.range = (base + cell_rel.start)..(base + cell_rel.end);
        self.caret.reversed = false;
        self.refresh_cursor();
    }

    pub fn table_delete_row(&mut self) {
        if self.is_read_only() {
            return;
        }
        let Some(mut ctx) = self.extract_active_table() else {
            return;
        };
        if ctx.rows.len() <= 1 {
            self.table_delete_table();
            return;
        }
        let remove_idx = ctx.active_row.min(ctx.rows.len() - 1);
        ctx.rows.remove(remove_idx);
        let target_row = remove_idx.min(ctx.rows.len() - 1);
        let (new_table, cell_rel) =
            format_markdown_table(&ctx.rows, &ctx.alignments, target_row, ctx.active_col);
        let base = ctx.table_source.start;
        self.edit(ctx.table_source, &new_table);
        self.caret
            .move_to(floor_boundary(&self.content, base + cell_rel.start));
        self.refresh_cursor();
    }

    pub fn table_delete_col(&mut self) {
        if self.is_read_only() {
            return;
        }
        let Some(mut ctx) = self.extract_active_table() else {
            return;
        };
        if ctx.alignments.len() <= 1 {
            self.table_delete_table();
            return;
        }
        let remove_col = ctx.active_col.min(ctx.alignments.len() - 1);
        ctx.alignments.remove(remove_col);
        for row in &mut ctx.rows {
            if remove_col < row.len() {
                row.remove(remove_col);
            }
        }
        let target_col = remove_col.min(ctx.alignments.len() - 1);
        let (new_table, cell_rel) =
            format_markdown_table(&ctx.rows, &ctx.alignments, ctx.active_row, target_col);
        let base = ctx.table_source.start;
        self.edit(ctx.table_source, &new_table);
        self.caret
            .move_to(floor_boundary(&self.content, base + cell_rel.start));
        self.refresh_cursor();
    }

    pub fn table_delete_table(&mut self) {
        if self.is_read_only() {
            return;
        }
        let Some(ctx) = self.extract_active_table() else {
            return;
        };
        let mut del_range = ctx.table_source.clone();
        if del_range.end < self.content.len() && self.content.as_bytes()[del_range.end] == b'\n' {
            del_range.end += 1;
        } else if del_range.start > 0 && self.content.as_bytes()[del_range.start - 1] == b'\n' {
            del_range.start -= 1;
        }
        let target = del_range.start;
        self.edit(del_range, "");
        self.caret.move_to(floor_boundary(&self.content, target));
        self.refresh_cursor();
    }

    pub fn table_align_column(&mut self, align: TableColumnAlign) {
        if self.is_read_only() {
            return;
        }
        let Some(mut ctx) = self.extract_active_table() else {
            return;
        };
        if let Some(slot) = ctx.alignments.get_mut(ctx.active_col) {
            *slot = align;
        }
        let (new_table, cell_rel) =
            format_markdown_table(&ctx.rows, &ctx.alignments, ctx.active_row, ctx.active_col);
        let base = ctx.table_source.start;
        self.edit(ctx.table_source, &new_table);
        self.caret
            .move_to(floor_boundary(&self.content, base + cell_rel.start));
        self.refresh_cursor();
    }

    pub fn tab_table_cell(&mut self, backwards: bool) -> bool {
        let cursor = self.cursor_offset();
        let parsed = self.parsed();
        let mut all_cells: Vec<Range<usize>> = Vec::new();
        let mut in_table = false;
        let mut table_end = 0usize;
        let mut num_cols = 2usize;
        for line in &parsed.lines {
            if let LineKind::TableRow {
                ref cells,
                ref table_source,
                ..
            } = line.kind
            {
                if cursor >= table_source.start && cursor <= table_source.end {
                    in_table = true;
                    table_end = table_source.end;
                    num_cols = cells.len().max(num_cols);
                    all_cells.extend(cells.iter().cloned());
                }
            }
        }
        if !in_table || all_cells.is_empty() {
            return false;
        }
        let cur_idx = all_cells
            .iter()
            .position(|c| cursor >= c.start && cursor <= c.end)
            .unwrap_or_else(|| {
                all_cells
                    .iter()
                    .rposition(|c| c.start <= cursor)
                    .unwrap_or(0)
            });
        let next_idx = if backwards {
            if cur_idx == 0 {
                // First cell: stay put instead of wrapping to the last cell
                return true;
            }
            cur_idx - 1
        } else {
            if cur_idx + 1 == all_cells.len() {
                let new_row = format!("\n|{}", "  |".repeat(num_cols));
                self.edit(table_end..table_end, &new_row);
                let first_cell_start = (table_end + 3).min(self.content.len());
                let target = floor_boundary(&self.content, first_cell_start);
                self.caret.range = target..target;
                self.caret.reversed = false;
                self.refresh_cursor();
                return true;
            }
            cur_idx + 1
        };
        let target = all_cells[next_idx].clone();
        self.caret.range = target;
        self.caret.reversed = false;
        self.refresh_cursor();
        true
    }

    pub fn snap_wysiwyg_table_delimiter(&mut self) {
        let cursor = self.cursor_offset();
        let parsed = self.parsed();
        if let Some(pline) = parsed.line_at_offset(cursor) {
            if matches!(pline.kind, LineKind::TableDelimiter) {
                let next_pos = (pline.source.end + 1).min(self.content.len());
                let target = floor_boundary(&self.content, next_pos);
                self.caret.move_to(target);
                self.refresh_cursor();
            }
        }
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
    fn snap_caret_to_visible_line_end(&mut self) {
        if !self.caret.range.is_empty() {
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
        let target = super::text_ops::home_offset(&self.content, self.cursor_offset());
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
            if let LineKind::TableRow {
                ref cells,
                ref raw_cells,
                ..
            } = pline.kind
            {
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
        let start = super::text_ops::home_offset(&self.content, cursor);
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
            if let LineKind::TableRow {
                ref cells,
                ref raw_cells,
                ..
            } = pline.kind
            {
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
        let current_line_start = super::text_ops::home_offset(&self.content, cursor);
        let target = if cursor > current_line_start {
            current_line_start
        } else if current_line_start > 0 {
            super::text_ops::home_offset(&self.content, current_line_start - 1)
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
            .unwrap_or_else(|| super::text_ops::line_end(&self.content, cursor));
        let target = if cursor < source_end {
            source_end + line_ending_len(&self.content, source_end).saturating_sub(1)
        } else if source_end < self.content.len() {
            let next = source_end + line_ending_len(&self.content, source_end);
            let next_end = parsed
                .line_at_offset(next)
                .map(|line| line.source.end)
                .unwrap_or_else(|| super::text_ops::line_end(&self.content, next));
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
            super::text_ops::word_left(&self.content, self.selection_range().start)
        } else {
            super::text_ops::word_left(&self.content, self.cursor_offset())
        };
        self.place(target, extend);
    }

    pub fn move_word_right(&mut self, extend: bool) {
        let origin = if !extend && !self.caret.range.is_empty() {
            self.selection_range().end
        } else {
            self.cursor_offset()
        };
        let mut target = super::text_ops::word_right(&self.content, origin);
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

    pub fn delete_word_left(&mut self) {
        if self.is_read_only() {
            return;
        }
        if !self.caret.range.is_empty() {
            self.edit(self.selection_range(), "");
        } else {
            let cursor = self.cursor_offset();
            let target = super::text_ops::word_left(&self.content, cursor);
            if target < cursor {
                self.edit(target..cursor, "");
            }
        }
    }

    pub fn delete_word_right(&mut self) {
        if self.is_read_only() {
            return;
        }
        if !self.caret.range.is_empty() {
            self.edit(self.selection_range(), "");
        } else {
            let cursor = self.cursor_offset();
            let mut target = super::text_ops::word_right(&self.content, cursor);
            if let Some(line) = self.parsed().line_at_offset(cursor) {
                let vend = visible_line_end(line, self.view_mode);
                if vend < line.source.end && cursor <= vend && target > vend {
                    target = vend;
                }
            }
            if target > cursor {
                self.edit(cursor..target, "");
            }
        }
    }

    pub fn delete_to_line_start(&mut self) {
        if self.is_read_only() {
            return;
        }
        if !self.caret.range.is_empty() {
            self.edit(self.selection_range(), "");
        } else {
            let cursor = self.cursor_offset();
            let target = super::text_ops::home_offset(&self.content, cursor);
            let end = {
                let parsed = self.parsed();
                match parsed.line_at_offset(cursor) {
                    Some(line) => {
                        let vend = visible_line_end(line, self.view_mode);
                        if cursor == vend && vend < line.source.end {
                            line.source.end
                        } else {
                            cursor
                        }
                    }
                    None => cursor,
                }
            };
            if target < end {
                self.edit(target..end, "");
            }
        }
    }

    pub fn delete_to_line_end(&mut self) {
        if self.is_read_only() {
            return;
        }
        if !self.caret.range.is_empty() {
            self.edit(self.selection_range(), "");
        } else {
            let cursor = self.cursor_offset();
            let raw = super::text_ops::line_end(&self.content, cursor);
            let target = self.parsed().line_at_offset(cursor).map_or(raw, |line| {
                let vend = visible_line_end(line, self.view_mode);
                if cursor < vend {
                    let end = line.source.end;
                    end + line_ending_len(&self.content, end).saturating_sub(1)
                } else {
                    vend
                }
            });
            if target > cursor {
                self.edit(cursor..target, "");
            }
        }
    }

    pub fn select_all(&mut self) {
        self.caret.range = 0..self.content.len();
        self.caret.reversed = false;
        self.caret.preferred_col = None;
        self.refresh_cursor();
    }

    pub fn select_word(&mut self) {
        let range = super::text_ops::word_extent(&self.content, self.cursor_offset());
        self.caret.range = range;
        self.caret.reversed = false;
        self.caret.preferred_col = None;
        self.refresh_cursor();
    }

    pub fn select_line(&mut self) {
        let range = super::text_ops::line_extent(&self.content, self.cursor_offset());
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

    pub fn selected_text(&self) -> String {
        let range = self.selection_range();
        self.content.get(range).unwrap_or("").to_string()
    }

    pub fn find_hit_ranges(&self, term: &str, match_case: bool) -> Vec<Range<usize>> {
        find_matches(&self.content, term, match_case)
    }

    pub fn select_match(
        &mut self,
        term: &str,
        match_case: bool,
        current: Option<usize>,
        forward: bool,
    ) -> Option<usize> {
        let hits = find_matches(&self.content, term, match_case);
        let index = if forward {
            find_next(&hits, self.cursor_offset(), current)
        } else {
            find_prev(&hits, self.selection_range().start, current)
        }?;
        let hit = hits.get(index)?.clone();
        self.caret.range = hit;
        self.caret.reversed = false;
        self.caret.preferred_col = None;
        self.refresh_cursor();
        Some(index)
    }

    pub fn replace_current_match(
        &mut self,
        term: &str,
        replacement: &str,
        match_case: bool,
        current: Option<usize>,
    ) -> Option<usize> {
        if self.is_read_only() || term.is_empty() {
            return None;
        }
        let hits = find_matches(&self.content, term, match_case);
        let index = current
            .filter(|index| hits.get(*index).is_some())
            .or_else(|| find_next(&hits, self.cursor_offset(), None))?;
        let hit = hits.get(index)?.clone();
        self.edit(hit, replacement);
        self.select_match(term, match_case, None, true)
    }

    pub fn replace_every_match(&mut self, term: &str, replacement: &str, match_case: bool) -> bool {
        if self.is_read_only() {
            return false;
        }
        let Some(updated) = replace_all(&self.content, term, replacement, match_case) else {
            return false;
        };
        self.set_content(updated);
        true
    }

    pub fn refresh_stats(&mut self) {
        let stats = compute_stats(&self.content);
        self.word_count = stats.words;
        self.char_count = stats.chars;
        self.line_count = stats.lines;
        self.reading_time_min = stats.reading_time_min;
        self.stats_revision = Some(self.revision);
    }

    /// Compute word/char/line stats only when a reader (the status bar) needs
    /// them, keyed by document revision so edits themselves skip the scan.
    pub fn ensure_stats(&mut self) {
        if self.stats_revision == Some(self.revision) {
            return;
        }
        self.refresh_stats();
    }

    pub fn refresh_cursor(&mut self) {
        let cursor = self.cursor_offset();
        let (line, col) = line_col(&self.content, cursor);
        self.cursor_line = line;
        self.cursor_col = col;
        let parsed = self.parsed();
        self.active_formats = compute_active_formats(&parsed, cursor, &self.pending_formats);
    }

    pub fn undo(&mut self) -> bool {
        let Some(caret) = self.history.undo(&mut self.content) else {
            return false;
        };
        self.revision = self.revision.wrapping_add(1);
        self.pending_formats = ActiveFormats::default();
        self.place_after_history(caret);
        self.snap_caret_to_visible_line_end();
        self.refresh_cursor();
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(caret) = self.history.redo(&mut self.content) else {
            return false;
        };
        self.revision = self.revision.wrapping_add(1);
        self.pending_formats = ActiveFormats::default();
        self.place_after_history(caret);
        self.snap_caret_to_visible_line_end();
        self.refresh_cursor();
        true
    }

    fn place_after_history(&mut self, caret: usize) {
        let caret = floor_boundary(
            &self.content,
            snap_off_ending_interior(&self.content, caret),
        );
        self.caret.move_to(caret);
        self.caret.clamp(&self.content);
        self.marked_range = None;
        self.is_modified = true;
        self.refresh_cursor();
    }

    fn clamp_range(&self, range: Range<usize>) -> Range<usize> {
        let start = floor_boundary(&self.content, range.start);
        let end = floor_boundary(&self.content, range.end);
        if end < start {
            end..start
        } else {
            start..end
        }
    }
}

#[derive(Debug, Clone)]
struct TableEditContext {
    table_source: Range<usize>,
    rows: Vec<Vec<String>>,
    alignments: Vec<TableColumnAlign>,
    active_row: usize,
    active_col: usize,
}

fn format_markdown_table(
    rows: &[Vec<String>],
    alignments: &[TableColumnAlign],
    target_row: usize,
    target_col: usize,
) -> (String, Range<usize>) {
    let num_cols = alignments.len().max(1);
    let mut out = String::new();
    let mut target_cell_range = 0..0;

    for (r_idx, row) in rows.iter().enumerate() {
        if r_idx > 0 {
            out.push('\n');
        }
        out.push('|');
        for c_idx in 0..num_cols {
            let cell_text = table_cell_source(row.get(c_idx).map(|s| s.as_str()).unwrap_or(""));
            out.push(' ');
            let c_start = out.len();
            out.push_str(&cell_text);
            let c_end = out.len();
            out.push_str(" |");
            if r_idx == target_row && c_idx == target_col {
                target_cell_range = c_start..c_end;
            }
        }
        if r_idx == 0 {
            out.push_str("\n|");
            for c_idx in 0..num_cols {
                let align = alignments
                    .get(c_idx)
                    .copied()
                    .unwrap_or(TableColumnAlign::None);
                let delim = match align {
                    TableColumnAlign::None => " --- |",
                    TableColumnAlign::Left => " :--- |",
                    TableColumnAlign::Center => " :---: |",
                    TableColumnAlign::Right => " ---: |",
                };
                out.push_str(delim);
            }
        }
    }

    (out, target_cell_range)
}

/// Write a cell back to Markdown. An existing `\|` stays escaped, and a bare `|` becomes `\|`.
fn table_cell_source(cell: &str) -> String {
    let mut out = String::with_capacity(cell.len());
    let mut chars = cell.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' && chars.peek() == Some(&'|') {
            out.push('\\');
            out.push('|');
            chars.next();
        } else if ch == '|' {
            out.push('\\');
            out.push('|');
        } else {
            out.push(ch);
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockFormatTarget {
    Heading(u8),
    BulletList,
    OrderedList,
    TaskList,
    Blockquote,
}

/// Turning a checklist off on an ordered task keeps `N.` / `N)`.
///
/// A line that is not already a task stays on the shared `- [ ] ` path, so a
/// plain ordered item still converts the same way the kit does. Markers the
/// document does not accept (column 4 or more, or an interrupting empty
/// marker) are not tasks here either.
fn ordered_checklist_line(
    content: &str,
    line_start: usize,
    target: BlockFormatTarget,
    turn_off: bool,
) -> Option<String> {
    let line_end_pos = line_end(content, line_start);
    let line = &content[line_start..line_end_pos];
    let item = accepted_list_marker(content, line_start)?;
    // An ordered task shows the checklist button on and the ordered-list
    // button off. The ordered-list click leaves the line unchanged so it
    // matches that off button.
    if target == BlockFormatTarget::OrderedList {
        if item.ordered && item.task.is_some() {
            return Some(line.to_string());
        }
        return None;
    }
    if target != BlockFormatTarget::TaskList {
        return None;
    }
    if !item.ordered || item.task.is_none() {
        return None;
    }
    if !turn_off {
        return Some(line.to_string());
    }
    let indent = &line[..item.indent_bytes];
    let bytes = line.as_bytes();
    let mut digit_end = item.indent_bytes;
    while digit_end < bytes.len() && bytes[digit_end].is_ascii_digit() {
        digit_end += 1;
    }
    let digits = &line[item.indent_bytes..digit_end];
    let punct = item.punct as char;
    let body = &line[item.prefix_end()..];
    Some(format!("{indent}{digits}{punct} {body}"))
}

fn parse_line_block_prefix(
    content: &str,
    line_start: usize,
) -> (usize, Option<BlockFormatTarget>, usize) {
    let line_end_pos = line_end(content, line_start);
    let line = &content[line_start..line_end_pos];
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    if indent <= 3 {
        for (lvl, pfx) in [
            (6u8, "###### "),
            (5, "##### "),
            (4, "#### "),
            (3, "### "),
            (2, "## "),
            (1, "# "),
        ] {
            if trimmed.starts_with(pfx) {
                return (
                    indent,
                    Some(BlockFormatTarget::Heading(lvl)),
                    indent + pfx.len(),
                );
            }
        }
    }
    if let Some(item) = accepted_list_marker(content, line_start) {
        let target = if item.task.is_some() && !item.ordered {
            BlockFormatTarget::TaskList
        } else if item.ordered {
            BlockFormatTarget::OrderedList
        } else {
            BlockFormatTarget::BulletList
        };
        return (item.indent_bytes, Some(target), item.prefix_end());
    }
    if trimmed.starts_with("> ") {
        return (indent, Some(BlockFormatTarget::Blockquote), indent + 2);
    }
    if trimmed == ">" {
        return (indent, Some(BlockFormatTarget::Blockquote), indent + 1);
    }
    (indent, None, indent)
}

/// True when `from..to` is only whitespace and delimiter markup, so a
/// neighboring span with matching flags can merge across that gap.
fn gap_is_ws_and_markers(content: &str, hidden: &[Range<usize>], from: usize, to: usize) -> bool {
    if to < from {
        return false;
    }
    let mut i = from;
    while i < to {
        if content[i..].starts_with('\n') {
            return false;
        }
        if let Some(h) = hidden.iter().find(|r| i >= r.start && i < r.end) {
            if !is_delimiter_markup(&content[h.start..h.end]) {
                return false;
            }
            i = h.end;
            continue;
        }
        let Some(ch) = content[i..].chars().next() else {
            break;
        };
        if ch.is_whitespace() && ch != '\n' {
            i += ch.len_utf8();
            continue;
        }
        return false;
    }
    true
}

fn is_delimiter_markup(s: &str) -> bool {
    let t = s.trim();
    !t.is_empty()
        && (t.chars().all(|c| matches!(c, '*' | '_' | '~' | '`'))
            || t.eq_ignore_ascii_case("<u>")
            || t.eq_ignore_ascii_case("</u>"))
}

fn covering_span(
    parsed: &ParsedDocument,
    pline: &ParsedLine,
    seg: &StyledSegment,
) -> (usize, usize) {
    let overlapping: Vec<&InlinePair> = pline
        .inline_pairs
        .iter()
        .filter(|p| {
            p.kind != InlineTagKind::Link
                && p.inner.start <= seg.source.start
                && p.inner.end >= seg.source.end
        })
        .collect();
    if overlapping.is_empty() {
        return (seg.source.start, seg.source.end);
    }
    let open = overlapping
        .iter()
        .map(|p| span_open_start(parsed, p))
        .min()
        .unwrap_or(seg.source.start);
    let close = overlapping
        .iter()
        .map(|p| span_close_end(parsed, p))
        .max()
        .unwrap_or(seg.source.end);
    (open, close)
}

/// When turning a format on, extend the rewrite so a neighboring span that
/// already has the same flags is merged instead of emitting `**aa**** bb**`.
fn expand_replace_to_matching_neighbors(
    content: &str,
    parsed: &ParsedDocument,
    pline: &ParsedLine,
    p_range: Range<usize>,
    mut replace: Range<usize>,
    tag_kind: InlineTagKind,
    should_turn_on: bool,
) -> (usize, usize) {
    if !should_turn_on {
        return (replace.start, replace.end);
    }
    let p_start = p_range.start;
    let p_end = p_range.end;
    let overlapping: Vec<&StyledSegment> = pline
        .styled_segments
        .iter()
        .filter(|s| s.source.start < p_end && s.source.end > p_start)
        .collect();
    if overlapping
        .windows(2)
        .any(|w| segment_flags(w[0]) != segment_flags(w[1]))
    {
        return (replace.start, replace.end);
    }
    let mut target = overlapping
        .first()
        .map(|s| segment_flags(s))
        .unwrap_or_default();
    match tag_kind {
        InlineTagKind::Bold => target.bold = true,
        InlineTagKind::Italic => target.italic = true,
        InlineTagKind::Underline => target.underline = true,
        InlineTagKind::Strike => target.strike = true,
        InlineTagKind::Code => target.code = true,
        InlineTagKind::Link => return (replace.start, replace.end),
    }
    if !target.any_set() {
        return (replace.start, replace.end);
    }

    let mut left_front = p_start;
    let mut right_front = p_end;
    loop {
        let mut changed = false;
        for seg in &pline.styled_segments {
            if segment_flags(seg) != target {
                continue;
            }
            if seg.source.end <= left_front
                && gap_is_ws_and_markers(content, &pline.hidden_ranges, seg.source.end, left_front)
            {
                let (open, close) = covering_span(parsed, pline, seg);
                if open < replace.start || close > replace.end || seg.source.start < left_front {
                    replace.start = replace.start.min(open);
                    replace.end = replace.end.max(close);
                    left_front = left_front.min(seg.source.start);
                    changed = true;
                }
            }
            if seg.source.start >= right_front
                && gap_is_ws_and_markers(
                    content,
                    &pline.hidden_ranges,
                    right_front,
                    seg.source.start,
                )
            {
                let (open, close) = covering_span(parsed, pline, seg);
                if close > replace.end || open < replace.start || seg.source.end > right_front {
                    replace.start = replace.start.min(open);
                    replace.end = replace.end.max(close);
                    right_front = right_front.max(seg.source.end);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    (replace.start, replace.end)
}

/// The end of the whole closing-marker run of the stacked group `pair`
/// belongs to (the non-link pairs sharing its content range), so inserted
/// spaces and exits clear every stacked closer (`***`, `~~**`).
fn span_close_end(_parsed: &ParsedDocument, pair: &InlinePair) -> usize {
    pair.group_close_end
}

/// The start of the whole opening-marker run of the stacked group `pair`
/// belongs to, so whole-span edits cover every stacked opener (`***`, `<u>**`).
fn span_open_start(_parsed: &ParsedDocument, pair: &InlinePair) -> usize {
    pair.group_open_start
}

fn nearby_pair_refs(parsed: &ParsedDocument, range: Range<usize>) -> Vec<&InlinePair> {
    parsed
        .lines_near(range)
        .iter()
        .flat_map(|l| l.inline_pairs.iter())
        .collect()
}

fn range_has_non_link_pairs(parsed: &ParsedDocument, range: Range<usize>) -> bool {
    parsed.lines_near(range.clone()).iter().any(|line| {
        line.inline_pairs.iter().any(|p| {
            p.kind != InlineTagKind::Link
                && p.open_delim.start < range.end
                && span_close_end(parsed, p) > range.start
        })
    })
}

fn line_pair_refs(parsed: &ParsedDocument, offset: usize) -> &[InlinePair] {
    parsed
        .line_at_offset(offset)
        .map(|l| l.inline_pairs.as_slice())
        .unwrap_or(&[])
}

fn pair_id(p: &InlinePair) -> (usize, usize, InlineTagKind) {
    (p.open_delim.start, p.close_delim.end, p.kind)
}

/// Connected component of overlapping non-link pairs on one line. Pairs on
/// different lines have disjoint source ranges, so a document-wide expansion
/// is the union of per-line expansions.
fn expand_line_group_pairs<'a>(
    line: &'a ParsedLine,
    parsed: &ParsedDocument,
    range: Range<usize>,
    group_pairs: &mut Vec<&'a InlinePair>,
    seen: &mut HashSet<(usize, usize, InlineTagKind)>,
) {
    let mut cands: Vec<&InlinePair> = line
        .inline_pairs
        .iter()
        .filter(|p| p.kind != InlineTagKind::Link)
        .collect();
    if cands.is_empty() {
        return;
    }
    cands.sort_by_key(|p| (p.open_delim.start, span_close_end(parsed, p)));
    let mut lo = range.start;
    let mut hi = range.end.max(range.start);
    loop {
        let lim = cands.partition_point(|p| p.open_delim.start < hi);
        let mut nlo = lo;
        let mut nhi = hi;
        for p in &cands[..lim] {
            let close = span_close_end(parsed, p);
            if close > nlo {
                nlo = nlo.min(p.open_delim.start);
                nhi = nhi.max(close);
            }
        }
        if nlo == lo && nhi == hi {
            break;
        }
        lo = nlo;
        hi = nhi;
    }
    group_pairs.reserve(cands.len());
    let lim = cands.partition_point(|p| p.open_delim.start < hi);
    for p in cands[..lim].iter().copied() {
        if span_close_end(parsed, p) > lo && seen.insert(pair_id(p)) {
            group_pairs.push(p);
        }
    }
}

fn collect_group_pairs(parsed: &ParsedDocument, range: Range<usize>) -> Vec<&InlinePair> {
    let lines = parsed.lines_near(range.clone());
    let mut group_pairs = Vec::with_capacity(lines.iter().map(|l| l.inline_pairs.len()).sum());
    let mut seen = HashSet::with_capacity(group_pairs.capacity());
    for line in lines {
        expand_line_group_pairs(line, parsed, range.clone(), &mut group_pairs, &mut seen);
    }
    group_pairs
}

fn add_contained_pairs<'a>(
    parsed: &'a ParsedDocument,
    replace: Range<usize>,
    group_pairs: &mut Vec<&'a InlinePair>,
) {
    let mut seen: HashSet<(usize, usize, InlineTagKind)> =
        group_pairs.iter().copied().map(pair_id).collect();
    for line in parsed.lines_near(replace.clone()) {
        for p in &line.inline_pairs {
            if p.kind == InlineTagKind::Link {
                continue;
            }
            if p.open_delim.start >= replace.start
                && span_close_end(parsed, p) <= replace.end
                && seen.insert(pair_id(p))
            {
                group_pairs.push(p);
            }
        }
    }
}

/// The next source position at or after `pos` that is not inside a hidden
/// (WYSIWYG-invisible) range.
fn next_visible_pos(parsed: &ParsedDocument, mut pos: usize) -> usize {
    while let Some(r) = parsed.hidden_ranges.iter().find(|r| pos >= r.start && pos < r.end) {
        pos = r.end;
    }
    pos
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FormatFlags {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub code: bool,
}

impl FormatFlags {
    fn any_set(self) -> bool {
        self.bold || self.italic || self.underline || self.strike || self.code
    }

    /// Bold, italic, and strike markers cannot open or close next to a space.
    fn flanking_sensitive(self) -> bool {
        self.bold || self.italic || self.strike
    }

    /// Flags that may stay on a space peeled out of a flanking-sensitive span.
    /// Underline tags do not depend on spaces, so the space can remain inside
    /// `<u>` while sitting outside `*` / `**` / `~~`.
    fn glue(self) -> Self {
        Self {
            underline: self.underline,
            ..Self::default()
        }
    }
}

#[derive(Clone, Copy)]
enum SharedKind {
    Underline,
    Strike,
    BoldItalic,
    Bold,
    Italic,
    Code,
}

fn flag_of(s: &WorkSlice, kind: SharedKind) -> bool {
    match kind {
        SharedKind::Underline => s.flags.underline,
        SharedKind::Strike => s.flags.strike,
        SharedKind::BoldItalic => s.flags.bold && s.flags.italic,
        SharedKind::Bold => s.flags.bold,
        SharedKind::Italic => s.flags.italic,
        SharedKind::Code => s.flags.code,
    }
}

fn wrap_for_shared(kind: SharedKind) -> (String, String) {
    match kind {
        SharedKind::Underline => ("<u>".into(), "</u>".into()),
        SharedKind::Strike => ("~~".into(), "~~".into()),
        SharedKind::BoldItalic => ("***".into(), "***".into()),
        SharedKind::Bold => ("**".into(), "**".into()),
        SharedKind::Italic => ("*".into(), "*".into()),
        SharedKind::Code => ("`".into(), "`".into()),
    }
}

fn clear_shared(mut flags: FormatFlags, kind: SharedKind) -> FormatFlags {
    match kind {
        SharedKind::Underline => flags.underline = false,
        SharedKind::Strike => flags.strike = false,
        SharedKind::BoldItalic => {
            flags.bold = false;
            flags.italic = false;
        }
        SharedKind::Bold => flags.bold = false,
        SharedKind::Italic => flags.italic = false,
        SharedKind::Code => flags.code = false,
    }
    flags
}

/// Longest consecutive run that shares a wrap-able flag, but is not the whole
/// list (`all_*` already handled that). Prefers outer kinds on a tie.
fn longest_shared_flag_run(slices: &[WorkSlice]) -> Option<(SharedKind, usize, usize)> {
    let kinds = [
        SharedKind::Underline,
        SharedKind::Strike,
        SharedKind::BoldItalic,
        SharedKind::Bold,
        SharedKind::Italic,
        SharedKind::Code,
    ];
    let mut best: Option<(SharedKind, usize, usize, usize)> = None;
    for kind in kinds {
        let mut i = 0;
        while i < slices.len() {
            if !flag_of(&slices[i], kind) {
                i += 1;
                continue;
            }
            let mut j = i;
            while j + 1 < slices.len() && flag_of(&slices[j + 1], kind) {
                j += 1;
            }
            let len = j - i + 1;
            if len >= 2 && len < slices.len() {
                let better = best.is_none_or(|(_, _, _, best_len)| len > best_len);
                if better {
                    best = Some((kind, i, j, len));
                }
            }
            i = j + 1;
        }
    }
    best.map(|(k, i, j, _)| (k, i, j))
}

fn segment_flags(seg: &StyledSegment) -> FormatFlags {
    FormatFlags {
        bold: seg.bold,
        italic: seg.italic,
        underline: seg.underline,
        strike: seg.strike,
        code: seg.code,
    }
}

/// One piece of text going through `render_styled_slices`, with the selected
/// byte range (if any) tracked independently of the wrap so adjacent pieces
/// with the same flags can merge into one span.
#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkSlice {
    text: String,
    flags: FormatFlags,
    sel: Option<Range<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledSlice {
    pub text: String,
    pub flags: FormatFlags,
    pub is_selected: bool,
}

fn wrap_delimiters(flags: &FormatFlags) -> (String, String) {
    let mut open = String::new();
    let mut close = String::new();
    if flags.underline {
        open.push_str("<u>");
        close.push_str("</u>");
    }
    if flags.strike {
        open.push_str("~~");
        close.insert_str(0, "~~");
    }
    if flags.bold && flags.italic {
        open.push_str("***");
        close.insert_str(0, "***");
    } else if flags.bold {
        open.push_str("**");
        close.insert_str(0, "**");
    } else if flags.italic {
        open.push('*');
        close.insert(0, '*');
    }
    if flags.code {
        open.push('`');
        close.insert(0, '`');
    }
    (open, close)
}

fn append_work_slice(dest: &mut WorkSlice, src: &WorkSlice) {
    let at = dest.text.len();
    dest.text.push_str(&src.text);
    if let Some(sel) = &src.sel {
        dest.sel = Some(match &dest.sel {
            Some(prev) => prev.start..(at + sel.end),
            None => (at + sel.start)..(at + sel.end),
        });
    }
}

fn merge_work_slices(slices: &[WorkSlice]) -> Vec<WorkSlice> {
    let mut merged: Vec<WorkSlice> = Vec::with_capacity(slices.len());
    for s in slices {
        if s.text.is_empty() {
            continue;
        }
        if let Some(last) = merged.last_mut() {
            if last.flags == s.flags {
                append_work_slice(last, s);
                continue;
            }
        }
        merged.push(s.clone());
    }
    merged
}

fn sel_overlap(sel: &Option<Range<usize>>, start: usize, end: usize) -> Option<Range<usize>> {
    let sel = sel.as_ref()?;
    let a = sel.start.max(start).min(end);
    let b = sel.end.max(start).min(end);
    if b > a {
        Some((a - start)..(b - start))
    } else {
        None
    }
}

/// Pull leading/trailing spaces out of flanking-sensitive spans so a split
/// at a word edge cannot write `aa** bb` or `***aa **bb`. Underline stays
/// on the peeled space so `<u>` can still wrap the whole phrase.
fn peel_flanking_whitespace(slices: Vec<WorkSlice>) -> Vec<WorkSlice> {
    if slices.len() <= 1 {
        return slices;
    }
    let mut out = Vec::new();
    for s in slices {
        if !s.flags.flanking_sensitive() || s.text.chars().all(char::is_whitespace) {
            out.push(s);
            continue;
        }
        let lead = s.text.len() - s.text.trim_start().len();
        let trail = s.text.len() - s.text.trim_end().len();
        if lead == 0 && trail == 0 {
            out.push(s);
            continue;
        }
        let glue = s.flags.glue();
        let core_end = s.text.len() - trail;
        if lead > 0 {
            out.push(WorkSlice {
                text: s.text[..lead].to_string(),
                flags: glue,
                sel: sel_overlap(&s.sel, 0, lead),
            });
        }
        if core_end > lead {
            out.push(WorkSlice {
                text: s.text[lead..core_end].to_string(),
                flags: s.flags,
                sel: sel_overlap(&s.sel, lead, core_end),
            });
        }
        if trail > 0 {
            out.push(WorkSlice {
                text: s.text[core_end..].to_string(),
                flags: glue,
                sel: sel_overlap(&s.sel, core_end, s.text.len()),
            });
        }
    }
    merge_work_slices(&out)
}

fn can_coalesce_ws_triple(left: &WorkSlice, mid: &WorkSlice, right: &WorkSlice) -> bool {
    let mid_ws = !mid.text.is_empty() && mid.text.chars().all(char::is_whitespace);
    let flags_match = left.flags == right.flags && left.flags.any_set();
    let mid_ok = mid.flags == FormatFlags::default() || mid.flags == left.flags;
    mid_ws && flags_match && mid_ok
}

/// `[bold "aa"] [plain " "] [bold "bb"]` becomes one span so turning a
/// format back on at a neighbor merges instead of writing `**aa**** bb**`.
fn coalesce_ws_between_matching(slices: Vec<WorkSlice>) -> Vec<WorkSlice> {
    let mut out: Vec<WorkSlice> = Vec::with_capacity(slices.len());
    for s in slices {
        out.push(s);
        while out.len() >= 3 {
            let n = out.len();
            if !can_coalesce_ws_triple(&out[n - 3], &out[n - 2], &out[n - 1]) {
                break;
            }
            let right = out.pop().unwrap();
            let mid = out.pop().unwrap();
            let left = out.last_mut().unwrap();
            append_work_slice(left, &mid);
            append_work_slice(left, &right);
        }
    }
    out
}

fn map_work_flags(
    slices: &[WorkSlice],
    mut f: impl FnMut(FormatFlags) -> FormatFlags,
) -> Vec<WorkSlice> {
    slices
        .iter()
        .map(|s| WorkSlice {
            text: s.text.clone(),
            flags: f(s.flags),
            sel: s.sel.clone(),
        })
        .collect()
}

fn mark_work_sel(sel_range: &mut Option<Range<usize>>, at: usize, s: &WorkSlice) {
    if let Some(sel) = &s.sel {
        let piece = (at + sel.start)..(at + sel.end);
        *sel_range = Some(match sel_range.take() {
            Some(prev) => prev.start.min(piece.start)..prev.end.max(piece.end),
            None => piece,
        });
    }
}

fn union_sel(sel_range: &mut Option<Range<usize>>, piece: Range<usize>) {
    *sel_range = Some(match sel_range.take() {
        Some(prev) => prev.start.min(piece.start)..prev.end.max(piece.end),
        None => piece,
    });
}

fn mark_wrapped_sel(
    sel_range: &mut Option<Range<usize>>,
    offset: usize,
    prefix_len: usize,
    suffix_len: usize,
    s: &WorkSlice,
    include_wraps: bool,
) {
    if let Some(sel) = &s.sel {
        if include_wraps && sel.start == 0 && sel.end == s.text.len() {
            union_sel(
                sel_range,
                offset..(offset + prefix_len + s.text.len() + suffix_len),
            );
        } else {
            mark_work_sel(sel_range, offset + prefix_len, s);
        }
    }
}

fn wrap_work_slice(
    s: &WorkSlice,
    prefix: &str,
    suffix: &str,
    offset: usize,
    sel_range: &mut Option<Range<usize>>,
    include_wraps: bool,
) -> String {
    mark_wrapped_sel(
        sel_range,
        offset,
        prefix.len(),
        suffix.len(),
        s,
        include_wraps,
    );
    format!("{}{}{}", prefix, s.text, suffix)
}

fn render_styled_slices(
    slices: &[StyledSlice],
    sel_range: &mut Option<Range<usize>>,
    offset: usize,
) -> String {
    render_styled_slices_opts(slices, sel_range, offset, false, false)
}

fn render_styled_slices_opts(
    slices: &[StyledSlice],
    sel_range: &mut Option<Range<usize>>,
    offset: usize,
    glued_left: bool,
    include_wraps_in_sel: bool,
) -> String {
    let work: Vec<WorkSlice> = slices
        .iter()
        .map(|s| WorkSlice {
            text: s.text.clone(),
            flags: s.flags,
            sel: if s.is_selected {
                Some(0..s.text.len())
            } else {
                None
            },
        })
        .collect();
    render_work_slices(&work, sel_range, offset, glued_left, include_wraps_in_sel)
}

fn slice_has_edge_ws(s: &WorkSlice) -> bool {
    s.text.starts_with(char::is_whitespace) || s.text.ends_with(char::is_whitespace)
}

fn render_work_slices(
    slices: &[WorkSlice],
    sel_range: &mut Option<Range<usize>>,
    offset: usize,
    glued_left: bool,
    include_wraps_in_sel: bool,
) -> String {
    // Merge matching flags first so turning a format back on joins the
    // neighbor. Do not peel yet: shared outer formats (`*…*`, `<u>…</u>`)
    // must still wrap spaces, which is valid as long as the opener/closer
    // sit against non-space at the ends.
    let merged = coalesce_ws_between_matching(merge_work_slices(slices));

    if merged.is_empty() {
        return String::new();
    }
    if merged.len() == 1 {
        let s = &merged[0];
        let (prefix, suffix) = wrap_delimiters(&s.flags);
        return wrap_work_slice(s, &prefix, &suffix, offset, sel_range, include_wraps_in_sel);
    }

    let all_underline = merged.iter().all(|s| s.flags.underline);
    let all_strike = merged.iter().all(|s| s.flags.strike);
    let all_code = merged.iter().all(|s| s.flags.code);
    let all_bold = merged.iter().all(|s| s.flags.bold);
    let all_italic = merged.iter().all(|s| s.flags.italic);

    if all_underline {
        let inner = map_work_flags(&merged, |mut f| {
            f.underline = false;
            f
        });
        let inner_str = render_work_slices(&inner, sel_range, offset + 3, false, include_wraps_in_sel);
        return format!("<u>{}</u>", inner_str);
    }

    if all_strike {
        let inner = map_work_flags(&merged, |mut f| {
            f.strike = false;
            f
        });
        let inner_str = render_work_slices(&inner, sel_range, offset + 2, false, include_wraps_in_sel);
        return format!("~~{}~~", inner_str);
    }

    if all_bold && all_italic {
        let inner = map_work_flags(&merged, |mut f| {
            f.bold = false;
            f.italic = false;
            f
        });
        let inner_str = render_work_slices(&inner, sel_range, offset + 3, false, include_wraps_in_sel);
        return format!("***{}***", inner_str);
    }

    if all_italic {
        let is_middle = merged.len() == 3
            && !merged.iter().any(|s| {
                s.flags.strike || s.flags.underline || s.flags.code || slice_has_edge_ws(s)
            })
            && merged[0].flags.bold
            && !merged[1].flags.bold
            && merged[2].flags.bold;
        if is_middle {
            if glued_left {
                let mut out = String::new();
                for s in &merged {
                    let (prefix, suffix) = if s.flags.bold {
                        ("***", "***")
                    } else {
                        ("*", "*")
                    };
                    mark_work_sel(sel_range, offset + out.len() + prefix.len(), s);
                    out.push_str(prefix);
                    out.push_str(&s.text);
                    out.push_str(suffix);
                }
                return out;
            }
            let mut out = String::from("_");
            for s in &merged {
                let prefix = if s.flags.bold { "**" } else { "" };
                let suffix = if s.flags.bold { "**" } else { "" };
                mark_work_sel(sel_range, offset + out.len() + prefix.len(), s);
                out.push_str(prefix);
                out.push_str(&s.text);
                out.push_str(suffix);
            }
            out.push('_');
            return out;
        }

        let inner = map_work_flags(&merged, |mut f| {
            f.italic = false;
            f
        });
        let inner_str = render_work_slices(&inner, sel_range, offset + 1, false, include_wraps_in_sel);
        return format!("*{}*", inner_str);
    }

    if all_bold {
        let is_middle = merged.len() == 3
            && !merged.iter().any(|s| {
                s.flags.strike || s.flags.underline || s.flags.code || slice_has_edge_ws(s)
            })
            && merged[0].flags.italic
            && !merged[1].flags.italic
            && merged[2].flags.italic;
        if is_middle {
            let mut out = String::new();
            for s in &merged {
                let (prefix, suffix) = if s.flags.italic {
                    ("_**", "**_")
                } else {
                    ("**", "**")
                };
                mark_work_sel(sel_range, offset + out.len() + prefix.len(), s);
                out.push_str(prefix);
                out.push_str(&s.text);
                out.push_str(suffix);
            }
            return out;
        }

        let inner = map_work_flags(&merged, |mut f| {
            f.bold = false;
            f
        });
        let inner_str = render_work_slices(&inner, sel_range, offset + 2, false, include_wraps_in_sel);
        return format!("**{}**", inner_str);
    }

    if all_code {
        // A code span's content is literal, so it must never contain other
        // delimiters. Keep one backtick span only when the non-code flags are
        // identical across the slices (they then differ only by selection);
        // otherwise close and reopen the code span around every non-code
        // change, keeping the backticks adjacent to the text.
        let non_code_equal = merged.iter().all(|s| {
            s.flags.bold == merged[0].flags.bold
                && s.flags.italic == merged[0].flags.italic
                && s.flags.underline == merged[0].flags.underline
                && s.flags.strike == merged[0].flags.strike
        });
        if non_code_equal {
            let inner = map_work_flags(&merged, |mut f| {
                f.code = false;
                f
            });
            let inner_str = render_work_slices(&inner, sel_range, offset + 1, false, include_wraps_in_sel);
            return format!("`{}`", inner_str);
        }
    }

    if let Some((kind, mut i, mut j)) = longest_shared_flag_run(&merged) {
        let ws = |s: &WorkSlice| !s.text.is_empty() && s.text.chars().all(char::is_whitespace);
        let orig_i = i;
        let orig_j = j;
        while j > i && ws(&merged[j]) {
            j -= 1;
        }
        while i < j && ws(&merged[i]) {
            i += 1;
        }
        if j > i && (i > 0 || j + 1 < merged.len()) {
            let (prefix, suffix) = wrap_for_shared(kind);
            // Collide against the slices that stay outside the original run,
            // not the whitespace just peeled off (that whitespace loses this
            // flag and will not wrap).
            let left_collide = orig_i > 0 && {
                let prev_close = wrap_delimiters(&merged[orig_i - 1].flags).1;
                !prev_close.is_empty()
                    && !prefix.is_empty()
                    && prev_close.chars().next_back() == prefix.chars().next()
            };
            let right_collide = orig_j + 1 < merged.len() && {
                let next_open = wrap_delimiters(&merged[orig_j + 1].flags).0;
                !suffix.is_empty()
                    && !next_open.is_empty()
                    && suffix.chars().next_back() == next_open.chars().next()
            };
            // Skip a wrap whose new markers would sit against a neighbor's
            // `*`/`**` and form `*a***`; peel + alternate delimiters instead.
            if !left_collide && !right_collide {
                // Whitespace peeled out of the wrap must lose that flag, or a
                // trailing space stays `** **now` instead of `** …** now`.
                let mut work = merged.clone();
                for s in &mut work[orig_i..i] {
                    s.flags = clear_shared(s.flags, kind);
                }
                if orig_j < work.len() {
                    for s in &mut work[j + 1..=orig_j] {
                        s.flags = clear_shared(s.flags, kind);
                    }
                }
                let left = render_work_slices(
                    &work[..i],
                    sel_range,
                    offset,
                    glued_left,
                    include_wraps_in_sel,
                );
                let inner = map_work_flags(&work[i..=j], |f| clear_shared(f, kind));
                let mid = render_work_slices(
                    &inner,
                    sel_range,
                    offset + left.len() + prefix.len(),
                    false,
                    include_wraps_in_sel,
                );
                let right = render_work_slices(
                    &work[j + 1..],
                    sel_range,
                    offset + left.len() + prefix.len() + mid.len() + suffix.len(),
                    false,
                    include_wraps_in_sel,
                );
                return format!("{left}{prefix}{mid}{suffix}{right}");
            }
        }
    }

    // Formats diverge here, so peel spaces out of flanking-sensitive spans
    // before writing markers. That turns `** aa` / `aa **` into ` **aa` /
    // `aa** ` and stops `***` / `****` from forming at a word edge.
    let merged = coalesce_ws_between_matching(peel_flanking_whitespace(merged));
    if merged.len() <= 1 {
        return render_work_slices(&merged, sel_range, offset, glued_left, include_wraps_in_sel);
    }

    // Adjacent spans whose delimiters collide (a closer and the next opener)
    // sharing a character) merge into one ambiguous run that CommonMark's
    // delimiter arithmetic mis-parses. Resolve collisions before rendering:
    // switch the later span to a delimiter style whose outermost character
    // differs (`__`/`_` for bold and italic, strike nested inside the other
    // formats), unless plain text follows it - then switch the earlier one,
    // whose new closer sits against the next span's opener instead.
    let mut use_alt = vec![false; merged.len()];
    for i in 1..merged.len() {
        let (_, prev_close) = wrap_delimiters(&merged[i - 1].flags);
        let (next_open, _) = wrap_delimiters(&merged[i].flags);
        let collide = !prev_close.is_empty()
            && !next_open.is_empty()
            && prev_close.ends_with(next_open.chars().next().unwrap());
        if !collide {
            continue;
        }
        let avoid = next_open.chars().next().unwrap();
        let alt_available = alternate_wrap(&merged[i].flags, avoid).is_some();
        let plain_follows =
            i + 1 == merged.len() || wrap_delimiters(&merged[i + 1].flags).0.is_empty();
        if alt_available && !plain_follows {
            use_alt[i] = true;
        } else if alt_available && alternate_wrap(&merged[i - 1].flags, avoid).is_some() {
            // Plain text follows this span: its underscore closer would go
            // intraword. Switch the earlier span instead; its new closer sits
            // against this span's opener, which is a delimiter character.
            use_alt[i - 1] = true;
        } else if alt_available {
            use_alt[i] = true;
        }
    }
    let mut out = String::new();
    for (idx, s) in merged.iter().enumerate() {
        let (prefix, suffix) = if use_alt[idx] {
            let avoid = if idx > 0 {
                wrap_delimiters(&merged[idx - 1].flags)
                    .1
                    .chars()
                    .next_back()
                    .unwrap_or('*')
            } else {
                '*'
            };
            alternate_wrap(&s.flags, avoid).unwrap_or_else(|| wrap_delimiters(&s.flags))
        } else {
            wrap_delimiters(&s.flags)
        };
        mark_wrapped_sel(
            sel_range,
            offset + out.len(),
            prefix.len(),
            suffix.len(),
            s,
            include_wraps_in_sel,
        );
        out.push_str(&prefix);
        out.push_str(&s.text);
        out.push_str(&suffix);
    }
    out
}

/// An alternate delimiter wrapping for `flags` whose outermost delimiter
/// character differs from `avoid`, used where two adjacent spans would
/// otherwise merge their closing and opening runs into one ambiguous run.
/// Code spans stay innermost in every variant; returns None when no
/// alternate exists.
fn alternate_wrap(flags: &FormatFlags, avoid: char) -> Option<(String, String)> {
    let mut open = String::new();
    let mut close = String::new();
    if avoid == '*' && (flags.bold || flags.italic) {
        // Bold and italic have a second syntax each; the underscore style
        // moves the boundary character off `*`.
        if flags.bold {
            open.push_str("__");
            close.insert_str(0, "__");
        } else {
            open.push('_');
            close.insert(0, '_');
        }
        if flags.bold && flags.italic {
            open.push('*');
            close.insert(0, '*');
        }
        if flags.code {
            open.push('`');
            close.insert(0, '`');
        }
        return Some((open, close));
    }
    if avoid == '~' && flags.strike {
        // Strike has no second syntax; nest it inside the other formats.
        let mut outer = false;
        if flags.underline {
            open.push_str("<u>");
            close.insert_str(0, "</u>");
            outer = true;
        }
        if flags.bold && flags.italic {
            open.push_str("***");
            close.insert_str(0, "***");
            outer = true;
        } else if flags.bold {
            open.push_str("**");
            close.insert_str(0, "**");
            outer = true;
        } else if flags.italic {
            open.push('*');
            close.insert(0, '*');
            outer = true;
        }
        if outer {
            open.push_str("~~");
            close.insert_str(0, "~~");
            return Some((open, close));
        }
    }
    None
}

fn render_styled_slices_with_selection_glued(
    slices: &[StyledSlice],
    glued_left: bool,
) -> (String, Range<usize>) {
    render_styled_slices_with_selection_opts(slices, glued_left, false)
}

fn render_styled_slices_with_selection_opts(
    slices: &[StyledSlice],
    glued_left: bool,
    include_wraps_in_sel: bool,
) -> (String, Range<usize>) {
    let mut sel = None;
    let res = render_styled_slices_opts(slices, &mut sel, 0, glued_left, include_wraps_in_sel);
    let range = sel.unwrap_or(0..res.len());
    (res, range)
}

/// The raw slice for a styled segment, including a backslash that escapes
/// its first character: escapes are consumed by the parser, so rebuilding a
/// region from segments must re-emit them or the escape is lost.
fn segment_raw_range(content: &str, source: &Range<usize>) -> Range<usize> {
    let mut start = source.start;
    if start > 0
        && content.as_bytes().get(start - 1) == Some(&b'\\')
        && content[start..].chars().next().is_some_and(|c| c.is_ascii_punctuation())
        && content.as_bytes().get(start - 2) != Some(&b'\\')
    {
        start -= 1;
    }
    start..source.end
}

fn apply_toggle(flags: &mut FormatFlags, kind: InlineTagKind, on: bool) {
    match kind {
        InlineTagKind::Bold => flags.bold = on,
        InlineTagKind::Italic => flags.italic = on,
        InlineTagKind::Underline => flags.underline = on,
        InlineTagKind::Strike => flags.strike = on,
        InlineTagKind::Code => flags.code = on,
        InlineTagKind::Link => {}
    }
}

fn segment_flags_at(segs: &[&StyledSegment], pos: usize) -> Option<FormatFlags> {
    let i = segs.partition_point(|s| s.source.start <= pos);
    if i == 0 {
        return None;
    }
    let s = segs[i - 1];
    if pos < s.source.end {
        Some(segment_flags(s))
    } else {
        None
    }
}

#[derive(Clone, Copy, Default)]
struct FlagCounts {
    bold: u16,
    italic: u16,
    underline: u16,
    strike: u16,
    code: u16,
}

impl FlagCounts {
    fn apply(&mut self, kind: InlineTagKind, open: bool) {
        let slot = match kind {
            InlineTagKind::Bold => &mut self.bold,
            InlineTagKind::Italic => &mut self.italic,
            InlineTagKind::Underline => &mut self.underline,
            InlineTagKind::Strike => &mut self.strike,
            InlineTagKind::Code => &mut self.code,
            InlineTagKind::Link => return,
        };
        if open {
            *slot = slot.saturating_add(1);
        } else {
            *slot = slot.saturating_sub(1);
        }
    }

    fn flags(self) -> FormatFlags {
        FormatFlags {
            bold: self.bold > 0,
            italic: self.italic > 0,
            underline: self.underline > 0,
            strike: self.strike > 0,
            code: self.code > 0,
        }
    }
}

/// Open/close events sorted by position so flags at successive offsets are O(1)
/// amortized. Closes (`-1`) sort before opens (`1`) at the same byte because
/// `pos < close.end` and `pos >= open.start`.
struct FlagSweep {
    events: Vec<(usize, i8, InlineTagKind)>,
    i: usize,
    counts: FlagCounts,
}

impl FlagSweep {
    fn new(pairs: &[&InlinePair]) -> Self {
        let mut events = Vec::with_capacity(pairs.len() * 2);
        for p in pairs {
            if p.kind == InlineTagKind::Link {
                continue;
            }
            events.push((p.open_delim.start, 1, p.kind));
            events.push((p.close_delim.end, -1, p.kind));
        }
        events.sort_unstable_by_key(|e| (e.0, e.1));
        Self {
            events,
            i: 0,
            counts: FlagCounts::default(),
        }
    }

    fn flags_at(&mut self, pos: usize) -> FormatFlags {
        while self.i < self.events.len() && self.events[self.i].0 <= pos {
            let (_, delta, kind) = self.events[self.i];
            self.counts.apply(kind, delta > 0);
            self.i += 1;
        }
        self.counts.flags()
    }
}

/// Openers and closers of every non-link span the caret sits in, merged into
/// adjacent runs so nested `**… *…* …**` closes as `***` / `***`.
fn spanning_marker_runs(
    content: &str,
    parsed: &ParsedDocument,
    cursor: usize,
) -> Option<(String, String)> {
    let mut spanning: Vec<InlinePair> = line_pair_refs(parsed, cursor)
        .iter()
        .filter(|p| {
            p.kind != InlineTagKind::Link
                && cursor > p.open_delim.start
                && cursor < p.close_delim.end
                && cursor >= p.inner.start
                && cursor <= p.inner.end
        })
        .cloned()
        .collect();
    if spanning.is_empty() {
        return None;
    }
    spanning.sort_by_key(|p| p.open_delim.start);
    let open_ranges =
        merge_adjacent_ranges(spanning.iter().map(|p| p.open_delim.clone()).collect());
    let close_ranges =
        merge_adjacent_ranges(spanning.iter().map(|p| p.close_delim.clone()).collect());
    let openers: String = open_ranges.iter().map(|r| &content[r.clone()]).collect();
    let closers: String = close_ranges.iter().map(|r| &content[r.clone()]).collect();
    Some((openers, closers))
}

fn merge_adjacent_ranges(mut rs: Vec<Range<usize>>) -> Vec<Range<usize>> {
    if rs.is_empty() {
        return rs;
    }
    rs.sort_by_key(|r| r.start);
    let mut out = vec![rs[0].clone()];
    for r in rs.into_iter().skip(1) {
        let last = out.last_mut().unwrap();
        if r.start <= last.end {
            last.end = last.end.max(r.end);
        } else {
            out.push(r);
        }
    }
    out
}

fn merge_skip_ranges(mut rs: Vec<Range<usize>>) -> Vec<Range<usize>> {
    rs.retain(|r| r.end > r.start);
    if rs.len() <= 1 {
        return rs;
    }
    rs.sort_by_key(|r| (r.start, r.end));
    let mut out: Vec<Range<usize>> = Vec::with_capacity(rs.len());
    for r in rs {
        if let Some(last) = out.last_mut() {
            if r.start <= last.end {
                last.end = last.end.max(r.end);
                continue;
            }
        }
        out.push(r);
    }
    out
}

fn skip_end_at(skips: &[Range<usize>], pos: usize) -> Option<usize> {
    let i = skips.partition_point(|r| r.start <= pos);
    if i == 0 {
        return None;
    }
    let r = &skips[i - 1];
    if pos < r.end {
        Some(r.end)
    } else {
        None
    }
}

fn selected_link_markup_ranges(pairs: &[&InlinePair], p_range: Range<usize>) -> Vec<Range<usize>> {
    let mut rs = Vec::new();
    for p in pairs {
        if p.kind != InlineTagKind::Link
            || p.inner.start < p_range.start
            || p.inner.end > p_range.end
            || p.inner.end <= p.inner.start
        {
            continue;
        }
        if p.open_delim.start < p.inner.start {
            rs.push(p.open_delim.start..p.inner.start);
        }
        if p.inner.end < p.close_delim.end {
            rs.push(p.inner.end..p.close_delim.end);
        }
    }
    merge_skip_ranges(rs)
}

/// Rebuild a source range from styled segments **and** the hidden markdown
/// between them (link/image syntax, HTML, escapes), skipping only the
/// delimiter runs in `skip` (the emphasis markers being rewritten).
fn collect_rewrite_slices(
    content: &str,
    pairs: &[&InlinePair],
    segs: &[&StyledSegment],
    replace: Range<usize>,
    p_range: Range<usize>,
    skip: &[Range<usize>],
    toggle: Option<(InlineTagKind, bool)>,
) -> Vec<StyledSlice> {
    let skip = merge_skip_ranges(skip.to_vec());
    let mut segs = segs.to_vec();
    segs.sort_by_key(|s| (s.source.start, s.source.end));
    let mut sweep = FlagSweep::new(pairs);
    let link_markup = selected_link_markup_ranges(pairs, p_range.clone());
    let mut bounds = Vec::with_capacity(
        8 + skip.len() * 2 + segs.len() * 2 + replace.end.saturating_sub(replace.start) / 4,
    );
    bounds.push(replace.start);
    bounds.push(replace.end);
    bounds.push(p_range.start.clamp(replace.start, replace.end));
    bounds.push(p_range.end.clamp(replace.start, replace.end));
    for r in &skip {
        if r.start > replace.start && r.start < replace.end {
            bounds.push(r.start);
        }
        if r.end > replace.start && r.end < replace.end {
            bounds.push(r.end);
        }
    }
    for seg in &segs {
        let raw = segment_raw_range(content, &seg.source);
        if raw.start > replace.start && raw.start < replace.end {
            bounds.push(raw.start);
        }
        if raw.end > replace.start && raw.end < replace.end {
            bounds.push(raw.end);
        }
    }
    // Split whitespace from words so a selected `" wo"` does not keep the
    // leading space inside `* wo*` (nf2). Interior spaces between matching
    // flags are coalesced again when wrapping.
    let mut pos = replace.start;
    while pos < replace.end {
        if let Some(end) = skip_end_at(&skip, pos) {
            pos = end.max(pos + 1);
            continue;
        }
        let ch = match content[pos..].chars().next() {
            Some(c) => c,
            None => break,
        };
        let next = pos + ch.len_utf8();
        if next < replace.end {
            let nch = match content[next..].chars().next() {
                Some(c) => c,
                None => break,
            };
            if ch == '\n' || nch == '\n' || ch.is_whitespace() != nch.is_whitespace() {
                bounds.push(next);
            }
        }
        pos = next;
    }
    bounds.sort_unstable();
    bounds.dedup();

    let turning_off = toggle.is_some_and(|(_, on)| !on);
    let mut slices: Vec<StyledSlice> = Vec::with_capacity(bounds.len().saturating_sub(1));
    for w in bounds.windows(2) {
        let a = w[0];
        let b = w[1];
        if a >= b || skip_end_at(&skip, a).is_some() {
            continue;
        }
        let piece = &content[a..b];
        let is_break = piece == "\n";
        let is_ws = !is_break && piece.chars().all(char::is_whitespace);
        let is_sel = a < p_range.end && b > p_range.start;
        let mut flags = if is_break {
            FormatFlags::default()
        } else {
            let swept = sweep.flags_at(a);
            segment_flags_at(&segs, a).unwrap_or(swept)
        };
        let in_selected_link_markup = turning_off && skip_end_at(&link_markup, a).is_some();
        // Turning a format on skips whitespace so flanking spaces stay
        // outside `*`/`**`/`~~`. Turning one off must still clear it from
        // selected spaces, or `**RAM** **usage**` becomes `RAM** **usage`.
        if (is_sel && !is_break && (!is_ws || turning_off)) || in_selected_link_markup {
            if let Some((kind, on)) = toggle {
                apply_toggle(&mut flags, kind, on);
            }
        }
        if let Some(last) = slices.last_mut() {
            if last.flags == flags && last.is_selected == is_sel && !is_break && last.text != "\n" {
                last.text.push_str(piece);
                continue;
            }
        }
        slices.push(StyledSlice {
            text: piece.to_string(),
            flags,
            is_selected: is_sel,
        });
    }
    slices
}

fn extend_or_splice_span(
    content: &str,
    parsed: &ParsedDocument,
    pending: &ActiveFormats,
    caret: usize,
    text: &str,
) -> Option<(Range<usize>, String, usize)> {
    if caret == 0 {
        return None;
    }
    let has_any_armed =
        pending.bold || pending.italic || pending.underline || pending.strike || pending.code;
    if !has_any_armed {
        return None;
    }
    let has_space = content.as_bytes().get(caret - 1) == Some(&b' ');
    let closer_end = if has_space { caret - 1 } else { caret };
    // Two continuation shapes: typing right after a span's closing run (or
    // after the trailing space it owns), and typing at the end of a span's
    // content while formats are armed - the second keeps the span's own
    // formats on the new text instead of nesting raw markers inside it.
    // Several stacked pairs can share a closing run; prefer one whose group
    // carries every armed format.
    let mut after_candidates: Vec<InlinePair> = line_pair_refs(parsed, closer_end)
        .iter()
        .rev()
        .filter(|p| {
            p.kind != InlineTagKind::Link
                && (span_close_end(parsed, p) == closer_end
                    || close_run_reaches(content, p, closer_end))
        })
        .cloned()
        .collect();
    let is_armed = |k: InlineTagKind| match k {
        InlineTagKind::Bold => pending.bold,
        InlineTagKind::Italic => pending.italic,
        InlineTagKind::Underline => pending.underline,
        InlineTagKind::Strike => pending.strike,
        InlineTagKind::Code => pending.code,
        InlineTagKind::Link => false,
    };
    // Several stacked pairs can share a closing run; prefer one whose group
    // carries every armed format. A pair whose closer sits inside that run
    // (it ends at the caret too) is also a candidate.
    after_candidates.sort_by_key(|p| {
        let carries_armed = [
            InlineTagKind::Bold,
            InlineTagKind::Italic,
            InlineTagKind::Underline,
            InlineTagKind::Strike,
            InlineTagKind::Code,
        ]
        .iter()
        .all(|&k| {
            !is_armed(k)
                || line_pair_refs(parsed, closer_end)
                    .iter()
                    .any(|q| q.kind == k && q.inner == p.inner)
        });
        if carries_armed {
            0
        } else {
            1
        }
    });
    for a in &after_candidates {
        if let Some(r) = splice_for_anchor(content, parsed, pending, caret, text, a, false) {
            return Some(r);
        }
    }
    let inside_candidate = line_pair_refs(parsed, caret)
        .iter()
        .rev()
        .find(|p| {
            p.kind != InlineTagKind::Link && p.inner.end == caret && p.inner.end > p.inner.start
        })
        .cloned()?;
    splice_for_anchor(content, parsed, pending, caret, text, &inside_candidate, true)
}

/// True when the maximal delimiter run that contains `pair`'s closer extends
/// up to `caret`, i.e. the caret stands right after the shared closing run.
fn close_run_reaches(content: &str, pair: &InlinePair, caret: usize) -> bool {
    let bytes = content.as_bytes();
    let Some(delim) = bytes.get(pair.close_delim.start).copied() else {
        return false;
    };
    pair.close_delim.end < caret
        && bytes[pair.close_delim.end..caret].iter().all(|&b| b == delim)
}

fn splice_for_anchor(
    content: &str,
    parsed: &ParsedDocument,
    pending: &ActiveFormats,
    caret: usize,
    text: &str,
    anchor: &InlinePair,
    inside: bool,
) -> Option<(Range<usize>, String, usize)> {
    let group: Vec<InlinePair> = line_pair_refs(parsed, anchor.inner.start)
        .iter()
        .filter(|p| p.kind != InlineTagKind::Link && p.inner == anchor.inner)
        .cloned()
        .collect();
    if group.is_empty() {
        return None;
    }
    let is_armed = |k: InlineTagKind| match k {
        InlineTagKind::Bold => pending.bold,
        InlineTagKind::Italic => pending.italic,
        InlineTagKind::Underline => pending.underline,
        InlineTagKind::Strike => pending.strike,
        InlineTagKind::Code => pending.code,
        InlineTagKind::Link => false,
    };
    let all_armed_in_group = [
        InlineTagKind::Bold,
        InlineTagKind::Italic,
        InlineTagKind::Underline,
        InlineTagKind::Strike,
        InlineTagKind::Code,
    ]
    .iter()
    .all(|&k| !is_armed(k) || group.iter().any(|p| p.kind == k));
    if !inside && !all_armed_in_group {
        return None;
    }
    if inside {
        // Only when the span is the innermost thing around the caret;
        // distinct nested pairs keep the old behavior.
        let nested = line_pair_refs(parsed, caret).iter().any(|p| {
            p.kind != InlineTagKind::Link
                && p.inner != anchor.inner
                && caret >= p.open_delim.start
                && caret <= p.close_delim.end
        });
        if nested {
            return None;
        }
    }

    let has_space = content.as_bytes().get(caret - 1) == Some(&b' ');
    let closer_end = if has_space { caret - 1 } else { caret };
    let group_open_start = span_open_start(parsed, anchor);
    let group_close_end = span_close_end(parsed, anchor);
    // Pairs whose closing markers sit inside the shared run that ends at the
    // caret are re-rendered together with the anchor's group, so their
    // openers are not left dangling by the replacement.
    let mut range_start = group_open_start;
    if !inside && caret > group_close_end {
        let delim = content.as_bytes()[anchor.close_delim.start];
        for p in line_pair_refs(parsed, caret) {
            if p.kind == InlineTagKind::Link || p.inner == anchor.inner {
                continue;
            }
            if p.close_delim.end > group_close_end
                && p.close_delim.end <= caret
                && content[group_close_end..p.close_delim.end]
                    .bytes()
                    .all(|b| b == delim)
            {
                range_start = range_start.min(span_open_start(parsed, p));
            }
        }
    }

    let trail_len = if inside {
        0
    } else {
        text.len() - text.trim_end().len()
    };
    let trail_ws = &text[text.len() - trail_len..];
    let trimmed_text = &text[..text.len() - trail_len];
    let lead_text = if inside {
        trimmed_text.to_string()
    } else if has_space {
        format!(" {}", trimmed_text)
    } else {
        trimmed_text.to_string()
    };

    let in_group = |k: InlineTagKind| group.iter().any(|p| p.kind == k);
    let cont_flags = if inside {
        FormatFlags {
            bold: pending.bold || in_group(InlineTagKind::Bold),
            italic: pending.italic || in_group(InlineTagKind::Italic),
            underline: pending.underline || in_group(InlineTagKind::Underline),
            strike: pending.strike || in_group(InlineTagKind::Strike),
            code: pending.code || in_group(InlineTagKind::Code),
        }
    } else {
        FormatFlags {
            bold: pending.bold,
            italic: pending.italic,
            underline: pending.underline,
            strike: pending.strike,
            code: pending.code,
        }
    };

    let seg_end_limit = if inside { caret } else { closer_end };
    let pline = parsed.line_at_offset(range_start);
    let existing_segs: Vec<&StyledSegment> = pline
        .map(|l| {
            l.styled_segments
                .iter()
                .filter(|s| s.source.start >= range_start && s.source.end <= seg_end_limit)
                .collect()
        })
        .unwrap_or_default();

    let mut slices: Vec<StyledSlice> = if !existing_segs.is_empty() {
        existing_segs
            .iter()
            .map(|s| {
                let raw = segment_raw_range(content, &s.source);
                StyledSlice {
                    text: content[raw].to_string(),
                    flags: FormatFlags {
                        bold: s.bold,
                        italic: s.italic,
                        underline: s.underline,
                        strike: s.strike,
                        code: s.code,
                    },
                    is_selected: false,
                }
            })
            .collect()
    } else {
        let anchor_flags = FormatFlags {
            bold: in_group(InlineTagKind::Bold),
            italic: in_group(InlineTagKind::Italic),
            underline: in_group(InlineTagKind::Underline),
            strike: in_group(InlineTagKind::Strike),
            code: in_group(InlineTagKind::Code),
        };
        vec![StyledSlice {
            text: content[anchor.inner.clone()].to_string(),
            flags: anchor_flags,
            is_selected: false,
        }]
    };

    if !lead_text.is_empty() {
        slices.push(StyledSlice {
            text: lead_text,
            flags: cont_flags,
            is_selected: true,
        });
    }

    let mut sel = None;
    let rendered = render_styled_slices(&slices, &mut sel, 0);
    let repl = format!("{}{}", rendered, trail_ws);
    let replace_end = if inside { group_close_end } else { caret };
    let new_caret = if trail_len > 0 {
        range_start + repl.len()
    } else if let Some(s) = sel {
        range_start + s.end
    } else {
        range_start + repl.len()
    };
    Some((range_start..replace_end, repl, new_caret))
}

/// Inline formats that stay armed at `cursor`, from every marker pair around
/// it. Used to keep typing inside an edited span and to carry formats across
/// a line break.
fn pending_formats_at(parsed: &ParsedDocument, cursor: usize) -> ActiveFormats {
    let mut fmt = ActiveFormats::default();
    for pair in line_pair_refs(parsed, cursor) {
        if cursor >= pair.open_delim.start && cursor <= pair.close_delim.end {
            match pair.kind {
                InlineTagKind::Bold => fmt.bold = true,
                InlineTagKind::Italic => fmt.italic = true,
                InlineTagKind::Underline => fmt.underline = true,
                InlineTagKind::Strike => fmt.strike = true,
                InlineTagKind::Code => fmt.code = true,
                InlineTagKind::Link => {}
            }
        }
    }
    fmt
}

fn compute_active_formats(
    parsed: &ParsedDocument,
    cursor: usize,
    pending: &ActiveFormats,
) -> ActiveFormats {
    let mut fmt = *pending;
    if let Some(line) = parsed.line_at_offset(cursor) {
        match line.kind {
            LineKind::Heading { level: 1 } => fmt.h1 = true,
            LineKind::Heading { level: 2 } => fmt.h2 = true,
            LineKind::Heading { level: 3.. } => fmt.h3 = true,
            LineKind::ListItem { ordered, task, .. } => {
                if task.is_some() {
                    fmt.checklist = true;
                } else if ordered {
                    fmt.ol = true;
                } else {
                    fmt.ul = true;
                }
            }
            LineKind::Blockquote => fmt.quote = true,
            LineKind::CodeFence { .. } | LineKind::CodeLine => fmt.code = true,
            LineKind::TableRow { .. } | LineKind::TableDelimiter => fmt.table = true,
            _ => {}
        }
        for pair in &line.inline_pairs {
            if cursor >= pair.open_delim.start && cursor <= pair.close_delim.end {
                match pair.kind {
                    InlineTagKind::Bold => fmt.bold = true,
                    InlineTagKind::Italic => fmt.italic = true,
                    InlineTagKind::Underline => fmt.underline = true,
                    InlineTagKind::Strike => fmt.strike = true,
                    InlineTagKind::Code => fmt.code = true,
                    InlineTagKind::Link => fmt.link = true,
                }
            }
        }
    }
    fmt
}

fn skip_hidden_left(parsed: &ParsedDocument, mut pos: usize) -> usize {
    while let Some(h) = parsed
        .hidden_ranges
        .iter()
        .find(|r| pos > r.start && pos <= r.end)
    {
        pos = h.start;
    }
    pos
}

fn skip_hidden_interior_left(parsed: &ParsedDocument, mut pos: usize) -> usize {
    while let Some(h) = parsed
        .hidden_ranges
        .iter()
        .find(|r| pos > r.start && pos < r.end)
    {
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

fn skip_hidden_right(parsed: &ParsedDocument, mut pos: usize) -> usize {
    if let Some(line) = parsed.line_at_offset(pos) {
        let vend = visible_line_end(line, ViewMode::Wysiwyg);
        if pos == vend && vend < line.source.end {
            return pos;
        }
    }
    while let Some(h) = parsed
        .hidden_ranges
        .iter()
        .find(|r| pos >= r.start && pos < r.end)
    {
        pos = h.end;
    }
    pos
}

fn floor_boundary(content: &str, offset: usize) -> usize {
    let offset = offset.min(content.len());
    if content.is_char_boundary(offset) {
        offset
    } else {
        super::text_ops::floor_char_boundary(content, offset)
    }
}

#[derive(Clone)]
struct TableRowInfo {
    source: Range<usize>,
    cells: Vec<Range<usize>>,
    is_header: bool,
}

#[derive(Clone)]
struct TableInfo {
    source: Range<usize>,
    rows: Vec<TableRowInfo>,
    delimiter_end: Option<usize>,
}

/// Given the text after a table's end, return where to insert what so a blank
/// line separates the table from the caret line, and where the caret goes
/// (all offsets relative to the start of `rest`, caret in the edited text).
/// An existing blank line is reused.
fn blank_gap_after_table(rest: &str) -> (usize, &'static str, usize) {
    if rest.is_empty() {
        return (0, "\n\n", 2);
    }
    let ne = line_ending_len(rest, 0);
    if ne == 0 {
        return (0, "\n\n", 2);
    }
    let after = &rest[ne..];
    if after.is_empty() {
        return (ne, "\n", ne + 1);
    }
    let l2_end = line_end(after, 0);
    if after[..l2_end].trim().is_empty() {
        let end_len = line_ending_len(after, l2_end);
        (0, "", ne + l2_end + end_len)
    } else {
        (ne, "\n", ne + 1)
    }
}


fn outer_is_delimited(content: &str, start: usize, end: usize, prefix: &str, suffix: &str) -> bool {
    if start > content.len() || end > content.len() || start > end {
        return false;
    }
    if !content.is_char_boundary(start) || !content.is_char_boundary(end) {
        return false;
    }
    if !content[..start].ends_with(prefix) || !content[end..].starts_with(suffix) {
        return false;
    }
    if prefix == "*" {
        let extra_before = start > 1 && content.as_bytes().get(start - 2) == Some(&b'*');
        let extra_after = content.as_bytes().get(end + 1) == Some(&b'*');
        let triple_before = start > 2 && content.as_bytes().get(start - 3) == Some(&b'*');
        let triple_after = content.as_bytes().get(end + 2) == Some(&b'*');
        let triple = extra_before && triple_before && extra_after && triple_after;
        let single = !extra_before && !extra_after;
        triple || single
    } else if prefix == "`" {
        let extra_before = start > 1 && content.as_bytes().get(start - 2) == Some(&b'`');
        let extra_after = content.as_bytes().get(end + 1) == Some(&b'`');
        !extra_before && !extra_after
    } else {
        true
    }
}

fn is_single_link(text: &str) -> Option<&str> {
    if !text.starts_with('[') || !text.ends_with(')') || text.starts_with("[^") {
        return None;
    }
    let close_bracket = text.find("](")?;
    let inner = &text[1..close_bracket];
    if inner.contains('[') || inner.contains(']') {
        return None;
    }
    let url_part = &text[close_bracket + 2..text.len() - 1];
    if url_part.contains(')') {
        return None;
    }
    Some(inner)
}

fn unwrap_outer_link(content: &str, start: usize, end: usize) -> Option<(Range<usize>, String)> {
    if start == 0 || end > content.len() || start > end {
        return None;
    }
    if !content.is_char_boundary(start) || !content.is_char_boundary(end) {
        return None;
    }
    if !content[..start].ends_with('[') || !content[end..].starts_with("](") {
        return None;
    }
    let rest = &content[end + 2..];
    let close_idx = rest.find(')')?;
    let url = &rest[..close_idx];
    if url.contains('\n') || url.contains('(') || url.contains(']') {
        return None;
    }
    let edit_start = start - 1;
    let edit_end = end + 2 + close_idx + 1;
    Some((edit_start..edit_end, content[start..end].to_string()))
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn block_prefix_len(line: &str) -> usize {
    block_prefix_in(line, 0)
}

/// Prefix length of the line that starts at `line_start` in `content`.
pub fn block_prefix_in(content: &str, line_start: usize) -> usize {
    let line_end_pos = super::text_ops::line_end(content, line_start);
    let line = &content[line_start..line_end_pos];
    let trimmed_start = line.trim_start();
    let indent_len = line.len() - trimmed_start.len();

    if indent_len <= 3 {
        for prefix in ["###### ", "##### ", "#### ", "### ", "## ", "# "] {
            if trimmed_start.starts_with(prefix) {
                return indent_len + prefix.len();
            }
        }
    }

    if let Some(item) = accepted_list_marker(content, line_start) {
        return item.prefix_end();
    }

    if trimmed_start.starts_with("> ") {
        return indent_len + 2;
    }

    0
}

fn cmd_selects_placeholder(cmd: &str) -> bool {
    matches!(
        cmd,
        "bold" | "italic" | "underline" | "strike" | "code" | "link"
    )
}

fn placeholder_prefix_len(cmd: &str) -> usize {
    match cmd {
        "bold" => 2,
        "italic" | "code" => 1,
        "underline" => 3,
        "strike" => 2,
        "link" => 1,
        _ => 0,
    }
}

fn placeholder_len(cmd: &str) -> usize {
    match cmd {
        "bold" => "bold text".len(),
        "italic" => "italic text".len(),
        "underline" => "underlined".len(),
        "strike" => "strikethrough".len(),
        "code" => "code".len(),
        "link" => "link".len(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{move_vertical_visual, EditorPaint, PaintRow};
    use gpui::{point, px, ShapedLine};

    fn find_doc(source: &str) -> DocumentTab {
        DocumentTab::new("t".into(), "t.md".into(), source.to_string(), None, None)
    }

    #[test]
    fn find_walks_every_match_selects_it_and_wraps() {
        let mut doc = find_doc("one cat, two Cat, three cat");
        assert_eq!(doc.find_hit_ranges("cat", false).len(), 3);
        assert_eq!(doc.find_hit_ranges("cat", true).len(), 2);

        let mut index = None;
        let mut seen = Vec::new();
        for _ in 0..4 {
            index = doc.select_match("cat", false, index, true);
            seen.push(index.unwrap());
            assert_eq!(doc.selected_text().to_lowercase(), "cat");
        }
        assert_eq!(seen, vec![0, 1, 2, 0]);

        let back = doc.select_match("cat", false, Some(0), false);
        assert_eq!(back, Some(2));
    }

    #[test]
    fn find_with_no_match_selects_nothing() {
        let mut doc = find_doc("hello");
        assert!(doc.find_hit_ranges("zzz", false).is_empty());
        assert_eq!(doc.select_match("zzz", false, None, true), None);
        assert_eq!(doc.selected_text(), "");
    }

    #[test]
    fn replace_current_replaces_one_and_moves_on() {
        let mut doc = find_doc("cat and cat and cat");
        let idx = doc.select_match("cat", false, None, true);
        let next = doc.replace_current_match("cat", "dog", false, idx);
        assert_eq!(doc.content, "dog and cat and cat");
        assert_eq!(next, Some(0));
        assert_eq!(doc.selected_text(), "cat");
    }

    #[test]
    fn replace_all_respects_case() {
        let mut doc = find_doc("cat Cat cat");
        assert!(doc.replace_every_match("cat", "dog", true));
        assert_eq!(doc.content, "dog Cat dog");
        assert!(!doc.replace_every_match("zzz", "x", true));
    }

    #[test]
    fn find_handles_multibyte_text() {
        let mut doc = find_doc("héllo wörld héllo");
        assert_eq!(doc.find_hit_ranges("HÉLLO", false).len(), 2);
        assert!(doc.replace_every_match("héllo", "x", true));
        assert_eq!(doc.content, "x wörld x");
    }

    #[test]
    fn enter_with_selection_leaving_a_table_cell_does_not_panic() {
        let source = "| a | b |\n| --- | --- |\n| hello | world |";
        let mut doc = DocumentTab::new("t".into(), "t.md".into(), source.to_string(), None, None);
        let start = source.find("hello").unwrap() + 2;
        let end = source.find("world").unwrap() + 2;
        doc.caret.range = start..end;
        doc.smart_newline(false);
        assert!(doc.content.starts_with(source));
    }

    #[test]
    fn outdent_selection_into_next_indent_keeps_char_boundaries() {
        let source = "a\n  ✓ b";
        let mut doc = DocumentTab::new("t".into(), "t.md".into(), source.to_string(), None, None);
        // End one byte into the next line's indent.
        doc.caret.range = 0..3;
        doc.outdent();
        let range = doc.caret.range.clone();
        assert!(doc.content.is_char_boundary(range.start));
        assert!(doc.content.is_char_boundary(range.end));
        assert_eq!(doc.content, "a\n✓ b");
        assert_eq!(range, 0..2);
    }

    #[test]
    fn test_compute_stats() {
        let text = "Hello world from LiteMark.\nSecond line with five words.";
        let stats = compute_stats(text);
        assert_eq!(stats.words, 9);
        assert_eq!(stats.lines, 2);
        assert_eq!(stats.chars, text.len());
        assert_eq!(stats.reading_time_min, 1);
    }

    #[test]
    fn test_undo_redo_stack() {
        let mut doc = DocumentTab::new(
            "tab-1".to_string(),
            "Doc.md".to_string(),
            "Initial".to_string(),
            None,
            None,
        );
        assert!(!doc.is_modified);

        doc.set_content("Version 2".to_string());
        assert!(doc.is_modified);
        assert_eq!(doc.content, "Version 2");

        assert!(doc.undo());
        assert_eq!(doc.content, "Initial");

        assert!(doc.redo());
        assert_eq!(doc.content, "Version 2");

        assert!(!doc.redo());
    }

    #[test]
    fn undo_history_is_diff_sized() {
        let mut doc = DocumentTab::new(
            "tab-1".into(),
            "Doc.md".into(),
            "x".repeat(2_000),
            None,
            None,
        );
        for _ in 0..80 {
            let end = doc.content.len();
            // A trailing space keeps each insert as its own undo step.
            doc.edit(end..end, "y ");
        }
        assert!(doc.undo_stored_bytes() < 2_000 * 80);
        assert_eq!(doc.content.len(), 2_160);
        assert!(doc.content.ends_with("y "));
        assert!(doc.undo());
        assert_eq!(doc.content.len(), 2_158);
        for _ in 0..79 {
            assert!(doc.undo());
        }
        assert_eq!(doc.content.chars().last(), Some('x'));
        assert!(!doc.undo());
    }

    #[test]
    fn a_typed_word_is_one_undo_and_replace_all_is_one_step() {
        let mut doc = DocumentTab::new("tab-1".into(), "Doc.md".into(), String::new(), None, None);
        for ch in ["h", "e", "l", "l", "o"] {
            let end = doc.content.len();
            doc.edit(end..end, ch);
        }
        assert_eq!(doc.content, "hello");
        assert!(doc.undo());
        assert_eq!(doc.content, "");
        assert!(!doc.undo());

        doc.edit(0..0, "hello");
        doc.edit(5..5, " ");
        doc.edit(6..6, "world");
        assert_eq!(doc.content, "hello world");
        assert!(doc.undo());
        assert_eq!(doc.content, "hello ");
        assert!(doc.undo());
        assert_eq!(doc.content, "hello");

        doc.edit(0..5, "cat cat");
        assert!(doc.replace_every_match("cat", "dog", true));
        assert_eq!(doc.content, "dog dog");
        assert!(doc.undo());
        assert_eq!(doc.content, "cat cat");
    }

    #[test]
    fn formatting_uses_the_selection_and_clear_keeps_markers() {
        let mut doc = DocumentTab::new(
            "tab-1".into(),
            "Doc.md".into(),
            "Hello world".into(),
            None,
            None,
        );
        doc.caret.range = 6..11;
        doc.apply_format("bold");
        assert_eq!(doc.content, "Hello **world**");
        assert!(!doc.content.ends_with("**bold text**") || doc.content.contains("**world**"));

        let mut doc = DocumentTab::new("tab-1".into(), "Doc.md".into(), "abc".into(), None, None);
        doc.caret.move_to(1);
        doc.apply_format("bold");
        assert_eq!(doc.content, "**abc**");
        // Toggling bold again strips delimiters
        doc.apply_format("bold");
        assert_eq!(doc.content, "abc");

        let mut empty_doc =
            DocumentTab::new("tab-1".into(), "Doc.md".into(), "".into(), None, None);
        empty_doc.apply_format("bold");
        assert_eq!(empty_doc.content, "**bold text**");

        let source = "- item *keep*\n* bullet `code`";
        let mut doc = DocumentTab::new("tab-1".into(), "Doc.md".into(), source.into(), None, None);
        doc.select_all();
        doc.apply_format("clear");
        assert_eq!(doc.content, "- item keep\n* bullet code");
    }

    #[test]
    fn move_line_up_and_down_and_undo() {
        let mut doc = DocumentTab::new(
            "tab-1".into(),
            "Doc.md".into(),
            "First line\nSecond line\nThird line".into(),
            None,
            None,
        );
        // Cursor on Second line
        doc.caret.move_to(15);
        doc.move_line_up();
        assert_eq!(doc.content, "Second line\nFirst line\nThird line");
        assert!(doc.undo());
        assert_eq!(doc.content, "First line\nSecond line\nThird line");

        // Move down
        doc.caret.move_to(15);
        doc.move_line_down();
        assert_eq!(doc.content, "First line\nThird line\nSecond line");
        assert!(doc.undo());
        assert_eq!(doc.content, "First line\nSecond line\nThird line");

        // Edge case: top line cannot move up
        doc.caret.move_to(2);
        doc.move_line_up();
        assert_eq!(doc.content, "First line\nSecond line\nThird line");
    }

    #[test]
    fn select_line_up_and_down_multiline() {
        let text = "Line one\nLine two\nLine three";
        let mut doc = DocumentTab::new("tab-1".into(), "Doc.md".into(), text.into(), None, None);
        // "Line one" is 0..8, newline at 8
        // "Line two" is 9..17, newline at 17
        // "Line three" is 18..28
        // Place cursor in the middle of Line two (index 13: "Line |two")
        doc.caret.move_to(13);

        // First Ctrl+Shift+Up selects to beginning of Line two (index 9)
        doc.select_line_up();
        assert_eq!(doc.selection_range(), 9..13);
        assert_eq!(doc.selected_text(), "Line");

        // Second Ctrl+Shift+Up extends beyond to beginning of Line one (index 0)
        doc.select_line_up();
        assert_eq!(doc.selection_range(), 0..13);
        assert_eq!(doc.selected_text(), "Line one\nLine");

        // Third Ctrl+Shift+Up stays at 0 (start of doc)
        doc.select_line_up();
        assert_eq!(doc.selection_range(), 0..13);

        // Now test select_line_down:
        doc.caret.move_to(13);
        // First Ctrl+Shift+Down selects to end of Line two (index 17)
        doc.select_line_down();
        assert_eq!(doc.selection_range(), 13..17);
        assert_eq!(doc.selected_text(), " two");

        // Second Ctrl+Shift+Down extends beyond to end of Line three (index 28)
        doc.select_line_down();
        assert_eq!(doc.selection_range(), 13..28);
        assert_eq!(doc.selected_text(), " two\nLine three");

        // Third Ctrl+Shift+Down stays at doc end
        doc.select_line_down();
        assert_eq!(doc.selection_range(), 13..28);
    }

    #[test]
    fn doc_start_and_end_navigation() {
        let mut doc = DocumentTab::new(
            "tab-1".into(),
            "Doc.md".into(),
            "Alpha\nBeta\nGamma".into(),
            None,
            None,
        );
        doc.caret.move_to(7); // In "Beta"
        doc.move_doc_start(false);
        assert_eq!(doc.cursor_offset(), 0);
        assert!(doc.caret.range.is_empty());

        doc.caret.move_to(7);
        doc.move_doc_start(true);
        assert_eq!(doc.selection_range(), 0..7);

        doc.caret.move_to(7);
        doc.move_doc_end(false);
        assert_eq!(doc.cursor_offset(), 16);
        assert!(doc.caret.range.is_empty());

        doc.caret.move_to(7);
        doc.move_doc_end(true);
        assert_eq!(doc.selection_range(), 7..16);
    }

    #[test]
    fn heading_toggle_and_level_switching() {
        let mut doc = DocumentTab::new(
            "tab-1".into(),
            "Doc.md".into(),
            "My heading\nParagraph".into(),
            None,
            None,
        );
        doc.caret.move_to(3);
        // Apply H1
        doc.apply_format("h1");
        assert_eq!(doc.content, "# My heading\nParagraph");

        // Switch to H2 (should not stack ## #)
        doc.apply_format("h2");
        assert_eq!(doc.content, "## My heading\nParagraph");

        // Toggle H2 off
        doc.apply_format("h2");
        assert_eq!(doc.content, "My heading\nParagraph");
    }

    #[test]
    fn inline_formatting_whitespace_trimming() {
        let mut doc = DocumentTab::new(
            "tab-1".into(),
            "Doc.md".into(),
            "leading  RAM usage  trailing".into(),
            None,
            None,
        );
        // Select "  RAM usage  " (includes leading and trailing spaces)
        doc.caret.range = 7..20;
        doc.apply_format("bold");
        assert_eq!(doc.content, "leading  **RAM usage**  trailing");

        // Toggle off
        doc.caret.range = 7..24;
        doc.apply_format("bold");
        assert_eq!(doc.content, "leading  RAM usage  trailing");
    }

    #[test]
    fn stacking_bold_and_italic_formatting() {
        let mut doc = DocumentTab::new(
            "tab-1".into(),
            "Doc.md".into(),
            "hello world".into(),
            None,
            None,
        );
        doc.caret.range = 6..11; // select "world"
        doc.apply_format("bold");
        assert_eq!(doc.content, "hello **world**");

        // Stack italic on top
        doc.apply_format("italic");
        assert_eq!(doc.content, "hello ***world***");

        // Toggle bold off
        doc.apply_format("bold");
        assert_eq!(doc.content, "hello *world*");

        // Toggle italic off
        doc.apply_format("italic");
        assert_eq!(doc.content, "hello world");
    }

    #[test]
    fn pdf_edits_are_ignored() {
        let mut doc = DocumentTab::new(
            "tab-1".into(),
            "scan.pdf".into(),
            "extracted".into(),
            None,
            None,
        );
        assert!(doc.is_read_only());
        doc.edit(0..9, "overwrite");
        assert_eq!(doc.content, "extracted");
    }

    #[test]
    fn find_replace_moves_through_matches() {
        let mut doc = DocumentTab::new(
            "tab-1".into(),
            "Doc.md".into(),
            "cat cat cat".into(),
            None,
            None,
        );
        let first = doc.select_match("cat", true, None, true).unwrap();
        assert_eq!(first, 0);
        assert_eq!(doc.selected_text(), "cat");
        let second = doc.select_match("cat", true, Some(first), true).unwrap();
        assert_eq!(second, 1);
        doc.replace_every_match("cat", "dog", true);
        assert_eq!(doc.content, "dog dog dog");
        assert!(doc.undo());
        assert_eq!(doc.content, "cat cat cat");
    }

    #[test]
    fn undo_redo_caret_stays_on_a_char_boundary() {
        let mut doc = DocumentTab::new("tab-1".into(), "Doc.md".into(), "ab".into(), None, None);
        doc.edit(0..2, "😀ក");
        assert!(doc.undo());
        assert_eq!(doc.content, "ab");
        assert_eq!(doc.cursor_offset(), 2);
        assert!(doc.content.is_char_boundary(doc.cursor_offset()));
        assert!(doc.redo());
        assert_eq!(doc.content, "😀ក");
        assert_eq!(doc.cursor_offset(), "😀ក".len());
        assert!(doc.content.is_char_boundary(doc.cursor_offset()));
        assert!(doc.content.is_char_boundary(doc.selection_range().start));
        assert!(doc.content.is_char_boundary(doc.selection_range().end));

        doc.caret.range = 2..2;
        doc.move_home(false);
        doc.move_end(false);
        doc.apply_format("bold");
        assert!(doc.content.is_char_boundary(doc.cursor_offset()));
        assert!(doc.content.is_char_boundary(doc.selection_range().start));
        assert!(doc.content.is_char_boundary(doc.selection_range().end));
    }

    #[test]
    fn shift_arrow_extends_from_the_anchor_and_clicks_select_word_and_line() {
        let mut doc = DocumentTab::new(
            "tab-1".into(),
            "Doc.md".into(),
            "abcd\nxy".into(),
            None,
            None,
        );
        doc.caret.move_to(3);
        doc.move_left(true);
        doc.move_left(true);
        assert_eq!(doc.selection_range(), 1..3);
        doc.move_right(true);
        assert_eq!(doc.selection_range(), 2..3);
        doc.move_right(true);
        doc.move_right(true);
        assert_eq!(doc.selected_text(), "d");

        doc.caret.move_to(1);
        doc.select_word();
        assert_eq!(doc.selected_text(), "abcd");
        doc.select_line();
        assert_eq!(doc.selected_text(), "abcd\n");
    }

    #[test]
    fn find_previous_leaves_the_current_match() {
        let mut doc = DocumentTab::new(
            "tab-1".into(),
            "Doc.md".into(),
            "cat cat cat".into(),
            None,
            None,
        );
        let first = doc.select_match("cat", true, None, true).unwrap();
        assert_eq!(first, 0);
        assert_eq!(doc.cursor_offset(), 3);
        let previous = doc.select_match("cat", true, Some(first), false).unwrap();
        assert_eq!(previous, 2);
        assert_eq!(doc.selected_text(), "cat");
        assert_ne!(doc.selection_range().start, 0);
    }

    #[test]
    fn word_navigation_and_word_deletion() {
        let mut doc = DocumentTab::new(
            "tab-1".into(),
            "Doc.md".into(),
            "hello world test".into(),
            None,
            None,
        );
        doc.caret.move_to(0);
        doc.move_word_right(false);
        assert_eq!(doc.cursor_offset(), 5);
        doc.move_word_right(true);
        assert_eq!(doc.selection_range(), 5..11);

        doc.caret.move_to(16);
        doc.delete_word_left();
        assert_eq!(doc.content, "hello world ");
        doc.delete_word_left();
        assert_eq!(doc.content, "hello ");

        doc.select_all();
        assert_eq!(doc.selection_range(), 0..6);
    }

    #[test]
    fn test_smart_newline_bullet_and_task_and_number() {
        // 1. Bullet continuation and exit
        let mut doc = DocumentTab::new("1".into(), "test.md".into(), "- Item 1".into(), None, None);
        doc.caret.move_to(8);
        doc.smart_newline(false);
        assert_eq!(doc.content, "- Item 1\n- ");
        assert_eq!(doc.cursor_offset(), 11);

        // Press Enter again on empty bullet -> exits list
        doc.smart_newline(false);
        assert_eq!(doc.content, "- Item 1\n");
        assert_eq!(doc.cursor_offset(), 9);

        // 2. Numbered list continuation and exit
        let mut doc2 =
            DocumentTab::new("2".into(), "test.md".into(), "1. First".into(), None, None);
        doc2.caret.move_to(8);
        doc2.smart_newline(false);
        assert_eq!(doc2.content, "1. First\n2. ");

        // Press Enter on empty "2. " -> exits list
        doc2.smart_newline(false);
        assert_eq!(doc2.content, "1. First\n");

        // 3. Task list continuation and exit
        let mut doc3 = DocumentTab::new(
            "3".into(),
            "test.md".into(),
            "- [ ] Todo".into(),
            None,
            None,
        );
        doc3.caret.move_to(10);
        doc3.smart_newline(false);
        assert_eq!(doc3.content, "- [ ] Todo\n- [ ] ");

        // Press Enter on empty task -> exits task list
        doc3.smart_newline(false);
        assert_eq!(doc3.content, "- [ ] Todo\n");

        // 4. Shift+Enter -> plain newline
        let mut doc4 = DocumentTab::new("4".into(), "test.md".into(), "- Item".into(), None, None);
        doc4.caret.move_to(6);
        doc4.smart_newline(true);
        assert_eq!(doc4.content, "- Item\n");

        // 5. Heading -> plain newline (does not duplicate #)
        let mut doc5 = DocumentTab::new("5".into(), "test.md".into(), "# Title".into(), None, None);
        doc5.caret.move_to(7);
        doc5.smart_newline(false);
        assert_eq!(doc5.content, "# Title\n");
    }

    #[test]
    fn test_smart_backspace_boundaries() {
        // 1. Heading demotion
        let mut doc =
            DocumentTab::new("1".into(), "test.md".into(), "# Heading".into(), None, None);
        doc.caret.move_to(2); // right after "# "
        doc.smart_backspace();
        assert_eq!(doc.content, "Heading");
        assert_eq!(doc.cursor_offset(), 0);

        let mut doc_h2 = DocumentTab::new(
            "2".into(),
            "test.md".into(),
            "## Heading".into(),
            None,
            None,
        );
        doc_h2.caret.move_to(3); // right after "## "
        doc_h2.smart_backspace();
        assert_eq!(doc_h2.content, "# Heading");

        // 2. Bullet list prefix removal
        let mut doc_bullet =
            DocumentTab::new("3".into(), "test.md".into(), "- Item".into(), None, None);
        doc_bullet.caret.move_to(2); // right after "- "
        doc_bullet.smart_backspace();
        assert_eq!(doc_bullet.content, "Item");

        // 3. Task list prefix demotion
        let mut doc_task = DocumentTab::new(
            "4".into(),
            "test.md".into(),
            "- [ ] Task".into(),
            None,
            None,
        );
        doc_task.caret.move_to(6); // right after "- [ ] "
        doc_task.smart_backspace();
        assert_eq!(doc_task.content, "- Task");

        // 4. Auto-paired brackets deletion
        let mut doc_pair = DocumentTab::new("5".into(), "test.md".into(), "()".into(), None, None);
        doc_pair.caret.move_to(1); // between ( and )
        doc_pair.smart_backspace();
        assert_eq!(doc_pair.content, "");

        // 5. Indented list item outdent on backspace
        let mut doc_indent = DocumentTab::new(
            "6".into(),
            "test.md".into(),
            "  - Subitem".into(),
            None,
            None,
        );
        doc_indent.caret.move_to(4); // right after "  - "
        doc_indent.smart_backspace();
        assert_eq!(doc_indent.content, "- Subitem");
    }

    #[test]
    fn test_indent_and_outdent() {
        let mut doc = DocumentTab::new(
            "1".into(),
            "test.md".into(),
            "- Item 1\n- Item 2".into(),
            None,
            None,
        );
        doc.caret.move_to(0);
        doc.indent();
        assert_eq!(doc.content, "  - Item 1\n- Item 2");

        doc.outdent();
        assert_eq!(doc.content, "- Item 1\n- Item 2");

        // Multi-line selection indent and outdent
        doc.caret.range = 0..doc.content.len();
        doc.indent();
        assert_eq!(doc.content, "  - Item 1\n  - Item 2");

        doc.outdent();
        assert_eq!(doc.content, "- Item 1\n- Item 2");
    }

    #[test]
    fn test_block_prefix_len() {
        assert_eq!(block_prefix_len("# Title"), 2);
        assert_eq!(block_prefix_len("### Title"), 4);
        assert_eq!(block_prefix_len("- Item"), 2);
        assert_eq!(block_prefix_len("  - Sub"), 4);
        assert_eq!(block_prefix_len("- [ ] Task"), 6);
        assert_eq!(block_prefix_len("1. One"), 3);
        assert_eq!(block_prefix_len("10. Ten"), 4);
        assert_eq!(block_prefix_len("> Quote"), 2);
        assert_eq!(block_prefix_len("Plain paragraph"), 0);
    }

    #[test]
    fn test_universal_reversible_inline_and_block_toggling() {
        // 1. Inline toggling with cursor inside word (no selection needed)
        for (cmd, expected_wrapped) in [
            ("bold", "hello **world**"),
            ("italic", "hello *world*"),
            ("underline", "hello <u>world</u>"),
            ("strike", "hello ~~world~~"),
            ("code", "hello `world`"),
            ("link", "hello [world](https://)"),
        ] {
            let mut doc = DocumentTab::new(
                "t".into(),
                "test.md".into(),
                "hello world".into(),
                None,
                None,
            );
            doc.place(8, false); // inside "world"
            doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
            assert_eq!(doc.content, expected_wrapped, "apply {cmd} failed");
            // Pressing the exact same command again must cleanly undo/unwrap it!
            doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
            assert_eq!(
                doc.content, "hello world",
                "second press of {cmd} did not unwrap"
            );
        }

        // 2. Block toggling ON and OFF idempotently, plus switching between block types
        for (cmd, expected_line) in [
            ("h1", "# My line"),
            ("h2", "## My line"),
            ("h3", "### My line"),
            ("ul", "- My line"),
            ("ol", "1. My line"),
            ("checklist", "- [ ] My line"),
            ("quote", "> My line"),
            ("codeblock", "```\nMy line\n```"),
        ] {
            let mut doc =
                DocumentTab::new("b".into(), "test.md".into(), "My line".into(), None, None);
            doc.place(3, false);
            doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
            assert_eq!(doc.content, expected_line, "apply block {cmd} failed");
            // Pressing a second time toggles off back to "My line"
            doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
            assert_eq!(
                doc.content, "My line",
                "second press of block {cmd} did not toggle off"
            );
        }

        // 3. Switching directly from Bullet List to Numbered List to Checklist without stacking
        let mut doc =
            DocumentTab::new("s".into(), "test.md".into(), "Task item".into(), None, None);
        doc.place(2, false);
        doc.apply_format_in_mode("ul", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "- Task item");
        assert!(doc.active_formats.ul);
        doc.apply_format_in_mode("ol", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "1. Task item");
        assert!(doc.active_formats.ol);
        assert!(!doc.active_formats.ul);
        doc.apply_format_in_mode("checklist", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "- [ ] Task item");
        assert!(doc.active_formats.checklist);
        assert!(!doc.active_formats.ol);
        doc.apply_format_in_mode("checklist", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "Task item");
        assert!(!doc.active_formats.checklist);
    }

    #[test]
    fn test_wysiwyg_pending_inline_formats_and_typing() {
        let mut doc = DocumentTab::new("p".into(), "test.md".into(), "hello ".into(), None, None);
        doc.place(6, false); // at empty end after space

        // Press Ctrl+B once -> arms pending bold, does not insert placeholder text
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "hello ");
        assert!(doc.pending_formats.bold);
        assert!(doc.active_formats.bold);

        // Press Ctrl+B again -> disarms pending bold
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "hello ");
        assert!(!doc.pending_formats.bold);
        assert!(!doc.active_formats.bold);

        // Re-arm Bold + Italic, then type "A" -> wraps in "***A***" and places cursor after A
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
        assert!(doc.active_formats.bold && doc.active_formats.italic);
        let range = doc.selection_range();
        doc.insert_text_in_mode(range, "A", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "hello ***A***");
        assert_eq!(doc.cursor_offset(), 10); // right after 'A', inside closing '***'
        assert!(doc.active_formats.bold && doc.active_formats.italic);
    }

    #[test]
    fn test_underline_toggle_selection_and_pending_formats() {
        let mut doc = DocumentTab::new(
            "u1".into(),
            "test.md".into(),
            "selected text to underline".into(),
            None,
            None,
        );
        // Select "text" (indices 9..13)
        doc.place(9, false);
        doc.place(13, true);
        doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "selected <u>text</u> to underline");
        assert!(doc.active_formats.underline);

        // Toggle again on selection should unwrap
        doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "selected text to underline");
        assert!(!doc.active_formats.underline);

        // Empty cursor in WYSIWYG mode toggles pending_formats
        let mut doc2 = DocumentTab::new("u2".into(), "test.md".into(), "".into(), None, None);
        doc2.place(0, false);
        doc2.apply_format_in_mode("underline", ViewMode::Wysiwyg);
        assert!(doc2.pending_formats.underline);
        assert!(doc2.active_formats.underline);

        // Disarm pending underline
        doc2.apply_format_in_mode("underline", ViewMode::Wysiwyg);
        assert!(!doc2.pending_formats.underline);
        assert!(!doc2.active_formats.underline);

        // Re-arm and type text -> wraps in <u>...</u>
        doc2.apply_format_in_mode("underline", ViewMode::Wysiwyg);
        let range = doc2.selection_range();
        doc2.insert_text_in_mode(range, "hello", ViewMode::Wysiwyg);
        assert_eq!(doc2.content, "<u>hello</u>");
        assert!(!doc2.pending_formats.underline);
    }

    /// Type `text` one character at a time at the current caret, as the
    /// editor does for real keystrokes.
    fn type_keystrokes(doc: &mut DocumentTab, text: &str) {
        for ch in text.chars() {
            let range = doc.selection_range();
            doc.insert_text_in_mode(range, &ch.to_string(), ViewMode::Wysiwyg);
        }
    }

    fn pending_flag(doc: &DocumentTab, cmd: &str) -> bool {
        match cmd {
            "bold" => doc.pending_formats.bold,
            "italic" => doc.pending_formats.italic,
            "strike" => doc.pending_formats.strike,
            _ => false,
        }
    }

    fn styled_flag(doc: &mut DocumentTab, cmd: &str) -> bool {
        let kind = match cmd {
            "bold" => InlineTagKind::Bold,
            "italic" => InlineTagKind::Italic,
            "strike" => InlineTagKind::Strike,
            _ => InlineTagKind::Link,
        };
        doc.parsed().inline_pairs.iter().any(|p| p.kind == kind)
    }

    /// Ctrl+B/I/U/Shift+S/` with no selection, type a word, then toggle the
    /// same format off: the span must stay formatted and the caret must move
    /// past the closing marker so the following words are plain.
    #[test]
    fn toggle_off_after_typing_keeps_the_span_and_exits_it() {
        for (cmd, prefix, suffix) in [
            ("bold", "**", "**"),
            ("italic", "*", "*"),
            ("underline", "<u>", "</u>"),
            ("strike", "~~", "~~"),
            ("code", "`", "`"),
        ] {
            let mut doc =
                DocumentTab::new("b".into(), "test.md".into(), "This is ".into(), None, None);
            doc.place(8, false);
            doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
            type_keystrokes(&mut doc, "very");
            let wrapped = format!("This is {prefix}very{suffix}");
            assert_eq!(doc.content, wrapped, "{cmd}: typing did not wrap");

            // Second toggle: exit the span, do not unwrap it
            doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
            assert_eq!(
                doc.content, wrapped,
                "{cmd}: second toggle removed the formatting"
            );
            assert!(!doc.content.contains('\u{200B}'));
            assert_eq!(
                doc.cursor_offset(),
                wrapped.len(),
                "{cmd}: caret did not move past the closing marker"
            );
            assert!(
                !doc.has_pending_inline_formats(),
                "{cmd}: pending formats stayed armed after leaving the span"
            );

            // Further typing is plain
            type_keystrokes(&mut doc, " important");
            assert_eq!(
                doc.content,
                format!("This is {prefix}very{suffix} important"),
                "{cmd}: text after leaving the span is not plain"
            );
        }
    }

    /// A space typed at the end of a `*`/`**`/`~~` span must not land right
    /// before the closer (a closer cannot follow whitespace in Markdown). It
    /// goes after the closing marker; the next letter pulls it back inside
    /// and extends the span, while stopping after the space keeps it outside.
    #[test]
    fn space_at_span_end_types_outside_the_closer() {
        for (cmd, open, close) in [
            ("bold", "**", "**"),
            ("italic", "*", "*"),
            ("strike", "~~", "~~"),
        ] {
            let mut doc = DocumentTab::new("s".into(), "test.md".into(), String::new(), None, None);
            doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
            type_keystrokes(&mut doc, "read");
            type_keystrokes(&mut doc, " ");
            assert_eq!(
                doc.content,
                format!("{open}read{close} "),
                "{cmd}: space did not move outside the closing marker"
            );
            assert!(!doc.content.contains('\u{200B}'));
            assert!(
                pending_flag(&doc, cmd),
                "{cmd}: format is not armed after the space"
            );
            let parsed = doc.parsed();
            let line = parsed.line_at_offset(0).unwrap();
            assert_eq!(
                line.display_text, "read ",
                "{cmd}: raw markers leaked into the display"
            );

            // The next letter extends the original span, keeping one span
            type_keystrokes(&mut doc, "me");
            assert_eq!(
                doc.content,
                format!("{open}read me{close}"),
                "{cmd}: continued typing did not extend the span"
            );
            assert!(
                styled_flag(&mut doc, cmd),
                "{cmd}: continued typing is not rendered formatted"
            );
        }
    }

    /// Enter with the caret inside a bold span closes it before the newline
    /// and reopens it on the new line; at the end of the span it leaves the
    /// format armed so the next line continues bold.
    #[test]
    fn enter_inside_bold_span_closes_and_reopens_it() {
        let mut doc = DocumentTab::new("e".into(), "test.md".into(), String::new(), None, None);
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        let range = doc.selection_range();
        doc.insert_text_in_mode(range, "first half", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "**first half**");

        // Caret between "first" and " half": split the span around the break
        doc.place(7, false);
        doc.smart_newline(false);
        assert_eq!(
            doc.content, "**first**\n **half**",
            "Enter did not close and reopen the bold span"
        );
        assert_eq!(
            doc.cursor_offset(),
            13,
            "caret is not at the start of the reopened span"
        );
        assert!(
            doc.active_formats.bold,
            "the reopened span is not bold at the caret"
        );

        // Typing at the end of the reopened span stays bold
        doc.place(doc.content.len() - 2, false);
        type_keystrokes(&mut doc, "!");
        assert_eq!(doc.content, "**first**\n **half!**");

        // Caret at the end of the span: close it, keep bold armed, the next
        // letter reopens it on the new line
        let mut doc2 = DocumentTab::new("e2".into(), "test.md".into(), String::new(), None, None);
        doc2.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        let range = doc2.selection_range();
        doc2.insert_text_in_mode(range, "first half", ViewMode::Wysiwyg);
        doc2.smart_newline(false);
        assert_eq!(doc2.content, "**first half**\n");
        assert!(doc2.pending_formats.bold);
        type_keystrokes(&mut doc2, "second");
        assert_eq!(doc2.content, "**first half**\n**second**");
    }

    /// Trailing spaces of an ATX heading stay in the WYSIWYG display and the
    /// caret mapping, so typing a space at the end of a heading does not
    /// vanish until the next letter.
    #[test]
    fn heading_trailing_space_stays_in_display_and_caret() {
        let mut doc = DocumentTab::new("h".into(), "test.md".into(), "# my".into(), None, None);
        doc.place(4, false);
        type_keystrokes(&mut doc, " ");
        assert_eq!(doc.content, "# my ");
        let parsed = doc.parsed();
        let line = parsed.line_at_offset(0).unwrap();
        assert_eq!(line.display_text, "my ", "trailing space left the display");
        assert_eq!(doc.cursor_offset(), 5, "caret fell back before the space");

        type_keystrokes(&mut doc, "note");
        assert_eq!(doc.content, "# my note");
    }

    /// Unformatting part of a larger span splits it cleanly instead of
    /// leaving stray markers; for underline it must not nest a second <u>.
    #[test]
    fn partial_unformat_splits_the_span() {
        for (cmd, open, close) in [
            ("bold", "**", "**"),
            ("italic", "*", "*"),
            ("underline", "<u>", "</u>"),
            ("strike", "~~", "~~"),
            ("code", "`", "`"),
        ] {
            let wrapped = format!("{open}Hello{close}");

            // Start of the span: "He" becomes plain, "llo" stays formatted
            let mut doc =
                DocumentTab::new("p".into(), "test.md".into(), wrapped.clone(), None, None);
            doc.caret.range = open.len()..open.len() + 2;
            doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
            assert_eq!(
                doc.content,
                format!("He{open}llo{close}"),
                "{cmd}: unformatting the start left stray markers"
            );

            // End of the span: "llo" becomes plain, "He" stays formatted
            let mut doc =
                DocumentTab::new("p".into(), "test.md".into(), wrapped.clone(), None, None);
            doc.caret.range = open.len() + 2..open.len() + 5;
            doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
            assert_eq!(
                doc.content,
                format!("{open}He{close}llo"),
                "{cmd}: unformatting the end left stray markers"
            );

            // Middle of the span: "ell" becomes plain, both halves formatted
            let mut doc = DocumentTab::new("p".into(), "test.md".into(), wrapped, None, None);
            doc.caret.range = open.len() + 1..open.len() + 4;
            doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
            assert_eq!(
                doc.content,
                format!("{open}H{close}ell{open}o{close}"),
                "{cmd}: unformatting the middle left stray markers"
            );
        }
    }

    fn flags_for_word(content: &str, word: &str) -> (bool, bool, bool, bool) {
        let parsed = parse_document(content);
        let segs: Vec<_> = parsed.lines[0]
            .styled_segments
            .iter()
            .filter(|s| {
                let text = &content[s.source.clone()];
                text == word || text.split_whitespace().any(|w| w == word)
            })
            .collect();
        assert!(
            !segs.is_empty(),
            "word {word:?} missing in {content:?} segs={:?}",
            parsed.lines[0]
                .styled_segments
                .iter()
                .map(|s| &content[s.source.clone()])
                .collect::<Vec<_>>()
        );
        let s = segs[0];
        (s.bold, s.italic, s.underline, s.strike)
    }

    fn assert_clean_display(content: &str, expected: &str) {
        assert!(
            !content.contains('\u{200B}'),
            "zero-width space in {content:?}"
        );
        assert!(
            !content.contains("****"),
            "adjacent bold runs in {content:?}"
        );
        let parsed = parse_document(content);
        assert_eq!(
            parsed.lines[0].display_text, expected,
            "raw markers in display of {content:?}"
        );
        for ch in ['*', '~', '_'] {
            assert!(
                !parsed.lines[0].display_text.contains(ch),
                "literal {ch:?} in display {:?} of {content:?}",
                parsed.lines[0].display_text
            );
        }
    }

    fn select_once(doc: &mut DocumentTab, word: &str) {
        let start = doc.content.find(word).expect(word);
        doc.caret.range = start..(start + word.len());
    }

    /// Unformatting a word whose selection ends on the space before the next
    /// word must move that space outside the leftover markers. Re-applying
    /// the format must merge back into the neighbor instead of `**aa**** bb**`.
    #[test]
    fn unformat_before_space_moves_space_outside_markers() {
        for (cmd, wrapped) in [
            ("bold", "**aa bb cc**"),
            ("italic", "*aa bb cc*"),
            ("strike", "~~aa bb cc~~"),
        ] {
            let mut doc = DocumentTab::new(
                "e1".into(),
                "test.md".into(),
                wrapped.to_string(),
                None,
                None,
            );
            select_once(&mut doc, "aa");
            doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
            assert_clean_display(&doc.content, "aa bb cc");
            let aa = flags_for_word(&doc.content, "aa");
            let bb = flags_for_word(&doc.content, "bb");
            let cc = flags_for_word(&doc.content, "cc");
            assert_eq!(
                aa,
                (false, false, false, false),
                "{cmd} left aa styled in {}",
                doc.content
            );
            let expect = match cmd {
                "bold" => (true, false, false, false),
                "italic" => (false, true, false, false),
                "strike" => (false, false, false, true),
                _ => unreachable!(),
            };
            assert_eq!(bb, expect, "{cmd} dropped bb in {}", doc.content);
            assert_eq!(cc, expect, "{cmd} dropped cc in {}", doc.content);
            assert!(
                !doc.content.contains("aa** ")
                    && !doc.content.contains("aa* ")
                    && !doc.content.contains("aa~~ "),
                "{cmd}: space stayed inside the leftover opener: {}",
                doc.content
            );

            // Turning the format back on merges with the neighboring span.
            select_once(&mut doc, "aa");
            doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
            assert_clean_display(&doc.content, "aa bb cc");
            assert_eq!(
                flags_for_word(&doc.content, "aa"),
                expect,
                "{cmd} re-apply: {}",
                doc.content
            );
            assert_eq!(
                flags_for_word(&doc.content, "bb"),
                expect,
                "{cmd} re-apply: {}",
                doc.content
            );
            assert!(
                !doc.content.contains("****") && !doc.content.contains("~~~~"),
                "{cmd} re-apply stacked markers: {}",
                doc.content
            );
        }

        // Same flanking split when italic is already on (bold+italic span).
        let mut doc = DocumentTab::new(
            "e1b".into(),
            "test.md".into(),
            "***aa bb cc***".into(),
            None,
            None,
        );
        select_once(&mut doc, "aa");
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        assert_clean_display(&doc.content, "aa bb cc");
        assert_eq!(
            flags_for_word(&doc.content, "aa"),
            (false, true, false, false)
        );
        assert_eq!(
            flags_for_word(&doc.content, "bb"),
            (true, true, false, false)
        );
        assert_eq!(
            flags_for_word(&doc.content, "cc"),
            (true, true, false, false)
        );

        // Underline tags do not depend on spaces; the leftover words stay
        // underlined while the flanking-sensitive markers move off the gap.
        let mut doc = DocumentTab::new(
            "e1c".into(),
            "test.md".into(),
            "<u>**aa bb cc**</u>".into(),
            None,
            None,
        );
        select_once(&mut doc, "aa");
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        assert_clean_display(&doc.content, "aa bb cc");
        assert_eq!(
            flags_for_word(&doc.content, "aa"),
            (false, false, true, false)
        );
        assert_eq!(
            flags_for_word(&doc.content, "bb"),
            (true, false, true, false)
        );
        assert_eq!(
            flags_for_word(&doc.content, "cc"),
            (true, false, true, false)
        );
    }

    /// Removing bold or italic from the middle or end of a bold+italic span
    /// must not write `***aa **bb cc*` runs that merge and stop pairing.
    #[test]
    fn stacked_unformat_at_word_edge_does_not_merge_stars() {
        // End: drop italic from "bb cc"
        let mut doc = DocumentTab::new(
            "e2".into(),
            "test.md".into(),
            "***aa bb cc***".into(),
            None,
            None,
        );
        doc.caret.range = 6..11; // "bb cc"
        doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
        assert_clean_display(&doc.content, "aa bb cc");
        assert_eq!(
            flags_for_word(&doc.content, "aa"),
            (true, true, false, false)
        );
        assert_eq!(
            flags_for_word(&doc.content, "bb"),
            (true, false, false, false)
        );
        assert_eq!(
            flags_for_word(&doc.content, "cc"),
            (true, false, false, false)
        );
        assert!(
            !doc.content.contains("***aa **") && !doc.content.contains("**aa ***"),
            "end unformat merged delimiter runs: {}",
            doc.content
        );

        // End: drop bold from "bb cc"
        let mut doc = DocumentTab::new(
            "e2b".into(),
            "test.md".into(),
            "***aa bb cc***".into(),
            None,
            None,
        );
        doc.caret.range = 6..11;
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        assert_clean_display(&doc.content, "aa bb cc");
        assert_eq!(
            flags_for_word(&doc.content, "aa"),
            (true, true, false, false)
        );
        assert_eq!(
            flags_for_word(&doc.content, "bb"),
            (false, true, false, false)
        );
        assert_eq!(
            flags_for_word(&doc.content, "cc"),
            (false, true, false, false)
        );

        // Middle: drop italic from "bb"
        let mut doc = DocumentTab::new(
            "e2c".into(),
            "test.md".into(),
            "***aa bb cc***".into(),
            None,
            None,
        );
        select_once(&mut doc, "bb");
        doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
        assert_clean_display(&doc.content, "aa bb cc");
        assert_eq!(
            flags_for_word(&doc.content, "aa"),
            (true, true, false, false)
        );
        assert_eq!(
            flags_for_word(&doc.content, "bb"),
            (true, false, false, false)
        );
        assert_eq!(
            flags_for_word(&doc.content, "cc"),
            (true, true, false, false)
        );
        assert!(
            !doc.content.contains("_**aa **_") && !doc.content.contains("**aa **_"),
            "middle italic unformat wrote colliding markers: {}",
            doc.content
        );

        // Same middle split with underline also on.
        let mut doc = DocumentTab::new(
            "e2d".into(),
            "test.md".into(),
            "<u>***aa bb cc***</u>".into(),
            None,
            None,
        );
        select_once(&mut doc, "bb");
        doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
        assert_clean_display(&doc.content, "aa bb cc");
        assert_eq!(
            flags_for_word(&doc.content, "aa"),
            (true, true, true, false)
        );
        assert_eq!(
            flags_for_word(&doc.content, "bb"),
            (true, false, true, false)
        );
        assert_eq!(
            flags_for_word(&doc.content, "cc"),
            (true, true, true, false)
        );
    }

    /// Stacked bold+italic typing (`***`, from Ctrl+B then Ctrl+I) must keep
    /// one span across spaces and both toggle-offs, and the text typed after
    /// leaving the stack must be plain. Same for bold+strike (`**~~`).
    #[test]
    fn stacked_typing_keeps_one_span_and_toggles_off_cleanly() {
        let mut doc = DocumentTab::new("x".into(), "test.md".into(), "Go ".into(), None, None);
        doc.place(3, false);
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "now");
        type_keystrokes(&mut doc, " ");
        assert_eq!(
            doc.content, "Go ***now*** ",
            "space did not move outside the stacked closing run"
        );
        let parsed = doc.parsed();
        let line = parsed.line_at_offset(0).unwrap();
        assert_eq!(
            line.display_text, "Go now ",
            "raw markers leaked into the display"
        );

        type_keystrokes(&mut doc, "please");
        assert_eq!(
            doc.content, "Go ***now please***",
            "stacked typing split the span"
        );

        doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
        assert_eq!(
            doc.content, "Go ***now please***",
            "Ctrl+I unwrapped the stacked span"
        );
        assert_eq!(
            doc.cursor_offset(),
            19,
            "Ctrl+I did not exit past the stacked closing run"
        );
        assert!(
            doc.pending_formats.bold,
            "bold did not stay armed after leaving italic"
        );

        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "Go ***now please***");
        assert!(
            !doc.has_pending_inline_formats(),
            "bold is still armed after the second toggle"
        );
        type_keystrokes(&mut doc, " ok");
        assert_eq!(doc.content, "Go ***now please*** ok");

        // Strike outer, bold inner: same stacked behavior
        let mut doc = DocumentTab::new("y".into(), "test.md".into(), String::new(), None, None);
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        doc.apply_format_in_mode("strike", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "was");
        type_keystrokes(&mut doc, " ");
        assert_eq!(doc.content, "~~**was**~~ ");
        type_keystrokes(&mut doc, "here");
        assert_eq!(doc.content, "~~**was here**~~");
        doc.apply_format_in_mode("strike", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "~~**was here**~~");
        assert!(doc.pending_formats.bold);
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        assert!(!doc.has_pending_inline_formats());
        type_keystrokes(&mut doc, " ok");
        assert_eq!(doc.content, "~~**was here**~~ ok");
    }

    /// Typing into a newly created formatted span is one undo step: a single
    /// undo removes the whole span (and redo brings it back), for plain and
    /// stacked spans alike.
    #[test]
    fn undo_removes_a_newly_typed_span_in_one_step() {
        let mut doc = DocumentTab::new("u".into(), "test.md".into(), "Hello ".into(), None, None);
        doc.place(6, false);
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "world");
        assert_eq!(doc.content, "Hello **world**");
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        assert!(doc.undo(), "undo failed");
        assert_eq!(
            doc.content, "Hello ",
            "one undo did not remove the whole span"
        );
        assert!(doc.redo(), "redo failed");
        assert_eq!(doc.content, "Hello **world**");

        // Stacked span with a space inside: still one undo step
        let mut doc = DocumentTab::new("v".into(), "test.md".into(), "Go ".into(), None, None);
        doc.place(3, false);
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "now please");
        doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        assert!(doc.undo(), "undo failed");
        assert_eq!(
            doc.content, "Go ",
            "one undo did not remove the stacked span"
        );
    }

    /// Extra spaces typed at the end of an emphasis span keep stacking
    /// outside the closer: the broken `**a  **` form never appears, the
    /// display stays formatted, and the next letter is still bold.
    #[test]
    fn extra_spaces_at_span_end_stay_outside_the_closer() {
        for (cmd, open, close) in [
            ("bold", "**", "**"),
            ("italic", "*", "*"),
            ("strike", "~~", "~~"),
        ] {
            let mut doc = DocumentTab::new("w".into(), "test.md".into(), String::new(), None, None);
            doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
            type_keystrokes(&mut doc, "a");
            type_keystrokes(&mut doc, " ");
            type_keystrokes(&mut doc, " ");
            assert_eq!(
                doc.content,
                format!("{open}a{close}  "),
                "{cmd}: the second space reentered the span"
            );
            let parsed = doc.parsed();
            let line = parsed.line_at_offset(0).unwrap();
            assert_eq!(
                line.display_text, "a  ",
                "{cmd}: raw markers leaked into the display"
            );
            assert!(
                pending_flag(&doc, cmd),
                "{cmd}: format is not armed after the spaces"
            );

            // the next letter is still formatted
            type_keystrokes(&mut doc, "b");
            assert_eq!(
                doc.content,
                format!("{open}a{close}  {open}b{close}"),
                "{cmd}: the letter after the spaces is not formatted"
            );
            assert!(styled_flag(&mut doc, cmd));
        }
    }

    /// Ctrl+B with no selection wraps the current word only when the caret
    /// is strictly inside it (letters on both sides). At the end of a word
    /// just typed the format is armed for the letters typed next instead, so
    /// `un{Ctrl+B}believ{Ctrl+B}able` saves as `un**believ**able`.
    #[test]
    fn toggle_at_word_end_arms_instead_of_wrapping_the_word() {
        let mut doc = DocumentTab::new("t".into(), "test.md".into(), String::new(), None, None);
        type_keystrokes(&mut doc, "un");
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "un", "Ctrl+B wrapped the word the caret ends");
        assert!(
            doc.pending_formats.bold,
            "bold is not armed at the word end"
        );

        type_keystrokes(&mut doc, "believ");
        assert_eq!(doc.content, "un**believ**");
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "un**believ**");
        type_keystrokes(&mut doc, "able");
        assert_eq!(doc.content, "un**believ**able");

        // A caret strictly inside a word still wraps the whole word
        let mut doc = DocumentTab::new("t2".into(), "test.md".into(), "Hello".into(), None, None);
        doc.place(3, false);
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "**Hello**");

        // A caret at the start of a word arms the format without wrapping
        let mut doc = DocumentTab::new("t3".into(), "test.md".into(), " world".into(), None, None);
        doc.place(1, false);
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        assert_eq!(doc.content, " world");
        assert!(doc.pending_formats.bold);
    }

    /// Inline code is always the innermost format with its backticks adjacent
    /// to the text, and no other delimiters are ever emitted inside a code
    /// span: when a non-code format turns off mid-typing, the code span
    /// closes, the format closes, and the code span reopens
    /// (`**`hi`**`yo`). Covers every pair and triple containing code.
    #[test]
    fn code_stays_innermost_when_a_non_code_format_toggles_off() {
        // (armed formats, format toggled off mid-typing, expected content,
        //  flags of "hi", flags of "yo" as (bold, italic, underline, strike, code))
        type CodeCombo = (
            &'static [&'static str],
            &'static str,
            &'static str,
            (bool, bool, bool, bool, bool),
            (bool, bool, bool, bool, bool),
        );
        let combos: &[CodeCombo] = &[
            (
                &["bold", "code"],
                "bold",
                "**`hi`**`yo`",
                (true, false, false, false, true),
                (false, false, false, false, true),
            ),
            (
                &["italic", "code"],
                "italic",
                "*`hi`*`yo`",
                (false, true, false, false, true),
                (false, false, false, false, true),
            ),
            (
                &["underline", "code"],
                "underline",
                "<u>`hi`</u>`yo`",
                (false, false, true, false, true),
                (false, false, false, false, true),
            ),
            (
                &["strike", "code"],
                "strike",
                "~~`hi`~~`yo`",
                (false, false, false, true, true),
                (false, false, false, false, true),
            ),
            (
                &["bold", "italic", "code"],
                "bold",
                "***`hi`**`yo`*",
                (true, true, false, false, true),
                (false, true, false, false, true),
            ),
            (
                &["bold", "underline", "code"],
                "bold",
                "<u>**`hi`**`yo`</u>",
                (true, false, true, false, true),
                (false, false, true, false, true),
            ),
            (
                &["bold", "strike", "code"],
                "bold",
                "~~**`hi`**`yo`~~",
                (true, false, false, true, true),
                (false, false, false, true, true),
            ),
            (
                &["italic", "underline", "code"],
                "italic",
                "<u>*`hi`*`yo`</u>",
                (false, true, true, false, true),
                (false, false, true, false, true),
            ),
            (
                &["italic", "strike", "code"],
                "italic",
                "~~*`hi`*`yo`~~",
                (false, true, false, true, true),
                (false, false, false, true, true),
            ),
            (
                &["underline", "strike", "code"],
                "underline",
                "~~<u>`hi`</u>`yo`~~",
                (false, false, true, true, true),
                (false, false, false, true, true),
            ),
        ];

        for (formats, toggle_off, expected, hi_flags, yo_flags) in combos {
            let mut doc = DocumentTab::new("c".into(), "test.md".into(), String::new(), None, None);
            type_keystrokes(&mut doc, "x ");
            for format in *formats {
                doc.apply_format_in_mode(format, ViewMode::Wysiwyg);
            }
            type_keystrokes(&mut doc, "hi");
            let mut flags = FormatFlags::default();
            for format in *formats {
                match *format {
                    "bold" => flags.bold = true,
                    "italic" => flags.italic = true,
                    "underline" => flags.underline = true,
                    "strike" => flags.strike = true,
                    "code" => flags.code = true,
                    _ => {}
                }
            }
            let (open, close) = wrap_delimiters(&flags);
            let wrapped = format!("x {}hi{}", open, close);
            assert_eq!(
                doc.content, wrapped,
                "{formats:?}: typing did not wrap with the code span innermost"
            );

            doc.apply_format_in_mode(toggle_off, ViewMode::Wysiwyg);
            type_keystrokes(&mut doc, "yo");
            assert_eq!(
                doc.content,
                format!("x {expected}"),
                "{formats:?}: toggling off {toggle_off} misplaced the delimiters"
            );

            // The saved Markdown renders as styled text, never as literal
            // markers inside a code span.
            let parsed = parse_document(&doc.content);
            let segments: Vec<(&str, bool, bool, bool, bool, bool)> = parsed.lines[0]
                .styled_segments
                .iter()
                .map(|s| {
                    (
                        &doc.content[s.source.clone()],
                        s.bold,
                        s.italic,
                        s.underline,
                        s.strike,
                        s.code,
                    )
                })
                .collect();
            let hi = segments
                .iter()
                .find(|(text, ..)| *text == "hi")
                .unwrap_or_else(|| panic!("{formats:?}: no segment for hi in {segments:?}"));
            let yo = segments
                .iter()
                .find(|(text, ..)| *text == "yo")
                .unwrap_or_else(|| panic!("{formats:?}: no segment for yo in {segments:?}"));
            assert_eq!(
                (hi.1, hi.2, hi.3, hi.4, hi.5),
                *hi_flags,
                "{formats:?}: wrong styles on hi"
            );
            assert_eq!(
                (yo.1, yo.2, yo.3, yo.4, yo.5),
                *yo_flags,
                "{formats:?}: wrong styles on yo"
            );
        }
    }

    /// No styled segment may render a delimiter run as literal text, and no
    /// edit may write a zero-width space into the content.
    fn assert_no_delimiter_runs(doc: &DocumentTab) {
        assert!(
            !doc.content.contains('\u{200B}'),
            "zero-width space written into {:?}",
            doc.content
        );
        let content = doc.content.clone();
        let parsed = parse_document(&content);
        for line in &parsed.lines {
            for seg in &line.styled_segments {
                let text = &content[seg.source.clone()];
                assert!(
                    !text.contains("**") && !text.contains("~~"),
                    "literal delimiter rendered in {text:?} of {content:?}"
                );
            }
        }
    }

    /// Delete skips hidden delimiters like Backspace does and drops a span
    /// whose last visible grapheme it removes, instead of eating marker
    /// characters (`x **ab**` must not become `xy*ab**`).
    #[test]
    fn delete_forward_skips_delimiters_and_drops_empty_spans() {
        let mut doc = DocumentTab::new("d1".into(), "test.md".into(), "x **ab**".into(), None, None);
        doc.place(1, false);
        doc.delete_forward_in_mode(ViewMode::Wysiwyg);
        assert_eq!(doc.content, "x**ab**", "Delete ate a visible character");
        doc.delete_forward_in_mode(ViewMode::Wysiwyg);
        assert_eq!(doc.content, "x**b**", "Delete ate an opener character instead of skipping it");
        doc.delete_forward_in_mode(ViewMode::Wysiwyg);
        assert_eq!(
            doc.content, "x",
            "Delete did not drop the span once it was empty"
        );
        let parsed = parse_document(&doc.content);
        assert!(parsed.lines[0].styled_segments.iter().all(|s| !s.bold));
    }

    /// Backspace through a stacked span never splits a delimiter: the empty
    /// span is dropped whole, and text typed afterwards stays outside the
    /// tags (`<u>z/u>` must be impossible).
    #[test]
    fn backspace_through_stacked_span_drops_it_whole() {
        let mut doc = DocumentTab::new("d2".into(), "test.md".into(), String::new(), None, None);
        for f in ["bold", "italic", "underline"] {
            doc.apply_format_in_mode(f, ViewMode::Wysiwyg);
        }
        type_keystrokes(&mut doc, "ab");
        assert_eq!(doc.content, "<u>***ab***</u>");
        for f in ["bold", "italic", "underline"] {
            doc.apply_format_in_mode(f, ViewMode::Wysiwyg);
        }
        for _ in 0..3 {
            doc.smart_backspace_in_mode(ViewMode::Wysiwyg);
        }
        assert_eq!(doc.content, "", "backspace split a delimiter");
        type_keystrokes(&mut doc, "z");
        assert_eq!(doc.content, "z");
        assert_no_delimiter_runs(&doc);
    }

    /// Overlapping toggles close and reopen their markers so the spans nest
    /// properly and round-trip through the parser: each variant below must
    /// display without raw markers and parse back to the typed spans, with
    /// no zero-width spaces and no literal delimiter runs.
    #[test]
    fn overlapping_toggles_nest_properly() {
        // (first format, second format, typed chars, expected spans as
        // (text, bold, italic, underline, strike))
        type OverlapVariant = (
            &'static str,
            &'static str,
            Vec<&'static str>,
            Vec<(&'static str, bool, bool, bool, bool)>,
        );
        let variants: Vec<OverlapVariant> = vec![
            (
                // 1-char: a italic, b bold+italic, c bold, d plain
                "italic",
                "bold",
                vec!["a", "b", "c", "d"],
                vec![
                    ("a", false, true, false, false),
                    ("b", true, true, false, false),
                    ("c", true, false, false, false),
                    ("d", false, false, false, false),
                ],
            ),
            (
                // 2-char variant of the same overlap
                "italic",
                "bold",
                vec!["ab", "cd", "ef", "gh"],
                vec![
                    ("ab", false, true, false, false),
                    ("cd", true, true, false, false),
                    ("ef", true, false, false, false),
                    ("gh", false, false, false, false),
                ],
            ),
            (
                // mirror order: bold first, then italic
                "bold",
                "italic",
                vec!["a", "b", "c", "d"],
                vec![
                    ("a", true, false, false, false),
                    ("b", true, true, false, false),
                    ("c", false, true, false, false),
                    ("d", false, false, false, false),
                ],
            ),
            (
                // strike first, then underline
                "strike",
                "underline",
                vec!["a", "b", "c", "d"],
                vec![
                    ("a", false, false, false, true),
                    ("b", false, false, true, true),
                    ("c", false, false, true, false),
                    ("d", false, false, false, false),
                ],
            ),
            (
                // underline first, then strike
                "underline",
                "strike",
                vec!["a", "b", "c", "d"],
                vec![
                    ("a", false, false, true, false),
                    ("b", false, false, true, true),
                    ("c", false, false, false, true),
                    ("d", false, false, false, false),
                ],
            ),
        ];
        for (first, second, chars, expected) in variants {
            let mut doc = DocumentTab::new("d3".into(), "test.md".into(), String::new(), None, None);
            doc.apply_format_in_mode(first, ViewMode::Wysiwyg);
            type_keystrokes(&mut doc, chars[0]);
            doc.apply_format_in_mode(second, ViewMode::Wysiwyg);
            type_keystrokes(&mut doc, chars[1]);
            doc.apply_format_in_mode(first, ViewMode::Wysiwyg);
            type_keystrokes(&mut doc, chars[2]);
            doc.apply_format_in_mode(second, ViewMode::Wysiwyg);
            type_keystrokes(&mut doc, chars[3]);

            assert!(
                !doc.content.contains('\u{200B}'),
                "{first}/{second}: zero-width space written"
            );
            let content = doc.content.clone();
            let parsed = parse_document(&content);
            assert_eq!(
                parsed.lines[0].display_text,
                chars.concat(),
                "{first}/{second}: raw markers visible in the display"
            );
            let segments: Vec<(&str, bool, bool, bool, bool)> = parsed.lines[0]
                .styled_segments
                .iter()
                .map(|s| {
                    (
                        content[s.source.clone()].as_ref(),
                        s.bold,
                        s.italic,
                        s.underline,
                        s.strike,
                    )
                })
                .collect();
            assert_eq!(segments, expected, "{first}/{second}: wrong spans");
            assert_no_delimiter_runs(&doc);
        }
    }

    /// Toggling a non-code format while code is armed splits the code span
    /// around the change instead of nesting delimiters inside the backticks;
    /// the code span stays innermost and adjacent to the text.
    #[test]
    fn toggles_split_code_span_keeping_code_innermost() {
        for (format, open, close) in [
            ("bold", "**", "**"),
            ("italic", "*", "*"),
            ("underline", "<u>", "</u>"),
            ("strike", "~~", "~~"),
        ] {
            let mut doc = DocumentTab::new("d4".into(), "test.md".into(), String::new(), None, None);
            doc.apply_format_in_mode("code", ViewMode::Wysiwyg);
            type_keystrokes(&mut doc, "a");
            doc.apply_format_in_mode(format, ViewMode::Wysiwyg);
            type_keystrokes(&mut doc, "b");
            doc.apply_format_in_mode(format, ViewMode::Wysiwyg);
            type_keystrokes(&mut doc, "c");
            assert_eq!(
                doc.content,
                format!("`a`{open}`b`{close}`c`"),
                "{format}: delimiters nested inside the code span"
            );

            let content = doc.content.clone();
            let parsed = parse_document(&content);
            let segments: Vec<(&str, bool, bool, bool, bool, bool)> = parsed.lines[0]
                .styled_segments
                .iter()
                .map(|s| {
                    (
                        content[s.source.clone()].trim_matches('\u{200B}'),
                        s.bold,
                        s.italic,
                        s.underline,
                        s.strike,
                        s.code,
                    )
                })
                .collect();
            let want = match format {
                "bold" => vec![("a", false, false, false, false, true), ("b", true, false, false, false, true), ("c", false, false, false, false, true)],
                "italic" => vec![("a", false, false, false, false, true), ("b", false, true, false, false, true), ("c", false, false, false, false, true)],
                "underline" => vec![("a", false, false, false, false, true), ("b", false, false, true, false, true), ("c", false, false, false, false, true)],
                _ => vec![("a", false, false, false, false, true), ("b", false, false, false, true, true), ("c", false, false, false, false, true)],
            };
            assert_eq!(segments, want, "{format}: wrong styles");
            assert_no_delimiter_runs(&doc);
        }
    }

    /// A delimiter after an emoji or symbol and before a letter loses its
    /// flanking (CommonMark counts symbols as punctuation). The document
    /// text stays exactly as typed - no zero-width separators - and the
    /// parser synthesizes the pair for the display, on the closer and the
    /// opener side alike.
    #[test]
    fn symbol_adjacent_delimiters_keep_flanking() {
        // Closer side: bold emoji, toggle off, type a letter.
        let mut doc = DocumentTab::new("d5".into(), "test.md".into(), String::new(), None, None);
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "\u{1F600}\u{1F600}");
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "**\u{1F600}\u{1F600}**");
        type_keystrokes(&mut doc, "x");
        assert_eq!(
            doc.content, "**\u{1F600}\u{1F600}**x",
            "the typed text was altered"
        );
        assert!(
            !doc.content.contains('\u{200B}'),
            "zero-width space written"
        );
        let content = doc.content.clone();
        let parsed = parse_document(&content);
        let segments: Vec<(&str, bool)> = parsed.lines[0]
            .styled_segments
            .iter()
            .map(|s| (content[s.source.clone()].as_ref(), s.bold))
            .collect();
        assert_eq!(segments, vec![("\u{1F600}\u{1F600}", true), ("x", false)]);

        // Opener side: a letter right before a fresh span holding a symbol.
        let mut doc = DocumentTab::new("d6".into(), "test.md".into(), "x".into(), None, None);
        doc.place(1, false);
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "\u{1F600}");
        assert_eq!(doc.content, "x**\u{1F600}**");
        assert!(
            !doc.content.contains('\u{200B}'),
            "zero-width space written"
        );
        let content = doc.content.clone();
        let parsed = parse_document(&content);
        let segments: Vec<(&str, bool)> = parsed.lines[0]
            .styled_segments
            .iter()
            .map(|s| (content[s.source.clone()].as_ref(), s.bold))
            .collect();
        assert_eq!(segments, vec![("x", false), ("\u{1F600}", true)]);
    }

    /// Typing a span's own delimiter character inside it escapes it (or
    /// re-fences a code span) so the span stays intact and renders styled.
    #[test]
    fn typing_span_delimiters_inside_keeps_the_span() {
        // Strike: `~` inside, then a space and more text.
        let mut doc = DocumentTab::new("d7".into(), "test.md".into(), String::new(), None, None);
        doc.apply_format_in_mode("strike", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "a");
        type_keystrokes(&mut doc, "~");
        assert_eq!(doc.content, "~~a\\~~~", "the tilde was not escaped");
        type_keystrokes(&mut doc, " ");
        type_keystrokes(&mut doc, "b");
        doc.apply_format_in_mode("strike", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "~~a\\~ b~~");
        let content = doc.content.clone();
        let parsed = parse_document(&content);
        let segments: Vec<(&str, bool)> = parsed.lines[0]
            .styled_segments
            .iter()
            .map(|s| (content[s.source.clone()].trim_matches('\\'), s.strike))
            .collect();
        assert_eq!(segments, vec![("a", true), ("~ b", true)]);

        // Italic: `*` inside.
        let mut doc = DocumentTab::new("d8".into(), "test.md".into(), String::new(), None, None);
        doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "a");
        type_keystrokes(&mut doc, "*");
        type_keystrokes(&mut doc, "b");
        assert_eq!(doc.content, "*a\\*b*");
        let content = doc.content.clone();
        let parsed = parse_document(&content);
        let segments: Vec<(&str, bool)> = parsed.lines[0]
            .styled_segments
            .iter()
            .map(|s| (content[s.source.clone()].trim_matches('\\'), s.italic))
            .collect();
        assert_eq!(segments, vec![("a", true), ("*b", true)]);

        // Code: a backtick typed inside the content re-fences the span with
        // a longer run; a backtick typed at the content's end acts as the
        // closing fence and moves the caret past it.
        let mut doc = DocumentTab::new("d9".into(), "test.md".into(), String::new(), None, None);
        doc.apply_format_in_mode("code", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "ab");
        doc.place(2, false);
        type_keystrokes(&mut doc, "`");
        assert_eq!(
            doc.content, "``a`b``",
            "the backtick inside the code span did not re-fence"
        );
        let content = doc.content.clone();
        let parsed = parse_document(&content);
        let segments: Vec<(&str, bool)> = parsed.lines[0]
            .styled_segments
            .iter()
            .map(|s| (content[s.source.clone()].as_ref(), s.code))
            .collect();
        assert_eq!(segments, vec![("a`b", true)]);

        let mut doc = DocumentTab::new("d10".into(), "test.md".into(), String::new(), None, None);
        doc.apply_format_in_mode("code", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "a");
        type_keystrokes(&mut doc, "`");
        assert_eq!(
            doc.content, "``a```",
            "the backtick did not join the content"
        );
        let parsed = doc.parsed();
        assert!(
            parsed.lines[0]
                .styled_segments
                .iter()
                .any(|s| s.code && doc.content[s.source.clone()] == *"a`"),
            "the re-fenced span is not displayed as code"
        );
        doc.apply_format_in_mode("code", ViewMode::Wysiwyg);
        assert_eq!(doc.cursor_offset(), 6, "the shortcut did not exit the span");
        type_keystrokes(&mut doc, "b");
        doc.apply_format_in_mode("code", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "c");
        assert_eq!(doc.content, "``a```b`c`");
        let content = doc.content.clone();
        let parsed = parse_document(&content);
        let segments: Vec<(&str, bool)> = parsed.lines[0]
            .styled_segments
            .iter()
            .map(|s| (content[s.source.clone()].as_ref(), s.code))
            .collect();
        assert_eq!(
            segments,
            vec![("a`", true), ("b", false), ("c", true)],
            "the literal backtick and the code spans rendered wrong"
        );
        assert_no_delimiter_runs(&doc);
    }

    /// The wrap path escapes the armed span's own delimiters in typed text
    /// (strike: `\~`), a literally typed `<` that would start an HTML tag is
    /// escaped as `\<`, and escape backslashes are hidden in the display
    /// with the caret mapping intact. No zero-width space is ever written.
    #[test]
    fn wrap_escapes_delimiters_lt_and_hides_escape_backslashes() {
        // Strike armed, text containing tildes (the wrap path).
        let mut doc = DocumentTab::new("w1".into(), "test.md".into(), String::new(), None, None);
        doc.apply_format_in_mode("strike", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "~a~");
        assert_eq!(doc.content, "~~\\~a\\~~~", "the tildes were not escaped");
        doc.apply_format_in_mode("strike", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, " b");
        assert_eq!(doc.content, "~~\\~a\\~~~ b");
        assert!(!doc.content.contains('\u{200B}'));
        let content = doc.content.clone();
        let parsed = parse_document(&content);
        let segments: Vec<(&str, bool)> = parsed.lines[0]
            .styled_segments
            .iter()
            .map(|s| (content[s.source.clone()].trim_matches('\\'), s.strike))
            .collect();
        assert_eq!(segments, vec![("~a", true), ("~", true), (" b", false)]);
        assert_no_delimiter_runs(&doc);

        // `<` typed before a letter, and a letter typed after a `<`.
        let mut doc = DocumentTab::new("w2".into(), "test.md".into(), String::new(), None, None);
        type_keystrokes(&mut doc, "a");
        type_keystrokes(&mut doc, "<");
        type_keystrokes(&mut doc, "b");
        assert_eq!(doc.content, "a\\<b", "the `<` was not escaped");
        assert_eq!(parse_document(&doc.content).lines[0].display_text, "a<b");

        let mut doc = DocumentTab::new("w3".into(), "test.md".into(), String::new(), None, None);
        type_keystrokes(&mut doc, "<");
        type_keystrokes(&mut doc, "b");
        assert_eq!(doc.content, "\\<b", "the `<` was not escaped");
        assert_eq!(parse_document(&doc.content).lines[0].display_text, "<b");

        // Escape backslashes are hidden in the display and the offsets map.
        let mut doc = DocumentTab::new(
            "w4".into(),
            "test.md".into(),
            "**a\\*b**".into(),
            None,
            None,
        );
        doc.place(3, false);
        let parsed = doc.parsed();
        let line = &parsed.lines[0];
        assert_eq!(line.display_text, "a*b");
        for (idx, ch) in line.display_text.char_indices() {
            let src = line.offset_map.display_to_source(idx);
            assert!(
                doc.content.is_char_boundary(src) && doc.content[src..].starts_with(ch),
                "display {ch:?} at {idx} maps to {src}"
            );
        }
        assert!(!doc.content.contains('\u{200B}'));
    }

    #[test]
    fn test_wysiwyg_delimiter_skipping_navigation_and_atomic_backspace() {
        let mut doc =
            DocumentTab::new("n".into(), "test.md".into(), "a **b** c".into(), None, None);
        doc.place(0, false);

        // Visible text is "a b c"
        // Step right across visible characters: 'a' -> ' ' -> 'b' -> ' ' -> 'c'
        doc.move_right_in_mode(false, ViewMode::Wysiwyg);
        assert_eq!(doc.cursor_offset(), 1); // after 'a'
        doc.move_right_in_mode(false, ViewMode::Wysiwyg);
        assert_eq!(doc.cursor_offset(), 4); // skipped space and opening "**" to land before 'b'
        doc.move_right_in_mode(false, ViewMode::Wysiwyg);
        assert_eq!(doc.cursor_offset(), 7); // stepped over 'b' and skipped closing "**"

        // Step left back onto 'b'
        doc.move_left_in_mode(false, ViewMode::Wysiwyg);
        assert_eq!(doc.cursor_offset(), 4); // before 'b'

        // Place right after 'b' (offset 5 or 7) and atomic backspace the single-char bold span
        doc.place(5, false);
        doc.smart_backspace_in_mode(ViewMode::Wysiwyg);
        assert_eq!(doc.content, "a  c");
    }

    #[test]
    fn test_table_cell_tab_navigation() {
        let table = "| Name | Role |\n| --- | --- |\n| Alice | Admin |";
        let mut doc = DocumentTab::new("tbl".into(), "test.md".into(), table.into(), None, None);
        doc.place(2, false); // inside "Name"

        assert!(doc.tab_table_cell(false));
        assert_eq!(doc.selected_text(), "Role");

        assert!(doc.tab_table_cell(false));
        assert_eq!(doc.selected_text(), "Alice");

        assert!(doc.tab_table_cell(false));
        assert_eq!(doc.selected_text(), "Admin");

        // Shift+Tab goes backwards
        assert!(doc.tab_table_cell(true));
        assert_eq!(doc.selected_text(), "Alice");

        // Forward Tab to Admin, then forward Tab on last cell appends a new row
        assert!(doc.tab_table_cell(false));
        assert_eq!(doc.selected_text(), "Admin");
        assert!(doc.tab_table_cell(false));
        assert!(doc.content.ends_with("\n|  |  |"));
    }

    #[test]
    fn test_table_insertion_and_row_addition() {
        let mut doc = DocumentTab::new("t".into(), "test.md".into(), String::new(), None, None);
        doc.apply_format_in_mode("table", ViewMode::Wysiwyg);
        assert_eq!(
            doc.content,
            "| Column 1 | Column 2 |\n| :--- | :--- |\n| Item 1 | Item 2 |"
        );
        assert_eq!(doc.selected_text(), "Column 1");

        // Calling "table" format while inside a table appends a new row
        doc.apply_format_in_mode("table", ViewMode::Wysiwyg);
        assert!(doc.content.ends_with("\n|  |  |"));
    }

    #[test]
    fn test_table_smart_newline_continuation_and_exit() {
        let table = "| Col 1 | Col 2 |\n| :--- | :--- |\n| Item 1 | Item 2 |";
        let mut doc = DocumentTab::new("t".into(), "test.md".into(), table.into(), None, None);
        doc.place(table.len(), false); // at end of "Item 2 |"

        // Press Enter on non-empty row -> adds a new row
        doc.smart_newline(false);
        assert!(doc.content.ends_with("\n|  |  |"));

        // Press Enter on the new empty row -> removes it and exits table to a newline after table
        doc.smart_newline(false);
        assert_eq!(doc.content, format!("{}\n\n", table));
        assert_eq!(doc.cursor_offset(), table.len() + 2);

        // Pressing Enter again outside table does not add a row to the table
        doc.smart_newline(false);
        assert_eq!(doc.content, format!("{}\n\n\n", table));
    }

    #[test]
    fn test_table_single_empty_row_exit_keeps_header_and_removes_empty_row() {
        // Table with only a single empty body row
        let table = "| Col 1 | Col 2 |\n| :--- | :--- |\n|  |  |";
        let mut doc = DocumentTab::new("t".into(), "test.md".into(), table.into(), None, None);
        doc.place(table.len(), false); // inside empty row

        doc.smart_newline(false);
        // Exiting the only empty row keeps the header and only removes the empty row!
        assert_eq!(doc.content, "| Col 1 | Col 2 |\n| :--- | :--- |\n\n");
        assert_eq!(doc.cursor_offset(), doc.content.len());
    }

    #[test]
    fn test_table_enter_splits_cell() {
        let table = "| Col 1 | Col 2 |\n| :--- | :--- |\n| First item | Second item |";
        let mut doc = DocumentTab::new("t".into(), "test.md".into(), table.into(), None, None);
        // Place cursor between "First " and "item" in Col 1
        let pos = table.find("First ").unwrap() + "First ".len();
        doc.place(pos, false);

        doc.smart_newline(false);
        let expected = "| Col 1 | Col 2 |\n| :--- | :--- |\n| First | Second item |\n| item |  |";
        assert_eq!(doc.content, expected);
        assert_eq!(doc.cursor_offset(), expected.find("item |  |").unwrap());
    }

    #[test]
    fn test_insert_table_with_size_and_undo() {
        let mut doc = DocumentTab::new("t".into(), "test.md".into(), String::new(), None, None);
        doc.insert_table_with_size(3, 4);
        assert_eq!(
            doc.content,
            "|  |  |  |\n| :--- | :--- | :--- |\n|  |  |  |\n|  |  |  |\n|  |  |  |"
        );
        assert_eq!(doc.selected_text(), "");
        assert_eq!(doc.cursor_offset(), 2);

        // Undo in a single step restores initial state
        assert!(doc.undo());
        assert_eq!(doc.content, "");
    }

    #[test]
    fn test_table_row_and_column_manipulations_and_undo() {
        let initial_table = "| Column 1 | Column 2 |\n| :--- | :--- |\n| Item 1 | Item 2 |";
        let mut doc = DocumentTab::new(
            "t".into(),
            "test.md".into(),
            initial_table.into(),
            None,
            None,
        );

        // Place cursor in Item 1
        let pos = doc.content.find("Item 1").unwrap() + 2;
        doc.place(pos, false);

        // Add row above
        doc.table_add_row_above();
        assert!(doc
            .content
            .contains("| Column 1 | Column 2 |\n| :--- | :--- |\n|  |  |\n| Item 1 | Item 2 |"));
        assert!(doc.undo());
        assert_eq!(doc.content, initial_table);

        // Add row below
        let pos = doc.content.find("Item 1").unwrap() + 2;
        doc.place(pos, false);
        doc.table_add_row_below();
        assert!(doc.content.ends_with("\n|  |  |"));
        assert!(doc.undo());
        assert_eq!(doc.content, initial_table);

        // Add column right
        let pos = doc.content.find("Item 1").unwrap() + 2;
        doc.place(pos, false);
        doc.table_add_col_right();
        assert!(doc.content.contains("| Item 1 |  | Item 2 |"));
        assert!(doc.undo());
        assert_eq!(doc.content, initial_table);

        // Add column left
        let pos = doc.content.find("Item 1").unwrap() + 2;
        doc.place(pos, false);
        doc.table_add_col_left();
        assert!(doc.content.contains("|  | Item 1 | Item 2 |"));
        assert!(doc.undo());
        assert_eq!(doc.content, initial_table);

        // Align column right
        let pos = doc.content.find("Item 1").unwrap() + 2;
        doc.place(pos, false);
        doc.table_align_column(TableColumnAlign::Right);
        assert!(doc.content.contains("| ---: | :--- |"));
        assert!(doc.undo());
        assert!(doc.content.contains("| :--- | :--- |"));

        // Delete row
        let pos = doc.content.find("Item 1").unwrap() + 2;
        doc.place(pos, false);
        doc.table_delete_row();
        assert_eq!(doc.content, "| Column 1 | Column 2 |\n| :--- | :--- |");
        assert!(doc.undo());
        assert_eq!(doc.content, initial_table);

        // Delete table
        let pos = doc.content.find("Item 1").unwrap() + 2;
        doc.place(pos, false);
        doc.table_delete_table();
        assert_eq!(doc.content, "");
        assert!(doc.undo());
        assert_eq!(doc.content, initial_table);
    }

    #[test]
    fn test_table_edit_keeps_escaped_pipe() {
        let table = "| a \\| b | c |\n| --- | --- |\n| d | e |";
        let mut doc = DocumentTab::new("t".into(), "test.md".into(), table.into(), None, None);
        doc.place(doc.content.find('a').unwrap(), false);
        doc.table_add_col_right();
        assert_eq!(
            doc.content,
            "| a \\| b |  | c |\n| --- | --- | --- |\n| d |  | e |"
        );
        assert!(!doc.content.contains("| a \\ |"));
    }

    #[test]
    fn test_table_edit_keeps_unset_alignment() {
        let table = "| a | b |\n| --- | --- |\n| 1 | 2 |";
        let mut doc = DocumentTab::new("t".into(), "test.md".into(), table.into(), None, None);
        doc.place(doc.content.find('a').unwrap(), false);
        doc.table_add_col_right();
        assert!(doc.content.contains("| --- | --- | --- |"));
        assert!(!doc.content.contains(":---"));

        let mixed = "| a | b |\n| :--- | --- |\n| 1 | 2 |";
        let mut doc = DocumentTab::new("t".into(), "test.md".into(), mixed.into(), None, None);
        doc.place(doc.content.find('1').unwrap(), false);
        doc.table_add_row_below();
        assert!(doc.content.contains("| :--- | --- |"));
        assert!(!doc.content.contains("| :--- | :--- |"));
        assert_eq!(doc.content, "| a | b |\n| :--- | --- |\n| 1 | 2 |\n|  |  |");
    }

    #[test]
    fn test_wysiwyg_table_boundary_guards() {
        let table = "| Col 1 | Col 2 |\n| :--- | :--- |\n| Item 1 | Item 2 |";
        let mut doc = DocumentTab::new("t".into(), "test.md".into(), table.into(), None, None);

        // Position at start of "Item 1" (right after leading pipe/space)
        let pos = table.find("Item 1").unwrap();
        doc.place(pos, false);

        // smart_backspace in WYSIWYG mode should NOT delete the delimiter or merge with previous cell
        doc.smart_backspace_in_mode(ViewMode::Wysiwyg);
        assert_eq!(doc.content, table);
        assert_eq!(doc.cursor_offset(), pos);

        // Position at end of "Item 1"
        let pos = table.find("Item 1").unwrap() + "Item 1".len();
        doc.place(pos, false);

        // delete_forward in WYSIWYG mode should NOT delete the delimiter
        doc.delete_forward_in_mode(ViewMode::Wysiwyg);
        assert_eq!(doc.content, table);
        assert_eq!(doc.cursor_offset(), pos);
    }

    #[test]
    fn test_enter_on_indented_empty_quote_leaves_quote() {
        let mut doc = DocumentTab::new("t".into(), "test.md".into(), "  > ".into(), None, None);
        doc.place("  > ".len(), false);

        // Enter on indented empty quote removes the quote line, leaving the quote completely
        doc.smart_newline(false);
        assert_eq!(doc.content, "");
    }

    #[test]
    fn test_enter_on_empty_list_item_with_selection() {
        let mut doc = DocumentTab::new("t".into(), "test.md".into(), "- ".into(), None, None);
        // Select the whole "- "
        doc.caret.range = 0..2;

        // Enter on empty list item with selection must replace selection with newline
        doc.smart_newline(false);
        assert_eq!(doc.content, "\n");
    }

    #[test]
    fn test_task_list_asterisk_continuation() {
        let mut doc = DocumentTab::new(
            "t".into(),
            "test.md".into(),
            "* [x] done".into(),
            None,
            None,
        );
        doc.place("* [x] done".len(), false);

        // Enter on "* [x] done" must continue with "* [ ] ", not "- [ ] "
        doc.smart_newline(false);
        assert_eq!(doc.content, "* [x] done\n* [ ] ");
    }

    #[test]
    fn test_emoji_formatting_no_crash() {
        let content = "🦀hello world";
        let mut doc = DocumentTab::new("t".into(), "test.md".into(), content.into(), None, None);
        // Place caret right after emoji (at byte 4)
        doc.place(4, false);

        // Press Ctrl+U (underline) in Wysiwyg mode next to emoji -> must not crash!
        doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);

        // Press Ctrl+I (italic) in Wysiwyg mode next to emoji -> must not crash!
        doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);

        let content2 = "hello🦀";
        let mut doc2 = DocumentTab::new("t".into(), "test.md".into(), content2.into(), None, None);
        doc2.place(content2.len(), false);
        doc2.apply_format_in_mode("underline", ViewMode::Wysiwyg);
        doc2.apply_format_in_mode("italic", ViewMode::Wysiwyg);
    }

    #[test]
    fn test_backspace_on_text_ending_with_dash_space() {
        let mut doc = DocumentTab::new("t".into(), "test.md".into(), "foo - ".into(), None, None);
        doc.place("foo - ".len(), false);

        // Backspacing on "foo - " must delete the space, NOT wipe out "foo - " to empty!
        doc.smart_backspace();
        assert_eq!(doc.content, "foo -");

        doc.smart_backspace();
        assert_eq!(doc.content, "foo ");
    }

    #[test]
    fn test_enter_on_indented_bullet_tab() {
        let mut doc = DocumentTab::new("t".into(), "test.md".into(), "\t- ".into(), None, None);
        doc.place("\t- ".len(), false);

        // Enter on "\t- " must outdent to "- ", NOT wipe to empty string!
        doc.smart_newline(false);
        assert_eq!(doc.content, "- ");

        // Enter on root "- " exits the list, removing the marker
        doc.smart_newline(false);
        assert_eq!(doc.content, "");
    }

    fn edit_probe(source: &str) -> (usize, usize, String, String, String, usize, String) {
        let prefix_len = block_prefix_len(source);
        let marker_len = crate::model::text_ops::list_marker_len(source);
        let mut newline_doc =
            DocumentTab::new("n".into(), "t.md".into(), source.into(), None, None);
        newline_doc.place(source.len(), false);
        newline_doc.smart_newline(false);
        let mut backspace_doc =
            DocumentTab::new("b".into(), "t.md".into(), source.into(), None, None);
        backspace_doc.place(source.len(), false);
        backspace_doc.smart_backspace();
        let mut toggle_doc = DocumentTab::new("o".into(), "t.md".into(), source.into(), None, None);
        toggle_doc.place(source.len(), false);
        toggle_doc.apply_format_in_mode("ol", ViewMode::Wysiwyg);
        let mut home_doc = DocumentTab::new("h".into(), "t.md".into(), source.into(), None, None);
        home_doc.place(0, false);
        home_doc.move_home_in_mode(false, ViewMode::Wysiwyg);
        let mut typed = source.to_string();
        let at = if prefix_len > 0 { prefix_len } else { 0 };
        typed.insert(at, 'Z');
        (
            prefix_len,
            marker_len,
            newline_doc.content,
            backspace_doc.content,
            toggle_doc.content,
            home_doc.cursor_offset(),
            typed,
        )
    }

    #[test]
    fn editing_helpers_accept_dot_and_paren() {
        let (prefix, marker, newline, _backspace, toggle, home, typed) = edit_probe("1. foo");
        assert_eq!(prefix, 3, "ed-dot prefix_len");
        assert_eq!(marker, 3, "ed-dot list_marker_len");
        assert_eq!(newline, "1. foo\n2. ", "ed-dot newline");
        assert_eq!(toggle, "foo", "ed-dot ol_toggle");
        assert_eq!(home, 3, "ed-dot home_from_0");
        assert_eq!(typed, "1. Zfoo", "ed-dot type_at_0");

        let (prefix, marker, newline, _backspace, toggle, home, typed) = edit_probe("1) foo");
        assert_eq!(prefix, 3, "ed-paren prefix_len");
        assert_eq!(marker, 3, "ed-paren list_marker_len");
        assert_eq!(newline, "1) foo\n2) ", "ed-paren newline");
        assert_eq!(toggle, "foo", "ed-paren ol_toggle");
        assert_eq!(home, 3, "ed-paren home_from_0");
        assert_eq!(typed, "1) Zfoo", "ed-paren type_at_0");

        let (_prefix, _marker, newline, backspace, _toggle, _home, _typed) = edit_probe("1. ");
        assert_eq!(newline, "", "ed-marker-dot newline");
        assert_eq!(backspace, "", "ed-marker-dot backspace");

        let (_prefix, _marker, newline, backspace, toggle, _home, typed) = edit_probe("1) ");
        assert_eq!(newline, "", "ed-marker-paren newline");
        assert_eq!(backspace, "", "ed-marker-paren backspace");
        assert_eq!(toggle, "", "ed-marker-paren ol_toggle");
        assert_eq!(typed, "1) Z", "ed-marker-paren type_at_0");

        let (prefix, marker, newline, _backspace, toggle, home, typed) = edit_probe("1. [ ] foo");
        assert_eq!(prefix, 7, "ed-task prefix_len");
        assert_eq!(marker, 7, "ed-task list_marker_len");
        assert_eq!(newline, "1. [ ] foo\n2. [ ] ", "ed-task newline");
        // The ordered-list button is off on an ordered task, so the click keeps the line.
        assert_eq!(toggle, "1. [ ] foo", "ed-task ol_toggle");
        assert_eq!(home, 7, "ed-task home_from_0");
        assert_eq!(typed, "1. [ ] Zfoo", "ed-task type_at_0");

        let (prefix, marker, newline, _backspace, toggle, home, typed) = edit_probe("1) [ ] foo");
        assert_eq!(prefix, 7, "ed-task-paren prefix_len");
        assert_eq!(marker, 7, "ed-task-paren list_marker_len");
        assert_eq!(newline, "1) [ ] foo\n2) [ ] ", "ed-task-paren newline");
        assert_eq!(toggle, "1) [ ] foo", "ed-task-paren ol_toggle");
        assert_eq!(home, 7, "ed-task-paren home_from_0");
        assert_eq!(typed, "1) [ ] Zfoo", "ed-task-paren type_at_0");

        let (prefix, marker, newline, _backspace, _toggle, home, typed) = edit_probe("- [ ] foo");
        assert_eq!(prefix, 6, "ed-bullet-task prefix_len");
        assert_eq!(marker, 6, "ed-bullet-task list_marker_len");
        assert_eq!(newline, "- [ ] foo\n- [ ] ", "ed-bullet-task newline");
        assert_eq!(home, 6, "ed-bullet-task home_from_0");
        assert_eq!(typed, "- [ ] Zfoo", "ed-bullet-task type_at_0");

        let (prefix, marker, newline, _backspace, toggle, home, typed) = edit_probe("10) foo");
        assert_eq!(prefix, 4, "ed-ten prefix_len");
        assert_eq!(marker, 4, "ed-ten list_marker_len");
        assert_eq!(newline, "10) foo\n11) ", "ed-ten newline");
        assert_eq!(toggle, "foo", "ed-ten ol_toggle");
        assert_eq!(home, 4, "ed-ten home_from_0");
        assert_eq!(typed, "10) Zfoo", "ed-ten type_at_0");

        let (prefix, _marker, newline, backspace, toggle, home, typed) = edit_probe("1)");
        assert_eq!(prefix, 2, "ed-empty-paren prefix_len");
        assert_eq!(newline, "", "ed-empty-paren newline");
        assert_eq!(backspace, "", "ed-empty-paren backspace");
        assert_eq!(toggle, "", "ed-empty-paren ol_toggle");
        assert_eq!(home, 2, "ed-empty-paren home_from_0");
        assert_eq!(typed, "1)Z", "ed-empty-paren type_at_0");

        let (prefix, _marker, newline, backspace, _toggle, home, typed) = edit_probe("1.");
        assert_eq!(prefix, 2, "ed-empty-dot prefix_len");
        assert_eq!(newline, "", "ed-empty-dot newline");
        assert_eq!(backspace, "", "ed-empty-dot backspace");
        assert_eq!(home, 2, "ed-empty-dot home_from_0");
        assert_eq!(typed, "1.Z", "ed-empty-dot type_at_0");
    }

    #[test]
    fn test_enter_with_selection_is_single_undo_step() {
        let mut doc = DocumentTab::new(
            "t".into(),
            "test.md".into(),
            "hello world".into(),
            None,
            None,
        );
        doc.caret.range = 6..11;
        doc.smart_newline(false);
        assert_eq!(doc.content, "hello \n");
        assert!(doc.undo());
        assert_eq!(doc.content, "hello world");
        assert!(!doc.undo());
    }

    #[test]
    fn unclosed_fence_toggle_unwraps_to_eof() {
        let mut doc = DocumentTab::new(
            "c".into(),
            "code.md".into(),
            "```\naaa\n".into(),
            None,
            None,
        );
        doc.place(5, false);
        doc.apply_format_in_mode("codeblock", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "aaa\n");
    }

    #[test]
    fn crlf_and_bare_cr_carets_stay_out_of_the_line_ending() {
        let crlf = "Foo\r\nbar\r\nbaz";
        let mut doc = DocumentTab::new("s".into(), "carets.md".into(), crlf.into(), None, None);
        doc.place(0, false);
        doc.move_end(false);
        assert_eq!(doc.cursor_offset(), 3, "End stops before CRLF");
        doc.insert_text_in_mode(
            doc.cursor_offset()..doc.cursor_offset(),
            "X",
            ViewMode::Wysiwyg,
        );
        assert_eq!(doc.content, "FooX\r\nbar\r\nbaz");

        let mut doc = DocumentTab::new("s".into(), "carets.md".into(), crlf.into(), None, None);
        doc.place(5, false);
        doc.move_end(false);
        assert_eq!(doc.cursor_offset(), 8, "End on bar stops before its CRLF");

        let bare = "Foo\rbar\rbaz";
        let mut doc = DocumentTab::new("s".into(), "carets.md".into(), bare.into(), None, None);
        doc.place(0, false);
        doc.move_end(false);
        assert_eq!(doc.cursor_offset(), 3);
        doc.insert_text_in_mode(3..3, "X", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "FooX\rbar\rbaz");

        let mut doc = DocumentTab::new("s".into(), "carets.md".into(), bare.into(), None, None);
        doc.place(1, false);
        doc.delete_to_line_end();
        assert_eq!(doc.content, "F\rbar\rbaz");

        let mut doc = DocumentTab::new("s".into(), "carets.md".into(), bare.into(), None, None);
        doc.place(0, false);
        doc.move_down(false);
        assert_eq!(doc.cursor_offset(), 4, "Down crosses a bare CR");
        doc.move_end(false);
        assert_eq!(doc.cursor_offset(), 7, "End stops before the next bare CR");
        doc.move_up(false);
        assert_eq!(
            doc.cursor_offset(),
            3,
            "Up crosses a bare CR and keeps the end column"
        );

        let mut doc = DocumentTab::new("s".into(), "carets.md".into(), crlf.into(), None, None);
        doc.place(doc.content.len(), false);
        for _ in 0..4 {
            doc.move_left_in_mode(false, ViewMode::Wysiwyg);
        }
        assert_ne!(
            doc.cursor_offset(),
            9,
            "Left must not sit between CR and LF"
        );
        let at = doc.cursor_offset();
        doc.insert_text_in_mode(at..at, "X", ViewMode::Wysiwyg);
        assert!(
            !doc.content.contains("\rX\n"),
            "typed between CR and LF: {:?}",
            doc.content
        );
    }

    #[test]
    fn move_line_keeps_crlf_and_bare_cr_endings() {
        let crlf = "Aaa\r\nBbb\r\nCcc";
        let mut doc = DocumentTab::new("s".into(), "lines.md".into(), crlf.into(), None, None);
        doc.place(5, false);
        doc.move_line_up();
        assert_eq!(doc.content, "Bbb\r\nAaa\r\nCcc");

        let mut doc = DocumentTab::new("s".into(), "lines.md".into(), crlf.into(), None, None);
        doc.place(5, false);
        doc.move_line_down();
        assert_eq!(doc.content, "Aaa\r\nCcc\r\nBbb");

        let bare = "Aaa\rBbb\rCcc";
        let mut doc = DocumentTab::new("s".into(), "lines.md".into(), bare.into(), None, None);
        doc.place(4, false);
        doc.move_line_up();
        assert_eq!(doc.content, "Bbb\rAaa\rCcc");

        let mut doc = DocumentTab::new("s".into(), "lines.md".into(), bare.into(), None, None);
        doc.place(4, false);
        doc.move_line_down();
        assert_eq!(doc.content, "Aaa\rCcc\rBbb");

        let lf = "Aaa\nBbb\nCcc";
        let mut doc = DocumentTab::new("s".into(), "lines.md".into(), lf.into(), None, None);
        doc.place(4, false);
        doc.move_line_up();
        assert_eq!(doc.content, "Bbb\nAaa\nCcc");
    }

    /// End, then Home, Backspace, and typing, the way CHECKER-4-R3 drove them.
    fn crlf_caret(source: &str) -> (usize, usize, String, String, String, String, String) {
        let mut ended = DocumentTab::new("e".into(), "t.md".into(), source.into(), None, None);
        ended.move_end(false);
        let end_at = ended.cursor_offset();
        let before_disp = ended.parsed().lines[0].display_text.clone();
        ended.move_home_in_mode(false, ViewMode::Wysiwyg);
        let home = ended.cursor_offset();

        let mut typed = DocumentTab::new("z".into(), "t.md".into(), source.into(), None, None);
        typed.move_end(false);
        let at = typed.cursor_offset();
        typed.edit(at..at, "Z");
        let typed_content = typed.content.clone();
        let typed_disp = typed.parsed().lines[0].display_text.clone();

        let mut backspaced = DocumentTab::new("b".into(), "t.md".into(), source.into(), None, None);
        backspaced.move_end(false);
        backspaced.smart_backspace_in_mode(ViewMode::Wysiwyg);
        let bs_content = backspaced.content.clone();
        let bs_disp = backspaced.parsed().lines[0].display_text.clone();

        (
            end_at,
            home,
            before_disp,
            typed_content,
            typed_disp,
            bs_content,
            bs_disp,
        )
    }

    #[test]
    fn crlf_home_after_end_on_1_dot_foo() {
        let (end_at, home, before_disp, _, _, _, _) = crlf_caret("1. foo\r\n");
        assert_eq!(end_at, 6, "end_at");
        assert_eq!(home, 3, "home after end");
        assert_eq!(before_disp, "foo", "before_disp");
    }

    #[test]
    fn crlf_first_backspace_on_1_dot_foo() {
        let (_, _, before_disp, _, _, bs_content, bs_disp) = crlf_caret("1. foo\r\n");
        assert_eq!(before_disp, "foo", "before_disp");
        assert_eq!(bs_content, "1. fo\r\n");
        assert_eq!(bs_disp, "fo");
    }

    #[test]
    fn crlf_type_z_at_end_on_1_dot_foo() {
        let (_, _, before_disp, typed_content, typed_disp, _, _) = crlf_caret("1. foo\r\n");
        assert_eq!(before_disp, "foo", "before_disp");
        assert_eq!(typed_content, "1. fooZ\r\n");
        assert_eq!(typed_disp, "fooZ");
        assert_eq!(typed_disp, format!("{before_disp}Z"));
    }

    #[test]
    fn bare_cr_after_marker_1_dot_crfoo_is_not_a_list_item() {
        let source = "1.\rfoo";
        let (end_at, home, before_disp, typed_content, typed_disp, bs_content, bs_disp) =
            crlf_caret(source);
        let doc = parse_document(source);
        assert!(
            matches!(
                doc.lines[0].kind,
                LineKind::ListItem {
                    ordered: true,
                    number: 1,
                    ..
                }
            ),
            "1. before a bare CR is an empty item, display {before_disp:?}"
        );
        assert!(
            matches!(doc.lines[1].kind, LineKind::Paragraph),
            "foo after the bare CR is its own line"
        );
        assert_eq!(doc.lines[1].display_text, "foo");
        assert_eq!(end_at, 2, "end_at");
        assert_eq!(home, 2, "home");
        assert_eq!(before_disp, "");
        assert_eq!(typed_content, "1.Z\rfoo");
        assert_eq!(typed_disp, "1.Z");
        assert_eq!(bs_content, "\rfoo");
        assert_eq!(bs_disp, "");
    }

    fn place_end(content: &str) -> DocumentTab {
        let mut doc = DocumentTab::new("p".into(), "t.md".into(), content.to_string(), None, None);
        doc.move_end(false);
        doc
    }

    #[test]
    fn checklist_toggle_keeps_ordered_marker() {
        for (source, off, on) in [
            ("1. [x] a", "1. a", "- [ ] a"),
            ("1) [x] a", "1) a", "- [ ] a"),
            ("12. [X] a", "12. a", "- [ ] a"),
        ] {
            let mut doc =
                DocumentTab::new("c".into(), "t.md".into(), source.to_string(), None, None);
            assert!(doc.active_formats.checklist, "{source} checklist");
            assert!(!doc.active_formats.ol, "{source} ol");
            doc.apply_format_in_mode("checklist", ViewMode::Wysiwyg);
            assert_eq!(doc.content, off, "{source} off");
            assert!(!doc.active_formats.checklist, "{source} checklist off");
            assert!(doc.active_formats.ol, "{source} ol on");
            doc.apply_format_in_mode("checklist", ViewMode::Wysiwyg);
            assert_eq!(doc.content, on, "{source} on");
            assert!(doc.active_formats.checklist, "{source} checklist on");
            assert!(!doc.active_formats.ol, "{source} ol off");
        }
    }

    #[test]
    fn edit_probes_for_interrupted_and_indented_markers() {
        // Empty `*` after a paragraph stays text. End lands on that line; Enter inserts a newline.
        let mut entered = DocumentTab::new("e".into(), "t.md".into(), "foo\n*".into(), None, None);
        entered.place(4, false);
        entered.move_end(false);
        assert_eq!(entered.cursor_offset(), 5);
        entered.smart_newline(false);
        assert_eq!(entered.content, "foo\n*\n");
        let mut typed = DocumentTab::new("t".into(), "t.md".into(), "foo\n*".into(), None, None);
        typed.place(5, false);
        typed.move_home_in_mode(false, ViewMode::Wysiwyg);
        assert_eq!(typed.cursor_offset(), 4);
        typed.edit(4..4, "Z");
        assert_eq!(typed.content, "foo\nZ*");
        let mut deleted = DocumentTab::new("d".into(), "t.md".into(), "foo\n*".into(), None, None);
        deleted.edit(4..5, "");
        assert_eq!(deleted.content, "foo\n");
        let mut backspaced =
            DocumentTab::new("b".into(), "t.md".into(), "foo\n*".into(), None, None);
        backspaced.place(5, false);
        backspaced.smart_backspace_in_mode(ViewMode::Wysiwyg);
        assert_eq!(backspaced.content, "foo\n");

        // `14)` after paragraph text is not a list marker.
        let paren = "The number of windows in my house is\n14)  The number of doors is 6.";
        let mut paren_enter =
            DocumentTab::new("e".into(), "t.md".into(), paren.to_string(), None, None);
        paren_enter.place(paren.find('\n').unwrap() + 1, false);
        paren_enter.move_end(false);
        let paren_end = paren_enter.cursor_offset();
        assert_eq!(paren_end, paren.len());
        paren_enter.smart_newline(false);
        assert_eq!(paren_enter.content, format!("{paren}\n"));
        let marker_at = paren.find("14)").unwrap();
        let mut paren_type =
            DocumentTab::new("t".into(), "t.md".into(), paren.to_string(), None, None);
        paren_type.edit(marker_at..marker_at, "Z");
        assert_eq!(
            paren_type.content,
            "The number of windows in my house is\nZ14)  The number of doors is 6."
        );
        let mut paren_bs =
            DocumentTab::new("b".into(), "t.md".into(), paren.to_string(), None, None);
        paren_bs.place(marker_at + "14)  ".len(), false);
        paren_bs.smart_backspace_in_mode(ViewMode::Wysiwyg);
        assert_eq!(
            paren_bs.content,
            "The number of windows in my house is\n14) The number of doors is 6."
        );

        // Column 4 marker is text. Enter does not continue a list.
        let indented = " \t63.\tnot";
        let mut ind_enter = place_end(indented);
        assert_eq!(ind_enter.cursor_offset(), indented.len());
        ind_enter.smart_newline(false);
        assert_eq!(ind_enter.content, " \t63.\tnot\n \t");
        let mut ind_type =
            DocumentTab::new("t".into(), "t.md".into(), indented.to_string(), None, None);
        ind_type.move_home_in_mode(false, ViewMode::Wysiwyg);
        assert_eq!(ind_type.cursor_offset(), 0);
        ind_type.edit(0..0, "Z");
        assert_eq!(ind_type.content, "Z \t63.\tnot");
        let mut ind_bs = place_end(indented);
        ind_bs.smart_backspace_in_mode(ViewMode::Wysiwyg);
        assert_eq!(ind_bs.content, " \t63.\tno");

        // A nested marker under a parent item still continues.
        let nested_src = "10) foo\n    - bar";
        let mut nested = DocumentTab::new(
            "n".into(),
            "t.md".into(),
            nested_src.to_string(),
            None,
            None,
        );
        nested.place(nested_src.find("    - bar").unwrap(), false);
        nested.move_end(false);
        assert_eq!(nested.cursor_offset(), nested_src.len());
        nested.smart_newline(false);
        assert_eq!(nested.content, "10) foo\n    - bar\n    - ");
    }

    #[test]
    fn enter_keeps_cr_on_ordered_task_and_paren_marker() {
        let mut task =
            DocumentTab::new("t".into(), "t.md".into(), "1. [ ] \r\n".into(), None, None);
        task.place(crate::model::text_ops::line_end(&task.content, 0), false);
        task.smart_newline(false);
        assert_eq!(task.content, "1. [ ] \n2. [ ] \r\n");
        let mut paren = DocumentTab::new("p".into(), "t.md".into(), "1) \r\n".into(), None, None);
        paren.place(crate::model::text_ops::line_end(&paren.content, 0), false);
        paren.smart_newline(false);
        assert_eq!(paren.content, "1) \n2) \r\n");
        // Plain dot and bullet markers still exit, the same as the kit.
        let mut dot = DocumentTab::new("d".into(), "t.md".into(), "1. \r\n".into(), None, None);
        dot.place(crate::model::text_ops::line_end(&dot.content, 0), false);
        dot.smart_newline(false);
        assert_eq!(dot.content, "\n");
        let mut bullet =
            DocumentTab::new("b".into(), "t.md".into(), "- [ ] \r\n".into(), None, None);
        bullet.place(crate::model::text_ops::line_end(&bullet.content, 0), false);
        bullet.smart_newline(false);
        assert_eq!(bullet.content, "\n");
    }

    /// Ordered-list button is off on an ordered task, and the click leaves
    /// the number and the box in place.
    #[test]
    fn ordered_button_off_does_not_strip_task() {
        for source in ["1. [x] a", "1) [x] a", "12. [X] a"] {
            let mut doc =
                DocumentTab::new("o".into(), "t.md".into(), source.to_string(), None, None);
            assert_eq!(
                format!(
                    "ol={} checklist={} content={:?}",
                    doc.active_formats.ol, doc.active_formats.checklist, doc.content
                ),
                format!("ol=false checklist=true content={source:?}"),
                "{source} before"
            );
            doc.apply_format_in_mode("ol", ViewMode::Wysiwyg);
            assert_eq!(
                format!(
                    "ol={} checklist={} content={:?}",
                    doc.active_formats.ol, doc.active_formats.checklist, doc.content
                ),
                format!("ol=false checklist=true content={source:?}"),
                "{source} after ol click"
            );
        }
    }

    /// Delete, Enter, Backspace, End, and type at the markers round 6 changed.
    #[test]
    fn edit_probes_for_nested_lazy_and_crlf_markers() {
        let nested = "- foo\n  *";
        let mut nested_end =
            DocumentTab::new("e".into(), "t.md".into(), nested.to_string(), None, None);
        nested_end.place(6, false);
        nested_end.move_end(false);
        assert_eq!(nested_end.cursor_offset(), nested.len());
        nested_end.smart_newline(false);
        assert_eq!(nested_end.content, "- foo\n  *\n  ");
        let mut nested_type =
            DocumentTab::new("t".into(), "t.md".into(), nested.to_string(), None, None);
        nested_type.place(6, false);
        nested_type.move_home_in_mode(false, ViewMode::Wysiwyg);
        assert_eq!(nested_type.cursor_offset(), 6);
        nested_type.edit(6..6, "Z");
        assert_eq!(nested_type.content, "- foo\nZ  *");
        let mut nested_del =
            DocumentTab::new("d".into(), "t.md".into(), nested.to_string(), None, None);
        nested_del.edit(8..9, "");
        assert_eq!(nested_del.content, "- foo\n  ");
        let mut nested_bs =
            DocumentTab::new("b".into(), "t.md".into(), nested.to_string(), None, None);
        nested_bs.place(nested.len(), false);
        nested_bs.smart_backspace_in_mode(ViewMode::Wysiwyg);
        assert_eq!(nested_bs.content, "- foo\n  ");

        let lazy = "- foo\nbar\n2. baz";
        let mut lazy_end =
            DocumentTab::new("e".into(), "t.md".into(), lazy.to_string(), None, None);
        lazy_end.place(lazy.rfind('\n').unwrap() + 1, false);
        lazy_end.move_end(false);
        assert_eq!(lazy_end.cursor_offset(), lazy.len());
        lazy_end.smart_newline(false);
        assert_eq!(lazy_end.content, "- foo\nbar\n2. baz\n3. ");
        let marker = lazy.rfind("2. ").unwrap();
        let mut lazy_del =
            DocumentTab::new("d".into(), "t.md".into(), lazy.to_string(), None, None);
        lazy_del.edit(marker..marker + 3, "");
        assert_eq!(lazy_del.content, "- foo\nbar\nbaz");
        let mut lazy_bs = DocumentTab::new("b".into(), "t.md".into(), lazy.to_string(), None, None);
        lazy_bs.place(lazy.len(), false);
        lazy_bs.smart_backspace_in_mode(ViewMode::Wysiwyg);
        assert_eq!(lazy_bs.content, "- foo\nbar\n2. ba");
        let mut lazy_type =
            DocumentTab::new("t".into(), "t.md".into(), lazy.to_string(), None, None);
        lazy_type.place(marker, false);
        lazy_type.move_home_in_mode(false, ViewMode::Wysiwyg);
        lazy_type.edit(lazy_type.cursor_offset()..lazy_type.cursor_offset(), "Z");
        assert_eq!(lazy_type.content, "- foo\nbar\n2. Zbaz");

        let crlf = "1. foo\r\n2.\r\n3. bar";
        let third = crlf.rfind('\n').unwrap() + 1;
        let mut crlf_end =
            DocumentTab::new("e".into(), "t.md".into(), crlf.to_string(), None, None);
        crlf_end.place(third, false);
        crlf_end.move_end(false);
        assert_eq!(crlf_end.cursor_offset(), crlf.len());
        crlf_end.smart_newline(false);
        assert_eq!(crlf_end.content, "1. foo\r\n2.\r\n3. bar\n4. ");
        let mut crlf_del =
            DocumentTab::new("d".into(), "t.md".into(), crlf.to_string(), None, None);
        crlf_del.edit(third..third + 3, "");
        assert_eq!(crlf_del.content, "1. foo\r\n2.\r\nbar");
        let mut crlf_type =
            DocumentTab::new("t".into(), "t.md".into(), crlf.to_string(), None, None);
        crlf_type.place(third, false);
        crlf_type.move_home_in_mode(false, ViewMode::Wysiwyg);
        let home = crlf_type.cursor_offset();
        crlf_type.edit(home..home, "Z");
        assert_eq!(crlf_type.content, "1. foo\r\n2.\r\n3. Zbar");
        let mut crlf_bs = DocumentTab::new("b".into(), "t.md".into(), crlf.to_string(), None, None);
        crlf_bs.place(crlf.len(), false);
        crlf_bs.smart_backspace_in_mode(ViewMode::Wysiwyg);
        assert_eq!(crlf_bs.content, "1. foo\r\n2.\r\n3. ba");
    }

    /// R5-1 / R6-1. Click, End, and Right share the visible end of a closing-hash
    /// heading. Delete there joins the next line the way a heading without a
    /// closer does (`# foo\nnext` becomes `# foonext`). Enter there breaks
    /// after the heading.
    #[test]
    fn closing_hash_click_end_delete_and_enter_keep_the_heading() {
        let src = "# foo #\nnext";
        let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
        let click = {
            let parsed = doc.parsed();
            let line = &parsed.lines[0];
            assert_eq!(line.display_text, "foo");
            line.offset_map.display_to_source(line.display_text.len())
        };

        doc.place(0, false);
        doc.move_end(false);
        assert_eq!(doc.cursor_offset(), click, "End lands on the click");
        doc.place(4, false);
        doc.move_right_in_mode(false, ViewMode::Wysiwyg);
        assert_eq!(doc.cursor_offset(), click, "Right lands on the click");
        doc.place(7, false);
        assert_eq!(
            doc.cursor_offset(),
            click,
            "place at the source end snaps to the click"
        );

        doc.place(click, false);
        doc.delete_forward_in_mode(ViewMode::Wysiwyg);
        // BASE Delete at the source end (offset 7, closer still visible) is
        // these bytes. Display `foo #next`: the hash is no longer a closer.
        assert_eq!(doc.content, "# foo #next");
        let parsed = doc.parsed();
        assert_eq!(parsed.lines.len(), 1);
        assert_eq!(parsed.lines[0].display_text, "foo #next");
        assert!(matches!(
            parsed.lines[0].kind,
            LineKind::Heading { level: 1 }
        ));

        let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
        doc.place(click, false);
        doc.smart_newline(false);
        assert_eq!(doc.content, "# foo #\n\nnext");
        let parsed = doc.parsed();
        assert_eq!(parsed.lines[0].display_text, "foo");
        assert_eq!(parsed.lines[1].display_text, "");
        assert_eq!(parsed.lines[2].display_text, "next");
        assert!(matches!(
            parsed.lines[0].kind,
            LineKind::Heading { level: 1 }
        ));
        assert!(matches!(parsed.lines[1].kind, LineKind::Empty));
        assert!(matches!(parsed.lines[2].kind, LineKind::Paragraph));

        let src = "## foo ##\nbar";
        let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
        let click = {
            let parsed = doc.parsed();
            let line = &parsed.lines[0];
            assert_eq!(line.display_text, "foo");
            line.offset_map.display_to_source(line.display_text.len())
        };
        doc.place(0, false);
        doc.move_end(false);
        assert_eq!(doc.cursor_offset(), click);
        doc.place(5, false);
        doc.move_right_in_mode(false, ViewMode::Wysiwyg);
        assert_eq!(doc.cursor_offset(), click);
        doc.place(click, false);
        doc.delete_forward_in_mode(ViewMode::Wysiwyg);
        assert_eq!(doc.content, "## foo ##bar");
        let parsed = doc.parsed();
        assert_eq!(parsed.lines.len(), 1);
        assert_eq!(parsed.lines[0].display_text, "foo ##bar");
        assert!(matches!(
            parsed.lines[0].kind,
            LineKind::Heading { level: 2 }
        ));

        let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
        doc.place(click, false);
        doc.smart_newline(false);
        assert_eq!(doc.content, "## foo ##\n\nbar");
        let parsed = doc.parsed();
        assert_eq!(parsed.lines[0].display_text, "foo");
        assert_eq!(parsed.lines[1].display_text, "");
        assert_eq!(parsed.lines[2].display_text, "bar");
        assert!(matches!(
            parsed.lines[0].kind,
            LineKind::Heading { level: 2 }
        ));
        assert!(matches!(parsed.lines[1].kind, LineKind::Empty));
        assert!(matches!(parsed.lines[2].kind, LineKind::Paragraph));
    }

    /// Enter after `---` inserts a plain break. The other rows are not rules
    /// on BASE: a spaced dash or star line is a list, and the indented star
    /// line is a paragraph whose indent Enter copies.
    #[test]
    fn enter_on_spaced_rules_inserts_a_plain_break() {
        let src = "---\nnext";
        let end = {
            let parsed = parse_document(src);
            assert!(matches!(parsed.lines[0].kind, LineKind::HorizontalRule));
            assert_eq!(parsed.lines[0].display_text, "");
            parsed.lines[0].source.end
        };
        let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
        doc.place(end, false);
        doc.smart_newline(false);
        assert_eq!(doc.content, "---\n\nnext");
        let parsed = doc.parsed();
        assert!(matches!(parsed.lines[0].kind, LineKind::HorizontalRule));
        assert_eq!(parsed.lines[0].display_text, "");
        assert!(matches!(parsed.lines[1].kind, LineKind::Empty));
        assert_eq!(parsed.lines[1].display_text, "");
        assert_eq!(parsed.lines[2].display_text, "next");

        let src = "- - -\nnext";
        let end = parse_document(src).lines[0].source.end;
        let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
        doc.place(end, false);
        doc.smart_newline(false);
        assert_eq!(doc.content, "- - -\n- \nnext");
        let parsed = doc.parsed();
        assert!(matches!(
            parsed.lines[0].kind,
            LineKind::ListItem {
                ordered: false,
                indent_cols: 0,
                ..
            }
        ));
        assert_eq!(parsed.lines[0].display_text, "- -");
        assert!(matches!(
            parsed.lines[1].kind,
            LineKind::ListItem { ordered: false, .. }
        ));
        assert_eq!(parsed.lines[1].display_text, "");
        assert_eq!(parsed.lines[2].display_text, "next");

        let src = "* * *\nnext";
        let end = parse_document(src).lines[0].source.end;
        let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
        doc.place(end, false);
        doc.smart_newline(false);
        assert_eq!(doc.content, "* * *\n* \nnext");
        let parsed = doc.parsed();
        assert_eq!(parsed.lines[0].display_text, "* *");
        assert!(matches!(
            parsed.lines[0].kind,
            LineKind::ListItem {
                ordered: false,
                indent_cols: 0,
                ..
            }
        ));
        assert_eq!(parsed.lines[1].display_text, "");
        assert_eq!(parsed.lines[2].display_text, "next");

        let src = "-     -      -      -\nnext";
        let end = parse_document(src).lines[0].source.end;
        let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
        doc.place(end, false);
        doc.smart_newline(false);
        assert_eq!(doc.content, "-     -      -      -\n- \nnext");
        let parsed = doc.parsed();
        assert_eq!(parsed.lines[0].display_text, "    -      -      -");
        assert!(matches!(
            parsed.lines[0].kind,
            LineKind::ListItem {
                ordered: false,
                indent_cols: 0,
                ..
            }
        ));
        assert_eq!(parsed.lines[1].display_text, "");
        assert_eq!(parsed.lines[2].display_text, "next");

        let src = " **  * ** * ** * **\nnext";
        let end = parse_document(src).lines[0].source.end;
        let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
        doc.place(end, false);
        doc.smart_newline(false);
        assert_eq!(doc.content, " **  * ** * ** * **\n \nnext");
        let parsed = doc.parsed();
        assert!(matches!(parsed.lines[0].kind, LineKind::Paragraph));
        assert_eq!(parsed.lines[0].display_text, " **  * ** * ** * **");
        assert!(matches!(parsed.lines[1].kind, LineKind::Empty));
        assert_eq!(parsed.lines[1].display_text, " ");
        assert_eq!(parsed.lines[2].display_text, "next");
    }

    /// R5-3. Enter at offset 4 of `Foo\rBar` inserts a blank line between them.
    #[test]
    fn enter_at_start_of_bare_cr_line_inserts_a_line() {
        let mut doc = DocumentTab::new("t".into(), "t.md".into(), "Foo\rBar".into(), None, None);
        doc.place(4, false);
        assert_eq!(doc.cursor_offset(), 4);
        doc.smart_newline(false);
        assert_eq!(doc.content, "Foo\r\n\nBar");
        let parsed = doc.parsed();
        assert_eq!(parsed.lines.len(), 3);
        assert_eq!(parsed.lines[0].display_text, "Foo");
        assert_eq!(parsed.lines[1].display_text, "");
        assert_eq!(parsed.lines[2].display_text, "Bar");
        assert!(matches!(parsed.lines[1].kind, LineKind::Empty));
    }

    /// R6-2. Up from the line under a closing-hash heading lands on the
    /// visible end, not on the hidden `#`.
    #[test]
    fn up_onto_closing_hash_lands_on_the_visible_end() {
        let src = "# foo #\nnext!!!!";
        let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
        doc.place(src.len(), false);
        assert_eq!(doc.cursor_offset(), src.len());
        doc.move_up(false);
        assert_eq!(doc.cursor_offset(), 5);
        let at = doc.cursor_offset();
        doc.insert_text_in_mode(at..at, "X", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "# fooX #\nnext!!!!");
        let parsed = doc.parsed();
        assert_eq!(parsed.lines[0].display_text, "fooX");
        assert!(matches!(
            parsed.lines[0].kind,
            LineKind::Heading { level: 1 }
        ));
    }

    /// R6-3. Shift+Enter uses the same heading and bare-CR breaks as Enter.
    #[test]
    fn shift_enter_keeps_closing_hash_and_bare_cr_line() {
        let src = "# foo #\nnext";
        let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
        let click = {
            let line = &doc.parsed().lines[0];
            line.offset_map.display_to_source(line.display_text.len())
        };
        doc.place(click, false);
        doc.smart_newline(true);
        assert_eq!(doc.content, "# foo #\n\nnext");
        let parsed = doc.parsed();
        assert_eq!(parsed.lines[0].display_text, "foo");
        assert_eq!(parsed.lines[1].display_text, "");
        assert_eq!(parsed.lines[2].display_text, "next");
        assert!(matches!(
            parsed.lines[0].kind,
            LineKind::Heading { level: 1 }
        ));
        assert!(matches!(parsed.lines[1].kind, LineKind::Empty));

        let mut bare = DocumentTab::new("t".into(), "t.md".into(), "Foo\rBar".into(), None, None);
        bare.place(4, false);
        bare.smart_newline(true);
        assert_eq!(bare.content, "Foo\r\n\nBar");
        let parsed = bare.parsed();
        assert_eq!(parsed.lines.len(), 3);
        assert_eq!(parsed.lines[0].display_text, "Foo");
        assert_eq!(parsed.lines[1].display_text, "");
        assert_eq!(parsed.lines[2].display_text, "Bar");
    }

    /// R6-4. Toolbar toggles follow the visible heading text. A leftover
    /// closer stays only when the line is still a heading, so pulldown does
    /// not show the hash (`# foo ##`).
    #[test]
    fn toolbar_toggle_uses_visible_heading_text() {
        let src = "# foo #\nnext";
        let click_of = |text: &str| {
            let doc = parse_document(text);
            let line = &doc.lines[0];
            line.offset_map.display_to_source(line.display_text.len())
        };
        let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
        doc.place(click_of(src), false);
        doc.apply_format_in_mode("h1", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "foo\nnext");
        let parsed = doc.parsed();
        assert_eq!(parsed.lines[0].display_text, "foo");
        assert!(matches!(parsed.lines[0].kind, LineKind::Paragraph));
        assert_eq!(parsed.lines[1].display_text, "next");

        let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
        doc.place(click_of(src), false);
        doc.apply_format_in_mode("ul", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "- foo\nnext");
        assert_eq!(doc.parsed().lines[0].display_text, "foo");

        let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
        doc.place(click_of(src), false);
        doc.apply_format_in_mode("ol", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "1. foo\nnext");
        assert_eq!(doc.parsed().lines[0].display_text, "foo");

        let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
        doc.place(click_of(src), false);
        doc.apply_format_in_mode("quote", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "> foo\nnext");
        assert_eq!(doc.parsed().lines[0].display_text, "foo");

        let src = "## foo ##\nbar";
        let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
        doc.place(click_of(src), false);
        doc.apply_format_in_mode("h1", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "# foo ##\nbar");
        let parsed = doc.parsed();
        assert_eq!(parsed.lines[0].display_text, "foo");
        assert!(matches!(
            parsed.lines[0].kind,
            LineKind::Heading { level: 1 }
        ));
    }

    /// R6-5. Delete-to-end and delete-word-right at the visible end keep the
    /// hidden closer. A heading without a closer is unchanged by delete-to-end
    /// and loses the next word to delete-word-right.
    #[test]
    fn delete_to_end_and_word_right_keep_the_closing_hash() {
        let src = "# foo #\nnext";
        let click = {
            let doc = parse_document(src);
            let line = &doc.lines[0];
            line.offset_map.display_to_source(line.display_text.len())
        };
        assert_eq!(click, 5);

        let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
        doc.place(click, false);
        doc.delete_to_line_end();
        assert_eq!(doc.content, "# foo #\nnext");
        let parsed = doc.parsed();
        assert_eq!(parsed.lines[0].display_text, "foo");
        assert_eq!(parsed.lines[1].display_text, "next");

        let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
        doc.place(click, false);
        doc.delete_word_right();
        assert_eq!(doc.content, "# foo #\nnext");
        let parsed = doc.parsed();
        assert_eq!(parsed.lines[0].display_text, "foo");
        assert_eq!(parsed.lines[1].display_text, "next");

        let plain = "# foo\nnext";
        let mut doc = DocumentTab::new("t".into(), "t.md".into(), plain.into(), None, None);
        doc.place(5, false);
        doc.delete_to_line_end();
        assert_eq!(doc.content, "# foo\nnext");
        assert_eq!(doc.parsed().lines[0].display_text, "foo");

        let mut doc = DocumentTab::new("t".into(), "t.md".into(), plain.into(), None, None);
        doc.place(5, false);
        doc.delete_word_right();
        assert_eq!(doc.content, "# foo");
        assert_eq!(doc.parsed().lines[0].display_text, "foo");
    }

    /// K2. Select-line then Delete, and delete-to-line-start, at the visible
    /// end of `# foo #\nnext`. BASE select-line uses `line_extent` (the whole
    /// line, including the ending), so Delete leaves `next`. BASE
    /// delete-to-line-start at the source end (offset 7, the closer is
    /// visible there) leaves `\nnext`. The no-closer line matches both.
    #[test]
    fn select_line_delete_and_delete_to_line_start_match_base() {
        let src = "# foo #\nnext";
        let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
        doc.place(5, false);
        assert_eq!(doc.cursor_offset(), 5);
        doc.select_line();
        doc.delete_forward_in_mode(ViewMode::Wysiwyg);
        assert_eq!(doc.content, "next");
        let parsed = doc.parsed();
        assert_eq!(parsed.lines.len(), 1);
        assert_eq!(parsed.lines[0].display_text, "next");
        assert!(matches!(parsed.lines[0].kind, LineKind::Paragraph));

        let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
        doc.place(5, false);
        doc.delete_to_line_start();
        assert_eq!(doc.content, "\nnext");
        let parsed = doc.parsed();
        assert_eq!(parsed.lines[0].display_text, "");
        assert!(matches!(parsed.lines[0].kind, LineKind::Empty));
        assert_eq!(parsed.lines[1].display_text, "next");
        assert!(matches!(parsed.lines[1].kind, LineKind::Paragraph));

        let plain = "# foo\nnext";
        let mut doc = DocumentTab::new("t".into(), "t.md".into(), plain.into(), None, None);
        doc.place(5, false);
        doc.select_line();
        doc.delete_forward_in_mode(ViewMode::Wysiwyg);
        assert_eq!(doc.content, "next");

        let mut doc = DocumentTab::new("t".into(), "t.md".into(), plain.into(), None, None);
        doc.place(5, false);
        doc.delete_to_line_start();
        assert_eq!(doc.content, "\nnext");
    }

    /// K4. Raw paints the source, so End and place stay on the raw line end.
    /// BASE `place` does not snap and BASE End on `# foo #\nnext` is offset 7.
    /// Rendered mode still snaps that offset back to the visible end.
    #[test]
    fn raw_end_and_place_use_the_source_line_end() {
        let src = "# foo #\nnext";
        let mut rendered = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
        rendered.view_mode = ViewMode::Wysiwyg;
        rendered.place(0, false);
        rendered.move_end(false);
        assert_eq!(rendered.cursor_offset(), 5);
        rendered.place(7, false);
        assert_eq!(rendered.cursor_offset(), 5);

        let mut raw = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
        raw.view_mode = ViewMode::Raw;
        raw.place(0, false);
        raw.move_end(false);
        assert_eq!(raw.cursor_offset(), 7);
        raw.place(7, false);
        assert_eq!(raw.cursor_offset(), 7);

        let plain = "# foo\nnext";
        let mut raw_plain = DocumentTab::new("t".into(), "t.md".into(), plain.into(), None, None);
        raw_plain.view_mode = ViewMode::Raw;
        raw_plain.place(0, false);
        raw_plain.move_end(false);
        assert_eq!(raw_plain.cursor_offset(), 5);
        raw_plain.place(5, false);
        assert_eq!(raw_plain.cursor_offset(), 5);
    }

    /// K3. Down on the only painted row of `# foo #` must land on the visible
    /// end, the same snap `move_down` applies. Typing there keeps the closer.
    /// BASE types at the source end and produces `# foo #X`, because BASE does
    /// not hide the closer. The rendered snap inserts at offset 5.
    #[test]
    fn down_on_the_last_painted_row_snaps_before_the_closing_hash() {
        let src = "# foo #";
        let mut doc = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
        let (byte_start, byte_end, offset_map, vend) = {
            let parsed = doc.parsed();
            let line = &parsed.lines[0];
            assert_eq!(line.display_text, "foo");
            let vend = line.offset_map.display_to_source(line.display_text.len());
            (
                line.source.start,
                line.source.end,
                line.offset_map.clone(),
                vend,
            )
        };
        assert_eq!(vend, 5);
        assert_eq!(doc.content.len(), 7);
        let paint = EditorPaint {
            wrap_cols: usize::MAX,
            line_height: px(28.0),
            total_height: px(28.0),
            rows: vec![PaintRow {
                line_index: 0,
                byte_start,
                byte_end,
                origin: point(px(0.0), px(0.0)),
                line_height: px(28.0),
                shaped: ShapedLine::default(),
                offset_map,
                checkbox_hit: None,
                link_hits: Vec::new(),
                cells: Vec::new(),
            }],
            bounds: None,
            ..EditorPaint::default()
        };
        doc.caret.move_to(0);
        let hit = move_vertical_visual(
            &paint,
            &doc.content,
            &mut doc.caret,
            1,
            false,
            ViewMode::Wysiwyg,
        );
        assert!(hit);
        assert_eq!(doc.cursor_offset(), vend);
        let at = doc.cursor_offset();
        doc.insert_text_in_mode(at..at, "X", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "# fooX #");
        let parsed = doc.parsed();
        assert_eq!(parsed.lines[0].display_text, "fooX");
        assert!(matches!(
            parsed.lines[0].kind,
            LineKind::Heading { level: 1 }
        ));

        let mut raw = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
        raw.caret.move_to(0);
        let raw_hit = move_vertical_visual(
            &paint,
            &raw.content,
            &mut raw.caret,
            1,
            false,
            ViewMode::Raw,
        );
        assert!(raw_hit);
        assert_eq!(raw.cursor_offset(), src.len());
        let at = raw.cursor_offset();
        raw.insert_text_in_mode(at..at, "X", ViewMode::Raw);
        assert_eq!(raw.content, "# foo #X");

        let mut shift = DocumentTab::new("t".into(), "t.md".into(), src.into(), None, None);
        shift.caret.move_to(0);
        let shift_hit = move_vertical_visual(
            &paint,
            &shift.content,
            &mut shift.caret,
            1,
            true,
            ViewMode::Wysiwyg,
        );
        assert!(shift_hit);
        // BASE `1d03313` Shift+Down on this last row selects `0..7`, and
        // Delete clears the line. The collapsed Down above still types at
        // the visible end.
        assert_eq!(shift.caret.range, 0..7);
        shift.delete_forward_in_mode(ViewMode::Wysiwyg);
        assert_eq!(shift.content, "");
    }

    fn heading_tab(text: &str) -> DocumentTab {
        DocumentTab::new("t".into(), "t.md".into(), text.into(), None, None)
    }

    /// `active_tab_mut` and `editor_surface` call `set_view_mode` with the
    /// mode the tab already has, before a paint or an edit.
    fn app_touch(doc: &mut DocumentTab) {
        let mode = doc.view_mode;
        doc.set_view_mode(mode);
    }

    /// K1. Ancestor 7ad1cff rendered End on `# foo #` is offset 5, and typing
    /// X there yields `# fooX #` (display `fooX`).
    #[test]
    fn raw_end_then_rendered_type_keeps_the_closer() {
        let mut doc = heading_tab("# foo #");
        doc.set_view_mode(ViewMode::Raw);
        doc.place(0, false);
        doc.move_end(false);
        assert_eq!(doc.cursor_offset(), 7);
        doc.set_view_mode(ViewMode::Wysiwyg);
        let at = doc.cursor_offset();
        doc.insert_text_in_mode(at..at, "X", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "# fooX #");
        assert_eq!(doc.parsed().lines[0].display_text, "fooX");
    }

    /// Ancestor rendered End then Backspace yields `# fo #` (display `fo`).
    #[test]
    fn raw_end_then_rendered_backspace_deletes_visible_text() {
        let mut doc = heading_tab("# foo #");
        doc.set_view_mode(ViewMode::Raw);
        doc.place(0, false);
        doc.move_end(false);
        doc.set_view_mode(ViewMode::Wysiwyg);
        doc.smart_backspace_in_mode(ViewMode::Wysiwyg);
        assert_eq!(doc.content, "# fo #");
        assert_eq!(doc.parsed().lines[0].display_text, "fo");
    }

    /// Ancestor rendered End then Ctrl-Delete on `# foo #\nnext` leaves both lines.
    #[test]
    fn raw_end_then_rendered_ctrl_delete_keeps_the_next_line() {
        let mut doc = heading_tab("# foo #\nnext");
        doc.set_view_mode(ViewMode::Raw);
        doc.place(0, false);
        doc.move_end(false);
        assert_eq!(doc.cursor_offset(), 7);
        doc.set_view_mode(ViewMode::Wysiwyg);
        doc.delete_word_right();
        assert_eq!(doc.content, "# foo #\nnext");
        assert_eq!(doc.parsed().lines[0].display_text, "foo");
        assert_eq!(doc.parsed().lines[1].display_text, "next");
    }

    /// Raw End, type X (`# foo #X`), switch to rendered, undo, redo, then type Y.
    /// Ancestor rendered End then Y is `# fooY #`.
    #[test]
    fn undo_redo_after_raw_edit_then_rendered_types_at_the_visible_end() {
        let mut doc = heading_tab("# foo #");
        doc.set_view_mode(ViewMode::Raw);
        doc.place(0, false);
        doc.move_end(false);
        let at = doc.cursor_offset();
        doc.insert_text_in_mode(at..at, "X", ViewMode::Raw);
        assert_eq!(doc.content, "# foo #X");
        doc.set_view_mode(ViewMode::Wysiwyg);
        assert!(doc.undo());
        assert_eq!(doc.content, "# foo #");
        assert_eq!(doc.cursor_offset(), 5);
        assert!(doc.redo());
        assert_eq!(doc.content, "# foo #X");
        assert_eq!(doc.cursor_offset(), "# foo #X".len());
        assert!(doc.undo());
        assert_eq!(doc.cursor_offset(), 5);
        let at = doc.cursor_offset();
        doc.insert_text_in_mode(at..at, "Y", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "# fooY #");
        assert_eq!(doc.parsed().lines[0].display_text, "fooY");
    }

    /// A caret that is valid in rendered mode stays put when the view becomes Raw.
    #[test]
    fn rendered_to_raw_keeps_a_valid_caret() {
        let mut doc = heading_tab("# foo #\nnext");
        doc.set_view_mode(ViewMode::Wysiwyg);
        doc.place(0, false);
        doc.move_end(false);
        assert_eq!(doc.cursor_offset(), 5);
        doc.set_view_mode(ViewMode::Raw);
        assert_eq!(doc.cursor_offset(), 5);
        assert_eq!(doc.content, "# foo #\nnext");
        doc.place(2, false);
        doc.set_view_mode(ViewMode::Wysiwyg);
        assert_eq!(doc.cursor_offset(), 2);
        doc.place(0, false);
        doc.set_view_mode(ViewMode::Raw);
        assert_eq!(doc.cursor_offset(), 0);
    }

    /// BASE `1d03313` deletes these raw ranges. The switch to rendered must
    /// leave the range in place. `# foo #\nnext` `0..7` deletes to `\nnext`.
    /// `# foo #` `2..7` deletes to `# `. `7..12` deletes to `# foo #`.
    /// `6..7` deletes to `# foo \nnext`.
    #[test]
    fn raw_selection_then_rendered_snaps_anchor_and_caret() {
        let mut doc = heading_tab("# foo #\nnext");
        doc.set_view_mode(ViewMode::Raw);
        doc.place(0, false);
        doc.move_end(true);
        assert_eq!(doc.caret.range, 0..7);
        doc.set_view_mode(ViewMode::Wysiwyg);
        assert_eq!(doc.caret.range, 0..7);
        doc.delete_forward_in_mode(ViewMode::Wysiwyg);
        assert_eq!(doc.content, "\nnext");

        let mut doc = heading_tab("# foo #");
        doc.set_view_mode(ViewMode::Raw);
        doc.caret.range = 2..7;
        doc.caret.reversed = true;
        doc.set_view_mode(ViewMode::Wysiwyg);
        assert_eq!(doc.caret.range, 2..7);
        assert!(doc.caret.reversed);
        doc.delete_forward_in_mode(ViewMode::Wysiwyg);
        assert_eq!(doc.content, "# ");

        let mut doc = heading_tab("# foo #\nnext");
        doc.set_view_mode(ViewMode::Raw);
        doc.caret.range = 7..12;
        doc.set_view_mode(ViewMode::Wysiwyg);
        assert_eq!(doc.caret.range, 7..12);
        doc.delete_forward_in_mode(ViewMode::Wysiwyg);
        assert_eq!(doc.content, "# foo #");

        let mut doc = heading_tab("# foo #\nnext");
        doc.set_view_mode(ViewMode::Raw);
        doc.caret.range = 6..7;
        doc.set_view_mode(ViewMode::Wysiwyg);
        assert_eq!(doc.caret.range, 6..7);
        doc.delete_forward_in_mode(ViewMode::Wysiwyg);
        assert_eq!(doc.content, "# foo \nnext");
    }

    /// BASE `1d03313` select-all, then Delete, Backspace, type X, or cut, on a
    /// document that ends on a closing-hash heading. Delete clears the file.
    #[test]
    fn rendered_select_all_on_a_final_heading_matches_base() {
        for src in ["# foo #", "hello\n# Title #", "## foo ##", "### ###"] {
            let mut doc = heading_tab(src);
            doc.select_all();
            app_touch(&mut doc);
            doc.delete_forward_in_mode(ViewMode::Wysiwyg);
            assert_eq!(doc.content, "", "{src:?} delete");

            let mut doc = heading_tab(src);
            doc.select_all();
            app_touch(&mut doc);
            doc.smart_backspace_in_mode(ViewMode::Wysiwyg);
            assert_eq!(doc.content, "", "{src:?} backspace");

            let mut doc = heading_tab(src);
            doc.select_all();
            app_touch(&mut doc);
            let range = doc.selection_range();
            doc.insert_text_in_mode(range, "X", ViewMode::Wysiwyg);
            assert_eq!(doc.content, "X", "{src:?} type");

            let mut doc = heading_tab(src);
            doc.select_all();
            app_touch(&mut doc);
            let clip = doc.selected_text();
            let range = doc.selection_range();
            doc.edit(range, "");
            assert_eq!(clip, src, "{src:?} clip");
            assert_eq!(doc.content, "", "{src:?} cut");
        }
    }

    /// BASE `1d03313` select-line on the closing-hash heading, then the same
    /// four edits. A following `hello\n` line stays. A one-line heading clears.
    #[test]
    fn rendered_select_line_on_a_final_heading_matches_base() {
        for src in ["# foo #", "## foo ##", "### ###"] {
            let mut doc = heading_tab(src);
            doc.place(0, false);
            doc.select_line();
            app_touch(&mut doc);
            doc.delete_forward_in_mode(ViewMode::Wysiwyg);
            assert_eq!(doc.content, "", "{src:?} delete");

            let mut doc = heading_tab(src);
            doc.place(0, false);
            doc.select_line();
            app_touch(&mut doc);
            doc.smart_backspace_in_mode(ViewMode::Wysiwyg);
            assert_eq!(doc.content, "", "{src:?} backspace");

            let mut doc = heading_tab(src);
            doc.place(0, false);
            doc.select_line();
            app_touch(&mut doc);
            let range = doc.selection_range();
            doc.insert_text_in_mode(range, "X", ViewMode::Wysiwyg);
            assert_eq!(doc.content, "X", "{src:?} type");

            let mut doc = heading_tab(src);
            doc.place(0, false);
            doc.select_line();
            app_touch(&mut doc);
            let clip = doc.selected_text();
            let range = doc.selection_range();
            doc.edit(range, "");
            assert_eq!(clip, src, "{src:?} clip");
            assert_eq!(doc.content, "", "{src:?} cut");
        }

        let src = "hello\n# Title #";
        let at = src.rfind('\n').map(|index| index + 1).unwrap_or(0);
        let mut doc = heading_tab(src);
        doc.place(at, false);
        doc.select_line();
        app_touch(&mut doc);
        doc.delete_forward_in_mode(ViewMode::Wysiwyg);
        assert_eq!(doc.content, "hello\n");

        let mut doc = heading_tab(src);
        doc.place(at, false);
        doc.select_line();
        app_touch(&mut doc);
        doc.smart_backspace_in_mode(ViewMode::Wysiwyg);
        assert_eq!(doc.content, "hello\n");

        let mut doc = heading_tab(src);
        doc.place(at, false);
        doc.select_line();
        app_touch(&mut doc);
        let range = doc.selection_range();
        doc.insert_text_in_mode(range, "X", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "hello\nX");

        let mut doc = heading_tab(src);
        doc.place(at, false);
        doc.select_line();
        app_touch(&mut doc);
        let clip = doc.selected_text();
        let range = doc.selection_range();
        doc.edit(range, "");
        assert_eq!(clip, "# Title #");
        assert_eq!(doc.content, "hello\n");
    }

    fn delete_forward(doc: &mut DocumentTab) {
        doc.delete_forward_in_mode(ViewMode::Wysiwyg);
    }

    /// BASE `1d03313` `move_end(true)` then Delete. The selection runs to the
    /// source end, and on CRLF through the CR up to the LF.
    #[test]
    fn shift_end_extend_deletes_through_the_closing_hash() {
        let cases = [
            ("# foo #", ""),
            ("# foo #\nnext", "\nnext"),
            ("## foo ##", ""),
            ("## bar ##", ""),
            ("### ###", ""),
            ("# foo #   ", ""),
            ("# foo #\r\nnext", "\nnext"),
            ("# café #", ""),
            ("# 😀 #", ""),
        ];
        for (src, after) in cases {
            let mut doc = heading_tab(src);
            doc.move_end(true);
            delete_forward(&mut doc);
            assert_eq!(doc.content, after, "end {src:?}");
        }

        let homes = [
            ("# foo #", "# "),
            ("# foo #\nnext", "# \nnext"),
            ("## foo ##", "## "),
            ("### ###", "### "),
            ("# foo #   ", "# "),
            ("# café #", "# "),
            ("# 😀 #", "# "),
        ];
        for (src, after) in homes {
            let mut doc = heading_tab(src);
            doc.move_home_in_mode(false, ViewMode::Wysiwyg);
            doc.move_end(true);
            delete_forward(&mut doc);
            assert_eq!(doc.content, after, "home-end {src:?}");
        }

        let src = "hello\n# foo #";
        let mut doc = heading_tab(src);
        doc.place(6, false);
        doc.move_end(true);
        delete_forward(&mut doc);
        assert_eq!(doc.content, "hello\n");
    }

    /// BASE `1d03313` `place` does not pull an extend target back to the
    /// visible end. Mouse drag and Ctrl+Shift+End both `place` the offset.
    #[test]
    fn place_extend_onto_the_closing_hash_deletes_it() {
        let mut doc = heading_tab("# foo #");
        doc.place(0, false);
        doc.place(7, true);
        assert_eq!(doc.caret.range, 0..7);
        delete_forward(&mut doc);
        assert_eq!(doc.content, "");

        let mut doc = heading_tab("# foo #\nnext");
        doc.place(0, false);
        doc.place(7, true);
        delete_forward(&mut doc);
        assert_eq!(doc.content, "\nnext");

        let mut doc = heading_tab("# foo #");
        doc.place(0, false);
        doc.place(6, true);
        delete_forward(&mut doc);
        assert_eq!(doc.content, "#");

        let mut doc = heading_tab("# foo #");
        doc.place(5, false);
        doc.place(7, true);
        delete_forward(&mut doc);
        assert_eq!(doc.content, "# foo");

        let src = "hello\n# Title #";
        let mut doc = heading_tab(src);
        doc.place(0, false);
        doc.place(src.len(), true);
        delete_forward(&mut doc);
        assert_eq!(doc.content, "");

        let mut doc = heading_tab("# foo #");
        doc.place(5, false);
        doc.move_word_right(true);
        delete_forward(&mut doc);
        assert_eq!(doc.content, "# foo");
    }

    /// K2. BASE `1d03313` Ctrl+Shift+Right from the visible end selects the
    /// hidden closer and stops there; it does not run on into the next word.
    #[test]
    fn move_word_right_from_the_visible_end_stops_at_the_closer() {
        let mut doc = heading_tab("# foo #\nnext");
        doc.place(5, false);
        doc.move_word_right(true);
        assert_eq!(doc.caret.range, 5..7);
        assert_eq!(doc.selected_text(), " #");
        delete_forward(&mut doc);
        assert_eq!(doc.content, "# foo\nnext");

        let mut doc = heading_tab("# foo #\n# bar #");
        doc.place(5, false);
        for _ in 0..3 {
            doc.move_word_right(true);
        }
        assert_eq!(doc.caret.range, 5..13);
        assert_eq!(doc.selected_text(), " #\n# bar");
        delete_forward(&mut doc);
        assert_eq!(doc.content, "# foo #");

        let mut doc = heading_tab("# foo #");
        doc.place(5, false);
        doc.move_word_right(true);
        assert_eq!(doc.caret.range, 5..7);
        delete_forward(&mut doc);
        assert_eq!(doc.content, "# foo");
    }

    fn paint_rows(doc: &mut DocumentTab) -> EditorPaint {
        let parsed = doc.parsed();
        let rows = parsed
            .lines
            .iter()
            .enumerate()
            .map(|(ix, line)| PaintRow {
                line_index: ix,
                byte_start: line.source.start,
                byte_end: line.source.end,
                origin: point(px(0.0), px(28.0 * ix as f32)),
                line_height: px(28.0),
                shaped: ShapedLine::default(),
                offset_map: line.offset_map.clone(),
                checkbox_hit: None,
                link_hits: Vec::new(),
                cells: Vec::new(),
            })
            .collect();
        let n = parsed.lines.len() as f32;
        EditorPaint {
            wrap_cols: usize::MAX,
            line_height: px(28.0),
            total_height: px(28.0 * n),
            rows,
            bounds: None,
            ..EditorPaint::default()
        }
    }

    /// BASE `1d03313` painted Shift+Down on the last row selects through the
    /// closer. Delete then matches End.
    #[test]
    fn painted_shift_down_on_the_last_row_deletes_the_closing_hash() {
        let mut doc = heading_tab("## bar ##");
        let paint = paint_rows(&mut doc);
        doc.caret.move_to(0);
        assert!(move_vertical_visual(
            &paint,
            &doc.content,
            &mut doc.caret,
            1,
            true,
            ViewMode::Wysiwyg,
        ));
        assert_eq!(doc.caret.range, 0..9);
        delete_forward(&mut doc);
        assert_eq!(doc.content, "");

        let src = "hello\n# foo #";
        let mut doc = heading_tab(src);
        let paint = paint_rows(&mut doc);
        doc.place(6, false);
        assert!(move_vertical_visual(
            &paint,
            &doc.content,
            &mut doc.caret,
            1,
            true,
            ViewMode::Wysiwyg,
        ));
        delete_forward(&mut doc);
        assert_eq!(doc.content, "hello\n");
    }

    /// BASE `1d03313` Ctrl+Shift+Down selects the source line, closer included.
    #[test]
    fn select_line_down_extends_through_the_closing_hash() {
        let mut doc = heading_tab("# foo #");
        doc.select_line_down();
        assert_eq!(doc.caret.range, 0..7);
        assert_eq!(doc.selected_text(), "# foo #");
        doc.select_line_down();
        assert_eq!(doc.caret.range, 0..7);
        delete_forward(&mut doc);
        assert_eq!(doc.content, "");

        let mut doc = heading_tab("# foo #\nnext");
        doc.select_line_down();
        assert_eq!(doc.caret.range, 0..7);
        assert_eq!(doc.selected_text(), "# foo #");
        delete_forward(&mut doc);
        assert_eq!(doc.content, "\nnext");

        let mut doc = heading_tab("## bar ##");
        doc.select_line_down();
        assert_eq!(doc.selected_text(), "## bar ##");
        delete_forward(&mut doc);
        assert_eq!(doc.content, "");

        let mut doc = heading_tab("# foo #\r\nnext");
        doc.select_line_down();
        assert_eq!(doc.caret.range, 0..8);
        delete_forward(&mut doc);
        assert_eq!(doc.content, "\nnext");
    }

    /// K1. BASE `1d03313` Ctrl+Shift+Down on the second press includes the
    /// CRLF, so Delete leaves the LF behind.
    #[test]
    fn second_select_line_down_includes_the_crlf() {
        let mut doc = heading_tab("alpha\r\nbeta\r\ngamma");
        doc.select_line_down();
        doc.select_line_down();
        assert_eq!(doc.caret.range, 0..12);
        assert_eq!(doc.selected_text(), "alpha\r\nbeta\r");
        delete_forward(&mut doc);
        assert_eq!(doc.content, "\ngamma");

        let mut doc = heading_tab("# foo #\r\n# bar #\r\nnext");
        doc.select_line_down();
        doc.select_line_down();
        assert_eq!(doc.caret.range, 0..17);
        delete_forward(&mut doc);
        assert_eq!(doc.content, "\nnext");

        let mut doc = heading_tab("alpha\nbeta\ngamma");
        doc.select_line_down();
        doc.select_line_down();
        assert_eq!(doc.caret.range, 0..10);
        delete_forward(&mut doc);
        assert_eq!(doc.content, "\ngamma");

        let mut doc = heading_tab("# foo #\n# bar #\nnext");
        doc.select_line_down();
        doc.select_line_down();
        assert_eq!(doc.caret.range, 0..15);
        delete_forward(&mut doc);
        assert_eq!(doc.content, "\nnext");
    }

    /// BASE `1d03313` Ctrl+Shift+Delete from before the visible end removes the
    /// closer. A caret already on the visible end stays a no-op
    /// (`delete_to_end_and_word_right_keep_the_closing_hash`).
    #[test]
    fn delete_to_line_end_from_before_the_closer_matches_base() {
        let mut doc = heading_tab("# foo #");
        doc.place(0, false);
        doc.delete_to_line_end();
        assert_eq!(doc.content, "");

        let mut doc = heading_tab("# foo #");
        doc.move_home_in_mode(false, ViewMode::Wysiwyg);
        doc.delete_to_line_end();
        assert_eq!(doc.content, "# ");

        let mut doc = heading_tab("# foo #\nnext");
        doc.place(0, false);
        doc.delete_to_line_end();
        assert_eq!(doc.content, "\nnext");

        let mut doc = heading_tab("# foo #\nnext");
        doc.move_home_in_mode(false, ViewMode::Wysiwyg);
        doc.delete_to_line_end();
        assert_eq!(doc.content, "# \nnext");

        let mut doc = heading_tab("# foo #\r\nnext");
        doc.place(0, false);
        doc.delete_to_line_end();
        assert_eq!(doc.content, "\nnext");

        let mut doc = heading_tab("# foo #\r\nnext");
        doc.move_home_in_mode(false, ViewMode::Wysiwyg);
        doc.delete_to_line_end();
        assert_eq!(doc.content, "# \nnext");
    }

    /// BASE `1d03313` Shift+Right then Shift+Left on `5..7` types `# fooX\nnext`.
    /// The place extend snap is what drops the closer.
    #[test]
    fn shift_right_then_left_on_a_closer_selection_types_over_it() {
        let mut doc = heading_tab("# foo #\nnext");
        doc.caret.range = 5..7;
        app_touch(&mut doc);
        doc.move_right_in_mode(true, ViewMode::Wysiwyg);
        doc.move_left_in_mode(true, ViewMode::Wysiwyg);
        let range = doc.selection_range();
        doc.insert_text_in_mode(range, "X", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "# fooX\nnext");
    }

    /// BASE `1d03313` Shift+Left then Shift+Right on `0..7` types `X\nnext`.
    #[test]
    fn shift_left_then_right_on_a_full_heading_selection_matches_base() {
        let mut doc = heading_tab("# foo #\nnext");
        doc.caret.range = 0..7;
        app_touch(&mut doc);
        doc.move_left_in_mode(true, ViewMode::Wysiwyg);
        doc.move_right_in_mode(true, ViewMode::Wysiwyg);
        let range = doc.selection_range();
        doc.insert_text_in_mode(range, "X", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "X\nnext");
    }

    /// 7ad1cff paints `# foo ` as `foo` and its visible end is offset 5. The
    /// trailing space is part of the visible heading now, so the snap target
    /// is offset 6 and typing X there appends after it.
    #[test]
    fn redo_after_raw_closer_delete_types_at_the_visible_end() {
        let mut doc = heading_tab("# foo #");
        doc.set_view_mode(ViewMode::Raw);
        doc.caret.range = 6..7;
        doc.delete_forward_in_mode(ViewMode::Raw);
        assert!(doc.undo());
        doc.set_view_mode(ViewMode::Wysiwyg);
        assert!(doc.redo());
        let range = doc.selection_range();
        doc.insert_text_in_mode(range, "X", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "# foo X");
        assert_eq!(doc.caret.range, 7..7);
    }

    #[test]
    fn test_format_commands_table_select_all_and_toggle() {
        let original_table = "| Header 1 | Header 2 |\n| :--- | :--- |\n| Cell 1 | Cell 2 |";

        let inline_commands = [
            (
                "bold",
                "| **Header 1** | **Header 2** |\n| :--- | :--- |\n| **Cell 1** | **Cell 2** |",
            ),
            (
                "italic",
                "| *Header 1* | *Header 2* |\n| :--- | :--- |\n| *Cell 1* | *Cell 2* |",
            ),
            (
                "underline",
                "| <u>Header 1</u> | <u>Header 2</u> |\n| :--- | :--- |\n| <u>Cell 1</u> | <u>Cell 2</u> |",
            ),
            (
                "strike",
                "| ~~Header 1~~ | ~~Header 2~~ |\n| :--- | :--- |\n| ~~Cell 1~~ | ~~Cell 2~~ |",
            ),
            (
                "code",
                "| `Header 1` | `Header 2` |\n| :--- | :--- |\n| `Cell 1` | `Cell 2` |",
            ),
            (
                "link",
                "| [Header 1](https://) | [Header 2](https://) |\n| :--- | :--- |\n| [Cell 1](https://) | [Cell 2](https://) |",
            ),
        ];

        for (cmd, expected_formatted) in inline_commands {
            // Test Raw mode
            let mut doc = DocumentTab::new(
                "tab".into(),
                "test.md".into(),
                original_table.into(),
                None,
                None,
            );
            doc.select_all();
            doc.apply_format_in_mode(cmd, ViewMode::Raw);
            assert_eq!(
                doc.content, expected_formatted,
                "Inline format '{}' failed on table in Raw mode",
                cmd
            );

            // Toggle off by applying again
            doc.apply_format_in_mode(cmd, ViewMode::Raw);
            assert_eq!(
                doc.content, original_table,
                "Inline format '{}' toggle-off failed on table in Raw mode",
                cmd
            );

            // Verify one undo step restores original
            doc.apply_format_in_mode(cmd, ViewMode::Raw);
            assert_eq!(doc.content, expected_formatted);
            assert!(doc.undo(), "Undo failed for '{}'", cmd);
            assert_eq!(
                doc.content, original_table,
                "Single undo step failed for '{}'",
                cmd
            );

            // Test WYSIWYG mode
            let mut doc_wysiwyg = DocumentTab::new(
                "tab".into(),
                "test.md".into(),
                original_table.into(),
                None,
                None,
            );
            doc_wysiwyg.select_all();
            doc_wysiwyg.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
            assert_eq!(
                doc_wysiwyg.content, expected_formatted,
                "Inline format '{}' failed on table in Wysiwyg mode",
                cmd
            );

            doc_wysiwyg.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
            assert_eq!(
                doc_wysiwyg.content, original_table,
                "Inline format '{}' toggle-off failed on table in Wysiwyg mode",
                cmd
            );
        }

        let line_commands = [
            "h1",
            "h2",
            "h3",
            "ul",
            "ol",
            "checklist",
            "quote",
            "codeblock",
        ];

        for cmd in line_commands {
            let mut doc = DocumentTab::new(
                "tab".into(),
                "test.md".into(),
                original_table.into(),
                None,
                None,
            );
            doc.select_all();
            doc.apply_format_in_mode(cmd, ViewMode::Raw);
            assert_eq!(
                doc.content, original_table,
                "Line format '{}' should skip table rows and preserve table",
                cmd
            );

            doc.apply_format_in_mode(cmd, ViewMode::Raw);
            assert_eq!(
                doc.content, original_table,
                "Line format '{}' second apply should keep table intact",
                cmd
            );

            // Also in WYSIWYG mode
            let mut doc_wysiwyg = DocumentTab::new(
                "tab".into(),
                "test.md".into(),
                original_table.into(),
                None,
                None,
            );
            doc_wysiwyg.select_all();
            doc_wysiwyg.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
            assert_eq!(
                doc_wysiwyg.content, original_table,
                "Line format '{}' in Wysiwyg should skip table rows",
                cmd
            );
        }
    }

    #[test]
    fn test_format_commands_plain_lines_select_all_and_toggle() {
        let original_plain = "First line\nSecond line\nThird line";

        let inline_commands = [
            ("bold", "**First line**\n**Second line**\n**Third line**"),
            ("italic", "*First line*\n*Second line*\n*Third line*"),
            (
                "underline",
                "<u>First line</u>\n<u>Second line</u>\n<u>Third line</u>",
            ),
            ("strike", "~~First line~~\n~~Second line~~\n~~Third line~~"),
            ("code", "`First line`\n`Second line`\n`Third line`"),
            (
                "link",
                "[First line](https://)\n[Second line](https://)\n[Third line](https://)",
            ),
        ];

        for (cmd, expected_formatted) in inline_commands {
            let mut doc = DocumentTab::new(
                "tab".into(),
                "test.md".into(),
                original_plain.into(),
                None,
                None,
            );
            doc.select_all();
            doc.apply_format_in_mode(cmd, ViewMode::Raw);
            assert_eq!(
                doc.content, expected_formatted,
                "Inline format '{}' failed across plain lines",
                cmd
            );

            // Toggle off
            doc.apply_format_in_mode(cmd, ViewMode::Raw);
            assert_eq!(
                doc.content, original_plain,
                "Inline format '{}' toggle-off failed across plain lines",
                cmd
            );

            // Undo as one step
            doc.apply_format_in_mode(cmd, ViewMode::Raw);
            assert_eq!(doc.content, expected_formatted);
            assert!(doc.undo(), "Undo failed for '{}'", cmd);
            assert_eq!(
                doc.content, original_plain,
                "Single undo step failed for '{}' across plain lines",
                cmd
            );
        }

        let line_commands = [
            ("h1", "# First line\n# Second line\n# Third line"),
            ("h2", "## First line\n## Second line\n## Third line"),
            ("h3", "### First line\n### Second line\n### Third line"),
            ("ul", "- First line\n- Second line\n- Third line"),
            ("ol", "1. First line\n2. Second line\n3. Third line"),
            (
                "checklist",
                "- [ ] First line\n- [ ] Second line\n- [ ] Third line",
            ),
            ("quote", "> First line\n> Second line\n> Third line"),
            ("codeblock", "```\nFirst line\nSecond line\nThird line\n```"),
        ];

        for (cmd, expected_formatted) in line_commands {
            let mut doc = DocumentTab::new(
                "tab".into(),
                "test.md".into(),
                original_plain.into(),
                None,
                None,
            );
            doc.select_all();
            doc.apply_format_in_mode(cmd, ViewMode::Raw);
            assert_eq!(
                doc.content, expected_formatted,
                "Line format '{}' failed across plain lines",
                cmd
            );

            // Toggle off
            doc.apply_format_in_mode(cmd, ViewMode::Raw);
            assert_eq!(
                doc.content, original_plain,
                "Line format '{}' toggle-off failed across plain lines",
                cmd
            );

            // Undo
            doc.apply_format_in_mode(cmd, ViewMode::Raw);
            assert_eq!(doc.content, expected_formatted);
            assert!(doc.undo(), "Undo failed for '{}'", cmd);
            assert_eq!(
                doc.content, original_plain,
                "Undo failed for '{}' across plain lines",
                cmd
            );
        }
    }

    #[test]
    fn test_format_commands_mixed_plain_lines_and_table() {
        let mixed =
            "Before table\n\n| Col 1 | Col 2 |\n| :--- | :--- |\n| Val 1 | Val 2 |\n\nAfter table";

        // Applying h1 affects lines before and after table, but skips the table rows
        let mut doc = DocumentTab::new("tab".into(), "test.md".into(), mixed.into(), None, None);
        doc.select_all();
        doc.apply_format("h1");
        let expected_h1 = "# Before table\n\n| Col 1 | Col 2 |\n| :--- | :--- |\n| Val 1 | Val 2 |\n\n# After table";
        assert_eq!(doc.content, expected_h1);

        // Toggle off restores
        doc.apply_format("h1");
        assert_eq!(doc.content, mixed);

        // Applying bold formats plain lines and table cells individually
        let mut doc_bold =
            DocumentTab::new("tab".into(), "test.md".into(), mixed.into(), None, None);
        doc_bold.select_all();
        doc_bold.apply_format("bold");
        let expected_bold = "**Before table**\n\n| **Col 1** | **Col 2** |\n| :--- | :--- |\n| **Val 1** | **Val 2** |\n\n**After table**";
        assert_eq!(doc_bold.content, expected_bold);

        // Toggle off restores
        doc_bold.apply_format("bold");
        assert_eq!(doc_bold.content, mixed);
    }

    #[test]
    fn test_inline_format_partial_selection_across_cells() {
        let doc_text = "| Alpha Beta | Gamma Delta |\n| :--- | :--- |";
        let mut doc = DocumentTab::new("tab".into(), "test.md".into(), doc_text.into(), None, None);

        // Select from "Beta" to "Gamma"
        let start = doc_text.find("Beta").unwrap();
        let end = doc_text.find("Delta").unwrap();
        doc.caret.range = start..end;
        doc.apply_format("bold");
        assert_eq!(
            doc.content,
            "| Alpha **Beta** | **Gamma** Delta |\n| :--- | :--- |"
        );

        // Toggle off
        doc.apply_format("bold");
        assert_eq!(doc.content, doc_text);
    }

    #[test]
    fn test_unwrap_outer_link_safety() {
        // Valid link
        let text = "before [inner](https://example.com) after";
        let inner_start = text.find("inner").unwrap();
        let inner_end = inner_start + "inner".len();
        assert_eq!(
            unwrap_outer_link(text, inner_start, inner_end),
            Some((
                text.find('[').unwrap()..(text.find(')').unwrap() + 1),
                "inner".to_string()
            ))
        );

        // Missing closing parenthesis must return None without panicking
        let unclosed = "before [inner](https://example.com after";
        let u_start = unclosed.find("inner").unwrap();
        let u_end = u_start + "inner".len();
        assert_eq!(unwrap_outer_link(unclosed, u_start, u_end), None);

        // Out of bounds / invalid bounds must return None without panicking
        assert_eq!(unwrap_outer_link(text, 0, text.len()), None);
        assert_eq!(unwrap_outer_link(text, 100, 200), None);
        assert_eq!(unwrap_outer_link("", 0, 0), None);
    }

    fn table_doc(text: &str) -> DocumentTab {
        DocumentTab::new("t".into(), "test.md".into(), text.into(), None, None)
    }

    const TBL: &str = "| A | B |\n| --- | --- |\n| a1 | b1 |\n| a2 | b2 |";

    fn line_kind_at(doc: &DocumentTab, needle: &str) -> LineKind {
        let at = doc.content.find(needle).unwrap();
        crate::markdown::parse_document(&doc.content)
            .line_at_offset(at)
            .unwrap()
            .kind
            .clone()
    }

    #[test]
    fn table_exit_leaves_blank_line_and_next_text_is_a_paragraph() {
        let mut doc = table_doc(TBL);
        doc.place(TBL.len(), false);
        doc.smart_newline(false); // adds a row
        doc.smart_newline(false); // exits
        assert_eq!(doc.content, format!("{TBL}\n\n"));
        assert_eq!(doc.cursor_offset(), doc.content.len());
        let at = doc.cursor_offset();
        doc.insert_text_in_mode(at..at, "hello", ViewMode::Wysiwyg);
        assert!(matches!(line_kind_at(&doc, "hello"), LineKind::Paragraph));
        assert!(matches!(
            line_kind_at(&doc, "b2"),
            LineKind::TableRow { .. }
        ));
    }

    #[test]
    fn table_exit_reuses_an_existing_blank_line() {
        let text = format!("{TBL}\n|  |  |\n\nafter");
        let mut doc = table_doc(&text);
        doc.place(text.find("|  |").unwrap() + 1, false);
        doc.smart_newline(false);
        assert_eq!(doc.content, format!("{TBL}\n\nafter"));
        assert_eq!(doc.cursor_offset(), TBL.len() + 2);
        // A directly adjacent paragraph gets a blank line inserted
        let text = format!("{TBL}\n|  |  |\n# after");
        let mut doc = table_doc(&text);
        doc.place(text.find("|  |").unwrap() + 1, false);
        doc.smart_newline(false);
        assert_eq!(doc.content, format!("{TBL}\n\n# after"));
        assert_eq!(doc.cursor_offset(), TBL.len() + 2);
        // one undo step
        assert!(doc.undo());
        assert_eq!(doc.content, text);
    }

    #[test]
    fn enter_on_empty_middle_row_moves_to_same_column_next_row() {
        let text = "| A | B |\n| --- | --- |\n|  |  |\n| x | y |";
        let mut doc = table_doc(text);
        // caret in the second (empty) cell of the empty row
        let row = text.find("|  |  |").unwrap();
        doc.place(row + 4, false);
        doc.smart_newline(false);
        assert_eq!(doc.content, text);
        assert_eq!(doc.cursor_offset(), text.find("y |").unwrap());
        // first column
        doc.place(row + 1, false);
        doc.smart_newline(false);
        assert_eq!(doc.content, text);
        assert_eq!(doc.cursor_offset(), text.find("x |").unwrap());
    }

    #[test]
    fn shift_enter_leaves_the_table_without_changing_it() {
        for pos in [0usize, TBL.find("a1").unwrap() + 1, TBL.find("b2").unwrap() + 2] {
            let mut doc = table_doc(TBL);
            doc.place(pos, false);
            doc.smart_newline(true);
            assert_eq!(doc.content, format!("{TBL}\n\n"));
            assert_eq!(doc.cursor_offset(), doc.content.len());
        }
        // existing text below keeps one blank line
        let text = format!("{TBL}\n# next");
        let mut doc = table_doc(&text);
        doc.place(text.find("a1").unwrap(), false);
        doc.smart_newline(true);
        assert_eq!(doc.content, format!("{TBL}\n\n# next"));
        // outside tables Shift+Enter is a plain newline
        let mut doc = table_doc("hello world");
        doc.place(5, false);
        doc.smart_newline(true);
        assert_eq!(doc.content, "hello\n world");
    }

    #[test]
    fn ctrl_enter_adds_a_row_below_in_the_same_column() {
        let mut doc = table_doc(TBL);
        let pos = TBL.find("b1").unwrap() + 1;
        doc.place(pos, false);
        assert!(doc.table_insert_row_below());
        let expected = "| A | B |\n| --- | --- |\n| a1 | b1 |\n|  |  |\n| a2 | b2 |";
        assert_eq!(doc.content, expected);
        assert_eq!(doc.cursor_offset(), expected.find("|  |  |").unwrap() + 5);
        // header row inserts below the delimiter
        let mut doc = table_doc(TBL);
        doc.place(2, false);
        assert!(doc.table_insert_row_below());
        assert!(doc.content.starts_with("| A | B |\n| --- | --- |\n|  |  |\n| a1"));
        assert_eq!(doc.cursor_offset(), doc.content.find("|  |  |").unwrap() + 1 + 2 - 1);
        // outside a table nothing happens
        let mut doc = table_doc("plain");
        assert!(!doc.table_insert_row_below());
        assert_eq!(doc.content, "plain");
    }

    #[test]
    fn up_and_down_leave_the_table_from_its_edge_rows() {
        // Down from the last row of a table that ends the note
        let mut doc = table_doc(TBL);
        doc.place(TBL.len() - 2, false);
        assert!(doc.table_edge_exit(1));
        assert_eq!(doc.content, format!("{TBL}\n\n"));
        assert_eq!(doc.cursor_offset(), doc.content.len());
        // Up from the first row of a table that starts the note
        let mut doc = table_doc(TBL);
        doc.place(3, false);
        assert!(doc.table_edge_exit(-1));
        assert_eq!(doc.content, format!("\n\n{TBL}"));
        assert_eq!(doc.cursor_offset(), 0);
        // With a line above and below the caret just moves there
        let text = format!("above\n\n{TBL}\n\nbelow");
        let mut doc = table_doc(&text);
        doc.place(text.find("A |").unwrap(), false);
        assert!(doc.table_edge_exit(-1));
        assert_eq!(doc.cursor_offset(), "above\n".len());
        doc.place(text.find("b2").unwrap(), false);
        assert!(doc.table_edge_exit(1));
        assert_eq!(doc.cursor_offset(), text.find("below").unwrap() - 1);
        // Middle rows are not edges
        doc.place(text.find("a1").unwrap(), false);
        assert!(!doc.table_edge_exit(1));
        assert!(!doc.table_edge_exit(-1));
    }

    #[test]
    fn left_and_right_jump_between_cells_not_pipes() {
        let mut doc = table_doc(TBL);
        let a1 = TBL.find("a1").unwrap();
        let b1 = TBL.find("b1").unwrap();
        // Left at the start of B1 goes to the end of A1
        doc.place(b1, false);
        doc.move_left_in_mode(false, ViewMode::Wysiwyg);
        assert_eq!(doc.cursor_offset(), a1 + 2);
        // Right at the end of A1 goes to the start of B1
        doc.move_right_in_mode(false, ViewMode::Wysiwyg);
        assert_eq!(doc.cursor_offset(), b1);
        // End of a row goes to the next row's first cell, start goes to previous row's last
        doc.place(b1 + 2, false);
        doc.move_right_in_mode(false, ViewMode::Wysiwyg);
        assert_eq!(doc.cursor_offset(), TBL.find("a2").unwrap());
        doc.move_left_in_mode(false, ViewMode::Wysiwyg);
        assert_eq!(doc.cursor_offset(), b1 + 2);
        // Walking right through the first body row never stops next to a pipe
        doc.place(a1, false);
        for _ in 0..4 {
            doc.move_right_in_mode(false, ViewMode::Wysiwyg);
            let c = doc.cursor_offset();
            assert_ne!(doc.content.as_bytes()[c], b'|', "caret before a pipe at {c}");
            assert_ne!(doc.content.as_bytes()[c - 1], b'|', "caret after a pipe at {c}");
        }
    }

    #[test]
    fn shift_tab_in_the_first_cell_stays_put() {
        let mut doc = table_doc(TBL);
        let a = TBL.find('A').unwrap();
        doc.place(a, false);
        assert!(doc.tab_table_cell(true));
        assert_eq!(doc.cursor_offset(), a);
        assert_eq!(doc.content, TBL);
        // elsewhere it still goes back one cell
        let b = TBL.find('B').unwrap();
        doc.place(b, false);
        assert!(doc.tab_table_cell(true));
        assert_eq!(doc.selection_range().start, a);
    }

    #[test]
    fn test_inline_format_stacked_combos_table_driven() {
        let combos: Vec<(&[&str], &str)> = vec![
            // 2-format combos (6)
            (&["bold", "italic"], "***Hello***"),
            (&["bold", "underline"], "<u>**Hello**</u>"),
            (&["bold", "strike"], "**~~Hello~~**"),
            (&["italic", "underline"], "<u>*Hello*</u>"),
            (&["italic", "strike"], "*~~Hello~~*"),
            (&["strike", "underline"], "<u>~~Hello~~</u>"),
            // 3-format combos (4)
            (&["bold", "italic", "underline"], "<u>***Hello***</u>"),
            (&["bold", "italic", "strike"], "***~~Hello~~***"),
            (&["bold", "strike", "underline"], "<u>**~~Hello~~**</u>"),
            (&["italic", "strike", "underline"], "<u>*~~Hello~~*</u>"),
        ];

        let positions = ["whole", "start", "middle", "end", "caret"];

        for (combo_formats, initial_md) in &combos {
            for &cmd in *combo_formats {
                for &pos in &positions {
                    let mut doc = DocumentTab::new(
                        "tab".into(),
                        "test.md".into(),
                        initial_md.to_string(),
                        None,
                        None,
                    );

                    let word_start = doc.content.find("Hello").unwrap();
                    match pos {
                        "whole" => {
                            doc.caret.range = word_start..(word_start + 5);
                        }
                        "start" => {
                            doc.caret.range = word_start..(word_start + 2);
                        }
                        "middle" => {
                            doc.caret.range = (word_start + 1)..(word_start + 4);
                        }
                        "end" => {
                            doc.caret.range = (word_start + 2)..(word_start + 5);
                        }
                        "caret" => {
                            doc.place(word_start + 2, false);
                        }
                        _ => unreachable!(),
                    }

                    doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);

                    let parsed = doc.parsed();
                    let line = parsed.lines.first().expect("Line not found");

                    // 1. No leaked markers in display text
                    assert_eq!(
                        line.display_text, "Hello",
                        "Display text leaked markers for combo {:?}, format {}, pos {}. Content was: {}",
                        combo_formats, cmd, pos, doc.content
                    );

                    // 2. Clean delimiters: no **** or <u><u> or ~~~~
                    assert!(
                        !doc.content.contains("****"),
                        "Content contains **** for combo {:?}, format {}, pos {}: {}",
                        combo_formats, cmd, pos, doc.content
                    );
                    assert!(
                        !doc.content.contains("<u><u>"),
                        "Content contains <u><u> for combo {:?}, format {}, pos {}: {}",
                        combo_formats, cmd, pos, doc.content
                    );
                    assert!(
                        !doc.content.contains("</u></u>"),
                        "Content contains </u></u> for combo {:?}, format {}, pos {}: {}",
                        combo_formats, cmd, pos, doc.content
                    );
                    assert!(
                        !doc.content.contains("~~~~"),
                        "Content contains ~~~~ for combo {:?}, format {}, pos {}: {}",
                        combo_formats, cmd, pos, doc.content
                    );

                    // 3. Verify format state of each letter in "Hello"
                    for i in 0..5 {
                        let should_have_toggled = match pos {
                            "whole" | "caret" => false,
                            "start" => i >= 2,
                            "middle" => i == 0 || i == 4,
                            "end" => i < 2,
                            _ => unreachable!(),
                        };

                        let src_offset = line.offset_map.display_to_source(i);
                        let seg = line
                            .styled_segments
                            .iter()
                            .find(|s| s.source.start <= src_offset && src_offset < s.source.end)
                            .unwrap_or_else(|| {
                                panic!(
                                    "No styled segment covering char index {} at source offset {} for combo {:?}, format {}, pos {}. Content: {}",
                                    i, src_offset, combo_formats, cmd, pos, doc.content
                                )
                            });

                        let has_cmd = match cmd {
                            "bold" => seg.bold,
                            "italic" => seg.italic,
                            "underline" => seg.underline,
                            "strike" => seg.strike,
                            _ => unreachable!(),
                        };
                        assert_eq!(
                            has_cmd, should_have_toggled,
                            "Format {} on char index {} expected {} but was {} for combo {:?}, pos {}. Content: {}",
                            cmd, i, should_have_toggled, has_cmd, combo_formats, pos, doc.content
                        );

                        // All other formats in combo_formats must remain true
                        for &other_cmd in *combo_formats {
                            if other_cmd == cmd {
                                continue;
                            }
                            let has_other = match other_cmd {
                                "bold" => seg.bold,
                                "italic" => seg.italic,
                                "underline" => seg.underline,
                                "strike" => seg.strike,
                                _ => unreachable!(),
                            };
                            assert!(
                                has_other,
                                "Other format {} on char index {} lost for combo {:?}, toggled {}, pos {}. Content: {}",
                                other_cmd, i, combo_formats, cmd, pos, doc.content
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn test_wysiwyg_typing_format_splice_inner_outer_plain() {
        // Arm bold + italic, type "now", turn off italic, type "ok", turn off bold, type " plain"
        let mut doc = DocumentTab::new("t1".into(), "test.md".into(), String::new(), None, None);
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "now");
        assert_eq!(doc.content, "***now***");
        doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "ok");
        assert_eq!(doc.content, "***now*ok**");
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, " plain");
        assert_eq!(doc.content, "***now*ok** plain");
        let parsed = doc.parsed();
        let line = parsed.lines.first().unwrap();
        assert_eq!(line.display_text, "nowok plain");

        // Arm bold + italic, type "now", turn off bold (outer), type "ok", turn off italic, type " plain"
        let mut doc = DocumentTab::new("t2".into(), "test.md".into(), String::new(), None, None);
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "now");
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "ok");
        assert_eq!(doc.content, "***now**ok*");
        doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, " plain");
        assert_eq!(doc.content, "***now**ok* plain");
        let parsed = doc.parsed();
        let line = parsed.lines.first().unwrap();
        assert_eq!(line.display_text, "nowok plain");

        // Arm bold + underline, type "now", turn off underline (inner), type "ok", turn off bold, type " plain"
        let mut doc = DocumentTab::new("t3".into(), "test.md".into(), String::new(), None, None);
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "now");
        assert_eq!(doc.content, "<u>**now**</u>");
        doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "ok");
        assert_eq!(doc.content, "**<u>now</u>ok**");
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, " plain");
        assert_eq!(doc.content, "**<u>now</u>ok** plain");
        let parsed = doc.parsed();
        let line = parsed.lines.first().unwrap();
        assert_eq!(line.display_text, "nowok plain");

        // Arm bold + underline, type "now", turn off bold (outer), type "ok", turn off underline, type " plain"
        let mut doc = DocumentTab::new("t4".into(), "test.md".into(), String::new(), None, None);
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "now");
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "ok");
        assert_eq!(doc.content, "<u>**now**ok</u>");
        doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, " plain");
        assert_eq!(doc.content, "<u>**now**ok</u> plain");
        let parsed = doc.parsed();
        let line = parsed.lines.first().unwrap();
        assert_eq!(line.display_text, "nowok plain");
    }

    #[test]
    fn test_wysiwyg_typing_req5_consecutive_italic_toggle() {
        let mut doc = DocumentTab::new("t".into(), "test.md".into(), String::new(), None, None);
        type_keystrokes(&mut doc, "a ");
        doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "b");
        doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
        doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "c");
        doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, " d");
        assert_eq!(doc.content, "a *bc* d");
        let line = doc.parsed().lines.first().unwrap().clone();
        assert_eq!(line.display_text, "a bc d");
    }

    #[test]
    fn test_wysiwyg_typing_req6_underline_and_code_trailing_space() {
        let mut doc = DocumentTab::new("u".into(), "test.md".into(), String::new(), None, None);
        doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "hello ");
        doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "world");
        assert_eq!(doc.content, "<u>hello</u> world");
        let line = doc.parsed().lines.first().unwrap().clone();
        assert_eq!(line.display_text, "hello world");

        let mut doc = DocumentTab::new("c".into(), "test.md".into(), String::new(), None, None);
        doc.apply_format_in_mode("code", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "hello ");
        doc.apply_format_in_mode("code", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "world");
        assert_eq!(doc.content, "`hello` world");
        let line = doc.parsed().lines.first().unwrap().clone();
        assert_eq!(line.display_text, "hello world");
    }

    #[test]
    fn test_wysiwyg_repro1_stacked_format_typing_continuation() {
        let mut doc = DocumentTab::new("r1".into(), "test.md".into(), String::new(), None, None);
        type_keystrokes(&mut doc, "This is a test for ");
        doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "combination i want ");
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "jlkj ");
        doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);

        assert_eq!(
            doc.content,
            "This is a test for <u>***combination i want** jlkj*</u> "
        );
        let parsed = crate::markdown::parse_document(&doc.content);
        let line = parsed.lines.first().unwrap();
        assert_eq!(line.display_text, "This is a test for combination i want jlkj ");

        let s_lead = line.styled_segments.iter().find(|s| s.source == (0..19)).unwrap();
        assert!(!s_lead.bold && !s_lead.italic && !s_lead.underline && !s_lead.strike);

        let s_combo = line.styled_segments.iter().find(|s| &doc.content[s.source.clone()] == "combination i want").unwrap();
        assert!(s_combo.bold && s_combo.italic && s_combo.underline);

        let s_jlkj = line.styled_segments.iter().find(|s| &doc.content[s.source.clone()] == " jlkj").unwrap();
        assert!(!s_jlkj.bold && s_jlkj.italic && s_jlkj.underline);
    }

    #[test]
    fn test_wysiwyg_repro2_stacked_format_typing_disarm_plain() {
        let mut doc = DocumentTab::new("r2".into(), "test.md".into(), String::new(), None, None);
        doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "combination i want");
        doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "fasjj");

        assert_eq!(doc.content, "<u>***combination i want***</u>fasjj");
        let parsed = crate::markdown::parse_document(&doc.content);
        let line = parsed.lines.first().unwrap();
        assert_eq!(line.display_text, "combination i wantfasjj");

        let s_formatted = line.styled_segments.iter().find(|s| &doc.content[s.source.clone()] == "combination i want").unwrap();
        assert!(s_formatted.bold && s_formatted.italic && s_formatted.underline && !s_formatted.strike);

        let s_plain = line.styled_segments.iter().find(|s| &doc.content[s.source.clone()] == "fasjj").unwrap();
        assert!(!s_plain.bold && !s_plain.italic && !s_plain.underline && !s_plain.strike);
    }

    #[test]
    fn test_all_stacked_combos_followed_by_unspaced_text() {
        let combos = [
            // Pairs
            (vec!["bold", "italic"], true, true, false, false),
            (vec!["bold", "underline"], true, false, true, false),
            (vec!["italic", "underline"], false, true, true, false),
            (vec!["strike", "underline"], false, false, true, true),
            (vec!["bold", "strike"], true, false, false, true),
            (vec!["italic", "strike"], false, true, false, true),
            // Triples
            (vec!["bold", "italic", "underline"], true, true, true, false),
            (vec!["bold", "strike", "underline"], true, false, true, true),
            (vec!["italic", "strike", "underline"], false, true, true, true),
            (vec!["bold", "italic", "strike"], true, true, false, true),
        ];

        for (fmts, b, i, u, s) in combos {
            let mut doc = DocumentTab::new("c".into(), "test.md".into(), String::new(), None, None);
            for &cmd in &fmts {
                doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
            }
            type_keystrokes(&mut doc, "word");
            for &cmd in &fmts {
                doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
            }
            type_keystrokes(&mut doc, "after");

            // Never emit empty format spans
            assert!(
                !doc.content.contains("<u></u>"),
                "Found empty <u></u> for {:?}: {:?}",
                fmts,
                doc.content
            );
            assert!(
                !doc.content.contains("****"),
                "Found empty **** for {:?}: {:?}",
                fmts,
                doc.content
            );
            assert!(
                !doc.content.contains("~~~~"),
                "Found empty ~~~~ for {:?}: {:?}",
                fmts,
                doc.content
            );

            // Combos with underline or single delimiter run (bold+italic) parse cleanly even without space
            if u || fmts == ["bold", "italic"] {
                let parsed = crate::markdown::parse_document(&doc.content);
                let line = parsed.lines.first().unwrap();
                assert_eq!(
                    line.display_text, "wordafter",
                    "Leaked delimiters for combo {:?}. Content was: {:?}",
                    fmts, doc.content
                );

                let seg_word = line
                    .styled_segments
                    .iter()
                    .find(|seg| &doc.content[seg.source.clone()] == "word")
                    .unwrap_or_else(|| panic!("Segment 'word' not found for {:?}, content: {:?}", fmts, doc.content));
                assert_eq!(seg_word.bold, b, "{:?}: bold mismatch", fmts);
                assert_eq!(seg_word.italic, i, "{:?}: italic mismatch", fmts);
                assert_eq!(seg_word.underline, u, "{:?}: underline mismatch", fmts);
                assert_eq!(seg_word.strike, s, "{:?}: strike mismatch", fmts);

                let seg_after = line
                    .styled_segments
                    .iter()
                    .find(|seg| &doc.content[seg.source.clone()] == "after")
                    .unwrap_or_else(|| panic!("Segment 'after' not found for {:?}, content: {:?}", fmts, doc.content));
                assert!(
                    !seg_after.bold && !seg_after.italic && !seg_after.underline && !seg_after.strike,
                    "{:?}: after was styled",
                    fmts
                );
            }
        }
    }

    #[test]
    fn test_arrowing_back_into_stacked_span_and_typing() {
        let mut doc = DocumentTab::new("arrow".into(), "test.md".into(), String::new(), None, None);
        doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "hello");
        doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, " world");

        // Arrow left 7 times back into the stacked span
        for _ in 0..7 {
            doc.move_left_in_mode(false, ViewMode::Wysiwyg);
        }
        type_keystrokes(&mut doc, "X");

        let parsed = crate::markdown::parse_document(&doc.content);
        let line = parsed.lines.first().unwrap();
        assert_eq!(line.display_text, "hellXo world");

        let seg_mod = line
            .styled_segments
            .iter()
            .find(|seg| &doc.content[seg.source.clone()] == "hellXo")
            .expect("hellXo segment not found");
        assert!(seg_mod.bold && seg_mod.italic && seg_mod.underline && !seg_mod.strike);

        let seg_world = line
            .styled_segments
            .iter()
            .find(|seg| &doc.content[seg.source.clone()] == " world")
            .expect("world segment not found");
        assert!(!seg_world.bold && !seg_world.italic && !seg_world.underline && !seg_world.strike);
    }

    #[test]
    fn test_b_i_s_c_pairs_and_triples_toggled_off_and_on_adjacent_to_letters() {
        let combos: Vec<(&[&str], bool, bool, bool, bool)> = vec![
            // Pairs (6)
            (&["bold", "italic"], true, true, false, false),
            (&["bold", "strike"], true, false, true, false),
            (&["bold", "code"], true, false, false, true),
            (&["italic", "strike"], false, true, true, false),
            (&["italic", "code"], false, true, false, true),
            (&["strike", "code"], false, false, true, true),
            // Triples (4)
            (&["bold", "italic", "strike"], true, true, true, false),
            (&["bold", "italic", "code"], true, true, false, true),
            (&["bold", "strike", "code"], true, false, true, true),
            (&["italic", "strike", "code"], false, true, true, true),
        ];

        for (combo, exp_b, exp_i, exp_s, exp_c) in combos {
            // 1. Toggled on immediately after a letter: type "a", arm combo, type "hi"
            {
                let mut doc = DocumentTab::new("on".into(), "test.md".into(), String::new(), None, None);
                type_keystrokes(&mut doc, "a");
                for &cmd in combo {
                    doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
                }
                type_keystrokes(&mut doc, "hi");

                let parsed = crate::markdown::parse_document(&doc.content);
                let line = parsed.lines.first().expect("line not found");
                assert_eq!(
                    line.display_text, "ahi",
                    "Leaked delimiters in display text when toggling on combo {:?}. Content was: {:?}",
                    combo, doc.content
                );

                // 'a' must be unstyled
                let a_src = line.offset_map.display_to_source(0);
                let a_seg = line
                    .styled_segments
                    .iter()
                    .find(|s| s.source.start <= a_src && a_src < s.source.end)
                    .unwrap_or_else(|| panic!("No styled segment covering 'a' for combo {:?}", combo));
                assert!(!a_seg.bold && !a_seg.italic && !a_seg.strike && !a_seg.code,
                    "Expected 'a' unstyled for combo {:?}, got B={} I={} S={} C={}",
                    combo, a_seg.bold, a_seg.italic, a_seg.strike, a_seg.code);

                // 'h' and 'i' must have the combo formats
                for ch_idx in 1..3 {
                    let src = line.offset_map.display_to_source(ch_idx);
                    let seg = line
                        .styled_segments
                        .iter()
                        .find(|s| s.source.start <= src && src < s.source.end)
                        .unwrap_or_else(|| panic!("No styled segment covering char {} for combo {:?}", ch_idx, combo));
                    assert_eq!(seg.bold, exp_b, "Bold mismatch on char {} for combo ON {:?}. Content: {:?}", ch_idx, combo, doc.content);
                    assert_eq!(seg.italic, exp_i, "Italic mismatch on char {} for combo ON {:?}. Content: {:?}", ch_idx, combo, doc.content);
                    assert_eq!(seg.strike, exp_s, "Strike mismatch on char {} for combo ON {:?}. Content: {:?}", ch_idx, combo, doc.content);
                    assert_eq!(seg.code, exp_c, "Code mismatch on char {} for combo ON {:?}. Content: {:?}", ch_idx, combo, doc.content);
                }
            }

            // 2. Toggled off with a letter typed immediately after: arm combo, type "hi", toggle off, type "y"
            {
                let mut doc = DocumentTab::new("off".into(), "test.md".into(), String::new(), None, None);
                for &cmd in combo {
                    doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
                }
                type_keystrokes(&mut doc, "hi");
                for &cmd in combo {
                    doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
                }
                type_keystrokes(&mut doc, "y");

                let parsed = crate::markdown::parse_document(&doc.content);
                let line = parsed.lines.first().expect("line not found");
                assert_eq!(
                    line.display_text, "hiy",
                    "Leaked delimiters in display text when toggling off combo {:?}. Content was: {:?}",
                    combo, doc.content
                );

                // 'h' and 'i' must have the combo formats
                for ch_idx in 0..2 {
                    let src = line.offset_map.display_to_source(ch_idx);
                    let seg = line
                        .styled_segments
                        .iter()
                        .find(|s| s.source.start <= src && src < s.source.end)
                        .unwrap_or_else(|| panic!("No styled segment covering char {} for combo {:?}", ch_idx, combo));
                    assert_eq!(seg.bold, exp_b, "Bold mismatch on char {} for combo OFF {:?}. Content: {:?}", ch_idx, combo, doc.content);
                    assert_eq!(seg.italic, exp_i, "Italic mismatch on char {} for combo OFF {:?}. Content: {:?}", ch_idx, combo, doc.content);
                    assert_eq!(seg.strike, exp_s, "Strike mismatch on char {} for combo OFF {:?}. Content: {:?}", ch_idx, combo, doc.content);
                    assert_eq!(seg.code, exp_c, "Code mismatch on char {} for combo OFF {:?}. Content: {:?}", ch_idx, combo, doc.content);
                }

                // 'y' must be unstyled
                let y_src = line.offset_map.display_to_source(2);
                let y_seg = line
                    .styled_segments
                    .iter()
                    .find(|s| s.source.start <= y_src && y_src < s.source.end)
                    .unwrap_or_else(|| panic!("No styled segment covering 'y' for combo {:?}", combo));
                assert!(!y_seg.bold && !y_seg.italic && !y_seg.strike && !y_seg.code,
                    "Expected 'y' unstyled for combo {:?}, got B={} I={} S={} C={}",
                    combo, y_seg.bold, y_seg.italic, y_seg.strike, y_seg.code);
            }

            // 3. With prefix "x ": "x ", arm combo, type "hi", toggle off, type "y"
            {
                let mut doc = DocumentTab::new("prefix_off".into(), "test.md".into(), String::new(), None, None);
                type_keystrokes(&mut doc, "x ");
                for &cmd in combo {
                    doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
                }
                type_keystrokes(&mut doc, "hi");
                for &cmd in combo {
                    doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
                }
                type_keystrokes(&mut doc, "y");

                let parsed = crate::markdown::parse_document(&doc.content);
                let line = parsed.lines.first().expect("line not found");
                assert_eq!(
                    line.display_text, "x hiy",
                    "Leaked delimiters in display text when toggling off with prefix combo {:?}. Content was: {:?}",
                    combo, doc.content
                );

                // 'x' and ' ' must be unstyled
                for ch_idx in 0..2 {
                    let src = line.offset_map.display_to_source(ch_idx);
                    let seg = line
                        .styled_segments
                        .iter()
                        .find(|s| s.source.start <= src && src < s.source.end)
                        .unwrap_or_else(|| panic!("No styled segment covering char {} for combo {:?}", ch_idx, combo));
                    assert!(!seg.bold && !seg.italic && !seg.strike && !seg.code);
                }

                // 'h' and 'i' must have the combo formats
                for ch_idx in 2..4 {
                    let src = line.offset_map.display_to_source(ch_idx);
                    let seg = line
                        .styled_segments
                        .iter()
                        .find(|s| s.source.start <= src && src < s.source.end)
                        .unwrap_or_else(|| panic!("No styled segment covering char {} for combo {:?}", ch_idx, combo));
                    assert_eq!(seg.bold, exp_b, "Bold mismatch on char {} for combo OFF {:?}. Content: {:?}", ch_idx, combo, doc.content);
                    assert_eq!(seg.italic, exp_i, "Italic mismatch on char {} for combo OFF {:?}. Content: {:?}", ch_idx, combo, doc.content);
                    assert_eq!(seg.strike, exp_s, "Strike mismatch on char {} for combo OFF {:?}. Content: {:?}", ch_idx, combo, doc.content);
                    assert_eq!(seg.code, exp_c, "Code mismatch on char {} for combo OFF {:?}. Content: {:?}", ch_idx, combo, doc.content);
                }

                // 'y' must be unstyled
                let y_src = line.offset_map.display_to_source(4);
                let y_seg = line
                    .styled_segments
                    .iter()
                    .find(|s| s.source.start <= y_src && y_src < s.source.end)
                    .unwrap_or_else(|| panic!("No styled segment covering 'y' for combo {:?}", combo));
                assert!(!y_seg.bold && !y_seg.italic && !y_seg.strike && !y_seg.code,
                    "Expected 'y' unstyled for combo {:?}, got B={} I={} S={} C={}",
                    combo, y_seg.bold, y_seg.italic, y_seg.strike, y_seg.code);
            }
        }
    }

    #[test]
    fn test_whitespace_only_inserts_with_each_format_armed() {
        let whitespaces = [" ", "\t", "   ", "  \t  "];
        let formats = [
            ("bold", true, false, false, false, false, "**word**"),
            ("italic", false, true, false, false, false, "*word*"),
            ("underline", false, false, true, false, false, "<u>word</u>"),
            ("strike", false, false, false, true, false, "~~word~~"),
            ("code", false, false, false, false, true, "`word`"),
        ];

        for ws in whitespaces {
            for (fmt, b, i, u, s, c, expected_span) in formats {
                // 1. In empty doc: verify plain insertion & pending formats preserved
                let mut doc =
                    DocumentTab::new("ws".into(), "test.md".into(), String::new(), None, None);
                doc.apply_format_in_mode(fmt, ViewMode::Wysiwyg);
                assert!(
                    doc.has_pending_inline_formats(),
                    "Format {} should be armed before inserting {:?}",
                    fmt,
                    ws
                );

                doc.insert_text_in_mode(0..0, ws, ViewMode::Wysiwyg);
                assert_eq!(
                    doc.content, ws,
                    "Whitespace {:?} with format {} should be inserted plainly",
                    ws, fmt
                );
                assert!(
                    doc.has_pending_inline_formats(),
                    "Format {} should remain armed after inserting whitespace {:?}",
                    fmt,
                    ws
                );

                type_keystrokes(&mut doc, "word");
                assert_eq!(
                    doc.content,
                    format!("{}{}", ws, expected_span),
                    "Typed text should wrap with armed format delimiters"
                );

                // 2. In paragraph (with prefix text): verify parse_document styled segments
                let mut doc2 =
                    DocumentTab::new("ws2".into(), "test.md".into(), "intro".into(), None, None);
                doc2.caret.move_to(5);
                doc2.apply_format_in_mode(fmt, ViewMode::Wysiwyg);
                doc2.insert_text_in_mode(5..5, ws, ViewMode::Wysiwyg);
                assert!(doc2.has_pending_inline_formats());
                type_keystrokes(&mut doc2, "word");

                let parsed = crate::markdown::parse_document(&doc2.content);
                let line = parsed.lines.first().unwrap();
                let seg = line
                    .styled_segments
                    .iter()
                    .find(|seg| &doc2.content[seg.source.clone()] == "word")
                    .unwrap_or_else(|| {
                        panic!(
                            "Segment 'word' not found for format {} with whitespace {:?}, content: {:?}",
                            fmt, ws, doc2.content
                        )
                    });
                assert_eq!(seg.bold, b, "{}: bold mismatch", fmt);
                assert_eq!(seg.italic, i, "{}: italic mismatch", fmt);
                assert_eq!(seg.underline, u, "{}: underline mismatch", fmt);
                assert_eq!(seg.strike, s, "{}: strike mismatch", fmt);
                assert_eq!(seg.code, c, "{}: code mismatch", fmt);
            }
        }
    }

    #[test]
    fn test_whitespace_only_inserts_in_table_cell_with_each_format_armed() {
        let whitespaces = [" ", "\t", "   ", "  \t  "];
        let formats = ["bold", "italic", "underline", "strike", "code"];

        for ws in whitespaces {
            for fmt in formats {
                let initial = "| x |\n| --- |\n";
                let mut doc = DocumentTab::new(
                    "table_ws".into(),
                    "test.md".into(),
                    initial.to_string(),
                    None,
                    None,
                );
                // Position caret right after 'x' at index 3 in "| x |"
                doc.caret.move_to(3);
                doc.apply_format_in_mode(fmt, ViewMode::Wysiwyg);
                assert!(
                    doc.has_pending_inline_formats(),
                    "Format {} should be armed in table cell",
                    fmt
                );

                // Inserting whitespace-only text must not panic
                doc.insert_text_in_mode(3..3, ws, ViewMode::Wysiwyg);
                assert!(
                    doc.has_pending_inline_formats(),
                    "Format {} should remain armed after whitespace insert in table cell",
                    fmt
                );
                assert_eq!(
                    &doc.content[..3 + ws.len()],
                    format!("| x{}", ws),
                    "Whitespace must be inserted plainly inside table cell"
                );

                // Now type text and ensure it receives the armed format
                type_keystrokes(&mut doc, "word");
                let parsed = crate::markdown::parse_document(&doc.content);
                let line = parsed.lines.first().unwrap();
                let seg = line
                    .styled_segments
                    .iter()
                    .find(|seg| &doc.content[seg.source.clone()] == "word")
                    .unwrap_or_else(|| {
                        panic!(
                            "Segment 'word' not found in table for format {} with ws {:?}, content: {:?}",
                            fmt, ws, doc.content
                        )
                    });
                match fmt {
                    "bold" => assert!(seg.bold),
                    "italic" => assert!(seg.italic),
                    "underline" => assert!(seg.underline),
                    "strike" => assert!(seg.strike),
                    "code" => assert!(seg.code),
                    _ => unreachable!(),
                }
            }
        }
    }

    #[test]
    fn test_no_empty_format_spans_after_toggling() {
        // Repro: x *~~hi~~*y
        let mut doc = DocumentTab::new("repro".into(), "test.md".into(), String::new(), None, None);
        type_keystrokes(&mut doc, "x ");
        doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
        doc.apply_format_in_mode("strike", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "hi");
        doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
        doc.apply_format_in_mode("strike", ViewMode::Wysiwyg);
        type_keystrokes(&mut doc, "y");

        assert_eq!(doc.content, "x ~~*hi*~~y");
        assert!(!doc.content.contains("<u></u>"));
        assert!(!doc.content.contains("****"));
        assert!(!doc.content.contains("~~~~"));

        // Toggle on and immediately toggle off for each format
        let formats = ["bold", "italic", "underline", "strike", "code"];
        for fmt in formats {
            let mut doc = DocumentTab::new("empty_toggle".into(), "test.md".into(), String::new(), None, None);
            doc.apply_format_in_mode(fmt, ViewMode::Wysiwyg);
            doc.apply_format_in_mode(fmt, ViewMode::Wysiwyg);
            assert_eq!(doc.content, "");
            assert!(!doc.content.contains("<u></u>"));
            assert!(!doc.content.contains("****"));
            assert!(!doc.content.contains("~~~~"));
            assert!(!doc.has_pending_inline_formats());
        }

        // Type word, toggle off, type word for each format
        for fmt in formats {
            let mut doc = DocumentTab::new("toggle_seq".into(), "test.md".into(), String::new(), None, None);
            doc.apply_format_in_mode(fmt, ViewMode::Wysiwyg);
            type_keystrokes(&mut doc, "abc");
            doc.apply_format_in_mode(fmt, ViewMode::Wysiwyg);
            type_keystrokes(&mut doc, "def");

            assert!(!doc.content.contains("<u></u>"), "Format {} left <u></u>: {:?}", fmt, doc.content);
            assert!(!doc.content.contains("****"), "Format {} left ****: {:?}", fmt, doc.content);
            assert!(!doc.content.contains("~~~~"), "Format {} left ~~~~: {:?}", fmt, doc.content);
        }
    }

    const PERF_LINE: &str =
        "Some words **bold text** and *it* plus <u>under</u> ~~st~~ `c` [link](http://x) 😀 tiếng\n";

    fn naive_span_close_end(parsed: &ParsedDocument, pair: &InlinePair) -> usize {
        parsed
            .inline_pairs
            .iter()
            .filter(|p| p.kind != InlineTagKind::Link && p.inner == pair.inner)
            .map(|p| p.close_delim.end)
            .max()
            .unwrap_or(pair.close_delim.end)
    }

    fn naive_span_open_start(parsed: &ParsedDocument, pair: &InlinePair) -> usize {
        parsed
            .inline_pairs
            .iter()
            .filter(|p| p.kind != InlineTagKind::Link && p.inner == pair.inner)
            .map(|p| p.open_delim.start)
            .min()
            .unwrap_or(pair.open_delim.start)
    }

    fn naive_collect_group_pairs(
        parsed: &ParsedDocument,
        range: Range<usize>,
    ) -> Vec<&InlinePair> {
        let mut group_pairs: Vec<&InlinePair> = parsed
            .inline_pairs
            .iter()
            .filter(|p| {
                p.kind != InlineTagKind::Link
                    && p.open_delim.start < range.end
                    && naive_span_close_end(parsed, p) > range.start
            })
            .collect();
        let mut expanded = true;
        while expanded {
            expanded = false;
            for p in &parsed.inline_pairs {
                if p.kind == InlineTagKind::Link
                    || group_pairs.iter().any(|g| std::ptr::eq(*g, p))
                {
                    continue;
                }
                let touches = group_pairs.iter().any(|g| {
                    p.open_delim.start < naive_span_close_end(parsed, g)
                        && naive_span_close_end(parsed, p) > g.open_delim.start
                });
                if touches {
                    group_pairs.push(p);
                    expanded = true;
                }
            }
        }
        group_pairs
    }

    fn pair_key(
        p: &InlinePair,
    ) -> (
        InlineTagKind,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
    ) {
        (
            p.kind,
            p.open_delim.start,
            p.open_delim.end,
            p.inner.start,
            p.inner.end,
            p.close_delim.start,
            p.close_delim.end,
        )
    }

    fn assert_helper_equivalence(parsed: &ParsedDocument) {
        for pair in &parsed.inline_pairs {
            assert_eq!(
                span_close_end(parsed, pair),
                naive_span_close_end(parsed, pair),
                "group_close_end mismatch for {pair:?}"
            );
            assert_eq!(
                span_open_start(parsed, pair),
                naive_span_open_start(parsed, pair),
                "group_open_start mismatch for {pair:?}"
            );
            assert_eq!(pair.group_close_end, naive_span_close_end(parsed, pair));
            assert_eq!(pair.group_open_start, naive_span_open_start(parsed, pair));
        }
        let n = parsed.inline_pairs.len().max(1);
        let step = (parsed.lines.last().map(|l| l.source.end).unwrap_or(0) / n.max(1)).max(1);
        for start in (0..=parsed.lines.last().map(|l| l.source.end).unwrap_or(0)).step_by(step.max(7)) {
            let end = (start + 11).min(parsed.lines.last().map(|l| l.source.end).unwrap_or(start));
            let mut naive: Vec<_> = naive_collect_group_pairs(parsed, start..end)
                .into_iter()
                .map(pair_key)
                .collect();
            let mut fast: Vec<_> = collect_group_pairs(parsed, start..end)
                .into_iter()
                .map(pair_key)
                .collect();
            naive.sort();
            fast.sort();
            assert_eq!(naive, fast, "group_pairs mismatch at {start}..{end}");
        }
    }

    fn mixed_perf_doc(n: usize) -> String {
        let mut s = String::with_capacity(n * PERF_LINE.len() + 200);
        s.push_str("# Title\n\n");
        s.push_str("- list **item**\n\n");
        s.push_str("```\ncode\n```\n\n");
        s.push_str("| a | b |\n| --- | --- |\n| **c** | d |\n\n");
        for _ in 0..n {
            s.push_str(PERF_LINE);
        }
        s
    }

    #[test]
    fn precomputed_span_bounds_match_naive_scan() {
        let content = mixed_perf_doc(8);
        let parsed = parse_document(&content);
        assert_helper_equivalence(&parsed);
        let mid = content.len() / 2;
        assert_eq!(
            parsed.line_at_offset(mid).map(|l| l.line_index),
            parsed
                .lines
                .iter()
                .find(|l| mid >= l.source.start && mid <= l.source.end)
                .map(|l| l.line_index)
        );
    }

    fn snapshot(doc: &DocumentTab) -> (String, Range<usize>, ActiveFormats, ActiveFormats) {
        (
            doc.content.clone(),
            doc.caret.range.clone(),
            doc.active_formats,
            doc.pending_formats,
        )
    }

    fn run_scripted_format_sequence(n: usize) -> Vec<(String, Range<usize>, ActiveFormats, ActiveFormats)> {
        let mut doc = DocumentTab::new(
            "perf".into(),
            "perf.md".into(),
            mixed_perf_doc(n),
            None,
            None,
        );
        let mut out = vec![snapshot(&doc)];
        let mid = doc.content.len() / 2;
        doc.place(mid, false);
        out.push(snapshot(&doc));
        assert_helper_equivalence(&doc.parsed());

        type_keystrokes(&mut doc, "Z");
        out.push(snapshot(&doc));
        assert_helper_equivalence(&doc.parsed());

        for cmd in ["bold", "italic", "underline", "strike", "code", "link"] {
            doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
            out.push(snapshot(&doc));
            type_keystrokes(&mut doc, "a");
            out.push(snapshot(&doc));
            doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
            out.push(snapshot(&doc));
            assert_helper_equivalence(&doc.parsed());
        }

        let start = doc.cursor_offset().saturating_sub(8);
        let end = (doc.cursor_offset() + 8).min(doc.content.len());
        doc.caret.range = start..end;
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        out.push(snapshot(&doc));
        doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
        out.push(snapshot(&doc));

        doc.smart_newline(false);
        out.push(snapshot(&doc));
        doc.smart_newline(true);
        out.push(snapshot(&doc));
        doc.smart_backspace_in_mode(ViewMode::Wysiwyg);
        out.push(snapshot(&doc));
        doc.delete_forward_in_mode(ViewMode::Wysiwyg);
        out.push(snapshot(&doc));
        assert!(doc.undo());
        out.push(snapshot(&doc));
        assert!(doc.redo());
        out.push(snapshot(&doc));
        assert!(doc.undo());
        out.push(snapshot(&doc));
        assert_helper_equivalence(&doc.parsed());
        out
    }

    #[test]
    fn format_toggle_helpers_match_naive_through_scripted_edits() {
        let a = run_scripted_format_sequence(12);
        let b = run_scripted_format_sequence(12);
        assert_eq!(a, b);
        assert!(a.len() > 10);
    }

    #[test]
    fn format_toggle_perf_bound() {
        let n = 80;
        let content = mixed_perf_doc(n);
        let mut doc = DocumentTab::new("p".into(), "p.md".into(), content, None, None);
        let mid = doc.content.len() / 2;
        doc.place(mid, false);
        let start = std::time::Instant::now();
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        let range = doc.selection_range();
        doc.insert_text_in_mode(range, "x", ViewMode::Wysiwyg);
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_millis() < 1500,
            "Ctrl+B + type took {:?} on n={n} (generous debug bound 1500ms)",
            elapsed
        );
    }

    #[test]
    fn format_toggle_selection_is_deterministic() {
        let content = mixed_perf_doc(24);
        let mut end = content.len() / 3;
        while end > 0 && !content.is_char_boundary(end) {
            end -= 1;
        }
        let apply = || {
            let mut doc = DocumentTab::new("p".into(), "p.md".into(), content.clone(), None, None);
            doc.caret.range = 0..end;
            doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
            snapshot(&doc)
        };
        assert_eq!(apply(), apply());
    }

    #[test]
    #[ignore]
    fn format_toggle_perf_bench() {
        let builder = std::thread::Builder::new().stack_size(32 * 1024 * 1024);
        let handle = builder
            .spawn(|| {
                fn median_ms(mut v: Vec<u128>) -> f64 {
                    v.sort();
                    let n = v.len();
                    if n % 2 == 1 {
                        v[n / 2] as f64
                    } else {
                        (v[n / 2 - 1] + v[n / 2]) as f64 / 2.0
                    }
                }
                for n in [100usize, 1000, 5000] {
                    let content = PERF_LINE.repeat(n);
                    let parse_ms = {
                        let mut times = Vec::new();
                        for _ in 0..5 {
                            let t = std::time::Instant::now();
                            let _ = parse_document(&content);
                            times.push(t.elapsed().as_micros());
                        }
                        median_ms(times) / 1000.0
                    };
                    let mut type_times = Vec::new();
                    let mut toggle_times = Vec::new();
                    let mut undo_times = Vec::new();
                    let mut sel_times = Vec::new();
                    for _ in 0..5 {
                        let mut doc = DocumentTab::new(
                            "b".into(),
                            "b.md".into(),
                            content.clone(),
                            None,
                            None,
                        );
                        let mid = doc.content.len() / 2;
                        doc.place(mid, false);
                        let _ = doc.parsed();
                        let t = std::time::Instant::now();
                        let range = doc.selection_range();
                        doc.insert_text_in_mode(range, "x", ViewMode::Wysiwyg);
                        type_times.push(t.elapsed().as_micros());

                        let mut doc = DocumentTab::new(
                            "b".into(),
                            "b.md".into(),
                            content.clone(),
                            None,
                            None,
                        );
                        let mid = doc.content.len() / 2;
                        doc.place(mid, false);
                        let _ = doc.parsed();
                        let t = std::time::Instant::now();
                        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
                        let range = doc.selection_range();
                        doc.insert_text_in_mode(range, "x", ViewMode::Wysiwyg);
                        toggle_times.push(t.elapsed().as_micros());
                        let t = std::time::Instant::now();
                        doc.undo();
                        undo_times.push(t.elapsed().as_micros());

                        let mut doc = DocumentTab::new(
                            "b".into(),
                            "b.md".into(),
                            content.clone(),
                            None,
                            None,
                        );
                        let mut end = content.len() / 3;
                        while end > 0 && !content.is_char_boundary(end) {
                            end -= 1;
                        }
                        doc.caret.range = 0..end;
                        let _ = doc.parsed();
                        let t = std::time::Instant::now();
                        doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
                        sel_times.push(t.elapsed().as_micros());
                    }
                    eprintln!(
                        "n={n} parse={:.2}ms type={:.2}ms ctrlb_type={:.2}ms undo={:.2}ms sel_italic={:.2}ms",
                        parse_ms,
                        median_ms(type_times) / 1000.0,
                        median_ms(toggle_times) / 1000.0,
                        median_ms(undo_times) / 1000.0,
                        median_ms(sel_times) / 1000.0
                    );
                }
            })
            .unwrap();
        handle.join().unwrap();
    }

    #[test]
    fn compute_stats_is_lazy_until_ensure_stats() {
        let mut doc = DocumentTab::new(
            "s".into(),
            "s.md".into(),
            "one two".into(),
            None,
            None,
        );
        assert_eq!(doc.word_count, 2);
        doc.insert_text_in_mode(doc.content.len()..doc.content.len(), " three four", ViewMode::Raw);
        let before = compute_stats(&doc.content);
        assert_eq!(
            doc.word_count, 2,
            "stats must stay stale until the status bar reads them"
        );
        doc.ensure_stats();
        assert_eq!(doc.word_count, before.words);
        assert_eq!(doc.char_count, before.chars);
        assert_eq!(doc.line_count, before.lines);
        assert_eq!(doc.reading_time_min, before.reading_time_min);
        doc.ensure_stats();
        assert_eq!(doc.word_count, before.words);
    }
}

#[cfg(test)]
mod format_toggle_gate;

#[cfg(test)]
mod diff_probe {
    use super::*;
    struct R(u64);
    impl R {
        fn n(&mut self, m: usize) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 % m as u64) as usize
        }
    }
    #[test]
    #[ignore]
    fn diffrun() {
        let seeds = [
            "abc **x**\n---\nnext *i*\n===\n\n[foo]: http://a\nsee [foo] and [bar]\n\n1. one\nlazy *t*\n\n<div>\nraw **b**\n</div>\n    indented code\n",
            "Hello **bold** and *it* <u>u</u> ~~s~~ `c` [l](http://x) 😀 tiếng ខ្មែរ\n",
            "# Head **b**\n- item *i*\n> quote <u>u</u>\n| a | **b** |\n|---|---|\n| *c* | d |\n\n```\ncode **x**\n```\n",
            "plain text\nline two with ***both*** here\n\nnew para `code` end\n",
        ];
        let toks = [
            "-", "=", "#", " [foo]", "| ", "> ", "1. ", "```", "a", "b", " ", "😀", "ế", "x y", "*",
            "\n",
        ];
        let cmds = ["bold", "italic", "underline", "strike", "code", "link"];
        let mut out = String::new();
        let mut bad = 0usize;
        let base_seed: u64 = std::env::var("DIFF_SEED")
            .ok()
            .and_then(|s| {
                let s = s.trim();
                if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
                    u64::from_str_radix(hex, 16).ok()
                } else {
                    s.parse().ok()
                }
            })
            .unwrap_or(0x9E3779B97F4A7C15);
        for (si, seed) in seeds.iter().enumerate() {
            for run in 0..150u64 {
                let mut r = R(base_seed ^ (si as u64 * 1000 + run + 1));
                let mut t = DocumentTab::new("a".into(), "a.md".into(), seed.repeat(3), None, None);
                for step in 0..60 {
                    let len = t.content.len();
                    let a = floor_boundary(&t.content, r.n(len + 1));
                    let op = r.n(10);
                    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        match op {
                            0..=3 => {
                                t.caret.move_to(a);
                                let p = t.caret.cursor();
                                t.insert_text_in_mode(
                                    p..p,
                                    toks[r.n(toks.len())],
                                    ViewMode::Wysiwyg,
                                );
                            }
                            4 => {
                                t.caret.move_to(a);
                                t.apply_format_in_mode(cmds[r.n(6)], ViewMode::Wysiwyg);
                            }
                            5 => {
                                let b = floor_boundary(&t.content, (a + r.n(12)).min(len));
                                t.caret.move_to(a);
                                t.caret.select_to(b);
                                t.apply_format_in_mode(cmds[r.n(6)], ViewMode::Wysiwyg);
                            }
                            6 => {
                                let b = floor_boundary(&t.content, (a + r.n(4)).min(len));
                                t.insert_text_in_mode(a..b, "", ViewMode::Wysiwyg);
                            }
                            7 => {
                                t.undo();
                            }
                            8 => {
                                t.caret.move_to(a);
                                let p = t.caret.cursor();
                                t.apply_format_in_mode(cmds[r.n(6)], ViewMode::Wysiwyg);
                                t.insert_text_in_mode(p..p, "zz", ViewMode::Wysiwyg);
                            }
                            _ => {
                                t.caret.move_to(a);
                                let p = t.caret.cursor();
                                t.insert_text_in_mode(p..p, "\n", ViewMode::Wysiwyg);
                            }
                        }
                    }));
                    let st = if res.is_err() {
                        "PANIC"
                    } else if *t.parsed() != parse_document(&t.content) {
                        "DIVERGE"
                    } else {
                        "ok"
                    };
                    if st != "ok" {
                        bad += 1;
                    }
                    out.push_str(&format!(
                        "{si}/{run}/{step} op{op} {st} c={} wc={:?} {:?}\n",
                        t.caret.cursor(),
                        t.parsed().lines.len(),
                        t.content
                    ));
                    if res.is_err() {
                        break;
                    }
                }
            }
        }
        if let Ok(path) = std::env::var("DIFF_OUT") {
            std::fs::write(path, &out).unwrap();
        }
        assert_eq!(bad, 0, "diff_probe DIVERGE/PANIC count={bad}");
    }
}
#[cfg(test)]
mod heading_format_tests {
    use super::*;

    fn run(src: &str, sel: std::ops::Range<usize>, cmds: &[&str], mode: ViewMode) -> String {
        let mut d = DocumentTab::new("t".into(), "t.md".into(), src.to_string(), None, None);
        d.caret.range = sel;
        for c in cmds {
            d.apply_format_in_mode(c, mode);
        }
        d.content
    }

    #[test]
    fn inline_formats_work_on_headings_in_both_modes() {
        for mode in [ViewMode::Raw, ViewMode::Wysiwyg] {
            let h = "# Hello world here";
            assert_eq!(run(h, 2..18, &["bold"], mode), "# **Hello world here**");
            assert_eq!(run(h, 8..13, &["bold"], mode), "# Hello **world** here");
            assert_eq!(run(h, 2..18, &["italic"], mode), "# *Hello world here*");
            assert_eq!(run(h, 2..18, &["strike"], mode), "# ~~Hello world here~~");
            assert_eq!(
                run(h, 2..18, &["bold", "italic", "underline"], mode),
                "# ***<u>Hello world here</u>***"
            );
        }
    }

    #[test]
    fn heading_format_toggles_off_and_can_be_unformatted_in_parts() {
        for mode in [ViewMode::Raw, ViewMode::Wysiwyg] {
            assert_eq!(run("# Hello world here", 2..18, &["bold", "bold"], mode), "# Hello world here");
            let b = "# **Hello world here**";
            assert_eq!(run(b, 10..15, &["bold"], mode), "# **Hello** world **here**");
            assert_eq!(run(b, 4..9, &["bold"], mode), "# Hello **world here**");
            assert_eq!(run(b, 16..20, &["bold"], mode), "# **Hello world** here");
            assert_eq!(run(b, 10..15, &["italic"], mode), "# **Hello *world* here**");
            assert_eq!(
                run("# ***Hello world here***", 11..16, &["italic"], mode),
                "# ***Hello* world *here***"
            );
        }
    }

    #[test]
    fn heading_marker_survives_formatting_a_selection_that_includes_it() {
        assert_eq!(
            run("# Hello world here", 0..18, &["bold"], ViewMode::Wysiwyg),
            "# **Hello world here**"
        );
        assert_eq!(run("## Title", 3..8, &["bold"], ViewMode::Wysiwyg), "## **Title**");
    }
}
