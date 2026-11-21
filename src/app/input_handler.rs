//! Platform text input (IME) bridge for the editor.

use super::*;
use crate::markdown::{InlineTagKind, LineKind};

fn ordered_range(range: Range<usize>) -> Range<usize> {
    if range.start <= range.end {
        range
    } else {
        range.end..range.start
    }
}

fn wysiwyg_selection_safe_to_wrap(
    parsed: &crate::markdown::ParsedDocument,
    content: &str,
    range: &Range<usize>,
    ch: char,
) -> bool {
    if content
        .get(range.clone())
        .is_some_and(|s| s.contains(['\n', '\r']))
    {
        return false;
    }
    for line in parsed.lines_overlapping(range.clone()) {
        if matches!(
            line.kind,
            LineKind::Heading { .. }
                | LineKind::ListItem { .. }
                | LineKind::Blockquote
                | LineKind::HorizontalRule
                | LineKind::CodeFence { .. }
                | LineKind::TableDelimiter
        ) && line.prefix_range.start < range.end
            && line.prefix_range.end > range.start
        {
            return false;
        }
        if let LineKind::TableRow { ref body, .. } = line.kind {
            if !body
                .cells
                .iter()
                .any(|c| range.start >= c.start && range.end <= c.end)
            {
                return false;
            }
        }
        for pair in &line.inline_pairs {
            let disjoint =
                range.end <= pair.open_delim.start || range.start >= pair.close_delim.end;
            let inside_inner = range.start >= pair.inner.start && range.end <= pair.inner.end;
            let encloses =
                range.start <= pair.open_delim.start && range.end >= pair.close_delim.end;
            if !(disjoint || inside_inner || encloses) {
                return false;
            }
            if ch == '`' && pair.kind == InlineTagKind::Code && inside_inner {
                return false;
            }
        }
    }
    true
}

impl EntityInputHandler for LiteMarkApp {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let tab = self.active_tab()?;
        let content = &tab.content;
        let mut range = ordered_range(tab.range_from_utf16(&range_utf16));
        range.start = range.start.min(content.len());
        range.end = range.end.min(content.len());
        if !content.is_char_boundary(range.start) || !content.is_char_boundary(range.end) {
            return None;
        }
        *adjusted_range = Some(tab.range_to_utf16(&range));
        Some(content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let tab = self.active_tab()?;
        Some(UTF16Selection {
            range: tab.range_to_utf16(&tab.selection_range()),
            reversed: tab.caret.reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        let tab = self.active_tab()?;
        tab.marked_range
            .as_ref()
            .map(|range| tab.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(tab) = self.active_tab_mut() {
            tab.marked_range = None;
            if tab.ime_opened_line.take().is_some() {
                tab.seal_history();
            }
        }
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let had_ime_line = self
            .active_tab_mut()
            .is_some_and(|tab| tab.ime_opened_line.take().is_some());
        if !text.is_empty() && !self.pointer_hidden {
            self.pointer_hidden = true;
            cx.notify();
        }
        if self.table_drag.is_some() || self.table_drag_press_held {
            let (drag, insert) = clear_drag_for_key(self.table_drag.take(), "key", text);
            self.table_drag = drag;
            if insert.is_empty() || self.table_drag_press_held {
                window.refresh();
                cx.notify();
                return;
            }
        }
        if self.forward_keystroke_to_field(text, window, cx) || self.input_blocked() {
            return;
        }
        if self.active_tab().is_some_and(|tab| tab.is_read_only()) {
            self.show_error("PDF files are read-only.");
            window.refresh();
            cx.notify();
            return;
        }
        let mut range = range_utf16
            .as_ref()
            .and_then(|range| {
                self.active_tab()
                    .map(|tab| ordered_range(tab.range_from_utf16(range)))
            })
            .or_else(|| {
                self.active_tab()
                    .and_then(|tab| tab.marked_range.clone().map(ordered_range))
            })
            .or_else(|| self.active_tab().map(|tab| tab.selection_range()))
            .unwrap_or(0..0);

        // 1. In WYSIWYG mode, preserve block prefixes when typing at start of line
        if self.effective_view_mode() == ViewMode::Wysiwyg && range.is_empty() && text != "\n" {
            let cursor = range.start;
            if let Some(tab) = self.active_tab_mut() {
                if tab.file_type.supports_markdown_features() {
                    let parsed = tab.parsed();
                    if let Some(pline) = parsed.line_at_offset(cursor) {
                        if matches!(
                            pline.kind,
                            LineKind::Heading { .. }
                                | LineKind::ListItem { .. }
                                | LineKind::Blockquote
                        ) && cursor < pline.prefix_range.end
                        {
                            let target = pline.prefix_range.end;
                            range = target..target;
                        }
                    }
                }
            }
        }

        // 2. Handle Enter / Newline
        if text == "\n" {
            if let Some(tab) = self.active_tab_mut() {
                tab.smart_newline(false);
            }
            self.refresh_find();
            window.refresh();
            cx.notify();
            return;
        }

        // 2b. If WYSIWYG pending inline formats are armed, wrap newly typed text directly
        let has_pending = self
            .active_tab()
            .is_some_and(|tab| tab.has_pending_inline_formats());
        if self.effective_view_mode() == ViewMode::Wysiwyg && range.is_empty() && has_pending {
            let mode = self.effective_view_mode();
            if let Some(tab) = self.active_tab_mut() {
                tab.insert_text_in_mode(range, text, mode);
            }
            self.refresh_find();
            window.refresh();
            cx.notify();
            return;
        }

        // 3. Selection auto-wrapping or typing auto-pairing
        let markdown_tab = self
            .active_tab()
            .is_none_or(|tab| tab.file_type.supports_markdown_features());
        // Typing at a picture never wraps or pairs its Markdown: it goes
        // straight to `insert_text_in_mode`, which opens a line for it.
        let on_picture = self.effective_view_mode() == ViewMode::Wysiwyg
            && self
                .active_tab_mut()
                .is_some_and(|tab| tab.range_on_picture(&range));
        let has_pending_table_col = self
            .active_tab()
            .is_some_and(|tab| tab.pending_table_col.is_some());
        if text.chars().count() == 1 && !on_picture && !has_pending_table_col {
            let ch = text.chars().next().unwrap();
            let is_wysiwyg = self.effective_view_mode() == ViewMode::Wysiwyg && markdown_tab;
            if !range.is_empty() {
                let closing = match ch {
                    '\'' | '`' if markdown_tab => Some(ch),
                    '*' | '_' | '~' if markdown_tab => Some(ch),
                    _ => code_pair_close(ch),
                };
                if let Some(close_ch) = closing {
                    let safe_wrap = if is_wysiwyg {
                        self.active_tab_mut().is_none_or(|tab| {
                            let parsed = tab.parsed();
                            wysiwyg_selection_safe_to_wrap(&parsed, &tab.content, &range, ch)
                        })
                    } else {
                        true
                    };
                    if safe_wrap {
                        if let Some(tab) = self.active_tab_mut() {
                            if let Some(selected) = tab.content.get(range.clone()) {
                                let selected_len = selected.len();
                                let wrapped = format!("{ch}{selected}{close_ch}");
                                tab.edit(range.clone(), &wrapped);
                                tab.caret.range =
                                    (range.start + 1)..(range.start + 1 + selected_len);
                                tab.caret.reversed = false;
                                tab.refresh_cursor();
                            }
                        }
                        self.refresh_find();
                        window.refresh();
                        cx.notify();
                        return;
                    }
                }
            } else {
                let cursor = range.start;
                let (
                    at_hidden_or_closer,
                    in_code_span,
                    starts_with_ch,
                    ends_with_double_tick,
                    prev_is_alnum,
                ) = if let Some(tab) = self.active_tab_mut() {
                    let (at_hidden, in_code) = if is_wysiwyg {
                        let parsed = tab.parsed();
                        parsed
                            .line_at_offset(cursor)
                            .map_or((false, false), |line| {
                                let hidden = line.hidden_ranges.iter().any(|h| {
                                    if cursor > h.start && cursor < h.end {
                                        return true;
                                    }
                                    if cursor == h.start {
                                        let single_tick_closer =
                                            line.inline_pairs.iter().any(|p| {
                                                p.kind == InlineTagKind::Code
                                                    && p.close_delim == *h
                                                    && h.len() == 1
                                            });
                                        return !single_tick_closer;
                                    }
                                    false
                                }) || line.inline_pairs.iter().any(|p| {
                                    (p.kind == InlineTagKind::Link
                                        || (p.kind == InlineTagKind::Code
                                            && p.close_delim.len() > 1))
                                        && cursor >= p.close_delim.start
                                        && cursor < p.close_delim.end
                                });
                                let code = line.inline_pairs.iter().any(|p| {
                                    p.kind == InlineTagKind::Code
                                        && cursor >= p.inner.start
                                        && cursor <= p.inner.end
                                });
                                (hidden, code)
                            })
                    } else {
                        (false, false)
                    };
                    let starts =
                        cursor < tab.content.len() && tab.content[cursor..].starts_with(ch);
                    let double_tick = tab.content[..cursor].ends_with("``");
                    let alnum = cursor > 0
                        && tab.content[..cursor]
                            .chars()
                            .next_back()
                            .is_some_and(|c| c.is_alphanumeric());
                    (at_hidden, in_code, starts, double_tick, alnum)
                } else {
                    (false, false, false, false, false)
                };
                // Over-typing closing bracket or quote
                let over_types = matches!(ch, ')' | ']' | '}' | '"')
                    || (markdown_tab && matches!(ch, '\'' | '`'));
                if over_types && !at_hidden_or_closer && starts_with_ch {
                    if let Some(tab) = self.active_tab_mut() {
                        tab.caret.move_to(cursor + ch.len_utf8());
                        tab.refresh_cursor();
                    }
                    window.refresh();
                    cx.notify();
                    return;
                }

                // Auto-closing brackets and backticks
                let suppress_pair = at_hidden_or_closer || (ch == '`' && in_code_span);
                let auto_pair = if suppress_pair {
                    None
                } else {
                    match ch {
                        '(' => Some("()"),
                        '[' => Some("[]"),
                        '{' => Some("{}"),
                        // A third backtick starts a fence: no closer.
                        '`' if markdown_tab && !ends_with_double_tick => Some("``"),
                        '"' => {
                            if !prev_is_alnum {
                                Some("\"\"")
                            } else {
                                None
                            }
                        }
                        _ => None,
                    }
                };

                if let Some(pair) = auto_pair {
                    if let Some(tab) = self.active_tab_mut() {
                        tab.edit(range, pair);
                        tab.caret.move_to(cursor + ch.len_utf8());
                        tab.refresh_cursor();
                    }
                    self.refresh_find();
                    window.refresh();
                    cx.notify();
                    return;
                }
            }
        }

        let mode = self.effective_view_mode();
        if let Some(tab) = self.active_tab_mut() {
            tab.insert_text_in_mode(range, text, mode);
            // The committed text ends the group that began with the opened line.
            if had_ime_line {
                tab.seal_history();
            }
        }
        self.refresh_find();
        window.refresh();
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.table_drag.is_some() || self.table_drag_press_held {
            let _ = swallow_table_drag_input(&mut self.table_drag, self.table_drag_press_held);
            window.refresh();
            cx.notify();
            return;
        }
        if self.forward_keystroke_to_field(new_text, window, cx) {
            return;
        }
        if self.active_tab().is_some_and(|tab| tab.is_read_only()) {
            self.show_error("PDF files are read-only.");
            cx.notify();
            window.refresh();
            return;
        }
        let range = range_utf16
            .as_ref()
            .and_then(|range| {
                self.active_tab()
                    .map(|tab| ordered_range(tab.range_from_utf16(range)))
            })
            .or_else(|| {
                self.active_tab()
                    .and_then(|tab| tab.marked_range.clone().map(ordered_range))
            })
            .or_else(|| self.active_tab().map(|tab| tab.selection_range()))
            .unwrap_or(0..0);
        // Composing at a picture starts on a line of its own.
        let range = if self.effective_view_mode() == ViewMode::Wysiwyg && !new_text.is_empty() {
            self.active_tab_mut()
                .and_then(|tab| tab.picture_open_line(&range))
                .unwrap_or(range)
        } else {
            range
        };
        let range = if self.effective_view_mode() == ViewMode::Wysiwyg && !new_text.is_empty() {
            self.active_tab_mut()
                .and_then(|tab| {
                    let target_col = tab.pending_table_col.take()?;
                    if !range.is_empty() || !tab.file_type.supports_markdown_features() {
                        return None;
                    }
                    let row_start = tab
                        .parsed()
                        .line_at_offset(range.start)
                        .filter(|l| {
                            matches!(l.kind, LineKind::TableRow { .. })
                                && range.start == l.source.end
                        })
                        .map(|l| l.source.start)?;
                    tab.materialize_table_row_cell(row_start, target_col)
                        .map(|c| c.start..c.start)
                })
                .unwrap_or(range)
        } else {
            range
        };
        let start = range.start;
        if let Some(tab) = self.active_tab_mut() {
            tab.edit(range, new_text);
            if new_text.is_empty() {
                tab.cancel_picture_ime_line();
                tab.marked_range = None;
            } else {
                let end = (start + new_text.len()).min(tab.content.len());
                tab.marked_range = Some(start..end);
            }
            if let Some(selected) = new_selected_range {
                let relative = ordered_range(range_from_utf16(new_text, &selected));
                let sel_start = (start + relative.start).min(tab.content.len());
                let sel_end = (start + relative.end).min(tab.content.len());
                tab.caret.range = sel_start..sel_end;
                tab.caret.reversed = false;
                tab.refresh_cursor();
            }
        }
        self.refresh_find();
        window.refresh();
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = ordered_range(self.active_tab()?.range_from_utf16(&range_utf16));
        let paint = self.editor_paint.borrow();
        let row = paint
            .rows
            .iter()
            .rev()
            .find(|row| row.byte_start <= range.start)?;
        if !row.cells.is_empty() {
            if let Some(cell) = row
                .cells
                .iter()
                .find(|c| c.raw_range.start <= range.start && range.start <= c.raw_range.end)
            {
                if let Some(line) = cell
                    .lines
                    .iter()
                    .find(|l| {
                        range.start >= l.offset_map.line_start
                            && range.start <= l.offset_map.line_end
                    })
                    .or_else(|| cell.lines.first())
                {
                    let local_start = line.offset_map.source_to_display(range.start);
                    let local_end = line.offset_map.source_to_display(range.end);
                    let x0 = line.shaped.x_for_index(local_start);
                    let x1 = line.shaped.x_for_index(local_end);
                    return Some(Bounds::from_corners(
                        point(line.text_origin.x + x0, line.text_origin.y),
                        point(
                            line.text_origin.x + x1.max(x0),
                            line.text_origin.y + line.line_height,
                        ),
                    ));
                }
                let local_start = cell.offset_map.source_to_display(range.start);
                let local_end = cell.offset_map.source_to_display(range.end);
                let x0 = cell.shaped.x_for_index(local_start);
                let x1 = cell.shaped.x_for_index(local_end);
                return Some(Bounds::from_corners(
                    point(cell.text_origin.x + x0, cell.text_origin.y),
                    point(
                        cell.text_origin.x + x1.max(x0),
                        cell.text_origin.y + row.line_height,
                    ),
                ));
            }
        }
        let local_start = row.offset_map.source_to_display(range.start);
        let local_end = row.offset_map.source_to_display(range.end);
        let x0 = row.shaped.x_for_index(local_start);
        let x1 = row.shaped.x_for_index(local_end);
        Some(Bounds::from_corners(
            point(row.origin.x + x0, row.origin.y),
            point(row.origin.x + x1.max(x0), row.origin.y + row.line_height),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let utf8 = {
            let paint = self.editor_paint.borrow();
            index_for_position(&paint, point)?
        };
        Some(self.active_tab().map_or(0, |tab| tab.offset_to_utf16(utf8)))
    }
}
