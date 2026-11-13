# Lane B: Markdown parser and document model

Status: complete

Toolchain: rustc/cargo 1.97, Linux x86_64, audited commit `ea03a57`. Read-only audit: no tracked source was
changed. Every repro is a throwaway `#[cfg(test)]` module in a scratch worktree, wired in with
`#[cfg(test)] mod lane_b_repro;` at the end of `src/model/document.rs` and run with
`cargo test --bin litemark <name> -- --ignored --nocapture --exact` (debug test profile). The repro code is
pasted under each finding because the worktree is discarded. Timings are debug-profile numbers; only the
growth rate is meaningful.

How text is stored: `DocumentTab.content` is the file's text, loaded verbatim (`fs::read` + `String::from_utf8`,
no BOM strip, no line-ending normalisation) and saved verbatim. `parse_document` only builds the display
projection; `markdown/exporter.rs` is used for HTML/plain-text export, not for saving. So open-then-save
without edits is byte-identical, and corruption can only come from edit operations.

Shared helper used by the repro tests:

```rust
use super::*;
use crate::model::table_ops::TableOp;
fn tab(src: &str) -> DocumentTab {
    DocumentTab::new("a".into(), "a.md".into(), src.to_string(), None, None)
}
```

## Findings

### Heading button/shortcut panics on a line indented with non-ASCII whitespace
- Severity: High
- Category: Logic
- Location: src/model/document/blocks.rs:200 (`&line[..indent.min(3)]` in `toggle_block_lines`); `indent` comes
  from `parse_line_block_prefix`, src/model/document/blocks.rs:496-497 (`line.trim_start()`, which strips Unicode
  whitespace, so `indent` is a byte count that can exceed 3 with a multi-byte character straddling byte 3).
- Problem: with the caret on a line whose leading whitespace is, for example, two no-break spaces
  (`"\u{a0}\u{a0}Title"`), or spaces followed by NBSP, U+2028 or U+3000, applying any heading level (H1 to H6, toolbar or shortcut, in
  WYSIWYG or Raw mode) slices the line in the middle of a character and panics. Release builds use
  `panic = "abort"`, so the app closes and every unsaved tab is lost. NBSP-indented lines are what pasting from
  web pages and word processors produces. Precondition is the unusual indent; the action itself is ordinary.
- Evidence: PROVEN on the unmodified tree.
  ```rust
  #[test] #[ignore]
  fn f1_heading_on_nbsp_indent_panics() {
      let mut t = tab("\u{a0}\u{a0}Title\n");   // two NBSPs, then text
      t.caret.move_to(6);
      t.apply_format_in_mode("h1", ViewMode::Wysiwyg);
  }
  ```
  Output:
  ```
  thread '...f1_heading_on_nbsp_indent_panics' panicked at src/model/document/blocks.rs:200:22:
  end byte index 3 is not a char boundary; it is inside '\u{a0}' (bytes 2..4 of string)
     at ./src/model/document/blocks.rs:200:22
     at ./src/model/document/inline_format.rs:34:18
  ```
  The op fuzzer hit the same site six times (NBSP and U+2028 indents, both view modes, h2/h3/h4/h6).
- Suggested fix: count the indent in ASCII spaces/tabs only (`trim_start_matches([' ', '\t'])`), or floor the
  cut to a char boundary before slicing.

### Editing a CRLF Markdown file writes bare LF line breaks (and Delete table leaves a bare CR)
- Severity: Medium
- Category: Logic
- Location: src/model/document/newline.rs:138-166 and the rest of `smart_newline` (hard-coded `"\n"`; the
  CRLF-aware `line_break`, newline.rs:489, is only called from `code_newline` at :515 and from blocks.rs:322);
  src/model/document/table.rs:68 (`table_insert_row_below`), :238 (`insert_table_with_size`), :330-354
  (`table_delete_table*`), :359 (`tab_table_cell`); src/model/document/blocks.rs:265 (`toggle_code_block`).
- Problem: in a `.md` file whose line endings are all CRLF (anything written on Windows), pressing Enter or
  Shift+Enter anywhere (paragraph, heading, list item, quote, fenced code), Tab in the last table cell,
  Ctrl+Enter "row below", Insert table and Code block all insert `\n` instead of `\r\n`. The file is saved with
  mixed line endings. Delete table removes the rows but leaves one `\r` behind, which the parser treats as an
  extra blank line. No text is lost, which is why this is Medium and not High, but it happens on every Enter
  and is silent. Non-Markdown tabs are fine (they go through `code_newline`), as are the menu table ops in
  `table_ops.rs` (Insert row below / Insert column keep CRLF), block toggles, indent and move-line.
- Evidence: PROVEN. Test (each case starts from a pure-CRLF document; `stray_endings` lists every LF without a
  CR before it and every CR without an LF after it):
  ```rust
  fn stray_endings(s: &str) -> Vec<usize> {
      let b = s.as_bytes();
      (0..b.len()).filter(|&i| (b[i] == b'\n' && (i == 0 || b[i - 1] != b'\r'))
          || (b[i] == b'\r' && b.get(i + 1) != Some(&b'\n'))).collect()
  }
  #[test] #[ignore]
  fn f2_crlf_document_gets_mixed_endings() {
      let table = "para\r\n\r\n| a | b |\r\n|---|---|\r\n| c | d |\r\n\r\nend\r\n";
      let cell_d = table.find("d |").unwrap();
      let cases: Vec<(&str, &str, usize, Box<dyn Fn(&mut DocumentTab)>)> = vec![
          ("Tab in last table cell", table, cell_d, Box::new(|t| { t.tab_table_cell(false); })),
          ("Ctrl+Enter row below", table, cell_d, Box::new(|t| { t.table_insert_row_below(); })),
          ("Insert row below (menu)", table, cell_d, Box::new(|t| { t.apply_table_op(TableOp::InsertRowBelow); })),
          ("Delete table", table, cell_d, Box::new(|t| t.table_delete_table())),
          ("Insert table 2x2", "para\r\n\r\nend\r\n", 4, Box::new(|t| t.insert_table_with_size(2, 2))),
          ("Code block", "para\r\n\r\nend\r\n", 2, Box::new(|t| t.apply_format_in_mode("codeblock", ViewMode::Wysiwyg))),
          ("Enter in paragraph", "one two\r\nend\r\n", 3, Box::new(|t| t.smart_newline(false))),
          ("Shift+Enter in paragraph", "one two\r\nend\r\n", 3, Box::new(|t| t.smart_newline(true))),
          ("Enter in list item", "- one two\r\n- x\r\n", 5, Box::new(|t| t.smart_newline(false))),
          ("Enter in fenced code", "```\r\ncode\r\n```\r\n", 7, Box::new(|t| t.smart_newline(false))),
      ];
      for (name, src, caret, op) in &cases {
          let mut t = tab(src);
          t.caret.move_to(*caret);
          t.refresh_cursor();
          op(&mut t);
          eprintln!("F2 {name:26} stray={:?} after={:?}", stray_endings(&t.content), t.content);
      }
  }
  ```
  Output (abridged to the relevant lines):
  ```
  F2 Tab in last table cell     MIXED at [39] after="para\r\n\r\n| a | b |\r\n|---|---|\r\n| c | d |\n|  |  |\r\n\r\nend\r\n"
  F2 Ctrl+Enter row below       MIXED at [39] after="para\r\n\r\n| a | b |\r\n|---|---|\r\n| c | d |\n|  |  |\r\n\r\nend\r\n"
  F2 Insert row below (menu)    ok    after="para\r\n\r\n| a | b |\r\n|---|---|\r\n| c | d |\r\n|  |  |\r\n\r\nend\r\n"
  F2 Delete table               MIXED at [6] after="para\r\n\r\r\n\r\nend\r\n"
  F2 Insert table 2x2           MIXED at [4, 5, 13, 29, 37, 38] after="para\n\n|  |  |\n| :--- | :--- |\n|  |  |\n\n\r\n\r\nend\r\n"
  F2 Code block                 MIXED at [3, 8] after="```\npara\n```\r\n\r\nend\r\n"
  F2 Enter in paragraph         MIXED at [3] after="one\n two\r\nend\r\n"
  F2 Enter inside bold          MIXED at [10] after="a **bold**\n **text** x\r\nend\r\n"
  F2 Shift+Enter in paragraph   MIXED at [3] after="one\n two\r\nend\r\n"
  F2 Enter in list item         MIXED at [5] after="- one\n-  two\r\n- x\r\n"
  F2 Enter in fenced code       MIXED at [7] after="```\r\nco\nde\r\n```\r\n"
  F2 Enter in quote             MIXED at [5] after="> one\n>  two\r\n"
  F2 Enter in heading           MIXED at [5] after="# one\n two\r\nx\r\n"
  F2 Bullet list on 2 lines     ok    after="- one\r\n- two\r\n"
  F2 Move line down             ok    after="two\r\none\r\n"
  ```
- Suggested fix: route every inserted break in the Markdown paths through `self.line_break(offset)` (it already
  exists and picks the local ending), and make table deletion remove the whole ending via `line_ending_len`.

### Block-format toggle over a large selection is quadratic in the number of lines
- Severity: Low
- Category: Performance
- Location: src/markdown/parser/lists.rs:357-368 (`accepted_list_marker_in` does `doc.lines.iter().find(..)`, a
  linear scan from the top, for every selected line); callers in src/model/document/blocks.rs:176 and :496-520
  (`toggle_block_lines` -> `parse_line_block_prefix`), two to three calls per line.
- Problem: Select All in a long document followed by a list / quote / heading toggle takes time proportional
  to lines squared, on the UI thread. Needs a few thousand lines to be felt; reported here only because it
  turned up in the scaling sweep (lane D owns general performance and should time it in release).
- Evidence: PROVEN (scaling). Select All + `apply_format_in_mode("ol")`, debug build:
  ```
  doc "1. a\n" x 2000 lines: 171.8 ms | x 4000: 658.3 ms (x3.8) | x 8000: 2232.3 ms (x3.4)
  doc "a **b** c\n" x 2000:  228.3 ms | x 4000: 898.8 ms (x3.9) | x 8000: 3025.5 ms (x3.4)
  ```
  Every other operation in the same sweep doubled when the input doubled.
- Suggested fix: look the line up with `doc.line_index_at(line_start)` (binary search, already exists).

### Format toggle on a line with thousands of nested spans is quadratic
- Severity: Low
- Category: Performance
- Location: src/model/document/inline_format.rs:11 (`apply_format_in_mode`, caret-in-word path that rewrites the
  whole line through `collect_rewrite_slices` / `render_work_slices`, src/model/document/inline_rewrite.rs:1055, :401)
- Problem: on one line made of n nested spans (`**a _b ` x n, `a`, ` b_ a**` x n) typing a character and then
  pressing Bold rewrites the line in time proportional to n squared. Parsing the same line is linear. At
  n = 20,000 (280 KB, a deliberately crafted line) the UI thread was busy for 34.8 s in a debug build. The
  rewrite itself is correct (the line is flattened to equivalent sibling spans, visible text unchanged).
  Needs both a crafted file and a format command on that line, hence Low.
- Evidence: PROVEN (scaling, debug build). Sequence: caret mid-line, `insert_text_in_mode("x")`, then
  `apply_format_in_mode("bold")`:
  ```
  n=250  (3.5 KB) bold=14ms | n=500 (7 KB) bold=40ms | n=1000 (14 KB) bold=132ms | n=2000 (28 KB) bold=463ms
  n=20000 (280 KB): type + bold + backspace = 34808.9 ms; parse of the same text = 238.7 ms
  ```
- Suggested fix: find the quadratic step in the slice merge (profile `render_work_slices`), or cap the rewrite
  to the pairs that actually cover the toggled word.

### `segment_raw_range` underflows when a document starts with a backslash escape (debug builds only)
- Severity: Low
- Category: Logic
- Location: src/model/document/inline_rewrite.rs:779 (`content.as_bytes().get(start - 2)` with `start == 1`)
- Problem: if the first two bytes of the document are a backslash escape (`\<`, `\*`, `\|`, `\#` ...) and a
  WYSIWYG selection on that line is replaced or reformatted, `start - 2` underflows. Debug and test builds
  panic with "attempt to subtract with overflow". The release profile does not enable `overflow-checks`, so
  there the subtraction wraps to `usize::MAX`, `.get()` returns `None` and the result is the intended one; no
  user-visible effect in shipped builds. It does abort `cargo run` sessions and would fail any test that
  reaches it.
- Evidence: PROVEN in the debug profile (release behaviour reasoned from the wrapping semantics, not run):
  ```rust
  #[test] #[ignore]
  fn lb_repro_a() {
      let mut t = tab("\\<Hello **bold** and\n");
      t.caret.move_to(2);
      t.insert_text_in_mode(8..10, "    ", ViewMode::Wysiwyg);
  }
  ```
  Output: `panicked at src/model/document/inline_rewrite.rs:779:35` (via `inline_rewrite.rs:1085`,
  `typing.rs:253`). The op fuzzer hit it 15 times, always with content starting with `\`.
- Suggested fix: `start.checked_sub(2).and_then(|i| bytes.get(i)) != Some(&b'\\')`.

### Case-insensitive Find never matches `İ` (and any character whose lowercase is two code points)
- Severity: Low
- Category: Logic
- Location: src/model/text_ops.rs:821-836 (`needle_chars = needle.chars().count()` counts the lowercased
  needle) and :841-869 (`caseless_match_len` counts source characters against that number).
- Problem: with "match case" off, searching for `İ` in `İstanbul` finds nothing, although the exact-case
  search finds it. `"İ".to_lowercase()` is two code points, so the matcher wants two source characters to be
  consumed and a one-character match can never satisfy `taken == count`.
- Evidence: PROVEN. `find_matches("İstanbul", "İ", false)` -> `[]`; `find_matches("İstanbul", "İ", true)` ->
  `[0..2]`. (Other probes behaved: `K`/`k`, `ǅ`/`ǆ`, `ΌΣΟΣ`/`όσος` all match.)
- Suggested fix: drive the loop by "needle exhausted" instead of a source-character count.

### A UTF-8 BOM hides the first line's block syntax
- Severity: Low
- Category: Logic
- Location: src/markdown/parser.rs:1970 (`parse_document`, no BOM handling); file load keeps the BOM in `content`
- Problem: a file saved as "UTF-8 with BOM" that starts with `# Title` shows the first line as a plain
  paragraph with a literal `# ` in WYSIWYG mode. The bytes on disk are not changed.
- Evidence: PROVEN. `parse_document("\u{feff}# Title\n").lines[0]` -> `kind=Paragraph display="\u{feff}# Title"`.
- Suggested fix: skip a leading U+FEFF when classifying the first line (keep it in `content` so it round-trips).

### Small edit-result oddities (grouped)
- Severity: Low
- Category: Logic
- Location: src/model/document/blocks.rs:20 (`indent`); src/model/document/newline.rs (ordered-list continuation)
- Problem: (a) Select All + Tab on `"- one\n- two\n"` gives `"  - one\n  - two\n  "`: the empty "line" after
  the final newline is indented too, so the file gains trailing spaces at EOF. (b) Enter at the end of
  `999999999. x` writes `1000000000. `, which has ten digits and is not a list marker (CommonMark allows nine),
  so the new line is a paragraph. The `u64 as usize` cast lane A flagged at lists.rs:101 is fine: the digit loop
  stops at nine digits, so the value is at most 999,999,999 on any target.
- Evidence: PROVEN. Output: `MISC indent_all after="  - one\n  - two\n  "` and
  `MISC ol_max after="999999999. x\n1000000000. " kinds=["ListItem", "Paragraph"]`.
- Suggested fix: skip an empty last line when indenting a selection; stop continuing the list (or cap) at
  999,999,999.

## Checked and found fine

- `diff_probe::incremental_parse_matches_full_parse` with `DIFF_SEED` = 5, 99999, 0xDEADBEEF: `ok` each
  (13.6 to 14.3 s). With the earlier seeds 1, 2, 3, 7, 42, 1234 that makes nine seeds, no DIVERGE or PANIC.
  Note: `DocumentTab::parsed()` is a full `parse_document` cached by revision, so there is no separate
  incremental path to diverge; the probe is in effect a panic fuzzer.
- Parser fuzz, 920,000 random documents (1 to 40 tokens from a 70-token alphabet: CR, CRLF, LF, tabs, NUL, BOM,
  U+2028, NBSP, emphasis/strike/backtick/tilde runs, brackets, links, `<u>`, HTML block starts, pipes, escaped
  pipes, delimiter rows, list markers incl. `999999999. `, footnote and reference definitions, emoji, ZWJ
  sequences, combining marks, Khmer), seeds 1 to 4. No panic; line sources ascending, in range and on char
  boundaries; `hidden_ranges` sorted, inside the line, on char boundaries; styled segments and inline pairs in
  range and on boundaries; `display_to_source` in range; `visible_text(0..len)` does not panic.
- Edit-operation fuzz, 103,000 runs of 40 operations each over seven seed documents (CRLF, bare CR, emoji/ZWJ/
  combining text, ragged table with escaped pipes, one-column table without final newline, empty document,
  tilde fences), WYSIWYG and Raw: typing any token, all 21 format commands, backspace, delete, Enter,
  Shift+Enter, undo, redo, indent/outdent, word/line deletes, move line, every `TableOp`, table row below,
  edge exit, insert/delete table, all caret moves, find/replace (including empty term, `\n`, `\r\n`), snippets,
  `place` past the end. Only the two panics reported above; with those two lines patched locally, seeds 2 to 6
  (100,000 runs) are clean. The caret was in range and on a char boundary after every step.
- Undo/redo exactness, 30,000 runs (seeds 11 to 13): undoing everything always restored the original bytes,
  and redoing step by step reproduced every intermediate document exactly.
- Parse time is linear for every pathological shape tried at n, 2n and 4n (n = 2000 repeats, up to 160 KB):
  unclosed `*`, `**`, `_`, `~~`, `[`, `![`, `[a](`, backticks (single, repeated, growing runs), backslashes,
  `<u>`, `<`, `&`, one long plain line, a 4000-column table, `>` x n, `- ` x n, 8000 list/quote/paragraph/
  setext/footnote/reference-definition lines, unclosed fence, HTML block, bare-CR lines. Worst case 0.77 ms/KB.
- Deep nesting on a 1 MB thread stack (the Windows main-thread size), n = 20,000: `>` x n, `> ` x n, `- ` x n,
  `[` x n, balanced brackets, nested links, nested images, `*` x n ... `*` x n, nested `*`/`_`/`~~`/`<u>`,
  `[a](((((`, `<div>` x n, 3000 growing backtick runs (4.5 MB), 20,000 reference uses, `&` x n, `<http://a` x n.
  `parse_document`, `export_to_html` and `export_to_plaintext` all returned; no stack overflow; each under
  0.5 s in debug.
- IME path (`src/app/input_handler.rs`): `range_from_utf16` always lands on char boundaries and clamps to the
  content length; `marked_range` is reset by every `edit`; the auto-pair and wrap-selection branches only add
  ASCII, so the `+ 1` caret arithmetic is right. Read only, not driven through gpui.
- Find/replace: empty term returns no hits and `replace_current_match` refuses it; matches do not overlap
  (`from = end`); `find_next`/`find_prev` index arithmetic holds for empty and single-hit lists (fuzzed).
