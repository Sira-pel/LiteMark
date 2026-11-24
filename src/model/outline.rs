//! The note outline: the headings of a Markdown document, read from the
//! parse the editor already keeps, so nothing is parsed again for it.

use crate::markdown::{atx_display_content_range, parse_document, LineKind, ParsedDocument};

/// One heading of the outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineItem {
    /// 1 for `#` up to 6 for `######`.
    pub level: u8,
    /// Heading text with inline Markdown stripped (`**bold**` is "bold").
    pub text: String,
    /// Byte offset of the start of the heading line.
    pub offset: usize,
    /// 0-based line index of the heading in `ParsedDocument::lines`.
    pub line_index: u32,
}

/// Every heading of `parsed`, in document order.
pub fn outline_items(parsed: &ParsedDocument, content: &str) -> Vec<OutlineItem> {
    parsed
        .lines
        .iter()
        .filter_map(|line| match line.kind {
            LineKind::Heading { level } => Some(OutlineItem {
                level,
                text: line.display_text(content).trim().to_string(),
                offset: line.source.start,
                line_index: line.line_index,
            }),
            _ => None,
        })
        .collect()
}

/// True when an incremental parse splice changed neither any heading in the
/// spliced range nor any subsequent heading's line index or byte offset.
pub fn can_skip_outline_patch(
    items: &[OutlineItem],
    parsed: &ParsedDocument,
    restart: usize,
    removed: usize,
    inserted: usize,
    byte_delta: isize,
) -> bool {
    let first_ix = items.partition_point(|it| (it.line_index as usize) < restart);
    let end_ix = items.partition_point(|it| (it.line_index as usize) < restart + removed);
    if first_ix != end_ix {
        return false;
    }
    let has_new_heading = parsed.lines[restart..restart + inserted]
        .iter()
        .any(|line| matches!(line.kind, LineKind::Heading { .. }));
    if has_new_heading {
        return false;
    }
    end_ix == items.len() || (byte_delta == 0 && inserted == removed)
}

/// Incrementally update `items` from a [`ParsedDocument::apply_edit`] splice.
pub fn patch_outline_items(
    items: &mut Vec<OutlineItem>,
    parsed: &ParsedDocument,
    content: &str,
    restart: usize,
    removed: usize,
    inserted: usize,
    byte_delta: isize,
) {
    let first_ix = items.partition_point(|it| (it.line_index as usize) < restart);
    let end_ix = items.partition_point(|it| (it.line_index as usize) < restart + removed);
    let new_headings: Vec<OutlineItem> = parsed.lines[restart..restart + inserted]
        .iter()
        .filter_map(|line| match line.kind {
            LineKind::Heading { level } => Some(OutlineItem {
                level,
                text: line.display_text(content).trim().to_string(),
                offset: line.source.start,
                line_index: line.line_index,
            }),
            _ => None,
        })
        .collect();
    let line_delta = inserted as isize - removed as isize;
    for item in &mut items[end_ix..] {
        item.line_index = (item.line_index as isize + line_delta) as u32;
        item.offset = (item.offset as isize + byte_delta) as usize;
    }
    if first_ix != end_ix || !new_headings.is_empty() {
        items.splice(first_ix..end_ix, new_headings);
    }
}

fn leading_fence_info(line: &str) -> Option<(u8, usize, &str)> {
    let bytes = line.as_bytes();
    let mut col = 0usize;
    let mut i = 0usize;
    while i < bytes.len() && col < 4 {
        match bytes[i] {
            b' ' => {
                col += 1;
                i += 1;
            }
            b'\t' => {
                col += 4 - (col % 4);
                if col > 3 {
                    return None;
                }
                i += 1;
            }
            _ => break,
        }
    }
    if col > 3 || i >= bytes.len() {
        return None;
    }
    let ch = bytes[i];
    if ch != b'`' && ch != b'~' {
        return None;
    }
    let start = i;
    while i < bytes.len() && bytes[i] == ch {
        i += 1;
    }
    let len = i - start;
    if len >= 3 {
        Some((ch, len, &line[i..]))
    } else {
        None
    }
}

/// Lightweight ATX heading scan for Raw mode that skips building a full AST.
pub fn outline_items_raw(content: &str) -> Vec<OutlineItem> {
    let mut items = Vec::new();
    let bytes = content.as_bytes();
    let mut pos = 0usize;
    let mut line_index = 0u32;
    let mut open_fence: Option<(u8, usize)> = None;

    while pos < bytes.len() {
        let line_start = pos;
        let line_end = match memchr::memchr2(b'\n', b'\r', &bytes[pos..]) {
            Some(rel) => pos + rel,
            None => bytes.len(),
        };
        let line = &content[line_start..line_end];

        if let Some((marker, open_len)) = open_fence {
            if let Some((ch, len, rest)) = leading_fence_info(line) {
                if ch == marker && len >= open_len && rest.bytes().all(|b| b == b' ' || b == b'\t')
                {
                    open_fence = None;
                }
            }
        } else {
            // Fast-skip lines that cannot start a fence or an ATX heading.
            let first_non_ws = line.bytes().find(|&b| b != b' ' && b != b'\t');
            if matches!(first_non_ws, Some(b'#' | b'`' | b'~')) {
                if let Some((ch, len, rest)) = leading_fence_info(line) {
                    if ch == b'~' || !rest.as_bytes().contains(&b'`') {
                        open_fence = Some((ch, len));
                    }
                } else if let Some((level, content_range)) = atx_display_content_range(line) {
                    let raw = &line[content_range];
                    let text = if !raw
                        .as_bytes()
                        .iter()
                        .any(|&b| matches!(b, b'*' | b'_' | b'~' | b'`' | b'[' | b'<' | b'\\'))
                    {
                        raw.trim().to_string()
                    } else {
                        let parsed = parse_document(line);
                        parsed
                            .lines
                            .first()
                            .map(|l| l.display_text(line).trim().to_string())
                            .unwrap_or_default()
                    };
                    items.push(OutlineItem {
                        level,
                        text,
                        offset: line_start,
                        line_index,
                    });
                }
            }
        }

        line_index += 1;
        if line_end == bytes.len() {
            break;
        }
        pos = if bytes[line_end] == b'\r' && bytes.get(line_end + 1) == Some(&b'\n') {
            line_end + 2
        } else {
            line_end + 1
        };
    }

    items
}

/// Index of the heading whose section holds `caret`: the last heading that
/// starts at or before it. `None` before the first heading.
pub fn current_heading(items: &[OutlineItem], caret: usize) -> Option<usize> {
    items
        .partition_point(|item| item.offset <= caret)
        .checked_sub(1)
}

/// Which headings the collapsed strip draws when only `cap` lines fit:
/// `(item index, is current section)` for each line. Headings are sampled
/// evenly, and the line standing for the current section is the last
/// sample at or before it.
pub fn strip_sample(count: usize, cap: usize, current: Option<usize>) -> Vec<(usize, bool)> {
    let picked: Vec<usize> = if count <= cap {
        (0..count).collect()
    } else if cap == 0 {
        Vec::new()
    } else if cap == 1 {
        vec![0]
    } else {
        (0..cap).map(|i| i * (count - 1) / (cap - 1)).collect()
    };
    let active = current.and_then(|c| picked.iter().rposition(|&i| i <= c));
    picked
        .into_iter()
        .enumerate()
        .map(|(slot, index)| (index, Some(slot) == active))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::parse_document;

    fn items(text: &str) -> Vec<OutlineItem> {
        outline_items(&parse_document(text), text)
    }

    #[test]
    fn lists_levels_text_and_offsets() {
        let text = "# One\n\nbody\n\n## Two\n### Three\n";
        let got = items(text);
        let summary: Vec<_> = got.iter().map(|i| (i.level, i.text.as_str())).collect();
        assert_eq!(summary, [(1, "One"), (2, "Two"), (3, "Three")]);
        assert_eq!(got[1].offset, text.find("## Two").unwrap());
    }

    #[test]
    fn inline_markdown_is_stripped() {
        let got = items("# **bold** and `code` and [link](http://x.y)\n");
        assert_eq!(got[0].text, "bold and code and link");
    }

    #[test]
    fn fenced_code_comments_are_not_headings() {
        assert!(items("```\n# not a heading\n```\n").is_empty());
    }

    #[test]
    fn no_headings_gives_an_empty_outline() {
        assert!(items("just text\n\n- a list\n").is_empty());
        assert_eq!(current_heading(&[], 5), None);
    }

    #[test]
    fn current_heading_follows_the_caret() {
        let text = "intro\n# A\ntext\n## B\nmore\n# C\n";
        let got = items(text);
        assert_eq!(current_heading(&got, 0), None, "before the first heading");
        assert_eq!(current_heading(&got, text.find("# A").unwrap()), Some(0));
        assert_eq!(current_heading(&got, text.find("text").unwrap()), Some(0));
        assert_eq!(current_heading(&got, text.find("## B").unwrap()), Some(1));
        assert_eq!(current_heading(&got, text.find("more").unwrap()), Some(1));
        assert_eq!(current_heading(&got, text.len()), Some(2));
    }

    #[test]
    fn the_strip_shows_every_heading_when_they_fit() {
        let lines = strip_sample(3, 40, Some(1));
        assert_eq!(lines, [(0, false), (1, true), (2, false)]);
        assert!(strip_sample(3, 40, None).iter().all(|(_, on)| !on));
    }

    #[test]
    fn the_strip_is_sampled_when_too_long() {
        let lines = strip_sample(200, 40, Some(150));
        assert_eq!(lines.len(), 40);
        assert_eq!(lines[0].0, 0);
        assert_eq!(lines[39].0, 199, "keeps the last heading");
        assert_eq!(lines.iter().filter(|(_, on)| *on).count(), 1);
        let (index, _) = lines.iter().find(|(_, on)| *on).unwrap();
        assert!(
            *index <= 150 && 150 - *index < 6,
            "the nearest line before it"
        );
        assert!(strip_sample(5, 0, Some(1)).is_empty());
    }

    #[test]
    fn setext_underlines_are_not_headings() {
        // The editor's parser only knows `#` headings, so the outline
        // matches what the editor itself shows as a heading.
        assert!(items("Title\n=====\n").is_empty());
    }

    #[test]
    fn raw_outline_matches_parsed_outline_for_atx_and_fences() {
        let doc = "# One\r\n\r\n```\r\n# inside\r\n```\r\n## **Two** `code` ##\n~~~\n# tilde\n~~~\n    # indented\n### Three\r";
        let expected = items(doc);
        let got = outline_items_raw(doc);
        assert_eq!(got, expected);
    }

    #[test]
    fn incremental_outline_patch_matches_full_scan_across_random_edits() {
        let mut text = "# Top\n\npara one\n\n## Mid\n\n```\n# fenced\n```\n\n### End\n".to_string();
        let mut doc = parse_document(&text);
        let mut cached = outline_items(&doc, &text);
        let snippets = [
            "abc",
            "\n# Added\n",
            "\n```\n",
            "## ",
            "### Sub\nline\n",
            " ",
            "\n",
        ];
        let mut seed = 0x9e37_79b9_7f4a_7c15u64;
        for step in 0..150 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let a = (seed as usize) % (text.len() + 1);
            let b = ((seed >> 16) as usize) % (text.len() + 1);
            let mut start = a.min(b);
            let mut end = a.max(b).min((start + 24).min(text.len()));
            while !text.is_char_boundary(start) {
                start -= 1;
            }
            while !text.is_char_boundary(end) {
                end += 1;
            }
            let ins = if (seed >> 32).is_multiple_of(3) {
                ""
            } else {
                snippets[((seed >> 40) as usize) % snippets.len()]
            };
            text.replace_range(start..end, ins);
            doc.apply_edit(&text, start..end, ins.len());
            if let Some((_, restart, removed, inserted)) = doc.meta.last_splice {
                if !can_skip_outline_patch(
                    &cached,
                    &doc,
                    restart,
                    removed,
                    inserted,
                    doc.meta.last_byte_delta,
                ) {
                    patch_outline_items(
                        &mut cached,
                        &doc,
                        &text,
                        restart,
                        removed,
                        inserted,
                        doc.meta.last_byte_delta,
                    );
                }
            } else {
                cached = outline_items(&doc, &text);
            }
            assert_eq!(cached, outline_items(&doc, &text), "step {step}");
        }
    }
}
