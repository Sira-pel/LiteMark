//! Editor keyboard and mouse events, including table chrome.

use super::*;

impl LiteMarkApp {
    pub(super) fn editor_backspace(&mut self, _cx: &mut App) {
        self.dispatch_editor_input(EditorInput::Backspace);
    }

    pub(super) fn editor_delete(&mut self, _cx: &mut App) {
        self.dispatch_editor_input(EditorInput::Delete);
    }

    pub(super) fn editor_delete_word_left(&mut self, _cx: &mut App) {
        self.dispatch_editor_input(EditorInput::DeleteWordLeft);
    }

    pub(super) fn editor_delete_word_right(&mut self, _cx: &mut App) {
        self.dispatch_editor_input(EditorInput::DeleteWordRight);
    }

    pub(super) fn editor_delete_line_start(&mut self, _cx: &mut App) {
        self.dispatch_editor_input(EditorInput::DeleteLineStart);
    }

    pub(super) fn editor_delete_line_end(&mut self, _cx: &mut App) {
        self.dispatch_editor_input(EditorInput::DeleteLineEnd);
    }

    pub(super) fn editor_newline(&mut self, _cx: &mut App) {
        self.dispatch_editor_input(EditorInput::Newline);
    }

    pub(super) fn editor_plain_newline(&mut self, _cx: &mut App) {
        self.dispatch_editor_input(EditorInput::PlainNewline);
    }

    pub(super) fn editor_table_row_below(&mut self, _cx: &mut App) {
        self.dispatch_editor_input(EditorInput::TableRowBelow);
    }

    pub(super) fn editor_indent(&mut self, _cx: &mut App) {
        self.dispatch_editor_input(EditorInput::Indent);
    }

    pub(super) fn editor_outdent(&mut self, _cx: &mut App) {
        self.dispatch_editor_input(EditorInput::Outdent);
    }

    pub(super) fn editor_move(&mut self, _cx: &mut App, edit: impl FnOnce(&mut DocumentTab)) {
        if self.block_table_drag_edit() {
            return;
        }
        if !document_accepts_keystrokes(self.field_focus)
            || self.field_owns_keys
            || self.input_blocked()
        {
            return;
        }
        if let Some(tab) = self.active_tab_mut() {
            edit(tab);
        }
    }

    pub(super) fn block_table_drag_edit(&mut self) -> bool {
        swallow_table_drag_input(&mut self.table_drag, self.table_drag_press_held)
    }

    pub(super) fn dispatch_editor_input(&mut self, input: EditorInput) {
        let id = self.active_tab_id.clone();
        let field_focus = self.field_focus;
        let field_owns_keys = self.field_owns_keys;
        let view_mode = self.effective_view_mode();
        let blocked = self.input_blocked();
        let press_held = self.table_drag_press_held;
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) else {
            let _ = swallow_table_drag_input(&mut self.table_drag, press_held);
            return;
        };
        let outcome = dispatch_editor_input(
            &mut crate::editor_input::EditorInputState {
                tab,
                drag: &mut self.table_drag,
                press_held,
                field_focus,
                field_owns_keys,
                view_mode,
                input_blocked: blocked,
            },
            input,
        );
        match outcome {
            DispatchOutcome::ReadOnly => self.show_error("PDF files are read-only."),
            DispatchOutcome::Applied => {
                self.refresh_find();
            }
            DispatchOutcome::Swallowed | DispatchOutcome::Ignored => {}
        }
    }

    pub(super) fn dismiss_editor_context_menu(&mut self) {
        self.editor_context_menu = None;
        self.table_action_anchor = None;
    }

    pub(super) fn table_chrome_interactive(&self) -> bool {
        self.effective_view_mode() == ViewMode::Wysiwyg
            && !self.active_tab().is_some_and(|tab| tab.is_read_only())
    }

    /// A hover control click edits the table and leaves the caret alone until the op runs.
    pub(super) fn handle_table_chrome_click(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.table_chrome_interactive() {
            return false;
        }
        let hit = {
            let paint = self.editor_paint.borrow();
            let occupied = occupied_content(&paint.rows);
            hit_test_occupied(&paint.tables, event.position, &occupied)
        };
        let Some(hit) = hit else {
            return false;
        };
        self.focus_handle.focus(window);
        self.applied_focus = Some(FocusSlot::Editor);
        self.field_focus = TextInput::Editor;
        self.field_owns_keys = false;
        self.selecting = false;
        self.sidebar_menu = None;
        let anchor = {
            let hover = self.table_hover.clone();
            let paint = self.editor_paint.borrow();
            action_anchor(&paint.tables, &hit, hover.as_ref())
        };
        match hit.kind {
            TableHitKind::ColGrip => {
                self.dismiss_editor_context_menu();
                self.table_drag_press_held = true;
                self.table_drag = Some(TableDrag {
                    source: hit.source,
                    kind: TableDragKind::Column,
                    from: hit.index,
                    anchor,
                    gap: hit.index,
                    drop_visible: true,
                });
            }
            TableHitKind::RowGrip => {
                self.dismiss_editor_context_menu();
                if hit.index != 0 {
                    self.table_drag_press_held = true;
                    self.table_drag = Some(TableDrag {
                        source: hit.source,
                        kind: TableDragKind::Row,
                        from: hit.index,
                        anchor,
                        gap: hit.index,
                        drop_visible: true,
                    });
                }
            }
            TableHitKind::ColAlign => {
                self.dismiss_editor_context_menu();
                self.table_drag = None;
                self.clear_table_drag_press();
                if let Some(tab) = self.active_tab_mut() {
                    let _ = tab.apply_table_op_at(anchor, TableOp::SetAlign(next_align(hit.align)));
                }
                self.retarget_table_hover();
                self.refresh_find();
            }
            TableHitKind::ColPlus => {
                self.dismiss_editor_context_menu();
                self.table_drag = None;
                self.clear_table_drag_press();
                if let Some(tab) = self.active_tab_mut() {
                    let _ = tab.apply_table_op_at(anchor, TableOp::InsertColRight);
                }
                self.retarget_table_hover();
                self.refresh_find();
            }
            TableHitKind::RowPlus => {
                self.dismiss_editor_context_menu();
                self.table_drag = None;
                self.clear_table_drag_press();
                if let Some(tab) = self.active_tab_mut() {
                    let _ = tab.apply_table_op_at(anchor, TableOp::InsertRowBelow);
                }
                self.retarget_table_hover();
                self.refresh_find();
            }
            TableHitKind::Corner => {
                self.table_drag = None;
                self.clear_table_drag_press();
                self.open_menu = None;
                self.table_action_anchor = Some(anchor);
                self.editor_context_menu = Some(EditorContextMenu {
                    position: event.position,
                    is_table: true,
                });
            }
        }
        self.table_pressed = true;
        window.refresh();
        cx.notify();
        true
    }

    pub(super) fn track_table_hover(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        if !self.table_chrome_interactive() {
            if self.table_hover.is_some()
                || self.table_drag.is_some()
                || self.table_hot.is_some()
            {
                self.table_hover = None;
                self.table_hot = None;
                self.table_drag = None;
                self.table_drag_press_held = false;
                cx.notify();
            }
            return;
        }
        let previous = self.table_hover.clone();
        let (hover, hot) = {
            let paint = self.editor_paint.borrow();
            let occupied = occupied_content(&paint.rows);
            let hit = hit_test_occupied(&paint.tables, at, &occupied);
            let hover = hover_from_hit(&paint.tables, at, previous.as_ref(), hit.as_ref());
            (hover, hit.as_ref().map(TableHot::from))
        };
        if self.table_hover != hover || self.table_hot != hot {
            self.table_hover = hover;
            self.table_hot = hot;
            cx.notify();
        }
    }

    pub(super) fn editor_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_table_drag_press();
        if let Some(drag) = self.table_drag.take() {
            let tables = self.editor_paint.borrow().tables.clone();
            let moved = self
                .active_tab_mut()
                .is_some_and(|tab| commit_drag_release(tab, &tables, &drag, event.position));
            if moved {
                self.retarget_table_hover();
                self.refresh_find();
            }
        }
        if self.selecting {
            self.selecting = false;
        }
        window.refresh();
        cx.notify();
    }

    pub(super) fn editor_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A new press ends any earlier table-drag hold, including one whose
        // mouse-up the hovered root never saw. A grip click re-arms it below.
        self.clear_table_drag_press();
        // Clicking the editor hands keyboard focus back from the sidebar.
        self.sidebar_keyboard = false;
        self.workspace_menu_open = false;
        // The inline create row stops its own presses, so any press that gets
        // here is a click away from it.
        if self.create_draft.is_some() {
            self.cancel_create(cx);
        }
        // A click outside the sidebar, or a plain click that isn't a
        // Shift+click, ends a multi-selection. A press inside the sidebar keeps
        // it: that press may be the start of dragging the whole selection, and
        // clicking a row clears it itself.
        let in_sidebar = self.show_sidebar && event.position.x < crate::theme::sidebar_width(window.viewport_size().width);
        if !window.modifiers().shift && !in_sidebar {
            self.multi_selection = None;
            self.selected_target = None;
        }
        if self.input_blocked() || window.default_prevented() {
            return;
        }
        if self.handle_table_chrome_click(event, window, cx) {
            return;
        }
        if self.effective_view_mode() == ViewMode::Wysiwyg {
            let checkbox_range = {
                let paint = self.editor_paint.borrow();
                checkbox_for_position(&paint, event.position)
            };
            if let Some(range) = checkbox_range {
                self.table_drag = None;
                self.clear_table_drag_press();
                if let Some(tab) = self.active_tab_mut() {
                    if !tab.is_read_only() && range.end <= tab.content.len() {
                        let current = &tab.content[range.clone()];
                        let replacement = if current == "[ ]" { "[x]" } else { "[ ]" };
                        tab.edit(range, replacement);
                    }
                }
                window.refresh();
                cx.notify();
                return;
            }
        }

        let (offset, clicked_link) = {
            let paint = self.editor_paint.borrow();
            let off = index_for_position(&paint, event.position);
            let link = off.and_then(|o| link_for_offset(&paint, o));
            (off, link)
        };
        let Some(offset) = offset else {
            return;
        };

        if (event.modifiers.control || event.modifiers.platform)
            && self.effective_view_mode() == ViewMode::Wysiwyg
        {
            if let Some(url) = clicked_link {
                cx.open_url(&url);
                return;
            }
        }

        self.sidebar_menu = None;
        self.dismiss_editor_context_menu();
        self.table_drag = None;
        self.clear_table_drag_press();
        self.focus_handle.focus(window);
        self.applied_focus = Some(FocusSlot::Editor);
        self.field_focus = TextInput::Editor;
        self.field_owns_keys = false;
        if let Some(tab) = self.active_tab_mut() {
            if event.click_count >= 3 {
                tab.place(offset, false);
                tab.select_line();
            } else if event.click_count == 2 {
                tab.place(offset, false);
                tab.select_word();
            } else {
                tab.place(offset, event.modifiers.shift);
            }
        }
        self.selecting = event.click_count <= 1;
        window.refresh();
        cx.notify();
    }

    pub(super) fn editor_mouse_move(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        // A move with no button means the originating press is over. On X11
        // a release outside the window is never delivered (no grab); the
        // first in-window move after that has pressed_button None. Wayland's
        // implicit grab already delivered MouseUp (on_mouse_up_out) before
        // Leave. Windows SetCapture also delivers MouseUp outside.
        if event.pressed_button.is_none()
            && (self.table_drag.is_some() || self.table_drag_press_held)
        {
            self.table_drag = None;
            self.clear_table_drag_press();
            cx.notify();
        }
        if self.input_blocked() {
            return;
        }
        if self.table_drag.is_some() {
            let drag = self.table_drag.clone().expect("drag");
            let gap = {
                let paint = self.editor_paint.borrow();
                drag_gap_if_inside(&paint.tables, &drag, event.position)
            };
            if let Some(drag) = self.table_drag.as_mut() {
                let before = (drag.gap, drag.drop_visible);
                update_drag_gap(drag, gap);
                if (drag.gap, drag.drop_visible) != before {
                    cx.notify();
                }
            }
            return;
        }
        self.track_table_hover(event.position, cx);
        if !self.selecting {
            return;
        }
        let offset = {
            let paint = self.editor_paint.borrow();
            index_for_position(&paint, event.position)
        };
        let Some(offset) = offset else {
            return;
        };
        if let Some(tab) = self.active_tab_mut() {
            tab.place(offset, true);
        }
        cx.notify();
    }

    pub(super) fn editor_right_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.table_drag = None;
        self.clear_table_drag_press();
        if self.input_blocked() || window.default_prevented() {
            return;
        }

        let offset = {
            let paint = self.editor_paint.borrow();
            index_for_position(&paint, event.position)
        };
        let Some(offset) = offset else {
            self.dismiss_editor_context_menu();
            return;
        };

        self.sidebar_menu = None;
        self.table_action_anchor = None;
        self.focus_handle.focus(window);
        self.applied_focus = Some(FocusSlot::Editor);
        self.field_focus = TextInput::Editor;
        self.field_owns_keys = false;

        if let Some(tab) = self.active_tab_mut() {
            let sel = tab.selection_range();
            if sel.is_empty() || !sel.contains(&offset) {
                tab.place(offset, false);
            }
        }

        let is_table = self.active_tab().is_some_and(|tab| tab.active_formats.table);
        self.open_menu = None;
        self.editor_context_menu = Some(EditorContextMenu {
            position: event.position,
            is_table,
        });

        window.refresh();
        cx.notify();
    }

    pub(super) fn active_content(&mut self) -> SharedString {
        let Some(tab) = self.tabs.iter().find(|t| t.id == self.active_tab_id) else {
            self.content_snapshot = None;
            return SharedString::default();
        };
        if let Some((id, revision, len, text)) = &self.content_snapshot {
            if *id == tab.id && *revision == tab.revision && *len == tab.content.len() {
                return text.clone();
            }
        }
        let text = SharedString::new(tab.content.as_str());
        self.content_snapshot = Some((
            tab.id.clone(),
            tab.revision,
            tab.content.len(),
            text.clone(),
        ));
        text
    }

    pub(super) fn editor_surface(
        &mut self,
        cx: &mut Context<Self>,
        theme: &Theme,
        show_line_numbers: bool,
    ) -> AnyElement {
        let zoom = self.settings.zoom_level as f32 / 100.0;
        let view_mode = self.effective_view_mode();
        let markdown_tab = self
            .active_tab()
            .is_none_or(|tab| tab.file_type.supports_markdown_features());
        let show_line_numbers = show_line_numbers || !markdown_tab;
        let content = self.active_content();
        let active_id = self.active_tab_id.clone();
        for tab in self.tabs.iter_mut().filter(|t| t.id != active_id) {
            tab.release_parse_cache();
        }
        let (parsed, selection, marked) = if let Some(tab) = self.active_tab_mut() {
            (
                tab.parsed(),
                tab.selection_range(),
                tab.marked_range.clone(),
            )
        } else {
            (Arc::new(parse_document("")), 0..0, None)
        };
        let font_family = match view_mode {
            ViewMode::Wysiwyg => Theme::FONT_PROSE.into(),
            ViewMode::Raw => Theme::FONT_MONO.into(),
        };
        EditorElement {
            entity: cx.entity(),
            focus: self.focus_handle.clone(),
            paint: self.editor_paint.clone(),
            content,
            parsed,
            view_mode,
            selection,
            marked,
            find_hits: self.find_hits.clone(),
            active_find: self.find_index,
            show_line_numbers,
            word_wrap: self.settings.word_wrap,
            tab_width: if markdown_tab { 0 } else { TAB_DISPLAY_COLUMNS },
            centered: markdown_tab && self.settings.content_alignment == ContentAlignment::Centered,
            max_measure: Some(sz::MEASURE_FALLBACK * zoom),
            font_size: px(self.settings.base_font_size * zoom),
            text_color: theme.text_primary.into(),
            strong_color: theme.text_strong.into(),
            muted_color: theme.text_muted.into(),
            faint_color: theme.text_faint.into(),
            h1_color: theme.h1.into(),
            h2_color: theme.h2.into(),
            code_color: theme.code_text.into(),
            code_bg: theme.tile_hover.into(),
            surface_bg: theme.bg_code.into(),
            border_color: theme.border_block.into(),
            link_color: theme.text_hi.into(),
            selection_color: theme.selection_bg.into(),
            find_color: find_match_color(theme.mode.is_dark(), false),
            cursor_color: theme.text_strong.into(),
            active_find_color: find_match_color(theme.mode.is_dark(), true),
            gutter_color: theme.text_muted.into(),
            gutter_bg: theme.bg_sidebar.into(),
            font_family,
            mono_font_family: Theme::FONT_MONO.into(),
            table_hover: self.table_hover.clone(),
            table_drag: self.table_drag.clone(),
            table_hot: self.table_hot.clone(),
            table_pressed: self.table_pressed,
            table_hover_fill: theme.bg_hover.into(),
            table_active_fill: theme.bg_active.into(),
        }
        .into_any_element()
    }
}

/// Translucent amber wash painted behind find matches, so the text on top
/// keeps its own color and stays readable. The current match is stronger.
fn find_match_color(dark: bool, active: bool) -> Hsla {
    let color = match (dark, active) {
        (false, false) => rgba(0xffd54f73),
        (false, true) => rgba(0xff9800a6),
        (true, false) => rgba(0xe6a70033),
        (true, true) => rgba(0xf59e0b6b),
    };
    color.into()
}
