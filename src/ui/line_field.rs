//! Single-line field used by Find & Replace. Implements [`EntityInputHandler`]
//! so typing and IME reach the text instead of a static label.

use std::ops::Range;
use std::sync::Arc;

use gpui::*;
use unicode_segmentation::UnicodeSegmentation;

type FieldChange = Arc<dyn Fn(&str, &mut App) + 'static>;
type FieldSubmit = Arc<dyn Fn(&str, bool, &mut App) + 'static>;
type FieldFocus = Arc<dyn Fn(&mut App) + 'static>;

pub struct LineField {
    field_id: SharedString,
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    selecting: bool,
    on_change: FieldChange,
    on_submit: FieldSubmit,
    on_focus: FieldFocus,
    text_color: Hsla,
    placeholder_color: Hsla,
    bg: Hsla,
    border: Hsla,
}

impl LineField {
    pub fn new(
        cx: &mut Context<Self>,
        field_id: impl Into<SharedString>,
        placeholder: impl Into<SharedString>,
        on_change: FieldChange,
        on_submit: FieldSubmit,
        on_focus: FieldFocus,
    ) -> Self {
        Self {
            field_id: field_id.into(),
            focus_handle: cx.focus_handle(),
            content: "".into(),
            placeholder: placeholder.into(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            selecting: false,
            on_change,
            on_submit,
            on_focus,
            text_color: rgb(0xd4d4d4).into(),
            placeholder_color: rgb(0x8a8a8a).into(),
            bg: rgb(0x212121).into(),
            border: rgb(0x2e2e2e).into(),
        }
    }

    #[allow(dead_code)]
    pub fn content(&self) -> &str {
        &self.content
    }

    #[allow(dead_code)]
    pub fn set_text(&mut self, text: impl Into<SharedString>) {
        self.content = text.into();
        self.selected_range = 0..self.content.len();
        self.selection_reversed = false;
        self.marked_range = None;
    }

    pub fn focus(&self, window: &mut Window) {
        self.focus_handle.focus(window);
    }

    pub fn is_focused(&self, window: &Window) -> bool {
        self.focus_handle.is_focused(window)
    }

    pub fn set_colors(
        &mut self,
        text: impl Into<Hsla>,
        placeholder: impl Into<Hsla>,
        bg: impl Into<Hsla>,
        border: impl Into<Hsla>,
    ) {
        self.text_color = text.into();
        self.placeholder_color = placeholder.into();
        self.bg = bg.into();
        self.border = border.into();
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn notify_change(&mut self, cx: &mut Context<Self>) {
        let text = self.content.to_string();
        let callback = self.on_change.clone();
        callback(&text, cx);
        cx.notify();
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let head = self.cursor_offset();
        if offset < head {
            self.selected_range = offset..head;
            self.selection_reversed = true;
        } else {
            self.selected_range = head..offset;
            self.selection_reversed = false;
        }
        cx.notify();
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content[..offset.min(self.content.len())]
            .grapheme_indices(true)
            .next_back()
            .map(|(index, _)| index)
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        let offset = offset.min(self.content.len());
        self.content[offset..]
            .grapheme_indices(true)
            .nth(1)
            .map(|(index, _)| offset + index)
            .unwrap_or(self.content.len())
    }

    fn index_for_mouse(&self, position: Point<Pixels>) -> usize {
        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };
        if self.content.is_empty() {
            return 0;
        }
        line.closest_index_for_x((position.x - bounds.left()).max(px(0.0)))
            .min(self.content.len())
    }

    fn left(&mut self, _: &crate::actions::EditorLeft, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let offset = self.previous_boundary(self.cursor_offset());
            self.move_to(offset, cx);
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    fn right(&mut self, _: &crate::actions::EditorRight, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let offset = self.next_boundary(self.cursor_offset());
            self.move_to(offset, cx);
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    fn select_left(
        &mut self,
        _: &crate::actions::EditorSelectLeft,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.previous_boundary(self.cursor_offset());
        self.select_to(offset, cx);
    }

    fn select_right(
        &mut self,
        _: &crate::actions::EditorSelectRight,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.next_boundary(self.cursor_offset());
        self.select_to(offset, cx);
    }

    fn home(&mut self, _: &crate::actions::EditorHome, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &crate::actions::EditorEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn select_home(&mut self, _: &crate::actions::EditorSelectHome, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(0, cx);
    }

    fn select_end(&mut self, _: &crate::actions::EditorSelectEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.content.len(), cx);
    }

    fn word_left(&mut self, _: &crate::actions::EditorWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let offset = crate::model::text_ops::word_left(&self.content, self.cursor_offset());
            self.move_to(offset, cx);
        } else {
            let offset = crate::model::text_ops::word_left(&self.content, self.selected_range.start);
            self.move_to(offset, cx);
        }
    }

    fn word_right(&mut self, _: &crate::actions::EditorWordRight, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let offset = crate::model::text_ops::word_right(&self.content, self.cursor_offset());
            self.move_to(offset, cx);
        } else {
            let offset = crate::model::text_ops::word_right(&self.content, self.selected_range.end);
            self.move_to(offset, cx);
        }
    }

    fn select_word_left(
        &mut self,
        _: &crate::actions::EditorSelectWordLeft,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = crate::model::text_ops::word_left(&self.content, self.cursor_offset());
        self.select_to(offset, cx);
    }

    fn select_word_right(
        &mut self,
        _: &crate::actions::EditorSelectWordRight,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = crate::model::text_ops::word_right(&self.content, self.cursor_offset());
        self.select_to(offset, cx);
    }

    fn backspace(
        &mut self,
        _: &crate::actions::EditorBackspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            let offset = self.previous_boundary(self.cursor_offset());
            self.select_to(offset, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(
        &mut self,
        _: &crate::actions::EditorDelete,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            let offset = self.next_boundary(self.cursor_offset());
            self.select_to(offset, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete_word_left(
        &mut self,
        _: &crate::actions::EditorDeleteWordLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            let offset = crate::model::text_ops::word_left(&self.content, self.cursor_offset());
            self.select_to(offset, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete_word_right(
        &mut self,
        _: &crate::actions::EditorDeleteWordRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            let offset = crate::model::text_ops::word_right(&self.content, self.cursor_offset());
            self.select_to(offset, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete_line_start(
        &mut self,
        _: &crate::actions::EditorDeleteLineStart,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            self.select_to(0, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete_line_end(
        &mut self,
        _: &crate::actions::EditorDeleteLineEnd,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            self.select_to(self.content.len(), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn select_all(
        &mut self,
        _: &crate::actions::SelectAll,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selected_range = 0..self.content.len();
        self.selection_reversed = false;
        window.refresh();
        cx.notify();
    }

    fn paste(&mut self, _: &crate::actions::Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            let flat = text.replace(['\n', '\r'], " ");
            self.replace_text_in_range(None, &flat, window, cx);
        }
    }

    fn copy(&mut self, _: &crate::actions::Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &crate::actions::Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn submit(
        &mut self,
        _: &crate::actions::FindNextMatch,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = self.content.to_string();
        let callback = self.on_submit.clone();
        callback(&text, false, cx);
    }

    fn submit_prev(
        &mut self,
        _: &crate::actions::FindPrevMatch,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = self.content.to_string();
        let callback = self.on_submit.clone();
        callback(&text, true, cx);
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            return;
        }
        // Capture phase: claim the click before a later sibling (the editor
        // or a resize border) can focus itself and swallow the keystrokes.
        self.focus_handle.focus(window);
        window.prevent_default();
        cx.stop_propagation();
        let callback = self.on_focus.clone();
        callback(cx);
        window.refresh();
        self.selecting = true;
        let index = self.index_for_mouse(event.position);
        if event.modifiers.shift {
            self.select_to(index, cx);
        } else {
            self.move_to(index, cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.selecting = false;
        cx.notify();
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.selecting {
            let index = self.index_for_mouse(event.position);
            self.select_to(index, cx);
        }
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8 = 0usize;
        let mut utf16 = 0usize;
        for ch in self.content.chars() {
            if utf16 >= offset {
                break;
            }
            utf16 += ch.len_utf16();
            utf8 += ch.len_utf8();
        }
        utf8
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16 = 0usize;
        let mut utf8 = 0usize;
        for ch in self.content.chars() {
            if utf8 >= offset {
                break;
            }
            utf8 += ch.len_utf8();
            utf16 += ch.len_utf16();
        }
        utf16
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }

    /// Insert text without telling the owner. Used when the editor input
    /// handler is still installed and must hand the keystroke to this field.
    pub fn push_input(&mut self, new_text: &str, cx: &mut Context<Self>) -> String {
        self.splice_text(None, new_text);
        cx.notify();
        self.content.to_string()
    }

    fn splice_text(&mut self, range_utf16: Option<Range<usize>>, new_text: &str) {
        let new_text = new_text.replace(['\n', '\r'], "");
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());
        let range = clamp_range(&self.content, range);
        self.content =
            (self.content[..range.start].to_owned() + &new_text + &self.content[range.end..])
                .into();
        let cursor = range.start + new_text.len();
        self.selected_range = cursor..cursor;
        self.marked_range = None;
    }
}

impl EntityInputHandler for LineField {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.splice_text(range_utf16, new_text);
        self.notify_change(cx);
        window.refresh();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace_text_in_range(range_utf16, new_text, window, cx);
        if !new_text.is_empty() {
            let end = self.selected_range.end;
            let start = end.saturating_sub(new_text.replace(['\n', '\r'], "").len());
            self.marked_range = Some(start..end);
        }
        if let Some(range) = new_selected_range {
            let utf8 = self.range_from_utf16(&range);
            let base = self
                .marked_range
                .as_ref()
                .map(|mark| mark.start)
                .unwrap_or(0);
            let start = (base + utf8.start).min(self.content.len());
            let end = (base + utf8.end).min(self.content.len());
            self.selected_range = start..end.max(start);
        }
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        Some(Bounds::from_corners(
            point(
                bounds.left() + layout.x_for_index(range.start),
                bounds.top(),
            ),
            point(
                bounds.left() + layout.x_for_index(range.end),
                bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let layout = self.last_layout.as_ref()?;
        let bounds = self.last_bounds?;
        let utf8 = layout.closest_index_for_x((point.x - bounds.left()).max(px(0.0)));
        Some(self.offset_to_utf16(utf8))
    }
}

impl Render for LineField {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id(self.field_id.clone())
            .key_context("LineField")
            .track_focus(&self.focus_handle)
            .flex_1()
            .h(px(28.0))
            .px(px(8.0))
            .rounded(px(4.0))
            .bg(self.bg)
            .border_1()
            .border_color(self.border)
            .cursor_text()
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::select_home))
            .on_action(cx.listener(Self::select_end))
            .on_action(cx.listener(Self::word_left))
            .on_action(cx.listener(Self::word_right))
            .on_action(cx.listener(Self::select_word_left))
            .on_action(cx.listener(Self::select_word_right))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::delete_word_left))
            .on_action(cx.listener(Self::delete_word_right))
            .on_action(cx.listener(Self::delete_line_start))
            .on_action(cx.listener(Self::delete_line_end))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::submit))
            .on_action(cx.listener(Self::submit_prev))
            .capture_any_mouse_down(cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .child(FieldElement { field: cx.entity() })
    }
}

fn clamp_range(content: &str, mut range: Range<usize>) -> Range<usize> {
    range.start = range.start.min(content.len());
    range.end = range.end.min(content.len());
    if range.end < range.start {
        range.end..range.start
    } else {
        range
    }
}

struct FieldElement {
    field: Entity<LineField>,
}

struct FieldPrepaint {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl Element for FieldElement {
    type RequestLayoutState = ();
    type PrepaintState = FieldPrepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = px(26.0).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let field = self.field.read(cx);
        let content = field.content.clone();
        let selected = field.selected_range.clone();
        let cursor = field.cursor_offset();
        let (display, color) = if content.is_empty() {
            (field.placeholder.clone(), field.placeholder_color)
        } else {
            (content.clone(), field.text_color)
        };
        let font = window.text_style().font();
        let run = TextRun {
            len: display.len(),
            font,
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = if let Some(marked) = field.marked_range.clone() {
            vec![
                TextRun {
                    len: marked.start,
                    ..run.clone()
                },
                TextRun {
                    len: marked.end.saturating_sub(marked.start),
                    underline: Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: display.len().saturating_sub(marked.end),
                    ..run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect::<Vec<_>>()
        } else {
            vec![run]
        };
        let font_size = px(13.0);
        let line = window
            .text_system()
            .shape_line(display, font_size, &runs, None);
        // Caret spans the text line, not the whole 26px field.
        let caret_height = (font_size * 1.3).min(bounds.size.height);
        let (selection, cursor_quad) =
            if selected.is_empty() && !content.is_empty() || content.is_empty() {
                (
                    None,
                    Some(fill(
                        Bounds::new(
                            point(
                                bounds.left() + line.x_for_index(cursor.min(line.len())),
                                bounds.top() + (bounds.size.height - caret_height) / 2.0,
                            ),
                            size(px(1.5), caret_height),
                        ),
                        field.text_color,
                    )),
                )
            } else {
                (
                    Some(fill(
                        Bounds::from_corners(
                            point(
                                bounds.left() + line.x_for_index(selected.start),
                                bounds.top(),
                            ),
                            point(
                                bounds.left() + line.x_for_index(selected.end),
                                bounds.bottom(),
                            ),
                        ),
                        hsla(0.6, 0.4, 0.5, 0.35),
                    )),
                    None,
                )
            };
        let cursor_quad = if field.focus_handle.is_focused(window) {
            cursor_quad
        } else {
            None
        };
        FieldPrepaint {
            line: Some(line),
            cursor: cursor_quad,
            selection,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus = self.field.read(cx).focus_handle.clone();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.field.clone()),
            cx,
        );
        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }
        if let Some(line) = prepaint.line.take() {
            let _ = line.paint(bounds.origin, bounds.size.height, window, cx);
            self.field.update(cx, |field, _| {
                field.last_layout = Some(line);
                field.last_bounds = Some(bounds);
            });
        }
        if let Some(cursor) = prepaint.cursor.take() {
            window.paint_quad(cursor);
        }
    }
}

impl IntoElement for FieldElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}
