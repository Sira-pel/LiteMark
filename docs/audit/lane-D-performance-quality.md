# Lane D: performance and code quality

Status: **partial, stopped on request.** The measurements below were taken, but the lane was stopped before
they were written up as findings, and the benchmark code was not saved into this file. Nothing here has been
reviewed. Not measured: startup, memory copies per tab, the update check and other UI-thread blocking.

Toolchain: rustc/cargo 1.97, Linux x86_64, 4 cores, audited commit `ea03a57`. All numbers are from a release
build (`cargo test --release`), in milliseconds unless marked, with test documents generated at the sizes shown.
Per-frame numbers come from gpui's test harness, not a real window. The benchmarks were a throwaway module
(`src/lane_d_bench.rs`) in a worktree that is not part of this branch, so anyone confirming a number has to
write the benchmark again.

## What the numbers show (leads, not confirmed findings)

| Lead | Key numbers |
|---|---|
| Sidebar draws every row on every frame, with no virtual list | 2,000 files: 248 ms per idle frame; 5,000 files: 635 to 794 ms; with all rows selected 2.9 to 3.7 s per frame. The same app without sidebar rows: about 1 ms |
| Select All plus list, quote or heading toggle is quadratic (lane B finding 3, now timed in release) | Realistic 100 KB note: about 120 ms; 1 MB: about 12.4 s; 5 MB: about 5.4 minutes. Select All plus bold stays linear (1 MB: 210 ms) |
| Large tables are laid out in full on every frame | 500 rows x 8 columns (54 KB): 25 ms per frame; 100 x 40: 117 ms; 2,000 x 8: 117 ms; 5,000 x 8: 320 ms |
| Every keystroke reparses the whole Markdown document and every frame walks every line | Keystroke plus frame: 100 KB about 5 ms, 1 MB about 55 to 60 ms, 5 MB about 340 to 375 ms. Passes 16 ms between 100 KB and 1 MB, and 100 ms between 1 MB and 5 MB. Plain text is about half that |
| Undo history drops whole-document edits above its 256 KB budget | After Select All plus typing, or Replace All plus one more keystroke, undo no longer restores the original at 200 to 300 KB and above |
| Typing 20,000 characters into a 1 MB note | 778 s of model time in total, about 39 ms per keystroke, in line with the reparse cost above |
| A second Markdown parser (`parse_markdown`, `src/markdown/parser.rs:223`) is reachable only from tests and has drifted | It classifies `~~~` fences, `+ item`, `1) item` and indented code as paragraphs, where `parse_document` does not |
| Lane B finding 4 (bold toggle on a line with thousands of nested spans) did not reproduce as quadratic in release | 273 KB line: bold 0.4 ms, typing 40 ms, parse 36 ms, all linear |

Fast enough at the sizes tried: find and replace-all (1 MB, 17,812 hits: 45 ms), paste of a document-sized
block (1 MB: 65 ms), table row and column operations in the model (5,000 x 8: under 51 ms), 300 open tabs
(13 ms per frame), sidebar tree build and search filter (5,000 files: under 22 ms).

Leads from reading the code that were not measured: `save_tab` rescans the whole notes folder on the UI
thread (`src/app/tabs.rs:56`); the sidebar tree is rebuilt, sorted and cloned on every frame
(`src/ui/sidebar.rs:143-221`, `:368`, `:614`); frame layout visits every line and culling only skips text
shaping (`src/ui/text_editor.rs:982` onward).

## Raw benchmark output

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

### legacy_parser_drift

```
LD drift "~~~\ncode\n~~~": parse_markdown=["Paragraph", "Paragraph", "Paragraph"]  parse_document=["CodeFence", "CodeLine", "CodeFence"]
LD drift "* item": parse_markdown=["ListItem"]  parse_document=["ListItem"]
LD drift "+ item": parse_markdown=["Paragraph"]  parse_document=["ListItem"]
LD drift "Title\n=====": parse_markdown=["Paragraph", "Paragraph"]  parse_document=["Paragraph", "Paragraph"]
LD drift "1) item": parse_markdown=["Paragraph"]  parse_document=["ListItem"]
LD drift "    indented code": parse_markdown=["Paragraph"]  parse_document=["CodeLine"]
```

