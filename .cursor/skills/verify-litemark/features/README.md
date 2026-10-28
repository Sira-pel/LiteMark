# LiteMark verification map

This directory is the maintained source for verifying user-facing behavior of LiteMark. Read this index before driving, then use the matching feature file as the recipe.

The primary proof is headless. The desktop window is not isolated and is not the default drive target.

## Baseline preconditions

- Source `.cursor/skills/verify-litemark/scripts/env.sh` so `RUSTUP_TOOLCHAIN=stable` is set.
- Build with `.cursor/skills/verify-litemark/scripts/launch.sh`. It runs `cargo build` and does not start the GUI.
- Run `.cursor/skills/verify-litemark/scripts/doctor.sh` and require `doctor ok`, `bin=target/debug/litemark`, and `hook=src/markdown/render_dump.rs`.
- Markdown proofs use `LITEMARK_WRAP_COLS=40` (the default). That is the column count passed to `visual_rows`, not a measured pixel width.
- Never launch `target/debug/litemark` from this kit. `scripts/refuse-gui.sh` is the command for window journeys.

## Driving conventions

- Start markdown recipes from the committed fixture file named in the feature.
- Treat every command as literal. Keep paths and flags unchanged.
- Run render proofs through `scripts/render_dump.sh` and `scripts/prove-markdown.sh`.
- Run build, test, clippy, and fmt through `scripts/ci.sh`.
- `prove-markdown.sh red` prints `red_caught=<n>/<total>` and exits 0 only when every red input's `broken-line.txt` is present and its `missing-line.txt` is absent. That is not a product pass.
- Do not remove proof artifacts during cleanup.

## Proof and skip reporting

- Small markdown proofs compare the dump byte-for-byte with `expect.txt`. `large-doc` is generated from `seed.txt` and checked with `sha256=` plus `required-lines.txt`.
- Each assertion line is a full dump line. Do not accept a substring match.
- UI journeys call `scripts/refuse-gui.sh`, exit 2, and print `reason=notes_dir_unisolated` and `reason=vulkan_icd_missing`. Do not report them as verified through the render dump.
- Record `RUN_ID` in `artifacts/markdown-render/<RUN_ID>/meta-*.txt`.

## Feature entry contract

Each feature file starts with an H1 title and one paragraph describing the user-visible behavior. It then uses exactly four H2 sections in this order.

1. `Sub-features`
2. `How to get to it (user POV)`
3. `Driving it with control-litemark`
4. `Gotchas`

## Features

- [Markdown render](./markdown-render.md) — headings, emphasis, nested lists, tables, fenced code, links, images, wrapping, and a large document timed through `parse_document` and `visual_rows`.
- [Open a file](./open-file.md) — File → Open…, Ctrl+O, or a path argument.
- [Find and replace](./find-replace.md) — Edit → Find, Ctrl+F, and the Replace / All bar.
- [Format toggle](./format-toggle.md) — WYSIWYG bold, italic, link, Enter, and undo, checked by `scripts/format-toggle-gate.sh` against `fixtures/format-toggle/cases.tsv`.
- [Table ops](./table-ops.md) — GFM table insert, delete, move, and alignment, checked by `scripts/table-ops-gate.sh` against `fixtures/table-ops/cases.tsv`.
- [Table hover](./table-hover.md) — column pill, row pill, and corner grip on a rendered table. The controls call the table ops. Geometry and drag bytes are unit-tested. `refuse-gui.sh` exits 2; a headless Xvfb run is in that feature doc.
- [Notes sidebar](./notes-sidebar.md) — View → Sidebar and Ctrl+\\.
- [Theme](./theme.md) — settings gear, then Dark, Light, or System.
