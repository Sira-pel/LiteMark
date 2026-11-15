# Lane A: tooling, FFI, error handling, CI, secrets

Toolchain: rustc/cargo 1.99.0, Linux x86_64. Main checkout stayed clean (`git status` empty after all runs). Raw outputs are in
a local scratch directory (not in the repo)
(test.out, clippy.out, clippy.json, fmt.out, audit.out, pdfpoc/).

## Tool results

### cargo test (whole suite)
- `test result: ok. 483 passed; 0 failed; 5 ignored; finished in 5.42s` (one test target, the bin; wall time 28.97s including a warm incremental build).
- Ignored: `render_dump`, `diff_probe::incremental_parse_matches_full_parse` (slow randomized probe, not run), `format_toggle_gate`, `table_ops_gate`, `format_toggle_perf_bench`.
- No failures. The `diff_probe` randomized probe is `#[ignore]` and CI never runs `--ignored`, so it is never exercised automatically.

### cargo clippy --all-targets -- -W clippy::pedantic
- 695 unique pedantic diagnostics (rustc output "658 warnings" for the test profile; deduplicated by lint + span from the JSON output). Time: 10s warm.
- Plain `cargo clippy --all-targets -- -D warnings` (what CI runs): clean, exit 0.
- Per lint:

| n | lint | n | lint |
|---|---|---|---|
| 127 | unreadable_literal | 5 | unnested_or_patterns |
| 93 | uninlined_format_args | 5 | manual_let_else |
| 65 | map_unwrap_or | 5 | case_sensitive_file_extension_comparisons |
| 45 | too_many_lines | 4 | bool_to_int_with_if |
| 37 | wildcard_imports | 4 | format_collect |
| 32 | cast_precision_loss | 3 | single_match_else |
| 31 | redundant_closure_for_method_calls | 3 | redundant_else |
| 22 | match_same_arms | 2 | default_trait_access |
| 21 | semicolon_if_nothing_returned | 2 | unused_self |
| 20 | needless_pass_by_value | 2 | stable_sort_primitive |
| 18 | cast_possible_wrap | 2 | fn_params_excessive_bools |
| 18 | if_not_else | 2 | ignored_unit_patterns |
| 18 | format_push_string | 2 | range_plus_one |
| 17 | doc_markdown | 2 | manual_string_new |
| 17 | cast_possible_truncation | 1 each | needless_continue, ref_option, comparison_chain, unnecessary_wraps, cast_lossless, large_types_passed_by_value, elidable_lifetime_names, large_stack_arrays, ignore_without_reason |
| 11 | cast_sign_loss | 11 | trivially_copy_pass_by_ref |
| 11 | assert_is_empty | 9 | struct_excessive_bools |
| 7 | similar_names | 7 | assigning_clones |
| 6 | items_after_statements | | |

- Extra pass with `-W clippy::indexing_slicing -W clippy::unwrap_used -W clippy::expect_used` (bin, non-test): 336 indexing/slicing, 27 unwrap, 5 expect. I opened every unwrap/expect in my scope (see "Error handling" below).
- I read every cast_possible_truncation / cast_sign_loss / cast_possible_wrap site outside tests. None is a bug: the `usize as i32/isize` casts are on tab/row/line counts (`cycle_tab` commands.rs:284-288, `step_cursor` sidebar_selection.rs:47-51, menus.rs:62-83, sidebar.rs:209-211), blocks.rs:241-257 casts are followed by `.max(start)` before going back to `usize`, and text_editor.rs:636/647/745/1073/1831 convert `f32` values that are floored and guarded (`as usize` saturates negatives and NaN to 0 in Rust). `random_suffix` `u128 as u64` truncation is intentional (hash mixing). lists.rs:101 `u64 as usize` is only a 32-bit-target concern (value already bounded by digit loop, see lane B).
- too_many_lines worst: `app/render.rs:6` (1779 lines), `ui/text_editor.rs:1027` (1103), `app/key_bindings.rs:7` (770), `ui/sidebar.rs:289` (711), `model/document/inline_format.rs:11` (706), `markdown/parser.rs:1970` (392), `ui/header_bar.rs:52` (333), `app.rs:213` (309), `model/document/newline.rs:138` (302). Maintainability only, listed as a Low finding below.
- Everything else (unreadable_literal, uninlined_format_args, map_unwrap_or, wildcard_imports, ...) is style noise and is not listed.

### cargo fmt --check
- Plain `cargo fmt --check`: FAILS. 405 `Diff in` hunks across ~50 files (largest: model/document/tests.rs 60, ui/text_editor.rs 52, ui/formatting_toolbar.rs 38, ui/menu.rs 29, ui/sidebar.rs 21).
- Repo's own gate (`.cursor/skills/verify-litemark/scripts/fmt-gate.sh`, run with `VERIFY_RUN_DIR` pointed at the scratchpad so nothing was written to the checkout): FAILS. Output: `fmt_baseline_files=21 fmt_baseline_hunks=182 fmt_observed_hunks=405 fmt_cargo_status=1 fmt_baseline_stale=167 fmt_gate=fail`, plus about 31 `fmt_new_file=` lines (app/commands.rs, app/render.rs, app/menus.rs, ui/table_chrome.rs, model/document/tests.rs, ...).
- How the gate works: it runs `cargo fmt --check` with the kit's `rustfmt.toml` (max_width 100, Unix newlines), hashes each `Diff in path:line` hunk, and compares against `fixtures/fmt-baseline.files` and `fixtures/fmt-baseline.hunks` (last updated 2026-10-26). It fails on a new file, a new hunk, a content collision, or a stale baseline entry.
- So the tree is not clean by either measure, and the baseline is stale: the split-large-files refactor (#53) moved code into new files, and 167 baseline entries no longer match.
- Neither is enforced in GitHub CI: the `Format` step in ci.yml prints "skipping" and exits 0 when the tree is dirty, and `fmt-gate.sh` only runs from the local `ci.sh`. See finding "CI never fails on formatting".

### cargo audit (installed with `cargo install cargo-audit --locked`, v0.23.0; 1m30s) / cargo deny
- Advisory DB: 1490 advisories, 747 crates scanned. Result: `error: 3 vulnerabilities found!` and `18 allowed warnings found`. Wall time 9.6s.
- cargo-deny: not installed, skipped.
- Vulnerabilities:
  - RUSTSEC-2026-0187 lopdf 0.34.0, stack overflow on deeply nested objects, 7.5 high. APPLIES (finding 1).
  - RUSTSEC-2026-0382 rustls-webpki 0.103.15, distrusted intermediates may be accepted. Applies to the update check (finding 4).
  - RUSTSEC-2026-0418 libz-sys 1.1.30, build script leaks env vars into the build directory. DISMISSED: build-time only, reached through gpui_util -> git2 -> libgit2-sys; nothing in Litemark's runtime or source calls it, and it affects the build tree, not the shipped binary.
- Warnings (all dismissed unless noted):
  - Unmaintained: async-std, instant, paste, pin-utils, proc-macro-error2, rustls-pemfile, rustybuzz (0.14.1 and 0.20.1), ttf-parser (0.20/0.21/0.25), zbus_names, zvariant, zvariant_derive. All transitive via gpui (cosmic-text/zed async-tar/zbus portal stack). We cannot upgrade them (`gpui = "=0.2.2"`); none is called directly.
  - Unsound: git2 0.20.4 (RUSTSEC-2026-0184 `BlameHunk` signature, -0183 `Remote::list`). Dismissed: git2 comes only from gpui_util; Litemark never calls blame or remotes. rustix 1.1.5 xattr zero-length buffer (RUSTSEC-2026-0486): dismissed, pulled by async-io under zed-async-tar (gpui_http_client), no xattr calls from us.
  - Yanked: async-executor 1.14.0. Dismissed: transitive via gpui; fix is `cargo update -p async-executor` if a newer compatible version exists.
- `pulldown-cmark 0.12`, `ureq 2.12.1`, `trash 5`, `rfd 0.15`: no advisory.
- `patches/xattr`: vendored xattr 0.2.3 (used only by zed-async-tar via gpui_http_client, never by Litemark code). Diffing against the crates.io 0.2.3 source: the only code difference is in `src/sys/mod.rs`, `pub const ENOATTR = ::libc::ENOATTR` became `::libc::ENODATA`. See finding "xattr patch applies ENODATA to every platform" (Low).

## Findings

### Opening a crafted PDF crashes the app (stack overflow in lopdf), on the UI thread
- Severity: Critical
- Category: Security
- Location: src/services/pdf_service.rs:3-4 (`lopdf::Document::load`), called from src/app/commands.rs:363 (UI thread, open file / sidebar click / drag-drop); Cargo.toml `lopdf = "0.34"`
- Problem: RUSTSEC-2026-0187. lopdf 0.34's nom parser recurses once per nesting level of arrays/dicts. A ~20 KB PDF with 10000 nested `[` overflows the stack. A stack overflow is a hard process abort (not an unwinding panic), so it kills the app and any unsaved tabs. Reachable by opening any `.pdf` the user received. `extract_text_from_pdf` runs synchronously on the main thread (the 8 MB main stack on Linux, 1 MB on Windows, so Windows is more exposed).
- Evidence: PROVEN. Scratch crate (not in the repo) at a local scratch directory with `lopdf = "=0.34.0"` and the same feature set as Litemark, building a valid single-xref PDF whose object 3 is `[`*n + `]`*n:
  ```
  n=1000    pdf size 2270 bytes     -> ok pages=0
  n=10000   pdf size ~20 KB         -> thread 'main' has overflowed its stack / fatal runtime error: stack overflow, aborting
  n=100000  (release build too)     -> thread 'main' has overflowed its stack / fatal runtime error: stack overflow, aborting
  ```
  This is the same call (`Document::load`) that Litemark makes; I did not drive the GUI.
- Suggested fix: upgrade lopdf to >=0.42 (check the `nom_parser` feature and `extract_text` API still match). Short term, run PDF extraction on a thread with a large explicit stack (`std::thread::Builder::new().stack_size(256 << 20)`) and also off the UI thread, which also fixes UI freezes on big PDFs; add a size cap before loading.

### rustls-webpki 0.103.15 accepts distrusted intermediates (update check TLS)
- Severity: Medium
- Category: Security
- Location: Cargo.lock (rustls-webpki 0.103.15 via ureq 2.12.1 and via gpui_http_client); src/services/update_service.rs:57-70
- Problem: RUSTSEC-2026-0382. `fetch_latest_release` makes one HTTPS request a day to api.github.com through ureq/rustls. A distrusted intermediate could be accepted, weakening the MITM guarantee for that request. Impact is bounded: the response is only parsed for `tag_name`/`html_url`, `html_url` must start with `https://github.com/Sira-pel/LiteMark/releases/`, and the app only opens that URL when the user clicks. Nothing is downloaded or executed.
- Evidence: UNPROVEN. Reasoned from `cargo tree -i rustls-webpki` (ureq is a direct dependency) and the advisory text; I did not construct a bad chain.
- Suggested fix: `cargo update -p rustls-webpki` (patched in 0.103.16; semver-compatible).

### Ctrl+click on a Markdown link hands any URL scheme to the OS
- Severity: Medium
- Category: Security
- Location: src/app/editor_events.rs:331-335 (`cx.open_url(&url)`), src/app/render.rs:1635 (update banner, fixed prefix, fine), src/ui/text_editor.rs:436 (`link_for_offset`), src/markdown/parser.rs:1436 (`dest_url` taken verbatim)
- Problem: the destination of `[text](dest)` is passed unfiltered to `open_url`, which on Windows ShellExecute's it and on Linux xdg-open's it. A `.md` file from an untrusted source can carry `file:///...`, a UNC path, a relative path, or a custom protocol handler (`ms-msdt:`, `search-ms:`, `vscode:` and similar). It needs a deliberate Ctrl/Cmd+click in WYSIWYG mode, which is why this is Medium and not higher, but the user cannot see the destination before clicking (the label can say anything).
- Evidence: UNPROVEN. Code read only: no scheme check exists on the path from parser to `open_url`; I did not click through on a GUI. `grep -rn scheme src` finds no allowlist.
- Suggested fix: allow only `http`, `https`, and `mailto` before calling `open_url`; for anything else show the target in a toast or confirm.

### Release workflow grants `contents: write` to every job and builds from an unpinned toolchain; releases are unsigned
- Severity: Medium
- Category: Security
- Location: .github/workflows/release.yml:7-8 (workflow-level `permissions: contents: write`), :16-60, :80-95 (all jobs run `cargo test`/build scripts with write token); :27 `dtolnay/rust-toolchain@<sha> # stable`
- Problem: the release workflow is tag-triggered only (good: no `pull_request_target`, no `${{ github.event.* }}` in `run:`; only `GITHUB_REF_NAME`/`github.token` via env, which is safe). Third-party actions are pinned to full commit SHAs (good). The remaining issues: (1) write permission is workflow-wide, so build steps that execute all of the dependency tree's build scripts (`cargo test`, `cargo build`, ~750 crates) hold a token that can modify releases and tags; scope write to the single upload step's job and give the build job `contents: read` plus a separate publish job. (2) The pinned toolchain action selects `stable` at run time, so the compiler is not reproducible. (3) Artifacts are not signed: SHA256SUMS sits next to the artifacts it vouches for, the Windows installer is unsigned, macOS is ad-hoc signed (`codesign --sign -`, build-macos.sh:69), and there is no provenance attestation. The in-app updater only links to the release page, so there is no auto-download risk.
- Evidence: UNPROVEN (configuration read in full; not executed).
- Suggested fix: per-job `permissions:` (build jobs `contents: read`; publish/upload jobs `contents: write`); optionally add `actions/attest-build-provenance` and Authenticode signing later.

### CI never fails on formatting; the repo's own fmt gate is stale and not wired into CI
- Severity: Medium
- Category: Standards
- Location: .github/workflows/ci.yml:49-57 (`Format` step), .cursor/skills/verify-litemark/fixtures/fmt-baseline.files and fmt-baseline.hunks, .cursor/skills/verify-litemark/scripts/fmt-gate.sh
- Problem: the CI `Format` step runs `cargo fmt --check` inside an `if`; on failure it echoes "skipping" and exits 0, so formatting can never fail a PR. Meanwhile the local gate (`fmt-gate.sh`, run by `ci.sh`) is not called from any workflow, and it currently fails: `fmt_observed_hunks=405`, baseline has 182, `fmt_baseline_stale=167`, about 31 files reported as `fmt_new_file`. The baseline was last refreshed 2026-10-26, before the file split in #53 moved code around. The hunks of drift grow silently.
- Evidence: PROVEN. Gate output pasted above (`fmt_gate=fail`). `grep -rn fmt-gate .github` returns nothing.
- Suggested fix: regenerate the baseline (or just run `cargo fmt` once, since 405 hunks is tractable, and delete the baseline machinery), then make the CI step fail for real.

### Settings written non-atomically and write errors dropped
- Severity: Medium
- Category: Logic
- Location: src/model/settings.rs:107-129 (`load` and `save`); callers: 15 `settings.save()` sites in src/app/commands.rs, file_ops.rs:54, render.rs:1652, commands.rs:152 (update-check path)
- Problem: `save()` does `fs::write(path, json)` (truncate then write, no temp file or rename). A crash or power loss mid-write leaves a truncated settings.json. `load()` then silently falls back to `Default` on any read or parse error, and the next `save()` overwrites the broken file, so recent folders, theme, font size and update-dismissal state are lost with no message. All errors in `save()` (`create_dir_all`, `to_string_pretty`, `write`) are discarded with `let _ =`, so a read-only config dir or full disk makes every setting silently not persist. With `panic = "abort"`, a crash during a save is more likely to land here than in a normal build. No test covers corrupt or truncated JSON (only `{}` and a valid sample).
- Evidence: UNPROVEN as a crash repro (reasoned from the code). Verified by reading that no temp+rename is used and that `Err` is dropped; the repo already has `atomic_save` in fs_service.rs that could be reused.
- Suggested fix: write through `atomic_save`, return `io::Result` from `save`, and show one toast on failure; on parse failure rename the bad file to `settings.json.bad` before falling back to defaults.

### Main window creation panics on failure
- Severity: Low
- Category: Standards
- Location: src/main.rs:302 (`.unwrap()` on `cx.open_window(...)`)
- Problem: if the display server is missing or the window cannot be created, the app panics (abort in release) with a raw `Result::unwrap` message and no user-facing text. Not reachable from file content, only from an environment problem.
- Evidence: UNPROVEN (needs a headless launch to trigger; not done).
- Suggested fix: `if let Err(err) = ... { eprintln!("Couldn't open the window: {err}"); std::process::exit(1) }`.

### xattr patch applies `ENODATA` to every platform
- Severity: Low
- Category: Standards
- Location: Cargo.toml `[patch.crates-io] xattr = { path = "patches/xattr" }`; patches/xattr/src/sys/mod.rs:12
- Problem: upstream 0.2.3 defines `ENOATTR = libc::ENOATTR`. The patch replaces it with `libc::ENODATA` inside the macro that is expanded for every supported platform (linux, android, macos, freebsd, netbsd). `libc::ENOATTR` does not exist on Linux, so the patch is needed there, but on macOS/FreeBSD/NetBSD "attribute not found" is ENOATTR (93 on macOS), not ENODATA (96), so any "missing attribute" detection in xattr on those targets would be wrong. It is also the only diff against the registry copy (verified with `diff -r`; only dotfiles are missing), so there is no other drift. The crate is used only by zed-async-tar's unpack path, which Litemark never calls, so there is no user-visible effect today. The patch has no comment or README note explaining why it exists, and the patch is old (xattr 1.x exists, but the dependent crate pins 0.2).
- Evidence: UNPROVEN (reading only; not built for macOS).
- Suggested fix: `#[cfg(target_os = "linux")]` ENODATA, keep ENOATTR elsewhere, and document the reason in Cargo.toml.

### win_caption.rs and header_bar.rs: Win32 code reasoned sound, no defects found; compile-checked only on CI
- Severity: Low
- Category: Standards
- Location: src/ui/win_caption.rs:21-114, src/ui/header_bar.rs:463-497
- Problem: none found, but nothing could be executed here. Details of the soundness review are in the "Checked" list. The residual risks: (a) return values of `ReleaseCapture`, `PostMessageW`, `ShowWindowAsync` are ignored (harmless: a failed post just means no drag starts), and (b) this module compiles only under `cfg(windows)`, and `x86_64-pc-windows-*` is not installed here, so `cargo check --target` could not be run (not installed; I did not install toolchains). It is covered by the `test-desktop` and `check-cross-platform` jobs on `windows-latest`.
- Evidence: UNPROVEN, reasoned from the Win32 contract.
- Suggested fix: none required; optionally add `// SAFETY:` comments to the `renameat2` call (fs_service.rs:196 lacks one while win_caption has them).

### 45 functions over 100 lines, several over 700
- Severity: Low
- Category: Standards
- Location: src/app/render.rs:6 (1779 lines), src/ui/text_editor.rs:1027 (1103), src/app/key_bindings.rs:7 (770), src/ui/sidebar.rs:289 (711), src/model/document/inline_format.rs:11 (706), src/markdown/parser.rs:1970 (392), src/ui/header_bar.rs:52 (333), src/app.rs:213 (309), src/model/document/newline.rs:138 (302)
- Problem: single render/key-binding functions of 700 to 1800 lines are hard to review and are where the 100+ identical `let _ = view.update(cx, |this, cx| {...})` blocks in render.rs live (render.rs:54-941, 1258, 1494-1515). No functional bug found there.
- Evidence: PROVEN (clippy `too_many_lines` counts above).
- Suggested fix: later refactor; not for this audit.

## Error handling in src/main.rs, app.rs, app/*, ui/*, theme.rs, model/{settings,session,workspace}.rs

All non-test `unwrap`/`expect`/`panic!`/`unreachable!` in this scope (I opened each one):
- src/main.rs:302 `open_window(...).unwrap()`: Low finding above.
- src/app/editor_events.rs:380 `self.table_drag.clone().expect("drag")`: guarded by `self.table_drag.is_some()` on the previous line, single-threaded UI. Fine.
- src/app/input_handler.rs:131 `text.chars().next().unwrap()`: guarded by `text.chars().count() == 1`. Fine.
- src/app/render.rs:1174 `MenuId::Table => unreachable!()`: the Table menu is handled earlier (render.rs:971 `if open_id == MenuId::Table`) so this arm cannot be hit. Fine, but fragile if line 971 changes.
- src/ui/table_chrome.rs:114 `tables.last_mut().expect("same table")`: `same` is only true when `tables.last()` is `Some`. Fine.
- src/ui/text_editor.rs:405 `partial_cmp(...).unwrap()`: panics only on NaN pixel widths; widths derive from measured text and a clamped positive target (`content_width.max(px(1.0))`, `num_cols > 0` is checked at :296). Not reachable in practice, noted for robustness; `f32::total_cmp` would remove it.
- No `todo!`, `unimplemented!`, `panic!` in this scope outside tests.
- Indexing on user-derived offsets: the `[start..end]` slices in `input_handler.rs` go through `range_from_utf16` (clamps in text_ops.rs:948-963) and `marked_range` is cleared on every edit (document.rs:472, 493, 696), so a stale range is not reused. The remaining `tabs[index]` lookups come from `position()` results. No reachable panic found. (Parser/model internals are lanes B and C.)

Swallowed errors on user-visible paths:
- Save, save-as, save-all, rename, move, new file/folder, trash: all return `Result<_, String>` and surface through `show_error` / `show_file_error` / bulk report (tabs.rs:6-60, file_ops.rs:216, 305, 338, 541-563, commands.rs:332-420). Fine.
- Settings write: swallowed (finding above). `seed_welcome_note` (fs_service.rs:70) and `get_or_create_notes_dir` (:63) discard errors, harmless (on failure the app starts with an in-memory welcome tab, app.rs:656).
- `welcome_tab` (app.rs:656) falls back silently to the built-in text if reading Welcome.md fails. Fine.
- Update check: every failure is `None` by design (update_service.rs:57-70); not user-visible, appropriate.
- The `let _ = view.update(...)` and `.ok()` on `this.update(cx, ...)` in render.rs/app.rs only ignore "view/window already gone". Fine.
- No crash-recovery or autosave exists (`grep -i autosave|recover` is empty), and `panic = "abort"` with no `panic::set_hook`, so any panic or stack overflow (finding 1) loses all unsaved tabs. Design note, not a defect on its own.

Logging of paths or content: no `println!`/`eprintln!`/log/tracing in app code. The only ones are in test gates and `markdown/render_dump.rs` (an `#[ignore]`d harness test). Nothing logs file paths or document text at runtime.

## unsafe and FFI inventory

Two `unsafe` areas in the whole of src/ and build.rs (build.rs has none; it panics at build time on icon embedding failure, which is acceptable):

1. src/services/fs_service.rs:29-37, 190-210: `unsafe extern "C" { fn renameat2(...) }` plus one call. Soundness: both `CString`s outlive the call (bound to locals in the same function), embedded NUL is rejected before the call by `CString::new`, the signature matches glibc/musl `int renameat2(int, const char*, int, const char*, unsigned int)`, `AT_FDCWD = -100` and `RENAME_NOREPLACE = 1` are the Linux constants, errors go through `io::Error::last_os_error()` right after the call. `ENOSYS = 38`, `EINVAL = 22` are right for x86/arm Linux (differ on MIPS/Alpha, which are not targets). Gated by `cfg(target_os = "linux")` with a portable fallback for other targets, and the portable fallback is compiled in tests on Linux too. glibc added `renameat2` in 2.28; the deb is built on ubuntu-22.04 so fine. Only nit: no `// SAFETY:` comment. UNPROVEN-by-execution but straightforward and exercised by the Linux tests (`safe_create_and_rename_do_not_overwrite`, `noreplace_falls_back_on_enosys_and_einval`).

2. src/ui/win_caption.rs: `extern "system"` block for user32 (`ReleaseCapture`, `PostMessageW`, `IsZoomed`, `ShowWindowAsync`, `GetCursorPos`, `GetSystemMetrics`). No window subclass, no callback and no userdata pointer exist (only posted messages), so there is no dangling-pointer or re-entrancy surface. Checks, all UNPROVEN (cannot run Windows):
   - Calling convention: `extern "system"` = stdcall on x86 and the single Win64 convention on x64. Correct for user32.
   - Types: HWND passed as `isize` (pointer-sized, ABI-equal to `HWND`); `BOOL` is `i32`; `UINT msg` is `u32`; `WPARAM` as `usize` and `LPARAM` as `isize` match `UINT_PTR`/`LONG_PTR`. `POINT` is `#[repr(C)] {i32,i32}` = `LONG x, y`. Correct.
   - HWND validity: obtained from `HasWindowHandle::window_handle(window)` for the live gpui `Window` and used immediately within the same call, never stored. `RawWindowHandle::Win32(h).hwnd` is `NonZeroIsize`, so it cannot be null. If the window were destroyed in between, `PostMessageW`/`ShowWindowAsync` just fail (documented: return 0 for an invalid handle); no memory unsafety.
   - Coordinate packing, win_caption.rs:95: `(y as i16 as u16 as u32) << 16 | (x as i16 as u16 as u32)`, then `as i32 as isize`. This is MAKELPARAM with signed 16-bit truncation. `WM_SYSCOMMAND` documents lParam as low word x, high word y in screen coordinates, read back with `GET_X_LPARAM`/`GET_Y_LPARAM` (signed `short` extraction), so negative coordinates on a monitor left of or above the primary are preserved. Sign extension across the `u32 -> i32 -> isize` conversion keeps the low 32 bits identical, and only the low 32 bits are read. Coordinates outside +/-32767 would wrap, but the Windows virtual screen cannot exceed that. Correct.
   - `wParam = 0xF010 | 2` (`SC_MOVE | HTCAPTION`). For `WM_SYSCOMMAND` the low four bits of wParam are reserved for system use, `SC_MOVE` with the HTCAPTION hint is the documented way custom title bars start a move. Correct.
   - Thread affinity: `GetCursorPos`, `GetSystemMetrics`, `ReleaseCapture` (calling-thread capture; called from the UI thread that received the press, which is the thread that owns the capture), `PostMessageW` (thread-safe), `ShowWindowAsync` (made for cross-thread). The `thread_local! PRESS` is only used from the UI thread (`arm_drag` and `drag_past_threshold` from header-bar mouse handlers), so it is consistent.
   - Error checks: `GetCursorPos` result is checked in both uses. `ReleaseCapture`, `PostMessageW`, `ShowWindowAsync` returns are ignored, which is harmless.
   - `cfg` boundary: `pub mod win_caption` is `#[cfg(windows)]` (ui/mod.rs:15-16), and `raw-window-handle` is a `[target.'cfg(windows)'.dependencies]`; every call site in header_bar.rs:463-497 has matching `#[cfg(windows)]`/`#[cfg(not(windows))]` pairs, so no Windows-only symbol leaks into Linux/macOS builds and no dead branch was found. The `#[link(name = "user32")]` extern block is declared without `unsafe` (edition 2021, allowed). I could not run `cargo check --target x86_64-pc-windows-*` because only `x86_64-unknown-linux-gnu` is installed.
   - The `winresource` build-dependency is under `cfg(windows)` and `build.rs` guards its use with `#[cfg(windows)]`.

## Secrets, personal paths, CI hygiene

- ci.yml: `permissions: contents: read` at workflow level (least privilege, good); `concurrency` cancels superseded PR runs; triggers are `push` to main and `pull_request` (no `pull_request_target`); no secrets referenced at all, so nothing is exposed to fork PRs; no `${{ github.event.* }}` interpolation inside any `run:` block (the only expression in a `run` path is `GITHUB_REF_NAME` through an environment variable in release.yml, which is safe). Weakness: all third-party actions in ci.yml (`actions/checkout@v4`, `dtolnay/rust-toolchain@stable`, `Swatinem/rust-cache@v2`) are pinned to tags/branches, while release.yml pins the same actions to commit SHAs. A moved tag would run unreviewed code in CI (read-only token, so impact is limited). Pin them to the same SHAs as release.yml. (Folded in here rather than a separate finding because ci.yml has read-only permissions and no secrets.)
- ci.yml `verify-kit` and `test` jobs use `sudo apt-get` on `ubuntu-22.04` runners, not a security issue.
- `check-cross-platform` is `continue-on-error: true` and duplicates the `cargo build` in `test-desktop`; it can never fail the PR. Low.
- Secret scan of tracked files (`git ls-files | xargs grep`) for api key, secret, password, private-key headers, `ghp_`, `github_pat`, `AKIA`, `xox`, `bearer`, certificate/signing: no real secret. Only matches are test fixtures (`prove-run-id.sh` canary files named `secret.txt`, and a `sanitize_name("../../etc/passwd")` test). No `.env`, `.pem`, `.key`, `.pfx`, `.p12` tracked. `git log --all -S` for `ghp_`, `AKIA`, `BEGIN PRIVATE KEY`, `BEGIN RSA`, `BEGIN OPENSSH`, `/home/<user>`, `C:\Users`: 0 commits each.
- Personal absolute paths in tracked files: only `../../../home/ubuntu` as a path-traversal test input in `.cursor/skills/verify-litemark/scripts/prove-run-id.sh:45` and EVIDENCE.md:41, which is not a leak. No `/home/<user>` or `C:\Users\...` in tracked content or history. (The untracked `.claude/worktrees/` directories do contain absolute paths in `.git` files, but `git ls-files | grep -c ^.claude` is 0, so they are not tracked.) Commit author emails (several personal addresses) are present in git history, which is normal metadata, not a code leak.

## Test coverage for risky paths

Counts of `#[test]` per file: fs_service.rs 24, session.rs 15, workspace.rs 7, update_service.rs 5, settings.rs 4, win_caption.rs 1, pdf_service.rs 0, app/file_ops.rs 0, app/tabs.rs 0, app/commands.rs 0.
- File save, rename, delete service layer: well covered in fs_service.rs (atomic save permissions/symlinks, temp cleanup, no-replace rename and its ENOSYS/EINVAL fallback, scan not following symlinks, folder copy/move, collisions, NUL names).
- No tests for the `LiteMarkApp` orchestration that calls them: `save_tab`, `finish_trash`, `apply_workspace_switch`, bulk trash, optimistic-close restore (src/app/tabs.rs, file_ops.rs have 0 tests). `trash::delete` is not covered anywhere.
- Settings/session load from corrupt JSON: NOT tested. `settings.rs` tests only `{}`, a valid round trip, and an old file lacking `show_code_files`; there is no test for truncated or non-JSON input, for the write failing, or for `save`/`load` at all (they hit the real config dir). `session.rs` is a pure tab-planning module (no persistence), so there is no session JSON to corrupt.
- Update check: `update_service.rs` has 5 tests covering version parsing, release parsing, the URL-prefix check, and the check interval. `fetch_latest_release` (network) is untested, as expected.
- PDF export/extract: `pdf_service.rs` has 0 tests. Neither the empty-pages error, a PDF with no text layer, a corrupt file, nor the deep-nesting crash above is covered. (There is no PDF export; PDFs are import-only and read-only.)
- Windows caption code: one pure test (`past_threshold`), and it only compiles and runs on Windows CI. The Win32 calls themselves cannot be tested without a window.
- The slow randomized `diff_probe` is `#[ignore]` and not wired into CI.

## Checked and found fine
- `cargo test`: all 483 pass, 0 failures; `clippy -D warnings` (the CI gate) is clean.
- Every `unsafe` block and `extern` declaration in src/ and build.rs (the two areas above); no `unsafe` elsewhere, no `#[no_mangle]`, no other raw-window-handle use.
- All non-test unwrap/expect/unreachable in src/main.rs, app.rs, app/*, ui/*, theme.rs, model/{settings,session,workspace}.rs, plus slicing in input_handler.rs.
- Save/rename/delete/open error propagation to the UI (tabs.rs, file_ops.rs, commands.rs).
- release.yml: tag-only trigger, SHA-pinned actions, no untrusted-input interpolation, tag/version check before release.
- ci.yml: read-only default permissions, no secrets, no `pull_request_target`.
- No secrets, keys, or personal paths in tracked files or history (patterns listed above).
- Runtime logging: no paths or document content are logged.
- Updater: only fetches release metadata, validates `html_url` prefix, never downloads or runs anything.
- `patches/xattr`: diffed against crates.io 0.2.3; single-line change.
- Build scripts (build-deb.sh, build-windows.ps1, build-macos.sh) skimmed: they write checksums; macOS is ad-hoc signed only.
