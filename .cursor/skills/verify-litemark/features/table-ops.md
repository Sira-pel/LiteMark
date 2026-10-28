# Table ops

A table edit rewrites one GFM table in the note and records one undo step. The bytes are the fixture literals. `parse_document` and pulldown-cmark read those literals back; the caret for a `cell:` row is the editor parser's cell start.

GFM 0.29 section 4.10 is the source of the bytes. Example 198 is the plain table. Example 199 is the colon alignment (`:---`, `:---:`, `---:`). The escaped-pipe example this repo calls gfm200 keeps `\|` in the source. Example 202's pipe-less `bar` stays byte for byte until a column edit rewrites it, and that rewrite takes the header's outer pipes. Example 203 is why the last column is not deleted. Example 204 keeps a short row short and keeps an extra cell such as `boo` on the line; the parsed table ignores the extra cell. A blank line ends a table, so a row that would become empty is `|  |`.

## What an edit keeps

- Untouched rows are copied byte for byte, including CRLF and a file that does not end in a newline.
- Line endings belong to the slot. Moving a row moves its text and leaves the endings where they are.
- A new cell copies the neighbour cell's leading and trailing spaces with the text removed. A new delimiter cell copies the hyphen count (at least 3) and the spaces, with no colons.
- Alignment rewrites only that delimiter cell.
- The header is row 0. It is not deleted, not moved, and not given a row above it. A body row is not moved to index 0.
- A rewritten row that would drop an empty edge cell, lose its last pipe, start a list/heading/quote/fence/html/footnote, or indent four spaces takes outer pipes even when the header had none, so the line stays a table. A cell that ends in a backslash is not followed by a glued pipe.
- A table inside a block quote or a list item is not a table in this parser, so the bytes stay and the edit does not record an undo step.

## Sub-features

- `table-ops-insert` inserts a row above or below, or a column left or right.
- `table-ops-delete` deletes a body row or a column, and refuses the header and the last column.
- `table-ops-move` moves a body row or a column. Undo then redo restores the bytes and the caret.
- `table-ops-align` sets a column to left, center, right, or none.
- `table-ops-selection` maps a selection through the replacement. A view switch does not move it. Hover is not part of this gate.

## How to get to it (user POV)

- Open a markdown note and put the caret in a table cell.
- Insert or delete a row or column, set the column alignment, or move a row or column.
- Undo once. The note returns to the previous bytes. Redo once. The edit returns.

The window is not started here. `scripts/refuse-gui.sh` exits 2 because the notes directory is unisolated and `/usr/share/vulkan/icd.d` is missing.

## Driving it with control-litemark

Preconditions:

- `scripts/env.sh` has been sourced so `RUSTUP_TOOLCHAIN=stable` is set when rustup is present.
- The gate does not start the window.

- **Gate.** Run `.cursor/skills/verify-litemark/scripts/table-ops-gate.sh`.
- **Pull requests.** Job `verify-kit` in `.github/workflows/ci.yml` runs that script on `opened`, `synchronize`, and `reopened`.
- **Rows.** The script reads `.cursor/skills/verify-litemark/fixtures/table-ops/cases.tsv` through the ignored test `table_ops_cases`. The pinned ids are `.cursor/skills/verify-litemark/fixtures/table-ops/expected-ids.txt`.
- **Lines.** Stdout has one whole line `case=<id> pass|fail` per pinned id, then `pass=N fail=N`.
- **Exit.** Exit 0 only when every pinned id has exactly one of those lines, the result count is at least `MIN_CASES` (180), and `fail=0`. A missing id, an extra id, a different order, or a result count below that floor exits non-zero (`count_below_min count=<n> min=<m>`).
- **Red.** `scripts/prove-table-ops-wrong-expect.sh` requires `case=insert-row-below-base fail`. `scripts/prove-table-ops-dropped-case.sh` requires `count_below_min`. Both restore the fixtures.
- **Plain tests.** `cargo test` does not run this test. The row is `#[ignore]`.

## Gotchas

- Expected bytes are exact. A substring match is not a pass.
- The editor signature keeps source escapes and backticks. Pulldown turns `\|` into `|` and a code span `` `c|d` `` into `c|d`. Those two columns are different on purpose.
- A table on the line after a paragraph, with or without a blank line, is a table in this parser and in pulldown 0.12. The `after-para` rows record both signatures, and they agree.
- A pipe inside a code span is cell text here. Pulldown 0.12's first pass counts that pipe, so the header and the delimiter disagree and pulldown reports no table. The `*-code` rows record pulldown `none`, except `delete-col-code`, where that cell is gone and pulldown sees the remaining table.
- Quote and list rows stay byte for byte. The editor signature is `none`. Pulldown still parses those lines as a table; that column records the table it sees.
- A word on the next line is another row (example 202). `sel-after-insert-col-left` puts a blank line before `Tail` so the selection ends after the table.
