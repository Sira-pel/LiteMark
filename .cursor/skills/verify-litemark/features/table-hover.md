# Table hover

Hovering a rendered table shows a quiet pill on the hovered column and the hovered row, and a separate 6-dot grip just outside the table's top-right corner. The controls call the same table edits as the menu. They do not start a second edit path.

The column pill sits on the top edge of that column's header cell, entirely above the cell, and no wider than the column. Chrome never covers or steals clicks from content, so a pill that would overlap the line above the table is not painted and is not hittable there. It holds a 6-dot grip, an alignment mark, and a plus. The row pill sits on the left edge of that row, entirely to the left of the cell, and holds a grip and a plus. The corner grip is only the six dots. A column pill is tested before the corner, so the plus on a narrow column stays the column control.

## Sub-features

- `table-hover-column` shows the column pill for the column under the pointer.
- `table-hover-row` shows the row pill for the row under the pointer.
- `table-hover-corner` shows the corner grip whenever that table is hovered, and opens the existing table menu without moving the caret first.
- `table-hover-drag` reorders a body row or a column. Any key cancels the drag and writes nothing. That includes Escape, a printable key, a bound editor action (Backspace, Delete, Enter, Tab), and a modifier. The key is consumed. A modifier cancels when the modifier state changes, which is how a Shift press arrives. While the mouse press that started the drag is still held, no key, chord, or text input may change the note, even after that modifier cancel. The press-held latch clears when that press ends: a mouse-up on or outside the hovered root, the next mouse-down, an in-window move with no button pressed, a MouseExited with no button, window focus loss, Escape, a right-click, or a non-drag chrome/checkbox click. A MouseExited that still has a button (X11 leave mid-press, no grab) keeps the latch; Wayland delivers MouseUp via the implicit grab before Leave; Windows uses SetCapture. A release outside the table bounds cancels and writes nothing, and the drop indicator hides while the pointer is outside. A release inside the table commits the move. The header cannot be dragged, and a row drop is never offered on the header. The chord oracle is 14 chords × 2 geometries = 28/28 held runs, not 17.
- `table-hover-plus` inserts a column to the right or a row below and leaves the chrome up while the pointer is still over that table.

## How to get to it (user POV)

- Open a markdown note in WYSIWYG and move the pointer over a table cell.
- Drag the grip to reorder. Click the alignment mark to cycle none, left, centre, right. Click plus to insert a column to the right or a row below.
- Click the corner grip for insert, delete, and alignment. Right-click a cell for the same menu at the caret.

## Driving it with control-litemark

Preconditions:

- `scripts/env.sh` has been sourced so `RUSTUP_TOOLCHAIN=stable` is set when rustup is present.

- **Geometry.** `cargo test -- column_pill_sits_on_the_header_top_edge row_pill_sits_on_the_left_edge corner_grip_sits_on_the_top_right column_pill_thirds_are_grip_align_and_plus row_pill_halves_are_grip_and_plus a_body_click_is_not_chrome corner_grip_is_the_top_right_and_keeps_the_cell narrow_column_pills_do_not_overlap_and_plus_wins row_gap_does_not_offer_the_header align_cycles_back_to_none final_index_keeps_a_drop_beside_the_item`
- **Drag bytes.** `cargo test -- drag_escape_writes_nothing drag_printable_key_writes_nothing drag_modifier_then_release_writes_nothing drag_release_outside_writes_nothing drag_release_inside_moves_the_row plus_retargets_hover_onto_the_edited_table handle_paste_does_not_edit_while_drag_press_is_held replace_text_in_range_does_not_edit_while_drag_press_is_held editor_move_does_not_move_while_drag_press_is_held editor_backspace_does_not_edit_while_drag_press_is_held swallow_blocks_format_and_bound_keys_while_drag_press_is_held editor_mouse_up_clears_press_held handle_window_deactivated_clears_press_held mouse_move_with_no_button_clears_latch_and_allows_typing mouse_exit_with_button_held_swallows_typing mouse_exit_held_then_reentry_move_held_still_swallows mouse_exit_held_then_no_button_move_allows_typing mouse_exit_with_no_button_clears_latch mouse_move_with_button_pressed_keeps_latch_and_swallows_typing handle_cut_does_not_edit_while_drag_press_is_held handle_insert_table_size_does_not_edit_while_drag_press_is_held handle_table_edit_action_does_not_edit_while_drag_press_is_held chrome_align_clears_press_held chrome_plus_clears_press_held plus_retargets_hover_onto_the_edited_table column_pill_does_not_hit_content_above_the_table`
- **Act position.** The table-ops gate includes `act-insert-col-right`, `act-sel-insert-col-right`, `act-move-row`, and `act-align-left`. The caret is not the byte the op uses. `.cursor/skills/verify-litemark/scripts/table-ops-gate.sh`
- **Kit refusal.** `.cursor/skills/verify-litemark/scripts/refuse-gui.sh` exits 2. That script is a hard-coded refusal. It is not evidence that the window cannot start.
- **Headless window.** The window runs on Xvfb with `VK_ICD_FILENAMES=/opt/google/chrome/vk_swiftshader_icd.json` and a temporary `HOME`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, and `XDG_DOCUMENTS_DIR`. Pass the note path as `argv[1]`. Do not launch into `dirs::document_dir()/Notes`.

## Gotchas

- A PDF tab and Raw view draw no chrome.
- Hover does not move the caret. A click or a drop does, by the table-op caret rule, and only when the selection is empty.
- The header row's grip does not start a drag. Plus on that row inserts the first body row.
- A table inside a block quote or a list is not a table in this parser, so it has no chrome.
- `src/main.rs` reads `args[1]` as a note to open.
