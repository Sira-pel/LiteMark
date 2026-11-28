//! Intermediate block/inline document model for PDF export.
//!
//! Built from Markdown (via `pulldown-cmark` + LiteMark's `==highlight==`,
//! `<u>` underline, and `![alt|WxH]` image-sizing rules), plain text, or code.

use crate::markdown::exporter::{apply_highlights, written_size, MAX_EMBED_BYTES, MAX_EMBED_TOTAL};
use crate::markdown::image::{resolve_image_target, size_suffix, ImageTarget};
use crate::model::FileType;
use crate::services::link_policy::openable_link;
use krilla::image::Image as KrillaImage;
use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use std::collections::HashMap;
use std::path::Path;

/// Largest single image dimension (width or height) allowed for PDF embedding.
pub(crate) const MAX_EMBED_IMAGE_DIM: u32 = 8_192;
/// Largest total pixel count allowed for PDF embedding (~32 MP).
pub(crate) const MAX_EMBED_IMAGE_PIXELS: u64 = 32_000_000;
/// Maximum allocation the fallback image decoder may make while decoding one image.
const MAX_DECODE_ALLOC_BYTES: u64 = 160 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LinkTarget {
    Uri(String),
    InternalHeading(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct InlineStyle {
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    pub(crate) underline: bool,
    pub(crate) strikethrough: bool,
    pub(crate) highlight: bool,
    pub(crate) code: bool,
    pub(crate) link: Option<LinkTarget>,
}

#[derive(Debug, Clone)]
pub(crate) struct InlineSpan {
    pub(crate) text: String,
    pub(crate) style: InlineStyle,
    /// When set, renders this embedded picture inline instead of `text`
    /// (`text` holds the alt text for heading outlines and accessibility).
    pub(crate) image: Option<Box<PdfImage>>,
}

impl InlineSpan {
    pub(crate) fn text(text: impl Into<String>, style: InlineStyle) -> Self {
        Self {
            text: text.into(),
            style,
            image: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TableAlign {
    Left,
    Center,
    Right,
}

impl From<Alignment> for TableAlign {
    fn from(value: Alignment) -> Self {
        match value {
            Alignment::Center => Self::Center,
            Alignment::Right => Self::Right,
            Alignment::None | Alignment::Left => Self::Left,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum PdfImageSource {
    Embedded {
        image: KrillaImage,
        pixel_width: u32,
        pixel_height: u32,
    },
    Placeholder {
        remote_uri: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct PdfImage {
    pub(crate) alt: String,
    pub(crate) requested_width: Option<u32>,
    pub(crate) requested_height: Option<u32>,
    pub(crate) source: PdfImageSource,
}

#[derive(Debug, Clone)]
pub(crate) struct ListItem {
    pub(crate) task: Option<bool>,
    pub(crate) blocks: Vec<PdfBlock>,
}

#[derive(Debug, Clone)]
pub(crate) struct FootnoteEntry {
    pub(crate) label: String,
    pub(crate) blocks: Vec<PdfBlock>,
}

#[derive(Debug, Clone)]
pub(crate) enum PdfBlock {
    Heading {
        level: u8,
        spans: Vec<InlineSpan>,
        plain_text: String,
        slug: String,
    },
    Paragraph(Vec<InlineSpan>),
    CodeBlock {
        lang: Option<String>,
        code: String,
    },
    /// Monospace full-width code document (for `FileType::Code` tabs).
    PlainCodeLines(Vec<String>),
    BlockQuote(Vec<PdfBlock>),
    List {
        ordered_start: Option<u64>,
        items: Vec<ListItem>,
    },
    Table {
        alignments: Vec<TableAlign>,
        header: Vec<Vec<InlineSpan>>,
        rows: Vec<Vec<Vec<InlineSpan>>>,
    },
    Image(PdfImage),
    HorizontalRule,
    FootnoteSection(Vec<FootnoteEntry>),
}

#[derive(Debug, Clone)]
pub(crate) struct PdfDocumentModel {
    pub(crate) blocks: Vec<PdfBlock>,
}

/// Convert `content` into a [`PdfDocumentModel`] according to `file_type`.
pub(crate) fn build_document_model(
    content: &str,
    file_type: FileType,
    base_dir: Option<&Path>,
    scope: Option<&Path>,
) -> PdfDocumentModel {
    match file_type {
        FileType::Code => build_code_model(content),
        FileType::PlainText | FileType::Pdf => build_plaintext_model(content),
        FileType::Markdown => build_markdown_model(content, base_dir, scope),
    }
}

fn build_code_model(content: &str) -> PdfDocumentModel {
    let lines: Vec<String> = content
        .split('\n')
        .map(|line| {
            line.strip_suffix('\r')
                .unwrap_or(line)
                .replace('\t', "    ")
        })
        .collect();
    PdfDocumentModel {
        blocks: vec![PdfBlock::PlainCodeLines(lines)],
    }
}

fn build_plaintext_model(content: &str) -> PdfDocumentModel {
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    let mut blocks = Vec::new();
    let mut current: Vec<InlineSpan> = Vec::new();

    for line in normalized.split('\n') {
        if line.trim().is_empty() {
            if !current.is_empty() {
                blocks.push(PdfBlock::Paragraph(std::mem::take(&mut current)));
            }
        } else {
            if !current.is_empty() {
                current.push(InlineSpan::text("\n", InlineStyle::default()));
            }
            current.push(InlineSpan::text(
                line.replace('\t', "    "),
                InlineStyle::default(),
            ));
        }
    }
    if !current.is_empty() {
        blocks.push(PdfBlock::Paragraph(current));
    }
    PdfDocumentModel { blocks }
}

/// GitHub-style heading slug for internal `#heading` anchor links.
pub(crate) fn heading_slug(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.trim().chars() {
        if ch.is_alphanumeric() || ch == '_' || ch == '-' {
            for lc in ch.to_lowercase() {
                out.push(lc);
            }
        } else if ch.is_whitespace() {
            out.push('-');
        }
    }
    out
}

fn classify_link(dest_url: &str) -> Option<LinkTarget> {
    let trimmed = dest_url.trim();
    if let Some(fragment) = trimmed.strip_prefix('#') {
        let slug = heading_slug(fragment);
        return (!slug.is_empty()).then_some(LinkTarget::InternalHeading(slug));
    }
    openable_link(trimmed).map(|u| LinkTarget::Uri(u.to_string()))
}

enum Container {
    Root(Vec<PdfBlock>),
    BlockQuote(Vec<PdfBlock>),
    List {
        ordered_start: Option<u64>,
        items: Vec<ListItem>,
    },
    ListItem {
        task: Option<bool>,
        blocks: Vec<PdfBlock>,
    },
    Footnote {
        label: String,
        blocks: Vec<PdfBlock>,
    },
}

impl Container {
    fn push_block(&mut self, block: PdfBlock) {
        match self {
            Container::Root(b)
            | Container::BlockQuote(b)
            | Container::ListItem { blocks: b, .. }
            | Container::Footnote { blocks: b, .. } => b.push(block),
            Container::List { items, .. } => {
                if let Some(last) = items.last_mut() {
                    last.blocks.push(block);
                }
            }
        }
    }
}

struct ActiveImage {
    dest_url: String,
    size: Option<(u32, Option<u32>)>,
    alt: String,
}

struct ActiveTable {
    alignments: Vec<TableAlign>,
    in_head: bool,
    header: Vec<Vec<InlineSpan>>,
    rows: Vec<Vec<Vec<InlineSpan>>>,
    current_row: Vec<Vec<InlineSpan>>,
    in_cell: bool,
}

fn build_markdown_model(
    markdown: &str,
    base_dir: Option<&Path>,
    scope: Option<&Path>,
) -> PdfDocumentModel {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let raw_events = Parser::new_ext(markdown, options).into_offset_iter();
    let events = apply_highlights(markdown, raw_events, true);

    let mut containers: Vec<Container> = vec![Container::Root(Vec::new())];
    let mut footnotes: Vec<FootnoteEntry> = Vec::new();
    let mut slug_counts: HashMap<String, usize> = HashMap::new();

    let mut style = InlineStyle::default();
    let mut inline_buf: Vec<InlineSpan> = Vec::new();
    let mut in_paragraph = false;
    let mut active_heading: Option<u8> = None;
    let mut active_code: Option<(Option<String>, String)> = None;
    let mut active_table: Option<ActiveTable> = None;
    let mut image_stack: Vec<ActiveImage> = Vec::new();
    let mut standalone_images_in_para: Vec<(PdfImage, InlineStyle)> = Vec::new();
    let mut para_has_non_image_content = false;
    let mut image_budget = MAX_EMBED_TOTAL;

    let push_inline_image = |buf: &mut Vec<InlineSpan>,
                             built: PdfImage,
                             base_style: &InlineStyle| match &built
        .source
    {
        PdfImageSource::Embedded { .. } => {
            buf.push(InlineSpan {
                text: built.alt.clone(),
                style: base_style.clone(),
                image: Some(Box::new(built)),
            });
        }
        PdfImageSource::Placeholder { remote_uri } => {
            let label = if built.alt.is_empty() {
                "[Image]".to_string()
            } else {
                format!("[Image: {}]", built.alt)
            };
            let mut img_style = base_style.clone();
            img_style.italic = true;
            if img_style.link.is_none() {
                if let Some(uri) = remote_uri {
                    img_style.link = Some(LinkTarget::Uri(uri.clone()));
                }
            }
            push_inline(buf, &label, &img_style);
        }
    };

    let demote_standalone_images = |standalone: &mut Vec<(PdfImage, InlineStyle)>,
                                    buf: &mut Vec<InlineSpan>| {
        for (img, img_style) in standalone.drain(..) {
            push_inline_image(buf, img, &img_style);
        }
    };

    let flush_loose_inline = |containers: &mut [Container], buf: &mut Vec<InlineSpan>| {
        if !buf.is_empty() {
            let spans = std::mem::take(buf);
            if let Some(top) = containers.last_mut() {
                top.push_block(PdfBlock::Paragraph(spans));
            }
        }
    };

    for (event, range) in events {
        // Inside an image tag: gather alt text only.
        if !image_stack.is_empty() {
            match event {
                Event::Start(Tag::Image { dest_url, .. }) => {
                    image_stack.push(ActiveImage {
                        dest_url: dest_url.into_string(),
                        size: written_size(markdown, &range),
                        alt: String::new(),
                    });
                }
                Event::End(TagEnd::Image) => {
                    if let Some(img) = image_stack.pop() {
                        let suffix = img
                            .size
                            .map_or(String::new(), |(w, h)| size_suffix(Some(w), h));
                        let clean_alt = img
                            .alt
                            .strip_suffix(suffix.as_str())
                            .unwrap_or(&img.alt)
                            .to_string();
                        if let Some(outer) = image_stack.last_mut() {
                            outer.alt.push_str(&clean_alt);
                        } else {
                            let built = resolve_pdf_image(
                                &img.dest_url,
                                clean_alt,
                                img.size,
                                base_dir,
                                scope,
                                &mut image_budget,
                            );
                            if in_paragraph
                                && !para_has_non_image_content
                                && inline_buf.is_empty()
                                && style.link.is_none()
                            {
                                standalone_images_in_para.push((built, style.clone()));
                            } else {
                                demote_standalone_images(
                                    &mut standalone_images_in_para,
                                    &mut inline_buf,
                                );
                                push_inline_image(&mut inline_buf, built, &style);
                                para_has_non_image_content = true;
                            }
                        }
                    }
                }
                Event::Text(t) | Event::Code(t) => {
                    if let Some(cur) = image_stack.last_mut() {
                        cur.alt.push_str(&t);
                    }
                }
                Event::SoftBreak | Event::HardBreak => {
                    if let Some(cur) = image_stack.last_mut() {
                        cur.alt.push(' ');
                    }
                }
                _ => {}
            }
            continue;
        }

        // Inside a fenced/indented code block: gather raw text.
        if let Some((_, ref mut code)) = active_code {
            match event {
                Event::Text(t) => code.push_str(&t),
                Event::End(TagEnd::CodeBlock) => {
                    if let Some((lang, mut code)) = active_code.take() {
                        if code.ends_with('\n') {
                            code.pop();
                        }
                        if let Some(top) = containers.last_mut() {
                            top.push_block(PdfBlock::CodeBlock { lang, code });
                        }
                    }
                }
                _ => {}
            }
            continue;
        }

        match event {
            Event::Start(Tag::Paragraph) => {
                flush_loose_inline(&mut containers, &mut inline_buf);
                in_paragraph = true;
                para_has_non_image_content = false;
                standalone_images_in_para.clear();
            }
            Event::End(TagEnd::Paragraph) => {
                in_paragraph = false;
                if !para_has_non_image_content && !standalone_images_in_para.is_empty() {
                    for (img, _) in standalone_images_in_para.drain(..) {
                        if let Some(top) = containers.last_mut() {
                            top.push_block(PdfBlock::Image(img));
                        }
                    }
                } else {
                    demote_standalone_images(&mut standalone_images_in_para, &mut inline_buf);
                    if !inline_buf.is_empty() {
                        let spans = std::mem::take(&mut inline_buf);
                        if let Some(top) = containers.last_mut() {
                            top.push_block(PdfBlock::Paragraph(spans));
                        }
                    }
                }
                para_has_non_image_content = false;
            }

            Event::Start(Tag::Heading { level, .. }) => {
                flush_loose_inline(&mut containers, &mut inline_buf);
                active_heading = Some(level as u8);
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some(level) = active_heading.take() {
                    let spans = std::mem::take(&mut inline_buf);
                    let plain_text: String = spans.iter().map(|s| s.text.as_str()).collect();
                    let base_slug = heading_slug(&plain_text);
                    let slug = unique_slug(base_slug, &mut slug_counts);
                    if let Some(top) = containers.last_mut() {
                        top.push_block(PdfBlock::Heading {
                            level,
                            spans,
                            plain_text,
                            slug,
                        });
                    }
                }
            }

            Event::Start(Tag::CodeBlock(kind)) => {
                flush_loose_inline(&mut containers, &mut inline_buf);
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => {
                        let first = info.split_whitespace().next().unwrap_or("").trim();
                        (!first.is_empty()).then(|| first.to_string())
                    }
                    CodeBlockKind::Indented => None,
                };
                active_code = Some((lang, String::new()));
            }

            Event::Start(Tag::BlockQuote(_)) => {
                flush_loose_inline(&mut containers, &mut inline_buf);
                containers.push(Container::BlockQuote(Vec::new()));
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                flush_loose_inline(&mut containers, &mut inline_buf);
                if let Some(Container::BlockQuote(blocks)) = containers.pop() {
                    if let Some(top) = containers.last_mut() {
                        top.push_block(PdfBlock::BlockQuote(blocks));
                    }
                }
            }

            Event::Start(Tag::List(start)) => {
                flush_loose_inline(&mut containers, &mut inline_buf);
                containers.push(Container::List {
                    ordered_start: start,
                    items: Vec::new(),
                });
            }
            Event::End(TagEnd::List(_)) => {
                flush_loose_inline(&mut containers, &mut inline_buf);
                if let Some(Container::List {
                    ordered_start,
                    items,
                }) = containers.pop()
                {
                    if let Some(top) = containers.last_mut() {
                        top.push_block(PdfBlock::List {
                            ordered_start,
                            items,
                        });
                    }
                }
            }

            Event::Start(Tag::Item) => {
                flush_loose_inline(&mut containers, &mut inline_buf);
                containers.push(Container::ListItem {
                    task: None,
                    blocks: Vec::new(),
                });
            }
            Event::End(TagEnd::Item) => {
                flush_loose_inline(&mut containers, &mut inline_buf);
                if let Some(Container::ListItem { task, blocks }) = containers.pop() {
                    if let Some(Container::List { items, .. }) = containers.last_mut() {
                        items.push(ListItem { task, blocks });
                    }
                }
            }

            Event::TaskListMarker(checked) => {
                if let Some(Container::ListItem { task, .. }) = containers.last_mut() {
                    *task = Some(checked);
                }
            }

            Event::Start(Tag::Table(aligns)) => {
                flush_loose_inline(&mut containers, &mut inline_buf);
                active_table = Some(ActiveTable {
                    alignments: aligns.into_iter().map(TableAlign::from).collect(),
                    in_head: false,
                    header: Vec::new(),
                    rows: Vec::new(),
                    current_row: Vec::new(),
                    in_cell: false,
                });
            }
            Event::Start(Tag::TableHead) => {
                if let Some(t) = active_table.as_mut() {
                    t.in_head = true;
                    t.current_row.clear();
                }
            }
            Event::End(TagEnd::TableHead) => {
                if let Some(t) = active_table.as_mut() {
                    t.header = std::mem::take(&mut t.current_row);
                    t.in_head = false;
                }
            }
            Event::Start(Tag::TableRow) => {
                if let Some(t) = active_table.as_mut() {
                    t.current_row.clear();
                }
            }
            Event::End(TagEnd::TableRow) => {
                if let Some(t) = active_table.as_mut() {
                    let row = std::mem::take(&mut t.current_row);
                    if !row.is_empty() {
                        t.rows.push(row);
                    }
                }
            }
            Event::Start(Tag::TableCell) => {
                inline_buf.clear();
                if let Some(t) = active_table.as_mut() {
                    t.in_cell = true;
                }
            }
            Event::End(TagEnd::TableCell) => {
                let cell = std::mem::take(&mut inline_buf);
                if let Some(t) = active_table.as_mut() {
                    t.current_row.push(cell);
                    t.in_cell = false;
                }
            }
            Event::End(TagEnd::Table) => {
                if let Some(t) = active_table.take() {
                    if let Some(top) = containers.last_mut() {
                        top.push_block(PdfBlock::Table {
                            alignments: t.alignments,
                            header: t.header,
                            rows: t.rows,
                        });
                    }
                }
            }

            Event::Start(Tag::FootnoteDefinition(label)) => {
                flush_loose_inline(&mut containers, &mut inline_buf);
                containers.push(Container::Footnote {
                    label: label.into_string(),
                    blocks: Vec::new(),
                });
            }
            Event::End(TagEnd::FootnoteDefinition) => {
                flush_loose_inline(&mut containers, &mut inline_buf);
                if let Some(Container::Footnote { label, blocks }) = containers.pop() {
                    footnotes.push(FootnoteEntry { label, blocks });
                }
            }

            Event::FootnoteReference(label) => {
                let mut ref_style = style.clone();
                ref_style.bold = true;
                push_inline(&mut inline_buf, &format!("[{label}]"), &ref_style);
                para_has_non_image_content = true;
            }

            Event::Start(Tag::Image { dest_url, .. }) => {
                image_stack.push(ActiveImage {
                    dest_url: dest_url.into_string(),
                    size: written_size(markdown, &range),
                    alt: String::new(),
                });
            }

            Event::Rule => {
                flush_loose_inline(&mut containers, &mut inline_buf);
                if let Some(top) = containers.last_mut() {
                    top.push_block(PdfBlock::HorizontalRule);
                }
            }

            // Inline style tags
            Event::Start(Tag::Strong) => style.bold = true,
            Event::End(TagEnd::Strong) => style.bold = false,
            Event::Start(Tag::Emphasis) => style.italic = true,
            Event::End(TagEnd::Emphasis) => style.italic = false,
            Event::Start(Tag::Strikethrough) => style.strikethrough = true,
            Event::End(TagEnd::Strikethrough) => style.strikethrough = false,
            Event::Start(Tag::Link { dest_url, .. }) => {
                style.link = classify_link(&dest_url);
            }
            Event::End(TagEnd::Link) => {
                style.link = None;
            }

            // Inline HTML emitted by LiteMark (`<u>`, `<mark>`, `<br>`).
            Event::Html(tag) | Event::InlineHtml(tag) => {
                let trimmed = tag.trim().to_ascii_lowercase();
                match trimmed.as_str() {
                    "<mark>" => style.highlight = true,
                    "</mark>" => style.highlight = false,
                    "<u>" => style.underline = true,
                    "</u>" => style.underline = false,
                    "<br>" | "<br/>" | "<br />" => {
                        push_inline(&mut inline_buf, "\n", &style);
                        para_has_non_image_content = true;
                    }
                    _ => {}
                }
            }

            Event::Text(text) => {
                if !text.is_empty() {
                    if !text.trim().is_empty() {
                        if !standalone_images_in_para.is_empty() {
                            demote_standalone_images(
                                &mut standalone_images_in_para,
                                &mut inline_buf,
                            );
                        }
                        para_has_non_image_content = true;
                        push_inline(&mut inline_buf, &text, &style);
                    } else if standalone_images_in_para.is_empty() {
                        push_inline(&mut inline_buf, &text, &style);
                    }
                }
            }
            Event::Code(code) => {
                if !standalone_images_in_para.is_empty() {
                    demote_standalone_images(&mut standalone_images_in_para, &mut inline_buf);
                }
                let mut code_style = style.clone();
                code_style.code = true;
                push_inline(&mut inline_buf, &code, &code_style);
                para_has_non_image_content = true;
            }
            Event::SoftBreak => {
                push_inline(&mut inline_buf, " ", &style);
            }
            Event::HardBreak => {
                push_inline(&mut inline_buf, "\n", &style);
                para_has_non_image_content = true;
            }
            _ => {}
        }
    }

    flush_loose_inline(&mut containers, &mut inline_buf);

    let mut blocks = match containers.into_iter().next() {
        Some(Container::Root(b)) => b,
        _ => Vec::new(),
    };
    if !footnotes.is_empty() {
        blocks.push(PdfBlock::FootnoteSection(footnotes));
    }
    PdfDocumentModel { blocks }
}

fn push_inline(buf: &mut Vec<InlineSpan>, text: &str, style: &InlineStyle) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = buf.last_mut() {
        if last.image.is_none() && last.style == *style {
            last.text.push_str(text);
            return;
        }
    }
    buf.push(InlineSpan::text(text, style.clone()));
}

fn unique_slug(base: String, counts: &mut HashMap<String, usize>) -> String {
    if base.is_empty() {
        return base;
    }
    let entry = counts.entry(base.clone()).or_insert(0);
    let idx = *entry;
    *entry += 1;
    if idx == 0 {
        base
    } else {
        format!("{base}-{idx}")
    }
}

fn resolve_pdf_image(
    dest_url: &str,
    alt: String,
    size: Option<(u32, Option<u32>)>,
    base_dir: Option<&Path>,
    scope: Option<&Path>,
    budget: &mut u64,
) -> PdfImage {
    let (requested_width, requested_height) = match size {
        Some((w, h)) => (Some(w), h),
        None => (None, None),
    };

    let target = resolve_image_target(dest_url, base_dir);
    let source = match target {
        ImageTarget::Local(path) => {
            if let Some(base) = base_dir {
                let effective_scope = scope.unwrap_or(base);
                load_local_krilla_image(&path, effective_scope, budget)
                    .map(
                        |(image, pixel_width, pixel_height)| PdfImageSource::Embedded {
                            image,
                            pixel_width,
                            pixel_height,
                        },
                    )
                    .unwrap_or(PdfImageSource::Placeholder { remote_uri: None })
            } else {
                PdfImageSource::Placeholder { remote_uri: None }
            }
        }
        ImageTarget::Remote(url) => PdfImageSource::Placeholder {
            remote_uri: openable_link(&url).map(str::to_string),
        },
        ImageTarget::Unavailable => PdfImageSource::Placeholder { remote_uri: None },
    };

    PdfImage {
        alt,
        requested_width,
        requested_height,
        source,
    }
}

fn load_local_krilla_image(
    path: &Path,
    scope: &Path,
    budget: &mut u64,
) -> Option<(KrillaImage, u32, u32)> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    if !matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp"
    ) {
        return None;
    }
    let canon_path = path.canonicalize().ok()?;
    let canon_scope = scope.canonicalize().ok()?;
    if !canon_path.starts_with(&canon_scope) {
        return None;
    }
    let meta = std::fs::metadata(&canon_path).ok()?;
    if !meta.is_file() || meta.len() > MAX_EMBED_BYTES || meta.len() > *budget {
        return None;
    }
    let bytes = std::fs::read(&canon_path).ok()?;
    if bytes.len() as u64 > *budget {
        return None;
    }

    let (krilla_img, w, h) = decode_to_krilla_image(&ext, bytes)?;
    *budget = budget.saturating_sub(meta.len());
    Some((krilla_img, w, h))
}

/// Read only the image header to verify width, height, and total pixel count
/// before allocating any decoded pixel buffer (guards against decompression bombs).
fn image_dimensions_within_limits(bytes: &[u8]) -> Option<(u32, u32)> {
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let (w, h) = reader.into_dimensions().ok()?;
    if w == 0
        || h == 0
        || w > MAX_EMBED_IMAGE_DIM
        || h > MAX_EMBED_IMAGE_DIM
        || u64::from(w) * u64::from(h) > MAX_EMBED_IMAGE_PIXELS
    {
        return None;
    }
    Some((w, h))
}

fn decode_to_krilla_image(ext: &str, bytes: Vec<u8>) -> Option<(KrillaImage, u32, u32)> {
    // Reject oversized dimensions from headers before any decoder allocates pixels.
    let (header_w, header_h) = image_dimensions_within_limits(&bytes)?;

    let direct = match ext {
        "png" => KrillaImage::from_png(bytes.clone().into(), true).ok(),
        "jpg" | "jpeg" => KrillaImage::from_jpeg(bytes.clone().into(), true).ok(),
        "gif" => KrillaImage::from_gif(bytes.clone().into(), true).ok(),
        "webp" => KrillaImage::from_webp(bytes.clone().into(), true).ok(),
        _ => None,
    };
    if let Some(img) = direct {
        let (w, h) = img.size();
        if w > 0
            && h > 0
            && w <= MAX_EMBED_IMAGE_DIM
            && h <= MAX_EMBED_IMAGE_DIM
            && u64::from(w) * u64::from(h) <= MAX_EMBED_IMAGE_PIXELS
        {
            return Some((img, w, h));
        }
    }

    // Fallback (covers BMP and any exotic PNG/JPEG/WebP subformat): decode with
    // explicit dimension & allocation caps and re-encode as PNG for krilla.
    let mut reader = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_EMBED_IMAGE_DIM);
    limits.max_image_height = Some(MAX_EMBED_IMAGE_DIM);
    limits.max_alloc = Some(MAX_DECODE_ALLOC_BYTES);
    reader.limits(limits);
    let decoded = reader.decode().ok()?;
    let (w, h) = (decoded.width(), decoded.height());
    if w == 0 || h == 0 || w != header_w || h != header_h {
        return None;
    }
    let mut png_bytes = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut png_bytes);
    decoded
        .write_to(&mut cursor, image::ImageFormat::Png)
        .ok()?;
    let img = KrillaImage::from_png(png_bytes.into(), true).ok()?;
    Some((img, w, h))
}
