#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub code: bool,
    pub link: Option<String>,
}

impl Span {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            code: false,
            link: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Heading {
        level: u8, // 1, 2, 3
        spans: Vec<Span>,
    },
    Paragraph {
        spans: Vec<Span>,
    },
    Blockquote {
        spans: Vec<Span>,
    },
    CodeBlock {
        language: Option<String>,
        code: String,
    },
    ListItem {
        ordered: bool,
        index: usize,
        is_task: bool,
        checked: bool,
        spans: Vec<Span>,
    },
    Table {
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    HorizontalRule,
    EmptyLine,
}

pub fn parse_spans(input: &str) -> Vec<Span> {
    if input.is_empty() {
        return Vec::new();
    }

    let mut options = pulldown_cmark::Options::empty();
    options.insert(pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
    let parser = pulldown_cmark::Parser::new_ext(input, options);

    let mut spans = Vec::new();
    let mut in_bold = false;
    let mut in_italic = false;
    let mut in_strike = false;
    let mut in_underline = false;
    let mut active_link: Option<String> = None;

    for event in parser {
        match event {
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::Strong) => in_bold = true,
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Strong) => in_bold = false,
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::Emphasis) => in_italic = true,
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Emphasis) => in_italic = false,
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::Strikethrough) => in_strike = true,
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Strikethrough) => in_strike = false,
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::Link { dest_url, .. }) => {
                active_link = Some(dest_url.to_string());
            }
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Link) => {
                active_link = None;
            }
            pulldown_cmark::Event::InlineHtml(h) | pulldown_cmark::Event::Html(h) => {
                if h.contains("<u>") {
                    in_underline = true;
                } else if h.contains("</u>") {
                    in_underline = false;
                }
            }
            pulldown_cmark::Event::Code(c) => {
                spans.push(Span {
                    text: c.to_string(),
                    bold: in_bold,
                    italic: in_italic,
                    underline: in_underline,
                    strike: in_strike,
                    code: true,
                    link: active_link.clone(),
                });
            }
            pulldown_cmark::Event::Text(t) => {
                spans.push(Span {
                    text: t.to_string(),
                    bold: in_bold,
                    italic: in_italic,
                    underline: in_underline,
                    strike: in_strike,
                    code: false,
                    link: active_link.clone(),
                });
            }
            _ => {}
        }
    }

    if spans.is_empty() {
        spans.push(Span::plain(input));
    }

    spans
}

pub fn parse_markdown(markdown: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let lines: Vec<&str> = markdown.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        if trimmed.is_empty() {
            blocks.push(Block::EmptyLine);
            i += 1;
            continue;
        }

        // Code block ```
        if trimmed.starts_with("```") {
            let language = trimmed.strip_prefix("```").map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
            let mut code_lines = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim().starts_with("```") {
                code_lines.push(lines[i]);
                i += 1;
            }
            if i < lines.len() {
                i += 1; // skip closing ```
            }
            blocks.push(Block::CodeBlock {
                language,
                code: code_lines.join("\n"),
            });
            continue;
        }

        // Horizontal rule ---, ***, ___
        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            blocks.push(Block::HorizontalRule);
            i += 1;
            continue;
        }

        // Headings #, ##, ###
        if let Some(rest) = trimmed.strip_prefix("### ") {
            blocks.push(Block::Heading {
                level: 3,
                spans: parse_spans(rest),
            });
            i += 1;
            continue;
        } else if let Some(rest) = trimmed.strip_prefix("## ") {
            blocks.push(Block::Heading {
                level: 2,
                spans: parse_spans(rest),
            });
            i += 1;
            continue;
        } else if let Some(rest) = trimmed.strip_prefix("# ") {
            blocks.push(Block::Heading {
                level: 1,
                spans: parse_spans(rest),
            });
            i += 1;
            continue;
        }

        // Blockquote >
        if let Some(rest) = trimmed.strip_prefix("> ") {
            blocks.push(Block::Blockquote {
                spans: parse_spans(rest),
            });
            i += 1;
            continue;
        }

        // Markdown Table | col1 | col2 |
        if trimmed.starts_with('|') && trimmed.ends_with('|') {
            let mut table_rows = Vec::new();
            while i < lines.len() {
                let cur = lines[i].trim();
                if cur.starts_with('|') && cur.ends_with('|') {
                    // Check if it is delimiter row |---|---|
                    let is_separator = cur.chars().all(|c| c == '|' || c == '-' || c == ':' || c == ' ');
                    if !is_separator {
                        let cells: Vec<String> = cur
                            .trim_matches('|')
                            .split('|')
                            .map(|c| c.trim().to_string())
                            .collect();
                        table_rows.push(cells);
                    }
                    i += 1;
                } else {
                    break;
                }
            }

            if !table_rows.is_empty() {
                let headers = table_rows.remove(0);
                blocks.push(Block::Table {
                    headers,
                    rows: table_rows,
                });
            }
            continue;
        }

        // Task List - [ ] or - [x]
        if let Some(rest) = trimmed.strip_prefix("- [ ] ").or_else(|| trimmed.strip_prefix("* [ ] ")) {
            blocks.push(Block::ListItem {
                ordered: false,
                index: 0,
                is_task: true,
                checked: false,
                spans: parse_spans(rest),
            });
            i += 1;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("- [x] ").or_else(|| trimmed.strip_prefix("- [X] "))
            .or_else(|| trimmed.strip_prefix("* [x] ")).or_else(|| trimmed.strip_prefix("* [X] ")) {
            blocks.push(Block::ListItem {
                ordered: false,
                index: 0,
                is_task: true,
                checked: true,
                spans: parse_spans(rest),
            });
            i += 1;
            continue;
        }

        // Unordered List - or *
        if let Some(rest) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
            blocks.push(Block::ListItem {
                ordered: false,
                index: 0,
                is_task: false,
                checked: false,
                spans: parse_spans(rest),
            });
            i += 1;
            continue;
        }

        // Ordered list 1.
        let mut is_ordered = false;
        let mut ord_num = 1;
        let mut ord_rest = "";
        for (idx, ch) in trimmed.char_indices() {
            if ch == '.' && idx > 0 {
                if let Ok(num) = trimmed[..idx].parse::<usize>() {
                    let rem = trimmed[idx + 1..].trim_start();
                    is_ordered = true;
                    ord_num = num;
                    ord_rest = rem;
                    break;
                }
            } else if !ch.is_ascii_digit() {
                break;
            }
        }

        if is_ordered {
            blocks.push(Block::ListItem {
                ordered: true,
                index: ord_num,
                is_task: false,
                checked: false,
                spans: parse_spans(ord_rest),
            });
            i += 1;
            continue;
        }

        // Default Paragraph
        blocks.push(Block::Paragraph {
            spans: parse_spans(line),
        });
        i += 1;
    }

    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_headings_and_hr() {
        let md = "# Title\n## Subtitle\n### Section\n---";
        let blocks = parse_markdown(md);
        assert_eq!(blocks.len(), 4);
        match &blocks[0] {
            Block::Heading { level, .. } => assert_eq!(*level, 1),
            _ => panic!("Expected H1"),
        }
        match &blocks[1] {
            Block::Heading { level, .. } => assert_eq!(*level, 2),
            _ => panic!("Expected H2"),
        }
        match &blocks[2] {
            Block::Heading { level, .. } => assert_eq!(*level, 3),
            _ => panic!("Expected H3"),
        }
        assert!(matches!(blocks[3], Block::HorizontalRule));
    }

    #[test]
    fn test_spans_formatting() {
        let input = "Normal **bold** *italic* <u>underlined</u> ~~strike~~ `code`";
        let spans = parse_spans(input);
        assert!(!spans.is_empty());
        let has_bold = spans.iter().any(|s| s.bold && s.text.contains("bold"));
        let has_italic = spans.iter().any(|s| s.italic && s.text.contains("italic"));
        let has_underline = spans.iter().any(|s| s.underline && s.text.contains("underlined"));
        let has_strike = spans.iter().any(|s| s.strike && s.text.contains("strike"));
        let has_code = spans.iter().any(|s| s.code && s.text.contains("code"));

        assert!(has_bold, "Expected bold span");
        assert!(has_italic, "Expected italic span");
        assert!(has_underline, "Expected underline span");
        assert!(has_strike, "Expected strike span");
        assert!(has_code, "Expected code span");
    }

    #[test]
    fn test_task_list() {
        let md = "- [ ] Task to do\n- [x] Completed task";
        let blocks = parse_markdown(md);
        assert_eq!(blocks.len(), 2);
        match &blocks[0] {
            Block::ListItem { is_task, checked, .. } => {
                assert!(is_task);
                assert!(!checked);
            }
            _ => panic!("Expected task item"),
        }
        match &blocks[1] {
            Block::ListItem { is_task, checked, .. } => {
                assert!(is_task);
                assert!(checked);
            }
            _ => panic!("Expected completed task item"),
        }
    }

    #[test]
    fn test_table_parsing() {
        let md = "| Col 1 | Col 2 |\n| :--- | :--- |\n| Val A | Val B |";
        let blocks = parse_markdown(md);
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            Block::Table { headers, rows } => {
                assert_eq!(headers.len(), 2);
                assert_eq!(headers[0], "Col 1");
                assert_eq!(headers[1], "Col 2");
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "Val A");
                assert_eq!(rows[0][1], "Val B");
            }
            _ => panic!("Expected table block"),
        }
    }
}

