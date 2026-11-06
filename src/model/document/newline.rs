//! Enter and Backspace: smart newlines, list and quote continuation, code fences.

use super::*;

impl DocumentTab {
    /// Enter at the visible end of a closing-hash heading inserts after the
    /// closer, so the heading stays one line.
    pub(super) fn heading_newline_at(&mut self, range: &Range<usize>) -> Option<usize> {
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
    pub(super) fn bare_cr_needs_extra_break(&self, range: &Range<usize>) -> bool {
        if !range.is_empty() {
            return false;
        }
        let cursor = range.start;
        let line_start = home_offset(&self.content, cursor);
        cursor == line_start
            && cursor > 0
            && self.content.as_bytes().get(cursor - 1) == Some(&b'\r')
    }

    /// Enter with the caret inside an inline formatting span: close the span
    /// before the newline and reopen it after `line_ins` on the new line, so
    /// no markers straddle the break. A caret at the end of the span closes
    /// it and keeps the formats armed so the next letter reopens them on the
    /// new line. `line_ins` is the line-prefix text the caller would insert
    /// at the caret (for example `"\n> "` inside a blockquote).
    pub(super) fn split_inline_span_newline(&mut self, range: &Range<usize>, line_ins: &str) -> bool {
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
    pub(super) fn line_break(&self, offset: usize) -> &'static str {
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
    pub(super) fn code_newline(&mut self, range: Range<usize>) {
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
}
