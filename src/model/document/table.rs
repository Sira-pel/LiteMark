//! Markdown table detection and table operations.

use super::*;

impl DocumentTab {
    /// The table whose block contains `pos`, with its body rows in order.
    pub(super) fn table_at(&mut self, pos: usize) -> Option<TableInfo> {
        let parsed = self.parsed();
        let mut info: Option<TableInfo> = None;
        for line in &parsed.lines {
            if let LineKind::TableRow {
                is_header,
                ref body,
                ..
            } = line.kind
            {
                let TableRowBody {
                    cells,
                    table_source,
                    ..
                } = &**body;
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
    pub(super) fn apply_table_exit(&mut self, remove_start: usize, remove_end: usize) {
        let nl = self.line_break(remove_start);
        let rest = &self.content[remove_end..];
        let (insert_at, insert_text, caret_rel) = blank_gap_after_table(rest, nl);
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
        if self.is_read_only() || !self.file_type.supports_markdown_features() {
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
            let cols = table
                .rows
                .first()
                .map(|r| r.cells.len())
                .unwrap_or(2)
                .max(2);
            let nl = self.line_break(at);
            let row = format!("{nl}|{}", "  |".repeat(cols));
            self.edit(at..at, &row);
            self.caret
                .move_to(floor_boundary(&self.content, at + nl.len() + 2));
            self.refresh_cursor();
            return true;
        };
        let row = &table.rows[row_pos];
        let col = row
            .cells
            .iter()
            .position(|c| cursor >= c.start.saturating_sub(1) && cursor <= c.end + 1)
            .unwrap_or_else(|| {
                row.cells
                    .iter()
                    .rposition(|c| c.start <= cursor)
                    .unwrap_or(0)
            });
        let cols = row.cells.len().max(2);
        let at = if row.is_header {
            table.delimiter_end.unwrap_or(row.source.end)
        } else {
            row.source.end
        };
        let nl = self.line_break(at);
        let new_row = format!("{nl}|{}", "  |".repeat(cols));
        self.edit(at..at, &new_row);
        let target = at + nl.len() + 2 + 3 * col.min(cols - 1);
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

    pub fn apply_table_op(&mut self, op: TableOp) -> bool {
        let pos = self.cursor_offset();
        self.apply_table_op_at(pos, op)
    }

    /// Plan `op` at `pos` and apply it. Undo restores the caret that was
    /// current when this was called, which can differ from `pos` when a
    /// hover control targets another cell.
    pub fn apply_table_op_at(&mut self, pos: usize, op: TableOp) -> bool {
        if self.is_read_only() || !self.file_type.supports_markdown_features() {
            return false;
        }
        let cursor = self.cursor_offset();
        let original_sel = self.selection_snap();
        let parsed = self.parsed();
        let Some(plan) = table_ops::plan(&self.content, &parsed, pos, op) else {
            return false;
        };
        drop(parsed);
        let mapped = original_sel
            .as_ref()
            .map(|sel| mapped_selection(sel, &plan));
        self.next_undo_caret = Some(cursor);
        self.next_redo_caret = Some(plan.caret);
        self.next_undo_sel = original_sel.clone();
        self.next_redo_sel = mapped.clone();
        let caret = plan.caret;
        self.edit(plan.range, &plan.new_text);
        if let Some(sel) = mapped {
            self.restore_selection(sel);
        } else {
            let caret = floor_boundary(&self.content, caret.min(self.content.len()));
            self.caret.move_to(caret);
            self.refresh_cursor();
        }
        true
    }

    pub(super) fn selection_snap(&self) -> Option<SelectionSnap> {
        if self.caret.range.is_empty() {
            None
        } else {
            Some(SelectionSnap {
                start: self.caret.range.start,
                end: self.caret.range.end,
                reversed: self.caret.reversed,
            })
        }
    }

    pub(super) fn restore_selection(&mut self, sel: SelectionSnap) {
        let start = floor_boundary(&self.content, sel.start.min(self.content.len()));
        let end = floor_boundary(&self.content, sel.end.min(self.content.len()));
        if end < start {
            self.caret.range = end..start;
            self.caret.reversed = !sel.reversed;
        } else if start == end {
            self.caret.move_to(start);
        } else {
            self.caret.range = start..end;
            self.caret.reversed = sel.reversed;
            self.caret.preferred_col = None;
        }
        self.caret.clamp(&self.content);
        self.refresh_cursor();
    }

    pub fn insert_table_with_size(&mut self, cols: usize, rows: usize) {
        if self.is_read_only() || !self.file_type.supports_markdown_features() {
            return;
        }
        let cols = cols.clamp(1, 10);
        let rows = rows.clamp(1, 20);

        let mut range = self.selection_range();
        let parsed = self.parsed();
        if let Some(table_end) = parsed.lines.iter().find_map(|l| {
            if let LineKind::TableRow { ref body, .. } = l.kind {
                let TableRowBody { table_source, .. } = &**body;
                if range.start >= table_source.start && range.start <= table_source.end {
                    return Some(table_source.end);
                }
            }
            None
        }) {
            range = table_end..table_end;
        }

        // Blank lines around the table use the line ending already there.
        let nl = self.line_break(range.start);
        let blank = format!("{nl}{nl}");
        let mut prefix = String::new();
        if range.start > 0 {
            let before = &self.content[..range.start];
            if !before.ends_with(['\n', '\r']) {
                prefix.push_str(&blank);
            } else if !before.ends_with(blank.as_str()) {
                prefix.push_str(nl);
            }
        }
        let mut suffix = String::new();
        if range.end < self.content.len() {
            let after = &self.content[range.end..];
            if !after.starts_with(['\n', '\r']) {
                suffix.push_str(&blank);
            } else if !after.starts_with(blank.as_str()) {
                suffix.push_str(nl);
            }
        }

        let mut table_rows: Vec<Vec<String>> = Vec::with_capacity(rows);
        for _ in 0..rows {
            table_rows.push(vec![String::new(); cols]);
        }
        let alignments = vec![TableColumnAlign::Left; cols];
        // The first cell sits on the first line, before any break, so its
        // range is the same after the breaks are converted.
        let (table_str, first_cell_rel) = format_markdown_table(&table_rows, &alignments, 0, 0);
        let table_str = if nl == "\n" {
            table_str
        } else {
            table_str.replace('\n', nl)
        };
        let full_insert = format!("{}{}{}", prefix, table_str, suffix);
        let base = range.start + prefix.len();
        self.edit(range, &full_insert);
        self.caret.range = (base + first_cell_rel.start)..(base + first_cell_rel.end);
        self.caret.reversed = false;
        self.refresh_cursor();
    }

    pub fn table_add_row_above(&mut self) {
        let _ = self.apply_table_op(TableOp::InsertRowAbove);
    }

    pub fn table_add_row_below(&mut self) {
        let _ = self.apply_table_op(TableOp::InsertRowBelow);
    }

    pub fn table_add_col_left(&mut self) {
        let _ = self.apply_table_op(TableOp::InsertColLeft);
    }

    pub fn table_add_col_right(&mut self) {
        let _ = self.apply_table_op(TableOp::InsertColRight);
    }

    pub fn table_delete_row(&mut self) {
        let _ = self.apply_table_op(TableOp::DeleteRow);
    }

    pub fn table_delete_col(&mut self) {
        let _ = self.apply_table_op(TableOp::DeleteCol);
    }

    /// Move the row at `pos` to `to_index`. Index 0 is the header and is refused.
    pub fn table_move_row_at(&mut self, pos: usize, to_index: usize) {
        let _ = self.apply_table_op_at(pos, TableOp::MoveRow { to_index });
    }

    /// Move the column at `pos` to `to_index`.
    pub fn table_move_col_at(&mut self, pos: usize, to_index: usize) {
        let _ = self.apply_table_op_at(pos, TableOp::MoveCol { to_index });
    }

    pub fn table_delete_table(&mut self) {
        let pos = self.cursor_offset();
        self.table_delete_table_at(pos);
    }

    /// Delete the table that contains `pos`. The caret is not required to sit in it.
    pub fn table_delete_table_at(&mut self, pos: usize) {
        if self.is_read_only() {
            return;
        }
        let Some(source) = table_ops::parsed_table_source_in(&self.parsed(), pos) else {
            return;
        };
        // Remove one whole line ending with the table (CR included), the one
        // after it if there is one, else the one before.
        let mut del_range = source;
        let after = line_ending_len(&self.content, del_range.end);
        if after > 0 {
            del_range.end += after;
        } else {
            del_range.start -= line_ending_len_before(&self.content, del_range.start);
        }
        let target = del_range.start;
        self.edit(del_range, "");
        self.caret.move_to(floor_boundary(&self.content, target));
        self.refresh_cursor();
    }

    pub fn table_align_column(&mut self, align: TableColumnAlign) {
        let _ = self.apply_table_op(TableOp::SetAlign(align));
    }

    /// Ensures a short table row at `line_start` has `|` separators through
    /// `target_col` and returns the materialized range for `target_col`.
    pub fn materialize_table_row_cell(
        &mut self,
        line_start: usize,
        target_col: usize,
    ) -> Option<Range<usize>> {
        let (line_end, cols, target) = {
            let parsed = self.parsed();
            let pline = parsed.line_at_offset(line_start)?;
            let LineKind::TableRow { ref body, .. } = pline.kind else {
                return None;
            };
            let cols = body.alignments.len();
            if cols == 0 {
                return None;
            }
            (pline.source.end, cols, target_col.min(cols - 1))
        };
        let line_str = self.content.get(line_start..line_end)?;
        let real_ranges = crate::markdown::parser::table_cell_ranges(line_str);
        if target < real_ranges.len() {
            let parsed = self.parsed();
            if let Some(pline) = parsed.line_at_offset(line_start) {
                if let LineKind::TableRow { ref body, .. } = pline.kind {
                    return body.cells.get(target).cloned();
                }
            }
            return None;
        }
        let has_trailing_pipe = real_ranges
            .last()
            .is_some_and(|last| last.end < line_str.trim_end().len());
        let missing = cols.saturating_sub(real_ranges.len());
        let mut suffix = String::new();
        if !has_trailing_pipe {
            suffix.push_str(if line_str.ends_with(' ') { "|" } else { " |" });
        }
        for _ in 0..missing {
            suffix.push_str("  |");
        }
        self.edit(line_end..line_end, &suffix);
        let parsed = self.parsed();
        let pline = parsed.line_at_offset(line_start)?;
        if let LineKind::TableRow { ref body, .. } = pline.kind {
            body.cells.get(target).cloned()
        } else {
            None
        }
    }

    pub fn tab_table_cell(&mut self, backwards: bool) -> bool {
        if !self.file_type.supports_markdown_features() {
            return false;
        }
        let cursor = self.cursor_offset();
        let parsed = self.parsed();
        struct TabCell {
            range: Range<usize>,
            line_start: usize,
            col_idx: usize,
            padded: bool,
        }
        let mut all_cells: Vec<TabCell> = Vec::new();
        let mut in_table = false;
        let mut table_end = 0usize;
        let mut num_cols = 2usize;
        for line in &parsed.lines {
            if let LineKind::TableRow { ref body, .. } = line.kind {
                let TableRowBody {
                    cells,
                    raw_cells,
                    table_source,
                    ..
                } = &**body;
                if cursor >= table_source.start && cursor <= table_source.end {
                    in_table = true;
                    table_end = table_source.end;
                    num_cols = cells.len().max(num_cols);
                    for (col_idx, (c, rc)) in cells.iter().zip(raw_cells.iter()).enumerate() {
                        let padded = col_idx > 0
                            && rc.is_empty()
                            && rc.start == line.source.end
                            && raw_cells
                                .get(col_idx - 1)
                                .is_some_and(|prev| prev.end == rc.start);
                        all_cells.push(TabCell {
                            range: c.clone(),
                            line_start: line.source.start,
                            col_idx,
                            padded,
                        });
                    }
                }
            }
        }
        if !in_table || all_cells.is_empty() {
            return false;
        }
        let cur_idx = all_cells
            .iter()
            .position(|c| !c.padded && cursor >= c.range.start && cursor <= c.range.end)
            .or_else(|| {
                all_cells
                    .iter()
                    .position(|c| cursor >= c.range.start && cursor <= c.range.end)
            })
            .unwrap_or_else(|| {
                all_cells
                    .iter()
                    .rposition(|c| c.range.start <= cursor)
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
                let nl = self.line_break(table_end);
                let new_row = format!("{nl}|{}", "  |".repeat(num_cols));
                self.edit(table_end..table_end, &new_row);
                let first_cell_start = (table_end + nl.len() + 2).min(self.content.len());
                let target = floor_boundary(&self.content, first_cell_start);
                self.caret.range = target..target;
                self.caret.reversed = false;
                self.refresh_cursor();
                return true;
            }
            cur_idx + 1
        };
        let next = &all_cells[next_idx];
        let target = if next.padded {
            let line_start = next.line_start;
            let col_idx = next.col_idx;
            drop(all_cells);
            drop(parsed);
            self.materialize_table_row_cell(line_start, col_idx)
                .unwrap_or(cursor..cursor)
        } else {
            next.range.clone()
        };
        self.caret.range = target;
        self.caret.reversed = false;
        self.refresh_cursor();
        true
    }

    pub fn snap_wysiwyg_table_delimiter(&mut self) {
        let cursor = self.cursor_offset();
        let parsed = self.parsed();
        let target = parsed.line_at_offset(cursor).and_then(|pline| {
            if !matches!(pline.kind, LineKind::TableDelimiter) {
                return None;
            }
            let ending = line_ending_len(&self.content, pline.source.end);
            if ending > 0 && pline.source.end + ending <= self.content.len() {
                Some(pline.source.end + ending)
            } else {
                // Header-only table at EOF: snap to the header row's first cell.
                parsed
                    .lines
                    .get(pline.line_index.saturating_sub(1))
                    .map(|prev| {
                        if let LineKind::TableRow { ref body, .. } = prev.kind {
                            body.cells
                                .first()
                                .map_or(prev.source.start, |cell| cell.start)
                        } else {
                            prev.source.start
                        }
                    })
                    .or(Some(0))
            }
        });
        if let Some(next_pos) = target {
            let target = floor_boundary(&self.content, next_pos.min(self.content.len()));
            self.caret.move_to(target);
            self.refresh_cursor();
        }
    }
}

pub(super) fn mapped_selection(sel: &SelectionSnap, plan: &table_ops::TablePlan) -> SelectionSnap {
    let start = plan.map_point(sel.start);
    let end = plan.map_point(sel.end);
    if end < start {
        SelectionSnap {
            start: end,
            end: start,
            reversed: !sel.reversed,
        }
    } else {
        SelectionSnap {
            start,
            end,
            reversed: sel.reversed,
        }
    }
}

pub(super) fn format_markdown_table(
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
pub(super) fn table_cell_source(cell: &str) -> String {
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

#[derive(Clone)]
pub(super) struct TableRowInfo {
    pub(super) source: Range<usize>,
    pub(super) cells: Vec<Range<usize>>,
    pub(super) is_header: bool,
}

#[derive(Clone)]
pub(super) struct TableInfo {
    pub(super) source: Range<usize>,
    pub(super) rows: Vec<TableRowInfo>,
    pub(super) delimiter_end: Option<usize>,
}

/// Given the text after a table's end, return where to insert what so a blank
/// line separates the table from the caret line, and where the caret goes
/// (all offsets relative to the start of `rest`, caret in the edited text).
/// An existing blank line is reused. New breaks are `nl`.
pub(super) fn blank_gap_after_table(rest: &str, nl: &str) -> (usize, String, usize) {
    if rest.is_empty() {
        return (0, format!("{nl}{nl}"), 2 * nl.len());
    }
    let ne = line_ending_len(rest, 0);
    if ne == 0 {
        return (0, format!("{nl}{nl}"), 2 * nl.len());
    }
    let after = &rest[ne..];
    if after.is_empty() {
        return (ne, nl.to_string(), ne + nl.len());
    }
    let l2_end = line_end(after, 0);
    if after[..l2_end].trim().is_empty() {
        let end_len = line_ending_len(after, l2_end);
        (0, String::new(), ne + l2_end + end_len)
    } else {
        (ne, nl.to_string(), ne + nl.len())
    }
}
