//! Block-level edits: indent and outdent, heading, list, quote and code block toggles.

use super::*;

impl DocumentTab {
    /// First and one-past-last byte of the lines a Tab or Shift+Tab touches. In
    /// code, a selection that ends exactly at the start of a line leaves that line out.
    pub(super) fn indent_target_lines(&self, range: &Range<usize>) -> (usize, usize) {
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

    pub(super) fn toggle_block_lines(&mut self, target: BlockFormatTarget) {
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

    pub(super) fn toggle_code_block(&mut self) {
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BlockFormatTarget {
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
pub(super) fn ordered_checklist_line(
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

pub(super) fn parse_line_block_prefix(
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

#[cfg_attr(not(test), allow(dead_code))]
pub fn block_prefix_len(line: &str) -> usize {
    block_prefix_in(line, 0)
}

/// Prefix length of the line that starts at `line_start` in `content`.
pub fn block_prefix_in(content: &str, line_start: usize) -> usize {
    let line_end_pos = crate::model::text_ops::line_end(content, line_start);
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
