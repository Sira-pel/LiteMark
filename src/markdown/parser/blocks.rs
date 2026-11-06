//! Block syntax: code fences, thematic breaks, ATX headings and indentation columns.

use super::*;

pub(super) struct OpenFence {
    /// Column-0..3 fence. A shorter run does not close it.
    pub(super) strict: bool,
    /// `` ` `` or `~`. Only the same character closes the fence.
    pub(super) marker: u8,
    pub(super) len: usize,
    pub(super) indent: usize,
    pub(super) open_range: std::ops::Range<usize>,
}

/// Thematic break: the line trims to `---`, `***`, or `___`.
///
/// At document level (`parent_col == 0`) only columns 0–3 count. A tab is
/// column 4, so `\t---` stays a paragraph. This parser has no indented-code
/// kind, so `    ---\rnext\r` stays text after the bare-CR split. Inside a
/// list the same 0–3 columns are counted past the item's content column, and
/// a tab stops at the next multiple of 4, so `- item` followed by `\t---` is
/// a rule.
pub(super) fn exact_thematic_break(line: &str, parent_col: usize) -> bool {
    let trimmed = line.trim();
    if !(trimmed == "---" || trimmed == "***" || trimmed == "___") {
        return false;
    }
    let column = leading_column(line);
    if column <= 3 {
        return true;
    }
    parent_col > 0 && column >= parent_col && column - parent_col <= 3
}

/// `(bytes, column)` of the first non-space, non-tab. Tabs advance to the next multiple of 4.
pub(super) fn split_indent(line: &str) -> (usize, usize) {
    let bytes = line.as_bytes();
    let mut column = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b' ' => {
                column += 1;
                index += 1;
            }
            b'\t' => {
                column += 4 - (column % 4);
                index += 1;
            }
            _ => break,
        }
    }
    (index, column)
}

pub(super) fn prefix_through_column(line: &str, min_col: usize) -> usize {
    let bytes = line.as_bytes();
    let mut column = 0usize;
    let mut index = 0usize;
    while index < bytes.len() && column < min_col {
        match bytes[index] {
            b' ' => {
                column += 1;
                index += 1;
            }
            b'\t' => {
                column += 4 - (column % 4);
                index += 1;
            }
            _ => break,
        }
    }
    index
}

/// Leading spaces only. A tab in the indent is `None` so the caller keeps red's trim rule.
pub(super) fn leading_spaces(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() && bytes[index] == b' ' {
        index += 1;
    }
    if index < bytes.len() && bytes[index] == b'\t' {
        return None;
    }
    Some(index)
}

/// Top-level fence at column 0–3: a run of three or more backticks or tildes.
/// A backtick fence's info string may not contain a backtick; a tilde
/// fence's may contain anything (CommonMark examples 119–147).
/// Returns (marker, run length, indent, info).
pub(super) fn strict_fence_open(line: &str) -> Option<(u8, usize, usize, Option<String>)> {
    let indent = leading_spaces(line)?;
    if indent > 3 {
        return None;
    }
    let bytes = line.as_bytes();
    let marker = *bytes.get(indent)?;
    if marker != b'`' && marker != b'~' {
        return None;
    }
    let mut end = indent;
    while end < bytes.len() && bytes[end] == marker {
        end += 1;
    }
    let len = end - indent;
    if len < 3 {
        return None;
    }
    let rest = &line[end..];
    if marker == b'`' && rest.contains('`') {
        return None;
    }
    let info = rest.trim_matches(|ch: char| ch == ' ' || ch == '\t');
    let info = if info.is_empty() {
        None
    } else {
        Some(info.to_string())
    };
    Some((marker, len, indent, info))
}

/// ` ``` aa ``` ` at column 0–3 is not a fence. Callers must not fall through to red's trim rule.
pub(super) fn backtick_info_rejects_fence(line: &str) -> bool {
    let Some(indent) = leading_spaces(line) else {
        return false;
    };
    if indent > 3 {
        return false;
    }
    let bytes = line.as_bytes();
    if indent >= bytes.len() || bytes[indent] != b'`' {
        return false;
    }
    let mut end = indent;
    while end < bytes.len() && bytes[end] == b'`' {
        end += 1;
    }
    end - indent >= 3 && line[end..].contains('`')
}

pub(super) fn is_closing_fence(line: &str, marker: u8, min_len: usize, max_indent: usize) -> bool {
    let (index, column) = split_indent(line);
    if index >= line.len() || column > max_indent {
        return false;
    }
    let bytes = line.as_bytes();
    if bytes[index] != marker {
        return false;
    }
    let mut end = index;
    while end < bytes.len() && bytes[end] == marker {
        end += 1;
    }
    if end - index < min_len {
        return false;
    }
    bytes[end..]
        .iter()
        .all(|byte| *byte == b' ' || *byte == b'\t')
}

/// A fence after a list item or blockquote keeps red's trimmed-``` rule.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum InListOrQuoteTracker {
    #[default]
    None,
    AfterListOrQuote,
    AfterListOrQuoteOther,
    AfterListOrQuoteBlank,
    AfterListOrQuoteBlankOther,
}

impl InListOrQuoteTracker {
    #[inline]
    pub(crate) fn is_in_list_or_quote(self) -> bool {
        self != Self::None
    }

    #[inline]
    pub(crate) fn push(&mut self, kind: &LineKind) {
        *self = match (&*self, kind) {
            (_, LineKind::ListItem { .. } | LineKind::Blockquote) => Self::AfterListOrQuote,
            (Self::AfterListOrQuote, LineKind::Empty) => Self::AfterListOrQuoteBlank,
            (Self::AfterListOrQuoteOther, LineKind::Empty) => Self::None,
            (Self::AfterListOrQuoteBlank, LineKind::Empty) => Self::AfterListOrQuoteBlank,
            (Self::AfterListOrQuoteBlankOther, LineKind::Empty) => Self::None,
            (Self::None, LineKind::Empty) => Self::None,
            (Self::AfterListOrQuote, _) => Self::AfterListOrQuoteOther,
            (Self::AfterListOrQuoteOther, _) => Self::AfterListOrQuoteOther,
            (Self::AfterListOrQuoteBlank, _) => Self::AfterListOrQuoteBlankOther,
            (Self::AfterListOrQuoteBlankOther, _) => Self::AfterListOrQuoteBlankOther,
            (Self::None, _) => Self::None,
        };
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn in_list_or_quote(lines: &[ParsedLine]) -> bool {
    let mut saw_blank = false;
    for line in lines.iter().rev() {
        match &line.kind {
            LineKind::Empty => saw_blank = true,
            LineKind::ListItem { .. } | LineKind::Blockquote => return true,
            LineKind::Paragraph
            | LineKind::Heading { .. }
            | LineKind::CodeFence { .. }
            | LineKind::CodeLine
            | LineKind::HorizontalRule
            | LineKind::TableDelimiter
            | LineKind::TableRow { .. } => {
                if saw_blank {
                    return false;
                }
            }
        }
    }
    false
}

pub(super) fn atx_heading(line: &str) -> Option<(u8, std::ops::Range<usize>)> {
    if let Some(found) = atx_heading_within(line, 3) {
        return Some(found);
    }
    // A tab is one byte. Red still treats up to three bytes of indent as a heading.
    let trimmed = line.trim_start();
    let indent_len = line.len() - trimmed.len();
    if indent_len > 3 {
        return None;
    }
    for (level, prefix) in [
        (6u8, "###### "),
        (5, "##### "),
        (4, "#### "),
        (3, "### "),
        (2, "## "),
        (1, "# "),
    ] {
        if trimmed.starts_with(prefix) {
            let start = indent_len + prefix.len();
            return Some((level, start..line.len()));
        }
    }
    None
}

/// `max_indent` is a column, with tabs advancing to the next multiple of 4.
pub(super) fn atx_heading_within(line: &str, max_indent: usize) -> Option<(u8, std::ops::Range<usize>)> {
    let bytes = line.as_bytes();
    let (mut index, column) = split_indent(line);
    if column > max_indent || index >= bytes.len() {
        return None;
    }
    let mut level = 0u8;
    while index < bytes.len() && bytes[index] == b'#' {
        level += 1;
        if level > 6 {
            return None;
        }
        index += 1;
    }
    // A tab is whitespace after the opening hashes (CommonMark example 10).
    let separator = index < bytes.len() && (bytes[index] == b' ' || bytes[index] == b'\t');
    if level == 0 || (index < bytes.len() && !separator) {
        return None;
    }
    let mut content_start = index;
    if index < bytes.len() {
        content_start = index + 1;
    }
    let mut end = bytes.len();
    while end > content_start && bytes[end - 1] == b' ' {
        end -= 1;
    }
    let hash_end = end;
    while end > content_start && bytes[end - 1] == b'#' {
        end -= 1;
    }
    // A closing run may consume the whole content (`### ###`). The space that
    // precedes it can be the opening separator, so `end` is allowed to land on
    // `content_start` but not before it. A backslash does not escape a space,
    // so `# foo\ #` still strips the closer. A backslash immediately before
    // the hash (`### foo \###`) is not a separating space, and the hash stays.
    let preceded_by_space = end > 0 && bytes[end - 1] == b' ';
    if end < hash_end && preceded_by_space {
        if end > content_start {
            end -= 1;
            while end > content_start && (bytes[end - 1] == b' ' || bytes[end - 1] == b'\t') {
                end -= 1;
            }
        }
    } else {
        end = bytes.len();
        while end > content_start && bytes[end - 1] == b' ' {
            end -= 1;
        }
    }
    while content_start < end && (bytes[content_start] == b' ' || bytes[content_start] == b'\t') {
        content_start += 1;
    }
    Some((level, content_start..end))
}

/// ATX content, with the opening marks and a closing-hash run removed.
/// `# **foo** #` keeps the inner `**`.
pub(crate) fn atx_content_range(line: &str) -> Option<std::ops::Range<usize>> {
    atx_heading(line).map(|(_level, content)| content)
}

/// ATX content range for the WYSIWYG display: like `atx_content_range`, but
/// when the heading has no closing-hash run its trailing spaces stay in the
/// content, so typing at the end of a heading keeps them visible. A tail that
/// contains a non-space byte is a closing-hash run and stays hidden.
pub(crate) fn atx_display_content_range(line: &str) -> Option<(u8, std::ops::Range<usize>)> {
    atx_heading(line).map(|(level, content)| {
        let tail_is_space = line[content.end..].bytes().all(|b| b == b' ' || b == b'\t');
        if tail_is_space {
            (level, content.start..line.len())
        } else {
            (level, content)
        }
    })
}

#[allow(clippy::single_range_in_vec_init)]
pub(super) fn fence_marker_line(
    index: usize,
    source: std::ops::Range<usize>,
    line: &str,
    kind: LineKind,
) -> ParsedLine {
    let line_start = source.start;
    let line_end = source.end;
    let hidden = if line_end > line_start {
        vec![line_start..line_end]
    } else {
        Vec::new()
    };
    let (display_text, offset_map) = RowOffsetMap::from_hidden(line, line_start, line_end, &hidden);
    ParsedLine {
        line_index: index,
        source,
        kind,
        prefix_range: line_start..line_end,
        hidden_ranges: hidden,
        inline_pairs: Vec::new(),
        display_text,
        offset_map: Arc::new(offset_map),
        link_hits: no_links(),
        styled_segments: Vec::new(),
    }
}

#[allow(clippy::single_range_in_vec_init)]
pub(super) fn code_content_line(
    index: usize,
    source: std::ops::Range<usize>,
    line: &str,
    strip: usize,
) -> ParsedLine {
    let line_start = source.start;
    let line_end = source.end;
    if strip == 0 {
        return ParsedLine {
            line_index: index,
            source: source.clone(),
            kind: LineKind::CodeLine,
            prefix_range: line_start..line_start,
            hidden_ranges: Vec::new(),
            inline_pairs: Vec::new(),
            display_text: line.to_string(),
            offset_map: Arc::new(RowOffsetMap::identity(line_start, line_end)),
            link_hits: no_links(),
            styled_segments: vec![StyledSegment {
                source,
                bold: false,
                italic: false,
                underline: false,
                strike: false,
                code: true,
                link_url: None,
            }],
        };
    }
    let content_start = line_start + strip;
    let hidden = vec![line_start..content_start];
    let (display_text, offset_map) =
        RowOffsetMap::from_hidden(line, line_start, content_start, &hidden);
    let mut styled_segments = Vec::new();
    if content_start < line_end {
        styled_segments.push(StyledSegment {
            source: content_start..line_end,
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            code: true,
            link_url: None,
        });
    }
    ParsedLine {
        line_index: index,
        source,
        kind: LineKind::CodeLine,
        prefix_range: line_start..content_start,
        hidden_ranges: hidden,
        inline_pairs: Vec::new(),
        display_text,
        offset_map: Arc::new(offset_map),
        link_hits: no_links(),
        styled_segments,
    }
}

pub(super) fn atx_line(
    index: usize,
    source: std::ops::Range<usize>,
    line: &str,
    level: u8,
    content: std::ops::Range<usize>,
) -> ParsedLine {
    let line_start = source.start;
    let line_end = source.end;
    let mut hidden = Vec::new();
    if content.start > 0 {
        hidden.push(line_start..line_start + content.start);
    }
    if content.end < line.len() {
        hidden.push(line_start + content.end..line_end);
    }
    let (inline_hidden, inline_pairs, styled_segments) = if content.is_empty() {
        (Vec::new(), Vec::new(), Vec::new())
    } else {
        parse_inline_syntax(
            &line[content.start..content.end],
            line_start + content.start,
        )
    };
    hidden.extend(inline_hidden);
    let hidden = merge_ranges(hidden);
    let (display_text, offset_map) =
        RowOffsetMap::from_hidden(line, line_start, line_start + content.start, &hidden);
    ParsedLine {
        line_index: index,
        source,
        kind: LineKind::Heading { level },
        prefix_range: line_start..line_start + content.start,
        hidden_ranges: hidden,
        inline_pairs,
        display_text,
        offset_map: Arc::new(offset_map),
        link_hits: no_links(),
        styled_segments,
    }
}

/// Tab, VT, FF, or space. Pulldown `is_ascii_whitespace_no_nl`.
pub(super) fn is_ascii_whitespace_no_nl(byte: u8) -> bool {
    byte == b'\t' || byte == 0x0b || byte == 0x0c || byte == b' '
}

/// Column scanner for list padding. Tabs expand to the next stop of 4,
/// counted from the byte after the previous tab (pulldown `scan_space_inner`).
#[derive(Clone, Copy)]
pub(super) struct ColScan<'a> {
    pub(super) bytes: &'a [u8],
    pub(super) ix: usize,
    pub(super) tab_start: usize,
    pub(super) spaces_remaining: usize,
}

impl<'a> ColScan<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            ix: 0,
            tab_start: 0,
            spaces_remaining: 0,
        }
    }

    pub(super) fn scan_space(&mut self, n_space: usize) -> bool {
        self.scan_space_inner(n_space) == 0
    }

    pub(super) fn scan_space_upto(&mut self, n_space: usize) -> usize {
        n_space - self.scan_space_inner(n_space)
    }

    pub(super) fn scan_space_inner(&mut self, mut n_space: usize) -> usize {
        let n_from_remaining = self.spaces_remaining.min(n_space);
        self.spaces_remaining -= n_from_remaining;
        n_space -= n_from_remaining;
        while n_space > 0 && self.ix < self.bytes.len() {
            match self.bytes[self.ix] {
                b' ' => {
                    self.ix += 1;
                    n_space -= 1;
                }
                b'\t' => {
                    let spaces = 4 - (self.ix - self.tab_start) % 4;
                    self.ix += 1;
                    self.tab_start = self.ix;
                    let n = spaces.min(n_space);
                    n_space -= n;
                    self.spaces_remaining = spaces - n;
                }
                _ => break,
            }
        }
        n_space
    }

    /// End of an editor line. A CR ends the marker only when it is the line
    /// ending (`2.\r` in a CRLF line). `1.\rfoo` keeps the CR in the text.
    pub(super) fn is_at_eol(&self) -> bool {
        match self.bytes.get(self.ix) {
            None => true,
            Some(&b'\n') => true,
            Some(&b'\r') => self.ix + 1 >= self.bytes.len() || self.bytes[self.ix + 1] == b'\n',
            Some(_) => false,
        }
    }

    pub(super) fn skip_indent(&mut self, indent: usize) {
        while self.ix < indent {
            if self.bytes.get(self.ix) == Some(&b'\t') {
                self.ix += 1;
                self.tab_start = self.ix;
                self.spaces_remaining = 0;
            } else {
                self.ix += 1;
            }
        }
    }
}

pub(super) fn rest_is_blank(bytes: &[u8]) -> bool {
    let mut index = 0;
    while index < bytes.len() && is_ascii_whitespace_no_nl(bytes[index]) {
        index += 1;
    }
    index == bytes.len() || bytes[index] == b'\n' || bytes[index] == b'\r'
}
