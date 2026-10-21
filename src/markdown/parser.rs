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
    /// Setext underline. Collapsed in WYSIWYG the same way as [`LineKind::TableDelimiter`].
    SetextUnderline,
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
                let mut inner = open_d.end..close_d.start;
                raw_hidden.push(open_d.clone());
                raw_hidden.push(close_d.clone());
                // CommonMark drops one leading and one trailing space when a
                // code span both starts and ends with a space and is not all spaces.
                let inner_text = &input[(inner.start - base)..(inner.end - base)];
                if inner_text.len() >= 2
                    && inner_text.starts_with(' ')
                    && inner_text.ends_with(' ')
                    && inner_text.bytes().any(|byte| byte != b' ')
                {
                    raw_hidden.push(inner.start..inner.start + 1);
                    raw_hidden.push((inner.end - 1)..inner.end);
                    inner = (inner.start + 1)..(inner.end - 1);
                }
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
    marker: u8,
    len: usize,
    indent: usize,
    open_range: std::ops::Range<usize>,
    max_indent: usize,
}

/// Setext underline: 0–3 spaces, one or more `=` or `-`, then only spaces or tabs.
fn setext_level(line: &str) -> Option<u8> {
    let bytes = line.as_bytes();
    let mut index = 0;
    let mut spaces = 0;
    while index < bytes.len() && bytes[index] == b' ' {
        spaces += 1;
        if spaces > 3 {
            return None;
        }
        index += 1;
    }
    if index >= bytes.len() {
        return None;
    }
    let marker = bytes[index];
    if marker != b'=' && marker != b'-' {
        return None;
    }
    let start = index;
    while index < bytes.len() && bytes[index] == marker {
        index += 1;
    }
    if index == start
        || !bytes[index..]
            .iter()
            .all(|byte| *byte == b' ' || *byte == b'\t')
    {
        return None;
    }
    Some(if marker == b'=' { 1 } else { 2 })
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

fn match_opening_fence(line: &str) -> Option<(u8, usize, usize, Option<String>)> {
    match_opening_fence_limited(line, 3)
}

fn match_opening_fence_limited(
    line: &str,
    max_indent: usize,
) -> Option<(u8, usize, usize, Option<String>)> {
    let (index, column) = split_indent(line);
    if index >= line.len() || column > max_indent {
        return None;
    }
    opening_fence_parts(line, index, column)
}

/// `index` is the first marker byte. `column` counts tabs to the next multiple of 4.
fn opening_fence_parts(
    line: &str,
    index: usize,
    column: usize,
) -> Option<(u8, usize, usize, Option<String>)> {
    let bytes = line.as_bytes();
    if index >= bytes.len() {
        return None;
    }
    let marker = bytes[index];
    if marker != b'`' && marker != b'~' {
        return None;
    }
    let mut end = index;
    while end < bytes.len() && bytes[end] == marker {
        end += 1;
    }
    let len = end - index;
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
    Some((marker, len, column, info))
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

fn fence_content_strip(line: &str, indent_columns: usize) -> usize {
    prefix_through_column(line, indent_columns)
}

/// Bytes to hide so the rest of the line starts at column 4. Tabs advance to
/// the next stop of 4.
fn indented_code_prefix(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut column = 0usize;
    let mut index = 0usize;
    while index < bytes.len() && column < 4 {
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
    if column >= 4 {
        Some(index)
    } else {
        None
    }
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

struct ListMarker {
    column: usize,
    prefix_len: usize,
    ordered: bool,
    number: usize,
    task: Option<bool>,
    checkbox_rel: Option<usize>,
}

fn list_marker(line: &str) -> Option<ListMarker> {
    let (indent_bytes, column) = split_indent(line);
    let rest = &line[indent_bytes..];
    if rest
        .strip_prefix("- [ ] ")
        .or_else(|| rest.strip_prefix("* [ ] "))
        .or_else(|| rest.strip_prefix("+ [ ] "))
        .is_some()
    {
        return Some(ListMarker {
            column,
            prefix_len: indent_bytes + 6,
            ordered: false,
            number: 0,
            task: Some(false),
            checkbox_rel: Some(indent_bytes + 2),
        });
    }
    if rest
        .strip_prefix("- [x] ")
        .or_else(|| rest.strip_prefix("- [X] "))
        .or_else(|| rest.strip_prefix("* [x] "))
        .or_else(|| rest.strip_prefix("* [X] "))
        .or_else(|| rest.strip_prefix("+ [x] "))
        .or_else(|| rest.strip_prefix("+ [X] "))
        .is_some()
    {
        return Some(ListMarker {
            column,
            prefix_len: indent_bytes + 6,
            ordered: false,
            number: 0,
            task: Some(true),
            checkbox_rel: Some(indent_bytes + 2),
        });
    }
    if rest.starts_with("- ") || rest.starts_with("* ") || rest.starts_with("+ ") {
        return Some(ListMarker {
            column,
            prefix_len: indent_bytes + 2,
            ordered: false,
            number: 0,
            task: None,
            checkbox_rel: None,
        });
    }
    if let Some(dot_pos) = rest.find(". ") {
        if dot_pos > 0 && rest[..dot_pos].bytes().all(|byte| byte.is_ascii_digit()) {
            if let Ok(number) = rest[..dot_pos].parse::<usize>() {
                return Some(ListMarker {
                    column,
                    prefix_len: indent_bytes + dot_pos + 2,
                    ordered: true,
                    number,
                    task: None,
                    checkbox_rel: None,
                });
            }
        }
    }
    None
}

fn continues_list(lines: &[ParsedLine], column: usize) -> bool {
    for prev in lines.iter().rev() {
        match &prev.kind {
            LineKind::Empty => continue,
            LineKind::ListItem { indent_cols, .. } => return *indent_cols < column,
            LineKind::Paragraph
            | LineKind::Heading { .. }
            | LineKind::Blockquote
            | LineKind::CodeFence { .. }
            | LineKind::CodeLine
            | LineKind::HorizontalRule
            | LineKind::SetextUnderline
            | LineKind::TableDelimiter
            | LineKind::TableRow { .. } => return false,
        }
    }
    false
}

fn indented_code_resumes(lines: &[SourceLine<'_>], from: usize, min_col: usize) -> bool {
    for later in lines.iter().skip(from + 1) {
        if later.text.trim().is_empty() {
            continue;
        }
        let (index, column) = split_indent(later.text);
        return index < later.text.len() && column >= min_col;
    }
    false
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

fn column_at_byte(line: &str, byte_index: usize) -> usize {
    let end = byte_index.min(line.len());
    let mut column = 0usize;
    for ch in line[..end].chars() {
        if ch == '\t' {
            column += 4 - (column % 4);
        } else {
            column += 1;
        }
    }
    column
}

fn line_indent_column(line: &str) -> Option<usize> {
    let (index, column) = split_indent(line);
    if index < line.len() {
        Some(column)
    } else {
        None
    }
}

fn list_content_column(line: &str, prefix_len: usize) -> usize {
    let base = column_at_byte(line, prefix_len);
    let after = line.get(prefix_len..).unwrap_or("");
    let (extra, extra_col) = split_indent(after);
    if extra < after.len() {
        base + extra_col
    } else {
        base
    }
}

/// Link definitions and HTML blocks are not classified (LM-04, LM-14).
/// These shapes are enough to leave the line as ordinary text, which is what
/// red did. An unfinished `<a title="a lot` is not one of them.
fn line_blocks_setext(line: &str) -> bool {
    if html_block_kind(line).is_some() {
        return true;
    }
    let trimmed = line.trim_start_matches([' ', '\t']);
    link_label_colon(trimmed)
        || trimmed == "["
        || trimmed.starts_with("]:")
        || complete_tag_line(trimmed)
}

fn line_keeps_following_indent(line: &str) -> bool {
    let trimmed = line.trim_start_matches([' ', '\t']);
    trimmed.starts_with('<') || link_label_colon(trimmed) || trimmed == "[" || trimmed.starts_with("]:")
}

/// `[label]:` with the colon tight against the closing bracket.
fn link_label_colon(trimmed: &str) -> bool {
    let bytes = trimmed.as_bytes();
    if bytes.first() != Some(&b'[') {
        return false;
    }
    let mut index = 1usize;
    let mut escaped = false;
    while index < bytes.len() {
        if escaped {
            escaped = false;
            index += 1;
            continue;
        }
        match bytes[index] {
            b'\\' => {
                escaped = true;
                index += 1;
            }
            b']' => return bytes.get(index + 1) == Some(&b':'),
            _ => index += 1,
        }
    }
    false
}

/// A whole line that is one tag, including `<pre/>`, which the HTML
/// start-condition tests reject because `/` follows the name.
fn complete_tag_line(trimmed: &str) -> bool {
    let bytes = trimmed.as_bytes();
    if bytes.first() != Some(&b'<') {
        return false;
    }
    let Some(end) = trimmed.find('>') else {
        return false;
    };
    bytes[end + 1..]
        .iter()
        .all(|byte| *byte == b' ' || *byte == b'\t')
}

fn is_paren_item_line(line: &str) -> bool {
    let (index, column) = split_indent(line);
    if column > 3 || index >= line.len() {
        return false;
    }
    let bytes = line.as_bytes();
    let mut cursor = index;
    let mut digits = 0usize;
    while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
        digits += 1;
        cursor += 1;
    }
    if digits == 0 || digits > 9 || bytes.get(cursor) != Some(&b')') {
        return false;
    }
    cursor += 1;
    cursor == bytes.len() || bytes[cursor] == b' ' || bytes[cursor] == b'\t'
}

fn paren_marker_prefix_len(line: &str) -> Option<usize> {
    if !is_paren_item_line(line) {
        return None;
    }
    let (index, _) = split_indent(line);
    let bytes = line.as_bytes();
    let mut cursor = index;
    while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
        cursor += 1;
    }
    cursor += 1;
    if cursor < bytes.len() && (bytes[cursor] == b' ' || bytes[cursor] == b'\t') {
        cursor += 1;
    }
    Some(cursor)
}

fn paragraph_run_keeps_indent(lines: &[ParsedLine], markdown: &str) -> bool {
    if paragraph_run_is_paren_item(lines, markdown) {
        return true;
    }
    let Some(start) = paragraph_run_start(lines) else {
        return false;
    };
    lines[start..]
        .iter()
        .any(|line| line_keeps_following_indent(&markdown[line.source.clone()]))
}

fn paragraph_run_is_paren_item(lines: &[ParsedLine], markdown: &str) -> bool {
    let Some(start) = paragraph_run_start(lines) else {
        return false;
    };
    is_paren_item_line(&markdown[lines[start].source.clone()])
}

/// Setext promotes an ordinary paragraph run and nothing else.
/// A `)` item, or any line that still looks like a link definition or HTML,
/// keeps the underline visible instead of being reclassified.
fn setext_promote_from(lines: &[ParsedLine], markdown: &str, run_start: usize) -> Option<usize> {
    let first = &markdown[lines[run_start].source.clone()];
    if is_paren_item_line(first) {
        return None;
    }
    let blocked = lines[run_start..]
        .iter()
        .any(|line| line_blocks_setext(&markdown[line.source.clone()]));
    if blocked {
        None
    } else {
        Some(run_start)
    }
}

fn starts_with_ignore_ascii_case(text: &str, prefix: &str) -> bool {
    let text = text.as_bytes();
    let prefix = prefix.as_bytes();
    text.len() >= prefix.len()
        && text[..prefix.len()]
            .iter()
            .zip(prefix)
            .all(|(left, right)| left.to_ascii_lowercase() == *right)
}

fn html_type1_open(rest: &str) -> bool {
    for tag in ["pre", "script", "style", "textarea"] {
        let open = format!("<{tag}");
        if !starts_with_ignore_ascii_case(rest, &open) {
            continue;
        }
        let next = rest.as_bytes().get(open.len()).copied();
        if next.is_none() || matches!(next, Some(b' ' | b'\t' | b'>')) {
            return true;
        }
    }
    false
}

fn is_html_block_tag(name: &str) -> bool {
    const TAGS: &[&str] = &[
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
    TAGS.iter().any(|tag| name.eq_ignore_ascii_case(tag))
}

fn html_type6_open(rest: &str) -> bool {
    let bytes = rest.as_bytes();
    if bytes.first() != Some(&b'<') {
        return false;
    }
    let mut index = 1usize;
    if bytes.get(index) == Some(&b'/') {
        index += 1;
    }
    let name_start = index;
    while index < bytes.len() && bytes[index].is_ascii_alphanumeric() {
        index += 1;
    }
    if index == name_start || !rest.is_char_boundary(name_start) || !rest.is_char_boundary(index) {
        return false;
    }
    if !is_html_block_tag(&rest[name_start..index]) {
        return false;
    }
    match bytes.get(index).copied() {
        None | Some(b' ' | b'\t' | b'>') => true,
        Some(b'/') => bytes.get(index + 1) == Some(&b'>'),
        _ => false,
    }
}

fn is_attr_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b':'
}

fn is_attr_name_byte(byte: u8) -> bool {
    is_attr_name_start(byte) || byte.is_ascii_digit() || byte == b'.' || byte == b'-'
}

fn skip_ascii_ws(index: &mut usize, bytes: &[u8]) {
    while *index < bytes.len() && matches!(bytes[*index], b' ' | b'\t') {
        *index += 1;
    }
}

fn skip_attr_value(index: &mut usize, bytes: &[u8]) -> bool {
    if *index >= bytes.len() {
        return false;
    }
    match bytes[*index] {
        b'"' | b'\'' => {
            let quote = bytes[*index];
            *index += 1;
            while *index < bytes.len() && bytes[*index] != quote {
                *index += 1;
            }
            if *index >= bytes.len() {
                return false;
            }
            *index += 1;
            true
        }
        b' ' | b'\t' | b'=' | b'<' | b'>' | b'`' => false,
        _ => {
            *index += 1;
            while *index < bytes.len()
                && !matches!(bytes[*index], b' ' | b'\t' | b'"' | b'\'' | b'=' | b'<' | b'>' | b'`')
            {
                *index += 1;
            }
            true
        }
    }
}

fn skip_html_attribute(index: &mut usize, bytes: &[u8]) -> bool {
    let start = *index;
    skip_ascii_ws(index, bytes);
    if *index == start || *index >= bytes.len() || !is_attr_name_start(bytes[*index]) {
        return false;
    }
    *index += 1;
    while *index < bytes.len() && is_attr_name_byte(bytes[*index]) {
        *index += 1;
    }
    let after_name = *index;
    skip_ascii_ws(index, bytes);
    if *index < bytes.len() && bytes[*index] == b'=' {
        *index += 1;
        skip_ascii_ws(index, bytes);
        skip_attr_value(index, bytes)
    } else {
        *index = after_name;
        true
    }
}

fn html_type7_open(rest: &str) -> bool {
    let bytes = rest.as_bytes();
    if bytes.first() != Some(&b'<') {
        return false;
    }
    let mut index = 1usize;
    let closing = if bytes.get(index) == Some(&b'/') {
        index += 1;
        true
    } else {
        false
    };
    let name_start = index;
    if index >= bytes.len() || !bytes[index].is_ascii_alphabetic() {
        return false;
    }
    index += 1;
    while index < bytes.len() && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'-') {
        index += 1;
    }
    if !rest.is_char_boundary(name_start) || !rest.is_char_boundary(index) {
        return false;
    }
    let name = &rest[name_start..index];
    if name.eq_ignore_ascii_case("pre")
        || name.eq_ignore_ascii_case("script")
        || name.eq_ignore_ascii_case("style")
        || name.eq_ignore_ascii_case("textarea")
    {
        return false;
    }
    if closing {
        skip_ascii_ws(&mut index, bytes);
        if bytes.get(index) != Some(&b'>') {
            return false;
        }
        index += 1;
    } else {
        while skip_html_attribute(&mut index, bytes) {}
        skip_ascii_ws(&mut index, bytes);
        if bytes.get(index) == Some(&b'/') {
            index += 1;
        }
        if bytes.get(index) != Some(&b'>') {
            return false;
        }
        index += 1;
    }
    bytes[index..]
        .iter()
        .all(|byte| *byte == b' ' || *byte == b'\t')
}

/// CommonMark HTML block start conditions 1–7, after at most three columns.
fn html_block_kind(line: &str) -> Option<u8> {
    let (index, column) = split_indent(line);
    if column > 3 || index >= line.len() || !line.is_char_boundary(index) {
        return None;
    }
    let rest = &line[index..];
    if html_type1_open(rest) {
        return Some(1);
    }
    if rest.starts_with("<!--") {
        return Some(2);
    }
    if rest.starts_with("<?") {
        return Some(3);
    }
    let bytes = rest.as_bytes();
    if bytes.len() >= 3 && bytes[0] == b'<' && bytes[1] == b'!' && bytes[2].is_ascii_alphabetic() {
        return Some(4);
    }
    if rest.starts_with("<![CDATA[") {
        return Some(5);
    }
    if html_type6_open(rest) {
        return Some(6);
    }
    if html_type7_open(rest) {
        Some(7)
    } else {
        None
    }
}

fn html_block_ends(kind: u8, line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    match kind {
        1 => ["</pre>", "</script>", "</style>", "</textarea>"]
            .iter()
            .any(|tag| lower.contains(tag)),
        2 => line.contains("-->"),
        3 => line.contains("?>"),
        4 => line.contains('>'),
        5 => line.contains("]]>"),
        6 | 7 => false,
        _ => false,
    }
}

fn line_interrupts_list(line: &str) -> bool {
    if is_thematic_break(line) || atx_heading(line).is_some() || html_block_kind(line).is_some() {
        return true;
    }
    list_marker(line).is_some() || match_opening_fence(line).is_some()
}

#[derive(Clone, Copy)]
struct ListFrame {
    marker_col: usize,
    content_col: usize,
    fence_marker: Option<(u8, usize)>,
}

fn push_list_frame(
    lists: &mut Vec<ListFrame>,
    marker_col: usize,
    content_col: usize,
    fence_marker: Option<(u8, usize)>,
) {
    while lists
        .last()
        .is_some_and(|frame| frame.marker_col >= marker_col)
    {
        lists.pop();
    }
    lists.push(ListFrame {
        marker_col,
        content_col,
        fence_marker,
    });
}

fn list_relative_rule(line: &str, lists: &[ListFrame]) -> bool {
    let Some(frame) = lists.last() else {
        return false;
    };
    let Some(column) = line_indent_column(line) else {
        return false;
    };
    column >= frame.content_col
        && column <= frame.content_col + 3
        && is_thematic_break(line.trim_start())
}

fn close_finished_lists(lists: &mut Vec<ListFrame>, previous_blank: bool, line: &str) {
    let Some(column) = line_indent_column(line) else {
        return;
    };
    let interrupts = line_interrupts_list(line);
    while let Some(frame) = lists.last() {
        if column >= frame.content_col {
            break;
        }
        if previous_blank || interrupts {
            lists.pop();
        } else {
            break;
        }
    }
}

/// Backticks after a blockquote stay fences at any indent, which is what red
/// did by trimming. Tildes stay at the top-level limit of 3 unless a list
/// widens that limit, so `> foo` / `    ~~~` remains a paragraph.
fn backtick_fence_limit(lines: &[ParsedLine]) -> usize {
    if nearest_nonempty(lines).is_some_and(|prev| matches!(prev.kind, LineKind::Blockquote)) {
        usize::MAX
    } else {
        3
    }
}

fn match_opening_fence_marked(
    line: &str,
    backtick_limit: usize,
    tilde_limit: usize,
) -> Option<(u8, usize, usize, Option<String>)> {
    let (index, column) = split_indent(line);
    if index >= line.len() {
        return None;
    }
    let marker = line.as_bytes()[index];
    let limit = match marker {
        b'`' => backtick_limit,
        b'~' => tilde_limit,
        _ => return None,
    };
    if column > limit {
        return None;
    }
    opening_fence_parts(line, index, column)
}

fn nearest_nonempty(lines: &[ParsedLine]) -> Option<&ParsedLine> {
    lines
        .iter()
        .rev()
        .find(|line| !matches!(line.kind, LineKind::Empty))
}

fn item_opens_fence(line: &str, prefix_len: usize) -> Option<(u8, usize)> {
    let rest = line.get(prefix_len..)?.trim_start_matches([' ', '\t']);
    let bytes = rest.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let marker = bytes[0];
    if marker != b'`' && marker != b'~' {
        return None;
    }
    let len = bytes.iter().take_while(|byte| **byte == marker).count();
    if len >= 3 {
        Some((marker, len))
    } else {
        None
    }
}

fn flow_text_line(
    index: usize,
    line_source: std::ops::Range<usize>,
    line: &str,
    kind: LineKind,
    prefix_len: usize,
) -> ParsedLine {
    let line_start = line_source.start;
    let prefix_range = line_start..(line_start + prefix_len);
    let body = &line[prefix_len..];
    let body_base = line_start + prefix_len;
    let (mut inline_hidden, inline_pairs, styled_segments) = parse_inline_syntax(body, body_base);
    if prefix_len > 0 {
        inline_hidden.push(prefix_range.clone());
    }
    let merged_hidden = merge_ranges(inline_hidden);
    let mapped = RowOffsetMap::from_hidden(line, line_start, body_base, &merged_hidden);
    let (display_text, offset_map) = mapped;
    ParsedLine {
        line_index: index,
        source: line_source,
        kind,
        prefix_range,
        hidden_ranges: merged_hidden,
        inline_pairs,
        display_text,
        offset_map,
        styled_segments,
    }
}

fn paragraph_run_start(lines: &[ParsedLine]) -> Option<usize> {
    let last = lines.last()?;
    if !matches!(last.kind, LineKind::Paragraph) {
        return None;
    }
    let mut start = lines.len() - 1;
    while start > 0 && matches!(lines[start - 1].kind, LineKind::Paragraph) {
        start -= 1;
    }
    Some(start)
}

fn is_bare_list_marker_line(line: &str) -> bool {
    let (index, column) = split_indent(line);
    if column > 3 {
        return false;
    }
    let rest = line[index..].trim_end_matches([' ', '\t']);
    if matches!(rest, "-" | "+" | "*") {
        return true;
    }
    let bytes = rest.as_bytes();
    if bytes.len() >= 2 && (bytes[bytes.len() - 1] == b'.' || bytes[bytes.len() - 1] == b')') {
        let digits = &bytes[..bytes.len() - 1];
        return !digits.is_empty()
            && digits.len() <= 9
            && digits.iter().all(|byte| byte.is_ascii_digit());
    }
    false
}

/// A setext underline does not promote a lazy continuation of a quote or list,
/// or an indented line that continues a bare marker we do not parse as an item.
fn setext_is_lazy(lines: &[ParsedLine], markdown: &str, run_start: usize) -> bool {
    if run_start > 0
        && matches!(
            lines[run_start - 1].kind,
            LineKind::Blockquote | LineKind::ListItem { .. }
        )
    {
        return true;
    }
    let mut saw_bare = false;
    for line in &lines[run_start..] {
        let text = &markdown[line.source.clone()];
        if is_bare_list_marker_line(text) {
            saw_bare = true;
            continue;
        }
        let (index, _) = split_indent(text);
        if saw_bare && index > 0 {
            return true;
        }
    }
    false
}

fn atx_heading(line: &str) -> Option<(u8, std::ops::Range<usize>)> {
    atx_heading_within(line, 3)
}

/// `max_indent` is a column, with tabs advancing to the next multiple of 4.
/// Inside a list that column is the item content column plus 3, so a tab
/// after `- foo` still opens a heading.
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

fn promote_setext(prev: &mut ParsedLine, markdown: &str, level: u8) -> Vec<std::ops::Range<usize>> {
    let line = &markdown[prev.source.clone()];
    let bytes = line.as_bytes();
    let mut lead = 0;
    while lead < bytes.len() && lead < 3 && bytes[lead] == b' ' {
        lead += 1;
    }
    let mut end = bytes.len();
    while end > lead && (bytes[end - 1] == b' ' || bytes[end - 1] == b'\t') {
        end -= 1;
    }
    let mut extra = Vec::new();
    if lead > 0 {
        extra.push(prev.source.start..prev.source.start + lead);
    }
    if end < bytes.len() {
        extra.push(prev.source.start + end..prev.source.end);
    }
    prev.kind = LineKind::Heading { level };
    if extra.is_empty() {
        return extra;
    }
    prev.hidden_ranges.extend(extra.iter().cloned());
    prev.hidden_ranges = merge_ranges(std::mem::take(&mut prev.hidden_ranges));
    let (display_text, offset_map) = RowOffsetMap::from_hidden(
        line,
        prev.source.start,
        prev.source.start + lead,
        &prev.hidden_ranges,
    );
    prev.display_text = display_text;
    prev.offset_map = offset_map;
    if lead > 0 {
        prev.prefix_range = prev.source.start..prev.source.start + lead;
    }
    extra
}

fn structural_line(
    index: usize,
    source: std::ops::Range<usize>,
    line: &str,
    kind: LineKind,
) -> ParsedLine {
    let line_start = source.start;
    let line_end = source.end;
    let mut hidden = Vec::new();
    if line_end > line_start {
        hidden.push(line_start..line_end);
    }
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
    let mut in_indented_code = false;
    let mut code_min_col = 4usize;
    let mut html_open: Option<u8> = None;
    let mut lists: Vec<ListFrame> = Vec::new();

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

    for (i, sline) in raw_lines.iter().enumerate() {
        let line = sline.text;
        let line_start = sline.start;
        let line_end = line_start + line.len();
        let line_source = line_start..line_end;
        let trimmed = line.trim();
        let trimmed_start = line.trim_start();
        let indent_len = line.len() - trimmed_start.len();

        let active_fence = open_fence.as_ref().map(|fence| {
            (
                fence.marker,
                fence.len,
                fence.indent,
                fence.max_indent,
                fence.open_range.clone(),
            )
        });
        if let Some((marker, len, indent, max_indent, open_range)) = active_fence {
            if is_closing_fence(line, marker, len, max_indent) {
                code_blocks.push((open_range, sline.start..sline.end));
                open_fence = None;
                let parsed = structural_line(
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
                let parsed =
                    code_content_line(i, line_source, line, fence_content_strip(line, indent));
                all_hidden.extend(parsed.hidden_ranges.iter().cloned());
                lines.push(parsed);
            }
            continue;
        }

        if let Some(html_kind) = html_open {
            let blank = trimmed.is_empty();
            if matches!(html_kind, 6 | 7) && blank {
                html_open = None;
            } else {
                if blank {
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
                } else {
                    let parsed = flow_text_line(i, line_source, line, LineKind::Paragraph, 0);
                    all_hidden.extend(parsed.hidden_ranges.iter().cloned());
                    all_pairs.extend(parsed.inline_pairs.iter().cloned());
                    lines.push(parsed);
                }
                if html_block_ends(html_kind, line) {
                    html_open = None;
                }
                in_indented_code = false;
                continue;
            }
        }

        if in_indented_code {
            if trimmed.is_empty() {
                if indented_code_resumes(&raw_lines, i, code_min_col) {
                    let parsed = code_content_line(
                        i,
                        line_source.clone(),
                        line,
                        prefix_through_column(line, code_min_col),
                    );
                    all_hidden.extend(parsed.hidden_ranges.iter().cloned());
                    lines.push(parsed);
                    continue;
                }
                in_indented_code = false;
            } else {
                let (index, column) = split_indent(line);
                if index < line.len() && column >= code_min_col {
                    let strip = prefix_through_column(line, code_min_col);
                    let parsed = code_content_line(i, line_source.clone(), line, strip);
                    all_hidden.extend(parsed.hidden_ranges.iter().cloned());
                    lines.push(parsed);
                    continue;
                }
                in_indented_code = false;
            }
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

        let previous_blank = lines
            .last()
            .is_some_and(|prev| matches!(prev.kind, LineKind::Empty));
        close_finished_lists(&mut lists, previous_blank, line);

        let fence_limit = lists
            .last()
            .map(|frame| frame.content_col.saturating_add(3))
            .unwrap_or_else(|| backtick_fence_limit(&lines));
        let skip_item_closer = lists.last().is_some_and(|frame| {
            frame.fence_marker.is_some_and(|(marker, len)| {
                is_closing_fence(line, marker, len, frame.content_col.saturating_add(3))
            })
        });
        if skip_item_closer {
            if let Some(frame) = lists.last_mut() {
                frame.fence_marker = None;
            }
            // The list item already opened this fence. Emit the closer and do
            // not start a new fence, so the following line stays a paragraph.
            let parsed = structural_line(
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
            continue;
        }
        let opening_fence = if let Some(frame) = lists.last() {
            let limit = frame.content_col.saturating_add(3);
            match_opening_fence_marked(line, limit, limit)
        } else if fence_limit == 3 {
            match_opening_fence(line)
        } else {
            match_opening_fence_marked(line, fence_limit, 3)
        };
        if let Some((marker, len, indent, language)) = opening_fence {
            open_fence = Some(OpenFence {
                marker,
                len,
                indent,
                open_range: sline.start..sline.end,
                max_indent: fence_limit,
            });
            let parsed = structural_line(
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

        if let Some(level) = setext_level(line) {
            if let Some(run_start) = paragraph_run_start(&lines) {
                if !setext_is_lazy(&lines, markdown, run_start) {
                    if let Some(promote_from) = setext_promote_from(&lines, markdown, run_start) {
                        for prev in &mut lines[promote_from..] {
                            let extra = promote_setext(prev, markdown, level);
                            all_hidden.extend(extra);
                        }
                        let parsed = structural_line(
                            i,
                            line_source.clone(),
                            line,
                            LineKind::SetextUnderline,
                        );
                        all_hidden.extend(parsed.hidden_ranges.iter().cloned());
                        lines.push(parsed);
                        continue;
                    }
                }
            }
        }

        if is_thematic_break(line) || list_relative_rule(line, &lists) {
            let parsed = structural_line(i, line_source, line, LineKind::HorizontalRule);
            all_hidden.extend(parsed.hidden_ranges.iter().cloned());
            lines.push(parsed);
            continue;
        }

        // Table rows
        if let Some(&(tbl_first, _tbl_last, ref tbl_src, ref tbl_aligns)) =
            table_ranges.iter().find(|&&(f, l, _, _)| i >= f && i <= l)
        {
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

        let atx_limit = lists
            .last()
            .map(|frame| frame.content_col.saturating_add(3))
            .unwrap_or(3);
        if let Some((level, content)) = atx_heading_within(line, atx_limit) {
            let parsed = atx_line(i, line_source, line, level, content);
            all_hidden.extend(parsed.hidden_ranges.iter().cloned());
            all_pairs.extend(parsed.inline_pairs.iter().cloned());
            lines.push(parsed);
            continue;
        }

        // Determine block kind & prefix length
        let mut kind = LineKind::Paragraph;
        let mut prefix_len = 0usize;

        if let Some(marker) = list_marker(line) {
            // A marker at column 4 or more is indented code, unless it continues
            // a list item whose marker sits in a smaller column.
            let nested = marker.column >= 4 && continues_list(&lines, marker.column);
            let paren_parent = marker.column >= 4
                && nearest_nonempty(&lines).is_some_and(|prev| {
                    matches!(prev.kind, LineKind::Paragraph)
                        && is_paren_item_line(&markdown[prev.source.clone()])
                });
            let quote_parent = marker.column >= 4
                && nearest_nonempty(&lines)
                    .is_some_and(|prev| matches!(prev.kind, LineKind::Blockquote));
            if marker.column < 4 || nested || paren_parent || quote_parent {
                let checkbox_source = marker.checkbox_rel.map(|rel| {
                    let start = line_start + rel;
                    start..(start + 3)
                });
                kind = LineKind::ListItem {
                    ordered: marker.ordered,
                    number: marker.number,
                    task: marker.task,
                    indent_cols: marker.column,
                    checkbox_source,
                };
                prefix_len = marker.prefix_len;
                let content_col = list_content_column(line, marker.prefix_len);
                let fence = item_opens_fence(line, marker.prefix_len);
                push_list_frame(&mut lists, marker.column, content_col, fence);
            }
        }

        if prefix_len == 0 {
            if let Some(_strip) = indented_code_prefix(line) {
                let previous_is_paragraph = lines.last().is_some_and(|prev| {
                    if !matches!(prev.kind, LineKind::Paragraph) {
                        return false;
                    }
                    // A bare marker is not a paragraph in CommonMark. An indented
                    // line after it can still open a code block (example 278).
                    !is_bare_list_marker_line(&markdown[prev.source.clone()])
                });
                if previous_is_paragraph {
                    // Indented code does not interrupt a paragraph. Hide the
                    // leading spaces, except a `)` item or an unclassified
                    // link/HTML run, which keeps the indent red displayed.
                    if !paragraph_run_keeps_indent(&lines, markdown) {
                        prefix_len = split_indent(line).0;
                    }
                } else if let Some(column) = line_indent_column(line) {
                    let threshold = lists
                        .last()
                        .map(|frame| frame.content_col + 4)
                        .unwrap_or(4);
                    let quote_holds = lists.is_empty()
                        && nearest_nonempty(&lines)
                            .is_some_and(|prev| matches!(prev.kind, LineKind::Blockquote));
                    let previous_item_holds = !previous_blank
                        && lines.last().is_some_and(|prev| {
                            matches!(prev.kind, LineKind::ListItem { .. })
                                && !is_bare_list_marker_line(&markdown[prev.source.clone()])
                        });
                    if column >= threshold && !quote_holds && !previous_item_holds {
                        let strip = prefix_through_column(line, threshold);
                        let parsed = code_content_line(i, line_source.clone(), line, strip);
                        all_hidden.extend(parsed.hidden_ranges.iter().cloned());
                        lines.push(parsed);
                        in_indented_code = true;
                        code_min_col = threshold;
                        continue;
                    }
                }
            }
        }

        // Red treats every trimmed backtick run as a fence. Keep a real indented
        // code block when one already opened. Otherwise a tab or a deep indent
        // after HTML or a quote would stay a paragraph and miss pulldown's code.
        if prefix_len == 0 && matches!(kind, LineKind::Paragraph) {
            if let Some((marker, len, indent, language)) =
                match_opening_fence_marked(line, usize::MAX, 0)
                    .filter(|(marker, _, _, _)| *marker == b'`')
            {
                open_fence = Some(OpenFence {
                    marker,
                    len,
                    indent,
                    open_range: sline.start..sline.end,
                    max_indent: usize::MAX,
                });
                let parsed = structural_line(
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
        }

        if prefix_len == 0 && trimmed_start.starts_with("> ") {
            kind = LineKind::Blockquote;
            prefix_len = indent_len + 2;
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

        if matches!(kind, LineKind::Paragraph) && is_paren_item_line(line) {
            let starts_block = !lines
                .last()
                .is_some_and(|prev| matches!(prev.kind, LineKind::Paragraph));
            if starts_block {
                if let Some(prefix) = paren_marker_prefix_len(line) {
                    let content_col = list_content_column(line, prefix);
                    let marker_col = column_at_byte(line, line.len() - line.trim_start().len());
                    push_list_frame(&mut lists, marker_col, content_col, None);
                }
            }
        }

        if html_open.is_none() && prefix_len == 0 && matches!(kind, LineKind::Paragraph) {
            if let Some(html_kind) = html_block_kind(line) {
                let previous_paragraph = lines
                    .last()
                    .is_some_and(|prev| matches!(prev.kind, LineKind::Paragraph));
                if html_kind != 7 || !previous_paragraph {
                    html_open = if html_block_ends(html_kind, line) {
                        None
                    } else {
                        Some(html_kind)
                    };
                }
            }
        }

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
        in_indented_code = false;
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
            LineKind::SetextUnderline => "setext".to_string(),
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

    /// CommonMark 0.31.2 leaf blocks from the audit samples and the spec
    /// examples they cite. Each case compares the whole kind, display, source,
    /// mark, and link record. A setext underline is a collapsed `setext` line
    /// whose source is the marker, not a horizontal rule and not a blank row.
    #[test]
    fn leaf_blocks_match_commonmark_examples() {
        let mut cases: Vec<(&str, &str, Vec<LeafExpect>)> = leaf_block_cases();
        cases.extend(leaf_block_cases_round2());
        cases.extend(leaf_block_cases_round3());
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
                "ex48-hr-indented-code",
                "    ***",
                vec![leaf_marks("code", "***", "    ***", "C:***")],
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
                "ex59-setext-h2",
                "Foo\n---\nbar",
                vec![
                    leaf("h2", "Foo", "Foo"),
                    leaf("setext", "", "---"),
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
                "ex69-atx-indented-code",
                "    # foo",
                vec![leaf_marks("code", "# foo", "    # foo", "C:# foo")],
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
                "ex80-setext-inline",
                "Foo *bar*\n=========\n\nFoo *bar*\n---------",
                vec![
                    leaf_marks("h1", "Foo bar", "Foo *bar*", "I:bar"),
                    leaf("setext", "", "========="),
                    leaf("empty", "", ""),
                    leaf_marks("h2", "Foo bar", "Foo *bar*", "I:bar"),
                    leaf("setext", "", "---------"),
                ],
            ),
            (
                "ex83-setext",
                "Foo\n-------------------------\n\nFoo\n=",
                vec![
                    leaf("h2", "Foo", "Foo"),
                    leaf("setext", "", "-------------------------"),
                    leaf("empty", "", ""),
                    leaf("h1", "Foo", "Foo"),
                    leaf("setext", "", "="),
                ],
            ),
            (
                "audit-setext-both",
                "Foo\n---\n\nBar\n===",
                vec![
                    leaf("h2", "Foo", "Foo"),
                    leaf("setext", "", "---"),
                    leaf("empty", "", ""),
                    leaf("h1", "Bar", "Bar"),
                    leaf("setext", "", "==="),
                ],
            ),
            (
                "ex84-setext-indent",
                "   Foo\n---\n\n  Foo\n-----\n\n  Foo\n  ===",
                vec![
                    leaf("h2", "Foo", "   Foo"),
                    leaf("setext", "", "---"),
                    leaf("empty", "", ""),
                    leaf("h2", "Foo", "  Foo"),
                    leaf("setext", "", "-----"),
                    leaf("empty", "", ""),
                    leaf("h1", "Foo", "  Foo"),
                    leaf("setext", "", "  ==="),
                ],
            ),
            (
                "ex86-setext-underline-indent",
                "Foo\n   ----      ",
                vec![leaf("h2", "Foo", "Foo"), leaf("setext", "", "   ----      ")],
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
                "ex89-setext-trailing-spaces",
                "Foo  \n-----",
                vec![leaf("h2", "Foo", "Foo  "), leaf("setext", "", "-----")],
            ),
            (
                "ex90-setext-backslash",
                "Foo\\\n----",
                vec![leaf("h2", "Foo\\", "Foo\\"), leaf("setext", "", "----")],
            ),
            (
                "ex91-setext-in-text",
                "`Foo\n----\n`\n\n<a title=\"a lot\n---\nof dashes\"/>",
                vec![
                    leaf("h2", "`Foo", "`Foo"),
                    leaf("setext", "", "----"),
                    leaf("p", "`", "`"),
                    leaf("empty", "", ""),
                    leaf("h2", "<a title=\"a lot", "<a title=\"a lot"),
                    leaf("setext", "", "---"),
                    leaf("p", "of dashes\"/>", "of dashes\"/>"),
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
                "ex96-hr-and-setext",
                "---\nFoo\n---\nBar\n---\nBaz",
                vec![
                    leaf("hr", "", "---"),
                    leaf("h2", "Foo", "Foo"),
                    leaf("setext", "", "---"),
                    leaf("h2", "Bar", "Bar"),
                    leaf("setext", "", "---"),
                    leaf("p", "Baz", "Baz"),
                ],
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
                "ex100-code-then-hr",
                "    foo\n---",
                vec![
                    leaf_marks("code", "foo", "    foo", "C:foo"),
                    leaf("hr", "", "---"),
                ],
            ),
            (
                "ex103-setext-after-blank",
                "Foo\n\nbar\n---\nbaz",
                vec![
                    leaf("p", "Foo", "Foo"),
                    leaf("empty", "", ""),
                    leaf("h2", "bar", "bar"),
                    leaf("setext", "", "---"),
                    leaf("p", "baz", "baz"),
                ],
            ),
            (
                "ex107-indented-code",
                "    a simple\n      indented code block",
                vec![
                    leaf_marks("code", "a simple", "    a simple", "C:a simple"),
                    leaf_marks(
                        "code",
                        "  indented code block",
                        "      indented code block",
                        "C:  indented code block",
                    ),
                ],
            ),
            (
                "cm40-tab",
                "\tfoo",
                vec![leaf_marks("code", "foo", "\tfoo", "C:foo")],
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
                "ex120-tilde-fence",
                "~~~\n<\n >\n~~~",
                vec![
                    leaf("fence-open", "", "~~~"),
                    leaf_marks("code", "<", "<", "C:<"),
                    leaf_marks("code", " >", " >", "C: >"),
                    leaf("fence-close", "", "~~~"),
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
                "ex123-backticks-inside-tilde",
                "~~~\naaa\n```\n~~~",
                vec![
                    leaf("fence-open", "", "~~~"),
                    leaf_marks("code", "aaa", "aaa", "C:aaa"),
                    leaf_marks("code", "```", "```", "C:```"),
                    leaf("fence-close", "", "~~~"),
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
                "ex125-tilde-len",
                "~~~~\naaa\n~~~\n~~~~",
                vec![
                    leaf("fence-open", "", "~~~~"),
                    leaf_marks("code", "aaa", "aaa", "C:aaa"),
                    leaf_marks("code", "~~~", "~~~", "C:~~~"),
                    leaf("fence-close", "", "~~~~"),
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
                "ex134-fence-indent-4",
                "    ```\n    aaa\n    ```",
                vec![
                    leaf_marks("code", "```", "    ```", "C:```"),
                    leaf_marks("code", "aaa", "    aaa", "C:aaa"),
                    leaf_marks("code", "```", "    ```", "C:```"),
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
                "ex139-tilde-closer-info",
                "~~~~~~\naaa\n~~~ ~~",
                vec![
                    leaf("fence-open", "", "~~~~~~"),
                    leaf_marks("code", "aaa", "aaa", "C:aaa"),
                    leaf_marks("code", "~~~ ~~", "~~~ ~~", "C:~~~ ~~"),
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
                "ex141-setext-fence-atx",
                "foo\n---\n~~~\nbar\n~~~\n# baz",
                vec![
                    leaf("h2", "foo", "foo"),
                    leaf("setext", "", "---"),
                    leaf("fence-open", "", "~~~"),
                    leaf_marks("code", "bar", "bar", "C:bar"),
                    leaf("fence-close", "", "~~~"),
                    leaf("h1", "baz", "# baz"),
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
                "ex143-tilde-info",
                "~~~~    ruby startline=3 $%@#$\ndef foo(x)\n  return 3\nend\n~~~~~~~",
                vec![
                    leaf(
                        "fence-open:ruby startline=3 $%@#$",
                        "",
                        "~~~~    ruby startline=3 $%@#$",
                    ),
                    leaf_marks("code", "def foo(x)", "def foo(x)", "C:def foo(x)"),
                    leaf_marks("code", "  return 3", "  return 3", "C:  return 3"),
                    leaf_marks("code", "end", "end", "C:end"),
                    leaf("fence-close", "", "~~~~~~~"),
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
                    leaf_marks("p", "aa", "``` aa ```", "C:aa"),
                    leaf("p", "foo", "foo"),
                ],
            ),
            (
                "ex146-tilde-info-backticks",
                "~~~ aa ``` ~~~\nfoo\n~~~",
                vec![
                    leaf("fence-open:aa ``` ~~~", "", "~~~ aa ``` ~~~"),
                    leaf_marks("code", "foo", "foo", "C:foo"),
                    leaf("fence-close", "", "~~~"),
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
                "quote-indent-4-is-code",
                "    > foo",
                vec![leaf_marks("code", "> foo", "    > foo", "C:> foo")],
            ),
            (
                "tab-list-not-code",
                "\t- foo",
                // CommonMark tabs: a marker at column 4 is indented code.
                // pulldown-cmark 0.12.2 text is `- foo`.
                vec![leaf_marks("code", "- foo", "\t- foo", "C:- foo")],
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
        ]
    }

    #[allow(clippy::too_many_lines)]
    fn leaf_block_cases_round2() -> Vec<(&'static str, &'static str, Vec<LeafExpect>)> {
        vec![
            // Example 49. Indented code does not interrupt a paragraph.
            (
                "ex49-indent-not-interrupt",
                "Foo\n    ***",
                vec![leaf("p", "Foo", "Foo"), leaf("p", "***", "    ***")],
            ),
            (
                "ex70-indent-not-interrupt",
                "foo\n    # bar",
                vec![leaf("p", "foo", "foo"), leaf("p", "# bar", "    # bar")],
            ),
            (
                "ex87-indent-not-interrupt",
                "Foo\n    ---",
                vec![leaf("p", "Foo", "Foo"), leaf("p", "---", "    ---")],
            ),
            (
                "ex113-indent-not-interrupt",
                "Foo\n    bar\n",
                vec![
                    leaf("p", "Foo", "Foo"),
                    leaf("p", "bar", "    bar"),
                    leaf("empty", "", ""),
                ],
            ),
            (
                "ex223-deep-indent-not-interrupt",
                "aaa\n             bbb\n                                       ccc",
                vec![
                    leaf("p", "aaa", "aaa"),
                    leaf("p", "bbb", "             bbb"),
                    leaf("p", "ccc", "                                       ccc"),
                ],
            ),
            (
                "ex636-hard-break-indent",
                "foo  \n     bar",
                vec![leaf("p", "foo  ", "foo  "), leaf("p", "bar", "     bar")],
            ),
            (
                "ex637-backslash-break-indent",
                "foo\\\n     bar",
                vec![leaf("p", "foo\\", "foo\\"), leaf("p", "bar", "     bar")],
            ),
            // Example 10. A tab after the opening hashes is the separator.
            (
                "ex10-atx-tab",
                "#\tFoo",
                vec![leaf("h1", "Foo", "#\tFoo")],
            ),
            // Example 81, 82, 95. The whole paragraph run is the heading.
            (
                "ex81-setext-run",
                "Foo *bar\nbaz*\n====",
                vec![
                    leaf("h1", "Foo *bar", "Foo *bar"),
                    leaf("h1", "baz*", "baz*"),
                    leaf("setext", "", "===="),
                ],
            ),
            (
                "ex82-setext-run-tab",
                "  Foo *bar\nbaz*\t\n====",
                vec![
                    leaf("h1", "Foo *bar", "  Foo *bar"),
                    leaf("h1", "baz*", "baz*\t"),
                    leaf("setext", "", "===="),
                ],
            ),
            (
                "ex95-setext-two-lines",
                "Foo\nBar\n---",
                vec![
                    leaf("h2", "Foo", "Foo"),
                    leaf("h2", "Bar", "Bar"),
                    leaf("setext", "", "---"),
                ],
            ),
            // Example 93. Lazy continuation is not promoted. `===` stays text.
            (
                "ex93-lazy-not-heading",
                "> foo\nbar\n===",
                vec![
                    leaf("quote", "foo", "> foo"),
                    leaf("p", "bar", "bar"),
                    leaf("p", "===", "==="),
                ],
            ),
            // pulldown: quote covers `> foo` and `bar`; `---` is a rule.
            (
                "quote-lazy-then-hr",
                "> foo\nbar\n---",
                vec![
                    leaf("quote", "foo", "> foo"),
                    leaf("p", "bar", "bar"),
                    leaf("hr", "", "---"),
                ],
            ),
            // Example 278. `  foo` must not become a heading. Empty markers
            // stay paragraphs (list items are LM-09).
            (
                "ex278-bare-marker-not-heading",
                "-\n  foo\n-\n  ```\n  bar\n  ```\n-\n      baz",
                vec![
                    leaf("p", "-", "-"),
                    leaf("p", "  foo", "  foo"),
                    leaf("p", "-", "-"),
                    leaf("fence-open", "", "  ```"),
                    leaf_marks("code", "bar", "  bar", "C:bar"),
                    leaf("fence-close", "", "  ```"),
                    leaf("p", "-", "-"),
                    leaf_marks("code", "  baz", "      baz", "C:  baz"),
                ],
            ),
            (
                "four-space-bullet-is-code",
                "    - one\n",
                vec![
                    leaf_marks("code", "- one", "    - one", "C:- one"),
                    leaf("empty", "", ""),
                ],
            ),
            (
                "four-space-ordered-is-code",
                "    1. foo",
                vec![leaf_marks("code", "1. foo", "    1. foo", "C:1. foo")],
            ),
            (
                "four-space-task-is-code",
                "    - [ ] todo",
                vec![leaf_marks(
                    "code",
                    "- [ ] todo",
                    "    - [ ] todo",
                    "C:- [ ] todo",
                )],
            ),
            (
                "blank-then-nested-list",
                "- foo\n\n    - bar",
                vec![
                    leaf("ul:0", "foo", "- foo"),
                    leaf("empty", "", ""),
                    leaf("ul:4", "bar", "    - bar"),
                ],
            ),
            (
                "tab-under-list",
                "- foo\n\t- bar",
                vec![
                    leaf("ul:0", "foo", "- foo"),
                    leaf("ul:4", "bar", "\t- bar"),
                ],
            ),
            (
                "deep-nested-list",
                "- foo\n    - bar\n        - baz",
                vec![
                    leaf("ul:0", "foo", "- foo"),
                    leaf("ul:4", "bar", "    - bar"),
                    leaf("ul:8", "baz", "        - baz"),
                ],
            ),
            (
                "three-space-list",
                "   - foo",
                vec![leaf("ul:3", "foo", "   - foo")],
            ),
            (
                "para-then-indented-marker",
                "Foo\n    - one",
                vec![leaf("p", "Foo", "Foo"), leaf("p", "- one", "    - one")],
            ),
            // Example 110. The marker and the interior blank are code.
            (
                "ex110-indented-code-marker",
                "    <a/>\n    *hi*\n\n    - one",
                vec![
                    leaf_marks("code", "<a/>", "    <a/>", "C:<a/>"),
                    leaf_marks("code", "*hi*", "    *hi*", "C:*hi*"),
                    leaf("code", "", ""),
                    leaf_marks("code", "- one", "    - one", "C:- one"),
                ],
            ),
            // Example 111. Blank and short whitespace lines stay inside the code.
            (
                "ex111-blank-inside-indent",
                "    chunk1\n\n    chunk2\n  \n \n \n    chunk3",
                vec![
                    leaf_marks("code", "chunk1", "    chunk1", "C:chunk1"),
                    leaf("code", "", ""),
                    leaf_marks("code", "chunk2", "    chunk2", "C:chunk2"),
                    leaf("code", "", "  "),
                    leaf("code", "", " "),
                    leaf("code", "", " "),
                    leaf_marks("code", "chunk3", "    chunk3", "C:chunk3"),
                ],
            ),
            // Example 112. Six spaces strip back to column 4.
            (
                "ex112-spaces-inside-indent",
                "    chunk1\n      \n      chunk2",
                vec![
                    leaf_marks("code", "chunk1", "    chunk1", "C:chunk1"),
                    leaf_marks("code", "  ", "      ", "C:  "),
                    leaf_marks("code", "  chunk2", "      chunk2", "C:  chunk2"),
                ],
            ),
            // Example 85. The blank between the chunks is a code line.
            (
                "ex85-indent-then-hr",
                "    Foo\n    ---\n\n    Foo\n---",
                vec![
                    leaf_marks("code", "Foo", "    Foo", "C:Foo"),
                    leaf_marks("code", "---", "    ---", "C:---"),
                    leaf("code", "", ""),
                    leaf_marks("code", "Foo", "    Foo", "C:Foo"),
                    leaf("hr", "", "---"),
                ],
            ),
            // Example 117. Leading and trailing blanks stay empty.
            (
                "ex117-indent-outer-blanks",
                "\n    \n    foo\n    \n",
                vec![
                    leaf("empty", "", ""),
                    leaf("empty", "    ", "    "),
                    leaf_marks("code", "foo", "    foo", "C:foo"),
                    leaf("empty", "    ", "    "),
                    leaf("empty", "", ""),
                ],
            ),
            // Example 264. Interior blanks of the open code block are code lines.
            // Column-4 strip leaves the list's extra indent (container indent is LM-09).
            (
                "ex264-code-inside-list",
                "- Foo\n\n      bar\n\n\n      baz",
                vec![
                    leaf("ul:0", "Foo", "- Foo"),
                    leaf("empty", "", ""),
                    leaf_marks("code", "bar", "      bar", "C:bar"),
                    leaf("code", "", ""),
                    leaf("code", "", ""),
                    leaf_marks("code", "baz", "      baz", "C:baz"),
                ],
            ),
            // Example 289. The ordered marker is indented code, blanks included.
            (
                "ex289-indented-ordered",
                "    1.  A paragraph\n        with two lines.\n\n            indented code\n\n        > A block quote.",
                vec![
                    leaf_marks(
                        "code",
                        "1.  A paragraph",
                        "    1.  A paragraph",
                        "C:1.  A paragraph",
                    ),
                    leaf_marks(
                        "code",
                        "    with two lines.",
                        "        with two lines.",
                        "C:    with two lines.",
                    ),
                    leaf("code", "", ""),
                    leaf_marks(
                        "code",
                        "        indented code",
                        "            indented code",
                        "C:        indented code",
                    ),
                    leaf("code", "", ""),
                    leaf_marks(
                        "code",
                        "    > A block quote.",
                        "        > A block quote.",
                        "C:    > A block quote.",
                    ),
                ],
            ),
            // CRLF. Each line is classified with the trailing CR removed.
            (
                "crlf-hr",
                "---\r\n",
                vec![leaf("hr", "", "---"), leaf("empty", "", "")],
            ),
            (
                "crlf-setext",
                "Foo\r\n---\r\nbar\r\n",
                vec![
                    leaf("h2", "Foo", "Foo"),
                    leaf("setext", "", "---"),
                    leaf("p", "bar", "bar"),
                    leaf("empty", "", ""),
                ],
            ),
            (
                "crlf-backtick-fence",
                "```\r\ncode\r\n```\r\n",
                vec![
                    leaf("fence-open", "", "```"),
                    leaf_marks("code", "code", "code", "C:code"),
                    leaf("fence-close", "", "```"),
                    leaf("empty", "", ""),
                ],
            ),
            (
                "crlf-tilde-fence",
                "~~~\r\ncode\r\n~~~\r\n",
                vec![
                    leaf("fence-open", "", "~~~"),
                    leaf_marks("code", "code", "code", "C:code"),
                    leaf("fence-close", "", "~~~"),
                    leaf("empty", "", ""),
                ],
            ),
            (
                "crlf-atx",
                "# foo #\r\n",
                vec![leaf("h1", "foo", "# foo #"), leaf("empty", "", "")],
            ),
            // pulldown strips the closer. The backslash stays (it does not escape the space).
            (
                "atx-backslash-space-closer",
                "# foo\\ #\n",
                vec![
                    leaf("h1", "foo\\", "# foo\\ #"),
                    leaf("empty", "", ""),
                ],
            ),
        ]
    }

    fn leaf_block_cases_round3() -> Vec<(&'static str, &'static str, Vec<LeafExpect>)> {
        vec![
            (
                "lone-cr-hrs",
                "---\r***\r",
                vec![
                    leaf("hr", "", "---"),
                    leaf("hr", "", "***"),
                    leaf("empty", "", ""),
                ],
            ),
            (
                "lone-cr-setext",
                "Foo\r---\rbar\r",
                vec![
                    leaf("h2", "Foo", "Foo"),
                    leaf("setext", "", "---"),
                    leaf("p", "bar", "bar"),
                    leaf("empty", "", ""),
                ],
            ),
            (
                "ex215-link-def-not-promoted",
                "[foo]: /url\nbar\n===\n[foo]",
                vec![
                    leaf("p", "[foo]: /url", "[foo]: /url"),
                    leaf("p", "bar", "bar"),
                    leaf("p", "===", "==="),
                    leaf("p", "[foo]", "[foo]"),
                ],
            ),
            (
                "ex216-link-def-equals-stays",
                "[foo]: /url\n===\n[foo]",
                vec![
                    leaf("p", "[foo]: /url", "[foo]: /url"),
                    leaf("p", "===", "==="),
                    leaf("p", "[foo]", "[foo]"),
                ],
            ),
            (
                "link-def-then-hr",
                "[foo]: /url\n---\n",
                vec![
                    leaf("p", "[foo]: /url", "[foo]: /url"),
                    leaf("hr", "", "---"),
                    leaf("empty", "", ""),
                ],
            ),
            (
                "html-setext-not-heading",
                "<div>\n===\n",
                vec![
                    leaf("p", "<div>", "<div>"),
                    leaf("p", "===", "==="),
                    leaf("empty", "", ""),
                ],
            ),
            (
                "ex296-paren-nested-marker",
                "10) foo\n    - bar",
                vec![
                    leaf("p", "10) foo", "10) foo"),
                    leaf("ul:4", "bar", "    - bar"),
                ],
            ),
            (
                "ex238-quote-indented-marker",
                "> foo\n    - bar",
                vec![
                    leaf("quote", "foo", "> foo"),
                    leaf("ul:4", "bar", "    - bar"),
                ],
            ),
            (
                "ex108-list-continuation-not-code",
                "  - foo\n\n    bar",
                vec![
                    leaf("ul:2", "foo", "  - foo"),
                    leaf("empty", "", ""),
                    leaf("p", "    bar", "    bar"),
                ],
            ),
            (
                "ex4-tab-continuation-not-code",
                "  - foo\n\n\tbar",
                vec![
                    leaf("ul:2", "foo", "  - foo"),
                    leaf("empty", "", ""),
                    leaf("p", "\tbar", "\tbar"),
                ],
            ),
            (
                "list-continuation-not-code-without-blank",
                "- foo\n      two",
                vec![
                    leaf("ul:0", "foo", "- foo"),
                    leaf("p", "      two", "      two"),
                ],
            ),
            (
                "list-code-after-blank",
                "- foo\n\n      two",
                vec![
                    leaf("ul:0", "foo", "- foo"),
                    leaf("empty", "", ""),
                    leaf_marks("code", "two", "      two", "C:two"),
                ],
            ),
            (
                "list-indented-hr",
                "- foo\n    ***\n\n    ---",
                vec![
                    leaf("ul:0", "foo", "- foo"),
                    leaf("hr", "", "    ***"),
                    leaf("empty", "", ""),
                    leaf("hr", "", "    ---"),
                ],
            ),
            (
                "paren-item-hr-not-setext",
                "10) foo\n---\n10) foo\n===",
                vec![
                    leaf("p", "10) foo", "10) foo"),
                    leaf("hr", "", "---"),
                    leaf("p", "10) foo", "10) foo"),
                    leaf("p", "===", "==="),
                ],
            ),
            (
                "paren-item-continuation-not-code",
                "10) foo\n\n    bar",
                vec![
                    leaf("p", "10) foo", "10) foo"),
                    leaf("empty", "", ""),
                    leaf("p", "    bar", "    bar"),
                ],
            ),
            (
                "ex309-notcode",
                "-   foo\n\n    notcode\n\n-   foo\n\n<!-- -->\n\n    code",
                vec![
                    leaf("ul:0", "  foo", "-   foo"),
                    leaf("empty", "", ""),
                    leaf("p", "    notcode", "    notcode"),
                    leaf("empty", "", ""),
                    leaf("ul:0", "  foo", "-   foo"),
                    leaf("empty", "", ""),
                    leaf("p", "<!-- -->", "<!-- -->"),
                    leaf("empty", "", ""),
                    leaf_marks("code", "code", "    code", "C:code"),
                ],
            ),
        ]
    }

    #[test]
    fn round4_link_html_and_tab_fences_match_red() {
        fn kinds(md: &str) -> Vec<(String, String)> {
            parse_document(md)
                .lines
                .iter()
                .map(|line| (leaf_kind_token(&line.kind), line.display_text.clone()))
                .collect()
        }

        assert_eq!(
            kinds("[foo]:\n/url\n===\n"),
            vec![
                ("p".into(), "[foo]:".into()),
                ("p".into(), "/url".into()),
                ("p".into(), "===".into()),
                ("empty".into(), "".into()),
            ]
        );
        assert_eq!(
            kinds("[\nfoo\n]: /url\nbar\n===\n"),
            vec![
                ("p".into(), "[".into()),
                ("p".into(), "foo".into()),
                ("p".into(), "]: /url".into()),
                ("p".into(), "bar".into()),
                ("p".into(), "===".into()),
                ("empty".into(), "".into()),
            ]
        );
        assert_eq!(
            kinds("- foo\n\n    [bar]: /url\n===\n"),
            vec![
                ("ul:0".into(), "foo".into()),
                ("empty".into(), "".into()),
                ("p".into(), "    [bar]: /url".into()),
                ("p".into(), "===".into()),
                ("empty".into(), "".into()),
            ]
        );
        assert_eq!(
            kinds("<pre/>\n===\n"),
            vec![
                ("p".into(), "<pre/>".into()),
                ("p".into(), "===".into()),
                ("empty".into(), "".into()),
            ]
        );
        assert_eq!(
            kinds("<pre/>\n---\n"),
            vec![
                ("p".into(), "<pre/>".into()),
                ("hr".into(), "".into()),
                ("empty".into(), "".into()),
            ]
        );
        assert_eq!(
            kinds("<!DOCTYPE html>\n    code\n"),
            vec![
                ("p".into(), "<!DOCTYPE html>".into()),
                ("p".into(), "    code".into()),
                ("empty".into(), "".into()),
            ]
        );
        assert_eq!(
            kinds("<pre>\n</script>\n    x\n"),
            vec![
                ("p".into(), "<pre>".into()),
                ("p".into(), "</script>".into()),
                ("p".into(), "    x".into()),
                ("empty".into(), "".into()),
            ]
        );
        assert_eq!(
            kinds("- foo\n\t```\n"),
            vec![
                ("ul:0".into(), "foo".into()),
                ("fence-open".into(), "".into()),
                ("code".into(), "".into()),
            ]
        );
        assert_eq!(
            kinds("- foo\n\n\t```\n"),
            vec![
                ("ul:0".into(), "foo".into()),
                ("empty".into(), "".into()),
                ("fence-open".into(), "".into()),
                ("code".into(), "".into()),
            ]
        );
        assert_eq!(
            kinds("> foo\n\n\t```\n"),
            vec![
                ("quote".into(), "foo".into()),
                ("empty".into(), "".into()),
                ("fence-open".into(), "".into()),
                ("code".into(), "".into()),
            ]
        );
        assert_eq!(
            kinds("- foo\n\t# hi\n"),
            vec![
                ("ul:0".into(), "foo".into()),
                ("h1".into(), "hi".into()),
                ("empty".into(), "".into()),
            ]
        );
        assert_eq!(
            kinds("1. foo\n\t# hi\n"),
            vec![
                ("ol:1:0".into(), "foo".into()),
                ("h1".into(), "hi".into()),
                ("empty".into(), "".into()),
            ]
        );
        assert_eq!(
            kinds("10) foo\n\t# hi\n"),
            vec![
                ("p".into(), "10) foo".into()),
                ("h1".into(), "hi".into()),
                ("empty".into(), "".into()),
            ]
        );
        assert_eq!(
            kinds("> foo\n\n      ```\n"),
            vec![
                ("quote".into(), "foo".into()),
                ("empty".into(), "".into()),
                ("fence-open".into(), "".into()),
                ("code".into(), "".into()),
            ]
        );
        assert_eq!(
            kinds("> foo\n    ~~~\n"),
            vec![
                ("quote".into(), "foo".into()),
                ("p".into(), "    ~~~".into()),
                ("empty".into(), "".into()),
            ]
        );
        assert_eq!(
            kinds("<!DOCTYPE html>\n\t```\n"),
            vec![
                ("p".into(), "<!DOCTYPE html>".into()),
                ("fence-open".into(), "".into()),
                ("code".into(), "".into()),
            ]
        );
    }

    #[test]
    fn lone_cr_source_ranges() {
        let md = "---\r***\r";
        let doc = parse_document(md);
        assert_eq!(doc.lines[0].source, 0..3);
        assert_eq!(doc.lines[1].source, 4..7);
        assert_eq!(&md[doc.lines[0].source.clone()], "---");
        assert_eq!(&md[doc.lines[1].source.clone()], "***");
        let setext = parse_document("Foo\r---\rbar");
        assert_eq!(setext.lines[0].source, 0..3);
        assert_eq!(setext.lines[1].source, 4..7);
        assert_eq!(setext.lines[2].source, 8..11);
        assert!(matches!(setext.lines[1].kind, LineKind::SetextUnderline));
    }

    #[test]
    fn html_lines_keep_red_display() {
        let table = "<table>\n  <tr>\n    <td>\n           hi\n    </td>\n  </tr>\n</table>\n\nokay.";
        let doc = parse_document(table);
        let td = doc
            .lines
            .iter()
            .find(|line| line.display_text.contains("<td>"))
            .expect("td");
        assert!(matches!(td.kind, LineKind::Paragraph));
        assert_eq!(td.display_text, "    <td>");
        let hi = doc
            .lines
            .iter()
            .find(|line| line.display_text.contains("hi"))
            .expect("hi");
        assert!(matches!(hi.kind, LineKind::Paragraph));
        assert!(
            hi.display_text.starts_with("           hi"),
            "{:?}",
            hi.display_text
        );
        assert!(!doc.lines.iter().any(|line| matches!(line.kind, LineKind::CodeLine)));

        let div = parse_document(" <div>\n  *hello*\n         <foo><a>");
        let foo = div
            .lines
            .iter()
            .find(|line| line.display_text.contains("<foo>"))
            .expect("foo");
        assert!(matches!(foo.kind, LineKind::Paragraph));
        assert_eq!(foo.display_text, "         <foo><a>");

        let cdata = "<![CDATA[\nfunction matchwo(a,b)\n{\n  if (a < b && a < 0) then {\n    return 1;\n\n  } else {\n\n    return 0;\n  }\n}\n]]>\nokay";
        let doc = parse_document(cdata);
        for needle in ["    return 1;", "    return 0;"] {
            let line = doc
                .lines
                .iter()
                .find(|line| line.display_text.contains(needle.trim()))
                .unwrap_or_else(|| panic!("missing {needle}"));
            assert!(
                matches!(line.kind, LineKind::Paragraph),
                "{needle} {:?}",
                line.kind
            );
            assert_eq!(line.display_text, needle);
        }
        assert!(!doc.lines.iter().any(|line| matches!(line.kind, LineKind::CodeLine)));
    }

    #[test]
    fn indented_code_stays_out_of_list_continuations() {
        let ex254 = "1.  A paragraph\n    with two lines.\n\n        indented code\n\n    > A block quote.";
        let doc = parse_document(ex254);
        let kinds: Vec<String> = doc.lines.iter().map(|line| leaf_kind_token(&line.kind)).collect();
        assert_eq!(
            kinds,
            vec!["ol:1:0", "p", "empty", "code", "empty", "quote"]
        );
        assert_eq!(doc.lines[1].display_text, "    with two lines.");
        assert_eq!(doc.lines[3].display_text, "indented code");
        assert!(!matches!(doc.lines[5].kind, LineKind::CodeLine));

        let ex263 = "1.  foo\n\n    ```\n    bar\n    ```\n\n    baz\n\n    > bam";
        let doc = parse_document(ex263);
        assert!(matches!(doc.lines[2].kind, LineKind::CodeFence { opening: true, .. }));
        assert!(matches!(doc.lines[3].kind, LineKind::CodeLine));
        assert_eq!(doc.lines[3].display_text, "bar");
        assert!(matches!(doc.lines[4].kind, LineKind::CodeFence { opening: false, .. }));
        assert!(matches!(doc.lines[6].kind, LineKind::Paragraph));
        assert!(!matches!(doc.lines[6].kind, LineKind::CodeLine));
        assert!(matches!(doc.lines[8].kind, LineKind::Blockquote));

        let ex324 = "1. ```\n   foo\n   ```\n\n   bar";
        let doc = parse_document(ex324);
        assert!(matches!(
            doc.lines[2].kind,
            LineKind::CodeFence { opening: false, .. }
        ));
        let bar = doc.lines.last().expect("bar");
        assert!(
            matches!(bar.kind, LineKind::Paragraph),
            "bar became {:?}",
            bar.kind
        );
        assert_eq!(bar.display_text, "   bar");

        let ex318 = "- a\n- ```\n  b\n\n\n  ```\n- c";
        let doc = parse_document(ex318);
        assert!(matches!(
            doc.lines[5].kind,
            LineKind::CodeFence { opening: false, .. }
        ));
        assert!(matches!(doc.lines[6].kind, LineKind::ListItem { .. }));
        assert_eq!(doc.lines[6].display_text, "c");
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
}
