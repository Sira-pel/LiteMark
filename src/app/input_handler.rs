//! Platform text input (IME) bridge for the editor.

use super::*;

impl EntityInputHandler for LiteMarkApp {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let content = self.active_content();
        let mut range = range_from_utf16(&content, &range_utf16);
        range.start = range.start.min(content.len());
        range.end = range.end.min(content.len());
        if !content.is_char_boundary(range.start) || !content.is_char_boundary(range.end) {
            return None;
        }
        *adjusted_range = Some(range_to_utf16(&content, &range));
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
            range: range_to_utf16(&tab.content, &tab.selection_range()),
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
            .map(|range| range_to_utf16(&tab.content, range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(tab) = self.active_tab_mut() {
            tab.marked_range = None;
        }
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
        let content = self.active_content();
        let mut range = range_utf16
            .as_ref()
            .map(|range| range_from_utf16(&content, range))
            .or_else(|| self.active_tab().and_then(|tab| tab.marked_range.clone()))
            .or_else(|| self.active_tab().map(|tab| tab.selection_range()))
            .unwrap_or(0..0);

        // 1. In WYSIWYG mode, preserve block prefixes when typing at start of line
        if self.effective_view_mode() == ViewMode::Wysiwyg && range.is_empty() && text != "\n" {
            let cursor = range.start;
            let line_start = home_offset(&content, cursor);
            let line_end_pos = line_end(&content, cursor);
            let line = &content[line_start..line_end_pos];
            let prefix_len = block_prefix_len(line);
            if prefix_len > 0 && cursor < line_start + prefix_len {
                let target = line_start + prefix_len;
                range = target..target;
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
        if text.chars().count() == 1 {
            let ch = text.chars().next().unwrap();
            if !range.is_empty() {
                let closing = match ch {
                    '\'' | '`' if markdown_tab => Some(ch),
                    '*' | '_' | '~' if markdown_tab => Some(ch),
                    _ => code_pair_close(ch),
                };
                if let Some(close_ch) = closing {
                    let selected = &content[range.clone()];
                    let wrapped = format!("{}{}{}", ch, selected, close_ch);
                    if let Some(tab) = self.active_tab_mut() {
                        tab.edit(range.clone(), &wrapped);
                        tab.caret.range = (range.start + 1)..(range.start + 1 + selected.len());
                        tab.caret.reversed = false;
                        tab.refresh_cursor();
                    }
                    self.refresh_find();
                    window.refresh();
                    cx.notify();
                    return;
                }
            } else {
                let cursor = range.start;
                // Over-typing closing bracket or quote
                let over_types = matches!(ch, ')' | ']' | '}' | '"')
                    || (markdown_tab && matches!(ch, '\'' | '`'));
                if over_types && cursor < content.len() && content[cursor..].starts_with(ch) {
                    if let Some(tab) = self.active_tab_mut() {
                        tab.caret.move_to(cursor + ch.len_utf8());
                        tab.refresh_cursor();
                    }
                    window.refresh();
                    cx.notify();
                    return;
                }

                // Auto-closing brackets and backticks
                let auto_pair = match ch {
                    '(' => Some("()"),
                    '[' => Some("[]"),
                    '{' => Some("{}"),
                    // A third backtick starts a fence: no closer.
                    '`' if markdown_tab && !content[..cursor].ends_with("``") => Some("``"),
                    '"' => {
                        let prev_is_alnum = cursor > 0
                            && content[..cursor]
                                .chars()
                                .next_back()
                                .is_some_and(|c| c.is_alphanumeric());
                        if !prev_is_alnum {
                            Some("\"\"")
                        } else {
                            None
                        }
                    }
                    _ => None,
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
        let content = self.active_content();
        let range = range_utf16
            .as_ref()
            .map(|range| range_from_utf16(&content, range))
            .or_else(|| self.active_tab().and_then(|tab| tab.marked_range.clone()))
            .or_else(|| self.active_tab().map(|tab| tab.selection_range()))
            .unwrap_or(0..0);
        let start = range.start;
        if let Some(tab) = self.active_tab_mut() {
            tab.edit(range, new_text);
            if new_text.is_empty() {
                tab.marked_range = None;
            } else {
                let end = (start + new_text.len()).min(tab.content.len());
                tab.marked_range = Some(start..end);
            }
            if let Some(selected) = new_selected_range {
                let relative = range_from_utf16(new_text, &selected);
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
        let content = self.active_content();
        let range = range_from_utf16(&content, &range_utf16);
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
                        range.start >= l.offset_map.line_start && range.start <= l.offset_map.line_end
                    })
                    .or_else(|| cell.lines.first())
                {
                    let local_start = line.offset_map.source_to_display(range.start);
                    let local_end = line.offset_map.source_to_display(range.end);
                    let x0 = line.shaped.x_for_index(local_start);
                    let x1 = line.shaped.x_for_index(local_end);
                    return Some(Bounds::from_corners(
                        point(line.text_origin.x + x0, line.text_origin.y),
                        point(line.text_origin.x + x1.max(x0), line.text_origin.y + line.line_height),
                    ));
                }
                let local_start = cell.offset_map.source_to_display(range.start);
                let local_end = cell.offset_map.source_to_display(range.end);
                let x0 = cell.shaped.x_for_index(local_start);
                let x1 = cell.shaped.x_for_index(local_end);
                return Some(Bounds::from_corners(
                    point(cell.text_origin.x + x0, cell.text_origin.y),
                    point(cell.text_origin.x + x1.max(x0), cell.text_origin.y + row.line_height),
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
        Some(offset_to_utf16(&self.active_content(), utf8))
    }
}
