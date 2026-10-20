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
    let mut start = 0usize;
    if markdown.is_empty() {
        return lines;
    }
    for piece in markdown.split_inclusive('\n') {
        let end = start + piece.len();
        let text = piece.strip_suffix('\n').unwrap_or(piece);
        lines.push(SourceLine { start, end, text });
        start = end;
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
    // Same fence tracker as `parse_document`: a tilde fence is not a table,
    // and an indented-4 backtick line is not a fence (`opening_fence`).
    let mut fence: Option<(u8, usize)> = None;

    while i < lines.len() {
        let line = lines[i].text;
        let line_source = lines[i].start..lines[i].end;

        if let Some((marker, length)) = fence {
            if closes_fence(line, marker, length) {
                fence = None;
            }
            blocks.push(Block::Paragraph {
                spans: parse_spans_at(line, lines[i].start),
                source: line_source,
            });
            i += 1;
            continue;
        }

        let trimmed = line.trim();

        if trimmed.is_empty() {
            blocks.push(Block::EmptyLine {
                source: line_source,
            });
            i += 1;
            continue;
        }

        // Code fence. GFM: "If the info string comes after a backtick fence,
        // it may not contain any backtick characters."
        if let Some((marker, length)) = opening_fence(line) {
            if marker == b'~' {
                fence = Some((marker, length));
                blocks.push(Block::Paragraph {
                    spans: parse_spans_at(line, lines[i].start),
                    source: line_source,
                });
                i += 1;
                continue;
            }
            let body = after_block_indent(line).unwrap_or(line);
            let info = body.get(length..).unwrap_or("").trim();
            let language = if info.is_empty() {
                None
            } else {
                Some(info.to_string())
            };
            let open = i;
            let mut code_lines = Vec::new();
            i += 1;
            let content_start = lines.get(i).map(|line| line.start);
            while i < lines.len() && !closes_fence(lines[i].text, marker, length) {
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
                i += 1; // skip closing fence
            }
            let source = content_start.unwrap_or(lines[open].start)..content_end;
            blocks.push(Block::CodeBlock {
                language,
                code: code_lines.join("\n"),
                source,
            });
            continue;
        }

        // Horizontal rule ---, ***, ___
        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
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

        // Markdown table. Column count is the delimiter row, which must match the header.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TableColumnAlign {
    /// Delimiter is plain `---`. A table edit must not turn this into `:---`.
    #[default]
    None,
    Left,
    Center,
    Right,
}

/// Inline code spans, including the backtick delimiters. An unclosed opener is not a span.
fn code_span_ranges(line: &str) -> Vec<std::ops::Range<usize>> {
    let bytes = line.as_bytes();
    let mut ranges = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'`' {
            index += 1;
            continue;
        }
        let open = index;
        while index < bytes.len() && bytes[index] == b'`' {
            index += 1;
        }
        let width = index - open;
        let mut cursor = index;
        let mut closed_at = None;
        while cursor < bytes.len() {
            if bytes[cursor] != b'`' {
                cursor += 1;
                continue;
            }
            let close = cursor;
            while cursor < bytes.len() && bytes[cursor] == b'`' {
                cursor += 1;
            }
            if cursor - close == width {
                closed_at = Some(cursor);
                break;
            }
        }
        if let Some(end) = closed_at {
            ranges.push(open..end);
            index = end;
        }
    }
    ranges
}

fn index_in_ranges(ranges: &[std::ops::Range<usize>], index: usize) -> bool {
    ranges
        .iter()
        .any(|range| index >= range.start && index < range.end)
}

fn separator_pipe(bytes: &[u8], code: &[std::ops::Range<usize>], index: usize) -> bool {
    if index >= bytes.len() || bytes[index] != b'|' || index_in_ranges(code, index) {
        return false;
    }
    let mut slashes = 0usize;
    let mut look = index;
    while look > 0 && bytes[look - 1] == b'\\' && !index_in_ranges(code, look - 1) {
        slashes += 1;
        look -= 1;
    }
    slashes.is_multiple_of(2)
}

/// Byte offsets of `|` that split a table cell.
/// A backslash-escaped pipe stays in the cell, and so does a pipe inside a code span.
pub fn unescaped_pipe_offsets(line: &str) -> Vec<usize> {
    let bytes = line.as_bytes();
    let code = code_span_ranges(line);
    let mut offsets = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if separator_pipe(bytes, &code, index) {
            offsets.push(index);
        }
        index += 1;
    }
    offsets
}

/// Raw cell slices for one row. Leading and trailing border pipes are optional.
/// The returned ranges include the spaces next to the pipes; callers trim those.
fn table_cell_ranges(line: &str) -> Vec<std::ops::Range<usize>> {
    let bytes = line.as_bytes();
    let code = code_span_ranges(line);
    let mut start = 0;
    while start < bytes.len() && bytes[start].is_ascii_whitespace() {
        start += 1;
    }
    let mut end = bytes.len();
    while end > start && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    if start >= end {
        return Vec::new();
    }
    if separator_pipe(bytes, &code, start) {
        start += 1;
    }

    let mut ranges = Vec::new();
    while start < end {
        let cell_start = start;
        while start < end && !separator_pipe(bytes, &code, start) {
            if let Some(span) = code.iter().find(|span| span.start == start) {
                start = span.end;
            } else {
                start += 1;
            }
        }
        let cell_end = start.min(end);
        ranges.push(cell_start..cell_end);
        if start < end && separator_pipe(bytes, &code, start) {
            start += 1;
            if start >= end {
                break;
            }
        } else {
            break;
        }
    }
    ranges
}

/// Cell text between separator pipes. `\|` and a pipe inside `` ` ` `` stay in the cell.
pub fn split_table_cells(line: &str) -> Vec<String> {
    table_cell_ranges(line)
        .into_iter()
        .map(|range| line[range].trim().to_string())
        .collect()
}

/// A delimiter cell is optional spaces, an optional colon, one or more hyphens,
/// and an optional colon. Spaces may sit between the colons and the hyphens.
/// A tab is not a space here: it disqualifies the cell.
fn is_delimiter_cell(cell: &str) -> bool {
    let bytes = cell.as_bytes();
    let mut index = 0;
    let mut end = bytes.len();
    while index < end && bytes[index] == b' ' {
        index += 1;
    }
    while end > index && bytes[end - 1] == b' ' {
        end -= 1;
    }
    if index >= end {
        return false;
    }
    if bytes[index] == b':' {
        index += 1;
    }
    while index < end && bytes[index] == b' ' {
        index += 1;
    }
    let hyphens_at = index;
    while index < end && bytes[index] == b'-' {
        index += 1;
    }
    if index == hyphens_at {
        return false;
    }
    while index < end && bytes[index] == b' ' {
        index += 1;
    }
    if index < end && bytes[index] == b':' {
        index += 1;
    }
    index == end
}

/// Alignment for a delimiter row, or `None` when the line is not one.
/// A delimiter must contain a pipe, so a thematic break stays a break.
fn delimiter_alignments(line: &str) -> Option<Vec<TableColumnAlign>> {
    if unescaped_pipe_offsets(line).is_empty() {
        return None;
    }
    let ranges = table_cell_ranges(line);
    if ranges.is_empty() {
        return None;
    }
    let mut alignments = Vec::with_capacity(ranges.len());
    for range in ranges {
        let cell = &line[range];
        if !is_delimiter_cell(cell) {
            return None;
        }
        alignments.push(table_column_align(cell));
    }
    Some(alignments)
}

/// Columns of leading spaces and tabs. A tab advances to the next multiple of 4.
fn leading_columns(line: &str) -> usize {
    let mut columns = 0;
    for byte in line.bytes() {
        match byte {
            b' ' => columns += 1,
            b'\t' => columns += 4 - (columns % 4),
            _ => break,
        }
    }
    columns
}

/// Text after a CommonMark indent of 0–3 columns.
/// `None` when the line is indented four or more, which is indented code.
fn after_block_indent(line: &str) -> Option<&str> {
    let bytes = line.as_bytes();
    let mut index = 0;
    let mut column = 0;
    while index < bytes.len() {
        match bytes[index] {
            b' ' => {
                if column >= 3 {
                    return None;
                }
                column += 1;
                index += 1;
            }
            b'\t' => {
                if column >= 3 {
                    return None;
                }
                let next = column + 4 - (column % 4);
                if next > 3 {
                    return None;
                }
                column = next;
                index += 1;
            }
            _ => return Some(&line[index..]),
        }
    }
    Some(&line[index..])
}

fn is_blank_line(line: &str) -> bool {
    line.bytes()
        .all(|byte| matches!(byte, b' ' | b'\t' | b'\r'))
}

/// CommonMark §4.1. Three or more of one marker, spaces and tabs allowed between them.
fn is_thematic_break(line: &str) -> bool {
    let Some(body) = after_block_indent(line) else {
        return false;
    };
    let bytes = body.as_bytes();
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

/// CommonMark §4.2. One to six `#`, then whitespace or the end of the line.
fn is_atx_heading_line(line: &str) -> bool {
    let Some(body) = after_block_indent(line) else {
        return false;
    };
    let bytes = body.as_bytes();
    let mut level = 0;
    while level < bytes.len() && bytes[level] == b'#' {
        level += 1;
    }
    if !(1..=6).contains(&level) {
        return false;
    }
    level == bytes.len() || matches!(bytes[level], b' ' | b'\t' | b'\r')
}

/// CommonMark §4.5. An opening fence is three or more backticks or tildes.
/// A backtick info string cannot itself contain a backtick.
fn opening_fence(line: &str) -> Option<(u8, usize)> {
    let body = after_block_indent(line)?;
    let bytes = body.as_bytes();
    let marker = *bytes.first()?;
    if marker != b'`' && marker != b'~' {
        return None;
    }
    let mut length = 0;
    while length < bytes.len() && bytes[length] == marker {
        length += 1;
    }
    if length < 3 {
        return None;
    }
    if marker == b'`' && bytes[length..].contains(&b'`') {
        return None;
    }
    Some((marker, length))
}

/// CommonMark §4.5 closing fence: the same marker, at least as long, then only whitespace.
fn closes_fence(line: &str, marker: u8, length: usize) -> bool {
    let Some(body) = after_block_indent(line) else {
        return false;
    };
    let bytes = body.as_bytes();
    let mut count = 0;
    while count < bytes.len() && bytes[count] == marker {
        count += 1;
    }
    count >= length
        && bytes[count..]
            .iter()
            .all(|byte| matches!(byte, b' ' | b'\t' | b'\r'))
}

/// CommonMark §5.1. A `>` marker, with or without a following space.
fn is_blockquote_marker(line: &str) -> bool {
    after_block_indent(line).is_some_and(|body| body.as_bytes().first() == Some(&b'>'))
}

/// CommonMark §5.2 and §5.3. Bullet or 1–9 digit `.` / `)` marker, then whitespace or end.
fn is_list_marker_line(line: &str) -> bool {
    let Some(body) = after_block_indent(line) else {
        return false;
    };
    let bytes = body.as_bytes();
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

/// CommonMark §4.6 block-tag names for HTML block type 6.
fn is_html_block_tag(name: &[u8]) -> bool {
    const TAGS: [&str; 62] = [
        "address",
        "article",
        "aside",
        "base",
        "basefont",
        "blockquote",
        "body",
        "caption",
        "center",
        "col",
        "colgroup",
        "dd",
        "details",
        "dialog",
        "dir",
        "div",
        "dl",
        "dt",
        "fieldset",
        "figcaption",
        "figure",
        "footer",
        "form",
        "frame",
        "frameset",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "head",
        "header",
        "hr",
        "html",
        "iframe",
        "legend",
        "li",
        "link",
        "main",
        "menu",
        "menuitem",
        "nav",
        "noframes",
        "ol",
        "optgroup",
        "option",
        "p",
        "param",
        "search",
        "section",
        "summary",
        "table",
        "tbody",
        "td",
        "tfoot",
        "th",
        "thead",
        "title",
        "tr",
        "track",
        "ul",
    ];
    let mut lower = [0u8; 16];
    if name.is_empty() || name.len() > lower.len() {
        return false;
    }
    for (slot, byte) in lower.iter_mut().zip(name.iter()) {
        *slot = byte.to_ascii_lowercase();
    }
    let Ok(key) = std::str::from_utf8(&lower[..name.len()]) else {
        return false;
    };
    TAGS.binary_search(&key).is_ok()
}

/// CommonMark §4.6 types 1–6. Assumes `rest` is the line after the leading `<`.
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

/// CommonMark §4.6. An HTML block of type 1–6, indented 0–3 columns.
fn is_html_block_start(line: &str) -> bool {
    let Some(body) = after_block_indent(line) else {
        return false;
    };
    let bytes = body.as_bytes();
    bytes.first() == Some(&b'<') && html_block_type_1_to_6(&bytes[1..])
}

/// Every leading space and tab, matching pulldown `LineStart::scan_all_space`.
fn strip_all_space_and_tab(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() && matches!(bytes[index], b' ' | b'\t') {
        index += 1;
    }
    &line[index..]
}

/// ASCII punctuation from CommonMark: U+0021–002F, U+003A–0040, U+005B–0060, U+007B–007E.
fn is_ascii_punctuation(byte: u8) -> bool {
    matches!(byte, b'!'..=b'/' | b':'..=b'@' | b'['..=b'`' | b'{'..=b'~')
}

/// pulldown `is_ascii_whitespace`: U+0009 through U+000D, or a space.
fn is_pulldown_whitespace(byte: u8) -> bool {
    (0x09..=0x0d).contains(&byte) || byte == b' '
}

/// Byte length of a footnote label after the opening `[^`, including the closing `]`.
/// Mirrors pulldown `scan_link_label_rest` with the table-row linebreak handler,
/// which refuses a line break. A whitespace-only label is not a label.
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

/// `[^label]:` when footnotes are enabled. The HTML exporter sets `ENABLE_FOOTNOTES`.
/// GFM 0.29 mentions footnotes only in the introduction and has no definition section.
/// pulldown `scan_paragraph_interrupt_no_table` ends the row on this form.
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

/// A body line whose cells are empty. pulldown `parse_table_row_inner` returns no
/// row when the line, after an optional pipe, is only the whitespace
/// `scan_whitespace_no_nl` accepts (space, tab, vertical tab, form feed).
/// A vertical tab is not a blank line: GFM blank lines are "only spaces or tabs".
fn is_empty_table_row(line: &str) -> bool {
    table_cell_ranges(line).is_empty()
        || line
            .bytes()
            .all(|byte| matches!(byte, b' ' | b'\t' | 0x0b | 0x0c | b'\r'))
}

/// GFM 0.29, Tables: "The table is broken at the first empty line, or beginning
/// of another block-level structure."
///
/// pulldown `parse_table_row` calls `scan_all_space`, which consumes every leading
/// space and tab, then `scan_paragraph_interrupt_no_table` on what remains.
/// GFM 0.29, Indented code: "An indented code block cannot interrupt a paragraph,
/// so there must be a blank line between a paragraph and a following indented
/// code block." A plain indented line therefore stays a row. A thematic break,
/// ATX heading, fence, quote, list marker, HTML block of type 1–6, or footnote
/// definition on the remainder ends the table.
fn line_breaks_table(line: &str) -> bool {
    if is_blank_line(line) {
        return true;
    }
    let body = strip_all_space_and_tab(line);
    is_thematic_break(body)
        || is_atx_heading_line(body)
        || opening_fence(body).is_some()
        || is_blockquote_marker(body)
        || is_list_marker_line(body)
        || is_html_block_start(body)
        || is_footnote_definition(body)
}

struct TableBlock {
    last: usize,
    delimiter: usize,
    alignments: Vec<TableColumnAlign>,
}

struct DetectedTable {
    first: usize,
    last: usize,
    delimiter: usize,
    source: std::ops::Range<usize>,
    alignments: Vec<TableColumnAlign>,
}

/// A table is a header row plus the next line as a delimiter of the same width.
/// The header needs an unescaped pipe outside code spans, and an indent under 4.
/// The delimiter row is the same: GFM's delimiter row is hyphen cells, and a line
/// indented four or more spaces cannot interrupt the paragraph the header started.
/// pulldown `scan_table_head` rejects `calc_indent` spaces greater than 3.
/// Later lines stay in the table until a blank line, another block, or a row with no cells.
fn table_block(lines: &[SourceLine<'_>], start: usize) -> Option<TableBlock> {
    if start + 1 >= lines.len() || line_breaks_table(lines[start].text) {
        return None;
    }
    let header = lines[start].text;
    if leading_columns(header) >= 4 || unescaped_pipe_offsets(header).is_empty() {
        return None;
    }
    let header_cells = table_cell_ranges(header);
    if leading_columns(lines[start + 1].text) >= 4 {
        return None;
    }
    let alignments = delimiter_alignments(lines[start + 1].text)?;
    if header_cells.is_empty() || alignments.len() != header_cells.len() {
        return None;
    }
    let mut last = start + 1;
    let mut index = start + 2;
    while index < lines.len()
        && !line_breaks_table(lines[index].text)
        && !is_empty_table_row(lines[index].text)
    {
        last = index;
        index += 1;
    }
    Some(TableBlock {
        last,
        delimiter: start + 1,
        alignments,
    })
}

/// `---` is unset, `:---` is left, `:---:` is center, and `---:` is right.
pub fn table_column_align(cell: &str) -> TableColumnAlign {
    let cell = cell.trim();
    if !cell.contains('-') {
        return TableColumnAlign::None;
    }
    match (cell.starts_with(':'), cell.ends_with(':')) {
        (true, true) => TableColumnAlign::Center,
        (true, false) => TableColumnAlign::Left,
        (false, true) => TableColumnAlign::Right,
        (false, false) => TableColumnAlign::None,
    }
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
        /// Byte ranges of each cell's editable content in the document
        cells: Vec<std::ops::Range<usize>>,
        /// Full inner byte ranges between pipes (`pipe[k]+1..pipe[k+1]`)
        raw_cells: Vec<std::ops::Range<usize>>,
        /// Column alignments parsed from the table's delimiter row
        alignments: Vec<TableColumnAlign>,
        /// Full table block byte range in the document
        table_source: std::ops::Range<usize>,
    },
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
        let mut display = String::with_capacity(line_text.len());
        let mut segments = Vec::new();
        let mut cursor = line_start;

        for h in hidden {
            let h_start = h.start.clamp(line_start, line_end);
            let h_end = h.end.clamp(line_start, line_end);
            if h_start > cursor {
                let slice = &line_text[(cursor - line_start)..(h_start - line_start)];
                let disp_start = display.len();
                display.push_str(slice);
                let disp_end = display.len();
                segments.push((disp_start..disp_end, cursor..h_start));
            }
            cursor = cursor.max(h_end);
        }

        if cursor < line_end {
            let slice = &line_text[(cursor - line_start)..(line_end - line_start)];
            let disp_start = display.len();
            display.push_str(slice);
            let disp_end = display.len();
            segments.push((disp_start..disp_end, cursor..line_end));
        }

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
            if overlap_end > overlap_start
                || (d.start == d.end && d.start >= disp_range.start && d.start <= disp_range.end)
            {
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
    pub offset_map: RowOffsetMap,
    pub styled_segments: Vec<StyledSegment>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedDocument {
    pub lines: Vec<ParsedLine>,
    /// All hidden byte ranges across the entire document (sorted, non-overlapping)
    pub hidden_ranges: Vec<std::ops::Range<usize>>,
    /// All inline delimiter pairs across the document
    pub inline_pairs: Vec<InlinePair>,
    /// Fenced code block ranges `(open_fence_line_range, close_fence_line_range)`
    pub code_blocks: Vec<(std::ops::Range<usize>, std::ops::Range<usize>)>,
}

impl ParsedDocument {
    pub fn line_at_offset(&self, offset: usize) -> Option<&ParsedLine> {
        self.lines
            .iter()
            .find(|l| offset >= l.source.start && offset <= l.source.end)
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

fn parse_inline_syntax(
    input: &str,
    base: usize,
) -> (
    Vec<std::ops::Range<usize>>,
    Vec<InlinePair>,
    Vec<StyledSegment>,
) {
    if input.is_empty() {
        return (Vec::new(), Vec::new(), Vec::new());
    }

    let mut options = pulldown_cmark::Options::empty();
    options.insert(pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
    let parser = pulldown_cmark::Parser::new_ext(input, options).into_offset_iter();

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
                        inline_pairs.push(InlinePair {
                            kind: InlineTagKind::Bold,
                            open_delim: open_d,
                            inner,
                            close_delim: close_d,
                        });
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
                        inline_pairs.push(InlinePair {
                            kind: InlineTagKind::Italic,
                            open_delim: open_d,
                            inner,
                            close_delim: close_d,
                        });
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
                        inline_pairs.push(InlinePair {
                            kind: InlineTagKind::Strike,
                            open_delim: open_d,
                            inner,
                            close_delim: close_d,
                        });
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
                    inline_pairs.push(InlinePair {
                        kind: InlineTagKind::Link,
                        open_delim: open_d,
                        inner,
                        close_delim: close_d,
                    });
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
                        inline_pairs.push(InlinePair {
                            kind: InlineTagKind::Underline,
                            open_delim: open_d,
                            inner,
                            close_delim: close_d,
                        });
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
                inline_pairs.push(InlinePair {
                    kind: InlineTagKind::Code,
                    open_delim: open_d,
                    inner: inner.clone(),
                    close_delim: close_d,
                });
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

    (raw_hidden, inline_pairs, styled_segments)
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
        if markdown.ends_with('\n') {
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
    let mut all_hidden = Vec::new();
    let mut all_pairs = Vec::new();
    let mut code_blocks = Vec::new();

    let mut in_code_block = false;
    let mut open_fence_range: Option<std::ops::Range<usize>> = None;

    // Header plus a same-width delimiter. Body rows stop at the next block.
    let mut table_ranges: Vec<DetectedTable> = Vec::new();
    {
        let mut idx = 0;
        let mut fence: Option<(u8, usize)> = None;
        while idx < raw_lines.len() {
            let text = raw_lines[idx].text;
            if let Some((marker, length)) = fence {
                if closes_fence(text, marker, length) {
                    fence = None;
                }
                idx += 1;
                continue;
            }
            if let Some(opened) = opening_fence(text) {
                fence = Some(opened);
                idx += 1;
                continue;
            }
            if let Some(table) = table_block(&raw_lines, idx) {
                let byte_start = raw_lines[idx].start;
                let last_line = &raw_lines[table.last];
                let byte_end = last_line.start + last_line.text.len();
                table_ranges.push(DetectedTable {
                    first: idx,
                    last: table.last,
                    delimiter: table.delimiter,
                    source: byte_start..byte_end,
                    alignments: table.alignments,
                });
                idx = table.last + 1;
            } else {
                idx += 1;
            }
        }
    }

    for (i, sline) in raw_lines.iter().enumerate() {
        let line = sline.text;
        let line_start = sline.start;
        let line_end = line_start + line.len();
        let line_source = line_start..line_end;
        let trimmed = line.trim();
        let trimmed_start = line.trim_start();
        let indent_len = line.len() - trimmed_start.len();

        // Check code block fences. Same `opening_fence` as `table_block`:
        // a backtick info string may not contain a backtick (GFM fenced code).
        if opening_fence(line).is_some_and(|(marker, _)| marker == b'`') {
            let full_with_nl = sline.start..sline.end;
            if !in_code_block {
                in_code_block = true;
                open_fence_range = Some(full_with_nl);
                let lang = trimmed
                    .strip_prefix("```")
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                let hidden = if line_end > line_start {
                    vec![line_start..line_end]
                } else {
                    Vec::new()
                };
                all_hidden.extend(hidden.iter().cloned());
                let (display_text, offset_map) =
                    RowOffsetMap::from_hidden(line, line_start, line_end, &hidden);
                lines.push(ParsedLine {
                    line_index: i,
                    source: line_source,
                    kind: LineKind::CodeFence {
                        language: lang,
                        opening: true,
                    },
                    prefix_range: line_start..line_end,
                    hidden_ranges: hidden,
                    inline_pairs: Vec::new(),
                    display_text,
                    offset_map,
                    styled_segments: Vec::new(),
                });
            } else {
                in_code_block = false;
                if let Some(open_r) = open_fence_range.take() {
                    code_blocks.push((open_r, full_with_nl));
                }
                let hidden = if line_end > line_start {
                    vec![line_start..line_end]
                } else {
                    Vec::new()
                };
                all_hidden.extend(hidden.iter().cloned());
                let (display_text, offset_map) =
                    RowOffsetMap::from_hidden(line, line_start, line_end, &hidden);
                lines.push(ParsedLine {
                    line_index: i,
                    source: line_source,
                    kind: LineKind::CodeFence {
                        language: None,
                        opening: false,
                    },
                    prefix_range: line_start..line_end,
                    hidden_ranges: hidden,
                    inline_pairs: Vec::new(),
                    display_text,
                    offset_map,
                    styled_segments: Vec::new(),
                });
            }
            continue;
        }

        if in_code_block {
            lines.push(ParsedLine {
                line_index: i,
                source: line_source.clone(),
                kind: LineKind::CodeLine,
                prefix_range: line_start..line_start,
                hidden_ranges: Vec::new(),
                inline_pairs: Vec::new(),
                display_text: line.to_string(),
                offset_map: RowOffsetMap::identity(line_start, line_end),
                styled_segments: vec![StyledSegment {
                    source: line_source,
                    bold: false,
                    italic: false,
                    underline: false,
                    strike: false,
                    code: true,
                    link_url: None,
                }],
            });
            continue;
        }

        if trimmed.is_empty() {
            lines.push(ParsedLine {
                line_index: i,
                source: line_source.clone(),
                kind: LineKind::Empty,
                prefix_range: line_start..line_start,
                hidden_ranges: Vec::new(),
                inline_pairs: Vec::new(),
                display_text: line.to_string(),
                offset_map: RowOffsetMap::identity(line_start, line_end),
                styled_segments: Vec::new(),
            });
            continue;
        }

        // Horizontal Rule
        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            let hidden = vec![line_start..line_end];
            all_hidden.extend(hidden.iter().cloned());
            let (display_text, offset_map) =
                RowOffsetMap::from_hidden(line, line_start, line_end, &hidden);
            lines.push(ParsedLine {
                line_index: i,
                source: line_source,
                kind: LineKind::HorizontalRule,
                prefix_range: line_start..line_end,
                hidden_ranges: hidden,
                inline_pairs: Vec::new(),
                display_text,
                offset_map,
                styled_segments: Vec::new(),
            });
            continue;
        }

        // Table rows. Only the delimiter line is hidden; cells follow its width.
        if let Some(table) = table_ranges
            .iter()
            .find(|table| i >= table.first && i <= table.last)
        {
            if i == table.delimiter {
                let hidden = vec![line_start..line_end];
                all_hidden.extend(hidden.iter().cloned());
                let (display_text, offset_map) =
                    RowOffsetMap::from_hidden(line, line_start, line_end, &hidden);
                lines.push(ParsedLine {
                    line_index: i,
                    source: line_source,
                    kind: LineKind::TableDelimiter,
                    prefix_range: line_start..line_end,
                    hidden_ranges: hidden,
                    inline_pairs: Vec::new(),
                    display_text,
                    offset_map,
                    styled_segments: Vec::new(),
                });
                continue;
            } else {
                let is_header = i == table.first;
                let cols = table.alignments.len();
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
                all_hidden.extend(merged_hidden.iter().cloned());
                all_pairs.extend(row_pairs.iter().cloned());
                let alignments = table.alignments.clone();
                lines.push(ParsedLine {
                    line_index: i,
                    source: line_source.clone(),
                    kind: LineKind::TableRow {
                        is_header,
                        cells,
                        raw_cells,
                        alignments,
                        table_source: table.source.clone(),
                    },
                    prefix_range: line_start..line_start,
                    hidden_ranges: merged_hidden,
                    inline_pairs: row_pairs,
                    display_text: line.to_string(),
                    offset_map: RowOffsetMap::identity(line_start, line_end),
                    styled_segments: row_segments,
                });
                continue;
            }
        }

        // Determine block kind & prefix length
        let mut kind = LineKind::Paragraph;
        let mut prefix_len = 0usize;

        if indent_len <= 3 {
            for (lvl, pfx) in [
                (6u8, "###### "),
                (5, "##### "),
                (4, "#### "),
                (3, "### "),
                (2, "## "),
                (1, "# "),
            ] {
                if trimmed_start.starts_with(pfx) {
                    kind = LineKind::Heading { level: lvl };
                    prefix_len = indent_len + pfx.len();
                    break;
                }
            }
        }

        if prefix_len == 0 {
            if let Some(_rest) = trimmed_start
                .strip_prefix("- [ ] ")
                .or_else(|| trimmed_start.strip_prefix("* [ ] "))
                .or_else(|| trimmed_start.strip_prefix("+ [ ] "))
            {
                let cb_start = line_start + indent_len + 2;
                kind = LineKind::ListItem {
                    ordered: false,
                    number: 0,
                    task: Some(false),
                    indent_cols: indent_len,
                    checkbox_source: Some(cb_start..(cb_start + 3)),
                };
                prefix_len = indent_len + 6;
            } else if let Some(_rest) = trimmed_start
                .strip_prefix("- [x] ")
                .or_else(|| trimmed_start.strip_prefix("- [X] "))
                .or_else(|| trimmed_start.strip_prefix("* [x] "))
                .or_else(|| trimmed_start.strip_prefix("* [X] "))
                .or_else(|| trimmed_start.strip_prefix("+ [x] "))
                .or_else(|| trimmed_start.strip_prefix("+ [X] "))
            {
                let cb_start = line_start + indent_len + 2;
                kind = LineKind::ListItem {
                    ordered: false,
                    number: 0,
                    task: Some(true),
                    indent_cols: indent_len,
                    checkbox_source: Some(cb_start..(cb_start + 3)),
                };
                prefix_len = indent_len + 6;
            } else if trimmed_start.starts_with("- ")
                || trimmed_start.starts_with("* ")
                || trimmed_start.starts_with("+ ")
            {
                kind = LineKind::ListItem {
                    ordered: false,
                    number: 0,
                    task: None,
                    indent_cols: indent_len,
                    checkbox_source: None,
                };
                prefix_len = indent_len + 2;
            } else if trimmed_start.starts_with("> ") {
                kind = LineKind::Blockquote;
                prefix_len = indent_len + 2;
            } else if let Some(dot_pos) = trimmed_start.find(". ") {
                if dot_pos > 0 && trimmed_start[..dot_pos].chars().all(|c| c.is_ascii_digit()) {
                    if let Ok(num) = trimmed_start[..dot_pos].parse::<usize>() {
                        kind = LineKind::ListItem {
                            ordered: true,
                            number: num,
                            task: None,
                            indent_cols: indent_len,
                            checkbox_source: None,
                        };
                        prefix_len = indent_len + dot_pos + 2;
                    }
                }
            }
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
        all_hidden.extend(merged_hidden.iter().cloned());
        all_pairs.extend(inline_pairs.iter().cloned());

        let (display_text, offset_map) =
            RowOffsetMap::from_hidden(line, line_start, body_base, &merged_hidden);

        lines.push(ParsedLine {
            line_index: i,
            source: line_source,
            kind,
            prefix_range,
            hidden_ranges: merged_hidden,
            inline_pairs,
            display_text,
            offset_map,
            styled_segments,
        });
    }

    ParsedDocument {
        lines,
        hidden_ranges: merge_ranges(all_hidden),
        inline_pairs: all_pairs,
        code_blocks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_headings_and_hr() {
        let md = "# Title\n## Subtitle\n### Section\n---";
        let blocks = parse_markdown(md);
        assert_eq!(blocks.len(), 4);
        match &blocks[0] {
            Block::Heading { level, .. } => assert_eq!(*level, 1),
            _ => panic!("Expected H1"),
        }
        match &blocks[1] {
            Block::Heading { level, .. } => assert_eq!(*level, 2),
            _ => panic!("Expected H2"),
        }
        match &blocks[2] {
            Block::Heading { level, .. } => assert_eq!(*level, 3),
            _ => panic!("Expected H3"),
        }
        assert!(matches!(blocks[3], Block::HorizontalRule { .. }));
    }

    #[test]
    fn test_spans_formatting() {
        let input = "Normal **bold** *italic* <u>underlined</u> ~~strike~~ `code`";
        let spans = parse_spans(input);
        assert!(!spans.is_empty());
        let has_bold = spans.iter().any(|s| s.bold && s.text.contains("bold"));
        let has_italic = spans.iter().any(|s| s.italic && s.text.contains("italic"));
        let has_underline = spans
            .iter()
            .any(|s| s.underline && s.text.contains("underlined"));
        let has_strike = spans.iter().any(|s| s.strike && s.text.contains("strike"));
        let has_code = spans.iter().any(|s| s.code && s.text.contains("code"));

        assert!(has_bold, "Expected bold span");
        assert!(has_italic, "Expected italic span");
        assert!(has_underline, "Expected underline span");
        assert!(has_strike, "Expected strike span");
        assert!(has_code, "Expected code span");
    }

    #[test]
    fn test_styled_segments_bold() {
        let input = "Normal **bold** text";
        let doc = parse_document(input);
        assert_eq!(doc.lines.len(), 1);
        let line = &doc.lines[0];
        println!("display: {:?}", line.display_text);
        println!("segments: {:?}", line.styled_segments);
        let bold_seg = line.styled_segments.iter().find(|s| s.bold);
        assert!(bold_seg.is_some(), "Expected bold styled segment");
    }

    #[test]
    fn test_task_list() {
        let md = "- [ ] Task to do\n- [x] Completed task";
        let blocks = parse_markdown(md);
        assert_eq!(blocks.len(), 2);
        match &blocks[0] {
            Block::ListItem {
                is_task, checked, ..
            } => {
                assert!(is_task);
                assert!(!checked);
            }
            _ => panic!("Expected task item"),
        }
        match &blocks[1] {
            Block::ListItem {
                is_task, checked, ..
            } => {
                assert!(is_task);
                assert!(checked);
            }
            _ => panic!("Expected completed task item"),
        }
    }

    #[test]
    fn escaped_pipe_is_not_a_column_break() {
        assert_eq!(
            split_table_cells(r"| a \| b | c |"),
            vec!["a \\| b".to_string(), "c".to_string()]
        );
        assert_eq!(table_column_align("---"), TableColumnAlign::None);
        assert_eq!(table_column_align(":---"), TableColumnAlign::Left);
        assert_eq!(table_column_align(":---:"), TableColumnAlign::Center);
        assert_eq!(table_column_align("---:"), TableColumnAlign::Right);

        let md = "| a \\| b | c |\n| --- | --- |\n| d | e |";
        let doc = parse_document(md);
        match &doc.lines[0].kind {
            LineKind::TableRow {
                cells, alignments, ..
            } => {
                assert_eq!(cells.len(), 2);
                assert_eq!(&md[cells[0].clone()], "a \\| b");
                assert_eq!(&md[cells[1].clone()], "c");
                assert_eq!(
                    alignments,
                    &[TableColumnAlign::None, TableColumnAlign::None]
                );
            }
            other => panic!("expected a table row, got {other:?}"),
        }
    }

    #[test]
    fn test_table_parsing() {
        let md = "| Col 1 | Col 2 |\n| :--- | :--- |\n| Val A | Val B |";
        let blocks = parse_markdown(md);
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            Block::Table { headers, rows, .. } => {
                assert_eq!(headers.len(), 2);
                assert_eq!(headers[0], "Col 1");
                assert_eq!(headers[1], "Col 2");
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "Val A");
                assert_eq!(rows[0][1], "Val B");
            }
            _ => panic!("Expected table block"),
        }
    }

    #[test]
    fn test_parse_document_table() {
        let md = "| **Col 1** | Col 2 |\n| :--- | :--- |\n| Val A | ~~Val B~~ |";
        let doc = parse_document(md);
        assert_eq!(doc.lines.len(), 3);
        assert!(matches!(
            doc.lines[0].kind,
            LineKind::TableRow {
                is_header: true,
                ..
            }
        ));
        assert!(matches!(doc.lines[1].kind, LineKind::TableDelimiter));
        assert!(matches!(
            doc.lines[2].kind,
            LineKind::TableRow {
                is_header: false,
                ..
            }
        ));
        assert_eq!(doc.lines[0].display_text, "| **Col 1** | Col 2 |");
        // Verify Col 1 is bold (via **), but Col 2 is not forced bold
        let bold_segs: Vec<_> = doc.lines[0]
            .styled_segments
            .iter()
            .filter(|s| s.bold)
            .collect();
        assert!(!bold_segs.is_empty());
        let normal_segs: Vec<_> = doc.lines[0]
            .styled_segments
            .iter()
            .filter(|s| !s.bold)
            .collect();
        assert!(!normal_segs.is_empty());
    }

    #[test]
    fn rendered_click_maps_to_source_and_strike_is_marked() {
        let md = "# Hi\n\n~~gone~~";
        let blocks = parse_markdown(md);
        let offset = source_offset_for_click(md, 0, 1).unwrap();
        assert!(md.is_char_boundary(offset));
        assert_eq!(&md[offset..offset + 1], "i");
        let strike = blocks.iter().find_map(|block| match block {
            Block::Paragraph { spans, .. } => spans.iter().find(|span| span.text.contains("gone")),
            _ => None,
        });
        assert!(strike.unwrap().strike);

        let bullet = source_offset_for_click("- alpha", 0, 0).unwrap();
        assert_eq!(&"- alpha"[bullet..bullet + 1], "a");
    }

    #[test]
    fn test_row_offset_map_roundtrip_and_multibyte() {
        let md = "# Hello **world** and [LiteMark](https://example.com) 😀កខ";
        let parsed = parse_document(md);
        assert_eq!(parsed.lines.len(), 1);
        let line = &parsed.lines[0];
        assert_eq!(line.display_text, "Hello world and LiteMark 😀កខ");

        // Display offset 0 -> 'H' at source byte 2 (after "# ")
        assert_eq!(line.offset_map.display_to_source(0), 2);
        assert_eq!(line.offset_map.source_to_display(2), 0);

        // Display offset 6 -> 'w' in "**world**" (source byte 10)
        assert_eq!(line.offset_map.display_to_source(6), 10);
        assert_eq!(line.offset_map.source_to_display(10), 6);

        // Display offset 16 -> 'L' in "[LiteMark]" (source byte 23)
        assert_eq!(line.offset_map.display_to_source(16), 23);
        assert_eq!(line.offset_map.source_to_display(23), 16);

        // Check every char boundary in display_text maps to a valid char boundary in md
        for (disp_idx, ch) in line.display_text.char_indices() {
            let src_idx = line.offset_map.display_to_source(disp_idx);
            assert!(md.is_char_boundary(src_idx));
            assert_eq!(md[src_idx..].chars().next(), Some(ch));
            assert_eq!(line.offset_map.source_to_display(src_idx), disp_idx);
        }
    }

    fn shape_of(markdown: &str) -> String {
        let doc = parse_document(markdown);
        let mut out = String::new();
        for (index, line) in doc.lines.iter().enumerate() {
            let code = line
                .styled_segments
                .iter()
                .filter(|segment| segment.code)
                .map(|segment| markdown[segment.source.clone()].to_string())
                .collect::<Vec<_>>();
            let body = match &line.kind {
                LineKind::Empty => "Empty".to_string(),
                LineKind::Paragraph => "Paragraph".to_string(),
                LineKind::Heading { level } => format!("Heading level={level}"),
                LineKind::Blockquote => "Blockquote".to_string(),
                LineKind::ListItem { .. } => "ListItem".to_string(),
                LineKind::CodeFence { .. } => "CodeFence".to_string(),
                LineKind::CodeLine => "CodeLine".to_string(),
                LineKind::HorizontalRule => "HorizontalRule".to_string(),
                LineKind::TableDelimiter => "TableDelimiter".to_string(),
                LineKind::TableRow {
                    is_header,
                    cells,
                    alignments,
                    ..
                } => {
                    let texts = cells
                        .iter()
                        .map(|range| markdown[range.clone()].to_string())
                        .collect::<Vec<_>>();
                    let align = alignments
                        .iter()
                        .map(|align| match align {
                            TableColumnAlign::None => "none",
                            TableColumnAlign::Left => "left",
                            TableColumnAlign::Center => "center",
                            TableColumnAlign::Right => "right",
                        })
                        .collect::<Vec<_>>()
                        .join(",");
                    format!("TableRow header={is_header} align=[{align}] cells={texts:?}")
                }
            };
            out.push_str(&format!(
                "line[{index}] {body} display={:?} code={code:?}\n",
                line.display_text
            ));
        }
        out
    }

    fn shape_mismatch(name: &str, markdown: &str, lines: &[&str]) -> Option<String> {
        let mut expected = String::new();
        for line in lines {
            expected.push_str(line);
            expected.push('\n');
        }
        let got = shape_of(markdown);
        if got == expected {
            None
        } else {
            Some(format!(
                "case {name}\n--- expected ---\n{expected}--- got ---\n{got}"
            ))
        }
    }

    #[test]
    fn gfm_table_columns_follow_the_delimiter() {
        let mut mismatches = Vec::new();
        let mut check = |name: &str, markdown: &str, lines: &[&str]| {
            if let Some(mismatch) = shape_mismatch(name, markdown, lines) {
                mismatches.push(mismatch);
            }
        };
        check(
            "gfm198",
            "| foo | bar |\n| --- | --- |\n| baz | bim |",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["foo", "bar"] display="| foo | bar |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["baz", "bim"] display="| baz | bim |" code=[]"#,
            ],
        );
        check(
            "gfm199",
            "| abc | defghi |\n:-: | -----------:\nbar | baz",
            &[
                r#"line[0] TableRow header=true align=[center,right] cells=["abc", "defghi"] display="| abc | defghi |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[center,right] cells=["bar", "baz"] display="bar | baz" code=[]"#,
            ],
        );
        check(
            "gfm200",
            "| f\\|oo  |\n| ------ |\n| b `\\|` az |\n| b **\\|** im |",
            &[
                r#"line[0] TableRow header=true align=[none] cells=["f\\|oo "] display="| f\\|oo  |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none] cells=["b `\\|` az"] display="| b `\\|` az |" code=["\\|"]"#,
                r#"line[3] TableRow header=false align=[none] cells=["b **\\|** im"] display="| b **\\|** im |" code=[]"#,
            ],
        );
        check(
            "gfm203",
            "| abc | def |\n| --- |\n| bar |",
            &[
                r#"line[0] Paragraph display="| abc | def |" code=[]"#,
                r#"line[1] Paragraph display="| --- |" code=[]"#,
                r#"line[2] Paragraph display="| bar |" code=[]"#,
            ],
        );
        check(
            "gfm204",
            "| abc | def |\n| --- | --- |\n| bar |\n| bar | baz | boo |",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["abc", "def"] display="| abc | def |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["bar", ""] display="| bar |" code=[]"#,
                r#"line[3] TableRow header=false align=[none,none] cells=["bar", "baz"] display="| bar | baz | boo |" code=[]"#,
            ],
        );
        check(
            "code-pipe",
            "| a | b |\n| --- | --- |\n| `c|d` | e |",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["`c|d`", "e"] display="| `c|d` | e |" code=["c|d"]"#,
            ],
        );
        check(
            "pipe-row-no-delim",
            "| a | b |\n| c | d |",
            &[
                r#"line[0] Paragraph display="| a | b |" code=[]"#,
                r#"line[1] Paragraph display="| c | d |" code=[]"#,
            ],
        );
        drop(check);
        assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
    }

    fn align_label(align: &TableColumnAlign) -> &'static str {
        match align {
            TableColumnAlign::None => "none",
            TableColumnAlign::Left => "left",
            TableColumnAlign::Center => "center",
            TableColumnAlign::Right => "right",
        }
    }

    fn pulldown_align(align: &pulldown_cmark::Alignment) -> &'static str {
        match align {
            pulldown_cmark::Alignment::None => "none",
            pulldown_cmark::Alignment::Left => "left",
            pulldown_cmark::Alignment::Center => "center",
            pulldown_cmark::Alignment::Right => "right",
        }
    }

    fn exporter_table_options() -> pulldown_cmark::Options {
        let mut options = pulldown_cmark::Options::empty();
        options.insert(pulldown_cmark::Options::ENABLE_TABLES);
        options.insert(pulldown_cmark::Options::ENABLE_FOOTNOTES);
        options.insert(pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
        options.insert(pulldown_cmark::Options::ENABLE_TASKLISTS);
        options
    }

    /// Column count, body-plus-header rows, alignments, and the in-table line mask.
    fn editor_table_membership(markdown: &str) -> (String, Vec<bool>) {
        let doc = parse_document(markdown);
        let mut tables = Vec::new();
        let mut mask = Vec::new();
        let mut cols = 0usize;
        let mut rows = 0usize;
        let mut align = String::new();
        let mut in_table = false;
        for line in &doc.lines {
            let table_line = matches!(
                line.kind,
                LineKind::TableRow { .. } | LineKind::TableDelimiter
            );
            mask.push(table_line);
            if table_line && !in_table {
                in_table = true;
                rows = 0;
            }
            if let LineKind::TableRow { alignments, .. } = &line.kind {
                cols = alignments.len();
                rows += 1;
                align = alignments
                    .iter()
                    .map(align_label)
                    .collect::<Vec<_>>()
                    .join(",");
            }
            if in_table && !table_line {
                tables.push(format!("cols={cols} rows={rows} align={align}"));
                in_table = false;
            }
        }
        if in_table {
            tables.push(format!("cols={cols} rows={rows} align={align}"));
        }
        let sig = if tables.is_empty() {
            "none".to_string()
        } else {
            tables.join(" | ")
        };
        (sig, mask)
    }

    fn pulldown_table_membership(markdown: &str) -> (String, Vec<bool>) {
        let doc = parse_document(markdown);
        let mut mask = vec![false; doc.lines.len()];
        let mut tables = Vec::new();
        let mut current: Option<(usize, String, usize)> = None;
        let events =
            pulldown_cmark::Parser::new_ext(markdown, exporter_table_options()).into_offset_iter();
        for (event, range) in events {
            match event {
                pulldown_cmark::Event::Start(pulldown_cmark::Tag::Table(aligns)) => {
                    let align = aligns
                        .iter()
                        .map(pulldown_align)
                        .collect::<Vec<_>>()
                        .join(",");
                    current = Some((aligns.len(), align, 0));
                    for (index, line) in doc.lines.iter().enumerate() {
                        if line.source.start < range.end && line.source.end > range.start {
                            mask[index] = true;
                        }
                    }
                }
                pulldown_cmark::Event::Start(pulldown_cmark::Tag::TableHead)
                | pulldown_cmark::Event::Start(pulldown_cmark::Tag::TableRow) => {
                    if let Some(table) = current.as_mut() {
                        table.2 += 1;
                    }
                }
                pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Table) => {
                    if let Some((cols, align, rows)) = current.take() {
                        tables.push(format!("cols={cols} rows={rows} align={align}"));
                    }
                }
                _ => {}
            }
        }
        let sig = if tables.is_empty() {
            "none".to_string()
        } else {
            tables.join(" | ")
        };
        (sig, mask)
    }

    /// Exact kinds and cells for the checker cases plus GFM examples 198–205.
    /// pulldown-cmark 0.12.2 with the exporter options is the oracle for which
    /// lines belong to a table, and for the column count and alignment.
    /// `double-backslash-pipe` stays a table: GFM says "If a backslash is itself
    /// escaped, the following character is not", so an even run leaves a real pipe.
    /// A delimiter indented four or more columns does not open a table.
    #[test]
    fn table_membership_kinds_and_cells() {
        let mut mismatches = Vec::new();
        let mut check = |name: &str, markdown: &str, lines: &[&str], preserved: bool| {
            if let Some(mismatch) = shape_mismatch(name, markdown, lines) {
                mismatches.push(mismatch);
            }
            let editor = editor_table_membership(markdown);
            let pulldown = pulldown_table_membership(markdown);
            let same = editor == pulldown;
            if preserved && same {
                mismatches.push(format!(
                    "case {name} preserved divergence now matches pulldown\neditor {:?} {:?}\npulldown {:?} {:?}",
                    editor.0, editor.1, pulldown.0, pulldown.1
                ));
            } else if !preserved && !same {
                mismatches.push(format!(
                    "case {name} membership diverges from pulldown\neditor {:?} {:?}\npulldown {:?} {:?}",
                    editor.0, editor.1, pulldown.0, pulldown.1
                ));
            }
        };
        check(
            "gfm198",
            "| foo | bar |\n| --- | --- |\n| baz | bim |",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["foo", "bar"] display="| foo | bar |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["baz", "bim"] display="| baz | bim |" code=[]"#,
            ],
            false,
        );
        check(
            "gfm199",
            "| abc | defghi |\n:-: | -----------:\nbar | baz",
            &[
                r#"line[0] TableRow header=true align=[center,right] cells=["abc", "defghi"] display="| abc | defghi |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[center,right] cells=["bar", "baz"] display="bar | baz" code=[]"#,
            ],
            false,
        );
        check(
            "gfm200",
            "| f\\|oo  |\n| ------ |\n| b `\\|` az |\n| b **\\|** im |",
            &[
                r#"line[0] TableRow header=true align=[none] cells=["f\\|oo "] display="| f\\|oo  |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none] cells=["b `\\|` az"] display="| b `\\|` az |" code=["\\|"]"#,
                r#"line[3] TableRow header=false align=[none] cells=["b **\\|** im"] display="| b **\\|** im |" code=[]"#,
            ],
            false,
        );
        check(
            "gfm201",
            "| abc | def |\n| --- | --- |\n| bar | baz |\n> bar",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["abc", "def"] display="| abc | def |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["bar", "baz"] display="| bar | baz |" code=[]"#,
                r#"line[3] Blockquote display="bar" code=[]"#,
            ],
            false,
        );
        check(
            "gfm202",
            "| abc | def |\n| --- | --- |\n| bar | baz |\nbar\n\nbar",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["abc", "def"] display="| abc | def |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["bar", "baz"] display="| bar | baz |" code=[]"#,
                r#"line[3] TableRow header=false align=[none,none] cells=["bar", ""] display="bar" code=[]"#,
                r#"line[4] Empty display="" code=[]"#,
                r#"line[5] Paragraph display="bar" code=[]"#,
            ],
            false,
        );
        check(
            "gfm203",
            "| abc | def |\n| --- |\n| bar |",
            &[
                r#"line[0] Paragraph display="| abc | def |" code=[]"#,
                r#"line[1] Paragraph display="| --- |" code=[]"#,
                r#"line[2] Paragraph display="| bar |" code=[]"#,
            ],
            false,
        );
        check(
            "gfm204",
            "| abc | def |\n| --- | --- |\n| bar |\n| bar | baz | boo |",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["abc", "def"] display="| abc | def |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["bar", ""] display="| bar |" code=[]"#,
                r#"line[3] TableRow header=false align=[none,none] cells=["bar", "baz"] display="| bar | baz | boo |" code=[]"#,
            ],
            false,
        );
        check(
            "gfm205",
            "| abc | def |\n| --- | --- |",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["abc", "def"] display="| abc | def |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
            ],
            false,
        );
        check(
            "code-pipe",
            "| a | b |\n| --- | --- |\n| `c|d` | e |",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["`c|d`", "e"] display="| `c|d` | e |" code=["c|d"]"#,
            ],
            false,
        );
        check(
            "no-delim",
            "| a | b |\n| c | d |",
            &[
                r#"line[0] Paragraph display="| a | b |" code=[]"#,
                r#"line[1] Paragraph display="| c | d |" code=[]"#,
            ],
            false,
        );
        check(
            "delim-wider",
            "| a | b |\n| --- | --- | --- |\n| x | y | z |",
            &[
                r#"line[0] Paragraph display="| a | b |" code=[]"#,
                r#"line[1] Paragraph display="| --- | --- | --- |" code=[]"#,
                r#"line[2] Paragraph display="| x | y | z |" code=[]"#,
            ],
            false,
        );
        check(
            "header-wider",
            "| a | b | c |\n| --- | --- |\n| x | y |",
            &[
                r#"line[0] Paragraph display="| a | b | c |" code=[]"#,
                r#"line[1] Paragraph display="| --- | --- |" code=[]"#,
                r#"line[2] Paragraph display="| x | y |" code=[]"#,
            ],
            false,
        );
        check(
            "one-col",
            "| a |\n| --- |\n| b |",
            &[
                r#"line[0] TableRow header=true align=[none] cells=["a"] display="| a |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none] cells=["b"] display="| b |" code=[]"#,
            ],
            false,
        );
        check(
            "delim-no-pipe-dash",
            "foo\n---\nbar",
            &[
                r#"line[0] Paragraph display="foo" code=[]"#,
                r#"line[1] HorizontalRule display="" code=[]"#,
                r#"line[2] Paragraph display="bar" code=[]"#,
            ],
            false,
        );
        check(
            "delim-no-pipe-colon",
            "foo\n:-:\nbar",
            &[
                r#"line[0] Paragraph display="foo" code=[]"#,
                r#"line[1] Paragraph display=":-:" code=[]"#,
                r#"line[2] Paragraph display="bar" code=[]"#,
            ],
            false,
        );
        check(
            "piped-header-bare-colon",
            "| foo |\n:-:\nbar",
            &[
                r#"line[0] Paragraph display="| foo |" code=[]"#,
                r#"line[1] Paragraph display=":-:" code=[]"#,
                r#"line[2] Paragraph display="bar" code=[]"#,
            ],
            false,
        );
        check(
            "pipe-only-header",
            "|\n| --- | --- |\n| a | b |",
            &[
                r#"line[0] Paragraph display="|" code=[]"#,
                r#"line[1] Paragraph display="| --- | --- |" code=[]"#,
                r#"line[2] Paragraph display="| a | b |" code=[]"#,
            ],
            false,
        );
        check(
            "pipe-only-body",
            "| a | b |\n| --- | --- |\n| c | d |\n|\n| e | f |",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Paragraph display="|" code=[]"#,
                r#"line[4] Paragraph display="| e | f |" code=[]"#,
            ],
            false,
        );
        check(
            "pipe-only-tail",
            "| a | b |\n| --- | --- |\n|",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] Paragraph display="|" code=[]"#,
            ],
            false,
        );
        check(
            "double-pipe-body",
            "| a | b |\n| --- | --- |\n||",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["", ""] display="||" code=[]"#,
            ],
            false,
        );
        check(
            "empty-cells",
            "| a | | b |\n| --- | --- | --- |\n| | x | |",
            &[
                r#"line[0] TableRow header=true align=[none,none,none] cells=["a", "", "b"] display="| a | | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none,none] cells=["", "x", ""] display="| | x | |" code=[]"#,
            ],
            false,
        );
        check(
            "space-only-cells",
            "| a |     |\n| --- | --- |\n| b | c |",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "   "] display="| a |     |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["b", "c"] display="| b | c |" code=[]"#,
            ],
            false,
        );
        check(
            "tab-only-cells",
            "| \tb | \t |\n| --- | --- |\n| c | d |",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["\tb", "\t"] display="| \tb | \t |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            ],
            false,
        );
        check(
            "escaped-pipe",
            "| a \\| b | c |\n| --- | --- |\n| d | e |",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a \\| b", "c"] display="| a \\| b | c |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["d", "e"] display="| d | e |" code=[]"#,
            ],
            false,
        );
        check(
            "double-backslash-pipe",
            "| a \\\\| b |\n| --- | --- |\n| c | d |",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a \\\\", "b"] display="| a \\\\| b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            ],
            true,
        );
        check(
            "unequal-backticks-body",
            "| a | b |\n| --- | --- |\n| `c|d`` | e |",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["`c", "d``"] display="| `c|d`` | e |" code=[]"#,
            ],
            false,
        );
        check(
            "unequal-backticks-header",
            "| `a|b`` | c |\n| --- | --- |\n| d | e |",
            &[
                r#"line[0] Paragraph display="| `a|b`` | c |" code=[]"#,
                r#"line[1] Paragraph display="| --- | --- |" code=[]"#,
                r#"line[2] Paragraph display="| d | e |" code=[]"#,
            ],
            false,
        );
        check(
            "unclosed-code",
            "| `a|b | c |\n| --- | --- |\n| d | e |",
            &[
                r#"line[0] Paragraph display="| `a|b | c |" code=[]"#,
                r#"line[1] Paragraph display="| --- | --- |" code=[]"#,
                r#"line[2] Paragraph display="| d | e |" code=[]"#,
            ],
            false,
        );
        check(
            "double-backtick-pipe",
            "| a | b |\n| --- | --- |\n| ``c|d`` | e |",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["``c|d``", "e"] display="| ``c|d`` | e |" code=["c|d"]"#,
            ],
            false,
        );
        check(
            "blank-ends",
            "| a | b |\n| --- | --- |\n| c | d |\n\npara",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Empty display="" code=[]"#,
                r#"line[4] Paragraph display="para" code=[]"#,
            ],
            false,
        );
        check(
            "plain-row",
            "| a | b |\n| --- | --- |\nhello",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["hello", ""] display="hello" code=[]"#,
            ],
            false,
        );
        check(
            "list-ends",
            "| a | b |\n| --- | --- |\n| c | d |\n- item",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] ListItem display="item" code=[]"#,
            ],
            false,
        );
        check(
            "quote-ends",
            "| a | b |\n| --- | --- |\n| c | d |\n> bar",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Blockquote display="bar" code=[]"#,
            ],
            false,
        );
        check(
            "quote-no-space",
            "| a | b |\n| --- | --- |\n| c | d |\n>bar",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Paragraph display=">bar" code=[]"#,
            ],
            false,
        );
        check(
            "fence-ends",
            "| a | b |\n| --- | --- |\n| c | d |\n```\ncode\n```",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] CodeFence display="" code=[]"#,
                r#"line[4] CodeLine display="code" code=["code"]"#,
                r#"line[5] CodeFence display="" code=[]"#,
            ],
            false,
        );
        check(
            "tilde-fence",
            "| a | b |\n| --- | --- |\n| c | d |\n~~~\ncode\n~~~",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Paragraph display="~~~" code=[]"#,
                r#"line[4] Paragraph display="code" code=[]"#,
                r#"line[5] Paragraph display="~~~" code=[]"#,
            ],
            false,
        );
        check(
            "heading-ends",
            "| a | b |\n| --- | --- |\n| c | d |\n# heading",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Heading level=1 display="heading" code=[]"#,
            ],
            false,
        );
        check(
            "hr-exact",
            "| a | b |\n| --- | --- |\n| c | d |\n---",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] HorizontalRule display="" code=[]"#,
            ],
            false,
        );
        check(
            "hr-four-dash",
            "| a | b |\n| --- | --- |\n| c | d |\n----",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Paragraph display="----" code=[]"#,
            ],
            false,
        );
        check(
            "hr-four-star",
            "| a | b |\n| --- | --- |\n| c | d |\n****",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Paragraph display="****" code=[]"#,
            ],
            false,
        );
        check(
            "hr-four-under",
            "| a | b |\n| --- | --- |\n| c | d |\n____",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Paragraph display="____" code=[]"#,
            ],
            false,
        );
        check(
            "hr-spaced-dash",
            "| a | b |\n| --- | --- |\n| c | d |\n- - -",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] ListItem display="- -" code=[]"#,
            ],
            false,
        );
        check(
            "hr-spaced-star",
            "| a | b |\n| --- | --- |\n| c | d |\n* * *",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] ListItem display="* *" code=[]"#,
            ],
            false,
        );
        check(
            "hr-star-pairs",
            "| a | b |\n| --- | --- |\n| c | d |\n** ** **",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Paragraph display="** ** **" code=[]"#,
            ],
            false,
        );
        check(
            "hr-indented-four-dash",
            "| a | b |\n| --- | --- |\n| c | d |\n   ----",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Paragraph display="   ----" code=[]"#,
            ],
            false,
        );
        check(
            "paren-list",
            "| a | b |\n| --- | --- |\n| c | d |\n1) item",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Paragraph display="1) item" code=[]"#,
            ],
            false,
        );
        check(
            "html-div",
            "| a | b |\n| --- | --- |\n| c | d |\n<div>",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Paragraph display="<div>" code=[]"#,
            ],
            false,
        );
        check(
            "html-pre",
            "| a | b |\n| --- | --- |\n| c | d |\n<pre>",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Paragraph display="<pre>" code=[]"#,
            ],
            false,
        );
        check(
            "html-comment",
            "| a | b |\n| --- | --- |\n| c | d |\n<!--",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Paragraph display="<!--" code=[]"#,
            ],
            false,
        );
        check(
            "html-pi",
            "| a | b |\n| --- | --- |\n| c | d |\n<?",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Paragraph display="<?" code=[]"#,
            ],
            false,
        );
        check(
            "html-decl",
            "| a | b |\n| --- | --- |\n| c | d |\n<!DOCTYPE html>",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Paragraph display="<!DOCTYPE html>" code=[]"#,
            ],
            false,
        );
        check(
            "html-cdata",
            "| a | b |\n| --- | --- |\n| c | d |\n<![CDATA[",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Paragraph display="<![CDATA[" code=[]"#,
            ],
            false,
        );
        check(
            "indent4-table",
            "    | a | b |\n    | --- | --- |\n    | c | d |",
            &[
                r#"line[0] Paragraph display="    | a | b |" code=[]"#,
                r#"line[1] Paragraph display="    | --- | --- |" code=[]"#,
                r#"line[2] Paragraph display="    | c | d |" code=[]"#,
            ],
            false,
        );
        check(
            "indent4-delim",
            "| a | b |\n    | --- | --- |\n| c | d |",
            &[
                r#"line[0] Paragraph display="| a | b |" code=[]"#,
                r#"line[1] Paragraph display="    | --- | --- |" code=[]"#,
                r#"line[2] Paragraph display="| c | d |" code=[]"#,
            ],
            false,
        );
        check(
            "indent3-table",
            "   | a | b |\n   | --- | --- |\n   | c | d |",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="   | a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="   | c | d |" code=[]"#,
            ],
            false,
        );
        check(
            "crlf",
            "| a | b |\r\n| --- | --- |\r\n| c | d |",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |\r" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            ],
            false,
        );
        check(
            "multibyte",
            "| 你好 | 世界 |\n| --- | --- |\n| 😀 | ខ |",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["你好", "世界"] display="| 你好 | 世界 |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["😀", "ខ"] display="| 😀 | ខ |" code=[]"#,
            ],
            false,
        );
        check(
            "delim-inner-space",
            "| a | b |\n| : --- | --- : |\n| c | d |",
            &[
                r#"line[0] TableRow header=true align=[left,right] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[left,right] cells=["c", "d"] display="| c | d |" code=[]"#,
            ],
            false,
        );
        check(
            "delim-tab",
            "| a | b |\n|\t---\t|\t---\t|\n| c | d |",
            &[
                r#"line[0] Paragraph display="| a | b |" code=[]"#,
                r#"line[1] Paragraph display="|\t---\t|\t---\t|" code=[]"#,
                r#"line[2] Paragraph display="| c | d |" code=[]"#,
            ],
            false,
        );
        check(
            "inside-fence",
            "```\n| a | b |\n| --- | --- |\n```",
            &[
                r#"line[0] CodeFence display="" code=[]"#,
                r#"line[1] CodeLine display="| a | b |" code=["| a | b |"]"#,
                r#"line[2] CodeLine display="| --- | --- |" code=["| --- | --- |"]"#,
                r#"line[3] CodeFence display="" code=[]"#,
            ],
            false,
        );
        check(
            "after-fence",
            "```\ncode\n```\n| a | b |\n| --- | --- |\n| c | d |",
            &[
                r#"line[0] CodeFence display="" code=[]"#,
                r#"line[1] CodeLine display="code" code=["code"]"#,
                r#"line[2] CodeFence display="" code=[]"#,
                r#"line[3] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[4] TableDelimiter display="" code=[]"#,
                r#"line[5] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            ],
            false,
        );
        check(
            "unclosed-fence",
            "```\n| a | b |\n| --- | --- |\n| c | d |",
            &[
                r#"line[0] CodeFence display="" code=[]"#,
                r#"line[1] CodeLine display="| a | b |" code=["| a | b |"]"#,
                r#"line[2] CodeLine display="| --- | --- |" code=["| --- | --- |"]"#,
                r#"line[3] CodeLine display="| c | d |" code=["| c | d |"]"#,
            ],
            false,
        );
        check(
            "header-no-pipe",
            "hello\n| --- |\nworld",
            &[
                r#"line[0] Paragraph display="hello" code=[]"#,
                r#"line[1] Paragraph display="| --- |" code=[]"#,
                r#"line[2] Paragraph display="world" code=[]"#,
            ],
            false,
        );
        check(
            "header-code-only-pipe",
            "`a|b`\n| --- |\nc",
            &[
                r#"line[0] Paragraph display="a|b" code=["a|b"]"#,
                r#"line[1] Paragraph display="| --- |" code=[]"#,
                r#"line[2] Paragraph display="c" code=[]"#,
            ],
            false,
        );
        check(
            "align-four",
            "| a | b | c | d |\n| :--- | :---: | ---: | --- |\n| e | f | g | h |",
            &[
                r#"line[0] TableRow header=true align=[left,center,right,none] cells=["a", "b", "c", "d"] display="| a | b | c | d |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[left,center,right,none] cells=["e", "f", "g", "h"] display="| e | f | g | h |" code=[]"#,
            ],
            false,
        );
        check(
            "single-hyphen",
            "| a | b |\n| - | - |\n| c | d |",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
            ],
            false,
        );
        check(
            "delim-empty-cell",
            "| a | b |\n| --- || --- |\n| c | d |",
            &[
                r#"line[0] Paragraph display="| a | b |" code=[]"#,
                r#"line[1] Paragraph display="| --- || --- |" code=[]"#,
                r#"line[2] Paragraph display="| c | d |" code=[]"#,
            ],
            false,
        );
        check(
            "body-looks-like-delim",
            "| a | b |\n| --- | --- |\n| c | d |\n| --- | --- |",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] TableRow header=false align=[none,none] cells=["---", "---"] display="| --- | --- |" code=[]"#,
            ],
            false,
        );
        check(
            "two-tables",
            "| a | b |\n| --- | --- |\n| c | d |\n\n| e | f |\n| --- | --- |\n| g | h |",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Empty display="" code=[]"#,
                r#"line[4] TableRow header=true align=[none,none] cells=["e", "f"] display="| e | f |" code=[]"#,
                r#"line[5] TableDelimiter display="" code=[]"#,
                r#"line[6] TableRow header=false align=[none,none] cells=["g", "h"] display="| g | h |" code=[]"#,
            ],
            false,
        );
        check(
            "code-escaped-pipe-cell",
            "| a | b |\n| --- | --- |\n| c `\\|` d | e |",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c `\\|` d", "e"] display="| c `\\|` d | e |" code=["\\|"]"#,
            ],
            false,
        );
        check(
            "ws-line-ends",
            "| a | b |\n| --- | --- |\n| c | d |\n   \n| e | f |",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Empty display="   " code=[]"#,
                r#"line[4] Paragraph display="| e | f |" code=[]"#,
            ],
            false,
        );
        check(
            "tab-line",
            "| a | b |\n| --- | --- |\n| c | d |\n\t\n| e | f |",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Empty display="\t" code=[]"#,
                r#"line[4] Paragraph display="| e | f |" code=[]"#,
            ],
            false,
        );
        check(
            "ordered-list",
            "| a | b |\n| --- | --- |\n| c | d |\n1. item",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] ListItem display="item" code=[]"#,
            ],
            false,
        );
        check(
            "ordered-two",
            "| a | b |\n| --- | --- |\n| c | d |\n2. item",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] ListItem display="item" code=[]"#,
            ],
            false,
        );
        check(
            "ordered-ten",
            "| a | b |\n| --- | --- |\n| c | d |\n1234567890. item",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] TableRow header=false align=[none,none] cells=["1234567890. item", ""] display="1234567890. item" code=[]"#,
            ],
            false,
        );
        check(
            "task-list",
            "| a | b |\n| --- | --- |\n| c | d |\n- [ ] item",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] ListItem display="item" code=[]"#,
            ],
            false,
        );
        check(
            "indent4-body",
            "| a | b |\n| --- | --- |\n| c | d |\n    more",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] TableRow header=false align=[none,none] cells=["more", ""] display="    more" code=[]"#,
            ],
            false,
        );
        check(
            "indent4-bare",
            "    a | b\n    --- | ---\n    c | d",
            &[
                r#"line[0] Paragraph display="    a | b" code=[]"#,
                r#"line[1] Paragraph display="    --- | ---" code=[]"#,
                r#"line[2] Paragraph display="    c | d" code=[]"#,
            ],
            false,
        );
        check(
            "no-outer-pipes",
            "a | b\n--- | ---\nc | d",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="a | b" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="c | d" code=[]"#,
            ],
            false,
        );
        check(
            "short-header-match",
            "a | b | c\n--- | ---",
            &[
                r#"line[0] Paragraph display="a | b | c" code=[]"#,
                r#"line[1] Paragraph display="--- | ---" code=[]"#,
            ],
            false,
        );
        check(
            "indent4-hr",
            "| a | b |\n| --- | --- |\n| c | d |\n    ----",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Paragraph display="    ----" code=[]"#,
            ],
            false,
        );
        check(
            "tab-hr",
            "| a | b |\n| --- | --- |\n| c | d |\n\t----",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Paragraph display="\t----" code=[]"#,
            ],
            false,
        );
        check(
            "indent4-heading",
            "| a | b |\n| --- | --- |\n| c | d |\n    # heading",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Paragraph display="    # heading" code=[]"#,
            ],
            false,
        );
        check(
            "tab-heading",
            "| a | b |\n| --- | --- |\n| c | d |\n\t# heading",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Heading level=1 display="heading" code=[]"#,
            ],
            false,
        );
        check(
            "indent4-quote",
            "| a | b |\n| --- | --- |\n| c | d |\n    > bar",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Blockquote display="bar" code=[]"#,
            ],
            false,
        );
        check(
            "tab-quote",
            "| a | b |\n| --- | --- |\n| c | d |\n\t> bar",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Blockquote display="bar" code=[]"#,
            ],
            false,
        );
        check(
            "indent4-list",
            "| a | b |\n| --- | --- |\n| c | d |\n    - item",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] ListItem display="item" code=[]"#,
            ],
            false,
        );
        check(
            "indent4-html",
            "| a | b |\n| --- | --- |\n| c | d |\n    <div>",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Paragraph display="    <div>" code=[]"#,
            ],
            false,
        );
        check(
            "tab-html",
            "| a | b |\n| --- | --- |\n| c | d |\n\t<div>",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Paragraph display="\t<div>" code=[]"#,
            ],
            false,
        );
        check(
            "indent4-tilde",
            "| a | b |\n| --- | --- |\n| c | d |\n    ~~~\ncode\n~~~",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Paragraph display="    ~~~" code=[]"#,
                r#"line[4] Paragraph display="code" code=[]"#,
                r#"line[5] Paragraph display="~~~" code=[]"#,
            ],
            false,
        );
        check(
            "tail-fence-indent4",
            "| a | b |\n| --- | --- |\n| c | d |\n    ```\ncode",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Paragraph display="    ```" code=[]"#,
                r#"line[4] Paragraph display="code" code=[]"#,
            ],
            false,
        );
        check(
            "indent4-footnote",
            "| a | b |\n| --- | --- |\n| c | d |\n    [^1]: note",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Paragraph display="    [^1]: note" code=[]"#,
            ],
            false,
        );
        check(
            "tab-delim",
            "| a | b |\n\t| --- | --- |\n| c | d |",
            &[
                r#"line[0] Paragraph display="| a | b |" code=[]"#,
                r#"line[1] Paragraph display="\t| --- | --- |" code=[]"#,
                r#"line[2] Paragraph display="| c | d |" code=[]"#,
            ],
            false,
        );
        check(
            "space-tab-delim",
            "| a | b |\n \t| --- | --- |\n| c | d |",
            &[
                r#"line[0] Paragraph display="| a | b |" code=[]"#,
                r#"line[1] Paragraph display=" \t| --- | --- |" code=[]"#,
                r#"line[2] Paragraph display="| c | d |" code=[]"#,
            ],
            false,
        );
        check(
            "tail-footnote",
            "| a | b |\n| --- | --- |\n| c | d |\n[^1]: note",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Paragraph display="[^1]: note" code=[]"#,
            ],
            false,
        );
        check(
            "tail-footnote-name",
            "| a | b |\n| --- | --- |\n| c | d |\n[^note]: text",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Paragraph display="[^note]: text" code=[]"#,
            ],
            false,
        );
        check(
            "backtick-info",
            "| a | b |\n| --- | --- |\n| c | d |\n``` not`fence",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] TableRow header=false align=[none,none] cells=["``` not`fence", ""] display="``` not`fence" code=[]"#,
            ],
            false,
        );
        check(
            "inside-tilde",
            "~~~\n| a | b |\n| --- | --- |\n| c | d |\n~~~",
            &[
                r#"line[0] Paragraph display="~~~" code=[]"#,
                r#"line[1] Paragraph display="| a | b |" code=[]"#,
                r#"line[2] Paragraph display="| --- | --- |" code=[]"#,
                r#"line[3] Paragraph display="| c | d |" code=[]"#,
                r#"line[4] Paragraph display="~~~" code=[]"#,
            ],
            false,
        );
        check(
            "tail-vtab",
            "| a | b |\n| --- | --- |\n| c | d |\n\u{000b}",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Empty display="\u{b}" code=[]"#,
            ],
            false,
        );
        drop(check);
        assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
        let table_rows = |markdown: &str| {
            parse_markdown(markdown)
                .into_iter()
                .filter_map(|block| match block {
                    Block::Table { rows, .. } => Some(rows.len() + 1),
                    Block::Heading { .. }
                    | Block::Paragraph { .. }
                    | Block::Blockquote { .. }
                    | Block::CodeBlock { .. }
                    | Block::ListItem { .. }
                    | Block::HorizontalRule { .. }
                    | Block::EmptyLine { .. } => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            table_rows("| a | b |\n| --- | --- |\n| c | d |\n    ```\ncode"),
            vec![2],
            "tail-fence-indent4"
        );
        assert_eq!(
            table_rows("~~~\n| a | b |\n| --- | --- |\n| c | d |\n~~~"),
            Vec::<usize>::new(),
            "inside-tilde"
        );
        assert_eq!(
            table_rows("| a | b |\n| --- | --- |\n| c | d |\n\u{000b}"),
            vec![2],
            "tail-vtab"
        );
    }
}
