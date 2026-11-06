//! GFM table detection: pipes, cells, delimiter rows and column alignment.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TableColumnAlign {
    /// Delimiter is plain `---`. A table edit must not turn this into `:---`.
    #[default]
    None,
    Left,
    Center,
    Right,
}

pub(super) fn code_span_ranges(line: &str) -> Vec<std::ops::Range<usize>> {
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

/// `ranges` must be sorted and non-overlapping (as `code_span_ranges` and
/// `merge_ranges` return them).
pub(super) fn index_in_ranges(ranges: &[std::ops::Range<usize>], index: usize) -> bool {
    let after = ranges.partition_point(|range| range.start <= index);
    after > 0 && index < ranges[after - 1].end
}

pub(super) fn separator_pipe(bytes: &[u8], code: &[std::ops::Range<usize>], index: usize) -> bool {
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

pub(super) fn table_cell_ranges(line: &str) -> Vec<std::ops::Range<usize>> {
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
            if let Ok(at) = code.binary_search_by_key(&start, |span| span.start) {
                start = code[at].end;
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

pub(super) fn is_delimiter_cell(cell: &str) -> bool {
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

pub(crate) fn delimiter_alignments(line: &str) -> Option<Vec<TableColumnAlign>> {
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

pub(super) fn is_empty_table_row(line: &str) -> bool {
    table_cell_ranges(line).is_empty()
        || line
            .bytes()
            .all(|byte| matches!(byte, b' ' | b'\t' | 0x0b | 0x0c | b'\r'))
}

pub(crate) fn line_breaks_table(line: &str) -> bool {
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

pub(super) struct TableBlock {
    pub(super) last: usize,
    pub(super) delimiter: usize,
    pub(super) alignments: Vec<TableColumnAlign>,
}

pub(super) struct DetectedTable {
    pub(super) first: usize,
    pub(super) last: usize,
    pub(super) delimiter: usize,
    pub(super) source: std::ops::Range<usize>,
    pub(super) alignments: Arc<[TableColumnAlign]>,
}

pub(super) fn table_block(lines: &[SourceLine<'_>], start: usize) -> Option<TableBlock> {
    // A header row needs a pipe; most lines have none, so check that first.
    if start + 1 >= lines.len()
        || memchr::memchr(b'|', lines[start].text.as_bytes()).is_none()
        || line_breaks_table(lines[start].text)
    {
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
