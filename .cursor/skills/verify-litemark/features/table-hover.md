# Table hover

Hovering a rendered table shows a quiet pill on the hovered column and the hovered row, and a separate 6-dot grip on the table's top-right corner. The controls call the same table edits as the menu. They do not start a second edit path.

The column pill sits on the top edge of that column's header cell. It holds a 6-dot grip, an alignment mark, and a plus. The row pill sits on the left edge of that row and holds a grip and a plus. The corner grip is only the six dots.

## Sub-features

- `table-hover-column` shows the column pill for the column under the pointer.
- `table-hover-row` shows the row pill for the row under the pointer.
- `table-hover-corner` shows the corner grip whenever that table is hovered, and opens the existing table menu without moving the caret first.
- `table-hover-drag` reorders a body row or a column. Escape, or any other key, drops the drag without writing. The header cannot be dragged, and a row drop is never offered on the header.

## How to get to it (user POV)

- Open a markdown note in WYSIWYG and move the pointer over a table cell.
- Drag the grip to reorder. Click the alignment mark to cycle none, left, centre, right. Click plus to insert a column to the right or a row below.
- Click the corner grip for insert, delete, and alignment. Right-click a cell for the same menu at the caret.

## Driving it with control-litemark

Preconditions:

- `scripts/env.sh` has been sourced so `RUSTUP_TOOLCHAIN=stable` is set when rustup is present.
- The window must not be started.

- **Geometry.** `cargo test -- column_pill_sits_on_the_header_top_edge row_pill_sits_on_the_left_edge corner_grip_sits_on_the_top_right column_pill_thirds_are_grip_align_and_plus row_pill_halves_are_grip_and_plus a_body_click_is_not_chrome corner_grip_is_the_top_right_and_keeps_the_cell row_gap_does_not_offer_the_header align_cycles_back_to_none final_index_keeps_a_drop_beside_the_item`
- **Bytes.** The edits those controls call are the table-ops gate: `.cursor/skills/verify-litemark/scripts/table-ops-gate.sh`.
- **Refuse the window.** Run `.cursor/skills/verify-litemark/scripts/refuse-gui.sh`. Exit code is 2. The notes directory is unisolated and `/usr/share/vulkan/icd.d` is missing.

## Gotchas

- A PDF tab and Raw view draw no chrome.
- Hover does not move the caret. A click or a drop does, by the table-op caret rule, and only when the selection is empty.
- The header row's grip does not start a drag. Plus on that row inserts the first body row.
- A table inside a block quote or a list is not a table in this parser, so it has no chrome.
- `src/main.rs` reads `args[1]` as a note to open. This kit does not launch that command.
