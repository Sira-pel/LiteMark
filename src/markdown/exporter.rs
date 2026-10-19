use pulldown_cmark::{html, Options, Parser};

pub fn markdown_to_html_body(markdown: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(markdown, options);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    html_output
}

/// File stem with one extension removed. `a.txt` becomes `a`, not `a.txt`.
pub fn export_stem(title: &str) -> &str {
    match title.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() && !ext.contains('/') => stem,
        _ => title,
    }
}

pub fn escape_html_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

pub fn export_to_html(title: &str, markdown: &str) -> String {
    let body_html = markdown_to_html_body(markdown);
    let title = escape_html_text(title);
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <title>{title}</title>
  <style>
    body {{
      font-family: 'Ubuntu', -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
      line-height: 1.6;
      max-width: 800px;
      margin: 40px auto;
      padding: 0 20px;
      color: #222;
    }}
    h1, h2, h3 {{ color: #111; }}
    code {{ background: #f4f4f4; padding: 2px 5px; border-radius: 4px; font-family: 'Ubuntu Mono', monospace; }}
    pre {{ background: #282c34; color: #abb2bf; padding: 16px; border-radius: 6px; overflow-x: auto; }}
    blockquote {{ border-left: 4px solid #888; padding-left: 14px; color: #555; margin: 16px 0; }}
    table {{ border-collapse: collapse; width: 100%; margin: 16px 0; }}
    th, td {{ border: 1px solid #ddd; padding: 8px 12px; text-align: left; }}
    th {{ background: #f8f8f8; font-weight: 600; }}
    ul.contains-task-list {{ list-style-type: none; padding-left: 0; }}
    li.task-list-item {{ display: flex; align-items: center; gap: 8px; }}
  </style>
</head>
<body>
  {body_html}
</body>
</html>"#
    )
}

/// Build an export body. `title` is the document title and `markdown` is the source.
pub fn render_export(kind: &str, title: &str, markdown: &str) -> Result<String, String> {
    match kind {
        "html" => Ok(export_to_html(title, markdown)),
        "md" | "markdown" => Ok(markdown.to_string()),
        "txt" | "text" => Ok(export_to_plaintext(markdown)),
        other => Err(format!("Unsupported export format: {other}")),
    }
}

pub fn export_to_plaintext(markdown: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(markdown, options);
    let mut text_output = String::new();

    for event in parser {
        match event {
            pulldown_cmark::Event::Text(t) => text_output.push_str(&t),
            pulldown_cmark::Event::Code(c) => text_output.push_str(&c),
            pulldown_cmark::Event::SoftBreak | pulldown_cmark::Event::HardBreak => {
                text_output.push('\n')
            }
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Paragraph) => {
                text_output.push_str("\n\n")
            }
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Heading(_)) => {
                text_output.push_str("\n\n")
            }
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Item) => text_output.push('\n'),
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::BlockQuote(_)) => {
                text_output.push('\n')
            }
            _ => {}
        }
    }

    text_output.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_to_html() {
        let md = "# LiteMark Test\n\nThis is **bold** and *italic*.";
        let html = export_to_html("Test Document", md);
        assert!(html.contains("<title>Test Document</title>"));
        assert!(html.contains("<h1>LiteMark Test</h1>"));
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("<em>italic</em>"));
        assert!(html.contains("font-family: 'Ubuntu'"));
    }

    #[test]
    fn test_export_to_plaintext() {
        let md = "# Header 1\n\nParagraph text with **bold**.";
        let plain = export_to_plaintext(md);
        assert!(plain.contains("Header 1"));
        assert!(plain.contains("Paragraph text with bold."));
        assert!(!plain.contains("<h1>"));
        assert!(!plain.contains("**"));
    }

    #[test]
    fn export_html_uses_title_then_markdown() {
        let html = render_export("html", "Page Title", "# Body Heading").unwrap();
        assert!(html.contains("<title>Page Title</title>"));
        assert!(html.contains("<h1>Body Heading</h1>"));
        assert!(!html.contains("<title># Body Heading</title>"));
        assert!(!html.contains("<h1>Page Title</h1>"));

        let escaped = export_to_html(r#"a<b>&"quoted""#, "x");
        assert!(escaped.contains("<title>a&lt;b&gt;&amp;&quot;quoted&quot;</title>"));
        assert!(!escaped.contains("<title>a<b>"));
        assert_eq!(export_stem("a.txt"), "a");
        assert_eq!(export_stem("notes.md"), "notes");
        assert_eq!(export_stem("README"), "README");

        let markdown = render_export("md", "Page Title", "# Body Heading").unwrap();
        assert_eq!(markdown, "# Body Heading");
        let text = render_export("txt", "Page Title", "# Body Heading").unwrap();
        assert!(text.contains("Body Heading"));
        assert!(!text.contains('#'));
    }

    #[test]
    fn test_format_markdown_table() {
        let headers = vec!["A".to_string(), "B".to_string()];
        let rows = vec![vec!["1".to_string(), "2".to_string()]];
        let out = format_markdown_table(&headers, &rows);
        assert_eq!(out, "| A | B |\n| :--- | :--- |\n| 1 | 2 |\n");
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn format_markdown_table(headers: &[String], rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    out.push_str("| ");
    out.push_str(&headers.join(" | "));
    out.push_str(" |\n| ");
    let separators = vec![":---"; headers.len()];
    out.push_str(&separators.join(" | "));
    out.push_str(" |\n");
    for row in rows {
        out.push_str("| ");
        let cells: Vec<String> = (0..headers.len())
            .map(|i| row.get(i).cloned().unwrap_or_default())
            .collect();
        out.push_str(&cells.join(" | "));
        out.push_str(" |\n");
    }
    out
}
