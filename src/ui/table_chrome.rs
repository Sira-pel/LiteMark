//! Hover chrome for a rendered table.
//!
//! A column pill sits on the top edge of the hovered header cell: a 6-dot
//! grip, an alignment mark, and a plus. A 6-dot grip sits on the table's
//! top-right corner. The hovered row gets the same kind of control on its
//! left edge, without an alignment mark. Bounds are derived from the painted
//! cell boxes so a click hits the same rect the frame drew.

use gpui::{point, Bounds, Pixels, Point, Size};

use crate::markdown::TableColumnAlign;
use crate::theme::sz;

use super::text_editor::{PaintCell, TableGeom};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TableDragKind {
    Row,
    Column,
}

#[derive(Clone, Debug)]
pub struct TableDrag {
    pub source: std::ops::Range<usize>,
    pub kind: TableDragKind,
    pub from: usize,
    pub anchor: usize,
    pub gap: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableHitKind {
    ColGrip,
    ColAlign,
    ColPlus,
    RowGrip,
    RowPlus,
    Corner,
}

#[derive(Clone, Debug)]
pub struct TableHit {
    pub source: std::ops::Range<usize>,
    pub kind: TableHitKind,
    pub index: usize,
    pub anchor: usize,
    pub align: TableColumnAlign,
}

pub fn next_align(align: TableColumnAlign) -> TableColumnAlign {
    match align {
        TableColumnAlign::None => TableColumnAlign::Left,
        TableColumnAlign::Left => TableColumnAlign::Center,
        TableColumnAlign::Center => TableColumnAlign::Right,
        TableColumnAlign::Right => TableColumnAlign::None,
    }
}

/// Final index after lifting `from` and inserting at the gap.
///
/// A gap is the boundary before column/row `gap`, or `len` to mean after the
/// last one. Dropping on either side of the dragged item lands back on `from`.
pub fn final_index(from: usize, gap: usize) -> usize {
    if gap > from {
        gap - 1
    } else {
        gap
    }
}

pub fn record_table_row(
    tables: &mut Vec<TableGeom>,
    source: std::ops::Range<usize>,
    is_header: bool,
    alignments: &[TableColumnAlign],
    row_bounds: Bounds<Pixels>,
    cells: &[PaintCell],
) {
    let anchors: Vec<usize> = cells.iter().map(|cell| cell.raw_range.start).collect();
    let same = tables
        .last()
        .is_some_and(|table| table.source == source && !is_header);
    if !same {
        let columns = cells
            .iter()
            .enumerate()
            .map(|(index, cell)| super::text_editor::TableColGeom {
                header_bounds: cell.box_bounds,
                align: alignments
                    .get(index)
                    .copied()
                    .unwrap_or(TableColumnAlign::None),
                anchor: cell.raw_range.start,
            })
            .collect();
        tables.push(TableGeom {
            source,
            bounds: row_bounds,
            columns,
            rows: vec![super::text_editor::TableRowGeom {
                index: 0,
                bounds: row_bounds,
                anchors,
            }],
        });
        return;
    }
    let table = tables.last_mut().expect("same table");
    let index = table.rows.len();
    table.bounds = union_bounds(table.bounds, row_bounds);
    table.rows.push(super::text_editor::TableRowGeom {
        index,
        bounds: row_bounds,
        anchors,
    });
}

fn union_bounds(a: Bounds<Pixels>, b: Bounds<Pixels>) -> Bounds<Pixels> {
    let left = a.left().min(b.left());
    let top = a.top().min(b.top());
    let right = a.right().max(b.right());
    let bottom = a.bottom().max(b.bottom());
    Bounds::new(point(left, top), size(right - left, bottom - top))
}

fn size(width: Pixels, height: Pixels) -> Size<Pixels> {
    Size { width, height }
}

pub fn contains(bounds: Bounds<Pixels>, point: Point<Pixels>) -> bool {
    point.x >= bounds.left()
        && point.x < bounds.right()
        && point.y >= bounds.top()
        && point.y < bounds.bottom()
}

pub fn column_pill_bounds(header: Bounds<Pixels>) -> Bounds<Pixels> {
    let width = sz::TABLE_PILL * 3.0;
    let height = sz::TABLE_PILL;
    let center_x = header.left() + header.size.width * 0.5;
    Bounds::new(
        point(center_x - width * 0.5, header.top() - height * 0.5),
        size(width, height),
    )
}

pub fn row_pill_bounds(row: Bounds<Pixels>) -> Bounds<Pixels> {
    let width = sz::TABLE_PILL * 2.0;
    let height = sz::TABLE_PILL;
    let center_y = row.top() + row.size.height * 0.5;
    Bounds::new(
        point(row.left() - width * 0.5, center_y - height * 0.5),
        size(width, height),
    )
}

pub fn corner_grip_bounds(table: Bounds<Pixels>) -> Bounds<Pixels> {
    let side = sz::TABLE_PILL;
    Bounds::new(
        point(table.right() - side * 0.5, table.top() - side * 0.5),
        size(side, side),
    )
}

fn third(bounds: Bounds<Pixels>, index: usize) -> Bounds<Pixels> {
    let width = bounds.size.width / 3.0;
    Bounds::new(
        point(bounds.left() + width * index as f32, bounds.top()),
        size(width, bounds.size.height),
    )
}

fn half(bounds: Bounds<Pixels>, index: usize) -> Bounds<Pixels> {
    let width = bounds.size.width / 2.0;
    Bounds::new(
        point(bounds.left() + width * index as f32, bounds.top()),
        size(width, bounds.size.height),
    )
}

fn cell_anchor(table: &TableGeom, row: usize, col: usize) -> usize {
    table
        .rows
        .get(row)
        .or_else(|| table.rows.first())
        .and_then(|row| {
            row.anchors
                .get(col)
                .copied()
                .or_else(|| row.anchors.last().copied())
        })
        .or_else(|| table.columns.get(col).map(|column| column.anchor))
        .unwrap_or(table.source.start)
}

/// Byte the control should edit.
///
/// A row control keeps the column already under the pointer. The corner grip
/// uses that same intersection, or the header's last cell when the pointer
/// came in from outside the table.
pub fn action_anchor(tables: &[TableGeom], hit: &TableHit, hover: Option<&TableHover>) -> usize {
    let Some(table) = tables.iter().find(|table| table.source == hit.source) else {
        return hit.anchor;
    };
    let same = hover.filter(|hover| hover.source == hit.source);
    let hover_col = same.and_then(|hover| hover.col);
    let hover_row = same.and_then(|hover| hover.row);
    match hit.kind {
        TableHitKind::ColGrip | TableHitKind::ColAlign | TableHitKind::ColPlus => hit.anchor,
        TableHitKind::RowGrip | TableHitKind::RowPlus => {
            cell_anchor(table, hit.index, hover_col.unwrap_or(0))
        }
        TableHitKind::Corner => cell_anchor(
            table,
            hover_row.unwrap_or(0),
            hover_col.unwrap_or_else(|| table.columns.len().saturating_sub(1)),
        ),
    }
}

pub fn hit_test(tables: &[TableGeom], point: Point<Pixels>) -> Option<TableHit> {
    for table in tables {
        let corner = corner_grip_bounds(table.bounds);
        if contains(corner, point) {
            let (index, anchor, align) = anchor_at(table, 0, 0);
            return Some(TableHit {
                source: table.source.clone(),
                kind: TableHitKind::Corner,
                index,
                anchor,
                align,
            });
        }
        for (index, column) in table.columns.iter().enumerate() {
            let pill = column_pill_bounds(column.header_bounds);
            if !contains(pill, point) {
                continue;
            }
            let kind = if contains(third(pill, 0), point) {
                TableHitKind::ColGrip
            } else if contains(third(pill, 1), point) {
                TableHitKind::ColAlign
            } else {
                TableHitKind::ColPlus
            };
            return Some(TableHit {
                source: table.source.clone(),
                kind,
                index,
                anchor: column.anchor,
                align: column.align,
            });
        }
        for row in &table.rows {
            let pill = row_pill_bounds(row.bounds);
            if !contains(pill, point) {
                continue;
            }
            let kind = if contains(half(pill, 0), point) {
                TableHitKind::RowGrip
            } else {
                TableHitKind::RowPlus
            };
            let anchor = row.anchors.first().copied().unwrap_or(0);
            return Some(TableHit {
                source: table.source.clone(),
                kind,
                index: row.index,
                anchor,
                align: TableColumnAlign::None,
            });
        }
    }
    None
}

fn anchor_at(table: &TableGeom, row: usize, col: usize) -> (usize, usize, TableColumnAlign) {
    let align = table
        .columns
        .get(col)
        .map(|column| column.align)
        .unwrap_or(TableColumnAlign::None);
    let row_geom = table.rows.get(row).or_else(|| table.rows.first());
    let anchor = row_geom
        .and_then(|row| row.anchors.get(col).or_else(|| row.anchors.last()).copied())
        .unwrap_or(0);
    (col, anchor, align)
}

pub fn cell_under(
    tables: &[TableGeom],
    at: Point<Pixels>,
) -> Option<(std::ops::Range<usize>, usize, usize)> {
    for table in tables {
        if !contains(table.bounds, at) {
            continue;
        }
        let col = table
            .columns
            .iter()
            .position(|column| {
                at.x >= column.header_bounds.left() && at.x < column.header_bounds.right()
            })
            .unwrap_or(0);
        let row = table
            .rows
            .iter()
            .position(|row| at.y >= row.bounds.top() && at.y < row.bounds.bottom())
            .unwrap_or(0);
        return Some((table.source.clone(), col, row));
    }
    None
}

pub fn column_gap(table: &TableGeom, x: Pixels) -> usize {
    for (index, column) in table.columns.iter().enumerate() {
        let mid = column.header_bounds.left() + column.header_bounds.size.width * 0.5;
        if x < mid {
            return index;
        }
    }
    table.columns.len()
}

pub fn row_gap(table: &TableGeom, y: Pixels) -> usize {
    let mut gap = table.rows.len();
    for (index, row) in table.rows.iter().enumerate() {
        let mid = row.bounds.top() + row.bounds.size.height * 0.5;
        if y < mid {
            gap = index;
            break;
        }
    }
    // Index 0 is the header. A drop there is refused, so the indicator
    // never offers that slot.
    gap.max(1)
}

pub fn gap_at(tables: &[TableGeom], drag: &TableDrag, point: Point<Pixels>) -> usize {
    let Some(table) = tables.iter().find(|table| table.source == drag.source) else {
        return drag.from;
    };
    match drag.kind {
        TableDragKind::Column => column_gap(table, point.x),
        TableDragKind::Row => row_gap(table, point.y),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableHover {
    pub source: std::ops::Range<usize>,
    pub col: Option<usize>,
    pub row: Option<usize>,
}

pub fn hover_at(
    tables: &[TableGeom],
    at: Point<Pixels>,
    previous: Option<&TableHover>,
) -> Option<TableHover> {
    if let Some(hit) = hit_test(tables, at) {
        let (mut col, mut row) = match hit.kind {
            TableHitKind::ColGrip | TableHitKind::ColAlign | TableHitKind::ColPlus => {
                (Some(hit.index), None)
            }
            TableHitKind::RowGrip | TableHitKind::RowPlus => (None, Some(hit.index)),
            TableHitKind::Corner => (None, None),
        };
        if let Some(previous) = previous.filter(|hover| hover.source == hit.source) {
            if col.is_none() {
                col = previous.col;
            }
            if row.is_none() {
                row = previous.row;
            }
        }
        return Some(TableHover {
            source: hit.source,
            col,
            row,
        });
    }
    let (source, col, row) = cell_under(tables, at)?;
    Some(TableHover {
        source,
        col: Some(col),
        row: Some(row),
    })
}

#[derive(Clone, Copy)]
pub struct TableChromePaint {
    pub surface: gpui::Hsla,
    pub border: gpui::Hsla,
    pub muted: gpui::Hsla,
    pub ink: gpui::Hsla,
}

pub fn paint(
    window: &mut gpui::Window,
    tables: &[TableGeom],
    hover: Option<&TableHover>,
    drag: Option<&TableDrag>,
    colors: TableChromePaint,
) {
    let Some(hover) = hover else {
        if let Some(drag) = drag {
            paint_indicator(window, tables, drag, colors.ink);
        }
        return;
    };
    let Some(table) = tables.iter().find(|table| table.source == hover.source) else {
        return;
    };
    if let Some(col) = hover.col.and_then(|index| table.columns.get(index)) {
        paint_pill(
            window,
            column_pill_bounds(col.header_bounds),
            colors.surface,
            colors.border,
        );
        let pill = column_pill_bounds(col.header_bounds);
        paint_dots(window, third(pill, 0), colors.muted);
        paint_align(window, third(pill, 1), col.align, colors.muted);
        paint_plus(window, third(pill, 2), colors.muted);
    }
    if let Some(row) = hover.row.and_then(|index| table.rows.get(index)) {
        let pill = row_pill_bounds(row.bounds);
        paint_pill(window, pill, colors.surface, colors.border);
        paint_dots(window, half(pill, 0), colors.muted);
        paint_plus(window, half(pill, 1), colors.muted);
    }
    paint_dots(window, corner_grip_bounds(table.bounds), colors.muted);
    if let Some(drag) = drag {
        paint_indicator(window, tables, drag, colors.ink);
    }
}

fn paint_pill(
    window: &mut gpui::Window,
    bounds: Bounds<Pixels>,
    surface: gpui::Hsla,
    border: gpui::Hsla,
) {
    let mut quad = gpui::fill(bounds, surface);
    quad.corner_radii = gpui::Corners::all(sz::R_LG);
    quad.border_widths = gpui::Edges::all(sz::STROKE);
    quad.border_color = border;
    window.paint_quad(quad);
}

fn paint_dots(window: &mut gpui::Window, bounds: Bounds<Pixels>, color: gpui::Hsla) {
    let dot = sz::TABLE_DOT;
    let gap = sz::TABLE_DOT;
    let grid_w = dot * 2.0 + gap;
    let grid_h = dot * 3.0 + gap * 2.0;
    let origin_x = bounds.left() + (bounds.size.width - grid_w) * 0.5;
    let origin_y = bounds.top() + (bounds.size.height - grid_h) * 0.5;
    for col in 0..2 {
        for row in 0..3 {
            let mut quad = gpui::fill(
                Bounds::new(
                    point(
                        origin_x + (dot + gap) * col as f32,
                        origin_y + (dot + gap) * row as f32,
                    ),
                    size(dot, dot),
                ),
                color,
            );
            quad.corner_radii = gpui::Corners::all(dot * 0.5);
            window.paint_quad(quad);
        }
    }
}

fn paint_align(
    window: &mut gpui::Window,
    bounds: Bounds<Pixels>,
    align: TableColumnAlign,
    color: gpui::Hsla,
) {
    let widths = [sz::R_LG, sz::R_MD, sz::R_SM];
    let thickness = sz::STROKE;
    let gap = sz::TABLE_DOT;
    let block_h = thickness * 3.0 + gap * 2.0;
    let top = bounds.top() + (bounds.size.height - block_h) * 0.5;
    for (index, width) in widths.into_iter().enumerate() {
        let x = match align {
            TableColumnAlign::Right => bounds.right() - sz::R_SM - width,
            TableColumnAlign::Center => bounds.left() + (bounds.size.width - width) * 0.5,
            TableColumnAlign::Left | TableColumnAlign::None => bounds.left() + sz::R_SM,
        };
        let y = top + (thickness + gap) * index as f32;
        window.paint_quad(gpui::fill(
            Bounds::new(point(x, y), size(width, thickness)),
            color,
        ));
    }
}

fn paint_plus(window: &mut gpui::Window, bounds: Bounds<Pixels>, color: gpui::Hsla) {
    let arm = sz::R_LG;
    let thickness = sz::STROKE;
    let cx = bounds.left() + bounds.size.width * 0.5;
    let cy = bounds.top() + bounds.size.height * 0.5;
    window.paint_quad(gpui::fill(
        Bounds::new(
            point(cx - arm * 0.5, cy - thickness * 0.5),
            size(arm, thickness),
        ),
        color,
    ));
    window.paint_quad(gpui::fill(
        Bounds::new(
            point(cx - thickness * 0.5, cy - arm * 0.5),
            size(thickness, arm),
        ),
        color,
    ));
}

fn paint_indicator(
    window: &mut gpui::Window,
    tables: &[TableGeom],
    drag: &TableDrag,
    color: gpui::Hsla,
) {
    let Some(table) = tables.iter().find(|table| table.source == drag.source) else {
        return;
    };
    let thickness = sz::TABLE_DOT;
    let bounds = match drag.kind {
        TableDragKind::Column => {
            let x = if drag.gap >= table.columns.len() {
                table.bounds.right()
            } else {
                table.columns[drag.gap].header_bounds.left()
            };
            Bounds::new(
                point(x - thickness * 0.5, table.bounds.top()),
                size(thickness, table.bounds.size.height),
            )
        }
        TableDragKind::Row => {
            let y = if drag.gap >= table.rows.len() {
                table.bounds.bottom()
            } else {
                table.rows[drag.gap].bounds.top()
            };
            Bounds::new(
                point(table.bounds.left(), y - thickness * 0.5),
                size(table.bounds.size.width, thickness),
            )
        }
    };
    window.paint_quad(gpui::fill(bounds, color));
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px, Bounds};

    fn bounds(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
    }

    #[test]
    fn column_pill_sits_on_the_header_top_edge() {
        let header = bounds(40.0, 20.0, 80.0, 28.0);
        let pill = column_pill_bounds(header);
        let mid_y = pill.top() + pill.size.height * 0.5;
        assert_eq!(mid_y, header.top());
        let mid_x = pill.left() + pill.size.width * 0.5;
        let header_mid = header.left() + header.size.width * 0.5;
        assert_eq!(mid_x, header_mid);
    }

    #[test]
    fn row_pill_sits_on_the_left_edge() {
        let row = bounds(40.0, 20.0, 80.0, 28.0);
        let pill = row_pill_bounds(row);
        let mid_x = pill.left() + pill.size.width * 0.5;
        assert_eq!(mid_x, row.left());
        let mid_y = pill.top() + pill.size.height * 0.5;
        let row_mid = row.top() + row.size.height * 0.5;
        assert_eq!(mid_y, row_mid);
    }

    #[test]
    fn corner_grip_sits_on_the_top_right() {
        let table = bounds(10.0, 30.0, 100.0, 40.0);
        let grip = corner_grip_bounds(table);
        let mid_x = grip.left() + grip.size.width * 0.5;
        let mid_y = grip.top() + grip.size.height * 0.5;
        assert_eq!(mid_x, table.right());
        assert_eq!(mid_y, table.top());
    }

    #[test]
    fn final_index_keeps_a_drop_beside_the_item() {
        assert_eq!(final_index(1, 0), 0);
        assert_eq!(final_index(1, 1), 1);
        assert_eq!(final_index(1, 2), 1);
        assert_eq!(final_index(1, 3), 2);
        assert_eq!(final_index(1, 4), 3);
    }

    fn sample_table() -> super::super::text_editor::TableGeom {
        use super::super::text_editor::{TableColGeom, TableGeom, TableRowGeom};
        TableGeom {
            source: 0..80,
            bounds: bounds(100.0, 80.0, 180.0, 48.0),
            columns: vec![
                TableColGeom {
                    header_bounds: bounds(100.0, 80.0, 90.0, 24.0),
                    align: TableColumnAlign::None,
                    anchor: 2,
                },
                TableColGeom {
                    header_bounds: bounds(190.0, 80.0, 90.0, 24.0),
                    align: TableColumnAlign::Left,
                    anchor: 10,
                },
            ],
            rows: vec![
                TableRowGeom {
                    index: 0,
                    bounds: bounds(100.0, 80.0, 180.0, 24.0),
                    anchors: vec![2, 10],
                },
                TableRowGeom {
                    index: 1,
                    bounds: bounds(100.0, 104.0, 180.0, 24.0),
                    anchors: vec![40, 50],
                },
            ],
        }
    }

    #[test]
    fn column_pill_thirds_are_grip_align_and_plus() {
        let tables = [sample_table()];
        let grip = hit_test(&tables, point(px(128.0), px(80.0))).expect("grip");
        assert_eq!(grip.kind, TableHitKind::ColGrip);
        assert_eq!(grip.index, 0);
        let align = hit_test(&tables, point(px(145.0), px(80.0))).expect("align");
        assert_eq!(align.kind, TableHitKind::ColAlign);
        let plus = hit_test(&tables, point(px(160.0), px(80.0))).expect("plus");
        assert_eq!(plus.kind, TableHitKind::ColPlus);
        assert_eq!(plus.anchor, 2);
    }

    #[test]
    fn row_pill_halves_are_grip_and_plus() {
        let tables = [sample_table()];
        let grip = hit_test(&tables, point(px(90.0), px(116.0))).expect("row grip");
        assert_eq!(grip.kind, TableHitKind::RowGrip);
        assert_eq!(grip.index, 1);
        let plus = hit_test(&tables, point(px(108.0), px(116.0))).expect("row plus");
        assert_eq!(plus.kind, TableHitKind::RowPlus);
    }

    #[test]
    fn a_body_click_is_not_chrome() {
        let tables = [sample_table()];
        assert!(hit_test(&tables, point(px(140.0), px(116.0))).is_none());
    }

    #[test]
    fn corner_grip_is_the_top_right_and_keeps_the_cell() {
        let tables = [sample_table()];
        let corner = hit_test(&tables, point(px(280.0), px(80.0))).expect("corner");
        assert_eq!(corner.kind, TableHitKind::Corner);
        let over_cell = hover_at(&tables, point(px(220.0), px(116.0)), None).expect("cell");
        assert_eq!(over_cell.col, Some(1));
        assert_eq!(over_cell.row, Some(1));
        let on_row = hover_at(&tables, point(px(90.0), px(116.0)), Some(&over_cell)).expect("row");
        assert_eq!(on_row.col, Some(1));
        assert_eq!(on_row.row, Some(1));
        let row_hit = hit_test(&tables, point(px(108.0), px(116.0))).expect("plus");
        assert_eq!(action_anchor(&tables, &row_hit, Some(&on_row)), 50);
        let on_corner =
            hover_at(&tables, point(px(280.0), px(80.0)), Some(&on_row)).expect("corner hover");
        assert_eq!(action_anchor(&tables, &corner, Some(&on_corner)), 50);
    }

    #[test]
    fn row_gap_does_not_offer_the_header() {
        let table = sample_table();
        assert_eq!(row_gap(&table, px(0.0)), 1);
        assert_eq!(row_gap(&table, px(200.0)), 2);
    }

    #[test]
    fn align_cycles_back_to_none() {
        let mut align = TableColumnAlign::None;
        for expect in [
            TableColumnAlign::Left,
            TableColumnAlign::Center,
            TableColumnAlign::Right,
            TableColumnAlign::None,
        ] {
            align = next_align(align);
            assert_eq!(align, expect);
        }
    }
}
