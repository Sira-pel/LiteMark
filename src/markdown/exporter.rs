use crate::markdown::image::{parse_image_line, size_suffix};
use pulldown_cmark::{html, CowStr, Event, Options, Parser, Tag, TagEnd};
use std::ops::Range;
use std::path::Path;

/// Largest local image embedded in an HTML export.
const MAX_EMBED_BYTES: u64 = 8 * 1024 * 1024;
/// Most image data embedded in one export.
const MAX_EMBED_TOTAL: u64 = 25 * 1024 * 1024;

#[cfg(test)]
pub fn markdown_to_html_body(markdown: &str) -> String {
    markdown_to_html_body_in(markdown, None, None)
}

// SVG is left out: a data: SVG can run script if opened on its own.
fn image_mime(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "bmp" => Some("image/bmp"),
        _ => None,
    }
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// `data:` URI for a local image of the note, so an exported page keeps its
/// pictures wherever it is opened. None leaves the link as written.
///
/// Only files inside `scope` (the workspace folder, or the note's own folder
/// outside a workspace) are embedded (a link to
/// `/home/me/.ssh/key.png` is not copied into a page that gets shared), and
/// `budget` is what is left of the export's size allowance.
fn embedded_image(target: &str, base: &Path, scope: &Path, budget: &mut u64) -> Option<String> {
    use crate::markdown::image::{resolve_image_target, ImageTarget};
    let ImageTarget::Local(path) = resolve_image_target(target, Some(base)) else {
        return None;
    };
    let mime = image_mime(&path)?;
    let path = path.canonicalize().ok()?;
    if !path.starts_with(scope.canonicalize().ok()?) {
        return None;
    }
    let meta = std::fs::metadata(&path).ok()?;
    if !meta.is_file() || meta.len() > MAX_EMBED_BYTES || meta.len() > *budget {
        return None;
    }
    let bytes = std::fs::read(&path).ok()?;
    // The file may have grown since its size was read.
    if bytes.len() as u64 > *budget {
        return None;
    }
    *budget -= bytes.len() as u64;
    Some(format!("data:{mime};base64,{}", base64(&bytes)))
}

/// HTML for `markdown`; with `base_dir` (the note's folder), local images are
/// embedded in the page.
pub fn markdown_to_html_body_in(
    markdown: &str,
    base_dir: Option<&Path>,
    scope: Option<&Path>,
) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let mut budget = MAX_EMBED_TOTAL;
    let parser = Parser::new_ext(markdown, options).into_offset_iter();
    let parser = parser.map(|(event, range)| match (event, base_dir) {
        (
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            }),
            Some(base),
        ) => {
            let scope = scope.unwrap_or(base);
            let dest_url =
                embedded_image(&dest_url, base, scope, &mut budget).map_or(dest_url, CowStr::from);
            (
                Event::Start(Tag::Image {
                    link_type,
                    dest_url,
                    title,
                    id,
                }),
                range,
            )
        }
        (event, _) => (event, range),
    });
    let mut html_output = String::new();
    html::push_html(
        &mut html_output,
        with_image_widths(markdown, parser).into_iter(),
    );
    html_output
}

/// The width and height of the image written at `range` of `markdown`: the
/// same rule the editor uses, so `a\|200` (an escaped bar) has none.
fn written_size(markdown: &str, range: &Range<usize>) -> Option<(u32, Option<u32>)> {
    let image = parse_image_line(markdown.get(range.clone())?)?;
    Some((image.width?, image.height))
}

/// Turns each `![alt|200](src)` into an `<img>` with `width="200"`. The alt
/// text reaches the parser's events as text between the image's start and
/// end, so it is gathered here; images without a width pass through as is.
fn with_image_widths<'a>(
    markdown: &str,
    events: impl Iterator<Item = (Event<'a>, Range<usize>)>,
) -> Vec<Event<'a>> {
    let mut out: Vec<Event<'a>> = Vec::new();
    type Size = Option<(u32, Option<u32>)>;
    // One entry per image being read; images can nest in another's alt text.
    let mut open: Vec<(usize, String, Size)> = Vec::new();
    for (event, range) in events {
        if matches!(event, Event::Start(Tag::Image { .. })) {
            open.push((out.len(), String::new(), written_size(markdown, &range)));
        } else if let Some((_, alt, _)) = open.last_mut() {
            match &event {
                Event::Text(t) | Event::Code(t) => alt.push_str(t),
                Event::SoftBreak | Event::HardBreak => alt.push(' '),
                _ => {}
            }
        }
        if matches!(event, Event::End(TagEnd::Image)) {
            if let Some((at, alt, size)) = open.pop() {
                let suffix = size.map_or(String::new(), |(w, h)| size_suffix(Some(w), h));
                let plain = alt
                    .strip_suffix(suffix.as_str())
                    .unwrap_or(&alt)
                    .to_string();
                // The enclosing image's alt text includes this one's.
                if let Some((_, outer, _)) = open.last_mut() {
                    outer.push_str(&plain);
                }
                if let (
                    Some((width, height)),
                    Event::Start(Tag::Image {
                        dest_url, title, ..
                    }),
                ) = (size, &out[at])
                {
                    let mut tag = format!(
                        "<img src=\"{}\" alt=\"{}\"",
                        escape_img_src(dest_url),
                        escape_html_text(&plain)
                    );
                    if !title.is_empty() {
                        tag.push_str(&format!(" title=\"{}\"", escape_html_text(title)));
                    }
                    match height {
                        // Both sizes stretch the picture; the ratio holds when the
                        // page is narrower than the width.
                        Some(height) => tag.push_str(&format!(
                            " width=\"{width}\" height=\"{height}\" style=\"height: auto; aspect-ratio: {width} / {height}\" />"
                        )),
                        None => tag.push_str(&format!(" width=\"{width}\" />")),
                    }
                    out.truncate(at);
                    out.push(Event::Html(CowStr::from(tag)));
                    continue;
                }
            }
        }
        out.push(event);
    }
    out
}

/// An image URL for an HTML attribute. `data:` URIs pass through; anything
/// else is percent-encoded the way the parser's own `<img>` output is, so a
/// path with spaces or quotes stays one valid URL.
fn escape_img_src(url: &str) -> String {
    if url.starts_with("data:") {
        return escape_html_text(url);
    }
    let mut out = String::with_capacity(url.len());
    for b in url.bytes() {
        match b {
            b'&' => out.push_str("&amp;"),
            b'\'' => out.push_str("&#x27;"),
            b'a'..=b'z'
            | b'A'..=b'Z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'~'
            | b':'
            | b'/'
            | b'?'
            | b'#'
            | b'@'
            | b'!'
            | b'$'
            | b'('
            | b')'
            | b'*'
            | b'+'
            | b','
            | b';'
            | b'='
            | b'%'
            | b'['
            | b']' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
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

#[cfg(test)]
pub fn export_to_html(title: &str, markdown: &str) -> String {
    export_to_html_in(title, markdown, None, None)
}

pub fn export_to_html_in(
    title: &str,
    markdown: &str,
    base_dir: Option<&Path>,
    scope: Option<&Path>,
) -> String {
    let body_html = markdown_to_html_body_in(markdown, base_dir, scope);
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
    pre code {{ background: none; padding: 0; color: inherit; }}
    blockquote {{ border-left: 4px solid #888; padding-left: 14px; color: #555; margin: 16px 0; }}
    table {{ border-collapse: collapse; width: 100%; margin: 16px 0; }}
    th, td {{ border: 1px solid #ddd; padding: 8px 12px; text-align: left; }}
    th {{ background: #f8f8f8; font-weight: 600; }}
    img {{ max-width: 100%; height: auto; }}
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
pub fn render_export(
    kind: &str,
    title: &str,
    markdown: &str,
    base_dir: Option<&Path>,
    scope: Option<&Path>,
) -> Result<String, String> {
    match kind {
        "html" => Ok(export_to_html_in(title, markdown, base_dir, scope)),
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

    let parser = Parser::new_ext(markdown, options).into_offset_iter();
    let mut text_output = String::new();
    let mut list_stack: Vec<Option<u64>> = Vec::new();
    let mut first_cell_in_row = false;
    // One entry per image being read: its alt text so far and the `|200` to
    // cut from it once the image ends.
    let mut images: Vec<(String, Option<String>)> = Vec::new();

    for (event, range) in parser {
        match event {
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::Image { .. }) => {
                let suffix = written_size(markdown, &range).map(|(w, h)| size_suffix(Some(w), h));
                images.push((String::new(), suffix));
            }
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Image) => {
                if let Some((alt, suffix)) = images.pop() {
                    let text = suffix
                        .as_deref()
                        .and_then(|suffix| alt.strip_suffix(suffix))
                        .unwrap_or(&alt);
                    match images.last_mut() {
                        Some((outer, _)) => outer.push_str(text),
                        None => text_output.push_str(text),
                    }
                }
            }
            pulldown_cmark::Event::Text(t) if !images.is_empty() => {
                if let Some((alt, _)) = images.last_mut() {
                    alt.push_str(&t);
                }
            }
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::List(start)) => {
                if !list_stack.is_empty() && !text_output.ends_with('\n') {
                    text_output.push('\n');
                }
                list_stack.push(start);
            }
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::List(_)) => {
                list_stack.pop();
                if list_stack.is_empty() && !text_output.ends_with("\n\n") {
                    text_output.push('\n');
                }
            }
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::Item) => {
                let depth = list_stack.len().saturating_sub(1);
                for _ in 0..depth {
                    text_output.push_str("  ");
                }
                match list_stack.last_mut() {
                    Some(Some(next)) => {
                        text_output.push_str(&format!("{next}. "));
                        *next += 1;
                    }
                    _ => text_output.push_str("- "),
                }
            }
            pulldown_cmark::Event::TaskListMarker(checked) => {
                text_output.push_str(if checked { "[x] " } else { "[ ] " });
            }
            pulldown_cmark::Event::Start(
                pulldown_cmark::Tag::TableHead | pulldown_cmark::Tag::TableRow,
            ) => {
                first_cell_in_row = true;
            }
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::TableCell) => {
                if !first_cell_in_row {
                    text_output.push('\t');
                }
                first_cell_in_row = false;
            }
            pulldown_cmark::Event::End(
                pulldown_cmark::TagEnd::TableHead | pulldown_cmark::TagEnd::TableRow,
            ) => {
                text_output.push('\n');
            }
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Table) => {
                text_output.push('\n');
            }
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
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Item) => {
                if !text_output.ends_with('\n') {
                    text_output.push('\n');
                }
            }
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::BlockQuote(_)) => {
                text_output.push('\n')
            }
            _ => {}
        }
    }

    text_output.trim().to_string()
}

#[cfg(test)]
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
        let html = render_export("html", "Page Title", "# Body Heading", None, None).unwrap();
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

        let markdown = render_export("md", "Page Title", "# Body Heading", None, None).unwrap();
        assert_eq!(markdown, "# Body Heading");
        let text = render_export("txt", "Page Title", "# Body Heading", None, None).unwrap();
        assert!(text.contains("Body Heading"));
        assert!(!text.contains('#'));
    }

    #[test]
    fn test_export_to_html_pre_code_inherits_dark_background() {
        let html = export_to_html("Code", "```rust\nlet x = 1;\n```");
        assert!(html.contains("pre code { background: none; padding: 0; color: inherit; }"));
        assert!(html.contains("<pre><code class=\"language-rust\">"));
    }

    #[test]
    fn test_export_to_plaintext_tables_lists_and_tasks() {
        let md = "\
| A | B |
| --- | --- |
| 1 | 2 |

- bullet one
- bullet two
  - nested bullet

3. third
4. fourth

- [ ] todo
- [x] done
";
        let plain = export_to_plaintext(md);
        assert_eq!(
            plain,
            "A\tB\n1\t2\n\n- bullet one\n- bullet two\n  - nested bullet\n\n3. third\n4. fourth\n\n- [ ] todo\n- [x] done"
        );
    }

    #[test]
    fn test_format_markdown_table() {
        let headers = vec!["A".to_string(), "B".to_string()];
        let rows = vec![vec!["1".to_string(), "2".to_string()]];
        let out = format_markdown_table(&headers, &rows);
        assert_eq!(out, "| A | B |\n| :--- | :--- |\n| 1 | 2 |\n");
    }

    #[test]
    fn html_export_writes_a_width_and_height_and_text_export_drops_them() {
        let md = "![cat|200x100](c.png)\n\nafter";
        let html = markdown_to_html_body_in(md, None, None);
        assert!(
            html.contains(
                r#"<img src="c.png" alt="cat" width="200" height="100" style="height: auto; aspect-ratio: 200 / 100" />"#
            ),
            "{html}"
        );
        let text = export_to_plaintext(md);
        assert!(
            text.contains("cat") && !text.contains('|') && !text.contains("200"),
            "{text}"
        );
    }

    #[test]
    fn html_export_writes_the_image_width() {
        let html = markdown_to_html_body_in(
            "![a \"cat\"|200](c.png \"T\")

![plain](p.png)

![a|b](q.png)

![e\\|200](r.png)",
            None,
            None,
        );
        assert!(
            html.contains(r#"<img src="r.png" alt="e|200" />"#),
            "{html}"
        );
        assert!(
            html.contains(r#"<img src="c.png" alt="a &quot;cat&quot;" title="T" width="200" />"#),
            "{html}"
        );
        assert!(
            html.contains(r#"<img src="p.png" alt="plain" />"#),
            "{html}"
        );
        assert!(html.contains(r#"<img src="q.png" alt="a|b" />"#), "{html}");
    }

    #[test]
    fn a_sized_image_is_still_embedded() {
        let dir = std::env::temp_dir().join(format!("litemark-export-w-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(dir.join("assets/a b.png"), b"abcd").unwrap();
        let html = markdown_to_html_body_in("![x|50](<assets/a b.png>)", Some(&dir), None);
        assert!(
            html.contains("src=\"data:image/png;base64,YWJjZA==\""),
            "{html}"
        );
        assert!(html.contains("width=\"50\""), "{html}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn text_export_drops_the_width_from_the_alt() {
        assert_eq!(export_to_plaintext("![cat|200](c.png)"), "cat");
        assert_eq!(export_to_plaintext("![a|b](c.png)"), "a|b");
    }

    #[test]
    fn html_export_embeds_local_images() {
        let dir = std::env::temp_dir().join(format!("litemark-export-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(dir.join("assets/a b.png"), b"abcd").unwrap();
        let md = "![x](<assets/a b.png>)\n\n![y](missing.png)\n\n![z](https://e.x/a.png)";
        let html = markdown_to_html_body_in(md, Some(&dir), None);
        assert!(
            html.contains("src=\"data:image/png;base64,YWJjZA==\""),
            "{html}"
        );
        assert!(html.contains("src=\"missing.png\""));
        assert!(html.contains("src=\"https://e.x/a.png\""));
        assert!(markdown_to_html_body("![x](a.png)").contains("src=\"a.png\""));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn html_export_embeds_only_images_in_the_notes_folder() {
        let root =
            std::env::temp_dir().join(format!("litemark-export-scope-{}", std::process::id()));
        let note = root.join("note");
        std::fs::create_dir_all(note.join("assets")).unwrap();
        std::fs::write(root.join("secret.png"), b"s").unwrap();
        std::fs::write(note.join("assets/a.png"), b"a").unwrap();
        let outside = root.join("secret.png");
        let md = format!(
            "![a](assets/a.png)\n\n![b](../secret.png)\n\n![c](<{}>)",
            outside.display()
        );
        let html = markdown_to_html_body_in(&md, Some(&note), None);
        assert_eq!(html.matches("data:image/png").count(), 1, "{html}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn html_export_embeds_images_from_a_sibling_folder_inside_the_workspace() {
        let root = std::env::temp_dir().join(format!("litemark-export-ws-{}", std::process::id()));
        let note = root.join("note");
        std::fs::create_dir_all(&note).unwrap();
        std::fs::create_dir_all(root.join("shared")).unwrap();
        std::fs::write(root.join("shared/a.png"), b"a").unwrap();
        let md = "![a](../shared/a.png)";
        assert!(!markdown_to_html_body_in(md, Some(&note), None).contains("data:"));
        assert!(markdown_to_html_body_in(md, Some(&note), Some(&root)).contains("data:image/png"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sized_image_src_is_percent_encoded() {
        let html = markdown_to_html_body_in("![a|100](<my pic \"x\".png>)", None, None);
        assert!(html.contains(r#"src="my%20pic%20%22x%22.png""#), "{html}");
        assert!(html.contains(r#"width="100""#), "{html}");
    }

    #[test]
    fn nested_sized_image_alt_excludes_inner_width() {
        let html = markdown_to_html_body_in("![a ![b|50](in.png) c|200](out.png)", None, None);
        assert!(
            html.contains(r#"<img src="out.png" alt="a b c" width="200" />"#),
            "{html}"
        );
    }

    #[test]
    fn html_export_makes_images_responsive() {
        assert!(export_to_html("t", "x").contains("img { max-width: 100%; height: auto; }"));
    }

    #[test]
    fn plaintext_export_strips_the_width_once_from_split_alt_text() {
        assert_eq!(export_to_plaintext("![a *b*|200](c.png)").trim(), "a b");
        assert_eq!(export_to_plaintext("![x|y|200](c.png)").trim(), "x|y");
    }
}
