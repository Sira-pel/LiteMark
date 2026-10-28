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
use crate::model::document::visible_line_end;
use crate::model::text_ops::{visual_rows, Caret};
use crate::model::ViewMode;
use crate::theme::sz;

#[derive(Clone)]
pub struct CellLine {
    pub text_origin: Point<Pixels>,
    pub line_height: Pixels,
    pub shaped: ShapedLine,
    pub offset_map: RowOffsetMap,
}

#[derive(Clone)]
pub struct PaintCell {
    pub raw_range: Range<usize>,
    pub inner_range: Range<usize>,
    pub box_bounds: Bounds<Pixels>,
    #[allow(dead_code)]
    pub text_origin: Point<Pixels>,
    #[allow(dead_code)]
    pub shaped: ShapedLine,
    #[allow(dead_code)]
    pub offset_map: RowOffsetMap,
    pub lines: Vec<CellLine>,
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

#[derive(Clone, Debug)]
pub struct TableColGeom {
    pub header_bounds: Bounds<Pixels>,
    pub align: TableColumnAlign,
    pub anchor: usize,
}

#[derive(Clone, Debug)]
pub struct TableRowGeom {
    pub index: usize,
    pub bounds: Bounds<Pixels>,
    pub anchors: Vec<usize>,
}

#[derive(Clone, Debug)]
pub struct TableGeom {
    pub source: Range<usize>,
    pub bounds: Bounds<Pixels>,
    pub columns: Vec<TableColGeom>,
    pub rows: Vec<TableRowGeom>,
    /// Visible editor area. Hover controls stay inside it. `None` means no limit.
    pub viewport: Option<Bounds<Pixels>>,
}

#[derive(Clone, Default)]
pub struct TableWidthCache {
    pub entries: std::collections::HashMap<Range<usize>, (u64, Pixels, Vec<Pixels>)>,
}

#[derive(Clone)]
pub struct EditorPaint {
    pub wrap_cols: usize,
    pub line_height: Pixels,
    pub total_height: Pixels,
    pub rows: Vec<PaintRow>,
    pub bounds: Option<Bounds<Pixels>>,
    pub table_width_cache: TableWidthCache,
    pub tables: Vec<TableGeom>,
}

impl Default for EditorPaint {
    fn default() -> Self {
        Self {
            wrap_cols: usize::MAX,
            line_height: px(28.0),
            total_height: px(0.0),
            rows: Vec::new(),
            bounds: None,
            table_width_cache: TableWidthCache::default(),
            tables: Vec::new(),
        }
    }
}

/// Cache key for a table's measured column widths: they depend on the text
/// and on the fonts used to draw it. Hashed in place so a redraw does not
/// copy the table's text.
fn table_width_key(table_text: &str, font: &str, mono_font: &str, font_size: Pixels) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    table_text.hash(&mut hasher);
    font.hash(&mut hasher);
    mono_font.hash(&mut hasher);
    f32::from(font_size).to_bits().hash(&mut hasher);
    hasher.finish()
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn str_display_width(s: &str) -> usize {
    s.chars()
        .map(|c| {
            if ('\u{1100}'..='\u{115F}').contains(&c)
                || ('\u{2E80}'..='\u{A4CF}').contains(&c)
                || ('\u{AC00}'..='\u{D7A3}').contains(&c)
                || ('\u{F900}'..='\u{FAFF}').contains(&c)
                || ('\u{FE10}'..='\u{FE19}').contains(&c)
                || ('\u{FE30}'..='\u{FE6F}').contains(&c)
                || ('\u{FF00}'..='\u{FF60}').contains(&c)
                || ('\u{FFE0}'..='\u{FFE6}').contains(&c)
                || ('\u{1F300}'..='\u{1FAFF}').contains(&c)
            {
                2
            } else {
                1
            }
        })
        .sum()
}

pub fn wrap_cell_text(
    cell_display: &str,
    avail_w: Pixels,
    shaped: &ShapedLine,
) -> Vec<(usize, usize)> {
    if cell_display.is_empty() {
        return vec![(0, 0)];
    }
    if shaped.width <= avail_w {
        return vec![(0, cell_display.len())];
    }

    let mut lines = Vec::new();
    let mut start = 0;
    let len = cell_display.len();

    while start < len {
        let start_x = shaped.x_for_index(start);
        let target_x = start_x + avail_w;
        let mut fit_idx = shaped.closest_index_for_x(target_x);

        while fit_idx > start && !cell_display.is_char_boundary(fit_idx) {
            fit_idx -= 1;
        }

        while fit_idx > start && shaped.x_for_index(fit_idx) - start_x > avail_w {
            let prev_char_len = cell_display[start..fit_idx]
                .chars()
                .next_back()
                .map(|c| c.len_utf8())
                .unwrap_or(0);
            if fit_idx - prev_char_len <= start {
                break;
            }
            fit_idx -= prev_char_len;
        }

        if fit_idx >= len {
            lines.push((start, len));
            break;
        }

        let mut break_idx = None;
        for (idx, ch) in cell_display[start..fit_idx].char_indices().rev() {
            if ch.is_whitespace() {
                break_idx = Some(start + idx);
                break;
            }
        }

        let (line_end, next_start) = if let Some(sp) = break_idx {
            if sp > start {
                let sp_char_len = cell_display[sp..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
                (sp, sp + sp_char_len)
            } else {
                let sp_char_len = cell_display[start..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
                (start + sp_char_len, start + sp_char_len)
            }
        } else {
            let advance = if fit_idx > start {
                fit_idx
            } else {
                let next_char_len = cell_display[start..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
                start + next_char_len
            };
            (advance, advance)
        };

        lines.push((start, line_end));
        start = next_start;
    }

    if lines.is_empty() {
        lines.push((0, 0));
    }
    lines
}

/// Horizontal padding inside a table cell, on each side. Shared by column
/// sizing and cell drawing so the gap before and after the text stays equal.
pub const TABLE_CELL_PAD_X: Pixels = px(12.0);

/// Spare width added to a fitted column so float rounding never makes the
/// text just miss its own column and wrap.
const TABLE_FIT_SLACK: Pixels = px(0.5);

/// `measure` returns the real drawn text width of one cell of `line`.
pub fn compute_table_column_widths(
    table_lines: &[&ParsedLine],
    content_width: Pixels,
    measure: &mut dyn FnMut(&ParsedLine, &Range<usize>) -> Pixels,
) -> Vec<Pixels> {
    if table_lines.is_empty() {
        return Vec::new();
    }
    let num_cols = table_lines
        .iter()
        .find_map(|line| {
            if let LineKind::TableRow { ref cells, .. } = line.kind {
                Some(cells.len())
            } else {
                None
            }
        })
        .unwrap_or(0);
    if num_cols == 0 {
        return Vec::new();
    }

    let target_table_w = content_width.max(px(1.0));
    let num_cols_f32 = num_cols as f32;
    // Start every column at an equal share of the editor width
    let equal_share = target_table_w / num_cols_f32;
    // Sensible min column width is 48px, but shrink as needed to keep table within editor width
    let min_col_w = (px(48.0)).min(equal_share);
    let pad_x = TABLE_CELL_PAD_X;

    // Measure natural content width for each column (minimum needed to avoid wrapping)
    let mut natural_w = vec![min_col_w; num_cols];
    for pline in table_lines {
        if let LineKind::TableRow { ref cells, .. } = pline.kind {
            for (col_idx, cell_range) in cells.iter().enumerate().take(num_cols) {
                let text_w = measure(pline, cell_range);
                let needed = (text_w + pad_x * 2.0 + TABLE_FIT_SLACK).max(min_col_w);
                if needed > natural_w[col_idx] {
                    natural_w[col_idx] = needed;
                }
            }
        }
    }

    // Start each column at equal share
    let mut widths = vec![equal_share; num_cols];

    // Identify expanding columns (content doesn't fit in equal share) and donor columns (spare room)
    // A column's floor is what its content needs (or min_col_w), but at most equal_share
    // A donor column can only shrink down to natural_w[c].max(min_col_w) without causing its content to wrap.
    let mut total_wanted = px(0.0);
    let mut total_spare = px(0.0);
    for &w in &natural_w {
        if w > equal_share {
            total_wanted += w - equal_share;
        } else {
            let floor = w.max(min_col_w);
            total_spare += equal_share - floor;
        }
    }

    if total_wanted > px(0.0) && total_spare > px(0.0) {
        let extra_to_transfer = total_wanted.min(total_spare);

        // 1. Take extra_to_transfer evenly from donor columns
        let mut remaining_reduction = extra_to_transfer;
        while remaining_reduction > px(0.01) {
            let active_donors: Vec<usize> = (0..num_cols)
                .filter(|&c| widths[c] > natural_w[c].max(min_col_w))
                .collect();
            if active_donors.is_empty() {
                break;
            }
            let fair_reduction = remaining_reduction / (active_donors.len() as f32);
            let mut reduced_this_round = px(0.0);
            for &c in &active_donors {
                let floor = natural_w[c].max(min_col_w);
                let can_give = widths[c] - floor;
                let take = fair_reduction.min(can_give).min(remaining_reduction);
                widths[c] -= take;
                reduced_this_round += take;
            }
            if reduced_this_round <= px(0.001) {
                break;
            }
            remaining_reduction = (remaining_reduction - reduced_this_round).max(px(0.0));
        }

        // 2. Give extra_to_transfer evenly to expanding columns
        let mut remaining_addition = extra_to_transfer;
        while remaining_addition > px(0.01) {
            let active_expanders: Vec<usize> = (0..num_cols)
                .filter(|&c| widths[c] < natural_w[c])
                .collect();
            if active_expanders.is_empty() {
                // If all expanders have reached natural_w, any remaining addition is split evenly among all expanders
                let extra_share = remaining_addition / (num_cols as f32);
                for w in &mut widths {
                    *w += extra_share;
                }
                break;
            }
            let fair_addition = remaining_addition / (active_expanders.len() as f32);
            let mut added_this_round = px(0.0);
            for &c in &active_expanders {
                let needed = natural_w[c] - widths[c];
                let give = fair_addition.min(needed).min(remaining_addition);
                widths[c] += give;
                added_this_round += give;
            }
            if added_this_round <= px(0.001) {
                break;
            }
            remaining_addition = (remaining_addition - added_this_round).max(px(0.0));
        }
    }

    // Ensure sum matches target_table_w exactly and no column is below min_col_w
    let sum_first_n_minus_1: Pixels = widths[..num_cols - 1].iter().copied().fold(px(0.0), |acc, x| acc + x);
    let last_w = (target_table_w - sum_first_n_minus_1).max(min_col_w);
    widths[num_cols - 1] = last_w;

    // Final safety clamp: if sum exceeds target_table_w due to float precision, trim from largest
    let total_sum: Pixels = widths.iter().copied().fold(px(0.0), |acc, x| acc + x);
    if total_sum > target_table_w {
        let excess = total_sum - target_table_w;
        if let Some((max_idx, _)) = widths.iter().enumerate().max_by(|a, b| a.1.partial_cmp(b.1).unwrap()) {
            widths[max_idx] = (widths[max_idx] - excess).max(min_col_w);
        }
    }

    widths
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

/// True when the caret sits in a wrapped table cell that has another visual
/// line in the `delta` direction, so Up/Down should move inside the cell.
pub fn cell_has_line_in_direction(paint: &EditorPaint, cursor: usize, delta: i32) -> bool {
    for row in &paint.rows {
        for cell in &row.cells {
            if cursor < cell.raw_range.start || cursor > cell.raw_range.end {
                continue;
            }
            let sub_ix = cell
                .lines
                .iter()
                .position(|l| cursor >= l.offset_map.line_start && cursor < l.offset_map.line_end)
                .or_else(|| cell.lines.iter().rposition(|l| cursor >= l.offset_map.line_start))
                .unwrap_or(0);
            return if delta > 0 {
                sub_ix + 1 < cell.lines.len()
            } else {
                sub_ix > 0
            };
        }
    }
    false
}

#[cfg(test)]
pub fn move_vertical_visual(
    paint: &EditorPaint,
    content: &str,
    caret: &mut Caret,
    delta: i32,
    extend: bool,
    mode: ViewMode,
) -> bool {
    let parsed = crate::markdown::parse_document(content);
    move_vertical_visual_in(paint, &parsed, content, caret, delta, extend, mode)
}

/// Move the caret one visual row up or down. `parsed` is the parse of
/// `content`; the editor passes its cached one so arrow keys never reparse.
pub fn move_vertical_visual_in(
    paint: &EditorPaint,
    parsed: &ParsedDocument,
    content: &str,
    caret: &mut Caret,
    delta: i32,
    extend: bool,
    mode: ViewMode,
) -> bool {
    if paint.rows.is_empty() {
        return false;
    }
    let snap = |offset: usize| -> usize {
        let Some(line) = parsed.line_at_offset(offset).filter(|_| !extend) else {
            return offset;
        };
        let vend = visible_line_end(line, mode);
        if offset > vend && offset <= line.source.end {
            vend
        } else {
            offset
        }
    };
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
    let (target_screen_x, current_cell_sub_ix) = if !current_row.cells.is_empty() {
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
            .or_else(|| current_row.cells.first())
            .unwrap();
        let sub_ix = cell
            .lines
            .iter()
            .position(|l| cursor >= l.offset_map.line_start && cursor < l.offset_map.line_end)
            .or_else(|| cell.lines.iter().rposition(|l| cursor >= l.offset_map.line_start))
            .unwrap_or(0);
        if let Some(cur_line) = cell.lines.get(sub_ix).or_else(|| cell.lines.first()) {
            let clamped = cursor.clamp(cur_line.offset_map.line_start, cur_line.offset_map.line_end);
            let display_idx = cur_line.offset_map.source_to_display(clamped);
            let x_in_cell = cur_line.shaped.x_for_index(display_idx);
            let tx = (cur_line.text_origin.x + x_in_cell)
                .clamp(cell.box_bounds.left() + px(1.0), (cell.box_bounds.right() - px(1.0)).max(cell.box_bounds.left() + px(1.0)));
            (tx, Some((cell, sub_ix)))
        } else {
            let clamped = cursor.clamp(cell.inner_range.start, cell.inner_range.end);
            let display_idx = cell.offset_map.source_to_display(clamped);
            let x_in_cell = cell.shaped.x_for_index(display_idx);
            let tx = (cell.text_origin.x + x_in_cell)
                .clamp(cell.box_bounds.left() + px(1.0), (cell.box_bounds.right() - px(1.0)).max(cell.box_bounds.left() + px(1.0)));
            (tx, Some((cell, 0)))
        }
    } else {
        let display_idx = current_row.offset_map.source_to_display(cursor);
        let x_in_row = current_row.shaped.x_for_index(display_idx);
        (current_row.origin.x + x_in_row, None)
    };

    // If moving vertically inside the same wrapped cell:
    if let Some((cell, sub_ix)) = current_cell_sub_ix {
        if delta > 0 && sub_ix + 1 < cell.lines.len() {
            let next_line = &cell.lines[sub_ix + 1];
            let local_x = (target_screen_x - next_line.text_origin.x).max(px(0.0));
            let target_display = next_line.shaped.closest_index_for_x(local_x);
            let target_source = next_line
                .offset_map
                .display_to_source(target_display)
                .clamp(cell.inner_range.start, cell.inner_range.end);
            let target_source = snap(target_source);
            if extend {
                caret.select_to(target_source);
            } else {
                caret.move_to(target_source);
            }
            return true;
        } else if delta < 0 && sub_ix > 0 {
            let prev_line = &cell.lines[sub_ix - 1];
            let local_x = (target_screen_x - prev_line.text_origin.x).max(px(0.0));
            let target_display = prev_line.shaped.closest_index_for_x(local_x);
            let target_source = prev_line
                .offset_map
                .display_to_source(target_display)
                .clamp(cell.inner_range.start, cell.inner_range.end);
            let target_source = snap(target_source);
            if extend {
                caret.select_to(target_source);
            } else {
                caret.move_to(target_source);
            }
            return true;
        }
    }

    let target_ix = if delta < 0 {
        if current_row_ix == 0 {
            let target = snap(0);
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
            let target = snap(content.len());
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
                if target_screen_x < target_row.cells.first().map(|c| c.box_bounds.left()).unwrap_or(px(0.0)) {
                    target_row.cells.first().unwrap()
                } else {
                    target_row.cells.last().unwrap()
                }
            });
        let target_line = if delta > 0 {
            cell.lines.first()
        } else {
            cell.lines.last()
        };
        if let Some(target_line) = target_line {
            let local_x = (target_screen_x - target_line.text_origin.x).max(px(0.0));
            let target_display = target_line.shaped.closest_index_for_x(local_x);
            target_line
                .offset_map
                .display_to_source(target_display)
                .clamp(cell.inner_range.start, cell.inner_range.end)
                .min(content.len())
        } else {
            let local_x = (target_screen_x - cell.text_origin.x).max(px(0.0));
            let target_display = cell.shaped.closest_index_for_x(local_x);
            cell.offset_map
                .display_to_source(target_display)
                .clamp(cell.inner_range.start, cell.inner_range.end)
                .min(content.len())
        }
    } else {
        let local_x = (target_screen_x - target_row.origin.x).max(px(0.0));
        let target_display = target_row.shaped.closest_index_for_x(local_x);
        target_row
            .offset_map
            .display_to_source(target_display)
            .clamp(target_row.byte_start, target_row.byte_end)
            .min(content.len())
    };
    let target_source = snap(target_source);

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
                if position.x < row.cells.first().map(|c| c.box_bounds.left()).unwrap_or(px(0.0)) {
                    row.cells.first().unwrap()
                } else {
                    row.cells.last().unwrap()
                }
            });
        let line_opt = if cell.lines.is_empty() {
            None
        } else {
            let per_line_h = cell.lines[0].line_height.max(px(1.0));
            let sub_ix = if position.y >= cell.box_bounds.top() {
                let idx = ((position.y - cell.box_bounds.top()) / per_line_h).floor() as usize;
                idx.min(cell.lines.len() - 1)
            } else {
                0
            };
            cell.lines.get(sub_ix).or_else(|| cell.lines.first())
        };
        if let Some(line) = line_opt {
            let local_x = (position.x - line.text_origin.x).max(px(0.0));
            let display_idx = line.shaped.closest_index_for_x(local_x);
            return line
                .offset_map
                .display_to_source(display_idx)
                .clamp(cell.inner_range.start, cell.inner_range.end);
        } else {
            let local_x = (position.x - cell.text_origin.x).max(px(0.0));
            let display_idx = cell.shaped.closest_index_for_x(local_x);
            return cell
                .offset_map
                .display_to_source(display_idx)
                .clamp(cell.inner_range.start, cell.inner_range.end);
        }
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

pub fn editor_home_visual(paint: &EditorPaint, caret: &mut Caret, extend: bool) -> bool {
    let cursor = caret.cursor();
    for row in &paint.rows {
        if !row.cells.is_empty() {
            for cell in &row.cells {
                if cursor >= cell.raw_range.start && cursor <= cell.raw_range.end {
                    let sub_ix = cell
                        .lines
                        .iter()
                        .position(|l| cursor >= l.offset_map.line_start && cursor < l.offset_map.line_end)
                        .or_else(|| cell.lines.iter().rposition(|l| cursor >= l.offset_map.line_start))
                        .unwrap_or(0);
                    let line = cell.lines.get(sub_ix).or_else(|| cell.lines.first());
                    let target = if let Some(line) = line {
                        if cursor > line.offset_map.line_start {
                            line.offset_map.line_start
                        } else {
                            cell.inner_range.start
                        }
                    } else {
                        cell.inner_range.start
                    };
                    if extend {
                        caret.select_to(target);
                    } else {
                        caret.move_to(target);
                    }
                    return true;
                }
            }
        }
    }
    false
}

pub fn editor_end_visual(paint: &EditorPaint, caret: &mut Caret, extend: bool) -> bool {
    let cursor = caret.cursor();
    for row in &paint.rows {
        if !row.cells.is_empty() {
            for cell in &row.cells {
                if cursor >= cell.raw_range.start && cursor <= cell.raw_range.end {
                    let sub_ix = cell
                        .lines
                        .iter()
                        .position(|l| cursor >= l.offset_map.line_start && cursor < l.offset_map.line_end)
                        .or_else(|| cell.lines.iter().rposition(|l| cursor >= l.offset_map.line_start))
                        .unwrap_or(cell.lines.len().saturating_sub(1));
                    let line = cell.lines.get(sub_ix).or_else(|| cell.lines.last());
                    let target = if let Some(line) = line {
                        if cursor < line.offset_map.line_end {
                            line.offset_map.line_end
                        } else {
                            cell.inner_range.end
                        }
                    } else {
                        cell.inner_range.end
                    };
                    if extend {
                        caret.select_to(target);
                    } else {
                        caret.move_to(target);
                    }
                    return true;
                }
            }
        }
    }
    false
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
    pub active_find_color: Hsla,
    pub gutter_color: Hsla,
    pub gutter_bg: Hsla,
    pub font_family: SharedString,
    pub mono_font_family: SharedString,
    pub table_hover: Option<super::table_chrome::TableHover>,
    pub table_drag: Option<super::table_chrome::TableDrag>,
    pub table_hot: Option<super::table_chrome::TableHot>,
    pub table_pressed: bool,
    pub table_hover_fill: Hsla,
    pub table_active_fill: Hsla,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EditorColors {
    pub text_color: Hsla,
    pub strong_color: Hsla,
    pub muted_color: Hsla,
    pub h1_color: Hsla,
    pub h2_color: Hsla,
    pub code_color: Hsla,
    pub code_bg: Hsla,
    pub link_color: Hsla,
    pub border_color: Hsla,
}

impl<V: 'static> EditorElement<V> {
    pub(crate) fn colors(&self) -> EditorColors {
        EditorColors {
            text_color: self.text_color,
            strong_color: self.strong_color,
            muted_color: self.muted_color,
            h1_color: self.h1_color,
            h2_color: self.h2_color,
            code_color: self.code_color,
            code_bg: self.code_bg,
            link_color: self.link_color,
            border_color: self.border_color,
        }
    }

    fn numbered(&self) -> bool {
        self.show_line_numbers && self.view_mode == ViewMode::Raw
    }

    /// `(gutter_box, text_left)`: the painted line-number column, sized to
    /// the widest number, and where text starts. Without numbers the box is
    /// empty and text keeps a fixed left margin.
    fn gutter(&self, char_width: Pixels, line_count: usize, pad: Pixels) -> (Pixels, Pixels) {
        if self.numbered() {
            let digits = line_count.max(1).ilog10() as f32 + 1.0;
            let gutter_box = pad * 2.0 + char_width * digits.max(2.0);
            (gutter_box, gutter_box + pad)
        } else {
            (px(0.0), px(24.0))
        }
    }
}

pub struct Prepaint {
    rows: Vec<PaintRow>,
    numbers: Vec<(Point<Pixels>, Pixels, ShapedLine)>,
    decor_lines: Vec<(Point<Pixels>, Pixels, ShapedLine)>,
    /// Checkmarks painted over checked task boxes: bounds and color.
    check_icons: Vec<(Bounds<Pixels>, Hsla)>,
    bg_quads: Vec<PaintQuad>,
    quads: Vec<PaintQuad>,
    strike_quads: Vec<PaintQuad>,
    cursor: Option<PaintQuad>,
    line_height: Pixels,
    total_height: Pixels,
    wrap_cols: usize,
    x_offset: Pixels,
    /// Width of the painted line-number column (zero when numbers are off).
    gutter_box: Pixels,
    table_width_cache: TableWidthCache,
    tables: Vec<TableGeom>,
}

/// Space on each side of a line number inside the gutter, and between the
/// gutter's divider and the text. Halved when the editor pane is narrow.
fn gutter_pad(editor_width: Pixels) -> Pixels {
    if editor_width < px(400.0) {
        px(6.0)
    } else {
        px(12.0)
    }
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
                if matches!(line.kind, LineKind::TableDelimiter) {
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
        // The parse is cached per revision, so this is O(1) per redraw.
        let line_count = self.parsed.lines.len();
        let inset = gutter_pad(bounds.size.width);
        let (gutter_box, gutter) = self.gutter(char_width, line_count, inset);
        let available_width = (bounds.size.width - gutter - px(48.0)).max(px(32.0));
        let (content_width, x_offset) = if self.centered {
            let max_w = self.max_measure.unwrap_or(available_width).min(available_width);
            let mut offset = ((available_width - max_w) / 2.0).max(px(0.0));
            if !self.numbered() {
                // The text column should sit on the pane's true midpoint, so
                // both margins match. Keep 24px on the right when narrow.
                let width = bounds.size.width;
                let centered = (width - max_w) / 2.0 - gutter;
                let room = width - px(24.0) - gutter - max_w;
                offset = centered.min(room).max(px(0.0));
            }
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
            let mut bg_quads = Vec::new();
            let mut quads = Vec::new();
            let mut cursor = None;
            let mut line_number = 0usize;
            let caret_line = (self.numbered() && self.selection.is_empty())
                .then(|| logical_line_number(&self.content, self.selection.start));
            let current_line_color = Hsla {
                a: self.selection_color.a * 0.35,
                ..self.selection_color
            };

            for (index, (start, end)) in rows_src.iter().copied().enumerate() {
                let at_line_start = index == 0
                    || self.content.as_bytes().get(start.wrapping_sub(1)) == Some(&b'\n');
                if at_line_start {
                    line_number += 1;
                }
                let is_current_line = caret_line == Some(line_number);
                let slice = if start == end {
                    SharedString::from(" ")
                } else {
                    SharedString::from(self.content[start..end].to_string())
                };
                let display_len = if start == end { 0 } else { slice.len() };
                let runs = raw_line_runs(
                    display_len.max(slice.len()),
                    start,
                    self.marked.clone(),
                    base_font.clone(),
                    self.text_color,
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
                for (hit_index, hit) in hits_near(&self.find_hits, start, end) {
                    let color = if self.active_find == Some(hit_index) {
                        self.active_find_color
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
                if is_current_line {
                    bg_quads.push(fill(
                        Bounds::new(
                            point(bounds.left() + x_offset, origin.y),
                            // Left-aligned text runs to the pane's edge; a
                            // centred column keeps the band under the text.
                            size(
                                if self.centered {
                                    gutter + content_width
                                } else {
                                    bounds.size.width - x_offset
                                },
                                base_line_height,
                            ),
                        ),
                        current_line_color,
                    ));
                }
                // Wrapped continuation rows get no number.
                if self.numbered() && at_line_start {
                    let label: SharedString = format!("{line_number}").into();
                    let num_run = TextRun {
                        len: label.len(),
                        font: base_font.clone(),
                        color: if is_current_line {
                            self.text_color
                        } else {
                            self.gutter_color
                        },
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    let shaped_num =
                        window
                            .text_system()
                            .shape_line(label, self.font_size, &[num_run], None);
                    // Right-aligned against the gutter's inner edge.
                    let number_x = bounds.left() + x_offset + gutter_box - inset - shaped_num.width;
                    numbers.push((point(number_x, origin.y), base_line_height, shaped_num));
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
            // Raw mode only carries the cache through to the next frame.
            let table_width_cache = std::mem::take(&mut self.paint.borrow_mut().table_width_cache);
            return Prepaint {
                rows,
                numbers,
                decor_lines: Vec::new(),
                check_icons: Vec::new(),
                bg_quads,
                quads,
                strike_quads: Vec::new(),
                cursor,
                line_height: base_line_height,
                total_height,
                wrap_cols,
                x_offset,
                gutter_box,
                table_width_cache,
                tables: Vec::new(),
            };
        }

        // WYSIWYG mode prepaint
        let mut rows = Vec::with_capacity(self.parsed.lines.len());
        let mut decor_lines = Vec::new();
        let mut check_icons: Vec<(Bounds<Pixels>, Hsla)> = Vec::new();
        let mut bg_quads = Vec::new();
        let mut quads = Vec::new();
        let mut strike_quads = Vec::new();
        let mut tables: Vec<TableGeom> = Vec::new();
        let mut cursor = None;
        let base_x = bounds.left() + gutter + x_offset;
        let mut cur_y = bounds.top() + px(16.0);
        // Single pass to collect all table rows grouped by table_source
        let mut table_rows: std::collections::HashMap<Range<usize>, Vec<&ParsedLine>> =
            std::collections::HashMap::new();
        for line in &self.parsed.lines {
            if let LineKind::TableRow { ref table_source, .. } = line.kind {
                table_rows.entry(table_source.clone()).or_default().push(line);
            }
        }

        let mut table_width_cache = std::mem::take(&mut self.paint.borrow_mut().table_width_cache);
        table_width_cache.entries.retain(|k, _| table_rows.contains_key(k));

        let mut table_col_widths: std::collections::HashMap<Range<usize>, Vec<Pixels>> =
            std::collections::HashMap::new();

        let colors = self.colors();

        for (line_idx, parsed_line) in self.parsed.lines.iter().enumerate() {
            if matches!(parsed_line.kind, LineKind::TableDelimiter) {
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
                ref table_source,
            } = parsed_line.kind
            {
                let col_widths = table_col_widths
                    .entry(table_source.clone())
                    .or_insert_with(|| {
                        let table_text = self.content.get(table_source.clone()).unwrap_or("");
                        let table_hash = table_width_key(
                            table_text,
                            &self.font_family,
                            &self.mono_font_family,
                            self.font_size,
                        );
                        if let Some((cached_hash, cached_width, widths)) =
                            table_width_cache.entries.get(table_source)
                        {
                            if *cached_hash == table_hash && *cached_width == content_width {
                                return widths.clone();
                            }
                        }
                        let empty_lines = Vec::new();
                        let lines = table_rows.get(table_source).unwrap_or(&empty_lines);
                        let mut measure = |pline: &ParsedLine, cell_range: &Range<usize>| -> Pixels {
                            let (row_fs, _, _, _) =
                                wysiwyg_line_metrics(&pline.kind, self.font_size);
                            let raw = self.content.get(cell_range.clone()).unwrap_or("");
                            let (display, map) = RowOffsetMap::from_hidden(
                                raw,
                                cell_range.start,
                                cell_range.start,
                                &pline.hidden_ranges,
                            );
                            if display.is_empty() {
                                return px(0.0);
                            }
                            let slice = SharedString::from(display.clone());
                            let runs = wysiwyg_cell_runs(
                                pline,
                                display.len(),
                                slice.len(),
                                &map,
                                self.marked.as_ref(),
                                &base_font,
                                &mono_font,
                                &colors,
                            );
                            window
                                .text_system()
                                .shape_line(slice, row_fs, &runs, None)
                                .width
                        };
                        let widths = compute_table_column_widths(lines, content_width, &mut measure);
                        table_width_cache.entries.insert(
                            table_source.clone(),
                            (table_hash, content_width, widths.clone()),
                        );
                        widths
                    });

                let num_cols = col_widths.len().max(1);
                let table_w: Pixels = col_widths.iter().copied().fold(px(0.0), |acc, x| acc + x);
                let pad_x = TABLE_CELL_PAD_X;

                let mut col_x_offsets = Vec::with_capacity(num_cols + 1);
                let mut cur_col_x = px(0.0);
                for &w in col_widths.iter() {
                    col_x_offsets.push(cur_col_x);
                    cur_col_x += w;
                }
                col_x_offsets.push(cur_col_x);

                struct CellPrep {
                    cell_range: Range<usize>,
                    raw_range: Range<usize>,
                    cell_x: Pixels,
                    col_w: Pixels,
                    lines: Vec<(usize, usize, RowOffsetMap, ShapedLine, Pixels)>,
                    default_shaped: ShapedLine,
                    default_map: RowOffsetMap,
                }

                let mut cell_preps = Vec::with_capacity(num_cols);
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

                    let col_w = col_widths.get(col_idx).copied().unwrap_or(px(48.0));
                    let cell_x = base_x + col_x_offsets.get(col_idx).copied().unwrap_or(px(0.0));
                    let avail_inner = (col_w - pad_x * 2.0).max(px(0.0));

                    let cell_raw_str = self.content.get(cell_range.clone()).unwrap_or("");
                    let (cell_display, cell_map) = RowOffsetMap::from_hidden(
                        cell_raw_str,
                        cell_range.start,
                        cell_range.start,
                        &parsed_line.hidden_ranges,
                    );
                    let disp_len = cell_display.len();
                    let full_slice: SharedString = if disp_len == 0 {
                        SharedString::from(" ")
                    } else {
                        SharedString::from(cell_display.clone())
                    };

                    let runs = wysiwyg_cell_runs(
                        parsed_line,
                        disp_len,
                        full_slice.len(),
                        &cell_map,
                        self.marked.as_ref(),
                        &base_font,
                        &mono_font,
                        &colors,
                    );
                    let full_shaped = window.text_system().shape_line(
                        full_slice,
                        row_font_size,
                        &runs,
                        None,
                    );

                    let wrapped_slices = wrap_cell_text(&cell_display, avail_inner, &full_shaped);
                    let mut lines = Vec::with_capacity(wrapped_slices.len());

                    for (d_start, d_end) in wrapped_slices {
                        let sub_len = d_end.saturating_sub(d_start);
                        let sub_map = if d_start == 0 && d_end == disp_len {
                            cell_map.clone()
                        } else {
                            cell_map.slice_display(d_start..d_end)
                        };
                        let sub_slice: SharedString = if sub_len == 0 {
                            SharedString::from(" ")
                        } else {
                            SharedString::from(cell_display[d_start..d_end].to_string())
                        };
                        let sub_runs = wysiwyg_cell_runs(
                            parsed_line,
                            sub_len,
                            sub_slice.len(),
                            &sub_map,
                            self.marked.as_ref(),
                            &base_font,
                            &mono_font,
                            &colors,
                        );
                        let shaped_sub = window.text_system().shape_line(
                            sub_slice,
                            row_font_size,
                            &sub_runs,
                            None,
                        );

                        let text_w = if sub_len == 0 { px(0.0) } else { shaped_sub.width };
                        let line_text_x = match align {
                            TableColumnAlign::None | TableColumnAlign::Left => cell_x + pad_x,
                            TableColumnAlign::Center => {
                                cell_x + pad_x + ((avail_inner - text_w) * 0.5).max(px(0.0))
                            }
                            TableColumnAlign::Right => {
                                cell_x + pad_x + (avail_inner - text_w).max(px(0.0))
                            }
                        };
                        lines.push((d_start, d_end, sub_map, shaped_sub, line_text_x));
                    }

                    cell_preps.push(CellPrep {
                        cell_range,
                        raw_range,
                        cell_x,
                        col_w,
                        lines,
                        default_shaped: full_shaped,
                        default_map: cell_map,
                    });
                }

                let max_subrows = cell_preps.iter().map(|p| p.lines.len()).max().unwrap_or(1);
                let row_height = row_line_height * (max_subrows as f32);

                // Table row horizontal borders
                if is_header {
                    bg_quads.push(fill(
                        Bounds::new(point(base_x, cur_y), size(table_w, sz::STROKE)),
                        self.border_color,
                    ));
                }
                bg_quads.push(fill(
                    Bounds::new(
                        point(base_x, cur_y + row_height - sz::STROKE),
                        size(table_w, sz::STROKE),
                    ),
                    self.border_color,
                ));

                // Vertical borders
                for &offset in &col_x_offsets {
                    bg_quads.push(fill(
                        Bounds::new(point(base_x + offset, cur_y), size(sz::STROKE, row_height)),
                        self.border_color,
                    ));
                }
                bg_quads.push(fill(
                    Bounds::new(
                        point(base_x + table_w - sz::STROKE, cur_y),
                        size(sz::STROKE, row_height),
                    ),
                    self.border_color,
                ));

                let mut paint_cells = Vec::with_capacity(num_cols);
                for (col_idx, prep) in cell_preps.into_iter().enumerate() {
                    let box_bounds = Bounds::new(point(prep.cell_x, cur_y), size(prep.col_w, row_height));
                    let mut cell_lines = Vec::with_capacity(prep.lines.len());

                    for (k, (_d_start, _d_end, sub_map, shaped_sub, line_text_x)) in prep.lines.into_iter().enumerate() {
                        let line_origin = point(line_text_x, cur_y + row_line_height * (k as f32));

                        // Selection and Find highlights inside cell
                        paint_ranges_mapped(
                            &mut quads,
                            line_origin,
                            row_line_height,
                            sub_map.line_start,
                            sub_map.line_end,
                            &sub_map,
                            &shaped_sub,
                            &self.selection,
                            self.selection_color,
                        );
                        for (hit_index, hit) in
                            hits_near(&self.find_hits, sub_map.line_start, sub_map.line_end)
                        {
                            let color = if self.active_find == Some(hit_index) {
                                self.active_find_color
                            } else {
                                self.find_color
                            };
                            paint_ranges_mapped(
                                &mut quads,
                                line_origin,
                                row_line_height,
                                sub_map.line_start,
                                sub_map.line_end,
                                &sub_map,
                                &shaped_sub,
                                hit,
                                color,
                            );
                        }

                        // Strikethrough inside cell
                        for seg in &parsed_line.styled_segments {
                            if !seg.strike
                                || seg.source.end <= sub_map.line_start
                                || seg.source.start >= sub_map.line_end
                            {
                                continue;
                            }
                            let s_start =
                                sub_map.source_to_display(seg.source.start.max(sub_map.line_start));
                            let s_end =
                                sub_map.source_to_display(seg.source.end.min(sub_map.line_end));
                            if s_end > s_start {
                                let x1 = shaped_sub.x_for_index(s_start);
                                let x2 = shaped_sub.x_for_index(s_end);
                                let strike_x = (line_origin.x + x1 - px(2.0)).max(prep.cell_x + px(2.0));
                                let strike_w = (x2 - x1) + px(4.0);
                                let pad = (row_line_height - shaped_sub.ascent - shaped_sub.descent)
                                    .max(px(0.0))
                                    / 2.0;
                                let strike_y = line_origin.y + (pad + shaped_sub.ascent * 1.5) * 0.5;
                                let color = if seg.bold || seg.italic {
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

                        cell_lines.push(CellLine {
                            text_origin: line_origin,
                            line_height: row_line_height,
                            shaped: shaped_sub,
                            offset_map: sub_map,
                        });
                    }

                    // Caret inside cell
                    let caret_in_cell = (self.selection.start >= prep.raw_range.start
                        && self.selection.start <= prep.raw_range.end)
                        || (col_idx == 0
                            && self.selection.start >= parsed_line.source.start
                            && self.selection.start < prep.raw_range.start)
                        || (col_idx + 1 == num_cols
                            && self.selection.start > prep.raw_range.end
                            && self.selection.start <= parsed_line.source.end);

                    if focused && self.selection.is_empty() && caret_in_cell && cursor.is_none() {
                        let target_line = cell_lines
                            .iter()
                            .find(|l| {
                                self.selection.start >= l.offset_map.line_start
                                    && self.selection.start < l.offset_map.line_end
                            })
                            .or_else(|| {
                                cell_lines
                                    .iter()
                                    .rposition(|l| self.selection.start >= l.offset_map.line_start)
                                    .map(|i| &cell_lines[i])
                            })
                            .or_else(|| cell_lines.first());

                        if let Some(target_line) = target_line {
                            let clamped_src = self
                                .selection
                                .start
                                .clamp(target_line.offset_map.line_start, target_line.offset_map.line_end);
                            let display_idx = target_line.offset_map.source_to_display(clamped_src);
                            let x = target_line.shaped.x_for_index(display_idx);
                            let pad = (row_line_height - target_line.shaped.ascent - target_line.shaped.descent)
                                .max(px(0.0))
                                / 2.0;
                            let caret_y = target_line.text_origin.y + pad;
                            let caret_h =
                                (target_line.shaped.ascent + target_line.shaped.descent).max(row_font_size);
                            cursor = Some(fill(
                                Bounds::new(
                                    point(target_line.text_origin.x + x, caret_y),
                                    size(px(2.0), caret_h),
                                ),
                                self.cursor_color,
                            ));
                        }
                    }

                    let default_origin = cell_lines.first().map(|l| l.text_origin).unwrap_or(point(prep.cell_x + pad_x, cur_y));
                    let default_map = cell_lines.first().map(|l| l.offset_map.clone()).unwrap_or(prep.default_map);
                    let default_shaped = cell_lines.first().map(|l| l.shaped.clone()).unwrap_or(prep.default_shaped);

                    paint_cells.push(PaintCell {
                        raw_range: prep.raw_range,
                        inner_range: prep.cell_range,
                        box_bounds,
                        text_origin: default_origin,
                        shaped: default_shaped,
                        offset_map: default_map,
                        lines: cell_lines,
                    });
                }

                let dummy_shaped = paint_cells
                    .first()
                    .map(|c| c.shaped.clone())
                    .unwrap_or_else(|| sample.clone());
                super::table_chrome::record_table_row(
                    &mut tables,
                    table_source.clone(),
                    is_header,
                    alignments,
                    Bounds::new(point(base_x, cur_y), size(table_w, row_height)),
                    &paint_cells,
                );
                rows.push(PaintRow {
                    line_index: line_idx,
                    byte_start: parsed_line.source.start,
                    byte_end: parsed_line.source.end,
                    origin: point(base_x, cur_y),
                    line_height: row_height,
                    shaped: dummy_shaped,
                    offset_map: parsed_line.offset_map.clone(),
                    checkbox_hit: None,
                    link_hits,
                    cells: paint_cells,
                });
                cur_y += row_height;
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
                            self.border_color,
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
                                let icon_sz = box_sz + px(2.0);
                                check_icons.push((
                                    Bounds::new(
                                        point(box_x - px(1.0), box_y - px(1.0)),
                                        size(icon_sz, icon_sz),
                                    ),
                                    check_mark_color(self.link_color),
                                ));
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
                    &colors,
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
                for (hit_index, hit) in
                    hits_near(&self.find_hits, sub_map.line_start, sub_map.line_end)
                {
                    let color = if self.active_find == Some(hit_index) {
                        self.active_find_color
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
                        let color = if seg.bold || seg.italic { self.strong_color } else { self.text_color };
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
            check_icons,
            bg_quads,
            quads,
            strike_quads,
            cursor,
            line_height: base_line_height,
            total_height,
            wrap_cols,
            x_offset,
            gutter_box,
            table_width_cache,
            tables,
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
        if self.numbered() {
            // Run to the bottom of the window even when the text is short;
            // the scroll container clips whatever lies past the editor pane.
            let height = bounds
                .size
                .height
                .max(window.viewport_size().height - bounds.origin.y);
            let left = bounds.origin.x + prepaint.x_offset;
            window.paint_quad(fill(
                Bounds::new(
                    point(left, bounds.origin.y),
                    size(prepaint.gutter_box, height),
                ),
                self.gutter_bg,
            ));
            window.paint_quad(fill(
                Bounds::new(
                    point(left + prepaint.gutter_box - px(1.0), bounds.origin.y),
                    size(px(1.0), height),
                ),
                self.border_color,
            ));
        }
        for quad in prepaint.bg_quads.drain(..) {
            window.paint_quad(quad);
        }
        for quad in prepaint.quads.drain(..) {
            window.paint_quad(quad);
        }
        for (icon_bounds, color) in prepaint.check_icons.drain(..) {
            let _ = window.paint_svg(
                icon_bounds,
                "icons/check.svg".into(),
                TransformationMatrix::default(),
                color,
                cx,
            );
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
                    window.with_content_mask(Some(ContentMask { bounds: cell.box_bounds }), |window| {
                        for line in &cell.lines {
                            let _ = line
                                .shaped
                                .paint(line.text_origin, line.line_height, window, cx);
                        }
                    });
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
            // Painting is done with the rows, so hand them over instead of
            // deep-copying every shaped line each frame.
            paint.rows = std::mem::take(&mut prepaint.rows);
            if self.word_wrap {
                paint.wrap_cols = prepaint.wrap_cols;
            } else {
                paint.wrap_cols = usize::MAX;
            }
            paint.table_width_cache = std::mem::take(&mut prepaint.table_width_cache);
            paint.tables = std::mem::take(&mut prepaint.tables);
            for table in &mut paint.tables {
                table.viewport = Some(bounds);
            }
        }
        let occupied = super::table_chrome::occupied_content(&self.paint.borrow().rows);
        super::table_chrome::paint(
            window,
            &self.paint.borrow().tables,
            self.table_hover.as_ref(),
            self.table_drag.as_ref(),
            self.table_hot.as_ref(),
            super::table_chrome::TableChromePaint {
                surface: self.surface_bg,
                border: self.border_color,
                muted: self.muted_color,
                ink: self.text_color,
                hover_fill: self.table_hover_fill,
                active_fill: self.table_active_fill,
                pressed: self.table_pressed,
            },
            &occupied,
        );
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

/// Checkmark color that contrasts with the checked box's fill: a dark mark on
/// a light fill (dark theme) and a white one on a dark fill (light theme).
fn check_mark_color(fill: Hsla) -> Hsla {
    if fill.l > 0.55 {
        hsla(0.0, 0.0, 0.08, 1.0)
    } else {
        gpui::white()
    }
}

fn wysiwyg_line_metrics(
    kind: &LineKind,
    base_font_size: Pixels,
) -> (Pixels, Pixels, Pixels, Pixels) {
    match kind {
        // Each level is a clear step above body text at every zoom, since
        // headings no longer get an automatic bold. The gap above scales with
        // the font so sections stay separated when zoomed.
        LineKind::Heading { level: 1 } => {
            let fs = base_font_size * 2.0;
            (fs, fs * 1.4, base_font_size * 0.7, px(0.0))
        }
        LineKind::Heading { level: 2 } => {
            let fs = base_font_size * 1.6;
            (fs, fs * 1.4, base_font_size * 0.55, px(0.0))
        }
        LineKind::Heading { level: 3 } => {
            let fs = base_font_size * 1.35;
            (fs, fs * 1.45, base_font_size * 0.4, px(0.0))
        }
        LineKind::Heading { .. } => {
            let fs = base_font_size * 1.15;
            (fs, fs * 1.5, base_font_size * 0.2, px(0.0))
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
        LineKind::TableDelimiter => (
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

/// GPUI merges neighbouring runs into the first run's font whenever color,
/// underline and strikethrough match, without comparing the fonts. Bold or
/// italic text that keeps its neighbours' color (headings do) would then draw
/// in the first run's weight. Give such a run an imperceptibly different alpha
/// so the font change survives.
fn keep_font_changes_distinct(runs: &mut [TextRun]) {
    for i in 1..runs.len() {
        let (before, rest) = runs.split_at_mut(i);
        let (prev, cur) = (&before[i - 1], &mut rest[0]);
        if prev.font != cur.font
            && prev.color == cur.color
            && prev.underline == cur.underline
            && prev.strikethrough == cur.strikethrough
        {
            cur.color.a = if cur.color.a > 0.5 {
                cur.color.a - 0.004
            } else {
                cur.color.a + 0.004
            };
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn wysiwyg_slice_runs(
    parsed_line: &ParsedLine,
    display_slice: Range<usize>,
    shaped_slice_len: usize,
    sub_map: &RowOffsetMap,
    marked: Option<&Range<usize>>,
    base_font: &Font,
    mono_font: &Font,
    colors: &EditorColors,
) -> Vec<TextRun> {
    let slice_len = display_slice.end.saturating_sub(display_slice.start);
    let (default_weight, default_color, use_mono) = match &parsed_line.kind {
        LineKind::Heading { level: 1 } => (FontWeight::NORMAL, colors.h1_color, false),
        LineKind::Heading { level: 2 } => (FontWeight::NORMAL, colors.h2_color, false),
        LineKind::Heading { .. } => (FontWeight::NORMAL, colors.strong_color, false),
        LineKind::CodeLine | LineKind::CodeFence { .. } => {
            (FontWeight::NORMAL, colors.code_color, true)
        }
        LineKind::Blockquote => (FontWeight::NORMAL, colors.muted_color, false),
        LineKind::TableRow { .. } => (FontWeight::NORMAL, colors.text_color, true),
        LineKind::ListItem {
            task: Some(true), ..
        } => (FontWeight::NORMAL, colors.muted_color, false),
        _ => (FontWeight::NORMAL, colors.text_color, false),
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
                colors,
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
        runs = apply_table_pipe_style(runs, slice_str, colors.border_color);
    }

    runs.retain(|r| r.len > 0);
    keep_font_changes_distinct(&mut runs);
    runs
}

#[allow(clippy::too_many_arguments)]
fn wysiwyg_cell_runs(
    parsed_line: &ParsedLine,
    disp_len: usize,
    shaped_slice_len: usize,
    cell_map: &RowOffsetMap,
    marked: Option<&Range<usize>>,
    base_font: &Font,
    mono_font: &Font,
    colors: &EditorColors,
) -> Vec<TextRun> {
    let default_weight = FontWeight::NORMAL;
    let default_color = colors.text_color;

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
            colors,
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
    keep_font_changes_distinct(&mut runs);
    runs
}

#[allow(clippy::too_many_arguments)]
fn segment_to_run(
    seg: &StyledSegment,
    len: usize,
    line_kind: &LineKind,
    default_weight: FontWeight,
    default_color: Hsla,
    use_mono: bool,
    base_font: &Font,
    mono_font: &Font,
    colors: &EditorColors,
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
        colors.code_color
    } else if seg.link_url.is_some() {
        colors.link_color
    } else if (seg.bold || seg.italic) && default_color == colors.text_color {
        colors.strong_color
    } else {
        default_color
    };

    let background_color = if seg.code {
        Some(colors.code_bg)
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
    font: Font,
    color: Hsla,
) -> Vec<TextRun> {
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

/// Find hits that can paint on a row spanning `row_start..=row_end`, with
/// their index in `hits`. Hits are sorted and disjoint, so a binary search
/// skips the rest instead of testing every match on every row.
fn hits_near(
    hits: &[Range<usize>],
    row_start: usize,
    row_end: usize,
) -> impl Iterator<Item = (usize, &Range<usize>)> {
    let first = hits.partition_point(|hit| hit.end <= row_start);
    hits[first..]
        .iter()
        .take_while(move |hit| hit.start <= row_end)
        .enumerate()
        .map(move |(i, hit)| (first + i, hit))
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

#[cfg(test)]
mod tests {
    use super::{
        compute_table_column_widths, editor_end_visual, editor_home_visual, index_in_rows,
        move_vertical_visual, wrap_cell_text, CellLine, EditorPaint, PaintCell, PaintRow,
        RowOffsetMap, TableWidthCache,
    };
    use crate::markdown::{parse_document, ParsedLine};
    use crate::model::document::ViewMode;
    use crate::model::text_ops::Caret;
    use gpui::{point, px, size, Bounds, Pixels, ShapedLine};

    /// Proportional stand-in for the text system: narrow glyphs are cheaper than wide ones.
    fn fake_measure(content: &str) -> impl FnMut(&ParsedLine, &std::ops::Range<usize>) -> Pixels + '_ {
        let content = content.to_string();
        move |_line, range| {
            let text = content.get(range.clone()).unwrap_or("").trim();
            px(text
                .chars()
                .map(|c| match c {
                    'i' | 'l' | '1' | '.' | ' ' => 4.0,
                    c if super::str_display_width(&c.to_string()) == 2 => 16.0,
                    _ => 8.0,
                })
                .sum::<f32>())
        }
    }

    #[test]
    fn test_column_sizing_short_stays_narrow_long_shares_remaining() {
        let md = "| ID | Status | Description |\n| --- | --- | --- |\n| 1 | OK | A very long description text that needs much more space than the short ID or Status columns |";
        let doc = parse_document(md);
        let table_lines: Vec<&ParsedLine> = doc.lines.iter().collect();

        let widths = compute_table_column_widths(&table_lines, px(600.0), &mut fake_measure(md));
        assert_eq!(widths.len(), 3);

        // ID and Status columns should stay narrow (near minimum column width)
        assert!(
            widths[0] < px(80.0),
            "ID column should stay narrow, got {:?}",
            widths[0]
        );
        assert!(
            widths[1] < px(100.0),
            "Status column should stay narrow, got {:?}",
            widths[1]
        );

        // Description column should take the lion's share of the remaining width
        assert!(
            widths[2] > px(400.0),
            "Description column should take remaining width, got {:?}",
            widths[2]
        );

        // Total width matches target table width
        let total: Pixels = widths.iter().copied().fold(px(0.0), |acc, w| acc + w);
        assert!((total - px(600.0)).abs() < px(1.0));
    }

    #[test]
    fn test_column_sizing_cjk_emoji_and_empty() {
        let md = "| 姓名 | 状态 | 备注 | 空 |\n| --- | --- | --- | --- |\n| 张三 | 🚀🎉 | 中文内容与English | |";
        let doc = parse_document(md);
        let table_lines: Vec<&ParsedLine> = doc.lines.iter().collect();

        let widths = compute_table_column_widths(&table_lines, px(400.0), &mut fake_measure(md));
        assert_eq!(widths.len(), 4);

        // All columns should be at least minimum width (48px)
        for (i, w) in widths.iter().enumerate() {
            assert!(
                *w >= px(48.0),
                "Column {i} width {:?} is below min width",
                w
            );
        }

        // CJK/English column (备注) exceeds its 100px share and grows larger than short columns
        assert!(widths[2] > widths[0]);
        assert!(widths[0] >= widths[3]); // empty cell column is at minimum or floor
    }

    #[test]
    fn test_multi_line_cell_vertical_navigation() {
        // Construct a mock EditorPaint with a 2-line wrapped cell
        let content = "| Hello world foo | B |\n| Second row | C |";
        let line0_map = RowOffsetMap::identity(2, 10);
        let line1_map = RowOffsetMap::identity(10, 17);

        let cell_0 = PaintCell {
            raw_range: 2..17,
            inner_range: 2..17,
            box_bounds: Bounds::new(point(px(10.0), px(0.0)), size(px(100.0), px(56.0))),
            text_origin: point(px(12.0), px(0.0)),
            shaped: ShapedLine::default(),
            offset_map: line0_map.clone(),
            lines: vec![
                CellLine {
                    text_origin: point(px(12.0), px(0.0)),
                    line_height: px(28.0),
                    shaped: ShapedLine::default(),
                    offset_map: line0_map,
                },
                CellLine {
                    text_origin: point(px(12.0), px(28.0)),
                    line_height: px(28.0),
                    shaped: ShapedLine::default(),
                    offset_map: line1_map,
                },
            ],
        };

        let row_0 = PaintRow {
            line_index: 0,
            byte_start: 0,
            byte_end: 22,
            origin: point(px(10.0), px(0.0)),
            line_height: px(56.0),
            shaped: ShapedLine::default(),
            offset_map: RowOffsetMap::identity(0, 22),
            checkbox_hit: None,
            link_hits: Vec::new(),
            cells: vec![cell_0],
        };

        let next_cell = PaintCell {
            raw_range: 25..36,
            inner_range: 25..36,
            box_bounds: Bounds::new(point(px(10.0), px(56.0)), size(px(100.0), px(28.0))),
            text_origin: point(px(12.0), px(56.0)),
            shaped: ShapedLine::default(),
            offset_map: RowOffsetMap::identity(25, 36),
            lines: vec![CellLine {
                text_origin: point(px(12.0), px(56.0)),
                line_height: px(28.0),
                shaped: ShapedLine::default(),
                offset_map: RowOffsetMap::identity(25, 36),
            }],
        };

        let row_1 = PaintRow {
            line_index: 1,
            byte_start: 23,
            byte_end: 42,
            origin: point(px(10.0), px(56.0)),
            line_height: px(28.0),
            shaped: ShapedLine::default(),
            offset_map: RowOffsetMap::identity(23, 42),
            checkbox_hit: None,
            link_hits: Vec::new(),
            cells: vec![next_cell],
        };

        let paint = EditorPaint {
            wrap_cols: 80,
            line_height: px(28.0),
            total_height: px(84.0),
            rows: vec![row_0, row_1],
            bounds: Some(Bounds::new(point(px(0.0), px(0.0)), size(px(400.0), px(100.0)))),
            table_width_cache: TableWidthCache::default(),
            tables: Vec::new(),
        };

        let mut caret = Caret::default();
        caret.move_to(5); // In Line 0 of Cell 0

        // Move Down: should move to Line 1 of the same cell (range 10..17)
        let moved = move_vertical_visual(&paint, content, &mut caret, 1, false, ViewMode::Wysiwyg);
        assert!(moved);
        assert!(
            (10..=17).contains(&caret.cursor()),
            "Moving down should land on Line 1 of same cell, got {}",
            caret.cursor()
        );

        // Move Down again: should move to Row 1 (range 25..36)
        let moved = move_vertical_visual(&paint, content, &mut caret, 1, false, ViewMode::Wysiwyg);
        assert!(moved);
        assert!(
            (25..=36).contains(&caret.cursor()),
            "Moving down should land on Row 1, got {}",
            caret.cursor()
        );

        // Move Up: should move back to Line 1 of Cell 0 (last line of that cell)
        let moved = move_vertical_visual(&paint, content, &mut caret, -1, false, ViewMode::Wysiwyg);
        assert!(moved);
        assert!(
            (10..=17).contains(&caret.cursor()),
            "Moving up should land on Line 1 of Cell 0, got {}",
            caret.cursor()
        );

        // Move Up again: should move to Line 0 of Cell 0
        let moved = move_vertical_visual(&paint, content, &mut caret, -1, false, ViewMode::Wysiwyg);
        assert!(moved);
        assert!(
            (2..=10).contains(&caret.cursor()),
            "Moving up should land on Line 0 of Cell 0, got {}",
            caret.cursor()
        );
    }

    #[test]
    fn test_multi_line_cell_click_hit_testing() {
        let line0_map = RowOffsetMap::identity(2, 10);
        let line1_map = RowOffsetMap::identity(10, 17);

        let cell = PaintCell {
            raw_range: 2..17,
            inner_range: 2..17,
            box_bounds: Bounds::new(point(px(10.0), px(0.0)), size(px(100.0), px(56.0))),
            text_origin: point(px(12.0), px(0.0)),
            shaped: ShapedLine::default(),
            offset_map: line0_map.clone(),
            lines: vec![
                CellLine {
                    text_origin: point(px(12.0), px(0.0)),
                    line_height: px(28.0),
                    shaped: ShapedLine::default(),
                    offset_map: line0_map,
                },
                CellLine {
                    text_origin: point(px(12.0), px(28.0)),
                    line_height: px(28.0),
                    shaped: ShapedLine::default(),
                    offset_map: line1_map,
                },
            ],
        };

        let row = PaintRow {
            line_index: 0,
            byte_start: 0,
            byte_end: 20,
            origin: point(px(10.0), px(0.0)),
            line_height: px(56.0),
            shaped: ShapedLine::default(),
            offset_map: RowOffsetMap::identity(0, 20),
            checkbox_hit: None,
            link_hits: Vec::new(),
            cells: vec![cell],
        };

        let paint = EditorPaint {
            wrap_cols: 80,
            line_height: px(28.0),
            total_height: px(56.0),
            rows: vec![row],
            bounds: Some(Bounds::new(point(px(0.0), px(0.0)), size(px(200.0), px(100.0)))),
            table_width_cache: TableWidthCache::default(),
            tables: Vec::new(),
        };

        // Click on Line 0 (y = 10px)
        let offset_line0 = index_in_rows(&paint, point(px(30.0), px(10.0)));
        assert!(
            offset_line0 <= 10,
            "Click on line 0 should yield line 0 offset, got {offset_line0}"
        );

        // Click on Line 1 (y = 35px)
        let offset_line1 = index_in_rows(&paint, point(px(30.0), px(35.0)));
        assert!(
            (10..=17).contains(&offset_line1),
            "Click on line 1 should yield line 1 offset, got {offset_line1}"
        );
    }

    #[test]
    fn test_multi_line_cell_home_and_end_visual() {
        let line0_map = RowOffsetMap::identity(2, 10);
        let line1_map = RowOffsetMap::identity(10, 17);

        let cell = PaintCell {
            raw_range: 2..17,
            inner_range: 2..17,
            box_bounds: Bounds::new(point(px(10.0), px(0.0)), size(px(100.0), px(56.0))),
            text_origin: point(px(12.0), px(0.0)),
            shaped: ShapedLine::default(),
            offset_map: line0_map.clone(),
            lines: vec![
                CellLine {
                    text_origin: point(px(12.0), px(0.0)),
                    line_height: px(28.0),
                    shaped: ShapedLine::default(),
                    offset_map: line0_map,
                },
                CellLine {
                    text_origin: point(px(12.0), px(28.0)),
                    line_height: px(28.0),
                    shaped: ShapedLine::default(),
                    offset_map: line1_map,
                },
            ],
        };

        let row = PaintRow {
            line_index: 0,
            byte_start: 0,
            byte_end: 20,
            origin: point(px(10.0), px(0.0)),
            line_height: px(56.0),
            shaped: ShapedLine::default(),
            offset_map: RowOffsetMap::identity(0, 20),
            checkbox_hit: None,
            link_hits: Vec::new(),
            cells: vec![cell],
        };

        let paint = EditorPaint {
            wrap_cols: 80,
            line_height: px(28.0),
            total_height: px(56.0),
            rows: vec![row],
            bounds: Some(Bounds::new(point(px(0.0), px(0.0)), size(px(200.0), px(100.0)))),
            table_width_cache: TableWidthCache::default(),
            tables: Vec::new(),
        };

        let mut caret = Caret::default();
        caret.move_to(14); // Middle of line 1

        let handled_home = editor_home_visual(&paint, &mut caret, false);
        assert!(handled_home);
        assert_eq!(caret.cursor(), 10, "Home should move to start of line 1");

        let handled_end = editor_end_visual(&paint, &mut caret, false);
        assert!(handled_end);
        assert_eq!(caret.cursor(), 17, "End should move to end of line 1");
    }

    #[test]
    fn test_wrap_cell_text_empty_and_fits() {
        let shaped = ShapedLine::default();
        let empty_wrap = wrap_cell_text("", px(100.0), &shaped);
        assert_eq!(empty_wrap, vec![(0, 0)]);

        let fits_wrap = wrap_cell_text("Hello world", px(100.0), &shaped);
        assert_eq!(fits_wrap, vec![(0, 11)]);
    }

    #[test]
    fn test_click_wrapped_cell_in_taller_row() {
        // Cell 0 has 4 lines (row height = 112px)
        let cell0 = PaintCell {
            raw_range: 0..20,
            inner_range: 0..20,
            box_bounds: Bounds::new(point(px(0.0), px(0.0)), size(px(100.0), px(112.0))),
            text_origin: point(px(0.0), px(0.0)),
            shaped: ShapedLine::default(),
            offset_map: RowOffsetMap::identity(0, 20),
            lines: (0..4)
                .map(|i| CellLine {
                    text_origin: point(px(0.0), px(28.0) * i as f32),
                    line_height: px(28.0),
                    shaped: ShapedLine::default(),
                    offset_map: RowOffsetMap::identity(i * 5, (i + 1) * 5),
                })
                .collect(),
        };

        // Cell 1 has 2 lines (Line 0: 20..27, Line 1: 28..35)
        let line0_map = RowOffsetMap::identity(20, 27);
        let line1_map = RowOffsetMap::identity(28, 35);
        let cell1 = PaintCell {
            raw_range: 20..35,
            inner_range: 20..35,
            box_bounds: Bounds::new(point(px(100.0), px(0.0)), size(px(100.0), px(112.0))),
            text_origin: point(px(100.0), px(0.0)),
            shaped: ShapedLine::default(),
            offset_map: line0_map.clone(),
            lines: vec![
                CellLine {
                    text_origin: point(px(100.0), px(0.0)),
                    line_height: px(28.0),
                    shaped: ShapedLine::default(),
                    offset_map: line0_map,
                },
                CellLine {
                    text_origin: point(px(100.0), px(28.0)),
                    line_height: px(28.0),
                    shaped: ShapedLine::default(),
                    offset_map: line1_map,
                },
            ],
        };

        let row = PaintRow {
            line_index: 0,
            byte_start: 0,
            byte_end: 35,
            origin: point(px(0.0), px(0.0)),
            line_height: px(112.0),
            shaped: ShapedLine::default(),
            offset_map: RowOffsetMap::identity(0, 35),
            checkbox_hit: None,
            link_hits: Vec::new(),
            cells: vec![cell0, cell1],
        };

        let paint = EditorPaint {
            wrap_cols: 80,
            line_height: px(28.0),
            total_height: px(112.0),
            rows: vec![row],
            bounds: Some(Bounds::new(point(px(0.0), px(0.0)), size(px(200.0), px(112.0)))),
            table_width_cache: TableWidthCache::default(),
            tables: Vec::new(),
        };

        // Click in Cell 1 on Line 1 at y = 35px (Line 0 is 0..28px, Line 1 is 28..56px)
        let offset = index_in_rows(&paint, point(px(150.0), px(35.0)));
        assert!(
            (28..=35).contains(&offset),
            "Click on line 1 (y=35px) in a 112px tall row should map to Line 1 (28..35), got {offset}"
        );

        // Click in Cell 1 at y = 80px (empty area below Line 1 in the 112px row)
        let offset_bottom = index_in_rows(&paint, point(px(150.0), px(80.0)));
        assert!(
            (28..=35).contains(&offset_bottom),
            "Click below line 1 (y=80px) in a 112px row should clamp to Line 1 (28..35), got {offset_bottom}"
        );
    }

    #[test]
    fn test_wide_table_many_columns_shrinks_minimum_and_stays_within_editor() {
        let mut header = String::from("|");
        let mut delim = String::from("|");
        let mut row = String::from("|");
        for i in 0..25 {
            header.push_str(&format!(" c{i} |"));
            delim.push_str("---|");
            row.push_str(&format!(" val{i} |"));
        }
        let md = format!("{header}\n{delim}\n{row}");
        let doc = parse_document(&md);
        let table_lines: Vec<&ParsedLine> = doc.lines.iter().collect();

        let target_w = px(500.0);
        let widths = compute_table_column_widths(&table_lines, target_w, &mut fake_measure(&md));
        assert_eq!(widths.len(), 25);

        // Sum of widths must not exceed editor width
        let total: Pixels = widths.iter().copied().fold(px(0.0), |acc, w| acc + w);
        assert!(
            (total - target_w).abs() < px(1.0),
            "Total table width {:?} should match editor width {:?}",
            total,
            target_w
        );

        // Minimum column width was shrunk from 48px to 20px so all 25 columns fit
        for (i, w) in widths.iter().enumerate() {
            assert!(
                *w <= px(30.0),
                "Column {i} width {:?} should be shrunk to fit within 500px",
                w
            );
        }
    }

    #[test]
    fn test_short_text_in_one_cell_does_not_move_borders() {
        let md_empty = "| | | |\n|---|---|---|\n| | | |";
        let doc_empty = parse_document(md_empty);
        let table_lines_empty: Vec<&ParsedLine> = doc_empty.lines.iter().collect();

        let target_w = px(600.0);
        let widths_empty = compute_table_column_widths(&table_lines_empty, target_w, &mut fake_measure(md_empty));
        assert_eq!(widths_empty.len(), 3);

        // Every column starts at an equal share (200px each)
        for (i, w) in widths_empty.iter().enumerate() {
            assert!(
                (*w - px(200.0)).abs() < px(1.0),
                "Empty table col {i} should be 200px, got {:?}",
                w
            );
        }

        // Typing short text ("Alice") in one cell fits within the 200px share,
        // so it does NOT move any column borders.
        let md_typed = "| Name | | |\n|---|---|---|\n| Alice | | |";
        let doc_typed = parse_document(md_typed);
        let table_lines_typed: Vec<&ParsedLine> = doc_typed.lines.iter().collect();

        let widths_typed = compute_table_column_widths(&table_lines_typed, target_w, &mut fake_measure(md_typed));
        assert_eq!(widths_typed.len(), 3);

        // Columns must still be exactly 200px each
        for (i, w) in widths_typed.iter().enumerate() {
            assert!(
                (*w - px(200.0)).abs() < px(1.0),
                "Short text in col 0 shouldn't move borders; col {i} should stay 200px, got {:?}",
                w
            );
        }
    }

    #[test]
    fn test_long_cell_grows_column_and_wraps_at_limit() {
        // Column 0 has very long text that cannot fit in the 200px share.
        // Columns 1 and 2 are empty.
        let long_text = "A".repeat(200);
        let md = format!("| {long_text} | | |\n|---|---|---|\n| val | | |");
        let doc = parse_document(&md);
        let table_lines: Vec<&ParsedLine> = doc.lines.iter().collect();

        let target_w = px(600.0);
        let widths = compute_table_column_widths(&table_lines, target_w, &mut fake_measure(&md));
        assert_eq!(widths.len(), 3);

        // Columns 1 and 2 gave up their spare room down to the minimum (48px)
        assert!(
            (widths[1] - px(48.0)).abs() < px(1.0),
            "Column 1 should shrink to min width (48px), got {:?}",
            widths[1]
        );
        assert!(
            (widths[2] - px(48.0)).abs() < px(1.0),
            "Column 2 should shrink to min width (48px), got {:?}",
            widths[2]
        );

        // Column 0 grew to take all remaining width (600 - 48 - 48 = 504px),
        // hitting the limit and staying within the editor width while wrapping.
        assert!(
            (widths[0] - px(504.0)).abs() < px(1.0),
            "Column 0 should grow to 504px, got {:?}",
            widths[0]
        );

        let total: Pixels = widths.iter().copied().fold(px(0.0), |acc, w| acc + w);
        assert!((total - target_w).abs() < px(1.0));
    }

    #[test]
    fn test_many_columns_still_fit() {
        // 30 columns on 600px width
        let mut header = String::from("|");
        let mut delim = String::from("|");
        let mut row = String::from("|");
        for i in 0..30 {
            header.push_str(&format!(" c{i} |"));
            delim.push_str("---|");
            row.push_str(&format!(" v{i} |"));
        }
        let md = format!("{header}\n{delim}\n{row}");
        let doc = parse_document(&md);
        let table_lines: Vec<&ParsedLine> = doc.lines.iter().collect();

        let target_w = px(600.0);
        let widths = compute_table_column_widths(&table_lines, target_w, &mut fake_measure(&md));
        assert_eq!(widths.len(), 30);

        // Each column gets equal share (600 / 30 = 20px)
        for (i, w) in widths.iter().enumerate() {
            assert!(
                (*w - px(20.0)).abs() < px(1.0),
                "Col {i} width should be 20px, got {:?}",
                w
            );
        }

        let total: Pixels = widths.iter().copied().fold(px(0.0), |acc, w| acc + w);
        assert!((total - target_w).abs() < px(1.0));
    }

    #[test]
    fn test_empty_lines_cell_fallbacks_do_not_panic() {
        let empty_cell = PaintCell {
            raw_range: 0..10,
            inner_range: 0..10,
            box_bounds: Bounds::new(point(px(0.0), px(0.0)), size(px(100.0), px(28.0))),
            text_origin: point(px(0.0), px(0.0)),
            shaped: ShapedLine::default(),
            offset_map: RowOffsetMap::identity(0, 10),
            lines: Vec::new(),
        };

        let row = PaintRow {
            line_index: 0,
            byte_start: 0,
            byte_end: 10,
            origin: point(px(0.0), px(0.0)),
            line_height: px(28.0),
            shaped: ShapedLine::default(),
            offset_map: RowOffsetMap::identity(0, 10),
            checkbox_hit: None,
            link_hits: Vec::new(),
            cells: vec![empty_cell],
        };

        let paint = EditorPaint {
            wrap_cols: 80,
            line_height: px(28.0),
            total_height: px(28.0),
            rows: vec![row],
            bounds: Some(Bounds::new(point(px(0.0), px(0.0)), size(px(100.0), px(28.0)))),
            table_width_cache: TableWidthCache::default(),
            tables: Vec::new(),
        };

        let mut caret = Caret::default();
        caret.move_to(5);

        // None of these should panic on empty cell.lines
        let _ = index_in_rows(&paint, point(px(10.0), px(10.0)));
        let _ = editor_home_visual(&paint, &mut caret, false);
        let _ = editor_end_visual(&paint, &mut caret, false);
        let _ = move_vertical_visual(&paint, "0123456789", &mut caret, 1, false, ViewMode::Wysiwyg);
    }

    #[test]
    fn test_fitted_column_has_equal_gap_before_and_after_text() {
        // Column 1 outgrows its equal share; the others have spare room to donate.
        let md = "| a | A fairly long cell with some words | c |\n| --- | --- | --- |\n| x | y | z |";
        let doc = parse_document(md);
        let table_lines: Vec<&ParsedLine> = doc.lines.iter().collect();
        let target_w = px(500.0);
        let widths = compute_table_column_widths(&table_lines, target_w, &mut fake_measure(md));

        let mut measure = fake_measure(md);
        let mut widest = px(0.0);
        for line in &table_lines {
            if let crate::markdown::LineKind::TableRow { cells, .. } = &line.kind {
                widest = widest.max(measure(line, &cells[1]));
            }
        }
        let equal_share = target_w / 3.0;
        assert!(widths[1] > equal_share, "column must have grown past its share");

        let left_gap = super::TABLE_CELL_PAD_X;
        let right_gap = widths[1] - super::TABLE_CELL_PAD_X - widest;
        assert!(
            (f32::from(left_gap) - f32::from(right_gap)).abs() <= 1.0,
            "left gap {:?} and right gap {:?} must match",
            left_gap,
            right_gap
        );
        let total: Pixels = widths.iter().copied().fold(px(0.0), |a, b| a + b);
        assert!(total <= target_w + px(0.01));
    }

    #[test]
    fn test_wysiwyg_slice_runs_standalone_italic_and_combos() {
        use gpui::{font, hsla, FontStyle, FontWeight};
        let colors = super::EditorColors {
            text_color: hsla(0.0, 0.0, 0.8, 1.0),
            strong_color: hsla(0.0, 0.0, 1.0, 1.0),
            muted_color: hsla(0.0, 0.0, 0.5, 1.0),
            h1_color: hsla(0.1, 0.8, 0.5, 1.0),
            h2_color: hsla(0.2, 0.8, 0.5, 1.0),
            code_color: hsla(0.3, 0.8, 0.5, 1.0),
            code_bg: hsla(0.4, 0.8, 0.5, 1.0),
            link_color: hsla(0.5, 0.8, 0.5, 1.0),
            border_color: hsla(0.6, 0.8, 0.5, 1.0),
        };
        let base_font = font("Ubuntu");
        let mono_font = font("Ubuntu Mono");

        // 1. Standalone italic
        let doc = parse_document("Normal *italic* text");
        let line = &doc.lines[0];
        let runs = super::wysiwyg_slice_runs(
            line,
            0..line.display_text.len(),
            line.display_text.len(),
            &line.offset_map,
            None,
            &base_font,
            &mono_font,
            &colors,
        );
        // Expect 3 runs: "Normal ", "italic", " text"
        assert_eq!(runs.len(), 3);
        assert_eq!(runs[0].color, colors.text_color);
        assert_eq!(runs[0].font.style, FontStyle::Normal);
        assert_eq!(runs[0].font.weight, FontWeight::NORMAL);

        // Standalone italic must have FontStyle::Italic and strong_color for high contrast
        assert_eq!(runs[1].font.style, FontStyle::Italic);
        assert_eq!(runs[1].font.weight, FontWeight::NORMAL);
        assert_eq!(runs[1].color, colors.strong_color);

        assert_eq!(runs[2].color, colors.text_color);
        assert_eq!(runs[2].font.style, FontStyle::Normal);

        // 2. Bold
        let doc_bold = parse_document("Normal **bold** text");
        let line_bold = &doc_bold.lines[0];
        let runs_bold = super::wysiwyg_slice_runs(
            line_bold,
            0..line_bold.display_text.len(),
            line_bold.display_text.len(),
            &line_bold.offset_map,
            None,
            &base_font,
            &mono_font,
            &colors,
        );
        assert_eq!(runs_bold.len(), 3);
        assert_eq!(runs_bold[1].font.style, FontStyle::Normal);
        assert_eq!(runs_bold[1].font.weight, FontWeight::BOLD);
        assert_eq!(runs_bold[1].color, colors.strong_color);

        // 3. Bold + Italic
        let doc_bi = parse_document("Normal ***bold italic*** text");
        let line_bi = &doc_bi.lines[0];
        let runs_bi = super::wysiwyg_slice_runs(
            line_bi,
            0..line_bi.display_text.len(),
            line_bi.display_text.len(),
            &line_bi.offset_map,
            None,
            &base_font,
            &mono_font,
            &colors,
        );
        assert_eq!(runs_bi.len(), 3);
        assert_eq!(runs_bi[1].font.style, FontStyle::Italic);
        assert_eq!(runs_bi[1].font.weight, FontWeight::BOLD);
        assert_eq!(runs_bi[1].color, colors.strong_color);

        // 4. Blockquote italic maintains blockquote's muted color
        let doc_bq = parse_document("> A *quote with italic* text");
        let line_bq = &doc_bq.lines[0];
        let runs_bq = super::wysiwyg_slice_runs(
            line_bq,
            0..line_bq.display_text.len(),
            line_bq.display_text.len(),
            &line_bq.offset_map,
            None,
            &base_font,
            &mono_font,
            &colors,
        );
        for run in &runs_bq {
            assert_eq!(run.color, colors.muted_color);
        }
    }
    #[test]
    fn headings_are_not_bold_unless_the_user_adds_it() {
        use gpui::{font, hsla, FontStyle, FontWeight};
        let colors = super::EditorColors {
            text_color: hsla(0.0, 0.0, 0.8, 1.0),
            strong_color: hsla(0.0, 0.0, 1.0, 1.0),
            muted_color: hsla(0.0, 0.0, 0.5, 1.0),
            h1_color: hsla(0.1, 0.8, 0.5, 1.0),
            h2_color: hsla(0.2, 0.8, 0.5, 1.0),
            code_color: hsla(0.3, 0.8, 0.5, 1.0),
            code_bg: hsla(0.4, 0.8, 0.5, 1.0),
            link_color: hsla(0.5, 0.8, 0.5, 1.0),
            border_color: hsla(0.6, 0.8, 0.5, 1.0),
        };
        let (base, mono) = (font("Ubuntu"), font("Ubuntu Mono"));
        let runs_for = |src: &str| {
            let doc = parse_document(src);
            let line = &doc.lines[0];
            super::wysiwyg_slice_runs(
                line,
                0..line.display_text.len(),
                line.display_text.len(),
                &line.offset_map,
                None,
                &base,
                &mono,
                &colors,
            )
        };

        let plain = runs_for("# Hello world");
        assert!(plain.iter().all(|r| r.font.weight == FontWeight::NORMAL));

        let mixed = runs_for("# Hello **world** and *more*");
        assert_eq!(mixed[0].font.weight, FontWeight::NORMAL);
        assert_eq!(mixed[1].font.weight, FontWeight::BOLD);
        assert_eq!(mixed[2].font.weight, FontWeight::NORMAL);
        assert_eq!(mixed[3].font.style, FontStyle::Italic);

        let all_bold = runs_for("## **Hello world**");
        assert!(all_bold.iter().all(|r| r.font.weight == FontWeight::BOLD));
    }
    /// GPUI merges neighbouring runs into the first one's font when color,
    /// underline and strikethrough match. Any two neighbours with different
    /// fonts must therefore differ in one of them, or the whole line draws in
    /// the first run's weight.
    #[test]
    fn neighbouring_runs_with_different_fonts_are_never_mergeable() {
        use gpui::{font, hsla};
        let colors = super::EditorColors {
            text_color: hsla(0.0, 0.0, 0.8, 1.0),
            strong_color: hsla(0.0, 0.0, 1.0, 1.0),
            muted_color: hsla(0.0, 0.0, 0.5, 1.0),
            h1_color: hsla(0.1, 0.8, 0.5, 1.0),
            h2_color: hsla(0.2, 0.8, 0.5, 1.0),
            code_color: hsla(0.3, 0.8, 0.5, 1.0),
            code_bg: hsla(0.4, 0.8, 0.5, 1.0),
            link_color: hsla(0.5, 0.8, 0.5, 1.0),
            border_color: hsla(0.6, 0.8, 0.5, 1.0),
        };
        let (base, mono) = (font("Ubuntu"), font("Ubuntu Mono"));
        let sources = [
            "# **Welcome** to LiteMark",
            "# Welcome to **LiteMark**",
            "# Welcome **to** LiteMark",
            "# *Welcome* to LiteMark",
            "# **Welcome** to *LiteMark*",
            "## **Welcome** to LiteMark",
            "### **Welcome** to LiteMark",
            "#### **Welcome** to LiteMark",
            "**Welcome** to LiteMark",
            "> **Welcome** to LiteMark",
            "- **Welcome** to LiteMark",
        ];
        for src in sources {
            let doc = parse_document(src);
            let line = &doc.lines[0];
            let runs = super::wysiwyg_slice_runs(
                line,
                0..line.display_text.len(),
                line.display_text.len(),
                &line.offset_map,
                None,
                &base,
                &mono,
                &colors,
            );
            assert!(runs.len() > 1, "{src}: expected mixed runs");
            for pair in runs.windows(2) {
                if pair[0].font != pair[1].font {
                    assert!(
                        pair[0].color != pair[1].color
                            || pair[0].underline != pair[1].underline
                            || pair[0].strikethrough != pair[1].strikethrough,
                        "{src}: GPUI would merge runs {:?} and {:?}",
                        pair[0].font.weight,
                        pair[1].font.weight
                    );
                }
            }
        }
    }
    #[test]
    fn heading_sizes_step_clearly_above_body_text_at_every_zoom() {
        use crate::markdown::LineKind;
        for base in [14.0_f32, 16.0, 24.0] {
            let size = |level: u8| {
                super::wysiwyg_line_metrics(&LineKind::Heading { level }, px(base)).0
            };
            let gap = |level: u8| {
                super::wysiwyg_line_metrics(&LineKind::Heading { level }, px(base)).2
            };
            // Strictly decreasing, and each at least 10% above the next level.
            for level in 1..4u8 {
                assert!(size(level) >= size(level + 1) * 1.1, "base {base} h{level}");
                assert!(gap(level) > gap(level + 1), "base {base} h{level} gap");
            }
            // H3 stays at least 30% above body text.
            assert!(size(3) >= px(base) * 1.3, "base {base} h3 vs body");
            assert!(size(4) > px(base), "base {base} h4 vs body");
        }
    }
    #[test]
    fn checkmark_contrasts_with_the_checked_box_fill() {
        use gpui::hsla;
        let on_light_fill = super::check_mark_color(hsla(0.0, 0.0, 0.9, 1.0));
        let on_dark_fill = super::check_mark_color(hsla(0.0, 0.0, 0.07, 1.0));
        assert!(on_light_fill.l < 0.2, "dark mark on the dark theme's light fill");
        assert!(on_dark_fill.l > 0.9, "white mark on the light theme's dark fill");
    }

    #[test]
    fn hits_near_matches_a_scan_of_every_hit() {
        let content = "aa a\n\naaa ba\na";
        let hits = crate::model::text_ops::find_matches(content, "a", true);
        for row_start in 0..=content.len() {
            for row_end in row_start..=content.len() {
                let near: Vec<usize> = super::hits_near(&hits, row_start, row_end)
                    .map(|(i, hit)| {
                        assert_eq!(&hits[i], hit);
                        i
                    })
                    .collect();
                let scanned: Vec<usize> = hits
                    .iter()
                    .enumerate()
                    .filter(|(_, h)| h.end > row_start && h.start <= row_end)
                    .map(|(i, _)| i)
                    .collect();
                assert_eq!(near, scanned, "row {row_start}..={row_end}");
            }
        }
    }
}
