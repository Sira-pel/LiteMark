# Code audit — final report (commit `ea03a57`)

Status: **complete.** All four audit lanes (A–D), the senior review pass, the consolidated final findings table (25 items), and the small-PR remediation plan are finished. **No source code, build configuration, or workflow files were modified.**

Audited commit: `ea03a57` (`main`).

## Audit status & resume log

| Step | State | Notes |
|---|---|---|
| Lane A (tooling, CI, FFI) | **Done** — [lane-A-tooling-ffi-ci.md](lane-A-tooling-ffi-ci.md) | 9 findings + checked inventory; `clippy`, `fmt-gate.sh`, `Cargo.lock` re-verified |
| Lane B (parser, document model) | **Done** — [lane-B-parser-model.md](lane-B-parser-model.md) | 8 initial leads; B1–B3, B5–B8 re-verified; B4 dropped in senior review |
| Lane C (filesystem, security) | **Done** — [lane-C-filesystem-security.md](lane-C-filesystem-security.md) | 12 findings; non-GPUI repros (incl. `LD_PRELOAD` `EINVAL` & 1 MiB stack/fd checks) re-verified |
| Lane D (performance, code quality) | **Done** — [lane-D-performance-quality.md](lane-D-performance-quality.md) | 8 findings; startup, memory/`ParsedDocument`, undo eviction, B3/B4, drift & dead-code completed |
| Senior review of lanes A to D | **Done** (below) | Reconciled cross-lane duplicates, verified locations/claims on `ea03a57`, settled severities |
| Final table (25 findings) & PR grouping | **Done** (below) | Sorted by severity; grouped into small fix PRs, security fixes first |

## Verification boundary on the continuation VM

All findings were verified directly against unmodified source files at `ea03a57`:
- **Re-executed directly (`rustc 1.99.1`, Linux x86_64)**: `cargo clippy --all-targets -- -D warnings` (passes), `.cursor/skills/verify-litemark/scripts/fmt-gate.sh` (`fmt_observed_hunks=405`, `fmt_baseline_stale=167`, `fmt_gate=fail`), and every non-GPUI repro and benchmark across Lanes A–D (`src/markdown`, `src/services`, `src/model`, plus `LD_PRELOAD` `RENAME_NOREPLACE` shim and `ulimit -n` stack/FD tests) in debug and release builds.
- **Code-verified + preserved from `ea03a57` GPUI test-harness runs**: GPUI binaries and `cargo test --no-run` cannot link on this VM because `libxkbcommon-x11-dev` is absent (`rust-lld: unable to find library -lxkbcommon-x11`). For the GPUI-driven items (`F03`/`C2`, `F09`/`C3`, `F12`/`D1`, `F13`/`D2`, `F15`/`D4` frames, `F24`/`C11`), exact call paths and line numbers were verified directly in `src/`, paired with the `ea03a57` GPUI `TestAppContext` measurements recorded in Lanes C and D.

## Senior review disposition (Lanes A–D)

Cross-lane reconciliation against `ea03a57` produced the 25-item table below:

- **Dropped (1)**:
  - **Lane B4** (*nested-span bold toggle quadratic*): Re-tested in `--release` at `n = 20,000` (`273 KB` single line): parse = `29.3 ms`, type = `27.1 ms`, bold = `0.40 ms` (linear, not quadratic; Lane B's `34.8 s` was a debug-build artifact).
- **Merged across lanes (4)**:
  - **Lane A3 -> C2 (`F03`)**: Both flagged unfiltered Markdown link destinations passed to `cx.open_url`. Adopted Lane C's **High** severity because GPUI on Windows passes the raw string to `ShellExecuteW` (launching executables or UNC SMB/NTLM connections from a single Ctrl/Cmd+click on a misleading label).
  - **Lane A6 -> C8 (`F21`)**: Both flagged non-atomic `settings.json` writes, dropped errors, and silent resets on parse error. Adopted Lane C's **Low** severity (only preferences/recent paths are lost; startup opens nothing from `settings.json`), incorporating the missing `zoom_level` serde default (`0` -> 0 px rows) and lack of bounds clamping.
  - **Lane B3 -> D3 (`F14`)**: Same root cause (`accepted_list_marker_in` linear scan in `src/markdown/parser/lists.rs:357-368`). Raised from Lane B's **Low** to **Medium** after release measurements showed `12.4 s` at 1 MB and `326 s` at 5 MB.
  - **Lane A9 -> D8 (`F25`)**: Folded A9 (45 functions `> 100` lines / `render.rs` boilerplate) with D8 (drifted `parse_markdown` legacy parser and 41 `allow(dead_code)` attributes) into one Code Quality entry.
- **Additional nuance discovered during re-verification (`C6` -> `F07`)**:
  - `scan_workspace_dir`'s recursive `walk` (`src/services/fs_service.rs:652-717`) keeps `fs::read_dir` handles alive across recursion frames until each `if let Ok(entries)` block returns. On Linux with `ulimit -n 1024`, scans silently stop at **depth 1,019** (`EMFILE`); with `ulimit -n 4096` (or Windows/macOS paths > 1,019), a 2,039-level tree overflows a 1 MiB stack and aborts **even in release**.
- **Low-severity consolidations & deferred notes (to keep final table `<= 25`)**:
  - Grouped **C9 + C10 (`F22`)** (filename handling edges in `fs_service.rs`), **B5 + B6 + B7 + B8 (`F23`)** (verified model/parser edge bugs), and folded **A7** (`open_window().unwrap()` at `src/main.rs:302`), **A8** (`patches/xattr/src/sys/mod.rs:12` Linux-only `ENODATA`), and **D7** (`ParsedDocument` ~20x active-tab heap amplification, bounded on inactive tabs by `release_parse_cache()` at `src/app/editor_events.rs:491-493`) into related rows (**D7** noted alongside `F15`; **A7/A8** documented as minor hygiene notes in Lane A and PR 12 below).
  - **Lane A's Win32 review (`win_caption.rs` / `header_bar.rs`)**: Contains no confirmed defect (`cfg(windows)` only, reasoned sound by API contract); excluded from the defect table.
  - **CI observation**: Note that on `ea03a57`, `test-desktop (macos-latest)` is sensitive to debug timing on `test_20000_row_table_parse_fast` (`1000 ms` debug threshold; `218 ms` debug / `29 ms` release on Linux x86_64).

## Final findings table (25 findings, sorted by severity)

| ID | Severity | Category | Src | Location | Problem | Status |
|---|---|---|---|---|---|---|
| F01 | **Critical** | Security | A1 | `src/services/pdf_service.rs:3-4`, `Cargo.toml` (`lopdf 0.34.0`), `src/app/commands.rs:363` | Opening a ~20 KB PDF with 10,000 nested `[` arrays overflows the stack in `lopdf` (`RUSTSEC-2026-0187`) on the UI thread, hard-aborting the process (`panic = "abort"`) and losing unsaved tabs. | **Verified** (release & debug `exit 134` at `ea03a57`) |
| F02 | **Critical** | Security | C1 | `src/services/pdf_service.rs:3-29`, `src/app/commands.rs:363` | No cap on PDF input bytes, page count, or extracted text. Multi-page PDFs referencing one compressed stream amplify ~476 KB to 11.5 GB RAM and SIGABRT/OOM on UI thread; persists on `lopdf 0.47.0`. | **Verified** (`lopdf` 0.34: 200p -> 400 MB md / 777 MB HWM in 2.3s) |
| F03 | **High** | Security | C2+A3 | `src/app/editor_events.rs:331-335`, `src/markdown/parser.rs:1436`, `src/ui/text_editor.rs:436` | Ctrl/Cmd+click on a WYSIWYG Markdown link hands any `dest_url` verbatim to `cx.open_url` (no scheme allowlist), enabling UNC NTLM leaks, local binaries, relative paths, and custom URL handlers (`ms-msdt:`, etc.). | **Code-verified** (`ea03a57` GPUI harness repro) |
| F04 | **High** | Logic | B1 | `src/model/document/blocks.rs:200`, `:496-497` | Heading toggle (`h1`–`h6`) slices `&line[..indent.min(3)]` where `indent` counts multi-byte Unicode whitespace from `trim_start()`, panicking (aborting in release) on `\u{a0}`/`\u{2028}`/`\u{3000}` indents. | **Verified** (release & debug panic at `blocks.rs:200:22`) |
| F05 | **Medium** | Security | A2 | `Cargo.lock` (`rustls-webpki 0.103.15`), `src/services/update_service.rs:57-70` | `RUSTSEC-2026-0382` weakens TLS intermediate certificate verification during daily GitHub update checks (bounded impact: nothing is auto-downloaded or executed). | **Verified** (`Cargo.lock` & code inspected) |
| F06 | **Medium** | Security | A4 | `.github/workflows/release.yml:7-8`, `:36` | `release.yml` grants `permissions: contents: write` to every job (including dependency build scripts), picks unpinned `stable` Rust at runtime, and publishes unsigned artifacts without provenance. | **Verified** (workflow config inspected) |
| F07 | **Medium** | Security | C6 | `src/services/fs_service.rs:652-717` (`walk`), `src/app.rs:217`, `src/app/tabs.rs:56` | Recursive workspace scan has no depth cap and holds `ReadDir` FDs across recursion: silently truncates scans at `EMFILE` (depth 1,019 under `ulimit -n 1024`) and stack-overflow aborts a 1 MiB stack at depth 2,039 even in release. | **Verified** (depth 1,019 truncation; `exit 134` at `ulimit -n 4096`) |
| F08 | **Medium** | Logic | B2 | `src/model/document/newline.rs:138-166`, `table.rs:68,238,330-359`, `blocks.rs:265` | Editing CRLF `.md` files inserts bare `\n` on Enter/Shift+Enter, code-block toggle, insert table, row below, and Tab in last cell; Delete table leaves a stray `\r` in place of the removed newline. | **Verified** (release & debug) |
| F09 | **Medium** | Logic | C3 | `src/app/tabs.rs:6-41`, `:83-93`, `src/app/sidebar.rs:242`, `render.rs:637` | `save_tab` does not track mtime/hash and silently overwrites files changed or deleted externally on disk; clicking an already-open file in the sidebar discards the fresh `read_note` result. | **Code-verified** (`ea03a57` GPUI harness repro) |
| F10 | **Medium** | Logic | C5 | `src/services/fs_service.rs:212-275` (`write_temp`, `rename`) | `atomic_save` always replaces the target inode: splits hard links (`nlink > 1`), resets owner/group/xattrs/ACLs, overwrites 0444 files in writable dirs without warning, and fails in read-only dirs. | **Verified** (hard-link split in release; uid drop from C) |
| F11 | **Medium** | Logic | C7 | `src/services/fs_service.rs:421` (`safe_rename`), `:460` (`move_note`), `:480` (`move_folder`) | Only `atomic_save_new` falls back on `ENOSYS`/`EINVAL` when `renameat2(RENAME_NOREPLACE)` is unsupported (NFS/FUSE on Linux); rename and drag-and-drop move fail with `Invalid argument`. | **Verified** (`LD_PRELOAD` `EINVAL` shim in release) |
| F12 | **Medium** | Performance | D1 | `src/ui/sidebar.rs:143-223`, `:368-384`, `:614` | When sidebar is open, every frame rebuilds, lowercase-sorts, and clones the entire tree and materializes all row elements without virtualization (`248 ms` at 2k files; `635–794 ms` idle / `3.7 s` selected at 5k files). | **Code-verified** (`ea03a57` GPUI frame timing) |
| F13 | **Medium** | Performance | D2 | `src/ui/text_editor.rs:1325-1650` (`shape_table`, `compute_table_column_widths`) | Placing a caret anywhere inside a table forces full-table shaping across all rows with no row-level viewport culling, hashing the full table every frame and shaping cells 2–3x on cache miss (`115–137 ms` at 2kx8; `320–344 ms` at 5kx8). | **Code-verified** (`ea03a57` GPUI frame timing) |
| F14 | **Medium** | Performance | D3+B3 | `src/markdown/parser/lists.rs:357-368`, `src/model/document/blocks.rs:176,496-520` | Block-format toggle (`ol`/`quote`/`heading`) calls `doc.lines.iter().find(...)` linearly per selected line, causing $O(n^2)$ UI stalls (`12.4 s` at 1 MB, `326 s` at 5 MB). | **Verified** (release: `42.9x` time for `8x` lines) |
| F15 | **Medium** | Performance | D4(+D7) | `src/model/document.rs` (`parsed`), `src/ui/text_editor.rs:982-1016, 1090` | Every keystroke reparses the entire document (`~29 ms/MB`, ~21 MB cached `ParsedDocument` heap/MB, freed on inactive tabs via `release_parse_cache`), and every frame walks all lines (`53–59 ms` key+frame at 1 MB, `351–376 ms` at 5 MB). | **Model verified** (`ea03a57` GPUI frame timing) |
| F16 | **Medium** | Performance | C4 | `src/services/fs_service.rs:330-337` (`read_note`), `src/main.rs:48` | Sidebar lists unknown/binary files (`.iso`, media); `read_note` reads the entire file on the UI thread before testing the first 8 KiB for NULs (256 MB sparse -> 779 MB HWM / 169 ms) and hangs indefinitely on FIFOs. | **Verified** (release 256 MB sparse `.iso`; FIFO hang in C) |
| F17 | **Medium** | Performance | D6 | `src/app/tabs.rs:26,40,56`, `src/app/commands.rs:343-410`, `drag_drop.rs`, `render.rs:1477` | `save_tab` unconditionally runs synchronous `rescan_workspace()` on the UI thread on every `Ctrl+S` (20.7 ms warm for 5k files; cold/network mounts stall UI), alongside sync Open/Export/Copy/`is_dir()` checks. | **Code + scan timing verified** |
| F18 | **Medium** | Logic | D5 | `src/model/text_ops.rs:13,191-194`, `src/model/document.rs:625-634` | `replace_every_match` records a full-document `2 * doc.len()` `DiffEdit` via `set_content`; on notes $\ge 129\text{ KB}$, typing 1 character immediately evicts the Replace All undo entry (`undo_bytes > 256 KiB`). | **Verified** (release at 200 KB, 300 KB, 1 MB) |
| F19 | **Medium** | Standards | A5 | `.github/workflows/ci.yml:49-57`, `.cursor/skills/verify-litemark/scripts/fmt-gate.sh` | CI `Format` step echoes `"skipping"` and exits `0` on dirty trees; `fmt-gate.sh` is unused in CI and fails locally (`fmt_observed_hunks=405`, `fmt_baseline_stale=167`, ~31 unlisted files). | **Verified** (`fmt-gate.sh` run on `ea03a57`) |
| F20 | **Low** | Security | C12 | `src/services/update_service.rs:18-21, 47-51`, `src/app/render.rs:1588-1635` | Update check checks `html_url.starts_with(...)` without normalizing `..`/`%2e%2e` (allowing arbitrary `github.com` redirects given API/TLS control) and renders raw unbounded/newline `tag_name` strings in the banner. | **Verified** (release & debug) |
| F21 | **Low** | Logic | C8+A6 | `src/model/settings.rs:35-41, 107-129` | `EditorSettings::save` truncates `settings.json` non-atomically and drops I/O errors; corrupt/unknown JSON silently wipes settings; `{}` loads `zoom_level = 0` (0 px invisible rows) and `show_sidebar = false`; numeric fields lack bounds clamping. | **Verified** (release & debug; 0 px rows in C) |
| F22 | **Low** | Logic | C9+C10 | `src/services/fs_service.rs:74-83, 234, 382` | `safe_rename` trims leading dots (`.gitignore` -> `gitignore`), and `atomic_save`'s `.{file_name}.{u64}.litemark-tmp` temp filename adds up to 37 bytes so files >~218 bytes open normally but fail every save (`ENAMETOOLONG`). | **Verified** (release & debug) |
| F23 | **Low** | Logic | B5–B8 | `inline_rewrite.rs:779`, `text_ops.rs:821-869`, `parser.rs:1970`, `blocks.rs:20`, `newline.rs` | Edge bugs: (B5) debug-only `start - 2` underflow panic on doc-start `\` escape; (B6) caseless find misses `İ`; (B7) UTF-8 BOM hides `#` heading on line 0; (B8) Select-All Tab indents empty EOF line and `999999999.` Enter emits invalid 10-digit item. | **Verified** (all 4 in debug & release) |
| F24 | **Low** | Standards | C11 | `src/app/file_ops.rs:294-305`, `src/model/session.rs:178-187`, `fs_service.rs:74-83` | `commit_save_naming` skips `sanitize_name` (`../` saves outside workspace, `/` creates dirs); `sanitize_name` also does not filter Windows reserved device names (`CON`, etc.) or `<>:"\|?*`. | **Code-verified** (`ea03a57` GPUI harness repro) |
| F25 | **Low** | Standards | D8+A9 | `src/markdown/parser.rs:11-535`, `exporter.rs:117`, `text_editor.rs:35-40`, `app/render.rs:6` | Code quality: ~380-line legacy `parse_markdown` AST tested only by 5 unit tests and drifted (`~~~`, `+ `, `1)`, 4-space code); 41 `allow(dead_code)` attributes (incl. unused `ShapedLine` in `PaintCell`); 45 functions >100 lines (`render.rs` 1,779 lines with 100+ repeated `view.update` closures). | **Verified** (release drift test & source grep) |

## PR grouping (small fix PRs, security fixes first)

1. **PR 1 — Security (PDF hard-abort & OOM bomb)**: `F01`, `F02` (`Cargo.toml`, `src/services/pdf_service.rs`, `src/app/commands.rs`)
   - Upgrade `lopdf` to `>= 0.42` (or `0.47`, dropping removed `nom_parser` feature) and rerun `cargo audit`.
   - Enforce pre-load file size limit, page cap, and cumulative extracted-byte cap in `extract_text_from_pdf`, and run PDF extraction on `cx.background_executor()` off the UI thread.
2. **PR 2 — Security (Link click scheme allowlist)**: `F03` (`src/app/editor_events.rs`)
   - Allow only `http`, `https`, and `mailto` URLs in link click handling; reject or prompt with full target string for any other scheme, UNC path, or file/relative path.
3. **PR 3 — Security (TLS dependency & updater URL/tag hardening)**: `F05`, `F20` (`Cargo.lock`, `src/services/update_service.rs`)
   - `cargo update -p rustls-webpki` (`RUSTSEC-2026-0382`).
   - Construct the Download URL from the validated semver tuple (`.../releases/tag/v{major}.{minor}.{patch}`) instead of raw `html_url`, and display only the sanitized numeric version.
4. **PR 4 — Security (Release workflow least-privilege & CI action pinning)**: `F06` (`.github/workflows/release.yml`, `.github/workflows/ci.yml`)
   - Move `contents: write` down from workflow level to dedicated upload/publish jobs, pin Rust toolchain version cleanly, and align third-party action commit-SHA pinning in `ci.yml`.
5. **PR 5 — Security & Reliability (Iterative bounded workspace scan)**: `F07` (`src/services/fs_service.rs`)
   - Replace recursive `walk` with an explicit `Vec` stack, drop each `ReadDir` before pushing children (fixing the `EMFILE` depth-1,019 silent truncation), and add depth/entry caps.
6. **PR 6 — Crash & Logic (Heading non-ASCII indent panic & debug escape underflow)**: `F04`, partial `F23` (`B5`) (`src/model/document/blocks.rs`, `src/model/document/inline_rewrite.rs`)
   - Count heading indentation using ASCII spaces/tabs only (or char-boundary floor before slicing at `blocks.rs:200`) and replace `start - 2` with `start.checked_sub(2)` at `inline_rewrite.rs:779`.
7. **PR 7 — Data Integrity & Filesystem Semantics (External file conflict, `renameat2` fallback, inode/long-name/dotfile fixes)**: `F09`, `F10`, `F11`, `F22`, `F24` (`src/services/fs_service.rs`, `src/app/tabs.rs`, `src/app/file_ops.rs`)
   - Track file mtime/len at open & save and prompt on external modification; move `ENOSYS`/`EINVAL` fallback inside `rename_noreplace`; fall back to in-place write when `nlink > 1` or ownership cannot be preserved and refuse read-only targets; use short fixed-length temp names; preserve leading dots in rename; pass `commit_save_naming` through tightened `sanitize_name`.
8. **PR 8 — Editing Fidelity & Undo (CRLF preservation & undo diff efficiency)**: `F08`, `F18`, remainder of `F23` (`B6`–`B8`) (`src/model/document/*.rs`, `src/model/text_ops.rs`, `src/markdown/parser.rs`)
   - Route Markdown newline/table/code-block helpers through `line_break(offset)` and clean table deletion with `line_ending_len`.
   - Trim common prefix/suffix (or store compact diffs) in `replace_every_match` and raise `UNDO_BYTE_BUDGET`.
   - Fix `İ` caseless find termination, skip leading UTF-8 BOM when classifying line 0, skip empty EOF trailing line on indent, and cap OL continuation at `999_999_999`.
9. **PR 9 — Performance: Block-format $O(n^2)$ & UI-thread file I/O**: `F14`, `F16`, `F17` (`src/markdown/parser/lists.rs`, `src/services/fs_service.rs`, `src/app/tabs.rs`, `src/app/commands.rs`, `src/app/render.rs`)
   - Switch `accepted_list_marker_in` to `doc.line_index_at(line_start)` binary search (immediate fix for `12.4 s` -> `<100 ms`).
   - Add `metadata()` regular-file + size-cap + 8 KiB streaming NUL precheck in `read_note` (rejecting non-regular files/FIFOs and huge binaries before allocation).
   - Skip `rescan_workspace()` in `save_tab` for existing unchanged paths and cache `recent_folders` `is_dir()` checks.
10. **PR 10 — Performance: Sidebar & Editor Rendering (virtualization, table row culling, layout caching)**: `F12`, `F13`, `F15` (`src/ui/sidebar.rs`, `src/ui/text_editor.rs`, `src/model/document.rs`)
    - Cache `build_sidebar_tree` output and render via GPUI `uniform_list`.
    - Add row-level viewport culling inside tables even when the caret is inside, cache column widths across non-width-changing edits, remove `PaintCell`'s dead `ShapedLine`/`RowOffsetMap` clones, and cache wrapped row heights per line revision/width.
11. **PR 11 — Settings Resilience**: `F21` (`src/model/settings.rs`)
    - Write settings via `atomic_save`, surface write failures cleanly, back up unparseable files to `settings.json.bad`, add `default_zoom_level()` (`100`) and `default_show_sidebar()` serde defaults, and clamp `zoom_level` / `base_font_size` on load.
12. **PR 12 — Tooling & Code Cleanup**: `F19`, `F25`, plus minor notes (`A7` `open_window().unwrap()` at `main.rs:302`, `A8` `patches/xattr/src/sys/mod.rs:12` `cfg(target_os = "linux")` `ENODATA`)
    - Run `cargo fmt` once, retire or regenerate the stale `fmt-baseline`, and make CI `Format` enforce cleanly.
    - Remove legacy `parse_markdown`/`parse_spans`/`Block`/`Span` (after migrating its 5 tests), prune genuine `#[allow(dead_code)]` items, and extract a small event-closure helper in `src/app/render.rs`.
