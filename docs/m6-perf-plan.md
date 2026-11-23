# Milestone 6: RAM, storage and robustness plan

Baseline: `fix/milestone-5` (PR 104, `14f8b0e`), `cargo test --release`: 869 pass, 10 ignored.
Branch for this work: `perf/milestone-6` (stacked on PR 104).

## 1. What was measured

### Real app (release build, Windows, private bytes after 8 s idle)

| Scenario | Private | Working set (peak) |
|---|---|---|
| Clean session, no note | 104 MB | 131 MB (198) |
| Clean session, 3.2 MB note (Raw) | 197 MB | 220 MB (277) |
| Your saved session (restores a 12.7 MB note), no new note | 338 MB | 360 MB (427) |
| Your saved session + 3.2 MB note | **1014 MB** | 962 MB (1081) |

The 3.2 MB sample is 20k blocks of: heading, blank, styled paragraph (bold, link, code, italic), bullet, task, blank.

### Heap (new ignored benchmark, `--features heap-count`)

`cargo test --release --features heap-count bench_ -- --ignored --nocapture --test-threads=1`

3.1 MB note, 120k lines:

| | Raw | WYSIWYG |
|---|---|---|
| Tab total (live heap) | 74.0 MiB | 79.5 MiB |
| Parse cache (`ParsedDocument`) | 52.9 MiB | 52.9 MiB |
| Paint rows (`EditorPaint::rows`) | 12.0 MiB | 20.3 MiB |
| Layout cache | 6.7 MiB | 4.9 MiB |

How the 52.9 MiB parse breaks down:

| Part | MiB | Note |
|---|---|---|
| `ParsedLine` itself (160 B × 120k) | 18.3 | 35 %; paid by every line, including blank ones |
| `styled_segments` (40 B each) | 12.2 | only styled paragraph lines |
| `offset_map_cached` | 6.7 | only lines with hidden inline markup |
| `inline_pairs` | 5.5 | |
| `hidden_ranges` | 3.4 | headings and list items hold only their prefix range |
| `display_text_owned` | 1.6 | |
| Checkpoints (7,501 × 160 B) | 1.2 | |
| Spare `Vec` capacity | about 5 | 620k slots allocated for 480k elements |

Two notes open (12.8 MB active + 3.2 MB):
- The 12.8 MB note alone: **288 MiB live in 1.09 M allocations**, about 600 B and 2.3 allocations per line. Windows adds about 16 B of heap header per allocation, roughly 17 MB more.
- After switching tabs, the inactive tab's parse is released (110 MiB live). **There is no leak.**
- The 1 GB in the real process is therefore the peak of both parses, plus freed memory the Windows heap keeps instead of returning to the OS.

### Already fixed (checked in code, nothing to do)

- Audit F03 (link scheme allowlist), F07 (iterative workspace walk with a depth cap), F14 (binary-searched list marker).
- `link_hits` already shares one empty `Arc`.
- Milestone 3 items: image LRU cache, zero-copy display text, optional offset maps, no `content_snapshot`.

## 2. Plan (ordered by measured win ÷ risk)

Each step is one commit with a regression test, plus a before and after number from the benchmark above.

### A. Parse memory (the biggest lever: about 17× note size today)

**A1. Boxed slices for the per-line vectors** (low risk, mostly mechanical)
- `hidden_ranges`, `inline_pairs` and `styled_segments` change from `Vec<T>` to `Box<[T]>`.
- Saves 24 B per line and removes all spare capacity. Expected about −8 MiB on the 3.1 MB sample (about −15 %).
- Construction sites: 9 struct literals in `parser.rs` and `parser/blocks.rs`; build with `Vec`, then `.into()`. The one reader that needs changing is `inline_format.rs:1196` (`.as_slice()` becomes `&*`).
- A trial patch compiled up to that one error; it was reverted when this plan was requested.
- Check: parse time in `bench_large_note_redraw` and the incremental-parse benchmark shouldn't regress. Converting with `.into()` reallocates only when capacity is larger than length.

**A2. Rare inline data behind one box**
- Move `inline_pairs`, `styled_segments`, `display_text_owned`, `offset_map_cached` and `link_hits` into `Option<Box<LineInline>>`.
- `ParsedLine` goes from 160 B to about 80 B. Blank, heading, list, code and plain lines then carry no inline allocation.
- Expected about −5 MiB on the dense sample, and more on prose or code-heavy notes, where most lines are plain.
- Cost: about 50 non-test reader sites move to accessor methods (`line.inline_pairs()` and so on). Mutation happens only in the parser's tail shift and line builders.
- Before starting: add plain-prose and code-heavy samples to the benchmark to confirm the win. On the dense sample, 4 of every 6 lines still need the box unless A3 is done too.

**A3. Derive the prefix hidden range from `prefix_len`** (measured after A2 and skipped: it would save 0.92 MiB, 2.3% of the parse cache. Merged ranges such as `- **b**` make the change risky.)
- Heading and list lines currently store one `hidden_ranges` entry that duplicates `prefix_len`.
- Deriving it removes one allocation per such line (about 2 MiB per 60k lines). It also lets A2 skip the box for those lines.
- Touches the 11 `hidden_ranges` reader sites. Gate: the existing WYSIWYG navigation and parse-equivalence tests.

**A4. Offset maps on demand (measure first)**
- 6.7 MiB, derivable from `hidden_ranges`. Build lazily for visible rows only if the redraw benchmark shows no slowdown.

### B. Paint and layout memory (12–20 MiB per 3 MB note)

**B1. Shrink `PaintRow`** (about 88 B per visual row)
- Use `u32` for `line_index`, `byte_start` and `byte_end`.
- Use `Option<Box<[PaintCell]>>` for `cells`, which is empty on non-table rows.
- Expected about −40 %.

**B2. Visible-only rows**
- Skipped in PR 104 because about 5 consumers expect every row to exist.
- Revisit only after B1, using the existing `viewport_culling_tests` equivalence test as the gate.

### C. Allocator and returning memory to the OS

**C1. Benchmark `mimalloc` RSS** (the item PR 104 left open)
- Rerun the real-app table above with `--features mimalloc`, adding the scenario of closing the large tab and measuring again.
- Make it the default if idle and after-close private bytes drop with no startup cost. That is the fix for the 1 GB peak that doesn't come back down.
- **Result: measured, not adopted. `mimalloc` stays opt-in** (it used more private bytes in every scenario).
  - Setup: real release binaries on Windows, Wysiwyg view, fresh settings (no tab restore), 3 runs per cell, median shown, sampled 8 s after each step. Notes: 3.6 MB (20,000 blocks) and 14.7 MB (80,000 blocks). Scenario 3 opens the big note, then hands off the small one so the big one becomes the inactive tab.
  - Private bytes in MB (working set in brackets); peak working set in the last column:

    | Scenario | default | mimalloc | delta private | peak WS default / mimalloc |
    |---|---|---|---|---|
    | 1. empty start | 102.8 (127.4) | 123.8 (128.7) | +21.0 | 196 / 197 |
    | 2. 3.6 MB note | 168.6 (191.8) | 212.6 (210.1) | +44.0 | 261 / 279 |
    | 3. 14.7 MB note, then 3.6 MB note | 191.0 (214.9) | 504.8 (476.3) | +313.8 | 471 / 524 |

  - Binary size: default 15,927,808 bytes, mimalloc 15,909,888 bytes (18 KB smaller, so size was not the problem).
  - Scenario 4 (close the big tab and drop caches in a test harness) was skipped: mimalloc already loses in scenarios 1–3, and the RSS probe needs a new `windows-sys` feature plus a gpui test.
  - Reading: mimalloc's private bytes include memory it keeps committed after frees (scenario 3 shows the biggest gap), the opposite of what C1 needed. Not tried: tuning `MIMALLOC_PURGE_DELAY` or `mi_collect`, which would add runtime config for an unproven gain.
  - Caveat: the default build's inactive-tab case (191 MB) is already far below the 1 GB seen earlier in the real session, which is consistent with the A1/A2/B1/C2 reductions landing since then.

**C2. Shrink after release**
- After `release_parse_cache` or a tab close, call `shrink_to_fit` on the tab's `content` when its capacity is far above its length.

### D. CPU and I/O robustness (small, independent)

- **D1. `disk_change`** reads the whole file into RAM on every save check (`fs_service.rs:466`). Stream-hash it in 64 KB chunks, and compare the file length first, since a size mismatch means the file changed without reading it.
  - The hash must stay identical to `content_hash`; add a test at chunk sizes 1, 7 and 65536.
  - Note: uncommitted edits doing exactly this were already in the working tree (`workspace.rs`, `fs_service.rs`) and weren't made in this session. Review them before redoing the work.
- **D2. `copy_into_note_assets`** reads up to 64 MB into memory (`fs_service.rs:580`). Stream it with `io::copy` into the same no-replace, unique-name path.
- **D3. PDF import** keeps the raw file bytes alive for the whole page loop. Drop them right after `load_mem_with_options`. Keep `EXTRACT_STACK_BYTES` unchanged: it is reserved address space, not RAM, and it guards the F01 stack-overflow fix.
- **D4. Undo eviction** uses `Vec::remove(0)` in a loop (`text_ops.rs:215`), which is O(n) per evicted entry. Switch to a `VecDeque`.
- **D5. `crash.log`** shows a 0.12.0 abort: re-entrant `LineField` update ("already being updated"). Find whether a later commit fixed it; if not, reproduce it in a gpui test and fix it. With `panic = "abort"` it loses unsaved tabs.

### E. Storage (binary 16.0 MB)

- **E1.** Exclude `icons/hicolor/*` and `windows/*` from `RustEmbed`; they are used only by packaging and `build.rs`. This needs rust-embed's `include-exclude` feature. Saves about 60 KB; grep `build.rs` and `scripts/` first.
- **E2.** Keep the fonts (2.4 MB): removing a weight changes rendering.
- **E3.** Optional: measure `opt-level = "s"` for the binary size versus the redraw benchmarks. Adopt it only if frame times don't move.
  - **Result: adopted `[profile.release.package."*"] opt-level = "z"`, with `gpui`, `pulldown-cmark`, `memchr` and `unicode-segmentation` kept at 3.** The app crate stays at 3 because `"*"` skips workspace members. Binary 15.93 MB to 14.64 MB (-8.1%).
  - Sizes (`litemark.exe`, bytes): base 15,927,808; `s` 14,093,824 (-11.5%); `z` 13,076,992 (-17.9%); `lto = "fat"` 15,191,552 (-4.6%, under the 5% bar, not benchmarked); deps `s` + hot at 3: 14,754,304 (-7.4%); deps `z` + hot at 3: 14,642,176 (-8.1%).
  - Speed: interleaved A/B, 3 rounds, baseline run before every candidate run (12 baseline runs, 3 per candidate). Medians in us, baseline / `s` / `z` / deps-`z`+hot-3:
    - Wysiwyg 100k idle redraw: 18297 / 17448 / 24353 / 17592
    - Raw 3k idle redraw: 824 / 933 / 1231 / 834
    - Raw 100k edit redraw: 7907 / 8225 / 9115 / 7938
    - apply_edit at top, styled: 4506 / 4581 / 4657 / 4588
    - same-length edit, styled: 180 / 201 / 217 / 174
  - Whole-crate `s` is 5-20% slower on Raw redraw and `z` is 25-35% slower on Wysiwyg redraw, so both are rejected. Deps-`z` with the hot crates at 3 is within noise on every number (best runs: Wysiwyg 100k idle 16911 vs 16245 us, styled top 4262 vs 4577 us).
  - Not measured: startup time (skipped as too noisy to resolve) and typing latency beyond the redraw-after-edit benchmark. Revert by deleting the `[profile.release.package.*]` sections in `Cargo.toml`.

## 3. Done means

- A1, A2 (if the prose and code samples confirm it), B1, C1, and D1–D5 each land as a commit with a test.
- The benchmark table above is re-measured and recorded in the PR body.
- `cargo test`, `cargo clippy --all-targets -- -D warnings` and the fmt gate all pass.
- The work goes up as a stacked PR on top of PR 104.

## 4. Tooling added on this branch (uncommitted)

- `heap-count` cargo feature: a counting global allocator in test builds only (`src/main.rs`). It reports live bytes and live allocations.
- `src/memory_bench_tests.rs`, both ignored:
  - `bench_note_heap`: per-structure breakdown and per-line histogram.
  - `bench_two_large_tabs`: live heap across opening a second note and switching tabs.
- Two `#[cfg(test)]` accessors (`parser.rs`, `document.rs`) that the benchmarks read.

## 5. Final checks (`e5a706d`)

### Heap (`bench_note_heap`, `bench_two_large_tabs`)

| | Before (`14f8b0e`) | Now |
|---|---|---|
| Dense 3.1 MB, parse cache | 52.9 MiB | 32.7 MiB |
| Dense 3.1 MB, paint rows (WYSIWYG) | 20.3 MiB | 8.7 MiB |
| Dense 3.1 MB, tab total (WYSIWYG) | 79.5 MiB | 47.8 MiB |
| 12.8 MB note alone, live | 288 MiB in 1.09 M allocs | 194 MiB in 1.01 M allocs |
| Big tab closed | n/a | 36.3 MiB |

### Real app (release, private bytes 8-9 s after launch, median of 3)

| Scenario | Before | Now |
|---|---|---|
| Clean session, no note | 104 MB | 115 MB (one run 103) |
| Clean session, 3.2 MB note | 197 MB | 173 MB |
| Saved session (12.7 MB note restored) | 338 MB | 267-279 MB |
| Saved session + 3.2 MB note | 1014 MB | **bimodal: 984-1127 MB in 8 of 17 runs, 173-174 MB in 9** |
| Binary size | 15.93 MB | 14.64 MB |

The last row is **not fixed**. The heap benchmark of the same two notes stays near 200 MiB, so the real app does something the benchmark doesn't, probably a startup race between restoring the 12.7 MB tab and opening the file passed on the command line. Next step: find what builds about 800 MB in that path.

### Speed (interleaved, 4 rounds, `ec6f034` pre-A2 vs `e5a706d`, best of 10 per run)

| Benchmark | `ec6f034` | `e5a706d` |
|---|---|---|
| WYSIWYG 100k idle redraw | 13.8-14.6 ms | 16.5-16.7 ms (**+20 %, every round**) |
| WYSIWYG 3k idle redraw | 0.88-1.07 ms | 1.01-1.11 ms |
| Raw 100k idle redraw | 6.6-7.3 ms | 6.1-7.3 ms |
| apply_edit at top, styled 100k | 5.25-5.47 ms | 3.91-4.16 ms (-25 %) |
| apply_edit at top, plain 100k | 585-682 us | 524-557 us |

The WYSIWYG regression on a 100k-line note where every line has bold text is real (about 2.8 ms). Candidates: A2 (inline box hop), B1, A4 (offset maps rebuilt on demand) and E3. Not yet bisected.
