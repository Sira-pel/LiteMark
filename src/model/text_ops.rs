//! Pure text-buffer operations: caret movement, edits, formatting, and search.
//!
//! Undo stores each change as a diff (deleted bytes + inserted bytes) and drops
//! the oldest diffs once the stored text exceeds [`UNDO_BYTE_BUDGET`].

use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

/// Maximum total bytes of deleted + inserted text kept on the undo and redo stacks.
pub const UNDO_BYTE_BUDGET: usize = 256 * 1024;

/// Hide zoom and save toasts after this long.
pub const TRANSIENT_TTL_MS: u64 = 1_500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffEdit {
    pub start: usize,
    pub deleted: String,
    pub inserted: String,
}

impl DiffEdit {
    pub fn cost(&self) -> usize {
        self.deleted.len() + self.inserted.len()
    }
}

#[derive(Debug, Clone, Default)]
pub struct UndoHistory {
    undo: Vec<DiffEdit>,
    redo: Vec<DiffEdit>,
    undo_bytes: usize,
    redo_bytes: usize,
}

impl UndoHistory {
    #[allow(dead_code)]
    pub fn stored_bytes(&self) -> usize {
        self.undo_bytes + self.redo_bytes
    }

    pub fn push(&mut self, edit: DiffEdit) {
        if edit.deleted.is_empty() && edit.inserted.is_empty() {
            return;
        }
        self.undo_bytes += edit.cost();
        self.undo.push(edit);
        while self.undo_bytes > UNDO_BYTE_BUDGET && self.undo.len() > 1 {
            let dropped = self.undo.remove(0);
            self.undo_bytes = self.undo_bytes.saturating_sub(dropped.cost());
        }
        self.redo.clear();
        self.redo_bytes = 0;
    }

    /// Undo the latest edit. Returns the inverse that was applied to `content`.
    pub fn undo(&mut self, content: &mut String) -> bool {
        let Some(edit) = self.undo.pop() else {
            return false;
        };
        self.undo_bytes = self.undo_bytes.saturating_sub(edit.cost());
        let end = edit.start + edit.inserted.len();
        if end > content.len() || content.get(edit.start..end).is_none() {
            return false;
        }
        content.replace_range(edit.start..end, &edit.deleted);
        let inverse = DiffEdit {
            start: edit.start,
            deleted: edit.inserted,
            inserted: edit.deleted,
        };
        self.redo_bytes += inverse.cost();
        self.redo.push(inverse);
        true
    }

    pub fn redo(&mut self, content: &mut String) -> bool {
        let Some(edit) = self.redo.pop() else {
            return false;
        };
        self.redo_bytes = self.redo_bytes.saturating_sub(edit.cost());
        let end = edit.start + edit.inserted.len();
        if end > content.len() || content.get(edit.start..end).is_none() {
            return false;
        }
        content.replace_range(edit.start..end, &edit.deleted);
        let inverse = DiffEdit {
            start: edit.start,
            deleted: edit.inserted,
            inserted: edit.deleted,
        };
        self.undo_bytes += inverse.cost();
        self.undo.push(inverse);
        true
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

    pub fn select_to(&mut self, offset: usize) {
        let head = self.cursor();
        if offset < head {
            self.range = offset..head;
            self.reversed = true;
        } else {
            self.range = head..offset;
            self.reversed = false;
        }
        self.preferred_col = None;
    }

    pub fn clamp(&mut self, len: usize) {
        self.range.start = self.range.start.min(len);
        self.range.end = self.range.end.min(len);
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

/// 1-based line and column at a byte offset.
pub fn line_col(content: &str, offset: usize) -> (usize, usize) {
    let offset = floor_char_boundary(content, offset.min(content.len()));
    let before = &content[..offset];
    let line = before.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let line_start = before.rfind('\n').map(|index| index + 1).unwrap_or(0);
    let col = content[line_start..offset].chars().count() + 1;
    (line, col)
}

pub fn line_start(content: &str, line_1: usize) -> usize {
    if line_1 <= 1 {
        return 0;
    }
    let mut line = 1usize;
    for (index, byte) in content.bytes().enumerate() {
        if byte == b'\n' {
            line += 1;
            if line == line_1 {
                return index + 1;
            }
        }
    }
    content.len()
}

pub fn line_end(content: &str, offset: usize) -> usize {
    let offset = offset.min(content.len());
    content[offset..]
        .find('\n')
        .map(|index| offset + index)
        .unwrap_or(content.len())
}

pub fn home_offset(content: &str, offset: usize) -> usize {
    let offset = offset.min(content.len());
    content[..offset]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0)
}

pub fn offset_at_line_col(content: &str, line_1: usize, col_1: usize) -> usize {
    let start = line_start(content, line_1);
    if start >= content.len() && line_1 > 1 && content.lines().count() < line_1 {
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
    let offset = offset_at_line_col(content, target_line, prefer);
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

/// Byte length of a markdown list marker, including its trailing space.
pub fn list_marker_len(line: &str) -> usize {
    let indent = line.len() - line.trim_start().len();
    let rest = &line[indent..];
    let marker = if rest.starts_with("- [ ] ")
        || rest.starts_with("- [x] ")
        || rest.starts_with("- [X] ")
        || rest.starts_with("* [ ] ")
        || rest.starts_with("* [x] ")
        || rest.starts_with("* [X] ")
    {
        6
    } else if rest.starts_with("- ") || rest.starts_with("* ") || rest.starts_with("+ ") {
        2
    } else {
        ordered_marker_len(rest).unwrap_or(0)
    };
    indent + marker
}

fn ordered_marker_len(rest: &str) -> Option<usize> {
    let digits = rest
        .bytes()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if digits == 0 {
        return None;
    }
    let after = &rest[digits..];
    if let Some(stripped) = after.strip_prefix(". ") {
        let _ = stripped;
        Some(digits + 2)
    } else {
        None
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

pub fn find_prev(hits: &[Range<usize>], cursor: usize, current: Option<usize>) -> Option<usize> {
    if hits.is_empty() {
        return None;
    }
    let scan_end = match current {
        Some(index) if hits.get(index).is_some_and(|hit| hit.start == cursor) => index,
        _ => hits.len(),
    };
    hits[..scan_end]
        .iter()
        .rposition(|hit| hit.start < cursor)
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
    let chars: Vec<usize> = content[start..end]
        .char_indices()
        .map(|(index, _)| start + index)
        .collect();
    if chars.len() <= wrap_cols {
        rows.push((start, end));
        return;
    }
    let mut index = 0usize;
    while index < chars.len() {
        let row_start = chars[index];
        let next = (index + wrap_cols).min(chars.len());
        let row_end = if next < chars.len() { chars[next] } else { end };
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

        content.insert_str(caret.cursor(), "\n");
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
        assert_eq!(find_prev(&hits, hits[1].start, Some(1)), Some(0));
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
            history.push(DiffEdit {
                start,
                deleted: String::new(),
                inserted: insert.clone(),
            });
            content.push_str(&insert);
        }
        let full_copies = content.len() * 200;
        assert!(history.stored_bytes() < full_copies / 4);
        assert!(history.stored_bytes() <= UNDO_BYTE_BUDGET + 64);
        let before = content.clone();
        assert!(history.undo(&mut content));
        assert_ne!(content, before);
        assert!(history.redo(&mut content));
        assert_eq!(content, before);
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
}
