use gpui::{ClipboardEntry, ClipboardItem, Image, ImageFormat};

#[derive(serde::Serialize, serde::Deserialize)]
struct CopiedMarkdown {
    visible: String,
    markdown: String,
}

/// The source rides along with the visible text, tagged with that text so a
/// paste can tell it still belongs to the clipboard's current contents.
pub fn clipboard_item(visible: String, markdown: String) -> ClipboardItem {
    if visible == markdown {
        return ClipboardItem::new_string(markdown);
    }
    let meta = serde_json::to_string(&CopiedMarkdown {
        visible: visible.clone(),
        markdown,
    })
    .unwrap_or_default();
    ClipboardItem::new_string_with_metadata(visible, meta)
}

/// Markdown tabs get the copied source when its visible text still matches
/// the clipboard text; anything else pastes the clipboard text.
pub fn paste_text(item: &ClipboardItem, into_markdown: bool) -> Option<String> {
    let text = item.text();
    if into_markdown {
        let copied = item
            .metadata()
            .and_then(|meta| serde_json::from_str::<CopiedMarkdown>(meta).ok());
        if let (Some(copied), Some(text)) = (copied, text.as_deref()) {
            if copied.visible == text {
                return Some(copied.markdown);
            }
        }
    }
    text
}

/// The image on the clipboard, when the clipboard holds one and no text.
/// Programs that copy a picture often add text too (a file name, an HTML
/// fragment); that case pastes as text, as it always did.
pub fn clipboard_image(item: &ClipboardItem) -> Option<&Image> {
    if item.text().is_some() {
        return None;
    }
    item.entries().iter().find_map(|entry| match entry {
        ClipboardEntry::Image(image) => Some(image),
        _ => None,
    })
}

/// PNG bytes for a clipboard image: PNG as it is, JPEG, WebP, GIF and BMP
/// re-encoded. Other formats (SVG, TIFF) are not supported.
pub fn clipboard_image_png(image: &Image) -> Result<Vec<u8>, String> {
    if image.format == ImageFormat::Png {
        return Ok(image.bytes.clone());
    }
    if matches!(image.format, ImageFormat::Svg | ImageFormat::Tiff) {
        return Err("This kind of image can't be pasted.".into());
    }
    let decoded = ::image::load_from_memory(&image.bytes).map_err(|err| err.to_string())?;
    let mut out = std::io::Cursor::new(Vec::new());
    decoded
        .write_to(&mut out, ::image::ImageFormat::Png)
        .map_err(|err| err.to_string())?;
    Ok(out.into_inner())
}

/// The clipboard text as a link destination: one `http(s)://` URL with
/// nothing else around it. Characters that would end or break a Markdown
/// link destination are percent-encoded.
pub fn clipboard_http_url(text: &str) -> Option<String> {
    let url = text.trim();
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .or_else(|| url.strip_prefix("HTTPS://"))
        .or_else(|| url.strip_prefix("HTTP://"))?;
    if rest.is_empty() || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    let mut out = String::with_capacity(url.len());
    for c in url.chars() {
        match c {
            '(' => out.push_str("%28"),
            ')' => out.push_str("%29"),
            '<' => out.push_str("%3C"),
            '>' => out.push_str("%3E"),
            _ => out.push(c),
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::document::DocumentTab;
    use gpui::ClipboardItem;

    fn copy_visible(source: &str) -> ClipboardItem {
        let mut doc = DocumentTab::new("t".into(), "Doc.md".into(), source.into(), None, None);
        doc.caret.move_to(0);
        doc.caret.select_to(source.len());
        clipboard_item(doc.selected_visible_text(), doc.selected_text())
    }

    #[test]
    fn wysiwyg_copy_pastes_back_as_exact_markdown() {
        let sources = [
            "# Title\n\n**bold** and *it* `code`\n\n- [x] done\n- [link](https://x.y)\n- ![img](a.png)\n  - nested\n\n1. one\n2. two",
            "- 😀 **héllo** wörld\n- **日本語** と *中文*\n- [текст](https://x.y) 🎉",
        ];
        for source in sources {
            let item = copy_visible(source);
            assert_ne!(item.text().as_deref(), Some(source));
            assert_eq!(paste_text(&item, true).as_deref(), Some(source));
            // Non-Markdown targets get the visible text.
            assert_eq!(paste_text(&item, false), item.text());
        }
    }

    fn png_bytes() -> Vec<u8> {
        let pixels = ::image::RgbaImage::from_pixel(2, 3, ::image::Rgba([200, 30, 30, 255]));
        let mut out = std::io::Cursor::new(Vec::new());
        pixels
            .write_to(&mut out, ::image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    #[test]
    fn an_image_only_clipboard_is_an_image_paste() {
        let png = Image::from_bytes(ImageFormat::Png, png_bytes());
        let item = ClipboardItem::new_image(&png);
        assert_eq!(clipboard_image(&item), Some(&png));
        assert_eq!(paste_text(&item, true), None);
    }

    #[test]
    fn a_text_clipboard_is_not_an_image_paste() {
        let item = ClipboardItem::new_string("photo.png".into());
        assert_eq!(clipboard_image(&item), None);
        assert_eq!(paste_text(&item, true).as_deref(), Some("photo.png"));
    }

    #[test]
    fn clipboard_images_become_png() {
        let png = png_bytes();
        let as_png = Image::from_bytes(ImageFormat::Png, png.clone());
        assert_eq!(clipboard_image_png(&as_png).unwrap(), png);

        let mut bmp = std::io::Cursor::new(Vec::new());
        ::image::RgbaImage::from_pixel(2, 3, ::image::Rgba([1, 2, 3, 255]))
            .write_to(&mut bmp, ::image::ImageFormat::Bmp)
            .unwrap();
        let converted =
            clipboard_image_png(&Image::from_bytes(ImageFormat::Bmp, bmp.into_inner())).unwrap();
        assert_eq!(&converted[1..4], b"PNG");

        let svg = Image::from_bytes(ImageFormat::Svg, b"<svg/>".to_vec());
        assert!(clipboard_image_png(&svg).is_err());
        let junk = Image::from_bytes(ImageFormat::Jpeg, b"not a jpeg".to_vec());
        assert!(clipboard_image_png(&junk).is_err());
    }

    #[test]
    fn stale_metadata_is_ignored() {
        let item = copy_visible("**bold**");
        let stale = ClipboardItem::new_string_with_metadata(
            "something else".into(),
            item.metadata().cloned().unwrap(),
        );
        assert_eq!(paste_text(&stale, true).as_deref(), Some("something else"));
    }
}
