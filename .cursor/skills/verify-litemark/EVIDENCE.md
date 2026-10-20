# Evidence

SHA: `f4a7e9b84e4bdbedf1adc15a027993ae2630fc2f`

Commands below ran on that commit (`verify/litemark-kit`). Run id `r3`. `scripts/env.sh` sets `RUSTUP_TOOLCHAIN=stable`. Logs are under `/tmp/litemark-verify-r3/evidence/`. `rustc 1.99.1 (58cf8f9c3 2026-10-13)`. `rustfmt 1.10.0-stable (58cf8f9c35 2026-10-13)`.

Stable is pinned by branch `cursor/linux-packaging-readme-4544`, blob `a24e5dd3af5ff2f6b0a7bfdad3530f4e580920db` (`.github/workflows/ci.yml`), step `dtolnay/rust-toolchain@stable`. `main` has no `rust-toolchain.toml`.

No packages were installed for this run.

## M1 `scripts/prove-ci-test-result.sh`

`.cargo/config.toml` set `runner = "true"` for the host triple, and `src/markdown/render_dump.rs` gained `fn zz_runner_should_fail` which panics. Both were restored. `ci.sh` exit 1.

```
===== test exit=0 log=/tmp/litemark-verify-r3/evidence/ci/test.log =====
test_result_ok=missing
ci_fail=1
proof=M1 rc=1 line=test_result_ok=missing
```

## M2 `scripts/prove-mutant-signals.sh`

`prove-red-mutants.sh` was signalled after `src/markdown/parser.rs` changed. Exit 0.

```
signal=SIGINT file=src/markdown/parser.rs sha_before=8d45a267aa7148b6ef9b0c1e09e6721487769c924b64c66aefc0c03b86f18b45 sha_after=8d45a267aa7148b6ef9b0c1e09e6721487769c924b64c66aefc0c03b86f18b45 src_byte_identical=1
signal=SIGINT file=src/model/text_ops.rs sha_before=8f8eda6a9807725ce93bcd525161275f3bfd48d7c466aa6dcfc2668c02e7fe57 sha_after=8f8eda6a9807725ce93bcd525161275f3bfd48d7c466aa6dcfc2668c02e7fe57 src_byte_identical=1
signal=SIGTERM file=src/markdown/parser.rs sha_before=8d45a267aa7148b6ef9b0c1e09e6721487769c924b64c66aefc0c03b86f18b45 sha_after=8d45a267aa7148b6ef9b0c1e09e6721487769c924b64c66aefc0c03b86f18b45 src_byte_identical=1
signal=SIGTERM file=src/model/text_ops.rs sha_before=8f8eda6a9807725ce93bcd525161275f3bfd48d7c466aa6dcfc2668c02e7fe57 sha_after=8f8eda6a9807725ce93bcd525161275f3bfd48d7c466aa6dcfc2668c02e7fe57 src_byte_identical=1
proof=M2 rc=0 line=src_byte_identical=1
```

## M3 `scripts/prove-run-id.sh`

Exit 0. The canary files under `/tmp/canary-victim` kept their sha256.

```
proof=M3 rc=1 line=run_id_rejected=/../canary-victim
proof=M3 rc=1 line=run_id_rejected=../canary-victim
proof=M3 rc=1 line=run_id_rejected=../../../home/ubuntu
proof=M3 rc=1 line=run_id_rejected=has space
cleanup_refuse path=/tmp/canary-victim
canary_survived=1
proof=M3 rc=1 line=cleanup_refuse path=/tmp/canary-victim
```

## M4 `scripts/prove-fmt-user-config.sh`

`HOME` pointed at a rustfmt.toml with `max_width = 40`. Exit 0.

```
fmt_gate=pass
fmt_observed_hunks=201
fmt_baseline_stale=0
proof=M4 rc=0 line=fmt_gate=pass
proof=M4 script=prove-fmt-absorb.sh rc=1 line=fmt_hunk_body=src/theme.rs:22
proof=M4 script=prove-fmt-collide.sh rc=1 line=fmt_hunk_body=src/model/settings.rs:1
proof=M4 script=prove-fmt-shift.sh rc=1 line=fmt_new_hunk=src/theme.rs:42
proof=M4 script=prove-fmt-red.sh rc=1 line=fmt_new_file=src/markdown/render_dump.rs
proof=M4 script=prove-fmt-stale.sh rc=1 line=fmt_baseline_stale=1
```

## M5 `scripts/prove-target-dir.sh`

`LITEMARK_BIN` was `$ROOT/target/debug/litemark` and `CARGO_TARGET_DIR` was `/tmp/else target`. Exit 0. The workspace binary's sha256 did not change.

```
READY bin=/tmp/else target/debug/litemark
stale_unused=1
proof=M5 rc=0 line=READY bin=/tmp/else target/debug/litemark
```

## P-L2 `scripts/prove-fmt-gate-pass-line.sh`

A `python3` stub that exits 0 was first on `PATH`, and `fn zz_fmt_red(){let _x=1;}` was appended to `src/markdown/render_dump.rs`. The file was restored. `ci.sh` exit 1.

```
===== fmt exit=0 =====
fmt_gate_pass=missing
ci_fail=1
proof=P-L2 rc=1 line=fmt_gate_pass=missing
```

## P-L4 `scripts/prove-doctor-bin.sh`

```
doctor fail bin=/bin/true not-litemark
proof=P-L4 rc=1 line=doctor fail bin=/bin/true not-litemark
```

## P-L5 `scripts/prove-seed-sha-shape.sh`

```
sha_shape_reject=sha256=zzzz
sha_shape_reject=sha256=00
sha_shape_ok=sha256=b171d00c7c6db19fec5b2b39da274c098a9b4041c432f58a26bf950e390a253c
proof=P-L5 rc=1 line=FAIL fixture=large-doc sha256_observed=sha256=b171d00c7c6db19fec5b2b39da274c098a9b4041c432f58a26bf950e390a253c sha256_expected=sha256=7eac2c0a0471e1b121b9a3b8c6a11020b4e2e98cffb26387f1db057596bd092c
```

`prove-large-doc-seed.sh` uses the same 64-hex check. Exit 0 of the proof, `prove-markdown.sh green` exit 1:

```
RED seed result=FAIL_AS_REQUIRED prove_exit=1
sha256_self=ok
FAIL fixture=large-doc sha256_observed=sha256=b171d00c7c6db19fec5b2b39da274c098a9b4041c432f58a26bf950e390a253c sha256_expected=sha256=7eac2c0a0471e1b121b9a3b8c6a11020b4e2e98cffb26387f1db057596bd092c
prove_markdown mode=green green_fail=1 artifacts=/workspace/.cursor/skills/verify-litemark/artifacts/markdown-render/r3
```

## P-L1 `scripts/prove-render-symlink.sh`

The headings oracle sha256 was unchanged. Exit 0.

```
render_refuse=symlink path=/tmp/litemark-verify-r3/symlink-expect.dump
proof=P-L1 rc=1 line=render_refuse=symlink path=/tmp/litemark-verify-r3/symlink-expect.dump
render_refuse=same_inode path=/workspace/.cursor/skills/verify-litemark/fixtures/headings/probe-hardlink.dump
oracle_sha_unchanged=1
proof=P-L1 rc=1 line=render_refuse=same_inode path=/workspace/.cursor/skills/verify-litemark/fixtures/headings/probe-hardlink.dump
```

## L-3 `scripts/prove-fmt-stale.sh`

`rustfmt --edition 2021 --config-path .cursor/skills/verify-litemark/rustfmt.toml src/theme.rs`, then the gate. `src/theme.rs` was restored. Gate exit 1.

```
fmt_baseline_hunks=201
fmt_observed_hunks=200
fmt_cargo_status=1
fmt_baseline_stale=1
fmt_gate=fail
proof=L-3 rc=1 line=fmt_baseline_stale=1
```

## P-L3

A `cargo` earlier on PATH that exits 0 is documented as out of scope in `SKILL.md` under Known limits. No code change.

## `scripts/ci.sh`

Exit 0. `ci_fail=0`.

```
===== build exit=0 log=/tmp/litemark-verify-r3/evidence/ci/build.log =====
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.32s
===== test exit=0 log=/tmp/litemark-verify-r3/evidence/ci/test.log =====
test result: ok. 110 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.02s
===== clippy exit=0 log=/tmp/litemark-verify-r3/evidence/ci/clippy.log =====
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.97s
warning: the following packages contain code that will be rejected by a future version of Rust: proc-macro-error2 v2.0.1
===== fmt: scripts/fmt-gate.sh =====
fmt_baseline_files=19
fmt_baseline_hunks=201
fmt_observed_hunks=201
fmt_cargo_status=1
fmt_baseline_stale=0
fmt_gate=pass
===== fmt exit=0 =====
ci_fail=0
```

## `prove-markdown.sh green`

Exit 0. `sha256_self=ok`. `sha256_ok=sha256=7eac2c0a0471e1b121b9a3b8c6a11020b4e2e98cffb26387f1db057596bd092c`. Large-doc `bytes=70444`, `lines=2001`, `wrap_cols=40`, `LITEMARK_ELAPSED_US=14662` (reported only).

```
prove_markdown mode=green green_fail=0 artifacts=/workspace/.cursor/skills/verify-litemark/artifacts/markdown-render/r3
```

## `prove-markdown.sh red`

Exit 0.

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
prove_markdown mode=red red_caught=9/9 artifacts=/workspace/.cursor/skills/verify-litemark/artifacts/markdown-render/r3
```

## `prove-red-mutants.sh`

Exit 0. `prove_red_mutants=pass`. Each row is rc 1 and `broken_line_absent=` for that fixture's broken line. Source was restored after each mutant, and `EXIT`/`INT`/`TERM` restore the backups.

## Fmt red fixtures

| Script | rc | line |
| --- | --- | --- |
| `prove-fmt-red.sh` | ci_exit=1 | `fmt_new_file=src/markdown/render_dump.rs` |
| `prove-fmt-absorb.sh` | gate_exit=1 | `fmt_hunk_body=src/theme.rs:22` |
| `prove-fmt-collide.sh` | gate_exit=1 | `fmt_hunk_body=src/model/settings.rs:1` |
| `prove-fmt-shift.sh` | gate_exit=1 | `fmt_new_hunk=src/theme.rs:42` |
| `prove-fmt-stale.sh` | gate_exit=1 | `fmt_baseline_stale=1` |

Absorption also printed `fmt_baseline_stale=1`. Collision printed `fmt_baseline_stale=5` and `fmt_observed_hunks=197`. Shift printed `fmt_baseline_stale=1`. The new-file case printed `fmt_baseline_stale=0`, `fmt_observed_hunks=202`, `fmt_gate=fail`, `ci_fail=1`.

## `cargo test`

Exit 0.

```
test result: ok. 110 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.02s
```

The ignored test is `markdown::render_dump::dump_render_tree`.

## `doctor.sh`

Exit 0. `bin=/workspace/target/debug/litemark`. `doctor ok`.
