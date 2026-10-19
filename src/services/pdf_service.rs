use std::path::Path;

pub fn extract_text_from_pdf(path: &Path) -> Result<(String, usize), String> {
    let doc = lopdf::Document::load(path)
        .map_err(|e| format!("Failed to read PDF document: {}", e))?;

    let pages = doc.get_pages();
    let total_pages = pages.len();

    if total_pages == 0 {
        return Err("PDF document contains no pages".to_string());
    }

    let mut page_sections = Vec::new();
    let mut page_nums: Vec<u32> = pages.keys().copied().collect();
    page_nums.sort();

    for page_num in page_nums {
        let text = doc.extract_text(&[page_num]).unwrap_or_default();
        let trimmed = text.trim();
        if trimmed.is_empty() {
            page_sections.push(format!("## Page {}\n\n*(No text layer detected on this page)*", page_num));
        } else {
            page_sections.push(format!("## Page {}\n\n{}", page_num, trimmed));
        }
    }

    let markdown = page_sections.join("\n\n---\n\n");
    Ok((markdown, total_pages))
}
