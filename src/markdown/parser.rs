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

    fn is_at_eol(&self) -> bool {
        self.bytes
            .get(self.ix)
            .map(|&byte| byte == b'\n')
            .unwrap_or(true)
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
#[derive(Default)]
struct ListWalk {
    open: Vec<usize>,
    paragraph: bool,
}

impl ListWalk {
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
            self.open.pop();
        }
    }

    /// A list marker this line is allowed to be, or nothing.
    ///
    /// CommonMark list items, rule 1 exception: an empty marker cannot
    /// interrupt a paragraph, and an ordered marker interrupts only when its
    /// start number is 1 (examples 285, 304, 367). Rule 4: a marker at column
    /// 4 or later is indented code unless it sits in a parent item (examples
    /// 1, 2, 107, and the nested marker in 296).
    fn take_list(&mut self, line: &str) -> Option<ListMarker> {
        self.prepare(line);
        let item = scan_list_marker(line)?;
        let marker_col = leading_column(line);
        let nested = !self.open.is_empty();
        if marker_col > 3 && !nested {
            return None;
        }
        let empty = item.task.is_none() && rest_is_blank(&line.as_bytes()[item.marker_end..]);
        if self.paragraph && !nested && (empty || (item.ordered && item.number != 1)) {
            return None;
        }
        self.open.push(column_at(line, item.marker_end));
        self.paragraph = false;
        Some(item)
    }

    fn note_block(&mut self, line: &str) {
        self.prepare(line);
        self.paragraph = false;
    }

    fn note_blank(&mut self) {
        self.paragraph = false;
    }

    fn note_paragraph(&mut self) {
        self.paragraph = self.open.is_empty();
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

        // Check code block fences
        if trimmed.starts_with("```") {
            list_walk.note_block(line);
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
            list_walk.note_block(line);
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

        // Horizontal Rule
        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
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
                        let (c_hid, c_pairs, c_segs) = parse_inline_syntax(inner_slice, c_start);
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

        if prefix_len > 0 {
            list_walk.note_block(line);
        } else if let Some(item) = list_walk.take_list(line) {
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

    /// F3 was Low. Stripping the CR hid it from the caret, so the editor
    /// keeps it in the line, the same as kit `4e72b711`. Pulldown still
    /// treats CR as a line ending in HTML.
    #[test]
    fn trailing_cr_stays_in_the_editor_line() {
        assert_eq!(
            observe_line("1.\r\n", 0),
            "Paragraph display=\"1.\\r\" strike=- link=- map=ok"
        );
        assert_eq!(
            observe_line("1)\r\n", 0),
            "Paragraph display=\"1)\\r\" strike=- link=- map=ok"
        );
        assert_eq!(
            observe_line("1. foo\r\n", 0),
            "ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"foo\\r\" strike=- link=- map=ok"
        );
        assert_eq!(
            observe_line("1. [ ]\r\n", 0),
            "ListItem ordered=true number=1 task=none indent=0 checkbox=- display=\"[ ]\\r\" strike=- link=- map=ok"
        );
        assert_eq!(
            observe_line("1. [ ] \r\n", 0),
            "ListItem ordered=true number=1 task=unchecked indent=0 checkbox=\"[ ]\" display=\"\\r\" strike=- link=- map=ok"
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
}
