use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

use crate::markdown::{
    accepted_list_marker_in, empty_parsed_document, parse_document, unescaped_pipe_offsets,
    InlinePair, InlineTagKind, LineKind, ParsedDocument, ParsedLine, StyledSegment,
    TableColumnAlign, TableRowBody, MAX_LIST_NUMBER,
};

use super::table_ops::{self, TableOp};
use super::text_ops::{
    count_line_endings, find_matches, find_next, find_prev, format_replacement, home_offset,
    line_col, line_col_near, line_end, line_ending_len, line_ending_len_before, line_start_flags,
    move_vertical_collapsed, next_grapheme, offset_from_utf16_near, offset_to_utf16_near,
    previous_grapheme, replace_all, snap_off_ending_interior, Caret, CountAnchor, DiffEdit,
    HistoryRestore, SelectionSnap, UndoHistory, TAB_DISPLAY_COLUMNS,
};

mod blocks;
mod inline_format;
mod inline_rewrite;
mod navigation;
mod newline;
mod picture;
mod table;
mod typing;

pub(crate) use blocks::*;
use inline_format::*;
pub(crate) use inline_rewrite::*;
pub(crate) use navigation::*;
use typing::*;

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

#[cfg(test)]
pub fn compute_stats(content: &str) -> DocumentStats {
    stats_and_line_endings(content).0
}

/// [`compute_stats`] plus the line endings it counted, which
/// [`StagedStats`] keeps up to date from each edit.
fn stats_and_line_endings(content: &str) -> (DocumentStats, usize) {
    let chars = content.chars().count();
    let words = content.split_whitespace().count();
    let endings = count_line_endings(content.as_bytes());
    let stats = DocumentStats {
        words,
        chars,
        lines: line_total(content, endings),
        reading_time_min: (words / 200).max(1),
    };
    (stats, endings)
}

/// Lines in `content`, given its count of line endings (LF, CRLF or a lone
/// CR, as the raw view's gutter counts them). A final line ending doesn't
/// start a new line, and empty text is one line.
fn line_total(content: &str, endings: usize) -> usize {
    if content.ends_with(['\n', '\r']) {
        endings.max(1)
    } else {
        endings + 1
    }
}

/// Line endings in `before + mid + after`. CRLF is two bytes, so one byte
/// of context on each side of an edit catches every ending it joins or splits.
fn window_line_endings(before: Option<u8>, mid: &str, after: Option<u8>) -> usize {
    let bytes = mid.as_bytes();
    let mut count = count_line_endings(bytes);
    // `mid` alone counts a trailing CR as an ending; a following LF joins it.
    if bytes.last() == Some(&b'\r') && after == Some(b'\n') {
        count -= 1;
    }
    match before {
        Some(b'\n') => count += 1,
        // A CR counts unless the byte after it is an LF.
        Some(b'\r') if bytes.first().or(after.as_ref()) != Some(&b'\n') => count += 1,
        _ => {}
    }
    if matches!(after, Some(b'\n' | b'\r')) {
        count += 1;
    }
    count
}

/// Incrementally maintained stats staged during `DocumentTab::edit` so
/// `ensure_stats` can publish up-to-date counts in O(1) without rescanning the
/// whole document on every keystroke.
#[derive(Debug, Clone, Copy)]
struct StagedStats {
    revision: u64,
    words: usize,
    chars: usize,
    line_endings: usize,
}

/// [`CountAnchor`]s for one revision of a tab's text.
#[derive(Debug, Clone, Copy, Default)]
struct CountAnchors {
    revision: u64,
    line: Option<CountAnchor>,
    utf16: Option<CountAnchor>,
}

/// Count whitespace-delimited words across `prefix + mid + suffix`. Expanding
/// an edited range by one char on each side captures every word-boundary
/// transition affected by the edit.
fn count_window_words(prefix: Option<char>, mid: &str, suffix: Option<char>) -> usize {
    let mut words = 0usize;
    let mut in_word = false;
    for ch in prefix.into_iter().chain(mid.chars()).chain(suffix) {
        if ch.is_whitespace() {
            in_word = false;
        } else if !in_word {
            in_word = true;
            words += 1;
        }
    }
    words
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
    /// The file as last read or written; `None` for a tab not backed by a
    /// file, or when the file couldn't be inspected.
    pub disk_version: Option<super::workspace::DiskVersion>,
    /// The file started with a UTF-8 byte order mark. It is kept out of
    /// `content`, so the first line's Markdown is recognized, and written
    /// back by [`DocumentTab::file_text`].
    pub has_bom: bool,
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
    /// Start of a picture line whose raw Markdown is being edited.
    pub picture_raw_line: Option<usize>,
    /// Padded column index clicked in a short table row before typing.
    pub pending_table_col: Option<usize>,
    /// Line break inserted for input-method text at a picture, and whether
    /// it went before the picture. Taken back if the input is cancelled.
    pub ime_opened_line: Option<(Range<usize>, bool)>,
    pub revision: u64,
    cached_parse: Option<(u64, Arc<ParsedDocument>)>,
    /// The edit that made `revision` from the one before it: the revision
    /// it applied to, the bytes it replaced, and the length written. Lets
    /// `parsed()` update the cached parse instead of parsing everything.
    last_edit: Option<(u64, Range<usize>, usize)>,
    stats_revision: Option<u64>,
    staged_stats: Option<StagedStats>,
    /// Where the last line and UTF-16 lookups landed, so the next lookup
    /// near the caret counts from there instead of from byte 0.
    count_anchors: std::cell::Cell<CountAnchors>,
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
    /// Whether `active_formats` has been computed for the current caret.
    /// Deferred on `DocumentTab::new` so background/restored tabs skip a full
    /// startup parse until activated or rendered.
    cursor_formats_ready: bool,
    #[cfg(test)]
    pub(crate) parse_clone_count: usize,
    #[cfg(test)]
    pub(crate) full_parse_count: usize,
}

impl DocumentTab {
    pub fn new(
        id: String,
        title: String,
        mut content: String,
        file_path: Option<PathBuf>,
        workspace_file_id: Option<String>,
    ) -> Self {
        let has_bom = content.starts_with('\u{feff}');
        if has_bom {
            content.drain(..'\u{feff}'.len_utf8());
        }
        let file_type = FileType::from_filename(&title);
        let (stats, line_endings) = stats_and_line_endings(&content);
        let default_style = indent_style(&title, file_type);
        let indent_style = if file_type.is_code() {
            detect_indent_style(&content, default_style)
        } else {
            default_style
        };

        Self {
            id,
            title,
            content,
            file_type,
            is_modified: false,
            file_path,
            workspace_file_id,
            disk_version: None,
            has_bom,
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
            picture_raw_line: None,
            pending_table_col: None,
            ime_opened_line: None,
            revision: 1,
            cached_parse: None,
            last_edit: None,
            stats_revision: Some(1),
            staged_stats: Some(StagedStats {
                revision: 1,
                words: stats.words,
                chars: stats.chars,
                line_endings,
            }),
            count_anchors: Default::default(),
            history: UndoHistory::default(),
            next_edit_opens_span_group: false,
            next_undo_caret: None,
            next_redo_caret: None,
            next_undo_sel: None,
            next_redo_sel: None,
            cursor_formats_ready: !file_type.supports_markdown_features(),
            #[cfg(test)]
            parse_clone_count: 0,
            #[cfg(test)]
            full_parse_count: 0,
        }
    }

    /// Replace the text with a newer version read from disk. Undo history is
    /// dropped (it describes the old text); the view mode and, where it
    /// still fits, the caret are kept. The revision keeps counting up so
    /// caches keyed on it never mistake the new text for the old.
    pub fn reload(&mut self, mut content: String) {
        let prev_cursor = self.cursor_offset();
        // Drop old buffers before rebuilding so reload doesn't hold 2x the
        // document + parse cache + undo history in memory simultaneously.
        self.content = String::new();
        self.cached_parse = None;
        self.last_edit = None;
        self.history = UndoHistory::default();
        self.count_anchors = Default::default();

        let has_bom = content.starts_with('\u{feff}');
        if has_bom {
            content.drain(..'\u{feff}'.len_utf8());
        }
        let file_type = FileType::from_filename(&self.title);
        let (stats, line_endings) = stats_and_line_endings(&content);
        let default_style = indent_style(&self.title, file_type);
        let indent_style = if file_type.is_code() {
            detect_indent_style(&content, default_style)
        } else {
            default_style
        };

        let next_revision = self.revision + 1;
        let cursor = floor_boundary(&content, prev_cursor.min(content.len()));

        self.content = content;
        self.file_type = file_type;
        self.is_modified = false;
        self.has_bom = has_bom;
        self.word_count = stats.words;
        self.char_count = stats.chars;
        self.line_count = stats.lines;
        self.reading_time_min = stats.reading_time_min;
        self.cursor_line = 1;
        self.cursor_col = 1;
        self.active_formats = ActiveFormats::default();
        self.pending_formats = ActiveFormats::default();
        self.caret = Caret::default();
        self.caret.move_to(cursor);
        self.indent_style = indent_style;
        self.force_inline_toggle = None;
        self.marked_range = None;
        self.picture_raw_line = None;
        self.pending_table_col = None;
        self.ime_opened_line = None;
        self.revision = next_revision;
        self.stats_revision = Some(next_revision);
        self.staged_stats = Some(StagedStats {
            revision: next_revision,
            words: stats.words,
            chars: stats.chars,
            line_endings,
        });
        self.next_edit_opens_span_group = false;
        self.next_undo_caret = None;
        self.next_redo_caret = None;
        self.next_undo_sel = None;
        self.next_redo_sel = None;
        self.cursor_formats_ready = !file_type.supports_markdown_features();
        #[cfg(test)]
        {
            self.parse_clone_count = 0;
            self.full_parse_count = 0;
        }
        self.refresh_cursor();
    }

    /// The text to write to the file: `content`, with the byte order mark
    /// put back if the file had one.
    pub fn file_text(&self) -> String {
        self.file_text_ref().into_owned()
    }

    /// [`DocumentTab::file_text`] without copying the document when the
    /// file has no byte order mark (the usual case). For hashing and
    /// comparing, which only read it.
    pub fn file_text_ref(&self) -> std::borrow::Cow<'_, str> {
        if self.has_bom {
            std::borrow::Cow::Owned(format!("\u{feff}{}", self.content))
        } else {
            std::borrow::Cow::Borrowed(&self.content)
        }
    }

    pub fn parsed(&mut self) -> Arc<ParsedDocument> {
        if !self.file_type.supports_markdown_features() {
            return empty_parsed_document();
        }
        if let Some((rev, ref doc)) = self.cached_parse {
            if rev == self.revision {
                return doc.clone();
            }
        }
        // One known edit since the cached parse: update it. In place when
        // nothing else holds it; otherwise on a copy.
        if let (Some((rev, _)), Some((edit_rev, range, new_len))) =
            (&self.cached_parse, &self.last_edit)
        {
            if rev == edit_rev && rev.wrapping_add(1) == self.revision {
                let (range, new_len) = (range.clone(), *new_len);
                let (_, mut doc) = self.cached_parse.take().expect("cached parse");
                #[cfg(test)]
                if Arc::strong_count(&doc) > 1 {
                    self.parse_clone_count += 1;
                }
                Arc::make_mut(&mut doc).apply_edit(&self.content, range, new_len);
                self.cached_parse = Some((self.revision, doc.clone()));
                return doc;
            }
        }
        #[cfg(test)]
        {
            self.full_parse_count += 1;
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

    #[cfg(test)]
    pub fn undo_stored_bytes(&self) -> usize {
        self.history.stored_bytes()
    }

    /// Replace the whole text as one undo step. Only the part between the
    /// common start and end is recorded, so a Replace All that changes a few
    /// words stores those words, not two copies of the document.
    pub fn set_content(&mut self, new_content: String) {
        if self.content == new_content {
            return;
        }
        let (old, new) = (self.content.as_bytes(), new_content.as_bytes());
        let mut prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
        while !self.content.is_char_boundary(prefix) || !new_content.is_char_boundary(prefix) {
            prefix -= 1;
        }
        let max_suffix = (old.len() - prefix).min(new.len() - prefix);
        let mut suffix = old
            .iter()
            .rev()
            .zip(new.iter().rev())
            .take(max_suffix)
            .take_while(|(a, b)| a == b)
            .count();
        while !self.content.is_char_boundary(old.len() - suffix)
            || !new_content.is_char_boundary(new.len() - suffix)
        {
            suffix -= 1;
        }
        let old_end = old.len() - suffix;
        let new_end = new.len() - suffix;
        self.history.seal();
        self.edit(prefix..old_end, &new_content[prefix..new_end]);
        self.history.seal();
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
        self.last_edit = Some((self.revision, range.clone(), new_text.len()));
        if let Some(staged) = self.staged_stats {
            let staged_next = (staged.revision == self.revision)
                .then(|| {
                    let old_slice = &self.content[range.clone()];
                    let before = &self.content[..range.start];
                    let after = &self.content[range.end..];
                    // Counts in a window around the edit, before and after it.
                    let shift =
                        |total: usize, new: usize, old: usize| (total + new).checked_sub(old);
                    let chars = shift(
                        staged.chars,
                        new_text.chars().count(),
                        old_slice.chars().count(),
                    )?;
                    let (b, a) = (before.bytes().last(), after.bytes().next());
                    let line_endings = shift(
                        staged.line_endings,
                        window_line_endings(b, new_text, a),
                        window_line_endings(b, old_slice, a),
                    )?;
                    let (b, a) = (before.chars().next_back(), after.chars().next());
                    let words = shift(
                        staged.words,
                        count_window_words(b, new_text, a),
                        count_window_words(b, old_slice, a),
                    )?;
                    Some(StagedStats {
                        revision: self.revision.wrapping_add(1),
                        words,
                        chars,
                        line_endings,
                    })
                })
                .flatten();
            // A count that would go negative means the staged numbers are
            // off; dropping them makes the next read recount from scratch.
            self.staged_stats = staged_next;
        }
        self.carry_count_anchors_past(&range);
        self.content.replace_range(range, new_text);
        self.shrink_content_if_bloated();
        let cursor = snap_off_ending_interior(&self.content, cursor_at);
        self.revision = self.revision.wrapping_add(1);
        self.caret.move_to(cursor.min(self.content.len()));
        self.marked_range = None;
        self.pending_table_col = None;
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

    pub fn select_match_in(
        &mut self,
        hits: &[Range<usize>],
        current: Option<usize>,
        forward: bool,
    ) -> Option<usize> {
        let index = if forward {
            find_next(hits, self.cursor_offset(), current)
        } else {
            find_prev(hits, self.selection_range().start, current)
        }?;
        let hit = hits.get(index)?.clone();
        if let Some(block) = self.picture_block_at(hit.start) {
            if hit.end <= block.end {
                self.picture_raw_line = Some(block.start);
            }
        }
        self.caret.range = hit;
        self.caret.reversed = false;
        self.caret.preferred_col = None;
        self.refresh_cursor();
        Some(index)
    }

    pub fn select_match(
        &mut self,
        term: &str,
        match_case: bool,
        current: Option<usize>,
        forward: bool,
    ) -> Option<usize> {
        let hits = find_matches(&self.content, term, match_case);
        self.select_match_in(&hits, current, forward)
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
        let next = self.select_match(term, match_case, None, true);
        if next.is_none() {
            self.snap_caret_to_visible_line_end();
            self.refresh_cursor();
        }
        next
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

    /// Anchors still valid for the current text.
    fn count_anchors(&self) -> CountAnchors {
        let anchors = self.count_anchors.get();
        if anchors.revision == self.revision {
            anchors
        } else {
            CountAnchors {
                revision: self.revision,
                ..CountAnchors::default()
            }
        }
    }

    /// Keep the anchors that an edit of `range` leaves valid: text before
    /// the edit doesn't change. A line anchor must sit strictly before it,
    /// since inserting an LF right after a lone CR moves that line start.
    fn carry_count_anchors_past(&mut self, range: &Range<usize>) {
        let anchors = self.count_anchors();
        self.count_anchors.set(CountAnchors {
            revision: self.revision.wrapping_add(1),
            line: anchors.line.filter(|a| a.byte < range.start),
            utf16: anchors.utf16.filter(|a| a.byte <= range.start),
        });
    }

    /// 1-based line and column of `offset`, as [`line_col`] counts them.
    pub fn line_col_at(&self, offset: usize) -> (usize, usize) {
        let mut anchors = self.count_anchors();
        let (line_col, anchor) = line_col_near(&self.content, offset, anchors.line);
        anchors.line = Some(anchor);
        self.count_anchors.set(anchors);
        line_col
    }

    /// UTF-16 offset of a byte offset (for the platform text input).
    pub fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut anchors = self.count_anchors();
        let (utf16, anchor) = offset_to_utf16_near(&self.content, offset, anchors.utf16);
        anchors.utf16 = Some(anchor);
        self.count_anchors.set(anchors);
        utf16
    }

    /// Byte offset of a UTF-16 offset (for the platform text input).
    pub fn offset_from_utf16(&self, utf16_offset: usize) -> usize {
        let mut anchors = self.count_anchors();
        let (byte, anchor) = offset_from_utf16_near(&self.content, utf16_offset, anchors.utf16);
        anchors.utf16 = Some(anchor);
        self.count_anchors.set(anchors);
        byte
    }

    pub fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    pub fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }

    pub fn refresh_stats(&mut self) {
        let (stats, line_endings) = stats_and_line_endings(&self.content);
        self.word_count = stats.words;
        self.char_count = stats.chars;
        self.line_count = stats.lines;
        self.reading_time_min = stats.reading_time_min;
        self.stats_revision = Some(self.revision);
        self.staged_stats = Some(StagedStats {
            revision: self.revision,
            words: stats.words,
            chars: stats.chars,
            line_endings,
        });
    }

    /// Compute word/char/line stats only when a reader (the status bar) needs
    /// them, keyed by document revision so edits themselves skip the scan.
    pub fn ensure_stats(&mut self) {
        if self.stats_revision == Some(self.revision) {
            return;
        }
        if let Some(staged) = self.staged_stats.filter(|s| s.revision == self.revision) {
            self.word_count = staged.words;
            self.char_count = staged.chars;
            self.line_count = line_total(&self.content, staged.line_endings);
            self.reading_time_min = (staged.words / 200).max(1);
            self.stats_revision = Some(self.revision);
            return;
        }
        self.refresh_stats();
    }

    /// Compute `active_formats` if deferred on startup/tab creation.
    pub fn ensure_cursor_formats(&mut self) {
        if !self.cursor_formats_ready {
            self.refresh_cursor();
        }
    }

    /// Reclaim excess `String` buffer capacity after a large deletion so a
    /// cleared multi-MB document doesn't keep its peak allocation forever.
    fn shrink_content_if_bloated(&mut self) {
        const MIN_SHRINK_CAP: usize = 64 * 1024;
        if self.content.capacity() > MIN_SHRINK_CAP
            && self.content.capacity() > self.content.len().saturating_mul(4)
        {
            let target = self.content.len().saturating_mul(2).max(4096);
            self.content.shrink_to(target);
        }
    }

    pub fn refresh_cursor(&mut self) {
        self.expire_picture_raw();
        self.cursor_formats_ready = true;
        let cursor = self.cursor_offset();
        if !self.file_type.supports_markdown_features() {
            // Raw tabs have no Markdown structure: skip the parse.
            let (line, col) = self.line_col_at(cursor);
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
        self.shrink_content_if_bloated();
        self.last_edit = Some((
            self.revision,
            restore.replaced.0.clone(),
            restore.replaced.1,
        ));
        self.staged_stats = None;
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
        self.shrink_content_if_bloated();
        self.last_edit = Some((
            self.revision,
            restore.replaced.0.clone(),
            restore.replaced.1,
        ));
        self.staged_stats = None;
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
#[cfg(test)]
mod picture_tests;
