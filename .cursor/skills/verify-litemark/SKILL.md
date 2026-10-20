---
name: verify-litemark
description: "Drive and prove LiteMark, a Rust + GPUI markdown note editor, with the CI cargo commands and a headless render-tree dump (parse_document + visual_rows) over a CommonMark fixture corpus. Use when verifying markdown rendering, wrapping, build, fmt, clippy, or tests, or when a renderer change needs a pass/fail dump."
---

# Verify LiteMark

Project-local verification for **LiteMark**, a Linux markdown note editor. The primary surface is the **GPUI desktop window**. This kit does not launch that window. The proof path is the production render tree: `parse_document` in `src/markdown/parser.rs` and `visual_rows` in `src/model/text_ops.rs`, which `EditorElement` uses to wrap each line.

Read `features/README.md` before driving. Drive one mapped feature per proof unless the task asks for more.

## Interview summary

| Question | Answer from this repo |
| --- | --- |
| Surface | GPUI desktop app (`src/main.rs`, `src/app.rs`). No browser, CLI, or IPC socket. |
| Run | `cargo build` then `target/debug/litemark` (optional path argument). `main` has no `.github` and no `rust-toolchain.toml`. Stable is pinned by branch `cursor/linux-packaging-readme-4544`, blob `a24e5dd3af5ff2f6b0a7bfdad3530f4e580920db` (`.github/workflows/ci.yml`), step `dtolnay/rust-toolchain@stable`. `Cargo.lock` pins `ar_archive_writer` 0.5.3 (`rust-version = 1.88.0`). |
| Drive | `control-litemark` scripts in this skill. Markdown goes through `scripts/render_dump.sh` (ignored test `markdown::render_dump::dump_render_tree`). Window journeys run `scripts/refuse-gui.sh` and stop. |
| Observe | Dump lines (`line[n]`, `segment[n][m]`, `cell[n][m]`, `row[n][m]`), `cargo` exit codes, and `LITEMARK_ELAPSED_US` for the large fixture. |
| Isolate | **Headless dump:** yes — it only reads the fixture path and writes the output path. **GUI:** no. `get_or_create_notes_dir` uses `dirs::document_dir()/Notes` and seeds `Welcome.md`. Do not start a second instance, and do not start the one this kit did not isolate. |

## Launch

There is no server. Launch builds the binary and leaves it unstarted.

```bash
export RUN_ID="${RUN_ID:-$(date +%Y%m%d%H%M%S)}"
source .cursor/skills/verify-litemark/scripts/env.sh
.cursor/skills/verify-litemark/scripts/launch.sh
```

Ready when `scripts/launch.sh` prints `READY bin=<path>` and that path is executable. It also prints `gui=not-started reason=notes-dir-not-isolated`.

`scripts/env.sh` sets `RUSTUP_TOOLCHAIN=stable` when `rustup` is present. Rustc 1.83 cannot parse the lockfile.

Teardown: `scripts/cleanup.sh`. It does not delete evidence or artifacts. It kills only a pid in `$VERIFY_RUN_DIR/app.pid`. Launch does not write that pidfile.

### CI commands

`main` has no `.github/workflows`. The commands come from branch `cursor/linux-packaging-readme-4544`, blob `a24e5dd3af5ff2f6b0a7bfdad3530f4e580920db` (`.github/workflows/ci.yml`). That blob is the provenance. This kit does not add a workflow to `main`.

Until `main` has a workflow, the kit's CI contract is:

```bash
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
.cursor/skills/verify-litemark/scripts/fmt-gate.sh
```

`scripts/ci.sh` runs that contract. The blob's fmt step prints `Tree is not fmt-clean; skipping cargo fmt --check.` and exits 0 when the tree is dirty. `ci.sh` does not copy that skip. `fmt-gate.sh` runs `cargo fmt --check` and allows dirt only for files in `fixtures/fmt-baseline.files` and hunk headers in `fixtures/fmt-baseline.hunks` (`path:line` from `Diff in`). Any other file prints `fmt_new_file=<path>` and fails. Any other hunk prints `fmt_new_hunk=<path:line>` and fails. A clean gate prints `fmt_gate=pass`.

```bash
source .cursor/skills/verify-litemark/scripts/env.sh
.cursor/skills/verify-litemark/scripts/ci.sh
```

`scripts/prove-fmt-red.sh` appends `fn zz_fmt_red(){let _x=1;}` to `src/markdown/render_dump.rs` (that path is absent from `fmt-baseline.files`), runs `ci.sh`, requires a non-zero exit and the exact line `fmt_new_file=src/markdown/render_dump.rs`, then restores the file.

### Environment prerequisites (Linux)

Allowed apt packages used to link GPUI:

- `libxkbcommon-dev` 1.6.0-1build1
- `libxkbcommon-x11-dev` 1.6.0-1build1
- `libfontconfig1-dev` 2.15.0-1.1ubuntu2
- `libegl1` 1.7.0-1build1
- `libegl-mesa0` 25.2.8-0ubuntu0.24.04.4

Rust: `rustup` stable, because that blob's step is `dtolnay/rust-toolchain@stable`. `main` does not pin a toolchain. Components: `rustfmt` and `clippy`. Do not install `mesa-vulkan-drivers`. The window is not started, so no Vulkan ICD is required.

## Doctor

```bash
source .cursor/skills/verify-litemark/scripts/env.sh
.cursor/skills/verify-litemark/scripts/doctor.sh
```

Doctor requires all of:

- `rustc` runs
- `target/debug/litemark` is executable
- `src/markdown/render_dump.rs` contains the exact line `fn dump_render_tree() {`
- every small green fixture has `fixtures/<name>/input.md` and `fixtures/<name>/expect.txt`
- `fixtures/large-doc/` has `seed.txt`, `input.sha256` (exact line `sha256=7eac2c0a0471e1b121b9a3b8c6a11020b4e2e98cffb26387f1db057596bd092c`), and `required-lines.txt` (exact line `lines=2001`)
- `fixtures/fmt-baseline.files` and `fixtures/fmt-baseline.hunks` exist
- every red fixture has `input.md`, `expect.path`, and `missing-line.txt`

Doctor prints `doctor ok` or exits 1. It does not start the GUI.

## Drive

Harness name: **control-litemark**.

```bash
# One fixture. Writes the dump and prints LITEMARK_ELAPSED_US=<n>.
.cursor/skills/verify-litemark/scripts/render_dump.sh \
  .cursor/skills/verify-litemark/fixtures/headings/input.md \
  /tmp/headings.dump.txt

# Whole corpus. Small dumps must be byte-identical to expect.txt.
# large-doc is generated from seed.txt; its sha256 must match input.sha256.
.cursor/skills/verify-litemark/scripts/prove-markdown.sh green

# Red fixtures. Prints red_caught=<n>/<total> and exits non-zero
# unless every red input is caught.
.cursor/skills/verify-litemark/scripts/prove-markdown.sh red

# Seed one wrong headings expect line, require green exit != 0, restore.
.cursor/skills/verify-litemark/scripts/prove-green-can-fail.sh

# CI contract, including the bounded fmt gate.
.cursor/skills/verify-litemark/scripts/ci.sh

# Seed a fmt break off the baseline. ci.sh must exit non-zero.
.cursor/skills/verify-litemark/scripts/prove-fmt-red.sh

# Window features. Exit 2. Do not launch the app yourself.
.cursor/skills/verify-litemark/scripts/refuse-gui.sh
```

The dump test calls `parse_document` and then `visual_rows(&display_text, wrap_cols)` with `LITEMARK_WRAP_COLS` (default 40). The editor computes `wrap_cols` from pixel width in `src/ui/text_editor.rs`; this harness passes 40 so the row text is reproducible without a font shaper. Small fixtures use `cmp` of the whole dump, which is full-line equality, not a substring search. `large-doc` is not committed: `scripts/generate-large-doc.py` expands `fixtures/large-doc/seed.txt` (400 sections) and the harness requires the exact line `sha256=7eac2c0a0471e1b121b9a3b8c6a11020b4e2e98cffb26387f1db057596bd092c` plus every full line in `required-lines.txt`, including `bytes=70444`, `lines=2001`, and `wrap_cols=40`.

`prove-markdown.sh red` prints `red_caught=<n>/<total>`. Exit 0 requires `red_caught` equal to the total (every missing exact line absent from its dump). A smaller count exits 1. `prove-green-can-fail.sh` replaces the exact line `line[0] kind=Heading level=1 source=0..7 display="Title"` with `line[0] kind=Heading level=9 source=0..7 display="Title"`, requires `prove-markdown.sh green` to exit non-zero and print `FAIL fixture=headings`, then restores the expect file.

Stable handles:

| Handle | Where |
| --- | --- |
| `line[n] kind=Heading level=1` | ATX heading in the dump |
| `segment[n][m] … bold=true` | emphasis span |
| `kind=ListItem … indent_cols=` | nested list |
| `kind=TableRow header=true` | table header |
| `kind=CodeFence opening=true language="rust"` | fenced code |
| `link="https://example.com/litemark"` | link segment |
| `row[n][m] display=<start>..<end> text=` | wrapped display row |
| `lines=<count>` | document line count, including the empty line a trailing newline adds |
| Menu `Open…`, `Find`, `Sidebar` | desktop; not driven |
| Settings labels `Dark`, `Light`, `System` | desktop; not driven |

Feature recipes live in `features/`.

## Evidence

```text
.cursor/skills/verify-litemark/artifacts/markdown-render/<RUN_ID>/
```

Working copy while the commands run:

```text
$VERIFY_RUN_DIR/evidence/    # default /tmp/litemark-verify-$RUN_ID/evidence/
```

`scripts/prove-markdown.sh` copies each dump into the artifacts directory before cleanup. `scripts/cleanup.sh` may delete `$VERIFY_RUN_DIR/logs` and never deletes `evidence/` or `artifacts/`.

Proof standards:

- The dump is the production functions, not a reimplementation of Markdown.
- Green proof for a small fixture is `cmp -s expect.txt observed.dump`.
- Green proof for `large-doc` is the exact line `sha256=7eac2c0a0471e1b121b9a3b8c6a11020b4e2e98cffb26387f1db057596bd092c` and every line of `required-lines.txt` present as a full dump line.
- Red proof names `missing_exact_line=` and the expect path. The observed dump must not contain that full line. The summary token is `red_caught=<n>/<total>`.
- `prove-green-can-fail.sh` must print `GREEN_CAN_FAIL exit=` with an exit other than 0, then `restored=`.
- `prove-fmt-red.sh` must print `fmt_new_file=src/markdown/render_dump.rs` and `ci_fail=1`.
- Do not report a refused window journey as verified.

## Cleanup

```bash
source .cursor/skills/verify-litemark/scripts/env.sh
.cursor/skills/verify-litemark/scripts/cleanup.sh
```

Rules:

- Launch does not start a process. If a later run wrote `$VERIFY_RUN_DIR/app.pid`, cleanup kills that pid only.
- Never `pkill` by the name `litemark`.
- Copy dumps into `artifacts/markdown-render/<RUN_ID>/` before deleting logs.
- Confirm `evidence/` and `artifacts/` still exist after cleanup.

## Helpers

| Script | Purpose |
| --- | --- |
| `scripts/env.sh` | `RUN_ID`, run dir, stable toolchain, wrap columns |
| `scripts/launch.sh` | `cargo build`; do not start the GUI |
| `scripts/doctor.sh` | Read-only harness health |
| `scripts/ci.sh` | CI contract: `cargo build`, `cargo test`, `cargo clippy --all-targets -- -D warnings`, `fmt-gate.sh` |
| `scripts/fmt-gate.sh` | Bounded `cargo fmt --check` against `fmt-baseline.files` and `fmt-baseline.hunks` |
| `scripts/generate-large-doc.py` | Expand `large-doc/seed.txt`; print `sha256=` and `bytes=` |
| `scripts/render_dump.sh` | One markdown file through the ignored dump test |
| `scripts/prove-markdown.sh` | Green corpus and red-fixture proofs (`red_caught=<n>/<total>`) |
| `scripts/prove-green-can-fail.sh` | Seed one wrong headings expect line, require green exit non-zero, restore |
| `scripts/prove-fmt-red.sh` | Seed `fn zz_fmt_red(){let _x=1;}` in `src/markdown/render_dump.rs`, require `ci.sh` to print `fmt_new_file=src/markdown/render_dump.rs` and exit non-zero, restore |
| `scripts/refuse-gui.sh` | Exit 2. Prints `reason=notes_dir_unisolated` and `reason=vulkan_icd_missing` |
| `scripts/cleanup.sh` | Keep evidence and artifacts |

All scripts are executable. Source `env.sh` in the same shell before calling the others when you set `RUN_ID` yourself.

## Known limits

These limits stay in the kit. They are why a window journey is not a proof.

- The GUI is not launched. `get_or_create_notes_dir` in `src/services/fs_service.rs` uses `dirs::document_dir()/Notes` and there is no override, so a start would write `Welcome.md` into the user documents directory. `scripts/refuse-gui.sh` prints the exact line `reason=notes_dir_unisolated path=dirs::document_dir()/Notes symbol=get_or_create_notes_dir file=src/services/fs_service.rs`.
- No Vulkan ICD is installed at `/usr/share/vulkan/icd.d`. `mesa-vulkan-drivers` is not an allowed install for this kit. `scripts/refuse-gui.sh` prints the exact line `reason=vulkan_icd_missing path=/usr/share/vulkan/icd.d`. The script then prints `command=not-run target/debug/litemark` and exits 2.
- Images are not a separate node in the dump. `![Alt text](https://example.com/note.png)` stays a paragraph. The exact line is `line[0] kind=Paragraph source=0..41 display="![Alt text](https://example.com/note.png)"`. The alt text is a segment: `segment[0][0] source=2..10 bold=false italic=false underline=false strike=false code=false link=- text="Alt text"`.

## Maintenance

When the render tree, fixture tokens, or CI commands change, run `/maintain-verification-skill` and update the feature map plus this file together.
