//! Multi-line editor surface. The focused entity implements [`EntityInputHandler`];
//! this element shapes the text, paints the caret, and records hit-test geometry.

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use gpui::*;

use crate::model::text_ops::visual_rows;

#[derive(Clone)]
pub struct PaintRow {
    pub byte_start: usize,
    pub byte_end: usize,
    pub origin: Point<Pixels>,
    pub shaped: ShapedLine,
}

#[derive(Clone)]
pub struct EditorPaint {
    pub wrap_cols: usize,
    pub line_height: Pixels,
    pub rows: Vec<PaintRow>,
    pub bounds: Option<Bounds<Pixels>>,
}

impl Default for EditorPaint {
    fn default() -> Self {
        Self {
            wrap_cols: usize::MAX,
            line_height: px(28.0),
            rows: Vec::new(),
            bounds: None,
        }
    }
}

pub fn index_for_position(paint: &EditorPaint, position: Point<Pixels>) -> Option<usize> {
    let bounds = paint.bounds?;
    if !bounds.contains(&position) || paint.rows.is_empty() {
        return None;
    }
    Some(index_in_rows(paint, position))
}

fn index_in_rows(paint: &EditorPaint, position: Point<Pixels>) -> usize {
    if paint.rows.is_empty() {
        return 0;
    }
    if position.y < paint.rows[0].origin.y {
        return paint.rows[0].byte_start;
    }
    let mut chosen = 0usize;
    for (index, row) in paint.rows.iter().enumerate() {
        if position.y >= row.origin.y {
            chosen = index;
        } else {
            break;
        }
    }
    let row = &paint.rows[chosen];
    let local_x = position.x - row.origin.x;
    if local_x < px(0.0) {
        return row.byte_start;
    }
    let local = row.shaped.closest_index_for_x(local_x);
    (row.byte_start + local).min(row.byte_end)
}

pub struct EditorElement<V: 'static> {
    pub entity: Entity<V>,
    pub focus: FocusHandle,
    pub paint: Rc<RefCell<EditorPaint>>,
    pub content: SharedString,
    pub selection: Range<usize>,
    pub marked: Option<Range<usize>>,
    pub find_hits: Vec<Range<usize>>,
    pub active_find: Option<usize>,
    pub show_line_numbers: bool,
    pub word_wrap: bool,
    pub centered: bool,
    pub max_measure: Option<Pixels>,
    pub font_size: Pixels,
    pub text_color: Hsla,
    pub selection_color: Hsla,
    pub find_color: Hsla,
    pub cursor_color: Hsla,
    pub gutter_color: Hsla,
    pub gutter_bg: Hsla,
    pub font_family: SharedString,
}

impl<V: 'static> EditorElement<V> {
    fn gutter(&self) -> Pixels {
        if self.show_line_numbers {
            px(52.0)
        } else {
            px(16.0)
        }
    }
}

pub struct Prepaint {
    rows: Vec<PaintRow>,
    numbers: Vec<(Point<Pixels>, ShapedLine)>,
    quads: Vec<PaintQuad>,
    cursor: Option<PaintQuad>,
    line_height: Pixels,
    wrap_cols: usize,
    x_offset: Pixels,
}

impl<V: EntityInputHandler> Element for EditorElement<V> {
    type RequestLayoutState = ();
    type PrepaintState = Prepaint;

    fn id(&self) -> Option<ElementId> {
        Some("raw-text-editor".into())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let wrap_cols = self.paint.borrow().wrap_cols;
        let rows = visual_rows(
            &self.content,
            if self.word_wrap {
                wrap_cols
            } else {
                usize::MAX
            },
        );
        let line_height = self.font_size * 1.65;
        let height = line_height * rows.len() as f32 + px(32.0);
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = height.into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        let font = font(self.font_family.clone());
        let sample = window.text_system().shape_line(
            "0".into(),
            self.font_size,
            &[TextRun {
                len: 1,
                font: font.clone(),
                color: self.text_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        );
        let char_width = sample.width.max(px(1.0));
        let gutter = self.gutter();
        let available_width = (bounds.size.width - gutter - px(32.0)).max(px(32.0));
        let (content_width, x_offset) = if self.centered {
            let max_w = self.max_measure.unwrap_or(available_width).min(available_width);
            let offset = ((available_width - max_w) / 2.0).max(px(0.0));
            (max_w, offset)
        } else {
            (available_width, px(0.0))
        };
        let wrap_cols = if self.word_wrap {
            (content_width / char_width).floor() as usize
        } else {
            usize::MAX
        };
        let wrap_cols = wrap_cols.max(1);
        let rows_src = visual_rows(&self.content, wrap_cols);
        let line_height = self.font_size * 1.65;

        let mut rows = Vec::with_capacity(rows_src.len());
        let mut numbers = Vec::new();
        let mut quads = Vec::new();
        let mut cursor = None;
        let focused = self.focus.is_focused(window);

        for (index, (start, end)) in rows_src.iter().copied().enumerate() {
            let slice = if start == end {
                SharedString::from(" ")
            } else {
                SharedString::from(self.content[start..end].to_string())
            };
            let display_len = if start == end { 0 } else { slice.len() };
            let is_heading = slice.starts_with("# ") || slice.starts_with("## ") || slice.starts_with("### ");
            let runs = line_runs(
                display_len.max(slice.len()),
                start,
                self.marked.clone(),
                font.clone(),
                self.text_color,
                is_heading,
            );
            let shaped = window
                .text_system()
                .shape_line(slice, self.font_size, &runs, None);
            let origin = point(
                bounds.left() + gutter + x_offset,
                bounds.top() + px(12.0) + line_height * index as f32,
            );
            paint_ranges(
                &mut quads,
                origin,
                line_height,
                start,
                end,
                &shaped,
                &self.selection,
                self.selection_color,
            );
            for (hit_index, hit) in self.find_hits.iter().enumerate() {
                let color = if self.active_find == Some(hit_index) {
                    self.cursor_color
                } else {
                    self.find_color
                };
                paint_ranges(
                    &mut quads,
                    origin,
                    line_height,
                    start,
                    end,
                    &shaped,
                    hit,
                    color,
                );
            }
            if focused
                && self.selection.is_empty()
                && self.selection.start >= start
                && self.selection.start <= end
            {
                let local = self.selection.start - start;
                let x = if start == end {
                    px(0.0)
                } else {
                    shaped.x_for_index(local.min(display_len))
                };
                cursor = Some(fill(
                    Bounds::new(point(origin.x + x, origin.y), size(px(1.5), line_height)),
                    self.cursor_color,
                ));
            }
            if self.show_line_numbers {
                let number = logical_line_number(&self.content, start);
                let label: SharedString = format!("{number}").into();
                let num_run = TextRun {
                    len: label.len(),
                    font: font.clone(),
                    color: self.gutter_color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let shaped_num =
                    window
                        .text_system()
                        .shape_line(label, self.font_size, &[num_run], None);
                let at_line_start = start == 0
                    || self.content.as_bytes().get(start.wrapping_sub(1)) == Some(&b'\n');
                if at_line_start {
                    numbers.push((point(bounds.left() + x_offset + px(8.0), origin.y), shaped_num));
                }
                rows.push(PaintRow {
                    byte_start: start,
                    byte_end: end,
                    origin,
                    shaped,
                });
            } else {
                rows.push(PaintRow {
                    byte_start: start,
                    byte_end: end,
                    origin,
                    shaped,
                });
            }
            let _ = display_len;
        }

        Prepaint {
            rows,
            numbers,
            quads,
            cursor,
            line_height,
            wrap_cols,
            x_offset,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.handle_input(
            &self.focus,
            ElementInputHandler::new(bounds, self.entity.clone()),
            cx,
        );
        if self.show_line_numbers {
            window.paint_quad(fill(
                Bounds::new(point(bounds.origin.x + prepaint.x_offset, bounds.origin.y), size(self.gutter(), bounds.size.height)),
                self.gutter_bg,
            ));
        }
        for quad in prepaint.quads.drain(..) {
            window.paint_quad(quad);
        }
        let line_height = prepaint.line_height;
        for (origin, shaped) in prepaint.numbers.drain(..) {
            let _ = shaped.paint(origin, line_height, window, cx);
        }
        for row in &prepaint.rows {
            let _ = row.shaped.paint(row.origin, line_height, window, cx);
        }
        if let Some(cursor) = prepaint.cursor.take() {
            window.paint_quad(cursor);
        }

        let wrap_changed = self.paint.borrow().wrap_cols != prepaint.wrap_cols;
        {
            let mut paint = self.paint.borrow_mut();
            paint.line_height = line_height;
            paint.bounds = Some(bounds);
            paint.rows = prepaint.rows.clone();
            if self.word_wrap {
                paint.wrap_cols = prepaint.wrap_cols;
            } else {
                paint.wrap_cols = usize::MAX;
            }
        }
        if wrap_changed && self.word_wrap {
            let entity = self.entity.clone();
            window.defer(cx, move |_, cx| {
                entity.update(cx, |_, cx| cx.notify());
            });
        }
    }
}

impl<V: EntityInputHandler> IntoElement for EditorElement<V> {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

fn logical_line_number(content: &str, offset: usize) -> usize {
    content[..offset.min(content.len())]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1
}

fn line_runs(
    len: usize,
    line_start: usize,
    marked: Option<Range<usize>>,
    mut font: Font,
    color: Hsla,
    is_heading: bool,
) -> Vec<TextRun> {
    if is_heading {
        font.weight = FontWeight::BOLD;
    }
    let base = TextRun {
        len,
        font,
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let Some(marked) = marked else {
        return vec![base];
    };
    let line_end = line_start + len;
    if marked.end <= line_start || marked.start >= line_end || len == 0 {
        return vec![base];
    }
    let local_start = marked.start.saturating_sub(line_start).min(len);
    let local_end = marked.end.saturating_sub(line_start).min(len);
    let underline = Some(UnderlineStyle {
        color: Some(color),
        thickness: px(1.0),
        wavy: false,
    });
    let mut runs = Vec::new();
    if local_start > 0 {
        runs.push(TextRun {
            len: local_start,
            ..base.clone()
        });
    }
    runs.push(TextRun {
        len: local_end - local_start,
        underline,
        ..base.clone()
    });
    if local_end < len {
        runs.push(TextRun {
            len: len - local_end,
            ..base.clone()
        });
    }
    runs.retain(|run| run.len > 0);
    if runs.is_empty() {
        vec![base]
    } else {
        runs
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_ranges(
    quads: &mut Vec<PaintQuad>,
    origin: Point<Pixels>,
    line_height: Pixels,
    row_start: usize,
    row_end: usize,
    shaped: &ShapedLine,
    range: &Range<usize>,
    color: Hsla,
) {
    if range.end <= row_start || range.start > row_end {
        return;
    }
    if range.is_empty() {
        return;
    }
    let local_start = range
        .start
        .saturating_sub(row_start)
        .min(row_end - row_start);
    let local_end = range.end.saturating_sub(row_start).min(row_end - row_start);
    if local_end <= local_start {
        return;
    }
    let x0 = shaped.x_for_index(local_start);
    let x1 = shaped.x_for_index(local_end).max(x0 + px(1.0));
    quads.push(fill(
        Bounds::from_corners(
            point(origin.x + x0, origin.y),
            point(origin.x + x1, origin.y + line_height),
        ),
        color,
    ));
}

pub struct WysiwygInputHook<V: 'static> {
    pub entity: Entity<V>,
    pub focus: FocusHandle,
}

impl<V: EntityInputHandler> Element for WysiwygInputHook<V> {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some("wysiwyg-input-hook".into())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.handle_input(
            &self.focus,
            ElementInputHandler::new(bounds, self.entity.clone()),
            cx,
        );
    }
}

impl<V: EntityInputHandler> IntoElement for WysiwygInputHook<V> {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

