//! The note outline: the headings of a Markdown document, read from the
//! parse the editor already keeps, so nothing is parsed again for it.

use crate::markdown::{LineKind, ParsedDocument};

/// One heading of the outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineItem {
    /// 1 for `#` up to 6 for `######`.
    pub level: u8,
    /// Heading text with inline Markdown stripped (`**bold**` is "bold").
    pub text: String,
    /// Byte offset of the start of the heading line.
    pub offset: usize,
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
            }),
            _ => None,
        })
        .collect()
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
}
