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

        // Horizontal rule ---, ***, ___
        if is_thematic_break(line)
            || trimmed == "---"
            || trimmed == "***"
            || trimmed == "___"
        {
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

        // Markdown Table | col1 | col2 |
        if trimmed.starts_with('|') && trimmed.ends_with('|') {
            let table_start = lines[i].start;
            let mut table_rows = Vec::new();
            let mut table_end = lines[i].end;
            while i < lines.len() {
                let cur = lines[i].text.trim();
                if cur.starts_with('|') && cur.ends_with('|') {
                    table_end = lines[i].end;
                    // Check if it is delimiter row |---|---|
                    let is_separator = cur
                        .chars()
                        .all(|c| c == '|' || c == '-' || c == ':' || c == ' ');
                    if !is_separator {
                        table_rows.push(split_table_cells(cur));
                    }
                    i += 1;
                } else {
                    break;
                }
            }

            if !table_rows.is_empty() {
                let headers = table_rows.remove(0);
                blocks.push(Block::Table {
                    headers,
                    rows: table_rows,
                    source: table_start..table_end,
                });
            }
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

/// Byte offsets of `|` that are not escaped with a backslash.
pub fn unescaped_pipe_offsets(line: &str) -> Vec<usize> {
    let bytes = line.as_bytes();
    let mut offsets = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'|' {
            let mut slashes = 0usize;
            let mut look = index;
            while look > 0 && bytes[look - 1] == b'\\' {
                slashes += 1;
                look -= 1;
            }
            if slashes.is_multiple_of(2) {
                offsets.push(index);
            }
        }
        index += 1;
    }
    offsets
}

/// Cell text between unescaped pipes. `\|` stays inside the cell.
pub fn split_table_cells(line: &str) -> Vec<String> {
    let pipes = unescaped_pipe_offsets(line);
    if pipes.len() < 2 {
        return Vec::new();
    }
    pipes
        .windows(2)
        .map(|pair| line[pair[0] + 1..pair[1]].trim().to_string())
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

struct OpenFence {
    /// Column-0..3 backtick fence. A shorter run does not close it.
    strict: bool,
    len: usize,
    indent: usize,
    open_range: std::ops::Range<usize>,
}

/// Thematic break: 0–3 spaces, then three or more of one of `-` `_` `*`,
/// with spaces or tabs allowed between them.
fn is_thematic_break(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut index = 0;
    let mut spaces = 0;
    while index < bytes.len() && bytes[index] == b' ' {
        spaces += 1;
        if spaces > 3 {
            return false;
        }
        index += 1;
    }
    if index >= bytes.len() {
        return false;
    }
    let marker = bytes[index];
    if marker != b'-' && marker != b'_' && marker != b'*' {
        return false;
    }
    let mut count = 0;
    while index < bytes.len() {
        if bytes[index] == b' ' || bytes[index] == b'\t' {
            index += 1;
        } else if bytes[index] == marker {
            count += 1;
            index += 1;
        } else {
            return false;
        }
    }
    count >= 3
}

/// A setext underline: 0–3 spaces, then one run of `=` or `-`, then spaces or tabs.
/// This is not a heading. It only stops an extended thematic break from hiding the line.
fn setext_underline_line(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut index = 0;
    let mut spaces = 0;
    while index < bytes.len() && bytes[index] == b' ' {
        spaces += 1;
        if spaces > 3 {
            return false;
        }
        index += 1;
    }
    if index >= bytes.len() {
        return false;
    }
    let marker = bytes[index];
    if marker != b'-' && marker != b'=' {
        return false;
    }
    let marker_at = index;
    while index < bytes.len() && bytes[index] == marker {
        index += 1;
    }
    if index == marker_at {
        return false;
    }
    bytes[index..]
        .iter()
        .all(|byte| *byte == b' ' || *byte == b'\t')
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
fn in_list_or_quote(lines: &[ParsedLine]) -> bool {
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
        while end > content_start && (bytes[end - 1] == b' ' || bytes[end - 1] == b'\t') {
            end -= 1;
        }
    }
    while content_start < end && (bytes[content_start] == b' ' || bytes[content_start] == b'\t') {
        content_start += 1;
    }
    Some((level, content_start..end))
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
/// A setext-shaped dash run is included: it interrupts a list even when the
/// line loop later keeps it as a paragraph under a real paragraph.
fn is_structural_line(line: &str) -> bool {
    if atx_heading(line).is_some() || is_thematic_break(line) {
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
        if is_structural_line(line) {
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
    let mut all_hidden = Vec::new();
    let mut all_pairs = Vec::new();
    let mut code_blocks = Vec::new();

    let mut open_fence: Option<OpenFence> = None;

    // Pre-detect table block ranges and column alignments
    let mut table_ranges: Vec<(usize, usize, std::ops::Range<usize>, Vec<TableColumnAlign>)> =
        Vec::new();
    {
        let mut idx = 0;
        while idx < raw_lines.len() {
            let t = raw_lines[idx].text.trim();
            if t.starts_with('|') && t.ends_with('|') && t.len() >= 2 {
                let start_idx = idx;
                let byte_start = raw_lines[idx].start;
                let mut byte_end = raw_lines[idx].start + raw_lines[idx].text.len();
                let mut alignments = Vec::new();
                while idx < raw_lines.len() {
                    let cur = raw_lines[idx].text.trim();
                    if cur.starts_with('|') && cur.ends_with('|') && cur.len() >= 2 {
                        byte_end = raw_lines[idx].start + raw_lines[idx].text.len();
                        let is_sep = cur.contains('-')
                            && cur
                                .chars()
                                .all(|c| c == '|' || c == '-' || c == ':' || c == ' ');
                        if is_sep && alignments.is_empty() {
                            alignments = split_table_cells(cur)
                                .iter()
                                .map(|cell| table_column_align(cell))
                                .collect();
                        }
                        idx += 1;
                    } else {
                        break;
                    }
                }
                table_ranges.push((start_idx, idx - 1, byte_start..byte_end, alignments));
            } else {
                idx += 1;
            }
        }
    }

    let mut list_walk = ListWalk::default();
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
                lines.push(parsed);
            } else {
                let strip = open_fence
                    .as_ref()
                    .filter(|fence| fence.strict)
                    .map(|fence| prefix_through_column(line, fence.indent))
                    .unwrap_or(0);
                let parsed = code_content_line(i, line_source, line, strip);
                all_hidden.extend(parsed.hidden_ranges.iter().cloned());
                lines.push(parsed);
            }
            continue;
        }
        let open_top = if in_list_or_quote(&lines) || backtick_info_rejects_fence(line) {
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
            lines.push(parsed);
            continue;
        }
        let skip_rejected = !in_list_or_quote(&lines) && backtick_info_rejects_fence(line);
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
            lines.push(parsed);
            continue;
        }

        if trimmed.is_empty() {
            list_walk.note_blank();
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

        // Horizontal rule. Red's exact trim match stays. A longer or spaced
        // break stays a paragraph when it is a setext underline of the
        // previous paragraph, which red also leaves as a paragraph.
        let red_exact_rule = trimmed == "---" || trimmed == "***" || trimmed == "___";
        let setext_underline = lines
            .last()
            .is_some_and(|prev| matches!(prev.kind, LineKind::Paragraph))
            && setext_underline_line(line);
        if red_exact_rule || (is_thematic_break(line) && !setext_underline) {
            list_walk.note_block(line);
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

        // Table rows
        if let Some(&(tbl_first, _tbl_last, ref tbl_src, ref tbl_aligns)) =
            table_ranges.iter().find(|&&(f, l, _, _)| i >= f && i <= l)
        {
            list_walk.note_block(line);
            let is_separator = trimmed.contains('-')
                && trimmed
                    .chars()
                    .all(|c| c == '|' || c == '-' || c == ':' || c == ' ');
            if is_separator {
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
                // Parse cell ranges inside `| cell1 | cell2 |`
                let is_header = i == tbl_first;
                let mut cells = Vec::new();
                let mut raw_cells = Vec::new();
                let mut row_hidden = Vec::new();
                let mut row_pairs = Vec::new();
                let mut row_segments = Vec::new();
                let pipe_positions = unescaped_pipe_offsets(line);
                for w in pipe_positions.windows(2) {
                    let raw_c_start = w[0] + 1;
                    let raw_c_end = w[1];
                    raw_cells.push((line_start + raw_c_start)..(line_start + raw_c_end));
                    let cell_slice = &line[raw_c_start..raw_c_end];
                    let lead = usize::from(cell_slice.starts_with(' '));
                    let rem = &cell_slice[lead..];
                    let trail = usize::from(rem.ends_with(' '));
                    let c_start = line_start + raw_c_start + lead;
                    let c_end = line_start + raw_c_end - trail;
                    cells.push(c_start..c_end);
                    if c_end > c_start {
                        let inner_slice = &line[(c_start - line_start)..(c_end - line_start)];
                        let (c_hid, c_pairs, c_segs) =
                            parse_inline_syntax(inner_slice, c_start);
                        row_hidden.extend(c_hid);
                        row_pairs.extend(c_pairs);
                        row_segments.extend(c_segs);
                    }
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
                let alignments: Vec<TableColumnAlign> = (0..cells.len())
                    .map(|col_idx| tbl_aligns.get(col_idx).copied().unwrap_or_default())
                    .collect();
                lines.push(ParsedLine {
                    line_index: i,
                    source: line_source.clone(),
                    kind: LineKind::TableRow {
                        is_header,
                        cells,
                        raw_cells,
                        alignments,
                        table_source: tbl_src.clone(),
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

        if let Some((level, content)) = atx_heading(line) {
            list_walk.note_block(line);
            let parsed = atx_line(i, line_source, line, level, content);
            all_hidden.extend(parsed.hidden_ranges.iter().cloned());
            all_pairs.extend(parsed.inline_pairs.iter().cloned());
            lines.push(parsed);
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
                vec![leaf("hr", "", "_____________________________________")],
            ),
            ("ex51-hr-spaced", " - - -", vec![leaf("hr", "", " - - -")]),
            (
                "ex52-hr-stars",
                " **  * ** * ** * **",
                vec![leaf("hr", "", " **  * ** * ** * **")],
            ),
            (
                "ex53-hr-spaced-dashes",
                "-     -      -      -",
                vec![leaf("hr", "", "-     -      -      -")],
            ),
            (
                "ex54-hr-trailing-space",
                "- - - -    ",
                vec![leaf("hr", "", "- - - -    ")],
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
                    leaf("hr", "", "* * *"),
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
                "ex65-escaped-atx",
                "\\## foo",
                vec![leaf("p", "\\## foo", "\\## foo")],
            ),
            (
                "ex66-atx-inline",
                "# foo *bar* \\*baz\\*",
                vec![leaf_marks(
                    "h1",
                    "foo bar \\*baz\\*",
                    "# foo *bar* \\*baz\\*",
                    "I:bar",
                )],
            ),
            (
                "ex67-atx-trim",
                "#                  foo                     ",
                vec![leaf(
                    "h1",
                    "foo",
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
                    leaf("h3", "foo \\###", "### foo \\###"),
                    leaf("h2", "foo #\\##", "## foo #\\##"),
                    leaf("h1", "foo \\#", "# foo \\#"),
                ],
            ),
            (
                "ex77-four-stars",
                "****\n## foo\n****",
                vec![
                    leaf("hr", "", "****"),
                    leaf("h2", "foo", "## foo"),
                    leaf("hr", "", "****"),
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
                    leaf("hr", "", "--- -"),
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
                vec![leaf("p", "Foo", "Foo"), leaf("hr", "", "- - -")],
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
                vec![leaf("hr", "", "----")],
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
}
