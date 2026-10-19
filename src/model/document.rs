use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FileType {
    #[default]
    Markdown,
    PlainText,
    Pdf,
}

impl FileType {
    pub fn from_filename(name: &str) -> Self {
        let lower = name.to_lowercase();
        if lower.ends_with(".pdf") {
            FileType::Pdf
        } else if lower.ends_with(".txt") || lower.ends_with(".text") || lower.ends_with(".log") {
            FileType::PlainText
        } else {
            FileType::Markdown
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
    pub undo_stack: Vec<String>,
    pub redo_stack: Vec<String>,
}

impl DocumentTab {
    pub fn new(id: String, title: String, content: String, file_path: Option<PathBuf>, workspace_file_id: Option<String>) -> Self {
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
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn set_content(&mut self, new_content: String) {
        if self.content != new_content {
            self.undo_stack.push(self.content.clone());
            if self.undo_stack.len() > 100 {
                self.undo_stack.remove(0);
            }
            self.redo_stack.clear();
            self.content = new_content;
            self.is_modified = true;
            self.refresh_stats();
        }
    }

    pub fn refresh_stats(&mut self) {
        let stats = compute_stats(&self.content);
        self.word_count = stats.words;
        self.char_count = stats.chars;
        self.line_count = stats.lines;
        self.reading_time_min = stats.reading_time_min;
    }

    pub fn undo(&mut self) -> bool {
        if let Some(prev) = self.undo_stack.pop() {
            self.redo_stack.push(self.content.clone());
            self.content = prev;
            self.is_modified = true;
            self.refresh_stats();
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(self.content.clone());
            self.content = next;
            self.is_modified = true;
            self.refresh_stats();
            true
        } else {
            false
        }
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
        let mut doc = DocumentTab::new("tab-1".to_string(), "Doc.md".to_string(), "Initial".to_string(), None, None);
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
}

