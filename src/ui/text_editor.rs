//! Multi-line editor surface for both Raw and WYSIWYG modes.
//! The focused entity implements [`EntityInputHandler`]; this element shapes
//! the text, paints block/inline decorations and the caret, and records
//! hit-test geometry with bidirectional source/display byte mapping.

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui::*;

use crate::markdown::{LineKind, ParsedDocument, ParsedLine, RowOffsetMap, StyledSegment};
use crate::model::text_ops::{visual_rows, Caret};
use crate::model::ViewMode;

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
    let display_idx = current_row.offset_map.source_to_display(cursor);
    let x_in_row = current_row.shaped.x_for_index(display_idx);
    let target_screen_x = current_row.origin.x + x_in_row;

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
    let local_x = (target_screen_x - target_row.origin.x).max(px(0.0));
    let target_display = target_row.shaped.closest_index_for_x(local_x);
    let target_source = target_row
        .offset_map
        .display_to_source(target_display)
        .clamp(target_row.byte_start, target_row.byte_end)
        .min(content.len());

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
                });
            }

            let total_height = base_line_height * rows.len() as f32 + px(32.0);
            return Prepaint {
                rows,
                numbers,
                decor_lines: Vec::new(),
                bg_quads: Vec::new(),
                quads,
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
        let mut cursor = None;
        let base_x = bounds.left() + gutter + x_offset;
        let mut cur_y = bounds.top() + px(16.0);

        for (line_idx, parsed_line) in self.parsed.lines.iter().enumerate() {
            let (row_font_size, row_line_height, top_gap, indent_x) =
                wysiwyg_line_metrics(&parsed_line.kind, self.font_size);
            cur_y += top_gap;

            let row_char_width = char_width * (row_font_size / self.font_size);
            let row_avail_width = (content_width - indent_x).max(px(32.0));
            let row_wrap_cols = if self.word_wrap {
                ((row_avail_width / row_char_width).floor() as usize).max(1)
            } else {
                usize::MAX
            };

            let sub_rows = visual_rows(&parsed_line.display_text, row_wrap_cols);
            let link_hits = collect_line_links(parsed_line);

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
                    LineKind::HorizontalRule | LineKind::TableDelimiter => {
                        let mid_y = cur_y + row_line_height * 0.5;
                        bg_quads.push(fill(
                            Bounds::new(point(base_x, mid_y), size(content_width, px(1.5))),
                            self.border_color,
                        ));
                    }
                    LineKind::TableRow { is_header, .. } => {
                        if *is_header {
                            bg_quads.push(fill(
                                Bounds::new(
                                    point(base_x, cur_y),
                                    size(content_width, row_line_height),
                                ),
                                self.surface_bg,
                            ));
                        }
                        bg_quads.push(fill(
                            Bounds::new(
                                point(base_x, cur_y + row_line_height - px(1.0)),
                                size(content_width, px(1.0)),
                            ),
                            self.border_color,
                        ));
                    }
                    LineKind::Heading { level: 1 | 2 } if is_last_sub => {
                        bg_quads.push(fill(
                            Bounds::new(
                                point(base_x, cur_y + row_line_height - px(1.0)),
                                size(content_width, px(1.0)),
                            ),
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
            let _ = row.shaped.paint(row.origin, row.line_height, window, cx);
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

fn wysiwyg_line_metrics(
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
        LineKind::HorizontalRule | LineKind::TableDelimiter => (
            base_font_size * 0.75,
            base_font_size * 1.1,
            px(2.0),
            px(0.0),
        ),
        LineKind::TableRow { .. } => {
            let fs = base_font_size * 0.95;
            (fs, base_font_size * 1.6, px(0.0), px(8.0))
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
        LineKind::Heading { .. } => (FontWeight::SEMIBOLD, editor.strong_color, false),
        LineKind::CodeLine | LineKind::CodeFence { .. } => {
            (FontWeight::NORMAL, editor.code_color, true)
        }
        LineKind::Blockquote => (FontWeight::NORMAL, editor.muted_color, false),
        LineKind::TableRow {
            is_header: true, ..
        } => (FontWeight::BOLD, editor.strong_color, true),
        LineKind::TableRow {
            is_header: false, ..
        } => (FontWeight::NORMAL, editor.text_color, true),
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

    let task_checked = matches!(line_kind, LineKind::ListItem { task: Some(true), .. });
    let strikethrough = if seg.strike || task_checked {
        Some(StrikethroughStyle {
            thickness: px(1.0),
            color: Some(editor.faint_color),
        })
    } else {
        None
    };

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
