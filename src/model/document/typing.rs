//! Text insertion and deletion in source and WYSIWYG modes.

use super::*;

impl DocumentTab {
    pub fn has_pending_inline_formats(&self) -> bool {
        let pf = self.pending_formats;
        pf.bold || pf.italic || pf.underline || pf.strike || pf.code
    }

    pub fn insert_text_in_mode(&mut self, range: Range<usize>, text: &str, mode: ViewMode) {
        if self.is_read_only() {
            return;
        }
        if mode == ViewMode::Wysiwyg && self.file_type.supports_markdown_features() {
            if let Some(target_col) = self.pending_table_col.take() {
                if range.is_empty() && text != "\n" {
                    let row_start = {
                        let parsed = self.parsed();
                        parsed
                            .line_at_offset(range.start)
                            .filter(|l| {
                                matches!(l.kind, LineKind::TableRow { .. })
                                    && range.start == l.source.end
                            })
                            .map(|l| l.source.start)
                    };
                    if let Some(line_start) = row_start {
                        let pf = self.pending_formats;
                        let formatted;
                        let payload =
                            if self.has_pending_inline_formats() && !text.trim().is_empty() {
                                let flags = FormatFlags {
                                    bold: pf.bold,
                                    italic: pf.italic,
                                    underline: pf.underline,
                                    strike: pf.strike,
                                    code: pf.code,
                                };
                                let (open, close) = wrap_delimiters(&flags);
                                formatted = format!("{open}{text}{close}");
                                formatted.as_str()
                            } else {
                                text
                            };
                        if self
                            .insert_into_padded_table_cell(line_start, target_col, payload)
                            .is_some()
                        {
                            // Only a successful insert uses up the pending formats.
                            if payload.len() != text.len() {
                                self.pending_formats = ActiveFormats::default();
                            }
                            return;
                        }
                    }
                }
            }
        } else {
            self.pending_table_col = None;
        }
        if mode == ViewMode::Wysiwyg
            && self.file_type.supports_markdown_features()
            && self.picture_insert(range.clone(), text)
        {
            return;
        }
        if mode == ViewMode::Wysiwyg
            && self.file_type.supports_markdown_features()
            && self.block_marker_insert(range.clone(), text)
        {
            return;
        }
        if mode == ViewMode::Wysiwyg
            && self.file_type.supports_markdown_features()
            && !range.is_empty()
            && !text.is_empty()
            && text != "\n"
            && self.replace_wysiwyg_selection(range.clone(), text)
        {
            return;
        }
        if text.trim().is_empty() {
            if mode == ViewMode::Wysiwyg
                && self.file_type.supports_markdown_features()
                && range.is_empty()
                && !text.is_empty()
            {
                let close_hit = {
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
                    pair.map(|pair| {
                        let pending = if self.has_pending_inline_formats() {
                            self.pending_formats
                        } else {
                            pending_formats_at(&parsed, range.start)
                        };
                        let at = span_close_end(&parsed, &pair);
                        (at, pending)
                    })
                };
                if let Some((at, pending)) = close_hit {
                    self.edit(at..at, text);
                    self.pending_formats = pending;
                    self.caret
                        .move_to(floor_boundary(&self.content, at + text.len()));
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
            && self.file_type.supports_markdown_features()
            && range.is_empty()
            && !text.is_empty()
            && text != "\n"
        {
            let pf = self.pending_formats;
            if self.has_pending_inline_formats() {
                let extended = {
                    let parsed = self.parsed();
                    // The armed formats extend a span ending right at the caret
                    // (with or without a trailing space) or continue a subset of its formats.
                    extend_or_splice_span(&self.content, &parsed, &pf, range.start, text)
                };
                if let Some((run_range, repl, caret)) = extended {
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
        let mut refence_pair: Option<InlinePair> = None;
        if mode == ViewMode::Wysiwyg
            && self.file_type.supports_markdown_features()
            && range.is_empty()
            && !text.is_empty()
        {
            let parsed = self.parsed();
            if let Some(pair) = line_pair_refs(&parsed, range.start)
                .iter()
                .rev()
                .find(|p| range.start >= p.inner.start && range.start <= p.inner.end)
            {
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
                        refence_pair = Some(pair.clone());
                    }
                    _ => {}
                }
            }
        }
        if let Some(pair) = refence_pair {
            // The backtick is literal content: re-fence the span
            // with a longer run so it stays intact.
            self.refence_code_span(&pair, range.start, text);
            return;
        }
        let text = escaped.as_deref().unwrap_or(text);

        // A literally typed `<` must not start an HTML tag (`<u>a<b>c</u>`);
        // escape it so it renders as text. Skipped inside code spans, whose
        // content is literal.
        if mode == ViewMode::Wysiwyg
            && self.file_type.supports_markdown_features()
            && range.is_empty()
            && text.chars().count() == 1
        {
            let in_code = {
                let parsed = self.parsed();
                line_pair_refs(&parsed, range.start).iter().any(|p| {
                    p.kind == InlineTagKind::Code
                        && range.start > p.inner.start
                        && range.start < p.inner.end
                })
            };
            if !in_code {
                let next_char = self.content[range.start..].chars().next();
                let starts_tag = next_char.is_some_and(|c| c.is_ascii_alphabetic() || c == '/');
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
    pub(super) fn replace_wysiwyg_selection(&mut self, range: Range<usize>, text: &str) -> bool {
        let (replace_start, replace_end, slices) = {
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
            let segs: Vec<AbsStyledSegment> = parsed
                .lines_overlapping(replace_start..replace_end)
                .iter()
                .flat_map(|l| l.abs_styled_segments())
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
            (replace_start, replace_end, slices)
        };
        let selected: Vec<&StyledSlice> = slices.iter().filter(|s| s.is_selected).collect();
        let typed_flags =
            if !selected.is_empty() && selected.iter().all(|s| s.flags == selected[0].flags) {
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
    pub(super) fn refence_code_span(&mut self, pair: &InlinePair, caret: usize, text: &str) {
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

    pub fn smart_backspace_in_mode(&mut self, mode: ViewMode) -> bool {
        self.pending_formats = ActiveFormats::default();
        if mode == ViewMode::Raw || !self.file_type.supports_markdown_features() {
            return self.smart_backspace();
        }
        if self.is_read_only() {
            return false;
        }
        if self.picture_remove(false)
            || self.picture_guard_join_back()
            || self.block_marker_backspace()
        {
            return true;
        }
        if !self.selection_range().is_empty() {
            let range = self.selection_range();
            self.edit_removing(range);
            return true;
        }
        let cursor = self.cursor_offset();
        if cursor == 0 {
            return false;
        }
        enum BackspaceAction {
            DeleteSpan {
                start: usize,
                end: usize,
            },
            DeleteLastGrapheme {
                prev_g: usize,
                prev_vis: usize,
                caret_after: usize,
            },
            Fallthrough,
        }
        let action = {
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
            let mut found = BackspaceAction::Fallthrough;
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
                        found = BackspaceAction::DeleteSpan {
                            start,
                            end: group_end,
                        };
                        break;
                    } else if cursor >= pair.inner.end && cursor <= group_end {
                        // Cursor was right outside the closing run -> delete the last
                        // visible grapheme inside `word`; never split a delimiter.
                        let deleted = prev_vis - prev_g;
                        found = BackspaceAction::DeleteLastGrapheme {
                            prev_g,
                            prev_vis,
                            caret_after: group_end - deleted,
                        };
                        break;
                    }
                }
            }
            found
        };

        match action {
            BackspaceAction::DeleteSpan { start, end } => {
                self.edit(start..end, "");
                self.caret.move_to(start);
                self.refresh_cursor();
                true
            }
            BackspaceAction::DeleteLastGrapheme {
                prev_g,
                prev_vis,
                caret_after,
            } => {
                self.edit(prev_g..prev_vis, "");
                self.caret
                    .move_to(floor_boundary(&self.content, caret_after));
                self.refresh_cursor();
                true
            }
            BackspaceAction::Fallthrough => self.smart_backspace(),
        }
    }

    pub fn delete_forward_in_mode(&mut self, mode: ViewMode) {
        if self.is_read_only() {
            return;
        }
        if mode == ViewMode::Wysiwyg
            && (self.picture_remove(true)
                || self.picture_guard_join()
                || self.block_marker_delete())
        {
            return;
        }
        if !self.selection_range().is_empty() {
            let range = self.selection_range();
            self.edit_removing(range);
            return;
        }
        let cursor = self.cursor_offset();
        if cursor >= self.content.len() {
            return;
        }
        if mode == ViewMode::Wysiwyg && self.file_type.supports_markdown_features() {
            enum DeleteAction {
                Done,
                JoinLines {
                    range: Range<usize>,
                },
                DeleteSpan {
                    start: usize,
                    end: usize,
                },
                DeleteChar {
                    start: usize,
                    end: usize,
                    caret_after: usize,
                },
                Raw,
            }
            let action = {
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
                                .get(pline.line_index as usize + 1)
                                .is_some_and(|n| {
                                    matches!(
                                        n.kind,
                                        LineKind::Heading { .. }
                                            | LineKind::Blockquote
                                            | LineKind::ListItem { .. }
                                    )
                                });
                            let own_text = pline.source.start
                                + block_prefix_in_parsed(
                                    &parsed,
                                    &self.content,
                                    pline.source.start,
                                );
                            let strip = if text_line(&pline.kind)
                                && next_is_block
                                && own_text < cursor
                            {
                                block_prefix_in_parsed(&parsed, &self.content, after + break_len)
                            } else {
                                0
                            };
                            let join_start = if !matches!(pline.kind, LineKind::Heading { .. })
                                && vend < pline.source.end
                            {
                                vend
                            } else {
                                after
                            };
                            DeleteAction::JoinLines {
                                range: join_start..after + break_len + strip,
                            }
                        } else {
                            DeleteAction::Done
                        }
                    } else {
                        // Delete skips hidden delimiters like Backspace does, and
                        // drops a span whose last visible grapheme it removes
                        // instead of leaving lone markers behind.
                        let next_vis = next_visible_pos(&parsed, cursor);
                        if next_vis >= self.content.len() {
                            return;
                        }
                        let mut found = None;
                        for pair in line_pair_refs(&parsed, next_vis).iter().rev() {
                            if next_vis < pair.inner.start || next_vis >= pair.inner.end {
                                continue;
                            }
                            let only = previous_grapheme(&self.content, pair.inner.end)
                                <= pair.inner.start;
                            if only {
                                found = Some(DeleteAction::DeleteSpan {
                                    start: span_open_start(&parsed, pair),
                                    end: span_close_end(&parsed, pair),
                                });
                            } else {
                                let g_end = next_grapheme(&self.content, next_vis);
                                found = Some(DeleteAction::DeleteChar {
                                    start: next_vis,
                                    end: g_end,
                                    caret_after: cursor.min(next_vis),
                                });
                            }
                            break;
                        }
                        found.unwrap_or_else(|| {
                            let g_end = next_grapheme(&self.content, next_vis);
                            DeleteAction::DeleteChar {
                                start: next_vis,
                                end: g_end,
                                caret_after: cursor.min(next_vis),
                            }
                        })
                    }
                } else {
                    DeleteAction::Raw
                }
            };

            match action {
                DeleteAction::Done => return,
                DeleteAction::JoinLines { range } => {
                    self.edit_removing(range);
                    return;
                }
                DeleteAction::DeleteSpan { start, end } => {
                    self.edit(start..end, "");
                    self.caret.move_to(floor_boundary(&self.content, start));
                    self.refresh_cursor();
                    return;
                }
                DeleteAction::DeleteChar {
                    start,
                    end,
                    caret_after,
                } => {
                    self.edit(start..end, "");
                    self.caret
                        .move_to(floor_boundary(&self.content, caret_after));
                    self.refresh_cursor();
                    return;
                }
                DeleteAction::Raw => {}
            }
        }
        let next = next_grapheme(&self.content, cursor);
        self.edit(cursor..next, "");
    }

    pub fn delete_word_left(&mut self) {
        if self.is_read_only() {
            return;
        }
        if self.picture_remove(false)
            || self.picture_guard_join_back()
            || self.block_marker_backspace()
        {
            return;
        }
        if !self.caret.range.is_empty() {
            self.edit(self.selection_range(), "");
        } else {
            let cursor = self.cursor_offset();
            match self.visible_word_step(cursor, false) {
                Some(super::word_nav::WordStep::Range(range)) => {
                    self.delete_visible_range(range);
                    return;
                }
                Some(super::word_nav::WordStep::Edge) => {
                    self.smart_backspace_in_mode(ViewMode::Wysiwyg);
                    return;
                }
                None => {}
            }
            let target = crate::model::text_ops::word_left(&self.content, cursor);
            if target < cursor {
                self.edit(target..cursor, "");
            }
        }
    }

    pub fn delete_word_right(&mut self) {
        if self.is_read_only() {
            return;
        }
        if self.picture_remove(true) || self.picture_guard_join() || self.block_marker_delete() {
            return;
        }
        if !self.caret.range.is_empty() {
            self.edit(self.selection_range(), "");
        } else {
            let cursor = self.cursor_offset();
            match self.visible_word_step(cursor, true) {
                Some(super::word_nav::WordStep::Range(range)) => {
                    self.delete_visible_range(range);
                    return;
                }
                Some(super::word_nav::WordStep::Edge) => {
                    // A heading's hidden closer is never deleted by a word step.
                    let before_closer = self.parsed().line_at_offset(cursor).is_some_and(|line| {
                        let vend = visible_line_end(line, self.view_mode);
                        vend < line.source.end && cursor <= vend
                    });
                    if !before_closer {
                        // At a line end: join the next line (guards apply),
                        // then take its first visible word, as in a raw view.
                        let ending = line_ending_len(&self.content, cursor);
                        let before_len = self.content.len();
                        self.delete_forward_in_mode(ViewMode::Wysiwyg);
                        if ending > 0
                            && self.content.len() + ending == before_len
                            && self.cursor_offset() == cursor
                        {
                            if let Some(super::word_nav::WordStep::Range(range)) =
                                self.visible_word_step(cursor, true)
                            {
                                self.delete_visible_range(range);
                            }
                        }
                    }
                    return;
                }
                None => {}
            }
            let mut target = crate::model::text_ops::word_right(&self.content, cursor);
            if self.view_mode == ViewMode::Wysiwyg && self.file_type.supports_markdown_features() {
                if let Some(line) = self.parsed().line_at_offset(cursor) {
                    let vend = visible_line_end(line, self.view_mode);
                    if vend < line.source.end && cursor <= vend && target > vend {
                        target = vend;
                    }
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
            self.edit_removing(self.selection_range());
        } else {
            let cursor = self.cursor_offset();
            let target = crate::model::text_ops::home_offset(&self.content, cursor);
            let end = if self.view_mode == ViewMode::Wysiwyg
                && self.file_type.supports_markdown_features()
            {
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
            } else {
                cursor
            };
            if target < end {
                self.edit_removing(target..end);
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
            let raw = crate::model::text_ops::line_end(&self.content, cursor);
            let target = if self.view_mode == ViewMode::Wysiwyg
                && self.file_type.supports_markdown_features()
            {
                self.parsed().line_at_offset(cursor).map_or(raw, |line| {
                    let vend = visible_line_end(line, self.view_mode);
                    if cursor < vend {
                        let end = line.source.end;
                        end + line_ending_len(&self.content, end).saturating_sub(1)
                    } else {
                        vend
                    }
                })
            } else {
                raw
            };
            if target > cursor {
                self.edit(cursor..target, "");
            }
        }
    }
}

pub(super) fn cmd_selects_placeholder(cmd: &str) -> bool {
    matches!(
        cmd,
        "bold" | "italic" | "underline" | "strike" | "code" | "link"
    )
}

pub(super) fn placeholder_prefix_len(cmd: &str) -> usize {
    match cmd {
        "bold" => 2,
        "italic" | "code" => 1,
        "underline" => 3,
        "strike" => 2,
        "link" => 1,
        _ => 0,
    }
}

pub(super) fn placeholder_len(cmd: &str) -> usize {
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
