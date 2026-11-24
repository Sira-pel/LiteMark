//! Pasting text into a note.

use super::*;

/// Characters that start or end inline Markdown. Pasted text with any of them
/// goes in verbatim, as it always did, so a Markdown copy keeps its markers.
const MARKER_CHARS: [char; 8] = ['*', '~', '`', '<', '_', '[', ']', '\\'];

/// Pasted text made safe for one table cell: line breaks become one space
/// and a `|` that is not already escaped is escaped, so the row keeps its
/// cells.
fn sanitize_table_cell_paste(text: &str) -> String {
    let text = text.trim_end_matches(['\r', '\n']);
    let mut out = String::with_capacity(text.len());
    let mut escaped = false;
    let mut in_break = false;
    for c in text.chars() {
        if c == '\r' || c == '\n' {
            if !in_break {
                out.push(' ');
                in_break = true;
            }
            escaped = false;
            continue;
        }
        in_break = false;
        if c == '|' && !escaped {
            out.push_str("\\|");
        } else {
            out.push(c);
        }
        escaped = c == '\\' && !escaped;
    }
    out
}

impl DocumentTab {
    /// `text` with its line breaks changed to the note's own at `at`
    /// (CRLF notes stay CRLF, LF notes stay LF).
    pub fn paste_with_note_breaks(&self, text: &str, at: usize) -> String {
        if !text.contains(['\r', '\n']) {
            return text.to_string();
        }
        let nl = self.line_break(at);
        let mut out = String::with_capacity(text.len());
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '\r' => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    out.push_str(nl);
                }
                '\n' => out.push_str(nl),
                _ => out.push(c),
            }
        }
        out
    }

    /// Whether `range` lies inside one table row (not its delimiter row).
    fn range_in_table_row(&mut self, range: &Range<usize>) -> bool {
        let parsed = self.parsed();
        parsed.line_at_offset(range.start).is_some_and(|line| {
            matches!(line.kind, LineKind::TableRow { .. }) && range.end <= line.source.end
        })
    }

    /// Paste `text` over `range`. Raw view and non-Markdown files only get
    /// their line breaks matched to the note. WYSIWYG also keeps a table row
    /// intact when pasting into a cell and rewrites the styled spans around
    /// a partly selected range so no marker is left unbalanced.
    pub fn paste_text_in_mode(&mut self, range: Range<usize>, text: &str, mode: ViewMode) {
        if self.is_read_only() {
            return;
        }
        let text = self.paste_with_note_breaks(text, range.start);
        if mode != ViewMode::Wysiwyg || !self.file_type.supports_markdown_features() {
            self.edit(range, &text);
            return;
        }
        if self.range_in_table_row(&range) {
            let cell_text = sanitize_table_cell_paste(&text);
            if range.is_empty() && self.pending_table_col.is_some() {
                // The caret is in a padded cell the row does not have yet.
                self.insert_text_in_mode(range, &cell_text, mode);
            } else {
                self.edit(range, &cell_text);
            }
            return;
        }
        if !range.is_empty()
            && !text.contains(['\r', '\n'])
            && !text.contains(MARKER_CHARS)
            && self.replace_wysiwyg_selection(range.clone(), &text)
        {
            return;
        }
        self.edit(range, &text);
    }
}
