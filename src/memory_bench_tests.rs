//! Manual benchmark: heap bytes a large note costs, split by what holds them.
//! Run with
//! `cargo test --release bench_note_heap -- --ignored --nocapture --test-threads=1`.

use super::*;
use crate::heap_counter::live_bytes;
use gpui::{TestAppContext, VisualTestContext};

use ::core::prelude::v1::test as std_test;

fn sample_note(blocks: usize) -> String {
    (0..blocks)
        .map(|i| {
            format!(
                "## Heading {i}\n\nSome **bold** text with a [link](https://example.com/{i}) and `code` here, plus _italic_ words to wrap around nicely.\n- item one\n- [ ] task two\n\n"
            )
        })
        .collect()
}

/// Plain prose: long unstyled paragraphs, no inline markup.
fn prose_note(blocks: usize) -> String {
    (0..blocks)
        .map(|i| {
            format!(
                "Paragraph {i} is plain prose without any markup at all, just words that run on long enough to wrap across a couple of rows in the editor.
A second line of the same paragraph keeps going with more ordinary words and a number {i}.

"
            )
        })
        .collect()
}

/// Code-heavy: fenced blocks of code lines between short headings.
fn code_note(blocks: usize) -> String {
    (0..blocks)
        .map(|i| {
            format!(
                "## Snippet {i}

```rust
fn add_{i}(a: i32, b: i32) -> i32 {{
    let sum = a + b; // **not bold**
    sum * 2
}}

fn main() {{ println!(\"{{}}\", add_{i}(1, 2)); }}
```

"
            )
        })
        .collect()
}

fn redraw(cx: &mut VisualTestContext) {
    cx.update(|window, _cx| window.refresh());
    cx.run_until_parked();
}

fn mib(bytes: isize) -> String {
    format!("{:7.1} MiB", bytes as f64 / (1024.0 * 1024.0))
}

#[ignore]
#[std_test]
fn bench_note_heap() {
    type Gen = fn(usize) -> String;
    let samples: [(&str, Gen); 3] = [("dense", sample_note), ("prose", prose_note), ("code", code_note)];
    for ((name, gen), mode) in samples.iter().flat_map(|s| [ViewMode::Raw, ViewMode::Wysiwyg].map(|m| (s, m))) {
        eprintln!("== {name} / {mode:?}");
        let doc = gen(20_000);
        let doc_len = doc.len() as isize;
        let mut tcx = TestAppContext::single();
        let base = live_bytes();
        let (view, cx) = tcx.add_window_view(|_window, cx| {
            let mut app = LiteMarkApp::test_new(&doc, cx);
            app.settings.view_mode = mode;
            app
        });
        let mut cx = cx.clone();
        drop(doc);
        let built = live_bytes();
        redraw(&mut cx);
        let drawn = live_bytes();
        let (parsed, layout, paint) = view.update(&mut cx, |app, _cx| {
            let before = live_bytes();
            let tab = app.active_tab_mut().expect("tab");
            let doc = tab.parsed();
            let n = doc.lines.len();
            let sz = |b: usize| mib(b as isize);
            let hidden: usize = doc.lines.iter().map(|l| l.hidden_ranges.len() * 16).sum();
            let pairs: usize = doc.lines.iter().map(|l| std::mem::size_of_val(l.inline_pairs())).sum();
            let segs: usize = doc.lines.iter().map(|l| std::mem::size_of_val(l.styled_segments())).sum();
            let links: usize = doc.lines.iter().map(|l| if l.has_inline_for_bench() { 16 + 24 + l.link_hits().capacity() * 24 } else { 0 }).sum();
            let boxed = doc.lines.iter().filter(|l| l.has_inline_for_bench()).count();
            let maps: usize = doc.lines.iter().filter_map(|l| l.offset_map_for_bench()).map(|m| 64 + m.segments.capacity() * 32).sum();
            let disp: usize = doc.lines.iter().map(|l| l.display_owned_len_for_bench()).sum();
            eprintln!(
                "  {n} lines ({boxed} with inline box), ParsedLine {} B inline -> {} | hidden {} | pairs {} | segments {} | links(Arc) {} | offset maps {} | display text {} | checkpoints {} x {} B",
                std::mem::size_of::<crate::markdown::parser::ParsedLine>(),
                sz(n * std::mem::size_of::<crate::markdown::parser::ParsedLine>()),
                sz(hidden), sz(pairs), sz(segs), sz(links), sz(maps), sz(disp),
                doc.meta.checkpoints.len(),
                std::mem::size_of_val(&doc.meta.checkpoints[0]),
            );
            let mut hist = std::collections::BTreeMap::<String, usize>::new();
            let (mut len_sum, mut cap_sum) = (0usize, 0usize);
            for l in &doc.lines {
                let key = format!(
                    "{:?} h{} p{} s{} l{} m{} d{}",
                    std::mem::discriminant(&l.kind),
                    l.hidden_ranges.len(),
                    l.inline_pairs().len(),
                    l.styled_segments().len(),
                    l.link_hits().len(),
                    l.offset_map_for_bench().is_some() as u8,
                    (l.display_owned_len_for_bench() > 0) as u8
                );
                *hist.entry(key).or_default() += 1;
                len_sum += l.hidden_ranges.len() + l.inline_pairs().len() + l.styled_segments().len();
                cap_sum += l.hidden_ranges.len() + l.inline_pairs().len() + l.styled_segments().len();
            }
            for (k, v) in hist {
                eprintln!("    {v:>7} x {k}");
            }
            eprintln!("  vec len {len_sum} vs capacity {cap_sum}");
            drop(doc);
            tab.release_parse_cache();
            let parsed = before - live_bytes();
            let mut paint = app.editor_paint.borrow_mut();
            {
                use std::collections::HashSet;
                let rows = &paint.rows;
                let maps: HashSet<usize> = rows
                    .iter()
                    .filter_map(|r| r.offset_map.as_ref().map(|m| std::sync::Arc::as_ptr(m) as usize))
                    .collect();
                let shaped: HashSet<usize> = rows.iter().map(|r| std::rc::Rc::as_ptr(&r.shaped) as usize).collect();
                let links: HashSet<usize> = rows
                    .iter()
                    .filter_map(|r| r.link_hits.as_ref().map(|l| std::sync::Arc::as_ptr(l) as usize))
                    .collect();
                let with_cells = rows.iter().filter(|r| !r.cells.is_empty()).count();
                eprintln!(
                    "  paint: {} rows (cap {}), PaintRow {} B -> {} inline | distinct maps {} | shaped {} | link vecs {} | rows with cells {}",
                    rows.len(),
                    rows.capacity(),
                    std::mem::size_of::<crate::ui::text_editor::PaintRow>(),
                    mib((rows.capacity() * std::mem::size_of::<crate::ui::text_editor::PaintRow>()) as isize),
                    maps.len(),
                    shaped.len(),
                    links.len(),
                    with_cells,
                );
            }
            let before = live_bytes();
            paint.layout_cache = Default::default();
            let layout = before - live_bytes();
            let before = live_bytes();
            *paint = Default::default();
            let paint = before - live_bytes();
            (parsed, layout, paint)
        });
        eprintln!(
            "{mode:?}: note {} | tab built {} | after first frame {} | parse cache {} | layout cache {} | paint rows {}",
            mib(doc_len),
            mib(built - base),
            mib(drawn - base),
            mib(parsed),
            mib(layout),
            mib(paint),
        );
    }
}

/// Two large notes, as in a restored session plus a newly opened file: what
/// is live after each step, so a parse or cache an inactive tab keeps shows up.
#[ignore]
#[std_test]
fn bench_two_large_tabs() {
    use crate::heap_counter::live_allocs;
    let dir = std::env::temp_dir().join(format!("litemark-heap-bench-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let second = dir.join("second.md");
    std::fs::write(&second, sample_note(20_000)).unwrap();
    let first = sample_note(80_000);
    let report = |step: &str, base: (isize, isize)| {
        eprintln!(
            "{step:<28} live {} in {:>9} allocations",
            mib(live_bytes() - base.0),
            live_allocs() - base.1
        );
    };
    let mut tcx = TestAppContext::single();
    let base = (live_bytes(), live_allocs());
    let (view, cx) = tcx.add_window_view(|_window, cx| {
        let mut app = LiteMarkApp::test_new(&first, cx);
        app.settings.view_mode = ViewMode::Raw;
        app
    });
    drop(first);
    let mut cx = cx.clone();
    redraw(&mut cx);
    report("12.8 MB note open", base);
    view.update(&mut cx, |app, cx| app.open_path(second.clone(), cx));
    cx.run_until_parked();
    redraw(&mut cx);
    report("+ 3.2 MB note opened", base);
    let ids: Vec<String> = view.read_with(&cx, |app, _| app.tabs.iter().map(|t| t.id.to_string()).collect());
    for round in 0..3 {
        for id in &ids {
            view.update(&mut cx, |app, cx| {
                app.activate_tab(id.clone());
                cx.notify();
            });
            redraw(&mut cx);
        }
        report(&format!("after switch round {round}"), base);
    }
    view.update(&mut cx, |app, _| {
        for tab in &app.tabs {
            eprintln!(
                "  tab {} len {} parse cached {}",
                tab.title,
                tab.content.len(),
                tab.has_parse_cache_for_bench()
            );
        }
    });
    // Close the big (first) tab while it is active: its text, undo stack and
    // painted rows should all be gone.
    view.update(&mut cx, |app, cx| {
        app.activate_tab(ids[0].clone());
        cx.notify();
    });
    redraw(&mut cx);
    report("big tab active", base);
    view.update(&mut cx, |app, cx| {
        app.handle_close_tab(ids[0].clone());
        cx.notify();
    });
    report("big tab closed (no frame)", base);
    redraw(&mut cx);
    report("big tab closed + frame", base);
    let _ = std::fs::remove_dir_all(&dir);
}
