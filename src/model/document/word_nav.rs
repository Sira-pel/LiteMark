//! Ctrl+Left/Right and Ctrl+Backspace/Delete in the WYSIWYG view work on the
//! text the view shows, not on the Markdown source: inline markers and block
//! prefixes are skipped, and inside a table the word stays in its cell.

use super::*;
use crate::model::text_ops::{word_left, word_right};

struct VisibleChar {
    /// Byte index in [`VisibleRun::text`].
    vis: usize,
    /// Source range of the character.
    start: usize,
    end: usize,
}

/// The visible text of one line, or of one table cell.
struct VisibleRun {
    text: String,
    chars: Vec<VisibleChar>,
}

impl VisibleRun {
    /// Byte index in `text` of the visible text before source offset `at`.
    fn index_of(&self, at: usize) -> usize {
        self.chars
            .iter()
            .take_while(|c| c.start < at)
            .last()
            .map_or(0, |c| c.vis + (c.end - c.start))
    }

    fn char_at(&self, vis: usize) -> Option<&VisibleChar> {
        self.chars.iter().find(|c| c.vis == vis)
    }

    fn char_ending_at(&self, vis: usize) -> Option<&VisibleChar> {
        self.chars
            .iter()
            .rev()
            .find(|c| c.vis + c.text_len() == vis)
    }
}

impl VisibleChar {
    fn text_len(&self) -> usize {
        self.end - self.start
    }
}

/// How a word step ends.
pub(super) enum WordStep {
    /// The source range of the visible text the step covers: moving goes to
    /// its far end, deleting removes it.
    Range(Range<usize>),
    /// No visible text left in this direction on the line or in the cell.
    Edge,
}

impl DocumentTab {
    fn visible_run_at(&mut self, origin: usize) -> Option<VisibleRun> {
        if self.view_mode != ViewMode::Wysiwyg || self.file_type != FileType::Markdown {
            return None;
        }
        if self.picture_block_at(origin).is_some() {
            return None;
        }
        let mode = self.view_mode;
        let parsed = self.parsed();
        let content = &self.content;
        let origin = origin.min(content.len());
        parsed.line_at_offset(origin).and_then(|line| {
            let span = match &line.kind {
                LineKind::TableRow { body, .. } => {
                    let TableRowBody {
                        cells, raw_cells, ..
                    } = &**body;
                    let i = raw_cells
                        .iter()
                        .position(|rc| origin >= rc.start && origin <= rc.end)?;
                    cells.get(i)?.clone()
                }
                LineKind::TableDelimiter
                | LineKind::HorizontalRule
                | LineKind::CodeFence { .. } => line.source.start..line.source.start,
                _ => {
                    let start = line.prefix_end().min(line.source.end);
                    let end = visible_line_end(line, mode).max(start);
                    start..end
                }
            };
            let mut text = String::new();
            let mut chars = Vec::new();
            let slice = content.get(span.clone())?;
            for (off, ch) in slice.char_indices() {
                let start = span.start + off;
                if line
                    .hidden_ranges
                    .iter()
                    .any(|r| r.start <= start && start < r.end)
                {
                    continue;
                }
                chars.push(VisibleChar {
                    vis: text.len(),
                    start,
                    end: start + ch.len_utf8(),
                });
                text.push(ch);
            }
            Some(VisibleRun { text, chars })
        })
    }

    /// The visible word left or right of `origin`, as a source range.
    /// `None` when the WYSIWYG logic does not apply (raw view, other files,
    /// a picture line); the caller then uses the source-text word steps.
    pub(super) fn visible_word_step(&mut self, origin: usize, forward: bool) -> Option<WordStep> {
        let run = self.visible_run_at(origin)?;
        let idx = run.index_of(origin);
        if forward {
            let target = word_right(&run.text, idx);
            if target <= idx || target > run.text.len() {
                return Some(WordStep::Edge);
            }
            let first = run.char_at(idx)?;
            let last = run.char_ending_at(target)?;
            Some(WordStep::Range(first.start..last.end))
        } else {
            let target = word_left(&run.text, idx);
            if target >= idx {
                return Some(WordStep::Edge);
            }
            let first = run.char_at(target)?;
            let last = run.char_ending_at(idx)?;
            Some(WordStep::Range(first.start..last.end))
        }
    }

    /// Delete visible text through the WYSIWYG replace path, so a span that
    /// loses text keeps valid markers.
    pub(super) fn delete_visible_range(&mut self, range: Range<usize>) {
        // Taking the last word of a span would leave a space before its
        // closing marker (or after its opening one), which ends the span, so
        // that space goes with the word.
        let (mut start, mut end) = (range.start, range.end);
        {
            let parsed = self.parsed();
            if let Some(line) = parsed.line_at_offset(start) {
                if line.hidden_ranges.iter().any(|r| r.start == end) {
                    while self.content[..start].ends_with([' ', '\t']) {
                        start -= 1;
                    }
                }
                if line.hidden_ranges.iter().any(|r| r.end == range.start) {
                    while self.content[end..].starts_with([' ', '\t']) {
                        end += 1;
                    }
                }
            }
        }
        let range = start..end;
        if !self.replace_wysiwyg_selection(range.clone(), "") {
            self.edit(range, "");
        }
    }
}
