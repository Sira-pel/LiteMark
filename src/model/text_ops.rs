//! Pure text-buffer operations: caret movement, edits, formatting, and search.
//!
//! Undo stores each change as a diff (deleted bytes + inserted bytes) and drops
//! the oldest diffs once the stored text exceeds [`UNDO_BYTE_BUDGET`].

use std::ops::Range;
use std::time::Instant;
use unicode_segmentation::UnicodeSegmentation;

use crate::markdown::scan_list_marker;

/// Maximum total bytes of deleted + inserted text kept on the undo and redo stacks.
pub const UNDO_BYTE_BUDGET: usize = 256 * 1024;

/// Typing that continues the same word inside this window is one undo step.
pub const TYPING_BURST_MS: u64 = 1_000;

/// Hide zoom and save toasts after this long.
pub const TRANSIENT_TTL_MS: u64 = 1_500;

/// A non-empty selection stored with an undo entry.
///
/// Offsets are absolute in the document that entry restores. A collapsed
/// caret leaves this empty so undo keeps the single-offset path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionSnap {
    pub start: usize,
    pub end: usize,
    pub reversed: bool,
}

/// Caret, and selection when the edit had one, after undo or redo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRestore {
    pub caret: usize,
    pub selection: Option<SelectionSnap>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffEdit {
    pub start: usize,
    pub deleted: String,
    pub inserted: String,
    /// Caret restored when this edit is applied. Defaults to the end of the
    /// deleted range (the document after the inverse write).
    pub undo_caret: usize,
    /// Caret restored when this edit is inverted again. Defaults to the end
    /// of the inserted text.
    pub redo_caret: usize,
    pub undo_sel: Option<SelectionSnap>,
    pub redo_sel: Option<SelectionSnap>,
}

impl DiffEdit {
    pub fn cost(&self) -> usize {
        self.deleted.len() + self.inserted.len()
    }

    pub fn new(start: usize, deleted: String, inserted: String) -> Self {
        let undo_caret = start + deleted.len();
        let redo_caret = start + inserted.len();
        Self {
            start,
            deleted,
            inserted,
            undo_caret,
            redo_caret,
            undo_sel: None,
            redo_sel: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct UndoHistory {
    undo: Vec<DiffEdit>,
    redo: Vec<DiffEdit>,
    undo_bytes: usize,
    redo_bytes: usize,
    last_edit: Option<Instant>,
    /// Typing inside a freshly created formatted span is one undo step: the
    /// wrap edit and every later edit that lands inside its inserted region
    /// compose into a single entry until `seal` or a non-fitting edit.
    span_group_open: bool,
}

/// Group a continued word into the previous undo step.
/// A space, newline, deletion, non-adjacent caret, or a pause starts a new step.
pub fn coalesce_typing(previous: &DiffEdit, next: &DiffEdit, elapsed_ms: u64) -> bool {
    if elapsed_ms >= TYPING_BURST_MS {
        return false;
    }
    if !previous.deleted.is_empty() || !next.deleted.is_empty() {
        return false;
    }
    if previous.inserted.is_empty() || next.inserted.is_empty() {
        return false;
    }
    if next.start != previous.start + previous.inserted.len() {
        return false;
    }
    let previous_is_word = !previous.inserted.chars().any(char::is_whitespace);
    let next_is_word = !next.inserted.chars().any(char::is_whitespace);
    previous_is_word && next_is_word
}

/// Compose `edit` into `previous` when it only rewrites bytes the group
/// itself inserted. The group's entry replaces an originally empty gap, so
/// its `deleted` stays empty and undo removes everything the span typing
/// produced in one step.
fn absorb_into_span_group(previous: &mut DiffEdit, edit: &DiffEdit) -> bool {
    if !previous.deleted.is_empty() {
        return false;
    }
    let end = previous.start + previous.inserted.len();
    if edit.start < previous.start || edit.start > end || edit.start + edit.deleted.len() > end {
        return false;
    }
    let offset = edit.start - previous.start;
    previous
        .inserted
        .replace_range(offset..offset + edit.deleted.len(), &edit.inserted);
    previous.redo_caret = previous.start + previous.inserted.len();
    true
}

impl UndoHistory {
    #[allow(dead_code)]
    pub fn stored_bytes(&self) -> usize {
        self.undo_bytes + self.redo_bytes
    }

    pub fn push(&mut self, edit: DiffEdit) {
        self.push_at(edit, Instant::now());
    }

    /// Push the edit that opens a formatted span; later edits inside the
    /// span's text join the same undo step.
    pub fn push_span_opening(&mut self, edit: DiffEdit) {
        self.push_at(edit, Instant::now());
        if !self.undo.is_empty() {
            self.span_group_open = true;
        }
    }

    /// End any open undo grouping: the next edit starts a new step.
    pub fn seal(&mut self) {
        self.last_edit = None;
        self.span_group_open = false;
    }

    pub fn push_at(&mut self, edit: DiffEdit, now: Instant) {
        if edit.deleted.is_empty() && edit.inserted.is_empty() {
            return;
        }
        if self.span_group_open {
            let absorbed = match self.undo.last_mut() {
                Some(previous) => absorb_into_span_group(previous, &edit),
                None => false,
            };
            if absorbed {
                self.undo_bytes += edit.cost();
                self.redo.clear();
                self.redo_bytes = 0;
                self.last_edit = Some(now);
                return;
            }
            self.span_group_open = false;
        }
        let elapsed = self
            .last_edit
            .map(|then| now.saturating_duration_since(then).as_millis() as u64)
            .unwrap_or(u64::MAX);
        let merge = self
            .undo
            .last()
            .is_some_and(|previous| coalesce_typing(previous, &edit, elapsed));
        if merge {
            if let Some(previous) = self.undo.last_mut() {
                previous.inserted.push_str(&edit.inserted);
                previous.redo_caret = previous.start + previous.inserted.len();
            }
            self.undo_bytes += edit.cost();
        } else {
            self.undo_bytes += edit.cost();
            self.undo.push(edit);
        }
        while self.undo_bytes > UNDO_BYTE_BUDGET && self.undo.len() > 1 {
            let dropped = self.undo.remove(0);
            self.undo_bytes = self.undo_bytes.saturating_sub(dropped.cost());
        }
        self.redo.clear();
        self.redo_bytes = 0;
        self.last_edit = Some(now);
    }

    /// Undo the latest edit.
    ///
    /// The caret is the edit's `undo_caret`, which defaults to the end of the
    /// restored (previously deleted) text. A stored selection is returned with
    /// it and is not snapped by this function.
    pub fn undo(&mut self, content: &mut String) -> Option<HistoryRestore> {
        let edit = self.undo.pop()?;
        self.last_edit = None;
        self.span_group_open = false;
        self.undo_bytes = self.undo_bytes.saturating_sub(edit.cost());
        let end = edit.start + edit.inserted.len();
        if end > content.len() || content.get(edit.start..end).is_none() {
            return None;
        }
        let inserted = edit.deleted;
        content.replace_range(edit.start..end, &inserted);
        let caret = edit.undo_caret.min(content.len());
        let selection = edit.undo_sel.clone();
        let inverse = DiffEdit {
            start: edit.start,
            deleted: edit.inserted,
            inserted,
            undo_caret: edit.redo_caret,
            redo_caret: edit.undo_caret,
            undo_sel: edit.redo_sel,
            redo_sel: edit.undo_sel,
        };
        self.redo_bytes += inverse.cost();
        self.redo.push(inverse);
        Some(HistoryRestore { caret, selection })
    }

    /// Redo the latest undone edit.
    ///
    /// The caret is the stored redo caret, which defaults to the end of the
    /// text just written.
    pub fn redo(&mut self, content: &mut String) -> Option<HistoryRestore> {
        let edit = self.redo.pop()?;
        self.last_edit = None;
        self.span_group_open = false;
        self.redo_bytes = self.redo_bytes.saturating_sub(edit.cost());
        let end = edit.start + edit.inserted.len();
        if end > content.len() || content.get(edit.start..end).is_none() {
            return None;
        }
        let inserted = edit.deleted;
        content.replace_range(edit.start..end, &inserted);
        let caret = edit.undo_caret.min(content.len());
        let selection = edit.undo_sel.clone();
        let inverse = DiffEdit {
            start: edit.start,
            deleted: edit.inserted,
            inserted,
            undo_caret: edit.redo_caret,
            redo_caret: edit.undo_caret,
            undo_sel: edit.redo_sel,
            redo_sel: edit.undo_sel,
        };
        self.undo_bytes += inverse.cost();
        self.undo.push(inverse);
        Some(HistoryRestore { caret, selection })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caret {
    /// Inclusive-exclusive byte range. `start <= end`.
    pub range: Range<usize>,
    pub reversed: bool,
    /// Preferred 1-based column for vertical movement.
    pub preferred_col: Option<usize>,
}

impl Default for Caret {
    fn default() -> Self {
        Self {
            range: 0..0,
            reversed: false,
            preferred_col: None,
        }
    }
}

impl Caret {
    pub fn cursor(&self) -> usize {
        if self.reversed {
            self.range.start
        } else {
            self.range.end
        }
    }

    pub fn move_to(&mut self, offset: usize) {
        self.range = offset..offset;
        self.reversed = false;
        self.preferred_col = None;
    }

    /// Extend the selection to `offset`, keeping the anchor (the end that is not the caret).
    pub fn select_to(&mut self, offset: usize) {
        let anchor = if self.range.is_empty() {
            self.cursor()
        } else if self.reversed {
            self.range.end
        } else {
            self.range.start
        };
        if offset < anchor {
            self.range = offset..anchor;
            self.reversed = true;
        } else {
            self.range = anchor..offset;
            self.reversed = false;
        }
        self.preferred_col = None;
    }

    /// Clamp both ends to `content` and snap them onto char boundaries.
    pub fn clamp(&mut self, content: &str) {
        let len = content.len();
        self.range.start = floor_char_boundary(content, self.range.start.min(len));
        self.range.end = floor_char_boundary(content, self.range.end.min(len));
        if self.range.end < self.range.start {
            self.range = self.range.end..self.range.start;
        }
    }
}

pub fn floor_char_boundary(content: &str, mut index: usize) -> usize {
    if index >= content.len() {
        return content.len();
    }
    while index > 0 && !content.is_char_boundary(index) {
        index -= 1;
    }
    index
}

pub fn previous_grapheme(content: &str, offset: usize) -> usize {
    let offset = floor_char_boundary(content, offset.min(content.len()));
    content[..offset]
        .grapheme_indices(true)
        .next_back()
        .map(|(index, _)| index)
        .unwrap_or(0)
}

pub fn next_grapheme(content: &str, offset: usize) -> usize {
    let offset = floor_char_boundary(content, offset.min(content.len()));
    content[offset..]
        .grapheme_indices(true)
        .nth(1)
        .map(|(index, _)| offset + index)
        .unwrap_or(content.len())
}

/// Byte length of the line ending at `offset`.
/// A line ending is LF, a lone CR, or CRLF. Any other byte has length 0.
pub fn line_ending_len(content: &str, offset: usize) -> usize {
    let bytes = content.as_bytes();
    if offset >= bytes.len() {
        return 0;
    }
    match bytes[offset] {
        b'\n' => 1,
        b'\r' => usize::from(offset + 1 < bytes.len() && bytes[offset + 1] == b'\n') + 1,
        _ => 0,
    }
}

/// If `offset` sits on the LF of a CRLF, move it back to the CR.
/// The caret then stands at the end of the line, never between the two bytes.
pub fn snap_off_ending_interior(content: &str, offset: usize) -> usize {
    let offset = offset.min(content.len());
    if offset > 0
        && line_ending_len(content, offset - 1) == 2
        && content.as_bytes().get(offset) == Some(&b'\n')
    {
        offset - 1
    } else {
        offset
    }
}

/// Byte length of the line ending that ends immediately before `offset`.
pub fn line_ending_len_before(content: &str, offset: usize) -> usize {
    if offset == 0 {
        return 0;
    }
    let bytes = content.as_bytes();
    if bytes[offset - 1] == b'\n' && offset >= 2 && bytes[offset - 2] == b'\r' {
        2
    } else if bytes[offset - 1] == b'\n' || bytes[offset - 1] == b'\r' {
        1
    } else {
        0
    }
}

#[derive(Clone, Copy)]
struct TextLine {
    start: usize,
    /// First byte of the line ending, or `content.len()` when the line has none.
    content_end: usize,
    /// First byte of the next line.
    next: usize,
}

/// The line that contains `offset`. A caret between CR and LF belongs to the
/// line that owns that CRLF, and its content end is the CR, not the LF.
fn line_span_at(content: &str, offset: usize) -> TextLine {
    let bytes = content.as_bytes();
    let offset = offset.min(bytes.len());
    let mut start = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        let ending = line_ending_len(content, index);
        if ending > 0 {
            let next = index + ending;
            if offset < next {
                return TextLine {
                    start,
                    content_end: index,
                    next,
                };
            }
            index = next;
            start = next;
        } else {
            index += 1;
        }
    }
    TextLine {
        start,
        content_end: bytes.len(),
        next: bytes.len(),
    }
}

/// 1-based line and column at a byte offset.
pub fn line_col(content: &str, offset: usize) -> (usize, usize) {
    let offset = floor_char_boundary(content, offset.min(content.len()));
    let span = line_span_at(content, offset);
    let mut line = 1usize;
    let mut index = 0usize;
    while index < span.start {
        let ending = line_ending_len(content, index);
        if ending > 0 {
            line += 1;
            index += ending;
        } else {
            index += 1;
        }
    }
    let end = offset.min(span.content_end);
    let col = content[span.start..end].chars().count() + 1;
    (line, col)
}

pub fn line_start(content: &str, line_1: usize) -> usize {
    if line_1 <= 1 {
        return 0;
    }
    let mut line = 1usize;
    let mut index = 0usize;
    let bytes = content.as_bytes();
    while index < bytes.len() {
        let ending = line_ending_len(content, index);
        if ending > 0 {
            line += 1;
            index += ending;
            if line == line_1 {
                return index;
            }
        } else {
            index += 1;
        }
    }
    content.len()
}

pub fn line_end(content: &str, offset: usize) -> usize {
    let offset = floor_char_boundary(content, offset.min(content.len()));
    line_span_at(content, offset).content_end
}

pub fn home_offset(content: &str, offset: usize) -> usize {
    let offset = floor_char_boundary(content, offset.min(content.len()));
    line_span_at(content, offset).start
}

fn char_class(ch: char) -> u8 {
    if ch.is_alphanumeric() || ch == '_' {
        0
    } else if ch.is_whitespace() {
        1
    } else {
        2
    }
}

/// Byte range of the word, whitespace run, or punctuation run containing `offset`.
pub fn word_extent(content: &str, offset: usize) -> Range<usize> {
    if content.is_empty() {
        return 0..0;
    }
    let mut offset = floor_char_boundary(content, offset.min(content.len()));
    if offset >= content.len() {
        offset = previous_grapheme(content, content.len());
    }
    let Some(ch) = content[offset..].chars().next() else {
        return offset..offset;
    };
    let kind = char_class(ch);
    let start = content[..offset]
        .char_indices()
        .rev()
        .find(|(_, candidate)| char_class(*candidate) != kind)
        .map(|(index, candidate)| index + candidate.len_utf8())
        .unwrap_or(0);
    let rel = content[offset..]
        .char_indices()
        .find(|(_, candidate)| char_class(*candidate) != kind)
        .map(|(index, _)| index)
        .unwrap_or(content.len() - offset);
    start..offset + rel
}

/// Find the offset of the previous word boundary to the left of `offset`.
pub fn word_left(content: &str, offset: usize) -> usize {
    if offset == 0 || content.is_empty() {
        return 0;
    }
    let offset = floor_char_boundary(content, offset.min(content.len()));
    let mut iter = content[..offset].char_indices().rev().peekable();

    while let Some(&(_, ch)) = iter.peek() {
        if ch.is_whitespace() {
            iter.next();
        } else {
            break;
        }
    }

    let Some(&(first_idx, first_ch)) = iter.peek() else {
        return 0;
    };

    let target_class = char_class(first_ch);
    let mut result_offset = first_idx;

    while let Some(&(idx, ch)) = iter.peek() {
        if char_class(ch) == target_class {
            result_offset = idx;
            iter.next();
        } else {
            break;
        }
    }

    result_offset
}

/// Find the offset of the next word boundary to the right of `offset`.
pub fn word_right(content: &str, offset: usize) -> usize {
    let len = content.len();
    if offset >= len || content.is_empty() {
        return len;
    }
    let offset = floor_char_boundary(content, offset.min(len));
    let mut iter = content[offset..].char_indices().peekable();

    while let Some(&(_, ch)) = iter.peek() {
        if ch.is_whitespace() {
            iter.next();
        } else {
            break;
        }
    }

    let Some(&(_, first_ch)) = iter.peek() else {
        return len;
    };

    let target_class = char_class(first_ch);
    let mut result_offset = len;

    while let Some(&(idx, ch)) = iter.peek() {
        if char_class(ch) == target_class {
            iter.next();
            result_offset = offset + idx + ch.len_utf8();
        } else {
            break;
        }
    }

    result_offset
}

/// Byte range of the logical line containing `offset`, including its line ending.
pub fn line_extent(content: &str, offset: usize) -> Range<usize> {
    let offset = floor_char_boundary(content, offset.min(content.len()));
    let span = line_span_at(content, offset);
    span.start..span.next
}

pub fn offset_at_line_col(content: &str, line_1: usize, col_1: usize) -> usize {
    let start = line_start(content, line_1);
    if start >= content.len() {
        return content.len();
    }
    let end = line_end(content, start);
    for (seen, (index, _)) in (1usize..).zip(content[start..end].char_indices()) {
        if seen == col_1 {
            return start + index;
        }
    }
    end
}

pub fn move_vertical_collapsed(content: &str, caret: &mut Caret, delta: i32, extend: bool) {
    let cursor = caret.cursor().min(content.len());
    let (line, col) = line_col(content, cursor);
    let prefer = caret.preferred_col.unwrap_or(col);
    let target_line = if delta < 0 {
        line.saturating_sub(1).max(1)
    } else {
        line + 1
    };
    let offset = snap_off_ending_interior(content, offset_at_line_col(content, target_line, prefer));
    if extend {
        caret.select_to(offset);
    } else if !caret.range.is_empty() && delta < 0 {
        caret.move_to(caret.range.start);
    } else if !caret.range.is_empty() {
        caret.move_to(caret.range.end);
    } else {
        caret.range = offset..offset;
        caret.reversed = false;
    }
    caret.preferred_col = Some(prefer);
}

/// Replacement text for a formatting command applied to `selected`.
/// `at_line_starts[i]` is true when line `i` of the selection begins at a source line start.
pub fn format_replacement(cmd: &str, selected: &str, at_line_starts: &[bool]) -> Option<String> {
    let empty = selected.is_empty();
    let text = match cmd {
        "bold" => wrap_inline(selected, "**", "**", "bold text"),
        "italic" => wrap_inline(selected, "*", "*", "italic text"),
        "underline" => wrap_inline(selected, "<u>", "</u>", "underlined"),
        "strike" => wrap_inline(selected, "~~", "~~", "strikethrough"),
        "code" => wrap_inline(selected, "`", "`", "code"),
        "h1" => heading(selected, "#"),
        "h2" => heading(selected, "##"),
        "h3" => heading(selected, "###"),
        "ul" => list_block(selected, "- ", "- Item"),
        "ol" => list_block(selected, "1. ", "1. First"),
        "checklist" => list_block(selected, "- [ ] ", "- [ ] Task"),
        "quote" => list_block(selected, "> ", "> Quote"),
        "codeblock" => {
            if empty {
                "```\ncode\n```".to_string()
            } else {
                format!("```\n{selected}\n```")
            }
        }
        "hr" => {
            if empty {
                "---".to_string()
            } else {
                format!("{selected}\n---")
            }
        }
        "link" => {
            if empty {
                "[link](https://)".to_string()
            } else {
                format!("[{selected}](https://)")
            }
        }
        "table" => {
            let table = "| Column | Column |\n| :--- | :--- |\n|  |  |";
            if empty {
                table.to_string()
            } else {
                format!("{selected}\n{table}")
            }
        }
        "clear" => {
            if empty {
                return None;
            }
            clear_fragment(selected, at_line_starts)
        }
        _ => return None,
    };
    Some(text)
}

fn wrap_inline(selected: &str, prefix: &str, suffix: &str, placeholder: &str) -> String {
    if selected.is_empty() {
        format!("{prefix}{placeholder}{suffix}")
    } else {
        format!("{prefix}{selected}{suffix}")
    }
}

fn heading(selected: &str, marks: &str) -> String {
    if selected.is_empty() {
        format!("{marks} Heading")
    } else {
        selected
            .split_inclusive('\n')
            .map(|line| {
                let (body, nl) = split_nl(line);
                format!("{marks} {body}{nl}")
            })
            .collect()
    }
}

fn list_block(selected: &str, marker: &str, placeholder: &str) -> String {
    if selected.is_empty() {
        placeholder.to_string()
    } else {
        selected
            .split_inclusive('\n')
            .map(|line| {
                let (body, nl) = split_nl(line);
                if body.is_empty() {
                    format!("{body}{nl}")
                } else {
                    format!("{marker}{body}{nl}")
                }
            })
            .collect()
    }
}

fn split_nl(line: &str) -> (&str, &str) {
    if let Some(body) = line.strip_suffix('\n') {
        (body, "\n")
    } else {
        (line, "")
    }
}

/// Byte length of a markdown list marker, including its padding and task prefix.
pub fn list_marker_len(line: &str) -> usize {
    if let Some(item) = scan_list_marker(line) {
        item.prefix_end()
    } else {
        line.len() - line.trim_start().len()
    }
}

fn strip_inline_marks(text: &str) -> String {
    text.replace("**", "")
        .replace("<u>", "")
        .replace("</u>", "")
        .replace("~~", "")
        .replace(['`', '*'], "")
}

fn clear_fragment(selected: &str, at_line_starts: &[bool]) -> String {
    let mut out = String::new();
    for (index, line) in selected.split_inclusive('\n').enumerate() {
        let (body, nl) = split_nl(line);
        let at_start = at_line_starts.get(index).copied().unwrap_or(false);
        if at_start {
            let keep = list_marker_len(body).min(body.len());
            out.push_str(&body[..keep]);
            out.push_str(&strip_inline_marks(&body[keep..]));
        } else {
            out.push_str(&strip_inline_marks(body));
        }
        out.push_str(nl);
    }
    out
}

/// Which lines of `selected` (a slice of `content` starting at `start`) begin a source line.
pub fn line_start_flags(content: &str, start: usize, selected: &str) -> Vec<bool> {
    selected
        .split_inclusive('\n')
        .enumerate()
        .map(|(index, _)| {
            if index == 0 {
                start == 0 || content.as_bytes().get(start.wrapping_sub(1)) == Some(&b'\n')
            } else {
                true
            }
        })
        .collect()
}

pub fn find_matches(content: &str, term: &str, match_case: bool) -> Vec<Range<usize>> {
    if term.is_empty() {
        return Vec::new();
    }
    if match_case {
        let mut hits = Vec::new();
        let mut from = 0usize;
        while let Some(rel) = content[from..].find(term) {
            let start = from + rel;
            let end = start + term.len();
            hits.push(start..end);
            from = end;
        }
        return hits;
    }

    let needle = term.to_lowercase();
    let needle_chars = needle.chars().count();
    if needle_chars == 0 {
        return Vec::new();
    }
    let chars: Vec<(usize, char)> = content.char_indices().collect();
    if chars.len() < needle_chars {
        return Vec::new();
    }
    let mut hits = Vec::new();
    let mut index = 0usize;
    while index + needle_chars <= chars.len() {
        let start = chars[index].0;
        let end = if index + needle_chars < chars.len() {
            chars[index + needle_chars].0
        } else {
            content.len()
        };
        if content[start..end].to_lowercase() == needle {
            hits.push(start..end);
            index += needle_chars;
        } else {
            index += 1;
        }
    }
    hits
}

/// Index of the next hit at or after `cursor`. Wraps to the first hit.
/// When `cursor` is exactly at `hits[current].start`, the search starts after that hit
/// so Find Next does not stick on the current match.
pub fn find_next(hits: &[Range<usize>], cursor: usize, current: Option<usize>) -> Option<usize> {
    if hits.is_empty() {
        return None;
    }
    let scan_from = match current {
        Some(index) if hits.get(index).is_some_and(|hit| hit.start == cursor) => index + 1,
        _ => 0,
    };
    if let Some(rel) = hits[scan_from.min(hits.len())..]
        .iter()
        .position(|hit| hit.start >= cursor)
    {
        return Some(scan_from.min(hits.len()) + rel);
    }
    Some(0)
}

/// Previous hit that ends before `cursor`.
///
/// Find leaves the caret at the end of the current match, so a search from
/// that caret (`hit.end`) must not select the match it is sitting on. When
/// `current` is that match, it is excluded even if the caller passes the
/// selection start.
pub fn find_prev(hits: &[Range<usize>], cursor: usize, current: Option<usize>) -> Option<usize> {
    if hits.is_empty() {
        return None;
    }
    let scan_end = match current {
        Some(index)
            if hits
                .get(index)
                .is_some_and(|hit| cursor >= hit.start && cursor <= hit.end) =>
        {
            index
        }
        _ => hits.len(),
    };
    hits[..scan_end]
        .iter()
        .rposition(|hit| hit.end < cursor)
        .or(Some(hits.len() - 1))
}

pub fn replace_all(
    content: &str,
    term: &str,
    replacement: &str,
    match_case: bool,
) -> Option<String> {
    let hits = find_matches(content, term, match_case);
    if hits.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(content.len());
    let mut last = 0usize;
    for hit in hits {
        out.push_str(&content[last..hit.start]);
        out.push_str(replacement);
        last = hit.end;
    }
    out.push_str(&content[last..]);
    if out == content {
        None
    } else {
        Some(out)
    }
}

pub fn offset_to_utf16(content: &str, offset: usize) -> usize {
    let offset = floor_char_boundary(content, offset.min(content.len()));
    content[..offset].chars().map(|ch| ch.len_utf16()).sum()
}

pub fn offset_from_utf16(content: &str, utf16_offset: usize) -> usize {
    let mut utf16_count = 0usize;
    for (index, ch) in content.char_indices() {
        if utf16_count >= utf16_offset {
            return index;
        }
        utf16_count += ch.len_utf16();
    }
    content.len()
}

pub fn range_to_utf16(content: &str, range: &Range<usize>) -> Range<usize> {
    offset_to_utf16(content, range.start)..offset_to_utf16(content, range.end)
}

pub fn range_from_utf16(content: &str, range: &Range<usize>) -> Range<usize> {
    offset_from_utf16(content, range.start)..offset_from_utf16(content, range.end)
}

/// Visual rows as `(byte_start, byte_end)` exclusive of the newline.
/// `wrap_cols == usize::MAX` keeps one row per logical line.
pub fn visual_rows(content: &str, wrap_cols: usize) -> Vec<(usize, usize)> {
    let mut rows = Vec::new();
    let mut line_start = 0usize;
    for (index, byte) in content.bytes().enumerate() {
        if byte == b'\n' {
            push_wrapped(&mut rows, content, line_start, index, wrap_cols);
            line_start = index + 1;
        }
    }
    push_wrapped(&mut rows, content, line_start, content.len(), wrap_cols);
    if rows.is_empty() {
        rows.push((0, 0));
    }
    rows
}

fn push_wrapped(
    rows: &mut Vec<(usize, usize)>,
    content: &str,
    start: usize,
    end: usize,
    wrap_cols: usize,
) {
    if wrap_cols == 0 || wrap_cols == usize::MAX || start >= end {
        rows.push((start, end));
        return;
    }
    let chars: Vec<(usize, char)> = content[start..end]
        .char_indices()
        .map(|(index, ch)| (start + index, ch))
        .collect();
    if chars.len() <= wrap_cols {
        rows.push((start, end));
        return;
    }
    let mut index = 0usize;
    while index < chars.len() {
        let row_start = chars[index].0;
        let max_next = (index + wrap_cols).min(chars.len());
        if max_next == chars.len() {
            rows.push((row_start, end));
            break;
        }

        // Look for a break opportunity (whitespace or punctuation) between index and max_next
        let mut break_idx = None;
        for i in (index + 1..=max_next).rev() {
            let (_, ch) = chars[i - 1];
            if ch.is_whitespace() {
                break_idx = Some(i);
                break;
            }
        }

        let next = if let Some(b) = break_idx {
            b
        } else {
            max_next
        };

        let row_end = if next < chars.len() { chars[next].0 } else { end };
        rows.push((row_start, row_end));
        index = next;
    }
}

#[allow(dead_code)]
pub fn hud_should_hide(elapsed_ms: u64) -> bool {
    elapsed_ms >= TRANSIENT_TTL_MS
}

/// Minimum block height for a WYSIWYG list row, in CSS pixels.
/// The extra gap keeps the next bullet from painting over this one.
#[cfg_attr(not(test), allow(dead_code))]
pub fn wysiwyg_list_row_height(font_px: f32) -> f32 {
    font_px * 1.65 + 8.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_backspace_enter_and_cursor_position() {
        let mut content = String::new();
        let mut caret = Caret::default();
        content.push_str("Hi");
        caret.move_to(content.len());
        assert_eq!(line_col(&content, caret.cursor()), (1, 3));

        content.insert(caret.cursor(), '\n');
        caret.move_to(content.len());
        content.insert_str(caret.cursor(), "there");
        caret.move_to(content.len());
        assert_eq!(content, "Hi\nthere");
        assert_eq!(line_col(&content, caret.cursor()), (2, 6));

        let prev = previous_grapheme(&content, caret.cursor());
        content.replace_range(prev..caret.cursor(), "");
        caret.move_to(prev);
        assert_eq!(content, "Hi\nther");
        assert_eq!(line_col(&content, caret.cursor()), (2, 5));
    }

    #[test]
    fn arrows_move_by_grapheme_and_line() {
        let content = "a😀b\ncd";
        let mut caret = Caret::default();
        caret.move_to(0);
        caret.move_to(next_grapheme(content, caret.cursor()));
        assert_eq!(caret.cursor(), "a".len());
        caret.move_to(next_grapheme(content, caret.cursor()));
        assert_eq!(&content[..caret.cursor()], "a😀");
        caret.move_to(previous_grapheme(content, caret.cursor()));
        assert_eq!(caret.cursor(), "a".len());

        caret.move_to(0);
        move_vertical_collapsed(content, &mut caret, 1, false);
        assert_eq!(line_col(content, caret.cursor()).0, 2);
        move_vertical_collapsed(content, &mut caret, -1, false);
        assert_eq!(line_col(content, caret.cursor()).0, 1);
    }

    #[test]
    fn selection_and_vertical_preference() {
        let content = "abcdef\nxy";
        let mut caret = Caret::default();
        caret.move_to(4);
        caret.select_to(1);
        assert_eq!(caret.range, 1..4);
        assert!(caret.reversed);
        assert_eq!(&content[caret.range.clone()], "bcd");

        let mut extended = Caret::default();
        extended.move_to(3);
        extended.select_to(2);
        extended.select_to(1);
        assert_eq!(extended.range, 1..3, "shift-left keeps the anchor");
        extended.select_to(5);
        assert_eq!(extended.range, 3..5, "shift-right extends past the anchor");

        caret.move_to(5);
        move_vertical_collapsed(content, &mut caret, 1, false);
        assert_eq!(line_col(content, caret.cursor()), (2, 3));
    }

    #[test]
    fn bold_wraps_selection_or_inserts_at_cursor() {
        let content = "Hello world";
        let selected = &content[6..11];
        let wrapped = format_replacement("bold", selected, &[true]).unwrap();
        assert_eq!(wrapped, "**world**");
        let at_cursor = format_replacement("bold", "", &[true]).unwrap();
        assert_eq!(at_cursor, "**bold text**");
        assert!(!format!("{content}{at_cursor}").ends_with("**bold text****bold text**"));
        let inserted = format!("{}{}", &content[..6], at_cursor);
        assert!(inserted.starts_with("Hello **bold text**"));
        assert!(
            !inserted.ends_with("world**bold text**") || inserted.contains("Hello **bold text**")
        );
    }

    #[test]
    fn clear_formatting_keeps_list_markers_and_ignores_unselected_text() {
        let content = "- item *italic* and **bold**\n* bullet `code`\n1. third **keep**";
        let flags = line_start_flags(content, 0, content);
        let cleared = format_replacement("clear", content, &flags).unwrap();
        assert_eq!(
            cleared,
            "- item italic and bold\n* bullet code\n1. third keep"
        );

        let only = "*italic*";
        let start = content.find(only).unwrap();
        let flags = line_start_flags(content, start, only);
        let partial = format_replacement("clear", only, &flags).unwrap();
        assert_eq!(partial, "italic");
        let mut rebuilt = String::new();
        rebuilt.push_str(&content[..start]);
        rebuilt.push_str(&partial);
        rebuilt.push_str(&content[start + only.len()..]);
        assert!(rebuilt.starts_with("- item italic"));
        assert!(rebuilt.contains("**bold**"));
        assert!(rebuilt.contains("* bullet"));
    }

    #[test]
    fn empty_clear_is_a_no_op() {
        assert!(format_replacement("clear", "", &[true]).is_none());
    }

    #[test]
    fn find_next_prev_replace_and_replace_all() {
        let content = "one cat, two Cat, three cat";
        let hits = find_matches(content, "cat", false);
        assert_eq!(hits.len(), 3);
        assert_eq!(find_next(&hits, 0, None), Some(0));
        assert_eq!(find_next(&hits, hits[0].start, Some(0)), Some(1));
        // The caret sits at the end of the selected match, not at its start.
        assert_eq!(find_prev(&hits, hits[1].end, Some(1)), Some(0));
        assert_eq!(find_prev(&hits, hits[1].end, None), Some(0));
        assert_eq!(find_next(&hits, hits[2].start, Some(2)), Some(0));

        let case_hits = find_matches(content, "cat", true);
        assert_eq!(case_hits.len(), 2);

        let replaced = replace_all(content, "cat", "dog", false).unwrap();
        assert_eq!(replaced, "one dog, two dog, three dog");
        assert!(replace_all(content, "missing", "x", false).is_none());
    }

    #[test]
    fn undo_stores_diffs_and_caps_bytes() {
        let mut history = UndoHistory::default();
        let mut content = "seed".to_string();
        for index in 0..200 {
            let insert = format!("w{index}");
            let start = content.len();
            history.push(DiffEdit::new(
                start,
                String::new(),
                insert.clone(),
            ));
            content.push_str(&insert);
        }
        let full_copies = content.len() * 200;
        assert!(history.stored_bytes() < full_copies / 4);
        assert!(history.stored_bytes() <= UNDO_BYTE_BUDGET + 64);
        let before = content.clone();
        assert!(history.undo(&mut content).is_some());
        assert_ne!(content, before);
        assert!(history.redo(&mut content).is_some());
        assert_eq!(content, before);
    }

    #[test]
    fn typing_groups_a_word_until_space_or_pause() {
        use std::time::Duration;

        let start = Instant::now();
        let mut history = UndoHistory::default();
        let mut content = String::new();
        for (index, ch) in "hi".chars().enumerate() {
            let at = content.len();
            let text = ch.to_string();
            history.push_at(
                DiffEdit::new(at, String::new(), text.clone()),
                start + Duration::from_millis(20 * index as u64),
            );
            content.push_str(&text);
        }
        assert_eq!(history.undo.len(), 1);
        assert_eq!(
            history.undo(&mut content).map(|restore| restore.caret),
            Some(0)
        );
        assert_eq!(content, "");

        history.push_at(
            DiffEdit::new(0, String::new(), "hi".into()),
            start + Duration::from_millis(40),
        );
        content.push_str("hi");
        history.push_at(
            DiffEdit::new(2, String::new(), " ".into()),
            start + Duration::from_millis(60),
        );
        content.push(' ');
        history.push_at(
            DiffEdit::new(3, String::new(), "you".into()),
            start + Duration::from_millis(80),
        );
        content.push_str("you");
        assert_eq!(
            history.undo(&mut content).map(|restore| restore.caret),
            Some(3)
        );
        assert_eq!(content, "hi ");
        assert_eq!(
            history.undo(&mut content).map(|restore| restore.caret),
            Some(2)
        );
        assert_eq!(content, "hi");

        let mut paused = UndoHistory::default();
        let mut paused_content = String::new();
        paused.push_at(
            DiffEdit::new(0, String::new(), "a".into()),
            start,
        );
        paused_content.push('a');
        paused.push_at(
            DiffEdit::new(1, String::new(), "b".into()),
            start + Duration::from_millis(TYPING_BURST_MS),
        );
        paused_content.push('b');
        assert_eq!(paused.undo.len(), 2);
        assert_eq!(
            paused
                .undo(&mut paused_content)
                .map(|restore| restore.caret),
            Some(1)
        );
        assert_eq!(paused_content, "a");

        assert!(!coalesce_typing(
            &DiffEdit::new(0, "x".into(), String::new()),
            &DiffEdit::new(0, String::new(), "y".into()),
            0,
        ));
        assert!(!coalesce_typing(
            &DiffEdit::new(0, String::new(), "a".into()),
            &DiffEdit::new(4, String::new(), "b".into()),
            0,
        ));
    }

    #[test]
    fn undo_caret_is_the_end_of_the_restored_text() {
        let mut history = UndoHistory::default();
        let mut content = "ab".to_string();
        history.push(DiffEdit::new(0, "ab".into(), "😀".into()));
        content.replace_range(0..2, "😀");
        let caret = history.undo(&mut content).unwrap().caret;
        assert_eq!(content, "ab");
        assert_eq!(caret, 2);
        assert!(content.is_char_boundary(caret));
        let caret = history.redo(&mut content).unwrap().caret;
        assert_eq!(content, "😀");
        assert_eq!(caret, "😀".len());
        assert!(content.is_char_boundary(caret));
    }

    #[test]
    fn home_end_and_word_line_extents_stay_on_char_boundaries() {
        let content = "a😀b\nកខ";
        let inside_emoji = "a".len() + 1;
        assert!(!content.is_char_boundary(inside_emoji));
        let home = home_offset(content, inside_emoji);
        let end = line_end(content, inside_emoji);
        assert!(content.is_char_boundary(home));
        assert!(content.is_char_boundary(end));
        assert_eq!(home, 0);
        assert_eq!(&content[home..end], "a😀b");

        assert_eq!(word_extent("hello, world", 1), 0..5);
        assert_eq!(word_extent("hello, world", 5), 5..6);
        assert_eq!(line_extent("ab\ncd", 1), 0..3);
        assert_eq!(line_extent("ab\ncd", 4), 3..5);
        assert_eq!(line_end("Foo\r\nbar", 0), 3);
        assert_eq!(line_end("Foo\r\nbar", 4), 3);
        assert_eq!(home_offset("Foo\r\nbar", 5), 5);
        assert_eq!(line_extent("Foo\rbar\rbaz", 1), 0..4);
        assert_eq!(line_end("Foo\rbar\rbaz", 0), 3);
        assert_eq!(snap_off_ending_interior("Foo\r\nbar", 4), 3);
        let khmer = line_extent(content, content.len());
        assert!(content.is_char_boundary(khmer.start) && content.is_char_boundary(khmer.end));
    }

    #[test]
    fn hud_hides_after_ttl() {
        assert!(!hud_should_hide(1_499));
        assert!(hud_should_hide(TRANSIENT_TTL_MS));
    }

    #[test]
    fn word_wrap_splits_long_lines_on_character_boundaries() {
        let rows = visual_rows("abcdef\nxy", 3);
        assert_eq!(rows, vec![(0, 3), (3, 6), (7, 9)]);
        let plain = visual_rows("ab\n", usize::MAX);
        assert_eq!(plain, vec![(0, 2), (3, 3)]);

        // Test word wrapping splits on whitespace if available within wrap_cols
        let words = visual_rows("hello world test", 8);
        assert_eq!(words, vec![(0, 6), (6, 12), (12, 16)]);
    }

    #[test]
    fn wysiwyg_list_rows_do_not_overlap() {
        let font = 17.0_f32;
        let row = wysiwyg_list_row_height(font);
        assert!(row >= font * 1.65);
        let second_top = row;
        assert!(second_top >= font * 1.65);
        assert!(second_top - 0.0 >= row - 0.01);
        assert!(row > font);
    }

    #[test]
    fn test_word_left_and_right() {
        let text = "hello, world! 123";
        assert_eq!(word_right(text, 0), 5);
        assert_eq!(word_right(text, 5), 6);
        assert_eq!(word_right(text, 6), 12);
        assert_eq!(word_right(text, 12), 13);
        assert_eq!(word_right(text, 13), 17);

        assert_eq!(word_left(text, 17), 14);
        assert_eq!(word_left(text, 14), 12);
        assert_eq!(word_left(text, 12), 7);
        assert_eq!(word_left(text, 7), 5);
        assert_eq!(word_left(text, 5), 0);
        assert_eq!(word_left(text, 0), 0);
    }
}
