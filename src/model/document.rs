use serde::{Deserialize, Serialize};
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

use crate::markdown::{
    parse_document, split_table_cells, unescaped_pipe_offsets, InlineTagKind, LineKind,
    ParsedDocument, TableColumnAlign,
};

use super::text_ops::{
    find_matches, find_next, find_prev, format_replacement, home_offset, line_col, line_end,
    line_start_flags, move_vertical_collapsed, next_grapheme, previous_grapheme, replace_all,
    Caret, DiffEdit, UndoHistory,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FileType {
    #[default]
    Markdown,
    PlainText,
    Pdf,
}

impl FileType {
    /// `.md` and `.markdown` are Markdown. `.pdf` is PDF. Every other name is plain text.
    pub fn from_filename(name: &str) -> Self {
        let lower = name.to_lowercase();
        if lower.ends_with(".pdf") {
            FileType::Pdf
        } else if lower.ends_with(".md") || lower.ends_with(".markdown") {
            FileType::Markdown
        } else {
            FileType::PlainText
        }
    }
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
    pub marked_range: Option<Range<usize>>,
    pub revision: u64,
    cached_parse: Option<(u64, Arc<ParsedDocument>)>,
    history: UndoHistory,
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
            marked_range: None,
            revision: 1,
            cached_parse: None,
            history: UndoHistory::default(),
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

    #[allow(dead_code)]
    pub fn parsed_snapshot(&self) -> Arc<ParsedDocument> {
        if let Some((rev, ref doc)) = self.cached_parse {
            if rev == self.revision {
                return doc.clone();
            }
        }
        Arc::new(parse_document(&self.content))
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
        if self.is_read_only() {
            return;
        }
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
        self.history.push(DiffEdit {
            start: range.start,
            deleted,
            inserted: new_text.to_string(),
        });
        let cursor = range.start + new_text.len();
        self.content.replace_range(range, new_text);
        self.revision = self.revision.wrapping_add(1);
        self.caret.move_to(cursor.min(self.content.len()));
        self.marked_range = None;
        self.is_modified = true;
        self.refresh_stats();
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

        let prev_line = self.content[prev_line_start..current_line_start - 1].to_string();
        let curr_line = self.content[current_line_start..current_line_end].to_string();
        let curr_len = curr_line.len();

        let new_text = format!("{}\n{}", curr_line, prev_line);
        self.edit(prev_line_start..current_line_end, &new_text);

        let new_cursor = (prev_line_start + col).min(prev_line_start + curr_len);
        self.caret.move_to(new_cursor);
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
        let next_line_start = current_line_end + 1;
        let next_line_end = super::text_ops::line_end(&self.content, next_line_start);

        let curr_line = self.content[current_line_start..current_line_end].to_string();
        let next_line = self.content[next_line_start..next_line_end].to_string();
        let curr_len = curr_line.len();
        let next_len = next_line.len();

        let new_text = format!("{}\n{}", next_line, curr_line);
        self.edit(current_line_start..next_line_end, &new_text);

        let new_line_start = current_line_start + next_len + 1;
        let new_cursor = (new_line_start + col).min(new_line_start + curr_len);
        self.caret.move_to(new_cursor);
        self.refresh_cursor();
    }

    pub fn smart_newline(&mut self, is_shift: bool) {
        if self.is_read_only() {
            return;
        }
        let range = self.selection_range();
        if is_shift {
            self.edit(range, "\n");
            return;
        }

        let cursor = range.start;
        let line_start = home_offset(&self.content, cursor);
        let line_end_pos = line_end(&self.content, cursor);

        // 0. Table rows: continuation or exit
        let parsed = self.parsed();
        if let Some(pline) = parsed.line_at_offset(cursor) {
            if let LineKind::TableRow { ref cells, is_header, .. } = pline.kind {
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
                            if let LineKind::TableRow { ref cells, is_header: false, .. } = first_body.kind {
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
                    // Empty row: remove this empty row, keeping header, and exit table to a new line below
                    let remove_start = if line_start > 0 && self.content.as_bytes()[line_start - 1] == b'\n' {
                        line_start - 1
                    } else {
                        line_start
                    };
                    let remove_end = line_end_pos;
                    let has_newline_after = remove_end < self.content.len() && self.content.as_bytes()[remove_end] == b'\n';
                    let replacement = if has_newline_after { "" } else { "\n" };
                    self.edit(remove_start..remove_end, replacement);
                    let target = (remove_start + 1).min(self.content.len());
                    self.caret.move_to(floor_boundary(&self.content, target));
                    self.refresh_cursor();
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
                                let before = &self.content[cell_start..range.start];
                                let after = &self.content[range.end..cell_end];
                                if !after.trim().is_empty() {
                                    split_cell_info = Some((k, cell_start, cell_end, before.to_string(), after.trim().to_string()));
                                }
                                break;
                            }
                        }
                    }

                    if let Some((col_idx, _c_start, c_end, before_str, after_str)) = split_cell_info {
                        // Split the cell into current row and a new row below in a single undo step
                        let repl = if before_str.ends_with(' ') { "" } else { " " };
                        let rest_of_line = &self.content[c_end..line_end_pos];
                        let mut new_cells = Vec::new();
                        for i in 0..col_count {
                            if i == col_idx {
                                new_cells.push(format!(" {} ", after_str));
                            } else {
                                new_cells.push("  ".to_string());
                            }
                        }
                        let new_row = format!("\n|{}|", new_cells.join("|"));
                        let combined = format!("{}{}{}", repl, rest_of_line, new_row);
                        let new_row_start = range.start + repl.len() + rest_of_line.len() + 1;
                        self.edit(range.start..line_end_pos, &combined);

                        let mut target = new_row_start + 1;
                        for cell in new_cells.iter().take(col_idx) {
                            target += cell.len() + 1;
                        }
                        target += 1;
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

        let line = &self.content[line_start..line_end_pos];
        let trimmed_start = line.trim_start();
        let indent_len = line.len() - trimmed_start.len();
        let indent = &line[..indent_len];

        // 1. Task lists: "- [ ] ", "- [x] ", etc.
        let task_prefixes = [
            "- [ ] ", "- [x] ", "- [X] ", "* [ ] ", "* [x] ", "* [X] ", "+ [ ] ", "+ [x] ", "+ [X] ",
        ];
        if let Some(&prefix) = task_prefixes.iter().find(|&&p| trimmed_start.starts_with(p)) {
            let is_empty_task = trimmed_start.len() == prefix.len()
                || (trimmed_start.len() > prefix.len() && trimmed_start[prefix.len()..].trim().is_empty());
            if is_empty_task {
                if !range.is_empty() {
                    self.edit(range, "\n");
                    return;
                }
                if !indent.is_empty() {
                    let outdent_len = if indent.ends_with('\t') || indent.len() < 2 { 1 } else { 2 };
                    self.edit(line_start..line_start + outdent_len, "");
                } else {
                    self.edit(line_start..line_end_pos, "");
                }
            } else {
                let bullet_char = prefix.chars().next().unwrap_or('-');
                let ins = format!("\n{}{} [ ] ", indent, bullet_char);
                self.edit(range, &ins);
            }
            return;
        }

        // 2. Bullet lists: "- ", "* ", "+ "
        let bullet_prefixes = ["- ", "* ", "+ "];
        if let Some(&prefix) = bullet_prefixes.iter().find(|&&p| trimmed_start.starts_with(p)) {
            let is_empty_bullet = trimmed_start.len() == prefix.len()
                || (trimmed_start.len() > prefix.len() && trimmed_start[prefix.len()..].trim().is_empty());
            if is_empty_bullet {
                if !range.is_empty() {
                    self.edit(range, "\n");
                    return;
                }
                if !indent.is_empty() {
                    let outdent_len = if indent.ends_with('\t') || indent.len() < 2 { 1 } else { 2 };
                    self.edit(line_start..line_start + outdent_len, "");
                } else {
                    self.edit(line_start..line_end_pos, "");
                }
            } else {
                let ins = format!("\n{}{}", indent, prefix);
                self.edit(range, &ins);
            }
            return;
        }

        // 3. Numbered lists: digits + ". "
        if let Some(dot_pos) = trimmed_start.find(". ") {
            if dot_pos > 0 && trimmed_start[..dot_pos].chars().all(|c| c.is_ascii_digit()) {
                if let Ok(num) = trimmed_start[..dot_pos].parse::<usize>() {
                    let marker_len = dot_pos + 2;
                    let is_empty_num = trimmed_start.len() == marker_len
                        || (trimmed_start.len() > marker_len && trimmed_start[marker_len..].trim().is_empty());
                    if is_empty_num {
                        if !range.is_empty() {
                            self.edit(range, "\n");
                            return;
                        }
                        if !indent.is_empty() {
                            let outdent_len = if indent.ends_with('\t') || indent.len() < 2 { 1 } else { 2 };
                            self.edit(line_start..line_start + outdent_len, "");
                        } else {
                            self.edit(line_start..line_end_pos, "");
                        }
                    } else {
                        let ins = format!("\n{}{}. ", indent, num + 1);
                        self.edit(range, &ins);
                    }
                    return;
                }
            }
        }

        // 4. Blockquotes: "> "
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
            } else {
                let ins = format!("\n{}> ", indent);
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
            self.edit(range, "\n");
            return;
        }

        // 6. Indented code/text: maintain indentation
        if !indent.is_empty() && !trimmed_start.is_empty() {
            let ins = format!("\n{}", indent);
            self.edit(range, &ins);
            return;
        }

        // 7. Default newline
        self.edit(range, "\n");
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

        // 1. Auto-pairing bracket/quote deletion: (|), [|], {|}, "|", '|', `|`
        if cursor < self.content.len() {
            let prev_char = self.content[..cursor].chars().next_back().unwrap();
            let next_char = self.content[cursor..].chars().next().unwrap();
            let is_paired = matches!(
                (prev_char, next_char),
                ('(', ')') | ('[', ']') | ('{', '}') | ('"', '"') | ('\'', '\'') | ('`', '`')
            );
            if is_paired {
                let p_len = prev_char.len_utf8();
                let n_len = next_char.len_utf8();
                self.edit(cursor - p_len..cursor + n_len, "");
                return true;
            }
        }

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

        let trimmed = line_before.trim_start();
        let indent_len = line_before.len() - trimmed.len();
        let indent = &line_before[..indent_len];

        // 3. Task list prefix at cursor boundary
        for prefix in [
            "- [ ] ", "- [x] ", "- [X] ", "* [ ] ", "* [x] ", "* [X] ", "+ [ ] ", "+ [x] ", "+ [X] ",
        ] {
            if trimmed == prefix {
                let start = cursor - 4;
                self.edit(start..cursor, "");
                return true;
            }
        }

        // 4. Bullet list prefix at cursor boundary
        for prefix in ["- ", "* ", "+ "] {
            if trimmed == prefix {
                if !indent.is_empty() {
                    let outdent_len = if indent.ends_with('\t') || indent.len() < 2 { 1 } else { 2 };
                    self.edit(line_start..line_start + outdent_len, "");
                } else {
                    self.edit(line_start..cursor, "");
                }
                return true;
            }
        }

        // 5. Numbered list prefix at cursor boundary
        if let Some(dot_pos) = trimmed.find(". ") {
            if dot_pos > 0
                && trimmed[..dot_pos].chars().all(|c| c.is_ascii_digit())
                && dot_pos + 2 == trimmed.len()
            {
                if !indent.is_empty() {
                    let outdent_len = if indent.ends_with('\t') || indent.len() < 2 { 1 } else { 2 };
                    self.edit(line_start..line_start + outdent_len, "");
                } else {
                    self.edit(line_start..cursor, "");
                }
                return true;
            }
        }

        // 6. Blockquote prefix at cursor boundary
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

    pub fn indent(&mut self) {
        if self.is_read_only() {
            return;
        }
        let range = self.selection_range();
        let start_line = home_offset(&self.content, range.start);
        let end_line = line_end(&self.content, range.end);

        let lines_str = self.content[start_line..end_line].to_string();
        let mut modified = Vec::new();
        for line in lines_str.split('\n') {
            modified.push(format!("  {}", line));
        }
        let replacement = modified.join("\n");
        let added_len = replacement.len() - lines_str.len();
        self.edit(start_line..end_line, &replacement);
        if range.is_empty() {
            self.caret.move_to(range.start + 2);
        } else {
            self.caret.range = (range.start + 2)..(range.end + added_len);
        }
        self.refresh_cursor();
    }

    pub fn outdent(&mut self) {
        if self.is_read_only() {
            return;
        }
        let range = self.selection_range();
        let start_line = home_offset(&self.content, range.start);
        let end_line = line_end(&self.content, range.end);

        let lines_str = self.content[start_line..end_line].to_string();
        let mut modified = Vec::new();
        let mut first_line_stripped = 0;
        for (i, line) in lines_str.split('\n').enumerate() {
            let stripped = if let Some(s) = line.strip_prefix("  ") {
                s
            } else if let Some(s) = line.strip_prefix(' ') {
                s
            } else if let Some(s) = line.strip_prefix('\t') {
                s
            } else {
                line
            };
            if i == 0 {
                first_line_stripped = line.len() - stripped.len();
            }
            modified.push(stripped.to_string());
        }
        let replacement = modified.join("\n");
        let removed_total = lines_str.len() - replacement.len();
        self.edit(start_line..end_line, &replacement);
        if range.is_empty() {
            let new_cursor = range.start.saturating_sub(first_line_stripped).max(start_line);
            self.caret.move_to(new_cursor);
        } else {
            let new_start = range.start.saturating_sub(first_line_stripped).max(start_line);
            let new_end = range.end.saturating_sub(removed_total).max(new_start);
            self.caret.range = new_start..new_end;
        }
        self.refresh_cursor();
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn apply_format(&mut self, cmd: &str) {
        self.apply_format_in_mode(cmd, ViewMode::Raw);
    }

    pub fn apply_format_in_mode(&mut self, cmd: &str, mode: ViewMode) {
        if self.is_read_only() {
            return;
        }

        // 1. Block-level formats: h1, h2, h3, ul, ol, checklist, quote
        let block_target = match cmd {
            "h1" => Some(BlockFormatTarget::Heading(1)),
            "h2" => Some(BlockFormatTarget::Heading(2)),
            "h3" => Some(BlockFormatTarget::Heading(3)),
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

            // Check if the cursor or selection is inside or wrapping an existing InlinePair of `tag_kind`
            if let Some(pair) = parsed
                .inline_pairs
                .iter()
                .rev()
                .find(|p| {
                    p.kind == tag_kind
                        && ((range.start >= p.open_delim.start && range.end <= p.close_delim.end)
                            || (range.start <= p.open_delim.start && range.end >= p.close_delim.end))
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
                let new_start = pair.open_delim.start;
                let new_end = new_start + inner_text.len();
                self.edit(edit_range, &inner_text);
                if range.is_empty() {
                    let rel = range.start.saturating_sub(pair.inner.start).min(inner_text.len());
                    self.caret.move_to(new_start + rel);
                } else {
                    self.caret.range = new_start..new_end;
                    self.caret.reversed = false;
                }
                self.pending_formats = ActiveFormats::default();
                self.refresh_cursor();
                return;
            }

            // If selection is empty, check if cursor is on/inside a word or enclosed by raw delimiters
            if range.is_empty() {
                let cursor = range.start;
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

                let word_start = super::text_ops::word_left(&self.content, cursor);
                let word_end = super::text_ops::word_right(&self.content, cursor);

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

                // Expand to word only if cursor is touching/inside an alphanumeric word on the same line
                let line_s = home_offset(&self.content, cursor);
                let line_e = line_end(&self.content, cursor);
                let w_start = word_start.max(line_s);
                let w_end = word_end.min(line_e);
                let touching_word = (cursor > w_start
                    && self.content[..cursor]
                        .chars()
                        .next_back()
                        .is_some_and(|c| c.is_alphanumeric() || c == '_'))
                    || (cursor < w_end
                        && self.content[cursor..]
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_alphanumeric() || c == '_'));
                if touching_word && w_start < w_end {
                    let extent = super::text_ops::word_extent(&self.content, cursor);
                    if self.content[extent.clone()]
                        .chars()
                        .any(|c| c.is_alphanumeric() || c == '_')
                    {
                        range = extent;
                    }
                }
            }

            let selected = self.content[range.clone()].to_string();

            if selected.is_empty() {
                // In WYSIWYG mode, toggling bold/italic/underline/strike/code at an empty cursor toggles pending_formats!
                if mode == ViewMode::Wysiwyg && tag_kind != InlineTagKind::Link {
                    match tag_kind {
                        InlineTagKind::Bold => self.pending_formats.bold = !self.pending_formats.bold,
                        InlineTagKind::Italic => self.pending_formats.italic = !self.pending_formats.italic,
                        InlineTagKind::Underline => self.pending_formats.underline = !self.pending_formats.underline,
                        InlineTagKind::Strike => self.pending_formats.strike = !self.pending_formats.strike,
                        InlineTagKind::Code => self.pending_formats.code = !self.pending_formats.code,
                        InlineTagKind::Link => {}
                    }
                    self.refresh_cursor();
                    return;
                }

                // In Raw mode (or for Link), insert placeholder and select inner placeholder so a second press unwraps/removes it
                self.edit(range.clone(), &format!("{}{}{}", prefix, placeholder, suffix));
                let start = floor_boundary(&self.content, range.start + prefix.len());
                let end = floor_boundary(&self.content, start + placeholder.len());
                self.caret.range = start..end;
                self.caret.reversed = false;
                self.refresh_cursor();
                return;
            }

            // Non-empty text: trim whitespace to place delimiters tightly around text
            let lead_len = selected.len() - selected.trim_start().len();
            let trail_len = selected.len() - selected.trim_end().len();

            if lead_len + trail_len < selected.len() {
                let leading = &selected[..lead_len];
                let core = &selected[lead_len..selected.len() - trail_len];
                let trailing = &selected[selected.len() - trail_len..];

                let core_wrapped = is_delimited(core, prefix, suffix);
                let core_start = range.start + lead_len;
                let core_end = range.end - trail_len;
                let outer_wrapped =
                    outer_is_delimited(&self.content, core_start, core_end, prefix, suffix);

                if core_wrapped {
                    let unwrapped = &core[prefix.len()..core.len() - suffix.len()];
                    let replacement = format!("{}{}{}", leading, unwrapped, trailing);
                    let new_start = range.start + lead_len;
                    let new_end = new_start + unwrapped.len();
                    self.edit(range, &replacement);
                    self.caret.range = new_start..new_end;
                    self.caret.reversed = false;
                    self.refresh_cursor();
                    return;
                } else if outer_wrapped {
                    let edit_range = (core_start - prefix.len())..(core_end + suffix.len());
                    let core_owned = core.to_string();
                    let new_start = core_start - prefix.len();
                    let new_end = new_start + core_owned.len();
                    self.edit(edit_range, &core_owned);
                    self.caret.range = new_start..new_end;
                    self.caret.reversed = false;
                    self.refresh_cursor();
                    return;
                } else {
                    let replacement = format!("{}{}{}{}{}", leading, prefix, core, suffix, trailing);
                    let inner_start = range.start + lead_len + prefix.len();
                    let inner_end = inner_start + core.len();
                    self.edit(range, &replacement);
                    self.caret.range = inner_start..inner_end;
                    self.caret.reversed = false;
                    self.refresh_cursor();
                    return;
                }
            }
        }

        if cmd == "table" {
            let range = self.selection_range();
            let parsed = self.parsed();

            // Check if cursor is currently inside an existing table
            let in_table = parsed.lines.iter().find(|l| {
                if let LineKind::TableRow { ref table_source, .. } = l.kind {
                    range.start >= table_source.start && range.start <= table_source.end
                } else {
                    false
                }
            });

            if let Some(table_line) = in_table {
                if let LineKind::TableRow { ref cells, ref table_source, .. } = table_line.kind {
                    let col_count = cells.len().max(2);
                    let row_str = format!("\n|{}", "  |".repeat(col_count));
                    let insert_pos = table_source.end;
                    self.edit(insert_pos..insert_pos, &row_str);
                    let target_pos = (insert_pos + 3).min(self.content.len());
                    self.caret.move_to(floor_boundary(&self.content, target_pos));
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

    fn toggle_block_lines(&mut self, target: BlockFormatTarget) {
        let range = self.selection_range();
        let start_line = home_offset(&self.content, range.start);
        let end_line = line_end(&self.content, range.end);
        let lines_str = &self.content[start_line..end_line];
        let split_lines: Vec<&str> = lines_str.split('\n').collect();

        // Check if all non-empty lines (or the single line) already match `target`
        let all_match = split_lines
            .iter()
            .filter(|l| !l.trim().is_empty() || split_lines.len() == 1)
            .all(|line| {
                let (_, existing, _) = parse_line_block_prefix(line);
                existing == Some(target)
            });

        let mut transformed = Vec::with_capacity(split_lines.len());
        let mut first_delta: isize = 0;
        let mut total_delta: isize = 0;

        for (idx, line) in split_lines.iter().enumerate() {
            let (indent, _existing, prefix_end) = parse_line_block_prefix(line);
            let leading_spaces = if matches!(target, BlockFormatTarget::Heading(_)) {
                &line[..indent.min(3)]
            } else {
                &line[..indent]
            };
            let body = &line[prefix_end..];
            let new_line = if all_match {
                format!("{}{}", leading_spaces, body)
            } else {
                let new_prefix = match target {
                    BlockFormatTarget::Heading(1) => "# ".to_string(),
                    BlockFormatTarget::Heading(2) => "## ".to_string(),
                    BlockFormatTarget::Heading(_) => "### ".to_string(),
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
            let new_cursor = (range.start as isize + first_delta)
                .max(start_line as isize) as usize;
            self.caret.move_to(floor_boundary(&self.content, new_cursor));
        } else {
            let new_start = (range.start as isize + first_delta)
                .max(start_line as isize) as usize;
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
            .find(|(open_r, close_r)| {
                range.start >= open_r.start
                    && range.end <= close_r.end
                    && (!close_r.is_empty() || range.start < close_r.end)
            })
            .cloned()
        {
            if close_fence.is_empty() {
                let inner = self.content[open_fence.end..close_fence.end].to_string();
                self.edit(open_fence.start..close_fence.end, &inner);
                self.caret.move_to(floor_boundary(
                    &self.content,
                    open_fence.start + inner.len(),
                ));
                self.refresh_cursor();
                return;
            }
            let inner_start = open_fence.end.min(close_fence.start);
            let inner_end = close_fence.start;
            let mut inner = self.content[inner_start..inner_end].to_string();
            if inner.ends_with('\n') && !close_fence.is_empty() {
                inner.pop();
            }
            let full_end = close_fence.end;
            let trailing_nl = if full_end <= self.content.len()
                && self.content[..full_end].ends_with('\n')
            {
                "\n"
            } else {
                ""
            };
            let replacement = format!("{}{}", inner, trailing_nl);
            self.edit(open_fence.start..full_end, &replacement);
            self.caret.move_to(floor_boundary(
                &self.content,
                open_fence.start + inner.len(),
            ));
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

        if selected.is_empty() {
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
            let wrapped = format!("```\n{}\n```", selected);
            let inner_start = range.start + 4;
            let inner_end = inner_start + selected.len();
            self.edit(range, &wrapped);
            self.caret.range = inner_start..inner_end;
            self.caret.reversed = false;
            self.refresh_cursor();
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
        if mode == ViewMode::Wysiwyg && range.is_empty() && !text.is_empty() && text != "\n" {
            let pf = self.pending_formats;
            if pf.bold || pf.italic || pf.underline || pf.strike || pf.code {
                let mut open = String::new();
                let mut close = String::new();
                if pf.bold {
                    open.push_str("**");
                    close.insert_str(0, "**");
                }
                if pf.italic {
                    open.push('*');
                    close.insert(0, '*');
                }
                if pf.underline {
                    open.push_str("<u>");
                    close.insert_str(0, "</u>");
                }
                if pf.strike {
                    open.push_str("~~");
                    close.insert_str(0, "~~");
                }
                if pf.code {
                    open.push('`');
                    close.insert(0, '`');
                }
                let wrapped = format!("{}{}{}", open, text, close);
                let cursor_after = range.start + open.len() + text.len();
                self.pending_formats = ActiveFormats::default();
                self.edit(range, &wrapped);
                self.caret.move_to(floor_boundary(&self.content, cursor_after));
                self.refresh_cursor();
                return;
            }
        }
        self.pending_formats = ActiveFormats::default();
        self.edit(range, text);
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
            previous_grapheme(&self.content, self.cursor_offset())
        };
        self.place(target, extend);
    }

    pub fn move_right(&mut self, extend: bool) {
        self.pending_formats = ActiveFormats::default();
        let target = if !extend && !self.caret.range.is_empty() {
            self.selection_range().end
        } else {
            next_grapheme(&self.content, self.cursor_offset())
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
            if let LineKind::TableRow {
                ref cells,
                ref raw_cells,
                ref table_source,
                ..
            } = pline.kind
            {
                if let Some(c_idx) = raw_cells
                    .iter()
                    .position(|rc| pos >= rc.start && pos <= rc.end)
                {
                    if pos <= cells[c_idx].start {
                        if c_idx > 0 {
                            let target = skip_hidden_left(&parsed, cells[c_idx - 1].end);
                            self.place(target, extend);
                        } else {
                            let prev_row_cell_end = parsed.lines[..pline.line_index]
                                .iter()
                                .rev()
                                .find_map(|l| {
                                    if let LineKind::TableRow {
                                        cells: ref pcells,
                                        table_source: ref pts,
                                        ..
                                    } = l.kind
                                    {
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
                    let prev_end = pline.source.start - 1;
                    let snapped = skip_hidden_left(&parsed, prev_end);
                    self.place(snapped, extend);
                } else {
                    self.place(0, extend);
                }
                return;
            }
        }
        pos = skip_hidden_left(&parsed, pos);
        pos = previous_grapheme(&self.content, pos);
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
            if let LineKind::TableRow {
                ref cells,
                ref raw_cells,
                ref table_source,
                ..
            } = pline.kind
            {
                if let Some(c_idx) = raw_cells
                    .iter()
                    .position(|rc| pos >= rc.start && pos <= rc.end)
                {
                    if pos >= cells[c_idx].end {
                        if c_idx + 1 < cells.len() {
                            let target = skip_hidden_right(&parsed, cells[c_idx + 1].start);
                            self.place(target, extend);
                        } else {
                            let next_row_cell_start = parsed.lines
                                .get(pline.line_index + 1..)
                                .and_then(|rest| {
                                    rest.iter().find_map(|l| {
                                        if let LineKind::TableRow {
                                            cells: ref ncells,
                                            table_source: ref nts,
                                            ..
                                        } = l.kind
                                        {
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
        }
        pos = skip_hidden_right(&parsed, pos);
        if pos >= self.content.len() {
            self.place(self.content.len(), extend);
            return;
        }
        if self.content.as_bytes().get(pos) == Some(&b'\n') {
            let next_line_start = pos + 1;
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
            if let LineKind::TableRow {
                ref cells,
                ref raw_cells,
                ..
            } = pline.kind
            {
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
        for pair in parsed.inline_pairs.iter().rev() {
            let prev_vis = if cursor == pair.close_delim.end {
                pair.inner.end
            } else {
                cursor
            };
            if prev_vis > pair.inner.start && prev_vis <= pair.inner.end {
                let prev_g = previous_grapheme(&self.content, prev_vis);
                if prev_g <= pair.inner.start && prev_vis == pair.inner.end {
                    // Only 1 visible grapheme inside this formatted span! Delete the whole span + delimiters!
                    let start = pair.open_delim.start;
                    let end = pair.close_delim.end;
                    self.edit(start..end, "");
                    self.caret.move_to(start);
                    self.refresh_cursor();
                    return true;
                } else if cursor == pair.close_delim.end {
                    // Cursor was right outside `**word**|` -> delete the last visible grapheme inside `word`
                    let close_len = pair.close_delim.len();
                    self.edit(prev_g..prev_vis, "");
                    self.caret
                        .move_to(floor_boundary(&self.content, prev_g + close_len));
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
                if let LineKind::TableRow {
                    ref cells,
                    ref raw_cells,
                    ..
                } = pline.kind
                {
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
            LineKind::TableRow {
                ref table_source, ..
            } => cursor >= table_source.start && cursor <= table_source.end,
            _ => false,
        })
    }

    fn extract_active_table(&mut self) -> Option<TableEditContext> {
        let cursor = self.cursor_offset();
        let parsed = self.parsed();
        let table_source = parsed.lines.iter().find_map(|l| {
            if let LineKind::TableRow {
                ref table_source, ..
            } = l.kind
            {
                if cursor >= table_source.start && cursor <= table_source.end {
                    return Some(table_source.clone());
                }
            }
            None
        })?;

        let mut num_cols = 1usize;
        let mut alignments = Vec::new();
        for l in &parsed.lines {
            if l.source.start >= table_source.start && l.source.end <= table_source.end {
                if let LineKind::TableRow {
                    alignments: ref al, ..
                } = l.kind
                {
                    num_cols = num_cols.max(al.len());
                    if alignments.is_empty() {
                        alignments = al.clone();
                    }
                }
            }
        }
        while alignments.len() < num_cols {
            alignments.push(TableColumnAlign::None);
        }

        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut active_row = 0usize;
        let mut active_col = 0usize;

        for l in &parsed.lines {
            if l.source.start >= table_source.start && l.source.end <= table_source.end {
                if let LineKind::TableRow { ref raw_cells, .. } = l.kind
                {
                    let r_idx = rows.len();
                    let line_text = self.content.get(l.source.clone()).unwrap_or("");
                    let split_cells = split_table_cells(line_text);
                    let mut row_vals = Vec::with_capacity(num_cols);
                    for c_idx in 0..num_cols {
                        row_vals.push(split_cells.get(c_idx).cloned().unwrap_or_default());
                    }
                    if cursor >= l.source.start && cursor <= l.source.end {
                        active_row = r_idx;
                        if let Some(c_idx) = raw_cells
                            .iter()
                            .position(|rc| cursor >= rc.start && cursor <= rc.end)
                        {
                            active_col = c_idx.min(num_cols - 1);
                        } else if let Some(c_idx) = raw_cells.iter().rposition(|rc| rc.start <= cursor) {
                            active_col = c_idx.min(num_cols - 1);
                        }
                    }
                    rows.push(row_vals);
                }
            }
        }

        if rows.is_empty() {
            return None;
        }

        Some(TableEditContext {
            table_source,
            rows,
            alignments,
            active_row,
            active_col,
        })
    }

    pub fn insert_table_with_size(&mut self, cols: usize, rows: usize) {
        if self.is_read_only() {
            return;
        }
        let cols = cols.clamp(1, 10);
        let rows = rows.clamp(1, 20);

        let mut range = self.selection_range();
        let parsed = self.parsed();
        if let Some(table_end) = parsed.lines.iter().find_map(|l| {
            if let LineKind::TableRow {
                ref table_source, ..
            } = l.kind
            {
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
        if self.is_read_only() {
            return;
        }
        let Some(mut ctx) = self.extract_active_table() else {
            return;
        };
        let num_cols = ctx.alignments.len().max(1);
        let insert_idx = ctx.active_row.min(ctx.rows.len());
        ctx.rows.insert(insert_idx, vec![String::new(); num_cols]);
        let (new_table, cell_rel) =
            format_markdown_table(&ctx.rows, &ctx.alignments, insert_idx, ctx.active_col);
        let base = ctx.table_source.start;
        self.edit(ctx.table_source, &new_table);
        self.caret.move_to(floor_boundary(&self.content, base + cell_rel.start));
        self.refresh_cursor();
    }

    pub fn table_add_row_below(&mut self) {
        if self.is_read_only() {
            return;
        }
        let Some(mut ctx) = self.extract_active_table() else {
            return;
        };
        let num_cols = ctx.alignments.len().max(1);
        let insert_idx = (ctx.active_row + 1).min(ctx.rows.len());
        ctx.rows.insert(insert_idx, vec![String::new(); num_cols]);
        let (new_table, cell_rel) =
            format_markdown_table(&ctx.rows, &ctx.alignments, insert_idx, ctx.active_col);
        let base = ctx.table_source.start;
        self.edit(ctx.table_source, &new_table);
        self.caret.move_to(floor_boundary(&self.content, base + cell_rel.start));
        self.refresh_cursor();
    }

    pub fn table_add_col_left(&mut self) {
        if self.is_read_only() {
            return;
        }
        let Some(mut ctx) = self.extract_active_table() else {
            return;
        };
        let insert_col = ctx.active_col.min(ctx.alignments.len());
        ctx.alignments.insert(insert_col, TableColumnAlign::None);
        for row in ctx.rows.iter_mut() {
            row.insert(insert_col.min(row.len()), String::new());
        }
        let (new_table, cell_rel) =
            format_markdown_table(&ctx.rows, &ctx.alignments, ctx.active_row, insert_col);
        let base = ctx.table_source.start;
        self.edit(ctx.table_source, &new_table);
        self.caret.range = (base + cell_rel.start)..(base + cell_rel.end);
        self.caret.reversed = false;
        self.refresh_cursor();
    }

    pub fn table_add_col_right(&mut self) {
        if self.is_read_only() {
            return;
        }
        let Some(mut ctx) = self.extract_active_table() else {
            return;
        };
        let insert_col = (ctx.active_col + 1).min(ctx.alignments.len());
        ctx.alignments.insert(insert_col, TableColumnAlign::None);
        for row in ctx.rows.iter_mut() {
            row.insert(insert_col.min(row.len()), String::new());
        }
        let (new_table, cell_rel) =
            format_markdown_table(&ctx.rows, &ctx.alignments, ctx.active_row, insert_col);
        let base = ctx.table_source.start;
        self.edit(ctx.table_source, &new_table);
        self.caret.range = (base + cell_rel.start)..(base + cell_rel.end);
        self.caret.reversed = false;
        self.refresh_cursor();
    }

    pub fn table_delete_row(&mut self) {
        if self.is_read_only() {
            return;
        }
        let Some(mut ctx) = self.extract_active_table() else {
            return;
        };
        if ctx.rows.len() <= 1 {
            self.table_delete_table();
            return;
        }
        let remove_idx = ctx.active_row.min(ctx.rows.len() - 1);
        ctx.rows.remove(remove_idx);
        let target_row = remove_idx.min(ctx.rows.len() - 1);
        let (new_table, cell_rel) =
            format_markdown_table(&ctx.rows, &ctx.alignments, target_row, ctx.active_col);
        let base = ctx.table_source.start;
        self.edit(ctx.table_source, &new_table);
        self.caret.move_to(floor_boundary(&self.content, base + cell_rel.start));
        self.refresh_cursor();
    }

    pub fn table_delete_col(&mut self) {
        if self.is_read_only() {
            return;
        }
        let Some(mut ctx) = self.extract_active_table() else {
            return;
        };
        if ctx.alignments.len() <= 1 {
            self.table_delete_table();
            return;
        }
        let remove_col = ctx.active_col.min(ctx.alignments.len() - 1);
        ctx.alignments.remove(remove_col);
        for row in &mut ctx.rows {
            if remove_col < row.len() {
                row.remove(remove_col);
            }
        }
        let target_col = remove_col.min(ctx.alignments.len() - 1);
        let (new_table, cell_rel) =
            format_markdown_table(&ctx.rows, &ctx.alignments, ctx.active_row, target_col);
        let base = ctx.table_source.start;
        self.edit(ctx.table_source, &new_table);
        self.caret.move_to(floor_boundary(&self.content, base + cell_rel.start));
        self.refresh_cursor();
    }

    pub fn table_delete_table(&mut self) {
        if self.is_read_only() {
            return;
        }
        let Some(ctx) = self.extract_active_table() else {
            return;
        };
        let mut del_range = ctx.table_source.clone();
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
        if self.is_read_only() {
            return;
        }
        let Some(mut ctx) = self.extract_active_table() else {
            return;
        };
        if let Some(slot) = ctx.alignments.get_mut(ctx.active_col) {
            *slot = align;
        }
        let (new_table, cell_rel) =
            format_markdown_table(&ctx.rows, &ctx.alignments, ctx.active_row, ctx.active_col);
        let base = ctx.table_source.start;
        self.edit(ctx.table_source, &new_table);
        self.caret.move_to(floor_boundary(&self.content, base + cell_rel.start));
        self.refresh_cursor();
    }

    pub fn tab_table_cell(&mut self, backwards: bool) -> bool {
        let cursor = self.cursor_offset();
        let parsed = self.parsed();
        let mut all_cells: Vec<Range<usize>> = Vec::new();
        let mut in_table = false;
        let mut table_end = 0usize;
        let mut num_cols = 2usize;
        for line in &parsed.lines {
            if let LineKind::TableRow { ref cells, ref table_source, .. } = line.kind {
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
                all_cells.len() - 1
            } else {
                cur_idx - 1
            }
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
        move_vertical_collapsed(&self.content, &mut self.caret, -1, extend);
        self.refresh_cursor();
    }

    pub fn move_down(&mut self, extend: bool) {
        self.pending_formats = ActiveFormats::default();
        move_vertical_collapsed(&self.content, &mut self.caret, 1, extend);
        self.refresh_cursor();
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
        let cursor = self.cursor_offset();
        let parsed = self.parsed();
        if let Some(pline) = parsed.line_at_offset(cursor) {
            if let LineKind::TableRow {
                ref cells,
                ref raw_cells,
                ..
            } = pline.kind
            {
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
            let vis_start = skip_hidden_right(&parsed, pline.prefix_range.end).min(pline.source.end);
            self.place(vis_start, extend);
        } else {
            self.move_home(extend);
        }
    }

    pub fn move_end(&mut self, extend: bool) {
        self.pending_formats = ActiveFormats::default();
        let line_start = super::text_ops::home_offset(&self.content, self.cursor_offset());
        let target = super::text_ops::line_end(&self.content, line_start);
        self.place(target, extend);
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
        let current_line_start = super::text_ops::home_offset(&self.content, cursor);
        let current_line_end = super::text_ops::line_end(&self.content, current_line_start);
        let target = if cursor < current_line_end {
            current_line_end
        } else if current_line_end < self.content.len() {
            let next_line_start = current_line_end + 1;
            super::text_ops::line_end(&self.content, next_line_start)
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
        let target = if !extend && !self.caret.range.is_empty() {
            super::text_ops::word_right(&self.content, self.selection_range().end)
        } else {
            super::text_ops::word_right(&self.content, self.cursor_offset())
        };
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
            let target = super::text_ops::word_right(&self.content, cursor);
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
            if target < cursor {
                self.edit(target..cursor, "");
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
            let target = super::text_ops::line_end(&self.content, cursor);
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
        let offset = floor_boundary(&self.content, offset);
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
    }

    pub fn refresh_cursor(&mut self) {
        let cursor = self.cursor_offset();
        let (line, col) = line_col(&self.content, cursor);
        self.cursor_line = line;
        self.cursor_col = col;
        let parsed = self.parsed();
        self.active_formats = compute_active_formats(&parsed, cursor, &self.pending_formats);
    }

    pub fn undo(&mut self) -> bool {
        let Some(caret) = self.history.undo(&mut self.content) else {
            return false;
        };
        self.revision = self.revision.wrapping_add(1);
        self.pending_formats = ActiveFormats::default();
        self.place_after_history(caret);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(caret) = self.history.redo(&mut self.content) else {
            return false;
        };
        self.revision = self.revision.wrapping_add(1);
        self.pending_formats = ActiveFormats::default();
        self.place_after_history(caret);
        true
    }

    fn place_after_history(&mut self, caret: usize) {
        let caret = floor_boundary(&self.content, caret);
        self.caret.move_to(caret);
        self.caret.clamp(&self.content);
        self.marked_range = None;
        self.is_modified = true;
        self.refresh_stats();
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

#[derive(Debug, Clone)]
struct TableEditContext {
    table_source: Range<usize>,
    rows: Vec<Vec<String>>,
    alignments: Vec<TableColumnAlign>,
    active_row: usize,
    active_col: usize,
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

fn parse_line_block_prefix(line: &str) -> (usize, Option<BlockFormatTarget>, usize) {
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
    for pfx in [
        "- [ ] ", "- [x] ", "- [X] ", "* [ ] ", "* [x] ", "* [X] ", "+ [ ] ", "+ [x] ", "+ [X] ",
    ] {
        if trimmed.starts_with(pfx) {
            return (indent, Some(BlockFormatTarget::TaskList), indent + pfx.len());
        }
    }
    for pfx in ["- ", "* ", "+ "] {
        if trimmed.starts_with(pfx) {
            return (
                indent,
                Some(BlockFormatTarget::BulletList),
                indent + pfx.len(),
            );
        }
    }
    if let Some(dot) = trimmed.find(". ") {
        if dot > 0 && trimmed[..dot].chars().all(|c| c.is_ascii_digit()) {
            return (
                indent,
                Some(BlockFormatTarget::OrderedList),
                indent + dot + 2,
            );
        }
    }
    if trimmed.starts_with("> ") {
        return (indent, Some(BlockFormatTarget::Blockquote), indent + 2);
    }
    if trimmed == ">" {
        return (indent, Some(BlockFormatTarget::Blockquote), indent + 1);
    }
    (indent, None, indent)
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
            LineKind::Heading { level: 3.. } => fmt.h3 = true,
            LineKind::ListItem {
                ordered,
                task,
                ..
            } => {
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
    while let Some(h) = parsed
        .hidden_ranges
        .iter()
        .find(|r| pos > r.start && pos <= r.end)
    {
        pos = h.start;
    }
    pos
}

fn skip_hidden_interior_left(parsed: &ParsedDocument, mut pos: usize) -> usize {
    while let Some(h) = parsed
        .hidden_ranges
        .iter()
        .find(|r| pos > r.start && pos < r.end)
    {
        pos = h.start;
    }
    pos
}

fn skip_hidden_right(parsed: &ParsedDocument, mut pos: usize) -> usize {
    while let Some(h) = parsed
        .hidden_ranges
        .iter()
        .find(|r| pos >= r.start && pos < r.end)
    {
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

fn is_delimited(text: &str, prefix: &str, suffix: &str) -> bool {
    if text.len() < prefix.len() + suffix.len() {
        return false;
    }
    if !text.starts_with(prefix) || !text.ends_with(suffix) {
        return false;
    }
    if prefix == "*" {
        let triple = text.starts_with("***") && text.ends_with("***") && text.len() >= 6;
        let single = !text.starts_with("**") && !text.ends_with("**");
        triple || single
    } else {
        true
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
    } else {
        true
    }
}

pub fn block_prefix_len(line: &str) -> usize {
    let trimmed_start = line.trim_start();
    let indent_len = line.len() - trimmed_start.len();

    if indent_len <= 3 {
        for prefix in ["###### ", "##### ", "#### ", "### ", "## ", "# "] {
            if trimmed_start.starts_with(prefix) {
                return indent_len + prefix.len();
            }
        }
    }

    for prefix in [
        "- [ ] ", "- [x] ", "- [X] ", "* [ ] ", "* [x] ", "* [X] ", "+ [ ] ", "+ [x] ", "+ [X] ",
    ] {
        if trimmed_start.starts_with(prefix) {
            return indent_len + prefix.len();
        }
    }

    for prefix in ["- ", "* ", "+ "] {
        if trimmed_start.starts_with(prefix) {
            return indent_len + prefix.len();
        }
    }

    if trimmed_start.starts_with("> ") {
        return indent_len + 2;
    }

    if let Some(dot_pos) = trimmed_start.find(". ") {
        if dot_pos > 0 && trimmed_start[..dot_pos].chars().all(|c| c.is_ascii_digit()) {
            return indent_len + dot_pos + 2;
        }
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
mod tests {
    use super::*;

    #[test]
    fn test_compute_stats() {
        let text = "Hello world from LiteMark.\nSecond line with five words.";
        let stats = compute_stats(text);
        assert_eq!(stats.words, 9);
        assert_eq!(stats.lines, 2);
        assert_eq!(stats.chars, text.len());
        assert_eq!(stats.reading_time_min, 1);
    }

    #[test]
    fn test_undo_redo_stack() {
        let mut doc = DocumentTab::new(
            "tab-1".to_string(),
            "Doc.md".to_string(),
            "Initial".to_string(),
            None,
            None,
        );
        assert!(!doc.is_modified);

        doc.set_content("Version 2".to_string());
        assert!(doc.is_modified);
        assert_eq!(doc.content, "Version 2");

        assert!(doc.undo());
        assert_eq!(doc.content, "Initial");

        assert!(doc.redo());
        assert_eq!(doc.content, "Version 2");

        assert!(!doc.redo());
    }

    #[test]
    fn undo_history_is_diff_sized() {
        let mut doc = DocumentTab::new(
            "tab-1".into(),
            "Doc.md".into(),
            "x".repeat(2_000),
            None,
            None,
        );
        for _ in 0..80 {
            let end = doc.content.len();
            // A trailing space keeps each insert as its own undo step.
            doc.edit(end..end, "y ");
        }
        assert!(doc.undo_stored_bytes() < 2_000 * 80);
        assert_eq!(doc.content.len(), 2_160);
        assert!(doc.content.ends_with("y "));
        assert!(doc.undo());
        assert_eq!(doc.content.len(), 2_158);
        for _ in 0..79 {
            assert!(doc.undo());
        }
        assert_eq!(doc.content.chars().last(), Some('x'));
        assert!(!doc.undo());
    }

    #[test]
    fn a_typed_word_is_one_undo_and_replace_all_is_one_step() {
        let mut doc = DocumentTab::new("tab-1".into(), "Doc.md".into(), String::new(), None, None);
        for ch in ["h", "e", "l", "l", "o"] {
            let end = doc.content.len();
            doc.edit(end..end, ch);
        }
        assert_eq!(doc.content, "hello");
        assert!(doc.undo());
        assert_eq!(doc.content, "");
        assert!(!doc.undo());

        doc.edit(0..0, "hello");
        doc.edit(5..5, " ");
        doc.edit(6..6, "world");
        assert_eq!(doc.content, "hello world");
        assert!(doc.undo());
        assert_eq!(doc.content, "hello ");
        assert!(doc.undo());
        assert_eq!(doc.content, "hello");

        doc.edit(0..5, "cat cat");
        assert!(doc.replace_every_match("cat", "dog", true));
        assert_eq!(doc.content, "dog dog");
        assert!(doc.undo());
        assert_eq!(doc.content, "cat cat");
    }

    #[test]
    fn formatting_uses_the_selection_and_clear_keeps_markers() {
        let mut doc = DocumentTab::new(
            "tab-1".into(),
            "Doc.md".into(),
            "Hello world".into(),
            None,
            None,
        );
        doc.caret.range = 6..11;
        doc.apply_format("bold");
        assert_eq!(doc.content, "Hello **world**");
        assert!(!doc.content.ends_with("**bold text**") || doc.content.contains("**world**"));

        let mut doc = DocumentTab::new("tab-1".into(), "Doc.md".into(), "abc".into(), None, None);
        doc.caret.move_to(1);
        doc.apply_format("bold");
        assert_eq!(doc.content, "**abc**");
        // Toggling bold again strips delimiters
        doc.apply_format("bold");
        assert_eq!(doc.content, "abc");

        let mut empty_doc = DocumentTab::new("tab-1".into(), "Doc.md".into(), "".into(), None, None);
        empty_doc.apply_format("bold");
        assert_eq!(empty_doc.content, "**bold text**");

        let source = "- item *keep*\n* bullet `code`";
        let mut doc = DocumentTab::new("tab-1".into(), "Doc.md".into(), source.into(), None, None);
        doc.select_all();
        doc.apply_format("clear");
        assert_eq!(doc.content, "- item keep\n* bullet code");
    }

    #[test]
    fn move_line_up_and_down_and_undo() {
        let mut doc = DocumentTab::new(
            "tab-1".into(),
            "Doc.md".into(),
            "First line\nSecond line\nThird line".into(),
            None,
            None,
        );
        // Cursor on Second line
        doc.caret.move_to(15);
        doc.move_line_up();
        assert_eq!(doc.content, "Second line\nFirst line\nThird line");
        assert!(doc.undo());
        assert_eq!(doc.content, "First line\nSecond line\nThird line");

        // Move down
        doc.caret.move_to(15);
        doc.move_line_down();
        assert_eq!(doc.content, "First line\nThird line\nSecond line");
        assert!(doc.undo());
        assert_eq!(doc.content, "First line\nSecond line\nThird line");

        // Edge case: top line cannot move up
        doc.caret.move_to(2);
        doc.move_line_up();
        assert_eq!(doc.content, "First line\nSecond line\nThird line");
    }

    #[test]
    fn select_line_up_and_down_multiline() {
        let text = "Line one\nLine two\nLine three";
        let mut doc = DocumentTab::new(
            "tab-1".into(),
            "Doc.md".into(),
            text.into(),
            None,
            None,
        );
        // "Line one" is 0..8, newline at 8
        // "Line two" is 9..17, newline at 17
        // "Line three" is 18..28
        // Place cursor in the middle of Line two (index 13: "Line |two")
        doc.caret.move_to(13);

        // First Ctrl+Shift+Up selects to beginning of Line two (index 9)
        doc.select_line_up();
        assert_eq!(doc.selection_range(), 9..13);
        assert_eq!(doc.selected_text(), "Line");

        // Second Ctrl+Shift+Up extends beyond to beginning of Line one (index 0)
        doc.select_line_up();
        assert_eq!(doc.selection_range(), 0..13);
        assert_eq!(doc.selected_text(), "Line one\nLine");

        // Third Ctrl+Shift+Up stays at 0 (start of doc)
        doc.select_line_up();
        assert_eq!(doc.selection_range(), 0..13);

        // Now test select_line_down:
        doc.caret.move_to(13);
        // First Ctrl+Shift+Down selects to end of Line two (index 17)
        doc.select_line_down();
        assert_eq!(doc.selection_range(), 13..17);
        assert_eq!(doc.selected_text(), " two");

        // Second Ctrl+Shift+Down extends beyond to end of Line three (index 28)
        doc.select_line_down();
        assert_eq!(doc.selection_range(), 13..28);
        assert_eq!(doc.selected_text(), " two\nLine three");

        // Third Ctrl+Shift+Down stays at doc end
        doc.select_line_down();
        assert_eq!(doc.selection_range(), 13..28);
    }

    #[test]
    fn doc_start_and_end_navigation() {
        let mut doc = DocumentTab::new(
            "tab-1".into(),
            "Doc.md".into(),
            "Alpha\nBeta\nGamma".into(),
            None,
            None,
        );
        doc.caret.move_to(7); // In "Beta"
        doc.move_doc_start(false);
        assert_eq!(doc.cursor_offset(), 0);
        assert!(doc.caret.range.is_empty());

        doc.caret.move_to(7);
        doc.move_doc_start(true);
        assert_eq!(doc.selection_range(), 0..7);

        doc.caret.move_to(7);
        doc.move_doc_end(false);
        assert_eq!(doc.cursor_offset(), 16);
        assert!(doc.caret.range.is_empty());

        doc.caret.move_to(7);
        doc.move_doc_end(true);
        assert_eq!(doc.selection_range(), 7..16);
    }

    #[test]
    fn heading_toggle_and_level_switching() {
        let mut doc = DocumentTab::new(
            "tab-1".into(),
            "Doc.md".into(),
            "My heading\nParagraph".into(),
            None,
            None,
        );
        doc.caret.move_to(3);
        // Apply H1
        doc.apply_format("h1");
        assert_eq!(doc.content, "# My heading\nParagraph");

        // Switch to H2 (should not stack ## #)
        doc.apply_format("h2");
        assert_eq!(doc.content, "## My heading\nParagraph");

        // Toggle H2 off
        doc.apply_format("h2");
        assert_eq!(doc.content, "My heading\nParagraph");
    }

    #[test]
    fn inline_formatting_whitespace_trimming() {
        let mut doc = DocumentTab::new(
            "tab-1".into(),
            "Doc.md".into(),
            "leading  RAM usage  trailing".into(),
            None,
            None,
        );
        // Select "  RAM usage  " (includes leading and trailing spaces)
        doc.caret.range = 7..20;
        doc.apply_format("bold");
        assert_eq!(doc.content, "leading  **RAM usage**  trailing");

        // Toggle off
        doc.caret.range = 7..24;
        doc.apply_format("bold");
        assert_eq!(doc.content, "leading  RAM usage  trailing");
    }

    #[test]
    fn stacking_bold_and_italic_formatting() {
        let mut doc = DocumentTab::new(
            "tab-1".into(),
            "Doc.md".into(),
            "hello world".into(),
            None,
            None,
        );
        doc.caret.range = 6..11; // select "world"
        doc.apply_format("bold");
        assert_eq!(doc.content, "hello **world**");

        // Stack italic on top
        doc.apply_format("italic");
        assert_eq!(doc.content, "hello ***world***");

        // Toggle bold off
        doc.apply_format("bold");
        assert_eq!(doc.content, "hello *world*");

        // Toggle italic off
        doc.apply_format("italic");
        assert_eq!(doc.content, "hello world");
    }

    #[test]
    fn pdf_edits_are_ignored() {
        let mut doc = DocumentTab::new(
            "tab-1".into(),
            "scan.pdf".into(),
            "extracted".into(),
            None,
            None,
        );
        assert!(doc.is_read_only());
        doc.edit(0..9, "overwrite");
        assert_eq!(doc.content, "extracted");
    }

    #[test]
    fn find_replace_moves_through_matches() {
        let mut doc = DocumentTab::new(
            "tab-1".into(),
            "Doc.md".into(),
            "cat cat cat".into(),
            None,
            None,
        );
        let first = doc.select_match("cat", true, None, true).unwrap();
        assert_eq!(first, 0);
        assert_eq!(doc.selected_text(), "cat");
        let second = doc.select_match("cat", true, Some(first), true).unwrap();
        assert_eq!(second, 1);
        doc.replace_every_match("cat", "dog", true);
        assert_eq!(doc.content, "dog dog dog");
        assert!(doc.undo());
        assert_eq!(doc.content, "cat cat cat");
    }

    #[test]
    fn undo_redo_caret_stays_on_a_char_boundary() {
        let mut doc = DocumentTab::new("tab-1".into(), "Doc.md".into(), "ab".into(), None, None);
        doc.edit(0..2, "😀ក");
        assert!(doc.undo());
        assert_eq!(doc.content, "ab");
        assert_eq!(doc.cursor_offset(), 2);
        assert!(doc.content.is_char_boundary(doc.cursor_offset()));
        assert!(doc.redo());
        assert_eq!(doc.content, "😀ក");
        assert_eq!(doc.cursor_offset(), "😀ក".len());
        assert!(doc.content.is_char_boundary(doc.cursor_offset()));
        assert!(doc.content.is_char_boundary(doc.selection_range().start));
        assert!(doc.content.is_char_boundary(doc.selection_range().end));

        doc.caret.range = 2..2;
        doc.move_home(false);
        doc.move_end(false);
        doc.apply_format("bold");
        assert!(doc.content.is_char_boundary(doc.cursor_offset()));
        assert!(doc.content.is_char_boundary(doc.selection_range().start));
        assert!(doc.content.is_char_boundary(doc.selection_range().end));
    }

    #[test]
    fn shift_arrow_extends_from_the_anchor_and_clicks_select_word_and_line() {
        let mut doc = DocumentTab::new(
            "tab-1".into(),
            "Doc.md".into(),
            "abcd\nxy".into(),
            None,
            None,
        );
        doc.caret.move_to(3);
        doc.move_left(true);
        doc.move_left(true);
        assert_eq!(doc.selection_range(), 1..3);
        doc.move_right(true);
        assert_eq!(doc.selection_range(), 2..3);
        doc.move_right(true);
        doc.move_right(true);
        assert_eq!(doc.selected_text(), "d");

        doc.caret.move_to(1);
        doc.select_word();
        assert_eq!(doc.selected_text(), "abcd");
        doc.select_line();
        assert_eq!(doc.selected_text(), "abcd\n");
    }

    #[test]
    fn find_previous_leaves_the_current_match() {
        let mut doc = DocumentTab::new(
            "tab-1".into(),
            "Doc.md".into(),
            "cat cat cat".into(),
            None,
            None,
        );
        let first = doc.select_match("cat", true, None, true).unwrap();
        assert_eq!(first, 0);
        assert_eq!(doc.cursor_offset(), 3);
        let previous = doc.select_match("cat", true, Some(first), false).unwrap();
        assert_eq!(previous, 2);
        assert_eq!(doc.selected_text(), "cat");
        assert_ne!(doc.selection_range().start, 0);
    }

    #[test]
    fn word_navigation_and_word_deletion() {
        let mut doc = DocumentTab::new(
            "tab-1".into(),
            "Doc.md".into(),
            "hello world test".into(),
            None,
            None,
        );
        doc.caret.move_to(0);
        doc.move_word_right(false);
        assert_eq!(doc.cursor_offset(), 5);
        doc.move_word_right(true);
        assert_eq!(doc.selection_range(), 5..11);

        doc.caret.move_to(16);
        doc.delete_word_left();
        assert_eq!(doc.content, "hello world ");
        doc.delete_word_left();
        assert_eq!(doc.content, "hello ");

        doc.select_all();
        assert_eq!(doc.selection_range(), 0..6);
    }

    #[test]
    fn test_smart_newline_bullet_and_task_and_number() {
        // 1. Bullet continuation and exit
        let mut doc = DocumentTab::new("1".into(), "test.md".into(), "- Item 1".into(), None, None);
        doc.caret.move_to(8);
        doc.smart_newline(false);
        assert_eq!(doc.content, "- Item 1\n- ");
        assert_eq!(doc.cursor_offset(), 11);

        // Press Enter again on empty bullet -> exits list
        doc.smart_newline(false);
        assert_eq!(doc.content, "- Item 1\n");
        assert_eq!(doc.cursor_offset(), 9);

        // 2. Numbered list continuation and exit
        let mut doc2 = DocumentTab::new("2".into(), "test.md".into(), "1. First".into(), None, None);
        doc2.caret.move_to(8);
        doc2.smart_newline(false);
        assert_eq!(doc2.content, "1. First\n2. ");

        // Press Enter on empty "2. " -> exits list
        doc2.smart_newline(false);
        assert_eq!(doc2.content, "1. First\n");

        // 3. Task list continuation and exit
        let mut doc3 = DocumentTab::new("3".into(), "test.md".into(), "- [ ] Todo".into(), None, None);
        doc3.caret.move_to(10);
        doc3.smart_newline(false);
        assert_eq!(doc3.content, "- [ ] Todo\n- [ ] ");

        // Press Enter on empty task -> exits task list
        doc3.smart_newline(false);
        assert_eq!(doc3.content, "- [ ] Todo\n");

        // 4. Shift+Enter -> plain newline
        let mut doc4 = DocumentTab::new("4".into(), "test.md".into(), "- Item".into(), None, None);
        doc4.caret.move_to(6);
        doc4.smart_newline(true);
        assert_eq!(doc4.content, "- Item\n");

        // 5. Heading -> plain newline (does not duplicate #)
        let mut doc5 = DocumentTab::new("5".into(), "test.md".into(), "# Title".into(), None, None);
        doc5.caret.move_to(7);
        doc5.smart_newline(false);
        assert_eq!(doc5.content, "# Title\n");
    }

    #[test]
    fn test_smart_backspace_boundaries() {
        // 1. Heading demotion
        let mut doc = DocumentTab::new("1".into(), "test.md".into(), "# Heading".into(), None, None);
        doc.caret.move_to(2); // right after "# "
        doc.smart_backspace();
        assert_eq!(doc.content, "Heading");
        assert_eq!(doc.cursor_offset(), 0);

        let mut doc_h2 = DocumentTab::new("2".into(), "test.md".into(), "## Heading".into(), None, None);
        doc_h2.caret.move_to(3); // right after "## "
        doc_h2.smart_backspace();
        assert_eq!(doc_h2.content, "# Heading");

        // 2. Bullet list prefix removal
        let mut doc_bullet = DocumentTab::new("3".into(), "test.md".into(), "- Item".into(), None, None);
        doc_bullet.caret.move_to(2); // right after "- "
        doc_bullet.smart_backspace();
        assert_eq!(doc_bullet.content, "Item");

        // 3. Task list prefix demotion
        let mut doc_task = DocumentTab::new("4".into(), "test.md".into(), "- [ ] Task".into(), None, None);
        doc_task.caret.move_to(6); // right after "- [ ] "
        doc_task.smart_backspace();
        assert_eq!(doc_task.content, "- Task");

        // 4. Auto-paired brackets deletion
        let mut doc_pair = DocumentTab::new("5".into(), "test.md".into(), "()".into(), None, None);
        doc_pair.caret.move_to(1); // between ( and )
        doc_pair.smart_backspace();
        assert_eq!(doc_pair.content, "");

        // 5. Indented list item outdent on backspace
        let mut doc_indent = DocumentTab::new("6".into(), "test.md".into(), "  - Subitem".into(), None, None);
        doc_indent.caret.move_to(4); // right after "  - "
        doc_indent.smart_backspace();
        assert_eq!(doc_indent.content, "- Subitem");
    }

    #[test]
    fn test_indent_and_outdent() {
        let mut doc = DocumentTab::new("1".into(), "test.md".into(), "- Item 1\n- Item 2".into(), None, None);
        doc.caret.move_to(0);
        doc.indent();
        assert_eq!(doc.content, "  - Item 1\n- Item 2");

        doc.outdent();
        assert_eq!(doc.content, "- Item 1\n- Item 2");

        // Multi-line selection indent and outdent
        doc.caret.range = 0..doc.content.len();
        doc.indent();
        assert_eq!(doc.content, "  - Item 1\n  - Item 2");

        doc.outdent();
        assert_eq!(doc.content, "- Item 1\n- Item 2");
    }

    #[test]
    fn test_block_prefix_len() {
        assert_eq!(block_prefix_len("# Title"), 2);
        assert_eq!(block_prefix_len("### Title"), 4);
        assert_eq!(block_prefix_len("- Item"), 2);
        assert_eq!(block_prefix_len("  - Sub"), 4);
        assert_eq!(block_prefix_len("- [ ] Task"), 6);
        assert_eq!(block_prefix_len("1. One"), 3);
        assert_eq!(block_prefix_len("10. Ten"), 4);
        assert_eq!(block_prefix_len("> Quote"), 2);
        assert_eq!(block_prefix_len("Plain paragraph"), 0);
    }

    #[test]
    fn test_universal_reversible_inline_and_block_toggling() {
        // 1. Inline toggling with cursor inside word (no selection needed)
        for (cmd, expected_wrapped) in [
            ("bold", "hello **world**"),
            ("italic", "hello *world*"),
            ("underline", "hello <u>world</u>"),
            ("strike", "hello ~~world~~"),
            ("code", "hello `world`"),
            ("link", "hello [world](https://)"),
        ] {
            let mut doc =
                DocumentTab::new("t".into(), "test.md".into(), "hello world".into(), None, None);
            doc.place(8, false); // inside "world"
            doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
            assert_eq!(doc.content, expected_wrapped, "apply {cmd} failed");
            // Pressing the exact same command again must cleanly undo/unwrap it!
            doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
            assert_eq!(doc.content, "hello world", "second press of {cmd} did not unwrap");
        }

        // 2. Block toggling ON and OFF idempotently, plus switching between block types
        for (cmd, expected_line) in [
            ("h1", "# My line"),
            ("h2", "## My line"),
            ("h3", "### My line"),
            ("ul", "- My line"),
            ("ol", "1. My line"),
            ("checklist", "- [ ] My line"),
            ("quote", "> My line"),
            ("codeblock", "```\nMy line\n```"),
        ] {
            let mut doc =
                DocumentTab::new("b".into(), "test.md".into(), "My line".into(), None, None);
            doc.place(3, false);
            doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
            assert_eq!(doc.content, expected_line, "apply block {cmd} failed");
            // Pressing a second time toggles off back to "My line"
            doc.apply_format_in_mode(cmd, ViewMode::Wysiwyg);
            assert_eq!(doc.content, "My line", "second press of block {cmd} did not toggle off");
        }

        // 3. Switching directly from Bullet List to Numbered List to Checklist without stacking
        let mut doc = DocumentTab::new("s".into(), "test.md".into(), "Task item".into(), None, None);
        doc.place(2, false);
        doc.apply_format_in_mode("ul", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "- Task item");
        assert!(doc.active_formats.ul);
        doc.apply_format_in_mode("ol", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "1. Task item");
        assert!(doc.active_formats.ol);
        assert!(!doc.active_formats.ul);
        doc.apply_format_in_mode("checklist", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "- [ ] Task item");
        assert!(doc.active_formats.checklist);
        assert!(!doc.active_formats.ol);
        doc.apply_format_in_mode("checklist", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "Task item");
        assert!(!doc.active_formats.checklist);
    }

    #[test]
    fn test_wysiwyg_pending_inline_formats_and_typing() {
        let mut doc = DocumentTab::new("p".into(), "test.md".into(), "hello ".into(), None, None);
        doc.place(6, false); // at empty end after space

        // Press Ctrl+B once -> arms pending bold, does not insert placeholder text
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "hello ");
        assert!(doc.pending_formats.bold);
        assert!(doc.active_formats.bold);

        // Press Ctrl+B again -> disarms pending bold
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "hello ");
        assert!(!doc.pending_formats.bold);
        assert!(!doc.active_formats.bold);

        // Re-arm Bold + Italic, then type "A" -> wraps in "***A***" and places cursor after A
        doc.apply_format_in_mode("bold", ViewMode::Wysiwyg);
        doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);
        assert!(doc.active_formats.bold && doc.active_formats.italic);
        let range = doc.selection_range();
        doc.insert_text_in_mode(range, "A", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "hello ***A***");
        assert_eq!(doc.cursor_offset(), 10); // right after 'A', inside closing '***'
        assert!(doc.active_formats.bold && doc.active_formats.italic);
    }

    #[test]
    fn test_underline_toggle_selection_and_pending_formats() {
        let mut doc = DocumentTab::new(
            "u1".into(),
            "test.md".into(),
            "selected text to underline".into(),
            None,
            None,
        );
        // Select "text" (indices 9..13)
        doc.place(9, false);
        doc.place(13, true);
        doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "selected <u>text</u> to underline");
        assert!(doc.active_formats.underline);

        // Toggle again on selection should unwrap
        doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);
        assert_eq!(doc.content, "selected text to underline");
        assert!(!doc.active_formats.underline);

        // Empty cursor in WYSIWYG mode toggles pending_formats
        let mut doc2 = DocumentTab::new("u2".into(), "test.md".into(), "".into(), None, None);
        doc2.place(0, false);
        doc2.apply_format_in_mode("underline", ViewMode::Wysiwyg);
        assert!(doc2.pending_formats.underline);
        assert!(doc2.active_formats.underline);

        // Disarm pending underline
        doc2.apply_format_in_mode("underline", ViewMode::Wysiwyg);
        assert!(!doc2.pending_formats.underline);
        assert!(!doc2.active_formats.underline);

        // Re-arm and type text -> wraps in <u>...</u>
        doc2.apply_format_in_mode("underline", ViewMode::Wysiwyg);
        let range = doc2.selection_range();
        doc2.insert_text_in_mode(range, "hello", ViewMode::Wysiwyg);
        assert_eq!(doc2.content, "<u>hello</u>");
        assert!(!doc2.pending_formats.underline);
    }

    #[test]
    fn test_wysiwyg_delimiter_skipping_navigation_and_atomic_backspace() {
        let mut doc =
            DocumentTab::new("n".into(), "test.md".into(), "a **b** c".into(), None, None);
        doc.place(0, false);

        // Visible text is "a b c"
        // Step right across visible characters: 'a' -> ' ' -> 'b' -> ' ' -> 'c'
        doc.move_right_in_mode(false, ViewMode::Wysiwyg);
        assert_eq!(doc.cursor_offset(), 1); // after 'a'
        doc.move_right_in_mode(false, ViewMode::Wysiwyg);
        assert_eq!(doc.cursor_offset(), 4); // skipped space and opening "**" to land before 'b'
        doc.move_right_in_mode(false, ViewMode::Wysiwyg);
        assert_eq!(doc.cursor_offset(), 7); // stepped over 'b' and skipped closing "**"

        // Step left back onto 'b'
        doc.move_left_in_mode(false, ViewMode::Wysiwyg);
        assert_eq!(doc.cursor_offset(), 4); // before 'b'

        // Place right after 'b' (offset 5 or 7) and atomic backspace the single-char bold span
        doc.place(5, false);
        doc.smart_backspace_in_mode(ViewMode::Wysiwyg);
        assert_eq!(doc.content, "a  c");
    }

    #[test]
    fn test_table_cell_tab_navigation() {
        let table = "| Name | Role |\n| --- | --- |\n| Alice | Admin |";
        let mut doc = DocumentTab::new("tbl".into(), "test.md".into(), table.into(), None, None);
        doc.place(2, false); // inside "Name"

        assert!(doc.tab_table_cell(false));
        assert_eq!(doc.selected_text(), "Role");

        assert!(doc.tab_table_cell(false));
        assert_eq!(doc.selected_text(), "Alice");

        assert!(doc.tab_table_cell(false));
        assert_eq!(doc.selected_text(), "Admin");

        // Shift+Tab goes backwards
        assert!(doc.tab_table_cell(true));
        assert_eq!(doc.selected_text(), "Alice");

        // Forward Tab to Admin, then forward Tab on last cell appends a new row
        assert!(doc.tab_table_cell(false));
        assert_eq!(doc.selected_text(), "Admin");
        assert!(doc.tab_table_cell(false));
        assert!(doc.content.ends_with("\n|  |  |"));
    }

    #[test]
    fn test_table_insertion_and_row_addition() {
        let mut doc = DocumentTab::new("t".into(), "test.md".into(), String::new(), None, None);
        doc.apply_format_in_mode("table", ViewMode::Wysiwyg);
        assert_eq!(
            doc.content,
            "| Column 1 | Column 2 |\n| :--- | :--- |\n| Item 1 | Item 2 |"
        );
        assert_eq!(doc.selected_text(), "Column 1");

        // Calling "table" format while inside a table appends a new row
        doc.apply_format_in_mode("table", ViewMode::Wysiwyg);
        assert!(doc.content.ends_with("\n|  |  |"));
    }

    #[test]
    fn test_table_smart_newline_continuation_and_exit() {
        let table = "| Col 1 | Col 2 |\n| :--- | :--- |\n| Item 1 | Item 2 |";
        let mut doc = DocumentTab::new("t".into(), "test.md".into(), table.into(), None, None);
        doc.place(table.len(), false); // at end of "Item 2 |"

        // Press Enter on non-empty row -> adds a new row
        doc.smart_newline(false);
        assert!(doc.content.ends_with("\n|  |  |"));

        // Press Enter on the new empty row -> removes it and exits table to a newline after table
        doc.smart_newline(false);
        assert_eq!(doc.content, format!("{}\n", table));
        assert_eq!(doc.cursor_offset(), table.len() + 1);

        // Pressing Enter again outside table does not add a row to the table
        doc.smart_newline(false);
        assert_eq!(doc.content, format!("{}\n\n", table));
    }

    #[test]
    fn test_table_single_empty_row_exit_keeps_header_and_removes_empty_row() {
        // Table with only a single empty body row
        let table = "| Col 1 | Col 2 |\n| :--- | :--- |\n|  |  |";
        let mut doc = DocumentTab::new("t".into(), "test.md".into(), table.into(), None, None);
        doc.place(table.len(), false); // inside empty row

        doc.smart_newline(false);
        // Exiting the only empty row keeps the header and only removes the empty row!
        assert_eq!(doc.content, "| Col 1 | Col 2 |\n| :--- | :--- |\n");
        assert_eq!(doc.cursor_offset(), doc.content.len());
    }

    #[test]
    fn test_table_enter_splits_cell() {
        let table = "| Col 1 | Col 2 |\n| :--- | :--- |\n| First item | Second item |";
        let mut doc = DocumentTab::new("t".into(), "test.md".into(), table.into(), None, None);
        // Place cursor between "First " and "item" in Col 1
        let pos = table.find("First ").unwrap() + "First ".len();
        doc.place(pos, false);

        doc.smart_newline(false);
        let expected = "| Col 1 | Col 2 |\n| :--- | :--- |\n| First | Second item |\n| item |  |";
        assert_eq!(doc.content, expected);
        assert_eq!(doc.cursor_offset(), expected.find("item |  |").unwrap());
    }

    #[test]
    fn test_insert_table_with_size_and_undo() {
        let mut doc = DocumentTab::new("t".into(), "test.md".into(), String::new(), None, None);
        doc.insert_table_with_size(3, 4);
        assert_eq!(
            doc.content,
            "|  |  |  |\n| :--- | :--- | :--- |\n|  |  |  |\n|  |  |  |\n|  |  |  |"
        );
        assert_eq!(doc.selected_text(), "");
        assert_eq!(doc.cursor_offset(), 2);

        // Undo in a single step restores initial state
        assert!(doc.undo());
        assert_eq!(doc.content, "");
    }

    #[test]
    fn test_table_row_and_column_manipulations_and_undo() {
        let initial_table = "| Column 1 | Column 2 |\n| :--- | :--- |\n| Item 1 | Item 2 |";
        let mut doc = DocumentTab::new("t".into(), "test.md".into(), initial_table.into(), None, None);

        // Place cursor in Item 1
        let pos = doc.content.find("Item 1").unwrap() + 2;
        doc.place(pos, false);

        // Add row above
        doc.table_add_row_above();
        assert!(doc.content.contains("| Column 1 | Column 2 |\n| :--- | :--- |\n|  |  |\n| Item 1 | Item 2 |"));
        assert!(doc.undo());
        assert_eq!(doc.content, initial_table);

        // Add row below
        let pos = doc.content.find("Item 1").unwrap() + 2;
        doc.place(pos, false);
        doc.table_add_row_below();
        assert!(doc.content.ends_with("\n|  |  |"));
        assert!(doc.undo());
        assert_eq!(doc.content, initial_table);

        // Add column right
        let pos = doc.content.find("Item 1").unwrap() + 2;
        doc.place(pos, false);
        doc.table_add_col_right();
        assert!(doc.content.contains("| Item 1 |  | Item 2 |"));
        assert!(doc.undo());
        assert_eq!(doc.content, initial_table);

        // Add column left
        let pos = doc.content.find("Item 1").unwrap() + 2;
        doc.place(pos, false);
        doc.table_add_col_left();
        assert!(doc.content.contains("|  | Item 1 | Item 2 |"));
        assert!(doc.undo());
        assert_eq!(doc.content, initial_table);

        // Align column right
        let pos = doc.content.find("Item 1").unwrap() + 2;
        doc.place(pos, false);
        doc.table_align_column(TableColumnAlign::Right);
        assert!(doc.content.contains("| ---: | :--- |"));
        assert!(doc.undo());
        assert!(doc.content.contains("| :--- | :--- |"));

        // Delete row
        let pos = doc.content.find("Item 1").unwrap() + 2;
        doc.place(pos, false);
        doc.table_delete_row();
        assert_eq!(doc.content, "| Column 1 | Column 2 |\n| :--- | :--- |");
        assert!(doc.undo());
        assert_eq!(doc.content, initial_table);

        // Delete table
        let pos = doc.content.find("Item 1").unwrap() + 2;
        doc.place(pos, false);
        doc.table_delete_table();
        assert_eq!(doc.content, "");
        assert!(doc.undo());
        assert_eq!(doc.content, initial_table);
    }

    #[test]
    fn test_table_edit_keeps_escaped_pipe() {
        let table = "| a \\| b | c |\n| --- | --- |\n| d | e |";
        let mut doc = DocumentTab::new("t".into(), "test.md".into(), table.into(), None, None);
        doc.place(doc.content.find('a').unwrap(), false);
        doc.table_add_col_right();
        assert_eq!(
            doc.content,
            "| a \\| b |  | c |\n| --- | --- | --- |\n| d |  | e |"
        );
        assert!(!doc.content.contains("| a \\ |"));
    }

    #[test]
    fn test_table_edit_keeps_unset_alignment() {
        let table = "| a | b |\n| --- | --- |\n| 1 | 2 |";
        let mut doc = DocumentTab::new("t".into(), "test.md".into(), table.into(), None, None);
        doc.place(doc.content.find('a').unwrap(), false);
        doc.table_add_col_right();
        assert!(doc.content.contains("| --- | --- | --- |"));
        assert!(!doc.content.contains(":---"));

        let mixed = "| a | b |\n| :--- | --- |\n| 1 | 2 |";
        let mut doc = DocumentTab::new("t".into(), "test.md".into(), mixed.into(), None, None);
        doc.place(doc.content.find('1').unwrap(), false);
        doc.table_add_row_below();
        assert!(doc.content.contains("| :--- | --- |"));
        assert!(!doc.content.contains("| :--- | :--- |"));
        assert_eq!(
            doc.content,
            "| a | b |\n| :--- | --- |\n| 1 | 2 |\n|  |  |"
        );
    }

    #[test]
    fn test_wysiwyg_table_boundary_guards() {
        let table = "| Col 1 | Col 2 |\n| :--- | :--- |\n| Item 1 | Item 2 |";
        let mut doc = DocumentTab::new("t".into(), "test.md".into(), table.into(), None, None);

        // Position at start of "Item 1" (right after leading pipe/space)
        let pos = table.find("Item 1").unwrap();
        doc.place(pos, false);

        // smart_backspace in WYSIWYG mode should NOT delete the delimiter or merge with previous cell
        doc.smart_backspace_in_mode(ViewMode::Wysiwyg);
        assert_eq!(doc.content, table);
        assert_eq!(doc.cursor_offset(), pos);

        // Position at end of "Item 1"
        let pos = table.find("Item 1").unwrap() + "Item 1".len();
        doc.place(pos, false);

        // delete_forward in WYSIWYG mode should NOT delete the delimiter
        doc.delete_forward_in_mode(ViewMode::Wysiwyg);
        assert_eq!(doc.content, table);
        assert_eq!(doc.cursor_offset(), pos);
    }

    #[test]
    fn test_enter_on_indented_empty_quote_leaves_quote() {
        let mut doc = DocumentTab::new("t".into(), "test.md".into(), "  > ".into(), None, None);
        doc.place("  > ".len(), false);

        // Enter on indented empty quote removes the quote line, leaving the quote completely
        doc.smart_newline(false);
        assert_eq!(doc.content, "");
    }

    #[test]
    fn test_enter_on_empty_list_item_with_selection() {
        let mut doc = DocumentTab::new("t".into(), "test.md".into(), "- ".into(), None, None);
        // Select the whole "- "
        doc.caret.range = 0..2;

        // Enter on empty list item with selection must replace selection with newline
        doc.smart_newline(false);
        assert_eq!(doc.content, "\n");
    }

    #[test]
    fn test_task_list_asterisk_continuation() {
        let mut doc = DocumentTab::new("t".into(), "test.md".into(), "* [x] done".into(), None, None);
        doc.place("* [x] done".len(), false);

        // Enter on "* [x] done" must continue with "* [ ] ", not "- [ ] "
        doc.smart_newline(false);
        assert_eq!(doc.content, "* [x] done\n* [ ] ");
    }

    #[test]
    fn test_emoji_formatting_no_crash() {
        let content = "🦀hello world";
        let mut doc = DocumentTab::new("t".into(), "test.md".into(), content.into(), None, None);
        // Place caret right after emoji (at byte 4)
        doc.place(4, false);

        // Press Ctrl+U (underline) in Wysiwyg mode next to emoji -> must not crash!
        doc.apply_format_in_mode("underline", ViewMode::Wysiwyg);

        // Press Ctrl+I (italic) in Wysiwyg mode next to emoji -> must not crash!
        doc.apply_format_in_mode("italic", ViewMode::Wysiwyg);

        let content2 = "hello🦀";
        let mut doc2 = DocumentTab::new("t".into(), "test.md".into(), content2.into(), None, None);
        doc2.place(content2.len(), false);
        doc2.apply_format_in_mode("underline", ViewMode::Wysiwyg);
        doc2.apply_format_in_mode("italic", ViewMode::Wysiwyg);
    }

    #[test]
    fn test_backspace_on_text_ending_with_dash_space() {
        let mut doc = DocumentTab::new("t".into(), "test.md".into(), "foo - ".into(), None, None);
        doc.place("foo - ".len(), false);

        // Backspacing on "foo - " must delete the space, NOT wipe out "foo - " to empty!
        doc.smart_backspace();
        assert_eq!(doc.content, "foo -");

        doc.smart_backspace();
        assert_eq!(doc.content, "foo ");
    }

    #[test]
    fn test_enter_on_indented_bullet_tab() {
        let mut doc = DocumentTab::new("t".into(), "test.md".into(), "\t- ".into(), None, None);
        doc.place("\t- ".len(), false);

        // Enter on "\t- " must outdent to "- ", NOT wipe to empty string!
        doc.smart_newline(false);
        assert_eq!(doc.content, "- ");

        // Enter on root "- " exits the list, removing the marker
        doc.smart_newline(false);
        assert_eq!(doc.content, "");
    }

    #[test]
    fn test_enter_with_selection_is_single_undo_step() {
        let mut doc = DocumentTab::new(
            "t".into(),
            "test.md".into(),
            "hello world".into(),
            None,
            None,
        );
        doc.caret.range = 6..11;
        doc.smart_newline(false);
        assert_eq!(doc.content, "hello \n");
        assert!(doc.undo());
        assert_eq!(doc.content, "hello world");
        assert!(!doc.undo());
    }
}


