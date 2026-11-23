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
