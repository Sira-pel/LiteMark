use serde::{Deserialize, Serialize};
use std::ops::Range;
use std::path::PathBuf;

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
    pub quote: bool,
    pub code: bool,
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
    pub caret: Caret,
    pub marked_range: Option<Range<usize>>,
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

        Self {
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
            caret: Caret::default(),
            marked_range: None,
            history: UndoHistory::default(),
        }
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
                if indent.len() >= 2 {
                    self.edit(line_start..line_start + 2, "");
                } else {
                    self.edit(line_start..line_end_pos, "");
                }
            } else {
                let ins = format!("\n{}- [ ] ", indent);
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
                if indent.len() >= 2 {
                    self.edit(line_start..line_start + 2, "");
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
                        if indent.len() >= 2 {
                            self.edit(line_start..line_start + 2, "");
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
                self.edit(line_start..line_end_pos, "");
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

        // 3. Task list prefix at cursor boundary
        for prefix in [
            "- [ ] ", "- [x] ", "- [X] ", "* [ ] ", "* [x] ", "* [X] ", "+ [ ] ", "+ [x] ", "+ [X] ",
        ] {
            if line_before.ends_with(prefix) {
                let start = cursor - 4;
                self.edit(start..cursor, "");
                return true;
            }
        }

        // 4. Bullet list prefix at cursor boundary
        for prefix in ["- ", "* ", "+ "] {
            if line_before.ends_with(prefix) {
                let trimmed = line_before.trim_start();
                let indent_len = line_before.len() - trimmed.len();
                if indent_len >= 2 {
                    self.edit(line_start..line_start + 2, "");
                } else {
                    self.edit(line_start..cursor, "");
                }
                return true;
            }
        }

        // 5. Numbered list prefix at cursor boundary
        if line_before.ends_with(". ") {
            let trimmed = line_before.trim_start();
            let indent_len = line_before.len() - trimmed.len();
            if let Some(dot_pos) = trimmed.find(". ") {
                if dot_pos > 0
                    && trimmed[..dot_pos].chars().all(|c| c.is_ascii_digit())
                    && dot_pos + 2 == trimmed.len()
                {
                    if indent_len >= 2 {
                        self.edit(line_start..line_start + 2, "");
                    } else {
                        self.edit(line_start..cursor, "");
                    }
                    return true;
                }
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

    pub fn apply_format(&mut self, cmd: &str) {
        if self.is_read_only() {
            return;
        }

        // 1. Headings (H1, H2, H3) - operates on whole line(s)
        if matches!(cmd, "h1" | "h2" | "h3") {
            let range = self.selection_range();
            let start_line = super::text_ops::home_offset(&self.content, range.start);
            let end_line = super::text_ops::line_end(&self.content, range.end);
            let target_level = match cmd {
                "h1" => 1,
                "h2" => 2,
                "h3" => 3,
                _ => 1,
            };
            let target_prefix = match target_level {
                1 => "# ",
                2 => "## ",
                3 => "### ",
                _ => "# ",
            };

            let lines_str = &self.content[start_line..end_line];
            let mut transformed_lines = Vec::new();
            for line in lines_str.split('\n') {
                let trimmed = line.trim_start();
                let leading_spaces = &line[..line.len() - trimmed.len()];
                let (existing_level, content_after_prefix) = if let Some(rest) = trimmed.strip_prefix("###### ") {
                    (Some(6), rest)
                } else if let Some(rest) = trimmed.strip_prefix("##### ") {
                    (Some(5), rest)
                } else if let Some(rest) = trimmed.strip_prefix("#### ") {
                    (Some(4), rest)
                } else if let Some(rest) = trimmed.strip_prefix("### ") {
                    (Some(3), rest)
                } else if let Some(rest) = trimmed.strip_prefix("## ") {
                    (Some(2), rest)
                } else if let Some(rest) = trimmed.strip_prefix("# ") {
                    (Some(1), rest)
                } else {
                    (None, trimmed)
                };

                if existing_level == Some(target_level) {
                    // Toggle off heading -> plain text
                    transformed_lines.push(format!("{}{}", leading_spaces, content_after_prefix));
                } else {
                    // Apply or switch heading level
                    transformed_lines.push(format!("{}{}{}", leading_spaces, target_prefix, content_after_prefix));
                }
            }
            let replacement = transformed_lines.join("\n");
            self.edit(start_line..end_line, &replacement);
            return;
        }

        // 2. Inline styling (bold, italic, underline, strike, code)
        let inline_delims = match cmd {
            "bold" => Some(("**", "**", "bold text")),
            "italic" => Some(("*", "*", "italic text")),
            "underline" => Some(("<u>", "</u>", "underlined")),
            "strike" => Some(("~~", "~~", "strikethrough")),
            "code" => Some(("`", "`", "code")),
            _ => None,
        };

        if let Some((prefix, suffix, placeholder)) = inline_delims {
            let mut range = self.selection_range();

            // If selection is empty, check if cursor is on/inside a word
            if range.is_empty() {
                let cursor = range.start;
                let word_start = super::text_ops::word_left(&self.content, cursor);
                let word_end = super::text_ops::word_right(&self.content, cursor);

                // Check if already enclosed by delimiters around the cursor/word
                let has_enclosing_delims = outer_is_delimited(&self.content, word_start, word_end, prefix, suffix);

                if has_enclosing_delims {
                    // Toggle off: strip the delimiters
                    let core = self.content[word_start..word_end].to_string();
                    let edit_range = word_start - prefix.len()..word_end + suffix.len();
                    let new_start = word_start - prefix.len();
                    let new_end = new_start + core.len();
                    self.edit(edit_range, &core);
                    self.caret.range = new_start..new_end;
                    self.caret.reversed = false;
                    self.refresh_cursor();
                    return;
                }

                // If on a valid non-empty word, expand to it
                if word_start < word_end
                    && self.content[word_start..word_end].chars().any(|c| c.is_alphanumeric())
                {
                    range = word_start..word_end;
                }
            }

            let selected = self.content[range.clone()].to_string();

            if selected.is_empty() {
                // Empty at whitespace / empty line -> insert placeholder
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

                // Check if core is already enclosed by prefix and suffix
                let core_wrapped = is_delimited(core, prefix, suffix);

                // Check if text outside the selection has prefix and suffix
                let outer_wrapped = outer_is_delimited(&self.content, range.start, range.end, prefix, suffix);

                if core_wrapped {
                    // Toggle off from inside core
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
                    // Toggle off from outside selection
                    let edit_range = range.start - prefix.len()..range.end + suffix.len();
                    let new_start = range.start - prefix.len();
                    let new_end = new_start + selected.len();
                    self.edit(edit_range, &selected);
                    self.caret.range = new_start..new_end;
                    self.caret.reversed = false;
                    self.refresh_cursor();
                    return;
                } else {
                    // Wrap tightly around core, keeping whitespace outside
                    let replacement = format!("{}{}{}{}{}", leading, prefix, core, suffix, trailing);
                    let new_start = range.start;
                    let new_end = range.start + replacement.len();
                    self.edit(range, &replacement);
                    self.caret.range = new_start..new_end;
                    self.caret.reversed = false;
                    self.refresh_cursor();
                    return;
                }
            }
        }

        // 3. Fallback for lists, clear, blockquote, hr, table, link
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
        let target = if !extend && !self.caret.range.is_empty() {
            self.selection_range().start
        } else {
            previous_grapheme(&self.content, self.cursor_offset())
        };
        self.place(target, extend);
    }

    pub fn move_right(&mut self, extend: bool) {
        let target = if !extend && !self.caret.range.is_empty() {
            self.selection_range().end
        } else {
            next_grapheme(&self.content, self.cursor_offset())
        };
        self.place(target, extend);
    }

    pub fn move_up(&mut self, extend: bool) {
        move_vertical_collapsed(&self.content, &mut self.caret, -1, extend);
        self.refresh_cursor();
    }

    pub fn move_down(&mut self, extend: bool) {
        move_vertical_collapsed(&self.content, &mut self.caret, 1, extend);
        self.refresh_cursor();
    }

    pub fn move_home(&mut self, extend: bool) {
        let target = super::text_ops::home_offset(&self.content, self.cursor_offset());
        self.place(target, extend);
    }

    pub fn move_end(&mut self, extend: bool) {
        let line_start = super::text_ops::home_offset(&self.content, self.cursor_offset());
        let target = super::text_ops::line_end(&self.content, line_start);
        self.place(target, extend);
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
        let (line, col) = line_col(&self.content, self.cursor_offset());
        self.cursor_line = line;
        self.cursor_col = col;
    }

    pub fn undo(&mut self) -> bool {
        let Some(caret) = self.history.undo(&mut self.content) else {
            return false;
        };
        self.place_after_history(caret);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(caret) = self.history.redo(&mut self.content) else {
            return false;
        };
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
        // To be italic, must be triple asterisks (***) or single asterisk (* not preceded/followed by *)
        let triple = text.starts_with("***") && text.ends_with("***") && text.len() >= 6;
        let single = !text.starts_with("**") && !text.ends_with("**");
        triple || single
    } else {
        true
    }
}

fn outer_is_delimited(content: &str, start: usize, end: usize, prefix: &str, suffix: &str) -> bool {
    if start < prefix.len() || content.len() < end + suffix.len() {
        return false;
    }
    let before = &content[start - prefix.len()..start];
    let after = &content[end..end + suffix.len()];
    if before != prefix || after != suffix {
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

    // Headings (up to 3 spaces before # in CommonMark)
    if indent_len <= 3 {
        for prefix in ["###### ", "##### ", "#### ", "### ", "## ", "# "] {
            if trimmed_start.starts_with(prefix) {
                return indent_len + prefix.len();
            }
        }
    }

    // Task lists
    for prefix in [
        "- [ ] ", "- [x] ", "- [X] ", "* [ ] ", "* [x] ", "* [X] ", "+ [ ] ", "+ [x] ", "+ [X] ",
    ] {
        if trimmed_start.starts_with(prefix) {
            return indent_len + prefix.len();
        }
    }

    // Bullet lists
    for prefix in ["- ", "* ", "+ "] {
        if trimmed_start.starts_with(prefix) {
            return indent_len + prefix.len();
        }
    }

    // Blockquote
    if trimmed_start.starts_with("> ") {
        return indent_len + 2;
    }

    // Numbered list: digits + ". "
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
}
