//! Pure text-buffer operations: caret movement, edits, formatting, and search.
//!
//! Undo stores each change as a diff (deleted bytes + inserted bytes) and drops
//! the oldest diffs once the stored text exceeds [`UNDO_BYTE_BUDGET`].

use std::ops::Range;
use std::time::Instant;
use unicode_segmentation::UnicodeSegmentation;

use crate::markdown::scan_list_marker;

/// Maximum total bytes of deleted + inserted text kept on the undo and redo stacks.
/// Large enough that one whole-document step (Select All + type on a
/// multi-megabyte note) survives the next keystroke; small next to the
/// parse cache of the same note.
pub const UNDO_BYTE_BUDGET: usize = 8 * 1024 * 1024;

/// Typing that continues the same word inside this window is one undo step.
pub const TYPING_BURST_MS: u64 = 1_000;

/// Columns between tab stops when a code or plain-text tab shows a `\t`.
pub const TAB_DISPLAY_COLUMNS: usize = 4;

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
    /// The bytes replaced in the content and the length written there.
    pub replaced: (std::ops::Range<usize>, usize),
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
    #[cfg(test)]
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
            let mut resized = None;
            let absorbed = match self.undo.last_mut() {
                Some(previous) => {
                    let before = previous.cost();
                    let ok = absorb_into_span_group(previous, &edit);
                    resized = Some((before, previous.cost()));
                    ok
                }
                None => false,
            };
            if absorbed {
                if let Some((before, after)) = resized {
                    self.undo_bytes = (self.undo_bytes + after).saturating_sub(before);
                }
                // A group that ended up empty (everything it inserted was
                // taken back) is no undo step at all.
                if self
                    .undo
                    .last()
                    .is_some_and(|p| p.deleted.is_empty() && p.inserted.is_empty())
                {
                    self.undo.pop();
                    self.span_group_open = false;
                }
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
        let replaced = (edit.start..end, inserted.len());
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
        Some(HistoryRestore {
            caret,
            selection,
            replaced,
        })
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
        let replaced = (edit.start..end, inserted.len());
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
        Some(HistoryRestore {
            caret,
            selection,
            replaced,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caret {
    /// Inclusive-exclusive byte range. `start <= end`.
    pub range: Range<usize>,
    pub reversed: bool,
    /// Preferred 1-based column for vertical movement.
    pub preferred_col: Option<usize>,
    /// Horizontal position (1/16 px) that Up/Down aim for, valid only while
    /// the caret still sits at the offset the vertical move put it on.
    pub preferred_x: Option<(usize, i32)>,
}

impl Default for Caret {
    fn default() -> Self {
        Self {
            range: 0..0,
            reversed: false,
            preferred_col: None,
            preferred_x: None,
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
        self.preferred_x = None;
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
        self.preferred_x = None;
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
    // Look only around `offset`: callers run this once per line of a
    // selection, and scanning from the top made those commands O(n²).
    let bytes = content.as_bytes();
    let offset = offset.min(bytes.len());
    let is_eol = |b: &u8| *b == b'\n' || *b == b'\r';
    // A caret between CR and LF: that CR ends this line, not the previous one.
    let between_crlf =
        offset > 0 && bytes.get(offset) == Some(&b'\n') && bytes[offset - 1] == b'\r';
    let look_back = if between_crlf { offset - 1 } else { offset };
    let start = bytes[..look_back]
        .iter()
        .rposition(is_eol)
        .map_or(0, |eol| eol + 1);
    match bytes[start..].iter().position(is_eol) {
        Some(rel) => {
            let content_end = start + rel;
            TextLine {
                start,
                content_end,
                next: content_end + line_ending_len(content, content_end),
            }
        }
        None => TextLine {
            start,
            content_end: bytes.len(),
            next: bytes.len(),
        },
    }
}

/// 1-based line and column at a byte offset.
pub fn line_col(content: &str, offset: usize) -> (usize, usize) {
    let offset = floor_char_boundary(content, offset.min(content.len()));
    let span = line_span_at(content, offset);
    let line = 1 + count_line_endings(&content.as_bytes()[..span.start]);
    let end = offset.min(span.content_end);
    let col = content[span.start..end].chars().count() + 1;
    (line, col)
}

/// A byte offset and what a count reached there: the 1-based line for
/// [`line_col_near`] (always at a line start), the UTF-16 offset for the
/// `*_utf16_near` functions. Lookups near the caret pass the last anchor
/// back in and count only the bytes in between, not the whole prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CountAnchor {
    pub byte: usize,
    pub count: usize,
}

/// [`line_col`], counting lines from `anchor` when given. Returns the anchor
/// for the next lookup: the start of the offset's line.
pub fn line_col_near(
    content: &str,
    offset: usize,
    anchor: Option<CountAnchor>,
) -> ((usize, usize), CountAnchor) {
    let offset = floor_char_boundary(content, offset.min(content.len()));
    let span = line_span_at(content, offset);
    let bytes = content.as_bytes();
    // Both ends are line starts, never between a CR and its LF, so the
    // endings counted between them are exactly the lines between them.
    let line = match anchor.filter(|a| a.byte <= bytes.len()) {
        Some(a) if a.byte <= span.start => a.count + count_line_endings(&bytes[a.byte..span.start]),
        Some(a) => a.count - count_line_endings(&bytes[span.start..a.byte]),
        None => 1 + count_line_endings(&bytes[..span.start]),
    };
    let end = offset.min(span.content_end);
    let col = content[span.start..end].chars().count() + 1;
    (
        (line, col),
        CountAnchor {
            byte: span.start,
            count: line,
        },
    )
}

fn utf16_len(text: &str) -> usize {
    if text.is_ascii() {
        text.len()
    } else {
        text.chars().map(char::len_utf16).sum()
    }
}

/// [`offset_to_utf16`], counting from `anchor` when given.
pub fn offset_to_utf16_near(
    content: &str,
    offset: usize,
    anchor: Option<CountAnchor>,
) -> (usize, CountAnchor) {
    let offset = floor_char_boundary(content, offset.min(content.len()));
    let utf16 = match anchor.filter(|a| a.byte <= content.len()) {
        Some(a) if a.byte <= offset => a.count + utf16_len(&content[a.byte..offset]),
        Some(a) => a.count - utf16_len(&content[offset..a.byte]),
        None => utf16_len(&content[..offset]),
    };
    let next = CountAnchor {
        byte: offset,
        count: utf16,
    };
    (utf16, next)
}

/// [`offset_from_utf16`], counting from `anchor` when given: the first char
/// boundary whose UTF-16 offset is at least `utf16_offset`.
pub fn offset_from_utf16_near(
    content: &str,
    utf16_offset: usize,
    anchor: Option<CountAnchor>,
) -> (usize, CountAnchor) {
    let a = anchor
        .filter(|a| a.byte <= content.len())
        .unwrap_or(CountAnchor { byte: 0, count: 0 });
    let (mut byte, mut count) = (a.byte, a.count);
    if count < utf16_offset {
        for ch in content[byte..].chars() {
            if count >= utf16_offset {
                break;
            }
            count += ch.len_utf16();
            byte += ch.len_utf8();
        }
    } else {
        // Step back while the previous boundary still reaches the target.
        while let Some(ch) = content[..byte].chars().next_back() {
            let before = count - ch.len_utf16();
            if before < utf16_offset {
                break;
            }
            byte -= ch.len_utf8();
            count = before;
        }
    }
    (byte, CountAnchor { byte, count })
}

/// Line endings in `bytes`: each LF, and each CR not followed by an LF (a
/// CRLF counts once). Runs on every caret move in raw tabs, so it uses
/// memchr rather than a byte-by-byte walk.
pub fn count_line_endings(bytes: &[u8]) -> usize {
    memchr::memchr2_iter(b'\n', b'\r', bytes)
        .filter(|&at| bytes[at] == b'\n' || bytes.get(at + 1) != Some(&b'\n'))
        .count()
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
    let offset =
        snap_off_ending_interior(content, offset_at_line_col(content, target_line, prefer));
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
        "h4" => heading(selected, "####"),
        "h5" => heading(selected, "#####"),
        "h6" => heading(selected, "######"),
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

    find_matches_lowered(content, &term.to_lowercase())
}

/// Case-insensitive search for `needle`, which the caller has already
/// lowercased, so a loop over many lines folds the term once.
pub fn find_matches_lowered(content: &str, needle: &str) -> Vec<Range<usize>> {
    if needle.is_empty() {
        return Vec::new();
    }
    let mut hits = Vec::new();
    let mut start = 0usize;
    let first_ch = needle.chars().next().unwrap();
    if first_ch.is_ascii() {
        let lower = first_ch as u8;
        let upper = lower.to_ascii_uppercase();
        // Only two non-ASCII Unicode scalar values lowercase to a sequence
        // starting with an ASCII byte: U+0130 ('İ' -> "i\u{307}", lead 0xC4)
        // and U+212A ('K' -> "k", lead 0xE2). UTF-8 lead bytes (>= 0xC0) and
        // ASCII bytes (< 0x80) can never appear mid-character, so `pos` is
        // always a valid char boundary.
        let extra_lead = match lower {
            b'i' => Some(0xC4u8),
            b'k' => Some(0xE2u8),
            _ => None,
        };
        let bytes = content.as_bytes();
        while start < bytes.len() {
            let rel = match extra_lead {
                Some(lead) => memchr::memchr3(lower, upper, lead, &bytes[start..]),
                None if lower == upper => memchr::memchr(lower, &bytes[start..]),
                None => memchr::memchr2(lower, upper, &bytes[start..]),
            };
            let Some(rel) = rel else {
                break;
            };
            let pos = start + rel;
            let rest = &content[pos..];
            if let Some(len) = caseless_match_len(rest, needle) {
                hits.push(pos..pos + len);
                start = pos + len;
            } else {
                start = pos + rest.chars().next().map_or(1, char::len_utf8);
            }
        }
        return hits;
    }

    while start < content.len() {
        let rest = &content[start..];
        if let Some(len) = caseless_match_len(rest, needle) {
            hits.push(start..start + len);
            start += len;
        } else {
            start += rest.chars().next().map_or(1, char::len_utf8);
        }
    }
    hits
}

/// First match of a term at or after a byte position, one at a time, so a
/// caller can stop scanning as soon as it has what it needs.
struct Matcher {
    term: String,
    needle: String,
    match_case: bool,
}

impl Matcher {
    fn new(term: &str, match_case: bool) -> Option<Self> {
        let needle = if match_case {
            String::new()
        } else {
            term.to_lowercase()
        };
        if term.is_empty() || (!match_case && needle.is_empty()) {
            return None;
        }
        Some(Self {
            term: term.to_string(),
            needle,
            match_case,
        })
    }

    /// The longest source text one match can span, in bytes. A caseless
    /// match can be longer than the needle: Kelvin `K` is 3 bytes and
    /// lowercases to a 1-byte `k`.
    fn max_len(&self) -> usize {
        if self.match_case {
            self.term.len()
        } else {
            self.needle.len() * 3 + 4
        }
    }

    fn next(&self, content: &str, from: usize) -> Option<Range<usize>> {
        if self.match_case {
            let rel = content.get(from..)?.find(&self.term)?;
            let start = from + rel;
            return Some(start..start + self.term.len());
        }
        // Same scan as `find_matches_lowered`, stopping at the first hit.
        let mut start = from;
        while start < content.len() {
            let rest = &content[start..];
            // Skip to a byte that can begin a match when that is cheap.
            let first = self.needle.chars().next()?;
            if first.is_ascii() {
                let lower = first as u8;
                let upper = lower.to_ascii_uppercase();
                let extra_lead = match lower {
                    b'i' => Some(0xC4u8),
                    b'k' => Some(0xE2u8),
                    _ => None,
                };
                let bytes = rest.as_bytes();
                let rel = match extra_lead {
                    Some(lead) => memchr::memchr3(lower, upper, lead, bytes),
                    None if lower == upper => memchr::memchr(lower, bytes),
                    None => memchr::memchr2(lower, upper, bytes),
                }?;
                start += rel;
            }
            let rest = &content[start..];
            if let Some(len) = caseless_match_len(rest, &self.needle) {
                return Some(start..start + len);
            }
            start += rest.chars().next().map_or(1, char::len_utf8);
        }
        None
    }
}

/// The matches of `term` in `content` after one edit, given the matches
/// before it. `edited` is the replaced range in the old text and `new_len`
/// the length of the text that replaced it. Equal to a fresh
/// [`find_matches`], but only the neighbourhood of the edit is searched.
///
/// Matching is greedy and non-overlapping, so an edit can re-pair the hits
/// after it (`aa` in `aaaa` shifts by one inserted `a`). The scan therefore
/// runs forward until a new hit lands exactly on a shifted old one: from
/// there the greedy state, and every later hit, is the same as before.
pub fn update_matches(
    old: &[Range<usize>],
    content: &str,
    term: &str,
    match_case: bool,
    edited: Range<usize>,
    new_len: usize,
) -> Vec<Range<usize>> {
    let Some(matcher) = Matcher::new(term, match_case) else {
        return Vec::new();
    };
    let (start, old_end) = (edited.start, edited.end);
    let new_end = start + new_len;
    // Hits wholly before the edit are untouched; those overlapping it are
    // gone; those wholly after it are candidates for resync.
    let kept = old.partition_point(|hit| hit.end <= start);
    let suffix_from = old.partition_point(|hit| hit.start < old_end);
    let suffix = &old[suffix_from.max(kept)..];
    let shift = |at: usize| (at + new_len).wrapping_sub(edited.len());
    let mut out: Vec<Range<usize>> = old[..kept].to_vec();
    // No match can begin before the last kept hit's end, and none can begin
    // so early that it would have ended before the edit (it would be kept).
    let floor = out.last().map_or(0, |hit| hit.end);
    let from = floor.max(floor_char_boundary(
        content,
        start.saturating_sub(matcher.max_len()),
    ));
    let mut pos = from;
    while let Some(hit) = matcher.next(content, pos) {
        if hit.start >= new_end {
            // Old coordinates of this hit, if it is one of the old ones.
            let old_start = hit.start.wrapping_add(edited.len()).wrapping_sub(new_len);
            if let Ok(ix) = suffix.binary_search_by(|o| o.start.cmp(&old_start)) {
                if shift(suffix[ix].end) == hit.end {
                    out.push(hit);
                    out.extend(
                        suffix[ix + 1..]
                            .iter()
                            .map(|o| shift(o.start)..shift(o.end)),
                    );
                    return out;
                }
            }
        }
        pos = hit.end;
        out.push(hit);
    }
    out
}

/// Byte length of the shortest start of `text` whose lowercase is `needle`,
/// without allocating. The match ends when the needle is used up at a
/// character boundary, so a character that lowercases to two (`İ` → `i̇`)
/// can match, and a needle never matches half of one.
fn caseless_match_len(text: &str, needle: &str) -> Option<usize> {
    let mut want = needle.chars().peekable();
    for (index, ch) in text.char_indices() {
        if want.peek().is_none() {
            return Some(index);
        }
        if ch == '\u{03A3}' {
            // Capital sigma lowercases by context (final sigma), which only
            // str::to_lowercase knows. Compare the shortest window from the
            // match start that lowercases to at least the needle's length.
            let mut end = index;
            for (at, c) in text[index..].char_indices() {
                end = index + at + c.len_utf8();
                if text[..end].to_lowercase().len() >= needle.len() {
                    break;
                }
            }
            return (text[..end].to_lowercase() == needle).then_some(end);
        }
        for lower in ch.to_lowercase() {
            if want.next() != Some(lower) {
                return None;
            }
        }
    }
    want.peek().is_none().then_some(text.len())
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
    // Hits are sorted and disjoint, so binary-search the first at/after the cursor.
    let from = scan_from.min(hits.len());
    let rel = hits[from..].partition_point(|hit| hit.start < cursor);
    if from + rel < hits.len() {
        return Some(from + rel);
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
        .partition_point(|hit| hit.end < cursor)
        .checked_sub(1)
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

#[cfg(test)]
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

#[cfg(test)]
pub fn range_to_utf16(content: &str, range: &Range<usize>) -> Range<usize> {
    let start = floor_char_boundary(content, range.start.min(content.len()));
    let end = floor_char_boundary(content, range.end.min(content.len()));
    let start_u16: usize = content[..start].chars().map(char::len_utf16).sum();
    if end == start {
        return start_u16..start_u16;
    }
    if end > start {
        let delta: usize = content[start..end].chars().map(char::len_utf16).sum();
        return start_u16..(start_u16 + delta);
    }
    let end_u16: usize = content[..end].chars().map(char::len_utf16).sum();
    start_u16..end_u16
}

pub fn range_from_utf16(content: &str, range: &Range<usize>) -> Range<usize> {
    if range.end < range.start {
        return offset_from_utf16(content, range.start)..offset_from_utf16(content, range.end);
    }
    let mut utf16_count = 0usize;
    let mut start_byte = None;
    for (index, ch) in content.char_indices() {
        if start_byte.is_none() && utf16_count >= range.start {
            start_byte = Some(index);
            if range.end == range.start {
                return index..index;
            }
        }
        if utf16_count >= range.end {
            return start_byte.unwrap_or(index)..index;
        }
        utf16_count += ch.len_utf16();
    }
    let start_byte = start_byte.unwrap_or(content.len());
    start_byte..content.len()
}

/// Visual rows as `(byte_start, byte_end)` exclusive of the line ending (`\r\n`, `\n`, or `\r`).
/// `wrap_cols == usize::MAX` keeps one row per logical line.
#[cfg(test)]
pub fn visual_rows(content: &str, wrap_cols: usize) -> Vec<(usize, usize)> {
    visual_rows_tabbed(content, wrap_cols, 0)
}

/// Like `visual_rows`, but a `\t` counts as the columns up to the next
/// `tab_width` stop (measured from the row start). `tab_width` 0 counts a tab as one column.
#[cfg(test)]
pub fn visual_rows_tabbed(
    content: &str,
    wrap_cols: usize,
    tab_width: usize,
) -> Vec<(usize, usize)> {
    visual_rows_measured(content, wrap_cols, tab_width, &CharWidths::uniform())
}

/// Like `visual_rows_tabbed`, but `wrap_cols` is a width budget in the units
/// of `widths`, so narrow letters pack more to a row than wide ones.
pub fn visual_rows_measured(
    content: &str,
    wrap_cols: usize,
    tab_width: usize,
    widths: &CharWidths,
) -> Vec<(usize, usize)> {
    let mut rows = Vec::new();
    let bytes = content.as_bytes();
    let mut line_start = 0usize;
    while let Some(rel) = memchr::memchr2(b'\n', b'\r', &bytes[line_start..]) {
        let line_end = line_start + rel;
        push_wrapped(
            &mut rows, content, line_start, line_end, wrap_cols, tab_width, widths,
        );
        line_start = if bytes[line_end] == b'\r' && bytes.get(line_end + 1) == Some(&b'\n') {
            line_end + 2
        } else {
            line_end + 1
        };
    }
    push_wrapped(
        &mut rows,
        content,
        line_start,
        content.len(),
        wrap_cols,
        tab_width,
        widths,
    );
    if rows.is_empty() {
        rows.push((0, 0));
    }
    rows
}

/// Start of the line holding the byte at `at` (CRLF counts as one ending).
fn line_start_at(content: &str, at: usize) -> usize {
    let bytes = content.as_bytes();
    let mut at = at.min(bytes.len());
    if at < bytes.len() && bytes[at] == b'\n' && at > 0 && bytes[at - 1] == b'\r' {
        at -= 1;
    }
    bytes[..at]
        .iter()
        .rposition(|&b| b == b'\n' || b == b'\r')
        .map_or(0, |i| i + 1)
}

/// `rows` (the rows of the text before an edit) brought up to date after
/// `old_range` of it was replaced by `new_len` bytes, giving the rows
/// `visual_rows_measured(content, ..)` would. Only the lines around the edit
/// are wrapped again; rows before it are kept and rows after it just shift.
/// `None` when the old rows don't line up with the edit (the caller then
/// rebuilds them all).
pub fn patch_visual_rows_measured(
    rows: &mut Vec<(usize, usize)>,
    content: &str,
    old_range: Range<usize>,
    new_len: usize,
    wrap_cols: usize,
    tab_width: usize,
    widths: &CharWidths,
) -> Option<()> {
    let bytes = content.as_bytes();
    let delta = new_len as isize - (old_range.end - old_range.start) as isize;
    let edit_end = old_range.start + new_len;
    if edit_end > bytes.len() || old_range.start > old_range.end {
        return None;
    }
    // Start two lines back: the edit can change how the line before it ends
    // (a CR followed by an inserted LF).
    let first = match old_range.start {
        0 => 0,
        at => match line_start_at(content, at - 1) {
            0 => 0,
            line => line_start_at(content, line - 1),
        },
    };
    let keep = rows.partition_point(|row| row.0 < first);

    let mut fresh = Vec::new();
    let mut line_start = first;
    let resume = loop {
        match memchr::memchr2(b'\n', b'\r', &bytes[line_start..]) {
            Some(rel) => {
                let line_end = line_start + rel;
                push_wrapped(
                    &mut fresh, content, line_start, line_end, wrap_cols, tab_width, widths,
                );
                line_start = if bytes[line_end] == b'\r' && bytes.get(line_end + 1) == Some(&b'\n')
                {
                    line_end + 2
                } else {
                    line_end + 1
                };
                if line_start >= edit_end {
                    break Some(line_start);
                }
            }
            None => {
                push_wrapped(
                    &mut fresh,
                    content,
                    line_start,
                    bytes.len(),
                    wrap_cols,
                    tab_width,
                    widths,
                );
                break None;
            }
        }
    };

    let resume_old = match resume {
        None => rows.len(),
        Some(at) => {
            let old_at = usize::try_from(at as isize - delta).ok()?;
            let index = rows.partition_point(|row| row.0 < old_at);
            // It must be the start of a line, not of a wrapped row inside
            // one: rows of a wrapped line touch, lines are a line ending apart.
            let starts_line = index > 0 && rows[index - 1].1 < old_at;
            if rows.get(index)?.0 != old_at || index < keep || !starts_line {
                return None;
            }
            index
        }
    };
    let tail = keep + fresh.len();
    rows.splice(keep..resume_old, fresh);
    for row in &mut rows[tail..] {
        row.0 = (row.0 as isize + delta) as usize;
        row.1 = (row.1 as isize + delta) as usize;
    }
    if rows.is_empty() {
        rows.push((0, 0));
    }
    Some(())
}

/// Number of rows `visual_rows` would return, without building the list for
/// content that fits on one row (the common case).
#[cfg(test)]
pub fn visual_row_count_tabbed(content: &str, wrap_cols: usize, tab_width: usize) -> usize {
    visual_row_count_measured(content, wrap_cols, tab_width, &CharWidths::uniform())
}

pub fn visual_row_count_measured(
    content: &str,
    wrap_cols: usize,
    tab_width: usize,
    widths: &CharWidths,
) -> usize {
    if wrap_cols == 0 || wrap_cols == usize::MAX {
        return count_line_endings(content.as_bytes()) + 1;
    }
    let mut count = 0usize;
    let bytes = content.as_bytes();
    let mut line_start = 0usize;
    let mut tally = |start: usize, end: usize| {
        count += if fits_one_row(content, start, end, wrap_cols, tab_width, widths) {
            1
        } else {
            count_wrapped(content, start, end, wrap_cols, tab_width, widths)
        };
    };
    while let Some(rel) = memchr::memchr2(b'\n', b'\r', &bytes[line_start..]) {
        let line_end = line_start + rel;
        tally(line_start, line_end);
        line_start = if bytes[line_end] == b'\r' && bytes.get(line_end + 1) == Some(&b'\n') {
            line_end + 2
        } else {
            line_end + 1
        };
    }
    tally(line_start, content.len());
    count
}

/// A line of `n` bytes has at most `n` chars, each at most `widths.max` wide,
/// so it fits unless a tab widens it.
fn fits_one_row(
    content: &str,
    start: usize,
    end: usize,
    wrap_cols: usize,
    tab_width: usize,
    widths: &CharWidths,
) -> bool {
    (end - start).saturating_mul(widths.max) <= wrap_cols
        && (tab_width == 0 || !content[start..end].contains('\t'))
}

/// Width of each char in the units of a wrap budget. `uniform` is one unit
/// per char (a column); a measured table holds the editor font's real widths.
#[derive(Clone, PartialEq, Debug)]
pub struct CharWidths {
    ascii: [u16; 128],
    other: u16,
    /// A typical Khmer letter, which is wider than a Latin one.
    khmer: u16,
    /// Width of the widest char, for the one-row fast path.
    max: usize,
    /// Width of a `0`, the unit a tab stop is counted in.
    unit: usize,
}

impl Default for CharWidths {
    fn default() -> Self {
        Self::uniform()
    }
}

impl CharWidths {
    pub fn uniform() -> Self {
        Self::from_ascii([1; 128], 1, 1)
    }

    /// `ascii[c]` is the width of char `c`; `other` is a typical non-ASCII
    /// letter, and `unit` the width of a `0`. Widths are at least 1.
    pub fn from_ascii(mut ascii: [u16; 128], other: u16, unit: u16) -> Self {
        for w in ascii.iter_mut() {
            *w = (*w).max(1);
        }
        let other = other.max(1);
        let widest = ascii
            .iter()
            .copied()
            .max()
            .unwrap_or(1)
            .max(other.saturating_mul(2));
        Self {
            ascii,
            other,
            khmer: other,
            max: usize::from(widest),
            unit: usize::from(unit.max(1)),
        }
    }

    /// Use `khmer` for the width of Khmer letters.
    pub fn with_khmer(mut self, khmer: u16) -> Self {
        self.khmer = khmer.max(1);
        self.max = self.max.max(usize::from(self.khmer));
        self
    }

    /// Width of `ch` when it follows `prev` in the text. A Khmer subscript
    /// consonant (the one after a coeng, U+17D2) is drawn under the letter
    /// before it, so it takes no room.
    pub fn width_after(&self, prev: Option<char>, ch: char) -> usize {
        if prev == Some('\u{17D2}') && matches!(ch as u32, 0x1780..=0x17A2) {
            return 0;
        }
        self.width(ch)
    }

    pub fn width(&self, ch: char) -> usize {
        if ch.is_ascii() {
            return usize::from(self.ascii[ch as usize]);
        }
        // Marks drawn on or under the letter before them take no room:
        // combining accents, Khmer above/below vowels, signs and the coeng,
        // zero-width characters and variation selectors.
        if matches!(
            ch as u32,
            0x0300..=0x036F
                | 0x17B4..=0x17B5
                | 0x17B7..=0x17BD
                | 0x17C6
                | 0x17C9..=0x17D3
                | 0x17DD
                | 0x200B..=0x200F
                | 0x20D0..=0x20FF
                | 0xFE00..=0xFE0F
        ) {
            return 0;
        }
        let wide = matches!(
            ch as u32,
            0x1100..=0x115F
                | 0x2E80..=0xA4CF
                | 0xAC00..=0xD7A3
                | 0xF900..=0xFAFF
                | 0xFE30..=0xFE6F
                | 0xFF00..=0xFF60
                | 0xFFE0..=0xFFE6
                | 0x1F300..=0x1FAFF
                | 0x20000..=0x3FFFD
        );
        if matches!(ch as u32, 0x1780..=0x17FF) {
            // Dependent vowels that sit beside a letter are narrower than one.
            let beside = matches!(ch as u32, 0x17B6 | 0x17BE..=0x17C5 | 0x17C7 | 0x17C8);
            return usize::from(if beside { self.khmer / 2 } else { self.khmer }.max(1));
        }
        usize::from(self.other) * if wide { 2 } else { 1 }
    }
}

/// True when `ch` belongs to the cluster of `prev` and must stay on its row:
/// a Khmer vowel, sign or coeng, the letter after a coeng (a subscript), or a
/// combining mark.
fn joins_previous(prev: char, ch: char) -> bool {
    matches!(ch, '\u{17B6}'..='\u{17D3}' | '\u{17DD}' | '\u{0300}'..='\u{036F}')
        || prev == '\u{17D2}'
}

fn push_wrapped(
    rows: &mut Vec<(usize, usize)>,
    content: &str,
    start: usize,
    end: usize,
    wrap_cols: usize,
    tab_width: usize,
    widths: &CharWidths,
) {
    for_each_wrapped(content, start, end, wrap_cols, tab_width, widths, |row| {
        rows.push(row)
    });
}

/// Number of rows `push_wrapped` would produce, without building them.
fn count_wrapped(
    content: &str,
    start: usize,
    end: usize,
    wrap_cols: usize,
    tab_width: usize,
    widths: &CharWidths,
) -> usize {
    let mut count = 0usize;
    for_each_wrapped(content, start, end, wrap_cols, tab_width, widths, |_| {
        count += 1
    });
    count
}

/// Streams the wrapped rows of `content[start..end]` to `emit` using byte
/// positions only, so wrapping a long line allocates nothing.
fn for_each_wrapped(
    content: &str,
    start: usize,
    end: usize,
    wrap_cols: usize,
    tab_width: usize,
    widths: &CharWidths,
    mut emit: impl FnMut((usize, usize)),
) {
    if wrap_cols == 0
        || wrap_cols == usize::MAX
        || start >= end
        || fits_one_row(content, start, end, wrap_cols, tab_width, widths)
    {
        emit((start, end));
        return;
    }
    let mut pos = start;
    while pos < end {
        let row_start = pos;
        // The furthest char that still fits in `wrap_cols` columns; a row
        // always takes at least one char.
        let mut col = 0usize;
        let mut max_next = pos;
        let mut prev: Option<char> = None;
        for ch in content[pos..end].chars() {
            let width = if tab_width > 0 && ch == '\t' {
                let stop = tab_width * widths.unit;
                stop - col % stop
            } else {
                widths.width_after(prev, ch)
            };
            if col + width > wrap_cols && max_next > pos {
                break;
            }
            col += width;
            max_next += ch.len_utf8();
            prev = Some(ch);
        }
        if max_next == end {
            emit((row_start, end));
            break;
        }

        // Look for a break opportunity (whitespace) between pos and max_next.
        let break_at = content[pos..max_next]
            .char_indices()
            .rev()
            .find(|(_, ch)| ch.is_whitespace())
            .map(|(off, ch)| pos + off + ch.len_utf8());

        let mut next = break_at.unwrap_or(max_next);
        // Never split a letter from the vowel, sign or subscript that
        // belongs to it: back up to the start of its cluster, unless that
        // would leave the row empty.
        if break_at.is_none() {
            let first_len = content[pos..].chars().next().map_or(1, char::len_utf8);
            let mut at = next;
            while at > pos + first_len && at < end {
                let Some(before) = content[..at].chars().next_back() else {
                    break;
                };
                let Some(here) = content[at..].chars().next() else {
                    break;
                };
                if !joins_previous(before, here) {
                    break;
                }
                at -= before.len_utf8();
            }
            if at > pos {
                next = at;
            }
        }
        emit((row_start, next));
        pos = next;
    }
}

#[cfg(test)]
pub fn hud_should_hide(elapsed_ms: u64) -> bool {
    elapsed_ms >= TRANSIENT_TTL_MS
}

/// Minimum block height for a WYSIWYG list row, in CSS pixels.
/// The extra gap keeps the next bullet from painting over this one.
#[cfg(test)]
pub fn wysiwyg_list_row_height(font_px: f32) -> f32 {
    font_px * 1.65 + 8.0
}

#[cfg(test)]
mod tests {
    #[test]
    fn patched_visual_rows_match_a_full_rebuild() {
        let alphabet: Vec<&str> = vec![
            "a", "b", "c", "d", " ", " ", "\n", "\r", "\r\n", "\t", "é", "ក", "long", "word",
        ];
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move |n: usize| {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((state >> 33) as usize) % n.max(1)
        };
        let widths = CharWidths::uniform();
        let (mut patched_count, mut total) = (0usize, 0usize);
        for round in 0..400 {
            let wrap_cols = [4usize, 9, 30, usize::MAX][round % 4];
            let tab_width = [0usize, 4][round % 2];
            let mut text = String::new();
            for _ in 0..next(60) {
                text.push_str(alphabet[next(alphabet.len())]);
            }
            let mut rows = visual_rows_measured(&text, wrap_cols, tab_width, &widths);
            for _ in 0..40 {
                let bounds: Vec<usize> = (0..=text.len())
                    .filter(|&i| text.is_char_boundary(i))
                    .collect();
                let a = bounds[next(bounds.len())];
                let b = bounds[next(bounds.len())];
                let (start, end) = (a.min(b), a.max(b));
                let mut insert = String::new();
                for _ in 0..next(4) {
                    insert.push_str(alphabet[next(alphabet.len())]);
                }
                text.replace_range(start..end, &insert);
                total += 1;
                let mut candidate = rows.clone();
                if patch_visual_rows_measured(
                    &mut candidate,
                    &text,
                    start..end,
                    insert.len(),
                    wrap_cols,
                    tab_width,
                    &widths,
                )
                .is_some()
                {
                    patched_count += 1;
                    let full = visual_rows_measured(&text, wrap_cols, tab_width, &widths);
                    assert_eq!(
                        candidate, full,
                        "text {text:?} edit {start}..{end} -> {insert:?}"
                    );
                    rows = candidate;
                } else {
                    rows = visual_rows_measured(&text, wrap_cols, tab_width, &widths);
                }
            }
        }
        assert!(
            patched_count * 2 > total,
            "{patched_count}/{total} edits patched"
        );
    }

    /// The original whole-document scan, kept to check the local version.
    fn line_span_at_reference(content: &str, offset: usize) -> TextLine {
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

    #[test]
    fn near_lookups_match_full_counts_from_any_anchor() {
        let samples = ["", "a", "a\r\nb\rc\n\nd", "é😀\r\nカ\n😀z\r", "x\r\n\r\n🔥"];
        for text in samples {
            let ends: Vec<usize> = (0..=text.len() + 1).collect();
            let u16_max = offset_to_utf16(text, text.len());
            for &from in &ends {
                let (_, line_anchor) = line_col_near(text, from, None);
                let (_, u16_anchor) = offset_to_utf16_near(text, from, None);
                for &to in &ends {
                    assert_eq!(
                        line_col_near(text, to, Some(line_anchor)).0,
                        line_col(text, to),
                        "{text:?} line_col {from} -> {to}"
                    );
                    assert_eq!(
                        offset_to_utf16_near(text, to, Some(u16_anchor)).0,
                        offset_to_utf16(text, to),
                        "{text:?} to_utf16 {from} -> {to}"
                    );
                }
                for target in 0..=u16_max + 1 {
                    assert_eq!(
                        offset_from_utf16_near(text, target, Some(u16_anchor)).0,
                        offset_from_utf16(text, target),
                        "{text:?} from_utf16 {from} -> {target}"
                    );
                }
            }
        }
    }

    #[test]
    fn line_col_matches_the_byte_walk_on_every_offset() {
        // The walk `line_col` used before it counted endings with memchr.
        fn reference(content: &str, offset: usize) -> (usize, usize) {
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
            (line, content[span.start..end].chars().count() + 1)
        }
        let alphabet = ['a', '\n', '\r', 'é'];
        for len in 0..=6u32 {
            for code in 0..4usize.pow(len) {
                let mut c = code;
                let text: String = (0..len)
                    .map(|_| {
                        let ch = alphabet[c % 4];
                        c /= 4;
                        ch
                    })
                    .collect();
                for offset in 0..=text.len() + 1 {
                    assert_eq!(
                        line_col(&text, offset),
                        reference(&text, offset),
                        "{text:?} at {offset}"
                    );
                }
            }
        }
    }

    #[test]
    fn line_span_at_matches_the_full_scan_on_every_offset() {
        let alphabet = *b"a\n\r";
        // Every string up to 7 bytes over {a, LF, CR}, every offset.
        for len in 0..=7u32 {
            for code in 0..3usize.pow(len) {
                let mut c = code;
                let text: String = (0..len)
                    .map(|_| {
                        let ch = alphabet[c % 3] as char;
                        c /= 3;
                        ch
                    })
                    .collect();
                for offset in 0..=text.len() + 1 {
                    let got = line_span_at(&text, offset);
                    let want = line_span_at_reference(&text, offset);
                    assert_eq!(
                        (got.start, got.content_end, got.next),
                        (want.start, want.content_end, want.next),
                        "{text:?} at {offset}"
                    );
                }
            }
        }
    }

    fn visual_row_count(content: &str, wrap_cols: usize) -> usize {
        visual_row_count_tabbed(content, wrap_cols, 0)
    }

    use super::*;

    #[test]
    fn visual_row_count_matches_visual_rows() {
        let samples = [
            "",
            "a",
            "a\n",
            "abc def ghi\n\nxy",
            "ααα βββ γγγ δδδ",
            "averyveryverylongword end",
        ];
        for text in samples {
            for cols in [0, 1, 3, 5, 8, 40, usize::MAX] {
                assert_eq!(
                    visual_row_count(text, cols),
                    visual_rows(text, cols).len(),
                    "{text:?} at {cols}"
                );
            }
        }
    }

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

    /// The allocation-per-position search `find_matches` replaced.
    /// Reference: at each position, the shortest window whose lowercase is
    /// at least as long as the needle's; a hit when it equals the needle.
    fn find_matches_by_lowercasing(content: &str, term: &str) -> Vec<Range<usize>> {
        let needle = term.to_lowercase();
        let mut hits = Vec::new();
        if needle.is_empty() {
            return hits;
        }
        let mut start = 0usize;
        while start < content.len() {
            let mut end = start;
            for (at, c) in content[start..].char_indices() {
                end = start + at + c.len_utf8();
                if content[start..end].to_lowercase().len() >= needle.len() {
                    break;
                }
            }
            if content[start..end].to_lowercase() == needle {
                hits.push(start..end);
                start = end;
            } else {
                start += content[start..].chars().next().map_or(1, char::len_utf8);
            }
        }
        hits
    }

    #[test]
    fn caseless_find_matches_lowercasing_each_window() {
        let contents = [
            "one cat, two Cat, three CAT",
            "\u{212A}elvin kelvin KELVIN",
            "İstanbul i\u{307}stanbul ISTANBUL",
            "ΟΔΟΣ οδος ΟΔΟΣΑ Σ σ ς",
            "Straße STRASSE strasse ẞ ß",
            "emoji 😀 cat😀CAT\r\nline two\r\n",
            "aaaa",
            "",
        ];
        let terms = [
            "cat", "kelvin", "\u{212A}", "k", "istanbul", "i̇", "İ", "οδος", "ΟΔΟΣ", "σ", "ς", "Σ",
            "ß", "ẞ", "ss", "😀c", "\r\nl", "aa", "a",
        ];
        for content in contents {
            for term in terms {
                assert_eq!(
                    find_matches(content, term, false),
                    find_matches_by_lowercasing(content, term),
                    "content {content:?} term {term:?}"
                );
            }
        }
        // Audit B6: `İ` lowercases to two characters and still matches.
        assert_eq!(find_matches("İstanbul", "İ", false), vec![0..2]);
        assert_eq!(find_matches("İstanbul", "İ", true), vec![0..2]);
        assert_eq!(find_matches("xİx İ", "i\u{307}", false), vec![1..3, 5..7]);
        // A needle never matches half of a character.
        assert_eq!(find_matches("İ", "i", false), Vec::<Range<usize>>::new());
        // Final sigma still depends on the letters before it.
        assert_eq!(find_matches("ΟΔΟΣ", "οδος", false), vec![0..8]);
    }

    #[test]
    fn undo_stores_diffs_and_caps_bytes() {
        let mut history = UndoHistory::default();
        let mut content = "seed".to_string();
        for index in 0..200 {
            let insert = format!("w{index}");
            let start = content.len();
            history.push(DiffEdit::new(start, String::new(), insert.clone()));
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
        paused.push_at(DiffEdit::new(0, String::new(), "a".into()), start);
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

    // --- Tab display ---

    #[test]
    fn tabbed_rows_wrap_by_displayed_columns() {
        // A tab is 4 columns and a break opportunity, so it ends the first row.
        let rows = visual_rows_tabbed("\tab\tcd", 6, 4);
        assert_eq!(rows, vec![(0, 1), (1, 6)]);
        // Without tab expansion the same line fits in one row.
        assert_eq!(visual_rows("\tab\tcd", 6), vec![(0, 6)]);
    }

    #[test]
    fn tabbed_row_count_matches_tabbed_rows() {
        let samples = ["", "\t", "\ta\tb\n\t\t\t\t", "ab\tcd ef\tgh ij", "x\n\ty z"];
        for text in samples {
            for cols in [0, 1, 3, 5, 8, 40, usize::MAX] {
                for tab in [0, 2, 4] {
                    assert_eq!(
                        visual_row_count_tabbed(text, cols, tab),
                        visual_rows_tabbed(text, cols, tab).len(),
                        "{text:?} cols {cols} tab {tab}"
                    );
                }
            }
        }
    }

    #[test]
    // Wall-clock limit: flaky on slow CI runners. CI runs it on its own (allowed to fail).
    #[ignore = "timing"]
    fn tabbed_rows_stay_within_budget_on_large_files() {
        use std::time::Instant;
        // 5 MB of short, tab-indented lines and a 1 MB single line.
        let lines = "\tlet value = compute(arg);\n".repeat(5 * 1024 * 1024 / 27);
        let one_line = "var a=1;\t".repeat(100_000);
        for wrap in [usize::MAX, 80] {
            let started = Instant::now();
            let rows = visual_rows_tabbed(&lines, wrap, 4);
            let count = visual_row_count_tabbed(&lines, wrap, 4);
            assert_eq!(rows.len(), count);
            let started_line = Instant::now();
            let rows = visual_rows_tabbed(&one_line, wrap, 4);
            assert_eq!(rows.len(), visual_row_count_tabbed(&one_line, wrap, 4));
            // Generous: even an unoptimised test build stays far under a visible freeze.
            assert!(
                started.elapsed().as_millis() < 3000,
                "5 MB took {:?}",
                started.elapsed()
            );
            assert!(
                started_line.elapsed().as_millis() < 2000,
                "1 MB line took {:?}",
                started_line.elapsed()
            );
        }
    }

    fn narrow_i_wide_w() -> CharWidths {
        let mut ascii = [10u16; 128];
        ascii[b'i' as usize] = 4;
        ascii[b'W' as usize] = 16;
        CharWidths::from_ascii(ascii, 10, 10)
    }

    #[test]
    fn a_row_never_starts_with_a_vowel_sign_or_a_subscript() {
        // Letter, vowel, letter, vowel ...: a break after the second letter
        // would leave a vowel alone at the start of the next row.
        let word = "កាកាកា";
        let rows = visual_rows(word, 3);
        for (start, _) in &rows {
            let first = word[*start..].chars().next().unwrap();
            assert!(!matches!(first, '\u{17B6}'..='\u{17D3}'), "{rows:?}");
        }
        // Letter, coeng, letter is one cluster too.
        let sub = "ក្កក្កក្ក";
        let rows = visual_rows(sub, 4);
        for (start, _) in &rows {
            let prev = sub[..*start].chars().next_back();
            assert_ne!(prev, Some('\u{17D2}'), "{rows:?}");
        }
        // A cluster wider than the row still takes at least one row, never none.
        assert!(!visual_rows("កាាាា", 2).is_empty());
    }

    #[test]
    fn narrow_letters_pack_more_to_a_row_than_wide_ones() {
        let widths = narrow_i_wide_w();
        // Budget 40: ten `i` (4 each) fit, but only two `W` (16 each).
        let narrow = visual_rows_measured(&"i".repeat(20), 40, 0, &widths);
        assert_eq!(narrow, vec![(0, 10), (10, 20)]);
        let wide = visual_rows_measured(&"W".repeat(6), 40, 0, &widths);
        assert_eq!(wide, vec![(0, 2), (2, 4), (4, 6)]);
    }

    #[test]
    fn measured_row_count_matches_measured_rows() {
        let widths = narrow_i_wide_w();
        for text in [
            "",
            "iiii",
            "WWWW WWWW iii",
            "a\nWWWWWWW\r\ni i i i i i i i i i i i",
        ] {
            for budget in [1, 16, 40, 100] {
                assert_eq!(
                    visual_row_count_measured(text, budget, 0, &widths),
                    visual_rows_measured(text, budget, 0, &widths).len(),
                    "{text:?} at {budget}"
                );
            }
        }
    }

    #[test]
    fn marks_that_stack_on_a_letter_take_no_room() {
        let widths = narrow_i_wide_w();
        assert_eq!(widths.width('\u{301}'), 0);
        assert_eq!(widths.width('\u{17D2}'), 0);
        assert_eq!(widths.width('\u{17B7}'), 0);
        assert_eq!(widths.width_after(Some('\u{17D2}'), '\u{1780}'), 0);
        assert_eq!(widths.width_after(None, '\u{1780}'), 10);
        // Letter, coeng, subscript letter, above vowel: one letter wide.
        let cluster = "\u{1780}\u{17D2}\u{1780}\u{17B7}";
        let text = cluster.repeat(5);
        // Budget 30 holds three clusters, and a row never ends inside one.
        let rows = visual_rows_measured(&text, 30, 0, &widths);
        assert_eq!(rows.len(), 2);
        let first = &text[rows[0].0..rows[0].1];
        assert_eq!(first.chars().count(), 12);
    }

    #[test]
    fn uniform_widths_wrap_like_columns() {
        let widths = CharWidths::uniform();
        assert_eq!(
            visual_rows_measured("hello world test", 8, 0, &widths),
            visual_rows("hello world test", 8)
        );
    }

    #[test]
    fn wide_non_ascii_chars_count_double() {
        let widths = narrow_i_wide_w();
        assert_eq!(widths.width('字'), 20);
        assert_eq!(widths.width('é'), 10);
    }

    #[test]
    fn visual_rows_strip_crlf_and_split_lone_cr() {
        // CRLF line whose visible text equals wrap_cols must not wrap a phantom "\r" row.
        assert_eq!(visual_rows("hello\r\nworld", 5), vec![(0, 5), (7, 12)]);
        assert_eq!(visual_row_count("hello\r\nworld", 5), 2);
        assert_eq!(visual_rows("ab\r\n", usize::MAX), vec![(0, 2), (4, 4)]);
        assert_eq!(visual_row_count("ab\r\n", usize::MAX), 2);

        // Lone CR files split into lines consistently with source_lines/line_col.
        assert_eq!(visual_rows("hello\rworld", 80), vec![(0, 5), (6, 11)]);
        assert_eq!(visual_row_count("hello\rworld", 80), 2);
        assert_eq!(
            visual_rows("a\rb\r", usize::MAX),
            vec![(0, 1), (2, 3), (4, 4)]
        );
        assert_eq!(visual_row_count("a\rb\r", usize::MAX), 3);

        // Mixed endings match row count across widths.
        let mixed = "a\r\nb\rc\nd\r\n\r\n";
        for cols in [0, 1, 2, 5, usize::MAX] {
            assert_eq!(
                visual_row_count_tabbed(mixed, cols, 4),
                visual_rows_tabbed(mixed, cols, 4).len(),
                "cols={cols}"
            );
        }
    }

    #[test]
    fn range_utf16_matches_single_endpoint_conversions() {
        let sample = "a😀éカ\r\n🔥z";
        for s in 0..=sample.len() + 2 {
            for e in 0..=sample.len() + 2 {
                assert_eq!(
                    range_to_utf16(sample, &(s..e)),
                    offset_to_utf16(sample, s)..offset_to_utf16(sample, e),
                    "range_to_utf16({s}..{e})"
                );
            }
        }
        let max_u16 = offset_to_utf16(sample, sample.len());
        for s in 0..=max_u16 + 2 {
            for e in 0..=max_u16 + 2 {
                assert_eq!(
                    range_from_utf16(sample, &(s..e)),
                    offset_from_utf16(sample, s)..offset_from_utf16(sample, e),
                    "range_from_utf16({s}..{e})"
                );
            }
        }
    }
}

#[cfg(test)]
mod emptied_group_tests {
    use super::*;

    #[test]
    fn an_emptied_span_group_gives_back_its_undo_bytes() {
        let mut h = UndoHistory::default();
        h.push_span_opening(DiffEdit::new(0, String::new(), "ab".into()));
        h.push(DiffEdit::new(0, "ab".into(), String::new()));
        assert_eq!(h.stored_bytes(), 0);
    }
}

#[cfg(test)]
mod update_matches_tests {
    use super::*;

    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self, n: usize) -> usize {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((self.0 >> 33) as usize) % n
        }
    }

    fn random_text(rng: &mut Lcg, len: usize, alphabet: &[&str]) -> String {
        (0..len)
            .map(|_| alphabet[rng.next(alphabet.len())])
            .collect()
    }

    #[test]
    fn incremental_matches_equal_a_fresh_search() {
        let alphabets: [&[&str]; 3] = [
            &["a", "b", "İ", "\r", "\n"],
            &["a", "A", "K", "k", "i", "I", "İ"],
            &["a", "a", "a", "b"],
        ];
        let terms = ["aa", "aba", "i", "a", "k", "ab", "İ", "aaa"];
        let mut rng = Lcg(7);
        for round in 0..4000 {
            let alphabet = alphabets[round % alphabets.len()];
            let len = rng.next(40);
            let before = random_text(&mut rng, len, alphabet);
            let term = terms[rng.next(terms.len())];
            let match_case = rng.next(2) == 0;
            // A random edit on char boundaries.
            let bounds: Vec<usize> = before
                .char_indices()
                .map(|(i, _)| i)
                .chain([before.len()])
                .collect();
            let a = bounds[rng.next(bounds.len())];
            let b = bounds[rng.next(bounds.len())];
            let (a, b) = (a.min(b), a.max(b));
            let len = rng.next(6);
            let inserted = random_text(&mut rng, len, alphabet);
            let mut after = before.clone();
            after.replace_range(a..b, &inserted);
            let old = find_matches(&before, term, match_case);
            let got = update_matches(&old, &after, term, match_case, a..b, inserted.len());
            assert_eq!(
                got,
                find_matches(&after, term, match_case),
                "{before:?} -> {after:?} edit {a}..{b} {inserted:?} term {term:?} case {match_case}"
            );
        }
    }
}
