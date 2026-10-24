# Format toggle

WYSIWYG inline formatting is applied through `DocumentTab`: bold, italic, links, Enter, and undo. The bytes saved in the note and the caret after the action are the proof.

## Sub-features

- `format-toggle-phrase` unbolds the first, middle, or last word of a bold phrase.
- `format-toggle-link` toggles a word inside a bold link, and types a space then a letter at the end of the link text.
- `format-toggle-run` toggles a word in a bold run that also holds an image, `<br>`, `<u>`, or a backslash escape.
- `format-toggle-partial` unbolds part of bold code or bold-italic text, including a letter glued to the span.
- `format-toggle-selection` toggles italic on a selection that starts in a bold word and ends in the next plain word, including a selection that crosses a line break.
- `format-toggle-caret` moves Left to the start of a bold word and toggles bold, and toggles a link with the caret at the start, middle, and end.
- `format-toggle-newline` presses Enter, Shift+Enter, or Enter in a table cell inside a span, including Enter inside nested emphasis.
- `format-toggle-type-undo` types after a toggle and undoes the edit.

## How to get to it (user POV)

- Open a markdown note in WYSIWYG.
- Select text, or place the caret, and press Ctrl+B, Ctrl+I, or the link command.
- Type, press Enter or Shift+Enter, move Left or Right, or undo.

## Driving it with control-litemark

Preconditions:

- `scripts/env.sh` has been sourced so `RUSTUP_TOOLCHAIN=stable` is set when rustup is present.
- The gate does not start the window.

- **Gate.** Run `.cursor/skills/verify-litemark/scripts/format-toggle-gate.sh`.
- **Pull requests.** Job `verify-kit` in `.github/workflows/ci.yml` runs that script on `opened`, `synchronize`, and `reopened`.
- **Rows.** The script reads `.cursor/skills/verify-litemark/fixtures/format-toggle/cases.tsv` through the ignored test `format_toggle_cases`. The pinned ids are `.cursor/skills/verify-litemark/fixtures/format-toggle/expected-ids.txt`.
- **Lines.** Stdout has one whole line `case=<id> pass|fail|xfail|xpass` per pinned id, then `pass=N fail=N xfail=N xpass=N`.
- **Exit.** Exit 0 only when every pinned id has exactly one of those lines, the result count is at least `MIN_CASES` (43), `fail=0`, and `xpass=0`. A missing id, an extra id, a different count, or a result count below that floor exits non-zero (`count_below_min count=<n> min=<m>`). Lowering `MIN_CASES` is the deliberate step for removing a case. `known_fail_main` rows that still match the recorded main bytes are `xfail`.
- **Plain tests.** `cargo test` does not run this test. The row is `#[ignore]`.

## Gotchas

- Expected bytes and the caret are exact. A substring match is not a pass.
- No fixture may expect a space directly before a closing emphasis marker. The trailing-space case is tested only by the follow-up-letter row (`m1-space-x`). `nf1-space` and `nf1-space-x` stay, because their closer follows `)`, not a space.
- `known_fail_main` means main does not produce the target. The target is the markdown for the formats the user asked for, with no stray markers. `main_bytes` and `main_caret` are what main emits. Matching those is `xfail`. Matching the target is `xpass`, and the row has to be updated. Anything else is `fail`.
- The actions call `apply_format_in_mode`, `insert_text_in_mode`, `smart_newline`, `move_left_in_mode`, `move_right_in_mode`, and `undo` in WYSIWYG. `parse_document` supplies the display length and segment flags recorded on a mismatch.
- Table-row `display_text` keeps the raw line. Hidden ranges cover the emphasis markers. The gate compares source bytes, not that display string.
