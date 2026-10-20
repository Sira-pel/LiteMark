# Evidence

Run id `r2`. `scripts/env.sh` sets `RUSTUP_TOOLCHAIN=stable`. Logs are under `/tmp/litemark-verify-r2/evidence/`. `rustc 1.99.1 (58cf8f9c3 2026-10-13)`. `rustfmt 1.10.0-stable (58cf8f9c35 2026-10-13)`.

Stable is pinned by branch `cursor/linux-packaging-readme-4544`, blob `a24e5dd3af5ff2f6b0a7bfdad3530f4e580920db` (`.github/workflows/ci.yml`), step `dtolnay/rust-toolchain@stable`. `main` has no `rust-toolchain.toml`.

The fmt baseline is the tip after the `src/markdown/mod.rs` reorder: 19 files, 201 hunks. `cargo fmt --check` on `main` (`fc681a8`) is 20 files and 202 hunks. The only header on main that the tip does not have is `src/markdown/mod.rs:1`. The tip adds no dirty hunk.

`ci.sh` does not apt-install the blob's `libxcb1-dev` or `libfreetype6-dev`. It runs build, test, clippy, and the fmt gate even when an earlier step fails. GitHub Actions stops the job at the first failing step.

## `scripts/ci.sh`

Exit 0. `ci_fail=0`.

```
===== build exit=0 log=/tmp/litemark-verify-r2/evidence/ci/build.log =====
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.73s
===== test exit=0 log=/tmp/litemark-verify-r2/evidence/ci/test.log =====
test result: ok. 110 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.02s
===== clippy exit=0 log=/tmp/litemark-verify-r2/evidence/ci/clippy.log =====
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.16s
warning: the following packages contain code that will be rejected by a future version of Rust: proc-macro-error2 v2.0.1
===== fmt: scripts/fmt-gate.sh =====
fmt_baseline_files=19
fmt_baseline_hunks=201
fmt_observed_hunks=201
fmt_cargo_status=1
fmt_gate=pass
===== fmt exit=0 =====
ci_fail=0
```

`fmt_cargo_status=1` is `cargo fmt --check` on the pre-existing tree. Every hunk token is on the baseline.

## `cargo test`

Exit 0. Same summary line:

```
test result: ok. 110 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.02s
```

The ignored test is `markdown::render_dump::dump_render_tree`.

## `scripts/prove-markdown.sh green`

Exit 0.

```
sha256_self=ok
sha256_ok=sha256=7eac2c0a0471e1b121b9a3b8c6a11020b4e2e98cffb26387f1db057596bd092c input=/tmp/litemark-verify-r2/large-doc/input.md
PASS fixture=headings ... LITEMARK_ELAPSED_US=177
PASS fixture=emphasis ... LITEMARK_ELAPSED_US=241
PASS fixture=nested-lists ... LITEMARK_ELAPSED_US=209
PASS fixture=tables ... LITEMARK_ELAPSED_US=206
PASS fixture=fenced-code ... LITEMARK_ELAPSED_US=107
PASS fixture=links ... LITEMARK_ELAPSED_US=189
PASS fixture=images ... LITEMARK_ELAPSED_US=244
PASS fixture=long-lines ... LITEMARK_ELAPSED_US=187
PASS fixture=large-doc ... LITEMARK_ELAPSED_US=14877
bytes=70444
lines=2001
wrap_cols=40
prove_markdown mode=green green_fail=0 artifacts=/workspace/.cursor/skills/verify-litemark/artifacts/markdown-render/r2
```

Long-line rows: `row[0][0] display=0..40 text="The quick brown fox jumps over the lazy "` and `row[2][0] display=0..40 text="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"`.

## `scripts/prove-large-doc-seed.sh`

`sections=399`. Exit 0 of the wrapper. `prove-markdown.sh green` exit 1. No `PASS fixture=` and no `LITEMARK_ELAPSED_US=`. Seed restored to `sections=400`.

```
RED seed result=FAIL_AS_REQUIRED prove_exit=1
seed=sections=399
sha256_self=ok
FAIL fixture=large-doc sha256_observed=sha256=b171d00c7c6db19fec5b2b39da274c098a9b4041c432f58a26bf950e390a253c sha256_expected=sha256=7eac2c0a0471e1b121b9a3b8c6a11020b4e2e98cffb26387f1db057596bd092c
prove_markdown mode=green green_fail=1 artifacts=/workspace/.cursor/skills/verify-litemark/artifacts/markdown-render/r2
```

## `scripts/prove-markdown.sh red`

Exit 0. `red_caught=9/9`.

```
broken_exact_line=line[0] kind=Paragraph source=0..5 display="Title"
broken_exact_line=segment[0][0] source=0..30 bold=false italic=false underline=false strike=false code=false link=- text="Normal bold italic strike code"
broken_exact_line=line[1] kind=ListItem ordered=false number=0 task=none indent_cols=0 checkbox=- source=9..16 display="child"
broken_exact_line=line[0] kind=Paragraph source=0..8 display="Name Qty"
broken_exact_line=line[0] kind=Paragraph source=0..4 display="rust"
broken_exact_line=segment[0][0] source=0..17 bold=false italic=false underline=false strike=false code=false link=- text="See LiteMark now."
broken_exact_line=line[0] kind=Paragraph source=0..41 display="![Alt text](https://example.com/note.png)"
broken_exact_line=row[0][1] display=40..50 text="aaaaaaaaaa"
broken_exact_line=line[1] kind=Empty source=12..12 display=""
red_caught=9/9
prove_markdown mode=red red_caught=9/9 artifacts=/workspace/.cursor/skills/verify-litemark/artifacts/markdown-render/r2
```

## `scripts/prove-red-mutants.sh`

Exit 0. `prove_red_mutants=pass`. Each `prove-markdown.sh red <fixture>` exited 1.

| fixture | mutant | rc | line |
| --- | --- | --- | --- |
| headings-missing-marker | parse_document default kind Paragraph to Blockquote | 1 | `broken_line_absent=line[0] kind=Paragraph source=0..5 display="Title"` |
| emphasis-no-markers | Text segment bold forced true | 1 | `broken_line_absent=segment[0][0] source=0..30 bold=false italic=false underline=false strike=false code=false link=- text="Normal bold italic strike code"` |
| nested-lists-flat | unordered list indent_cols + 7 | 1 | `broken_line_absent=line[1] kind=ListItem ordered=false number=0 task=none indent_cols=0 checkbox=- source=9..16 display="child"` |
| tables-not-a-table | paragraph display_text gains ! | 1 | `broken_line_absent=line[0] kind=Paragraph source=0..8 display="Name Qty"` |
| fenced-code-no-fence | fence detector also matches line rust | 1 | `broken_line_absent=line[0] kind=Paragraph source=0..4 display="rust"` |
| links-plain | Text segment link_url forced to mutant://link | 1 | `broken_line_absent=segment[0][0] source=0..17 bold=false italic=false underline=false strike=false code=false link=- text="See LiteMark now."` |
| images-markup | display starting with ![ replaced by ALT | 1 | `broken_line_absent=line[0] kind=Paragraph source=0..41 display="![Alt text](https://example.com/note.png)"` |
| long-lines-hard-wrap | visual_rows skips wrapping | 1 | `broken_line_absent=row[0][1] display=40..50 text="aaaaaaaaaa"` |
| trailing-empty | trailing newline does not append an empty line | 1 | `broken_line_absent=line[1] kind=Empty source=12..12 display=""` |

`images-plain`, `long-lines-short`, and `large-doc-tiny` are gone. Their inputs could not emit the token they named.

## Fmt reds

Clean gate before these seeds: `fmt_gate=pass`, `fmt_observed_hunks=201`, `fmt_cargo_status=1`.

### `scripts/prove-fmt-absorb.sh`

`let _new_break=1;` inside the `src/theme.rs:22` hunk. Gate exit 1. Header stayed `:22`. Observed hunks stayed 201.

```
RED fmt result=FAIL_AS_REQUIRED gate_exit=1
seed=let _new_break=1;
absorbed_header=src/theme.rs:22
fmt_hunk_body=src/theme.rs:22
fmt_baseline_files=19
fmt_baseline_hunks=201
fmt_observed_hunks=201
fmt_cargo_status=1
fmt_gate=fail
```

### `scripts/prove-fmt-collide.sh`

`rustfmt --edition 2021 src/model/settings.rs`, then `fn zz_coincide(){let _x=1;}` at line 1. Gate exit 1. Observed hunks 197. The settings header is still `:1` and the body is the new function.

```
RED fmt result=FAIL_AS_REQUIRED gate_exit=1
seed=fn zz_coincide(){let _x=1;}
colliding_header=src/model/settings.rs:1
fmt_hunk_body=src/model/settings.rs:1
fmt_baseline_files=19
fmt_baseline_hunks=201
fmt_observed_hunks=197
fmt_cargo_status=1
fmt_gate=fail
```

### `scripts/prove-fmt-shift.sh`

20 `// pad` lines. Gate exit 1.

```
RED fmt result=FAIL_AS_REQUIRED gate_exit=1
seed=20 lines // pad
fmt_new_hunk=src/theme.rs:42
fmt_baseline_files=19
fmt_baseline_hunks=201
fmt_observed_hunks=201
fmt_cargo_status=1
fmt_gate=fail
```

### `scripts/prove-fmt-red.sh`

`fn zz_fmt_red(){let _x=1;}` appended to `src/markdown/render_dump.rs`. `ci.sh` exit 1. File restored.

```
RED fmt result=FAIL_AS_REQUIRED ci_exit=1
seed=fn zz_fmt_red(){let _x=1;}
file_not_on_baseline=src/markdown/render_dump.rs
fmt_new_file=src/markdown/render_dump.rs
fmt_new_hunk=src/markdown/render_dump.rs:196
fmt_baseline_files=19
fmt_baseline_hunks=201
fmt_observed_hunks=202
fmt_cargo_status=1
fmt_gate=fail
ci_fail=1
```

## `scripts/doctor.sh`

```
rustc=rustc 1.99.1 (58cf8f9c3 2026-10-13)
bin=/workspace/target/debug/litemark
hook=/workspace/src/markdown/render_dump.rs
test=markdown::render_dump::dump_render_tree
wrap_cols=40
gui=refused notes_dir=dirs::document_dir()/Notes
doctor ok
```

## Installs

- rustup toolchain `stable` — `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, `rustfmt 1.10.0-stable`, `clippy`
- `libxkbcommon-dev` `1.6.0-1build1`
- `libxkbcommon-x11-dev` `1.6.0-1build1`
- `libfontconfig1-dev` `2.15.0-1.1ubuntu2`
- `libegl1` `1.7.0-1build1`
- `libegl-mesa0` `25.2.8-0ubuntu0.24.04.4`

`libfreetype-dev` `2.13.2+dfsg-1ubuntu0.2` was pulled in by `libfontconfig1-dev`. `mesa-vulkan-drivers` was not installed. `libxcb1-dev` and `libfreetype6-dev` from the workflow blob are not installed by `ci.sh`.
