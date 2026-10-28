//! Hover chrome for a rendered table.
//!
//! A column pill sits on the top edge of the hovered header cell, entirely
//! above the cell and no wider than that column: a 6-dot grip, an alignment
//! mark, and a plus. A 6-dot grip sits just outside the table's top-right
//! corner. The hovered row gets a grip and a plus entirely to the left of
//! the row. Bounds are derived from the painted cell boxes so a click hits
//! the same rect the frame drew.

use gpui::{point, px, Bounds, Pixels, Point, Size};

use crate::markdown::{ParsedDocument, TableColumnAlign};
use crate::model::table_ops::parsed_table_source_in;
use crate::model::DocumentTab;
use crate::theme::sz;

use super::text_editor::{PaintCell, PaintRow, TableGeom};

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
    /// False while the pointer is outside the table so the drop line hides.
    pub drop_visible: bool,
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
            viewport: None,
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

fn bounds_overlap(a: Bounds<Pixels>, b: Bounds<Pixels>) -> bool {
    a.left() < b.right() && b.left() < a.right() && a.top() < b.bottom() && b.top() < a.bottom()
}

/// Glyph boxes of non-table rows. Chrome must not cover or steal these.
pub fn occupied_content(rows: &[PaintRow]) -> Vec<Bounds<Pixels>> {
    rows.iter()
        .filter(|row| row.cells.is_empty())
        .map(|row| {
            Bounds::new(
                row.origin,
                Size {
                    width: row.shaped.width,
                    height: row.line_height,
                },
            )
        })
        .collect()
}

fn chrome_blocked(bounds: Bounds<Pixels>, occupied: &[Bounds<Pixels>]) -> bool {
    occupied
        .iter()
        .any(|content| bounds_overlap(bounds, *content))
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
    let height = sz::TABLE_PILL;
    let natural = sz::TABLE_PILL * 3.0;
    let width = if header.size.width < natural {
        header.size.width
    } else {
        natural
    };
    let left = header.left() + (header.size.width - width) * 0.5;
    Bounds::new(
        point(left, header.top() - height - sz::TABLE_GAP),
        size(width, height),
    )
}

pub fn row_pill_bounds(row: Bounds<Pixels>) -> Bounds<Pixels> {
    let width = sz::TABLE_PILL * 2.0;
    let natural_h = sz::TABLE_PILL;
    let height = if row.size.height < natural_h {
        row.size.height
    } else {
        natural_h
    };
    let top = row.top() + (row.size.height - height) * 0.5;
    Bounds::new(
        point(row.left() - width - sz::TABLE_GAP, top),
        size(width, height),
    )
}

pub fn corner_grip_bounds(table: Bounds<Pixels>) -> Bounds<Pixels> {
    let side = sz::TABLE_PILL;
    Bounds::new(
        point(table.right() + sz::TABLE_GAP, table.top() - side - sz::TABLE_GAP),
        size(side, side),
    )
}

/// Slide a control back inside the visible editor so a table at the edge of
/// the pane, or one scrolled under the toolbar, keeps its controls on screen.
fn fit(bounds: Bounds<Pixels>, viewport: Option<Bounds<Pixels>>) -> Bounds<Pixels> {
    let Some(view) = viewport else {
        return bounds;
    };
    let margin = sz::TABLE_EDGE;
    let max_left = (view.right() - margin - bounds.size.width).max(view.left() + margin);
    let max_top = (view.bottom() - margin - bounds.size.height).max(view.top() + margin);
    let left = bounds.left().max(view.left() + margin).min(max_left);
    let top = bounds.top().max(view.top() + margin).min(max_top);
    Bounds::new(point(left, top), bounds.size)
}

fn column_pill(table: &TableGeom, header: Bounds<Pixels>) -> Bounds<Pixels> {
    fit(column_pill_bounds(header), table.viewport)
}

/// Row pill that narrows to the room left of the table instead of covering
/// its first column. Below two slots of [`sz::TABLE_MIN_SLOT`] it keeps its
/// width and slides over the table, as [`fit`] does.
fn row_pill(table: &TableGeom, row: Bounds<Pixels>) -> Bounds<Pixels> {
    let pill = row_pill_bounds(row);
    let Some(view) = table.viewport else {
        return pill;
    };
    let room = row.left() - sz::TABLE_GAP - view.left() - sz::TABLE_EDGE;
    let min_width = sz::TABLE_MIN_SLOT * 2.0;
    if room >= pill.size.width || room < min_width {
        return fit(pill, table.viewport);
    }
    let left = row.left() - sz::TABLE_GAP - room;
    fit(
        Bounds::new(point(left, pill.top()), size(room, pill.size.height)),
        table.viewport,
    )
}

/// Corner grip, or `None` when it would fall outside the editor.
fn corner_pill(table: &TableGeom) -> Option<Bounds<Pixels>> {
    let corner = corner_grip_bounds(table.bounds);
    let inside = table.viewport.is_none_or(|view| {
        corner.left() >= view.left()
            && corner.right() <= view.right()
            && corner.top() >= view.top()
            && corner.bottom() <= view.bottom()
    });
    inside.then_some(corner)
}

/// Area that picks a column pill: the pill plus the gap down to its header,
/// so the pointer can cross the gap without the controls disappearing.
fn column_pill_hit(pill: Bounds<Pixels>, header: Bounds<Pixels>) -> Bounds<Pixels> {
    let height = (header.top() - pill.top()).max(pill.size.height);
    Bounds::new(pill.origin, size(pill.size.width, height))
}

fn row_pill_hit(pill: Bounds<Pixels>, row: Bounds<Pixels>) -> Bounds<Pixels> {
    let width = (row.left() - pill.left()).max(pill.size.width);
    Bounds::new(pill.origin, size(width, pill.size.height))
}

fn corner_hit(corner: Bounds<Pixels>, table: Bounds<Pixels>) -> Bounds<Pixels> {
    let anchor = Bounds::new(
        point(table.right() - px(1.0), table.top() - px(1.0)),
        size(px(1.0), px(1.0)),
    );
    union_bounds(corner, anchor)
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

#[cfg_attr(not(test), allow(dead_code))]
pub fn hit_test(tables: &[TableGeom], point: Point<Pixels>) -> Option<TableHit> {
    hit_test_occupied(tables, point, &[])
}

pub fn hit_test_occupied(
    tables: &[TableGeom],
    point: Point<Pixels>,
    occupied: &[Bounds<Pixels>],
) -> Option<TableHit> {
    if occupied.iter().any(|content| contains(*content, point)) {
        return None;
    }
    for table in tables {
        for (index, column) in table.columns.iter().enumerate() {
            let pill = column_pill(table, column.header_bounds);
            let area = column_pill_hit(pill, column.header_bounds);
            if !contains(area, point) || chrome_blocked(pill, occupied) {
                continue;
            }
            let kind = if contains(third(area, 0), point) {
                TableHitKind::ColGrip
            } else if contains(third(area, 1), point) {
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
            let pill = row_pill(table, row.bounds);
            let area = row_pill_hit(pill, row.bounds);
            if !contains(area, point) || chrome_blocked(pill, occupied) {
                continue;
            }
            // Split on the painted pill so the gap beside the table belongs to the plus.
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
        let corner = corner_pill(table);
        if corner.is_some_and(|corner| {
            contains(corner_hit(corner, table.bounds), point) && !chrome_blocked(corner, occupied)
        }) {
            let (index, anchor, align) = anchor_at(table, 0, 0);
            return Some(TableHit {
                source: table.source.clone(),
                kind: TableHitKind::Corner,
                index,
                anchor,
                align,
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

/// Gap while the pointer is inside the dragged table. Outside, hide the line.
pub fn drag_gap_if_inside(
    tables: &[TableGeom],
    drag: &TableDrag,
    at: Point<Pixels>,
) -> Option<usize> {
    let table = tables.iter().find(|table| table.source == drag.source)?;
    if !contains(table.bounds, at) {
        return None;
    }
    Some(gap_at(tables, drag, at))
}

/// Update the live drop gap. `None` hides the indicator until the pointer
/// re-enters the table.
pub fn update_drag_gap(drag: &mut TableDrag, gap: Option<usize>) {
    match gap {
        Some(gap) => {
            drag.gap = gap;
            drag.drop_visible = true;
        }
        None => drag.drop_visible = false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragRelease {
    Cancel,
    Move(usize),
}

/// A release inside the table commits. A release outside cancels and writes nothing.
pub fn drag_release(tables: &[TableGeom], drag: &TableDrag, at: Point<Pixels>) -> DragRelease {
    let Some(table) = tables.iter().find(|table| table.source == drag.source) else {
        return DragRelease::Cancel;
    };
    if !contains(table.bounds, at) {
        return DragRelease::Cancel;
    }
    let gap = match drag.kind {
        TableDragKind::Column => column_gap(table, at.x),
        TableDragKind::Row => row_gap(table, at.y),
    };
    let to = final_index(drag.from, gap);
    if to == drag.from {
        return DragRelease::Cancel;
    }
    if matches!(drag.kind, TableDragKind::Row) && to == 0 {
        return DragRelease::Cancel;
    }
    DragRelease::Move(to)
}

/// Apply a release. Returns whether the document bytes changed.
pub fn commit_drag_release(
    doc: &mut DocumentTab,
    tables: &[TableGeom],
    drag: &TableDrag,
    at: Point<Pixels>,
) -> bool {
    let DragRelease::Move(to) = drag_release(tables, drag, at) else {
        return false;
    };
    match drag.kind {
        TableDragKind::Row => doc.table_move_row_at(drag.anchor, to),
        TableDragKind::Column => doc.table_move_col_at(drag.anchor, to),
    }
    true
}

/// Characters a key writes while a drag is active.
///
/// Every key is consumed: Escape, a modifier, and a printable key. The
/// returned string is empty, so the key writes nothing.
pub fn drag_key_insert(_key: &str, _typed: &str) -> &'static str {
    ""
}

/// A key cancels the drag. The string is what that key is allowed to insert.
pub fn clear_drag_for_key(
    drag: Option<TableDrag>,
    key: &str,
    typed: &str,
) -> (Option<TableDrag>, &'static str) {
    if drag.is_none() {
        return (None, "");
    }
    (None, drag_key_insert(key, typed))
}

/// A modifier press cancels the drag. Nothing is written.
pub fn clear_drag_for_modifier(_drag: Option<TableDrag>) -> Option<TableDrag> {
    None
}

/// Keep a hover on the table after an edit moves its byte range.
pub fn retarget_hover(
    parsed: &ParsedDocument,
    content: &str,
    hover: &TableHover,
) -> Option<TableHover> {
    let probe = hover.source.start.min(content.len());
    let source = parsed_table_source_in(parsed, probe)?;
    Some(TableHover {
        source,
        col: hover.col,
        row: hover.row,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableHover {
    pub source: std::ops::Range<usize>,
    pub col: Option<usize>,
    pub row: Option<usize>,
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn hover_at(
    tables: &[TableGeom],
    at: Point<Pixels>,
    previous: Option<&TableHover>,
) -> Option<TableHover> {
    hover_at_occupied(tables, at, previous, &[])
}

pub fn hover_at_occupied(
    tables: &[TableGeom],
    at: Point<Pixels>,
    previous: Option<&TableHover>,
    occupied: &[Bounds<Pixels>],
) -> Option<TableHover> {
    hover_from_hit(
        tables,
        at,
        previous,
        hit_test_occupied(tables, at, occupied).as_ref(),
    )
}

/// Hover for a pointer position whose control hit was already computed.
pub fn hover_from_hit(
    tables: &[TableGeom],
    at: Point<Pixels>,
    previous: Option<&TableHover>,
    hit: Option<&TableHit>,
) -> Option<TableHover> {
    if let Some(hit) = hit {
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
            source: hit.source.clone(),
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

/// The control under the pointer, for the hover and pressed highlight.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableHot {
    pub source: std::ops::Range<usize>,
    pub kind: TableHitKind,
    pub index: usize,
}

impl From<&TableHit> for TableHot {
    fn from(hit: &TableHit) -> Self {
        Self {
            source: hit.source.clone(),
            kind: hit.kind,
            index: hit.index,
        }
    }
}

#[derive(Clone, Copy)]
pub struct TableChromePaint {
    pub surface: gpui::Hsla,
    pub border: gpui::Hsla,
    pub muted: gpui::Hsla,
    pub ink: gpui::Hsla,
    pub hover_fill: gpui::Hsla,
    pub active_fill: gpui::Hsla,
    /// Mouse button is down on a table control.
    pub pressed: bool,
}

pub fn paint(
    window: &mut gpui::Window,
    tables: &[TableGeom],
    hover: Option<&TableHover>,
    drag: Option<&TableDrag>,
    hot: Option<&TableHot>,
    colors: TableChromePaint,
    occupied: &[Bounds<Pixels>],
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
        let pill = column_pill(table, col.header_bounds);
        if !chrome_blocked(pill, occupied) {
            paint_pill(window, pill, colors.surface, colors.border);
            for (slot, kind) in [
                (0, TableHitKind::ColGrip),
                (1, TableHitKind::ColAlign),
                (2, TableHitKind::ColPlus),
            ] {
                let fill = button_fill(hot, &table.source, kind, hover.col, &colors);
                paint_button(window, third(pill, slot), fill);
            }
            paint_dots(window, third(pill, 0), colors.muted);
            paint_align(window, third(pill, 1), col.align, colors.muted);
            paint_plus(window, third(pill, 2), colors.muted);
        }
    }
    if let Some(row) = hover.row.and_then(|index| table.rows.get(index)) {
        let pill = row_pill(table, row.bounds);
        if !chrome_blocked(pill, occupied) {
            paint_pill(window, pill, colors.surface, colors.border);
            for (slot, kind) in [(0, TableHitKind::RowGrip), (1, TableHitKind::RowPlus)] {
                let fill = button_fill(hot, &table.source, kind, hover.row, &colors);
                paint_button(window, half(pill, slot), fill);
            }
            paint_dots(window, half(pill, 0), colors.muted);
            paint_plus(window, half(pill, 1), colors.muted);
        }
    }
    if let Some(corner) = corner_pill(table).filter(|corner| !chrome_blocked(*corner, occupied)) {
        let fill = button_fill(hot, &table.source, TableHitKind::Corner, None, &colors);
        paint_button(window, corner, fill);
        paint_dots(window, corner, colors.muted);
    }
    if let Some(drag) = drag {
        paint_indicator(window, tables, drag, colors.ink);
    }
}

/// Fill for one control: the pressed tint while the button is down on it, the
/// hover tint while the pointer is over it, nothing otherwise.
fn button_fill(
    hot: Option<&TableHot>,
    source: &std::ops::Range<usize>,
    kind: TableHitKind,
    index: Option<usize>,
    colors: &TableChromePaint,
) -> Option<gpui::Hsla> {
    let hot = hot?;
    let same_index = index.map_or(kind == TableHitKind::Corner, |index| index == hot.index);
    if hot.source != *source || hot.kind != kind || !same_index {
        return None;
    }
    Some(if colors.pressed {
        colors.active_fill
    } else {
        colors.hover_fill
    })
}

fn paint_button(window: &mut gpui::Window, slot: Bounds<Pixels>, fill: Option<gpui::Hsla>) {
    let Some(fill) = fill else {
        return;
    };
    let inset = sz::TABLE_BTN_INSET;
    let bounds = Bounds::new(
        point(slot.left() + inset, slot.top() + inset),
        size(
            slot.size.width - inset * 2.0,
            slot.size.height - inset * 2.0,
        ),
    );
    let mut quad = gpui::fill(bounds, fill);
    quad.corner_radii = gpui::Corners::all(sz::R_MD);
    window.paint_quad(quad);
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

fn slot_scale(slot: Bounds<Pixels>, natural: Pixels) -> f32 {
    let avail: f32 = slot.size.width.into();
    let need: f32 = natural.into();
    if need <= 0.0 {
        1.0
    } else {
        (avail / need).clamp(0.0, 1.0)
    }
}

fn align_icon_pad(slot: Bounds<Pixels>) -> Pixels {
    sz::TABLE_ICON_PAD * slot_scale(slot, sz::TABLE_ICON_PAD + sz::TABLE_BAR_LG)
}

fn align_bar_widths(slot: Bounds<Pixels>) -> [Pixels; 3] {
    let scale = slot_scale(slot, sz::TABLE_ICON_PAD + sz::TABLE_BAR_LG);
    [
        sz::TABLE_BAR_LG * scale,
        sz::TABLE_BAR_MD * scale,
        sz::TABLE_BAR_SM * scale,
    ]
}

fn plus_arm(slot: Bounds<Pixels>) -> Pixels {
    sz::TABLE_PLUS_ARM * slot_scale(slot, sz::TABLE_PLUS_ARM)
}

fn paint_align(
    window: &mut gpui::Window,
    bounds: Bounds<Pixels>,
    align: TableColumnAlign,
    color: gpui::Hsla,
) {
    let pad = align_icon_pad(bounds);
    let widths = align_bar_widths(bounds);
    let thickness = sz::STROKE;
    let gap = sz::TABLE_DOT;
    let block_h = thickness * 3.0 + gap * 2.0;
    let top = bounds.top() + (bounds.size.height - block_h) * 0.5;
    for (index, width) in widths.into_iter().enumerate() {
        let x = match align {
            TableColumnAlign::Right => bounds.right() - pad - width,
            TableColumnAlign::Center => bounds.left() + (bounds.size.width - width) * 0.5,
            TableColumnAlign::Left | TableColumnAlign::None => bounds.left() + pad,
        };
        let y = top + (thickness + gap) * index as f32;
        window.paint_quad(gpui::fill(
            Bounds::new(point(x, y), size(width, thickness)),
            color,
        ));
    }
}

fn paint_plus(window: &mut gpui::Window, bounds: Bounds<Pixels>, color: gpui::Hsla) {
    let arm = plus_arm(bounds);
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
    if !drag.drop_visible {
        return;
    }
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
    use super::super::text_editor::{TableColGeom, TableGeom, TableRowGeom};
    use super::*;
    use crate::model::table_ops::{parsed_table_source, TableOp};
    use gpui::{point, px, Bounds};

    fn bounds(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
    }

    fn overlaps(a: Bounds<Pixels>, b: Bounds<Pixels>) -> bool {
        a.left() < b.right() && b.left() < a.right() && a.top() < b.bottom() && b.top() < a.bottom()
    }

    fn center_of(bounds: Bounds<Pixels>) -> Point<Pixels> {
        point(
            bounds.left() + bounds.size.width * 0.5,
            bounds.top() + bounds.size.height * 0.5,
        )
    }

    #[test]
    fn column_pill_sits_on_the_header_top_edge() {
        let header = bounds(40.0, 20.0, 80.0, 28.0);
        let pill = column_pill_bounds(header);
        assert_eq!(pill.bottom(), header.top() - sz::TABLE_GAP);
        assert!(pill.left() >= header.left());
        assert!(pill.right() <= header.right());
        assert!(!overlaps(pill, header));
        let mid_x = pill.left() + pill.size.width * 0.5;
        let header_mid = header.left() + header.size.width * 0.5;
        assert_eq!(mid_x, header_mid);
    }

    #[test]
    fn row_pill_sits_on_the_left_edge() {
        let row = bounds(40.0, 20.0, 80.0, 28.0);
        let pill = row_pill_bounds(row);
        assert_eq!(pill.right(), row.left() - sz::TABLE_GAP);
        assert!(pill.top() >= row.top());
        assert!(pill.bottom() <= row.bottom());
        assert!(!overlaps(pill, row));
        let mid_y = pill.top() + pill.size.height * 0.5;
        let row_mid = row.top() + row.size.height * 0.5;
        assert_eq!(mid_y, row_mid);
    }

    #[test]
    fn corner_grip_sits_on_the_top_right() {
        let table = bounds(10.0, 30.0, 100.0, 40.0);
        let grip = corner_grip_bounds(table);
        assert_eq!(grip.left(), table.right() + sz::TABLE_GAP);
        assert_eq!(grip.bottom(), table.top() - sz::TABLE_GAP);
        assert!(!overlaps(grip, table));
    }

    #[test]
    fn pill_hit_area_bridges_the_gap_to_the_table() {
        let header = bounds(40.0, 60.0, 80.0, 28.0);
        let pill = column_pill_bounds(header);
        let area = column_pill_hit(pill, header);
        assert_eq!(area.top(), pill.top());
        assert_eq!(area.bottom(), header.top());
        let row = bounds(100.0, 20.0, 80.0, 28.0);
        let pill = row_pill_bounds(row);
        let area = row_pill_hit(pill, row);
        assert_eq!(area.left(), pill.left());
        assert_eq!(area.right(), row.left());
    }

    #[test]
    fn controls_slide_back_inside_the_viewport() {
        let area = bounds(0.0, 40.0, 400.0, 300.0);
        let view = Some(area);
        let margin = sz::TABLE_EDGE;
        let row = row_pill_bounds(bounds(20.0, 100.0, 200.0, 28.0));
        assert!(row.left() < area.left());
        assert_eq!(fit(row, view).left(), margin);
        let col = column_pill_bounds(bounds(100.0, 50.0, 80.0, 28.0));
        assert!(col.top() < area.top());
        assert_eq!(fit(col, view).top(), area.top() + margin);
        let inside = bounds(100.0, 100.0, 50.0, 28.0);
        assert_eq!(fit(inside, view), inside);
        assert_eq!(fit(col, None), col);
    }

    #[test]
    fn row_pill_narrows_to_the_room_beside_the_table() {
        let mut table = sample_table();
        let row = table.rows[0].bounds;
        let room = px(40.0);
        let edge = row.left() - sz::TABLE_GAP - room - sz::TABLE_EDGE;
        table.viewport = Some(bounds(
            f32::from(edge),
            0.0,
            f32::from(row.right() - edge),
            400.0,
        ));
        let pill = row_pill(&table, row);
        assert_eq!(pill.size.width, room);
        assert_eq!(pill.right(), row.left() - sz::TABLE_GAP);
        assert!(!overlaps(pill, row));
    }

    #[test]
    fn row_gap_beside_the_table_belongs_to_the_plus() {
        let tables = [sample_table()];
        let row = tables[0].rows[0].bounds;
        let pill = row_pill_bounds(row);
        let grip = hit_test(&tables, center_of(half(pill, 0))).expect("grip");
        assert_eq!(grip.kind, TableHitKind::RowGrip);
        let in_gap = point(row.left() - sz::TABLE_GAP * 0.5, center_of(pill).y);
        let plus = hit_test(&tables, in_gap).expect("gap");
        assert_eq!(plus.kind, TableHitKind::RowPlus);
        let just_inside_grip = point(half(pill, 0).right() - px(1.0), center_of(pill).y);
        let still_grip = hit_test(&tables, just_inside_grip).expect("grip edge");
        assert_eq!(still_grip.kind, TableHitKind::RowGrip);
    }

    #[test]
    fn final_index_keeps_a_drop_beside_the_item() {
        assert_eq!(final_index(1, 0), 0);
        assert_eq!(final_index(1, 1), 1);
        assert_eq!(final_index(1, 2), 1);
        assert_eq!(final_index(1, 3), 2);
        assert_eq!(final_index(1, 4), 3);
    }

    fn sample_table() -> TableGeom {
        TableGeom {
            viewport: None,
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
        let pill = column_pill_bounds(tables[0].columns[0].header_bounds);
        let grip = hit_test(&tables, center_of(third(pill, 0))).expect("grip");
        assert_eq!(grip.kind, TableHitKind::ColGrip);
        assert_eq!(grip.index, 0);
        let align = hit_test(&tables, center_of(third(pill, 1))).expect("align");
        assert_eq!(align.kind, TableHitKind::ColAlign);
        let plus = hit_test(&tables, center_of(third(pill, 2))).expect("plus");
        assert_eq!(plus.kind, TableHitKind::ColPlus);
        assert_eq!(plus.anchor, 2);
    }

    #[test]
    fn row_pill_halves_are_grip_and_plus() {
        let tables = [sample_table()];
        let pill = row_pill_bounds(tables[0].rows[1].bounds);
        let grip = hit_test(&tables, center_of(half(pill, 0))).expect("row grip");
        assert_eq!(grip.kind, TableHitKind::RowGrip);
        assert_eq!(grip.index, 1);
        let plus = hit_test(&tables, center_of(half(pill, 1))).expect("row plus");
        assert_eq!(plus.kind, TableHitKind::RowPlus);
    }

    #[test]
    fn a_body_click_is_not_chrome() {
        let tables = [sample_table()];
        assert!(hit_test(&tables, point(px(140.0), px(116.0))).is_none());
        assert!(hit_test(&tables, point(px(145.0), px(90.0))).is_none());
    }

    #[test]
    fn corner_grip_is_the_top_right_and_keeps_the_cell() {
        let tables = [sample_table()];
        let grip = corner_grip_bounds(tables[0].bounds);
        let corner = hit_test(&tables, center_of(grip)).expect("corner");
        assert_eq!(corner.kind, TableHitKind::Corner);
        let over_cell = hover_at(&tables, point(px(220.0), px(116.0)), None).expect("cell");
        assert_eq!(over_cell.col, Some(1));
        assert_eq!(over_cell.row, Some(1));
        let row_pill = row_pill_bounds(tables[0].rows[1].bounds);
        let on_row =
            hover_at(&tables, center_of(half(row_pill, 0)), Some(&over_cell)).expect("row");
        assert_eq!(on_row.col, Some(1));
        assert_eq!(on_row.row, Some(1));
        let row_hit = hit_test(&tables, center_of(half(row_pill, 1))).expect("plus");
        assert_eq!(action_anchor(&tables, &row_hit, Some(&on_row)), 50);
        let on_corner = hover_at(&tables, center_of(grip), Some(&on_row)).expect("corner hover");
        assert_eq!(action_anchor(&tables, &corner, Some(&on_corner)), 50);
    }

    #[test]
    fn narrow_column_pills_do_not_overlap_and_plus_wins() {
        let table = TableGeom {
            viewport: None,
            source: 0..20,
            bounds: bounds(0.0, 40.0, 64.0, 24.0),
            columns: vec![
                TableColGeom {
                    header_bounds: bounds(0.0, 40.0, 32.0, 24.0),
                    align: TableColumnAlign::None,
                    anchor: 2,
                },
                TableColGeom {
                    header_bounds: bounds(32.0, 40.0, 32.0, 24.0),
                    align: TableColumnAlign::None,
                    anchor: 8,
                },
            ],
            rows: vec![TableRowGeom {
                index: 0,
                bounds: bounds(0.0, 40.0, 64.0, 24.0),
                anchors: vec![2, 8],
            }],
        };
        let left = column_pill_bounds(table.columns[0].header_bounds);
        let right = column_pill_bounds(table.columns[1].header_bounds);
        let corner = corner_grip_bounds(table.bounds);
        assert!(!overlaps(left, right));
        assert!(!overlaps(left, table.columns[0].header_bounds));
        assert!(!overlaps(right, table.columns[1].header_bounds));
        assert!(!overlaps(right, corner));
        assert!(left.right() <= right.left());
        let plus = hit_test(&[table], center_of(third(right, 2))).expect("plus");
        assert_eq!(plus.kind, TableHitKind::ColPlus);
        assert_eq!(plus.index, 1);
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

    const TABLE: &str = "| A | B |\n| --- | --- |\n| d | e |\n| g | h |\n";
    const TABLE_MOVED: &str = "| A | B |\n| --- | --- |\n| g | h |\n| d | e |\n";

    fn table_doc() -> DocumentTab {
        let mut doc = DocumentTab::new("t".into(), "t.md".into(), TABLE.into(), None, None);
        doc.caret.move_to(26);
        doc
    }

    /// Header plus two body rows. A release at y=55 is past the last midpoint, so
    /// the row gap is 3 and `final_index(1, 3)` is 2. x=20 is left of the table.
    fn two_body_rows() -> TableGeom {
        TableGeom {
            viewport: None,
            source: 0..43,
            bounds: bounds(100.0, 0.0, 200.0, 60.0),
            columns: vec![TableColGeom {
                header_bounds: bounds(100.0, 0.0, 200.0, 20.0),
                align: TableColumnAlign::None,
                anchor: 2,
            }],
            rows: vec![
                TableRowGeom {
                    index: 0,
                    bounds: bounds(100.0, 0.0, 200.0, 20.0),
                    anchors: vec![2],
                },
                TableRowGeom {
                    index: 1,
                    bounds: bounds(100.0, 20.0, 200.0, 20.0),
                    anchors: vec![26],
                },
                TableRowGeom {
                    index: 2,
                    bounds: bounds(100.0, 40.0, 200.0, 20.0),
                    anchors: vec![36],
                },
            ],
        }
    }

    fn row_drag() -> TableDrag {
        TableDrag {
            source: 0..43,
            kind: TableDragKind::Row,
            from: 1,
            anchor: 26,
            gap: 1,
            drop_visible: true,
        }
    }

    fn write_key(doc: &mut DocumentTab, key: &str, typed: &str, drag: TableDrag) {
        let tables = [two_body_rows()];
        let (still, insert) = clear_drag_for_key(Some(drag), key, typed);
        if !insert.is_empty() {
            let at = doc.cursor_offset();
            doc.edit(at..at, insert);
        }
        if let Some(drag) = still {
            commit_drag_release(doc, &tables, &drag, point(px(150.0), px(55.0)));
        }
    }

    #[test]
    fn drag_escape_writes_nothing() {
        let mut doc = table_doc();
        write_key(&mut doc, "escape", "", row_drag());
        assert_eq!(doc.content, TABLE);
    }

    #[test]
    fn drag_printable_key_writes_nothing() {
        let mut doc = table_doc();
        write_key(&mut doc, "x", "x", row_drag());
        assert_eq!(doc.content, TABLE);
    }

    #[test]
    fn drag_modifier_then_release_writes_nothing() {
        let mut doc = table_doc();
        let tables = [two_body_rows()];
        let still = clear_drag_for_modifier(Some(row_drag()));
        if let Some(drag) = still {
            commit_drag_release(&mut doc, &tables, &drag, point(px(150.0), px(55.0)));
        }
        assert_eq!(doc.content, TABLE);
    }

    #[test]
    fn drag_release_outside_writes_nothing() {
        let mut doc = table_doc();
        let tables = [two_body_rows()];
        let drag = row_drag();
        assert_eq!(
            drag_release(&tables, &drag, point(px(20.0), px(55.0))),
            DragRelease::Cancel
        );
        assert!(!commit_drag_release(
            &mut doc,
            &tables,
            &drag,
            point(px(20.0), px(55.0))
        ));
        assert_eq!(doc.content, TABLE);
    }

    #[test]
    fn drag_release_inside_moves_the_row() {
        let mut doc = table_doc();
        let tables = [two_body_rows()];
        let drag = row_drag();
        assert_eq!(
            drag_release(&tables, &drag, point(px(150.0), px(55.0))),
            DragRelease::Move(2)
        );
        assert!(commit_drag_release(
            &mut doc,
            &tables,
            &drag,
            point(px(150.0), px(55.0))
        ));
        assert_eq!(doc.content, TABLE_MOVED);
    }

    #[test]
    fn plus_retargets_hover_onto_the_edited_table() {
        let mut doc = table_doc();
        let before = parsed_table_source(&doc.content, 26).expect("table");
        let hover = TableHover {
            source: before.clone(),
            col: Some(0),
            row: Some(1),
        };
        assert!(doc.apply_table_op_at(26, TableOp::InsertColRight));
        let next = retarget_hover(&doc.parsed(), &doc.content, &hover).expect("hover");
        assert_eq!(next.col, Some(0));
        assert_eq!(next.row, Some(1));
        assert_ne!(next.source.end, hover.source.end);
        assert_eq!(
            next.source,
            parsed_table_source(&doc.content, next.source.start).expect("retarget")
        );
    }

    #[test]
    fn column_pill_does_not_hit_content_above_the_table() {
        let tables = [sample_table()];
        let pill = column_pill_bounds(tables[0].columns[0].header_bounds);
        let paragraph = bounds(
            f32::from(pill.left()),
            f32::from(pill.top()),
            f32::from(pill.size.width),
            f32::from(pill.size.height),
        );
        let at = center_of(third(pill, 1));
        assert!(contains(paragraph, at));
        assert!(hit_test(&tables, at).is_some());
        assert!(hit_test_occupied(&tables, at, &[paragraph]).is_none());
        assert_eq!(hover_at_occupied(&tables, at, None, &[paragraph]), None);
    }

    #[test]
    fn drop_indicator_hides_outside_the_table() {
        let mut drag = row_drag();
        update_drag_gap(&mut drag, None);
        assert!(!drag.drop_visible);
        update_drag_gap(&mut drag, Some(2));
        assert!(drag.drop_visible);
        assert_eq!(drag.gap, 2);
    }

    #[test]
    fn align_bars_fit_a_narrow_slot() {
        let slot = bounds(0.0, 0.0, 10.0, 16.0);
        let pad = align_icon_pad(slot);
        for width in align_bar_widths(slot) {
            assert!(pad + width <= slot.size.width);
        }
        assert!(plus_arm(slot) <= slot.size.width);
    }
}
