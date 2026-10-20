# Markdown render

Markdown render is the editor's view of a note: each source line becomes a kind, a display string, styled segments, and wrapped rows.

## Sub-features

- `render-headings` shows `# Title` as heading level 1 with display `Title`.
- `render-emphasis` marks bold, italic, strike, and code segments.
- `render-lists` keeps nested indent, tasks, and ordered numbers.
- `render-tables` reads a header row, alignments, and body cells.
- `render-code` reads a rust fence and the code line.
- `render-links` keeps the URL `https://example.com/litemark` on the link text.
- `render-images` keeps the image markup in the display string and the alt text on one segment.
- `render-wrap` splits a long line at column 40.
- `render-large` generates the 2001-line document from `fixtures/large-doc/seed.txt` and records `LITEMARK_ELAPSED_US`.

## How to get to it (user POV)

- Open a markdown note in the editor. The window paints `parse_document` lines and wraps them with `visual_rows`.
- This kit feeds the fixture file instead of typing into the window, because the window is not isolated.

## Driving it with control-litemark

Preconditions:

- `scripts/launch.sh` has printed `READY bin=`.
- `scripts/doctor.sh` has printed `doctor ok`.
- `LITEMARK_WRAP_COLS` is `40`.

- **Headings.** Run `.cursor/skills/verify-litemark/scripts/prove-markdown.sh green`. Fixture `headings` passes only when the dump is byte-identical to `.cursor/skills/verify-litemark/fixtures/headings/expect.txt`. That file contains the exact line `line[0] kind=Heading level=1 source=0..7 display="Title"`.
- **Emphasis.** The same command's `emphasis` fixture contains the exact line `segment[0][1] source=9..13 bold=true italic=false underline=false strike=false code=false link=- text="bold"`.
- **Nested lists.** The `nested-lists` expect contains the exact line `line[1] kind=ListItem ordered=false number=0 task=none indent_cols=2 checkbox=- source=9..18 display="child"`.
- **Tables.** The `tables` expect contains the exact line `line[0] kind=TableRow header=true cells=2 source=0..14 display="| Name | Qty |"` and `cell[0][1] source=9..12 align=right text="Qty"`.
- **Fenced code.** The `fenced-code` expect contains the exact line `line[0] kind=CodeFence opening=true language="rust" source=0..7 display=""` and `line[1] kind=CodeLine source=8..20 display="fn main() {}"`.
- **Links.** The `links` expect contains the exact line `segment[0][1] source=5..13 bold=false italic=false underline=false strike=false code=false link="https://example.com/litemark" text="LiteMark"`.
- **Images.** The `images` expect contains the exact line `line[0] kind=Paragraph source=0..41 display="![Alt text](https://example.com/note.png)"` and `segment[0][0] source=2..10 bold=false italic=false underline=false strike=false code=false link=- text="Alt text"`. The display string keeps the `![` markup. There is no image kind.
- **Wrapping.** The `long-lines` expect contains the exact line `row[0][0] display=0..40 text="The quick brown fox jumps over the lazy "` and `row[2][0] display=0..40 text="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"`.
- **Large document.** `scripts/generate-large-doc.py` writes the input from `fixtures/large-doc/seed.txt`. The harness requires the exact line `sha256=7eac2c0a0471e1b121b9a3b8c6a11020b4e2e98cffb26387f1db057596bd092c` and every full line in `fixtures/large-doc/required-lines.txt`, including `bytes=70444`, `lines=2001`, and `wrap_cols=40`. The script prints `LITEMARK_ELAPSED_US=` from the parse. That number is not compared.
- **Red proofs.** Run `.cursor/skills/verify-litemark/scripts/prove-markdown.sh red`. It prints `red_caught=<n>/<total>` and exits 0 only when every fixture's `broken-line.txt` is a full dump line and its `missing-line.txt` is not. A smaller count exits 1.
- **Images.** `images-markup` feeds `![Alt text](https://example.com/note.png)` and requires `line[0] kind=Paragraph source=0..41 display="![Alt text](https://example.com/note.png)"`.
- **Hard wrap.** `long-lines-hard-wrap` feeds 50 `a` characters and requires `row[0][1] display=40..50 text="aaaaaaaaaa"`.
- **Trailing empty line.** `trailing-empty` feeds `# Section 1` plus a newline and requires `line[1] kind=Empty source=12..12 display=""`.
- **Renderer mutants.** `.cursor/skills/verify-litemark/scripts/prove-red-mutants.sh` changes the renderer once per fixture and requires `broken_line_absent=` for that broken line.
- **Bad large seed.** `.cursor/skills/verify-litemark/scripts/prove-large-doc-seed.sh` sets `sections=399` and requires `green_fail=1` before any render.
- **Green can fail.** Run `.cursor/skills/verify-litemark/scripts/prove-green-can-fail.sh`. It replaces `line[0] kind=Heading level=1 source=0..7 display="Title"` with `line[0] kind=Heading level=9 source=0..7 display="Title"`, requires `prove-markdown.sh green` to exit non-zero, then restores the expect file.

## Gotchas

- A trailing newline becomes a final `kind=Empty` line (`parse_document` appends one when the source ends with `\n`).
- `wrap_cols=40` is the harness column count. The editor derives columns from glyph width in `src/ui/text_editor.rs`. Do not treat 40 as that pixel width.
- Image syntax is not a `LineKind`. The alt text is a segment and the display string still contains `![Alt text](https://example.com/note.png)`.
- Table display text keeps the pipe row. Delimiter rows are `kind=TableDelimiter` with an empty display string.
- `prove-markdown.sh red` must print `red_caught=` equal to the total. Caught means `broken_exact_line=` is present and `missing_exact_line=` is absent. If the broken line is missing, the script exits 1.
- Do not bless a new dump by copying it over `expect.txt` unless the exact tokens above are still the ones you mean to require.
