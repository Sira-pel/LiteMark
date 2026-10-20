//! Multi-line editor surface for both Raw and WYSIWYG modes.
//! The focused entity implements [`EntityInputHandler`]; this element shapes
//! the text, paints block/inline decorations and the caret, and records
//! hit-test geometry with bidirectional source/display byte mapping.

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui::*;

use crate::markdown::{
    LineKind, ParsedDocument, ParsedLine, RowOffsetMap, StyledSegment, TableColumnAlign,
};
use crate::model::text_ops::{visual_rows, Caret};
use crate::model::ViewMode;

#[derive(Clone)]
pub struct PaintCell {
    pub raw_range: Range<usize>,
    pub inner_range: Range<usize>,
    pub box_bounds: Bounds<Pixels>,
    pub text_origin: Point<Pixels>,
    pub shaped: ShapedLine,
    pub offset_map: RowOffsetMap,
}

#[derive(Clone)]
pub struct PaintRow {
    pub line_index: usize,
    pub byte_start: usize,
    pub byte_end: usize,
    pub origin: Point<Pixels>,
    pub line_height: Pixels,
    pub shaped: ShapedLine,
    pub offset_map: RowOffsetMap,
    pub checkbox_hit: Option<(Bounds<Pixels>, Range<usize>)>,
    pub link_hits: Vec<(Range<usize>, String)>,
    pub cells: Vec<PaintCell>,
}

#[derive(Clone)]
pub struct EditorPaint {
    pub wrap_cols: usize,
    pub line_height: Pixels,
    pub total_height: Pixels,
    pub rows: Vec<PaintRow>,
    pub bounds: Option<Bounds<Pixels>>,
}

impl Default for EditorPaint {
    fn default() -> Self {
        Self {
            wrap_cols: usize::MAX,
            line_height: px(28.0),
            total_height: px(0.0),
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

pub fn checkbox_for_position(paint: &EditorPaint, position: Point<Pixels>) -> Option<Range<usize>> {
    let bounds = paint.bounds?;
    if !bounds.contains(&position) {
        return None;
    }
    for row in &paint.rows {
        if let Some((ref box_bounds, ref range)) = row.checkbox_hit {
            if box_bounds.contains(&position) {
                return Some(range.clone());
            }
        }
    }
    None
}

pub fn link_for_offset(paint: &EditorPaint, source_offset: usize) -> Option<String> {
    for row in &paint.rows {
        if source_offset >= row.byte_start && source_offset <= row.byte_end {
            for (range, url) in &row.link_hits {
                if source_offset >= range.start && source_offset <= range.end {
                    return Some(url.clone());
                }
            }
        }
    }
    None
}

pub fn move_vertical_visual(
    paint: &EditorPaint,
    content: &str,
    caret: &mut Caret,
    delta: i32,
    extend: bool,
) -> bool {
    if paint.rows.is_empty() {
        return false;
    }
    let cursor = if !extend && !caret.range.is_empty() {
        if delta < 0 {
            caret.range.start
        } else {
            caret.range.end
        }
    } else {
        caret.cursor()
    };

    let mut current_row_ix = 0usize;
    for (ix, row) in paint.rows.iter().enumerate() {
        let is_last = ix + 1 == paint.rows.len();
        let next_same_line = paint
            .rows
            .get(ix + 1)
            .is_some_and(|next| next.line_index == row.line_index);
        let matches = if is_last || !next_same_line {
            cursor >= row.byte_start && cursor <= row.byte_end
        } else {
            cursor >= row.byte_start && cursor < row.byte_end
        };
        if matches {
            current_row_ix = ix;
            break;
        }
        if cursor >= row.byte_start {
            current_row_ix = ix;
        }
    }

    let current_row = &paint.rows[current_row_ix];
    let target_screen_x = if !current_row.cells.is_empty() {
        let cell = current_row
            .cells
            .iter()
            .find(|c| cursor >= c.raw_range.start && cursor <= c.raw_range.end)
            .or_else(|| {
                current_row
                    .cells
                    .iter()
                    .rev()
                    .find(|c| c.raw_range.start <= cursor)
            })
            .unwrap_or(&current_row.cells[0]);
        let clamped = cursor.clamp(cell.inner_range.start, cell.inner_range.end);
        let display_idx = cell.offset_map.source_to_display(clamped);
        let x_in_cell = cell.shaped.x_for_index(display_idx);
        (cell.text_origin.x + x_in_cell)
            .clamp(cell.box_bounds.left() + px(1.0), (cell.box_bounds.right() - px(1.0)).max(cell.box_bounds.left() + px(1.0)))
    } else {
        let display_idx = current_row.offset_map.source_to_display(cursor);
        let x_in_row = current_row.shaped.x_for_index(display_idx);
        current_row.origin.x + x_in_row
    };

    let target_ix = if delta < 0 {
        if current_row_ix == 0 {
            let target = 0;
            if extend {
                caret.select_to(target);
            } else {
                caret.move_to(target);
            }
            return true;
        }
        current_row_ix.saturating_sub((-delta) as usize)
    } else {
        if current_row_ix + 1 >= paint.rows.len() {
            let target = content.len();
            if extend {
                caret.select_to(target);
            } else {
                caret.move_to(target);
            }
            return true;
        }
        (current_row_ix + delta as usize).min(paint.rows.len() - 1)
    };

    let target_row = &paint.rows[target_ix];
    let target_source = if !target_row.cells.is_empty() {
        let cell = target_row
            .cells
            .iter()
            .find(|c| {
                target_screen_x >= c.box_bounds.left() && target_screen_x < c.box_bounds.right()
            })
            .unwrap_or_else(|| {
                if target_screen_x < target_row.cells[0].box_bounds.left() {
                    &target_row.cells[0]
                } else {
                    target_row.cells.last().unwrap()
                }
            });
        let local_x = (target_screen_x - cell.text_origin.x).max(px(0.0));
        let target_display = cell.shaped.closest_index_for_x(local_x);
        cell.offset_map
            .display_to_source(target_display)
            .clamp(cell.inner_range.start, cell.inner_range.end)
            .min(content.len())
    } else {
        let local_x = (target_screen_x - target_row.origin.x).max(px(0.0));
        let target_display = target_row.shaped.closest_index_for_x(local_x);
        target_row
            .offset_map
            .display_to_source(target_display)
            .clamp(target_row.byte_start, target_row.byte_end)
            .min(content.len())
    };

    if extend {
        caret.select_to(target_source);
    } else {
        caret.move_to(target_source);
    }
    true
}

fn index_in_rows(paint: &EditorPaint, position: Point<Pixels>) -> usize {
    if paint.rows.is_empty() {
        return 0;
    }
    if position.y < paint.rows[0].origin.y {
        let first = &paint.rows[0];
        if let Some(first_cell) = first.cells.first() {
            return first_cell.inner_range.start;
        }
        return first.offset_map.display_to_source(0);
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
    if !row.cells.is_empty() {
        let cell = row
            .cells
            .iter()
            .find(|c| position.x >= c.box_bounds.left() && position.x < c.box_bounds.right())
            .unwrap_or_else(|| {
                if position.x < row.cells[0].box_bounds.left() {
                    &row.cells[0]
                } else {
                    row.cells.last().unwrap()
                }
            });
        let local_x = (position.x - cell.text_origin.x).max(px(0.0));
        let display_idx = cell.shaped.closest_index_for_x(local_x);
        return cell
            .offset_map
            .display_to_source(display_idx)
            .clamp(cell.inner_range.start, cell.inner_range.end);
    }
    let local_x = position.x - row.origin.x;
    if local_x <= px(0.0) {
        return row.offset_map.display_to_source(0);
    }
    let display_idx = row.shaped.closest_index_for_x(local_x);
    row.offset_map
        .display_to_source(display_idx)
        .clamp(row.byte_start, row.byte_end)
}

pub struct EditorElement<V: 'static> {
    pub entity: Entity<V>,
    pub focus: FocusHandle,
    pub paint: Rc<RefCell<EditorPaint>>,
    pub content: SharedString,
    pub parsed: Arc<ParsedDocument>,
    pub view_mode: ViewMode,
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
    pub strong_color: Hsla,
    pub muted_color: Hsla,
    #[allow(dead_code)]
    pub faint_color: Hsla,
    pub h1_color: Hsla,
    pub h2_color: Hsla,
    pub code_color: Hsla,
    pub code_bg: Hsla,
    pub surface_bg: Hsla,
    pub border_color: Hsla,
    pub link_color: Hsla,
    pub selection_color: Hsla,
    pub find_color: Hsla,
    pub cursor_color: Hsla,
    pub gutter_color: Hsla,
    pub gutter_bg: Hsla,
    pub font_family: SharedString,
    pub mono_font_family: SharedString,
}

impl<V: 'static> EditorElement<V> {
    fn gutter(&self) -> Pixels {
        if self.show_line_numbers && self.view_mode == ViewMode::Raw {
            px(52.0)
        } else {
            px(24.0)
        }
    }
}

pub struct Prepaint {
    rows: Vec<PaintRow>,
    numbers: Vec<(Point<Pixels>, Pixels, ShapedLine)>,
    decor_lines: Vec<(Point<Pixels>, Pixels, ShapedLine)>,
    bg_quads: Vec<PaintQuad>,
    quads: Vec<PaintQuad>,
    strike_quads: Vec<PaintQuad>,
    cursor: Option<PaintQuad>,
    line_height: Pixels,
    total_height: Pixels,
    wrap_cols: usize,
    x_offset: Pixels,
}

impl<V: EntityInputHandler> Element for EditorElement<V> {
    type RequestLayoutState = ();
    type PrepaintState = Prepaint;

    fn id(&self) -> Option<ElementId> {
        Some("unified-text-editor".into())
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
        let paint_borrow = self.paint.borrow();
        let wrap_cols = if self.word_wrap {
            paint_borrow.wrap_cols
        } else {
            usize::MAX
        };
        let estimated_height = if self.view_mode == ViewMode::Wysiwyg {
            let mut h = px(32.0);
            for line in &self.parsed.lines {
                if matches!(
                    line.kind,
                    LineKind::TableDelimiter | LineKind::SetextUnderline
                ) {
                    continue;
                }
                let (font_sz, line_h, top_gap, _) =
                    wysiwyg_line_metrics(&line.kind, self.font_size);
                let _ = font_sz;
                let wrapped = if line.display_text.is_empty() {
                    1
                } else {
                    visual_rows(&line.display_text, wrap_cols).len().max(1)
                };
                h += top_gap + line_h * wrapped as f32;
            }
            h.max(paint_borrow.total_height)
        } else {
            let rows = visual_rows(&self.content, wrap_cols);
            let line_height = self.font_size * 1.65;
            line_height * rows.len() as f32 + px(32.0)
        };
        drop(paint_borrow);

        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = estimated_height.into();
        style.min_size.width = px(0.0).into();
        style.max_size.width = relative(1.).into();
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
        let base_font = font(self.font_family.clone());
        let mono_font = font(self.mono_font_family.clone());
        let sample = window.text_system().shape_line(
            "0".into(),
            self.font_size,
            &[TextRun {
                len: 1,
                font: base_font.clone(),
                color: self.text_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        );
        let char_width = sample.width.max(px(1.0));
        let gutter = self.gutter();
        let available_width = (bounds.size.width - gutter - px(48.0)).max(px(32.0));
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
        let base_line_height = self.font_size * 1.65;
        let focused = self.focus.is_focused(window);

        if self.view_mode == ViewMode::Raw {
            let rows_src = visual_rows(&self.content, wrap_cols);
            let mut rows = Vec::with_capacity(rows_src.len());
            let mut numbers = Vec::new();
            let mut quads = Vec::new();
            let mut cursor = None;

            for (index, (start, end)) in rows_src.iter().copied().enumerate() {
                let slice = if start == end {
                    SharedString::from(" ")
                } else {
                    SharedString::from(self.content[start..end].to_string())
                };
                let display_len = if start == end { 0 } else { slice.len() };
                let is_heading = slice.starts_with("# ")
                    || slice.starts_with("## ")
                    || slice.starts_with("### ");
                let runs = raw_line_runs(
                    display_len.max(slice.len()),
                    start,
                    self.marked.clone(),
                    base_font.clone(),
                    self.text_color,
                    is_heading,
                );
                let shaped =
                    window
                        .text_system()
                        .shape_line(slice, self.font_size, &runs, None);
                let origin = point(
                    bounds.left() + gutter + x_offset,
                    bounds.top() + px(12.0) + base_line_height * index as f32,
                );
                let offset_map = RowOffsetMap::identity(start, end);

                paint_ranges_mapped(
                    &mut quads,
                    origin,
                    base_line_height,
                    start,
                    end,
                    &offset_map,
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
                    paint_ranges_mapped(
                        &mut quads,
                        origin,
                        base_line_height,
                        start,
                        end,
                        &offset_map,
                        &shaped,
                        hit,
                        color,
                    );
                }
                let is_last_row = index + 1 == rows_src.len();
                let next_is_newline = self.content.as_bytes().get(end) == Some(&b'\n');
                let caret_matches = if is_last_row || next_is_newline {
                    self.selection.start >= start && self.selection.start <= end
                } else {
                    self.selection.start >= start && self.selection.start < end
                };
                if focused && self.selection.is_empty() && caret_matches {
                    let local = (self.selection.start - start).min(display_len);
                    let x = if start == end {
                        px(0.0)
                    } else {
                        shaped.x_for_index(local)
                    };
                    let pad = (base_line_height - shaped.ascent - shaped.descent).max(px(0.0)) / 2.0;
                    let caret_y = origin.y + pad;
                    let caret_h = shaped.ascent + shaped.descent;
                    cursor = Some(fill(
                        Bounds::new(point(origin.x + x, caret_y), size(px(2.0), caret_h)),
                        self.cursor_color,
                    ));
                }
                let line_number = logical_line_number(&self.content, start);
                if self.show_line_numbers {
                    let at_line_start = start == 0
                        || self.content.as_bytes().get(start.wrapping_sub(1)) == Some(&b'\n');
                    if at_line_start {
                        let label: SharedString = format!("{line_number}").into();
                        let num_run = TextRun {
                            len: label.len(),
                            font: base_font.clone(),
                            color: self.gutter_color,
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        };
                        let shaped_num = window.text_system().shape_line(
                            label,
                            self.font_size,
                            &[num_run],
                            None,
                        );
                        numbers.push((
                            point(bounds.left() + x_offset + px(8.0), origin.y),
                            base_line_height,
                            shaped_num,
                        ));
                    }
                }
                rows.push(PaintRow {
                    line_index: line_number.saturating_sub(1),
                    byte_start: start,
                    byte_end: end,
                    origin,
                    line_height: base_line_height,
                    shaped,
                    offset_map,
                    checkbox_hit: None,
                    link_hits: Vec::new(),
                    cells: Vec::new(),
                });
            }

            let total_height = base_line_height * rows.len() as f32 + px(32.0);
            return Prepaint {
                rows,
                numbers,
                decor_lines: Vec::new(),
                bg_quads: Vec::new(),
                quads,
                strike_quads: Vec::new(),
                cursor,
                line_height: base_line_height,
                total_height,
                wrap_cols,
                x_offset,
            };
        }

        // WYSIWYG mode prepaint
        let mut rows = Vec::with_capacity(self.parsed.lines.len());
        let mut decor_lines = Vec::new();
        let mut bg_quads = Vec::new();
        let mut quads = Vec::new();
        let mut strike_quads = Vec::new();
        let mut cursor = None;
        let base_x = bounds.left() + gutter + x_offset;
        let mut cur_y = bounds.top() + px(16.0);

        for (line_idx, parsed_line) in self.parsed.lines.iter().enumerate() {
            if matches!(
                parsed_line.kind,
                LineKind::TableDelimiter | LineKind::SetextUnderline
            ) {
                continue;
            }
            let (row_font_size, row_line_height, top_gap, indent_x) =
                wysiwyg_line_metrics(&parsed_line.kind, self.font_size);
            cur_y += top_gap;

            let link_hits = collect_line_links(parsed_line);

            if let LineKind::TableRow {
                is_header,
                ref cells,
                ref raw_cells,
                ref alignments,
                ..
            } = parsed_line.kind
            {
                let num_cols = cells.len().max(1);
                let table_w = content_width;
                let col_w = table_w / (num_cols as f32);
                let pad_x = px(12.0);

                // Table row horizontal borders
                if is_header {
                    bg_quads.push(fill(
                        Bounds::new(point(base_x, cur_y), size(table_w, px(1.0))),
                        self.border_color,
                    ));
                }
                bg_quads.push(fill(
                    Bounds::new(
                        point(base_x, cur_y + row_line_height - px(1.0)),
                        size(table_w, px(1.0)),
                    ),
                    self.border_color,
                ));

                // Vertical borders (left outer, inner dividers, right outer)
                for c_border in 0..=num_cols {
                    let vx = if c_border == num_cols {
                        base_x + table_w - px(1.0)
                    } else {
                        base_x + col_w * (c_border as f32)
                    };
                    bg_quads.push(fill(
                        Bounds::new(point(vx, cur_y), size(px(1.0), row_line_height)),
                        self.border_color,
                    ));
                }

                let mut paint_cells = Vec::with_capacity(num_cols);
                for col_idx in 0..num_cols {
                    let cell_range = cells
                        .get(col_idx)
                        .cloned()
                        .unwrap_or(parsed_line.source.start..parsed_line.source.start);
                    let raw_range = raw_cells
                        .get(col_idx)
                        .cloned()
                        .unwrap_or_else(|| cell_range.clone());
                    let align = alignments
                        .get(col_idx)
                        .copied()
                        .unwrap_or(TableColumnAlign::Left);

                    let cell_x = base_x + col_w * (col_idx as f32);
                    let box_bounds =
                        Bounds::new(point(cell_x, cur_y), size(col_w, row_line_height));

                    let cell_raw_str = self.content.get(cell_range.clone()).unwrap_or("");
                    let (cell_display, cell_map) = RowOffsetMap::from_hidden(
                        cell_raw_str,
                        cell_range.start,
                        cell_range.start,
                        &parsed_line.hidden_ranges,
                    );
                    let disp_len = cell_display.len();
                    let shaped_str: SharedString = if disp_len == 0 {
                        SharedString::from(" ")
                    } else {
                        SharedString::from(cell_display)
                    };

                    let runs = wysiwyg_cell_runs(
                        parsed_line,
                        disp_len,
                        shaped_str.len(),
                        &cell_map,
                        self.marked.as_ref(),
                        &base_font,
                        &mono_font,
                        self,
                    );
                    let shaped_cell = window.text_system().shape_line(
                        shaped_str,
                        row_font_size,
                        &runs,
                        None,
                    );

                    let text_w = if disp_len == 0 {
                        px(0.0)
                    } else {
                        shaped_cell.width
                    };
                    let avail_inner = (col_w - pad_x * 2.0).max(px(0.0));
                    let text_x = match align {
                        TableColumnAlign::None | TableColumnAlign::Left => cell_x + pad_x,
                        TableColumnAlign::Center => {
                            cell_x + pad_x + ((avail_inner - text_w) * 0.5).max(px(0.0))
                        }
                        TableColumnAlign::Right => {
                            cell_x + pad_x + (avail_inner - text_w).max(px(0.0))
                        }
                    };
                    let text_origin = point(text_x, cur_y);

                    // Selection and Find highlights inside cell
                    paint_ranges_mapped(
                        &mut quads,
                        text_origin,
                        row_line_height,
                        cell_map.line_start,
                        cell_map.line_end,
                        &cell_map,
                        &shaped_cell,
                        &self.selection,
                        self.selection_color,
                    );
                    for (hit_index, hit) in self.find_hits.iter().enumerate() {
                        let color = if self.active_find == Some(hit_index) {
                            self.cursor_color
                        } else {
                            self.find_color
                        };
                        paint_ranges_mapped(
                            &mut quads,
                            text_origin,
                            row_line_height,
                            cell_map.line_start,
                            cell_map.line_end,
                            &cell_map,
                            &shaped_cell,
                            hit,
                            color,
                        );
                    }

                    // Strikethrough inside cell
                    for seg in &parsed_line.styled_segments {
                        if !seg.strike
                            || seg.source.end <= cell_range.start
                            || seg.source.start >= cell_range.end
                        {
                            continue;
                        }
                        let s_start =
                            cell_map.source_to_display(seg.source.start.max(cell_range.start));
                        let s_end =
                            cell_map.source_to_display(seg.source.end.min(cell_range.end));
                        if s_end > s_start {
                            let x1 = shaped_cell.x_for_index(s_start);
                            let x2 = shaped_cell.x_for_index(s_end);
                            let strike_x = (text_origin.x + x1 - px(2.0)).max(cell_x + px(2.0));
                            let strike_w = (x2 - x1) + px(4.0);
                            let pad = (row_line_height - shaped_cell.ascent - shaped_cell.descent)
                                .max(px(0.0))
                                / 2.0;
                            let strike_y = text_origin.y + (pad + shaped_cell.ascent * 1.5) * 0.5;
                            let color = if seg.bold {
                                self.strong_color
                            } else {
                                self.text_color
                            };
                            let thickness = if seg.bold {
                                px(2.0)
                            } else {
                                px(1.75)
                            };
                            strike_quads.push(fill(
                                Bounds::new(point(strike_x, strike_y), size(strike_w, thickness)),
                                color,
                            ));
                        }
                    }

                    // Caret inside cell
                    let caret_in_cell = (self.selection.start >= raw_range.start
                        && self.selection.start <= raw_range.end)
                        || (col_idx == 0
                            && self.selection.start >= parsed_line.source.start
                            && self.selection.start < raw_range.start)
                        || (col_idx + 1 == num_cols
                            && self.selection.start > raw_range.end
                            && self.selection.start <= parsed_line.source.end);
                    if focused && self.selection.is_empty() && caret_in_cell && cursor.is_none() {
                        let clamped_src =
                            self.selection.start.clamp(cell_range.start, cell_range.end);
                        let display_idx = cell_map.source_to_display(clamped_src).min(disp_len);
                        let x = if disp_len == 0 {
                            px(0.0)
                        } else {
                            shaped_cell.x_for_index(display_idx)
                        };
                        let pad = (row_line_height - shaped_cell.ascent - shaped_cell.descent)
                            .max(px(0.0))
                            / 2.0;
                        let caret_y = text_origin.y + pad;
                        let caret_h =
                            (shaped_cell.ascent + shaped_cell.descent).max(row_font_size);
                        cursor = Some(fill(
                            Bounds::new(
                                point(text_origin.x + x, caret_y),
                                size(px(2.0), caret_h),
                            ),
                            self.cursor_color,
                        ));
                    }

                    paint_cells.push(PaintCell {
                        raw_range,
                        inner_range: cell_range,
                        box_bounds,
                        text_origin,
                        shaped: shaped_cell,
                        offset_map: cell_map,
                    });
                }

                let dummy_shaped = paint_cells
                    .first()
                    .map(|c| c.shaped.clone())
                    .unwrap_or_else(|| sample.clone());
                rows.push(PaintRow {
                    line_index: line_idx,
                    byte_start: parsed_line.source.start,
                    byte_end: parsed_line.source.end,
                    origin: point(base_x, cur_y),
                    line_height: row_line_height,
                    shaped: dummy_shaped,
                    offset_map: parsed_line.offset_map.clone(),
                    checkbox_hit: None,
                    link_hits,
                    cells: paint_cells,
                });
                cur_y += row_line_height;
                continue;
            }

            let row_char_width = char_width * (row_font_size / self.font_size);
            let row_avail_width = (content_width - indent_x).max(px(32.0));
            let row_wrap_cols = if self.word_wrap {
                ((row_avail_width / row_char_width).floor() as usize).max(1)
            } else {
                usize::MAX
            };

            let sub_rows = visual_rows(&parsed_line.display_text, row_wrap_cols);

            for (sub_idx, (d_start, d_end)) in sub_rows.iter().copied().enumerate() {
                let is_last_sub = sub_idx + 1 == sub_rows.len();
                let sub_map = if sub_rows.len() == 1 {
                    parsed_line.offset_map.clone()
                } else {
                    parsed_line.offset_map.slice_display(d_start..d_end)
                };

                let row_origin = point(base_x + indent_x, cur_y);

                // Block backgrounds / decorations
                match &parsed_line.kind {
                    LineKind::CodeFence {
                        language,
                        opening: true,
                    } => {
                        bg_quads.push(fill(
                            Bounds::new(
                                point(base_x, cur_y),
                                size(content_width, row_line_height),
                            ),
                            self.surface_bg,
                        ));
                        bg_quads.push(fill(
                            Bounds::new(point(base_x, cur_y), size(content_width, px(1.0))),
                            self.border_color,
                        ));
                        if sub_idx == 0 {
                            if let Some(lang) = language.as_ref().filter(|l| !l.is_empty()) {
                                let badge: SharedString = lang.clone().into();
                                let badge_run = TextRun {
                                    len: badge.len(),
                                    font: mono_font.clone(),
                                    color: self.muted_color,
                                    background_color: None,
                                    underline: None,
                                    strikethrough: None,
                                };
                                let shaped_badge = window.text_system().shape_line(
                                    badge,
                                    self.font_size * 0.75,
                                    &[badge_run],
                                    None,
                                );
                                decor_lines.push((
                                    point(
                                        base_x + content_width - shaped_badge.width - px(10.0),
                                        cur_y,
                                    ),
                                    row_line_height,
                                    shaped_badge,
                                ));
                            }
                        }
                    }
                    LineKind::CodeLine => {
                        bg_quads.push(fill(
                            Bounds::new(
                                point(base_x, cur_y),
                                size(content_width, row_line_height),
                            ),
                            self.surface_bg,
                        ));
                    }
                    LineKind::CodeFence { opening: false, .. } => {
                        bg_quads.push(fill(
                            Bounds::new(
                                point(base_x, cur_y),
                                size(content_width, row_line_height),
                            ),
                            self.surface_bg,
                        ));
                        bg_quads.push(fill(
                            Bounds::new(
                                point(base_x, cur_y + row_line_height - px(1.0)),
                                size(content_width, px(1.0)),
                            ),
                            self.border_color,
                        ));
                    }
                    LineKind::Blockquote => {
                        let bar_x = base_x + px(4.0);
                        bg_quads.push(fill(
                            Bounds::new(point(bar_x, cur_y), size(px(3.0), row_line_height)),
                            self.link_color,
                        ));
                    }
                    LineKind::HorizontalRule => {
                        let mid_y = cur_y + row_line_height * 0.5;
                        bg_quads.push(fill(
                            Bounds::new(point(base_x, mid_y), size(content_width, px(1.5))),
                            self.border_color,
                        ));
                    }
                    _ => {}
                }

                // First sub-row markers (bullets, numbers, task checkboxes)
                let mut checkbox_hit = None;
                if sub_idx == 0 {
                    if let LineKind::ListItem {
                        ordered,
                        number,
                        task,
                        ref checkbox_source,
                        ..
                    } = parsed_line.kind
                    {
                        if let Some(checked) = task {
                            let box_sz = px(14.0);
                            let box_x = row_origin.x - px(20.0);
                            let box_y = cur_y + (row_line_height - box_sz) * 0.5;
                            let box_bounds =
                                Bounds::new(point(box_x, box_y), size(box_sz, box_sz));
                            let mut q = fill(
                                box_bounds,
                                if checked {
                                    self.link_color
                                } else {
                                    self.surface_bg
                                },
                            );
                            q.corner_radii = Corners::all(px(3.0));
                            q.border_widths = Edges::all(px(1.5));
                            q.border_color = if checked {
                                self.link_color
                            } else {
                                self.muted_color
                            };
                            bg_quads.push(q);
                            if checked {
                                let inner_sz = px(6.0);
                                let inner_bounds = Bounds::new(
                                    point(box_x + px(4.0), box_y + px(4.0)),
                                    size(inner_sz, inner_sz),
                                );
                                let mut check_dot = fill(inner_bounds, gpui::white());
                                check_dot.corner_radii = Corners::all(px(1.5));
                                bg_quads.push(check_dot);
                            }
                            if let Some(src_range) = checkbox_source.clone().or_else(|| {
                                find_task_bracket_range(&self.content, &parsed_line.source)
                            }) {
                                let hit_bounds = Bounds::new(
                                    point(box_x - px(4.0), cur_y),
                                    size(box_sz + px(8.0), row_line_height),
                                );
                                checkbox_hit = Some((hit_bounds, src_range));
                            }
                        } else if ordered {
                            let label: SharedString = format!("{number}.").into();
                            let run = TextRun {
                                len: label.len(),
                                font: base_font.clone(),
                                color: self.muted_color,
                                background_color: None,
                                underline: None,
                                strikethrough: None,
                            };
                            let shaped_marker = window.text_system().shape_line(
                                label,
                                row_font_size,
                                &[run],
                                None,
                            );
                            let marker_x =
                                (row_origin.x - shaped_marker.width - px(6.0)).max(base_x);
                            decor_lines.push((
                                point(marker_x, cur_y),
                                row_line_height,
                                shaped_marker,
                            ));
                        } else {
                            let dot_sz = px(5.0);
                            let dot_x = row_origin.x - px(14.0);
                            let dot_y = cur_y + (row_line_height - dot_sz) * 0.5;
                            let mut q = fill(
                                Bounds::new(point(dot_x, dot_y), size(dot_sz, dot_sz)),
                                self.muted_color,
                            );
                            q.corner_radii = Corners::all(px(2.5));
                            bg_quads.push(q);
                        }
                    }
                }

                // Shape the display text slice
                let display_len = d_end.saturating_sub(d_start);
                let slice: SharedString = if display_len == 0 {
                    SharedString::from(" ")
                } else {
                    SharedString::from(parsed_line.display_text[d_start..d_end].to_string())
                };
                let runs = wysiwyg_slice_runs(
                    parsed_line,
                    d_start..d_end,
                    slice.len(),
                    &sub_map,
                    self.marked.as_ref(),
                    &base_font,
                    &mono_font,
                    self,
                );
                let shaped =
                    window
                        .text_system()
                        .shape_line(slice, row_font_size, &runs, None);

                // Selection and Find highlights
                paint_ranges_mapped(
                    &mut quads,
                    row_origin,
                    row_line_height,
                    sub_map.line_start,
                    sub_map.line_end,
                    &sub_map,
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
                    paint_ranges_mapped(
                        &mut quads,
                        row_origin,
                        row_line_height,
                        sub_map.line_start,
                        sub_map.line_end,
                        &sub_map,
                        &shaped,
                        hit,
                        color,
                    );
                }

                // Strikethrough lines (bright, solid, and slightly extended past the words)
                for seg in &parsed_line.styled_segments {
                    if !seg.strike {
                        continue;
                    }
                    let seg_disp_start = parsed_line.offset_map.source_to_display(seg.source.start);
                    let seg_disp_end = parsed_line.offset_map.source_to_display(seg.source.end);
                    if seg_disp_end <= d_start || seg_disp_start >= d_end {
                        continue;
                    }
                    let seg_start = seg_disp_start.max(d_start);
                    let seg_end = seg_disp_end.min(d_end);
                    if seg_end > seg_start {
                        let sub_disp_start = seg_start - d_start;
                        let sub_disp_end = seg_end - d_start;
                        let x1 = shaped.x_for_index(sub_disp_start);
                        let x2 = shaped.x_for_index(sub_disp_end);
                        let extend_left = if seg_start == seg_disp_start { px(3.0) } else { px(0.0) };
                        let extend_right = if seg_end == seg_disp_end { px(3.0) } else { px(0.0) };
                        let strike_x = (row_origin.x + x1 - extend_left).max(base_x);
                        let strike_w = (x2 - x1) + extend_left + extend_right;
                        let pad = (row_line_height - shaped.ascent - shaped.descent).max(px(0.0)) / 2.0;
                        let strike_y = row_origin.y + (pad + shaped.ascent * 1.5) * 0.5;
                        let color = if seg.bold { self.strong_color } else { self.text_color };
                        let thickness = if seg.bold { px(2.0) } else { px(1.75) };
                        strike_quads.push(fill(
                            Bounds::new(point(strike_x, strike_y), size(strike_w, thickness)),
                            color,
                        ));
                    }
                }
                if matches!(parsed_line.kind, LineKind::ListItem { task: Some(true), .. }) && display_len > 0 {
                    let x1 = shaped.x_for_index(0);
                    let x2 = shaped.x_for_index(display_len);
                    let extend_left = if sub_idx == 0 { px(3.0) } else { px(0.0) };
                    let extend_right = if is_last_sub { px(3.0) } else { px(0.0) };
                    let strike_x = (row_origin.x + x1 - extend_left).max(base_x);
                    let strike_w = (x2 - x1) + extend_left + extend_right;
                    let pad = (row_line_height - shaped.ascent - shaped.descent).max(px(0.0)) / 2.0;
                    let strike_y = row_origin.y + (pad + shaped.ascent * 1.5) * 0.5;
                    strike_quads.push(fill(
                        Bounds::new(point(strike_x, strike_y), size(strike_w, px(1.75))),
                        self.muted_color,
                    ));
                }

                // Caret painting
                let caret_matches = if is_last_sub {
                    self.selection.start >= sub_map.line_start
                        && self.selection.start <= sub_map.line_end
                } else {
                    self.selection.start >= sub_map.line_start
                        && self.selection.start < sub_map.line_end
                };
                if focused && self.selection.is_empty() && caret_matches && cursor.is_none() {
                    let display_idx = sub_map
                        .source_to_display(self.selection.start)
                        .min(display_len);
                    let x = if display_len == 0 {
                        px(0.0)
                    } else {
                        shaped.x_for_index(display_idx)
                    };
                    let pad =
                        (row_line_height - shaped.ascent - shaped.descent).max(px(0.0)) / 2.0;
                    let caret_y = row_origin.y + pad;
                    let caret_h = (shaped.ascent + shaped.descent).max(row_font_size);
                    cursor = Some(fill(
                        Bounds::new(point(row_origin.x + x, caret_y), size(px(2.0), caret_h)),
                        self.cursor_color,
                    ));
                }

                rows.push(PaintRow {
                    line_index: line_idx,
                    byte_start: sub_map.line_start,
                    byte_end: sub_map.line_end,
                    origin: row_origin,
                    line_height: row_line_height,
                    shaped,
                    offset_map: sub_map,
                    checkbox_hit,
                    link_hits: link_hits.clone(),
                    cells: Vec::new(),
                });

                cur_y += row_line_height;
            }
        }

        let total_height = (cur_y - bounds.top() + px(32.0)).max(px(64.0));
        Prepaint {
            rows,
            numbers: Vec::new(),
            decor_lines,
            bg_quads,
            quads,
            strike_quads,
            cursor,
            line_height: base_line_height,
            total_height,
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
        if self.show_line_numbers && self.view_mode == ViewMode::Raw {
            window.paint_quad(fill(
                Bounds::new(
                    point(bounds.origin.x + prepaint.x_offset, bounds.origin.y),
                    size(self.gutter(), bounds.size.height),
                ),
                self.gutter_bg,
            ));
        }
        for quad in prepaint.bg_quads.drain(..) {
            window.paint_quad(quad);
        }
        for quad in prepaint.quads.drain(..) {
            window.paint_quad(quad);
        }
        for (origin, lh, shaped) in prepaint.numbers.drain(..) {
            let _ = shaped.paint(origin, lh, window, cx);
        }
        for (origin, lh, shaped) in prepaint.decor_lines.drain(..) {
            let _ = shaped.paint(origin, lh, window, cx);
        }
        for row in &prepaint.rows {
            if row.cells.is_empty() {
                let _ = row.shaped.paint(row.origin, row.line_height, window, cx);
            } else {
                for cell in &row.cells {
                    let _ = cell
                        .shaped
                        .paint(cell.text_origin, row.line_height, window, cx);
                }
            }
        }
        for strike in prepaint.strike_quads.drain(..) {
            window.paint_quad(strike);
        }
        if let Some(cursor) = prepaint.cursor.take() {
            window.paint_quad(cursor);
        }

        let wrap_changed = self.paint.borrow().wrap_cols != prepaint.wrap_cols;
        {
            let mut paint = self.paint.borrow_mut();
            paint.line_height = prepaint.line_height;
            paint.total_height = prepaint.total_height;
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

pub(crate) fn wysiwyg_line_metrics(
    kind: &LineKind,
    base_font_size: Pixels,
) -> (Pixels, Pixels, Pixels, Pixels) {
    match kind {
        LineKind::Heading { level: 1 } => {
            let fs = base_font_size * 1.7;
            (fs, fs * 1.45, px(8.0), px(0.0))
        }
        LineKind::Heading { level: 2 } => {
            let fs = base_font_size * 1.4;
            (fs, fs * 1.45, px(6.0), px(0.0))
        }
        LineKind::Heading { level: 3 } => {
            let fs = base_font_size * 1.2;
            (fs, fs * 1.45, px(4.0), px(0.0))
        }
        LineKind::Heading { .. } => {
            let fs = base_font_size * 1.08;
            (fs, fs * 1.5, px(2.0), px(0.0))
        }
        LineKind::ListItem { indent_cols, .. } => (
            base_font_size,
            base_font_size * 1.65,
            px(0.0),
            px(24.0 + (*indent_cols / 2) as f32 * 18.0),
        ),
        LineKind::Blockquote => (
            base_font_size,
            base_font_size * 1.65,
            px(0.0),
            px(16.0),
        ),
        LineKind::CodeFence { .. } => {
            let fs = base_font_size * 0.78;
            (fs, base_font_size * 1.1, px(0.0), px(12.0))
        }
        LineKind::CodeLine => {
            let fs = base_font_size * 0.92;
            (fs, base_font_size * 1.5, px(0.0), px(14.0))
        }
        LineKind::HorizontalRule => (
            base_font_size * 0.75,
            base_font_size * 1.1,
            px(2.0),
            px(0.0),
        ),
        LineKind::TableDelimiter | LineKind::SetextUnderline => (
            px(0.0),
            px(0.0),
            px(0.0),
            px(0.0),
        ),
        LineKind::TableRow { is_header, .. } => {
            let fs = base_font_size * 0.95;
            let row_h = (base_font_size * 1.95).max(px(34.0));
            let top_gap = if *is_header { px(6.0) } else { px(0.0) };
            (fs, row_h, top_gap, px(0.0))
        }
        LineKind::Paragraph | LineKind::Empty => {
            (base_font_size, base_font_size * 1.65, px(0.0), px(0.0))
        }
    }
}

fn find_task_bracket_range(content: &str, line_range: &Range<usize>) -> Option<Range<usize>> {
    if line_range.end > content.len() {
        return None;
    }
    let slice = &content[line_range.clone()];
    let pos = slice
        .find("[ ]")
        .or_else(|| slice.find("[x]"))
        .or_else(|| slice.find("[X]"))?;
    Some((line_range.start + pos)..(line_range.start + pos + 3))
}

fn collect_line_links(line: &ParsedLine) -> Vec<(Range<usize>, String)> {
    let mut links = Vec::new();
    for seg in &line.styled_segments {
        if let Some(ref url) = seg.link_url {
            if seg.source.end > seg.source.start {
                links.push((seg.source.clone(), url.clone()));
            }
        }
    }
    links
}

#[allow(clippy::too_many_arguments)]
fn wysiwyg_slice_runs<V: 'static>(
    parsed_line: &ParsedLine,
    display_slice: Range<usize>,
    shaped_slice_len: usize,
    sub_map: &RowOffsetMap,
    marked: Option<&Range<usize>>,
    base_font: &Font,
    mono_font: &Font,
    editor: &EditorElement<V>,
) -> Vec<TextRun> {
    let slice_len = display_slice.end.saturating_sub(display_slice.start);
    let (default_weight, default_color, use_mono) = match &parsed_line.kind {
        LineKind::Heading { level: 1 } => (FontWeight::BOLD, editor.h1_color, false),
        LineKind::Heading { level: 2 } => (FontWeight::BOLD, editor.h2_color, false),
        LineKind::Heading { .. } => (FontWeight::BOLD, editor.strong_color, false),
        LineKind::CodeLine | LineKind::CodeFence { .. } => {
            (FontWeight::NORMAL, editor.code_color, true)
        }
        LineKind::Blockquote => (FontWeight::NORMAL, editor.muted_color, false),
        LineKind::TableRow { .. } => (FontWeight::NORMAL, editor.text_color, true),
        LineKind::ListItem {
            task: Some(true), ..
        } => (FontWeight::NORMAL, editor.muted_color, false),
        _ => (FontWeight::NORMAL, editor.text_color, false),
    };

    if slice_len == 0 {
        let mut f = if use_mono {
            mono_font.clone()
        } else {
            base_font.clone()
        };
        f.weight = default_weight;
        return vec![TextRun {
            len: shaped_slice_len.max(1),
            font: f,
            color: default_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        }];
    }

    let mut runs: Vec<TextRun> = Vec::new();
    let mut covered = display_slice.start;

    for seg in &parsed_line.styled_segments {
        let seg_disp_start = parsed_line.offset_map.source_to_display(seg.source.start);
        let seg_disp_end = parsed_line.offset_map.source_to_display(seg.source.end);
        if seg_disp_end <= display_slice.start || seg_disp_start >= display_slice.end {
            continue;
        }
        let seg_start = seg_disp_start.max(display_slice.start);
        let seg_end = seg_disp_end.min(display_slice.end);
        if seg_start > covered {
            let mut f = if use_mono {
                mono_font.clone()
            } else {
                base_font.clone()
            };
            f.weight = default_weight;
            runs.push(TextRun {
                len: seg_start - covered,
                font: f,
                color: default_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            });
        }
        if seg_end > seg_start {
            runs.push(segment_to_run(
                seg,
                seg_end - seg_start,
                &parsed_line.kind,
                default_weight,
                default_color,
                use_mono,
                base_font,
                mono_font,
                editor,
            ));
            covered = seg_end;
        }
    }

    if covered < display_slice.end {
        let mut f = if use_mono {
            mono_font.clone()
        } else {
            base_font.clone()
        };
        f.weight = default_weight;
        runs.push(TextRun {
            len: display_slice.end - covered,
            font: f,
            color: default_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    }

    if let Some(marked_range) = marked {
        if marked_range.end > sub_map.line_start && marked_range.start < sub_map.line_end {
            let m_start = sub_map.source_to_display(marked_range.start).min(slice_len);
            let m_end = sub_map.source_to_display(marked_range.end).min(slice_len);
            if m_end > m_start {
                runs = apply_underline_to_runs(runs, m_start..m_end, default_color);
            }
        }
    }

    if matches!(parsed_line.kind, LineKind::TableRow { .. }) {
        let slice_str = &parsed_line.display_text[display_slice];
        runs = apply_table_pipe_style(runs, slice_str, editor.border_color);
    }

    runs.retain(|r| r.len > 0);
    runs
}

#[allow(clippy::too_many_arguments)]
fn wysiwyg_cell_runs<V: 'static>(
    parsed_line: &ParsedLine,
    disp_len: usize,
    shaped_slice_len: usize,
    cell_map: &RowOffsetMap,
    marked: Option<&Range<usize>>,
    base_font: &Font,
    mono_font: &Font,
    editor: &EditorElement<V>,
) -> Vec<TextRun> {
    let default_weight = FontWeight::NORMAL;
    let default_color = editor.text_color;

    if disp_len == 0 {
        let mut f = base_font.clone();
        f.weight = default_weight;
        return vec![TextRun {
            len: shaped_slice_len.max(1),
            font: f,
            color: default_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        }];
    }

    let mut runs: Vec<TextRun> = Vec::new();
    let mut covered = 0usize;

    for seg in &parsed_line.styled_segments {
        if seg.source.end <= cell_map.line_start || seg.source.start >= cell_map.line_end {
            continue;
        }
        let seg_start = cell_map
            .source_to_display(seg.source.start.max(cell_map.line_start))
            .min(disp_len);
        let seg_end = cell_map
            .source_to_display(seg.source.end.min(cell_map.line_end))
            .min(disp_len);
        if seg_end <= seg_start {
            continue;
        }
        if seg_start > covered {
            let mut f = base_font.clone();
            f.weight = default_weight;
            runs.push(TextRun {
                len: seg_start - covered,
                font: f,
                color: default_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            });
        }
        runs.push(segment_to_run(
            seg,
            seg_end - seg_start,
            &parsed_line.kind,
            default_weight,
            default_color,
            false,
            base_font,
            mono_font,
            editor,
        ));
        covered = seg_end;
    }

    if covered < disp_len {
        let mut f = base_font.clone();
        f.weight = default_weight;
        runs.push(TextRun {
            len: disp_len - covered,
            font: f,
            color: default_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    }

    if let Some(marked_range) = marked {
        if marked_range.end > cell_map.line_start && marked_range.start < cell_map.line_end {
            let m_start = cell_map.source_to_display(marked_range.start).min(disp_len);
            let m_end = cell_map.source_to_display(marked_range.end).min(disp_len);
            if m_end > m_start {
                runs = apply_underline_to_runs(runs, m_start..m_end, default_color);
            }
        }
    }

    runs.retain(|r| r.len > 0);
    runs
}

#[allow(clippy::too_many_arguments)]
fn segment_to_run<V: 'static>(
    seg: &StyledSegment,
    len: usize,
    line_kind: &LineKind,
    default_weight: FontWeight,
    default_color: Hsla,
    use_mono: bool,
    base_font: &Font,
    mono_font: &Font,
    editor: &EditorElement<V>,
) -> TextRun {
    let mut font = if seg.code || use_mono {
        mono_font.clone()
    } else {
        base_font.clone()
    };
    font.weight = if seg.bold {
        FontWeight::BOLD
    } else {
        default_weight
    };
    if seg.italic || matches!(line_kind, LineKind::Blockquote) {
        font.style = FontStyle::Italic;
    }

    let color = if seg.code {
        editor.code_color
    } else if seg.link_url.is_some() {
        editor.link_color
    } else if seg.bold && default_color == editor.text_color {
        editor.strong_color
    } else {
        default_color
    };

    let background_color = if seg.code {
        Some(editor.code_bg)
    } else {
        None
    };

    let underline = if seg.underline || seg.link_url.is_some() {
        Some(UnderlineStyle {
            thickness: px(1.0),
            color: Some(color),
            wavy: false,
        })
    } else {
        None
    };

    let strikethrough = None;

    TextRun {
        len,
        font,
        color,
        background_color,
        underline,
        strikethrough,
    }
}

fn apply_underline_to_runs(
    runs: Vec<TextRun>,
    target: Range<usize>,
    color: Hsla,
) -> Vec<TextRun> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    let u_style = Some(UnderlineStyle {
        color: Some(color),
        thickness: px(1.0),
        wavy: false,
    });
    for run in runs {
        let start = offset;
        let end = offset + run.len;
        offset = end;
        if end <= target.start || start >= target.end {
            out.push(run);
            continue;
        }
        let u_start = target.start.max(start);
        let u_end = target.end.min(end);
        if u_start > start {
            out.push(TextRun {
                len: u_start - start,
                ..run.clone()
            });
        }
        out.push(TextRun {
            len: u_end - u_start,
            underline: u_style,
            ..run.clone()
        });
        if u_end < end {
            out.push(TextRun {
                len: end - u_end,
                ..run
            });
        }
    }
    out
}

fn apply_table_pipe_style(
    runs: Vec<TextRun>,
    slice_text: &str,
    border_color: Hsla,
) -> Vec<TextRun> {
    let mut out = runs;
    for (idx, ch) in slice_text.char_indices() {
        if ch == '|' {
            let target = idx..(idx + 1);
            let mut next_out = Vec::new();
            let mut offset = 0usize;
            for run in out {
                let start = offset;
                let end = offset + run.len;
                offset = end;
                if end <= target.start || start >= target.end {
                    next_out.push(run);
                    continue;
                }
                let p_start = target.start.max(start);
                let p_end = target.end.min(end);
                if p_start > start {
                    next_out.push(TextRun {
                        len: p_start - start,
                        ..run.clone()
                    });
                }
                let mut pipe_font = run.font.clone();
                pipe_font.weight = FontWeight::NORMAL;
                next_out.push(TextRun {
                    len: p_end - p_start,
                    font: pipe_font,
                    color: border_color,
                    ..run.clone()
                });
                if p_end < end {
                    next_out.push(TextRun {
                        len: end - p_end,
                        ..run
                    });
                }
            }
            out = next_out;
        }
    }
    out
}

fn logical_line_number(content: &str, offset: usize) -> usize {
    content[..offset.min(content.len())]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1
}

fn raw_line_runs(
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
    apply_underline_to_runs(vec![base], local_start..local_end, color)
}

#[allow(clippy::too_many_arguments)]
fn paint_ranges_mapped(
    quads: &mut Vec<PaintQuad>,
    origin: Point<Pixels>,
    line_height: Pixels,
    row_start: usize,
    row_end: usize,
    offset_map: &RowOffsetMap,
    shaped: &ShapedLine,
    range: &Range<usize>,
    color: Hsla,
) {
    if range.end <= row_start || range.start > row_end || range.is_empty() {
        return;
    }
    let clamped_start = range.start.clamp(row_start, row_end);
    let clamped_end = range.end.clamp(row_start, row_end);
    if clamped_end <= clamped_start && range.start < row_end {
        return;
    }
    let local_start = offset_map.source_to_display(clamped_start);
    let local_end = offset_map.source_to_display(clamped_end);
    let x0 = shaped.x_for_index(local_start);
    let x1 = if local_end > local_start {
        shaped.x_for_index(local_end).max(x0 + px(2.0))
    } else if range.start <= row_start && range.end > row_end {
        (shaped.width).max(x0 + px(6.0))
    } else {
        return;
    };
    quads.push(fill(
        Bounds::from_corners(
            point(origin.x + x0, origin.y),
            point(origin.x + x1, origin.y + line_height),
        ),
        color,
    ));
}
