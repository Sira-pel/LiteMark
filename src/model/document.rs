use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

use crate::markdown::{
    accepted_list_marker, accepted_list_marker_in, parse_document, unescaped_pipe_offsets,
    InlinePair, InlineTagKind, LineKind, ParsedDocument, ParsedLine, StyledSegment,
    TableColumnAlign, TableRowBody,
};

use super::table_ops::{self, TableOp};
use super::text_ops::{
    find_matches, find_next, find_prev, format_replacement, home_offset, line_col, line_end,
    line_ending_len, line_ending_len_before, line_start_flags, move_vertical_collapsed,
    next_grapheme, previous_grapheme, replace_all, snap_off_ending_interior, Caret, DiffEdit,
    HistoryRestore, SelectionSnap, UndoHistory, TAB_DISPLAY_COLUMNS,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FileType {
    #[default]
    Markdown,
    PlainText,
    Code,
    Pdf,
}

/// Extensions (lowercase, no dot) that open as code.
pub const CODE_EXTENSIONS: &[&str] = &[
    "rs", "c", "cpp", "cc", "cxx", "h", "hpp", "go", "java", "kt", "swift", "cs", "zig", "py",
    "js", "ts", "jsx", "tsx", "mjs", "cjs", "sh", "bash", "zsh", "fish", "rb", "php", "lua", "pl",
    "r", "json", "jsonc", "toml", "yaml", "yml", "xml", "html", "css", "scss", "sass", "less",
    "sql", "ini", "conf", "mk", "vue", "svelte",
];

/// Extensions (lowercase, no dot) of the text and Markdown files the Open dialog lists
/// besides code. `.pdf` is added by the dialog.
pub const TEXT_EXTENSIONS: &[&str] = &["md", "markdown", "txt", "text", "log"];

/// Whole file names (lowercase) that open as code although they have no extension.
const CODE_FILE_NAMES: &[&str] = &[
    "dockerfile",
    "makefile",
    "gnumakefile",
    ".env",
    ".gitignore",
];

impl FileType {
    /// `.md` and `.markdown` are Markdown. `.pdf` is PDF. Known source extensions and
    /// build file names are code. Every other name is plain text.
    pub fn from_filename(name: &str) -> Self {
        let lower = name.to_lowercase();
        if lower.ends_with(".pdf") {
            return FileType::Pdf;
        }
        if lower.ends_with(".md") || lower.ends_with(".markdown") {
            return FileType::Markdown;
        }
        if CODE_FILE_NAMES.contains(&lower.as_str()) {
            return FileType::Code;
        }
        match lower.rsplit_once('.') {
            Some((stem, ext)) if !stem.is_empty() && CODE_EXTENSIONS.contains(&ext) => {
                FileType::Code
            }
            _ => FileType::PlainText,
        }
    }

    /// Only Markdown tabs get Markdown formatting, lists, tables and WYSIWYG.
    pub fn supports_markdown_features(self) -> bool {
        self == FileType::Markdown
    }

    pub fn is_code(self) -> bool {
        self == FileType::Code
    }

    /// Code and plain text ignore the global view-mode, line-number and alignment choices.
    pub fn forces_raw(self) -> bool {
        matches!(self, FileType::Code | FileType::PlainText)
    }

    /// Code and plain text never render as formatted Markdown, whatever the global mode.
    pub fn effective_view_mode(self, global: ViewMode) -> ViewMode {
        if self.forces_raw() {
            ViewMode::Raw
        } else {
            global
        }
    }
}

/// What one indent level is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndentStyle {
    Tabs,
    Spaces(usize),
}

impl IndentStyle {
    /// Text of one indent level.
    pub fn unit(self) -> String {
        match self {
            IndentStyle::Tabs => "\t".to_string(),
            IndentStyle::Spaces(n) => " ".repeat(n),
        }
    }

    /// Columns per indent stop (a tab counts as one level).
    fn width(self) -> usize {
        match self {
            IndentStyle::Tabs => 1,
            IndentStyle::Spaces(n) => n.max(1),
        }
    }
}

/// Default indent for a file: tabs for Makefiles and Go, 4 spaces for other code,
/// 2 spaces for Markdown and plain text.
pub fn indent_style(name: &str, file_type: FileType) -> IndentStyle {
    let lower = name.to_lowercase();
    if lower == "makefile"
        || lower == "gnumakefile"
        || lower.ends_with(".mk")
        || lower.ends_with(".go")
    {
        return IndentStyle::Tabs;
    }
    if file_type.is_code() || file_type == FileType::PlainText {
        IndentStyle::Spaces(4)
    } else {
        IndentStyle::Spaces(2)
    }
}

/// Look at the first 200 lines: mostly tab-indented lines mean tabs, a clear space
/// majority with a consistent step means that step, anything mixed keeps `default`.
pub fn detect_indent_style(content: &str, default: IndentStyle) -> IndentStyle {
    let mut tab_lines = 0usize;
    let mut widths = Vec::new();
    for line in content.lines().take(200) {
        if line.trim().is_empty() {
            continue;
        }
        if line.starts_with('\t') {
            tab_lines += 1;
        } else if line.starts_with(' ') {
            // " * text" continuation lines of block comments are not indentation.
            if line.trim_start().starts_with('*') {
                continue;
            }
            widths.push(line.len() - line.trim_start_matches(' ').len());
        }
    }
    let space_lines = widths.len();
    if tab_lines > 2 * space_lines && tab_lines > 0 {
        return IndentStyle::Tabs;
    }
    if space_lines <= 2 * tab_lines {
        return default;
    }
    // A stray odd line (an aligned continuation) must not hide the step: take the
    // step that is the shallowest indent and that most lines are a multiple of.
    let shallowest = widths.iter().copied().min().unwrap_or(0);
    for step in [4, 2, 3] {
        let fits = widths.iter().filter(|w| **w % step == 0).count();
        if shallowest == step && fits * 4 >= space_lines * 3 {
            return IndentStyle::Spaces(step);
        }
    }
    default
}

/// Closing character for the pairs auto-closed in code: `( [ { "`.
pub fn code_pair_close(open: char) -> Option<char> {
    match open {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        '"' => Some('"'),
        _ => None,
    }
}

/// A file with a zero byte is binary, even when the bytes happen to be valid UTF-8.
pub fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8 * 1024).any(|&b| b == 0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ViewMode {
    #[default]
    Wysiwyg,
    Raw,
}

impl ViewMode {
    pub fn toggle(&self) -> Self {
        match self {
            ViewMode::Wysiwyg => ViewMode::Raw,
            ViewMode::Raw => ViewMode::Wysiwyg,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ViewMode::Wysiwyg => "WYSIWYG",
            ViewMode::Raw => "Raw",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ActiveFormats {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub h1: bool,
    pub h2: bool,
    pub h3: bool,
    pub h4: bool,
    pub h5: bool,
    pub h6: bool,
    pub ul: bool,
    pub ol: bool,
    pub checklist: bool,
    pub quote: bool,
    pub code: bool,
    pub link: bool,
    pub table: bool,
}

#[derive(Debug, Clone)]
pub struct DocumentStats {
    pub words: usize,
    pub chars: usize,
    pub lines: usize,
    pub reading_time_min: usize,
}

pub fn compute_stats(content: &str) -> DocumentStats {
    let chars = content.chars().count();
    let words = content.split_whitespace().count();
    let lines = if content.is_empty() {
        1
    } else {
        content.lines().count().max(1)
    };
    let reading_time_min = (words / 200).max(1);

    DocumentStats {
        words,
        chars,
        lines,
        reading_time_min,
    }
}

#[derive(Debug, Clone)]
pub struct DocumentTab {
    pub id: String,
    pub title: String,
    pub content: String,
    pub file_type: FileType,
    pub is_modified: bool,
    pub file_path: Option<PathBuf>,
    pub workspace_file_id: Option<String>,
    pub word_count: usize,
    pub char_count: usize,
    pub line_count: usize,
    pub reading_time_min: usize,
    pub pdf_page_count: Option<usize>,
    pub cursor_line: usize,
    pub cursor_col: usize,
    pub active_formats: ActiveFormats,
    pub pending_formats: ActiveFormats,
    pub caret: Caret,
    pub view_mode: ViewMode,
    pub indent_style: IndentStyle,
    /// Set only on the scratch tab used by Clear Formatting: forces inline
    /// toggles to turn the format off instead of deciding from the selection.
    force_inline_toggle: Option<bool>,
    pub marked_range: Option<Range<usize>>,
    pub revision: u64,
    cached_parse: Option<(u64, Arc<ParsedDocument>)>,
    stats_revision: Option<u64>,
    history: UndoHistory,
    /// Set right before the edit that wraps freshly typed text into a new
    /// formatted span, so that edit and the typing after it share one undo
    /// step.
    next_edit_opens_span_group: bool,
    /// When set, the next `edit` records this caret on its undo entry instead
    /// of the end of the deleted range (Enter rewrites a whole span).
    next_undo_caret: Option<usize>,
    /// Caret restored by redo of the next edit. Defaults to the end of the
    /// inserted text when unset.
    next_redo_caret: Option<usize>,
    /// Selection restored by undo of the next edit. Set only when the edit
    /// started with a non-empty selection.
    next_undo_sel: Option<SelectionSnap>,
    /// Selection restored by redo of the next edit.
    next_redo_sel: Option<SelectionSnap>,
}

impl DocumentTab {
    pub fn new(
        id: String,
        title: String,
        content: String,
        file_path: Option<PathBuf>,
        workspace_file_id: Option<String>,
    ) -> Self {
        let file_type = FileType::from_filename(&title);
        let stats = compute_stats(&content);
        let default_style = indent_style(&title, file_type);
        let indent_style = if file_type.is_code() {
            detect_indent_style(&content, default_style)
        } else {
            default_style
        };

        let mut tab = Self {
            id,
            title,
            content,
            file_type,
            is_modified: false,
            file_path,
            workspace_file_id,
            word_count: stats.words,
            char_count: stats.chars,
            line_count: stats.lines,
            reading_time_min: stats.reading_time_min,
            pdf_page_count: None,
            cursor_line: 1,
            cursor_col: 1,
            active_formats: ActiveFormats::default(),
            pending_formats: ActiveFormats::default(),
            caret: Caret::default(),
            view_mode: ViewMode::Wysiwyg,
            indent_style,
            force_inline_toggle: None,
            marked_range: None,
            revision: 1,
            cached_parse: None,
            stats_revision: Some(1),
            history: UndoHistory::default(),
            next_edit_opens_span_group: false,
            next_undo_caret: None,
            next_redo_caret: None,
            next_undo_sel: None,
            next_redo_sel: None,
        };
        tab.refresh_cursor();
        tab
    }

    pub fn parsed(&mut self) -> Arc<ParsedDocument> {
        if let Some((rev, ref doc)) = self.cached_parse {
            if rev == self.revision {
                return doc.clone();
            }
        }
        let doc = Arc::new(parse_document(&self.content));
        self.cached_parse = Some((self.revision, doc.clone()));
        doc
    }

    /// Drop the cached parse of a tab that isn't on screen; `parsed()`
    /// rebuilds it when the tab is shown again.
    pub fn release_parse_cache(&mut self) {
        self.cached_parse = None;
    }

    #[allow(dead_code)]
    pub fn parsed_snapshot(&self) -> Arc<ParsedDocument> {
        if let Some((rev, ref doc)) = self.cached_parse {
            if rev == self.revision {
                return doc.clone();
            }
        }
        Arc::new(parse_document(&self.content))
    }

    /// Re-detect the type after Save As or a rename. A change drops the parse cache,
    /// which was built for the old type. Returns whether the type changed.
    pub fn set_file_type(&mut self, file_type: FileType) -> bool {
        // The name may have changed even when the type did not (a.rs -> Makefile).
        let default_style = indent_style(&self.title, file_type);
        self.indent_style = if file_type.is_code() {
            detect_indent_style(&self.content, default_style)
        } else {
            default_style
        };
        if self.file_type == file_type {
            return false;
        }
        self.file_type = file_type;
        self.cached_parse = None;
        self.pending_formats = ActiveFormats::default();
        true
    }

    pub fn is_read_only(&self) -> bool {
        self.file_type == FileType::Pdf
    }

    pub fn cursor_offset(&self) -> usize {
        floor_boundary(&self.content, self.caret.cursor())
    }

    pub fn selection_range(&self) -> Range<usize> {
        let start = floor_boundary(&self.content, self.caret.range.start);
        let end = floor_boundary(&self.content, self.caret.range.end);
        if end < start {
            end..start
        } else {
            start..end
        }
    }

    #[allow(dead_code)]
    pub fn undo_stored_bytes(&self) -> usize {
        self.history.stored_bytes()
    }

    pub fn set_content(&mut self, new_content: String) {
        if self.content != new_content {
            let end = self.content.len();
            self.edit(0..end, &new_content);
        }
    }

    /// Replace `range` and record a diff. Read-only documents ignore the edit.
    pub fn edit(&mut self, range: Range<usize>, new_text: &str) {
        let undo_caret_override = self.next_undo_caret.take();
        let redo_caret_override = self.next_redo_caret.take();
        let undo_sel = self.next_undo_sel.take();
        let redo_sel = self.next_redo_sel.take();
        if self.is_read_only() {
            return;
        }
        let opens_span_group = std::mem::take(&mut self.next_edit_opens_span_group);
        let range = self.clamp_range(range);
        if !self.content.is_char_boundary(range.start) || !self.content.is_char_boundary(range.end)
        {
            return;
        }
        let deleted = self.content[range.clone()].to_string();
        if deleted == new_text {
            let cursor = range.start + new_text.len();
            self.caret.move_to(cursor.min(self.content.len()));
            self.marked_range = None;
            self.refresh_cursor();
            return;
        }
        let undo_caret = undo_caret_override.unwrap_or(range.end);
        let redo_caret = redo_caret_override.unwrap_or(range.start + new_text.len());
        let mut edit = DiffEdit::new(range.start, deleted, new_text.to_string());
        edit.undo_caret = undo_caret;
        edit.redo_caret = redo_caret;
        edit.undo_sel = undo_sel;
        edit.redo_sel = redo_sel;
        if opens_span_group {
            self.history.push_span_opening(edit);
        } else {
            self.history.push(edit);
        }
        let cursor_at = range.start + new_text.len();
        self.content.replace_range(range, new_text);
        let cursor = snap_off_ending_interior(&self.content, cursor_at);
        self.revision = self.revision.wrapping_add(1);
        self.caret.move_to(cursor.min(self.content.len()));
        self.marked_range = None;
        self.is_modified = true;
        self.refresh_cursor();
    }

    pub fn move_line_up(&mut self) {
        if self.is_read_only() {
            return;
        }
        let cursor = self.cursor_offset();
        let current_line_start = super::text_ops::home_offset(&self.content, cursor);
        if current_line_start == 0 {
            return;
        }
        let col = cursor - current_line_start;
        let prev_line_start = super::text_ops::home_offset(&self.content, current_line_start - 1);
        let current_line_end = super::text_ops::line_end(&self.content, current_line_start);
        let break_len = line_ending_len_before(&self.content, current_line_start);
        if break_len == 0 || current_line_start < break_len {
            return;
        }
        let break_at = current_line_start - break_len;
        let ending = self.content[break_at..current_line_start].to_string();
        let prev_line = self.content[prev_line_start..break_at].to_string();
        let curr_line = self.content[current_line_start..current_line_end].to_string();
        let curr_len = curr_line.len();

        let new_text = format!("{curr_line}{ending}{prev_line}");
        self.edit(prev_line_start..current_line_end, &new_text);

        let new_cursor = (prev_line_start + col).min(prev_line_start + curr_len);
        self.caret.move_to(new_cursor);
        self.snap_caret_to_visible_line_end();
        self.refresh_cursor();
    }

    pub fn move_line_down(&mut self) {
        if self.is_read_only() {
            return;
        }
        let cursor = self.cursor_offset();
        let current_line_start = super::text_ops::home_offset(&self.content, cursor);
        let current_line_end = super::text_ops::line_end(&self.content, current_line_start);
        if current_line_end >= self.content.len() {
            return;
        }
        let col = cursor - current_line_start;
        let ending_len = line_ending_len(&self.content, current_line_end);
        if ending_len == 0 {
            return;
        }
        let next_line_start = current_line_end + ending_len;
        let next_line_end = super::text_ops::line_end(&self.content, next_line_start);

        let curr_line = self.content[current_line_start..current_line_end].to_string();
        let ending = self.content[current_line_end..next_line_start].to_string();
        let next_line = self.content[next_line_start..next_line_end].to_string();
        let curr_len = curr_line.len();
        let next_len = next_line.len();

        let new_text = format!("{next_line}{ending}{curr_line}");
        self.edit(current_line_start..next_line_end, &new_text);

        let new_line_start = current_line_start + next_len + ending_len;
        let new_cursor = (new_line_start + col).min(new_line_start + curr_len);
        self.caret.move_to(new_cursor);
        self.snap_caret_to_visible_line_end();
        self.refresh_cursor();
    }

    /// Enter at the visible end of a closing-hash heading inserts after the
    /// closer, so the heading stays one line.
    fn heading_newline_at(&mut self, range: &Range<usize>) -> Option<usize> {
        if !range.is_empty() {
            return None;
        }
        let parsed = self.parsed();
        let line = parsed.line_at_offset(range.start)?;
        let vend = visible_line_end(line, self.view_mode);
        if range.start == vend && vend < line.source.end {
            Some(line.source.end)
        } else {
            None
        }
    }

    /// `Foo\r` + `\n` + `Bar` is one CRLF ending. Enter at the start of the
    /// bare-CR line has to insert two breaks to add a line.
    fn bare_cr_needs_extra_break(&self, range: &Range<usize>) -> bool {
        if !range.is_empty() {
            return false;
        }
        let cursor = range.start;
        let line_start = home_offset(&self.content, cursor);
        cursor == line_start
            && cursor > 0
            && self.content.as_bytes().get(cursor - 1) == Some(&b'\r')
    }

    /// The table whose block contains `pos`, with its body rows in order.
    fn table_at(&mut self, pos: usize) -> Option<TableInfo> {
        let parsed = self.parsed();
        let mut info: Option<TableInfo> = None;
        for line in &parsed.lines {
            if let LineKind::TableRow {
                is_header,
                ref body,
                ..
            } = line.kind
            {
                let TableRowBody {
                    cells,
                    table_source,
                    ..
                } = &**body;
                if pos < table_source.start || pos > table_source.end {
                    continue;
                }
                let info = info.get_or_insert_with(|| TableInfo {
                    source: table_source.clone(),
                    rows: Vec::new(),
                    delimiter_end: None,
                });
                info.rows.push(TableRowInfo {
                    source: line.source.clone(),
                    cells: cells.clone(),
                    is_header,
                });
            }
        }
        let info = info.as_mut()?;
        for line in &parsed.lines {
            if matches!(line.kind, LineKind::TableDelimiter)
                && line.source.start >= info.source.start
                && line.source.end <= info.source.end
            {
                info.delimiter_end = Some(line.source.end);
            }
        }
        info.rows.sort_by_key(|r| r.source.start);
        info.clone().into()
    }

    /// Replace `remove_start..remove_end` (an optional trailing empty row,
    /// otherwise empty at the table end) and put the caret on a paragraph line
    /// below the table, with exactly one blank line between them. One undo step.
    fn apply_table_exit(&mut self, remove_start: usize, remove_end: usize) {
        let rest = &self.content[remove_end..];
        let (insert_at, insert_text, caret_rel) = blank_gap_after_table(rest);
        let end = remove_end + insert_at;
        let replacement = format!("{}{}", &self.content[remove_end..end], insert_text);
        if remove_start != end || !replacement.is_empty() {
            self.edit(remove_start..end, &replacement);
        }
        let target = floor_boundary(&self.content, remove_start + caret_rel);
        self.caret.move_to(target);
        self.refresh_cursor();
    }

    /// Ctrl+Enter: add an empty row below the current row and put the caret in
    /// the same column. Returns false outside a table.
    pub fn table_insert_row_below(&mut self) -> bool {
        if self.is_read_only() || !self.file_type.supports_markdown_features() {
            return false;
        }
        let cursor = self.selection_range().end;
        let Some(table) = self.table_at(cursor) else {
            return false;
        };
        let Some(row_pos) = table
            .rows
            .iter()
            .position(|r| cursor >= r.source.start && cursor <= r.source.end)
        else {
            // On the delimiter row: add the first body row
            let at = table.delimiter_end.unwrap_or(table.source.end);
            let cols = table.rows.first().map(|r| r.cells.len()).unwrap_or(2).max(2);
            let row = format!("\n|{}", "  |".repeat(cols));
            self.edit(at..at, &row);
            self.caret.move_to(floor_boundary(&self.content, at + 3));
            self.refresh_cursor();
            return true;
        };
        let row = &table.rows[row_pos];
        let col = row
            .cells
            .iter()
            .position(|c| cursor >= c.start.saturating_sub(1) && cursor <= c.end + 1)
            .unwrap_or_else(|| row.cells.iter().rposition(|c| c.start <= cursor).unwrap_or(0));
        let cols = row.cells.len().max(2);
        let at = if row.is_header {
            table.delimiter_end.unwrap_or(row.source.end)
        } else {
            row.source.end
        };
        let new_row = format!("\n|{}", "  |".repeat(cols));
        self.edit(at..at, &new_row);
        let target = at + 3 + 3 * col.min(cols - 1);
        self.caret.move_to(floor_boundary(&self.content, target));
        self.refresh_cursor();
        true
    }

    /// Up on the first row or Down on the last row of a table leaves the table.
    /// Returns false when the caret is not on that edge row.
    pub fn table_edge_exit(&mut self, delta: i32) -> bool {
        if self.is_read_only() {
            return false;
        }
        let cursor = self.cursor_offset();
        let Some(table) = self.table_at(cursor) else {
            return false;
        };
        let Some(row_pos) = table
            .rows
            .iter()
            .position(|r| cursor >= r.source.start && cursor <= r.source.end)
        else {
            return false;
        };
        if delta < 0 {
            if row_pos != 0 {
                return false;
            }
            let start = table.source.start;
            if start == 0 {
                self.edit(0..0, "\n\n");
                self.caret.move_to(0);
            } else {
                let ending = line_ending_len_before(&self.content, start);
                self.caret
                    .move_to(floor_boundary(&self.content, start - ending.max(1)));
            }
            self.pending_formats = ActiveFormats::default();
            self.refresh_cursor();
            true
        } else {
            if row_pos + 1 != table.rows.len() {
                return false;
            }
            let end = table.source.end;
            let ending = line_ending_len(&self.content, end);
            if end + ending >= self.content.len() {
                // The table ends the note: make room below it
                self.apply_table_exit(end, end);
            } else {
                self.caret.move_to(end + ending);
                self.refresh_cursor();
            }
            self.pending_formats = ActiveFormats::default();
            true
        }
    }

    /// Enter with the caret inside an inline formatting span: close the span
    /// before the newline and reopen it after `line_ins` on the new line, so
    /// no markers straddle the break. A caret at the end of the span closes
    /// it and keeps the formats armed so the next letter reopens them on the
    /// new line. `line_ins` is the line-prefix text the caller would insert
    /// at the caret (for example `"\n> "` inside a blockquote).
    fn split_inline_span_newline(&mut self, range: &Range<usize>, line_ins: &str) -> bool {
        if !range.is_empty() {
            return false;
        }
        let cursor = range.start;
        let parsed = self.parsed();
        let mut spanning: Vec<InlinePair> = line_pair_refs(&parsed, cursor)
            .iter()
            .filter(|p| {
                p.kind != InlineTagKind::Link
                    && cursor > p.open_delim.start
                    && cursor < p.close_delim.end
                    && cursor >= p.inner.start
                    && cursor <= p.inner.end
            })
            .cloned()
            .collect();
        if spanning.is_empty() {
            return false;
        }
        spanning.sort_by_key(|p| p.open_delim.start);

        let innermost = spanning.last().cloned().unwrap();
        if cursor == innermost.inner.end {
            // End of the span: leave it, keeping the formats armed for the
            // new line.
            let pending = pending_formats_at(&parsed, cursor);
            let at = spanning
                .iter()
                .map(|p| p.close_delim.end)
                .max()
                .unwrap_or(innermost.close_delim.end);
            self.edit(at..at, line_ins);
            self.pending_formats = pending;
            self.caret
                .move_to(floor_boundary(&self.content, at + line_ins.len()));
            self.refresh_cursor();
            return true;
        }

        let run_start = spanning
            .iter()
            .map(|p| p.open_delim.start)
            .min()
            .unwrap_or(innermost.open_delim.start);
        let run_end = spanning
            .iter()
            .map(|p| p.close_delim.end)
            .max()
            .unwrap_or(innermost.close_delim.end);

        let open_ranges =
            merge_adjacent_ranges(spanning.iter().map(|p| p.open_delim.clone()).collect());
        let close_ranges =
            merge_adjacent_ranges(spanning.iter().map(|p| p.close_delim.clone()).collect());
        let openers: String = open_ranges
            .iter()
            .map(|r| &self.content[r.clone()])
            .collect();
        let closers: String = close_ranges
            .iter()
            .map(|r| &self.content[r.clone()])
            .collect();

        let prefix = self.content[run_start..cursor].to_string();
        let suffix = self.content[cursor..run_end].to_string();
        let before_core_len = prefix.trim_end().len();
        let before_tail = &prefix[before_core_len..];
        let after_head_len = suffix.len() - suffix.trim_start().len();
        let after_head = &suffix[..after_head_len];
        let after_core = &suffix[after_head_len..];
        let mut repl = String::new();
        if before_core_len > 0 {
            repl.push_str(&prefix[..before_core_len]);
            repl.push_str(&closers);
        }
        repl.push_str(before_tail);
        repl.push_str(line_ins);
        if !after_core.is_empty() {
            repl.push_str(after_head);
            repl.push_str(&openers);
            repl.push_str(after_core);
        } else {
            repl.push_str(after_head);
        }
        let caret = if after_core.is_empty() {
            run_start + repl.len()
        } else {
            run_start + repl.len() - after_core.len()
        };
        self.next_undo_caret = Some(cursor);
        self.edit(run_start..run_end, &repl);
        self.caret.move_to(floor_boundary(&self.content, caret));
        self.refresh_cursor();
        true
    }

    pub fn smart_newline(&mut self, is_shift: bool) {
        if self.is_read_only() {
            return;
        }
        self.history.seal();
        let range = self.selection_range();
        if !self.file_type.supports_markdown_features() {
            self.code_newline(range);
            return;
        }
        if is_shift {
            // Shift+Enter in a table cell leaves the table without touching it.
            if let Some(table) = self.table_at(range.start) {
                self.apply_table_exit(table.source.end, table.source.end);
                return;
            }
            if let Some(at) = self.heading_newline_at(&range) {
                self.edit(at..at, "\n");
                return;
            }
            if self.bare_cr_needs_extra_break(&range) {
                self.edit(range.start..range.start, "\n\n");
                return;
            }
            if self.split_inline_span_newline(&range, "\n") {
                return;
            }
            self.edit(range, "\n");
            return;
        }

        let cursor = range.start;
        let line_start = home_offset(&self.content, cursor);
        let line_end_pos = line_end(&self.content, cursor);

        // 0. Table rows: continuation or exit
        let parsed = self.parsed();
        if let Some(pline) = parsed.line_at_offset(cursor) {
            if let LineKind::TableRow {
                is_header,
                ref body,
                ..
            } = pline.kind
            {
                let TableRowBody { cells, .. } = &**body;
                let col_count = cells.len().max(2);
                let is_empty_row = cells.iter().all(|c| {
                    if c.end <= self.content.len() && c.start <= c.end {
                        self.content[c.clone()].trim().is_empty()
                    } else {
                        true
                    }
                });

                if is_header {
                    // Header row: find delimiter line and jump to / create first data row
                    if let Some(delim_line) = parsed.lines.get(pline.line_index + 1) {
                        let after_delim = delim_line.source.end;
                        // Check if there is already a body row after delimiter
                        if let Some(first_body) = parsed.lines.get(pline.line_index + 2) {
                            if let LineKind::TableRow {
                                is_header: false,
                                ref body,
                                ..
                            } = first_body.kind
                            {
                                let TableRowBody { cells, .. } = &**body;
                                if let Some(first_cell) = cells.first() {
                                    self.caret.move_to(first_cell.start);
                                    self.refresh_cursor();
                                    return;
                                }
                            }
                        }
                        // Otherwise insert first body row after delimiter
                        let new_row = format!("\n|{}", "  |".repeat(col_count));
                        self.edit(after_delim..after_delim, &new_row);
                        let target = (after_delim + 3).min(self.content.len());
                        self.caret.move_to(floor_boundary(&self.content, target));
                        self.refresh_cursor();
                        return;
                    }
                }

                if is_empty_row {
                    let table = self.table_at(cursor);
                    let row_pos = table
                        .as_ref()
                        .and_then(|t| t.rows.iter().position(|r| r.source.start == pline.source.start));
                    if let (Some(table), Some(row_pos)) = (table, row_pos) {
                        if row_pos + 1 < table.rows.len() {
                            // Not the last row: move to the same column in the next row
                            let col = table.rows[row_pos]
                                .cells
                                .iter()
                                .position(|c| cursor >= c.start.saturating_sub(1) && cursor <= c.end + 1)
                                .unwrap_or(0);
                            let next = &table.rows[row_pos + 1];
                            let target = next
                                .cells
                                .get(col)
                                .or(next.cells.last())
                                .map(|c| c.start)
                                .unwrap_or(next.source.start);
                            self.caret.move_to(floor_boundary(&self.content, target));
                            self.refresh_cursor();
                            return;
                        }
                    }
                    // Last empty row: remove it, keeping the header, and exit below the table
                    let remove_start =
                        if line_start > 0 && self.content.as_bytes()[line_start - 1] == b'\n' {
                            line_start - 1
                        } else {
                            line_start
                        };
                    self.apply_table_exit(remove_start, line_end_pos);
                    return;
                } else {
                    // Non-empty row: check if cursor is inside a cell with content to split
                    let row_text = &self.content[line_start..line_end_pos];
                    let pipe_indices: Vec<usize> = unescaped_pipe_offsets(row_text)
                        .into_iter()
                        .map(|idx| line_start + idx)
                        .collect();

                    let mut split_cell_info: Option<(usize, usize, usize, String, String)> = None;
                    if pipe_indices.len() >= 2 {
                        for k in 0..pipe_indices.len() - 1 {
                            let cell_start = pipe_indices[k] + 1;
                            let cell_end = pipe_indices[k + 1];
                            if cursor >= cell_start && cursor <= cell_end {
                                // Only split when the whole selection sits inside this cell;
                                // otherwise the start/end slices below would be out of order.
                                if range.start < cell_start || range.end > cell_end {
                                    break;
                                }
                                let before = &self.content[cell_start..range.start];
                                let after = &self.content[range.end..cell_end];
                                if !after.trim().is_empty() {
                                    split_cell_info = Some((
                                        k,
                                        cell_start,
                                        cell_end,
                                        before.to_string(),
                                        after.trim().to_string(),
                                    ));
                                }
                                break;
                            }
                        }
                    }

                    if let Some((col_idx, _c_start, c_end, before_str, after_str)) = split_cell_info
                    {
                        // Split the cell into current row and a new row below in a single undo step.
                        // Close spans in the current cell and reopen them on the new row so
                        // `| **first half** |` becomes `| **first** |` / `| **half** |`.
                        let (span_open, span_close) =
                            spanning_marker_runs(&self.content, &parsed, cursor)
                                .unwrap_or_default();
                        let after_str = format!("{span_open}{after_str}");
                        let tail_len = before_str.len() - before_str.trim_end().len();
                        let edit_start = range.start - tail_len;
                        let tail = &self.content[edit_start..range.start];
                        let rest_of_line = &self.content[c_end..line_end_pos];
                        let n_cols = cells.len().max(1);
                        let mut new_cells = Vec::new();
                        for i in 0..n_cols {
                            if i == col_idx {
                                new_cells.push(format!(" {} ", after_str));
                            } else {
                                new_cells.push("  ".to_string());
                            }
                        }
                        let new_row = format!("\n|{}|", new_cells.join("|"));
                        let combined =
                            format!("{}{}{}{}", span_close, tail, rest_of_line, new_row);
                        let new_row_start = edit_start
                            + span_close.len()
                            + tail.len()
                            + rest_of_line.len()
                            + 1;
                        self.edit(edit_start..line_end_pos, &combined);

                        let mut target = new_row_start + 1;
                        for cell in new_cells.iter().take(col_idx) {
                            target += cell.len() + 1;
                        }
                        target += 1;
                        target += span_open.len();
                        let caret_pos = floor_boundary(&self.content, target);
                        self.caret.move_to(caret_pos);
                        self.refresh_cursor();
                        return;
                    }

                    let new_row = format!("\n|{}", "  |".repeat(col_count));
                    self.edit(line_end_pos..line_end_pos, &new_row);
                    let target = (line_end_pos + 3).min(self.content.len());
                    self.caret.move_to(floor_boundary(&self.content, target));
                    self.refresh_cursor();
                    return;
                }
            }
        }

        if let Some(at) = self.heading_newline_at(&range) {
            self.edit(at..at, "\n");
            return;
        }
        if self.bare_cr_needs_extra_break(&range) {
            self.edit(range.start..range.start, "\n\n");
            return;
        }

        let parsed = self.parsed();
        let line = &self.content[line_start..line_end_pos];
        let trimmed_start = line.trim_start();
        let indent = &line[..line.len() - trimmed_start.len()];

        if let Some(item) = accepted_list_marker_in(&parsed, &self.content, line_start) {
            let body = &line[item.prefix_end()..];
            // The caret stops before a CR line ending, so the CR is not in
            // `body`. An ordered task or a paren marker still does not exit
            // when that ending is a CR. A plain `1. ` or `- [ ] ` does.
            let ending_is_cr = self.content.as_bytes().get(line_end_pos) == Some(&b'\r');
            let cr_keeps_item =
                ending_is_cr && (item.punct == b')' || (item.ordered && item.task.is_some()));
            let is_empty = body.trim().is_empty() && !cr_keeps_item;
            if is_empty {
                if !range.is_empty() {
                    self.edit(range, "\n");
                    return;
                }
                if item.indent_bytes > 0 {
                    let indent = &line[..item.indent_bytes];
                    let outdent_len = if indent.ends_with('\t') || indent.len() < 2 {
                        1
                    } else {
                        2
                    };
                    self.edit(line_start..line_start + outdent_len, "");
                } else {
                    // A CRLF ending loses the CR with the marker, leaving LF.
                    // The caret never stood on that CR.
                    let remove_end = if line_ending_len(&self.content, line_end_pos) == 2 {
                        line_end_pos + 1
                    } else {
                        line_end_pos
                    };
                    self.edit(line_start..remove_end, "");
                }
            } else {
                if range.is_empty() && cursor < line_start + item.prefix_end() {
                    // Caret at or inside the marker: open a blank line above
                    // instead of copying the marker onto the moved text.
                    self.edit(line_start..line_start, "\n");
                    self.caret.move_to(cursor + 1);
                    self.refresh_cursor();
                    return;
                }
                let kept = &line[..item.indent_bytes];
                let cont = item.continuation(kept);
                if self.split_inline_span_newline(&range, &cont) {
                    return;
                }
                self.edit(range, &cont);
            }
            return;
        }

        // Blockquotes: "> "
        if trimmed_start.starts_with("> ") || trimmed_start == ">" {
            let is_empty_quote = trimmed_start == ">"
                || trimmed_start == "> "
                || trimmed_start[1..].trim().is_empty();
            if is_empty_quote {
                if !range.is_empty() {
                    self.edit(range, "\n");
                } else {
                    self.edit(line_start..line_end_pos, "");
                }
            } else if range.is_empty() && cursor < line_start + indent.len() + 2 {
                self.edit(line_start..line_start, "\n");
                self.caret.move_to(cursor + 1);
                self.refresh_cursor();
            } else {
                let ins = format!("\n{}> ", indent);
                if self.split_inline_span_newline(&range, &ins) {
                    return;
                }
                self.edit(range, &ins);
            }
            return;
        }

        // 5. Headings: "# ", "## ", etc. -> normal newline
        let is_heading = indent.len() <= 3
            && ["###### ", "##### ", "#### ", "### ", "## ", "# "]
                .iter()
                .any(|p| trimmed_start.starts_with(p));
        if is_heading {
            let text_start = line_start + block_prefix_in(&self.content, line_start);
            if range.is_empty() && cursor <= text_start && !line[text_start - line_start..].trim().is_empty() {
                // Before the heading text: push the heading down, keep it a heading.
                self.edit(line_start..line_start, "\n");
                self.caret.move_to(cursor + 1);
                self.refresh_cursor();
                return;
            }
            if self.split_inline_span_newline(&range, "\n") {
                return;
            }
            self.edit(range, "\n");
            return;
        }

        // 5b. A whitespace-only line right under indented code (left by the
        // previous Enter): Enter clears it and ends the block, the way Enter
        // on an empty list item ends the list.
        if trimmed_start.is_empty() && !indent.is_empty() && range.is_empty() {
            let above = parsed
                .line_at_offset(line_start)
                .and_then(|line| line.line_index.checked_sub(1))
                .and_then(|index| parsed.lines.get(index));
            if above.is_some_and(|line| is_indented_code(&parsed, line)) {
                self.edit(line_start..line_end_pos, "");
                return;
            }
        }

        // 6. Indented code/text: maintain indentation
        if !indent.is_empty() && !trimmed_start.is_empty() {
            let ins = format!("\n{}", indent);
            if self.split_inline_span_newline(&range, &ins) {
                return;
            }
            self.edit(range, &ins);
            return;
        }

        // 7. Default newline
        if self.split_inline_span_newline(&range, "\n") {
            return;
        }
        self.edit(range, "\n");
    }

    /// The line break to insert at `offset`: the ending of the line being split, else
    /// the one before it, else the file's first ending, so mixed files stay local.
    fn line_break(&self, offset: usize) -> &'static str {
        let ending = |at: usize| match line_ending_len(&self.content, at) {
            2 => Some("\r\n"),
            1 if self.content.as_bytes()[at] == b'\n' => Some("\n"),
            _ => None,
        };
        let line_start = home_offset(&self.content, offset);
        if let Some(found) = ending(line_end(&self.content, offset)) {
            return found;
        }
        match line_ending_len_before(&self.content, line_start) {
            2 => return "\r\n",
            1 if line_start > 0 && self.content.as_bytes()[line_start - 1] == b'\n' => return "\n",
            _ => {}
        }
        match self.content.find('\n') {
            Some(i) if i > 0 && self.content.as_bytes()[i - 1] == b'\r' => "\r\n",
            _ => "\n",
        }
    }

    /// Enter in a non-Markdown file: break the line and keep its leading whitespace.
    /// No list, quote or table continuation, and nothing is parsed. In code an opener
    /// (`{ ( [`, or `:` in Python) adds one indent level, and Enter between a matching
    /// pair opens a three-line block. All of it is a single edit, so one undo step.
    fn code_newline(&mut self, range: Range<usize>) {
        let nl = self.line_break(range.start);
        let line_start = home_offset(&self.content, range.start);
        let before = &self.content[line_start..range.start];
        let indent: String = before
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        if !self.file_type.is_code() {
            self.edit(range, &format!("{nl}{indent}"));
            return;
        }
        let unit = self.indent_style.unit();
        let opener = before.trim_end_matches([' ', '\t']).chars().next_back();
        let python_colon = opener == Some(':') && self.title.to_lowercase().ends_with(".py");
        let deeper = matches!(opener, Some('{' | '(' | '['));
        let inner = if deeper || python_colon {
            format!("{indent}{unit}")
        } else {
            indent.clone()
        };

        let prev = self.content[..range.start].chars().next_back();
        let next = self.content[range.end..].chars().next();
        let between_pair = match (prev, next) {
            (Some(open), Some(close)) => open != '"' && code_pair_close(open) == Some(close),
            _ => false,
        };
        if between_pair {
            let caret = range.start + nl.len() + inner.len();
            self.next_redo_caret = Some(caret);
            self.edit(range, &format!("{nl}{inner}{nl}{indent}"));
            self.caret.move_to(caret);
            self.refresh_cursor();
            return;
        }
        self.edit(range, &format!("{nl}{inner}"));
    }

    pub fn smart_backspace(&mut self) -> bool {
        if self.is_read_only() {
            return false;
        }
        let range = self.selection_range();
        if !range.is_empty() {
            self.edit(range, "");
            return true;
        }

        let cursor = self.cursor_offset();
        if cursor == 0 {
            return false;
        }

        // 1. Auto-pairing bracket/quote deletion: (|), [|], {|}, "|", and in Markdown '|', `|`
        if cursor < self.content.len() {
            let prev_char = self.content[..cursor].chars().next_back().unwrap();
            let next_char = self.content[cursor..].chars().next().unwrap();
            let markdown = self.file_type.supports_markdown_features();
            let is_paired = code_pair_close(prev_char) == Some(next_char)
                || (markdown && matches!((prev_char, next_char), ('\'', '\'') | ('`', '`')));
            if is_paired {
                let p_len = prev_char.len_utf8();
                let n_len = next_char.len_utf8();
                self.edit(cursor - p_len..cursor + n_len, "");
                return true;
            }
        }

        if !self.file_type.supports_markdown_features() {
            let line_start = home_offset(&self.content, cursor);
            let line_before = &self.content[line_start..cursor];
            // In leading spaces, step back to the previous indent stop.
            if let IndentStyle::Spaces(_) = self.indent_style {
                let width = self.indent_style.width();
                if !line_before.is_empty() && line_before.chars().all(|c| c == ' ') {
                    let col = line_before.len();
                    let step = match col % width {
                        0 => width,
                        rest => rest,
                    };
                    self.edit(cursor - step..cursor, "");
                    return true;
                }
            }
            let prev = previous_grapheme(&self.content, cursor);
            self.edit(prev..cursor, "");
            return true;
        }

        let parsed = self.parsed();
        let line_start = home_offset(&self.content, cursor);
        let line_before = &self.content[line_start..cursor];

        // 2. Heading prefix at cursor boundary
        for (prefix, demoted) in [
            ("###### ", "##### "),
            ("##### ", "#### "),
            ("#### ", "### "),
            ("### ", "## "),
            ("## ", "# "),
            ("# ", ""),
        ] {
            if line_before == prefix {
                self.edit(line_start..cursor, demoted);
                return true;
            }
        }

        if let Some(item) = accepted_list_marker_in(&parsed, &self.content, line_start) {
            if item.prefix_end() == line_before.len() {
                if item.task.is_some() {
                    let start = line_start + item.marker_end;
                    self.edit(start..cursor, "");
                    return true;
                }
                if item.indent_bytes > 0 {
                    let indent = &line_before[..item.indent_bytes];
                    let outdent_len = if indent.ends_with('\t') || indent.len() < 2 {
                        1
                    } else {
                        2
                    };
                    self.edit(line_start..line_start + outdent_len, "");
                } else {
                    self.edit(line_start..cursor, "");
                }
                return true;
            }
        }

        // Blockquote prefix at cursor boundary
        if line_before == "> " {
            self.edit(line_start..cursor, "");
            return true;
        }

        // 7. Indentation spaces deletion (outdent 2 spaces if only spaces before cursor on this line)
        if cursor >= line_start + 2 && line_before.chars().all(|c| c == ' ') {
            self.edit(cursor - 2..cursor, "");
            return true;
        }

        // 8. Normal backspace: previous grapheme
        let prev = previous_grapheme(&self.content, cursor);
        self.edit(prev..cursor, "");
        true
    }

    /// First and one-past-last byte of the lines a Tab or Shift+Tab touches. In
    /// code, a selection that ends exactly at the start of a line leaves that line out.
    fn indent_target_lines(&self, range: &Range<usize>) -> (usize, usize) {
        let start_line = home_offset(&self.content, range.start);
        let mut end = range.end;
        if !self.file_type.supports_markdown_features()
            && range.end > range.start
            && home_offset(&self.content, range.end) == range.end
        {
            end -= line_ending_len_before(&self.content, range.end);
        }
        (start_line, line_end(&self.content, end).max(start_line))
    }

    pub fn indent(&mut self) {
        if self.is_read_only() {
            return;
        }
        let range = self.selection_range();
        let markdown = self.file_type.supports_markdown_features();
        let style = self.indent_style;
        if !markdown && range.is_empty() {
            // A lone caret inserts up to the next indent stop.
            let line_start = home_offset(&self.content, range.start);
            let text = match style {
                IndentStyle::Tabs => "\t".to_string(),
                IndentStyle::Spaces(_) => {
                    let width = style.width();
                    let mut col = 0;
                    for c in self.content[line_start..range.start].chars() {
                        col = if c == '\t' {
                            (col / TAB_DISPLAY_COLUMNS + 1) * TAB_DISPLAY_COLUMNS
                        } else {
                            col + 1
                        };
                    }
                    " ".repeat(width - col % width)
                }
            };
            self.edit(range, &text);
            self.refresh_cursor();
            return;
        }
        let (start_line, end_line) = self.indent_target_lines(&range);
        let unit = style.unit();
        let lines_str = self.content[start_line..end_line].to_string();
        let mut modified = Vec::new();
        let mut first_added = 0;
        for (i, line) in lines_str.split('\n').enumerate() {
            let blank = line.trim().is_empty();
            if !markdown && blank {
                modified.push(line.to_string());
                continue;
            }
            if i == 0 {
                first_added = unit.len();
            }
            modified.push(format!("{unit}{line}"));
        }
        let replacement = modified.join("\n");
        let added_len = replacement.len() - lines_str.len();
        self.edit(start_line..end_line, &replacement);
        if range.is_empty() {
            self.caret.move_to(range.start + first_added);
        } else {
            self.caret.range = (range.start + first_added)..(range.end + added_len);
        }
        self.snap_caret_to_visible_line_end();
        self.refresh_cursor();
    }

    pub fn outdent(&mut self) {
        if self.is_read_only() {
            return;
        }
        let range = self.selection_range();
        let (start_line, end_line) = self.indent_target_lines(&range);
        let strip_spaces = match self.indent_style {
            IndentStyle::Tabs => 4,
            IndentStyle::Spaces(n) => n.max(1),
        };

        let lines_str = self.content[start_line..end_line].to_string();
        let mut modified = Vec::new();
        let mut first_line_stripped = 0;
        // Indent removed from lines at or before the selection end, so the end
        // shifts only by what was removed in front of it.
        let mut removed_before_end = 0;
        let mut line_start_pos = start_line;
        for (i, line) in lines_str.split('\n').enumerate() {
            let strip = if line.starts_with('\t') {
                1
            } else {
                line.bytes()
                    .take(strip_spaces)
                    .take_while(|b| *b == b' ')
                    .count()
            };
            let stripped = &line[strip..];
            if i == 0 {
                first_line_stripped = strip;
            }
            removed_before_end += strip.min(range.end.saturating_sub(line_start_pos));
            line_start_pos += line.len() + 1;
            modified.push(stripped.to_string());
        }
        let replacement = modified.join("\n");
        self.edit(start_line..end_line, &replacement);
        if range.is_empty() {
            let new_cursor = range
                .start
                .saturating_sub(first_line_stripped)
                .max(start_line);
            self.caret.move_to(new_cursor);
        } else {
            let new_start = range
                .start
                .saturating_sub(first_line_stripped)
                .max(start_line);
            let new_end = range.end.saturating_sub(removed_before_end).max(new_start);
            let new_end = floor_boundary(&self.content, new_end).max(new_start);
            self.caret.range = new_start..new_end;
        }
        self.snap_caret_to_visible_line_end();
        self.refresh_cursor();
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn apply_format(&mut self, cmd: &str) {
        self.apply_format_in_mode(cmd, ViewMode::Raw);
    }

    pub fn apply_format_in_mode(&mut self, cmd: &str, mode: ViewMode) {
        if self.is_read_only() || !self.file_type.supports_markdown_features() {
            return;
        }
        // A format toggle is an undo boundary: pending span typing groups end
        // here so text typed after the toggle starts a new step.
        self.history.seal();

        // 1. Block-level formats: h1, h2, h3, ul, ol, checklist, quote
        let block_target = match cmd {
            "h1" => Some(BlockFormatTarget::Heading(1)),
            "h2" => Some(BlockFormatTarget::Heading(2)),
            "h3" => Some(BlockFormatTarget::Heading(3)),
            "h4" => Some(BlockFormatTarget::Heading(4)),
            "h5" => Some(BlockFormatTarget::Heading(5)),
            "h6" => Some(BlockFormatTarget::Heading(6)),
            "ul" => Some(BlockFormatTarget::BulletList),
            "ol" => Some(BlockFormatTarget::OrderedList),
            "checklist" => Some(BlockFormatTarget::TaskList),
            "quote" => Some(BlockFormatTarget::Blockquote),
            _ => None,
        };
        if let Some(target) = block_target {
            self.toggle_block_lines(target);
            return;
        }

        // 2. Code block toggle (Ctrl+Shift+C)
        if cmd == "codeblock" {
            self.toggle_code_block();
            return;
        }

        // 3. Inline styling & Link: bold, italic, underline, strike, code, link
        let inline_spec = match cmd {
            "bold" => Some((InlineTagKind::Bold, "**", "**", "bold text")),
            "italic" => Some((InlineTagKind::Italic, "*", "*", "italic text")),
            "underline" => Some((InlineTagKind::Underline, "<u>", "</u>", "underlined")),
            "strike" => Some((InlineTagKind::Strike, "~~", "~~", "strikethrough")),
            "code" => Some((InlineTagKind::Code, "`", "`", "code")),
            "link" => Some((InlineTagKind::Link, "[", "](https://)", "link")),
            _ => None,
        };

        if let Some((tag_kind, prefix, suffix, placeholder)) = inline_spec {
            let mut range = self.selection_range();
            let parsed = self.parsed();

            // If selection is empty, check if cursor is on/inside a word or enclosed by raw delimiters
            if range.is_empty() {
                let cursor = range.start;

                let word_start = super::text_ops::word_left(&self.content, cursor);
                let word_end = super::text_ops::word_right(&self.content, cursor);
                let line_s = home_offset(&self.content, cursor);
                let line_e = parsed
                    .line_at_offset(cursor)
                    .map(|line| visible_line_end(line, self.view_mode))
                    .unwrap_or_else(|| line_end(&self.content, cursor));
                let w_start = word_start.max(line_s);
                let w_end = word_end.min(line_e);
                let touching_word = cursor > w_start
                    && cursor < w_end
                    && self.content[..cursor]
                        .chars()
                        .next_back()
                        .is_some_and(|c| c.is_alphanumeric() || c == '_')
                    && self.content[cursor..]
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphanumeric() || c == '_');
                if touching_word && w_start < w_end {
                    let extent = super::text_ops::word_extent(&self.content, cursor);
                    if self.content[extent.clone()]
                        .chars()
                        .any(|c| c.is_alphanumeric() || c == '_')
                    {
                        range = extent;
                    }
                }

                if range.is_empty() {
                    // Check if cursor is strictly inside an existing InlinePair of `tag_kind`
                    if let Some(pair) = line_pair_refs(&parsed, range.start)
                        .iter()
                        .rev()
                        .find(|p| {
                            p.kind == tag_kind
                                && range.start > p.open_delim.start
                                && range.end < span_close_end(&parsed, p)
                        })
                        .cloned()
                    {
                        let inner_text = self.content[pair.inner.clone()].to_string();
                        let edit_range = pair.open_delim.start..pair.close_delim.end;
                        // If the inner text is just the default unmodified placeholder and the user immediately toggled off,
                        // remove the placeholder too so pressing Ctrl+B twice on an empty spot leaves it empty!
                        if inner_text == placeholder && range == pair.inner {
                            let pos = pair.open_delim.start;
                            self.edit(edit_range, "");
                            self.caret.move_to(pos);
                            self.pending_formats = ActiveFormats::default();
                            self.refresh_cursor();
                            return;
                        }
                        if mode == ViewMode::Wysiwyg && tag_kind != InlineTagKind::Link {
                            // Word-style exit: the span stays formatted and the
                            // caret moves past the whole closing run so further
                            // typing is plain in the toggled format. Other
                            // formats stacked around the caret stay armed for
                            // the next letter, and a second press of the same
                            // toggle then disarms them. Links unwrap instead.
                            let mut pending = pending_formats_at(&parsed, range.start);
                            match tag_kind {
                                InlineTagKind::Bold => pending.bold = false,
                                InlineTagKind::Italic => pending.italic = false,
                                InlineTagKind::Underline => pending.underline = false,
                                InlineTagKind::Strike => pending.strike = false,
                                InlineTagKind::Code => pending.code = false,
                                InlineTagKind::Link => {}
                            }
                            if range.start > pair.open_delim.start {
                                let group_end = span_close_end(&parsed, &pair);
                                self.caret.move_to(floor_boundary(&self.content, group_end));
                            }
                            self.pending_formats = pending;
                            self.refresh_cursor();
                            return;
                        }
                        let new_start = pair.open_delim.start;
                        self.edit(edit_range, &inner_text);
                        let rel = range
                            .start
                            .saturating_sub(pair.inner.start)
                            .min(inner_text.len());
                        self.caret.move_to(new_start + rel);
                        self.pending_formats = ActiveFormats::default();
                        self.refresh_cursor();
                        return;
                    }

                    // Check empty delimiters right around cursor, e.g. `**|**`
                    if outer_is_delimited(&self.content, cursor, cursor, prefix, suffix) {
                        let edit_range = (cursor - prefix.len())..(cursor + suffix.len());
                        let pos = cursor - prefix.len();
                        self.edit(edit_range, "");
                        self.caret.move_to(pos);
                        self.pending_formats = ActiveFormats::default();
                        self.refresh_cursor();
                        return;
                    }

                    if outer_is_delimited(&self.content, word_start, word_end, prefix, suffix) {
                        let core = self.content[word_start..word_end].to_string();
                        let edit_range = (word_start - prefix.len())..(word_end + suffix.len());
                        let new_start = word_start - prefix.len();
                        let new_end = new_start + core.len();
                        self.edit(edit_range, &core);
                        self.caret.range = new_start..new_end;
                        self.caret.reversed = false;
                        self.pending_formats = ActiveFormats::default();
                        self.refresh_cursor();
                        return;
                    }
                }
            }

            if range.is_empty() {
                // Caret on the opener: unwrap the span (Left to offset 0 of
                // `**Hello**` then Ctrl+B must drop the markers).
                if let Some(pair) = line_pair_refs(&parsed, range.start)
                    .iter()
                    .rev()
                    .find(|p| p.kind == tag_kind && range.start == p.open_delim.start)
                    .cloned()
                {
                    let inner_text = self.content[pair.inner.clone()].to_string();
                    let edit_range = pair.open_delim.start..pair.close_delim.end;
                    self.edit(edit_range, &inner_text);
                    self.caret.move_to(pair.open_delim.start);
                    self.pending_formats = ActiveFormats::default();
                    self.refresh_cursor();
                    return;
                }

                // In WYSIWYG mode, toggling bold/italic/underline/strike/code at an empty cursor toggles pending_formats!
                if mode == ViewMode::Wysiwyg && tag_kind != InlineTagKind::Link {
                    match tag_kind {
                        InlineTagKind::Bold => {
                            self.pending_formats.bold = !self.pending_formats.bold
                        }
                        InlineTagKind::Italic => {
                            self.pending_formats.italic = !self.pending_formats.italic
                        }
                        InlineTagKind::Underline => {
                            self.pending_formats.underline = !self.pending_formats.underline
                        }
                        InlineTagKind::Strike => {
                            self.pending_formats.strike = !self.pending_formats.strike
                        }
                        InlineTagKind::Code => {
                            self.pending_formats.code = !self.pending_formats.code
                        }
                        InlineTagKind::Link => {}
                    }
                    self.refresh_cursor();
                    return;
                }

                // In Raw mode (or for Link), insert placeholder and select inner placeholder so a second press unwraps/removes it
                self.edit(
                    range.clone(),
                    &format!("{}{}{}", prefix, placeholder, suffix),
                );
                let start = floor_boundary(&self.content, range.start + prefix.len());
                let end = floor_boundary(&self.content, start + placeholder.len());
                self.caret.range = start..end;
                self.caret.reversed = false;
                self.refresh_cursor();
                return;
            }

            // Non-empty selection: collect parts across table cells or lines separately
            let orig_cursor = if self.caret.range.is_empty() {
                Some(self.caret.range.start)
            } else {
                None
            };
            let orig_sel = range.clone();
            struct InlinePartInfo {
                range: Range<usize>,
                has_format: bool,
                edit_range: Range<usize>,
                unwrapped_text: String,
                wrapped_text: String,
                /// For a partial unformat, the new range of the selection's
                /// text after the surrounding span was split.
                split_inner: Option<Range<usize>>,
            }

            let mut candidate_ranges: Vec<Range<usize>> = Vec::new();

            for pline in parsed.lines_overlapping(range.clone()) {
                let line_start = range.start.max(pline.source.start);
                let line_end = range.end.min(pline.source.end);
                if line_start >= line_end {
                    continue;
                }
                match &pline.kind {
                    // Code block bodies show markers literally, so a select-all
                    // toggle leaves them alone like the fences around them.
                    LineKind::TableDelimiter
                    | LineKind::CodeFence { .. }
                    | LineKind::CodeLine
                    | LineKind::HorizontalRule => {
                        continue;
                    }
                    LineKind::TableRow { ref body, .. } => {
                        let TableRowBody {
                            cells, raw_cells, ..
                        } = &**body;
                        let cell_ranges = if raw_cells.is_empty() {
                            cells
                        } else {
                            raw_cells
                        };
                        for c_range in cell_ranges {
                            let c_start = range.start.max(c_range.start);
                            let c_end = range.end.min(c_range.end);
                            if c_start >= c_end {
                                continue;
                            }
                            let slice = &self.content[c_start..c_end];
                            let trimmed = slice.trim();
                            if trimmed.is_empty() {
                                continue;
                            }
                            let lead = slice.len() - slice.trim_start().len();
                            let trail = slice.len() - slice.trim_end().len();
                            let part_start = c_start + lead;
                            let part_end = c_end - trail;
                            candidate_ranges.push(part_start..part_end);
                        }
                    }
                    _ => {
                        let content_start = pline.prefix_range.end.max(pline.source.start);
                        // A heading's closing-hash run (`# Title #`) is hidden
                        // syntax; wrapping it would make the `#` visible text.
                        let content_end = match pline.kind {
                            LineKind::Heading { .. } => {
                                let text = self.content[pline.source.clone()]
                                    .trim_end_matches(['\n', '\r']);
                                crate::markdown::parser::atx_content_range(text)
                                    .map_or(pline.source.end, |c| pline.source.start + c.end)
                            }
                            _ => pline.source.end,
                        };
                        let l_start = range.start.max(content_start);
                        let l_end = range.end.min(content_end);
                        if l_start >= l_end {
                            continue;
                        }
                        let slice = &self.content[l_start..l_end];
                        let trimmed = slice.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        let lead = slice.len() - slice.trim_start().len();
                        let trail = slice.len() - slice.trim_end().len();
                        let part_start = l_start + lead;
                        let part_end = l_end - trail;
                        candidate_ranges.push(part_start..part_end);
                    }
                }
            }

            if candidate_ranges.is_empty() {
                return;
            }

            if tag_kind != InlineTagKind::Link {
                // Rewrite each line (or table cell) on its own. Inline pairs never
                // cross lines, and one span over several lines would put markers
                // before list, task and quote prefixes and across paragraphs.
                let rewrite_ranges: Vec<Range<usize>> = candidate_ranges.clone();
                let rewrite = |on: bool| -> Vec<Option<InlineRewrite>> {
                    rewrite_ranges
                        .iter()
                        .map(|p_range| {
                            let p_start = p_range.start;
                            let p_end = p_range.end;
                            let pline = match parsed.line_at_offset(p_start) {
                                Some(l) => l,
                                None => return None,
                            };

                            let mut group_pairs = collect_group_pairs(&parsed, p_start..p_end);

                            let (replace_start, replace_end) = if group_pairs.is_empty() {
                                (p_start, p_end)
                            } else {
                                let r_start = group_pairs
                                    .iter()
                                    .map(|p| p.open_delim.start)
                                    .min()
                                    .unwrap()
                                    .min(p_start);
                                let r_end = group_pairs
                                    .iter()
                                    .map(|p| span_close_end(&parsed, p))
                                    .max()
                                    .unwrap()
                                    .max(p_end);
                                (r_start, r_end)
                            };
                            let (replace_start, replace_end) = expand_replace_to_matching_neighbors(
                                &self.content,
                                &parsed,
                                pline,
                                p_start..p_end,
                                replace_start..replace_end,
                                tag_kind,
                                on,
                            );
                            // Neighbor expansion may pull in spans that did not overlap
                            // the selection (`aa` next to `**bb cc**`). Skip those
                            // delimiters so they are rewritten instead of kept as text.
                            add_contained_pairs(
                                &parsed,
                                replace_start..replace_end,
                                &mut group_pairs,
                            );

                            let simple_wrap = group_pairs.is_empty()
                                && replace_start == p_start
                                && replace_end == p_end;
                            let slices: Vec<StyledSlice> = if simple_wrap {
                                let mut flags = FormatFlags::default();
                                apply_toggle(&mut flags, tag_kind, on);
                                vec![StyledSlice {
                                    text: self.content[p_start..p_end].to_string(),
                                    flags,
                                    is_selected: true,
                                }]
                            } else {
                                let skip: Vec<Range<usize>> = group_pairs
                                    .iter()
                                    .flat_map(|p| [p.open_delim.clone(), p.close_delim.clone()])
                                    .collect();
                                let segs: Vec<&StyledSegment> = parsed
                                    .lines_overlapping(replace_start..replace_end)
                                    .iter()
                                    .flat_map(|l| l.styled_segments.iter())
                                    .collect();
                                let local_pairs =
                                    nearby_pair_refs(&parsed, replace_start..replace_end);
                                collect_rewrite_slices(
                                    &self.content,
                                    &local_pairs,
                                    &segs,
                                    replace_start..replace_end,
                                    p_start..p_end,
                                    &skip,
                                    Some((tag_kind, on)),
                                )
                            };
                            let glued_left = replace_start > 0
                                && self.content[..replace_start]
                                    .chars()
                                    .next_back()
                                    .is_some_and(|c| !c.is_whitespace());
                            // Each line is rewritten on its own, so ask the whole
                            // selection whether it crosses a line break.
                            let include_wraps = self.content[orig_sel.clone()].contains('\n');
                            let (rendered_str, sel_in_rendered) =
                                render_styled_slices_with_selection_opts(
                                    &slices,
                                    glued_left,
                                    include_wraps,
                                );
                            Some((
                                replace_start..replace_end,
                                rendered_str,
                                sel_in_rendered,
                                p_start,
                            ))
                        })
                        .collect()
                };

                // Lines the parser cannot show in this format (a `~~~` fence it
                // reads as text, an indented line that drops `<u>`) are left
                // alone. Writing markers there would not render, and the next
                // press would wrap them again instead of toggling off.
                let trial_on = rewrite(true);
                let can_format =
                    verify_inline_rewrites(&self.content, &parsed, &trial_on, tag_kind, true);
                if !can_format.iter().any(|ok| *ok) {
                    return;
                }

                // Whitespace between two spans (`**a** **b**`) is unformatted
                // but invisible, so it must not make a second press re-apply
                // the format instead of removing it.
                let global_all_have_format = rewrite_ranges
                    .iter()
                    .zip(&can_format)
                    .filter(|(_, ok)| **ok)
                    .all(|(p_range, _)| {
                        let Some(pline) = parsed.line_at_offset(p_range.start) else {
                            return false;
                        };
                        let mut segs = pline.styled_segments.iter().filter(|s| {
                            s.source.start < p_range.end
                                && s.source.end > p_range.start
                                && !self.content[s.source.clone()].trim().is_empty()
                        });
                        let mut any = false;
                        segs.all(|s| {
                            any = true;
                            segment_flags(s).get(tag_kind)
                        }) && any
                    });
                let should_turn_on = self.force_inline_toggle.unwrap_or(!global_all_have_format);

                let (results, ok) = if should_turn_on {
                    (trial_on, can_format)
                } else {
                    let off = rewrite(false);
                    let ok = verify_inline_rewrites(&self.content, &parsed, &off, tag_kind, false);
                    (off, ok)
                };
                let mut candidate_results: Vec<InlineRewrite> = results
                    .into_iter()
                    .zip(ok)
                    .filter_map(|(result, ok)| if ok { result } else { None })
                    .collect();

                if rewrite_ranges.len() == 1 && !candidate_results.is_empty() {
                    let (rep_range, rendered_str, sel_in_rendered, p_start) = candidate_results.remove(0);
                    self.edit(rep_range.clone(), &rendered_str);

                    if let Some(cur) = orig_cursor {
                        let cursor_in_sel = cur.saturating_sub(p_start);
                        let target_pos = rep_range.start + sel_in_rendered.start + cursor_in_sel.min(sel_in_rendered.len());
                        self.caret.move_to(floor_boundary(&self.content, target_pos));
                    } else {
                        let new_start = rep_range.start + sel_in_rendered.start;
                        let new_end = rep_range.start + sel_in_rendered.end;
                        self.caret.range = new_start..new_end;
                        self.caret.reversed = false;
                    }
                    self.pending_formats = ActiveFormats::default();
                    self.refresh_cursor();
                    return;
                }

                let edits: Vec<(Range<usize>, String, Range<usize>)> = candidate_results
                    .into_iter()
                    .filter(|(r, repl, _, _)| self.content[r.clone()] != *repl)
                    .map(|(r, repl, sel, _)| (r, repl, sel))
                    .collect();

                if edits.is_empty() {
                    return;
                }

                let min_start = edits.first().unwrap().0.start;
                let max_end = edits.last().unwrap().0.end;
                let span_range = min_start..max_end;

                let mut replacement = String::with_capacity(
                    max_end - min_start + edits.len() * (prefix.len() + suffix.len()),
                );
                let mut cursor = min_start;
                for (r, repl, _) in &edits {
                    if r.start > cursor {
                        replacement.push_str(&self.content[cursor..r.start]);
                    }
                    replacement.push_str(repl);
                    cursor = r.end;
                }
                if max_end > cursor {
                    replacement.push_str(&self.content[cursor..max_end]);
                }

                self.edit(span_range, &replacement);

                // A selection edge inside a rewritten span lands on the selected
                // text's edge in the replacement, not at the end of the span.
                let map_offset = |offset: usize, is_start: bool| -> usize {
                    if offset < min_start {
                        return offset;
                    }
                    let mut mapped = min_start;
                    let mut orig_cursor = min_start;
                    for (r, repl, sel) in &edits {
                        if offset < r.start {
                            mapped += offset - orig_cursor;
                            return mapped;
                        }
                        mapped += r.start - orig_cursor;
                        if offset <= r.end {
                            mapped += if is_start { sel.start } else { sel.end };
                            return mapped;
                        }
                        mapped += repl.len();
                        orig_cursor = r.end;
                    }
                    if offset > orig_cursor {
                        mapped += offset - orig_cursor;
                    }
                    mapped
                };
                let new_start = map_offset(orig_sel.start, true);
                let new_end = map_offset(orig_sel.end, false);
                let new_start = floor_boundary(&self.content, new_start);
                let new_end = floor_boundary(&self.content, new_end);
                self.caret.range = new_start..new_end;
                self.caret.reversed = false;
                self.pending_formats = ActiveFormats::default();
                self.refresh_cursor();
                return;
            }

            let mut parts: Vec<InlinePartInfo> = Vec::with_capacity(candidate_ranges.len());

            for p_range in candidate_ranges {
                let p_start = p_range.start;
                let p_end = p_range.end;
                let part_text = &self.content[p_start..p_end];

                let (has_format, edit_range, unwrapped_text, split_inner) = if tag_kind
                    == InlineTagKind::Link
                {
                    if let Some(inner) = is_single_link(part_text) {
                        (true, p_start..p_end, inner.to_string(), None)
                    } else if let Some(pair) = line_pair_refs(&parsed, p_start).iter().find(|p| {
                        p.kind == InlineTagKind::Link
                            && (p.inner == (p_start..p_end)
                                || (p.open_delim.start == p_start && p.close_delim.end == p_end))
                    }) {
                        (
                            true,
                            pair.open_delim.start..pair.close_delim.end,
                            self.content[pair.inner.clone()].to_string(),
                            None,
                        )
                    } else if let Some((outer_range, inner)) =
                        unwrap_outer_link(&self.content, p_start, p_end)
                    {
                        (true, outer_range, inner, None)
                    } else {
                        (false, p_start..p_end, String::new(), None)
                    }
                } else {
                    (false, p_start..p_end, String::new(), None)
                };

                let wrapped_text = format!("{}{}{}", prefix, part_text, suffix);
                parts.push(InlinePartInfo {
                    range: p_range,
                    has_format,
                    edit_range,
                    unwrapped_text,
                    wrapped_text,
                    split_inner,
                });
            }

            let every_part_has_format = parts.iter().all(|p| p.has_format);
            let edits: Vec<(Range<usize>, String)> = if every_part_has_format {
                parts
                    .iter()
                    .map(|p| (p.edit_range.clone(), p.unwrapped_text.clone()))
                    .collect()
            } else {
                parts
                    .iter()
                    .filter(|p| !p.has_format)
                    .map(|p| (p.edit_range.clone(), p.wrapped_text.clone()))
                    .collect()
            };

            if edits.is_empty() {
                return;
            }

            let min_start = edits.first().unwrap().0.start;
            let max_end = edits.last().unwrap().0.end;
            let span_range = min_start..max_end;

            let mut replacement = String::with_capacity(
                max_end - min_start + edits.len() * (prefix.len() + suffix.len()),
            );
            let mut cursor = min_start;
            for (r, repl) in &edits {
                if r.start > cursor {
                    replacement.push_str(&self.content[cursor..r.start]);
                }
                replacement.push_str(repl);
                cursor = r.end;
            }
            if max_end > cursor {
                replacement.push_str(&self.content[cursor..max_end]);
            }

            self.edit(span_range, &replacement);

            if parts.len() == 1
                && (orig_sel == (parts[0].range.start..parts[0].range.end)
                    || orig_sel == (parts[0].edit_range.start..parts[0].edit_range.end))
            {
                if let (Some(cur), true) = (orig_cursor, every_part_has_format) {
                    let unwrapped_len = edits[0].1.len();
                    let rel = cur
                        .saturating_sub(parts[0].range.start)
                        .min(unwrapped_len);
                    self.caret.move_to(min_start + rel);
                    self.caret.reversed = false;
                    self.pending_formats = ActiveFormats::default();
                    self.refresh_cursor();
                    return;
                }
                if let Some(split) = parts[0].split_inner.clone() {
                    self.caret.range = split;
                } else if every_part_has_format {
                    let unwrapped_len = edits[0].1.len();
                    self.caret.range = min_start..min_start + unwrapped_len;
                } else {
                    let inner_start = min_start + prefix.len();
                    let inner_end = inner_start + (parts[0].range.end - parts[0].range.start);
                    self.caret.range = inner_start..inner_end;
                }
                self.caret.reversed = false;
            } else {
                let map_offset = |offset: usize| -> usize {
                    if offset <= min_start {
                        return offset;
                    }
                    let mut mapped = min_start;
                    let mut orig_cursor = min_start;
                    for (r, repl) in &edits {
                        if offset <= r.start {
                            mapped += offset - orig_cursor;
                            return mapped;
                        }
                        mapped += r.start - orig_cursor;
                        if offset <= r.end {
                            mapped += repl.len();
                            return mapped;
                        }
                        mapped += repl.len();
                        orig_cursor = r.end;
                    }
                    if offset > orig_cursor {
                        mapped += offset - orig_cursor;
                    }
                    mapped
                };
                let new_start = map_offset(orig_sel.start);
                let new_end = map_offset(orig_sel.end);
                let new_start = floor_boundary(&self.content, new_start);
                let new_end = floor_boundary(&self.content, new_end).max(new_start);
                self.caret.range = new_start..new_end;
                self.caret.reversed = false;
            }
            self.pending_formats = ActiveFormats::default();
            self.refresh_cursor();
            return;
        }

        if cmd == "table" {
            let range = self.selection_range();
            let parsed = self.parsed();

            // Check if cursor is currently inside an existing table
            let in_table = parsed.lines.iter().find(|l| {
                if let LineKind::TableRow { ref body, .. } = l.kind {
                    let TableRowBody { table_source, .. } = &**body;
                    range.start >= table_source.start && range.start <= table_source.end
                } else {
                    false
                }
            });

            if let Some(table_line) = in_table {
                if let LineKind::TableRow { ref body, .. } = table_line.kind {
                    let TableRowBody {
                        cells,
                        table_source,
                        ..
                    } = &**body;
                    let col_count = cells.len().max(2);
                    let row_str = format!("\n|{}", "  |".repeat(col_count));
                    let insert_pos = table_source.end;
                    self.edit(insert_pos..insert_pos, &row_str);
                    let target_pos = (insert_pos + 3).min(self.content.len());
                    self.caret
                        .move_to(floor_boundary(&self.content, target_pos));
                    self.refresh_cursor();
                    return;
                }
            }

            // Outside table: insert clean standard 2-column table template
            let mut prefix = String::new();
            if range.start > 0 {
                let prev_char = self.content[..range.start].chars().next_back();
                if let Some(c) = prev_char {
                    if c != '\n' {
                        prefix.push_str("\n\n");
                    } else if !self.content[..range.start].ends_with("\n\n") {
                        prefix.push('\n');
                    }
                }
            }
            let mut suffix = String::new();
            if range.end < self.content.len() {
                let next_char = self.content[range.end..].chars().next();
                if let Some(c) = next_char {
                    if c != '\n' {
                        suffix.push_str("\n\n");
                    } else if !self.content[range.end..].starts_with("\n\n") {
                        suffix.push('\n');
                    }
                }
            }
            let table_template = "| Column 1 | Column 2 |\n| :--- | :--- |\n| Item 1 | Item 2 |";
            let full_insert = format!("{}{}{}", prefix, table_template, suffix);
            self.edit(range.clone(), &full_insert);
            let col1_start = range.start + prefix.len() + 2;
            let col1_end = col1_start + "Column 1".len();
            self.caret.range = col1_start..col1_end;
            self.caret.reversed = false;
            self.refresh_cursor();
            return;
        }

        if cmd == "clear" {
            self.clear_inline_formats(mode);
            return;
        }

        // 4. Fallback for clear, hr, table
        let range = self.selection_range();
        let selected = self.content[range.clone()].to_string();
        let flags = line_start_flags(&self.content, range.start, &selected);
        let Some(replacement) = format_replacement(cmd, &selected, &flags) else {
            return;
        };
        let cursor_after_placeholder = cmd_selects_placeholder(cmd) && selected.is_empty();
        let prefix_len = placeholder_prefix_len(cmd);
        self.edit(range.clone(), &replacement);
        if cursor_after_placeholder {
            let start = floor_boundary(&self.content, range.start + prefix_len);
            let end = floor_boundary(&self.content, start + placeholder_len(cmd));
            self.caret.range = start..end;
            self.caret.reversed = false;
            self.refresh_cursor();
        }
    }

    /// True when any text in `range` carries the inline format `kind`.
    fn range_has_format(&mut self, range: &Range<usize>, kind: InlineTagKind) -> bool {
        let parsed = self.parsed();
        parsed.lines_overlapping(range.clone()).iter().any(|line| {
            line.styled_segments.iter().any(|seg| {
                seg.source.start < range.end
                    && seg.source.end > range.start
                    && match kind {
                        InlineTagKind::Bold => seg.bold,
                        InlineTagKind::Italic => seg.italic,
                        InlineTagKind::Underline => seg.underline,
                        InlineTagKind::Strike => seg.strike,
                        InlineTagKind::Code => seg.code,
                        InlineTagKind::Link => false,
                    }
            })
        })
    }

    /// Clear Formatting: removes bold, italic, underline, strikethrough and
    /// code from the selection (or the word at the caret), splitting spans
    /// that stick out of it. The work happens on a scratch copy and lands as
    /// one edit, so a single undo restores everything.
    fn clear_inline_formats(&mut self, mode: ViewMode) {
        let mut range = self.selection_range();
        if range.is_empty() {
            let extent = super::text_ops::word_extent(&self.content, range.start);
            if extent.is_empty()
                || !self.content[extent.clone()]
                    .chars()
                    .any(|c| c.is_alphanumeric() || c == '_')
            {
                return;
            }
            range = extent;
        }
        let mut tmp = DocumentTab::new(
            "clear".into(),
            "clear.md".into(),
            self.content.clone(),
            None,
            None,
        );
        tmp.view_mode = mode;
        tmp.force_inline_toggle = Some(false);
        tmp.caret.range = range;

        let kinds = [
            (InlineTagKind::Bold, "bold"),
            (InlineTagKind::Italic, "italic"),
            (InlineTagKind::Underline, "underline"),
            (InlineTagKind::Strike, "strike"),
            (InlineTagKind::Code, "code"),
        ];
        for (kind, cmd) in kinds {
            let sel = tmp.selection_range();
            if tmp.range_has_format(&sel, kind) {
                tmp.apply_format_in_mode(cmd, mode);
            }
        }

        // Links and any markers wholly inside the selection.
        let sel = tmp.selection_range();
        let selected = tmp.content[sel.clone()].to_string();
        let flags = line_start_flags(&tmp.content, sel.start, &selected);
        if let Some(replacement) = format_replacement("clear", &selected, &flags) {
            if replacement != selected {
                tmp.edit(sel.clone(), &replacement);
                tmp.caret.range = sel.start..sel.start + replacement.len();
            }
        }

        let (old, new) = (&self.content, &tmp.content);
        if old == new {
            return;
        }
        let mut prefix = old
            .bytes()
            .zip(new.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        while !old.is_char_boundary(prefix) || !new.is_char_boundary(prefix) {
            prefix -= 1;
        }
        let max_suffix = old.len().min(new.len()) - prefix;
        let mut suffix = old
            .bytes()
            .rev()
            .zip(new.bytes().rev())
            .take(max_suffix)
            .take_while(|(a, b)| a == b)
            .count();
        while !old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix)
        {
            suffix -= 1;
        }
        let replacement = new[prefix..new.len() - suffix].to_string();
        let target = tmp.selection_range();
        self.edit(prefix..old.len() - suffix, &replacement);
        let end = floor_boundary(&self.content, target.end);
        let start = floor_boundary(&self.content, target.start).min(end);
        self.caret.range = start..end;
        self.caret.reversed = false;
        self.pending_formats = ActiveFormats::default();
        self.refresh_cursor();
    }

    fn toggle_block_lines(&mut self, target: BlockFormatTarget) {
        let range = self.selection_range();
        let start_line = home_offset(&self.content, range.start);
        let end_line = line_end(&self.content, range.end);
        let lines_owned = self.content[start_line..end_line].to_string();
        let split_lines: Vec<&str> = lines_owned.split('\n').collect();

        // Check if all non-empty lines (or the single line) already match `target`.
        // An ordered task is a checklist too: the button is on, and turning it
        // off must keep the ordered marker instead of writing `- [ ] `.
        let mut line_at = Vec::with_capacity(split_lines.len());
        let mut abs = start_line;
        for (idx, line) in split_lines.iter().enumerate() {
            line_at.push(abs);
            abs += line.len();
            if idx + 1 < split_lines.len() {
                abs += 1;
            }
        }

        let parsed = self.parsed();
        let is_table_line = |at: usize| -> bool {
            parsed.line_at_offset(at).is_some_and(|l| {
                matches!(l.kind, LineKind::TableRow { .. } | LineKind::TableDelimiter)
            })
        };

        let non_table_indices: Vec<usize> = (0..split_lines.len())
            .filter(|&i| !is_table_line(line_at[i]))
            .collect();

        if non_table_indices.is_empty() {
            // All selected lines are table lines: skip so the table never breaks!
            return;
        }

        let all_match = non_table_indices
            .iter()
            .copied()
            .filter(|&idx| !split_lines[idx].trim().is_empty() || non_table_indices.len() == 1)
            .all(|idx| {
                let at = line_at[idx];
                if target == BlockFormatTarget::TaskList {
                    accepted_list_marker_in(&parsed, &self.content, at)
                        .is_some_and(|item| item.task.is_some())
                } else {
                    let (_, existing, _) = parse_line_block_prefix(&parsed, &self.content, at);
                    existing == Some(target)
                }
            });

        let mut transformed = Vec::with_capacity(split_lines.len());
        let mut first_delta: isize = 0;
        let mut total_delta: isize = 0;

        for (idx, line) in split_lines.iter().enumerate() {
            if is_table_line(line_at[idx]) {
                transformed.push(line.to_string());
                continue;
            }
            if line.trim().is_empty() && split_lines.len() > 1 {
                transformed.push(line.to_string());
                continue;
            }
            let (indent, _existing, prefix_end) =
                parse_line_block_prefix(&parsed, &self.content, line_at[idx]);
            let leading_spaces = if matches!(target, BlockFormatTarget::Heading(_)) {
                &line[..indent.min(3)]
            } else {
                &line[..indent]
            };
            let raw_body = &line[prefix_end..];
            let has_hidden_closer =
                parsed
                    .line_at_offset(line_at[idx])
                    .is_some_and(|parsed_line| {
                        visible_line_end(parsed_line, self.view_mode) < parsed_line.source.end
                    });
            let strip_closer = has_hidden_closer
                && (all_match || !matches!(target, BlockFormatTarget::Heading(_)));
            let body = if strip_closer {
                crate::markdown::parser::atx_content_range(line)
                    .map(|content| &line[content])
                    .unwrap_or(raw_body)
            } else {
                raw_body
            };
            let new_line = if let Some(rewritten) =
                ordered_checklist_line(&parsed, &self.content, line_at[idx], target, all_match)
            {
                rewritten
            } else if all_match {
                format!("{}{}", leading_spaces, body)
            } else {
                let new_prefix = match target {
                    BlockFormatTarget::Heading(1) => "# ".to_string(),
                    BlockFormatTarget::Heading(2) => "## ".to_string(),
                    BlockFormatTarget::Heading(3) => "### ".to_string(),
                    BlockFormatTarget::Heading(4) => "#### ".to_string(),
                    BlockFormatTarget::Heading(5) => "##### ".to_string(),
                    BlockFormatTarget::Heading(_) => "###### ".to_string(),
                    BlockFormatTarget::BulletList => "- ".to_string(),
                    BlockFormatTarget::OrderedList => format!("{}. ", idx + 1),
                    BlockFormatTarget::TaskList => "- [ ] ".to_string(),
                    BlockFormatTarget::Blockquote => "> ".to_string(),
                };
                format!("{}{}{}", leading_spaces, new_prefix, body)
            };
            let delta = new_line.len() as isize - line.len() as isize;
            if idx == 0 {
                first_delta = delta;
            }
            total_delta += delta;
            transformed.push(new_line);
        }

        let replacement = transformed.join("\n");
        self.edit(start_line..end_line, &replacement);
        if range.is_empty() {
            let new_cursor = (range.start as isize + first_delta).max(start_line as isize) as usize;
            self.caret
                .move_to(floor_boundary(&self.content, new_cursor));
        } else {
            let new_start = (range.start as isize + first_delta).max(start_line as isize) as usize;
            let new_end = (range.end as isize + total_delta).max(new_start as isize) as usize;
            self.caret.range =
                floor_boundary(&self.content, new_start)..floor_boundary(&self.content, new_end);
            self.caret.reversed = false;
        }
        self.refresh_cursor();
    }

    fn toggle_code_block(&mut self) {
        let range = self.selection_range();
        let parsed = self.parsed();

        // Check if selection/cursor is inside a fenced code block
        if let Some((open_fence, close_fence)) = parsed
            .code_blocks
            .iter()
            .find(|(open_r, close_r)| range.start >= open_r.start && range.end <= close_r.end)
            .cloned()
        {
            let inner_start = open_fence.end.min(close_fence.start);
            let inner_end = close_fence.start;
            let mut inner = self.content[inner_start..inner_end].to_string();
            // Drop the newline that separated the body from the closer. An
            // unclosed fence has an empty close range, and the document's own
            // trailing newline is added back below, so the same pop applies.
            if inner.ends_with('\n') {
                inner.pop();
            }
            let full_end = close_fence.end;
            let trailing_nl =
                if full_end <= self.content.len() && self.content[..full_end].ends_with('\n') {
                    "\n"
                } else {
                    ""
                };
            let replacement = format!("{}{}", inner, trailing_nl);
            let unwrapped_len = inner.len();
            self.edit(open_fence.start..full_end, &replacement);
            self.caret.range = floor_boundary(&self.content, open_fence.start)
                ..floor_boundary(&self.content, open_fence.start + unwrapped_len);
            self.caret.reversed = false;
            self.refresh_cursor();
            return;
        }

        // Indented code: turn the whole block into a backtick fence and drop
        // the four-column indent that made it code, so it is not kept as
        // text inside the fence.
        if let Some(lines) = indented_code_block_at(&parsed, range.start) {
            let first = lines.first().expect("block has a line");
            let last = lines.last().expect("block has a line");
            let block = first.source.start..last.source.end;
            let mut body = String::with_capacity(block.len());
            for (index, line) in lines.iter().enumerate() {
                if matches!(line.kind, LineKind::CodeLine) {
                    body.push_str(&self.content[line.prefix_range.end..line.source.end]);
                }
                if let Some(next) = lines.get(index + 1) {
                    body.push_str(&self.content[line.source.end..next.source.start]);
                }
            }
            // A fence longer than any backtick run in the code, so a ```
            // line inside cannot close it early (CommonMark example 123).
            let longest_run = body.split(|ch| ch != '`').map(str::len).max().unwrap_or(0);
            let fence = "`".repeat(longest_run.max(2) + 1);
            let nl = self.line_break(block.start);
            let replacement = format!("{fence}{nl}{body}{nl}{fence}");
            self.edit(block.clone(), &replacement);
            let inner = block.start + fence.len() + nl.len();
            self.caret.range = floor_boundary(&self.content, inner)
                ..floor_boundary(&self.content, inner + body.len());
            self.caret.reversed = false;
            self.refresh_cursor();
            return;
        }

        // Also check if `selected` itself starts and ends with ```
        let selected = self.content[range.clone()].to_string();
        if selected.starts_with("```\n") && selected.ends_with("\n```") && selected.len() >= 8 {
            let unwrapped = selected[4..selected.len() - 4].to_string();
            let start = range.start;
            let end = start + unwrapped.len();
            self.edit(range, &unwrapped);
            self.caret.range = start..end;
            self.caret.reversed = false;
            self.refresh_cursor();
            return;
        }

        let is_table_line = |at: usize| -> bool {
            parsed.line_at_offset(at).is_some_and(|l| {
                matches!(l.kind, LineKind::TableRow { .. } | LineKind::TableDelimiter)
            })
        };

        if selected.is_empty() {
            if is_table_line(range.start) {
                // Inside a table: skip so table never breaks!
                return;
            }
            let line_s = home_offset(&self.content, range.start);
            let line_e = line_end(&self.content, range.start);
            let line_text = self.content[line_s..line_e].to_string();
            if line_text.is_empty() {
                self.edit(range.clone(), "```\ncode\n```");
                self.caret.range = (range.start + 4)..(range.start + 8);
                self.caret.reversed = false;
            } else {
                let wrapped = format!("```\n{}\n```", line_text);
                self.edit(line_s..line_e, &wrapped);
                self.caret.range = (line_s + 4)..(line_s + 4 + line_text.len());
                self.caret.reversed = false;
            }
            self.refresh_cursor();
        } else {
            let start_line = home_offset(&self.content, range.start);
            let end_line = line_end(&self.content, range.end);
            let lines_owned = self.content[start_line..end_line].to_string();
            let split_lines: Vec<&str> = lines_owned.split('\n').collect();

            let mut line_at = Vec::with_capacity(split_lines.len());
            let mut abs = start_line;
            for (idx, line) in split_lines.iter().enumerate() {
                line_at.push(abs);
                abs += line.len();
                if idx + 1 < split_lines.len() {
                    abs += 1;
                }
            }

            let non_table_count = (0..split_lines.len())
                .filter(|&i| !is_table_line(line_at[i]))
                .count();
            if non_table_count == 0 {
                // All selected lines are table lines: skip so table never breaks!
                return;
            }

            let has_table_lines = non_table_count < split_lines.len();
            if !has_table_lines {
                // Pure non-table selection: wrap the selection directly
                let wrapped = format!("```\n{}\n```", selected);
                let inner_start = range.start + 4;
                let inner_end = inner_start + selected.len();
                self.edit(range, &wrapped);
                self.caret.range = inner_start..inner_end;
                self.caret.reversed = false;
                self.refresh_cursor();
            } else {
                // Mixed selection: wrap non-table chunks, keep table lines intact
                let mut transformed = Vec::new();
                let mut chunk: Vec<&str> = Vec::new();
                for (i, line) in split_lines.iter().enumerate() {
                    if is_table_line(line_at[i]) {
                        if !chunk.is_empty() {
                            transformed.push(format!("```\n{}\n```", chunk.join("\n")));
                            chunk.clear();
                        }
                        transformed.push(line.to_string());
                    } else {
                        chunk.push(line);
                    }
                }
                if !chunk.is_empty() {
                    transformed.push(format!("```\n{}\n```", chunk.join("\n")));
                }
                let replacement = transformed.join("\n");
                let new_len = replacement.len();
                self.edit(start_line..end_line, &replacement);
                self.caret.range = start_line..(start_line + new_len);
                self.caret.reversed = false;
                self.refresh_cursor();
            }
        }
    }

    pub fn has_pending_inline_formats(&self) -> bool {
        let pf = self.pending_formats;
        pf.bold || pf.italic || pf.underline || pf.strike || pf.code
    }

    pub fn insert_text_in_mode(&mut self, range: Range<usize>, text: &str, mode: ViewMode) {
        if self.is_read_only() {
            return;
        }
        if mode == ViewMode::Wysiwyg
            && !range.is_empty()
            && !text.is_empty()
            && text != "\n"
            && self.replace_wysiwyg_selection(range.clone(), text)
        {
            return;
        }
        if text.trim().is_empty() {
            if mode == ViewMode::Wysiwyg && range.is_empty() && !text.is_empty() {
                let parsed = self.parsed();
                // A Markdown closer cannot sit right after whitespace, so whitespace
                // typed at the end of a `*`/`**`/`~~` span goes after the whole
                // closing run; the span's formats stay armed for what is typed
                // next. Extra whitespace keeps stacking outside the closer.
                let line_pairs = line_pair_refs(&parsed, range.start);
                let pair = line_pairs
                    .iter()
                    .rev()
                    .find(|p| {
                        matches!(
                            p.kind,
                            InlineTagKind::Bold
                                | InlineTagKind::Italic
                                | InlineTagKind::Strike
                                | InlineTagKind::Underline
                                | InlineTagKind::Code
                        ) && range.start == p.close_delim.start
                            && !line_pairs.iter().any(|o| {
                                o.kind != InlineTagKind::Link
                                    && o.inner.start <= p.inner.start
                                    && o.inner.end >= p.inner.end
                                    && o.inner != p.inner
                            })
                    })
                    .cloned();
                if let Some(pair) = pair {
                    let pending = if self.has_pending_inline_formats() {
                        self.pending_formats
                    } else {
                        pending_formats_at(&parsed, range.start)
                    };
                    let at = span_close_end(&parsed, &pair);
                    self.edit(at..at, text);
                    self.pending_formats = pending;
                    self.caret.move_to(floor_boundary(&self.content, at + text.len()));
                    self.refresh_cursor();
                    return;
                }
            }
            let pf = self.pending_formats;
            self.edit(range, text);
            self.pending_formats = pf;
            return;
        }
        if mode == ViewMode::Wysiwyg
            && range.is_empty()
            && !text.is_empty()
            && text != "\n"
        {
            let pf = self.pending_formats;
            if self.has_pending_inline_formats() {
                let parsed = self.parsed();
                // The armed formats extend a span ending right at the caret
                // (with or without a trailing space) or continue a subset of its formats.
                if let Some((run_range, repl, caret)) =
                    extend_or_splice_span(&self.content, &parsed, &pf, range.start, text)
                {
                    self.pending_formats = ActiveFormats::default();
                    self.edit(run_range, &repl);
                    self.caret.move_to(floor_boundary(&self.content, caret));
                    self.refresh_cursor();
                    return;
                }
            }
            if pf.bold || pf.italic || pf.underline || pf.strike || pf.code {
                let flags = FormatFlags {
                    bold: pf.bold,
                    italic: pf.italic,
                    underline: pf.underline,
                    strike: pf.strike,
                    code: pf.code,
                };
                let lead_len = text.len() - text.trim_start().len();
                let trail_len = text.len() - text.trim_end().len();
                let lead_ws = &text[..lead_len];
                let trail_ws = &text[text.len() - trail_len..];
                let core = &text[lead_len..text.len() - trail_len];
                // The span's own delimiter characters inside the typed text
                // would end the span early; escape them so it stays intact.
                let core_owned;
                let core = if pf.strike && core.contains('~') {
                    core_owned = core.replace('~', "\\~");
                    &core_owned
                } else if (pf.bold || pf.italic) && core.contains('*') {
                    core_owned = core.replace('*', "\\*");
                    &core_owned
                } else {
                    core
                };

                let (open, close) = wrap_delimiters(&flags);
                let wrapped = format!("{}{}{}{}{}", lead_ws, open, core, close, trail_ws);
                let close_start = wrapped.len() - trail_ws.len() - close.len();
                let cursor_after = if trail_len > 0 {
                    range.start + wrapped.len()
                } else {
                    range.start + close_start
                };
                if trail_len == 0 {
                    self.pending_formats = ActiveFormats::default();
                }
                self.next_edit_opens_span_group = true;
                self.edit(range, &wrapped);
                self.caret
                    .move_to(floor_boundary(&self.content, cursor_after));
                self.refresh_cursor();
                return;
            }
        }
        // Typing a span's own delimiter characters inside it would end the
        // span early: escape them so the span stays intact. Backticks cannot
        // be escaped inside a code span, so the span is re-fenced with a
        // longer backtick run instead.
        let mut escaped: Option<String> = None;
        if mode == ViewMode::Wysiwyg && range.is_empty() && !text.is_empty() {
            let parsed = self.parsed();
            if let Some(pair) = line_pair_refs(&parsed, range.start).iter().rev().find(|p| {
                range.start >= p.inner.start && range.start <= p.inner.end
            }) {
                match pair.kind {
                    InlineTagKind::Bold | InlineTagKind::Italic => {
                        if text.contains('*') {
                            escaped = Some(text.replace('*', "\\*"));
                        }
                    }
                    InlineTagKind::Strike => {
                        if text.contains('~') {
                            escaped = Some(text.replace('~', "\\~"));
                        }
                    }
                    InlineTagKind::Code if text.contains('`') => {
                        // The backtick is literal content: re-fence the span
                        // with a longer run so it stays intact.
                        self.refence_code_span(pair, range.start, text);
                        return;
                    }
                    _ => {}
                }
            }
        }
        let text = escaped.as_deref().unwrap_or(text);

        // A literally typed `<` must not start an HTML tag (`<u>a<b>c</u>`);
        // escape it so it renders as text. Skipped inside code spans, whose
        // content is literal.
        if mode == ViewMode::Wysiwyg && range.is_empty() && text.chars().count() == 1 {
            let parsed = self.parsed();
            let in_code = line_pair_refs(&parsed, range.start).iter().any(|p| {
                p.kind == InlineTagKind::Code
                    && range.start > p.inner.start
                    && range.start < p.inner.end
            });
            if !in_code {
                let next_char = self.content[range.start..].chars().next();
                let starts_tag =
                    next_char.is_some_and(|c| c.is_ascii_alphabetic() || c == '/');
                let prev_is_lt = range.start > 0
                    && self.content[..range.start].ends_with('<')
                    && !self.content[..range.start].ends_with("\\<");
                let typed_tag_cont = text
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '/');
                if text == "<" && starts_tag {
                    let escaped = format!("\\{text}");
                    let start = range.start;
                    self.pending_formats = ActiveFormats::default();
                    self.edit(start..start, &escaped);
                    self.caret
                        .move_to(floor_boundary(&self.content, start + escaped.len()));
                    self.refresh_cursor();
                    return;
                }
                if typed_tag_cont && prev_is_lt {
                    // The `<` right before the caret becomes `\<` and the
                    // typed character follows it, in one edit.
                    let lt = range.start - 1;
                    let repl = format!("\\<{text}");
                    let caret = lt + repl.len();
                    self.pending_formats = ActiveFormats::default();
                    self.edit(lt..range.start, &repl);
                    self.caret.move_to(floor_boundary(&self.content, caret));
                    self.refresh_cursor();
                    return;
                }
            }
        }
        self.pending_formats = ActiveFormats::default();
        self.edit(range, text);
    }

    /// Replace a WYSIWYG selection that overlaps inline markers by rewriting
    /// the surrounding spans so unselected text keeps its formats and the
    /// typed text is inserted plain (`**He**Xld` not `**He*X*rld`).
    fn replace_wysiwyg_selection(&mut self, range: Range<usize>, text: &str) -> bool {
        let parsed = self.parsed();
        let overlapping = nearby_pair_refs(&parsed, range.clone()).iter().any(|p| {
            p.kind != InlineTagKind::Link
                && p.open_delim.start < range.end
                && p.close_delim.end > range.start
        });
        if !overlapping {
            return false;
        }
        let group_pairs = collect_group_pairs(&parsed, range.clone());
        let replace_start = group_pairs
            .iter()
            .map(|p| p.open_delim.start)
            .min()
            .unwrap_or(range.start)
            .min(range.start);
        let replace_end = group_pairs
            .iter()
            .map(|p| span_close_end(&parsed, p))
            .max()
            .unwrap_or(range.end)
            .max(range.end);
        let skip: Vec<Range<usize>> = group_pairs
            .iter()
            .flat_map(|p| [p.open_delim.clone(), p.close_delim.clone()])
            .collect();
        let segs: Vec<&StyledSegment> = parsed
            .lines_overlapping(replace_start..replace_end)
            .iter()
            .flat_map(|l| l.styled_segments.iter())
            .collect();
        let local_pairs = nearby_pair_refs(&parsed, replace_start..replace_end);
        let slices = collect_rewrite_slices(
            &self.content,
            &local_pairs,
            &segs,
            replace_start..replace_end,
            range.clone(),
            &skip,
            None,
        );
        let selected: Vec<&StyledSlice> = slices.iter().filter(|s| s.is_selected).collect();
        let typed_flags = if !selected.is_empty()
            && selected.iter().all(|s| s.flags == selected[0].flags)
        {
            selected[0].flags
        } else {
            FormatFlags::default()
        };
        let mut out = Vec::new();
        let mut replaced = false;
        for s in slices {
            if s.is_selected {
                if !replaced {
                    out.push(StyledSlice {
                        text: text.to_string(),
                        flags: typed_flags,
                        is_selected: true,
                    });
                    replaced = true;
                }
            } else {
                out.push(s);
            }
        }
        if !replaced {
            return false;
        }
        let glued_left = replace_start > 0
            && self.content[..replace_start]
                .chars()
                .next_back()
                .is_some_and(|c| !c.is_whitespace());
        let (rendered, sel) = render_styled_slices_with_selection_glued(&out, glued_left);
        self.edit(replace_start..replace_end, &rendered);
        self.caret
            .move_to(floor_boundary(&self.content, replace_start + sel.end));
        self.pending_formats = ActiveFormats::default();
        self.refresh_cursor();
        true
    }

    /// Typing a backtick inside a code span cannot be escaped (backslash is
    /// literal inside code); re-fence the whole span with a longer backtick
    /// run so the span stays intact.
    fn refence_code_span(&mut self, pair: &InlinePair, caret: usize, text: &str) {
        let inner_start = pair.inner.start;
        let inner_end = pair.inner.end;
        let mut new_inner = String::with_capacity(inner_end - inner_start + text.len());
        new_inner.push_str(&self.content[inner_start..caret]);
        new_inner.push_str(text);
        new_inner.push_str(&self.content[caret..inner_end]);
        let mut max_run = 0usize;
        let mut run = 0usize;
        for c in new_inner.chars() {
            if c == '`' {
                run += 1;
                max_run = max_run.max(run);
            } else {
                run = 0;
            }
        }
        let fence = "`".repeat(max_run.max(pair.open_delim.len() - 1) + 1);
        let repl = format!("{}{}{}", fence, new_inner, fence);
        let caret_after = pair.open_delim.start + fence.len() + (caret - inner_start) + text.len();
        self.edit(pair.open_delim.start..pair.close_delim.end, &repl);
        self.caret
            .move_to(floor_boundary(&self.content, caret_after));
        self.refresh_cursor();
    }

    #[allow(dead_code)]
    pub fn insert_snippet(&mut self, snippet: &str) {
        let cursor = self.cursor_offset();
        self.edit(
            self.selection_range().start..self.selection_range().end,
            snippet,
        );
        let _ = cursor;
    }

    pub fn move_left(&mut self, extend: bool) {
        self.pending_formats = ActiveFormats::default();
        let target = if !extend && !self.caret.range.is_empty() {
            self.selection_range().start
        } else {
            let pos = self.cursor_offset();
            let ending = line_ending_len_before(&self.content, pos);
            if ending > 0 {
                pos - ending
            } else {
                previous_grapheme(&self.content, pos)
            }
        };
        self.place(target, extend);
    }

    pub fn move_right(&mut self, extend: bool) {
        self.pending_formats = ActiveFormats::default();
        let target = if !extend && !self.caret.range.is_empty() {
            self.selection_range().end
        } else {
            let pos = self.cursor_offset();
            let ending = line_ending_len(&self.content, pos);
            if ending > 0 {
                pos + ending
            } else {
                next_grapheme(&self.content, pos)
            }
        };
        self.place(target, extend);
    }

    pub fn move_left_in_mode(&mut self, extend: bool, mode: ViewMode) {
        self.pending_formats = ActiveFormats::default();
        if mode == ViewMode::Raw {
            self.move_left(extend);
            return;
        }
        if !extend && !self.caret.range.is_empty() {
            let start = self.selection_range().start;
            self.place(start, false);
            return;
        }
        let parsed = self.parsed();
        let mut pos = self.cursor_offset();
        if pos == 0 {
            return;
        }
        if let Some(pline) = parsed.line_at_offset(pos) {
            if extend && pos > visible_line_end(pline, self.view_mode) && pos <= pline.source.end {
                return self.move_left(true);
            }
            if let LineKind::TableRow { ref body, .. } = pline.kind {
                let TableRowBody {
                    cells,
                    raw_cells,
                    table_source,
                    ..
                } = &**body;
                if let Some(c_idx) = raw_cells
                    .iter()
                    .position(|rc| pos >= rc.start && pos <= rc.end)
                {
                    if pos <= cells[c_idx].start {
                        if c_idx > 0 {
                            let target = skip_hidden_left(&parsed, cells[c_idx - 1].end);
                            self.place(target, extend);
                        } else {
                            let prev_row_cell_end =
                                parsed.lines[..pline.line_index].iter().rev().find_map(|l| {
                                    if let LineKind::TableRow { ref body, .. } = l.kind {
                                        let TableRowBody {
                                            cells: pcells,
                                            table_source: pts,
                                            ..
                                        } = &**body;
                                        if pts == table_source {
                                            return pcells.last().map(|c| c.end);
                                        }
                                    }
                                    None
                                });
                            if let Some(prev_end) = prev_row_cell_end {
                                let target = skip_hidden_left(&parsed, prev_end);
                                self.place(target, extend);
                            } else if table_source.start > 0 {
                                let target = skip_hidden_left(&parsed, table_source.start - 1);
                                self.place(target, extend);
                            } else {
                                self.place(0, extend);
                            }
                        }
                        return;
                    }
                }
            }
            if pos <= pline.prefix_range.end && pos >= pline.source.start {
                if pline.source.start > 0 {
                    let prev = pline.source.start - 1;
                    let snapped = skip_hidden_left(&parsed, prev);
                    self.place(if extend { prev } else { snapped }, extend);
                } else {
                    self.place(0, extend);
                }
                return;
            }
        }
        pos = skip_hidden_left(&parsed, pos);
        let ending = line_ending_len_before(&self.content, pos);
        pos = if ending > 0 {
            pos - ending
        } else {
            previous_grapheme(&self.content, pos)
        };
        pos = skip_hidden_interior_left(&parsed, pos);
        if let Some(pline) = parsed.line_at_offset(pos) {
            if pos < pline.prefix_range.end {
                pos = pline.prefix_range.end;
            }
        }
        self.place(pos, extend);
    }

    pub fn move_right_in_mode(&mut self, extend: bool, mode: ViewMode) {
        self.pending_formats = ActiveFormats::default();
        if mode == ViewMode::Raw {
            self.move_right(extend);
            return;
        }
        if !extend && !self.caret.range.is_empty() {
            let end = self.selection_range().end;
            self.place(end, false);
            return;
        }
        let parsed = self.parsed();
        let mut pos = self.cursor_offset();
        if pos >= self.content.len() {
            return;
        }
        if let Some(pline) = parsed.line_at_offset(pos) {
            if let LineKind::TableRow { ref body, .. } = pline.kind {
                let TableRowBody {
                    cells,
                    raw_cells,
                    table_source,
                    ..
                } = &**body;
                if let Some(c_idx) = raw_cells
                    .iter()
                    .position(|rc| pos >= rc.start && pos <= rc.end)
                {
                    if pos >= cells[c_idx].end {
                        if c_idx + 1 < cells.len() {
                            let target = skip_hidden_right(&parsed, cells[c_idx + 1].start);
                            self.place(target, extend);
                        } else {
                            let next_row_cell_start =
                                parsed.lines.get(pline.line_index + 1..).and_then(|rest| {
                                    rest.iter().find_map(|l| {
                                        if let LineKind::TableRow { ref body, .. } = l.kind {
                                            let TableRowBody {
                                                cells: ncells,
                                                table_source: nts,
                                                ..
                                            } = &**body;
                                            if nts == table_source {
                                                return ncells.first().map(|c| c.start);
                                            }
                                        }
                                        None
                                    })
                                });
                            if let Some(next_start) = next_row_cell_start {
                                let target = skip_hidden_right(&parsed, next_start);
                                self.place(target, extend);
                            } else {
                                let after = (table_source.end + 1).min(self.content.len());
                                self.place(after, extend);
                            }
                        }
                        return;
                    }
                }
            }
            if pos < pline.prefix_range.end {
                pos = pline.prefix_range.end;
            }
            let vend = visible_line_end(pline, self.view_mode);
            let source_end = pline.source.end;
            if extend && pos >= vend && pos < source_end {
                return self.move_right(true);
            }
            if pos >= vend && vend < source_end {
                let break_len = line_ending_len(&self.content, source_end);
                if break_len > 0 {
                    let next_line_start = source_end + break_len;
                    let prefix_end = parsed
                        .line_at_offset(next_line_start)
                        .map(|nline| nline.prefix_range.end);
                    if let Some(prefix_end) = prefix_end {
                        let target = skip_hidden_right(&parsed, prefix_end);
                        self.place(target, extend);
                        return;
                    }
                }
                self.place(vend, extend);
                return;
            }
        }
        pos = skip_hidden_right(&parsed, pos);
        if pos >= self.content.len() {
            self.place(self.content.len(), extend);
            return;
        }
        let break_len = line_ending_len(&self.content, pos);
        if break_len > 0 {
            let next_line_start = pos + break_len;
            if let Some(nline) = parsed.line_at_offset(next_line_start) {
                let target = skip_hidden_right(&parsed, nline.prefix_range.end);
                self.place(target, extend);
                return;
            }
        }
        pos = next_grapheme(&self.content, pos);
        pos = skip_hidden_right(&parsed, pos);
        self.place(pos, extend);
    }

    pub fn smart_backspace_in_mode(&mut self, mode: ViewMode) -> bool {
        self.pending_formats = ActiveFormats::default();
        if mode == ViewMode::Raw {
            return self.smart_backspace();
        }
        if self.is_read_only() {
            return false;
        }
        if !self.selection_range().is_empty() {
            let range = self.selection_range();
            self.edit(range, "");
            return true;
        }
        let cursor = self.cursor_offset();
        if cursor == 0 {
            return false;
        }
        let parsed = self.parsed();

        // Guard table pipe delimiters in WYSIWYG mode
        if let Some(pline) = parsed.line_at_offset(cursor) {
            if let LineKind::TableRow { ref body, .. } = pline.kind {
                let TableRowBody {
                    cells, raw_cells, ..
                } = &**body;
                let at_cell_start_or_pipe = raw_cells
                    .iter()
                    .zip(cells.iter())
                    .find(|(rc, _)| cursor >= rc.start && cursor <= rc.end)
                    .map(|(_, c)| cursor <= c.start)
                    .unwrap_or(true);
                if at_cell_start_or_pipe {
                    return false;
                }
            }
        }

        // 1. Check if cursor is inside or immediately after an InlinePair
        for pair in line_pair_refs(&parsed, cursor).iter().rev() {
            let group_end = span_close_end(&parsed, pair);
            let prev_vis = if cursor >= pair.inner.end && cursor <= group_end {
                pair.inner.end
            } else {
                cursor
            };
            if prev_vis > pair.inner.start && prev_vis <= pair.inner.end {
                let prev_g = previous_grapheme(&self.content, prev_vis);
                if prev_g <= pair.inner.start && prev_vis == pair.inner.end {
                    // Only 1 visible grapheme inside this formatted span! Delete the whole span + delimiters!
                    let start = span_open_start(&parsed, pair);
                    self.edit(start..group_end, "");
                    self.caret.move_to(start);
                    self.refresh_cursor();
                    return true;
                } else if cursor >= pair.inner.end && cursor <= group_end {
                    // Cursor was right outside the closing run -> delete the last
                    // visible grapheme inside `word`; never split a delimiter.
                    let deleted = prev_vis - prev_g;
                    self.edit(prev_g..prev_vis, "");
                    self.caret
                        .move_to(floor_boundary(&self.content, group_end - deleted));
                    self.refresh_cursor();
                    return true;
                }
            }
        }

        self.smart_backspace()
    }

    pub fn delete_forward_in_mode(&mut self, mode: ViewMode) {
        if self.is_read_only() {
            return;
        }
        if !self.selection_range().is_empty() {
            let range = self.selection_range();
            self.edit(range, "");
            return;
        }
        let cursor = self.cursor_offset();
        if cursor >= self.content.len() {
            return;
        }
        if mode == ViewMode::Wysiwyg {
            let parsed = self.parsed();
            if let Some(pline) = parsed.line_at_offset(cursor) {
                if let LineKind::TableRow { ref body, .. } = pline.kind {
                    let TableRowBody {
                        cells, raw_cells, ..
                    } = &**body;
                    let at_cell_end_or_pipe = raw_cells
                        .iter()
                        .zip(cells.iter())
                        .find(|(rc, _)| cursor >= rc.start && cursor <= rc.end)
                        .map(|(_, c)| cursor >= c.end)
                        .unwrap_or(true);
                    if at_cell_end_or_pipe {
                        return;
                    }
                }
                let vend = visible_line_end(pline, self.view_mode);
                if cursor == vend
                    && (vend < pline.source.end
                        || line_ending_len(&self.content, pline.source.end) > 0)
                {
                    let after = pline.source.end;
                    let break_len = line_ending_len(&self.content, after);
                    if break_len > 0 {
                        // The joined line's hidden marker would show as text.
                        let text_line = |k: &LineKind| {
                            matches!(
                                k,
                                LineKind::Paragraph
                                    | LineKind::Heading { .. }
                                    | LineKind::Blockquote
                                    | LineKind::ListItem { .. }
                            )
                        };
                        let next_is_block = parsed
                            .lines
                            .get(pline.line_index + 1)
                            .is_some_and(|n| {
                                matches!(
                                    n.kind,
                                    LineKind::Heading { .. }
                                        | LineKind::Blockquote
                                        | LineKind::ListItem { .. }
                                )
                            });
                        let own_text = pline.source.start
                            + block_prefix_in(&self.content, pline.source.start);
                        let strip = if text_line(&pline.kind) && next_is_block && own_text < cursor
                        {
                            block_prefix_in(&self.content, after + break_len)
                        } else {
                            0
                        };
                        self.edit(after..after + break_len + strip, "");
                    }
                    return;
                }

                // Delete skips hidden delimiters like Backspace does, and
                // drops a span whose last visible grapheme it removes
                // instead of leaving lone markers behind.
                let next_vis = next_visible_pos(&parsed, cursor);
                if next_vis >= self.content.len() {
                    return;
                }
                for pair in line_pair_refs(&parsed, next_vis).iter().rev() {
                    if next_vis < pair.inner.start || next_vis >= pair.inner.end {
                        continue;
                    }
                    let only =
                        previous_grapheme(&self.content, pair.inner.end) <= pair.inner.start;
                    if only {
                        let start = span_open_start(&parsed, pair);
                        let end = span_close_end(&parsed, pair);
                        self.edit(start..end, "");
                        self.caret.move_to(floor_boundary(&self.content, start));
                        self.refresh_cursor();
                        return;
                    }
                    let g_end = next_grapheme(&self.content, next_vis);
                    self.edit(next_vis..g_end, "");
                    self.caret
                        .move_to(floor_boundary(&self.content, cursor.min(next_vis)));
                    self.refresh_cursor();
                    return;
                }
                let g_end = next_grapheme(&self.content, next_vis);
                self.edit(next_vis..g_end, "");
                self.caret.move_to(floor_boundary(&self.content, cursor.min(next_vis)));
                self.refresh_cursor();
                return;
            }
        }
        let next = next_grapheme(&self.content, cursor);
        self.edit(cursor..next, "");
    }

    #[allow(dead_code)]
    pub fn is_in_table(&mut self) -> bool {
        let cursor = self.cursor_offset();
        let parsed = self.parsed();
        parsed.lines.iter().any(|l| match l.kind {
            LineKind::TableRow { ref body, .. } => {
                cursor >= body.table_source.start && cursor <= body.table_source.end
            }
            _ => false,
        })
    }

    pub fn apply_table_op(&mut self, op: TableOp) -> bool {
        let pos = self.cursor_offset();
        self.apply_table_op_at(pos, op)
    }

    /// Plan `op` at `pos` and apply it. Undo restores the caret that was
    /// current when this was called, which can differ from `pos` when a
    /// hover control targets another cell.
    pub fn apply_table_op_at(&mut self, pos: usize, op: TableOp) -> bool {
        if self.is_read_only() || !self.file_type.supports_markdown_features() {
            return false;
        }
        let cursor = self.cursor_offset();
        let original_sel = self.selection_snap();
        let Some(plan) = table_ops::plan(&self.content, pos, op) else {
            return false;
        };
        let mapped = original_sel
            .as_ref()
            .map(|sel| mapped_selection(sel, &plan));
        self.next_undo_caret = Some(cursor);
        self.next_redo_caret = Some(plan.caret);
        self.next_undo_sel = original_sel.clone();
        self.next_redo_sel = mapped.clone();
        let caret = plan.caret;
        self.edit(plan.range, &plan.new_text);
        if let Some(sel) = mapped {
            self.restore_selection(sel);
        } else {
            let caret = floor_boundary(&self.content, caret.min(self.content.len()));
            self.caret.move_to(caret);
            self.refresh_cursor();
        }
        true
    }

    fn selection_snap(&self) -> Option<SelectionSnap> {
        if self.caret.range.is_empty() {
            None
        } else {
            Some(SelectionSnap {
                start: self.caret.range.start,
                end: self.caret.range.end,
                reversed: self.caret.reversed,
            })
        }
    }

    fn restore_selection(&mut self, sel: SelectionSnap) {
        let start = floor_boundary(&self.content, sel.start.min(self.content.len()));
        let end = floor_boundary(&self.content, sel.end.min(self.content.len()));
        if end < start {
            self.caret.range = end..start;
            self.caret.reversed = !sel.reversed;
        } else if start == end {
            self.caret.move_to(start);
        } else {
            self.caret.range = start..end;
            self.caret.reversed = sel.reversed;
            self.caret.preferred_col = None;
        }
        self.caret.clamp(&self.content);
        self.refresh_cursor();
    }

    pub fn insert_table_with_size(&mut self, cols: usize, rows: usize) {
        if self.is_read_only() || !self.file_type.supports_markdown_features() {
            return;
        }
        let cols = cols.clamp(1, 10);
        let rows = rows.clamp(1, 20);

        let mut range = self.selection_range();
        let parsed = self.parsed();
        if let Some(table_end) = parsed.lines.iter().find_map(|l| {
            if let LineKind::TableRow { ref body, .. } = l.kind {
                let TableRowBody { table_source, .. } = &**body;
                if range.start >= table_source.start && range.start <= table_source.end {
                    return Some(table_source.end);
                }
            }
            None
        }) {
            range = table_end..table_end;
        }

        let mut prefix = String::new();
        if range.start > 0 {
            let prev_char = self.content[..range.start].chars().next_back();
            if let Some(c) = prev_char {
                if c != '\n' {
                    prefix.push_str("\n\n");
                } else if !self.content[..range.start].ends_with("\n\n") {
                    prefix.push('\n');
                }
            }
        }
        let mut suffix = String::new();
        if range.end < self.content.len() {
            let next_char = self.content[range.end..].chars().next();
            if let Some(c) = next_char {
                if c != '\n' {
                    suffix.push_str("\n\n");
                } else if !self.content[range.end..].starts_with("\n\n") {
                    suffix.push('\n');
                }
            }
        }

        let mut table_rows: Vec<Vec<String>> = Vec::with_capacity(rows);
        for _ in 0..rows {
            table_rows.push(vec![String::new(); cols]);
        }
        let alignments = vec![TableColumnAlign::Left; cols];
        let (table_str, first_cell_rel) = format_markdown_table(&table_rows, &alignments, 0, 0);
        let full_insert = format!("{}{}{}", prefix, table_str, suffix);
        let base = range.start + prefix.len();
        self.edit(range, &full_insert);
        self.caret.range = (base + first_cell_rel.start)..(base + first_cell_rel.end);
        self.caret.reversed = false;
        self.refresh_cursor();
    }

    pub fn table_add_row_above(&mut self) {
        let _ = self.apply_table_op(TableOp::InsertRowAbove);
    }

    pub fn table_add_row_below(&mut self) {
        let _ = self.apply_table_op(TableOp::InsertRowBelow);
    }

    pub fn table_add_col_left(&mut self) {
        let _ = self.apply_table_op(TableOp::InsertColLeft);
    }

    pub fn table_add_col_right(&mut self) {
        let _ = self.apply_table_op(TableOp::InsertColRight);
    }

    pub fn table_delete_row(&mut self) {
        let _ = self.apply_table_op(TableOp::DeleteRow);
    }

    pub fn table_delete_col(&mut self) {
        let _ = self.apply_table_op(TableOp::DeleteCol);
    }

    /// Move the row at `pos` to `to_index`. Index 0 is the header and is refused.
    pub fn table_move_row_at(&mut self, pos: usize, to_index: usize) {
        let _ = self.apply_table_op_at(pos, TableOp::MoveRow { to_index });
    }

    /// Move the column at `pos` to `to_index`.
    pub fn table_move_col_at(&mut self, pos: usize, to_index: usize) {
        let _ = self.apply_table_op_at(pos, TableOp::MoveCol { to_index });
    }

    pub fn table_delete_table(&mut self) {
        let pos = self.cursor_offset();
        self.table_delete_table_at(pos);
    }

    /// Delete the table that contains `pos`. The caret is not required to sit in it.
    pub fn table_delete_table_at(&mut self, pos: usize) {
        if self.is_read_only() {
            return;
        }
        let Some(source) = table_ops::parsed_table_source_in(&self.parsed(), pos) else {
            return;
        };
        let mut del_range = source;
        if del_range.end < self.content.len() && self.content.as_bytes()[del_range.end] == b'\n' {
            del_range.end += 1;
        } else if del_range.start > 0 && self.content.as_bytes()[del_range.start - 1] == b'\n' {
            del_range.start -= 1;
        }
        let target = del_range.start;
        self.edit(del_range, "");
        self.caret.move_to(floor_boundary(&self.content, target));
        self.refresh_cursor();
    }

    pub fn table_align_column(&mut self, align: TableColumnAlign) {
        let _ = self.apply_table_op(TableOp::SetAlign(align));
    }

    pub fn tab_table_cell(&mut self, backwards: bool) -> bool {
        if !self.file_type.supports_markdown_features() {
            return false;
        }
        let cursor = self.cursor_offset();
        let parsed = self.parsed();
        let mut all_cells: Vec<Range<usize>> = Vec::new();
        let mut in_table = false;
        let mut table_end = 0usize;
        let mut num_cols = 2usize;
        for line in &parsed.lines {
            if let LineKind::TableRow { ref body, .. } = line.kind {
                let TableRowBody {
                    cells,
                    table_source,
                    ..
                } = &**body;
                if cursor >= table_source.start && cursor <= table_source.end {
                    in_table = true;
                    table_end = table_source.end;
                    num_cols = cells.len().max(num_cols);
                    all_cells.extend(cells.iter().cloned());
                }
            }
        }
        if !in_table || all_cells.is_empty() {
            return false;
        }
        let cur_idx = all_cells
            .iter()
            .position(|c| cursor >= c.start && cursor <= c.end)
            .unwrap_or_else(|| {
                all_cells
                    .iter()
                    .rposition(|c| c.start <= cursor)
                    .unwrap_or(0)
            });
        let next_idx = if backwards {
            if cur_idx == 0 {
                // First cell: stay put instead of wrapping to the last cell
                return true;
            }
            cur_idx - 1
        } else {
            if cur_idx + 1 == all_cells.len() {
                let new_row = format!("\n|{}", "  |".repeat(num_cols));
                self.edit(table_end..table_end, &new_row);
                let first_cell_start = (table_end + 3).min(self.content.len());
                let target = floor_boundary(&self.content, first_cell_start);
                self.caret.range = target..target;
                self.caret.reversed = false;
                self.refresh_cursor();
                return true;
            }
            cur_idx + 1
        };
        let target = all_cells[next_idx].clone();
        self.caret.range = target;
        self.caret.reversed = false;
        self.refresh_cursor();
        true
    }

    pub fn snap_wysiwyg_table_delimiter(&mut self) {
        let cursor = self.cursor_offset();
        let parsed = self.parsed();
        if let Some(pline) = parsed.line_at_offset(cursor) {
            if matches!(pline.kind, LineKind::TableDelimiter) {
                let next_pos = (pline.source.end + 1).min(self.content.len());
                let target = floor_boundary(&self.content, next_pos);
                self.caret.move_to(target);
                self.refresh_cursor();
            }
        }
    }

    pub fn move_up(&mut self, extend: bool) {
        self.pending_formats = ActiveFormats::default();
        self.history.seal();
        move_vertical_collapsed(&self.content, &mut self.caret, -1, extend);
        self.snap_caret_to_visible_line_end();
        self.refresh_cursor();
    }

    pub fn move_down(&mut self, extend: bool) {
        self.pending_formats = ActiveFormats::default();
        self.history.seal();
        move_vertical_collapsed(&self.content, &mut self.caret, 1, extend);
        self.snap_caret_to_visible_line_end();
        self.refresh_cursor();
    }

    pub fn set_view_mode(&mut self, mode: ViewMode) {
        self.view_mode = mode;
        self.snap_caret_to_visible_line_end();
        self.refresh_cursor();
    }

    /// Up/Down and a mode switch land on a source column. A collapsed caret
    /// that stopped inside a hidden closing-hash run moves to the visible
    /// end. A selection keeps both endpoints.
    fn snap_caret_to_visible_line_end(&mut self) {
        if !self.caret.range.is_empty() || !self.file_type.supports_markdown_features() {
            return;
        }
        let offset = self.cursor_offset();
        let parsed = self.parsed();
        let Some(line) = parsed.line_at_offset(offset) else {
            return;
        };
        let vend = visible_line_end(line, self.view_mode);
        if offset > vend && offset <= line.source.end {
            let prefer = self.caret.preferred_col;
            self.caret.range = vend..vend;
            self.caret.reversed = false;
            self.caret.preferred_col = prefer;
        }
    }

    pub fn move_home(&mut self, extend: bool) {
        self.pending_formats = ActiveFormats::default();
        let target = super::text_ops::home_offset(&self.content, self.cursor_offset());
        self.place(target, extend);
    }

    pub fn move_home_in_mode(&mut self, extend: bool, mode: ViewMode) {
        self.pending_formats = ActiveFormats::default();
        if mode == ViewMode::Raw {
            self.move_home(extend);
            return;
        }
        let cursor = snap_off_ending_interior(&self.content, self.cursor_offset());
        let parsed = self.parsed();
        if let Some(pline) = parsed.line_at_offset(cursor) {
            if let LineKind::TableRow { ref body, .. } = pline.kind {
                let TableRowBody {
                    cells, raw_cells, ..
                } = &**body;
                if let Some((_, cell)) = raw_cells
                    .iter()
                    .zip(cells.iter())
                    .find(|(rc, _)| cursor >= rc.start && cursor <= rc.end)
                {
                    let vis_start = skip_hidden_right(&parsed, cell.start).min(cell.end);
                    self.place(vis_start, extend);
                    return;
                }
            }
            let vis_start =
                skip_hidden_right(&parsed, pline.prefix_range.end).min(pline.source.end);
            self.place(vis_start, extend);
        } else {
            self.move_home(extend);
        }
    }

    pub fn move_end(&mut self, extend: bool) {
        self.pending_formats = ActiveFormats::default();
        let cursor = self.cursor_offset();
        let start = super::text_ops::home_offset(&self.content, cursor);
        let nl = self.content[start..]
            .find('\n')
            .map_or(self.content.len(), |i| start + i);
        let target = if extend {
            nl
        } else {
            self.parsed()
                .line_at_offset(cursor)
                .map_or(nl, |l| visible_line_end(l, self.view_mode))
        };
        self.place(target, extend);
    }

    pub fn move_end_in_mode(&mut self, extend: bool, mode: ViewMode) {
        self.pending_formats = ActiveFormats::default();
        if mode == ViewMode::Raw {
            self.move_end(extend);
            return;
        }
        let cursor = self.cursor_offset();
        let parsed = self.parsed();
        if let Some(pline) = parsed.line_at_offset(cursor) {
            if let LineKind::TableRow { ref body, .. } = pline.kind {
                let TableRowBody {
                    cells, raw_cells, ..
                } = &**body;
                if let Some((_, cell)) = raw_cells
                    .iter()
                    .zip(cells.iter())
                    .find(|(rc, _)| cursor >= rc.start && cursor <= rc.end)
                {
                    self.place(cell.end, extend);
                    return;
                }
            }
        }
        self.move_end(extend);
    }

    pub fn select_line_up(&mut self) {
        let cursor = self.cursor_offset();
        let current_line_start = super::text_ops::home_offset(&self.content, cursor);
        let target = if cursor > current_line_start {
            current_line_start
        } else if current_line_start > 0 {
            super::text_ops::home_offset(&self.content, current_line_start - 1)
        } else {
            0
        };
        self.place(target, true);
    }

    pub fn select_line_down(&mut self) {
        let cursor = self.cursor_offset();
        let parsed = self.parsed();
        let source_end = parsed
            .line_at_offset(cursor)
            .map(|line| line.source.end)
            .unwrap_or_else(|| super::text_ops::line_end(&self.content, cursor));
        let target = if cursor < source_end {
            source_end + line_ending_len(&self.content, source_end).saturating_sub(1)
        } else if source_end < self.content.len() {
            let next = source_end + line_ending_len(&self.content, source_end);
            let next_end = parsed
                .line_at_offset(next)
                .map(|line| line.source.end)
                .unwrap_or_else(|| super::text_ops::line_end(&self.content, next));
            next_end + line_ending_len(&self.content, next_end).saturating_sub(1)
        } else {
            self.content.len()
        };
        self.place(target, true);
    }

    pub fn move_doc_start(&mut self, extend: bool) {
        self.place(0, extend);
    }

    pub fn move_doc_end(&mut self, extend: bool) {
        let len = self.content.len();
        self.place(len, extend);
    }

    pub fn move_word_left(&mut self, extend: bool) {
        let target = if !extend && !self.caret.range.is_empty() {
            super::text_ops::word_left(&self.content, self.selection_range().start)
        } else {
            super::text_ops::word_left(&self.content, self.cursor_offset())
        };
        self.place(target, extend);
    }

    pub fn move_word_right(&mut self, extend: bool) {
        let origin = if !extend && !self.caret.range.is_empty() {
            self.selection_range().end
        } else {
            self.cursor_offset()
        };
        let mut target = super::text_ops::word_right(&self.content, origin);
        if let Some(line) = self.parsed().line_at_offset(origin) {
            let vend = visible_line_end(line, self.view_mode);
            if vend < line.source.end
                && origin <= vend
                && target > vend
                && target <= line.source.end
            {
                target = line.source.end;
            }
        }
        self.place(target, extend);
    }

    pub fn delete_word_left(&mut self) {
        if self.is_read_only() {
            return;
        }
        if !self.caret.range.is_empty() {
            self.edit(self.selection_range(), "");
        } else {
            let cursor = self.cursor_offset();
            let target = super::text_ops::word_left(&self.content, cursor);
            if target < cursor {
                self.edit(target..cursor, "");
            }
        }
    }

    pub fn delete_word_right(&mut self) {
        if self.is_read_only() {
            return;
        }
        if !self.caret.range.is_empty() {
            self.edit(self.selection_range(), "");
        } else {
            let cursor = self.cursor_offset();
            let mut target = super::text_ops::word_right(&self.content, cursor);
            if let Some(line) = self.parsed().line_at_offset(cursor) {
                let vend = visible_line_end(line, self.view_mode);
                if vend < line.source.end && cursor <= vend && target > vend {
                    target = vend;
                }
            }
            if target > cursor {
                self.edit(cursor..target, "");
            }
        }
    }

    pub fn delete_to_line_start(&mut self) {
        if self.is_read_only() {
            return;
        }
        if !self.caret.range.is_empty() {
            self.edit(self.selection_range(), "");
        } else {
            let cursor = self.cursor_offset();
            let target = super::text_ops::home_offset(&self.content, cursor);
            let end = {
                let parsed = self.parsed();
                match parsed.line_at_offset(cursor) {
                    Some(line) => {
                        let vend = visible_line_end(line, self.view_mode);
                        if cursor == vend && vend < line.source.end {
                            line.source.end
                        } else {
                            cursor
                        }
                    }
                    None => cursor,
                }
            };
            if target < end {
                self.edit(target..end, "");
            }
        }
    }

    pub fn delete_to_line_end(&mut self) {
        if self.is_read_only() {
            return;
        }
        if !self.caret.range.is_empty() {
            self.edit(self.selection_range(), "");
        } else {
            let cursor = self.cursor_offset();
            let raw = super::text_ops::line_end(&self.content, cursor);
            let target = self.parsed().line_at_offset(cursor).map_or(raw, |line| {
                let vend = visible_line_end(line, self.view_mode);
                if cursor < vend {
                    let end = line.source.end;
                    end + line_ending_len(&self.content, end).saturating_sub(1)
                } else {
                    vend
                }
            });
            if target > cursor {
                self.edit(cursor..target, "");
            }
        }
    }

    pub fn select_all(&mut self) {
        self.caret.range = 0..self.content.len();
        self.caret.reversed = false;
        self.caret.preferred_col = None;
        self.refresh_cursor();
    }

    pub fn select_word(&mut self) {
        let range = super::text_ops::word_extent(&self.content, self.cursor_offset());
        self.caret.range = range;
        self.caret.reversed = false;
        self.caret.preferred_col = None;
        self.refresh_cursor();
    }

    pub fn select_line(&mut self) {
        let range = super::text_ops::line_extent(&self.content, self.cursor_offset());
        self.caret.range = range;
        self.caret.reversed = false;
        self.caret.preferred_col = None;
        self.refresh_cursor();
    }

    pub fn place(&mut self, offset: usize, extend: bool) {
        self.pending_formats = ActiveFormats::default();
        let mut offset = floor_boundary(&self.content, offset);
        if !extend {
            offset = snap_off_ending_interior(&self.content, offset);
            if let Some(line) = self.parsed().line_at_offset(offset) {
                let vend = visible_line_end(line, self.view_mode);
                if offset > vend && offset <= line.source.end {
                    offset = vend;
                }
            }
        }
        if extend {
            self.caret.select_to(offset);
        } else {
            self.caret.move_to(offset);
        }
        self.refresh_cursor();
    }

    pub fn selected_text(&self) -> String {
        let range = self.selection_range();
        self.content.get(range).unwrap_or("").to_string()
    }

    /// The selection as the WYSIWYG view shows it, without Markdown markers.
    /// Raw mode and non-Markdown files get the source text unchanged.
    pub fn selected_visible_text(&mut self) -> String {
        if self.view_mode != ViewMode::Wysiwyg || self.file_type != FileType::Markdown {
            return self.selected_text();
        }
        let range = self.selection_range();
        if range.is_empty() {
            return String::new();
        }
        self.parsed().visible_text(&self.content, range)
    }

    pub fn find_hit_ranges(&self, term: &str, match_case: bool) -> Vec<Range<usize>> {
        find_matches(&self.content, term, match_case)
    }

    pub fn select_match(
        &mut self,
        term: &str,
        match_case: bool,
        current: Option<usize>,
        forward: bool,
    ) -> Option<usize> {
        let hits = find_matches(&self.content, term, match_case);
        let index = if forward {
            find_next(&hits, self.cursor_offset(), current)
        } else {
            find_prev(&hits, self.selection_range().start, current)
        }?;
        let hit = hits.get(index)?.clone();
        self.caret.range = hit;
        self.caret.reversed = false;
        self.caret.preferred_col = None;
        self.refresh_cursor();
        Some(index)
    }

    pub fn replace_current_match(
        &mut self,
        term: &str,
        replacement: &str,
        match_case: bool,
        current: Option<usize>,
    ) -> Option<usize> {
        if self.is_read_only() || term.is_empty() {
            return None;
        }
        let hits = find_matches(&self.content, term, match_case);
        let index = current
            .filter(|index| hits.get(*index).is_some())
            .or_else(|| find_next(&hits, self.cursor_offset(), None))?;
        let hit = hits.get(index)?.clone();
        self.edit(hit, replacement);
        self.select_match(term, match_case, None, true)
    }

    pub fn replace_every_match(&mut self, term: &str, replacement: &str, match_case: bool) -> bool {
        if self.is_read_only() {
            return false;
        }
        let Some(updated) = replace_all(&self.content, term, replacement, match_case) else {
            return false;
        };
        self.set_content(updated);
        true
    }

    pub fn refresh_stats(&mut self) {
        let stats = compute_stats(&self.content);
        self.word_count = stats.words;
        self.char_count = stats.chars;
        self.line_count = stats.lines;
        self.reading_time_min = stats.reading_time_min;
        self.stats_revision = Some(self.revision);
    }

    /// Compute word/char/line stats only when a reader (the status bar) needs
    /// them, keyed by document revision so edits themselves skip the scan.
    pub fn ensure_stats(&mut self) {
        if self.stats_revision == Some(self.revision) {
            return;
        }
        self.refresh_stats();
    }

    pub fn refresh_cursor(&mut self) {
        let cursor = self.cursor_offset();
        if !self.file_type.supports_markdown_features() {
            // Raw tabs have no Markdown structure: skip the parse.
            let (line, col) = line_col(&self.content, cursor);
            self.cursor_line = line;
            self.cursor_col = col;
            self.active_formats = ActiveFormats::default();
            return;
        }
        let parsed = self.parsed();
        let (line, col) = parsed_line_col(&self.content, &parsed, cursor);
        self.cursor_line = line;
        self.cursor_col = col;
        self.active_formats = compute_active_formats(&parsed, cursor, &self.pending_formats);
    }

    pub fn undo(&mut self) -> bool {
        let Some(restore) = self.history.undo(&mut self.content) else {
            return false;
        };
        self.revision = self.revision.wrapping_add(1);
        self.pending_formats = ActiveFormats::default();
        self.place_after_history(restore);
        self.snap_caret_to_visible_line_end();
        self.refresh_cursor();
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(restore) = self.history.redo(&mut self.content) else {
            return false;
        };
        self.revision = self.revision.wrapping_add(1);
        self.pending_formats = ActiveFormats::default();
        self.place_after_history(restore);
        self.snap_caret_to_visible_line_end();
        self.refresh_cursor();
        true
    }

    fn place_after_history(&mut self, restore: HistoryRestore) {
        self.marked_range = None;
        self.is_modified = true;
        if let Some(sel) = restore.selection {
            self.restore_selection(sel);
            return;
        }
        let caret = floor_boundary(
            &self.content,
            snap_off_ending_interior(&self.content, restore.caret),
        );
        self.caret.move_to(caret);
        self.caret.clamp(&self.content);
        self.refresh_cursor();
    }

    fn clamp_range(&self, range: Range<usize>) -> Range<usize> {
        let start = floor_boundary(&self.content, range.start);
        let end = floor_boundary(&self.content, range.end);
        if end < start {
            end..start
        } else {
            start..end
        }
    }
}

fn mapped_selection(sel: &SelectionSnap, plan: &table_ops::TablePlan) -> SelectionSnap {
    let start = plan.map_point(sel.start);
    let end = plan.map_point(sel.end);
    if end < start {
        SelectionSnap {
            start: end,
            end: start,
            reversed: !sel.reversed,
        }
    } else {
        SelectionSnap {
            start,
            end,
            reversed: sel.reversed,
        }
    }
}

fn format_markdown_table(
    rows: &[Vec<String>],
    alignments: &[TableColumnAlign],
    target_row: usize,
    target_col: usize,
) -> (String, Range<usize>) {
    let num_cols = alignments.len().max(1);
    let mut out = String::new();
    let mut target_cell_range = 0..0;

    for (r_idx, row) in rows.iter().enumerate() {
        if r_idx > 0 {
            out.push('\n');
        }
        out.push('|');
        for c_idx in 0..num_cols {
            let cell_text = table_cell_source(row.get(c_idx).map(|s| s.as_str()).unwrap_or(""));
            out.push(' ');
            let c_start = out.len();
            out.push_str(&cell_text);
            let c_end = out.len();
            out.push_str(" |");
            if r_idx == target_row && c_idx == target_col {
                target_cell_range = c_start..c_end;
            }
        }
        if r_idx == 0 {
            out.push_str("\n|");
            for c_idx in 0..num_cols {
                let align = alignments
                    .get(c_idx)
                    .copied()
                    .unwrap_or(TableColumnAlign::None);
                let delim = match align {
                    TableColumnAlign::None => " --- |",
                    TableColumnAlign::Left => " :--- |",
                    TableColumnAlign::Center => " :---: |",
                    TableColumnAlign::Right => " ---: |",
                };
                out.push_str(delim);
            }
        }
    }

    (out, target_cell_range)
}

/// Write a cell back to Markdown. An existing `\|` stays escaped, and a bare `|` becomes `\|`.
fn table_cell_source(cell: &str) -> String {
    let mut out = String::with_capacity(cell.len());
    let mut chars = cell.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' && chars.peek() == Some(&'|') {
            out.push('\\');
            out.push('|');
            chars.next();
        } else if ch == '|' {
            out.push('\\');
            out.push('|');
        } else {
            out.push(ch);
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockFormatTarget {
    Heading(u8),
    BulletList,
    OrderedList,
    TaskList,
    Blockquote,
}

/// Turning a checklist off on an ordered task keeps `N.` / `N)`.
///
/// A line that is not already a task stays on the shared `- [ ] ` path, so a
/// plain ordered item still converts the same way the kit does. Markers the
/// document does not accept (column 4 or more, or an interrupting empty
/// marker) are not tasks here either.
fn ordered_checklist_line(
    parsed: &ParsedDocument,
    content: &str,
    line_start: usize,
    target: BlockFormatTarget,
    turn_off: bool,
) -> Option<String> {
    let line_end_pos = line_end(content, line_start);
    let line = &content[line_start..line_end_pos];
    let item = accepted_list_marker_in(parsed, content, line_start)?;
    // An ordered task shows the checklist button on and the ordered-list
    // button off. The ordered-list click leaves the line unchanged so it
    // matches that off button.
    if target == BlockFormatTarget::OrderedList {
        if item.ordered && item.task.is_some() {
            return Some(line.to_string());
        }
        return None;
    }
    if target != BlockFormatTarget::TaskList {
        return None;
    }
    if !item.ordered || item.task.is_none() {
        return None;
    }
    if !turn_off {
        return Some(line.to_string());
    }
    let indent = &line[..item.indent_bytes];
    let bytes = line.as_bytes();
    let mut digit_end = item.indent_bytes;
    while digit_end < bytes.len() && bytes[digit_end].is_ascii_digit() {
        digit_end += 1;
    }
    let digits = &line[item.indent_bytes..digit_end];
    let punct = item.punct as char;
    let body = &line[item.prefix_end()..];
    Some(format!("{indent}{digits}{punct} {body}"))
}

fn parse_line_block_prefix(
    parsed: &ParsedDocument,
    content: &str,
    line_start: usize,
) -> (usize, Option<BlockFormatTarget>, usize) {
    let line_end_pos = line_end(content, line_start);
    let line = &content[line_start..line_end_pos];
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    if indent <= 3 {
        for (lvl, pfx) in [
            (6u8, "###### "),
            (5, "##### "),
            (4, "#### "),
            (3, "### "),
            (2, "## "),
            (1, "# "),
        ] {
            if trimmed.starts_with(pfx) {
                return (
                    indent,
                    Some(BlockFormatTarget::Heading(lvl)),
                    indent + pfx.len(),
                );
            }
        }
    }
    if let Some(item) = accepted_list_marker_in(parsed, content, line_start) {
        let target = if item.task.is_some() && !item.ordered {
            BlockFormatTarget::TaskList
        } else if item.ordered {
            BlockFormatTarget::OrderedList
        } else {
            BlockFormatTarget::BulletList
        };
        return (item.indent_bytes, Some(target), item.prefix_end());
    }
    if trimmed.starts_with("> ") {
        return (indent, Some(BlockFormatTarget::Blockquote), indent + 2);
    }
    if trimmed == ">" {
        return (indent, Some(BlockFormatTarget::Blockquote), indent + 1);
    }
    (indent, None, indent)
}

/// True when `from..to` is only whitespace and delimiter markup, so a
/// neighboring span with matching flags can merge across that gap.
fn gap_is_ws_and_markers(content: &str, hidden: &[Range<usize>], from: usize, to: usize) -> bool {
    if to < from {
        return false;
    }
    let mut i = from;
    while i < to {
        if content[i..].starts_with('\n') {
            return false;
        }
        if let Some(h) = hidden.iter().find(|r| i >= r.start && i < r.end) {
            if !is_delimiter_markup(&content[h.start..h.end]) {
                return false;
            }
            i = h.end;
            continue;
        }
        let Some(ch) = content[i..].chars().next() else {
            break;
        };
        if ch.is_whitespace() && ch != '\n' {
            i += ch.len_utf8();
            continue;
        }
        return false;
    }
    true
}

fn is_delimiter_markup(s: &str) -> bool {
    let t = s.trim();
    !t.is_empty()
        && (t.chars().all(|c| matches!(c, '*' | '_' | '~' | '`'))
            || t.eq_ignore_ascii_case("<u>")
            || t.eq_ignore_ascii_case("</u>"))
}

fn covering_span(
    parsed: &ParsedDocument,
    pline: &ParsedLine,
    seg: &StyledSegment,
) -> (usize, usize) {
    let overlapping: Vec<&InlinePair> = pline
        .inline_pairs
        .iter()
        .filter(|p| {
            p.kind != InlineTagKind::Link
                && p.inner.start <= seg.source.start
                && p.inner.end >= seg.source.end
        })
        .collect();
    if overlapping.is_empty() {
        return (seg.source.start, seg.source.end);
    }
    let open = overlapping
        .iter()
        .map(|p| span_open_start(parsed, p))
        .min()
        .unwrap_or(seg.source.start);
    let close = overlapping
        .iter()
        .map(|p| span_close_end(parsed, p))
        .max()
        .unwrap_or(seg.source.end);
    (open, close)
}

/// When turning a format on, extend the rewrite so a neighboring span that
/// already has the same flags is merged instead of emitting `**aa**** bb**`.
fn expand_replace_to_matching_neighbors(
    content: &str,
    parsed: &ParsedDocument,
    pline: &ParsedLine,
    p_range: Range<usize>,
    mut replace: Range<usize>,
    tag_kind: InlineTagKind,
    should_turn_on: bool,
) -> (usize, usize) {
    if !should_turn_on {
        return (replace.start, replace.end);
    }
    let p_start = p_range.start;
    let p_end = p_range.end;
    let overlapping: Vec<&StyledSegment> = pline
        .styled_segments
        .iter()
        .filter(|s| s.source.start < p_end && s.source.end > p_start)
        .collect();
    if overlapping
        .windows(2)
        .any(|w| segment_flags(w[0]) != segment_flags(w[1]))
    {
        return (replace.start, replace.end);
    }
    let mut target = overlapping
        .first()
        .map(|s| segment_flags(s))
        .unwrap_or_default();
    match tag_kind {
        InlineTagKind::Bold => target.bold = true,
        InlineTagKind::Italic => target.italic = true,
        InlineTagKind::Underline => target.underline = true,
        InlineTagKind::Strike => target.strike = true,
        InlineTagKind::Code => target.code = true,
        InlineTagKind::Link => return (replace.start, replace.end),
    }
    if !target.any_set() {
        return (replace.start, replace.end);
    }

    let mut left_front = p_start;
    let mut right_front = p_end;
    loop {
        let mut changed = false;
        for seg in &pline.styled_segments {
            if segment_flags(seg) != target {
                continue;
            }
            if seg.source.end <= left_front
                && gap_is_ws_and_markers(content, &pline.hidden_ranges, seg.source.end, left_front)
            {
                let (open, close) = covering_span(parsed, pline, seg);
                if open < replace.start || close > replace.end || seg.source.start < left_front {
                    replace.start = replace.start.min(open);
                    replace.end = replace.end.max(close);
                    left_front = left_front.min(seg.source.start);
                    changed = true;
                }
            }
            if seg.source.start >= right_front
                && gap_is_ws_and_markers(
                    content,
                    &pline.hidden_ranges,
                    right_front,
                    seg.source.start,
                )
            {
                let (open, close) = covering_span(parsed, pline, seg);
                if close > replace.end || open < replace.start || seg.source.end > right_front {
                    replace.start = replace.start.min(open);
                    replace.end = replace.end.max(close);
                    right_front = right_front.max(seg.source.end);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    (replace.start, replace.end)
}

/// The end of the whole closing-marker run of the stacked group `pair`
/// belongs to (the non-link pairs sharing its content range), so inserted
/// spaces and exits clear every stacked closer (`***`, `~~**`).
fn span_close_end(_parsed: &ParsedDocument, pair: &InlinePair) -> usize {
    pair.group_close_end
}

/// The start of the whole opening-marker run of the stacked group `pair`
/// belongs to, so whole-span edits cover every stacked opener (`***`, `<u>**`).
fn span_open_start(_parsed: &ParsedDocument, pair: &InlinePair) -> usize {
    pair.group_open_start
}

fn nearby_pair_refs(parsed: &ParsedDocument, range: Range<usize>) -> Vec<&InlinePair> {
    parsed
        .lines_near(range)
        .iter()
        .flat_map(|l| l.inline_pairs.iter())
        .collect()
}

/// A code line that comes from indentation, not from inside a fence.
fn is_indented_code(parsed: &ParsedDocument, line: &ParsedLine) -> bool {
    matches!(line.kind, LineKind::CodeLine)
        && !parsed
            .code_blocks
            .iter()
            .any(|(open, close)| line.source.start >= open.start && line.source.start <= close.end)
}

/// The indented code block around `offset`: its consecutive code lines plus
/// any run of blank lines between them (CommonMark example 117), or `None`
/// when `offset` is not on an indented code line.
fn indented_code_block_at(parsed: &ParsedDocument, offset: usize) -> Option<&[ParsedLine]> {
    let at = parsed.line_at_offset(offset)?.line_index;
    let lines = &parsed.lines;
    let code = |index: usize| is_indented_code(parsed, &lines[index]);
    if !code(at) {
        return None;
    }
    let blank = |index: usize| matches!(lines[index].kind, LineKind::Empty);
    let mut first = at;
    let mut probe = at;
    while probe > 0 && (code(probe - 1) || blank(probe - 1)) {
        probe -= 1;
        if code(probe) {
            first = probe;
        }
    }
    let mut last = at;
    let mut probe = at;
    while probe + 1 < lines.len() && (code(probe + 1) || blank(probe + 1)) {
        probe += 1;
        if code(probe) {
            last = probe;
        }
    }
    Some(&lines[first..=last])
}

fn line_pair_refs(parsed: &ParsedDocument, offset: usize) -> &[InlinePair] {
    parsed
        .line_at_offset(offset)
        .map(|l| l.inline_pairs.as_slice())
        .unwrap_or(&[])
}

fn pair_id(p: &InlinePair) -> (usize, usize, InlineTagKind) {
    (p.open_delim.start, p.close_delim.end, p.kind)
}

/// Connected component of overlapping non-link pairs on one line. Pairs on
/// different lines have disjoint source ranges, so a document-wide expansion
/// is the union of per-line expansions.
fn expand_line_group_pairs<'a>(
    line: &'a ParsedLine,
    parsed: &ParsedDocument,
    range: Range<usize>,
    group_pairs: &mut Vec<&'a InlinePair>,
    seen: &mut HashSet<(usize, usize, InlineTagKind)>,
) {
    let mut cands: Vec<&InlinePair> = line
        .inline_pairs
        .iter()
        .filter(|p| p.kind != InlineTagKind::Link)
        .collect();
    if cands.is_empty() {
        return;
    }
    cands.sort_by_key(|p| (p.open_delim.start, span_close_end(parsed, p)));
    let mut lo = range.start;
    let mut hi = range.end.max(range.start);
    loop {
        let lim = cands.partition_point(|p| p.open_delim.start < hi);
        let mut nlo = lo;
        let mut nhi = hi;
        for p in &cands[..lim] {
            let close = span_close_end(parsed, p);
            if close > nlo {
                nlo = nlo.min(p.open_delim.start);
                nhi = nhi.max(close);
            }
        }
        if nlo == lo && nhi == hi {
            break;
        }
        lo = nlo;
        hi = nhi;
    }
    group_pairs.reserve(cands.len());
    let lim = cands.partition_point(|p| p.open_delim.start < hi);
    for p in cands[..lim].iter().copied() {
        if span_close_end(parsed, p) > lo && seen.insert(pair_id(p)) {
            group_pairs.push(p);
        }
    }
}

fn collect_group_pairs(parsed: &ParsedDocument, range: Range<usize>) -> Vec<&InlinePair> {
    let lines = parsed.lines_near(range.clone());
    let mut group_pairs = Vec::with_capacity(lines.iter().map(|l| l.inline_pairs.len()).sum());
    let mut seen = HashSet::with_capacity(group_pairs.capacity());
    for line in lines {
        expand_line_group_pairs(line, parsed, range.clone(), &mut group_pairs, &mut seen);
    }
    group_pairs
}

fn add_contained_pairs<'a>(
    parsed: &'a ParsedDocument,
    replace: Range<usize>,
    group_pairs: &mut Vec<&'a InlinePair>,
) {
    let mut seen: HashSet<(usize, usize, InlineTagKind)> =
        group_pairs.iter().copied().map(pair_id).collect();
    for line in parsed.lines_near(replace.clone()) {
        for p in &line.inline_pairs {
            if p.kind == InlineTagKind::Link {
                continue;
            }
            if p.open_delim.start >= replace.start
                && span_close_end(parsed, p) <= replace.end
                && seen.insert(pair_id(p))
            {
                group_pairs.push(p);
            }
        }
    }
}

/// `line_col` answered from the parsed line table instead of rescanning the
/// text from the start.
fn parsed_line_col(content: &str, parsed: &ParsedDocument, offset: usize) -> (usize, usize) {
    let offset = super::text_ops::floor_char_boundary(content, offset.min(content.len()));
    let idx = parsed.lines.partition_point(|l| l.source.start <= offset);
    let Some(line) = idx.checked_sub(1).map(|i| &parsed.lines[i]) else {
        return line_col(content, offset);
    };
    let end = offset.min(line.source.end);
    (idx, content[line.source.start..end].chars().count() + 1)
}

/// The next source position at or after `pos` that is not inside a hidden
/// (WYSIWYG-invisible) range.
fn next_visible_pos(parsed: &ParsedDocument, mut pos: usize) -> usize {
    while let Some(r) = hidden_range_at(parsed, pos, |r| pos >= r.start && pos < r.end) {
        pos = r.end;
    }
    pos
}

/// First hidden range of the line containing `pos` that satisfies `pred`.
/// Hidden ranges never cover the newline, so the line is enough.
fn hidden_range_at(
    parsed: &ParsedDocument,
    pos: usize,
    pred: impl Fn(&std::ops::Range<usize>) -> bool,
) -> Option<std::ops::Range<usize>> {
    parsed
        .line_at_offset(pos)?
        .hidden_ranges
        .iter()
        .find(|r| pred(r))
        .cloned()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FormatFlags {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub code: bool,
}

impl FormatFlags {
    fn get(self, kind: InlineTagKind) -> bool {
        match kind {
            InlineTagKind::Bold => self.bold,
            InlineTagKind::Italic => self.italic,
            InlineTagKind::Underline => self.underline,
            InlineTagKind::Strike => self.strike,
            InlineTagKind::Code => self.code,
            InlineTagKind::Link => false,
        }
    }

    fn any_set(self) -> bool {
        self.bold || self.italic || self.underline || self.strike || self.code
    }

    /// Bold, italic, and strike markers cannot open or close next to a space.
    fn flanking_sensitive(self) -> bool {
        self.bold || self.italic || self.strike
    }

    /// Flags that may stay on a space peeled out of a flanking-sensitive span.
    /// Underline tags do not depend on spaces, so the space can remain inside
    /// `<u>` while sitting outside `*` / `**` / `~~`.
    fn glue(self) -> Self {
        Self {
            underline: self.underline,
            ..Self::default()
        }
    }
}

#[derive(Clone, Copy)]
enum SharedKind {
    Underline,
    Strike,
    BoldItalic,
    Bold,
    Italic,
    Code,
}

fn flag_of(s: &WorkSlice, kind: SharedKind) -> bool {
    match kind {
        SharedKind::Underline => s.flags.underline,
        SharedKind::Strike => s.flags.strike,
        SharedKind::BoldItalic => s.flags.bold && s.flags.italic,
        SharedKind::Bold => s.flags.bold,
        SharedKind::Italic => s.flags.italic,
        SharedKind::Code => s.flags.code,
    }
}

fn wrap_for_shared(kind: SharedKind) -> (String, String) {
    match kind {
        SharedKind::Underline => ("<u>".into(), "</u>".into()),
        SharedKind::Strike => ("~~".into(), "~~".into()),
        SharedKind::BoldItalic => ("***".into(), "***".into()),
        SharedKind::Bold => ("**".into(), "**".into()),
        SharedKind::Italic => ("*".into(), "*".into()),
        SharedKind::Code => ("`".into(), "`".into()),
    }
}

fn clear_shared(mut flags: FormatFlags, kind: SharedKind) -> FormatFlags {
    match kind {
        SharedKind::Underline => flags.underline = false,
        SharedKind::Strike => flags.strike = false,
        SharedKind::BoldItalic => {
            flags.bold = false;
            flags.italic = false;
        }
        SharedKind::Bold => flags.bold = false,
        SharedKind::Italic => flags.italic = false,
        SharedKind::Code => flags.code = false,
    }
    flags
}

/// Longest consecutive run that shares a wrap-able flag, but is not the whole
/// list (`all_*` already handled that). Prefers outer kinds on a tie.
fn longest_shared_flag_run(slices: &[WorkSlice]) -> Option<(SharedKind, usize, usize)> {
    let kinds = [
        SharedKind::Underline,
        SharedKind::Strike,
        SharedKind::BoldItalic,
        SharedKind::Bold,
        SharedKind::Italic,
        SharedKind::Code,
    ];
    let mut best: Option<(SharedKind, usize, usize, usize)> = None;
    for kind in kinds {
        let mut i = 0;
        while i < slices.len() {
            if !flag_of(&slices[i], kind) {
                i += 1;
                continue;
            }
            let mut j = i;
            while j + 1 < slices.len() && flag_of(&slices[j + 1], kind) {
                j += 1;
            }
            let len = j - i + 1;
            if len >= 2 && len < slices.len() {
                let better = best.is_none_or(|(_, _, _, best_len)| len > best_len);
                if better {
                    best = Some((kind, i, j, len));
                }
            }
            i = j + 1;
        }
    }
    best.map(|(k, i, j, _)| (k, i, j))
}

fn segment_flags(seg: &StyledSegment) -> FormatFlags {
    FormatFlags {
        bold: seg.bold,
        italic: seg.italic,
        underline: seg.underline,
        strike: seg.strike,
        code: seg.code,
    }
}

/// One piece of text going through `render_styled_slices`, with the selected
/// byte range (if any) tracked independently of the wrap so adjacent pieces
/// with the same flags can merge into one span.
#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkSlice {
    text: String,
    flags: FormatFlags,
    sel: Option<Range<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledSlice {
    pub text: String,
    pub flags: FormatFlags,
    pub is_selected: bool,
}

fn wrap_delimiters(flags: &FormatFlags) -> (String, String) {
    let mut open = String::new();
    let mut close = String::new();
    if flags.underline {
        open.push_str("<u>");
        close.push_str("</u>");
    }
    if flags.strike {
        open.push_str("~~");
        close.insert_str(0, "~~");
    }
    if flags.bold && flags.italic {
        open.push_str("***");
        close.insert_str(0, "***");
    } else if flags.bold {
        open.push_str("**");
        close.insert_str(0, "**");
    } else if flags.italic {
        open.push('*');
        close.insert(0, '*');
    }
    if flags.code {
        open.push('`');
        close.insert(0, '`');
    }
    (open, close)
}

fn append_work_slice(dest: &mut WorkSlice, src: &WorkSlice) {
    let at = dest.text.len();
    dest.text.push_str(&src.text);
    if let Some(sel) = &src.sel {
        dest.sel = Some(match &dest.sel {
            Some(prev) => prev.start..(at + sel.end),
            None => (at + sel.start)..(at + sel.end),
        });
    }
}

fn merge_work_slices(slices: &[WorkSlice]) -> Vec<WorkSlice> {
    let mut merged: Vec<WorkSlice> = Vec::with_capacity(slices.len());
    for s in slices {
        if s.text.is_empty() {
            continue;
        }
        if let Some(last) = merged.last_mut() {
            if last.flags == s.flags {
                append_work_slice(last, s);
                continue;
            }
        }
        merged.push(s.clone());
    }
    merged
}

fn sel_overlap(sel: &Option<Range<usize>>, start: usize, end: usize) -> Option<Range<usize>> {
    let sel = sel.as_ref()?;
    let a = sel.start.max(start).min(end);
    let b = sel.end.max(start).min(end);
    if b > a {
        Some((a - start)..(b - start))
    } else {
        None
    }
}

/// Pull leading/trailing spaces out of flanking-sensitive spans so a split
/// at a word edge cannot write `aa** bb` or `***aa **bb`. Underline stays
/// on the peeled space so `<u>` can still wrap the whole phrase.
fn peel_flanking_whitespace(slices: Vec<WorkSlice>) -> Vec<WorkSlice> {
    if slices.len() <= 1 {
        return slices;
    }
    let mut out = Vec::new();
    for s in slices {
        if !s.flags.flanking_sensitive() || s.text.chars().all(char::is_whitespace) {
            out.push(s);
            continue;
        }
        let lead = s.text.len() - s.text.trim_start().len();
        let trail = s.text.len() - s.text.trim_end().len();
        if lead == 0 && trail == 0 {
            out.push(s);
            continue;
        }
        let glue = s.flags.glue();
        let core_end = s.text.len() - trail;
        if lead > 0 {
            out.push(WorkSlice {
                text: s.text[..lead].to_string(),
                flags: glue,
                sel: sel_overlap(&s.sel, 0, lead),
            });
        }
        if core_end > lead {
            out.push(WorkSlice {
                text: s.text[lead..core_end].to_string(),
                flags: s.flags,
                sel: sel_overlap(&s.sel, lead, core_end),
            });
        }
        if trail > 0 {
            out.push(WorkSlice {
                text: s.text[core_end..].to_string(),
                flags: glue,
                sel: sel_overlap(&s.sel, core_end, s.text.len()),
            });
        }
    }
    merge_work_slices(&out)
}

fn can_coalesce_ws_triple(left: &WorkSlice, mid: &WorkSlice, right: &WorkSlice) -> bool {
    let mid_ws = !mid.text.is_empty() && mid.text.chars().all(char::is_whitespace);
    let flags_match = left.flags == right.flags && left.flags.any_set();
    let mid_ok = mid.flags == FormatFlags::default() || mid.flags == left.flags;
    mid_ws && flags_match && mid_ok
}

/// `[bold "aa"] [plain " "] [bold "bb"]` becomes one span so turning a
/// format back on at a neighbor merges instead of writing `**aa**** bb**`.
fn coalesce_ws_between_matching(slices: Vec<WorkSlice>) -> Vec<WorkSlice> {
    let mut out: Vec<WorkSlice> = Vec::with_capacity(slices.len());
    for s in slices {
        out.push(s);
        while out.len() >= 3 {
            let n = out.len();
            if !can_coalesce_ws_triple(&out[n - 3], &out[n - 2], &out[n - 1]) {
                break;
            }
            let right = out.pop().unwrap();
            let mid = out.pop().unwrap();
            let left = out.last_mut().unwrap();
            append_work_slice(left, &mid);
            append_work_slice(left, &right);
        }
    }
    out
}

fn map_work_flags(
    slices: &[WorkSlice],
    mut f: impl FnMut(FormatFlags) -> FormatFlags,
) -> Vec<WorkSlice> {
    slices
        .iter()
        .map(|s| WorkSlice {
            text: s.text.clone(),
            flags: f(s.flags),
            sel: s.sel.clone(),
        })
        .collect()
}

fn mark_work_sel(sel_range: &mut Option<Range<usize>>, at: usize, s: &WorkSlice) {
    if let Some(sel) = &s.sel {
        let piece = (at + sel.start)..(at + sel.end);
        *sel_range = Some(match sel_range.take() {
            Some(prev) => prev.start.min(piece.start)..prev.end.max(piece.end),
            None => piece,
        });
    }
}

fn union_sel(sel_range: &mut Option<Range<usize>>, piece: Range<usize>) {
    *sel_range = Some(match sel_range.take() {
        Some(prev) => prev.start.min(piece.start)..prev.end.max(piece.end),
        None => piece,
    });
}

fn mark_wrapped_sel(
    sel_range: &mut Option<Range<usize>>,
    offset: usize,
    prefix_len: usize,
    suffix_len: usize,
    s: &WorkSlice,
    include_wraps: bool,
) {
    if let Some(sel) = &s.sel {
        if include_wraps && sel.start == 0 && sel.end == s.text.len() {
            union_sel(
                sel_range,
                offset..(offset + prefix_len + s.text.len() + suffix_len),
            );
        } else {
            mark_work_sel(sel_range, offset + prefix_len, s);
        }
    }
}

fn wrap_work_slice(
    s: &WorkSlice,
    prefix: &str,
    suffix: &str,
    offset: usize,
    sel_range: &mut Option<Range<usize>>,
    include_wraps: bool,
) -> String {
    mark_wrapped_sel(
        sel_range,
        offset,
        prefix.len(),
        suffix.len(),
        s,
        include_wraps,
    );
    format!("{}{}{}", prefix, s.text, suffix)
}

fn render_styled_slices(
    slices: &[StyledSlice],
    sel_range: &mut Option<Range<usize>>,
    offset: usize,
) -> String {
    render_styled_slices_opts(slices, sel_range, offset, false, false)
}

fn render_styled_slices_opts(
    slices: &[StyledSlice],
    sel_range: &mut Option<Range<usize>>,
    offset: usize,
    glued_left: bool,
    include_wraps_in_sel: bool,
) -> String {
    let work: Vec<WorkSlice> = slices
        .iter()
        .map(|s| WorkSlice {
            text: s.text.clone(),
            flags: s.flags,
            sel: if s.is_selected {
                Some(0..s.text.len())
            } else {
                None
            },
        })
        .collect();
    render_work_slices(&work, sel_range, offset, glued_left, include_wraps_in_sel)
}

fn slice_has_edge_ws(s: &WorkSlice) -> bool {
    s.text.starts_with(char::is_whitespace) || s.text.ends_with(char::is_whitespace)
}

fn render_work_slices(
    slices: &[WorkSlice],
    sel_range: &mut Option<Range<usize>>,
    offset: usize,
    glued_left: bool,
    include_wraps_in_sel: bool,
) -> String {
    // Merge matching flags first so turning a format back on joins the
    // neighbor. Do not peel yet: shared outer formats (`*…*`, `<u>…</u>`)
    // must still wrap spaces, which is valid as long as the opener/closer
    // sit against non-space at the ends.
    let merged = coalesce_ws_between_matching(merge_work_slices(slices));

    if merged.is_empty() {
        return String::new();
    }
    if merged.len() == 1 {
        let s = &merged[0];
        let (prefix, suffix) = wrap_delimiters(&s.flags);
        return wrap_work_slice(s, &prefix, &suffix, offset, sel_range, include_wraps_in_sel);
    }

    let all_underline = merged.iter().all(|s| s.flags.underline);
    let all_strike = merged.iter().all(|s| s.flags.strike);
    let all_code = merged.iter().all(|s| s.flags.code);
    let all_bold = merged.iter().all(|s| s.flags.bold);
    let all_italic = merged.iter().all(|s| s.flags.italic);

    if all_underline {
        let inner = map_work_flags(&merged, |mut f| {
            f.underline = false;
            f
        });
        let inner_str = render_work_slices(&inner, sel_range, offset + 3, false, include_wraps_in_sel);
        return format!("<u>{}</u>", inner_str);
    }

    if all_strike {
        let inner = map_work_flags(&merged, |mut f| {
            f.strike = false;
            f
        });
        let inner_str = render_work_slices(&inner, sel_range, offset + 2, false, include_wraps_in_sel);
        return format!("~~{}~~", inner_str);
    }

    if all_bold && all_italic {
        let inner = map_work_flags(&merged, |mut f| {
            f.bold = false;
            f.italic = false;
            f
        });
        let inner_str = render_work_slices(&inner, sel_range, offset + 3, false, include_wraps_in_sel);
        return format!("***{}***", inner_str);
    }

    if all_italic {
        let is_middle = merged.len() == 3
            && !merged.iter().any(|s| {
                s.flags.strike || s.flags.underline || s.flags.code || slice_has_edge_ws(s)
            })
            && merged[0].flags.bold
            && !merged[1].flags.bold
            && merged[2].flags.bold;
        if is_middle {
            if glued_left {
                let mut out = String::new();
                for s in &merged {
                    let (prefix, suffix) = if s.flags.bold {
                        ("***", "***")
                    } else {
                        ("*", "*")
                    };
                    mark_work_sel(sel_range, offset + out.len() + prefix.len(), s);
                    out.push_str(prefix);
                    out.push_str(&s.text);
                    out.push_str(suffix);
                }
                return out;
            }
            let mut out = String::from("_");
            for s in &merged {
                let prefix = if s.flags.bold { "**" } else { "" };
                let suffix = if s.flags.bold { "**" } else { "" };
                mark_work_sel(sel_range, offset + out.len() + prefix.len(), s);
                out.push_str(prefix);
                out.push_str(&s.text);
                out.push_str(suffix);
            }
            out.push('_');
            return out;
        }

        let inner = map_work_flags(&merged, |mut f| {
            f.italic = false;
            f
        });
        let inner_str = render_work_slices(&inner, sel_range, offset + 1, false, include_wraps_in_sel);
        return format!("*{}*", inner_str);
    }

    if all_bold {
        let is_middle = merged.len() == 3
            && !merged.iter().any(|s| {
                s.flags.strike || s.flags.underline || s.flags.code || slice_has_edge_ws(s)
            })
            && merged[0].flags.italic
            && !merged[1].flags.italic
            && merged[2].flags.italic;
        if is_middle {
            let mut out = String::new();
            for s in &merged {
                let (prefix, suffix) = if s.flags.italic {
                    ("_**", "**_")
                } else {
                    ("**", "**")
                };
                mark_work_sel(sel_range, offset + out.len() + prefix.len(), s);
                out.push_str(prefix);
                out.push_str(&s.text);
                out.push_str(suffix);
            }
            return out;
        }

        let inner = map_work_flags(&merged, |mut f| {
            f.bold = false;
            f
        });
        let inner_str = render_work_slices(&inner, sel_range, offset + 2, false, include_wraps_in_sel);
        return format!("**{}**", inner_str);
    }

    if all_code {
        // A code span's content is literal, so it must never contain other
        // delimiters. Keep one backtick span only when the non-code flags are
        // identical across the slices (they then differ only by selection);
        // otherwise close and reopen the code span around every non-code
        // change, keeping the backticks adjacent to the text.
        let non_code_equal = merged.iter().all(|s| {
            s.flags.bold == merged[0].flags.bold
                && s.flags.italic == merged[0].flags.italic
                && s.flags.underline == merged[0].flags.underline
                && s.flags.strike == merged[0].flags.strike
        });
        if non_code_equal {
            let inner = map_work_flags(&merged, |mut f| {
                f.code = false;
                f
            });
            let inner_str = render_work_slices(&inner, sel_range, offset + 1, false, include_wraps_in_sel);
            return format!("`{}`", inner_str);
        }
    }

    if let Some((kind, mut i, mut j)) = longest_shared_flag_run(&merged) {
        let ws = |s: &WorkSlice| !s.text.is_empty() && s.text.chars().all(char::is_whitespace);
        let orig_i = i;
        let orig_j = j;
        while j > i && ws(&merged[j]) {
            j -= 1;
        }
        while i < j && ws(&merged[i]) {
            i += 1;
        }
        if j > i && (i > 0 || j + 1 < merged.len()) {
            let (prefix, suffix) = wrap_for_shared(kind);
            // Collide against the slices that stay outside the original run,
            // not the whitespace just peeled off (that whitespace loses this
            // flag and will not wrap).
            let left_collide = orig_i > 0 && {
                let prev_close = wrap_delimiters(&merged[orig_i - 1].flags).1;
                !prev_close.is_empty()
                    && !prefix.is_empty()
                    && prev_close.chars().next_back() == prefix.chars().next()
            };
            let right_collide = orig_j + 1 < merged.len() && {
                let next_open = wrap_delimiters(&merged[orig_j + 1].flags).0;
                !suffix.is_empty()
                    && !next_open.is_empty()
                    && suffix.chars().next_back() == next_open.chars().next()
            };
            // Skip a wrap whose new markers would sit against a neighbor's
            // `*`/`**` and form `*a***`; peel + alternate delimiters instead.
            if !left_collide && !right_collide {
                // Whitespace peeled out of the wrap must lose that flag, or a
                // trailing space stays `** **now` instead of `** …** now`.
                let mut work = merged.clone();
                for s in &mut work[orig_i..i] {
                    s.flags = clear_shared(s.flags, kind);
                }
                if orig_j < work.len() {
                    for s in &mut work[j + 1..=orig_j] {
                        s.flags = clear_shared(s.flags, kind);
                    }
                }
                let left = render_work_slices(
                    &work[..i],
                    sel_range,
                    offset,
                    glued_left,
                    include_wraps_in_sel,
                );
                let inner = map_work_flags(&work[i..=j], |f| clear_shared(f, kind));
                let mid = render_work_slices(
                    &inner,
                    sel_range,
                    offset + left.len() + prefix.len(),
                    false,
                    include_wraps_in_sel,
                );
                let right = render_work_slices(
                    &work[j + 1..],
                    sel_range,
                    offset + left.len() + prefix.len() + mid.len() + suffix.len(),
                    false,
                    include_wraps_in_sel,
                );
                return format!("{left}{prefix}{mid}{suffix}{right}");
            }
        }
    }

    // Formats diverge here, so peel spaces out of flanking-sensitive spans
    // before writing markers. That turns `** aa` / `aa **` into ` **aa` /
    // `aa** ` and stops `***` / `****` from forming at a word edge.
    let merged = coalesce_ws_between_matching(peel_flanking_whitespace(merged));
    if merged.len() <= 1 {
        return render_work_slices(&merged, sel_range, offset, glued_left, include_wraps_in_sel);
    }

    // Adjacent spans whose delimiters collide (a closer and the next opener)
    // sharing a character) merge into one ambiguous run that CommonMark's
    // delimiter arithmetic mis-parses. Resolve collisions before rendering:
    // switch the later span to a delimiter style whose outermost character
    // differs (`__`/`_` for bold and italic, strike nested inside the other
    // formats), unless plain text follows it - then switch the earlier one,
    // whose new closer sits against the next span's opener instead.
    let mut use_alt = vec![false; merged.len()];
    for i in 1..merged.len() {
        let (_, prev_close) = wrap_delimiters(&merged[i - 1].flags);
        let (next_open, _) = wrap_delimiters(&merged[i].flags);
        let collide = !prev_close.is_empty()
            && !next_open.is_empty()
            && prev_close.ends_with(next_open.chars().next().unwrap());
        if !collide {
            continue;
        }
        let avoid = next_open.chars().next().unwrap();
        let alt_available = alternate_wrap(&merged[i].flags, avoid).is_some();
        let plain_follows =
            i + 1 == merged.len() || wrap_delimiters(&merged[i + 1].flags).0.is_empty();
        if alt_available && !plain_follows {
            use_alt[i] = true;
        } else if alt_available && alternate_wrap(&merged[i - 1].flags, avoid).is_some() {
            // Plain text follows this span: its underscore closer would go
            // intraword. Switch the earlier span instead; its new closer sits
            // against this span's opener, which is a delimiter character.
            use_alt[i - 1] = true;
        } else if alt_available {
            use_alt[i] = true;
        }
    }
    let mut out = String::new();
    for (idx, s) in merged.iter().enumerate() {
        let (prefix, suffix) = if use_alt[idx] {
            let avoid = if idx > 0 {
                wrap_delimiters(&merged[idx - 1].flags)
                    .1
                    .chars()
                    .next_back()
                    .unwrap_or('*')
            } else {
                '*'
            };
            alternate_wrap(&s.flags, avoid).unwrap_or_else(|| wrap_delimiters(&s.flags))
        } else {
            wrap_delimiters(&s.flags)
        };
        mark_wrapped_sel(
            sel_range,
            offset + out.len(),
            prefix.len(),
            suffix.len(),
            s,
            include_wraps_in_sel,
        );
        out.push_str(&prefix);
        out.push_str(&s.text);
        out.push_str(&suffix);
    }
    out
}

/// An alternate delimiter wrapping for `flags` whose outermost delimiter
/// character differs from `avoid`, used where two adjacent spans would
/// otherwise merge their closing and opening runs into one ambiguous run.
/// Code spans stay innermost in every variant; returns None when no
/// alternate exists.
fn alternate_wrap(flags: &FormatFlags, avoid: char) -> Option<(String, String)> {
    let mut open = String::new();
    let mut close = String::new();
    if avoid == '*' && (flags.bold || flags.italic) {
        // Bold and italic have a second syntax each; the underscore style
        // moves the boundary character off `*`.
        if flags.bold {
            open.push_str("__");
            close.insert_str(0, "__");
        } else {
            open.push('_');
            close.insert(0, '_');
        }
        if flags.bold && flags.italic {
            open.push('*');
            close.insert(0, '*');
        }
        if flags.code {
            open.push('`');
            close.insert(0, '`');
        }
        return Some((open, close));
    }
    if avoid == '~' && flags.strike {
        // Strike has no second syntax; nest it inside the other formats.
        let mut outer = false;
        if flags.underline {
            open.push_str("<u>");
            close.insert_str(0, "</u>");
            outer = true;
        }
        if flags.bold && flags.italic {
            open.push_str("***");
            close.insert_str(0, "***");
            outer = true;
        } else if flags.bold {
            open.push_str("**");
            close.insert_str(0, "**");
            outer = true;
        } else if flags.italic {
            open.push('*');
            close.insert(0, '*');
            outer = true;
        }
        if outer {
            open.push_str("~~");
            close.insert_str(0, "~~");
            return Some((open, close));
        }
    }
    None
}

fn render_styled_slices_with_selection_glued(
    slices: &[StyledSlice],
    glued_left: bool,
) -> (String, Range<usize>) {
    render_styled_slices_with_selection_opts(slices, glued_left, false)
}

fn render_styled_slices_with_selection_opts(
    slices: &[StyledSlice],
    glued_left: bool,
    include_wraps_in_sel: bool,
) -> (String, Range<usize>) {
    let mut sel = None;
    let res = render_styled_slices_opts(slices, &mut sel, 0, glued_left, include_wraps_in_sel);
    let range = sel.unwrap_or(0..res.len());
    (res, range)
}

/// The raw slice for a styled segment, including a backslash that escapes
/// its first character: escapes are consumed by the parser, so rebuilding a
/// region from segments must re-emit them or the escape is lost.
fn segment_raw_range(content: &str, source: &Range<usize>) -> Range<usize> {
    let mut start = source.start;
    if start > 0
        && content.as_bytes().get(start - 1) == Some(&b'\\')
        && content[start..].chars().next().is_some_and(|c| c.is_ascii_punctuation())
        && content.as_bytes().get(start - 2) != Some(&b'\\')
    {
        start -= 1;
    }
    start..source.end
}

/// One line's inline toggle: (replaced source range, replacement, selected
/// text inside the replacement, start of the selected text in the source).
type InlineRewrite = (Range<usize>, String, Range<usize>, usize);

/// Whether each rewrite parses the way it was meant to: the line keeps its
/// block kind, and every visible segment of the selected text has the format
/// when `on`, or lacks it when not. All rewrites are applied together and
/// parsed once, so a line is judged in its real block context (a nested list,
/// a quote). Rewrites must not overlap, which holds because each sits on its
/// own line or table cell.
fn verify_inline_rewrites(
    content: &str,
    parsed: &ParsedDocument,
    rewrites: &[Option<InlineRewrite>],
    kind: InlineTagKind,
    on: bool,
) -> Vec<bool> {
    let mut next = String::with_capacity(content.len() + rewrites.len() * 8);
    let mut cursor = 0;
    let mut placed: Vec<Option<(usize, Range<usize>)>> = Vec::with_capacity(rewrites.len());
    for rewrite in rewrites {
        let Some((range, text, sel, _)) = rewrite else {
            placed.push(None);
            continue;
        };
        next.push_str(&content[cursor..range.start]);
        let start = next.len();
        next.push_str(text);
        cursor = range.end;
        placed.push(Some((range.start, start + sel.start..start + sel.end)));
    }
    next.push_str(&content[cursor..]);
    let reparsed = crate::markdown::parse_document(&next);

    placed
        .into_iter()
        .map(|place| {
            let Some((old_start, sel)) = place else {
                return false;
            };
            let (Some(old_line), Some(new_line)) =
                (parsed.line_at_offset(old_start), reparsed.line_at_offset(sel.start))
            else {
                return false;
            };
            if std::mem::discriminant(&old_line.kind) != std::mem::discriminant(&new_line.kind) {
                return false;
            }
            let mut any = false;
            let all_match = new_line
                .styled_segments
                .iter()
                .filter(|s| {
                    s.source.start < sel.end
                        && s.source.end > sel.start
                        && !next[s.source.clone()].trim().is_empty()
                })
                .all(|s| {
                    any = true;
                    segment_flags(s).get(kind) == on
                });
            // Turning off only needs no segment left formatted: a line may show
            // nothing once its markers go (`~~~`, a `[^1]: note` definition).
            all_match && (any || !on)
        })
        .collect()
}

fn apply_toggle(flags: &mut FormatFlags, kind: InlineTagKind, on: bool) {
    match kind {
        InlineTagKind::Bold => flags.bold = on,
        InlineTagKind::Italic => flags.italic = on,
        InlineTagKind::Underline => flags.underline = on,
        InlineTagKind::Strike => flags.strike = on,
        InlineTagKind::Code => flags.code = on,
        InlineTagKind::Link => {}
    }
}

fn segment_flags_at(segs: &[&StyledSegment], pos: usize) -> Option<FormatFlags> {
    let i = segs.partition_point(|s| s.source.start <= pos);
    if i == 0 {
        return None;
    }
    let s = segs[i - 1];
    if pos < s.source.end {
        Some(segment_flags(s))
    } else {
        None
    }
}

#[derive(Clone, Copy, Default)]
struct FlagCounts {
    bold: u16,
    italic: u16,
    underline: u16,
    strike: u16,
    code: u16,
}

impl FlagCounts {
    fn apply(&mut self, kind: InlineTagKind, open: bool) {
        let slot = match kind {
            InlineTagKind::Bold => &mut self.bold,
            InlineTagKind::Italic => &mut self.italic,
            InlineTagKind::Underline => &mut self.underline,
            InlineTagKind::Strike => &mut self.strike,
            InlineTagKind::Code => &mut self.code,
            InlineTagKind::Link => return,
        };
        if open {
            *slot = slot.saturating_add(1);
        } else {
            *slot = slot.saturating_sub(1);
        }
    }

    fn flags(self) -> FormatFlags {
        FormatFlags {
            bold: self.bold > 0,
            italic: self.italic > 0,
            underline: self.underline > 0,
            strike: self.strike > 0,
            code: self.code > 0,
        }
    }
}

/// Open/close events sorted by position so flags at successive offsets are O(1)
/// amortized. Closes (`-1`) sort before opens (`1`) at the same byte because
/// `pos < close.end` and `pos >= open.start`.
struct FlagSweep {
    events: Vec<(usize, i8, InlineTagKind)>,
    i: usize,
    counts: FlagCounts,
}

impl FlagSweep {
    fn new(pairs: &[&InlinePair]) -> Self {
        let mut events = Vec::with_capacity(pairs.len() * 2);
        for p in pairs {
            if p.kind == InlineTagKind::Link {
                continue;
            }
            events.push((p.open_delim.start, 1, p.kind));
            events.push((p.close_delim.end, -1, p.kind));
        }
        events.sort_unstable_by_key(|e| (e.0, e.1));
        Self {
            events,
            i: 0,
            counts: FlagCounts::default(),
        }
    }

    fn flags_at(&mut self, pos: usize) -> FormatFlags {
        while self.i < self.events.len() && self.events[self.i].0 <= pos {
            let (_, delta, kind) = self.events[self.i];
            self.counts.apply(kind, delta > 0);
            self.i += 1;
        }
        self.counts.flags()
    }
}

/// Openers and closers of every non-link span the caret sits in, merged into
/// adjacent runs so nested `**… *…* …**` closes as `***` / `***`.
fn spanning_marker_runs(
    content: &str,
    parsed: &ParsedDocument,
    cursor: usize,
) -> Option<(String, String)> {
    let mut spanning: Vec<InlinePair> = line_pair_refs(parsed, cursor)
        .iter()
        .filter(|p| {
            p.kind != InlineTagKind::Link
                && cursor > p.open_delim.start
                && cursor < p.close_delim.end
                && cursor >= p.inner.start
                && cursor <= p.inner.end
        })
        .cloned()
        .collect();
    if spanning.is_empty() {
        return None;
    }
    spanning.sort_by_key(|p| p.open_delim.start);
    let open_ranges =
        merge_adjacent_ranges(spanning.iter().map(|p| p.open_delim.clone()).collect());
    let close_ranges =
        merge_adjacent_ranges(spanning.iter().map(|p| p.close_delim.clone()).collect());
    let openers: String = open_ranges.iter().map(|r| &content[r.clone()]).collect();
    let closers: String = close_ranges.iter().map(|r| &content[r.clone()]).collect();
    Some((openers, closers))
}

fn merge_adjacent_ranges(mut rs: Vec<Range<usize>>) -> Vec<Range<usize>> {
    if rs.is_empty() {
        return rs;
    }
    rs.sort_by_key(|r| r.start);
    let mut out = vec![rs[0].clone()];
    for r in rs.into_iter().skip(1) {
        let last = out.last_mut().unwrap();
        if r.start <= last.end {
            last.end = last.end.max(r.end);
        } else {
            out.push(r);
        }
    }
    out
}

fn merge_skip_ranges(mut rs: Vec<Range<usize>>) -> Vec<Range<usize>> {
    rs.retain(|r| r.end > r.start);
    if rs.len() <= 1 {
        return rs;
    }
    rs.sort_by_key(|r| (r.start, r.end));
    let mut out: Vec<Range<usize>> = Vec::with_capacity(rs.len());
    for r in rs {
        if let Some(last) = out.last_mut() {
            if r.start <= last.end {
                last.end = last.end.max(r.end);
                continue;
            }
        }
        out.push(r);
    }
    out
}

fn skip_end_at(skips: &[Range<usize>], pos: usize) -> Option<usize> {
    let i = skips.partition_point(|r| r.start <= pos);
    if i == 0 {
        return None;
    }
    let r = &skips[i - 1];
    if pos < r.end {
        Some(r.end)
    } else {
        None
    }
}

fn selected_link_markup_ranges(pairs: &[&InlinePair], p_range: Range<usize>) -> Vec<Range<usize>> {
    let mut rs = Vec::new();
    for p in pairs {
        if p.kind != InlineTagKind::Link
            || p.inner.start < p_range.start
            || p.inner.end > p_range.end
            || p.inner.end <= p.inner.start
        {
            continue;
        }
        if p.open_delim.start < p.inner.start {
            rs.push(p.open_delim.start..p.inner.start);
        }
        if p.inner.end < p.close_delim.end {
            rs.push(p.inner.end..p.close_delim.end);
        }
    }
    merge_skip_ranges(rs)
}

/// Rebuild a source range from styled segments **and** the hidden markdown
/// between them (link/image syntax, HTML, escapes), skipping only the
/// delimiter runs in `skip` (the emphasis markers being rewritten).
fn collect_rewrite_slices(
    content: &str,
    pairs: &[&InlinePair],
    segs: &[&StyledSegment],
    replace: Range<usize>,
    p_range: Range<usize>,
    skip: &[Range<usize>],
    toggle: Option<(InlineTagKind, bool)>,
) -> Vec<StyledSlice> {
    let skip = merge_skip_ranges(skip.to_vec());
    let mut segs = segs.to_vec();
    segs.sort_by_key(|s| (s.source.start, s.source.end));
    let mut sweep = FlagSweep::new(pairs);
    let link_markup = selected_link_markup_ranges(pairs, p_range.clone());
    let mut bounds = Vec::with_capacity(
        8 + skip.len() * 2 + segs.len() * 2 + replace.end.saturating_sub(replace.start) / 4,
    );
    bounds.push(replace.start);
    bounds.push(replace.end);
    bounds.push(p_range.start.clamp(replace.start, replace.end));
    bounds.push(p_range.end.clamp(replace.start, replace.end));
    for r in &skip {
        if r.start > replace.start && r.start < replace.end {
            bounds.push(r.start);
        }
        if r.end > replace.start && r.end < replace.end {
            bounds.push(r.end);
        }
    }
    for seg in &segs {
        let raw = segment_raw_range(content, &seg.source);
        if raw.start > replace.start && raw.start < replace.end {
            bounds.push(raw.start);
        }
        if raw.end > replace.start && raw.end < replace.end {
            bounds.push(raw.end);
        }
    }
    // Split whitespace from words so a selected `" wo"` does not keep the
    // leading space inside `* wo*` (nf2). Interior spaces between matching
    // flags are coalesced again when wrapping.
    let mut pos = replace.start;
    while pos < replace.end {
        if let Some(end) = skip_end_at(&skip, pos) {
            pos = end.max(pos + 1);
            continue;
        }
        let ch = match content[pos..].chars().next() {
            Some(c) => c,
            None => break,
        };
        let next = pos + ch.len_utf8();
        if next < replace.end {
            let nch = match content[next..].chars().next() {
                Some(c) => c,
                None => break,
            };
            if ch == '\n' || nch == '\n' || ch.is_whitespace() != nch.is_whitespace() {
                bounds.push(next);
            }
        }
        pos = next;
    }
    bounds.sort_unstable();
    bounds.dedup();

    let turning_off = toggle.is_some_and(|(_, on)| !on);
    let mut slices: Vec<StyledSlice> = Vec::with_capacity(bounds.len().saturating_sub(1));
    for w in bounds.windows(2) {
        let a = w[0];
        let b = w[1];
        if a >= b || skip_end_at(&skip, a).is_some() {
            continue;
        }
        let piece = &content[a..b];
        let is_break = piece == "\n";
        let is_ws = !is_break && piece.chars().all(char::is_whitespace);
        let is_sel = a < p_range.end && b > p_range.start;
        let mut flags = if is_break {
            FormatFlags::default()
        } else {
            let swept = sweep.flags_at(a);
            segment_flags_at(&segs, a).unwrap_or(swept)
        };
        let in_selected_link_markup = turning_off && skip_end_at(&link_markup, a).is_some();
        // Turning a format on skips whitespace so flanking spaces stay
        // outside `*`/`**`/`~~`. Turning one off must still clear it from
        // selected spaces, or `**RAM** **usage**` becomes `RAM** **usage`.
        if (is_sel && !is_break && (!is_ws || turning_off)) || in_selected_link_markup {
            if let Some((kind, on)) = toggle {
                apply_toggle(&mut flags, kind, on);
            }
        }
        if let Some(last) = slices.last_mut() {
            if last.flags == flags && last.is_selected == is_sel && !is_break && last.text != "\n" {
                last.text.push_str(piece);
                continue;
            }
        }
        slices.push(StyledSlice {
            text: piece.to_string(),
            flags,
            is_selected: is_sel,
        });
    }
    slices
}

fn extend_or_splice_span(
    content: &str,
    parsed: &ParsedDocument,
    pending: &ActiveFormats,
    caret: usize,
    text: &str,
) -> Option<(Range<usize>, String, usize)> {
    if caret == 0 {
        return None;
    }
    let has_any_armed =
        pending.bold || pending.italic || pending.underline || pending.strike || pending.code;
    if !has_any_armed {
        return None;
    }
    let has_space = content.as_bytes().get(caret - 1) == Some(&b' ');
    let closer_end = if has_space { caret - 1 } else { caret };
    // Two continuation shapes: typing right after a span's closing run (or
    // after the trailing space it owns), and typing at the end of a span's
    // content while formats are armed - the second keeps the span's own
    // formats on the new text instead of nesting raw markers inside it.
    // Several stacked pairs can share a closing run; prefer one whose group
    // carries every armed format.
    let mut after_candidates: Vec<InlinePair> = line_pair_refs(parsed, closer_end)
        .iter()
        .rev()
        .filter(|p| {
            p.kind != InlineTagKind::Link
                && (span_close_end(parsed, p) == closer_end
                    || close_run_reaches(content, p, closer_end))
        })
        .cloned()
        .collect();
    let is_armed = |k: InlineTagKind| match k {
        InlineTagKind::Bold => pending.bold,
        InlineTagKind::Italic => pending.italic,
        InlineTagKind::Underline => pending.underline,
        InlineTagKind::Strike => pending.strike,
        InlineTagKind::Code => pending.code,
        InlineTagKind::Link => false,
    };
    // Several stacked pairs can share a closing run; prefer one whose group
    // carries every armed format. A pair whose closer sits inside that run
    // (it ends at the caret too) is also a candidate.
    after_candidates.sort_by_key(|p| {
        let carries_armed = [
            InlineTagKind::Bold,
            InlineTagKind::Italic,
            InlineTagKind::Underline,
            InlineTagKind::Strike,
            InlineTagKind::Code,
        ]
        .iter()
        .all(|&k| {
            !is_armed(k)
                || line_pair_refs(parsed, closer_end)
                    .iter()
                    .any(|q| q.kind == k && q.inner == p.inner)
        });
        if carries_armed {
            0
        } else {
            1
        }
    });
    for a in &after_candidates {
        if let Some(r) = splice_for_anchor(content, parsed, pending, caret, text, a, false) {
            return Some(r);
        }
    }
    let inside_candidate = line_pair_refs(parsed, caret)
        .iter()
        .rev()
        .find(|p| {
            p.kind != InlineTagKind::Link && p.inner.end == caret && p.inner.end > p.inner.start
        })
        .cloned()?;
    splice_for_anchor(content, parsed, pending, caret, text, &inside_candidate, true)
}

/// True when the maximal delimiter run that contains `pair`'s closer extends
/// up to `caret`, i.e. the caret stands right after the shared closing run.
fn close_run_reaches(content: &str, pair: &InlinePair, caret: usize) -> bool {
    let bytes = content.as_bytes();
    let Some(delim) = bytes.get(pair.close_delim.start).copied() else {
        return false;
    };
    pair.close_delim.end < caret
        && bytes[pair.close_delim.end..caret].iter().all(|&b| b == delim)
}

fn splice_for_anchor(
    content: &str,
    parsed: &ParsedDocument,
    pending: &ActiveFormats,
    caret: usize,
    text: &str,
    anchor: &InlinePair,
    inside: bool,
) -> Option<(Range<usize>, String, usize)> {
    let group: Vec<InlinePair> = line_pair_refs(parsed, anchor.inner.start)
        .iter()
        .filter(|p| p.kind != InlineTagKind::Link && p.inner == anchor.inner)
        .cloned()
        .collect();
    if group.is_empty() {
        return None;
    }
    let is_armed = |k: InlineTagKind| match k {
        InlineTagKind::Bold => pending.bold,
        InlineTagKind::Italic => pending.italic,
        InlineTagKind::Underline => pending.underline,
        InlineTagKind::Strike => pending.strike,
        InlineTagKind::Code => pending.code,
        InlineTagKind::Link => false,
    };
    let all_armed_in_group = [
        InlineTagKind::Bold,
        InlineTagKind::Italic,
        InlineTagKind::Underline,
        InlineTagKind::Strike,
        InlineTagKind::Code,
    ]
    .iter()
    .all(|&k| !is_armed(k) || group.iter().any(|p| p.kind == k));
    if !inside && !all_armed_in_group {
        return None;
    }
    if inside {
        // Only when the span is the innermost thing around the caret;
        // distinct nested pairs keep the old behavior.
        let nested = line_pair_refs(parsed, caret).iter().any(|p| {
            p.kind != InlineTagKind::Link
                && p.inner != anchor.inner
                && caret >= p.open_delim.start
                && caret <= p.close_delim.end
        });
        if nested {
            return None;
        }
    }

    let has_space = content.as_bytes().get(caret - 1) == Some(&b' ');
    let closer_end = if has_space { caret - 1 } else { caret };
    let group_open_start = span_open_start(parsed, anchor);
    let group_close_end = span_close_end(parsed, anchor);
    // Pairs whose closing markers sit inside the shared run that ends at the
    // caret are re-rendered together with the anchor's group, so their
    // openers are not left dangling by the replacement.
    let mut range_start = group_open_start;
    if !inside && caret > group_close_end {
        let delim = content.as_bytes()[anchor.close_delim.start];
        for p in line_pair_refs(parsed, caret) {
            if p.kind == InlineTagKind::Link || p.inner == anchor.inner {
                continue;
            }
            if p.close_delim.end > group_close_end
                && p.close_delim.end <= caret
                && content[group_close_end..p.close_delim.end]
                    .bytes()
                    .all(|b| b == delim)
            {
                range_start = range_start.min(span_open_start(parsed, p));
            }
        }
    }

    let trail_len = if inside {
        0
    } else {
        text.len() - text.trim_end().len()
    };
    let trail_ws = &text[text.len() - trail_len..];
    let trimmed_text = &text[..text.len() - trail_len];
    let lead_text = if inside {
        trimmed_text.to_string()
    } else if has_space {
        format!(" {}", trimmed_text)
    } else {
        trimmed_text.to_string()
    };

    let in_group = |k: InlineTagKind| group.iter().any(|p| p.kind == k);
    let cont_flags = if inside {
        FormatFlags {
            bold: pending.bold || in_group(InlineTagKind::Bold),
            italic: pending.italic || in_group(InlineTagKind::Italic),
            underline: pending.underline || in_group(InlineTagKind::Underline),
            strike: pending.strike || in_group(InlineTagKind::Strike),
            code: pending.code || in_group(InlineTagKind::Code),
        }
    } else {
        FormatFlags {
            bold: pending.bold,
            italic: pending.italic,
            underline: pending.underline,
            strike: pending.strike,
            code: pending.code,
        }
    };

    let seg_end_limit = if inside { caret } else { closer_end };
    let pline = parsed.line_at_offset(range_start);
    let existing_segs: Vec<&StyledSegment> = pline
        .map(|l| {
            l.styled_segments
                .iter()
                .filter(|s| s.source.start >= range_start && s.source.end <= seg_end_limit)
                .collect()
        })
        .unwrap_or_default();

    let mut slices: Vec<StyledSlice> = if !existing_segs.is_empty() {
        existing_segs
            .iter()
            .map(|s| {
                let raw = segment_raw_range(content, &s.source);
                StyledSlice {
                    text: content[raw].to_string(),
                    flags: FormatFlags {
                        bold: s.bold,
                        italic: s.italic,
                        underline: s.underline,
                        strike: s.strike,
                        code: s.code,
                    },
                    is_selected: false,
                }
            })
            .collect()
    } else {
        let anchor_flags = FormatFlags {
            bold: in_group(InlineTagKind::Bold),
            italic: in_group(InlineTagKind::Italic),
            underline: in_group(InlineTagKind::Underline),
            strike: in_group(InlineTagKind::Strike),
            code: in_group(InlineTagKind::Code),
        };
        vec![StyledSlice {
            text: content[anchor.inner.clone()].to_string(),
            flags: anchor_flags,
            is_selected: false,
        }]
    };

    if !lead_text.is_empty() {
        slices.push(StyledSlice {
            text: lead_text,
            flags: cont_flags,
            is_selected: true,
        });
    }

    let mut sel = None;
    let rendered = render_styled_slices(&slices, &mut sel, 0);
    let repl = format!("{}{}", rendered, trail_ws);
    let replace_end = if inside { group_close_end } else { caret };
    let new_caret = if trail_len > 0 {
        range_start + repl.len()
    } else if let Some(s) = sel {
        range_start + s.end
    } else {
        range_start + repl.len()
    };
    Some((range_start..replace_end, repl, new_caret))
}

/// Inline formats that stay armed at `cursor`, from every marker pair around
/// it. Used to keep typing inside an edited span and to carry formats across
/// a line break.
fn pending_formats_at(parsed: &ParsedDocument, cursor: usize) -> ActiveFormats {
    let mut fmt = ActiveFormats::default();
    for pair in line_pair_refs(parsed, cursor) {
        if cursor >= pair.open_delim.start && cursor <= pair.close_delim.end {
            match pair.kind {
                InlineTagKind::Bold => fmt.bold = true,
                InlineTagKind::Italic => fmt.italic = true,
                InlineTagKind::Underline => fmt.underline = true,
                InlineTagKind::Strike => fmt.strike = true,
                InlineTagKind::Code => fmt.code = true,
                InlineTagKind::Link => {}
            }
        }
    }
    fmt
}

fn compute_active_formats(
    parsed: &ParsedDocument,
    cursor: usize,
    pending: &ActiveFormats,
) -> ActiveFormats {
    let mut fmt = *pending;
    if let Some(line) = parsed.line_at_offset(cursor) {
        match line.kind {
            LineKind::Heading { level: 1 } => fmt.h1 = true,
            LineKind::Heading { level: 2 } => fmt.h2 = true,
            LineKind::Heading { level: 3 } => fmt.h3 = true,
            LineKind::Heading { level: 4 } => fmt.h4 = true,
            LineKind::Heading { level: 5 } => fmt.h5 = true,
            LineKind::Heading { level: 6.. } => fmt.h6 = true,
            LineKind::ListItem { ordered, task, .. } => {
                if task.is_some() {
                    fmt.checklist = true;
                } else if ordered {
                    fmt.ol = true;
                } else {
                    fmt.ul = true;
                }
            }
            LineKind::Blockquote => fmt.quote = true,
            LineKind::CodeFence { .. } | LineKind::CodeLine => fmt.code = true,
            LineKind::TableRow { .. } | LineKind::TableDelimiter => fmt.table = true,
            _ => {}
        }
        for pair in &line.inline_pairs {
            if cursor >= pair.open_delim.start && cursor <= pair.close_delim.end {
                match pair.kind {
                    InlineTagKind::Bold => fmt.bold = true,
                    InlineTagKind::Italic => fmt.italic = true,
                    InlineTagKind::Underline => fmt.underline = true,
                    InlineTagKind::Strike => fmt.strike = true,
                    InlineTagKind::Code => fmt.code = true,
                    InlineTagKind::Link => fmt.link = true,
                }
            }
        }
    }
    fmt
}

fn skip_hidden_left(parsed: &ParsedDocument, mut pos: usize) -> usize {
    while let Some(h) = hidden_range_at(parsed, pos, |r| pos > r.start && pos <= r.end) {
        pos = h.start;
    }
    pos
}

fn skip_hidden_interior_left(parsed: &ParsedDocument, mut pos: usize) -> usize {
    while let Some(h) = hidden_range_at(parsed, pos, |r| pos > r.start && pos < r.end) {
        pos = h.start;
    }
    pos
}

/// End of the text the caret can stand on. A rendered closing-hash heading
/// hides the closer, so this is the click, End, and Right target. Raw paints
/// the source, and every other line keeps the source end.
pub(crate) fn visible_line_end(line: &ParsedLine, mode: ViewMode) -> usize {
    if mode != ViewMode::Raw && matches!(line.kind, LineKind::Heading { .. }) {
        line.offset_map.display_to_source(line.display_text.len())
    } else {
        line.source.end
    }
}

fn skip_hidden_right(parsed: &ParsedDocument, mut pos: usize) -> usize {
    if let Some(line) = parsed.line_at_offset(pos) {
        let vend = visible_line_end(line, ViewMode::Wysiwyg);
        if pos == vend && vend < line.source.end {
            return pos;
        }
    }
    while let Some(h) = hidden_range_at(parsed, pos, |r| pos >= r.start && pos < r.end) {
        pos = h.end;
    }
    pos
}

fn floor_boundary(content: &str, offset: usize) -> usize {
    let offset = offset.min(content.len());
    if content.is_char_boundary(offset) {
        offset
    } else {
        super::text_ops::floor_char_boundary(content, offset)
    }
}

#[derive(Clone)]
struct TableRowInfo {
    source: Range<usize>,
    cells: Vec<Range<usize>>,
    is_header: bool,
}

#[derive(Clone)]
struct TableInfo {
    source: Range<usize>,
    rows: Vec<TableRowInfo>,
    delimiter_end: Option<usize>,
}

/// Given the text after a table's end, return where to insert what so a blank
/// line separates the table from the caret line, and where the caret goes
/// (all offsets relative to the start of `rest`, caret in the edited text).
/// An existing blank line is reused.
fn blank_gap_after_table(rest: &str) -> (usize, &'static str, usize) {
    if rest.is_empty() {
        return (0, "\n\n", 2);
    }
    let ne = line_ending_len(rest, 0);
    if ne == 0 {
        return (0, "\n\n", 2);
    }
    let after = &rest[ne..];
    if after.is_empty() {
        return (ne, "\n", ne + 1);
    }
    let l2_end = line_end(after, 0);
    if after[..l2_end].trim().is_empty() {
        let end_len = line_ending_len(after, l2_end);
        (0, "", ne + l2_end + end_len)
    } else {
        (ne, "\n", ne + 1)
    }
}


fn outer_is_delimited(content: &str, start: usize, end: usize, prefix: &str, suffix: &str) -> bool {
    if start > content.len() || end > content.len() || start > end {
        return false;
    }
    if !content.is_char_boundary(start) || !content.is_char_boundary(end) {
        return false;
    }
    if !content[..start].ends_with(prefix) || !content[end..].starts_with(suffix) {
        return false;
    }
    if prefix == "*" {
        let extra_before = start > 1 && content.as_bytes().get(start - 2) == Some(&b'*');
        let extra_after = content.as_bytes().get(end + 1) == Some(&b'*');
        let triple_before = start > 2 && content.as_bytes().get(start - 3) == Some(&b'*');
        let triple_after = content.as_bytes().get(end + 2) == Some(&b'*');
        let triple = extra_before && triple_before && extra_after && triple_after;
        let single = !extra_before && !extra_after;
        triple || single
    } else if prefix == "`" {
        let extra_before = start > 1 && content.as_bytes().get(start - 2) == Some(&b'`');
        let extra_after = content.as_bytes().get(end + 1) == Some(&b'`');
        !extra_before && !extra_after
    } else {
        true
    }
}

fn is_single_link(text: &str) -> Option<&str> {
    if !text.starts_with('[') || !text.ends_with(')') || text.starts_with("[^") {
        return None;
    }
    let close_bracket = text.find("](")?;
    let inner = &text[1..close_bracket];
    if inner.contains('[') || inner.contains(']') {
        return None;
    }
    let url_part = &text[close_bracket + 2..text.len() - 1];
    if url_part.contains(')') {
        return None;
    }
    Some(inner)
}

fn unwrap_outer_link(content: &str, start: usize, end: usize) -> Option<(Range<usize>, String)> {
    if start == 0 || end > content.len() || start > end {
        return None;
    }
    if !content.is_char_boundary(start) || !content.is_char_boundary(end) {
        return None;
    }
    if !content[..start].ends_with('[') || !content[end..].starts_with("](") {
        return None;
    }
    let rest = &content[end + 2..];
    let close_idx = rest.find(')')?;
    let url = &rest[..close_idx];
    if url.contains('\n') || url.contains('(') || url.contains(']') {
        return None;
    }
    let edit_start = start - 1;
    let edit_end = end + 2 + close_idx + 1;
    Some((edit_start..edit_end, content[start..end].to_string()))
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn block_prefix_len(line: &str) -> usize {
    block_prefix_in(line, 0)
}

/// Prefix length of the line that starts at `line_start` in `content`.
pub fn block_prefix_in(content: &str, line_start: usize) -> usize {
    let line_end_pos = super::text_ops::line_end(content, line_start);
    let line = &content[line_start..line_end_pos];
    let trimmed_start = line.trim_start();
    let indent_len = line.len() - trimmed_start.len();

    if indent_len <= 3 {
        for prefix in ["###### ", "##### ", "#### ", "### ", "## ", "# "] {
            if trimmed_start.starts_with(prefix) {
                return indent_len + prefix.len();
            }
        }
    }

    if let Some(item) = accepted_list_marker(content, line_start) {
        return item.prefix_end();
    }

    if trimmed_start.starts_with("> ") {
        return indent_len + 2;
    }

    0
}

fn cmd_selects_placeholder(cmd: &str) -> bool {
    matches!(
        cmd,
        "bold" | "italic" | "underline" | "strike" | "code" | "link"
    )
}

fn placeholder_prefix_len(cmd: &str) -> usize {
    match cmd {
        "bold" => 2,
        "italic" | "code" => 1,
        "underline" => 3,
        "strike" => 2,
        "link" => 1,
        _ => 0,
    }
}

fn placeholder_len(cmd: &str) -> usize {
    match cmd {
        "bold" => "bold text".len(),
        "italic" => "italic text".len(),
        "underline" => "underlined".len(),
        "strike" => "strikethrough".len(),
        "code" => "code".len(),
        "link" => "link".len(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod format_toggle_gate;
#[cfg(test)]
mod table_ops_gate;


#[cfg(test)]
mod diff_probe;
#[cfg(test)]
mod heading_format_tests;
#[cfg(test)]
mod heading_keyboard_selection_tests;
#[cfg(test)]
mod heading_paragraph_parity_tests;

#[cfg(test)]
mod clear_formatting_tests;
