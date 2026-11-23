# Evaluation: `ropey` backing store for `DocumentTab::content`

Milestone 5 asked to evaluate replacing the contiguous `String` with a
`ropey::Rope`. **Recommendation: do not migrate.**

## Measurements

Scratch crate, `ropey 1`, release build, Windows (timer resolution ~100 ns).
Text is ASCII lines of ~60 bytes. "Edit" is an insert plus a remove of one
char, so a single edit is about half of the figure.

| Size | Edit at start (String / Rope) | Edit in middle (String / Rope) | Snapshot clone (String / Rope) | Rope to contiguous `String` |
|---|---|---|---|---|
| 1 MB | 32 us / 0.2 us | 15 us / 0.2 us | 22 us / ~0 | 59 us |
| 5 MB | 543 us / 0.2 us | 286 us / 0.2 us | 1.7 ms / ~0 | 5.7 ms |
| 20 MB | 2.4 ms / 0.4 us | 1.1 ms / 0.3 us | 4.9 ms / ~0 | 15.6 ms |

## Why not

- **The edit cost it removes is small next to the other per-frame costs.** A
  single insert in a 5 MB note is about 0.3 ms with a `String`, against a
  13 ms WYSIWYG frame and a 6 ms incremental parse (100k-line benchmarks in
  `viewport_culling_tests.rs` and `parser/incremental_tests.rs`).
- **The editor needs contiguous `&str`.** About 450 uses of `.content` in 27
  non-test files, and about 290 call sites that pass `&str` (the parser,
  `memchr` scans, row wrapping, `display_text` borrowing zero-copy slices of
  the note). A rope only gives chunks, so each of those needs a rewrite or a
  flatten, and flattening costs more than the edit it saved (5.7 ms at 5 MB).
- **Snapshots are not the pain point today.** A save still clones the whole
  text on the UI thread (`start_async_save` calls `tab.file_text()` before it
  spawns), and only the disk write runs in the background. That clone is about
  1.7 ms at 5 MB, so a rope's free snapshot would not change much.

## When to revisit

If profiling on real multi-MB notes shows `String` memmove dominating typing
latency (not the case at 5 MB in these numbers), or if a feature needs cheap
snapshots of the whole text. Even then, a gap buffer around the caret would
fit the contiguous-`&str` API better than a rope.

## Benchmark source

```rust
use ropey::Rope;
use std::time::{Duration, Instant};

fn best<F: FnMut()>(n: usize, mut f: F) -> Duration {
    (0..n).map(|_| { let t = Instant::now(); f(); t.elapsed() }).min().unwrap()
}

fn main() {
    for mb in [1usize, 5, 20] {
        let line = "a plain paragraph of text with some **bold** words in it, ok\n";
        let text: String = line.repeat(mb * 1_000_000 / line.len());
        let mut s = text.clone();
        let mut r = Rope::from_str(&text);
        println!("== {mb} MB, {} lines", r.len_lines());

        let t_s = best(200, || { s.insert(10, 'x'); s.replace_range(10..11, ""); });
        let t_r = best(200, || { r.insert_char(10, 'x'); r.remove(10..11); });
        println!("insert+remove at start: String {t_s:?}  Rope {t_r:?}");

        let mid = s.len() / 2;
        let t_s = best(200, || { s.insert(mid, 'x'); s.replace_range(mid..mid + 1, ""); });
        let t_r = best(200, || { r.insert_char(mid, 'x'); r.remove(mid..mid + 1); });
        println!("insert+remove at middle: String {t_s:?}  Rope {t_r:?}");

        let t_clone_s = best(20, || { std::hint::black_box(s.clone()); });
        let t_clone_r = best(20, || { std::hint::black_box(r.clone()); });
        println!("snapshot (clone): String {t_clone_s:?}  Rope {t_clone_r:?}");

        // A contiguous &str, which most of the editor needs today.
        let t_flat = best(5, || { std::hint::black_box(r.to_string()); });
        println!("Rope -> contiguous String: {t_flat:?}");

        // Reading a 200-byte slice near the middle.
        let t_sl_s = best(1000, || { std::hint::black_box(&s[mid..mid + 200]); });
        let t_sl_r = best(1000, || { std::hint::black_box(r.byte_slice(mid..mid + 200).to_string()); });
        println!("200-byte slice: &str {t_sl_s:?}  Rope slice->String {t_sl_r:?}");

        // Line lookup
        let t_ln = best(1000, || { std::hint::black_box(r.byte_to_line(mid)); });
        println!("byte_to_line (Rope): {t_ln:?}");
    }
}
```
