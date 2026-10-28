//! Table block model and the edits that rewrite one.
//!
//! GFM (spec 0.29, section 4.10) is the source of the bytes. This module
//! quotes the rules it follows and does not reformat a row it did not change.
//!
//! A table is "a single header row, a delimiter row separating the header
//! from the data, and zero or more data rows." The delimiter row "consists of
//! cells whose only content are hyphens (`-`), and optionally, a leading or
//! trailing colon (`:`), or both, to indicate left, right, or center
//! alignment respectively." Example 198 is the plain table. Example 199 is
//! `:-:` / `---:` alignment. "Include a pipe in a cell's content by escaping
//! it, including inside other inline spans" (the example this repo calls
//! gfm200). Example 203 is not a table when the delimiter cell count differs
//! from the header. Example 204: "If there are a number of cells fewer than
//! the number of cells in the header row, empty cells are inserted. If there
//! are greater, the excess is ignored." The example's markdown still contains
//! `boo`; the HTML drops it. Example 205 is a header and a delimiter with no
//! body. "The table is broken at the first empty line, or beginning of
//! another block-level structure."
//!
//! # Normalisation
//!
//! Slot endings stay put. Row text moves; the line ending after a slot does
//! not. A file that does not end in a newline still does not. CRLF slots stay
//! CRLF. A new row copies the nearest full-width row (the neighbour when that
//! row has a cell for every column, otherwise the header): same outer pipes,
//! and each new cell is the neighbour cell's leading and trailing spaces with
//! the text removed. A new delimiter cell keeps that neighbour's hyphen count
//! (at least 3) and its surrounding spaces, with no colons (`---`, not
//! `:---`). Alignment rewrites only that delimiter cell: same hyphen count,
//! colons per the alignment, spaces around the token kept. Untouched rows are
//! copied byte for byte.
//!
//! Padding is not written back onto a short row unless the edit needs a slot
//! that row does not have (a column insert or a column move). Extra cells
//! past the delimiter width stay on the line and shift with the cells in
//! front of them. They are not a column of their own (example 204 ignores
//! them in the table and keeps them in the line).
//!
//! The header is row 0. It is not deleted. It is not moved. A body row is not
//! moved to index 0, so a drag across the header boundary is refused and the
//! header text stays the header. Inserting above the header is refused,
//! because that line would become the header. The last remaining column is
//! not deleted: a zero-column block is not a GFM table (example 203).
//!
//! A row that would become empty is written as `|  |` so the line is not the
//! blank line that ends a table. A pipe-less row (example 202's `bar`) stays
//! byte for byte until a column edit rewrites it. The rewrite then takes the
//! header's outer pipes, so the line still parses as separate cells. A
//! rewritten row that would drop an empty edge cell, lose its last pipe, or
//! pick up four leading spaces (so the parser would not see a table) takes
//! outer pipes even when the header had none.
//!
//! Tables inside a block quote or a list item are not tables in this parser
//! (`>` and a list marker break the block). Ops do not find them, so the
//! bytes stay. A table that is the next block after a paragraph, including
//! one that starts on the following line with no blank line, is edited when
//! this parser reports a table there.
//!
//! Pipes come from [`unescaped_pipe_offsets`]: `\|` and a pipe inside a code
//! span are cell text, not separators.

use std::ops::Range;

use crate::markdown::parser::delimiter_alignments;
use crate::markdown::{parse_document, unescaped_pipe_offsets, LineKind, TableColumnAlign};

/// One table edit. The anchor row and column come from the caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableOp {
    InsertRowAbove,
    InsertRowBelow,
    InsertColLeft,
    InsertColRight,
    DeleteRow,
    DeleteCol,
    /// Final row index. `0` is the header and is refused.
    MoveRow {
        to_index: usize,
    },
    /// Final column index.
    MoveCol {
        to_index: usize,
    },
    SetAlign(TableColumnAlign),
}

/// Replacement for one table, the caret inside the edited cell, and a map of
/// byte ranges that kept their text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TablePlan {
    pub range: Range<usize>,
    pub new_text: String,
    pub caret: usize,
    maps: Vec<(Range<usize>, usize)>,
}

impl TablePlan {
    /// Map an offset through the replacement.
    ///
    /// Points before the table stay. Points after it shift by the length
    /// delta. Points inside a copied span keep their place in that span.
    pub fn map_point(&self, point: usize) -> usize {
        let start = self.range.start;
        let old_end = self.range.end;
        let new_len = self.new_text.len();
        if point <= start {
            return point;
        }
        if point >= old_end {
            return point - old_end + start + new_len;
        }
        for (old, new_start) in &self.maps {
            if point >= old.start && point <= old.end {
                return new_start + (point - old.start);
            }
        }
        let rel = point - start;
        start + rel.min(new_len)
    }
}

/// Parser range of the table that contains `pos`, without the line ending
/// that follows the last row. `None` when `pos` is not in a table, which
/// includes a table written inside a block quote or a list item.
pub fn parsed_table_source(content: &str, pos: usize) -> Option<Range<usize>> {
    let parsed = parse_document(content);
    for line in &parsed.lines {
        if let LineKind::TableRow { table_source, .. } = &line.kind {
            if pos >= table_source.start && pos <= table_source.end {
                return Some(table_source.clone());
            }
        }
    }
    None
}

/// Plan `op` at `pos`. `None` when there is no table, the op is refused, or
/// the bytes would not change.
pub fn plan(content: &str, pos: usize, op: TableOp) -> Option<TablePlan> {
    let source = parsed_table_source(content, pos)?;
    let end = extend_through_ending(content, source.end);
    let range = source.start..end;
    let block = content.get(range.clone())?;
    let mut table = Table::parse(block, range.start)?;
    let anchor = table.anchor_at(pos)?;
    let target = table.apply(op, anchor)?;
    let laid = table.lay_out(range.start);
    if laid.text == block {
        return None;
    }
    let caret = table.caret_at(&laid, target);
    let maps = table.maps(&laid);
    Some(TablePlan {
        range,
        new_text: laid.text,
        caret,
        maps,
    })
}

fn extend_through_ending(content: &str, mut end: usize) -> usize {
    let bytes = content.as_bytes();
    if end < bytes.len() && bytes[end] == b'\r' {
        end += 1;
        if end < bytes.len() && bytes[end] == b'\n' {
            end += 1;
        }
    } else if end < bytes.len() && bytes[end] == b'\n' {
        end += 1;
    }
    end
}

#[derive(Clone, Debug)]
struct Parts {
    prefix: String,
    suffix: String,
    leading_pipe: bool,
    trailing_pipe: bool,
    cells: Vec<String>,
}

#[derive(Clone, Debug)]
struct Row {
    /// Set for a row that was in the original block.
    key: Option<u64>,
    abs: usize,
    text: String,
    original: Parts,
    parts: Parts,
    dirty: bool,
    /// Old cell index to current cell index.
    cell_map: Vec<Option<usize>>,
}

struct Table {
    header: Row,
    header_ending: String,
    delim: Row,
    delim_ending: String,
    body: Vec<Row>,
    body_endings: Vec<String>,
    next_key: u64,
}

struct LaidOut {
    text: String,
    at: Vec<usize>,
}

#[derive(Clone, Copy)]
struct Anchor {
    row: usize,
    col: usize,
}

impl Table {
    fn parse(block: &str, origin: usize) -> Option<Self> {
        let lines = split_block(block);
        if lines.len() < 2 {
            return None;
        }
        delimiter_alignments(&lines[1].0)?;
        let mut abs = origin;
        let mut rows = Vec::with_capacity(lines.len());
        for (index, (text, ending)) in lines.into_iter().enumerate() {
            let row = Row::load(text, abs, index as u64);
            abs += row.text.len() + ending.len();
            rows.push((row, ending));
        }
        let mut iter = rows.into_iter();
        let (header, header_ending) = iter.next()?;
        let (delim, delim_ending) = iter.next()?;
        if header.parts.cells.len() != delim.parts.cells.len() || delim.parts.cells.is_empty() {
            return None;
        }
        let mut body = Vec::new();
        let mut body_endings = Vec::new();
        for (row, ending) in iter {
            body.push(row);
            body_endings.push(ending);
        }
        let next_key = (2 + body.len()) as u64;
        Some(Self {
            header,
            header_ending,
            delim,
            delim_ending,
            body,
            body_endings,
            next_key,
        })
    }

    fn col_count(&self) -> usize {
        self.delim.parts.cells.len()
    }

    fn row_count(&self) -> usize {
        1 + self.body.len()
    }

    fn anchor_at(&self, pos: usize) -> Option<Anchor> {
        let col_count = self.col_count();
        if pos >= self.header.abs
            && self.line_owns(self.header.abs, &self.header.text, self.delim.abs, pos)
        {
            let col = column_at(&self.header.parts, self.header.abs, pos, col_count);
            return Some(Anchor { row: 0, col });
        }
        let after_delim = self.body.first().map(|row| row.abs).unwrap_or(usize::MAX);
        if pos >= self.delim.abs
            && self.line_owns(self.delim.abs, &self.delim.text, after_delim, pos)
        {
            let col = column_at(&self.delim.parts, self.delim.abs, pos, col_count);
            return Some(Anchor { row: 0, col });
        }
        for (index, row) in self.body.iter().enumerate() {
            let next = self
                .body
                .get(index + 1)
                .map(|next| next.abs)
                .unwrap_or(usize::MAX);
            if pos >= row.abs && self.line_owns(row.abs, &row.text, next, pos) {
                let col = column_at(&row.parts, row.abs, pos, col_count);
                return Some(Anchor {
                    row: index + 1,
                    col,
                });
            }
        }
        None
    }

    fn line_owns(&self, abs: usize, text: &str, next_abs: usize, pos: usize) -> bool {
        let end = abs + text.len();
        pos >= abs && pos <= end && pos < next_abs.max(end.saturating_add(1))
    }

    fn apply(&mut self, op: TableOp, anchor: Anchor) -> Option<Anchor> {
        let col_count = self.col_count();
        let col = anchor.col.min(col_count.saturating_sub(1));
        let target = match op {
            TableOp::InsertRowAbove => {
                if anchor.row == 0 {
                    return None;
                }
                let at = anchor.row - 1;
                self.insert_body_row(at)?;
                Some(Anchor {
                    row: anchor.row,
                    col,
                })
            }
            TableOp::InsertRowBelow => {
                let at = anchor.row;
                self.insert_body_row(at)?;
                Some(Anchor {
                    row: anchor.row + 1,
                    col,
                })
            }
            TableOp::InsertColLeft => {
                self.insert_col(col);
                Some(Anchor {
                    row: anchor.row,
                    col,
                })
            }
            TableOp::InsertColRight => {
                self.insert_col(col + 1);
                Some(Anchor {
                    row: anchor.row,
                    col: col + 1,
                })
            }
            TableOp::DeleteRow => {
                if anchor.row == 0 || anchor.row >= self.row_count() {
                    return None;
                }
                self.delete_body(anchor.row - 1);
                let row = anchor.row.min(self.row_count().saturating_sub(1));
                Some(Anchor { row, col })
            }
            TableOp::DeleteCol => {
                if col_count <= 1 || col >= col_count {
                    return None;
                }
                self.delete_col(col);
                let col = col.min(self.col_count().saturating_sub(1));
                Some(Anchor {
                    row: anchor.row.min(self.row_count().saturating_sub(1)),
                    col,
                })
            }
            TableOp::MoveRow { to_index } => {
                if anchor.row == 0 || to_index == 0 || anchor.row == to_index {
                    return None;
                }
                if anchor.row >= self.row_count() || to_index >= self.row_count() {
                    return None;
                }
                let from = anchor.row - 1;
                let to = to_index - 1;
                let row = self.body.remove(from);
                self.body.insert(to, row);
                Some(Anchor { row: to_index, col })
            }
            TableOp::MoveCol { to_index } => {
                if col == to_index || to_index >= col_count || col >= col_count {
                    return None;
                }
                self.move_col(col, to_index);
                Some(Anchor {
                    row: anchor.row,
                    col: to_index,
                })
            }
            TableOp::SetAlign(align) => {
                if col >= col_count {
                    return None;
                }
                let cell = self.delim.parts.cells.get(col)?.clone();
                let rewritten = align_cell(&cell, align);
                if rewritten == cell {
                    return None;
                }
                self.delim.parts.cells[col] = rewritten;
                self.delim.dirty = true;
                Some(Anchor {
                    row: anchor.row,
                    col,
                })
            }
        };
        self.ensure_dirty_rows();
        target
    }

    fn ensure_dirty_rows(&mut self) {
        ensure_row(&mut self.header);
        ensure_row(&mut self.delim);
        for row in &mut self.body {
            ensure_row(row);
        }
    }

    fn insert_body_row(&mut self, body_index: usize) -> Option<()> {
        let cols = self.col_count();
        let style = self.style_for_insert(body_index);
        let row = Row::fresh(&style, cols, &mut self.next_key);
        self.insert_body(body_index, row);
        Some(())
    }

    fn style_for_insert(&self, body_index: usize) -> Parts {
        let cols = self.col_count();
        if let Some(row) = self.body.get(body_index) {
            if row.parts.cells.len() >= cols {
                return row.parts.clone();
            }
        }
        if body_index > 0 {
            if let Some(row) = self.body.get(body_index - 1) {
                if row.parts.cells.len() >= cols {
                    return row.parts.clone();
                }
            }
        }
        if self.header.parts.cells.len() >= cols {
            return self.header.parts.clone();
        }
        self.header.parts.clone()
    }

    fn insert_body(&mut self, index: usize, row: Row) {
        let ending = self.block_ending();
        if index >= self.body.len() {
            if self.body.is_empty() && self.delim_ending.is_empty() {
                self.delim_ending = ending;
                self.body.push(row);
                self.body_endings.push(String::new());
                return;
            }
            if self.body_endings.last().is_some_and(|slot| slot.is_empty()) {
                if let Some(last) = self.body_endings.last_mut() {
                    *last = ending;
                }
                self.body.push(row);
                self.body_endings.push(String::new());
                return;
            }
            self.body.push(row);
            self.body_endings.push(ending);
            return;
        }
        self.body.insert(index, row);
        self.body_endings.insert(index, ending);
    }

    fn delete_body(&mut self, index: usize) {
        let removed_ending = self.body_endings.remove(index);
        self.body.remove(index);
        if index == self.body.len() && removed_ending.is_empty() {
            if let Some(last) = self.body_endings.last_mut() {
                last.clear();
            } else {
                self.delim_ending.clear();
            }
        }
    }

    fn block_ending(&self) -> String {
        if !self.header_ending.is_empty() {
            return self.header_ending.clone();
        }
        if !self.delim_ending.is_empty() {
            return self.delim_ending.clone();
        }
        self.body_endings
            .iter()
            .find(|ending| !ending.is_empty())
            .cloned()
            .unwrap_or_else(|| "\n".to_string())
    }

    fn insert_col(&mut self, at: usize) {
        let at = at.min(self.col_count());
        let pipes = self.header.parts.clone();
        Self::insert_col_row(&mut self.header, at, false);
        Self::insert_col_row(&mut self.delim, at, true);
        for row in &mut self.body {
            Self::insert_col_row(row, at, false);
            adopt_header_pipes(row, &pipes);
        }
    }

    fn insert_col_row(row: &mut Row, at: usize, delim: bool) {
        while row.parts.cells.len() < at {
            let template = row
                .parts
                .cells
                .last()
                .cloned()
                .unwrap_or_else(|| "  ".to_string());
            let pad = if delim {
                align_cell(&template, TableColumnAlign::None)
            } else {
                empty_like(&template)
            };
            row.parts.cells.push(pad);
        }
        let template = template_at(&row.parts.cells, at);
        let cell = if delim {
            align_cell(&template, TableColumnAlign::None)
        } else {
            empty_like(&template)
        };
        row.note_insert(at);
        row.parts.cells.insert(at, cell);
        row.dirty = true;
    }

    fn delete_col(&mut self, index: usize) {
        let style = self.header.parts.clone();
        Self::delete_col_row(&mut self.header, index, &style);
        Self::delete_col_row(&mut self.delim, index, &style);
        for row in &mut self.body {
            Self::delete_col_row(row, index, &style);
            adopt_header_pipes(row, &style);
        }
    }

    fn delete_col_row(row: &mut Row, index: usize, style: &Parts) {
        if row.parts.cells.len() > index {
            row.note_remove(index);
            row.parts.cells.remove(index);
            row.dirty = true;
        }
        if row.parts.cells.is_empty() {
            row.parts.cells.push(placeholder_cell(style));
            row.parts.leading_pipe = true;
            row.parts.trailing_pipe = true;
            row.dirty = true;
        }
    }

    fn move_col(&mut self, from: usize, to: usize) {
        let cols = self.col_count();
        Self::move_col_row(&mut self.header, from, to, cols, false);
        Self::move_col_row(&mut self.delim, from, to, cols, true);
        let pipes = self.header.parts.clone();
        for row in &mut self.body {
            Self::move_col_row(row, from, to, cols, false);
            adopt_header_pipes(row, &pipes);
        }
    }

    fn move_col_row(row: &mut Row, from: usize, to: usize, cols: usize, delim: bool) {
        while row.parts.cells.len() < cols {
            let template = row
                .parts
                .cells
                .last()
                .cloned()
                .unwrap_or_else(|| "  ".to_string());
            let pad = if delim {
                align_cell(&template, TableColumnAlign::None)
            } else {
                empty_like(&template)
            };
            row.parts.cells.push(pad);
            row.dirty = true;
        }
        if from >= row.parts.cells.len() || to >= row.parts.cells.len() || from == to {
            return;
        }
        let cell = row.parts.cells.remove(from);
        row.parts.cells.insert(to, cell);
        row.note_move(from, to);
        row.dirty = true;
    }

    fn lay_out(&self, origin: usize) -> LaidOut {
        let mut text = String::new();
        let mut at = Vec::with_capacity(2 + self.body.len());
        let mut cursor = origin;
        at.push(cursor);
        text.push_str(&self.header.line_text());
        cursor += self.header.line_text().len();
        text.push_str(&self.header_ending);
        cursor += self.header_ending.len();
        at.push(cursor);
        text.push_str(&self.delim.line_text());
        cursor += self.delim.line_text().len();
        text.push_str(&self.delim_ending);
        cursor += self.delim_ending.len();
        for (row, ending) in self.body.iter().zip(&self.body_endings) {
            at.push(cursor);
            let line = row.line_text();
            cursor += line.len();
            text.push_str(&line);
            text.push_str(ending);
            cursor += ending.len();
        }
        LaidOut { text, at }
    }

    fn caret_at(&self, laid: &LaidOut, target: Anchor) -> usize {
        // `laid.at` is [header, delimiter, body...]. Body row r is at index r+1.
        let line_at = if target.row == 0 {
            laid.at.first().copied().unwrap_or(0)
        } else {
            laid.at.get(target.row + 1).copied().unwrap_or(0)
        };
        let parts = if target.row == 0 {
            &self.header.parts
        } else {
            self.body
                .get(target.row - 1)
                .map(|row| &row.parts)
                .unwrap_or(&self.header.parts)
        };
        content_caret(line_at, parts, target.col)
    }

    fn maps(&self, laid: &LaidOut) -> Vec<(Range<usize>, usize)> {
        let mut maps = Vec::new();
        self.map_row(&self.header, laid.at.first().copied(), &mut maps);
        self.map_row(&self.delim, laid.at.get(1).copied(), &mut maps);
        for (index, row) in self.body.iter().enumerate() {
            self.map_row(row, laid.at.get(index + 2).copied(), &mut maps);
        }
        maps
    }

    fn map_row(&self, row: &Row, new_abs: Option<usize>, maps: &mut Vec<(Range<usize>, usize)>) {
        let Some(new_abs) = new_abs else {
            return;
        };
        if row.key.is_none() {
            return;
        }
        if !row.dirty {
            let len = row.text.len();
            maps.push((row.abs..row.abs + len, new_abs));
            return;
        }
        let old_spans = cell_spans(row.abs, &row.original);
        let new_spans = cell_spans(new_abs, &row.parts);
        for (old_index, slot) in row.cell_map.iter().enumerate() {
            let Some(new_index) = slot else {
                continue;
            };
            let Some(old_cell) = row.original.cells.get(old_index) else {
                continue;
            };
            let Some(new_cell) = row.parts.cells.get(*new_index) else {
                continue;
            };
            if old_cell != new_cell {
                continue;
            }
            let (Some(old_span), Some(new_span)) =
                (old_spans.get(old_index), new_spans.get(*new_index))
            else {
                continue;
            };
            if old_span.len() == new_span.len() {
                maps.push((old_span.clone(), new_span.start));
            }
        }
    }
}

impl Row {
    fn load(text: String, abs: usize, key: u64) -> Self {
        let parts = split_parts(&text);
        let cell_map = (0..parts.cells.len()).map(Some).collect();
        Self {
            key: Some(key),
            abs,
            text,
            original: parts.clone(),
            parts,
            dirty: false,
            cell_map,
        }
    }

    fn fresh(style: &Parts, cols: usize, next_key: &mut u64) -> Self {
        let mut cells = Vec::with_capacity(cols);
        for index in 0..cols {
            let template = style
                .cells
                .get(index)
                .or_else(|| style.cells.last())
                .map(|cell| cell.as_str())
                .unwrap_or("  ");
            cells.push(empty_like(template));
        }
        let parts = Parts {
            prefix: style.prefix.clone(),
            suffix: style.suffix.clone(),
            leading_pipe: style.leading_pipe,
            trailing_pipe: style.trailing_pipe,
            cells,
        };
        *next_key += 1;
        Self {
            key: None,
            abs: 0,
            text: String::new(),
            original: parts.clone(),
            parts,
            dirty: true,
            cell_map: Vec::new(),
        }
    }

    fn line_text(&self) -> String {
        if self.dirty {
            join_parts(&self.parts)
        } else {
            self.text.clone()
        }
    }

    fn note_insert(&mut self, at: usize) {
        for index in self.cell_map.iter_mut().flatten() {
            if *index >= at {
                *index += 1;
            }
        }
    }

    fn note_remove(&mut self, at: usize) {
        for slot in &mut self.cell_map {
            if let Some(index) = *slot {
                if index == at {
                    *slot = None;
                } else if index > at {
                    *slot = Some(index - 1);
                }
            }
        }
    }

    fn note_move(&mut self, from: usize, to: usize) {
        for slot in &mut self.cell_map {
            if let Some(index) = *slot {
                if index == from {
                    *slot = Some(to);
                } else if from < to && index > from && index <= to {
                    *slot = Some(index - 1);
                } else if to < from && index >= to && index < from {
                    *slot = Some(index + 1);
                }
            }
        }
    }
}

/// A pipe-less row that a column edit rewrites would join as `bar|` or
/// `|bar`, and the parser would not see the same cells. Take the header's
/// outer pipes so the rewritten line splits again.
fn adopt_header_pipes(row: &mut Row, header: &Parts) {
    if !row.dirty || row.parts.leading_pipe || row.parts.trailing_pipe {
        return;
    }
    if header.leading_pipe || header.trailing_pipe {
        row.parts.leading_pipe = header.leading_pipe;
        row.parts.trailing_pipe = header.trailing_pipe;
    }
}

fn cell_is_blank(cell: &str) -> bool {
    cell.bytes().all(|byte| byte == b' ' || byte == b'\t')
}

fn line_indent_columns(line: &str) -> usize {
    let mut column = 0usize;
    for byte in line.bytes() {
        match byte {
            b' ' => column += 1,
            b'\t' => column += 4 - (column % 4),
            _ => break,
        }
    }
    column
}

fn trim_prefix_space(prefix: &mut String) -> bool {
    match prefix.as_bytes().last() {
        Some(&b' ') | Some(&b'\t') => {
            prefix.pop();
            true
        }
        _ => false,
    }
}

/// Keep a rewritten row as a table row: empty edge cells need a pipe so they
/// do not fold into the line edge, a single leftover cell needs a pipe so
/// the delimiter still matches, and four leading spaces would leave the
/// table (the parser stops a table at column 4).
fn ensure_row(row: &mut Row) {
    if !row.dirty {
        return;
    }
    ensure_parts(&mut row.parts);
}

fn ensure_parts(parts: &mut Parts) {
    if parts.cells.is_empty() {
        parts.cells.push("  ".to_string());
        parts.leading_pipe = true;
        parts.trailing_pipe = true;
    } else {
        if !parts.leading_pipe && cell_is_blank(&parts.cells[0]) {
            parts.leading_pipe = true;
        }
        let last = parts.cells.len() - 1;
        if !parts.trailing_pipe && cell_is_blank(&parts.cells[last]) {
            parts.trailing_pipe = true;
        }
        if !parts.leading_pipe && !parts.trailing_pipe && parts.cells.len() == 1 {
            parts.leading_pipe = true;
            parts.trailing_pipe = true;
        }
    }
    for _ in 0..8 {
        let joined = join_parts(parts);
        let parsed = split_parts(&joined);
        let indent = line_indent_columns(&joined);
        let pipes = unescaped_pipe_offsets(&joined);
        if parsed.cells.len() == parts.cells.len() && indent < 4 && !pipes.is_empty() {
            return;
        }
        if indent >= 4 {
            if !parts.leading_pipe {
                parts.leading_pipe = true;
                continue;
            }
            if trim_prefix_space(&mut parts.prefix) {
                continue;
            }
        }
        if pipes.is_empty() {
            parts.leading_pipe = true;
            parts.trailing_pipe = true;
            continue;
        }
        if parsed.cells.len() < parts.cells.len() {
            if !parts.leading_pipe {
                parts.leading_pipe = true;
                continue;
            }
            if !parts.trailing_pipe {
                parts.trailing_pipe = true;
                continue;
            }
        }
        return;
    }
}

fn template_at(cells: &[String], at: usize) -> String {
    if at > 0 {
        if let Some(cell) = cells.get(at - 1) {
            return cell.clone();
        }
    }
    cells
        .get(at)
        .cloned()
        .or_else(|| cells.last().cloned())
        .unwrap_or_else(|| "  ".to_string())
}

fn placeholder_cell(style: &Parts) -> String {
    let cell = style
        .cells
        .first()
        .map(|cell| empty_like(cell))
        .unwrap_or_else(|| "  ".to_string());
    if cell.is_empty() {
        "  ".to_string()
    } else {
        cell
    }
}

fn split_block(block: &str) -> Vec<(String, String)> {
    let bytes = block.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'\n' {
            lines.push((block[start..index].to_string(), "\n".to_string()));
            index += 1;
            start = index;
        } else if bytes[index] == b'\r' {
            let (end, ending) = if index + 1 < bytes.len() && bytes[index + 1] == b'\n' {
                (index + 2, "\r\n".to_string())
            } else {
                (index + 1, "\r".to_string())
            };
            lines.push((block[start..index].to_string(), ending));
            index = end;
            start = end;
        } else {
            index += 1;
        }
    }
    if start < bytes.len() || lines.is_empty() {
        lines.push((block[start..].to_string(), String::new()));
    }
    lines
}

fn split_parts(line: &str) -> Parts {
    let bytes = line.as_bytes();
    let mut start = 0usize;
    while start < bytes.len() && bytes[start].is_ascii_whitespace() {
        start += 1;
    }
    let mut end = bytes.len();
    while end > start && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    if start >= end {
        return Parts {
            prefix: line.to_string(),
            suffix: String::new(),
            leading_pipe: false,
            trailing_pipe: false,
            cells: Vec::new(),
        };
    }
    let prefix = line[..start].to_string();
    let suffix = line[end..].to_string();
    let body = &line[start..end];
    let pipes = unescaped_pipe_offsets(body);
    if pipes.is_empty() {
        return Parts {
            prefix,
            suffix,
            leading_pipe: false,
            trailing_pipe: false,
            cells: vec![body.to_string()],
        };
    }
    let mut leading_pipe = false;
    let mut cursor = 0usize;
    if pipes[0] == 0 {
        leading_pipe = true;
        cursor = 1;
    }
    let mut cells = Vec::new();
    for pipe in &pipes {
        if *pipe < cursor {
            continue;
        }
        cells.push(body[cursor..*pipe].to_string());
        cursor = *pipe + 1;
    }
    let trailing_pipe = cursor == body.len() && pipes.last().copied() == Some(body.len() - 1);
    if cursor < body.len() {
        cells.push(body[cursor..].to_string());
    }
    Parts {
        prefix,
        suffix,
        leading_pipe,
        trailing_pipe,
        cells,
    }
}

fn join_parts(parts: &Parts) -> String {
    let mut out = String::new();
    out.push_str(&parts.prefix);
    if parts.leading_pipe {
        out.push('|');
    }
    for (index, cell) in parts.cells.iter().enumerate() {
        if index > 0 {
            out.push('|');
        }
        out.push_str(cell);
    }
    if parts.trailing_pipe {
        out.push('|');
    }
    out.push_str(&parts.suffix);
    out
}

fn cell_spans(line_abs: usize, parts: &Parts) -> Vec<Range<usize>> {
    let mut cursor = line_abs + parts.prefix.len();
    if parts.leading_pipe {
        cursor += 1;
    }
    let mut spans = Vec::with_capacity(parts.cells.len());
    for (index, cell) in parts.cells.iter().enumerate() {
        let start = cursor;
        cursor += cell.len();
        spans.push(start..cursor);
        let more = index + 1 < parts.cells.len();
        if more || parts.trailing_pipe {
            cursor += 1;
        }
    }
    spans
}

fn column_at(parts: &Parts, line_abs: usize, pos: usize, col_count: usize) -> usize {
    let spans = cell_spans(line_abs, parts);
    if spans.is_empty() || col_count == 0 {
        return 0;
    }
    let last = col_count - 1;
    for index in 0..spans.len() {
        let next = spans
            .get(index + 1)
            .map(|next| next.start)
            .unwrap_or(usize::MAX);
        if pos < next {
            return index.min(last);
        }
    }
    spans.len().saturating_sub(1).min(last)
}

fn content_caret(line_abs: usize, parts: &Parts, col: usize) -> usize {
    let spans = cell_spans(line_abs, parts);
    if spans.is_empty() {
        return line_abs + parts.prefix.len();
    }
    let index = col.min(spans.len() - 1);
    let span = &spans[index];
    let cell = parts
        .cells
        .get(index)
        .map(|cell| cell.as_str())
        .unwrap_or("");
    let lead = cell.bytes().take_while(|byte| *byte == b' ').count();
    if lead == cell.len() {
        if cell.len() >= 2 {
            return span.start + 1;
        }
        return span.start;
    }
    span.start + lead
}

fn empty_like(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut lead = 0usize;
    while lead < bytes.len() && bytes[lead] == b' ' {
        lead += 1;
    }
    let mut trail = 0usize;
    while trail + lead < bytes.len() && bytes[bytes.len() - 1 - trail] == b' ' {
        trail += 1;
    }
    if lead + trail >= bytes.len() {
        return raw.to_string();
    }
    " ".repeat(lead + trail)
}

fn align_cell(raw: &str, align: TableColumnAlign) -> String {
    let bytes = raw.as_bytes();
    let mut lead = 0usize;
    while lead < bytes.len() && bytes[lead] == b' ' {
        lead += 1;
    }
    let mut trail = 0usize;
    while trail + lead < bytes.len() && bytes[bytes.len() - 1 - trail] == b' ' {
        trail += 1;
    }
    let token = &raw[lead..bytes.len() - trail];
    let hyphens = token.bytes().filter(|byte| *byte == b'-').count().max(3);
    let dashes = "-".repeat(hyphens);
    let rewritten = match align {
        TableColumnAlign::None => dashes,
        TableColumnAlign::Left => format!(":{dashes}"),
        TableColumnAlign::Right => format!("{dashes}:"),
        TableColumnAlign::Center => format!(":{dashes}:"),
    };
    format!(
        "{}{}{}",
        &raw[..lead],
        rewritten,
        &raw[bytes.len() - trail..]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(line: &str) {
        assert_eq!(join_parts(&split_parts(line)), line, "roundtrip {line:?}");
    }

    #[test]
    fn split_joins_the_spec_lines() {
        for line in [
            "| foo | bar |",
            "| --- | --- |",
            "bar | baz",
            "bar",
            "| f\\|oo  |",
            "| b `\\|` az |",
            "| `c|d` | e |",
            "| bar | baz | boo |",
            ":-: | -----------:",
            "| café | 東京 |",
            "  | foo |  ",
        ] {
            roundtrip(line);
        }
    }

    #[test]
    fn raw_cells_match_parser_ranges() {
        let src = "| a \\| b | c |\n| --- | --- |\n| `c|d` | e |\n| bar |\n| bar | baz | boo |";
        let parsed = parse_document(src);
        for line in &parsed.lines {
            if let LineKind::TableRow { raw_cells, .. } = &line.kind {
                let text = &src[line.source.clone()];
                let parts = split_parts(text);
                let spans = cell_spans(line.source.start, &parts);
                let comparable = spans.len().min(raw_cells.len());
                for index in 0..comparable {
                    assert_eq!(spans[index].clone(), raw_cells[index].clone(), "{text:?}");
                }
            }
        }
    }

    fn cell_start(content: &str, row: usize, col: usize) -> usize {
        let parsed = parse_document(content);
        let mut seen = 0usize;
        for line in &parsed.lines {
            if let LineKind::TableRow { cells, .. } = &line.kind {
                if seen == row {
                    return cells.get(col).map(|range| range.start).unwrap_or(0);
                }
                seen += 1;
            }
        }
        panic!("no cell {row},{col} in {content:?}");
    }

    fn apply(content: &str, row: usize, col: usize, op: TableOp) -> (String, usize) {
        let pos = cell_start(content, row, col);
        let plan = plan(content, pos, op).unwrap_or_else(|| panic!("op refused on {content:?}"));
        let mut out = content.to_string();
        out.replace_range(plan.range.clone(), &plan.new_text);
        (out, plan.caret)
    }

    #[test]
    fn gfm198_insert_row_below_keeps_the_example_bytes() {
        // Example 198, plus a newline the example's markdown block does not
        // require. The two original rows stay byte-identical.
        let src = "| foo | bar |\n| --- | --- |\n| baz | bim |\n";
        let (out, caret) = apply(src, 1, 0, TableOp::InsertRowBelow);
        let expect = "| foo | bar |\n| --- | --- |\n| baz | bim |\n|  |  |\n";
        assert_eq!(out, expect);
        assert_eq!(caret, cell_start(expect, 2, 0));
    }

    #[test]
    fn gfm200_escaped_pipe_survives_a_column_insert() {
        let src = "| f\\|oo  |\n| ------ |\n| b `\\|` az |\n| b **\\|** im |\n";
        let (out, _) = apply(src, 0, 0, TableOp::InsertColRight);
        assert!(out.contains("| f\\|oo  |"));
        assert!(out.contains("| b `\\|` az |"));
        assert!(out.contains("| b **\\|** im |"));
        assert!(out.contains("| ------ |"));
    }

    #[test]
    fn gfm204_keeps_the_extra_cell_and_the_short_row() {
        let src = "| abc | def |\n| --- | --- |\n| bar |\n| bar | baz | boo |\n";
        let (out, _) = apply(src, 1, 0, TableOp::InsertColRight);
        assert!(out.contains("| bar | baz | boo |") || out.contains("| bar |  | baz | boo |"));
        assert!(out.starts_with("| abc |"));
        assert!(out.contains("\n| bar |") || out.contains("\nbar|") || out.contains("\nbar\n"));
        let (deleted, _) = apply(src, 1, 0, TableOp::DeleteRow);
        assert_eq!(
            deleted,
            "| abc | def |\n| --- | --- |\n| bar | baz | boo |\n"
        );
    }

    #[test]
    fn header_is_not_deleted_or_moved_under_the_delimiter() {
        let src = "| A | B |\n| --- | --- |\n| d | e |\n| g | h |\n";
        let pos = cell_start(src, 0, 0);
        assert!(plan(src, pos, TableOp::DeleteRow).is_none());
        assert!(plan(src, pos, TableOp::InsertRowAbove).is_none());
        assert!(plan(src, pos, TableOp::MoveRow { to_index: 1 }).is_none());
        let body = cell_start(src, 1, 0);
        assert!(plan(src, body, TableOp::MoveRow { to_index: 0 }).is_none());
        assert_eq!(src, src);
    }

    #[test]
    fn single_column_delete_is_refused() {
        let src = "| A |\n| --- |\n| d |\n";
        let pos = cell_start(src, 1, 0);
        assert!(plan(src, pos, TableOp::DeleteCol).is_none());
    }

    #[test]
    fn align_rewrites_only_the_delimiter_cell() {
        // Example 199's colon rule on a padded delimiter: none → left is `:---`.
        let src = "| A | B |\n| --- | --- |\n| d | e |\n";
        let (out, caret) = apply(src, 1, 0, TableOp::SetAlign(TableColumnAlign::Left));
        assert_eq!(out, "| A | B |\n| :--- | --- |\n| d | e |\n");
        assert_eq!(caret, cell_start(&out, 1, 0));
        let (back, _) = apply(&out, 1, 0, TableOp::SetAlign(TableColumnAlign::None));
        assert_eq!(back, src);
    }

    #[test]
    fn crlf_slots_stay_crlf_and_eof_has_no_new_trailing_newline() {
        let src = "| A | B |\r\n| --- | --- |\r\n| d | e |\r\n| g | h |";
        let (out, _) = apply(src, 2, 0, TableOp::InsertRowBelow);
        assert_eq!(
            out,
            "| A | B |\r\n| --- | --- |\r\n| d | e |\r\n| g | h |\r\n|  |  |"
        );
        assert!(!out.ends_with('\n'));
    }

    #[test]
    fn quote_and_list_tables_are_not_found() {
        let quote = "> | A | B |\n> | --- | --- |\n> | d | e |\n";
        assert!(parsed_table_source(quote, 2).is_none());
        let list = "- | A | B |\n  | --- | --- |\n  | d | e |\n";
        assert!(plan(list, 2, TableOp::InsertRowBelow).is_none());
    }

    fn table_row_count(content: &str) -> usize {
        parse_document(content)
            .lines
            .iter()
            .filter(|line| matches!(line.kind, LineKind::TableRow { .. }))
            .count()
    }

    fn table_col_count(content: &str) -> usize {
        for line in &parse_document(content).lines {
            if let LineKind::TableRow { alignments, .. } = &line.kind {
                return alignments.len();
            }
        }
        0
    }

    /// GFM tables may omit the outer pipes (example 199's body, and
    /// `a | b` / `--|--` / `1 | 2`). An edit must not drop the table.
    #[test]
    fn no_outer_pipes_insert_col_left_keeps_the_table() {
        let src = "a | b\n--|--\n1 | 2\n";
        assert_eq!(table_row_count(src), 2);
        assert_eq!(table_col_count(src), 2);
        let (out, _) = apply(src, 0, 0, TableOp::InsertColLeft);
        assert_eq!(out, "| |a | b\n---|--|--\n| |1 | 2\n");
        assert_eq!(table_row_count(&out), 2);
        assert_eq!(table_col_count(&out), 3);
    }

    #[test]
    fn no_outer_pipes_insert_col_right_keeps_the_table() {
        let src = "a | b\n--|--\n1 | 2\n";
        let (out, _) = apply(src, 0, 1, TableOp::InsertColRight);
        assert_eq!(out, "a | b| |\n--|--|---\n1 | 2| |\n");
        assert_eq!(table_row_count(&out), 2);
        assert_eq!(table_col_count(&out), 3);
    }

    #[test]
    fn no_outer_pipes_delete_col_keeps_a_one_column_table() {
        let src = "a | b\n--|--\n1 | 2\n";
        let (out, _) = apply(src, 0, 0, TableOp::DeleteCol);
        assert_eq!(out, "| b|\n|--|\n| 2|\n");
        assert_eq!(table_row_count(&out), 2);
        assert_eq!(table_col_count(&out), 1);
    }

    #[test]
    fn no_outer_pipes_move_col_does_not_indent_into_a_code_block() {
        let src = "  b c  |  é😀  |\n:--|--:\n";
        assert_eq!(table_row_count(src), 1);
        let (out, _) = apply(src, 0, 0, TableOp::MoveCol { to_index: 1 });
        assert_eq!(out, "  |  é😀  |b c  |\n--:|:--\n");
        assert_eq!(table_row_count(&out), 1);
        for line in out.lines() {
            let lead = line.bytes().take_while(|byte| *byte == b' ').count();
            assert!(lead < 4, "move col indented {lead} spaces, line {line:?}");
        }
    }

    #[test]
    fn insert_row_below_a_single_cell_without_trailing_pipe_stays_a_table_row() {
        let src = "| a \n|---\n";
        assert_eq!(table_row_count(src), 1);
        let (out, _) = apply(src, 0, 0, TableOp::InsertRowBelow);
        assert_eq!(out, "| a \n|---\n| | \n");
        assert_eq!(table_row_count(&out), 2);
    }

    #[test]
    fn insert_row_below_a_pipe_less_table_stays_a_table_row() {
        let src = "a | b\n--|--\n1 | 2\n";
        let (out, caret) = apply(src, 1, 0, TableOp::InsertRowBelow);
        assert_eq!(out, "a | b\n--|--\n1 | 2\n| | |\n");
        assert_eq!(table_row_count(&out), 3);
        assert_eq!(table_col_count(&out), 2);
        let plan = plan(&out, caret, TableOp::DeleteRow)
            .unwrap_or_else(|| panic!("delete of the new row refused on {out:?}"));
        let mut restored = out;
        restored.replace_range(plan.range, &plan.new_text);
        assert_eq!(restored, src);
    }

    /// Escaped pipes are cell text. Deleting a column must not drop body rows.
    #[test]
    fn delete_col_keeps_body_rows_when_a_cell_holds_an_escaped_pipe() {
        let src = "a \\| b | c\n---|---\n1 \\| 2 | 3\n";
        assert_eq!(table_row_count(src), 2);
        assert_eq!(table_col_count(src), 2);
        let (out, _) = apply(src, 0, 1, TableOp::DeleteCol);
        assert_eq!(out, "|a \\| b |\n|---|\n|1 \\| 2 |\n");
        assert_eq!(table_row_count(&out), 2);
        assert_eq!(table_col_count(&out), 1);
    }
}
