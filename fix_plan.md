# LiteMark Fix & Optimization Plan

- **Version / Baseline:** `v0.13.0` (`2026-11-21`)
- **Test Baseline:** `cargo test` — `789 passed; 0 failed; 7 ignored`
- **Lint Baseline:** `cargo clippy --all-targets` — 1 test-only warning (`src/services/fs_service.rs:2089`, `clippy::permissions_set_readonly_false`)

## Priority Legend

- **P0 — Critical:** Process crash / abort (`panic = "abort"` in release) or data loss
- **P1 — High:** Broken core editing/navigation/rendering behavior, leaks, or severe latency/RAM wins
- **P2 — Medium:** Edge-case bugs, exporter/parser mismatches, or meaningful CPU/RAM improvements
- **P3 — Low / Architectural:** Minor edge cases, polish, or larger structural refactors for multi-MB scale

---

## 1. Image & Picture Features (Bugs & Edge Cases)

### P0 — Critical

- [ ] **Fix out-of-bounds abort when an animated GIF/WebP shares a `line_idx` or thumbnail slot with a static/shorter image**
  - **Where:** `src/ui/text_editor.rs:2569-2570`, `src/ui/text_editor.rs:2787-2788` (`EditorElement::prepaint`)
  - **Cause:** `img(...).id(("note-image", line_idx))` and `("note-thumb", line_idx * 4096 + k.min(4095))` omit the image path/tab/stamp. GPUI `0.2.2` persists `ImgState { frame_index }` by `ElementId`; when a static or shorter image later occupies that slot (tab switch, line insert, row edit, disk replace), GPUI skips the multi-frame check and indexes `self.data[frame_index]`, panicking out-of-bounds.
  - **Fix:** Hash `(line_idx, k, path, stamp_or_gen)` (incrementing generation on disk `changed`) into the `ElementId`.

### P1 — High

- [ ] **Fix unbounded GPUI `RenderImage` retention and GPU `sprite_atlas` texture leak**
  - **Where:** `src/ui/text_editor.rs:2535-2538`, `2569`, `2736-2739`, `2787`, `3327`; `src/ui/note_images.rs:70-74`
  - **Cause:** `img(path.clone())` uses GPUI's unbounded `App::loading_assets` cache, never evicting on scroll, delete, or tab close; disk replacement calls `remove_asset(cx)` without `cx.drop_image(image, Some(window))`, leaking GPU atlas memory.
  - **Fix:** Implement via Category 4's bounded LRU downscaling `gpui::ImageCache` with explicit `cx.drop_image` on eviction, reload, and tab close.

- [ ] **Prevent caret from getting trapped inside picture/thumbnail blocks**
  - **Where:** `src/model/document/picture.rs:96-163, 388-419, 470-481`; `src/model/document.rs:815-832, 985-1035`; `src/model/document/navigation.rs:262-287`
  - **Cause:**
    1. `set_picture_width`, `set_picture_size_at`, and `remove_row_image` don't set `next_undo_sel` / `next_redo_sel`, so Undo/Redo on thumbnail rows places a collapsed caret inside the row range.
    2. Toggling Raw → WYSIWYG leaves a caret inside `![alt](path)` without snapping to block edges.
    3. Find/Replace (`select_match_in`) selects inside `![alt](path)` in WYSIWYG without enabling `picture_raw_line`.
  - **Fix:** Record `next_undo_sel` / `next_redo_sel` on picture edits; extend `snap_caret_to_visible_line_end` to snap collapsed carets inside WYSIWYG `picture_block_at` to the nearest block edge; enable `picture_raw_line` when a Find match lands inside a WYSIWYG picture block.

- [ ] **Stop tall pictures from scrolling off-screen when clicked or navigated onto**
  - **Where:** `src/app/scroll.rs:85-93` (`follow_caret`), `src/app/scroll.rs:126` (`scroll_caret_to_top`)
  - **Cause:** `margin = row.line_height` uses the picture's full height (up to `492px+`), inverting `(view.top() + margin, view.bottom() - margin)` when `row.line_height >= view.size.height / 2.0` and violently scrolling the selected picture off the top.
  - **Fix:** Use `paint.line_height` (single text line height) for `margin` and `scroll_caret_to_top`, and align tall rows (`row.line_height > view.size.height - 2 * margin`) to `view.top() + margin` only when out of view.

- [ ] **Fix `Ctrl+Left` / `Ctrl+Right` getting permanently stuck at picture blocks in WYSIWYG**
  - **Where:** `src/model/document/navigation.rs:427-456` (`move_word_left`, `move_word_right`), `498-519` (`place`)
  - **Cause:** `word_left`/`word_right` steps 1–2 bytes into the picture Markdown and `place` snaps back to the starting edge.
  - **Fix:** In WYSIWYG mode, check `picture_block_at` on origin/target and jump directly to `block.start` (left) or `block.end` (right).

- [ ] **Prevent clicking the Status Bar from hitting scrolled-off pictures, resize handles, or hidden text rows underneath**
  - **Where:** `src/ui/status_bar.rs:54-161`; `src/app/editor_events.rs:326-345, 415-525`; `src/ui/text_editor.rs:2558, 2682, 2778, 2871, 3497`
  - **Cause:** `render_status_bar` doesn't stop mouse-down propagation, and `handle_picture_handle_press` / `editor_mouse_down` don't gate on `self.editor_scroll.bounds().contains(&event.position)` while `paint.pictures` / `paint.picture_handle` store unclipped window coordinates.
  - **Fix:** Stop `MouseDown` propagation on `render_status_bar` and gate editor mouse-down/handle/right-click handlers on `self.editor_scroll.bounds().contains(&event.position)` (and clip recorded hit targets to `window.content_mask().bounds`).

### P2 — Medium & P3 — Low

- [ ] **Trigger height-correction redraw even for remote-only or unsaved-note images** (`P2`)
  - **Where:** `src/ui/text_editor.rs:3542-3555` (`EditorElement::paint`)
  - **Fix:** Remove the `paint.images.has_entries()` gate (or gate on `!paint.pictures.is_empty()`), preventing clipped document bottoms when only `ImageTarget::Remote` / `Unavailable` exist.

- [ ] **Keep `find_hits`, input guards, and drag cursor consistent across picture resize & mode switch** (`P2`)
  - **Where:** `src/app/editor_events.rs:32-49, 272-288`; `src/app/commands.rs:761-777, 945-957`
  - **Fix:** Call `self.refresh_find()` after drag-resize (`editor_mouse_up`) and F3 (`editor_cycle_picture_size`); add `field_owns_keys` / `block_table_drag_edit()` checks to F2/F3 handlers; call `self.cancel_image_drag()` on mode switches.

- [ ] **Fix right-click and `Shift+Click` selection behavior on pictures & thumbnail rows** (`P2`)
  - **Where:** `src/app/editor_events.rs:491-521` (`editor_mouse_down`), `613-634` (`editor_right_mouse_down`)
  - **Fix:** Hit-test `paint.pictures` directly in `editor_right_mouse_down` so right-clicking a selected non-first thumbnail doesn't deselect it; in `Shift+Click` from above (`anchor < block.start`), extend selection to `block.end`.

- [ ] **Use magic-byte format sniffing in `read_image_size` and fix `svg_size` attribute parsing** (`P2`)
  - **Where:** `src/ui/note_images.rs:96-160` (`read_image_size`, `svg_size`)
  - **Fix:** Replace `image::image_dimensions(path)` with `image::ImageReader::open(path).ok()?.with_guessed_format().ok()?.into_dimensions().ok()?`; in `svg_size`, check `before_ok` before requiring quotes and skip unquoted earlier attributes (e.g., `stroke-width=2`) + check leading whitespace before `viewBox`.

- [ ] **Fix `resolve_image_target` off-screen `exists()` I/O and Windows drive-relative / `file:` edge cases** (`P2`)
  - **Where:** `src/markdown/image.rs:274-319` (`resolve_image_target`), `358-388` (`file_url_path`)
  - **Fix:** Defer `decoded` vs `literal` disk fallback to `ImageSizeCache` (not every frame in `prepaint`), check `literal` fallback for absolute paths too, reject Windows drive-relative `X:foo.png` paths, and require `file:` URLs to resolve to absolute/rooted paths.

- [ ] **Fix Khmer / combining-mark wrapping splitting the first cluster after its base consonant** (`P2`)
  - **Where:** `src/model/text_ops.rs:1452-1461` (`push_wrapped`)
  - **Fix:** Change `while at > index + 1` to `while at > index` (and avoid cutting mid-cluster when an entire cluster exceeds `wrap_cols`).

- [ ] **Show true raw Markdown when editing a picture inline via F2 / double-click** (`P2`)
  - **Where:** `src/ui/text_editor.rs:2487-2494` (`EditorElement::prepaint`)
  - **Fix:** When `editing_raw` is `true`, shape `line_text` directly with an identity `RowOffsetMap` so `\` escapes and inline delimiters aren't hidden.

- [ ] **Fix image paste/drop edge cases (`own_lines_with`, `Ctrl+A` blank line, table cells, async guard)** (`P2`)
  - **Where:** `src/app/note_assets.rs:77-128, 338-355`; `src/app/commands.rs:1036-1053`
  - **Fix:** Check `ends_with(['\n', '\r'])` in `own_lines_with`; avoid emitting a leading blank line when `Ctrl+A` replaces an entire block line; handle table-cell and single-thumbnail replacements cleanly; re-check `tab.file_type == FileType::Markdown && !tab.is_read_only()` after background encoding completes.

- [ ] **Clip small placeholder text to `block_bounds` & ignore double-clicks in `handle_picture_handle_press`** (`P3`)
  - **Where:** `src/ui/text_editor.rs:2622-2653, 2823-2859`; `src/app/editor_events.rs:326-345`
  - **Fix:** Paint placeholder `decor_lines` inside `window.with_content_mask(Some(ContentMask { bounds: block_bounds }), ...)` (and align corner-drag preview on unsized placeholders with final height); return `false` from `handle_picture_handle_press` when `event.click_count != 1`.

- [ ] **Fix HTML & plaintext export bugs for images** (`P2`)
  - **Where:** `src/markdown/exporter.rs:135-189` (`with_image_widths`), `218-258` (`export_to_html_in`), `286-306` (`export_to_plaintext`)
  - **Fix:** Add `img { max-width: 100%; height: auto; }` to HTML CSS; percent-encode non-`data:` `dest_url` values in `with_image_widths`; use a stack instead of `open: Option<...>` for nested images; buffer alt text per image in `export_to_plaintext` and strip `width_suffix` once on `TagEnd::Image`.

---

## 2. Markdown Parser & Raw <-> Preview (WYSIWYG) Pipeline

*(Verified clean: `ParsedDocument::apply_edit` in `src/markdown/parser.rs:1776-1895` maintains checkpoint, table, lookahead, and offset parity with full parses.)*

### P1 — High

- [ ] **Fix `visible_line_end` trapping the caret inside trailing formatting/links on headings**
  - **Where:** `src/model/document/navigation.rs:262-287, 546-552`; `src/app/commands.rs:290-295`; `src/app/editor_events.rs:686`
  - **Cause:** `visible_line_end` uses `display_to_source(display_text.len())` on headings, stopping before trailing `**`, `` ` ``, `~~`, `</u>`, `](url)` instead of only before closing ` ##`. Next frame's `active_tab_mut()` → `set_view_mode` → `snap_caret_to_visible_line_end` pulls the caret back inside `# **bold**` or `# [link](url)`.
  - **Fix:** Store the heading's actual content end (from `atx_display_content_range` before any closing-hash run) on `LineKind::Heading` / `ParsedLine` and return `line.source.start + content_end` from `visible_line_end`.

- [ ] **Include leading, trailing, and wrap-boundary hidden delimiters in soft-wrapped `sub_map` bounds**
  - **Where:** `src/markdown/parser.rs:628-685` (`RowOffsetMap::slice_display`, `slice_bounds`); `src/ui/text_editor.rs:796-814, 2985-2989, 3276-3282`
  - **Cause:** `slice_display` sets `line_start`/`line_end` to the first/last *visible* segment, excluding leading `# `/`>`/`**`, trailing `**`/`](url)`, and hidden delimiters at internal wrap splits — causing the caret to vanish and `move_vertical_rows` to miss the row.
  - **Fix:** Clamp sub-row 0's `byte_start` / `sub_map.line_start` to `parsed_line.source.start`, the last sub-row's `byte_end` / `sub_map.line_end` to `parsed_line.source.end`, and keep `row[k].byte_end == row[k + 1].byte_start` across adjacent wrapped rows.

- [ ] **Use cached `tab.parsed()` instead of single-line `block_prefix_len(line)` in `replace_text_in_range`**
  - **Where:** `src/app/input_handler.rs:98-109`; `src/model/document/blocks.rs:559-600`
  - **Cause:** Parsing a single line in isolation treats `# comment` inside a fenced code block as an H1, `2. item` inside a paragraph continuation as a list, and misses `#\tHeading` / extra spaces.
  - **Fix:** Read `prefix_range.end` from `tab.parsed().line_at_offset(cursor)` (or `block_prefix_in_parsed`).

- [ ] **Advance `link_stack` `inner_end` on nested inline closers and handle linked images**
  - **Where:** `src/markdown/parser.rs:1038-1138` (`parse_inline_with_pulldown`)
  - **Cause:** `link_stack` only advances `inner_end` on `Event::Text`/`Code`, not `End(Strong/Emphasis/Strikethrough)` or `InlineHtml("</u>")`, corrupting `[**bold**](url)` on `Ctrl+K` unwrap/Backspace, and ignores `Start(Image)`/`End(Image)` inside `[![alt](img.png)](url)`.
  - **Fix:** Update `last_link.2 = last_link.2.max(abs_end)` on nested inline closers and track nested `Image` tags.

- [ ] **Stop `prepare_for_pulldown` from mutating characters inside link destinations**
  - **Where:** `src/markdown/parser.rs:893-953, 1018-1022, 1106-1108`
  - **Cause:** Replacing bytes adjacent to mixed delimiters (`_*`, `*_`, `~_`, etc.) with `'.'` mutates URLs inside `[text](https://example.com/foo_*bar)` and corrupts `link_hits` / `dest_url`.
  - **Fix:** Skip link destinations (`(...)` / `<...>`) and code spans in `prepare_for_pulldown`, or extract `dest_url` from the unmutated original `input` slice.

- [ ] **Replace hardcoded `+ 1` / `- 1` and `.find('\n')` with line-ending-aware helpers (`\r\n` / `\r`)**
  - **Where:** `src/model/document/table.rs:436-447` (`snap_wysiwyg_table_delimiter`); `src/model/document/navigation.rs:98, 110, 194, 330-332` (`move_left_in_mode`, `move_right_in_mode`, `move_end`)
  - **Cause:** Hardcoded `± 1` lands inside `\r\n`, causing vanishing carets, sticky table exits, and split `\r\n` selections; header-only tables at EOF leave the caret on the hidden `TableDelimiter` line.
  - **Fix:** Use `line_ending_len`, `line_ending_len_before`, and `text_ops::line_end`; on a header-only table at EOF, snap `snap_wysiwyg_table_delimiter` to the header row's first cell.

- [ ] **Guard over-typing and selection auto-wrap against WYSIWYG hidden delimiters**
  - **Where:** `src/app/input_handler.rs:147-218` (`replace_text_in_range`)
  - **Cause:** Typing `]` at the end of `[hello](url)` steps into hidden `](url)`; typing `` ` `` at the end of `` ``foo`` `` splits the closer; auto-wrapping a selection spanning `**` traps hidden delimiters.
  - **Fix:** Suppress over-typing when the cursor sits at a hidden delimiter / `Code` / `Link` closer in WYSIWYG, and route selections overlapping inline pairs safely through `insert_text_in_mode` / `replace_wysiwyg_selection`.

### P2 — Medium & P3 — Low

- [ ] **Centralize Raw <-> WYSIWYG & tab-switch state resets and sync `follow_caret` / `total_height`** (`P2`)
  - **Where:** `src/app/commands.rs:312-326, 761-777, 945-957`; `src/app/scroll.rs:61-104`; `src/ui/text_editor.rs:1322-1350, 3481-3488, 3542-3555`
  - **Fix:** Share one mode-switch helper for `handle_toggle_mode` and `handle_set_view_mode` that clears `table_hover`/`table_hot`/`table_pressed`/`table_drag`/`image_drag`/`editor_context_menu`/`pending_formats`/IME state; snap off `TableDelimiter` & picture interiors on entry to WYSIWYG (and on tab switch); include `ViewMode` in `caret_followed` and `EditorPaint::painted_view_mode`; reset `paint.total_height` on mode/tab change.

- [ ] **Prevent Raw-mode IME composition underline from leaking onto earlier tab-expanded lines** (`P2`)
  - **Where:** `src/ui/text_editor.rs:1573-1587` (`EditorElement::prepaint`)
  - **Fix:** Change `.map(...)` to `.and_then(|m| if m.end <= start || m.start >= end { None } else { Some(...) })`.

- [ ] **Prevent `synthesize_flanking_pairs` from crossing existing pairs or styling HTML comments / inline image URLs** (`P2`)
  - **Where:** `src/markdown/parser.rs:1325-1584`
  - **Fix:** Add HTML tag/comment and inline image byte ranges to `blocked` and reject candidate pairs that partially overlap existing `InlinePair` bounds.

- [ ] **Do not underline text after an unclosed `<u>`** (`P2`)
  - **Where:** `src/markdown/parser.rs:1120-1138, 1168-1183`
  - **Fix:** Clear `underline` on segments at or after the earliest unclosed `<u>` when `!underline_stack.is_empty()` at end-of-line.

- [ ] **Align parser edge cases with `exporter.rs` (CommonMark / GFM)** (`P2`–`P3`)
  - **Where:** `src/markdown/parser/tables.rs:60-71, 150-183`; `src/markdown/parser/blocks.rs:24-34, 228-300`; `src/markdown/parser.rs:2310`; `src/markdown/image.rs:54-59`
  - **Items:**
    - [ ] Align `is_delimiter_cell` (allow `\t` whitespace, reject spaces between `:` and `-`).
    - [ ] Align `exact_thematic_break` with `is_thematic_break` (`----`, `- - -`).
    - [ ] Align `atx_heading_within` tabs around closing `#` and reject leading `\t` / `\u{00A0}` before `#`.
    - [ ] Align GFM table pipe-splitting / `\|` unescaping inside code spans and prevent block-level triggers (fences/comments) inside `parse_inline_syntax` for table cells.
    - [ ] Reject unescaped `<` inside `<...>` image destinations (`image.rs:54-59`).
    - [ ] *(Architectural / Backlog)* Decide whether to support Setext headings, nested block containers (`> # Title`, `> >`), reference links, footnotes, multi-line inline spans, and HTML entities in WYSIWYG.

- [ ] **Fix low-severity parser / input edge cases** (`P3`)
  - [ ] Task checkbox click on `- [\t] task` (`src/app/editor_events.rs:429-435`; expose `checkbox_click_replacement` in `src/markdown/parser/lists.rs:384-396` and use `.get(range)`).
  - [ ] Normalize reversed UTF-16 ranges in `EntityInputHandler` (`src/app/input_handler.rs:15-22, 156, 256-288`).
  - [ ] Clamp `RowOffsetMap::slice_display` / `slice_bounds` safely when `d.len() != s.len()` (`src/markdown/parser.rs:628-685`).
  - [ ] Materialize missing `|` separators when editing padded cells in short table rows (`src/markdown/parser.rs:2316-2319`).
  - [ ] Fix test-only `clippy::permissions_set_readonly_false` warning (`src/services/fs_service.rs:2089`).

---

## 3. CPU, Rendering & I/O Performance Optimizations

### Phase 3A — Quick, Low-Risk High-ROI Wins

- [ ] **Skip `occupied_content(&paint.rows)` scan when there are no tables (or no table hover/drag)**
  - **Where:** `src/app/editor_events.rs:144, 249`; `src/ui/text_editor.rs:3516`; `src/ui/table_chrome.rs:132-146`
  - **Fix:** Early-return when `paint.tables.is_empty()` (and in `paint` when `table_hover.is_none() && table_drag.is_none()`).

- [ ] **Short-circuit `DocumentTab::set_view_mode` when `self.view_mode == mode`**
  - **Where:** `src/model/document/navigation.rs:262-266`; `src/app/commands.rs:290-295`
  - **Fix:** Add `if self.view_mode == mode { return; }` to eliminate redundant `snap_caret_to_visible_line_end` + `refresh_cursor` on every frame, keystroke, and drag-move.

- [ ] **Eliminate per-frame full-file `logical_line_number` scans in Raw / code / plain-text mode**
  - **Where:** `src/ui/text_editor.rs:1448-1452, 1524-1525`
  - **Fix:** Pass `tab.line_count` and `tab.cursor_line` directly into `EditorElement`.

- [ ] **Eliminate double `disk_change_for` full-file read + hash on `Ctrl+S`**
  - **Where:** `src/app/commands.rs:365, 375`; `src/app/tabs.rs:63`
  - **Fix:** Pass `overwrite: true` (or reuse the check result) into `save_tab_with` after `handle_save` verified `DiskChange::Unchanged`.

- [ ] **Remove unused `entry.metadata()` syscall in workspace scan & coalesce bulk-trash rescans**
  - **Where:** `src/services/fs_service.rs:1157-1163` (`WorkspaceFile::updated_at` is never read); `src/app/tabs.rs:145`; `src/app/file_ops.rs:574`
  - **Fix:** Remove `updated_at` `metadata()` stat calls (2× speedup on large scans) and rescan only once at the end of bulk trash.

- [ ] **Broaden `is_plain_inline` fast path & skip unnecessary inline/table scanning**
  - **Where:** `src/markdown/parser.rs:992-1016, 1189`; `src/markdown/parser/tables.rs:86-125, 185-272`
  - **Fix:** Allow trailing spaces and leading digits/quotes/punctuation in `is_plain_inline` when no inline syntax trigger bytes (`* _ ~ \` [ ] < > \ & ! |`) appear; guard `synthesize_flanking_pairs` with `memchr3(b'*', b'~', b'`', ...)`; skip `code_span_ranges` in `tables.rs` when no backtick is present and avoid `Vec` allocs in `is_empty_table_row`.

- [ ] **Binary-search `paint.rows` (`partition_point`) across hit-test, nav, scroll, and IME + gate `link_for_offset`**
  - **Where:** `src/ui/text_editor.rs:644-1158`; `src/app/scroll.rs:76-146`; `src/app/input_handler.rs:303-307`; `src/app/editor_events.rs:446`
  - **Fix:** Replace linear `paint.rows` scans with `partition_point`; only run `link_for_offset` in `editor_mouse_down` when Ctrl/Cmd is held in WYSIWYG mode.

- [ ] **Trim per-frame overhead in `LiteMarkApp::render` and `EditorElement`**
  - **Where:** `src/app/render.rs:27, 64-1005, 1397-1413, 2188-2228`; `src/app/outline.rs:72`; `src/ui/outline.rs:49-189`; `src/ui/sidebar.rs:402-414`; `src/ui/text_editor.rs:1383-1396, 3340-3345`; `src/model/document.rs:295-301, 985-1019`; `src/main.rs:379-397`
  - **Items:**
    - [ ] Move `sync_tab_settings()` out of `render()` to actual tab mutation sites.
    - [ ] Sync `LineField` colors in `sync_frame_state` only when the resolved theme changes.
    - [ ] Pass `Arc<Vec<OutlineItem>>` without `.to_vec()`, skip `card_width` when collapsed, and virtualize expanded outline with `uniform_list`.
    - [ ] Cache sidebar search results in `SidebarTreeCache` keyed by `(workspace_generation, search_query)`.
    - [ ] Lazily construct menu/modal/overlay closures only inside active `.when(...)` / `if let` branches, and guard no-op `cx.notify()` / redundant `window.refresh()`.
    - [ ] Insert cursor `Hitbox`es only for visible pictures inside `band`, and cache `char_width` (`"0"`) inside `LayoutCache` under `widths_key`.
    - [ ] Replace `window_line_endings` heap `Vec<u8>` allocs with boundary-checked `count_line_endings`; avoid clearing `staged_stats` / double `refresh_cursor` in undo/redo/typing.
    - [ ] Replace the 250ms polling timer in `main.rs:379-397` with event-driven wakeup from `single_instance`.

### Phase 3B — Medium-Effort High-Impact Speedups

- [ ] **Move `ImageSizeCache` disk `stat`/header I/O and `parse_image_line`/`row` off the render hot path**
  - **Where:** `src/ui/note_images.rs:44-113`; `src/ui/text_editor.rs:2490-2538, 2711-2740`
  - **Fix:** Run `stamp_of` and `read_image_size` on `cx.background_executor()`, return `None` for off-screen cache misses, throttle visible `stat` rechecks (e.g. 1–2s TTL or window-activation), and cache parsed image metadata instead of re-parsing every frame.

- [ ] **Move `scan_workspace_dir`, `atomic_save`, `save_settings`, and `read_note` off the UI thread**
  - **Where:** `src/app/sidebar.rs:347-363`; `src/app/tabs.rs:61-161, 343-409`; `src/app.rs:899-911`; `src/services/fs_service.rs:415-477, 1193-1267`
  - **Fix:** Dispatch on `cx.background_executor()` with generation/revision guards so UI never blocks on disk or `fsync`; apply incremental workspace updates on single-file create/rename/move.

- [ ] **Cache `Rc<ShapedLine>` across frames in `LayoutCache` (and wrapped cells in `TableWidthCache`)**
  - **Where:** `src/ui/text_editor.rs:1560-1687, 2150-2227, 3121-3175`
  - **Fix:** Store `Rc<ShapedLine>` per `(line_idx, sub_idx)` keyed by `(wrap_cols, theme_key, marked_overlap)` and splice alongside `LayoutCache::lines` on `last_splice`.

- [ ] **Make `push_wrapped` zero-allocation and add per-line Raw-mode wrap caching**
  - **Where:** `src/model/text_ops.rs:1187-1261, 1391-1471`; `src/ui/text_editor.rs:294-313`
  - **Fix:** Stream `char_indices()` in `push_wrapped` without `Vec<(usize, char)>`, count rows directly in `visual_row_count_measured`, and cache Raw-mode wrapped rows per logical line incrementally.

- [ ] **Make `find_hits` incremental on edit and optimize Quick Open (`Ctrl+P` / `Ctrl+Shift+F`)**
  - **Where:** `src/app/find.rs:6-43`; `src/model/text_ops.rs:939-1085`; `src/app/quick_open.rs:147-159, 319-325`; `src/model/quick_open.rs:145-381`
  - **Fix:** Shift/re-search only the edited window `(edit_start - term)..(edit_end + term)` in `find_hits`; ASCII fast-path `find_matches` and use `partition_point` in `find_next`/`find_prev`; pre-fold Quick Open names, avoid per-line `term.to_lowercase()`, use `select_nth_unstable_by` for top-50, and parallelize content search on the background executor.

### Phase 3C — Larger Architectural Upgrades (Multi-MB Scale)

- [ ] **O(log N) height index in `LayoutCache` + visible-only `PaintRow`s** (`P3`)
  - **Where:** `src/ui/text_editor.rs:1322-3316`
  - **Fix:** Maintain a Fenwick/chunked prefix-sum of line heights updated on `last_splice` so `request_layout` is O(1) and `prepaint` jumps straight to `band.top` without scanning or allocating `PaintRow`s for off-screen lines.

- [ ] **Line-relative offsets in `ParsedLine` + tighter checkpoint interval** (`P3`)
  - **Where:** `src/markdown/parser.rs:1718-1922`
  - **Fix:** Store intra-line ranges relative to `line.source.start` (eliminating `O(N)` tail walks and `Arc::make_mut` clones on every keystroke) and lower `CHECKPOINT_EVERY` from `64` to `16`.

- [ ] **Split `LiteMarkApp` hot UI panels into independent GPUI entities** (`P3`)
  - **Where:** `src/app/render.rs:23-2083`
  - **Fix:** Separate Editor surface, Sidebar, Header/Toolbar, StatusBar, and Outline entities so typing/scrolling doesn't rebuild the entire window tree.

- [ ] **Evaluate a Rope / Piece-Table backing store for `DocumentTab::content`** (`P3`)
  - **Where:** `src/model/document.rs:343, 719`; `src/model/text_ops.rs:238, 274`
  - **Fix:** Replace contiguous `String` (`O(N)` memmove on edits near start of multi-MB files) with `ropey::Rope` (`O(log N)` edits + `O(1)` snapshots).

---

## 4. RAM / Memory Footprint Reduction (Same Functionality, Faster Speed)

### P1 — High Impact

- [ ] **Bounded LRU, background-downscaling GPUI `ImageCache` + `cx.drop_image` eviction (10×–80× RAM/VRAM cut per image)**
  - **Where:** `src/ui/text_editor.rs:2535-2605, 2736-2822`; `src/ui/note_images.rs`
  - **Cause:** Full-res 4000×3000 decode takes ~48 MB CPU + ~48 MB GPU atlas = ~96 MB forever, even for 160px thumbnails.
  - **Fix:** Attach a bounded LRU `gpui::ImageCache` (e.g. 64–128 MB pixel budget), downscale static rasters on `background_executor` (using `image = "0.25"`) to `<= 2×` display size, and call `cx.drop_image(img, Some(window))` on eviction, reload, and tab close.

- [ ] **Eliminate duplicate full-document `content_snapshot: SharedString` copies (saves 1×–2× doc size, up to 64 MB)**
  - **Where:** `src/app.rs:205`; `src/app/editor_events.rs:649-681`; `src/app/input_handler.rs:90`; `src/ui/text_editor.rs:221, 294-313`
  - **Cause:** Every keystroke copies the full document into `SharedString`, and `LayoutCache::raw_rows` retains the previous `SharedString` simultaneously (up to 3 full copies in RAM).
  - **Fix:** Have `EditorElement` read `&tab.content` via `self.entity.read(cx)`, borrow `&tab.content` directly in `replace_text_in_range`, and key `LayoutCache::raw_rows` by `(tab_id, revision)`.

- [ ] **Shrink `ParsedLine` (~400 B & 4 mallocs/line → ~96–112 B & 0 mallocs/plain line; ~3.5×–4× AST RAM reduction)**
  - **Where:** `src/markdown/parser.rs:362-715, 973-984, 1964-1978, 2209-2417`; `src/markdown/parser/blocks.rs:360-418`
  - **Items:**
    - [ ] Replace `display_text: String` with zero-copy `Option<Box<str>>` (`None` = `&content[prefix_end..source.end]`).
    - [ ] Replace `offset_map: Arc<RowOffsetMap>` with `Option<Arc<RowOffsetMap>>` (`None` = O(1) prefix arithmetic; eliminates 2 mallocs on every plain/empty/code line).
    - [ ] Shrink `LineKind` from `56 B` to `24 B` (pack `ListItem` fields to `u32`/`u16`/`u8`/sentinel, `Box<str>` for `CodeFence::language`).
    - [ ] Leave `styled_segments` empty on unstyled plain/code lines (`wysiwyg_slice_runs` already applies default styles), pack `StyledSegment` bools into `u8` + `Option<Arc<str>>`, and share `Arc<str>` with `link_hits`.
    - [ ] Replace `prefix_range: Range<usize>` (16 B) with `prefix_len: u32` (4 B) and `line_index: usize` with `u32`.

- [ ] **Cut table `ShapedLine` bloat, `LayoutCache::lines` mallocs, and `PaintRow` double-buffering**
  - **Where:** `src/ui/text_editor.rs:34-67, 97-107, 196-232, 1518, 1703-1843, 2121-2129, 2428-2466, 2930-2951, 3500`
  - **Items:**
    - [ ] Wrap `CellLine::shaped` (~3.1 KB inline!) in `Rc<ShapedLine>` and drop duplicate `PaintCell::shaped` / `offset_map` (>15× table memory cut).
    - [ ] Replace `LayoutCache::lines` `Option<Rc<LineRows>>` with `enum CachedRows { Uncached, Single { wrap_cols: u32 }, Wrapped(Rc<LineRows>) }` (0 mallocs for 1-row lines) and store `estimate` as `Vec<f32>` (`NaN` sentinel).
    - [ ] Move `PaintRow::checkbox_hit` (40 B/row) into flat `EditorPaint::checkbox_hits` and reuse `paint.rows` `Vec` capacity across frames instead of double-buffering.
    - [ ] Clear/shrink `TableWidthCache` and `layout_cache.table_keys` when switching to Raw mode or a table-free tab.

- [ ] **Index-based `SidebarTreeItem` (~50× smaller tree cache) & deduplicated workspace IDs (~65% smaller index)**
  - **Where:** `src/ui/sidebar.rs:139-229, 402-411`; `src/model/workspace.rs:33-49`; `src/services/fs_service.rs:1230-1255`
  - **Fix:** Store `{ index: u32, depth: u16 }` (8 B, 0 allocs) in `SidebarTreeItem` instead of deep-cloning `WorkspaceFolder`/`WorkspaceFile`; share `folder_id` via `Arc<str>` or compact IDs instead of repeating `"file-<abs_path>"` / `"folder-<abs_path>"` 3× per entry.

### P2 — Medium & P3 — Low Impact

- [ ] **Defer startup parsing in `DocumentTab::new`, lazy-load clean background tabs, and fix `reload` 2× spike** (`P2`)
  - **Where:** `src/app/tabs.rs:343-389`; `src/model/document.rs:410-509, 967-983`
  - **Fix:** Avoid full-doc `self.parsed()` at offset 0 in `DocumentTab::new` (especially for restored background tabs whose parse cache is dropped on frame 1); optionally lazy-load unmodified file-backed inactive tabs on first activation; drop old buffers before rebuilding in `reload`; drain UTF-8 BOM in-place; shrink `content` capacity after huge deletions.

- [ ] **Shrink `UndoHistory` struct size, fix cost accounting, and bound multi-tab / replace-all memory** (`P2`)
  - **Where:** `src/model/text_ops.rs:16, 48-93, 189, 214-219`; `src/model/document.rs:614-641, 864-873`
  - **Fix:** Box `(Option<SelectionSnap>, Option<SelectionSnap>)` in `DiffEdit` (152 B → ~80 B) and use `Box<str>` for sealed edits; include `size_of::<DiffEdit>()` in `cost()`; switch `undo` to `VecDeque` (O(1) `pop_front`); `shrink_to_fit()` `redo` on clear; consider a global undo cap across inactive tabs.

- [ ] **Eliminate transient multi-MB allocations across I/O, PDF, image paste, export, and Quick Open** (`P2`)
  - **Where:** `src/services/pdf_service.rs:26, 72-83, 112-136`; `src/services/fs_service.rs:466-477, 565-581`; `src/app/tabs.rs:99`; `src/app/note_assets.rs:104-106`; `src/app/commands.rs:511`; `src/app/quick_open.rs:147-152`
  - **Items:**
    - [ ] Stream-hash `disk_change` with a 64 KB buffer instead of `fs::read(path)`.
    - [ ] Use `tab.file_text_ref()` in `save_tab_with` (zero-copy when `!has_bom`).
    - [ ] Stream `copy_into_note_assets` via `io::copy` instead of `fs::read`.
    - [ ] Drop raw PDF `bytes` right after `Document::load_mem_with_options` (or `load_with_options`), write directly via `fmt::Write`, and trim `EXTRACT_STACK_BYTES` from 64 MB if safe.
    - [ ] Slice locally in `note_assets.rs:104` instead of cloning `tab.content` twice.
    - [ ] Clone export content in `handle_export` only *after* `pick_save_file` confirms a path.
    - [ ] Pass `&[WorkspaceFile]` into Quick Open matching instead of cloning `Vec<PathBuf>` every keystroke, and materialize only top-50 `NoteHit`s.

- [ ] **Minor RAM & binary footprint trims** (`P3`)
  - **Where:** `src/main.rs:22-24`; `src/app/render.rs:1306`; `Cargo.toml:9-46`
  - **Items:**
    - [ ] Add `#[exclude = "windows/*"]` and `#[exclude = "icons/hicolor/*"]` to `#[derive(RustEmbed)]` in `src/main.rs`.
    - [ ] Clear `self.menu_items_cache` when `self.open_menu` is `None`.
    - [ ] Benchmark `mimalloc` as `#[global_allocator]` for lower fragmentation and faster RSS release after closing large tabs.
    - [ ] Evaluate OS TLS / `native-tls` vs `ureq`'s `rustls` + `webpki-roots` static tables (used only for daily update checks).

---

## 5. Recommended Execution Milestones

1. **Milestone 1 — Crash & High-Severity Image / UX Fixes:** Animated GIF `ElementId` fix, status-bar click isolation, tall-picture scroll margin, `Ctrl+Left/Right` picture jump, caret-trap fixes (`set_view_mode` / Undo / Find), `follow_caret` & mode-switch state resets.
2. **Milestone 2 — Parser & WYSIWYG Parity Fixes:** `visible_line_end` heading content end, wrapped `sub_map` boundary clamping, `replace_text_in_range` parsed prefix + over-typing guards, `link_stack` + `prepare_for_pulldown` URL preservation, CRLF/CR navigation & table delimiter fixes, `exporter.rs` / GFM table alignment.
3. **Milestone 3 — Bounded Downscaling `ImageCache` & High-Impact RAM Cuts:** `gpui::ImageCache` with downscaling + `cx.drop_image`, removing `content_snapshot` copies, `ParsedLine` compaction, `Rc<ShapedLine>` in tables + `CachedRows` + `paint.rows` reuse, index-based `SidebarTreeItem`, deferred startup parse.
4. **Milestone 4 — Hot-Path CPU & Async I/O Wins:** `occupied_content` early return, `set_view_mode` short-circuit, Raw `line_count` pass-through, single-read `Ctrl+S`, remove `updated_at` `stat`, `partition_point` everywhere, streaming `push_wrapped`, background `ImageSizeCache` / workspace scan / save / read, `Rc<ShapedLine>` frame cache, incremental Find & faster Quick Open.
5. **Milestone 5 — Deep Multi-MB Structural Scaling:** Fenwick height index + visible-only `PaintRow`s, line-relative `ParsedLine` offsets, entity split for `LiteMarkApp` panels, `ropey` evaluation, `mimalloc` RSS benchmarking.

### Verification Gate (Per Milestone)
- `cargo test` (all unit & integration tests passing + new regression tests for every bug fixed)
- `cargo clippy --all-targets` (0 warnings)
- Manual UI checks: animated GIF + PNG tab/row swap, tall image click/scroll, `# **bold**` & `[**bold**](url)` editing, CRLF files, Raw ↔ WYSIWYG mode toggle, large-note typing/scrolling/RSS check
