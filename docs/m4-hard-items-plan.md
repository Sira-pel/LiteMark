# Milestone 4: plan for the remaining hard items

Constraint: no UI can be run in this environment, so every item needs a headless
test. Check how `tab_restore_tests.rs` / `sidebar_render_tests.rs` build the app;
with a gpui test context, `run_until_parked()` makes background-executor work
deterministic.

## Order (safest first)

**A. Incremental `find_hits`** (pure logic)
- `DocumentTab::edit` records `last_edit: (rev_before, start, old_len, new_len)`.
- `refresh_find` goes incremental only if the cached key's revision equals
  `rev_before` and the new revision is the next one; otherwise full search
  (undo, replace-all, reload stay correct by default).
- Matching is greedy/leftmost/non-overlapping, so a fixed window is wrong for
  periodic terms (`"aa"` in `"aaaa"`). Re-search from slightly before the edit and
  keep scanning forward until a new hit lands exactly on a shifted old hit; splice
  the rest of the old hits from there.
- Caseless matches can change byte length (`İ`): margin covers the needle's max
  byte expansion, snapped to char boundaries.
- Gate: randomized differential test against `find_matches` (alphabet
  `a b İ \r \n`, terms `aa`, `aba`, `i`, both case modes).

**B. Parallel Quick Open content search**
- Check first whether `search_contents` already runs on the background executor.
- Parallelize per file (`std::thread::scope`, ordered batches), merge in file order
  applying `per_note` / `max_total`. Test output is identical to sequential.

**C. Background workspace scan**
- Classify the 15 `rescan_workspace` callers: those that read `self.workspace`
  right after (create, rename, select) stay synchronous; fire-and-forget ones
  (trash, settings toggles, external changes) use `rescan_workspace_async`.
- `scan_generation` counter; apply the `is_open` map and `refresh_workspace_ids`
  when the result arrives, not when it starts. A sync rescan bumps the generation
  so an in-flight async result is dropped.
- Test: two overlapping async scans, only the newer is applied.

**D. Async note save (Ctrl+S only)** *(changes behaviour: needs user go-ahead)*
- Keep `save_tab` sync for save-and-close, save-all, prompts.
- Snapshot `(text, revision, path)`; serialize writes per path.
- On completion compute `disk_version` from the snapshot text (not the tab's
  current text); clear `is_modified` only if the revision still matches.
- Window close waits for in-flight saves.

**E. Async note read above ~1 MB** *(changes behaviour: needs user go-ahead)*
- Small reads stay synchronous; a loading state is a UX change.

**F. `Rc<ShapedLine>` cache** — deferred: needs profiling on a real machine.
gpui already caches line layouts between frames; a wrong cache key shows stale
highlights that no headless test here would catch.

## Status

- A incremental find: done (differential test, 4000 random edits)
- B parallel Quick Open content search: done (ordered-merge equivalence test)
- C background workspace scan: done (callers that read the tree right after stay sync; generation-guarded)
- D async Ctrl+S: done (existing file, same path only; one write per note; close waits)
- E async read above 1 MB: done (toast + one big read at a time)
- F `Rc<ShapedLine>` cache: deferred, needs profiling on a real machine
