use std::sync::Arc;

mod tables;
mod blocks;
mod lists;

pub use tables::*;
pub(crate) use blocks::*;
pub use lists::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub code: bool,
    pub link: Option<String>,
    /// Byte range of this span's text in the original markdown.
    pub source: std::ops::Range<usize>,
}

impl Span {
    pub fn plain(text: impl Into<String>) -> Self {
        let text = text.into();
        let source = 0..text.len();
        Self {
            text,
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            code: false,
            link: None,
            source,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
#[cfg_attr(not(test), allow(dead_code))]
pub enum Block {
    Heading {
        level: u8, // 1, 2, 3
        spans: Vec<Span>,
        source: std::ops::Range<usize>,
    },
    Paragraph {
        spans: Vec<Span>,
        source: std::ops::Range<usize>,
    },
    Blockquote {
        spans: Vec<Span>,
        source: std::ops::Range<usize>,
    },
    CodeBlock {
        language: Option<String>,
        code: String,
        source: std::ops::Range<usize>,
    },
    ListItem {
        ordered: bool,
        index: usize,
        is_task: bool,
        checked: bool,
        spans: Vec<Span>,
        source: std::ops::Range<usize>,
    },
    Table {
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
        source: std::ops::Range<usize>,
    },
    HorizontalRule {
        source: std::ops::Range<usize>,
    },
    EmptyLine {
        source: std::ops::Range<usize>,
    },
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn parse_spans(input: &str) -> Vec<Span> {
    parse_spans_at(input, 0)
}

fn parse_spans_at(input: &str, base: usize) -> Vec<Span> {
    if input.is_empty() {
        return Vec::new();
    }

    let mut options = pulldown_cmark::Options::empty();
    options.insert(pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
    let parser = pulldown_cmark::Parser::new_ext(input, options).into_offset_iter();

    let mut spans = Vec::new();
    let mut in_bold = false;
    let mut in_italic = false;
    let mut in_strike = false;
    let mut in_underline = false;
    let mut active_link: Option<String> = None;

    for (event, range) in parser {
        let source = (base + range.start)..(base + range.end);
        match event {
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::Strong) => in_bold = true,
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Strong) => in_bold = false,
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::Emphasis) => in_italic = true,
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Emphasis) => in_italic = false,
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::Strikethrough) => in_strike = true,
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Strikethrough) => in_strike = false,
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::Link { dest_url, .. }) => {
                active_link = Some(dest_url.to_string());
            }
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Link) => {
                active_link = None;
            }
            pulldown_cmark::Event::InlineHtml(h) | pulldown_cmark::Event::Html(h) => {
                if h.contains("<u>") {
                    in_underline = true;
                } else if h.contains("</u>") {
                    in_underline = false;
                }
            }
            pulldown_cmark::Event::Code(c) => {
                spans.push(Span {
                    text: c.to_string(),
                    bold: in_bold,
                    italic: in_italic,
                    underline: in_underline,
                    strike: in_strike,
                    code: true,
                    link: active_link.clone(),
                    source,
                });
            }
            pulldown_cmark::Event::Text(t) => {
                spans.push(Span {
                    text: t.to_string(),
                    bold: in_bold,
                    italic: in_italic,
                    underline: in_underline,
                    strike: in_strike,
                    code: false,
                    link: active_link.clone(),
                    source,
                });
            }
            _ => {}
        }
    }

    if spans.is_empty() {
        let mut plain = Span::plain(input);
        plain.source = base..base + input.len();
        spans.push(plain);
    }

    spans
}

struct SourceLine<'a> {
    start: usize,
    end: usize,
    text: &'a str,
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

#[cfg_attr(not(test), allow(dead_code))]
fn inline_at(line_start: usize, line: &str, rest: &str) -> usize {
    line.find(rest)
        .map(|index| line_start + index)
        .unwrap_or(line_start)
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn parse_markdown(markdown: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let lines = source_lines(markdown);
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].text;
        let line_source = lines[i].start..lines[i].end;
        let trimmed = line.trim();

        if trimmed.is_empty() {
            blocks.push(Block::EmptyLine {
                source: line_source,
            });
            i += 1;
            continue;
        }

        // Code block ```
        if trimmed.starts_with("```") {
            let language = trimmed
                .strip_prefix("```")
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let open = i;
            let mut code_lines = Vec::new();
            i += 1;
            let content_start = lines.get(i).map(|line| line.start);
            while i < lines.len() && !lines[i].text.trim().starts_with("```") {
                code_lines.push(lines[i].text);
                i += 1;
            }
            let content_end = if i > open + 1 {
                let last = &lines[i - 1];
                last.start + last.text.len()
            } else {
                content_start.unwrap_or(lines[open].end)
            };
            if i < lines.len() {
                i += 1; // skip closing ```
            }
            let source = content_start.unwrap_or(lines[open].start)..content_end;
            blocks.push(Block::CodeBlock {
                language,
                code: code_lines.join("\n"),
                source,
            });
            continue;
        }

        // Horizontal rule: exact `---`, `***`, or `___`, indent 0–3.
        if exact_thematic_break(line, 0) {
            blocks.push(Block::HorizontalRule {
                source: line_source,
            });
            i += 1;
            continue;
        }

        // Headings #, ##, ###
        if let Some(rest) = trimmed.strip_prefix("### ") {
            let base = inline_at(lines[i].start, line, rest);
            blocks.push(Block::Heading {
                level: 3,
                spans: parse_spans_at(rest, base),
                source: line_source,
            });
            i += 1;
            continue;
        } else if let Some(rest) = trimmed.strip_prefix("## ") {
            let base = inline_at(lines[i].start, line, rest);
            blocks.push(Block::Heading {
                level: 2,
                spans: parse_spans_at(rest, base),
                source: line_source,
            });
            i += 1;
            continue;
        } else if let Some(rest) = trimmed.strip_prefix("# ") {
            let base = inline_at(lines[i].start, line, rest);
            blocks.push(Block::Heading {
                level: 1,
                spans: parse_spans_at(rest, base),
                source: line_source,
            });
            i += 1;
            continue;
        }

        // Blockquote >
        if let Some(rest) = trimmed.strip_prefix("> ") {
            let base = inline_at(lines[i].start, line, rest);
            blocks.push(Block::Blockquote {
                spans: parse_spans_at(rest, base),
                source: line_source,
            });
            i += 1;
            continue;
        }

        // GFM table: the header's next line is a delimiter row of the same
        // column count. Body rows follow the delimiter's shape.
        if let Some(table) = table_block(&lines, i) {
            let table_start = lines[i].start;
            let table_end = lines[table.last].end;
            let cols = table.alignments.len();
            let mut table_rows = Vec::new();
            for (row, line) in lines.iter().enumerate().take(table.last + 1).skip(i) {
                if row == table.delimiter {
                    continue;
                }
                let mut cells = split_table_cells(line.text);
                if cells.len() > cols {
                    cells.truncate(cols);
                }
                while cells.len() < cols {
                    cells.push(String::new());
                }
                table_rows.push(cells);
            }
            if !table_rows.is_empty() {
                let headers = table_rows.remove(0);
                blocks.push(Block::Table {
                    headers,
                    rows: table_rows,
                    source: table_start..table_end,
                });
            }
            i = table.last + 1;
            continue;
        }

        // Task List - [ ] or - [x]
        if let Some(rest) = trimmed
            .strip_prefix("- [ ] ")
            .or_else(|| trimmed.strip_prefix("* [ ] "))
        {
            let base = inline_at(lines[i].start, line, rest);
            blocks.push(Block::ListItem {
                ordered: false,
                index: 0,
                is_task: true,
                checked: false,
                spans: parse_spans_at(rest, base),
                source: line_source,
            });
            i += 1;
            continue;
        }
        if let Some(rest) = trimmed
            .strip_prefix("- [x] ")
            .or_else(|| trimmed.strip_prefix("- [X] "))
            .or_else(|| trimmed.strip_prefix("* [x] "))
            .or_else(|| trimmed.strip_prefix("* [X] "))
        {
            let base = inline_at(lines[i].start, line, rest);
            blocks.push(Block::ListItem {
                ordered: false,
                index: 0,
                is_task: true,
                checked: true,
                spans: parse_spans_at(rest, base),
                source: line_source,
            });
            i += 1;
            continue;
        }

        // Unordered List - or *
        if let Some(rest) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            let base = inline_at(lines[i].start, line, rest);
            blocks.push(Block::ListItem {
                ordered: false,
                index: 0,
                is_task: false,
                checked: false,
                spans: parse_spans_at(rest, base),
                source: line_source,
            });
            i += 1;
            continue;
        }

        // Ordered list 1.
        let mut is_ordered = false;
        let mut ord_num = 1;
        let mut ord_rest = "";
        for (idx, ch) in trimmed.char_indices() {
            if ch == '.' && idx > 0 {
                if let Ok(num) = trimmed[..idx].parse::<usize>() {
                    let rem = trimmed[idx + 1..].trim_start();
                    is_ordered = true;
                    ord_num = num;
                    ord_rest = rem;
                    break;
                }
            } else if !ch.is_ascii_digit() {
                break;
            }
        }

        if is_ordered {
            let base = inline_at(lines[i].start, line, ord_rest);
            blocks.push(Block::ListItem {
                ordered: true,
                index: ord_num,
                is_task: false,
                checked: false,
                spans: parse_spans_at(ord_rest, base),
                source: line_source,
            });
            i += 1;
            continue;
        }

        // Default Paragraph
        let base = lines[i].start;
        blocks.push(Block::Paragraph {
            spans: parse_spans_at(line, base),
            source: line_source,
        });
        i += 1;
    }

    blocks
}

#[cfg_attr(not(test), allow(dead_code))]
fn block_source(block: &Block) -> std::ops::Range<usize> {
    match block {
        Block::Heading { source, .. }
        | Block::Paragraph { source, .. }
        | Block::Blockquote { source, .. }
        | Block::CodeBlock { source, .. }
        | Block::ListItem { source, .. }
        | Block::Table { source, .. }
        | Block::HorizontalRule { source }
        | Block::EmptyLine { source } => source.clone(),
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn spans_of(block: &Block) -> Option<&[Span]> {
    match block {
        Block::Heading { spans, .. }
        | Block::Paragraph { spans, .. }
        | Block::Blockquote { spans, .. }
        | Block::ListItem { spans, .. } => Some(spans),
        _ => None,
    }
}

/// Map a click on rendered block `block_index` at `rendered_char` back to a source offset.
#[cfg_attr(not(test), allow(dead_code))]
pub fn source_offset_for_click(
    markdown: &str,
    block_index: usize,
    rendered_char: usize,
) -> Option<usize> {
    let blocks = parse_markdown(markdown);
    let block = blocks.get(block_index)?;
    if let Some(spans) = spans_of(block) {
        if spans.is_empty() {
            return Some(floor_source(markdown, block_source(block).end));
        }
        let mut seen = 0usize;
        for span in spans {
            let chars = span.text.chars().count();
            if rendered_char < seen + chars {
                let within = span
                    .text
                    .char_indices()
                    .nth(rendered_char - seen)
                    .map(|(index, _)| index)
                    .unwrap_or(span.text.len());
                let offset = (span.source.start + within).min(span.source.end);
                return Some(floor_source(markdown, offset));
            }
            seen += chars;
        }
        let end = spans
            .last()
            .map(|span| span.source.end)
            .unwrap_or(block_source(block).start);
        return Some(floor_source(markdown, end));
    }
    if let Block::CodeBlock { source, code, .. } = block {
        let within = code
            .char_indices()
            .nth(rendered_char)
            .map(|(index, _)| index)
            .unwrap_or(code.len());
        return Some(floor_source(
            markdown,
            source.start + within.min(source.len()),
        ));
    }
    Some(floor_source(markdown, block_source(block).start))
}

#[cfg_attr(not(test), allow(dead_code))]
fn floor_source(markdown: &str, mut offset: usize) -> usize {
    if offset > markdown.len() {
        return markdown.len();
    }
    while offset > 0 && !markdown.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
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
    Heading { level: u8 }, // 1..=6
    Blockquote,
    ListItem {
        ordered: bool,
        number: usize,
        task: Option<bool>, // Some(true) = [x], Some(false) = [ ], None = normal bullet/number
        indent_cols: usize,
        checkbox_source: Option<std::ops::Range<usize>>, // byte range of "[ ]" or "[x]"
    },
    CodeFence {
        language: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledSegment {
    /// Byte range in the original markdown `content`
    pub source: std::ops::Range<usize>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub code: bool,
    pub link_url: Option<String>,
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

    pub fn from_hidden(
        line_text: &str,
        line_start: usize,
        visible_start: usize,
        hidden: &[std::ops::Range<usize>],
    ) -> (String, Self) {
        let line_end = line_start + line_text.len();
        // Walk the visible gaps twice: once to size the buffers exactly, once
        // to fill them, so no temporary list is allocated.
        let for_each_visible = |f: &mut dyn FnMut(std::ops::Range<usize>)| {
            let mut cursor = line_start;
            for h in hidden {
                let h_start = h.start.clamp(line_start, line_end);
                let h_end = h.end.clamp(line_start, line_end);
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

    /// Slice a sub-map for a wrapped visual row covering `disp_range` in `display_text`.
    pub fn slice_display(&self, disp_range: std::ops::Range<usize>) -> Self {
        let mut sub_segments = Vec::new();
        for (d, s) in &self.segments {
            if d.end <= disp_range.start || d.start >= disp_range.end {
                continue;
            }
            let overlap_start = d.start.max(disp_range.start);
            let overlap_end = d.end.min(disp_range.end);
            if overlap_end > overlap_start || (d.start == d.end && d.start >= disp_range.start && d.start <= disp_range.end) {
                let src_start = s.start + (overlap_start - d.start);
                let src_end = s.start + (overlap_end - d.start);
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

/// Shared empty `link_hits`, so lines without links don't each allocate one.
fn no_links() -> Arc<Vec<(std::ops::Range<usize>, String)>> {
    thread_local! {
        static EMPTY: Arc<Vec<(std::ops::Range<usize>, String)>> = Arc::default();
    }
    EMPTY.with(Arc::clone)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedLine {
    pub line_index: usize,
    /// Full byte range of the line (excluding `\n`)
    pub source: std::ops::Range<usize>,
    pub kind: LineKind,
    /// Byte range of block prefix hidden in WYSIWYG (e.g. `# `, `- [ ] `, `> `)
    pub prefix_range: std::ops::Range<usize>,
    /// All byte ranges hidden in WYSIWYG on this line (sorted, non-overlapping)
    pub hidden_ranges: Vec<std::ops::Range<usize>>,
    /// Paired inline delimiter ranges for atomic backspace & smart toggle-off
    pub inline_pairs: Vec<InlinePair>,
    /// Visible text in WYSIWYG mode (with hidden_ranges stripped)
    pub display_text: String,
    pub offset_map: Arc<RowOffsetMap>,
    pub styled_segments: Vec<StyledSegment>,
    /// Link spans `(source, url)`, filled once when the document is parsed so
    /// painting can share them instead of rebuilding them every frame.
    pub link_hits: Arc<Vec<(std::ops::Range<usize>, String)>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedDocument {
    pub lines: Vec<ParsedLine>,
    /// Fenced code block ranges `(open_fence_line_range, close_fence_line_range)`
    pub code_blocks: Vec<(std::ops::Range<usize>, std::ops::Range<usize>)>,
}

impl ParsedDocument {
    /// Every inline delimiter pair in the document, line by line. Only the
    /// tests want the flat view; the editor looks pairs up by line.
    #[cfg(test)]
    pub fn all_inline_pairs(&self) -> impl Iterator<Item = &InlinePair> {
        self.lines.iter().flat_map(|l| l.inline_pairs.iter())
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
                        if clip.start <= line.prefix_range.end {
                            out.push_str(&" ".repeat(*indent_cols));
                            match (task, ordered) {
                                (Some(true), _) => out.push_str("☑ "),
                                (Some(false), _) => out.push_str("☐ "),
                                (None, true) => out.push_str(&format!("{number}. ")),
                                (None, false) => out.push_str("• "),
                            }
                        }
                    }
                    let mut cursor = clip.start;
                    for hidden in &line.hidden_ranges {
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

pub(crate) fn prepare_for_pulldown(input: &str) -> std::borrow::Cow<'_, str> {
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut modified = None;

    let is_delim = |b: u8| b == b'*' || b == b'~' || b == b'_' || b == b'`';

    let mut i = 0;
    while i < len {
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
    Vec<StyledSegment>,
);

fn parse_inline_syntax(input: &str, base: usize) -> InlineSyntax {
    if input.is_empty() {
        return (Vec::new(), Vec::new(), Vec::new());
    }
    // Block structure is decided before this point, so leading indentation
    // is only text. Pulldown would read four columns of it as indented code
    // and drop every inline format (`    <u>x</u>`, a lazy continuation line).
    let body = input.trim_start_matches([' ', '\t']);
    if body.len() < input.len() {
        let lead = input.len() - body.len();
        return parse_inline_syntax(body, base + lead);
    }
    if is_plain_inline(input) {
        let plain = StyledSegment {
            source: base..base + input.len(),
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            code: false,
            link_url: None,
        };
        return (Vec::new(), Vec::new(), vec![plain]);
    }
    parse_inline_with_pulldown(input, base)
}

/// Text pulldown-cmark would return as one unstyled run: it starts with a
/// letter (so no list, heading, quote or indented code), has no edge
/// whitespace to trim, no control characters, and no byte that can open
/// inline syntax.
fn is_plain_inline(input: &str) -> bool {
    let bytes = input.as_bytes();
    let (Some(&first), Some(&last)) = (bytes.first(), bytes.last()) else {
        return false;
    };
    (first.is_ascii_alphabetic() || !first.is_ascii())
        && last != b' '
        && !bytes.iter().any(|&b| {
            b.is_ascii_control()
                || matches!(
                    b,
                    b'*' | b'_' | b'~' | b'`' | b'[' | b']' | b'<' | b'>' | b'\\' | b'&' | b'!' | b'|'
                )
        })
}

fn parse_inline_with_pulldown(input: &str, base: usize) -> InlineSyntax {
    let prepared = prepare_for_pulldown(input);
    let mut options = pulldown_cmark::Options::empty();
    options.insert(pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
    let parser = pulldown_cmark::Parser::new_ext(&prepared, options).into_offset_iter();

    let mut raw_hidden = Vec::new();
    let mut inline_pairs = Vec::new();
    let mut styled_segments = Vec::new();

    let mut bold_stack: Vec<usize> = Vec::new();
    let mut italic_stack: Vec<usize> = Vec::new();
    let mut strike_stack: Vec<usize> = Vec::new();
    let mut underline_stack: Vec<usize> = Vec::new();
    let mut link_stack: Vec<(usize, String, usize)> = Vec::new(); // (outer_start, url, last_inner_end)

    for (event, range) in parser {
        let abs_start = base + range.start;
        let abs_end = base + range.end;
        match event {
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::Strong) => {
                bold_stack.push(abs_start);
            }
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Strong) => {
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
                link_stack.push((abs_start, dest_url.to_string(), abs_start + 1));
            }
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Link) => {
                if let Some((open_start, _url, inner_end)) = link_stack.pop() {
                    let open_d = open_start..(open_start + 1).min(abs_end);
                    let close_start = inner_end.clamp(open_d.end, abs_end);
                    let close_d = close_start..abs_end;
                    let inner = open_d.end..close_d.start;
                    raw_hidden.push(open_d.clone());
                    raw_hidden.push(close_d.clone());
                    inline_pairs.push(InlinePair::new(
                        InlineTagKind::Link,
                        open_d,
                        inner,
                        close_d,
                    ));
                }
            }
            pulldown_cmark::Event::InlineHtml(h) | pulldown_cmark::Event::Html(h) => {
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
                    last_link.2 = abs_end;
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
                    styled_segments.push(StyledSegment {
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
                    last_link.2 = abs_end;
                }
                if abs_end > abs_start {
                    styled_segments.push(StyledSegment {
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
            _ => {}
        }
    }

    normalize_shared_run_pairs(&mut inline_pairs);
    synthesize_flanking_pairs(
        input,
        base,
        &mut raw_hidden,
        &mut inline_pairs,
        &mut styled_segments,
    );

    // Backslash escapes: the parser consumes `\X` (X an ASCII punctuation
    // character) and shows only `X`, so the backslash is hidden in the
    // WYSIWYG display with the offset map carrying the caret across it.
    // Backslashes inside code spans stay visible: code content is literal.
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
                if !index_in_ranges(&code_inner, abs) {
                    raw_hidden.push(abs..abs + 1);
                    escapes.push(abs);
                }
                i += 2;
            } else {
                i += 1;
            }
        }
        if !escapes.is_empty() {
            // The text segment covering an escape keeps the escaped character
            // only; the backslash is cut out of it.
            let mut rebuilt: Vec<StyledSegment> = Vec::with_capacity(styled_segments.len());
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
                        rebuilt.push(StyledSegment {
                            source: cursor..e,
                            ..s.clone()
                        });
                    }
                    cursor = e + 1;
                }
                if s.source.end > cursor {
                    rebuilt.push(StyledSegment {
                        source: cursor..s.source.end,
                        ..s
                    });
                }
            }
            styled_segments = rebuilt;
        }
    }

    finalize_pair_groups(&mut inline_pairs);
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
    raw_hidden: &mut Vec<std::ops::Range<usize>>,
    inline_pairs: &mut Vec<InlinePair>,
    styled_segments: &mut Vec<StyledSegment>,
) {
    let bytes = input.as_bytes();
    // Runs of unescaped emphasis delimiters, in source order.
    let mut runs: Vec<(usize, usize, u8)> = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if (b == b'*' || b == b'~' || b == b'`') && !is_escaped_delim(input, i) {
            let start = i;
            while i < bytes.len() && bytes[i] == b && !is_escaped_delim(input, i) {
                i += 1;
            }
            runs.push((start, i, b));
        } else {
            i += 1;
        }
    }
    if runs.len() < 2 {
        return;
    }
    // A run is out of play when it starts inside a code span or was consumed
    // as the opener or closer of a parsed pair. Every byte of a consumed run
    // sits inside an open or close delim: a `***` split into `**` + `*` is
    // covered, but an extra leftover backtick in ``a``` is not. Both are
    // checked against sorted, merged ranges so long lines stay linear.
    let code_inner = merge_ranges(
        inline_pairs
            .iter()
            .filter(|p| p.kind == InlineTagKind::Code)
            .map(|p| p.inner.clone())
            .collect(),
    );
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
        .map(|&(s, e, _)| index_in_ranges(&code_inner, base + s) || consumed(base + s..base + e))
        .collect();
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
            !used[k] && !blocked[k] && c_start != o_end && (ch != b'`' || c_end - c_start > o_len)
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
        let or_kind = |s: &mut StyledSegment| match kind {
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
        let mut rebuilt: Vec<StyledSegment> = Vec::new();
        let mut inner_covered = false;
        for s in styled_segments.drain(..) {
            if s.source.start < inner.end && s.source.end > inner.start {
                if s.source.start < inner.start {
                    rebuilt.push(StyledSegment {
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
                    rebuilt.push(StyledSegment {
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
                        next.push(StyledSegment {
                            source: s.source.start..run.start,
                            ..s.clone()
                        });
                    }
                    if s.source.end > run.end {
                        next.push(StyledSegment {
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
            let mut seg = StyledSegment {
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

#[allow(clippy::single_range_in_vec_init)]
pub fn parse_document(markdown: &str) -> ParsedDocument {
    let raw_lines = if markdown.is_empty() {
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
    };

    let mut lines = Vec::with_capacity(raw_lines.len());
    let mut list_quote_tracker = InListOrQuoteTracker::default();
    let mut code_blocks = Vec::new();

    let mut open_fence: Option<OpenFence> = None;
    let mut table: Option<DetectedTable> = None;

    let mut list_walk = ListWalk::default();

    macro_rules! push_line {
        ($line:expr) => {{
            let p = $line;
            list_quote_tracker.push(&p.kind);
            lines.push(p);
        }};
    }
    for (i, sline) in raw_lines.iter().enumerate() {
        let line = sline.text;
        let line_start = sline.start;
        let line_end = line_start + line.len();
        let line_source = line_start..line_end;
        let trimmed = line.trim();
        let trimmed_start = line.trim_start();
        let indent_len = line.len() - trimmed_start.len();

        // Fences. Top-level backtick and tilde fences follow CommonMark. A
        // fence in a list item or quote, and a tab-indented backtick fence,
        // keep red's trimmed-marker rule.
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
                let fence = open_fence.take().expect("fence");
                code_blocks.push((fence.open_range, sline.start..sline.end));
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
            open_fence = Some(OpenFence {
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
                    language,
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
        let after_block = lines.last().is_none_or(|prev: &ParsedLine| {
            matches!(
                prev.kind,
                LineKind::Empty
                    | LineKind::Heading { .. }
                    | LineKind::HorizontalRule
                    | LineKind::CodeLine
                    | LineKind::CodeFence { opening: false, .. }
            )
        });
        // `\t- foo` stays a list item, as `take_list` accepts it (red parity).
        let indented_code = after_block
            && !trimmed.is_empty()
            && leading_column(line) >= 4
            && list_walk.thematic_parent_col(line) == 0
            && !(single_tab_marker(line) && scan_list_marker(line).is_some());
        // Inside an indented code block (blank lines included) a ```` line is
        // code. Red's trimmed-backtick rule below would open a fence there and
        // swallow the rest of the document. A fence is open only in the
        // branch above, so a code line before the blanks is indented code.
        let continues_indented_code = indented_code
            && lines
                .iter()
                .rev()
                .find(|prev| !matches!(prev.kind, LineKind::Empty))
                .is_some_and(|prev| matches!(prev.kind, LineKind::CodeLine));
        if continues_indented_code {
            list_walk.note_block(line);
            let strip = prefix_through_column(line, 4);
            let parsed = code_content_line(i, line_source, line, strip);
            push_line!(parsed);
            continue;
        }

        let skip_rejected = !list_quote_tracker.is_in_list_or_quote() && backtick_info_rejects_fence(line);
        if trimmed.starts_with("```") && !skip_rejected {
            list_walk.note_block(line);
            let lang = trimmed
                .strip_prefix("```")
                .map(|info| info.trim().to_string())
                .filter(|info| !info.is_empty());
            open_fence = Some(OpenFence {
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
                .map(str::to_string);
            open_fence = Some(OpenFence {
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
                line_index: i,
                source: line_source.clone(),
                kind: LineKind::Empty,
                prefix_range: line_start..line_start,
                hidden_ranges: Vec::new(),
                inline_pairs: Vec::new(),
                display_text: line.to_string(),
                offset_map: Arc::new(RowOffsetMap::identity(line_start, line_end)),
                link_hits: no_links(),
                styled_segments: Vec::new(),
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
            let hidden = vec![line_start..line_end];
            let (display_text, offset_map) =
                RowOffsetMap::from_hidden(line, line_start, line_end, &hidden);
            push_line!(ParsedLine {
                line_index: i,
                source: line_source,
                kind: LineKind::HorizontalRule,
                prefix_range: line_start..line_end,
                hidden_ranges: hidden,
                inline_pairs: Vec::new(),
                display_text,
                offset_map: Arc::new(offset_map),
                link_hits: no_links(),
                styled_segments: Vec::new(),
            });
            continue;
        }

        // Table rows. Detection starts only on a line the fence walk did not
        // consume, and the block follows the delimiter row's column shape.
        if table.as_ref().is_none_or(|t| i > t.last) {
            table = table_block(&raw_lines, i).map(|block| {
                let byte_start = raw_lines[i].start;
                let last_line = &raw_lines[block.last];
                DetectedTable {
                    first: i,
                    last: block.last,
                    delimiter: block.delimiter,
                    source: byte_start..(last_line.start + last_line.text.len()),
                    alignments: block.alignments.into(),
                }
            });
        }
        if let Some(tbl) = table.as_ref().filter(|tbl| i >= tbl.first && i <= tbl.last) {
            list_walk.note_block(line);
            if i == tbl.delimiter {
                let hidden = vec![line_start..line_end];
                let (display_text, offset_map) =
                    RowOffsetMap::from_hidden(line, line_start, line_end, &hidden);
                push_line!(ParsedLine {
                    line_index: i,
                    source: line_source,
                    kind: LineKind::TableDelimiter,
                    prefix_range: line_start..line_end,
                    hidden_ranges: hidden,
                    inline_pairs: Vec::new(),
                    display_text,
                    offset_map: Arc::new(offset_map),
                    link_hits: no_links(),
                    styled_segments: Vec::new(),
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
                for raw in table_cell_ranges(line).into_iter().take(cols) {
                    let raw_abs = (line_start + raw.start)..(line_start + raw.end);
                    let cell_slice = &line[raw.clone()];
                    let lead = usize::from(cell_slice.starts_with(' '));
                    let rem = &cell_slice[lead..];
                    let trail = usize::from(rem.ends_with(' '));
                    let c_start = raw_abs.start + lead;
                    let c_end = raw_abs.end - trail;
                    raw_cells.push(raw_abs);
                    cells.push(c_start..c_end);
                    if c_end > c_start {
                        let inner_slice = &line[(c_start - line_start)..(c_end - line_start)];
                        let (c_hid, c_pairs, c_segs) = parse_inline_syntax(inner_slice, c_start);
                        row_hidden.extend(c_hid);
                        row_pairs.extend(c_pairs);
                        row_segments.extend(c_segs);
                    }
                }
                while cells.len() < cols {
                    cells.push(line_end..line_end);
                    raw_cells.push(line_end..line_end);
                }
                if row_segments.is_empty() {
                    row_segments.push(StyledSegment {
                        source: line_source.clone(),
                        bold: false,
                        italic: false,
                        underline: false,
                        strike: false,
                        code: false,
                        link_url: None,
                    });
                }
                let merged_hidden = merge_ranges(row_hidden);
                push_line!(ParsedLine {
                    line_index: i,
                    source: line_source.clone(),
                    kind: LineKind::TableRow {
                        is_header,
                        body: Box::new(TableRowBody {
                            cells,
                            raw_cells,
                            alignments: Arc::clone(&tbl.alignments),
                            table_source: tbl.source.clone(),
                        }),
                    },
                    prefix_range: line_start..line_start,
                    hidden_ranges: merged_hidden,
                    inline_pairs: row_pairs,
                    display_text: line.to_string(),
                    offset_map: Arc::new(RowOffsetMap::identity(line_start, line_end)),
                    link_hits: no_links(),
                    styled_segments: row_segments,
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

        if let Some(item) = list_walk.take_list(line) {
            let checkbox_source = item.task.as_ref().map(|task| {
                let start = line_start + task.checkbox_at;
                start..start + 3
            });
            kind = LineKind::ListItem {
                ordered: item.ordered,
                number: item.number,
                task: item.task.as_ref().map(|task| task.checked),
                indent_cols: indent_len,
                checkbox_source,
            };
            prefix_len = item.prefix_end();
        } else if trimmed_start.starts_with("> ") {
            list_walk.note_block(line);
            kind = LineKind::Blockquote;
            prefix_len = indent_len + 2;
        } else {
            list_walk.note_paragraph();
        }

        let prefix_range = line_start..(line_start + prefix_len);
        let body = &line[prefix_len..];
        let body_base = line_start + prefix_len;
        let (mut inline_hidden, inline_pairs, styled_segments) =
            parse_inline_syntax(body, body_base);

        if prefix_len > 0 {
            inline_hidden.push(prefix_range.clone());
        }
        let merged_hidden = merge_ranges(inline_hidden);

        let (display_text, offset_map) =
            RowOffsetMap::from_hidden(line, line_start, body_base, &merged_hidden);

        push_line!(ParsedLine {
            line_index: i,
            source: line_source,
            kind,
            prefix_range,
            hidden_ranges: merged_hidden,
            inline_pairs,
            display_text,
            offset_map: Arc::new(offset_map),
            link_hits: no_links(),
            styled_segments,
        });
    }

    if let Some(fence) = open_fence {
        code_blocks.push((fence.open_range, markdown.len()..markdown.len()));
    }

    for line in &mut lines {
        let links: Vec<_> = line
            .styled_segments
            .iter()
            .filter_map(|seg| match &seg.link_url {
                Some(url) if seg.source.end > seg.source.start => {
                    Some((seg.source.clone(), url.clone()))
                }
                _ => None,
            })
            .collect();
        if !links.is_empty() {
            line.link_hits = Arc::new(links);
        }
    }

    ParsedDocument {
        lines,
        code_blocks,
    }
}

#[cfg(test)]
mod tests;
