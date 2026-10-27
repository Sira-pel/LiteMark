use gpui::ClipboardItem;

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
