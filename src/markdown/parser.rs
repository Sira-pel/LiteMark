use std::sync::Arc;

mod blocks;
mod lists;
mod tables;

pub(crate) use blocks::*;
pub use lists::*;
pub use tables::*;

#[derive(Clone, Copy)]
struct SourceLine<'a> {
    start: usize,
    end: usize,
    text: &'a str,
}

/// Source lines of a text from a line start to the end (plus the empty last
/// line after a final line ending), split only as far as the parser reads.
/// An incremental parse usually catches up with the old parse a line or two
/// after the edit, so it shouldn't split the rest of the file first.
struct LineSource<'a> {
    text: &'a str,
    lines: Vec<SourceLine<'a>>,
    /// Byte where the next unsplit line starts.
    next: usize,
    /// Every line, including any final empty one, has been split.
    done: bool,
}

impl<'a> LineSource<'a> {
    fn from(text: &'a str, from: usize) -> Self {
        Self {
            text,
            lines: Vec::new(),
            next: from,
            done: false,
        }
    }

    /// All lines of `text`, split up front (a full parse reads them all).
    fn all(text: &'a str) -> Self {
        Self {
            text,
            lines: all_source_lines(text),
            next: text.len(),
            done: true,
        }
    }

    fn get(&mut self, index: usize) -> Option<SourceLine<'a>> {
        while self.lines.len() <= index && !self.done {
            self.split_next();
        }
        self.lines.get(index).copied()
    }

    fn len_hint(&self) -> usize {
        self.lines.len()
    }

    fn split_next(&mut self) {
        let bytes = self.text.as_bytes();
        let start = self.next;
        let ends_with_break = self.text.ends_with(['\n', '\r']);
        if start >= bytes.len() {
            // Empty text is one empty line, and so is the end of a text
            // whose last line has an ending.
            if self.text.is_empty() || ends_with_break {
                self.lines.push(SourceLine {
                    start: bytes.len(),
                    end: bytes.len(),
                    text: "",
                });
            }
            self.done = true;
            return;
        }
        let (text_end, end) = match memchr::memchr2(b'\n', b'\r', &bytes[start..]) {
            None => (bytes.len(), bytes.len()),
            Some(skip) => {
                let at = start + skip;
                let crlf = bytes[at] == b'\r' && bytes.get(at + 1) == Some(&b'\n');
                (at, at + 1 + usize::from(crlf))
            }
        };
        self.lines.push(SourceLine {
            start,
            end,
            text: &self.text[start..text_end],
        });
        self.next = end;
        if end >= bytes.len() && !ends_with_break {
            self.done = true;
        }
    }
}

fn source_lines(markdown: &str) -> Vec<SourceLine<'_>> {
    let mut lines = Vec::new();
    let bytes = markdown.as_bytes();
    let mut start = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        // Jump straight to the next line ending.
        match memchr::memchr2(b'\n', b'\r', &bytes[index..]) {
            Some(skip) => index += skip,
            None => break,
        }
        // A line ending is LF, a lone CR, or CRLF (CommonMark 0.31.2).
        if bytes[index] == b'\n' {
            lines.push(SourceLine {
                start,
                end: index + 1,
                text: &markdown[start..index],
            });
            index += 1;
            start = index;
        } else {
            let end = if index + 1 < bytes.len() && bytes[index + 1] == b'\n' {
                index + 2
            } else {
                index + 1
            };
            lines.push(SourceLine {
                start,
                end,
                text: &markdown[start..index],
            });
            index = end;
            start = end;
        }
    }
    if start < bytes.len() {
        lines.push(SourceLine {
            start,
            end: bytes.len(),
            text: &markdown[start..],
        });
    }
    lines
}

// ============================================================================
// Line-Indexed Syntax Projection & Offset Map for Unified True WYSIWYG Editor
// ============================================================================

fn is_blank_line(line: &str) -> bool {
    line.bytes()
        .all(|byte| matches!(byte, b' ' | b'\t' | b'\r'))
}

fn strip_all_space_and_tab(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() && matches!(bytes[index], b' ' | b'\t') {
        index += 1;
    }
    &line[index..]
}

fn is_thematic_break(line: &str) -> bool {
    let bytes = line.as_bytes();
    let Some(&marker) = bytes.first() else {
        return false;
    };
    if !matches!(marker, b'-' | b'_' | b'*') {
        return false;
    }
    let mut count = 0;
    for &byte in bytes {
        if byte == marker {
            count += 1;
        } else if !matches!(byte, b' ' | b'\t' | b'\r') {
            return false;
        }
    }
    count >= 3
}

fn marker_run(body: &str) -> Option<(u8, usize)> {
    let bytes = body.as_bytes();
    let marker = *bytes.first()?;
    if marker != b'`' && marker != b'~' {
        return None;
    }
    let mut length = 0;
    while length < bytes.len() && bytes[length] == marker {
        length += 1;
    }
    if length < 3 || (marker == b'`' && bytes[length..].contains(&b'`')) {
        return None;
    }
    Some((marker, length))
}

fn opening_fence(line: &str) -> Option<(u8, usize)> {
    marker_run(line)
}

fn is_blockquote_marker(line: &str) -> bool {
    line.as_bytes().first() == Some(&b'>')
}

fn is_list_marker_line(line: &str) -> bool {
    let bytes = line.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let rest = if matches!(bytes[0], b'-' | b'+' | b'*') {
        &bytes[1..]
    } else if bytes[0].is_ascii_digit() {
        let mut index = 0;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if index == 0 || index > 9 || index >= bytes.len() {
            return false;
        }
        if !matches!(bytes[index], b'.' | b')') {
            return false;
        }
        &bytes[index + 1..]
    } else {
        return false;
    };
    rest.is_empty() || matches!(rest[0], b' ' | b'\t' | b'\r')
}

fn is_html_block_tag(name: &[u8]) -> bool {
    let Ok(key) = std::str::from_utf8(name) else {
        return false;
    };
    if key.is_empty() || key.len() > 16 {
        return false;
    }
    const TAGS: &str = "address article aside base basefont blockquote body caption center col colgroup dd details dialog dir div dl dt fieldset figcaption figure footer form frame frameset h1 h2 h3 h4 h5 h6 head header hr html iframe legend li link main menu menuitem nav noframes ol optgroup option p param search section summary table tbody td tfoot th thead title tr track ul";
    TAGS.split(' ').any(|tag| tag.eq_ignore_ascii_case(key))
}

fn html_block_type_1_to_6(rest: &[u8]) -> bool {
    const TYPE_1: [&[u8]; 4] = [b"script", b"pre", b"style", b"textarea"];
    for tag in TYPE_1 {
        if rest.len() < tag.len() || !rest[..tag.len()].eq_ignore_ascii_case(tag) {
            continue;
        }
        if rest.len() == tag.len() {
            return true;
        }
        let next = rest[tag.len()];
        if next.is_ascii_whitespace() || next == b'>' {
            return true;
        }
    }
    if rest.starts_with(b"!--") || rest.starts_with(b"?") || rest.starts_with(b"![CDATA[") {
        return true;
    }
    if rest.len() >= 2 && rest[0] == b'!' && rest[1].is_ascii_alphabetic() {
        return true;
    }
    let slash = usize::from(rest.first() == Some(&b'/'));
    let tail = &rest[slash..];
    let mut name_len = 0;
    while name_len < tail.len() && tail[name_len].is_ascii_alphanumeric() {
        name_len += 1;
    }
    if !is_html_block_tag(&tail[..name_len]) {
        return false;
    }
    let after = &tail[name_len..];
    after.is_empty()
        || matches!(after[0], b' ' | b'\t' | b'\r' | b'\n' | b'>')
        || after.starts_with(b"/>")
}

fn is_html_block_start(line: &str) -> bool {
    let bytes = line.as_bytes();
    bytes.first() == Some(&b'<') && html_block_type_1_to_6(&bytes[1..])
}

fn is_ascii_punctuation(byte: u8) -> bool {
    matches!(byte, b'!'..=b'/' | b':'..=b'@' | b'['..=b'`' | b'{'..=b'~')
}

fn is_pulldown_whitespace(byte: u8) -> bool {
    (0x09..=0x0d).contains(&byte) || byte == b' '
}

fn footnote_label_byte_len(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut index = 0;
    let mut only_white_space = true;
    let mut codepoints = 0usize;
    loop {
        if codepoints >= 1000 {
            return None;
        }
        match *bytes.get(index)? {
            b'[' => return None,
            b']' => break,
            b'|' if index > 0 && bytes[index - 1] == b'\\' => {
                index += 1;
                only_white_space = false;
            }
            b'\\' if bytes.get(index + 1) == Some(&b'|') => {
                index += 2;
                codepoints += 1;
                only_white_space = false;
            }
            b'\\'
                if bytes
                    .get(index + 1)
                    .is_some_and(|byte| is_ascii_punctuation(*byte)) =>
            {
                index += 2;
                codepoints += 2;
                only_white_space = false;
            }
            byte if is_pulldown_whitespace(byte) => {
                let start = index;
                let mut whitespaces = 0usize;
                while index < bytes.len() && is_pulldown_whitespace(bytes[index]) {
                    if matches!(bytes[index], b'\n' | b'\r') {
                        return None;
                    }
                    whitespaces += if bytes[index] == b' ' { 1 } else { 2 };
                    index += 1;
                }
                if whitespaces > 1 {
                    codepoints += index - start;
                } else {
                    codepoints += 1;
                }
            }
            byte => {
                only_white_space = false;
                index += 1;
                if byte & 0b1000_0000 != 0 {
                    codepoints += 1;
                }
            }
        }
    }
    if only_white_space {
        None
    } else {
        Some(index + 1)
    }
}

fn is_footnote_definition(line: &str) -> bool {
    let bytes = line.as_bytes();
    if !bytes.starts_with(b"[^") {
        return false;
    }
    let Some(label_len) = footnote_label_byte_len(&line[2..]) else {
        return false;
    };
    bytes.get(2 + label_len) == Some(&b':')
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineKind {
    Empty,
    Paragraph,
    Heading {
        level: u8,
    }, // 1..=6
    Blockquote,
    ListItem {
        ordered: bool,
        number: u32,
        task: Option<bool>, // Some(true) = [x], Some(false) = [ ], None = normal bullet/number
        indent_cols: u16,
        /// Byte offset of `"[ ]"` or `"[x]"` relative to `source.start`, or `u16::MAX` if none.
        checkbox_offset: u16,
    },
    CodeFence {
        language: Option<Box<Box<str>>>,
        opening: bool,
    },
    CodeLine,
    HorizontalRule,
    TableDelimiter,
    TableRow {
        is_header: bool,
        /// Boxed so the rare table variant does not size every `LineKind`
        body: Box<TableRowBody>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TableRowBody {
    /// Byte ranges of each cell's editable content in the document
    pub cells: Vec<std::ops::Range<usize>>,
    /// Full inner byte ranges between pipes (`pipe[k]+1..pipe[k+1]`)
    pub raw_cells: Vec<std::ops::Range<usize>>,
    /// Column alignments parsed from the table's delimiter row, shared by
    /// every row of the table
    pub alignments: Arc<[TableColumnAlign]>,
    /// Full table block byte range in the document
    pub table_source: std::ops::Range<usize>,
}

/// Line-relative byte range (`u32` offsets from `ParsedLine::source.start`).
/// Supports lines up to 4 GB without `u16` overflow, while keeping `ParsedLine`
/// compact and making `Shift::parsed_line` skip all per-line inline vectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LineRange {
    pub start: u32,
    pub end: u32,
}

impl LineRange {
    #[inline]
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    #[inline]
    pub fn from_rel(range: std::ops::Range<usize>) -> Self {
        Self {
            start: u32::try_from(range.start).unwrap_or(u32::MAX),
            end: u32::try_from(range.end).unwrap_or(u32::MAX),
        }
    }

    #[inline]
    pub const fn to_abs(self, line_start: usize) -> std::ops::Range<usize> {
        (line_start + self.start as usize)..(line_start + self.end as usize)
    }
}

pub const STYLE_BOLD: u8 = 1 << 0;
pub const STYLE_ITALIC: u8 = 1 << 1;
pub const STYLE_UNDERLINE: u8 = 1 << 2;
pub const STYLE_STRIKE: u8 = 1 << 3;
pub const STYLE_CODE: u8 = 1 << 4;
pub const STYLE_LINK: u8 = 1 << 5;

/// Compact line-relative styled segment (`12 B`, `Copy`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StyledSegment {
    pub range: LineRange,
    pub flags: u8,
    pub link_index: u16,
}

impl StyledSegment {
    #[inline]
    pub const fn source(&self, line_start: usize) -> std::ops::Range<usize> {
        self.range.to_abs(line_start)
    }

    #[inline]
    pub const fn bold(&self) -> bool {
        (self.flags & STYLE_BOLD) != 0
    }

    #[inline]
    pub const fn italic(&self) -> bool {
        (self.flags & STYLE_ITALIC) != 0
    }

    #[inline]
    pub const fn underline(&self) -> bool {
        (self.flags & STYLE_UNDERLINE) != 0
    }

    #[inline]
    pub const fn strike(&self) -> bool {
        (self.flags & STYLE_STRIKE) != 0
    }

    #[inline]
    pub const fn code(&self) -> bool {
        (self.flags & STYLE_CODE) != 0
    }

    #[inline]
    pub const fn is_link(&self) -> bool {
        (self.flags & STYLE_LINK) != 0
    }

    #[inline]
    pub const fn to_abs(self, line_start: usize) -> AbsStyledSegment {
        AbsStyledSegment {
            source: self.range.to_abs(line_start),
            bold: self.bold(),
            italic: self.italic(),
            underline: self.underline(),
            strike: self.strike(),
            code: self.code(),
            is_link: self.is_link(),
        }
    }
}

/// Document-absolute view of a [`StyledSegment`] for multi-line editing helpers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbsStyledSegment {
    pub source: std::ops::Range<usize>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub code: bool,
    pub is_link: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RawStyledSegment {
    pub(super) source: std::ops::Range<usize>,
    pub(super) bold: bool,
    pub(super) italic: bool,
    pub(super) underline: bool,
    pub(super) strike: bool,
    pub(super) code: bool,
    pub(super) link_url: Option<Arc<str>>,
}

impl RawStyledSegment {
    #[inline]
    pub(super) fn is_plain(&self) -> bool {
        !self.bold
            && !self.italic
            && !self.underline
            && !self.strike
            && !self.code
            && self.link_url.is_none()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InlineTagKind {
    Bold,
    Italic,
    Underline,
    Strike,
    Code,
    Link,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlinePair {
    pub kind: InlineTagKind,
    pub open_delim: std::ops::Range<usize>,
    pub inner: std::ops::Range<usize>,
    pub close_delim: std::ops::Range<usize>,
    /// Min `open_delim.start` among non-link pairs that share `inner`.
    pub group_open_start: usize,
    /// Max `close_delim.end` among non-link pairs that share `inner`.
    pub group_close_end: usize,
}

impl InlinePair {
    pub fn new(
        kind: InlineTagKind,
        open_delim: std::ops::Range<usize>,
        inner: std::ops::Range<usize>,
        close_delim: std::ops::Range<usize>,
    ) -> Self {
        let group_open_start = open_delim.start;
        let group_close_end = close_delim.end;
        Self {
            kind,
            open_delim,
            inner,
            close_delim,
            group_open_start,
            group_close_end,
        }
    }
}

/// Line-relative stored representation of [`InlinePair`] (`36 B`, `Copy`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineInlinePair {
    pub kind: InlineTagKind,
    pub open_delim: LineRange,
    pub inner: LineRange,
    pub close_delim: LineRange,
    pub group_open_start: u32,
    pub group_close_end: u32,
}

impl LineInlinePair {
    #[inline]
    pub fn from_rel(pair: &InlinePair) -> Self {
        Self {
            kind: pair.kind,
            open_delim: LineRange::from_rel(pair.open_delim.clone()),
            inner: LineRange::from_rel(pair.inner.clone()),
            close_delim: LineRange::from_rel(pair.close_delim.clone()),
            group_open_start: u32::try_from(pair.group_open_start).unwrap_or(u32::MAX),
            group_close_end: u32::try_from(pair.group_close_end).unwrap_or(u32::MAX),
        }
    }

    #[inline]
    pub const fn to_abs(self, line_start: usize) -> InlinePair {
        InlinePair {
            kind: self.kind,
            open_delim: self.open_delim.to_abs(line_start),
            inner: self.inner.to_abs(line_start),
            close_delim: self.close_delim.to_abs(line_start),
            group_open_start: line_start + self.group_open_start as usize,
            group_close_end: line_start + self.group_close_end as usize,
        }
    }
}

/// Maps between a visible WYSIWYG display string and original source byte offsets.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RowOffsetMap {
    /// Visible slices from the source buffer: `(display_range, source_range)`
    pub segments: Vec<(std::ops::Range<usize>, std::ops::Range<usize>)>,
    pub line_start: usize,
    pub line_end: usize,
    pub visible_start: usize,
}

impl RowOffsetMap {
    pub fn identity(start: usize, end: usize) -> Self {
        let len = end.saturating_sub(start);
        Self {
            segments: vec![(0..len, start..end)],
            line_start: start,
            line_end: end,
            visible_start: start,
        }
    }

    /// The map `from_hidden` builds, without the display string: only the
    /// line's byte range and its hidden ranges are needed, so it can be
    /// rebuilt on demand instead of being kept for every line.
    pub fn from_hidden_ranges(
        line_start: usize,
        line_end: usize,
        visible_start: usize,
        hidden_base: usize,
        hidden: &[LineRange],
    ) -> Self {
        let mut segments = Vec::with_capacity(hidden.len() + 1);
        let mut disp = 0usize;
        let mut push = |src: std::ops::Range<usize>| {
            let start = disp;
            disp += src.len();
            segments.push((start..disp, src));
        };
        let mut cursor = line_start;
        for &h in hidden {
            let abs = h.to_abs(hidden_base);
            let h_start = abs.start.clamp(line_start, line_end);
            let h_end = abs.end.clamp(line_start, line_end);
            if h_start > cursor {
                push(cursor..h_start);
            }
            cursor = cursor.max(h_end);
        }
        if cursor < line_end {
            push(cursor..line_end);
        }
        let visible_start = visible_start.clamp(line_start, line_end);
        if segments.is_empty() {
            let anchor = visible_start;
            segments.push((0..0, anchor..anchor));
        }
        Self {
            segments,
            line_start,
            line_end,
            visible_start,
        }
    }

    pub fn from_hidden(
        line_text: &str,
        line_start: usize,
        visible_start: usize,
        hidden_base: usize,
        hidden: &[LineRange],
    ) -> (String, Self) {
        let line_end = line_start + line_text.len();
        // Walk the visible gaps twice: once to size the buffers exactly, once
        // to fill them, so no temporary list is allocated.
        let for_each_visible = |f: &mut dyn FnMut(std::ops::Range<usize>)| {
            let mut cursor = line_start;
            for &h in hidden {
                let abs = h.to_abs(hidden_base);
                let h_start = abs.start.clamp(line_start, line_end);
                let h_end = abs.end.clamp(line_start, line_end);
                if h_start > cursor {
                    f(cursor..h_start);
                }
                cursor = cursor.max(h_end);
            }
            if cursor < line_end {
                f(cursor..line_end);
            }
        };

        let mut visible_bytes = 0usize;
        let mut visible_count = 0usize;
        for_each_visible(&mut |r| {
            visible_bytes += r.len();
            visible_count += 1;
        });

        let mut display = String::with_capacity(visible_bytes);
        let mut segments = Vec::with_capacity(visible_count.max(1));
        for_each_visible(&mut |src| {
            let slice = &line_text[(src.start - line_start)..(src.end - line_start)];
            let disp_start = display.len();
            display.push_str(slice);
            segments.push((disp_start..display.len(), src));
        });

        if segments.is_empty() {
            let anchor = visible_start.clamp(line_start, line_end);
            segments.push((0..0, anchor..anchor));
        }

        (
            display,
            Self {
                segments,
                line_start,
                line_end,
                visible_start: visible_start.clamp(line_start, line_end),
            },
        )
    }

    /// Show each `\t` of a raw row as spaces up to the next `tab_width` column
    /// (counted from the row start). Returns `None` when the row has no tab.
    /// The text keeps its real tabs; only the shaped string and this map change.
    pub fn expand_tabs(text: &str, row_start: usize, tab_width: usize) -> Option<(String, Self)> {
        if tab_width == 0 || !text.contains('\t') {
            return None;
        }
        let mut display = String::with_capacity(text.len() + 3 * tab_width);
        let mut segments = Vec::new();
        let mut col = 0usize;
        let mut run_src = 0usize;
        let mut run_disp = 0usize;
        for (i, ch) in text.char_indices() {
            if ch != '\t' {
                display.push(ch);
                col += 1;
                continue;
            }
            if i > run_src {
                segments.push((run_disp..display.len(), row_start + run_src..row_start + i));
            }
            let width = tab_width - col % tab_width;
            let disp_start = display.len();
            display.extend(std::iter::repeat_n(' ', width));
            segments.push((disp_start..display.len(), row_start + i..row_start + i + 1));
            col += width;
            run_src = i + 1;
            run_disp = display.len();
        }
        if text.len() > run_src {
            segments.push((
                run_disp..display.len(),
                row_start + run_src..row_start + text.len(),
            ));
        }
        let end = row_start + text.len();
        Some((
            display,
            Self {
                segments,
                line_start: row_start,
                line_end: end,
                visible_start: row_start,
            },
        ))
    }

    /// Map a byte offset in the source Markdown to the byte offset in `ShapedLine`.
    pub fn source_to_display(&self, src: usize) -> usize {
        if self.segments.is_empty() {
            return 0;
        }
        for (disp, s) in &self.segments {
            if src < s.start {
                return disp.start;
            }
            if src >= s.start && src <= s.end {
                if disp.len() != s.len() {
                    // An expanded span (a tab shown as spaces): the caret sits at
                    // one edge of it, never inside.
                    return if src == s.end { disp.end } else { disp.start };
                }
                return disp.start + (src - s.start).min(disp.len());
            }
        }
        self.segments.last().map(|(d, _)| d.end).unwrap_or(0)
    }

    /// Map a byte offset in `ShapedLine` back to source Markdown.
    pub fn display_to_source(&self, disp: usize) -> usize {
        if self.segments.is_empty() {
            return self.visible_start;
        }
        for (d, s) in &self.segments {
            if disp < d.end {
                if d.len() != s.len() {
                    // Inside an expanded span, snap to the nearer edge.
                    let into = disp.saturating_sub(d.start);
                    return if into * 2 >= d.len() { s.end } else { s.start };
                }
                let offset_in_seg = disp.saturating_sub(d.start).min(s.len());
                return s.start + offset_in_seg;
            }
        }
        self.segments
            .last()
            .map(|(_, s)| s.end)
            .unwrap_or(self.line_end)
    }

    fn slice_source(pos: usize, d: &std::ops::Range<usize>, s: &std::ops::Range<usize>) -> usize {
        if d.len() != s.len() {
            if pos <= d.start {
                s.start
            } else if pos >= d.end || (pos - d.start) * 2 >= d.len() {
                s.end
            } else {
                s.start
            }
        } else {
            s.start + (pos - d.start)
        }
    }

    /// Slice a sub-map for a wrapped visual row covering `disp_range` in `display_text`.
    /// `(line_start, line_end)` of `slice_display(disp_range)`, without
    /// building the sliced map.
    #[cfg(test)]
    pub fn slice_bounds(&self, disp_range: std::ops::Range<usize>) -> (usize, usize) {
        let mut first = None;
        let mut last = None;
        for (d, s) in &self.segments {
            if d.end <= disp_range.start || d.start >= disp_range.end {
                continue;
            }
            let overlap_start = d.start.max(disp_range.start);
            let overlap_end = d.end.min(disp_range.end);
            if overlap_end > overlap_start
                || (d.start == d.end && d.start >= disp_range.start && d.start <= disp_range.end)
            {
                first.get_or_insert_with(|| Self::slice_source(overlap_start, d, s));
                last = Some(Self::slice_source(overlap_end, d, s));
            }
        }
        (
            first.unwrap_or_else(|| self.display_to_source(disp_range.start)),
            last.unwrap_or_else(|| self.display_to_source(disp_range.end)),
        )
    }

    pub fn slice_display(&self, disp_range: std::ops::Range<usize>) -> Self {
        let mut sub_segments = Vec::new();
        for (d, s) in &self.segments {
            if d.end <= disp_range.start || d.start >= disp_range.end {
                continue;
            }
            let overlap_start = d.start.max(disp_range.start);
            let overlap_end = d.end.min(disp_range.end);
            if overlap_end > overlap_start
                || (d.start == d.end && d.start >= disp_range.start && d.start <= disp_range.end)
            {
                let src_start = Self::slice_source(overlap_start, d, s);
                let src_end = Self::slice_source(overlap_end, d, s);
                let new_d_start = overlap_start - disp_range.start;
                let new_d_end = overlap_end - disp_range.start;
                sub_segments.push((new_d_start..new_d_end, src_start..src_end));
            }
        }
        let line_start = sub_segments
            .first()
            .map(|(_, s)| s.start)
            .unwrap_or_else(|| self.display_to_source(disp_range.start));
        let line_end = sub_segments
            .last()
            .map(|(_, s)| s.end)
            .unwrap_or_else(|| self.display_to_source(disp_range.end));
        if sub_segments.is_empty() {
            sub_segments.push((0..0, line_start..line_start));
        }
        Self {
            segments: sub_segments,
            line_start,
            line_end,
            visible_start: line_start,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedLine {
    pub line_index: u32,
    /// Full byte range of the line (excluding `\n`)
    pub source: std::ops::Range<usize>,
    /// Heading content end relative to `source.start` (before any closing-hash
    /// run or trailing hard-break `\`); `source.len()` for every other line.
    pub content_end: u32,
    /// Byte length of block prefix hidden in WYSIWYG (e.g. `# `, `- [ ] `, `> `)
    pub prefix_len: u32,
    /// Blockquote nesting depth (`0` = not inside a blockquote, `1` = `> `,
    /// `2` = `> > `, etc.). Fits in existing tail padding (`72 B` total).
    pub quote_depth: u8,
    pub kind: LineKind,
    /// All byte ranges hidden in WYSIWYG on this line, relative to `source.start`
    /// (sorted, non-overlapping).
    pub hidden_ranges: Box<[LineRange]>,
    /// Inline markup data; `None` for lines that carry none (blank, heading,
    /// list, code and plain-text lines), so they cost no extra allocation.
    inline: Option<Box<LineInline>>,
}

pub type LineLinkHits = Arc<[(LineRange, Arc<str>)]>;

/// The inline data of a line that has any: styled text, links or hidden
/// inline markup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineInline {
    /// Paired inline delimiter ranges (line-relative) for atomic backspace & smart toggle-off
    pub inline_pairs: Box<[LineInlinePair]>,
    /// Visible text in WYSIWYG mode when it differs from `&content[prefix_end()..source.end]`
    /// (e.g. inline hidden markup). `None` borrows zero-copy from the document.
    display_text_owned: Option<Box<str>>,
    pub styled_segments: Box<[StyledSegment]>,
    /// Line-relative link spans `(range, url)`, filled once when sealed so
    /// painting can share them instead of rebuilding them every frame.
    pub link_hits: Option<LineLinkHits>,
}

impl LineInline {
    /// Seal line-relative inline data into a [`LineInline`] box, or `None` when
    /// it is all empty (so equal parses always agree on whether a line has a box).
    pub(super) fn boxed(
        inline_pairs: Vec<InlinePair>,
        raw_segments: Vec<RawStyledSegment>,
        display_text_owned: Option<Box<str>>,
    ) -> Option<Box<Self>> {
        if inline_pairs.is_empty() && raw_segments.is_empty() && display_text_owned.is_none() {
            return None;
        }
        let inline_pairs: Box<[LineInlinePair]> =
            inline_pairs.iter().map(LineInlinePair::from_rel).collect();
        let mut links: Vec<(LineRange, Arc<str>)> = Vec::new();
        let mut styled_segments: Vec<StyledSegment> = Vec::with_capacity(raw_segments.len());
        for seg in raw_segments {
            let range = LineRange::from_rel(seg.source.clone());
            let mut flags = 0u8;
            if seg.bold {
                flags |= STYLE_BOLD;
            }
            if seg.italic {
                flags |= STYLE_ITALIC;
            }
            if seg.underline {
                flags |= STYLE_UNDERLINE;
            }
            if seg.strike {
                flags |= STYLE_STRIKE;
            }
            if seg.code {
                flags |= STYLE_CODE;
            }
            let link_index = if let Some(url) = seg.link_url {
                flags |= STYLE_LINK;
                if seg.source.end > seg.source.start {
                    let idx = links.len();
                    links.push((range, url));
                    u16::try_from(idx).unwrap_or(u16::MAX)
                } else {
                    u16::MAX
                }
            } else {
                u16::MAX
            };
            styled_segments.push(StyledSegment {
                range,
                flags,
                link_index,
            });
        }
        let link_hits = if links.is_empty() {
            None
        } else {
            Some(Arc::from(links))
        };
        Some(Box::new(Self {
            inline_pairs,
            display_text_owned,
            styled_segments: styled_segments.into(),
            link_hits,
        }))
    }
}

impl ParsedLine {
    #[cfg(all(test, feature = "heap-count"))]
    pub fn offset_map_for_bench(&self) -> Option<&RowOffsetMap> {
        None
    }

    #[cfg(all(test, feature = "heap-count"))]
    pub fn display_owned_len_for_bench(&self) -> usize {
        self.display_text_owned().map_or(0, |s| s.len())
    }

    #[cfg(all(test, feature = "heap-count"))]
    pub fn has_inline_for_bench(&self) -> bool {
        self.inline.is_some()
    }

    /// Document-absolute hidden ranges for this line.
    #[inline]
    pub fn hidden_ranges_abs(
        &self,
    ) -> impl ExactSizeIterator<Item = std::ops::Range<usize>> + DoubleEndedIterator + '_ {
        let base = self.source.start;
        self.hidden_ranges.iter().map(move |r| r.to_abs(base))
    }

    /// Line-relative paired inline delimiter ranges.
    #[inline]
    pub fn line_inline_pairs(&self) -> &[LineInlinePair] {
        self.inline.as_deref().map_or(&[], |i| &i.inline_pairs)
    }

    /// Paired inline delimiter ranges (converted to document-absolute offsets).
    #[inline]
    pub fn inline_pairs(
        &self,
    ) -> impl ExactSizeIterator<Item = InlinePair> + DoubleEndedIterator + '_ {
        let base = self.source.start;
        self.line_inline_pairs().iter().map(move |p| p.to_abs(base))
    }

    #[inline]
    pub fn styled_segments(&self) -> &[StyledSegment] {
        self.inline.as_deref().map_or(&[], |i| &i.styled_segments)
    }

    /// Styled segments converted to document-absolute [`AbsStyledSegment`]s.
    #[inline]
    pub fn abs_styled_segments(
        &self,
    ) -> impl ExactSizeIterator<Item = AbsStyledSegment> + DoubleEndedIterator + '_ {
        let base = self.source.start;
        self.styled_segments().iter().map(move |s| s.to_abs(base))
    }

    /// Line-relative link spans `(range, url)`.
    #[cfg(test)]
    #[inline]
    pub fn link_hits(&self) -> &[(LineRange, Arc<str>)] {
        self.inline
            .as_deref()
            .and_then(|i| i.link_hits.as_deref())
            .unwrap_or(&[])
    }

    /// Shared `Arc` of line-relative link spans, if this line has any links.
    #[inline]
    pub fn link_hits_arc(&self) -> Option<&LineLinkHits> {
        self.inline.as_deref()?.link_hits.as_ref()
    }

    /// Resolve the link URL for a [`StyledSegment`] on this line.
    #[cfg(test)]
    #[inline]
    pub fn segment_link_url(&self, seg: &StyledSegment) -> Option<&str> {
        if !seg.is_link() {
            return None;
        }
        let hits = self.link_hits();
        if seg.link_index != u16::MAX {
            if let Some((_, url)) = hits.get(seg.link_index as usize) {
                return Some(url.as_ref());
            }
        }
        hits.iter()
            .find(|(r, _)| *r == seg.range)
            .map(|(_, url)| url.as_ref())
    }

    #[inline]
    pub(crate) fn display_text_owned(&self) -> Option<&str> {
        self.inline.as_deref()?.display_text_owned.as_deref()
    }

    /// The explicit offset map of a line with inline hidden markup, rebuilt
    /// from its hidden ranges. `None` means the O(1) prefix arithmetic
    /// (`prefix_end()..source.end`) applies. Not stored: it costs a little to
    /// build, so callers should reuse the result across several lookups.
    pub(crate) fn offset_map_cached(&self) -> Option<RowOffsetMap> {
        self.display_text_owned()?;
        Some(RowOffsetMap::from_hidden_ranges(
            self.source.start,
            self.source.end,
            self.prefix_end(),
            self.source.start,
            &self.hidden_ranges,
        ))
    }

    /// Byte offset where the hidden block prefix ends.
    #[inline]
    pub fn prefix_end(&self) -> usize {
        self.source.start + self.prefix_len as usize
    }

    /// Visible text in WYSIWYG mode (with hidden ranges stripped). Borrows
    /// zero-copy from `content` whenever the line has no inline hidden markup.
    #[inline]
    pub fn display_text<'a>(&'a self, content: &'a str) -> &'a str {
        if let Some(owned) = self.display_text_owned() {
            owned
        } else {
            let start = self.prefix_end().min(self.source.end);
            content.get(start..self.source.end).unwrap_or("")
        }
    }

    /// Byte length of the visible WYSIWYG display text, in O(1) without
    /// borrowing the document text.
    #[inline]
    pub fn display_len(&self) -> usize {
        match self.display_text_owned() {
            Some(owned) => owned.len(),
            None => self.source.end.saturating_sub(self.prefix_end()),
        }
    }

    #[inline]
    fn has_only_prefix_hidden(&self) -> bool {
        (self.hidden_ranges.is_empty() && self.prefix_len == 0)
            || (self.hidden_ranges.len() == 1
                && self.hidden_ranges[0].start == 0
                && self.hidden_ranges[0].end as usize == self.prefix_len as usize)
    }

    #[inline]
    fn visit_visible_segments(
        &self,
        mut f: impl FnMut(std::ops::Range<usize>, std::ops::Range<usize>) -> bool,
    ) {
        let line_start = self.source.start;
        let line_end = self.source.end;
        let mut disp = 0usize;
        let mut cursor = line_start;
        let mut emitted = false;
        for &h in &self.hidden_ranges {
            let abs = h.to_abs(line_start);
            let h_start = abs.start.clamp(line_start, line_end);
            let h_end = abs.end.clamp(line_start, line_end);
            if h_start > cursor {
                let start = disp;
                disp += h_start - cursor;
                emitted = true;
                if !f(start..disp, cursor..h_start) {
                    return;
                }
            }
            cursor = cursor.max(h_end);
        }
        if cursor < line_end {
            let start = disp;
            disp += line_end - cursor;
            emitted = true;
            let _ = f(start..disp, cursor..line_end);
        }
        if !emitted {
            let anchor = self.prefix_end().clamp(line_start, line_end);
            let _ = f(0..0, anchor..anchor);
        }
    }

    #[inline]
    pub fn source_to_display(&self, source_offset: usize) -> usize {
        if self.has_only_prefix_hidden() {
            let vis = self.prefix_end().min(self.source.end);
            return source_offset
                .saturating_sub(vis)
                .min(self.source.end.saturating_sub(vis));
        }
        let mut result = 0usize;
        self.visit_visible_segments(|d, s| {
            if source_offset < s.start {
                result = d.start;
                return false;
            }
            if source_offset <= s.end {
                result = d.start + (source_offset - s.start).min(d.len());
                return false;
            }
            result = d.end;
            true
        });
        result
    }

    #[inline]
    pub fn display_to_source(&self, display_offset: usize) -> usize {
        if self.has_only_prefix_hidden() {
            let vis = self.prefix_end().min(self.source.end);
            return (vis + display_offset).min(self.source.end);
        }
        let mut result = self.prefix_end().clamp(self.source.start, self.source.end);
        self.visit_visible_segments(|d, s| {
            if display_offset < d.end {
                let offset_in_seg = display_offset.saturating_sub(d.start).min(s.len());
                result = s.start + offset_in_seg;
                return false;
            }
            result = s.end;
            true
        });
        result
    }

    #[inline]
    pub fn slice_bounds(&self, disp_range: std::ops::Range<usize>) -> (usize, usize) {
        if self.has_only_prefix_hidden() {
            return (
                self.display_to_source(disp_range.start),
                self.display_to_source(disp_range.end),
            );
        }
        let mut first = None;
        let mut last = None;
        self.visit_visible_segments(|d, s| {
            if d.end <= disp_range.start || d.start >= disp_range.end {
                return true;
            }
            let overlap_start = d.start.max(disp_range.start);
            let overlap_end = d.end.min(disp_range.end);
            if overlap_end > overlap_start
                || (d.start == d.end && d.start >= disp_range.start && d.start <= disp_range.end)
            {
                first.get_or_insert_with(|| s.start + (overlap_start - d.start));
                last = Some(s.start + (overlap_end - d.start));
            }
            true
        });
        (
            first.unwrap_or_else(|| self.display_to_source(disp_range.start)),
            last.unwrap_or_else(|| self.display_to_source(disp_range.end)),
        )
    }

    pub fn slice_display(&self, disp_range: std::ops::Range<usize>) -> RowOffsetMap {
        if let Some(map) = self.offset_map_cached() {
            map.slice_display(disp_range)
        } else {
            let start = self.display_to_source(disp_range.start);
            let end = self.display_to_source(disp_range.end);
            RowOffsetMap::identity(start, end)
        }
    }

    /// Materialize an `Arc<RowOffsetMap>` (used only for visible shaped rows).
    pub fn to_offset_map(&self) -> Arc<RowOffsetMap> {
        if let Some(map) = self.offset_map_cached() {
            return Arc::new(map);
        }
        let vis = self.prefix_end().min(self.source.end);
        let len = self.source.end.saturating_sub(vis);
        Arc::new(RowOffsetMap {
            segments: vec![(0..len, vis..self.source.end)],
            line_start: self.source.start,
            line_end: self.source.end,
            visible_start: vis,
        })
    }

    /// Byte range of `"[ ]"` or `"[x]"` on a task-list item, if present.
    #[inline]
    pub fn checkbox_source(&self) -> Option<std::ops::Range<usize>> {
        match self.kind {
            LineKind::ListItem {
                checkbox_offset, ..
            } if checkbox_offset != u16::MAX => {
                let start = self.source.start + checkbox_offset as usize;
                Some(start..start + 3)
            }
            _ => None,
        }
    }
}

/// Build `display_text_owned` only when `hidden` hides bytes beyond the
/// leading `0..prefix_len` prefix. Otherwise it stays `None` (zero heap
/// allocations). The offset map is rebuilt from `hidden` when needed.
pub(super) fn build_display(
    line: &str,
    prefix_len: usize,
    hidden: &[LineRange],
) -> Option<Box<str>> {
    let only_prefix = (hidden.is_empty() && prefix_len == 0)
        || (hidden.len() == 1 && hidden[0].start == 0 && hidden[0].end as usize == prefix_len);
    if only_prefix {
        return None;
    }
    let (display, _) = RowOffsetMap::from_hidden(line, 0, prefix_len, 0, hidden);
    Some(display.into_boxed_str())
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedDocument {
    pub lines: Vec<ParsedLine>,
    /// Fenced code block ranges `(open_fence_line_range, close_fence_line_range)`
    pub code_blocks: Vec<(std::ops::Range<usize>, std::ops::Range<usize>)>,
    /// Bookkeeping for incremental updates; not part of equality.
    pub meta: ParseMeta,
}

/// What an incremental update and the caches built on a parse need, beyond
/// the parse itself. Two parses of the same text compare equal whatever
/// their meta.
#[derive(Clone, Debug, Default)]
pub struct ParseMeta {
    /// Parser state before every `CHECKPOINT_EVERY`-th line (and wherever
    /// an update resumed), to resume after an edit.
    pub(crate) checkpoints: Vec<(usize, ParseState)>,
    /// Changes whenever the lines change, so caches can tell parses apart.
    pub generation: u64,
    /// Number of [`LineKind::TableRow`] lines in the document.
    pub(crate) table_line_count: usize,
    /// The last incremental update: `(generation before it, first line,
    /// lines removed, lines inserted)`. Lets caches keyed by line index
    /// follow the edit instead of starting over.
    pub last_splice: Option<(u64, usize, usize, usize)>,
    /// Byte delta of the last incremental edit (`new_len - old_len`), used
    /// by caches that also store byte offsets (e.g. `outline_cache`).
    pub last_byte_delta: isize,
}

impl PartialEq for ParseMeta {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for ParseMeta {}

impl ParsedDocument {
    /// Every inline delimiter pair in the document, line by line. Only the
    /// tests want the flat view; the editor looks pairs up by line.
    #[cfg(test)]
    pub fn all_inline_pairs(&self) -> impl Iterator<Item = InlinePair> + '_ {
        self.lines.iter().flat_map(|l| l.inline_pairs())
    }

    /// Index of the line whose source range contains `offset` (inclusive end,
    /// so a line's terminating newline maps to that line).
    pub fn line_index_at(&self, offset: usize) -> Option<usize> {
        if self.lines.is_empty() {
            return None;
        }
        let i = self.lines.partition_point(|l| l.source.start <= offset);
        if i == 0 {
            return None;
        }
        let idx = i - 1;
        if offset <= self.lines[idx].source.end {
            Some(idx)
        } else {
            None
        }
    }

    pub fn line_at_offset(&self, offset: usize) -> Option<&ParsedLine> {
        self.line_index_at(offset).map(|i| &self.lines[i])
    }

    /// Lines whose source overlaps half-open `range`. An empty range is the
    /// line that contains the caret. `pad` extra lines on each side are
    /// included so neighbor-span expansion still sees adjacent pairs.
    pub fn overlapping_line_span(
        &self,
        range: std::ops::Range<usize>,
        pad: usize,
    ) -> std::ops::Range<usize> {
        if self.lines.is_empty() {
            return 0..0;
        }
        let (first, last) = if range.end <= range.start {
            match self.line_index_at(range.start) {
                Some(i) => (i, i + 1),
                None => return 0..0,
            }
        } else {
            let first = self.lines.partition_point(|l| l.source.end <= range.start);
            let last = self.lines.partition_point(|l| l.source.start < range.end);
            (first, last.max(first))
        };
        first.saturating_sub(pad)..last.saturating_add(pad).min(self.lines.len())
    }

    pub fn lines_overlapping(&self, range: std::ops::Range<usize>) -> &[ParsedLine] {
        let span = self.overlapping_line_span(range, 0);
        &self.lines[span]
    }

    pub fn lines_near(&self, range: std::ops::Range<usize>) -> &[ParsedLine] {
        let span = self.overlapping_line_span(range, 1);
        &self.lines[span]
    }

    /// The text of `range` as the WYSIWYG view shows it: inline markers,
    /// block prefixes, fence lines and rules are dropped, and list items get
    /// a plain bullet back. Tables are copied as their Markdown source.
    pub fn visible_text(&self, content: &str, range: std::ops::Range<usize>) -> String {
        let mut out = String::new();
        // A line counts while the range touches its text or its line ending.
        let first = self.lines.partition_point(|l| l.source.end < range.start);
        let last = self.lines.partition_point(|l| l.source.start < range.end);
        for index in first..last.max(first) {
            let line = &self.lines[index];
            let next_start = self
                .lines
                .get(index + 1)
                .map(|next| next.source.start)
                .unwrap_or(content.len());
            let clip_end = range.end.min(line.source.end);
            let clip = range.start.max(line.source.start).min(clip_end)..clip_end;
            let ending = &content[range.start.clamp(line.source.end, next_start)
                ..range.end.clamp(line.source.end, next_start)];
            match &line.kind {
                LineKind::CodeFence { .. } | LineKind::HorizontalRule => continue,
                LineKind::TableRow { .. } | LineKind::TableDelimiter => {
                    out.push_str(&content[clip]);
                }
                kind => {
                    if let LineKind::ListItem {
                        ordered,
                        number,
                        task,
                        indent_cols,
                        ..
                    } = kind
                    {
                        if clip.start <= line.prefix_end() {
                            out.push_str(&" ".repeat(*indent_cols as usize));
                            match (task, ordered) {
                                (Some(true), _) => out.push_str("☑ "),
                                (Some(false), _) => out.push_str("☐ "),
                                (None, true) => out.push_str(&format!("{number}. ")),
                                (None, false) => out.push_str("• "),
                            }
                        }
                    }
                    let mut cursor = clip.start;
                    for &h in &line.hidden_ranges {
                        let hidden = h.to_abs(line.source.start);
                        if hidden.end <= cursor || hidden.start >= clip.end {
                            continue;
                        }
                        if hidden.start > cursor {
                            out.push_str(&content[cursor..hidden.start]);
                        }
                        cursor = cursor.max(hidden.end);
                    }
                    if cursor < clip.end {
                        out.push_str(&content[cursor..clip.end]);
                    }
                }
            }
            out.push_str(ending);
        }
        out
    }
}

pub(super) fn merge_line_ranges(ranges: Vec<std::ops::Range<usize>>) -> Vec<LineRange> {
    merge_ranges(ranges)
        .into_iter()
        .map(LineRange::from_rel)
        .collect()
}

fn merge_ranges(mut ranges: Vec<std::ops::Range<usize>>) -> Vec<std::ops::Range<usize>> {
    ranges.retain(|r| r.end > r.start);
    ranges.sort_by_key(|r| (r.start, r.end));
    let mut merged: Vec<std::ops::Range<usize>> = Vec::with_capacity(ranges.len());
    for r in ranges {
        if let Some(last) = merged.last_mut() {
            if r.start <= last.end {
                last.end = last.end.max(r.end);
                continue;
            }
        }
        merged.push(r);
    }
    merged
}

fn skip_link_destination(bytes: &[u8], open_paren: usize) -> Option<usize> {
    let len = bytes.len();
    let mut j = open_paren + 1;
    while j < len && bytes[j].is_ascii_whitespace() {
        j += 1;
    }
    if bytes.get(j) == Some(&b'<') {
        j += 1;
        while j < len {
            match bytes[j] {
                b'\\' if j + 1 < len => j += 2,
                b'>' => {
                    j += 1;
                    break;
                }
                b'<' | b'\n' | b'\r' => return None,
                _ => j += 1,
            }
        }
    }
    let mut depth = 1usize;
    let mut quote: Option<u8> = None;
    while j < len && depth > 0 {
        let b = bytes[j];
        if b == b'\\' && j + 1 < len {
            j += 2;
            continue;
        }
        if let Some(q) = quote {
            if b == q {
                quote = None;
            }
            j += 1;
            continue;
        }
        match b {
            b'(' => {
                depth += 1;
                j += 1;
            }
            b')' => {
                depth -= 1;
                j += 1;
            }
            b'"' | b'\'' => {
                quote = Some(b);
                j += 1;
            }
            _ => j += 1,
        }
    }
    (depth == 0).then_some(j)
}

fn skip_autolink(bytes: &[u8], lt: usize) -> Option<usize> {
    let mut j = lt + 1;
    let mut has_scheme_or_at = false;
    while j < bytes.len() {
        match bytes[j] {
            b'>' => return has_scheme_or_at.then_some(j + 1),
            b'<' | b' ' | b'\t' | b'\n' | b'\r' => return None,
            b':' | b'@' => {
                has_scheme_or_at = true;
                j += 1;
            }
            _ => j += 1,
        }
    }
    None
}

fn paren_match_table(bytes: &[u8], from: usize) -> Vec<usize> {
    let mut matches = vec![0usize; bytes.len() - from];
    let mut stack: Vec<usize> = Vec::new();
    let mut i = from;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if i + 1 < bytes.len() => i += 2,
            b'(' => {
                stack.push(i);
                i += 1;
            }
            b')' => {
                if let Some(open) = stack.pop() {
                    matches[open - from] = i + 1;
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    matches
}

pub(crate) fn prepare_for_pulldown(input: &str) -> std::borrow::Cow<'_, str> {
    let bytes = input.as_bytes();
    let len = bytes.len();
    if memchr::memchr3(b'*', b'~', b'_', bytes).is_none() {
        return std::borrow::Cow::Borrowed(input);
    }
    let mut modified = None;
    let mut failed_tick_widths: u64 = 0;
    let mut failed_wide_ticks: Vec<usize> = Vec::new();
    let mut no_close_paren = false;
    let mut paren_matches: Option<(usize, Vec<usize>)> = None;

    let is_delim = |b: u8| b == b'*' || b == b'~' || b == b'_' || b == b'`';

    let mut i = 0;
    while i < len {
        if bytes[i] == b'\\' && i + 1 < len {
            i += 2;
            continue;
        }
        if bytes[i] == b'`' {
            let open = i;
            while i < len && bytes[i] == b'`' {
                i += 1;
            }
            let width = i - open;
            let already_failed = if width < 64 {
                (failed_tick_widths & (1u64 << width)) != 0
            } else {
                failed_wide_ticks.contains(&width)
            };
            if already_failed {
                continue;
            }
            let mut cursor = i;
            let mut closed_at = None;
            while let Some(rel) = memchr::memchr(b'`', &bytes[cursor..]) {
                let close = cursor + rel;
                cursor = close;
                while cursor < len && bytes[cursor] == b'`' {
                    cursor += 1;
                }
                if cursor - close == width {
                    closed_at = Some(cursor);
                    break;
                }
            }
            if let Some(end) = closed_at {
                i = end;
            } else if width < 64 {
                failed_tick_widths |= 1u64 << width;
            } else {
                failed_wide_ticks.push(width);
            }
            continue;
        }
        if bytes[i] == b']' && bytes.get(i + 1) == Some(&b'(') {
            if !no_close_paren {
                let table_end = paren_matches
                    .as_ref()
                    .and_then(|(base, table)| table.get(i + 1 - *base).copied())
                    .unwrap_or(0);
                if paren_matches.is_none() || table_end > 0 {
                    if let Some(after) = skip_link_destination(bytes, i + 1) {
                        i = after;
                        continue;
                    }
                }
                if table_end > 0 {
                    i = table_end;
                    continue;
                }
                if paren_matches.is_none() {
                    if memchr::memchr(b')', &bytes[i + 2..]).is_none() {
                        no_close_paren = true;
                    } else {
                        paren_matches = Some((i + 1, paren_match_table(bytes, i + 1)));
                    }
                }
            }
            i += 2;
            continue;
        }
        if bytes[i] == b'<' {
            if let Some(after) = skip_autolink(bytes, i) {
                i = after;
                continue;
            }
        }
        if bytes[i] == b'~' || bytes[i] == b'*' || bytes[i] == b'_' {
            let delim_byte = bytes[i];
            let start = i;
            while i < len && bytes[i] == delim_byte {
                i += 1;
            }
            let end = i;

            // Check opening delimiter preceded by alphanumeric and followed by code backtick (e.g. `a**`hi`**` or `a*`hi`*`)
            if start > 0 && end < len {
                let prev_ch = input[..start].chars().next_back().unwrap();
                let next_b = bytes[end];
                if prev_ch.is_alphanumeric() && next_b == b'`' {
                    let prev_len = prev_ch.len_utf8();
                    let buf = modified.get_or_insert_with(|| input.as_bytes().to_vec());
                    for b in &mut buf[start - prev_len..start] {
                        *b = b'.';
                    }
                }
            }

            // Check closing delimiter preceded by inner delimiter and followed by alphanumeric (e.g. `hi*~~y` or `hi`**y`)
            if start > 0 && end < len {
                let prev_b = bytes[start - 1];
                if is_delim(prev_b) {
                    let mut p = start;
                    while p > 0 && is_delim(bytes[p - 1]) {
                        p -= 1;
                    }
                    if p > 0 {
                        let prev_ch = input[..p].chars().next_back().unwrap();
                        let next_ch = input[end..].chars().next().unwrap();
                        if prev_ch.is_alphanumeric() && next_ch.is_alphanumeric() {
                            let ch_len = next_ch.len_utf8();
                            let buf = modified.get_or_insert_with(|| input.as_bytes().to_vec());
                            for b in &mut buf[end..end + ch_len] {
                                *b = b'.';
                            }
                        }
                    }
                }
            }
        } else {
            i += 1;
        }
    }

    match modified {
        Some(buf) => std::borrow::Cow::Owned(String::from_utf8(buf).unwrap()),
        None => std::borrow::Cow::Borrowed(input),
    }
}

type InlineSyntax = (
    Vec<std::ops::Range<usize>>,
    Vec<InlinePair>,
    Vec<RawStyledSegment>,
);

fn parse_inline_syntax(input: &str, base: usize) -> InlineSyntax {
    parse_inline_syntax_inner(input, base, false, false)
}

fn parse_inline_syntax_line(input: &str, base: usize, allow_hard_break: bool) -> InlineSyntax {
    parse_inline_syntax_inner(input, base, false, allow_hard_break)
}

fn parse_inline_syntax_cell(input: &str, base: usize) -> InlineSyntax {
    parse_inline_syntax_inner(input, base, true, false)
}

fn parse_inline_syntax_inner(
    input: &str,
    base: usize,
    in_table_cell: bool,
    allow_hard_break: bool,
) -> InlineSyntax {
    if input.is_empty() {
        return (Vec::new(), Vec::new(), Vec::new());
    }
    // Block structure is decided before this point, so leading indentation
    // is only text. Pulldown would read four columns of it as indented code
    // and drop every inline format (`    <u>x</u>`, a lazy continuation line).
    let body = input.trim_start_matches([' ', '\t']);
    if body.len() < input.len() {
        let lead = input.len() - body.len();
        return parse_inline_syntax_inner(body, base + lead, in_table_cell, allow_hard_break);
    }
    if is_plain_inline(input) {
        return (Vec::new(), Vec::new(), Vec::new());
    }
    parse_inline_with_pulldown(input, base, in_table_cell, allow_hard_break)
}

/// Text pulldown-cmark would return as one unstyled run: no byte that can
/// open inline syntax (`*`, `_`, `~`, `` ` ``, `[`, `<`, `\`).
fn is_plain_inline(input: &str) -> bool {
    !input
        .as_bytes()
        .iter()
        .any(|&b| matches!(b, b'*' | b'_' | b'~' | b'`' | b'[' | b'<' | b'\\'))
}

fn parse_inline_with_pulldown(
    input: &str,
    base: usize,
    in_table_cell: bool,
    allow_hard_break: bool,
) -> InlineSyntax {
    let prepared = prepare_for_pulldown(input);
    let owned_prefixed;
    let (parse_input, offset_shift) = if in_table_cell {
        owned_prefixed = format!("x {prepared}");
        (owned_prefixed.as_str(), 2usize)
    } else {
        (prepared.as_ref(), 0usize)
    };
    let mut options = pulldown_cmark::Options::empty();
    options.insert(pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
    let parser = pulldown_cmark::Parser::new_ext(parse_input, options).into_offset_iter();

    let mut raw_hidden = Vec::new();
    let mut inline_pairs = Vec::new();
    let mut styled_segments = Vec::new();
    let mut blocked_ranges: Vec<std::ops::Range<usize>> = Vec::new();

    let mut bold_stack: Vec<usize> = Vec::new();
    let mut italic_stack: Vec<usize> = Vec::new();
    let mut strike_stack: Vec<usize> = Vec::new();
    let mut underline_stack: Vec<usize> = Vec::new();
    let mut link_stack: Vec<(usize, Arc<str>, usize)> = Vec::new(); // (outer_start, url, last_inner_end)
    let mut image_stack: Vec<usize> = Vec::new();

    for (event, raw_range) in parser {
        if raw_range.end <= offset_shift {
            continue;
        }
        let range = (raw_range.start.max(offset_shift) - offset_shift)
            ..(raw_range.end.max(offset_shift) - offset_shift);
        let abs_start = base + range.start;
        let abs_end = base + range.end;
        match event {
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::Image { .. }) => {
                image_stack.push(abs_start);
            }
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Image) => {
                if let Some(open_start) = image_stack.pop() {
                    blocked_ranges.push(open_start..abs_end);
                    if image_stack.is_empty() {
                        if let Some(last_link) = link_stack.last_mut() {
                            last_link.2 = last_link.2.max(abs_end);
                        }
                        if !link_stack.is_empty() && abs_end > open_start {
                            styled_segments.push(RawStyledSegment {
                                source: open_start..abs_end,
                                bold: !bold_stack.is_empty(),
                                italic: !italic_stack.is_empty(),
                                underline: !underline_stack.is_empty(),
                                strike: !strike_stack.is_empty(),
                                code: false,
                                link_url: link_stack.last().map(|(_, u, _)| u.clone()),
                            });
                        }
                    }
                }
            }
            _ if !image_stack.is_empty() => {}
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::Strong) => {
                bold_stack.push(abs_start);
            }
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Strong) => {
                if let Some(last_link) = link_stack.last_mut() {
                    last_link.2 = last_link.2.max(abs_end);
                }
                if let Some(open_start) = bold_stack.pop() {
                    let delim_len = 2.min((abs_end.saturating_sub(open_start)) / 2);
                    if delim_len > 0 {
                        let open_d = open_start..(open_start + delim_len);
                        let close_d = (abs_end - delim_len)..abs_end;
                        let inner = open_d.end..close_d.start;
                        raw_hidden.push(open_d.clone());
                        raw_hidden.push(close_d.clone());
                        inline_pairs.push(InlinePair::new(
                            InlineTagKind::Bold,
                            open_d,
                            inner,
                            close_d,
                        ));
                    }
                }
            }
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::Emphasis) => {
                italic_stack.push(abs_start);
            }
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Emphasis) => {
                if let Some(last_link) = link_stack.last_mut() {
                    last_link.2 = last_link.2.max(abs_end);
                }
                if let Some(open_start) = italic_stack.pop() {
                    let delim_len = 1.min((abs_end.saturating_sub(open_start)) / 2);
                    if delim_len > 0 {
                        let open_d = open_start..(open_start + delim_len);
                        let close_d = (abs_end - delim_len)..abs_end;
                        let inner = open_d.end..close_d.start;
                        raw_hidden.push(open_d.clone());
                        raw_hidden.push(close_d.clone());
                        inline_pairs.push(InlinePair::new(
                            InlineTagKind::Italic,
                            open_d,
                            inner,
                            close_d,
                        ));
                    }
                }
            }
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::Strikethrough) => {
                strike_stack.push(abs_start);
            }
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Strikethrough) => {
                if let Some(last_link) = link_stack.last_mut() {
                    last_link.2 = last_link.2.max(abs_end);
                }
                if let Some(open_start) = strike_stack.pop() {
                    let slice_len = abs_end.saturating_sub(open_start);
                    let delim_len = if input[range.start..range.end].starts_with("~~") {
                        2.min(slice_len / 2)
                    } else {
                        1.min(slice_len / 2)
                    };
                    if delim_len > 0 {
                        let open_d = open_start..(open_start + delim_len);
                        let close_d = (abs_end - delim_len)..abs_end;
                        let inner = open_d.end..close_d.start;
                        raw_hidden.push(open_d.clone());
                        raw_hidden.push(close_d.clone());
                        inline_pairs.push(InlinePair::new(
                            InlineTagKind::Strike,
                            open_d,
                            inner,
                            close_d,
                        ));
                    }
                }
            }
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::Link { dest_url, .. }) => {
                link_stack.push((abs_start, Arc::from(dest_url.as_ref()), abs_start + 1));
            }
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Link) => {
                if let Some((open_start, _url, inner_end)) = link_stack.pop() {
                    let open_d = open_start..(open_start + 1).min(abs_end);
                    let close_start = inner_end.clamp(open_d.end, abs_end);
                    let close_d = close_start..abs_end;
                    let inner = open_d.end..close_d.start;
                    raw_hidden.push(open_d.clone());
                    raw_hidden.push(close_d.clone());
                    inline_pairs.push(InlinePair::new(InlineTagKind::Link, open_d, inner, close_d));
                }
            }
            pulldown_cmark::Event::InlineHtml(h) | pulldown_cmark::Event::Html(h) => {
                blocked_ranges.push(abs_start..abs_end);
                if let Some(last_link) = link_stack.last_mut() {
                    last_link.2 = last_link.2.max(abs_end);
                }
                if h.eq_ignore_ascii_case("<u>") {
                    underline_stack.push(abs_start);
                } else if h.eq_ignore_ascii_case("</u>") {
                    if let Some(open_start) = underline_stack.pop() {
                        let open_d = open_start..(open_start + 3).min(abs_start);
                        let close_d = abs_start..abs_end;
                        let inner = open_d.end..close_d.start;
                        raw_hidden.push(open_d.clone());
                        raw_hidden.push(close_d.clone());
                        inline_pairs.push(InlinePair::new(
                            InlineTagKind::Underline,
                            open_d,
                            inner,
                            close_d,
                        ));
                    }
                }
            }
            pulldown_cmark::Event::Code(_c) => {
                if let Some(last_link) = link_stack.last_mut() {
                    last_link.2 = last_link.2.max(abs_end);
                }
                let raw_slice = &input[range.start..range.end];
                let ticks = raw_slice.bytes().take_while(|&b| b == b'`').count();
                let open_d = abs_start..(abs_start + ticks).min(abs_end);
                let close_d = abs_end.saturating_sub(ticks).max(open_d.end)..abs_end;
                let inner = open_d.end..close_d.start;
                raw_hidden.push(open_d.clone());
                raw_hidden.push(close_d.clone());
                inline_pairs.push(InlinePair::new(
                    InlineTagKind::Code,
                    open_d,
                    inner.clone(),
                    close_d,
                ));
                if inner.end > inner.start {
                    styled_segments.push(RawStyledSegment {
                        source: inner,
                        bold: !bold_stack.is_empty(),
                        italic: !italic_stack.is_empty(),
                        underline: !underline_stack.is_empty(),
                        strike: !strike_stack.is_empty(),
                        code: true,
                        link_url: link_stack.last().map(|(_, u, _)| u.clone()),
                    });
                }
            }
            pulldown_cmark::Event::Text(_t) => {
                if let Some(last_link) = link_stack.last_mut() {
                    last_link.2 = last_link.2.max(abs_end);
                }
                if abs_end > abs_start {
                    styled_segments.push(RawStyledSegment {
                        source: abs_start..abs_end,
                        bold: !bold_stack.is_empty(),
                        italic: !italic_stack.is_empty(),
                        underline: !underline_stack.is_empty(),
                        strike: !strike_stack.is_empty(),
                        code: false,
                        link_url: link_stack.last().map(|(_, u, _)| u.clone()),
                    });
                }
            }
            pulldown_cmark::Event::SoftBreak | pulldown_cmark::Event::HardBreak => {
                if let Some(last_link) = link_stack.last_mut() {
                    last_link.2 = last_link.2.max(abs_end);
                }
            }
            _ => {}
        }
    }

    if let Some(&first_unclosed) = underline_stack.first() {
        for seg in &mut styled_segments {
            if seg.source.start >= first_unclosed {
                seg.underline = false;
            }
        }
    }

    normalize_shared_run_pairs(&mut inline_pairs);
    synthesize_flanking_pairs(
        input,
        base,
        &blocked_ranges,
        &mut raw_hidden,
        &mut inline_pairs,
        &mut styled_segments,
    );

    // Backslash escapes: the parser consumes `\X` (X an ASCII punctuation
    // character) and shows only `X`, so the backslash is hidden in the
    // WYSIWYG display with the offset map carrying the caret across it.
    // Backslashes inside code spans stay visible, except `\|` inside GFM table
    // cells which unescapes to `|`.
    let bytes = input.as_bytes();
    if bytes.contains(&b'\\') {
        let code_inner = merge_ranges(
            inline_pairs
                .iter()
                .filter(|p| p.kind == InlineTagKind::Code)
                .map(|p| p.inner.clone())
                .collect(),
        );
        // Absolute offsets of hidden backslashes, ascending.
        let mut escapes: Vec<usize> = Vec::new();
        let mut i = 0usize;
        while i < bytes.len() {
            if bytes[i] == b'\\' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_punctuation() {
                let abs = base + i;
                let in_code = index_in_ranges(&code_inner, abs);
                if !in_code || (in_table_cell && bytes[i + 1] == b'|') {
                    raw_hidden.push(abs..abs + 1);
                    escapes.push(abs);
                }
                i += 2;
            } else if allow_hard_break
                && !in_table_cell
                && bytes[i] == b'\\'
                && i + 1 == bytes.len()
            {
                let abs = base + i;
                if !index_in_ranges(&code_inner, abs) {
                    raw_hidden.push(abs..abs + 1);
                    escapes.push(abs);
                }
                i += 1;
            } else {
                i += 1;
            }
        }
        if !escapes.is_empty() {
            // The text segment covering an escape keeps the escaped character
            // only; the backslash is cut out of it.
            let mut rebuilt: Vec<RawStyledSegment> = Vec::with_capacity(styled_segments.len());
            for s in styled_segments.drain(..) {
                let first = escapes.partition_point(|&e| e < s.source.start);
                let inside = &escapes[first..];
                let count = inside.partition_point(|&e| e < s.source.end);
                if count == 0 {
                    rebuilt.push(s);
                    continue;
                }
                let mut cursor = s.source.start;
                for &e in &inside[..count] {
                    if e > cursor {
                        rebuilt.push(RawStyledSegment {
                            source: cursor..e,
                            ..s.clone()
                        });
                    }
                    cursor = e + 1;
                }
                if s.source.end > cursor {
                    rebuilt.push(RawStyledSegment {
                        source: cursor..s.source.end,
                        ..s
                    });
                }
            }
            styled_segments = rebuilt;
        }
    }

    finalize_pair_groups(&mut inline_pairs);
    if styled_segments.iter().all(RawStyledSegment::is_plain) {
        styled_segments.clear();
        styled_segments.shrink_to_fit();
    }
    (raw_hidden, inline_pairs, styled_segments)
}

/// Up to this many pairs on a line, pair lookups scan the list directly:
/// for a handful of pairs that beats building an index.
const SMALL_PAIRS: usize = 16;

/// Fill `group_open_start` / `group_close_end` so stacked markers that share
/// an `inner` range (`***`, `~~**`, `<u>**`) are O(1) to look up later.
fn finalize_pair_groups(pairs: &mut [InlinePair]) {
    if pairs.len() <= SMALL_PAIRS {
        for i in 0..pairs.len() {
            let (mut open, mut close) = (pairs[i].open_delim.start, pairs[i].close_delim.end);
            if pairs[i].kind != InlineTagKind::Link {
                for p in pairs.iter() {
                    if p.kind != InlineTagKind::Link && p.inner == pairs[i].inner {
                        open = open.min(p.open_delim.start);
                        close = close.max(p.close_delim.end);
                    }
                }
            }
            pairs[i].group_open_start = open;
            pairs[i].group_close_end = close;
        }
        return;
    }
    let mut groups: std::collections::HashMap<(usize, usize), (usize, usize)> =
        std::collections::HashMap::with_capacity(pairs.len());
    for p in pairs.iter().filter(|p| p.kind != InlineTagKind::Link) {
        let group = groups
            .entry((p.inner.start, p.inner.end))
            .or_insert((p.open_delim.start, p.close_delim.end));
        group.0 = group.0.min(p.open_delim.start);
        group.1 = group.1.max(p.close_delim.end);
    }
    for p in pairs.iter_mut() {
        (p.group_open_start, p.group_close_end) = if p.kind == InlineTagKind::Link {
            (p.open_delim.start, p.close_delim.end)
        } else {
            groups[&(p.inner.start, p.inner.end)]
        };
    }
}

/// True when the delimiter byte at `pos` is backslash-escaped and therefore
/// not part of any delimiter run.
fn is_escaped_delim(input: &str, pos: usize) -> bool {
    let bytes = input.as_bytes();
    let mut backslashes = 0usize;
    let mut p = pos;
    while p > 0 && bytes[p - 1] == b'\\' {
        backslashes += 1;
        p -= 1;
    }
    backslashes % 2 == 1
}

/// Fast zero-alloc check whether `run` is already covered by `extra_blocked`,
/// a `Code` span's inner range, or a parsed pair's `open_delim`/`close_delim`.
fn delim_covered_fast(
    run: &std::ops::Range<usize>,
    inline_pairs: &[InlinePair],
    extra_blocked: &[std::ops::Range<usize>],
) -> bool {
    if extra_blocked
        .iter()
        .any(|b| b.start < run.end && b.end > run.start)
    {
        return true;
    }
    for p in inline_pairs {
        if p.kind == InlineTagKind::Code && p.inner.start < run.end && p.inner.end > run.start {
            return true;
        }
        if (p.open_delim.start <= run.start && run.end <= p.open_delim.end)
            || (p.close_delim.start <= run.start && run.end <= p.close_delim.end)
        {
            return true;
        }
    }
    false
}

/// Delimiter runs that pulldown's emphasis matching rejected can still be
/// what the user typed. Two rules reject valid-looking pairs: the flanking
/// punctuation conditions (a closer after punctuation/symbol before a
/// letter, or the mirrored opener) and the "multiple of 3" odd-match rule
/// for runs that can both open and close. When runs were left unconsumed
/// and a relaxed pairing (whitespace flanking only, no odd-match rule)
/// matches them, synthesize the pair for the display: the document text
/// stays exactly as typed, the markers are hidden, and the styled segments
/// carry the intended formatting.
fn synthesize_flanking_pairs(
    input: &str,
    base: usize,
    extra_blocked: &[std::ops::Range<usize>],
    raw_hidden: &mut Vec<std::ops::Range<usize>>,
    inline_pairs: &mut Vec<InlinePair>,
    styled_segments: &mut Vec<RawStyledSegment>,
) {
    let bytes = input.as_bytes();
    // Most lines carry no delimiter at all: one SIMD scan skips the rest.
    let Some(first_delim) = memchr::memchr3(b'*', b'~', b'`', bytes) else {
        return;
    };
    // Zero-alloc pre-pass: if fewer than 2 delimiter runs are uncovered by
    // pulldown-cmark, synthesis cannot form a pair.
    let mut uncovered = 0usize;
    let mut scan = first_delim;
    while scan < bytes.len() {
        let b = bytes[scan];
        if (b == b'*' || b == b'~' || b == b'`') && !is_escaped_delim(input, scan) {
            let start = scan;
            while scan < bytes.len() && bytes[scan] == b && !is_escaped_delim(input, scan) {
                scan += 1;
            }
            let abs_run = (base + start)..(base + scan);
            if !delim_covered_fast(&abs_run, inline_pairs, extra_blocked) {
                uncovered += 1;
                if uncovered >= 2 {
                    break;
                }
            }
        } else {
            let next = memchr::memchr3(b'*', b'~', b'`', &bytes[scan + 1..]);
            match next {
                Some(rel) => scan = scan + 1 + rel,
                None => break,
            }
        }
    }
    if uncovered < 2 {
        return;
    }

    // Runs of unescaped emphasis delimiters, in source order.
    let mut runs: Vec<(usize, usize, u8)> = Vec::new();
    let mut i = first_delim;
    while i < bytes.len() {
        let b = bytes[i];
        if (b == b'*' || b == b'~' || b == b'`') && !is_escaped_delim(input, i) {
            let start = i;
            while i < bytes.len() && bytes[i] == b && !is_escaped_delim(input, i) {
                i += 1;
            }
            runs.push((start, i, b));
        } else if let Some(rel) = memchr::memchr3(b'*', b'~', b'`', &bytes[i + 1..]) {
            i = i + 1 + rel;
        } else {
            break;
        }
    }
    if runs.len() < 2 {
        return;
    }
    // A run is out of play when it overlaps a code span, HTML tag/comment,
    // inline image, or was consumed as the opener or closer of a parsed pair.
    let has_code_pair = inline_pairs.iter().any(|p| p.kind == InlineTagKind::Code);
    let blocked_spans = if !has_code_pair && extra_blocked.is_empty() {
        Vec::new()
    } else {
        let mut blocked_list: Vec<std::ops::Range<usize>> = inline_pairs
            .iter()
            .filter(|p| p.kind == InlineTagKind::Code)
            .map(|p| p.inner.clone())
            .collect();
        blocked_list.extend(extra_blocked.iter().cloned());
        merge_ranges(blocked_list)
    };
    let overlaps_blocked = |run: &std::ops::Range<usize>| {
        if blocked_spans.is_empty() {
            return false;
        }
        let after = blocked_spans.partition_point(|b| b.start < run.end);
        after > 0 && blocked_spans[after - 1].end > run.start
    };
    let delims = merge_ranges(
        inline_pairs
            .iter()
            .flat_map(|p| [p.open_delim.clone(), p.close_delim.clone()])
            .collect(),
    );
    let consumed = |run: std::ops::Range<usize>| {
        let after = delims.partition_point(|d| d.start <= run.start);
        after > 0 && run.end <= delims[after - 1].end
    };
    let mut blocked: Vec<bool> = runs
        .iter()
        .map(|&(s, e, _)| {
            let run = base + s..base + e;
            overlaps_blocked(&run) || consumed(run)
        })
        .collect();
    if blocked.iter().filter(|&&b| !b).count() < 2 {
        return;
    }
    // Closer candidates by delimiter: emphasis keyed by exact run length,
    // backticks (any length) under length 0. Relaxed flanking: only the
    // whitespace conditions apply, so a closer cannot follow whitespace. The
    // punctuation conditions and the odd-match rule are exactly what
    // rejected these runs in the first place.
    let closer_slot = |ch: u8, len: usize| match (ch, len) {
        (b'*', 1) => Some(0),
        (b'*', 2) => Some(1),
        (b'~', 2) => Some(2),
        (b'`', _) => Some(3),
        _ => None,
    };
    let mut closers: [Vec<usize>; 4] = Default::default();
    for (k, &(s, e, ch)) in runs.iter().enumerate() {
        if input[..s]
            .chars()
            .next_back()
            .is_some_and(char::is_whitespace)
        {
            continue;
        }
        if let Some(slot) = closer_slot(ch, e - s) {
            closers[slot].push(k);
        }
    }
    let mut used = vec![false; runs.len()];
    for open_idx in 0..runs.len() {
        if used[open_idx] || blocked[open_idx] {
            continue;
        }
        let (o_start, o_end, ch) = runs[open_idx];
        let o_len = o_end - o_start;
        // An opener must be followed by something (not whitespace).
        if input[o_end..]
            .chars()
            .next()
            .is_none_or(char::is_whitespace)
        {
            continue;
        }
        // Equal-length emphasis runs (`~~` only for strike, at most two);
        // code fences pair with a longer later run (the content's own
        // trailing backticks merged with the closer), taking its tail as the
        // closer. Single-backtick openers are not re-paired: unequal
        // single/double backticks in ordinary text stay literal.
        if ch == b'`' && o_len < 2 {
            continue;
        }
        let Some(slot) = closer_slot(ch, o_len) else {
            continue;
        };
        let candidates = &closers[slot];
        let from = candidates.partition_point(|&k| k <= open_idx);
        let found = candidates[from..].iter().copied().find(|&k| {
            let (c_start, c_end, _) = runs[k];
            if used[k] || blocked[k] || c_start == o_end || (ch == b'`' && c_end - c_start <= o_len)
            {
                return false;
            }
            let closer_start = if ch == b'`' && c_end - c_start > o_len {
                c_end - o_len
            } else {
                c_start
            };
            let cand_start = base + o_start;
            let cand_end = base + c_end;
            let cand_inner_start = base + o_end;
            let cand_inner_end = base + closer_start;
            !inline_pairs.iter().any(|p| {
                let disjoint = cand_end <= p.open_delim.start || cand_start >= p.close_delim.end;
                let inside_p = cand_start >= p.inner.start && cand_end <= p.inner.end;
                let encloses_p =
                    p.open_delim.start >= cand_inner_start && p.close_delim.end <= cand_inner_end;
                !(disjoint || inside_p || encloses_p)
            })
        });
        let Some(close_idx) = found else {
            continue;
        };
        let (c_start, c_end, _) = runs[close_idx];
        let closer_start = if ch == b'`' && c_end - c_start > o_end - o_start {
            c_end - (o_end - o_start)
        } else {
            c_start
        };
        let len = if ch == b'`' {
            o_end - o_start
        } else {
            c_end - c_start
        };
        let kind = if ch == b'`' {
            InlineTagKind::Code
        } else if ch == b'~' {
            InlineTagKind::Strike
        } else if len == 2 {
            InlineTagKind::Bold
        } else {
            InlineTagKind::Italic
        };
        let inner = if ch == b'`' {
            base + o_end..base + closer_start
        } else {
            base + o_end..base + c_start
        };
        used[open_idx] = true;
        used[close_idx] = true;
        raw_hidden.push(base + o_start..base + o_end);
        raw_hidden.push(base + closer_start..base + c_end);
        inline_pairs.push(InlinePair::new(
            kind,
            base + o_start..base + o_end,
            inner.clone(),
            base + closer_start..base + c_end,
        ));
        let or_kind = |s: &mut RawStyledSegment| match kind {
            InlineTagKind::Bold => s.bold = true,
            InlineTagKind::Italic => s.italic = true,
            InlineTagKind::Underline => s.underline = true,
            InlineTagKind::Strike => s.strike = true,
            InlineTagKind::Code => s.code = true,
            InlineTagKind::Link => {}
        };
        // Keep segments already inside the new span (nested bold, underline,
        // links) and OR the synthesized style onto each. Only fill a gap
        // when the inner range had no segments at all.
        let mut rebuilt: Vec<RawStyledSegment> = Vec::new();
        let mut inner_covered = false;
        for s in styled_segments.drain(..) {
            if s.source.start < inner.end && s.source.end > inner.start {
                if s.source.start < inner.start {
                    rebuilt.push(RawStyledSegment {
                        source: s.source.start..inner.start,
                        ..s.clone()
                    });
                }
                let mid_start = s.source.start.max(inner.start);
                let mid_end = s.source.end.min(inner.end);
                if mid_end > mid_start {
                    let mut mid = s.clone();
                    mid.source = mid_start..mid_end;
                    or_kind(&mut mid);
                    rebuilt.push(mid);
                    inner_covered = true;
                }
                if s.source.end > inner.end {
                    rebuilt.push(RawStyledSegment {
                        source: inner.end..s.source.end,
                        ..s
                    });
                }
            } else {
                rebuilt.push(s);
            }
        }
        // Marker runs are hidden: cut them out of every segment before
        // merging, so leftover closer ticks are not glued onto the text
        // that follows (` ``a```b`c` ` must keep `b` separate from `` ` ``).
        for run in [
            base + o_start..base + o_end,
            base + closer_start..base + c_end,
        ] {
            let mut next = Vec::new();
            for s in std::mem::take(&mut rebuilt) {
                if s.source.start < run.end && s.source.end > run.start {
                    if s.source.start < run.start {
                        next.push(RawStyledSegment {
                            source: s.source.start..run.start,
                            ..s.clone()
                        });
                    }
                    if s.source.end > run.end {
                        next.push(RawStyledSegment {
                            source: run.end..s.source.end,
                            ..s
                        });
                    }
                } else {
                    next.push(s);
                }
            }
            rebuilt = next;
        }
        rebuilt.sort_by_key(|s| s.source.start);
        styled_segments.reserve(rebuilt.len());
        for s in rebuilt {
            if let Some(last) = styled_segments.last_mut() {
                if last.source.end == s.source.start
                    && last.bold == s.bold
                    && last.italic == s.italic
                    && last.underline == s.underline
                    && last.strike == s.strike
                    && last.code == s.code
                    && last.link_url == s.link_url
                {
                    last.source.end = s.source.end;
                    continue;
                }
            }
            styled_segments.push(s);
        }
        if !inner_covered {
            let mut seg = RawStyledSegment {
                source: inner.clone(),
                bold: false,
                italic: false,
                underline: false,
                strike: false,
                code: false,
                link_url: None,
            };
            or_kind(&mut seg);
            let pos = styled_segments
                .iter()
                .position(|s| s.source.start > inner.start)
                .unwrap_or(styled_segments.len());
            styled_segments.insert(pos, seg);
        }
        if kind == InlineTagKind::Code {
            // Runs inside the new code span no longer open or close.
            let first = runs.partition_point(|r| base + r.0 < inner.start);
            let last = runs.partition_point(|r| base + r.0 < inner.end);
            for flag in &mut blocked[first..last] {
                *flag = true;
            }
        }
    }
}

/// pulldown reports markers that share one delimiter run (`***x***`,
/// `**~~x~~**`) with overlapping event ranges, so the outer pair's `inner`
/// claims the inner pair's markers as its content. When a pair's inner is
/// exactly another pair wrapped in its delimiters, adopt the nested inner as
/// the real content range, so caret mapping, exits, and splits see the actual
/// text. Display and hidden ranges are unaffected.
fn normalize_shared_run_pairs(inline_pairs: &mut [InlinePair]) {
    // Pairs by their outer extent, in index order; delims never change here.
    // Few pairs (the usual line) are scanned directly instead.
    let mut by_outer: std::collections::HashMap<(usize, usize), Vec<usize>> =
        std::collections::HashMap::new();
    if inline_pairs.len() > SMALL_PAIRS {
        for (j, p) in inline_pairs.iter().enumerate() {
            by_outer
                .entry((p.open_delim.start, p.close_delim.end))
                .or_default()
                .push(j);
        }
    }
    loop {
        let mut changed = false;
        for i in 0..inline_pairs.len() {
            let inner = inline_pairs[i].inner.clone();
            let fits = |j: usize| {
                j != i
                    && inline_pairs[j].open_delim.start == inner.start
                    && inline_pairs[j].close_delim.end == inner.end
                    && inline_pairs[j].inner.start > inner.start
                    && inline_pairs[j].inner.end < inner.end
            };
            let nested = if inline_pairs.len() > SMALL_PAIRS {
                by_outer
                    .get(&(inner.start, inner.end))
                    .and_then(|candidates| candidates.iter().copied().find(|&j| fits(j)))
            } else {
                (0..inline_pairs.len()).find(|&j| fits(j))
            };
            if let Some(j) = nested {
                inline_pairs[i].inner = inline_pairs[j].inner.clone();
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

pub fn parse_document(markdown: &str) -> ParsedDocument {
    let mut raw_lines = LineSource::all(markdown);
    let mut lines = Vec::with_capacity(raw_lines.len_hint());
    let mut state = ParseState::default();
    let mut checkpoints = Vec::with_capacity(raw_lines.len_hint() / CHECKPOINT_EVERY + 1);
    parse_lines(&mut raw_lines, 0, &mut state, &mut lines, |i, _, state| {
        if i % CHECKPOINT_EVERY == 0 {
            checkpoints.push((i, state.clone()));
        }
        Step::Parse
    });
    let code_blocks = code_blocks_of(&lines, markdown.len());
    let table_line_count = lines
        .iter()
        .filter(|l| matches!(l.kind, LineKind::TableRow { .. }))
        .count();
    ParsedDocument {
        lines,
        code_blocks,
        meta: ParseMeta {
            checkpoints,
            generation: next_generation(),
            table_line_count,
            last_splice: None,
            last_byte_delta: 0,
        },
    }
}

/// Shared empty parse for tabs that do not use Markdown structure.
pub fn empty_parsed_document() -> Arc<ParsedDocument> {
    static EMPTY: std::sync::OnceLock<Arc<ParsedDocument>> = std::sync::OnceLock::new();
    EMPTY
        .get_or_init(|| Arc::new(ParsedDocument::default()))
        .clone()
}

/// Where an edit sits and how far it moves what follows it: old positions at
/// or past `old_end` move by `delta` bytes, old lines at or past `old_line`
/// by `line_delta` lines.
#[derive(Clone, Copy)]
struct Shift {
    old_end: usize,
    delta: isize,
    old_line: usize,
    line_delta: isize,
}

impl Shift {
    fn pos(self, p: usize) -> usize {
        if p >= self.old_end {
            (p as isize + self.delta) as usize
        } else {
            p
        }
    }

    fn range(self, r: &mut std::ops::Range<usize>) {
        r.start = self.pos(r.start);
        r.end = self.pos(r.end);
    }

    fn line(self, l: usize) -> usize {
        if l >= self.old_line {
            (l as isize + self.line_delta) as usize
        } else {
            l
        }
    }

    fn state(self, state: &ParseState) -> ParseState {
        let mut state = state.clone();
        self.state_in_place(&mut state);
        state
    }

    fn state_in_place(self, state: &mut ParseState) {
        if let Some(fence) = &mut state.open_fence {
            self.range(&mut fence.open_range);
        }
        if let Some(table) = &mut state.table {
            table.first = self.line(table.first);
            table.last = self.line(table.last);
            table.delimiter = self.line(table.delimiter);
            self.range(&mut table.source);
        }
    }

    /// Move every line of `lines` (all after the edit). A long tail is split
    /// across threads: the lines are independent, and each one's ranges and
    /// shared maps are scattered on the heap, so the walk is memory-bound.
    fn parsed_lines(self, lines: &mut [ParsedLine]) {
        const PER_THREAD: usize = 8192;
        // Same length and same line count: nothing moves.
        if self.delta == 0 && self.line_delta == 0 {
            return;
        }
        let threads = std::thread::available_parallelism()
            .map_or(1, |n| n.get())
            .min(lines.len() / PER_THREAD);
        if threads < 2 {
            lines.iter_mut().for_each(|line| self.parsed_line(line));
            return;
        }
        let chunk = lines.len().div_ceil(threads);
        std::thread::scope(|scope| {
            for part in lines.chunks_mut(chunk) {
                scope.spawn(move || part.iter_mut().for_each(|line| self.parsed_line(line)));
            }
        });
    }

    /// Move a line that comes after the edit. Its text is unchanged, so
    /// every position in it moves by the same amount. Because hidden ranges,
    /// inline pairs, styled segments, and link hits are stored relative to
    /// `line.source.start`, none of them need to be touched here.
    fn parsed_line(self, line: &mut ParsedLine) {
        line.line_index = self.line(line.line_index as usize) as u32;
        if self.delta == 0 {
            return;
        }
        self.range(&mut line.source);
        if let LineKind::TableRow { body, .. } = &mut line.kind {
            for r in body.cells.iter_mut().chain(body.raw_cells.iter_mut()) {
                self.range(r);
            }
            self.range(&mut body.table_source);
        }
    }
}

impl ParsedDocument {
    /// Bring this parse up to date after an edit that replaced the bytes
    /// `old` of the parsed text with `new_len` bytes, giving `text`.
    ///
    /// Parsing resumes from the last checkpoint before the edit (one line
    /// early, since a table header depends on the line under it, and outside
    /// any table, whose rows were found by looking ahead) and stops at the
    /// first old checkpoint after the edit where the parser state matches
    /// the old one moved by the edit. From there on the old lines are the
    /// right ones, moved in place. The result equals `parse_document(text)`.
    pub fn apply_edit(&mut self, text: &str, old: std::ops::Range<usize>, new_len: usize) {
        let checkpoints = &self.meta.checkpoints;
        if self.lines.is_empty() || checkpoints.first().is_none_or(|(l, _)| *l != 0) {
            *self = parse_document(text);
            return;
        }
        let delta = new_len as isize - (old.end - old.start) as isize;
        let new_edit_end = old.start + new_len;
        // Last line starting at or before the edit (an offset inside a CRLF
        // belongs to no line's range, so not `line_index_at`).
        let first = self
            .lines
            .partition_point(|l| l.source.start <= old.start)
            .saturating_sub(1);
        let target = first.saturating_sub(1);
        let below = checkpoints.partition_point(|(l, _)| *l <= target);
        let (restart, restart_state) = checkpoints[..below]
            .iter()
            .rev()
            .find(|(_, state)| state.table.is_none())
            .cloned()
            .unwrap_or_else(|| (0, ParseState::default()));
        let restart_byte = self.lines[restart].source.start;
        let mut raw = LineSource::from(text, restart_byte);

        let old_lines = &self.lines;
        let mut new_lines = Vec::new();
        let mut new_checkpoints = Vec::new();
        let mut resync = None;
        let mut state = restart_state;
        parse_lines(
            &mut raw,
            restart,
            &mut state,
            &mut new_lines,
            |i, start, now| {
                if start >= new_edit_end && i > restart {
                    let old_start = (start as isize - delta) as usize;
                    let k = old_lines.partition_point(|l| l.source.start < old_start);
                    let at_line_start = old_lines
                        .get(k)
                        .is_some_and(|l| l.source.start == old_start);
                    if at_line_start {
                        if let Ok(ix) = checkpoints.binary_search_by_key(&k, |(l, _)| *l) {
                            let shift = Shift {
                                old_end: old.end,
                                delta,
                                old_line: k,
                                line_delta: i as isize - k as isize,
                            };
                            if shift.state(&checkpoints[ix].1) == *now {
                                resync = Some((i, k, ix, shift));
                                return Step::Stop;
                            }
                        }
                    }
                }
                if (i - restart) % CHECKPOINT_EVERY == 0 {
                    new_checkpoints.push((i, now.clone()));
                }
                Step::Parse
            },
        );

        let inserted = new_lines.len();
        let old_generation = self.meta.generation;
        // Checkpoints before the restart stay as they are: split them off
        // instead of cloning every parser state up to the edit.
        let mut checkpoints = std::mem::take(&mut self.meta.checkpoints);
        let cut = checkpoints.partition_point(|(l, _)| *l < restart);
        let mut later_checkpoints = checkpoints.split_off(cut);
        checkpoints.extend(new_checkpoints);
        let is_fence = |line: &ParsedLine| matches!(line.kind, LineKind::CodeFence { .. });
        let is_table = |line: &ParsedLine| matches!(line.kind, LineKind::TableRow { .. });
        let new_fences = new_lines.iter().any(is_fence);
        let new_tables = new_lines.iter().filter(|l| is_table(l)).count();
        let (removed, old_fences, old_tables, shift) = match resync {
            Some((_, k, ix, shift)) => {
                let old_fences = self.lines[restart..k].iter().any(is_fence);
                let old_tables = self.lines[restart..k]
                    .iter()
                    .filter(|l| is_table(l))
                    .count();
                self.lines.splice(restart..k, new_lines);
                shift.parsed_lines(&mut self.lines[restart + inserted..]);
                let mut kept = later_checkpoints.split_off(ix - cut);
                for (line, state) in &mut kept {
                    *line = shift.line(*line);
                    shift.state_in_place(state);
                }
                checkpoints.extend(kept);
                (k - restart, old_fences, old_tables, shift)
            }
            None => {
                let removed = self.lines.len() - restart;
                let old_fences = self.lines[restart..].iter().any(is_fence);
                let old_tables = self.lines[restart..].iter().filter(|l| is_table(l)).count();
                self.lines.truncate(restart);
                self.lines.extend(new_lines);
                let shift = Shift {
                    old_end: old.end,
                    delta,
                    old_line: 0,
                    line_delta: 0,
                };
                (removed, old_fences, old_tables, shift)
            }
        };
        if old_fences || new_fences {
            self.code_blocks = code_blocks_of(&self.lines, text.len());
        } else {
            // No fence line was replaced, so the blocks are the same ones and
            // only their byte positions move (an unclosed block ends at the
            // end of the text, which moves too).
            for (open, close) in &mut self.code_blocks {
                shift.range(open);
                shift.range(close);
            }
        }
        let table_line_count = self.meta.table_line_count - old_tables + new_tables;
        self.meta = ParseMeta {
            checkpoints,
            generation: next_generation(),
            table_line_count,
            last_splice: Some((old_generation, restart, removed, inserted)),
            last_byte_delta: delta,
        };
    }
}

/// Every source line of `markdown`, plus the empty last line after a final
/// line ending.
fn all_source_lines(markdown: &str) -> Vec<SourceLine<'_>> {
    if markdown.is_empty() {
        vec![SourceLine {
            start: 0,
            end: 0,
            text: "",
        }]
    } else {
        let mut v = source_lines(markdown);
        if markdown.ends_with('\n') || markdown.ends_with('\r') {
            let end = markdown.len();
            v.push(SourceLine {
                start: end,
                end,
                text: "",
            });
        }
        v
    }
}

/// Lines between checkpoints of the parser state.
pub(crate) const CHECKPOINT_EVERY: usize = 16;

/// What `parse_lines` does with the next line.
pub(crate) enum Step {
    Parse,
    Stop,
}

/// Everything the parse of one line depends on besides the line itself and
/// the lines after it: lets a parse resume in the middle of a document.
#[derive(Clone, Debug, PartialEq)]
pub struct ParseState {
    tracker: InListOrQuoteTracker,
    open_fence: Option<OpenFence>,
    table: Option<DetectedTable>,
    list_walk: ListWalk,
    /// The previous line ends a block (or there is none): indented code may
    /// start here.
    prev_ends_block: bool,
    /// The last non-blank line is code, so an indented line continues it.
    last_content_is_code: bool,
}

impl Default for ParseState {
    fn default() -> Self {
        Self {
            tracker: InListOrQuoteTracker::default(),
            open_fence: None,
            table: None,
            list_walk: ListWalk::default(),
            prev_ends_block: true,
            last_content_is_code: false,
        }
    }
}

fn next_generation() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// Fenced code blocks `(opening line incl. ending, closing line incl.
/// ending)`; an unclosed fence runs to the end of the document.
fn code_blocks_of(
    lines: &[ParsedLine],
    text_len: usize,
) -> Vec<(std::ops::Range<usize>, std::ops::Range<usize>)> {
    let with_ending = |j: usize| -> std::ops::Range<usize> {
        let end = lines.get(j + 1).map_or(text_len, |next| next.source.start);
        lines[j].source.start..end
    };
    let mut blocks = Vec::new();
    let mut open = None;
    for (j, line) in lines.iter().enumerate() {
        if let LineKind::CodeFence { opening, .. } = line.kind {
            if opening {
                open = Some(with_ending(j));
            } else if let Some(open_range) = open.take() {
                blocks.push((open_range, with_ending(j)));
            }
        }
    }
    if let Some(open_range) = open {
        blocks.push((open_range, text_len..text_len));
    }
    blocks
}

/// Parse `raw_lines` (line `base` of the document first) onto `lines`.
/// `on_line` sees each line's index, start byte and the state before it, and
/// can stop the parse there; the return value is then that line's position
/// in `raw_lines`.
// The line loop moved here from parse_document, with its allow.
#[allow(clippy::single_range_in_vec_init)]
/// Parse one or more leading blockquote `>` markers on `line`.
/// Returns `(depth, prefix_byte_len)` if the outermost `>` has at most
/// `max_first_indent` leading columns. Subsequent nested `>` markers allow up
/// to 3 columns of leading indent each.
pub fn strip_blockquote_markers(line: &str, max_first_indent: usize) -> Option<(u8, usize)> {
    let (indent_bytes, indent_cols) = split_indent(line);
    if indent_cols > max_first_indent {
        return None;
    }
    let after = &line[indent_bytes..];
    if !after.starts_with('>') {
        return None;
    }
    let space = usize::from(matches!(after.as_bytes().get(1), Some(b' ' | b'\t')));
    let mut pos = indent_bytes + 1 + space;
    let mut depth = 1u8;
    loop {
        let rest = &line[pos..];
        let (sub_bytes, sub_cols) = split_indent(rest);
        if sub_cols > 3 {
            break;
        }
        let sub_after = &rest[sub_bytes..];
        if !sub_after.starts_with('>') {
            break;
        }
        let sub_space = usize::from(matches!(sub_after.as_bytes().get(1), Some(b' ' | b'\t')));
        pos += sub_bytes + 1 + sub_space;
        depth = depth.saturating_add(1);
    }
    Some((depth, pos))
}

/// Byte length of the outermost `>` marker (with up to 3 columns of indent and
/// optional trailing space/tab) on `line`.
pub fn strip_first_blockquote_marker_end(line: &str) -> Option<usize> {
    let (indent_bytes, indent_cols) = split_indent(line);
    if indent_cols > 3 {
        return None;
    }
    let after = &line[indent_bytes..];
    if !after.starts_with('>') {
        return None;
    }
    let space = usize::from(matches!(after.as_bytes().get(1), Some(b' ' | b'\t')));
    Some(indent_bytes + 1 + space)
}

/// Given a blockquote prefix slice (such as `"> > "` or `">   > "`), return
/// the byte offset of the start of its innermost `>` marker (including any
/// leading spaces/tabs before that `>`).
pub fn strip_last_blockquote_marker_start(prefix: &str) -> Option<usize> {
    let gt = prefix.rfind('>')?;
    let mut start = gt;
    while start > 0 && matches!(prefix.as_bytes()[start - 1], b' ' | b'\t') {
        start -= 1;
    }
    // If the whitespace right before `gt` belongs to the previous `>`'s single
    // separator space (e.g. `"> >"` where `gt == 2` and byte 1 is `' '`), keep
    // that separator space on the outer quote!
    if start > 0 && prefix.as_bytes()[start - 1] == b'>' && gt - start >= 1 {
        start += 1;
    }
    Some(start)
}

fn parse_lines(
    raw_lines: &mut LineSource<'_>,
    base: usize,
    state: &mut ParseState,
    lines: &mut Vec<ParsedLine>,
    mut on_line: impl FnMut(usize, usize, &ParseState) -> Step,
) -> Option<usize> {
    let mut next_local = 0usize;
    let mut pending_header_cells: Option<Vec<std::ops::Range<usize>>> = None;
    while let Some(sline) = raw_lines.get(next_local) {
        let local = next_local;
        next_local += 1;
        let i = base + local;
        // A table that ended above this line is gone: the next table check
        // runs either way, and dropping it keeps states comparable.
        if state.table.as_ref().is_some_and(|t| i > t.last) {
            state.table = None;
        }
        match on_line(i, sline.start, &*state) {
            Step::Parse => {}
            Step::Stop => return Some(local),
        }
        let ParseState {
            tracker: list_quote_tracker,
            open_fence,
            table,
            list_walk,
            prev_ends_block,
            last_content_is_code,
        } = &mut *state;
        macro_rules! push_line {
            ($line:expr) => {{
                let p = $line;
                if p.quote_depth > 0 {
                    list_quote_tracker.push(&LineKind::Blockquote);
                } else {
                    list_quote_tracker.push(&p.kind);
                }
                *prev_ends_block = matches!(
                    p.kind,
                    LineKind::Empty
                        | LineKind::Heading { .. }
                        | LineKind::HorizontalRule
                        | LineKind::CodeLine
                        | LineKind::CodeFence { opening: false, .. }
                );
                if !matches!(p.kind, LineKind::Empty) {
                    *last_content_is_code = matches!(p.kind, LineKind::CodeLine);
                }
                lines.push(p);
            }};
        }
        let line = sline.text;
        let line_start = sline.start;
        let line_end = line_start + line.len();
        let line_source = line_start..line_end;
        let trimmed = line.trim();
        let (indent_bytes, indent_cols) = split_indent(line);
        let after_indent = &line[indent_bytes..];

        // Fences. Top-level backtick and tilde fences follow CommonMark. A
        // fence in a list item or quote keeps red's trimmed-marker rule.
        if open_fence.is_some() {
            list_walk.note_block(line);
            let closes = open_fence.as_ref().is_some_and(|fence| {
                if fence.strict {
                    is_closing_fence(line, fence.marker, fence.len, 3)
                } else if fence.marker == b'~' {
                    trimmed.starts_with("~~~")
                } else {
                    trimmed.starts_with("```")
                }
            });
            if closes {
                let _fence = open_fence.take().expect("fence");
                let parsed = fence_marker_line(
                    i,
                    line_source,
                    line,
                    LineKind::CodeFence {
                        language: None,
                        opening: false,
                    },
                );
                push_line!(parsed);
            } else {
                let strip = open_fence
                    .as_ref()
                    .filter(|fence| fence.strict)
                    .map(|fence| prefix_through_column(line, fence.indent))
                    .unwrap_or(0);
                let parsed = code_content_line(i, line_source, line, strip);
                push_line!(parsed);
            }
            continue;
        }
        let open_top =
            if list_quote_tracker.is_in_list_or_quote() || backtick_info_rejects_fence(line) {
                None
            } else {
                strict_fence_open(line)
            };
        if let Some((marker, len, indent, language)) = open_top {
            list_walk.note_block(line);
            *open_fence = Some(OpenFence {
                strict: true,
                marker,
                len,
                indent,
                open_range: sline.start..sline.end,
            });
            let parsed = fence_marker_line(
                i,
                line_source,
                line,
                LineKind::CodeFence {
                    language: language.map(|s| Box::new(s.into_boxed_str())),
                    opening: true,
                },
            );
            push_line!(parsed);
            continue;
        }
        // Indented code: four or more columns at document level. It cannot
        // interrupt a paragraph, so the line before must end a block (or be
        // indented code itself); after paragraph, list or quote text the line
        // is a lazy continuation. Inside a list item the indent belongs to
        // the item. The first four columns are hidden like a fence's indent.
        let after_block = *prev_ends_block;
        // `\t- foo` stays a list item, as `take_list` accepts it (red parity).
        let indented_code = after_block
            && !trimmed.is_empty()
            && leading_column(line) >= 4
            && list_walk.thematic_parent_col(line) == 0
            && !(single_tab_marker(line) && scan_list_marker(line).is_some());
        // Inside an indented code block (blank lines included, even after a
        // list or quote) a ```` line continues indented code instead of
        // falling through to the trimmed fence rules below.
        let continues_indented_code = indented_code && *last_content_is_code;
        if continues_indented_code {
            list_walk.note_block(line);
            let strip = prefix_through_column(line, 4);
            let parsed = code_content_line(i, line_source, line, strip);
            push_line!(parsed);
            continue;
        }

        if list_quote_tracker.is_in_list_or_quote() && trimmed.starts_with("```") {
            list_walk.note_block(line);
            let lang = trimmed
                .strip_prefix("```")
                .map(|info| info.trim())
                .filter(|info| !info.is_empty())
                .map(|info| Box::new(Box::<str>::from(info)));
            *open_fence = Some(OpenFence {
                strict: false,
                marker: b'`',
                len: 3,
                indent: 0,
                open_range: sline.start..sline.end,
            });
            let parsed = fence_marker_line(
                i,
                line_source,
                line,
                LineKind::CodeFence {
                    language: lang,
                    opening: true,
                },
            );
            push_line!(parsed);
            continue;
        }
        // A tilde fence in a list item or quote follows the same trimmed rule
        // as a backtick one there. At document level a tilde line indented
        // four columns is indented code, not a fence.
        if list_quote_tracker.is_in_list_or_quote() && trimmed.starts_with("~~~") {
            list_walk.note_block(line);
            let lang = Some(trimmed.trim_start_matches('~').trim())
                .filter(|info| !info.is_empty())
                .map(|info| Box::new(Box::<str>::from(info)));
            *open_fence = Some(OpenFence {
                strict: false,
                marker: b'~',
                len: 3,
                indent: 0,
                open_range: sline.start..sline.end,
            });
            let parsed = fence_marker_line(
                i,
                line_source,
                line,
                LineKind::CodeFence {
                    language: lang,
                    opening: true,
                },
            );
            push_line!(parsed);
            continue;
        }

        if trimmed.is_empty() {
            list_walk.note_blank();
            push_line!(ParsedLine {
                line_index: i as u32,
                source: line_source,
                content_end: line.len() as u32,
                prefix_len: 0,
                quote_depth: 0,
                kind: LineKind::Empty,
                hidden_ranges: Box::default(),
                inline: None,
            });
            continue;
        }

        if indented_code {
            list_walk.note_block(line);
            let strip = prefix_through_column(line, 4);
            let parsed = code_content_line(i, line_source, line, strip);
            push_line!(parsed);
            continue;
        }

        // Horizontal rule: exact `---`, `***`, or `___`, indent 0–3 past the
        // open list item (document level rejects four spaces and a tab).
        if exact_thematic_break(line, list_walk.thematic_parent_col(line)) {
            list_walk.note_block(line);
            let hidden = if line.is_empty() {
                Vec::new()
            } else {
                vec![LineRange::new(0, line.len() as u32)]
            };
            push_line!(ParsedLine {
                line_index: i as u32,
                source: line_source,
                content_end: line.len() as u32,
                prefix_len: line.len() as u32,
                quote_depth: 0,
                kind: LineKind::HorizontalRule,
                hidden_ranges: hidden.into(),
                inline: None,
            });
            continue;
        }

        // Table rows. Detection starts only on a line the fence walk did not
        // consume, and the block follows the delimiter row's column shape.
        if table.as_ref().is_none_or(|t| i > t.last) {
            *table = table_block(raw_lines, local).map(|block| {
                pending_header_cells = Some(block.header_cells);
                let byte_start = sline.start;
                let last_line = raw_lines.get(block.last).expect("table line");
                DetectedTable {
                    first: i,
                    last: base + block.last,
                    delimiter: base + block.delimiter,
                    source: byte_start..(last_line.start + last_line.text.len()),
                    alignments: block.alignments.into(),
                }
            });
        }
        if let Some(tbl) = table.as_ref().filter(|tbl| i >= tbl.first && i <= tbl.last) {
            list_walk.note_block(line);
            if i == tbl.delimiter {
                pending_header_cells = None;
                let hidden = if line.is_empty() {
                    Vec::new()
                } else {
                    vec![LineRange::new(0, line.len() as u32)]
                };
                push_line!(ParsedLine {
                    line_index: i as u32,
                    source: line_source,
                    content_end: line.len() as u32,
                    prefix_len: line.len() as u32,
                    quote_depth: 0,
                    kind: LineKind::TableDelimiter,
                    hidden_ranges: hidden.into(),
                    inline: None,
                });
                continue;
            } else {
                let is_header = i == tbl.first;
                let cols = tbl.alignments.len();
                let mut cells = Vec::new();
                let mut raw_cells = Vec::new();
                let mut row_hidden = Vec::new();
                let mut row_pairs = Vec::new();
                let mut row_segments = Vec::new();
                let cell_ranges = if is_header {
                    pending_header_cells
                        .take()
                        .unwrap_or_else(|| table_cell_ranges(line))
                } else {
                    table_cell_ranges(line)
                };
                for raw in cell_ranges.into_iter().take(cols) {
                    let raw_abs = (line_start + raw.start)..(line_start + raw.end);
                    let cell_slice = &line[raw.clone()];
                    let lead = usize::from(cell_slice.starts_with(' '));
                    let rem = &cell_slice[lead..];
                    let trail = usize::from(rem.ends_with(' '));
                    let c_start_rel = raw.start + lead;
                    let c_end_rel = raw.end - trail;
                    let c_start = line_start + c_start_rel;
                    let c_end = line_start + c_end_rel;
                    raw_cells.push(raw_abs);
                    cells.push(c_start..c_end);
                    if c_end_rel > c_start_rel {
                        let inner_slice = &line[c_start_rel..c_end_rel];
                        let (c_hid, c_pairs, c_segs) =
                            parse_inline_syntax_cell(inner_slice, c_start_rel);
                        row_hidden.extend(c_hid);
                        row_pairs.extend(c_pairs);
                        row_segments.extend(c_segs);
                    }
                }
                while cells.len() < cols {
                    cells.push(line_end..line_end);
                    raw_cells.push(line_end..line_end);
                }
                let merged_hidden = merge_line_ranges(row_hidden);
                push_line!(ParsedLine {
                    line_index: i as u32,
                    source: line_source,
                    content_end: line.len() as u32,
                    prefix_len: 0,
                    quote_depth: 0,
                    kind: LineKind::TableRow {
                        is_header,
                        body: Box::new(TableRowBody {
                            cells,
                            raw_cells,
                            alignments: Arc::clone(&tbl.alignments),
                            table_source: tbl.source.clone(),
                        }),
                    },
                    hidden_ranges: merged_hidden.into(),
                    inline: LineInline::boxed(row_pairs, row_segments, None),
                });
                continue;
            }
        }

        if let Some((level, content)) = atx_display_content_range(line) {
            list_walk.note_block(line);
            let parsed = atx_line(i, line_source, line, level, content);
            push_line!(parsed);
            continue;
        }

        // Determine block kind & prefix length
        let mut kind = LineKind::Paragraph;
        let mut prefix_len = 0usize;
        let mut quote_depth = 0u8;
        let mut content_end = line.len();
        let mut extra_hidden: Option<std::ops::Range<usize>> = None;

        if let Some(item) = list_walk.take_list(line) {
            let checkbox_offset = item
                .task
                .as_ref()
                .and_then(|task| u16::try_from(task.checkbox_at).ok())
                .filter(|&off| off != u16::MAX)
                .unwrap_or(u16::MAX);
            kind = LineKind::ListItem {
                ordered: item.ordered,
                number: item.number.min(u32::MAX as usize) as u32,
                task: item.task.as_ref().map(|task| task.checked),
                indent_cols: indent_cols.min(u16::MAX as usize) as u16,
                checkbox_offset,
            };
            prefix_len = item.prefix_end();
        } else if let Some((depth, q_len)) =
            strip_blockquote_markers(line, list_walk.thematic_parent_col(line) + 3)
        {
            list_walk.note_block(line);
            quote_depth = depth;
            let inner = &line[q_len..];
            if let Some((level, content)) = atx_display_content_range(inner) {
                kind = LineKind::Heading { level };
                prefix_len = q_len + content.start;
                content_end = q_len + content.end;
                if content_end < line.len() {
                    extra_hidden = Some(content_end..line.len());
                }
            } else if exact_thematic_break(inner, 0) {
                let hidden = if line.is_empty() {
                    Vec::new()
                } else {
                    vec![LineRange::new(0, line.len() as u32)]
                };
                push_line!(ParsedLine {
                    line_index: i as u32,
                    source: line_source,
                    content_end: line.len() as u32,
                    prefix_len: line.len() as u32,
                    quote_depth,
                    kind: LineKind::HorizontalRule,
                    hidden_ranges: hidden.into(),
                    inline: None,
                });
                continue;
            } else if split_indent(inner).1 <= 3 {
                if let Some(item) = scan_list_marker(inner) {
                    let checkbox_offset = item
                        .task
                        .as_ref()
                        .and_then(|task| u16::try_from(q_len + task.checkbox_at).ok())
                        .filter(|&off| off != u16::MAX)
                        .unwrap_or(u16::MAX);
                    kind = LineKind::ListItem {
                        ordered: item.ordered,
                        number: item.number.min(u32::MAX as usize) as u32,
                        task: item.task.as_ref().map(|task| task.checked),
                        indent_cols: split_indent(inner).1.min(u16::MAX as usize) as u16,
                        checkbox_offset,
                    };
                    prefix_len = q_len + item.prefix_end();
                } else {
                    kind = LineKind::Blockquote;
                    prefix_len = q_len;
                }
            } else {
                kind = LineKind::Blockquote;
                prefix_len = q_len;
            }
        } else {
            let _ = after_indent;
            let _ = indent_bytes;
            list_walk.note_paragraph();
        }

        let allow_hard_break = matches!(
            kind,
            LineKind::Paragraph | LineKind::Blockquote | LineKind::ListItem { .. }
        ) && raw_lines.get(local + 1).is_some_and(|next| {
            if quote_depth > 0 {
                strip_blockquote_markers(next.text, 3)
                    .map(|(_, q)| !next.text[q..].trim().is_empty())
                    .unwrap_or(false)
            } else {
                !next.text.trim().is_empty()
            }
        });
        let prefix_range = 0..prefix_len;
        let body = &line[prefix_len..content_end];
        let (mut inline_hidden, inline_pairs, styled_segments) =
            parse_inline_syntax_line(body, prefix_len, allow_hard_break);

        if allow_hard_break
            && inline_hidden
                .iter()
                .any(|r| r.start + 1 == line.len() && r.end == line.len())
        {
            content_end = line.len() - 1;
        }
        if prefix_len > 0 {
            inline_hidden.push(prefix_range);
        }
        if let Some(trail) = extra_hidden {
            inline_hidden.push(trail);
        }
        let merged_hidden = merge_line_ranges(inline_hidden);

        let display_text_owned = build_display(line, prefix_len, &merged_hidden);

        push_line!(ParsedLine {
            line_index: i as u32,
            source: line_source,
            content_end: content_end as u32,
            prefix_len: prefix_len as u32,
            quote_depth,
            kind,
            hidden_ranges: merged_hidden.into(),
            inline: LineInline::boxed(inline_pairs, styled_segments, display_text_owned),
        });
    }
    None
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod incremental_tests;
