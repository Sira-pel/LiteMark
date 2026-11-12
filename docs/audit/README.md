# Code audit — progress note (paused 2026-11-12)

Status: **paused, about one quarter done.** One of four audit lanes finished. The other three were stopped
mid-run and wrote no findings. Nothing in this folder has been through the senior-review pass or a final
check yet, so treat every item as a lead, not a confirmed defect. No source code was changed.

Audited commit: `ea03a57` (main).

## Resume log

Paused again on 2026-11-12, shortly after resuming. Lanes B and C were restarted and then stopped a few
minutes in, before either wrote a findings file. Nothing new is confirmed.

| Step | State |
|---|---|
| Lane B (parser, document model) | Stopped early, no findings. One partial result, below |
| Lane C (filesystem, security) | Stopped early, no findings, nothing to keep |
| Lane D (performance, code quality) | Not started; run it alone so timings are not disturbed |
| Senior review of lanes A to D | Not started |
| Final table (at most 25 findings) and PR grouping | Not started |

Partial result from lane B: the ignored randomized probe `diff_probe::incremental_parse_matches_full_parse`
passed with `DIFF_SEED` set to 1, 2, 3, 7, 42 and 1234 (about 18 s each, rustc 1.97). No divergence between
the incremental and the full parse on those seeds. Seed 99999 was started and not finished.

To resume: run lanes B, C and D from the start, at most two agents at a time, then the senior review.
The Linux build needs `libxkbcommon-dev` and `libxkbcommon-x11-dev`; `cargo test --no-run` takes about
3 minutes cold. The lane A numbers were taken with rustc 1.99; this session had 1.97.

## Scope of the audit

1. Logic bugs: panics, off-by-one, empty input, CRLF and bare `\r`, Unicode, very large files, stale state.
2. Performance: per-frame and per-keystroke work, full reparses and copies, O(n²) loops, UI-thread I/O, startup.
3. Inefficient code: duplication, dead code, overly long functions, wrong data structures.
4. Standards: clippy pedantic, `cargo fmt --check`, error handling, tests for risky paths, `cfg` boundaries.
5. Security: untrusted files, paths and symlinks, save safety, links and commands, `unsafe`/FFI,
   dependencies, secrets and CI permissions.

Planned deliverable: a table of at most 25 findings sorted by severity, then a grouping into small PRs,
security fixes first.

## How the work was split

| Lane | Area | State |
|---|---|---|
| A | Tooling results, dependencies, CI, secrets, `unsafe`/FFI, error handling in app/ui | **Done** — [lane-A-tooling-ffi-ci.md](lane-A-tooling-ffi-ci.md) |
| B | Markdown parser and document model: logic bugs, crafted input | Stopped mid-run, no findings written |
| C | Filesystem, paths, save safety, links, update check, PDF | Stopped mid-run, no findings written |
| D | Performance and code quality, with measurements | Stopped mid-run, no findings written |
| — | Senior review of all lanes, then final spot-check | Not started |

## Tool results (lane A)

| Command | Result |
|---|---|
| `cargo test` | 483 passed, 0 failed, 5 ignored |
| `cargo clippy --all-targets -- -D warnings` (the CI gate) | clean |
| `cargo clippy --all-targets -- -W clippy::pedantic` | 695 diagnostics; cast lints opened one by one, no bug found in them |
| `cargo fmt --check` | fails, 405 hunks |
| Repo's own `fmt-gate.sh` | fails, 167 stale baseline entries |
| `cargo audit` | 3 vulnerabilities, 18 warnings; 2 vulnerabilities apply to code we use |
| `cargo deny` | not installed, skipped |

## Findings so far (lane A only, not yet reviewed)

| Severity | Category | Location | Problem | Evidence |
|---|---|---|---|---|
| Critical | Security | `src/services/pdf_service.rs:3-4`, `Cargo.toml` (lopdf 0.34) | A ~20 KB PDF with 10,000 nested arrays overflows the stack in lopdf (RUSTSEC-2026-0187) and aborts the app, losing unsaved tabs. Runs on the UI thread. | Proven in a scratch crate against `lopdf::Document::load`; not driven through the GUI |
| Medium | Security | `Cargo.lock` (rustls-webpki 0.103.15), `src/services/update_service.rs:57-70` | RUSTSEC-2026-0382 weakens TLS for the daily update check. Impact is bounded: nothing is downloaded or executed. | Unproven, from the advisory and `cargo tree` |
| Medium | Security | `src/app/editor_events.rs:331-335` | Ctrl+click on a Markdown link passes any URL scheme (`file://`, UNC, custom handlers) to the OS with no allowlist. | Unproven, code read only |
| Medium | Security | `.github/workflows/release.yml:7-8` | `contents: write` is granted to every job, including the ones that run dependency build scripts. Releases are unsigned. | Unproven, configuration read |
| Medium | Standards | `.github/workflows/ci.yml:49-57` | The CI Format step exits 0 on a dirty tree, so formatting can never fail a PR. The local fmt baseline is stale since the file split. | Proven, gate output |
| Medium | Logic | `src/model/settings.rs:107-129` | Settings are written with `fs::write` (not atomic) and write errors are dropped. A truncated file silently resets to defaults. | Unproven, code read only |
| Low | Standards | `src/main.rs:302` | Window creation is unwrapped; a missing display aborts with a raw panic message. | Unproven |
| Low | Standards | `patches/xattr/src/sys/mod.rs:12` | The vendored patch uses `ENODATA` on every platform, which is wrong for macOS and the BSDs. No user-visible effect today. | Unproven |
| Low | Standards | `src/app/render.rs:6` and eight others | 45 functions over 100 lines; the longest is 1,779 lines. | Proven, clippy counts |

Also from lane A, with no defect found: the Win32 code in `src/ui/win_caption.rs` and the `renameat2` call in
`src/services/fs_service.rs` (reasoned from the API contracts, could not be run on Windows here), secrets and
personal paths in tracked files and history, and runtime logging. Details are in the lane A file.

## What is left

- Lanes B, C and D need to be run again from the start. Their partial proof tests were throwaway files in
  local worktrees and are not part of this branch.
- Lane C overlaps two lane A items (link schemes, settings writes) and should confirm or drop them with a
  real repro.
- Senior review of every finding, then the final table of at most 25 items and the PR grouping.

## Provisional fix grouping (from lane A only)

1. Security: upgrade lopdf, move PDF loading off the UI thread, add a size cap.
2. Security: `cargo update -p rustls-webpki`.
3. Security: scheme allowlist for link clicks (`http`, `https`, `mailto`).
4. Security: per-job permissions in `release.yml`; pin actions in `ci.yml` to commit SHAs.
5. Reliability: atomic settings writes through the existing `atomic_save`, and surface write errors.
6. Standards: run `cargo fmt` once, then make the CI Format step fail for real.
