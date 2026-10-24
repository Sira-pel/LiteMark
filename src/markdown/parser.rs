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
        // A line ending is LF, a lone CR, or CRLF (CommonMark 0.31.2).
        if bytes[index] == b'\n' {
            lines.push(SourceLine {
                start,
                end: index + 1,
                text: &markdown[start..index],
            });
            index += 1;
            start = index;
        } else if bytes[index] == b'\r' {
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
        } else {
            index += 1;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TableColumnAlign {
    /// Delimiter is plain `---`. A table edit must not turn this into `:---`.
    #[default]
    None,
    Left,
    Center,
    Right,
}

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

/// Byte offsets of `|` that are not escaped with a backslash or inside code.
pub fn unescaped_pipe_offsets(line: &str) -> Vec<usize> {
    let bytes = line.as_bytes();
    let code = code_span_ranges(line);
    let mut offsets = Vec::new();
    for index in 0..bytes.len() {
        if separator_pipe(bytes, &code, index) {
            offsets.push(index);
        }
    }
    offsets
}

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

/// Cell text between unescaped pipes. `\|` stays inside the cell.
pub fn split_table_cells(line: &str) -> Vec<String> {
    table_cell_ranges(line)
        .into_iter()
        .map(|range| line[range].trim().to_string())
        .collect()
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

fn is_empty_table_row(line: &str) -> bool {
    table_cell_ranges(line).is_empty()
        || line
            .bytes()
            .all(|byte| matches!(byte, b' ' | b'\t' | 0x0b | 0x0c | b'\r'))
}

fn line_breaks_table(line: &str) -> bool {
    if is_blank_line(line) {
        return true;
    }
    let body = strip_all_space_and_tab(line);
    is_thematic_break(body)
        || atx_heading(body).is_some()
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

fn table_block(lines: &[SourceLine<'_>], start: usize) -> Option<TableBlock> {
    if start + 1 >= lines.len() || line_breaks_table(lines[start].text) {
        return None;
    }
    let header = lines[start].text;
    if leading_column(header) >= 4 || unescaped_pipe_offsets(header).is_empty() {
        return None;
    }
    let header_cells = table_cell_ranges(header);
    if leading_column(lines[start + 1].text) >= 4 {
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
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_punctuation() {
            let abs = base + i..base + i + 1;
            let in_code = inline_pairs.iter().any(|p| {
                p.kind == InlineTagKind::Code
                    && p.inner.start <= abs.start
                    && abs.end <= p.inner.end
            });
            if !in_code {
                raw_hidden.push(abs.clone());
                // The text segment covering the escape keeps the escaped
                // character only; the backslash is hidden.
                let mut rebuilt: Vec<StyledSegment> = Vec::new();
                for s in styled_segments.drain(..) {
                    if s.source.start < abs.end && s.source.end > abs.start {
                        if s.source.start < abs.start {
                            rebuilt.push(StyledSegment {
                                source: s.source.start..abs.start,
                                ..s.clone()
                            });
                        }
                        if s.source.end > abs.end {
                            rebuilt.push(StyledSegment {
                                source: abs.end..s.source.end,
                                ..s.clone()
                            });
                        }
                    } else {
                        rebuilt.push(s);
                    }
                }
                styled_segments = rebuilt;
            }
            i += 2;
        } else {
            i += 1;
        }
    }

    (raw_hidden, inline_pairs, styled_segments)
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
    fn in_code(inline_pairs: &[InlinePair], pos: usize) -> bool {
        inline_pairs
            .iter()
            .any(|p| p.kind == InlineTagKind::Code && p.inner.start <= pos && pos < p.inner.end)
    }
    // A run consumed as the opener or closer of a parsed pair is not
    // eligible for re-pairing.
    fn consumed(inline_pairs: &[InlinePair], pos: usize) -> bool {
        inline_pairs
            .iter()
            .any(|p| p.open_delim.start == pos || p.close_delim.start == pos)
    }
    let mut used = vec![false; runs.len()];
    for open_idx in 0..runs.len() {
        if used[open_idx]
            || in_code(inline_pairs, runs[open_idx].0)
            || consumed(inline_pairs, runs[open_idx].0)
        {
            continue;
        }
        let (o_start, o_end, ch) = runs[open_idx];
        // An opener must be followed by something (not whitespace).
        if input[o_end..]
            .chars()
            .next()
            .is_none_or(char::is_whitespace)
        {
            continue;
        }
        for close_idx in open_idx + 1..runs.len() {
            if used[close_idx]
                || in_code(inline_pairs, runs[close_idx].0)
                || consumed(inline_pairs, runs[close_idx].0)
            {
                continue;
            }
            let (c_start, c_end, c_ch) = runs[close_idx];
            if c_ch != ch || c_start == o_end {
                continue;
            }
            // Equal-length emphasis runs; code fences pair with a longer
            // later run (the content's own trailing backticks merged with
            // the closer), taking its tail as the closer. Single-backtick
            // openers are not re-paired: unequal single/double backticks in
            // ordinary text stay literal.
            if ch == b'`' {
                if o_end - o_start < 2 || c_end - c_start <= o_end - o_start {
                    continue;
                }
            } else if c_end - c_start != o_end - o_start
                || (ch == b'~' && c_end - c_start != 2)
                || c_end - c_start > 2
            {
                continue;
            }
            // Relaxed flanking: only the whitespace conditions apply. The
            // punctuation conditions and the odd-match rule are exactly what
            // rejected these runs in the first place.
            if input[c_start..]
                .chars()
                .next()
                .is_some_and(char::is_whitespace)
            {
                continue;
            }
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
            inline_pairs.push(InlinePair {
                kind,
                open_delim: base + o_start..base + o_end,
                inner: inner.clone(),
                close_delim: base + closer_start..base + c_end,
            });
            let seg = StyledSegment {
                source: inner.clone(),
                bold: kind == InlineTagKind::Bold,
                italic: kind == InlineTagKind::Italic,
                underline: false,
                strike: kind == InlineTagKind::Strike,
                code: kind == InlineTagKind::Code,
                link_url: None,
            };
            // The region was literal text: trim the old segments to the
            // parts outside the synthesized span (the remainder is hidden by
            // the marker runs below).
            let mut rebuilt: Vec<StyledSegment> = Vec::new();
            for s in styled_segments.drain(..) {
                if s.source.start < inner.end && s.source.end > inner.start {
                    if s.source.start < inner.start {
                        rebuilt.push(StyledSegment {
                            source: s.source.start..inner.start,
                            ..s.clone()
                        });
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
            *styled_segments = rebuilt;
            styled_segments
                .retain(|s| !(s.source.start >= inner.start && s.source.end <= inner.end));
            for run in [
                base + o_start..base + o_end,
                base + closer_start..base + c_end,
            ] {
                styled_segments
                    .retain(|s| !(s.source.start >= run.start && s.source.end <= run.end));
            }
            let pos = styled_segments
                .iter()
                .position(|s| s.source.start > inner.start)
                .unwrap_or(styled_segments.len());
            styled_segments.insert(pos, seg);
            break;
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
    loop {
        let mut changed = false;
        for i in 0..inline_pairs.len() {
            let inner = inline_pairs[i].inner.clone();
            let nested = (0..inline_pairs.len()).find(|&j| {
                j != i
                    && inline_pairs[j].open_delim.start == inner.start
                    && inline_pairs[j].close_delim.end == inner.end
                    && inline_pairs[j].inner.start > inner.start
                    && inline_pairs[j].inner.end < inner.end
            });
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

struct OpenFence {
    /// Column-0..3 backtick fence. A shorter run does not close it.
    strict: bool,
    len: usize,
    indent: usize,
    open_range: std::ops::Range<usize>,
}

/// Thematic break: the line trims to `---`, `***`, or `___`.
///
/// At document level (`parent_col == 0`) only columns 0–3 count. A tab is
/// column 4, so `\t---` stays a paragraph. This parser has no indented-code
/// kind, so `    ---\rnext\r` stays text after the bare-CR split. Inside a
/// list the same 0–3 columns are counted past the item's content column, and
/// a tab stops at the next multiple of 4, so `- item` followed by `\t---` is
/// a rule.
fn exact_thematic_break(line: &str, parent_col: usize) -> bool {
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
fn split_indent(line: &str) -> (usize, usize) {
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

fn prefix_through_column(line: &str, min_col: usize) -> usize {
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
fn leading_spaces(line: &str) -> Option<usize> {
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

/// Top-level backtick fence at column 0–3. Tildes are not fences.
fn strict_backtick_open(line: &str) -> Option<(usize, usize, Option<String>)> {
    let indent = leading_spaces(line)?;
    if indent > 3 {
        return None;
    }
    let bytes = line.as_bytes();
    if indent >= bytes.len() || bytes[indent] != b'`' {
        return None;
    }
    let mut end = indent;
    while end < bytes.len() && bytes[end] == b'`' {
        end += 1;
    }
    let len = end - indent;
    if len < 3 {
        return None;
    }
    let rest = &line[end..];
    if rest.contains('`') {
        return None;
    }
    let info = rest.trim_matches(|ch: char| ch == ' ' || ch == '\t');
    let info = if info.is_empty() {
        None
    } else {
        Some(info.to_string())
    };
    Some((len, indent, info))
}

/// ` ``` aa ``` ` at column 0–3 is not a fence. Callers must not fall through to red's trim rule.
fn backtick_info_rejects_fence(line: &str) -> bool {
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

fn is_closing_fence(line: &str, marker: u8, min_len: usize, max_indent: usize) -> bool {
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

fn atx_heading(line: &str) -> Option<(u8, std::ops::Range<usize>)> {
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
fn atx_heading_within(line: &str, max_indent: usize) -> Option<(u8, std::ops::Range<usize>)> {
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
fn fence_marker_line(
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
        offset_map,
        styled_segments: Vec::new(),
    }
}

#[allow(clippy::single_range_in_vec_init)]
fn code_content_line(
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
            offset_map: RowOffsetMap::identity(line_start, line_end),
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
        offset_map,
        styled_segments,
    }
}

fn atx_line(
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
        offset_map,
        styled_segments,
    }
}

/// Tab, VT, FF, or space. Pulldown `is_ascii_whitespace_no_nl`.
fn is_ascii_whitespace_no_nl(byte: u8) -> bool {
    byte == b'\t' || byte == 0x0b || byte == 0x0c || byte == b' '
}

/// Column scanner for list padding. Tabs expand to the next stop of 4,
/// counted from the byte after the previous tab (pulldown `scan_space_inner`).
#[derive(Clone, Copy)]
struct ColScan<'a> {
    bytes: &'a [u8],
    ix: usize,
    tab_start: usize,
    spaces_remaining: usize,
}

impl<'a> ColScan<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            ix: 0,
            tab_start: 0,
            spaces_remaining: 0,
        }
    }

    fn scan_space(&mut self, n_space: usize) -> bool {
        self.scan_space_inner(n_space) == 0
    }

    fn scan_space_upto(&mut self, n_space: usize) -> usize {
        n_space - self.scan_space_inner(n_space)
    }

    fn scan_space_inner(&mut self, mut n_space: usize) -> usize {
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
    fn is_at_eol(&self) -> bool {
        match self.bytes.get(self.ix) {
            None => true,
            Some(&b'\n') => true,
            Some(&b'\r') => self.ix + 1 >= self.bytes.len() || self.bytes[self.ix + 1] == b'\n',
            Some(_) => false,
        }
    }

    fn skip_indent(&mut self, indent: usize) {
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

fn rest_is_blank(bytes: &[u8]) -> bool {
    let mut index = 0;
    while index < bytes.len() && is_ascii_whitespace_no_nl(bytes[index]) {
        index += 1;
    }
    index == bytes.len() || bytes[index] == b'\n' || bytes[index] == b'\r'
}

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

fn parse_ordered_marker(col: &mut ColScan<'_>) -> Option<(bool, usize, u8)> {
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
fn finish_list_padding(col: &mut ColScan<'_>) -> usize {
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

fn task_marker_at(bytes: &[u8], at: usize) -> Option<TaskMarker> {
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
fn leading_column(line: &str) -> usize {
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

fn column_at(line: &str, byte_index: usize) -> usize {
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

fn line_is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

/// Open list items, so a marker can be refused when it would interrupt a
/// paragraph or when it starts at column 4 or later outside a parent item.
struct ListWalk {
    open: Vec<usize>,
    /// Paragraph-open flag per container. Index 0 is the document. Index
    /// `i + 1` is the content of `open[i]`.
    para: Vec<bool>,
}

impl Default for ListWalk {
    fn default() -> Self {
        Self {
            open: Vec::new(),
            para: vec![false],
        }
    }
}

fn marker_is_empty(line: &str, item: &ListMarker) -> bool {
    item.task.is_none() && rest_is_blank(&line.as_bytes()[item.marker_end..])
}

/// Red keeps a marker after exactly one tab (`\t- foo`) as a list. Four
/// spaces, and a space followed by a tab, stay text.
fn single_tab_marker(line: &str) -> bool {
    let bytes = line.as_bytes();
    bytes.first() == Some(&b'\t') && bytes.get(1).is_some_and(|byte| *byte != b' ' && *byte != b'\t')
}

/// A line the block parser records as a heading, rule, fence, quote, or table
/// rather than as paragraph text. Those lines close a list item.
///
/// Headings and thematic breaks use the same classifiers as the line loop.
/// `parent_col` is the list content column, or 0 at document level.
fn is_structural_line(line: &str, parent_col: usize) -> bool {
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
    fn in_paragraph(&self) -> bool {
        self.para.last().copied().unwrap_or(false)
    }

    fn set_paragraph(&mut self, open: bool) {
        if let Some(flag) = self.para.last_mut() {
            *flag = open;
        }
    }

    /// A dedented line stays in the item when it is paragraph continuation
    /// text (a lazy continuation). A marker that would start a list in the
    /// parent container closes the item instead.
    fn continues_lazily(&self, line: &str) -> bool {
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

    fn prepare(&mut self, line: &str) {
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
    fn take_list(&mut self, line: &str) -> Option<ListMarker> {
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

    fn note_block(&mut self, line: &str) {
        self.prepare(line);
        self.set_paragraph(false);
    }

    fn note_blank(&mut self) {
        self.set_paragraph(false);
    }

    fn note_paragraph(&mut self) {
        self.set_paragraph(true);
    }

    /// Content column of the deepest open item this line is indented into.
    /// Document level is 0, so four leading spaces stay indented text.
    fn thematic_parent_col(&self, line: &str) -> usize {
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
    let doc = parse_document(markdown);
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
    let mut all_hidden = Vec::new();
    let mut all_pairs = Vec::new();
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

        // Fences. Top-level backtick fences follow CommonMark. A fence in a
        // list item or quote, a tab-indented fence, and a tilde line keep red.
        if open_fence.is_some() {
            list_walk.note_block(line);
            let closes = open_fence.as_ref().is_some_and(|fence| {
                if fence.strict {
                    is_closing_fence(line, b'`', fence.len, 3)
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
                all_hidden.extend(parsed.hidden_ranges.iter().cloned());
                push_line!(parsed);
            } else {
                let strip = open_fence
                    .as_ref()
                    .filter(|fence| fence.strict)
                    .map(|fence| prefix_through_column(line, fence.indent))
                    .unwrap_or(0);
                let parsed = code_content_line(i, line_source, line, strip);
                all_hidden.extend(parsed.hidden_ranges.iter().cloned());
                push_line!(parsed);
            }
            continue;
        }
        let open_top = if list_quote_tracker.is_in_list_or_quote() || backtick_info_rejects_fence(line) {
            None
        } else {
            strict_backtick_open(line)
        };
        if let Some((len, indent, language)) = open_top {
            list_walk.note_block(line);
            open_fence = Some(OpenFence {
                strict: true,
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
            all_hidden.extend(parsed.hidden_ranges.iter().cloned());
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
            all_hidden.extend(parsed.hidden_ranges.iter().cloned());
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
                offset_map: RowOffsetMap::identity(line_start, line_end),
                styled_segments: Vec::new(),
            });
            continue;
        }

        // Horizontal rule: exact `---`, `***`, or `___`, indent 0–3 past the
        // open list item (document level rejects four spaces and a tab).
        if exact_thematic_break(line, list_walk.thematic_parent_col(line)) {
            list_walk.note_block(line);
            let hidden = vec![line_start..line_end];
            all_hidden.extend(hidden.iter().cloned());
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
                offset_map,
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
                    alignments: block.alignments,
                }
            });
        }
        if let Some(tbl) = table.as_ref().filter(|tbl| i >= tbl.first && i <= tbl.last) {
            list_walk.note_block(line);
            if i == tbl.delimiter {
                let hidden = vec![line_start..line_end];
                all_hidden.extend(hidden.iter().cloned());
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
                    offset_map,
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
                all_hidden.extend(merged_hidden.iter().cloned());
                all_pairs.extend(row_pairs.iter().cloned());
                push_line!(ParsedLine {
                    line_index: i,
                    source: line_source.clone(),
                    kind: LineKind::TableRow {
                        is_header,
                        cells,
                        raw_cells,
                        alignments: tbl.alignments.clone(),
                        table_source: tbl.source.clone(),
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

        if let Some((level, content)) = atx_display_content_range(line) {
            list_walk.note_block(line);
            let parsed = atx_line(i, line_source, line, level, content);
            all_hidden.extend(parsed.hidden_ranges.iter().cloned());
            all_pairs.extend(parsed.inline_pairs.iter().cloned());
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
        all_hidden.extend(merged_hidden.iter().cloned());
        all_pairs.extend(inline_pairs.iter().cloned());

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
            offset_map,
            styled_segments,
        });
    }

    if let Some(fence) = open_fence {
        code_blocks.push((fence.open_range, markdown.len()..markdown.len()));
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
    fn test_in_list_or_quote_tracker_exhaustive() {
        let sample_kinds = [
            LineKind::Paragraph,
            LineKind::Empty,
            LineKind::ListItem {
                ordered: false,
                number: 1,
                task: None,
                indent_cols: 0,
                checkbox_source: None,
            },
            LineKind::Blockquote,
            LineKind::TableRow {
                is_header: false,
                cells: Vec::new(),
                raw_cells: Vec::new(),
                alignments: Vec::new(),
                table_source: 0..0,
            },
        ];

        fn check_all(
            kinds: &[LineKind],
            seq: &mut Vec<ParsedLine>,
            tracker: InListOrQuoteTracker,
            depth: usize,
        ) {
            assert_eq!(
                tracker.is_in_list_or_quote(),
                in_list_or_quote(seq),
                "Mismatch for sequence: {:?}",
                seq.iter().map(|l| match &l.kind {
                    LineKind::Empty => "E",
                    LineKind::ListItem { .. } => "L",
                    LineKind::Blockquote => "B",
                    _ => "O",
                }).collect::<Vec<_>>()
            );
            if depth >= 8 {
                return;
            }
            for kind in kinds {
                let mut next_tracker = tracker;
                next_tracker.push(kind);
                seq.push(ParsedLine {
                    line_index: depth,
                    source: 0..0,
                    kind: kind.clone(),
                    prefix_range: 0..0,
                    hidden_ranges: Vec::new(),
                    inline_pairs: Vec::new(),
                    display_text: String::new(),
                    offset_map: RowOffsetMap::identity(0, 0),
                    styled_segments: Vec::new(),
                });
                check_all(kinds, seq, next_tracker, depth + 1);
                seq.pop();
            }
        }

        let mut seq = Vec::new();
        check_all(&sample_kinds, &mut seq, InListOrQuoteTracker::default(), 0);
    }

    #[test]
    fn test_20000_row_table_parse_fast() {
        use std::fmt::Write;
        let mut md = String::with_capacity(20002 * 20);
        md.push_str("| col1 | col2 |\n| --- | --- |\n");
        for i in 0..20000 {
            let _ = writeln!(md, "| cell_{i}_1 | cell_{i}_2 |");
        }

        let start = std::time::Instant::now();
        let doc = parse_document(&md);
        let elapsed = start.elapsed();

        assert_eq!(doc.lines.len(), 20003);
        let max_allowed_ms = if cfg!(debug_assertions) { 1000 } else { 100 };
        assert!(
            elapsed.as_millis() < max_allowed_ms,
            "Parsing 20,000 table rows took too long: {elapsed:?}"
        );
    }

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
        assert!(matches!(doc.lines[0].kind, LineKind::TableRow { is_header: true, .. }));
        assert!(matches!(doc.lines[1].kind, LineKind::TableDelimiter));
        assert!(matches!(doc.lines[2].kind, LineKind::TableRow { is_header: false, .. }));
        assert_eq!(doc.lines[0].display_text, "| **Col 1** | Col 2 |");
        // Verify Col 1 is bold (via **), but Col 2 is not forced bold
        let bold_segs: Vec<_> = doc.lines[0].styled_segments.iter().filter(|s| s.bold).collect();
        assert!(!bold_segs.is_empty());
        let normal_segs: Vec<_> = doc.lines[0].styled_segments.iter().filter(|s| !s.bold).collect();
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

    struct LeafExpect {
        kind: &'static str,
        display: &'static str,
        source: &'static str,
        marks: &'static str,
        links: &'static str,
    }

    fn leaf(kind: &'static str, display: &'static str, source: &'static str) -> LeafExpect {
        LeafExpect {
            kind,
            display,
            source,
            marks: "-",
            links: "-",
        }
    }

    fn leaf_marks(
        kind: &'static str,
        display: &'static str,
        source: &'static str,
        marks: &'static str,
    ) -> LeafExpect {
        LeafExpect {
            kind,
            display,
            source,
            marks,
            links: "-",
        }
    }

    fn leaf_link(
        kind: &'static str,
        display: &'static str,
        source: &'static str,
        links: &'static str,
    ) -> LeafExpect {
        LeafExpect {
            kind,
            display,
            source,
            marks: "-",
            links,
        }
    }

    fn leaf_kind_token(kind: &LineKind) -> String {
        match kind {
            LineKind::Empty => "empty".to_string(),
            LineKind::Paragraph => "p".to_string(),
            LineKind::Heading { level } => format!("h{level}"),
            LineKind::Blockquote => "quote".to_string(),
            LineKind::ListItem {
                ordered,
                number,
                task,
                indent_cols,
                ..
            } => match (ordered, task) {
                (false, None) => format!("ul:{indent_cols}"),
                (true, None) => format!("ol:{number}:{indent_cols}"),
                (false, Some(false)) => format!("task:{indent_cols}"),
                (false, Some(true)) => format!("task-x:{indent_cols}"),
                (true, Some(_)) => format!("ol-task:{number}:{indent_cols}"),
            },
            LineKind::CodeFence {
                language: Some(language),
                opening: true,
            } => format!("fence-open:{language}"),
            LineKind::CodeFence {
                language: None,
                opening: true,
            } => "fence-open".to_string(),
            LineKind::CodeFence { opening: false, .. } => "fence-close".to_string(),
            LineKind::CodeLine => "code".to_string(),
            LineKind::HorizontalRule => "hr".to_string(),
            LineKind::TableDelimiter => "tdelim".to_string(),
            LineKind::TableRow { .. } => "row".to_string(),
        }
    }

    fn leaf_dump(md: &str) -> Result<String, String> {
        let doc = parse_document(md);
        let mut out = String::new();
        for line in &doc.lines {
            for (idx, ch) in line.display_text.char_indices() {
                let src = line.offset_map.display_to_source(idx);
                if !md.is_char_boundary(src) || !md[src..].starts_with(ch) {
                    return Err(format!(
                        "map display {ch:?} at {idx} -> source {src} in {:?}",
                        line.display_text
                    ));
                }
            }
            let mut marks = Vec::new();
            let mut links = Vec::new();
            for segment in &line.styled_segments {
                let mut flags = String::new();
                if segment.bold {
                    flags.push('B');
                }
                if segment.italic {
                    flags.push('I');
                }
                if segment.code {
                    flags.push('C');
                }
                if segment.strike {
                    flags.push('S');
                }
                let text = md.get(segment.source.clone()).unwrap_or("");
                if !flags.is_empty() && !text.is_empty() {
                    marks.push(format!("{flags}:{text}"));
                }
                if let Some(url) = &segment.link_url {
                    links.push(url.clone());
                }
            }
            let marks = if marks.is_empty() {
                "-".to_string()
            } else {
                marks.join(",")
            };
            let links = if links.is_empty() {
                "-".to_string()
            } else {
                links.join(",")
            };
            let source = &md[line.source.clone()];
            out.push_str(&format!(
                "{}\tdisplay={:?}\tsource={:?}\tmarks={marks}\tlinks={links}\n",
                leaf_kind_token(&line.kind),
                line.display_text,
                source,
            ));
        }
        Ok(out)
    }

    fn expect_dump(lines: &[LeafExpect]) -> String {
        let mut out = String::new();
        for line in lines {
            out.push_str(&format!(
                "{}\tdisplay={:?}\tsource={:?}\tmarks={}\tlinks={}\n",
                line.kind, line.display, line.source, line.marks, line.links,
            ));
        }
        out
    }

    #[test]
    fn unclosed_fence_spans_to_eof() {
        let md = "```\naaa\n";
        let doc = parse_document(md);
        assert_eq!(doc.code_blocks.len(), 1);
        let (open, close) = &doc.code_blocks[0];
        assert_eq!(open.start, 0);
        assert!(open.end > open.start);
        assert_eq!(close.start, md.len());
        assert_eq!(close.end, md.len());
    }

    /// CommonMark 0.31.2 leaf blocks this parser keeps: ATX headings, thematic
    /// breaks, and top-level backtick fences. Each case compares the whole
    /// kind, display, source, mark, and link record.
    #[test]
    fn leaf_blocks_match_commonmark_examples() {
        let cases: Vec<(&str, &str, Vec<LeafExpect>)> = leaf_block_cases();
        let mut failures = Vec::new();
        for (name, md, expected) in &cases {
            let want = expect_dump(expected);
            match leaf_dump(md) {
                Ok(actual) if actual == want => {}
                Ok(actual) => {
                    failures.push(format!("CASE {name}\nexpected:\n{want}actual:\n{actual}"))
                }
                Err(err) => failures.push(format!("CASE {name} {err}")),
            }
        }
        assert!(
            failures.is_empty(),
            "{} leaf-block case(s) failed:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    #[allow(clippy::too_many_lines)]
    fn leaf_block_cases() -> Vec<(&'static str, &'static str, Vec<LeafExpect>)> {
        vec![
            (
                "ex43-hr",
                "***\n---\n___",
                vec![
                    leaf("hr", "", "***"),
                    leaf("hr", "", "---"),
                    leaf("hr", "", "___"),
                ],
            ),
            ("ex44-not-hr", "+++", vec![leaf("p", "+++", "+++")]),
            ("ex45-equals-not-hr", "===", vec![leaf("p", "===", "===")]),
            (
                "ex46-too-short",
                "--\n**\n__",
                vec![
                    leaf("p", "--", "--"),
                    leaf("p", "**", "**"),
                    leaf("p", "__", "__"),
                ],
            ),
            (
                "ex47-hr-indent",
                " ***\n  ***\n   ***",
                vec![
                    leaf("hr", "", " ***"),
                    leaf("hr", "", "  ***"),
                    leaf("hr", "", "   ***"),
                ],
            ),
            (
                "ex50-hr-long",
                "_____________________________________",
                vec![leaf(
                    "p",
                    "_____________________________________",
                    "_____________________________________",
                )],
            ),
            (
                "ex51-hr-spaced",
                " - - -",
                vec![leaf("ul:1", "- -", " - - -")],
            ),
            (
                "ex52-hr-stars",
                " **  * ** * ** * **",
                vec![leaf("p", " **  * ** * ** * **", " **  * ** * ** * **")],
            ),
            (
                "ex53-hr-spaced-dashes",
                "-     -      -      -",
                vec![leaf("ul:0", "    -      -      -", "-     -      -      -")],
            ),
            (
                "ex54-hr-trailing-space",
                "- - - -    ",
                vec![leaf("ul:0", "- - -    ", "- - - -    ")],
            ),
            (
                "ex55-not-hr",
                "_ _ _ _ a\n\na------\n\n---a---",
                vec![
                    leaf("p", "_ _ _ _ a", "_ _ _ _ a"),
                    leaf("empty", "", ""),
                    leaf("p", "a------", "a------"),
                    leaf("empty", "", ""),
                    leaf("p", "---a---", "---a---"),
                ],
            ),
            (
                "ex56-mixed-not-hr",
                " *-*",
                vec![leaf_marks("p", " -", " *-*", "I:-")],
            ),
            (
                "ex57-hr-between-lists",
                "- foo\n***\n- bar",
                vec![
                    leaf("ul:0", "foo", "- foo"),
                    leaf("hr", "", "***"),
                    leaf("ul:0", "bar", "- bar"),
                ],
            ),
            (
                "ex58-hr-not-setext",
                "Foo\n***\nbar",
                vec![
                    leaf("p", "Foo", "Foo"),
                    leaf("hr", "", "***"),
                    leaf("p", "bar", "bar"),
                ],
            ),
            (
                "ex60-hr-vs-list",
                "* Foo\n* * *\n* Bar",
                vec![
                    leaf("ul:0", "Foo", "* Foo"),
                    leaf("ul:0", "* *", "* * *"),
                    leaf("ul:0", "Bar", "* Bar"),
                ],
            ),
            (
                "ex62-atx-levels",
                "# foo\n## foo\n### foo\n#### foo\n##### foo\n###### foo",
                vec![
                    leaf("h1", "foo", "# foo"),
                    leaf("h2", "foo", "## foo"),
                    leaf("h3", "foo", "### foo"),
                    leaf("h4", "foo", "#### foo"),
                    leaf("h5", "foo", "##### foo"),
                    leaf("h6", "foo", "###### foo"),
                ],
            ),
            (
                "ex63-seven-hashes",
                "####### foo",
                vec![leaf("p", "####### foo", "####### foo")],
            ),
            (
                "audit-seven-hashes",
                "####### no",
                vec![leaf("p", "####### no", "####### no")],
            ),
            (
                "ex64-atx-needs-space",
                "#5 bolt\n\n#hashtag",
                vec![
                    leaf("p", "#5 bolt", "#5 bolt"),
                    leaf("empty", "", ""),
                    leaf("p", "#hashtag", "#hashtag"),
                ],
            ),
            (
                // The WYSIWYG display hides escape backslashes, so the
                // display shows the escaped characters unescaped.
                "ex65-escaped-atx",
                "\\## foo",
                vec![leaf("p", "## foo", "\\## foo")],
            ),
            (
                "ex66-atx-inline",
                "# foo *bar* \\*baz\\*",
                vec![leaf_marks(
                    "h1",
                    "foo bar *baz*",
                    "# foo *bar* \\*baz\\*",
                    "I:bar",
                )],
            ),
            (
                // CommonMark 0.31.2 ex67 trims the trailing run, but the
                // WYSIWYG display keeps it so typing spaces at the end of a
                // heading does not swallow them until the next letter.
                "ex67-atx-trim",
                "#                  foo                     ",
                vec![leaf(
                    "h1",
                    "foo                     ",
                    "#                  foo                     ",
                )],
            ),
            (
                "ex68-atx-indent",
                " ### foo\n  ## foo\n   # foo",
                vec![
                    leaf("h3", "foo", " ### foo"),
                    leaf("h2", "foo", "  ## foo"),
                    leaf("h1", "foo", "   # foo"),
                ],
            ),
            (
                "ex69-four-spaces-not-atx",
                "    # foo",
                vec![leaf("p", "    # foo", "    # foo")],
            ),
            (
                "ex71-closing-hash",
                "## foo ##\n  ###   bar    ###",
                vec![
                    leaf("h2", "foo", "## foo ##"),
                    leaf("h3", "bar", "  ###   bar    ###"),
                ],
            ),
            (
                "ex72-closing-hash-run",
                "# foo ##################################\n##### foo ##",
                vec![
                    leaf("h1", "foo", "# foo ##################################"),
                    leaf("h5", "foo", "##### foo ##"),
                ],
            ),
            (
                "ex73-closing-hash-spaces",
                "### foo ###     ",
                vec![leaf("h3", "foo", "### foo ###     ")],
            ),
            (
                "ex74-hash-not-closer",
                "### foo ### b",
                vec![leaf("h3", "foo ### b", "### foo ### b")],
            ),
            (
                "ex75-hash-needs-space",
                "# foo#",
                vec![leaf("h1", "foo#", "# foo#")],
            ),
            (
                "ex76-escaped-closer",
                "### foo \\###\n## foo #\\##\n# foo \\#",
                vec![
                    leaf("h3", "foo ###", "### foo \\###"),
                    leaf("h2", "foo ###", "## foo #\\##"),
                    leaf("h1", "foo #", "# foo \\#"),
                ],
            ),
            (
                "ex77-four-stars",
                "****\n## foo\n****",
                vec![
                    leaf("p", "****", "****"),
                    leaf("h2", "foo", "## foo"),
                    leaf("p", "****", "****"),
                ],
            ),
            (
                "ex78-atx-interrupts",
                "Foo bar\n# baz\nBar foo",
                vec![
                    leaf("p", "Foo bar", "Foo bar"),
                    leaf("h1", "baz", "# baz"),
                    leaf("p", "Bar foo", "Bar foo"),
                ],
            ),
            (
                "ex79-empty-atx",
                "## \n#\n### ###",
                vec![
                    leaf("h2", "", "## "),
                    leaf("h1", "", "#"),
                    leaf("h3", "", "### ###"),
                ],
            ),
            (
                "ex88-not-setext",
                "Foo\n= =\n\nFoo\n--- -",
                vec![
                    leaf("p", "Foo", "Foo"),
                    leaf("p", "= =", "= ="),
                    leaf("empty", "", ""),
                    leaf("p", "Foo", "Foo"),
                    leaf("p", "--- -", "--- -"),
                ],
            ),
            (
                "ex92-quote-then-hr",
                "> Foo\n---",
                vec![leaf("quote", "Foo", "> Foo"), leaf("hr", "", "---")],
            ),
            (
                "ex94-list-then-hr",
                "- Foo\n---",
                vec![leaf("ul:0", "Foo", "- Foo"), leaf("hr", "", "---")],
            ),
            (
                "ex97-equals-after-blank",
                "\n====",
                vec![leaf("empty", "", ""), leaf("p", "====", "====")],
            ),
            (
                "ex98-two-hrs",
                "---\n---",
                vec![leaf("hr", "", "---"), leaf("hr", "", "---")],
            ),
            (
                "ex119-backtick-fence",
                "```\n<\n >\n```",
                vec![
                    leaf("fence-open", "", "```"),
                    leaf_marks("code", "<", "<", "C:<"),
                    leaf_marks("code", " >", " >", "C: >"),
                    leaf("fence-close", "", "```"),
                ],
            ),
            (
                "ex122-tilde-inside-backticks",
                "```\naaa\n~~~\n```",
                vec![
                    leaf("fence-open", "", "```"),
                    leaf_marks("code", "aaa", "aaa", "C:aaa"),
                    leaf_marks("code", "~~~", "~~~", "C:~~~"),
                    leaf("fence-close", "", "```"),
                ],
            ),
            (
                "ex124-fence-len",
                "````\naaa\n```\n``````",
                vec![
                    leaf("fence-open", "", "````"),
                    leaf_marks("code", "aaa", "aaa", "C:aaa"),
                    leaf_marks("code", "```", "```", "C:```"),
                    leaf("fence-close", "", "``````"),
                ],
            ),
            (
                "ex126-open-fence",
                "```",
                vec![leaf("fence-open", "", "```")],
            ),
            (
                "ex127-unclosed-fence",
                "`````\n\n```\naaa",
                vec![
                    leaf("fence-open", "", "`````"),
                    leaf("code", "", ""),
                    leaf_marks("code", "```", "```", "C:```"),
                    leaf_marks("code", "aaa", "aaa", "C:aaa"),
                ],
            ),
            (
                "ex129-fence-blanks",
                "```\n\n  \n```",
                vec![
                    leaf("fence-open", "", "```"),
                    leaf("code", "", ""),
                    leaf_marks("code", "  ", "  ", "C:  "),
                    leaf("fence-close", "", "```"),
                ],
            ),
            (
                "ex130-empty-fence",
                "```\n```",
                vec![
                    leaf("fence-open", "", "```"),
                    leaf("fence-close", "", "```"),
                ],
            ),
            (
                "ex131-fence-indent-1",
                " ```\n aaa\naaa\n```",
                vec![
                    leaf("fence-open", "", " ```"),
                    leaf_marks("code", "aaa", " aaa", "C:aaa"),
                    leaf_marks("code", "aaa", "aaa", "C:aaa"),
                    leaf("fence-close", "", "```"),
                ],
            ),
            (
                "ex132-fence-indent-2",
                "  ```\naaa\n  aaa\naaa\n  ```",
                vec![
                    leaf("fence-open", "", "  ```"),
                    leaf_marks("code", "aaa", "aaa", "C:aaa"),
                    leaf_marks("code", "aaa", "  aaa", "C:aaa"),
                    leaf_marks("code", "aaa", "aaa", "C:aaa"),
                    leaf("fence-close", "", "  ```"),
                ],
            ),
            (
                "ex133-fence-indent-3",
                "   ```\n   aaa\n    aaa\n  aaa\n   ```",
                vec![
                    leaf("fence-open", "", "   ```"),
                    leaf_marks("code", "aaa", "   aaa", "C:aaa"),
                    leaf_marks("code", " aaa", "    aaa", "C: aaa"),
                    leaf_marks("code", "aaa", "  aaa", "C:aaa"),
                    leaf("fence-close", "", "   ```"),
                ],
            ),
            (
                "ex135-closer-indent",
                "```\naaa\n  ```",
                vec![
                    leaf("fence-open", "", "```"),
                    leaf_marks("code", "aaa", "aaa", "C:aaa"),
                    leaf("fence-close", "", "  ```"),
                ],
            ),
            (
                "ex136-opener-indent-3",
                "   ```\naaa\n  ```",
                vec![
                    leaf("fence-open", "", "   ```"),
                    leaf_marks("code", "aaa", "aaa", "C:aaa"),
                    leaf("fence-close", "", "  ```"),
                ],
            ),
            (
                "ex137-closer-indent-4",
                "```\naaa\n    ```",
                vec![
                    leaf("fence-open", "", "```"),
                    leaf_marks("code", "aaa", "aaa", "C:aaa"),
                    leaf_marks("code", "    ```", "    ```", "C:    ```"),
                ],
            ),
            (
                "ex138-backtick-in-info",
                "``` ```\naaa",
                vec![
                    leaf_marks("p", " ", "``` ```", "C: "),
                    leaf("p", "aaa", "aaa"),
                ],
            ),
            (
                "ex140-fence-interrupts",
                "foo\n```\nbar\n```\nbaz",
                vec![
                    leaf("p", "foo", "foo"),
                    leaf("fence-open", "", "```"),
                    leaf_marks("code", "bar", "bar", "C:bar"),
                    leaf("fence-close", "", "```"),
                    leaf("p", "baz", "baz"),
                ],
            ),
            (
                "ex142-fence-info",
                "```ruby\ndef foo(x)\n  return 3\nend\n```",
                vec![
                    leaf("fence-open:ruby", "", "```ruby"),
                    leaf_marks("code", "def foo(x)", "def foo(x)", "C:def foo(x)"),
                    leaf_marks("code", "  return 3", "  return 3", "C:  return 3"),
                    leaf_marks("code", "end", "end", "C:end"),
                    leaf("fence-close", "", "```"),
                ],
            ),
            (
                "ex144-info-semicolon",
                "````;\n````",
                vec![
                    leaf("fence-open:;", "", "````;"),
                    leaf("fence-close", "", "````"),
                ],
            ),
            (
                "ex145-backtick-info-not-fence",
                "``` aa ```\nfoo",
                vec![
                    leaf_marks("p", " aa ", "``` aa ```", "C: aa "),
                    leaf("p", "foo", "foo"),
                ],
            ),
            (
                "ex147-closer-info",
                "```\n``` aaa\n```",
                vec![
                    leaf("fence-open", "", "```"),
                    leaf_marks("code", "``` aaa", "``` aaa", "C:``` aaa"),
                    leaf("fence-close", "", "```"),
                ],
            ),
            (
                "bold-closing-hash",
                "## **foo** ##",
                vec![leaf_marks("h2", "foo", "## **foo** ##", "B:foo")],
            ),
            (
                "heading-link",
                "# [Foo](/url)",
                vec![leaf_link("h1", "Foo", "# [Foo](/url)", "/url")],
            ),
            (
                "nested-list-not-code",
                "- foo\n    - bar",
                vec![
                    leaf("ul:0", "foo", "- foo"),
                    leaf("ul:4", "bar", "    - bar"),
                ],
            ),
            (
                "quote-indent-3",
                "   > foo",
                vec![leaf("quote", "foo", "   > foo")],
            ),
            (
                "tab-list-not-code",
                "\t- foo",
                vec![leaf("ul:1", "foo", "\t- foo")],
            ),
            (
                "double-tilde-not-fence",
                "~~Hi~~",
                vec![leaf_marks("p", "Hi", "~~Hi~~", "S:Hi")],
            ),
            (
                "blank-spaces-not-code",
                "    ",
                vec![leaf("empty", "    ", "    ")],
            ),
            (
                "spaced-dashes-after-paragraph",
                "Foo\n- - -",
                vec![leaf("p", "Foo", "Foo"), leaf("ul:0", "- -", "- - -")],
            ),
            (
                "four-dashes-after-paragraph-stay-text",
                "Foo\n ----\nFoo\n   ----      ",
                vec![
                    leaf("p", "Foo", "Foo"),
                    leaf("p", " ----", " ----"),
                    leaf("p", "Foo", "Foo"),
                    leaf("p", "   ----      ", "   ----      "),
                ],
            ),
            (
                "four-dashes-at-top-are-hr",
                "----",
                vec![leaf("p", "----", "----")],
            ),
            (
                "table-unchanged",
                "| a | b |\n| --- | --- |\n| c | d |",
                vec![
                    leaf("row", "| a | b |", "| a | b |"),
                    leaf("tdelim", "", "| --- | --- |"),
                    leaf("row", "| c | d |", "| c | d |"),
                ],
            ),
            (
                "fence-tabs",
                "```\n\tkeep\tme\n```",
                vec![
                    leaf("fence-open", "", "```"),
                    leaf_marks("code", "\tkeep\tme", "\tkeep\tme", "C:\tkeep\tme"),
                    leaf("fence-close", "", "```"),
                ],
            ),
            (
                "top-fence-keeps-shorter-backticks-as-code",
                "`````\naaa\n```\nbar",
                vec![
                    leaf("fence-open", "", "`````"),
                    leaf_marks("code", "aaa", "aaa", "C:aaa"),
                    leaf_marks("code", "```", "```", "C:```"),
                    leaf_marks("code", "bar", "bar", "C:bar"),
                ],
            ),
            (
                "list-fence-closes-on-three-backticks",
                "- foo\n````\ncode\n```\nbar",
                vec![
                    leaf("ul:0", "foo", "- foo"),
                    leaf("fence-open:`", "", "````"),
                    leaf_marks("code", "code", "code", "C:code"),
                    leaf("fence-close", "", "```"),
                    leaf("p", "bar", "bar"),
                ],
            ),
            (
                "tilde-lines-stay-paragraphs",
                "~~~\ncode\n~~~",
                vec![
                    leaf("p", "~~~", "~~~"),
                    leaf("p", "code", "code"),
                    leaf("p", "~~~", "~~~"),
                ],
            ),
            (
                "quote-blank-then-fence-info-stays-fence",
                "> foo\n\n    ``` aa ```",
                vec![
                    leaf("quote", "foo", "> foo"),
                    leaf("empty", "", ""),
                    leaf("fence-open:aa ```", "", "    ``` aa ```"),
                ],
            ),
            (
                "tab-fence-stays-red",
                "\t```\ncode",
                vec![
                    leaf("fence-open", "", "\t```"),
                    leaf_marks("code", "code", "code", "C:code"),
                ],
            ),
        ]
    }

    fn kind_report(md: &str, kind: &LineKind) -> String {
        match kind {
            LineKind::Empty => "Empty".to_string(),
            LineKind::Paragraph => "Paragraph".to_string(),
            LineKind::Heading { level } => format!("Heading level={level}"),
            LineKind::Blockquote => "Blockquote".to_string(),
            LineKind::ListItem {
                ordered,
                number,
                task,
                indent_cols,
                checkbox_source,
            } => {
                let task = match task {
                    None => "none",
                    Some(false) => "unchecked",
                    Some(true) => "checked",
                };
                let checkbox = match checkbox_source {
                    Some(range) => format!("{:?}", &md[range.clone()]),
                    None => "-".to_string(),
                };
                format!(
                    "ListItem ordered={ordered} number={number} task={task} indent={indent_cols} checkbox={checkbox}"
                )
            }
            LineKind::CodeFence { language, opening } => {
                format!("CodeFence opening={opening} language={language:?}")
            }
            LineKind::CodeLine => "CodeLine".to_string(),
            LineKind::HorizontalRule => "HorizontalRule".to_string(),
            LineKind::TableDelimiter => "TableDelimiter".to_string(),
            LineKind::TableRow {
                is_header, cells, ..
            } => format!("TableRow header={is_header} cells={}", cells.len()),
        }
    }

    fn strike_report(md: &str, line: &ParsedLine) -> String {
        let mut parts = Vec::new();
        for segment in &line.styled_segments {
            if segment.strike {
                parts.push(format!("{:?}", &md[segment.source.clone()]));
            }
        }
        if parts.is_empty() {
            "-".to_string()
        } else {
            parts.join(",")
        }
    }

    fn link_report(line: &ParsedLine) -> String {
        let mut urls = Vec::new();
        for segment in &line.styled_segments {
            if let Some(url) = &segment.link_url {
                urls.push(format!("{url:?}"));
            }
        }
        if urls.is_empty() {
            "-".to_string()
        } else {
            urls.join(",")
        }
    }

    fn map_status(md: &str, line: &ParsedLine) -> String {
        for (idx, ch) in line.display_text.char_indices() {
            let src = line.offset_map.display_to_source(idx);
            if !md.is_char_boundary(src) {
                return format!("fail boundary {src}");
            }
            match md[src..].chars().next() {
                Some(got) if got == ch => {}
                other => {
                    return format!("fail display {ch:?}@{idx} source {other:?}@{src}");
                }
            }
        }
        "ok".to_string()
    }

    fn observe_line(md: &str, index: usize) -> String {
        let doc = parse_document(md);
        let line = &doc.lines[index];
        format!(
            "{} display={:?} strike={} link={} map={}",
            kind_report(md, &line.kind),
            line.display_text,
            strike_report(md, line),
            link_report(line),
            map_status(md, line),
        )
    }

    fn observe_doc(label: &str, md: &str) -> String {
        let doc = parse_document(md);
        let mut out = String::new();
        for index in 0..doc.lines.len() {
            if index > 0 {
                out.push('\n');
            }
            out.push_str(&format!("{label}[{index}] {}", observe_line(md, index)));
        }
        out
    }

    /// CommonMark examples 265, 266, 283, 296, and 302, plus GFM ordered tasks.
    #[test]
    fn markers_match_gfm() {
        let observed = format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            observe_doc("cm302", "1. foo\n2. bar\n3) baz"),
            observe_doc("cm296", "10) foo\n    - bar"),
            observe_doc("cm266", "1234567890. not ok"),
            observe_doc("cm265", "123456789. ok"),
            observe_doc("task", "1. [ ] first\n2. [x] second"),
            observe_doc("ex283", "1. foo\n2.\n3. bar"),
            observe_doc("eol_dot", "1."),
            observe_doc("eol_paren", "1)"),
            observe_doc("tab_dot", "1.\tfoo"),
            observe_doc("tab_paren", "1)\tfoo"),
            observe_doc("task_tab", "1. [ ]\tfoo"),
            observe_doc("task_eol", "1. [ ]"),
            observe_doc("task_x_eol", "1. [x]"),
            observe_doc("task_sp_eol", "1. [ ] "),
            observe_doc("task_tight", "1. [ ]foo"),
            observe_doc("task_gap", "1.  [ ] foo"),
            observe_doc("bullet_gap", "-  [ ] foo"),
            observe_doc("task_spaces", "1. [ ]  foo"),
            observe_doc("task_tabs", "1. [ ]\t\tbar"),
        );
        let expected = "\
cm302[0] ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"foo\" strike=- link=- map=ok
cm302[1] ListItem ordered=true number=2 task=none indent=0 checkbox=- display=\"bar\" strike=- link=- map=ok
cm302[2] ListItem ordered=true number=3 task=none indent=0 checkbox=- display=\"baz\" strike=- link=- map=ok
cm296[0] ListItem ordered=true number=10 task=none indent=0 checkbox=- display=\"foo\" strike=- link=- map=ok
cm296[1] ListItem ordered=false number=0 task=none indent=4 checkbox=- display=\"bar\" strike=- link=- map=ok
cm266[0] Paragraph display=\"1234567890. not ok\" strike=- link=- map=ok
cm265[0] ListItem ordered=true number=123456789 task=none indent=0 checkbox=- display=\"ok\" strike=- link=- map=ok
task[0] ListItem ordered=true number=1 task=unchecked indent=0 checkbox=\"[ ]\" display=\"first\" strike=- link=- map=ok
task[1] ListItem ordered=true number=2 task=checked indent=0 checkbox=\"[x]\" display=\"second\" strike=- link=- map=ok
ex283[0] ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"foo\" strike=- link=- map=ok
ex283[1] ListItem ordered=true number=2 task=none indent=0 checkbox=- display=\"\" strike=- link=- map=ok
ex283[2] ListItem ordered=true number=3 task=none indent=0 checkbox=- display=\"bar\" strike=- link=- map=ok
eol_dot[0] ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"\" strike=- link=- map=ok
eol_paren[0] ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"\" strike=- link=- map=ok
tab_dot[0] ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"foo\" strike=- link=- map=ok
tab_paren[0] ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"foo\" strike=- link=- map=ok
task_tab[0] ListItem ordered=true number=1 task=unchecked indent=0 checkbox=\"[ ]\" display=\"foo\" strike=- link=- map=ok
task_eol[0] ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"[ ]\" strike=- link=- map=ok
task_x_eol[0] ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"[x]\" strike=- link=- map=ok
task_sp_eol[0] ListItem ordered=true number=1 task=unchecked indent=0 checkbox=\"[ ]\" display=\"\" strike=- link=- map=ok
task_tight[0] ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"[ ]foo\" strike=- link=- map=ok
task_gap[0] ListItem ordered=true number=1 task=unchecked indent=0 checkbox=\"[ ]\" display=\"foo\" strike=- link=- map=ok
bullet_gap[0] ListItem ordered=false number=0 task=unchecked indent=0 checkbox=\"[ ]\" display=\"foo\" strike=- link=- map=ok
task_spaces[0] ListItem ordered=true number=1 task=unchecked indent=0 checkbox=\"[ ]\" display=\"foo\" strike=- link=- map=ok
task_tabs[0] ListItem ordered=true number=1 task=unchecked indent=0 checkbox=\"[ ]\" display=\"bar\" strike=- link=- map=ok";
        assert_eq!(observed, expected);
    }

    #[test]
    fn task_eol_is_not_a_task() {
        for (source, line) in [
            (
                "1. [ ]",
                "ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"[ ]\" strike=- link=- map=ok",
            ),
            (
                "1. [x]",
                "ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"[x]\" strike=- link=- map=ok",
            ),
            (
                "1. [X]",
                "ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"[X]\" strike=- link=- map=ok",
            ),
            (
                "1) [ ]",
                "ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"[ ]\" strike=- link=- map=ok",
            ),
            (
                "- [ ]",
                "ListItem ordered=false number=0 task=none indent=0 checkbox=- display=\"[ ]\" strike=- link=- map=ok",
            ),
            (
                "1. [ ] ",
                "ListItem ordered=true number=1 task=unchecked indent=0 checkbox=\"[ ]\" display=\"\" strike=- link=- map=ok",
            ),
            (
                "1. [ ]\t",
                "ListItem ordered=true number=1 task=unchecked indent=0 checkbox=\"[ ]\" display=\"\" strike=- link=- map=ok",
            ),
        ] {
            assert_eq!(observe_line(source, 0), line, "{source:?}");
        }
    }

    #[test]
    fn list_padding_is_one_to_four_columns() {
        for (source, ordered, task) in [
            ("1.  foo", true, "none"),
            ("1.   foo", true, "none"),
            ("1.    foo", true, "none"),
            ("1)  foo", true, "none"),
            ("1. \tfoo", true, "none"),
            ("1.  [ ] foo", true, "unchecked"),
            ("1.   [ ] foo", true, "unchecked"),
            ("1.    [ ] foo", true, "unchecked"),
            ("-  [ ] foo", false, "unchecked"),
        ] {
            let checkbox = if task == "none" {
                "-".to_string()
            } else {
                "\"[ ]\"".to_string()
            };
            let number = usize::from(ordered);
            assert_eq!(
                observe_line(source, 0),
                format!(
                    "ListItem ordered={ordered} number={number} task={task} indent=0 checkbox={checkbox} display=\"foo\" strike=- link=- map=ok"
                ),
                "{source:?}"
            );
        }
        assert_eq!(
            observe_line("1.     foo", 0),
            "ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"    foo\" strike=- link=- map=ok"
        );
        assert_eq!(
            observe_line("1.     [ ] foo", 0),
            "ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"    [ ] foo\" strike=- link=- map=ok"
        );
    }

    /// A CR LF ending is not part of the line text. The marker scan still
    /// treats that ending as the end of the marker, so these stay list items
    /// and the display is the text before the CR.
    #[test]
    fn trailing_cr_stays_in_the_editor_line() {
        assert_eq!(
            observe_line("1.\r\n", 0),
            "ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"\" strike=- link=- map=ok"
        );
        assert_eq!(
            observe_line("1)\r\n", 0),
            "ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"\" strike=- link=- map=ok"
        );
        assert_eq!(
            observe_line("1. foo\r\n", 0),
            "ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"foo\" strike=- link=- map=ok"
        );
        assert_eq!(
            observe_line("1. [ ]\r\n", 0),
            "ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"[ ]\" strike=- link=- map=ok"
        );
        assert_eq!(
            observe_line("1. [ ] \r\n", 0),
            "ListItem ordered=true number=1 task=unchecked indent=0 checkbox=\"[ ]\" display=\"\" strike=- link=- map=ok"
        );
    }

    #[test]
    fn unordered_task_uses_task_marker() {
        assert_eq!(
            observe_line("- [ ]\tfoo", 0),
            "ListItem ordered=false number=0 task=unchecked indent=0 checkbox=\"[ ]\" display=\"foo\" strike=- link=- map=ok"
        );
        assert_eq!(
            observe_line("- [ ]  foo", 0),
            "ListItem ordered=false number=0 task=unchecked indent=0 checkbox=\"[ ]\" display=\"foo\" strike=- link=- map=ok"
        );
        assert_eq!(
            observe_line("* [x] bar", 0),
            "ListItem ordered=false number=0 task=checked indent=0 checkbox=\"[x]\" display=\"bar\" strike=- link=- map=ok"
        );
        assert_eq!(
            observe_line("+ [ ] baz", 0),
            "ListItem ordered=false number=0 task=unchecked indent=0 checkbox=\"[ ]\" display=\"baz\" strike=- link=- map=ok"
        );
    }

    #[test]
    fn task_bracket_accepts_tab_vt_ff() {
        for (source, checkbox) in [
            ("1. [\t] foo", "\"[\\t]\""),
            ("1. [\u{b}] foo", "\"[\\u{b}]\""),
            ("1. [\u{c}] foo", "\"[\\u{c}]\""),
        ] {
            assert_eq!(
                observe_line(source, 0),
                format!(
                    "ListItem ordered=true number=1 task=unchecked indent=0 checkbox={checkbox} display=\"foo\" strike=- link=- map=ok"
                ),
                "{source:?}"
            );
        }
        assert_eq!(
            observe_line("1. [ ]\u{a0}foo", 0),
            "ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"[ ]\\u{a0}foo\" strike=- link=- map=ok"
        );
    }

    /// Examples 285 and 367: an empty marker does not interrupt a paragraph.
    #[test]
    fn empty_marker_does_not_interrupt_paragraph() {
        let ex285 = observe_doc("ex285", "foo\n*\n\nfoo\n1.\n");
        assert_eq!(
            ex285,
            "\
ex285[0] Paragraph display=\"foo\" strike=- link=- map=ok
ex285[1] Paragraph display=\"*\" strike=- link=- map=ok
ex285[2] Empty display=\"\" strike=- link=- map=ok
ex285[3] Paragraph display=\"foo\" strike=- link=- map=ok
ex285[4] Paragraph display=\"1.\" strike=- link=- map=ok
ex285[5] Empty display=\"\" strike=- link=- map=ok"
        );
        let ex367 = observe_doc("ex367", "*foo bar\n*\n");
        assert_eq!(
            ex367,
            "\
ex367[0] Paragraph display=\"*foo bar\" strike=- link=- map=ok
ex367[1] Paragraph display=\"*\" strike=- link=- map=ok
ex367[2] Empty display=\"\" strike=- link=- map=ok"
        );
        let dash = observe_doc("dash", "foo\n-\n");
        assert_eq!(
            dash,
            "\
dash[0] Paragraph display=\"foo\" strike=- link=- map=ok
dash[1] Paragraph display=\"-\" strike=- link=- map=ok
dash[2] Empty display=\"\" strike=- link=- map=ok"
        );
    }

    /// Example 304: after paragraph text, only an ordered marker with start 1 interrupts.
    #[test]
    fn ordered_marker_interrupts_only_when_start_is_one() {
        let paren = observe_doc(
            "paren304",
            "The number of windows in my house is\n14)  The number of doors is 6.\n",
        );
        assert_eq!(
            paren,
            "\
paren304[0] Paragraph display=\"The number of windows in my house is\" strike=- link=- map=ok
paren304[1] Paragraph display=\"14)  The number of doors is 6.\" strike=- link=- map=ok
paren304[2] Empty display=\"\" strike=- link=- map=ok"
        );
        let dot = observe_doc(
            "ex304",
            "The number of windows in my house is\n14.  The number of doors is 6.\n",
        );
        assert_eq!(
            dot,
            "\
ex304[0] Paragraph display=\"The number of windows in my house is\" strike=- link=- map=ok
ex304[1] Paragraph display=\"14.  The number of doors is 6.\" strike=- link=- map=ok
ex304[2] Empty display=\"\" strike=- link=- map=ok"
        );
        let start = observe_doc(
            "ex305",
            "The number of windows in my house is\n1.  The number of doors is 6.\n",
        );
        assert_eq!(
            start,
            "\
ex305[0] Paragraph display=\"The number of windows in my house is\" strike=- link=- map=ok
ex305[1] ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"The number of doors is 6.\" strike=- link=- map=ok
ex305[2] Empty display=\"\" strike=- link=- map=ok"
        );
        let start_paren = observe_doc(
            "paren305",
            "The number of windows in my house is\n1)  The number of doors is 6.\n",
        );
        assert_eq!(
            start_paren,
            "\
paren305[0] Paragraph display=\"The number of windows in my house is\" strike=- link=- map=ok
paren305[1] ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"The number of doors is 6.\" strike=- link=- map=ok
paren305[2] Empty display=\"\" strike=- link=- map=ok"
        );
    }

    /// A marker at column 4 or later is not a list item unless a parent item contains it.
    #[test]
    fn marker_at_column_four_is_not_a_list_item() {
        let tabbed = observe_doc("tab", " \t63.\tnot\n");
        assert_eq!(
            tabbed,
            "\
tab[0] Paragraph display=\" \\t63.\\tnot\" strike=- link=- map=ok
tab[1] Empty display=\"\" strike=- link=- map=ok"
        );
        let indented = observe_doc("code", "    - code\n");
        assert_eq!(
            indented,
            "\
code[0] Paragraph display=\"    - code\" strike=- link=- map=ok
code[1] Empty display=\"\" strike=- link=- map=ok"
        );
        let nested = observe_doc("nest", "10) foo\n    - bar\n");
        assert_eq!(
            nested,
            "\
nest[0] ListItem ordered=true number=10 task=none indent=0 checkbox=- display=\"foo\" strike=- link=- map=ok
nest[1] ListItem ordered=false number=0 task=none indent=4 checkbox=- display=\"bar\" strike=- link=- map=ok
nest[2] Empty display=\"\" strike=- link=- map=ok"
        );
    }

    #[test]
    fn checkbox_click_ticks_tab_vt_ff_and_unticks() {
        fn click(source: &str) -> String {
            let doc = parse_document(source);
            let LineKind::ListItem {
                checkbox_source: Some(range),
                ..
            } = &doc.lines[0].kind
            else {
                panic!("no checkbox in {source:?}");
            };
            let replacement = checkbox_click_replacement(&source[range.clone()]);
            let mut out = source.to_string();
            out.replace_range(range.clone(), replacement);
            out
        }

        assert_eq!(click("1. [\t] a"), "1. [x] a");
        assert_eq!(click("1. [\u{b}] a"), "1. [x] a");
        assert_eq!(click("1. [\u{c}] a"), "1. [x] a");
        assert_eq!(click("1) [\t] a"), "1) [x] a");
        assert_eq!(click("- [\t] a"), "- [x] a");
        assert_eq!(click("1. [ ] a"), "1. [x] a");
        assert_eq!(click("1. [x] a"), "1. [ ] a");
        assert_eq!(click("1. [X] a"), "1. [ ] a");
        assert_eq!(click(&click("1. [\t] a")), "1. [ ] a");
        assert_eq!(click(&click("1. [\u{b}] a")), "1. [ ] a");
        assert_eq!(click(&click("1. [\u{c}] a")), "1. [ ] a");
    }

    /// CommonMark list items, exception 1 (spec lines 4123–4129): an empty
    /// marker, or an ordered marker whose start is not 1, does not interrupt
    /// paragraph continuation text. That includes the paragraph inside a
    /// parent item. A blank line ends the paragraph, and a start of 1 with
    /// text still opens a nested list.
    #[test]
    fn nested_marker_does_not_interrupt_item_paragraph() {
        assert_eq!(
            observe_doc("star", "- foo\n  *\n"),
            "\
star[0] ListItem ordered=false number=0 task=none indent=0 checkbox=- display=\"foo\" strike=- link=- map=ok
star[1] Paragraph display=\"  *\" strike=- link=- map=ok
star[2] Empty display=\"\" strike=- link=- map=ok"
        );
        assert_eq!(
            observe_doc("one", "- foo\n  1.\n"),
            "\
one[0] ListItem ordered=false number=0 task=none indent=0 checkbox=- display=\"foo\" strike=- link=- map=ok
one[1] Paragraph display=\"  1.\" strike=- link=- map=ok
one[2] Empty display=\"\" strike=- link=- map=ok"
        );
        assert_eq!(
            observe_doc("two", "- foo\n  bar\n  2) baz\n"),
            "\
two[0] ListItem ordered=false number=0 task=none indent=0 checkbox=- display=\"foo\" strike=- link=- map=ok
two[1] Paragraph display=\"  bar\" strike=- link=- map=ok
two[2] Paragraph display=\"  2) baz\" strike=- link=- map=ok
two[3] Empty display=\"\" strike=- link=- map=ok"
        );
        assert_eq!(
            observe_doc("fourteen", "- parent\n  14) a\n"),
            "\
fourteen[0] ListItem ordered=false number=0 task=none indent=0 checkbox=- display=\"parent\" strike=- link=- map=ok
fourteen[1] Paragraph display=\"  14) a\" strike=- link=- map=ok
fourteen[2] Empty display=\"\" strike=- link=- map=ok"
        );
        assert_eq!(
            observe_doc("checked", "- parent\n  14) [x] a\n"),
            "\
checked[0] ListItem ordered=false number=0 task=none indent=0 checkbox=- display=\"parent\" strike=- link=- map=ok
checked[1] Paragraph display=\"  14) [x] a\" strike=- link=- map=ok
checked[2] Empty display=\"\" strike=- link=- map=ok"
        );
        assert_eq!(
            observe_doc("blank", "- parent\n\n  *\n"),
            "\
blank[0] ListItem ordered=false number=0 task=none indent=0 checkbox=- display=\"parent\" strike=- link=- map=ok
blank[1] Empty display=\"\" strike=- link=- map=ok
blank[2] ListItem ordered=false number=0 task=none indent=2 checkbox=- display=\"\" strike=- link=- map=ok
blank[3] Empty display=\"\" strike=- link=- map=ok"
        );
        assert_eq!(
            observe_doc("start", "- foo\n  1. x\n"),
            "\
start[0] ListItem ordered=false number=0 task=none indent=0 checkbox=- display=\"foo\" strike=- link=- map=ok
start[1] ListItem ordered=true number=1 task=none indent=2 checkbox=- display=\"x\" strike=- link=- map=ok
start[2] Empty display=\"\" strike=- link=- map=ok"
        );
        assert_eq!(
            observe_doc("after", "- parent\n\n  14) a\n"),
            "\
after[0] ListItem ordered=false number=0 task=none indent=0 checkbox=- display=\"parent\" strike=- link=- map=ok
after[1] Empty display=\"\" strike=- link=- map=ok
after[2] ListItem ordered=true number=14 task=none indent=2 checkbox=- display=\"a\" strike=- link=- map=ok
after[3] Empty display=\"\" strike=- link=- map=ok"
        );
    }

    /// Laziness (spec lines 4814–4821): `bar` continues the item, and `2. baz`
    /// then starts a new list. `foo` then `2. baz` stays one paragraph
    /// (example 304).
    #[test]
    fn ordered_marker_after_dedent_stays_list() {
        assert_eq!(
            observe_doc("lazy", "- foo\nbar\n2. baz\n"),
            "\
lazy[0] ListItem ordered=false number=0 task=none indent=0 checkbox=- display=\"foo\" strike=- link=- map=ok
lazy[1] Paragraph display=\"bar\" strike=- link=- map=ok
lazy[2] ListItem ordered=true number=2 task=none indent=0 checkbox=- display=\"baz\" strike=- link=- map=ok
lazy[3] Empty display=\"\" strike=- link=- map=ok"
        );
        assert_eq!(
            observe_doc("para", "foo\n2. baz\n"),
            "\
para[0] Paragraph display=\"foo\" strike=- link=- map=ok
para[1] Paragraph display=\"2. baz\" strike=- link=- map=ok
para[2] Empty display=\"\" strike=- link=- map=ok"
        );
    }

    /// Example 283 with CR LF: `2.\r` is an empty item, so `3. bar` stays item 3.
    #[test]
    fn crlf_empty_item_does_not_drop_next_item() {
        assert_eq!(
            observe_doc("crlf", "1. foo\r\n2.\r\n3. bar"),
            "\
crlf[0] ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"foo\" strike=- link=- map=ok
crlf[1] ListItem ordered=true number=2 task=none indent=0 checkbox=- display=\"\" strike=- link=- map=ok
crlf[2] ListItem ordered=true number=3 task=none indent=0 checkbox=- display=\"bar\" strike=- link=- map=ok"
        );
        assert_eq!(
            observe_doc("bare", "1.\rfoo"),
            "\
bare[0] ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"\" strike=- link=- map=ok
bare[1] Paragraph display=\"foo\" strike=- link=- map=ok"
        );
    }

    /// Exact `---`, `***`, and `___` are rules, including after HTML, which is
    /// BASE (no HTML-block guard). Every other shape is the paragraph or list
    /// BASE already parses.
    #[test]
    fn extended_rules_inside_html_stay_visible_text() {
        let outside_src = "Foo\n****\n";
        let outside = parse_document(outside_src);
        assert!(matches!(outside.lines[1].kind, LineKind::Paragraph));
        assert_eq!(outside.lines[1].display_text, "****");
        assert_eq!(&outside_src[outside.lines[1].source.clone()], "****");
        let kept = "- item\n<br>\n---\n";
        let list_rule = parse_document(kept);
        assert!(matches!(list_rule.lines[2].kind, LineKind::HorizontalRule));
        assert_eq!(list_rule.lines[2].display_text, "");
        assert_eq!(&kept[list_rule.lines[2].source.clone()], "---");
        let indented = "- item\n  <div>\n---\n";
        let indented_rule = parse_document(indented);
        assert!(matches!(
            indented_rule.lines[2].kind,
            LineKind::HorizontalRule
        ));
        assert_eq!(indented_rule.lines[2].display_text, "");
        assert_eq!(&indented[indented_rule.lines[2].source.clone()], "---");
        let indented_blank = "- item\n\n  <div>\n---\n";
        let blank_rule = parse_document(indented_blank);
        assert!(matches!(blank_rule.lines[3].kind, LineKind::HorizontalRule));
        assert_eq!(blank_rule.lines[3].display_text, "");
        assert_eq!(&indented_blank[blank_rule.lines[3].source.clone()], "---");
        let div_rule = "<div>\n---\n";
        let div_doc = parse_document(div_rule);
        assert_eq!(&div_rule[div_doc.lines[1].source.clone()], "---");
        assert_eq!(div_doc.lines[1].display_text, "");
        assert!(matches!(div_doc.lines[1].kind, LineKind::HorizontalRule));

        let cases = [
            ("<pre>\n****\n", 1, "****"),
            ("<div>\n****\n", 1, "****"),
            ("<!--\n****\n", 1, "****"),
            ("<script>\n****\n", 1, "****"),
            ("<pre>\n____\n", 1, "____"),
            ("<pre>\n_ _ _\n", 1, "_ _ _"),
            ("<pre>\n\n****\n", 2, "****"),
            ("Para\n<pre>\n****\n", 2, "****"),
            ("<pre>\n   ****\n", 1, "   ****"),
            ("<pre>\r****\r", 1, "****"),
        ];
        for (src, index, display) in cases {
            let doc = parse_document(src);
            let line = &doc.lines[index];
            assert_eq!(&src[line.source.clone()], display, "{src:?}");
            assert_eq!(line.display_text, display, "{src:?}");
            assert!(
                matches!(line.kind, LineKind::Paragraph),
                "{src:?} kind {:?}",
                line.kind
            );
        }

        let stars = "<pre>\n* * *\n";
        let star_doc = parse_document(stars);
        assert_eq!(&stars[star_doc.lines[1].source.clone()], "* * *");
        assert_eq!(star_doc.lines[1].display_text, "* *");
        assert!(matches!(
            star_doc.lines[1].kind,
            LineKind::ListItem {
                ordered: false,
                indent_cols: 0,
                ..
            }
        ));
        let dashes = "<pre>\n- - -\n";
        let dash_doc = parse_document(dashes);
        assert_eq!(&dashes[dash_doc.lines[1].source.clone()], "- - -");
        assert_eq!(dash_doc.lines[1].display_text, "- -");
        assert!(matches!(
            dash_doc.lines[1].kind,
            LineKind::ListItem {
                ordered: false,
                indent_cols: 0,
                ..
            }
        ));
        let tabs = "<pre>\n*\t*\t*\n";
        let tab_doc = parse_document(tabs);
        assert_eq!(&tabs[tab_doc.lines[1].source.clone()], "*\t*\t*");
        assert_eq!(tab_doc.lines[1].display_text, "*\t*");
        assert!(matches!(
            tab_doc.lines[1].kind,
            LineKind::ListItem {
                ordered: false,
                indent_cols: 0,
                ..
            }
        ));
    }

    /// R6-9. Four spaces before `---` is indented text once a bare CR is a
    /// line break. This parser has no indented-code block (BASE shows
    /// `    indented` as a paragraph). Pulldown, which does not split CR,
    /// returns one code block for the unsplit bytes. The line must not be a
    /// rule: display keeps `    ---`, then paragraph `next`.
    #[test]
    fn four_space_dash_rule_after_bare_cr_stays_text() {
        let src = "    ---\rnext\r";
        let doc = parse_document(src);
        assert!(
            !matches!(doc.lines[0].kind, LineKind::HorizontalRule),
            "kind {:?}",
            doc.lines[0].kind
        );
        assert!(matches!(doc.lines[0].kind, LineKind::Paragraph));
        assert_eq!(doc.lines[0].display_text, "    ---");
        assert_eq!(&src[doc.lines[0].source.clone()], "    ---");
        assert!(matches!(doc.lines[1].kind, LineKind::Paragraph));
        assert_eq!(doc.lines[1].display_text, "next");
        assert_eq!(&src[doc.lines[1].source.clone()], "next");
    }

    /// A blank line leaves the item open. Four spaces is two past `- `'s
    /// content column, so the line is a rule. The same bytes at column 0
    /// stay a paragraph.
    #[test]
    fn four_space_rule_inside_an_open_list_is_a_rule() {
        let top = "    ---\nnext\n";
        let top_doc = parse_document(top);
        assert!(matches!(top_doc.lines[0].kind, LineKind::Paragraph));
        assert_eq!(top_doc.lines[0].display_text, "    ---");

        let cases = [
            ("- item\n\n    ---\n", 2),
            ("1. item\n\n    ---\n", 2),
            ("10) item\n\n    ---\n", 2),
            ("- [ ] item\n\n    ---\n", 2),
            ("1. [ ] item\n\n    ---\n", 2),
            ("- a\n  - b\n\n    ---\n", 3),
            ("- a\n  - b\n    <!DOCTYPE html>\n    ---\n", 3),
        ];
        for (src, index) in cases {
            let doc = parse_document(src);
            assert!(
                matches!(doc.lines[index].kind, LineKind::HorizontalRule),
                "{src:?} kind {:?}",
                doc.lines[index].kind
            );
            assert_eq!(doc.lines[index].display_text, "", "{src:?}");
            assert_eq!(&src[doc.lines[index].source.clone()], "    ---", "{src:?}");
        }
    }

    /// K1. A tab inside an open list is a column stop of 4, same as BASE
    /// `leading_column`. Extra indent past the item content column is 2, so
    /// the line is a rule. A tab at column 0 stays a paragraph.
    #[test]
    fn tab_rule_inside_an_open_list_is_a_rule() {
        let top = "\t---\n";
        let top_doc = parse_document(top);
        assert!(matches!(top_doc.lines[0].kind, LineKind::Paragraph));
        assert_eq!(top_doc.lines[0].display_text, "\t---");
        assert_eq!(&top[top_doc.lines[0].source.clone()], "\t---");

        let cases = [
            ("- item\n\n\t---\n", 2, "\t---"),
            ("- item\n\t___\n", 1, "\t___"),
            ("* item\n\t***\n", 1, "\t***"),
        ];
        for (src, index, source) in cases {
            let doc = parse_document(src);
            assert!(
                matches!(doc.lines[index].kind, LineKind::HorizontalRule),
                "{src:?} kind {:?}",
                doc.lines[index].kind
            );
            assert_eq!(doc.lines[index].display_text, "", "{src:?}");
            assert_eq!(&src[doc.lines[index].source.clone()], source, "{src:?}");
        }
    }

    /// BASE `1d03313` display of `# foo #\t` is `foo #\t` (the tab stays, and
    /// the hash is text). `#\t#` stays a heading whose display is `#`.
    #[test]
    fn tab_after_a_closing_hash_stays_in_the_heading_text() {
        let doc = parse_document("# foo #\t");
        assert!(matches!(doc.lines[0].kind, LineKind::Heading { level: 1 }));
        assert_eq!(doc.lines[0].display_text, "foo #\t");

        let kept = parse_document("# foo\t#");
        assert_eq!(kept.lines[0].display_text, "foo\t#");

        let trailed = parse_document("# foo\t");
        assert_eq!(trailed.lines[0].display_text, "foo\t");

        let spaces = parse_document("# foo #   ");
        assert_eq!(spaces.lines[0].display_text, "foo");

        let empty = parse_document("### ###");
        assert_eq!(empty.lines[0].display_text, "");

        let tab_hash = parse_document("#\t#");
        assert!(matches!(
            tab_hash.lines[0].kind,
            LineKind::Heading { level: 1 }
        ));
        assert_eq!(tab_hash.lines[0].display_text, "#");
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
        // pulldown 0.12.2: code-open: / code-line / code-close.
        // HTML after the table: <pre><code>code\n</code></pre>
        // Main's leaf rule keeps tilde lines as paragraphs, not fences.
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
        // Main builds a ListItem for a paren ordered marker.
        check(
            "paren-list",
            "| a | b |\n| --- | --- |\n| c | d |\n1) item",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] ListItem display="item" code=[]"#,
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
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
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
        // Main leaves a four-column marker outside a parent item as text.
        check(
            "indent4-list",
            "| a | b |\n| --- | --- |\n| c | d |\n    - item",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] Paragraph display="    - item" code=[]"#,
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
        // Main's leaf rule keeps tilde lines as paragraphs, not code.
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
        // Main opens its trimmed backtick fence after the table.
        check(
            "tail-fence-indent4",
            "| a | b |\n| --- | --- |\n| c | d |\n    ```\ncode",
            &[
                r#"line[0] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[1] TableDelimiter display="" code=[]"#,
                r#"line[2] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[3] CodeFence display="" code=[]"#,
                r#"line[4] CodeLine display="code" code=["code"]"#,
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
        // Main's leaf rule keeps tilde lines as paragraphs, so the table
        // between them stays a table even though pulldown fenced it.
        check(
            "inside-tilde",
            "~~~\n| a | b |\n| --- | --- |\n| c | d |\n~~~",
            &[
                r#"line[0] Paragraph display="~~~" code=[]"#,
                r#"line[1] TableRow header=true align=[none,none] cells=["a", "b"] display="| a | b |" code=[]"#,
                r#"line[2] TableDelimiter display="" code=[]"#,
                r#"line[3] TableRow header=false align=[none,none] cells=["c", "d"] display="| c | d |" code=[]"#,
                r#"line[4] Paragraph display="~~~" code=[]"#,
            ],
            true,
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
            vec![2],
            "inside-tilde"
        );
        assert_eq!(
            table_rows("| a | b |\n| --- | --- |\n| c | d |\n\u{000b}"),
            vec![2],
            "tail-vtab"
        );
    }
}
