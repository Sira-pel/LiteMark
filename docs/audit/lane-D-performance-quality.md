# Lane D: performance and code quality

Status: complete

Audited commit: `ea03a57` (`main`). Read-only audit: no tracked source was changed.

## Environment and measurement notes

- **GPUI-harness and editor-frame timings** (`idle_frame`, `keystroke_app`, `tables` GPUI frame numbers, `sidebar` GPUI frame numbers, `tabs` GPUI frame numbers, `bulk_ops`, `components`, `keystroke_model`, `lane_b3` 5 MB run, `lane_b4`): taken in a release build (`cargo test --release`, `rustc 1.97`, Linux x86_64, 4 cores) at `ea03a57` via a throwaway GPUI test module (`src/lane_d_bench.rs`) on `TestAppContext`. They are preserved in the **Raw benchmark output** section below. On the continuation VM (`rustc 1.99.1`, Linux x86_64, 2 cores), `libxkbcommon-x11-dev` is not installed, so GPUI binaries and `cargo test --no-run` cannot link (`rust-lld: unable to find library -lxkbcommon-x11`).
- **Previously skipped measurements and re-verifications** (startup workspace scan/seed/open, per-tab memory and `ParsedDocument` footprint, `release_parse_cache`, undo eviction, B3 vs B4 scaling, 20,000-row table parsing, legacy parser drift, UI-thread blocking inventory, dead-code & duplication audit): executed directly against the unmodified `src/markdown`, `src/services`, and `src/model` modules at `ea03a57` in `--release` (`rustc 1.99.1`) using a standalone non-GPUI harness crate, and verified by reading the GPUI/app code (`src/ui/sidebar.rs`, `src/ui/text_editor.rs`, `src/app/*.rs`). The non-GPUI benchmark code is included in **Reproducible non-GPUI benchmark code** below.

## Findings

### D1. Sidebar rebuilds its tree on every frame and renders every row without virtualization
- Severity: Medium
- Category: Performance
- Location: `src/ui/sidebar.rs:143-223` (`build_sidebar_tree`), `src/ui/sidebar.rs:368-384` (unconditional rebuild per render), `src/ui/sidebar.rs:614` (`.children(tree_items.into_iter().map(...))`); gated by `self.show_sidebar` at `src/app/render.rs:1374`.
- Problem: When the sidebar is visible, every UI frame rebuilds the entire flat tree: scanning folders/files per parent level, sorting every level with `to_lowercase()` allocations (`:178, :203`), and cloning every `WorkspaceFolder` and `WorkspaceFile`. It then materializes GPUI element trees for every visible row inside `.children(...)` instead of using a virtualized list (`uniform_list`). When collapsed, the sidebar is skipped and frame time drops back to ~1 ms.
- Evidence: PROVEN in the `ea03a57` release GPUI harness and verified in `src/ui/sidebar.rs`:
  ```text
  20 folders / 2,000 files: build_sidebar_tree=  1.33 ms | idle_frame=247.8 ms (no-sidebar-rows baseline 0.83 ms) | frame_all_selected= 644.3 ms
  50 folders / 5,000 files: build_sidebar_tree=  9.36 ms | idle_frame=635.0 ms (no-sidebar-rows baseline 0.99 ms) | frame_all_selected=2856.2 ms
  200 folders/ 5,000 files: build_sidebar_tree= 21.31 ms | idle_frame=793.7 ms (no-sidebar-rows baseline 1.12 ms) | frame_all_selected=3729.2 ms
  ```
- Suggested fix: Cache the flattened/filtered `Vec<SidebarTreeItem>` keyed by workspace/expansion/search revision, and render rows with GPUI `uniform_list` so only visible viewport rows allocate elements and paint.

### D2. Large tables shape and lay out every row and cell on every frame and keystroke inside the table
- Severity: Medium
- Category: Performance
- Location: `src/ui/text_editor.rs:1325-1650` (`table_width_key` `:1328`, `shape_table` `:1389-1413`, `compute_table_column_widths` `:1449-1483`, cell wrap/shaping loop `:1508-1600`, `PaintCell` `:35-40, 1785-1797`).
- Problem: Off-screen tables can skip shaping when heights are cached, **except** when any caret/anchor is within two lines of the table (`:1410-1412`: `anchor_lines.iter().any(|&a| a + 2 >= first && a <= last + 2)`). Placing the caret anywhere inside a large table forces `shape_table = true` for **all rows of that table**, with no row-level viewport culling inside the table. Worse, on every frame `table_width_key` hashes the entire raw slice of the table, and whenever the cache misses (first frame, resize, or **every keystroke inside the table**), `compute_table_column_widths` shapes every cell (`:1449-1478`) and `:1550-1587` shapes every cell again (`full_shaped` + `shaped_sub` = 2x–3x text shaping across the whole table). Additionally, `PaintCell` (`:35-40`) carries three `#[allow(dead_code)]` fields (`text_origin`, `shaped: ShapedLine`, `offset_map: RowOffsetMap`) that are still populated/cloned at `:1785-1797` alongside `lines: Vec<CellLine>`.
- Evidence: PROVEN in the `ea03a57` release GPUI harness and verified in `src/ui/text_editor.rs`:
  ```text
  500x8  ( 54 KB): model key_in_cell= 0.78 ms | app frame_after_key_in_cell= 24.7 ms | idle_frame= 25.5 ms
  100x40 ( 54 KB): model key_in_cell= 0.89 ms | app frame_after_key_in_cell=117.2 ms | idle_frame=118.7 ms
  2000x8 (229 KB): model key_in_cell= 3.22 ms | app frame_after_key_in_cell=115.4 ms | idle_frame=117.0 ms
  5000x8 (587 KB): model key_in_cell=11.37 ms | app frame_after_key_in_cell=323.7 ms | idle_frame=319.6 ms
  ```
- Suggested fix: Track uniform or cached per-row heights and cull table rows by `band.contains(cur_y, row_height)` just like ordinary lines even when the caret is inside the table; keep `column_widths` cached across intra-cell edits whose measured width does not grow or shrink column maxima; remove the dead fields from `PaintCell`.

### D3. Block-format toggle (`ol`, `quote`, `heading`) over a large selection is O(n²) (`accepted_list_marker_in`)
- Severity: Medium
- Category: Performance (upgraded from Lane B's Low after release timings)
- Location: `src/markdown/parser/lists.rs:357-368` (`accepted_list_marker_in` runs `doc.lines.iter().find(...)`), called per selected line from `src/model/document/blocks.rs:176` (`toggle_block_lines`) and `:496-520` (`parse_line_block_prefix`).
- Problem: Toggling an ordered list, blockquote, or heading over a multi-line selection scans `doc.lines` from index `0` for every line in the selection (2–3 linear scans per line). On a realistic 1 MB document, Select All + `ol`/`quote`/`h2` blocks the UI thread for **12.4 seconds**; at 5 MB it blocks for **~5.4 minutes**. By contrast, Select All + `bold` stays linear (`210 ms` at 1 MB).
- Evidence: PROVEN in `ea03a57` release (`cargo test --release`) and re-verified on VM release harness:
  - Realistic note (`ea03a57` release):
    - `10 KB` (255 lines): `ol` 2.4 ms, `quote` 2.2 ms, `h2` 2.3 ms (vs `bold` 2.9 ms)
    - `100 KB` (2,526 lines): `ol` 129.6 ms, `quote` 116.2 ms, `h2` 116.0 ms (vs `bold` 17.5 ms)
    - `1 MB` (25,478 lines): `ol` **12,383.5 ms**, `quote` **12,520.7 ms**, `h2` **12,423.3 ms** (vs `bold` 209.9 ms)
    - `5 MB` (126,383 lines): `ol` **322,473 ms**, `quote` **326,145 ms**, `h2` **326,002 ms** (vs `bold` 1,142.8 ms)
  - VM release re-verification (`"1. a\n"` x N): `2,000` lines = `39.70 ms`, `4,000` = `107.28 ms`, `8,000` = `428.86 ms`, `16,000` = `1,703.94 ms` (`8x` lines -> `42.9x` time).
- Suggested fix: Replace `doc.lines.iter().find(|l| l.source.start == line_start)` with `doc.line_index_at(line_start)` (binary search, already present on `ParsedDocument`), or walk the cursor monotonically through `doc.lines` while iterating selected lines.

### D4. Every keystroke reparses the entire document and every frame walks all lines
- Severity: Medium
- Category: Performance
- Location: `src/model/document.rs` (`DocumentTab::parsed()` full `parse_document`), `src/ui/text_editor.rs:982-1016` (`EditorElement::request_layout` walking every `parsed.lines` or full `content`), `src/ui/text_editor.rs:1090` (Raw `prepaint` allocating `visual_rows_tabbed` for every line).
- Problem: Every single character edit bumps `revision` and forces a full `parse_document` (`~29 ms/MB` in release), and every frame (including idle frames and caret blinks) walks every line of the document in `request_layout` and `prepaint` to recompute wrapped row counts and `y` coordinates, using culling only to skip `shape_line`. Keystroke + frame crosses the 16.6 ms 60 fps budget between 100 KB (`~5.2 ms`) and 1 MB (`53–59 ms`), and reaches `341–376 ms` at 5 MB. Note: opening a 5 MB file (`read_note + parse`) takes `185.8 ms` on a warm page cache.
- Evidence: PROVEN in `ea03a57` release GPUI harness and VM non-GPUI release harness:
  - Components (`ea03a57`): `10 KB` parse `0.285 ms`; `100 KB` parse `2.72 ms`; `1 MB` parse `28.80 ms`; `5 MB` parse `170.21 ms`.
  - Keystroke + frame (`ea03a57`, `md-wysiwyg` mid): `10 KB` `1.48 ms`; `100 KB` `5.23 ms`; `1 MB` `59.21 ms` (handler `38.0 ms` + frame `21.2 ms`); `5 MB` `351.12 ms` (handler `211.3 ms` + frame `139.8 ms`).
  - Idle frames (`ea03a57`): `100 KB` `2.5–2.7 ms`; `1 MB` `13.4–16.0 ms`; `5 MB` `85.4–107.3 ms`.
  - VM release `OPEN_TAB` (`read_note + DocumentTab::new + parse`): `100 KB` `4.96 ms`; `1 MB` `35.98 ms`; `5 MB` `185.80 ms`.
- Suggested fix: Debounce/coalesce or make block+inline parsing incremental per dirty paragraph/fence; cache wrapped line heights per line revision and window width so `request_layout` and `prepaint` only visit lines affected by edits or width changes.

### D5. Undo byte budget (256 KiB) evicts whole-document edits (`Replace All`, `Select All + type`) on the next keystroke
- Severity: Medium
- Category: Logic
- Location: `src/model/text_ops.rs:13` (`UNDO_BYTE_BUDGET: usize = 256 * 1024`), `src/model/text_ops.rs:191-194` (`while self.undo_bytes > UNDO_BYTE_BUDGET && self.undo.len() > 1 { self.undo.remove(0); }`), `src/model/document.rs:625-634` (`replace_every_match` calls `self.set_content(updated)` -> `edit(0..end, &new_content)`).
- Problem: `replace_every_match` computes a brand-new `String` and passes the entire `0..end` span to `set_content`, recording a single `DiffEdit` costing `old.len() + new.len()` bytes (`2 * doc_len`), even when only a handful of matches changed. On any document `>= ~129 KB`, that single entry immediately exceeds `UNDO_BYTE_BUDGET` (`256 KiB`). `push_at` keeps it only as long as `undo.len() == 1`; as soon as the user types a single character (`undo.len() == 2`), `self.undo.remove(0)` **evicts the entire Replace All step**, making it impossible to undo back to the pre-replace document. The same happens for `Select All + type/paste` on `>= ~256 KB` notes.
- Evidence: PROVEN in both the `ea03a57` GPUI harness (`select-all+type` fails by 300 KB; `replace-all+type 1 letter` fails by 200 KB) and the VM release harness (`make_realistic_md`, single `"paragraph" -> "PARAGRAPH"` replace-all + `"x"` at offset 0):
  ```text
  UNDO 100KB: after_replace_bytes= 204802 after_plus_1key_bytes=204803 original_restored=true
  UNDO 200KB: after_replace_bytes= 409602 after_plus_1key_bytes=     1 original_restored=false
  UNDO 300KB: after_replace_bytes= 614400 after_plus_1key_bytes=     1 original_restored=false
  UNDO   1MB: after_replace_bytes=2097154 after_plus_1key_bytes=     1 original_restored=false
  ```
- Suggested fix: Have `replace_every_match` record a minimal diff (trimmed common prefix/suffix at minimum, or per-match undo batch) instead of cloning/storing `2 * doc_len`, and raise `UNDO_BYTE_BUDGET` (256 KiB is tiny compared to the ~21 MB parse cache of a 1 MB note) or cap by step count/megabytes.

### D6. UI-thread synchronous file I/O, including a full `rescan_workspace()` on every `Ctrl+S`
- Severity: Medium
- Category: Performance
- Location: `src/app/tabs.rs:26, 40, 56` (`save_tab` does synchronous `atomic_save`/`atomic_save_new` and unconditionally calls `self.rescan_workspace()`), `src/app/commands.rs:343, 363, 378, 393, 410` (Open dialog, `extract_text_from_pdf`, `read_note`, export + `atomic_save`), `src/app/file_ops.rs:46, 52` (`copy_note_into_workspace`, `scan_workspace_dir`), `src/app/sidebar.rs:242` and `src/app/render.rs:637` (`read_note` on click), `src/app/drag_drop.rs` (`read_note`, `copy_note_into_workspace`, recursive `copy_folder`, `move_note`, `move_folder`, `rescan_workspace`), `src/model/settings.rs:120` (15 sync `settings.save()` call sites), `src/app.rs:214-228` (startup). Also `src/app/render.rs:1477` calls `p.is_dir()` on every recent folder per frame while the workspace menu is open.
- Problem: Whereas `fetch_latest_release` (`app/commands.rs:123-126`) and `move_to_trash` (`app/file_ops.rs:527, 638`) properly run on `cx.background_executor()`, **every other filesystem operation runs synchronously on the UI thread**. Most visibly, `save_tab` calls `self.rescan_workspace()` on **every** `Ctrl+S` (even when updating an already-tracked file whose folder/tree did not change). On local NVMe with a warm page cache, startup (`seed` + `scan` + `read_welcome` + `parse`) is fast (`0.27 ms` for 10 files, `1.42 ms` for 500, `4.48 ms` for 2,000, `20.7 ms` for 5,000), and Lane C measured 100,000 files at `~250 ms` warm (`/usr` 78k files at `1.25 s` in debug). On cold disks, FUSE, cloud-synced folders, or NFS, every save/open/click stalls the UI thread.
- Evidence: PROVEN by code inspection + VM release timing (`STARTUP ws`):
  ```text
  STARTUP ws folders= 1 files=  10: seed+scan+read_welcome+tab+parse =  0.269 ms (warm tmpfs)
  STARTUP ws folders=10 files= 500: seed+scan+read_welcome+tab+parse =  1.417 ms
  STARTUP ws folders=20 files=2000: seed+scan+read_welcome+tab+parse =  4.483 ms
  STARTUP ws folders=50 files=5000: seed+scan+read_welcome+tab+parse = 20.724 ms
  ```
- Suggested fix: Skip `rescan_workspace()` on `save_tab` when saving an existing workspace file whose path is unchanged (just update the existing entry's metadata if needed), and move file loads, saves, PDF extraction, folder scans, and folder copies onto `cx.background_executor()`.

### D7. `ParsedDocument` expands memory ~20x over source text when cached; `release_parse_cache` bounds inactive tabs
- Severity: Low
- Category: Performance
- Location: `src/markdown/parser.rs` (`ParsedDocument`, `ParsedLine`), `src/model/document.rs:394, 417-436`, `src/app/editor_events.rs:491-493` (`release_parse_cache()` on non-active tabs).
- Problem: Each cached `ParsedDocument` stores a `ParsedLine` (`sizeof = 200` bytes) plus `StyledSegment`s, `InlinePair`s, `hidden_ranges`, `link_hits`, `OffsetMap` byte-mapping `Vec`s, and a `display_text` `String` for every source line. For a realistic 1 MB Markdown note (`43,432` lines), caching `ParsedDocument` adds **~20 MB** (`+21.04 MB` total RSS per cached 1 MB tab across 20 tabs). Because `editor_events.rs:491-493` releases `parse_cache` on all inactive tabs during render, 300 idle tabs stay fast (`12.9 ms/frame`) and retain only `content: String` + `UndoHistory`; however, the active tab still carries ~21x memory and `DocumentTab::content_snapshot` (`SharedString::from(self.content.clone())`), so switching back and forth between large notes re-parses and allocates ~20x per switch.
- Evidence: PROVEN in VM release harness (`20 x 1 MB` `make_realistic_md` tabs):
  ```text
  MEM 20x1MB tabs: +430,868 KB (+21.04 MB per cached 1 MB tab)
  MEM 1MB ParsedDocument details: lines=43432 segments=65151 inline_pairs=15512 display_text_cap=745KB sizeof(ParsedLine)=200
  ```
- Suggested fix: Compact `ParsedLine` (e.g., use compact/arena ranges or compute `OffsetMap` lazily for visible/edited lines rather than allocating per-line vectors up front for off-screen lines).

### D8. Dead and drifted legacy Markdown parser, 41 `allow(dead_code)` attributes, and render boilerplate
- Severity: Low
- Category: Standards / Code quality
- Location:
  - Legacy parser: `src/markdown/parser.rs:11-162` (`Block`, `Span`), `:215-535` (`parse_markdown`, `parse_spans`, `source_offset_for_click`, `block_source`, `spans_of`, `floor_source`) — ~380 lines gated by `#[cfg_attr(not(test), allow(dead_code))]` and exercised only by 5 legacy tests in `src/markdown/parser/tests.rs`.
  - Dead/test-only helpers with `allow(dead_code)` (41 attributes across `src/`): `src/markdown/exporter.rs:117` (`format_markdown_table`), `src/services/fs_service.rs:628` (`safe_delete`), `src/model/document.rs:394, 441` (`parsed_snapshot`, `undo_stored_bytes`), `src/model/text_ops.rs:131, 1108`, `src/model/document/table.rs:161` (`is_in_table`), `src/model/document/typing.rs:332` (`insert_snippet`), `src/ui/text_editor.rs:35-40` (`PaintCell`'s unused `text_origin`, `shaped: ShapedLine`, `offset_map`), `:869`, `src/ui/sidebar.rs:225` (`SidebarProps`), `src/ui/settings_menu.rs:7` (`SettingsMenuProps`), `src/ui/formatting_toolbar.rs:344, 366`, `src/ui/line_field.rs:64, 69`, `src/theme.rs:29, 81, 86, 149`, `src/app.rs:146`, `src/app/commands.rs:575`, `src/model/workspace.rs:4, 15, 25`.
  - Oversized functions & boilerplate: 45 clippy `too_many_lines` diagnostics (worst: `src/app/render.rs:6` at 1,779 lines containing 100+ identical `let _ = view.update(...)` closures across `:54-941`; `src/ui/text_editor.rs:1027` at 1,103 lines; `src/app/key_bindings.rs:7` at 770 lines; `src/ui/sidebar.rs:289` at 711 lines; `src/model/document/inline_format.rs:11` at 706 lines).
- Problem: The legacy `parse_markdown` AST has drifted from production `parse_document`, classifying tilde fences (`~~~`), `+ item`, `1) item`, and 4-space indented code as `Paragraph` while `parse_document` parses them as `CodeFence`, `ListItem`, and `CodeLine`. Keeping ~380 lines of dead parser code alongside production code misleads readers and tests an obsolete parser. Elsewhere, 41 `allow(dead_code)` suppressions hide unused code—including `PaintCell` still constructing and cloning `ShapedLine` and `RowOffsetMap` into dead fields (`text_editor.rs:1785-1797`).
- Evidence: PROVEN in both `ea03a57` and VM release harness + `grep -rn "allow(dead_code)" src/`:
  ```text
  DRIFT "~~~\ncode\n~~~": parse_markdown=["Paragraph", "Paragraph", "Paragraph"] vs parse_document=["CodeFence", "CodeLine", "CodeFence"]
  DRIFT "+ item":           parse_markdown=["Paragraph"]                         vs parse_document=["ListItem"]
  DRIFT "1) item":          parse_markdown=["Paragraph"]                         vs parse_document=["ListItem"]
  DRIFT "    indented code":parse_markdown=["Paragraph"]                         vs parse_document=["CodeLine"]
  ```
- Suggested fix: Remove `parse_markdown`/`parse_spans`/`Block`/`Span` (migrate or remove the 5 unit tests in `parser/tests.rs`), delete the genuinely unused `#[allow(dead_code)]` items (especially `PaintCell`'s dead `ShapedLine`/`RowOffsetMap` fields and `safe_delete`), and extract a helper for `view.update(...)` event handlers in `app/render.rs`.

## Checked and found fine

- **Lane B finding 4 (bold toggle on a line with thousands of nested spans)**: **Does not reproduce as quadratic in release.** On `n = 20,000` nested spans (`~273 KB` single line), release parse = `29.3 ms`, type = `27.1 ms`, bold = `0.40 ms` (`0.013 ms` at `n = 500`, `0.031 ms` at `n = 2,000`, `0.059 ms` at `n = 4,000` -> linear). Lane B's `34.8 s` was a debug-build artifact.
- **Update check network call (`src/app/commands.rs:123-126`) and `move_to_trash` (`src/app/file_ops.rs:527, 638`)**: Both spawn onto `cx.background_executor()` off the UI thread, and `fetch_latest_release` enforces a 5 s timeout (`update_service.rs:5`).
- **Inactive-tab parse cache release (`src/app/editor_events.rs:491-493`)**: Actively releases `parse_cache` for all tabs other than `active_id` during render; 300 open tabs ran at `12.9 ms/frame` in the GPUI harness.
- **20,000-row table parse speed**: Measured `29.4 ms` in release and `218.1 ms` in debug on Linux x86_64 (well under the 1,000 ms debug test budget in `src/markdown/parser/tests.rs:150-168`; CI observed macOS debug taking longer ~1.48 s due to unoptimized debug checks + macOS runner hardware variance).
- **Find, Replace-All throughput, 1 MB paste, model-layer table operations, and sidebar search filter**: In release builds, 1 MB find-next is `3.2 ms`, 1 MB replace-all (`17,812` hits) is `45.1 ms`, 1 MB paste is `65.4 ms`, 5,000x8 model-layer table edits are `<51 ms`, and `sidebar_search_matches` at 5,000 files is `<0.5 ms`.
- **Settings startup cost & `show_sidebar` note**: Startup in `src/app.rs:452` opens `show_sidebar: true` regardless of `settings.show_sidebar` (which also lacks a `default_show_sidebar` serde default at `settings.rs:40-41`), causing no performance surprise on `{}` files.

## Reproducible non-GPUI benchmark code

To re-run the non-GPUI Lane D measurements on any machine (including without `libxkbcommon-x11-dev`), compile against unmodified `src/markdown`, `src/services`, and `src/model` (`cargo run --release`):

```rust
// Link against unmodified crate modules: markdown, services, model (with a ThemeMode enum stub).
fn make_realistic_md(target_bytes: usize) -> String {
    let block = "\
## Section Heading with **bold** and *italic*\n\n\
This is a normal paragraph containing a [link](https://example.com/docs) and `inline_code` plus ~~strike~~.\n\n\
- First list item in markdown\n- [ ] Unchecked task item\n- [x] Checked task item\n\n\
> A blockquote line for testing.\n\n\
| Name | Value | Notes |\n| --- | --- | --- |\n| alpha | 123 | ok |\n\n";
    let mut s = String::with_capacity(target_bytes + block.len());
    while s.len() < target_bytes { s.push_str(block); }
    s.truncate(target_bytes);
    while !s.is_char_boundary(s.len()) { s.pop(); }
    if !s.ends_with('\n') { s.push('\n'); }
    s
}
// 1) Startup: loop (folders, files) in [(1,10),(10,500),(20,2000),(50,5000)], time seed_welcome_note + scan_workspace_dir + read_note + DocumentTab::new + parsed().
// 2) Memory per tab: allocate 20 x 1 MB DocumentTab, call .parsed(), read /proc/self/status VmRSS before/after release_parse_cache(); inspect ParsedLine count & size_of::<ParsedLine>().
// 3) Undo eviction: tab(&make_realistic_md(bytes)).replace_every_match("paragraph", "PARAGRAPH", true); then edit(0..0, "x"); while t.undo() {} -> assert_eq!(t.content, orig).
// 4) B3 vs B4 release scaling: repeat "1. a\n" x lines + apply_format_in_mode("ol", Wysiwyg); repeat "**a _b " x n + "a" + " b_ a**" x n + parse_document + insert_text_in_mode + apply_format_in_mode("bold", Wysiwyg).
// 5) Legacy parser drift: compare parse_markdown(s) vs parse_document(s) on "~~~\ncode\n~~~", "+ item", "1) item", "    indented code".
```

## Raw benchmark output (preserved from `ea03a57` release GPUI + model run)

### components

```
LD comp  10KB lines=   255 parse=   0.285 stats=  0.019 snapshot= 0.000 visual_rows=  0.030 row_count=  0.028 find_ci=   0.032 find_cs=  0.007 hits=184 to_utf16(end)= 0.005 from_utf16(end)x2= 0.021 line_col(end)=  0.022 home_offset(end)=  0.008  [ms]
LD comp 100KB lines=  2526 parse=   2.716 stats=  0.196 snapshot= 0.003 visual_rows=  0.288 row_count=  0.251 find_ci=   0.327 find_cs=  0.066 hits=1770 to_utf16(end)= 0.053 from_utf16(end)x2= 0.214 line_col(end)=  0.216 home_offset(end)=  0.080  [ms]
LD comp   1MB lines= 25478 parse=  28.800 stats=  1.997 snapshot= 0.045 visual_rows=  2.935 row_count=  2.572 find_ci=   3.289 find_cs=  0.682 hits=17812 to_utf16(end)= 0.559 from_utf16(end)x2= 2.305 line_col(end)=  2.286 home_offset(end)=  0.842  [ms]
LD comp   5MB lines=126383 parse= 170.209 stats= 10.004 snapshot= 0.401 visual_rows= 14.744 row_count= 14.840 find_ci=  16.638 find_cs=  3.525 hits=88337 to_utf16(end)= 2.798 from_utf16(end)x2=10.843 line_col(end)= 11.168 home_offset(end)=  4.179  [ms]
```

### keystroke_model

```
LD key-model  10KB md-wysiwyg mid insert_text_in_mode=   0.274 ms
LD key-model  10KB md-raw     end insert_text_in_mode=   0.273 ms
LD key-model  10KB md-raw     mid insert_text_in_mode=   0.276 ms
LD key-model  10KB txt        end insert_text_in_mode=   0.022 ms
LD key-model  10KB txt        mid insert_text_in_mode=   0.011 ms
LD key-model 100KB md-wysiwyg end insert_text_in_mode=   2.733 ms
LD key-model 100KB md-wysiwyg mid insert_text_in_mode=   2.700 ms
LD key-model 100KB md-raw     end insert_text_in_mode=   2.660 ms
LD key-model 100KB md-raw     mid insert_text_in_mode=   2.680 ms
LD key-model 100KB txt        end insert_text_in_mode=   0.221 ms
LD key-model 100KB txt        mid insert_text_in_mode=   0.110 ms
LD key-model   1MB md-wysiwyg end insert_text_in_mode=  31.770 ms
LD key-model   1MB md-wysiwyg mid insert_text_in_mode=  30.456 ms
LD key-model   1MB md-raw     end insert_text_in_mode=  29.681 ms
LD key-model   1MB md-raw     mid insert_text_in_mode=  30.092 ms
LD key-model   1MB txt        end insert_text_in_mode=   2.252 ms
LD key-model   1MB txt        mid insert_text_in_mode=   1.133 ms
LD key-model   5MB md-wysiwyg end insert_text_in_mode= 187.555 ms
LD key-model   5MB md-wysiwyg mid insert_text_in_mode= 184.398 ms
LD key-model   5MB md-raw     end insert_text_in_mode= 187.348 ms
LD key-model   5MB md-raw     mid insert_text_in_mode= 182.276 ms
LD key-model   5MB txt        end insert_text_in_mode=  11.242 ms
LD key-model   5MB txt        mid insert_text_in_mode=   5.861 ms
```

### keystroke_app

```
LD key-app  10KB md-wysiwyg mid rows=   337 shaped= 101 handler=   0.333 frame_after=   1.144 total=   1.477 idle_frame=   1.154 ms
LD key-app  10KB md-raw     end rows=   339 shaped=  69 handler=   0.332 frame_after=   1.042 total=   1.375 idle_frame=   0.986 ms
LD key-app  10KB md-raw     mid rows=   339 shaped= 105 handler=   0.349 frame_after=   1.062 total=   1.411 idle_frame=   1.063 ms
LD key-app  10KB txt        end rows=   297 shaped=  69 handler=   0.048 frame_after=   1.129 total=   1.177 idle_frame=   1.120 ms
LD key-app  10KB txt        mid rows=   297 shaped= 105 handler=   0.027 frame_after=   1.226 total=   1.253 idle_frame=   1.187 ms
LD key-app 100KB md-wysiwyg end rows=  3315 shaped=  66 handler=   2.886 frame_after=   2.361 total=   5.247 idle_frame=   2.200 ms
LD key-app 100KB md-wysiwyg mid rows=  3315 shaped= 102 handler=   2.869 frame_after=   2.365 total=   5.234 idle_frame=   2.237 ms
LD key-app 100KB md-raw     end rows=  3342 shaped=  69 handler=   2.911 frame_after=   2.277 total=   5.188 idle_frame=   2.122 ms
LD key-app 100KB md-raw     mid rows=  3342 shaped= 105 handler=   3.406 frame_after=   2.528 total=   5.934 idle_frame=   2.278 ms
LD key-app 100KB txt        end rows=  2934 shaped=  69 handler=   0.484 frame_after=   2.709 total=   3.193 idle_frame=   2.716 ms
LD key-app 100KB txt        mid rows=  2934 shaped= 105 handler=   0.228 frame_after=   2.697 total=   2.925 idle_frame=   2.615 ms
LD key-app   1MB md-wysiwyg end rows= 33424 shaped=  66 handler=  34.791 frame_after=  18.459 total=  53.250 idle_frame=  13.337 ms
LD key-app   1MB md-wysiwyg mid rows= 33424 shaped= 103 handler=  38.031 frame_after=  21.178 total=  59.209 idle_frame=  12.890 ms
LD key-app   1MB md-raw     end rows= 33698 shaped=  69 handler=  37.333 frame_after=  19.331 total=  56.664 idle_frame=  14.589 ms
LD key-app   1MB md-raw     mid rows= 33698 shaped= 105 handler=  38.717 frame_after=  20.572 total=  59.288 idle_frame=  14.593 ms
LD key-app   1MB txt        end rows= 29588 shaped=  69 handler=   4.519 frame_after=  21.968 total=  26.487 idle_frame=  17.042 ms
LD key-app   1MB txt        mid rows= 29588 shaped= 105 handler=   2.778 frame_after=  21.881 total=  24.659 idle_frame=  15.435 ms
LD key-app   5MB md-wysiwyg end rows=165794 shaped=  66 handler= 209.077 frame_after= 132.550 total= 341.627 idle_frame=  93.907 ms
LD key-app   5MB md-wysiwyg mid rows=165794 shaped= 102 handler= 211.338 frame_after= 139.784 total= 351.122 idle_frame= 102.244 ms
LD key-app   5MB md-raw     end rows=167153 shaped=  69 handler= 236.672 frame_after= 134.420 total= 371.092 idle_frame=  82.448 ms
LD key-app   5MB md-raw     mid rows=167153 shaped= 105 handler= 242.121 frame_after= 133.579 total= 375.700 idle_frame=  83.966 ms
LD key-app   5MB txt        end rows=146768 shaped=  69 handler=  25.978 frame_after= 141.319 total= 167.297 idle_frame=  93.110 ms
LD key-app   5MB txt        mid rows=146768 shaped= 105 handler=  12.787 frame_after= 132.515 total= 145.302 idle_frame=  83.693 ms
```

### idle_frames

```
LD idle  10KB md-raw     idle_frame=   1.031 caret_move+frame=   1.227 select_all_frame=   1.090 find_open_frame=   1.281 (hits=184) handler_with_find_open=   0.433 ms
LD idle 100KB md-wysiwyg idle_frame=   2.507 caret_move+frame=   2.485 select_all_frame=   3.568 find_open_frame=   3.516 (hits=1770) handler_with_find_open=   5.252 ms
LD idle 100KB md-raw     idle_frame=   2.695 caret_move+frame=   2.349 select_all_frame=   2.531 find_open_frame=   2.495 (hits=1770) handler_with_find_open=   3.461 ms
LD idle   1MB md-wysiwyg idle_frame=  13.403 caret_move+frame=  12.794 select_all_frame=  15.707 find_open_frame=  14.143 (hits=17812) handler_with_find_open=  67.080 ms
LD idle   1MB md-raw     idle_frame=  15.975 caret_move+frame=  15.573 select_all_frame=  17.468 find_open_frame=  15.592 (hits=17812) handler_with_find_open=  53.075 ms
LD idle   5MB md-wysiwyg idle_frame=  85.385 caret_move+frame=  88.720 select_all_frame= 103.516 find_open_frame=  85.885 (hits=88337) handler_with_find_open= 235.547 ms
LD idle   5MB md-raw     idle_frame= 107.269 caret_move+frame=  86.308 select_all_frame=  97.612 find_open_frame= 117.342 (hits=88337) handler_with_find_open= 248.801 ms
```

### bulk_ops

```
LD bulk 100KB paste(doc-sized block)=   5.339 replace_all(1770 hits)=   4.290 replace_one=   3.459 find_next=   0.316 select_all_copy=   0.105 ms
LD bulk   1MB paste(doc-sized block)=  65.397 replace_all(17812 hits)=  45.110 replace_one=  37.901 find_next=   3.249 select_all_copy=   1.860 ms
LD bulk   5MB paste(doc-sized block)= 307.334 replace_all(88337 hits)= 245.767 replace_one= 206.854 find_next=  16.673 select_all_copy=  11.276 ms
```

### lane_b3

```
LD laneB3 1. a       lines=  4000 (  19 KB) select-all + ol =    119.179 ms
LD laneB3 1. a       lines=  8000 (  39 KB) select-all + ol =    468.850 ms
LD laneB3 1. a       lines= 16000 (  78 KB) select-all + ol =   1917.883 ms
LD laneB3 1. a       lines= 32000 ( 156 KB) select-all + ol =   9220.228 ms
LD laneB3 a **b** c  lines=  2000 (  19 KB) select-all + ol =     35.169 ms
LD laneB3 a **b** c  lines=  4000 (  39 KB) select-all + ol =    129.727 ms
LD laneB3 a **b** c  lines=  8000 (  78 KB) select-all + ol =    505.212 ms
LD laneB3 a **b** c  lines= 16000 ( 156 KB) select-all + ol =   2023.537 ms
LD laneB3 a **b** c  lines= 32000 ( 312 KB) select-all + ol =   9390.530 ms
LD laneB3 realistic  10KB lines=   255 select-all + ol    =      2.424 ms
LD laneB3 realistic  10KB lines=   255 select-all + quote =      2.237 ms
LD laneB3 realistic  10KB lines=   255 select-all + h2    =      2.274 ms
LD laneB3 realistic  10KB lines=   255 select-all + bold  =      2.889 ms
LD laneB3 realistic 100KB lines=  2526 select-all + ol    =    129.619 ms
LD laneB3 realistic 100KB lines=  2526 select-all + quote =    116.247 ms
LD laneB3 realistic 100KB lines=  2526 select-all + h2    =    115.957 ms
LD laneB3 realistic 100KB lines=  2526 select-all + bold  =     17.471 ms
LD laneB3 realistic   1MB lines= 25478 select-all + ol    =  12383.488 ms
LD laneB3 realistic   1MB lines= 25478 select-all + quote =  12520.670 ms
LD laneB3 realistic   1MB lines= 25478 select-all + h2    =  12423.310 ms
LD laneB3 realistic   1MB lines= 25478 select-all + bold  =    209.924 ms
LD laneB3 realistic   5MB lines=126383 select-all + ol    = 322472.960 ms
LD laneB3 realistic   5MB lines=126383 select-all + quote = 326145.474 ms
LD laneB3 realistic   5MB lines=126383 select-all + h2    = 326001.911 ms
LD laneB3 realistic   5MB lines=126383 select-all + bold  =   1142.823 ms
```

### lane_b4

```
LD laneB4 n=  500 (  6 KB) type=    0.681 bold=     0.009 parse=   0.570 ms
LD laneB4 n= 1000 ( 13 KB) type=    1.416 bold=     0.016 parse=   1.216 ms
LD laneB4 n= 2000 ( 27 KB) type=    3.086 bold=     0.035 parse=   2.580 ms
LD laneB4 n= 4000 ( 54 KB) type=    5.971 bold=     0.062 parse=   4.992 ms
LD laneB4 n=20000 (273 KB) type=   39.872 bold=     0.423 parse=  35.642 ms
```

### tables

```
LD table 500x8 (54 KB) row_below=   2.381 col_right=   3.599 del_col=   3.078 move_row=   2.466 key_in_cell(model)=   0.782 tab_key=   0.009 | app: handler=   1.180 frame_after_key_in_cell=  24.727 frame_after_key_outside=  25.371 idle_frame=  25.544 ms
LD table 2000x8 (229 KB) row_below=   9.873 col_right=  14.653 del_col=  12.784 move_row=  10.092 key_in_cell(model)=   3.218 tab_key=   0.073 | app: handler=   5.519 frame_after_key_in_cell= 115.409 frame_after_key_outside= 137.451 idle_frame= 116.975 ms
LD table 5000x8 (587 KB) row_below=  29.425 col_right=  50.792 del_col=  41.109 move_row=  42.954 key_in_cell(model)=  11.369 tab_key=   0.231 | app: handler=  22.524 frame_after_key_in_cell= 323.722 frame_after_key_outside= 344.412 idle_frame= 319.615 ms
LD table 100x40 (54 KB) row_below=   4.049 col_right=   5.332 del_col=   4.858 move_row=   3.701 key_in_cell(model)=   0.889 tab_key=   0.010 | app: handler=   1.558 frame_after_key_in_cell= 117.192 frame_after_key_outside=  98.815 idle_frame= 118.698 ms
```

### sidebar

```
LD sidebar folders= 20 files= 2000 build_sidebar_tree=   1.332 search_filter=  0.161 | app: idle_frame=  247.811 (no-sidebar-rows baseline  0.834) keystroke+frame=  210.541 shift_down_handler=   7.703 frame_all_selected=  644.315 refresh_workspace_ids(50 tabs)=  3.787 ms
LD sidebar folders= 50 files= 5000 build_sidebar_tree=   9.362 search_filter=  0.458 | app: idle_frame=  635.032 (no-sidebar-rows baseline  0.993) keystroke+frame=  641.084 shift_down_handler=  52.402 frame_all_selected= 2856.195 refresh_workspace_ids(50 tabs)=  8.684 ms
LD sidebar folders=200 files= 5000 build_sidebar_tree=  21.311 search_filter=  0.463 | app: idle_frame=  793.665 (no-sidebar-rows baseline  1.117) keystroke+frame=  775.175 shift_down_handler=  79.880 frame_all_selected= 3729.237 refresh_workspace_ids(50 tabs)=  9.031 ms
```

### tabs

```
LD tabs n= 20 idle_frame=   1.933 keystroke+frame=   2.234 ms
LD tabs n=100 idle_frame=   4.465 keystroke+frame=   4.716 ms
LD tabs n=300 idle_frame=  12.914 keystroke+frame=  11.750 ms
```

### undo_memory

```
LD undo typed=  2000 bytes  undo_stored_bytes=2000
LD undo typed=  3000 bytes  undo_stored_bytes=3000
LD undo typed=  4000 bytes  undo_stored_bytes=4000
LD undo typed=  5000 bytes  undo_stored_bytes=5000
LD undo typed= 10000 bytes  undo_stored_bytes=10000
LD undo typed= 15000 bytes  undo_stored_bytes=15000
LD undo typed= 20000 bytes  undo_stored_bytes=20000
LD undo 20000 keystrokes in 1MB md (model only, Raw): 777.8 s
LD undo doc= 100KB  select-all+type 2 letters: undo steps=2 original restored=true (len after undo=102935)  |  replace-all+type 1 letter: undo steps=2 original restored=true  |  replace-all alone stores 205870 bytes
LD undo doc= 200KB  select-all+type 2 letters: undo steps=2 original restored=true (len after undo=205372)  |  replace-all+type 1 letter: undo steps=1 original restored=false  |  replace-all alone stores 410744 bytes
LD undo doc= 300KB  select-all+type 2 letters: undo steps=1 original restored=false (len after undo=307527)  |  replace-all+type 1 letter: undo steps=1 original restored=false  |  replace-all alone stores 615054 bytes
LD undo doc=1024KB  select-all+type 2 letters: undo steps=1 original restored=false (len after undo=1049246)  |  replace-all+type 1 letter: undo steps=1 original restored=false  |  replace-all alone stores 2098492 bytes
```

### vm_continuation_release (non-GPUI, `rustc 1.99.1`, `ea03a57`)

```
STARTUP ws folders=1 files=10: seed+scan+read_welcome+tab+parse = 0.269 ms (listed=11 folders=1)
STARTUP ws folders=10 files=500: seed+scan+read_welcome+tab+parse = 1.417 ms (listed=501 folders=10)
STARTUP ws folders=20 files=2000: seed+scan+read_welcome+tab+parse = 4.483 ms (listed=2001 folders=20)
STARTUP ws folders=50 files=5000: seed+scan+read_welcome+tab+parse = 20.724 ms (listed=5001 folders=50)
OPEN_TAB 100KB: read_note+DocumentTab::new+parse = 4.956 ms
OPEN_TAB 1MB: read_note+DocumentTab::new+parse = 35.981 ms
OPEN_TAB 5MB: read_note+DocumentTab::new+parse = 185.799 ms
MEM 20x1MB tabs: +430868KB (+21.04MB per cached tab)
MEM 1MB ParsedDocument details: lines=43432 segments=65151 inline_pairs=15512 display_text_cap=745KB sizeof(ParsedLine)=200
UNDO 100KB: after_replace_bytes=204802 after_plus_1key_bytes=204803 original_restored=true
UNDO 200KB: after_replace_bytes=409602 after_plus_1key_bytes=1 original_restored=false
UNDO 300KB: after_replace_bytes=614400 after_plus_1key_bytes=1 original_restored=false
UNDO 1MB: after_replace_bytes=2097154 after_plus_1key_bytes=1 original_restored=false
B3 lines= 2000: select-all + ol = 39.704 ms
B3 lines= 4000: select-all + ol = 107.280 ms
B3 lines= 8000: select-all + ol = 428.856 ms
B3 lines=16000: select-all + ol = 1703.940 ms
B4 n=  500 (  6KB): parse=0.516ms type=0.529ms bold=0.013ms
B4 n= 1000 ( 13KB): parse=1.077ms type=1.059ms bold=0.016ms
B4 n= 2000 ( 27KB): parse=2.265ms type=2.168ms bold=0.031ms
B4 n= 4000 ( 54KB): parse=4.766ms type=4.537ms bold=0.059ms
B4 n=20000 (273KB): parse=29.332ms type=27.060ms bold=0.401ms
TABLE_20K: lines=20003 elapsed=29.385 ms (debug=218.071 ms)
```

### legacy_parser_drift

```
LD drift "~~~\ncode\n~~~": parse_markdown=["Paragraph", "Paragraph", "Paragraph"]  parse_document=["CodeFence", "CodeLine", "CodeFence"]
LD drift "* item": parse_markdown=["ListItem"]  parse_document=["ListItem"]
LD drift "+ item": parse_markdown=["Paragraph"]  parse_document=["ListItem"]
LD drift "Title\n=====": parse_markdown=["Paragraph", "Paragraph"]  parse_document=["Paragraph", "Paragraph"]
LD drift "1) item": parse_markdown=["Paragraph"]  parse_document=["ListItem"]
LD drift "    indented code": parse_markdown=["Paragraph"]  parse_document=["CodeLine"]
```
