//! Pure-Rust native PDF export for Markdown, plain text, and code tabs.
//!
//! Pipeline:
//! 1. [`model::build_document_model`] parses the document into blocks/inlines
//!    (applying `==highlight==`, `<u>`, `![alt|WxH]`, `export_scope` local image
//!    checks, and link validation).
//! 2. [`layout::layout_document`] shapes text via [`fonts::FontBook`] (bundled
//!    Ubuntu / Ubuntu Mono + lazy OS fallback with `rustybuzz`), breaks lines,
//!    and paginates blocks (preventing orphan headings and repeating table
//!    headers across page breaks).
//! 3. [`render::render_pdf_bytes`] emits compressed PDF bytes via `krilla` with
//!    subsetted Unicode fonts, clickable external/internal links, document
//!    outline (bookmarks), metadata, and optional page numbers.

mod fonts;
mod layout;
mod model;
mod render;

use crate::model::{FileType, PdfExportSettings};
use std::path::Path;

/// Outcome of a PDF export.
pub struct PdfExportOutput {
    pub bytes: Vec<u8>,
    #[cfg(test)]
    pub page_count: usize,
    pub truncated: bool,
}

/// Export `content` to a self-contained PDF byte buffer.
///
/// Offline by default: local images are embedded only when inside `scope`
/// (or `base_dir`), and remote image URLs are rendered as clickable
/// placeholders without making network requests.
pub fn export_to_pdf(
    title: &str,
    content: &str,
    file_type: FileType,
    settings: PdfExportSettings,
    base_dir: Option<&Path>,
    scope: Option<&Path>,
) -> Result<PdfExportOutput, String> {
    let mut fonts = fonts::FontBook::new()?;
    let doc_model = model::build_document_model(content, file_type, base_dir, scope);
    let laid_out = layout::layout_document(&doc_model, settings, &mut fonts);
    #[cfg(test)]
    let page_count = laid_out.pages.len();
    let truncated = laid_out.truncated;
    let bytes = render::render_pdf_bytes(title, &laid_out, settings, &mut fonts)?;
    Ok(PdfExportOutput {
        bytes,
        #[cfg(test)]
        page_count,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PdfFontSize, PdfMargins, PdfOrientation, PdfPageSize, PdfTheme};
    use lopdf::Document as LopdfDoc;

    fn load_lopdf(bytes: &[u8]) -> LopdfDoc {
        assert!(bytes.starts_with(b"%PDF-"), "missing PDF header");
        LopdfDoc::load_mem(bytes).expect("generated PDF should parse with lopdf")
    }

    #[test]
    fn exports_rich_markdown_with_outline_links_and_metadata() {
        let md = "\
# Main Title

Hello **bold**, *italic*, <u>underline</u>, ~~strike~~, ==highlight==, and `inline_code`.

Check [Example](https://example.com) and jump to [Details](#details).

## Details

- Bullet one
- [x] Done task
- [ ] Open task

> Quoted thought with **emphasis**.

```rust
fn main() {
    println!(\"hello\");
}
```

| Name | Value |
| :--- | ---: |
| Alpha | 42 |
| Beta | 99 |

---

Footnote ref[^1].

[^1]: Footnote body text.
";
        let out = export_to_pdf(
            "Sample Note",
            md,
            FileType::Markdown,
            PdfExportSettings::default(),
            None,
            None,
        )
        .expect("PDF export should succeed");
        assert!(out.page_count >= 1);
        assert!(!out.truncated);

        let doc = load_lopdf(&out.bytes);
        assert_eq!(doc.get_pages().len(), out.page_count);

        // Verify text extraction works (ToUnicode CMap present).
        let extracted = doc.extract_text(&[1]).expect("extract text from page 1");
        assert!(extracted.contains("Main Title"), "{extracted}");
        assert!(extracted.contains("highlight"), "{extracted}");
        assert!(extracted.contains("inline_code"), "{extracted}");
        assert!(extracted.contains("Alpha"), "{extracted}");
        assert!(
            extracted.contains("1 / 1"),
            "page number missing: {extracted}"
        );

        // Verify document outline (bookmarks) exists in catalog.
        let catalog = doc.catalog().expect("PDF catalog");
        assert!(
            catalog.get(b"Outlines").is_ok(),
            "PDF catalog should include Outlines for headings"
        );

        // Verify page 1 has link annotations (external URI + internal #details).
        let page1_id = *doc.get_pages().get(&1).unwrap();
        let page1_dict = doc.get_dictionary(page1_id).unwrap();
        let annots = page1_dict
            .get(b"Annots")
            .expect("page 1 should have link annotations")
            .as_array()
            .unwrap();
        assert!(
            annots.len() >= 2,
            "expected >=2 link annotations, got {}",
            annots.len()
        );
    }

    #[test]
    fn respects_page_size_orientation_and_page_numbers_toggle() {
        let settings = PdfExportSettings {
            page_size: PdfPageSize::Letter,
            orientation: PdfOrientation::Landscape,
            margins: PdfMargins::Wide,
            theme: PdfTheme::Dark,
            font_size: PdfFontSize::Large,
            page_numbers: false,
            open_after_export: false,
        };
        let out = export_to_pdf(
            "Dark Landscape",
            "# Hello",
            FileType::Markdown,
            settings,
            None,
            None,
        )
        .unwrap();
        let doc = load_lopdf(&out.bytes);
        let page1_id = *doc.get_pages().get(&1).unwrap();
        let media_box = doc
            .get_dictionary(page1_id)
            .unwrap()
            .get(b"MediaBox")
            .unwrap()
            .as_array()
            .unwrap();
        let w = media_box[2].as_float().unwrap();
        let h = media_box[3].as_float().unwrap();
        // Letter landscape: 792 x 612 pt.
        assert!((w - 792.0).abs() < 1.0, "expected ~792, got {w}");
        assert!((h - 612.0).abs() < 1.0, "expected ~612, got {h}");

        let text = doc.extract_text(&[1]).unwrap();
        assert!(
            !text.contains("1 / 1"),
            "page numbers should be off: {text}"
        );
    }

    #[test]
    fn repeats_table_header_across_page_breaks() {
        let mut md = String::from("| ColA | ColB |\n| --- | --- |\n");
        for i in 0..80 {
            md.push_str(&format!("| Row{i} | Value{i} |\n"));
        }
        let out = export_to_pdf(
            "Long Table",
            &md,
            FileType::Markdown,
            PdfExportSettings::default(),
            None,
            None,
        )
        .unwrap();
        assert!(out.page_count >= 2, "table should span >=2 pages");

        let doc = load_lopdf(&out.bytes);
        let p1 = doc.extract_text(&[1]).unwrap();
        let p2 = doc.extract_text(&[2]).unwrap();
        assert!(
            p1.contains("ColA") && p1.contains("ColB"),
            "page 1 header: {p1}"
        );
        assert!(
            p2.contains("ColA") && p2.contains("ColB"),
            "page 2 should repeat header: {p2}"
        );
    }

    #[test]
    fn embeds_in_scope_local_image_and_blocks_out_of_scope_image() {
        let root = std::env::temp_dir().join(format!("litemark-pdf-img-{}", std::process::id()));
        let note_dir = root.join("note");
        std::fs::create_dir_all(note_dir.join("assets")).unwrap();

        // Build a valid 2x2 PNG using the `image` crate.
        let mut png_bytes = Vec::new();
        let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]));
        img.write_to(
            &mut std::io::Cursor::new(&mut png_bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
        std::fs::write(note_dir.join("assets/ok.png"), &png_bytes).unwrap();
        std::fs::write(root.join("secret.png"), &png_bytes).unwrap();

        let md = "\
![in-scope|120](assets/ok.png)

![outside](../secret.png)

![remote pic](https://example.com/pic.png)
";
        let out = export_to_pdf(
            "Images",
            md,
            FileType::Markdown,
            PdfExportSettings::default(),
            Some(&note_dir),
            Some(&note_dir),
        )
        .unwrap();
        let doc = load_lopdf(&out.bytes);
        let text = doc.extract_text(&[1]).unwrap();
        // Out-of-scope and remote images render as placeholders with alt text;
        // in-scope image embeds directly (no placeholder text for `in-scope`).
        assert!(
            !text.contains("Image: in-scope"),
            "local image should embed: {text}"
        );
        assert!(
            text.contains("Image: outside"),
            "out-of-scope should be placeholder: {text}"
        );
        assert!(
            text.contains("Image: remote pic"),
            "remote should be placeholder: {text}"
        );

        // Remote placeholder should carry a clickable URI link annotation.
        let page1_id = *doc.get_pages().get(&1).unwrap();
        let annots = doc
            .get_dictionary(page1_id)
            .unwrap()
            .get(b"Annots")
            .unwrap()
            .as_array()
            .unwrap();
        assert!(
            !annots.is_empty(),
            "remote image placeholder should be clickable"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn code_and_plaintext_tabs_do_not_parse_markdown() {
        let code = "# not a heading\nfn **not_bold**() {}";
        let code_out = export_to_pdf(
            "main.rs",
            code,
            FileType::Code,
            PdfExportSettings::default(),
            None,
            None,
        )
        .unwrap();
        let code_doc = load_lopdf(&code_out.bytes);
        let code_text = code_doc.extract_text(&[1]).unwrap();
        assert!(code_text.contains("# not a heading"), "{code_text}");
        assert!(code_text.contains("**not_bold**"), "{code_text}");
        assert!(code_doc.catalog().unwrap().get(b"Outlines").is_err());

        let plain = "# literal hash\n**literal stars**";
        let plain_out = export_to_pdf(
            "notes.txt",
            plain,
            FileType::PlainText,
            PdfExportSettings::default(),
            None,
            None,
        )
        .unwrap();
        let plain_doc = load_lopdf(&plain_out.bytes);
        let plain_text = plain_doc.extract_text(&[1]).unwrap();
        assert!(plain_text.contains("# literal hash"), "{plain_text}");
        assert!(plain_text.contains("**literal stars**"), "{plain_text}");
    }

    #[test]
    fn plain_text_keeps_leading_indentation() {
        let plain = "top\n    indented four\n\tindented tab";
        let out = export_to_pdf(
            "notes.txt",
            plain,
            FileType::PlainText,
            PdfExportSettings::default(),
            None,
            None,
        )
        .unwrap();
        let doc = load_lopdf(&out.bytes);
        let text = doc.extract_text(&[1]).unwrap();
        assert!(text.contains("    indented four"), "{text:?}");
        assert!(text.contains("    indented tab"), "{text:?}");
    }

    #[test]
    fn handles_unicode_and_non_latin_scripts_without_crashing() {
        let md = "# สวัสดี ไทย — 日本語 — Tiếng Việt — العربية — Привет\n\nParagraph with emoji 😀 and math ≈ ∞.";
        let out = export_to_pdf(
            "Unicode",
            md,
            FileType::Markdown,
            PdfExportSettings {
                theme: PdfTheme::Sepia,
                ..Default::default()
            },
            None,
            None,
        )
        .unwrap();
        let doc = load_lopdf(&out.bytes);
        assert_eq!(doc.get_pages().len(), out.page_count);
    }

    #[test]
    fn blocks_unsafe_link_schemes_in_pdf_annotations() {
        let md = "[bad](javascript:alert(1)) [file](file:///etc/passwd) [ok](https://example.com)";
        let out = export_to_pdf(
            "Links",
            md,
            FileType::Markdown,
            PdfExportSettings::default(),
            None,
            None,
        )
        .unwrap();
        let doc = load_lopdf(&out.bytes);
        let page1_id = *doc.get_pages().get(&1).unwrap();
        let annots = doc
            .get_dictionary(page1_id)
            .unwrap()
            .get(b"Annots")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(
            annots.len(),
            1,
            "only https:// link should become an annotation"
        );
    }

    #[test]
    fn renders_inline_images_in_paragraphs_headings_and_tables() {
        let root =
            std::env::temp_dir().join(format!("litemark-pdf-inline-img-{}", std::process::id()));
        std::fs::create_dir_all(root.join("assets")).unwrap();

        let mut png_bytes = Vec::new();
        let img = image::RgbaImage::from_pixel(4, 4, image::Rgba([0, 128, 255, 255]));
        img.write_to(
            &mut std::io::Cursor::new(&mut png_bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
        std::fs::write(root.join("assets/icon.png"), &png_bytes).unwrap();

        let md = "\
# Heading with ![hdr-icon|14](assets/icon.png) badge

Before ![para-icon|16](assets/icon.png) after, and [![linked-icon|16](assets/icon.png)](https://example.com).

| Col | Pic |
| --- | --- |
| Row | ![cell-icon|20](assets/icon.png) |
";
        let out = export_to_pdf(
            "Inline Images",
            md,
            FileType::Markdown,
            PdfExportSettings::default(),
            Some(&root),
            Some(&root),
        )
        .unwrap();
        let doc = load_lopdf(&out.bytes);
        let text = doc.extract_text(&[1]).unwrap();
        assert!(
            !text.contains("[Image:"),
            "inline local images should render as pictures, not placeholder text: {text}"
        );
        assert!(text.contains("Before") && text.contains("after"), "{text}");

        // Page 1 resources must contain an XObject image entry.
        let page1_id = *doc.get_pages().get(&1).unwrap();
        let page1_dict = doc.get_dictionary(page1_id).unwrap();
        let resources = page1_dict
            .get(b"Resources")
            .and_then(|o| doc.dereference(o))
            .unwrap()
            .1
            .as_dict()
            .unwrap();
        let xobjects = resources
            .get(b"XObject")
            .and_then(|o| doc.dereference(o))
            .expect("page 1 should embed image XObject")
            .1
            .as_dict()
            .unwrap();
        assert!(!xobjects.is_empty(), "expected embedded image XObject");

        // Linked inline image should still produce a clickable annotation.
        let annots = page1_dict
            .get(b"Annots")
            .expect("linked inline image should create an annotation")
            .as_array()
            .unwrap();
        assert_eq!(annots.len(), 1);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pdf_tab_exports_as_plain_text_without_markdown_parsing() {
        let extracted = "## Page 1\n\n# Not a heading\n**not bold**";
        let out = export_to_pdf(
            "scanned.pdf",
            extracted,
            FileType::Pdf,
            PdfExportSettings::default(),
            None,
            None,
        )
        .unwrap();
        let doc = load_lopdf(&out.bytes);
        let text = doc.extract_text(&[1]).unwrap();
        assert!(text.contains("## Page 1"), "{text}");
        assert!(text.contains("# Not a heading"), "{text}");
        assert!(text.contains("**not bold**"), "{text}");
        assert!(
            doc.catalog().unwrap().get(b"Outlines").is_err(),
            "PDF tab should not parse # lines into outline headings"
        );
    }

    #[test]
    fn rejects_oversized_image_dimensions_before_decoding() {
        // Minimal PNG signature + IHDR claiming 10,000 x 10,000 pixels (~400 MB RGBA),
        // only 33 bytes on disk. Must be rejected from its header without decoding.
        let mut crafted = Vec::new();
        crafted.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]); // PNG signature
        crafted.extend_from_slice(&13u32.to_be_bytes()); // IHDR length
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(b"IHDR");
        ihdr.extend_from_slice(&(model::MAX_EMBED_IMAGE_DIM + 1).to_be_bytes()); // width
        ihdr.extend_from_slice(&(model::MAX_EMBED_IMAGE_DIM + 1).to_be_bytes()); // height
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA
        crafted.extend_from_slice(&ihdr);
        crafted.extend_from_slice(&0u32.to_be_bytes()); // dummy CRC (dimensions only)

        let root = std::env::temp_dir().join(format!("litemark-pdf-bomb-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("bomb.png"), &crafted).unwrap();

        let out = export_to_pdf(
            "Bomb",
            "![huge bomb](bomb.png)",
            FileType::Markdown,
            PdfExportSettings::default(),
            Some(&root),
            Some(&root),
        )
        .unwrap();
        let doc = load_lopdf(&out.bytes);
        let text = doc.extract_text(&[1]).unwrap();
        assert!(
            text.contains("Image: huge bomb"),
            "oversized image should fall back to placeholder: {text}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn very_long_note_stops_at_max_pages_and_marks_truncated() {
        // 70,000 lines inside a fenced code block exceed 2,000 landscape pages
        // while staying fast to lay out in debug tests.
        let mut md = String::with_capacity(70_016);
        md.push_str("```\nstart\n");
        for _ in 0..70_000 {
            md.push('\n');
        }
        md.push_str("end\n```");

        let settings = PdfExportSettings {
            page_size: PdfPageSize::Letter,
            orientation: PdfOrientation::Landscape,
            margins: PdfMargins::Wide,
            theme: PdfTheme::Light,
            font_size: PdfFontSize::Large,
            page_numbers: false,
            open_after_export: false,
        };
        let out = export_to_pdf("Very Long", &md, FileType::Markdown, settings, None, None)
            .expect("long export should stop cleanly at page cap");
        assert!(out.truncated, "export should be marked truncated");
        assert_eq!(out.page_count, layout::MAX_PDF_EXPORT_PAGES);

        let doc = load_lopdf(&out.bytes);
        assert_eq!(doc.get_pages().len(), layout::MAX_PDF_EXPORT_PAGES);
    }
}
