//! List and task markers, and the list walk that decides which markers are accepted.

use super::*;

/// One list marker, shared by the parser and the editing helpers.
///
/// Ordered markers are 1–9 digits and then `.` or `)`. Padding after `.`, `)`,
/// or a bullet is 1–4 columns (tabs use a stop of 4). Five or more columns
/// keep one column in the marker and leave the rest as content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListMarker {
    pub indent_bytes: usize,
    pub ordered: bool,
    pub number: usize,
    pub punct: u8,
    /// Byte offset where the marker and its padding end.
    pub marker_end: usize,
    pub task: Option<TaskMarker>,
}

/// GFM task marker. `checkbox_at` is the `[`. `end` is after the required
/// whitespace and any further run of tab, VT, FF, or space.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskMarker {
    pub checked: bool,
    pub checkbox_at: usize,
    pub end: usize,
}

impl ListMarker {
    pub fn prefix_end(&self) -> usize {
        self.task
            .as_ref()
            .map(|task| task.end)
            .unwrap_or(self.marker_end)
    }

    /// The next Enter continuation, including its leading newline.
    pub fn continuation(&self, indent: &str) -> String {
        let punct = self.punct as char;
        if self.task.is_some() {
            if self.ordered {
                format!("\n{indent}{}{punct} [ ] ", self.number + 1)
            } else {
                format!("\n{indent}{punct} [ ] ")
            }
        } else if self.ordered {
            format!("\n{indent}{}{punct} ", self.number + 1)
        } else {
            format!("\n{indent}{punct} ")
        }
    }
}

pub fn scan_list_marker(line: &str) -> Option<ListMarker> {
    let bytes = line.as_bytes();
    let indent_bytes = line.len() - line.trim_start().len();
    let mut col = ColScan::new(bytes);
    col.skip_indent(indent_bytes);
    if col.ix >= bytes.len() {
        return None;
    }
    let (ordered, number, punct) = if matches!(bytes[col.ix], b'-' | b'+' | b'*') {
        let punct = bytes[col.ix];
        col.ix += 1;
        if col.scan_space(1) || col.is_at_eol() {
            (false, 0, punct)
        } else {
            return None;
        }
    } else if bytes[col.ix].is_ascii_digit() {
        parse_ordered_marker(&mut col)?
    } else {
        return None;
    };
    let marker_end = finish_list_padding(&mut col);
    let task = task_marker_at(bytes, marker_end);
    Some(ListMarker {
        indent_bytes,
        ordered,
        number,
        punct,
        marker_end,
        task,
    })
}

pub(super) fn parse_ordered_marker(col: &mut ColScan<'_>) -> Option<(bool, usize, u8)> {
    let bytes = col.bytes;
    let start = col.ix;
    let mut ix = start + 1;
    let mut value = u64::from(bytes[start] - b'0');
    while ix < bytes.len() && ix - start < 10 {
        let byte = bytes[ix];
        ix += 1;
        if byte.is_ascii_digit() {
            value = value * 10 + u64::from(byte - b'0');
        } else if byte == b'.' || byte == b')' {
            col.ix = ix;
            if col.scan_space(1) || col.is_at_eol() {
                return Some((true, value as usize, byte));
            }
            return None;
        } else {
            return None;
        }
    }
    None
}

/// Consume 0–3 extra padding columns after the required separator.
/// Four extra columns (five total) stay with the content.
pub(super) fn finish_list_padding(col: &mut ColScan<'_>) -> usize {
    let saved = *col;
    if rest_is_blank(&col.bytes[col.ix..]) {
        return col.ix;
    }
    let post_indent = col.scan_space_upto(4);
    if post_indent < 4 {
        col.ix
    } else {
        *col = saved;
        col.ix
    }
}

pub(super) fn task_marker_at(bytes: &[u8], at: usize) -> Option<TaskMarker> {
    if bytes.get(at) != Some(&b'[') {
        return None;
    }
    let checked = match bytes.get(at + 1) {
        Some(&byte) if is_ascii_whitespace_no_nl(byte) => false,
        Some(b'x') | Some(b'X') => true,
        _ => return None,
    };
    if bytes.get(at + 2) != Some(&b']') {
        return None;
    }
    let after = at + 3;
    let follows = bytes
        .get(after)
        .copied()
        .is_some_and(is_ascii_whitespace_no_nl);
    if !follows {
        return None;
    }
    let mut end = after + 1;
    while end < bytes.len() && is_ascii_whitespace_no_nl(bytes[end]) {
        end += 1;
    }
    Some(TaskMarker {
        checked,
        checkbox_at: at,
        end,
    })
}

/// Columns of leading spaces and tabs. A tab advances to the next stop of 4.
pub(super) fn leading_column(line: &str) -> usize {
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

pub(super) fn column_at(line: &str, byte_index: usize) -> usize {
    let mut column = 0usize;
    for (index, byte) in line.bytes().enumerate() {
        if index >= byte_index {
            break;
        }
        if byte == b'\t' {
            column += 4 - (column % 4);
        } else {
            column += 1;
        }
    }
    column
}

pub(super) fn line_is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

/// Open list items, so a marker can be refused when it would interrupt a
/// paragraph or when it starts at column 4 or later outside a parent item.
pub(super) struct ListWalk {
    pub(super) open: Vec<usize>,
    /// Paragraph-open flag per container. Index 0 is the document. Index
    /// `i + 1` is the content of `open[i]`.
    pub(super) para: Vec<bool>,
}

impl Default for ListWalk {
    fn default() -> Self {
        Self {
            open: Vec::new(),
            para: vec![false],
        }
    }
}

pub(super) fn marker_is_empty(line: &str, item: &ListMarker) -> bool {
    item.task.is_none() && rest_is_blank(&line.as_bytes()[item.marker_end..])
}

/// Red keeps a marker after exactly one tab (`\t- foo`) as a list. Four
/// spaces, and a space followed by a tab, stay text.
pub(super) fn single_tab_marker(line: &str) -> bool {
    let bytes = line.as_bytes();
    bytes.first() == Some(&b'\t') && bytes.get(1).is_some_and(|byte| *byte != b' ' && *byte != b'\t')
}

/// A line the block parser records as a heading, rule, fence, quote, or table
/// rather than as paragraph text. Those lines close a list item.
///
/// Headings and thematic breaks use the same classifiers as the line loop.
/// `parent_col` is the list content column, or 0 at document level.
pub(super) fn is_structural_line(line: &str, parent_col: usize) -> bool {
    if atx_heading(line).is_some() || exact_thematic_break(line, parent_col) {
        return true;
    }
    let trimmed = line.trim();
    if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
        return true;
    }
    let trimmed_start = line.trim_start();
    if trimmed_start.starts_with("> ") || trimmed_start == ">" {
        return true;
    }
    trimmed.starts_with('|') && trimmed.ends_with('|') && trimmed.len() >= 2
}

impl ListWalk {
    pub(super) fn in_paragraph(&self) -> bool {
        self.para.last().copied().unwrap_or(false)
    }

    pub(super) fn set_paragraph(&mut self, open: bool) {
        if let Some(flag) = self.para.last_mut() {
            *flag = open;
        }
    }

    /// A dedented line stays in the item when it is paragraph continuation
    /// text (a lazy continuation). A marker that would start a list in the
    /// parent container closes the item instead.
    pub(super) fn continues_lazily(&self, line: &str) -> bool {
        if !self.in_paragraph() {
            return false;
        }
        if is_structural_line(line, self.thematic_parent_col(line)) {
            return false;
        }
        let Some(item) = scan_list_marker(line) else {
            return true;
        };
        let marker_col = leading_column(line);
        let parent_depth = self.open.len().saturating_sub(1);
        let nested = parent_depth > 0;
        if marker_col > 3 && !nested && !single_tab_marker(line) {
            return true;
        }
        let parent_para = self.para.get(parent_depth).copied().unwrap_or(false);
        let empty = marker_is_empty(line, &item);
        parent_para && (empty || (item.ordered && item.number != 1))
    }

    pub(super) fn prepare(&mut self, line: &str) {
        if line_is_blank(line) {
            return;
        }
        let column = leading_column(line);
        while self
            .open
            .last()
            .copied()
            .is_some_and(|content| column < content)
        {
            if self.continues_lazily(line) {
                return;
            }
            self.open.pop();
            self.para.pop();
        }
    }

    /// A list marker this line is allowed to be, or nothing.
    ///
    /// CommonMark list items, rule 1 exception: an empty marker cannot
    /// interrupt a paragraph, and an ordered marker interrupts only when its
    /// start number is 1. That includes a paragraph inside a parent item
    /// (examples 285, 304, 367). A blank line ends that paragraph, so the
    /// same marker can then open a nested list. A lazy continuation stays in
    /// the item, and a marker after it can start a new list (example 283,
    /// including a CRLF empty item). Rule 4: a marker at column 4 or later is
    /// indented code unless it sits in a parent item (examples 1, 2, 107, and
    /// the nested marker in 296).
    pub(super) fn take_list(&mut self, line: &str) -> Option<ListMarker> {
        self.prepare(line);
        let item = scan_list_marker(line)?;
        let marker_col = leading_column(line);
        let nested = !self.open.is_empty();
        if marker_col > 3 && !nested && !single_tab_marker(line) {
            return None;
        }
        let empty = marker_is_empty(line, &item);
        if self.in_paragraph() && (empty || (item.ordered && item.number != 1)) {
            return None;
        }
        let has_text = !rest_is_blank(&line.as_bytes()[item.prefix_end()..]);
        self.open.push(column_at(line, item.marker_end));
        self.para.push(has_text);
        let parent = self.para.len() - 2;
        self.para[parent] = false;
        Some(item)
    }

    pub(super) fn note_block(&mut self, line: &str) {
        self.prepare(line);
        self.set_paragraph(false);
    }

    pub(super) fn note_blank(&mut self) {
        self.set_paragraph(false);
    }

    pub(super) fn note_paragraph(&mut self) {
        self.set_paragraph(true);
    }

    /// Content column of the deepest open item this line is indented into.
    /// Document level is 0, so four leading spaces stay indented text.
    pub(super) fn thematic_parent_col(&self, line: &str) -> usize {
        let column = leading_column(line);
        self.open
            .iter()
            .rev()
            .copied()
            .find(|content| column >= *content)
            .unwrap_or(0)
    }
}

/// The list marker `parse_document` keeps for the line that begins at `line_start`.
pub fn accepted_list_marker(markdown: &str, line_start: usize) -> Option<ListMarker> {
    accepted_list_marker_in(&parse_document(markdown), markdown, line_start)
}

/// [`accepted_list_marker`] against `doc`, the parse of `markdown`, so
/// callers that already hold the parse do not pay for another one.
pub fn accepted_list_marker_in(
    doc: &ParsedDocument,
    markdown: &str,
    line_start: usize,
) -> Option<ListMarker> {
    let line = doc
        .lines
        .iter()
        .find(|line| line.source.start == line_start)?;
    if !matches!(line.kind, LineKind::ListItem { .. }) {
        return None;
    }
    scan_list_marker(&markdown[line.source.start..line.source.end])
}

/// Wysiwyg checkbox click. An unchecked box (space, tab, VT, or FF) becomes
/// `[x]`. A ticked box becomes `[ ]`.
#[cfg(test)]
pub fn checkbox_click_replacement(current: &str) -> &'static str {
    let bytes = current.as_bytes();
    if bytes.len() == 3 && bytes[0] == b'[' && bytes[2] == b']' {
        if is_ascii_whitespace_no_nl(bytes[1]) {
            return "[x]";
        }
        if bytes[1] == b'x' || bytes[1] == b'X' {
            return "[ ]";
        }
    }
    "[ ]"
}
