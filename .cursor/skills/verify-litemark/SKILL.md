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
| Run | `cargo build` then `target/debug/litemark` (optional path argument). CI, on `origin/cursor/linux-packaging-readme-4544:.github/workflows/ci.yml`, uses Rust stable. `main` has no `.github` and no `rust-toolchain.toml`. `Cargo.lock` pins `ar_archive_writer` 0.5.3 (`rust-version = 1.88.0`). |
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

`main` has no workflow file. The only workflow in this remote is `origin/cursor/linux-packaging-readme-4544:.github/workflows/ci.yml`. `scripts/ci.sh` runs those steps:

```bash
source .cursor/skills/verify-litemark/scripts/env.sh
.cursor/skills/verify-litemark/scripts/ci.sh
```

Exact commands:

```bash
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

The format step matches CI: when `cargo fmt --check` exits non-zero, the step prints `Tree is not fmt-clean; skipping cargo fmt --check.` and does not fail `ci.sh`. The raw exit is still printed. That skip is not a formatting guarantee. `scripts/prove-fmt-red.sh` is the proof that `cargo fmt --check` can fail.

### Environment prerequisites (Linux)

Allowed apt packages used to link GPUI:

- `libxkbcommon-dev` 1.6.0-1build1
- `libxkbcommon-x11-dev` 1.6.0-1build1
- `libfontconfig1-dev` 2.15.0-1.1ubuntu2
- `libegl1` 1.7.0-1build1
- `libegl-mesa0` 25.2.8-0ubuntu0.24.04.4

Rust: stable via rustup (`dtolnay/rust-toolchain@stable` in that CI workflow), with `rustfmt` and `clippy`. Do not install `mesa-vulkan-drivers` for this kit. The screenshot path is not used.

## Doctor

```bash
source .cursor/skills/verify-litemark/scripts/env.sh
.cursor/skills/verify-litemark/scripts/doctor.sh
```

Doctor requires all of:

- `rustc` runs
- `target/debug/litemark` is executable
- `src/markdown/render_dump.rs` contains the exact line `fn dump_render_tree() {`
- every green fixture has `fixtures/<name>/input.md` and `fixtures/<name>/expect.txt`
- every red fixture has `input.md`, `expect.path`, and `missing-line.txt`

Doctor prints `doctor ok` or exits 1. It does not start the GUI.

## Drive

Harness name: **control-litemark**.

```bash
# One fixture. Writes the dump and prints LITEMARK_ELAPSED_US=<n>.
.cursor/skills/verify-litemark/scripts/render_dump.sh \
  .cursor/skills/verify-litemark/fixtures/headings/input.md \
  /tmp/headings.dump.txt

# Whole corpus. Green must be byte-identical to expect.txt.
.cursor/skills/verify-litemark/scripts/prove-markdown.sh green

# Red fixtures must fail. Exit 0 means each missing exact line was absent.
.cursor/skills/verify-litemark/scripts/prove-markdown.sh red

# CI steps.
.cursor/skills/verify-litemark/scripts/ci.sh

# Window features. Exit 2. Do not launch the app yourself.
.cursor/skills/verify-litemark/scripts/refuse-gui.sh
```

The dump test calls `parse_document` and then `visual_rows(&display_text, wrap_cols)` with `LITEMARK_WRAP_COLS` (default 40). The editor computes `wrap_cols` from pixel width in `src/ui/text_editor.rs`; this harness passes 40 so the row text is reproducible without a font shaper. Comparison is `cmp` of the whole dump, which is full-line equality, not a substring search.

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
- Green proof is `cmp -s expect.txt observed.dump`.
- Red proof names `missing_exact_line=` and the expect path. The observed dump must not contain that full line.
- `prove-markdown.sh red` exiting 0 means the reds failed as required.
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
| `scripts/ci.sh` | `cargo build`, `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` with CI's skip |
| `scripts/render_dump.sh` | One markdown file through the ignored dump test |
| `scripts/prove-markdown.sh` | Green corpus and red-fixture proofs |
| `scripts/prove-fmt-red.sh` | Seed `fn zz_fmt_red(){let x=1;}`, require `cargo fmt --check` to fail, restore the file |
| `scripts/refuse-gui.sh` | Exit 2 for window journeys |
| `scripts/cleanup.sh` | Keep evidence and artifacts |

All scripts are executable. Source `env.sh` in the same shell before calling the others when you set `RUN_ID` yourself.

## Maintenance

When the render tree, fixture tokens, or CI commands change, run `/maintain-verification-skill` and update the feature map plus this file together.
