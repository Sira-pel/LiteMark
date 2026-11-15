//! Read-only text extraction from PDF files.
//!
//! PDFs come from anywhere, so extraction is bounded: the file size, the
//! number of pages, the decompressed size of each stream and page, the total
//! text kept and the time spent are all capped. A crafted file therefore
//! fails with an error or opens truncated instead of exhausting memory or
//! hanging. Callers run this off the UI thread.

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

/// Largest PDF file that is opened at all.
pub const MAX_PDF_FILE_BYTES: u64 = 100 * 1024 * 1024;
/// Most pages whose text is extracted; later pages are skipped with a note.
pub const MAX_PDF_PAGES: usize = 2_000;
/// Most bytes any single stream may decompress to while loading, and the
/// most decompressed content any single page may have.
pub const MAX_PDF_STREAM_BYTES: usize = 32 * 1024 * 1024;
/// Most Markdown kept from one PDF; extraction stops once it is reached.
pub const MAX_PDF_TEXT_BYTES: usize = 16 * 1024 * 1024;
/// Most time spent extracting text before stopping with a note.
pub const MAX_PDF_EXTRACT_TIME: Duration = Duration::from_secs(20);
/// Stack for the extraction thread. lopdf parses some structures
/// recursively; a large stack keeps a deeply nested file from overflowing.
const EXTRACT_STACK_BYTES: usize = 64 * 1024 * 1024;

/// Extract the text of the PDF at `path` as Markdown, one section per page.
/// Returns the Markdown and the document's total page count.
pub fn extract_text_from_pdf(path: &Path) -> Result<(String, usize), String> {
    let path = path.to_path_buf();
    std::thread::Builder::new()
        .name("pdf-extract".into())
        .stack_size(EXTRACT_STACK_BYTES)
        .spawn(move || extract_with_limits(&path, &Limits::DEFAULT))
        .map_err(|err| format!("Couldn't start reading the PDF: {err}"))?
        .join()
        .map_err(|_| "Reading the PDF failed unexpectedly.".to_string())?
}

#[derive(Clone, Copy, Debug)]
struct Limits {
    file_bytes: u64,
    pages: usize,
    stream_bytes: usize,
    text_bytes: usize,
    time: Duration,
}

impl Limits {
    const DEFAULT: Limits = Limits {
        file_bytes: MAX_PDF_FILE_BYTES,
        pages: MAX_PDF_PAGES,
        stream_bytes: MAX_PDF_STREAM_BYTES,
        text_bytes: MAX_PDF_TEXT_BYTES,
        time: MAX_PDF_EXTRACT_TIME,
    };
}

fn extract_with_limits(path: &Path, limits: &Limits) -> Result<(String, usize), String> {
    let meta = fs::metadata(path).map_err(|err| format!("Failed to read PDF document: {err}"))?;
    if !meta.is_file() {
        return Err("Not a regular file.".to_string());
    }
    if meta.len() > limits.file_bytes {
        return Err(format!(
            "The file is {}, larger than the {} limit for PDFs.",
            mib(meta.len()),
            mib(limits.file_bytes)
        ));
    }
    let bytes = fs::read(path).map_err(|err| format!("Failed to read PDF document: {err}"))?;
    extract_from_bytes(&bytes, limits)
}

fn extract_from_bytes(bytes: &[u8], limits: &Limits) -> Result<(String, usize), String> {
    let started = Instant::now();
    let options = lopdf::LoadOptions {
        max_decompressed_size: Some(limits.stream_bytes),
        ..Default::default()
    };
    let doc = lopdf::Document::load_mem_with_options(bytes, options)
        .map_err(|e| format!("Failed to read PDF document: {}", e))?;

    let pages = doc.get_pages();
    let total_pages = pages.len();

    if total_pages == 0 {
        return Err("PDF document contains no pages".to_string());
    }

    // `get_pages` is a BTreeMap, so its keys are already in page order.
    let page_nums: Vec<u32> = pages.keys().copied().collect();
    let mut markdown = String::new();
    let mut stopped: Option<String> = None;

    for (done, &page_num) in page_nums.iter().enumerate() {
        if done == limits.pages {
            stopped = Some(format!(
                "Only the first {} of {total_pages} pages were extracted.",
                limits.pages
            ));
            break;
        }
        if started.elapsed() > limits.time {
            stopped = Some(format!(
                "Extraction stopped after {} seconds, at page {done} of {total_pages}.",
                limits.time.as_secs()
            ));
            break;
        }
        let section = match doc.extract_text_with_limit(&[page_num], limits.stream_bytes) {
            Ok(text) if !text.trim().is_empty() => {
                format!("## Page {}\n\n{}", page_num, text.trim())
            }
            Ok(_) => format!(
                "## Page {}\n\n*(No text layer detected on this page)*",
                page_num
            ),
            Err(_) => format!("## Page {}\n\n*(Couldn't read this page)*", page_num),
        };
        let separator = if markdown.is_empty() {
            ""
        } else {
            "\n\n---\n\n"
        };
        if markdown.len() + separator.len() + section.len() > limits.text_bytes {
            stopped = Some(format!(
                "Extraction stopped at page {done} of {total_pages}: the text passed the {} limit.",
                mib(limits.text_bytes as u64)
            ));
            break;
        }
        markdown.push_str(separator);
        markdown.push_str(&section);
    }

    if let Some(note) = stopped {
        if !markdown.is_empty() {
            markdown.push_str("\n\n---\n\n");
        }
        markdown.push_str(&format!("*({note})*"));
    }
    Ok((markdown, total_pages))
}

fn mib(bytes: u64) -> String {
    format!("{} MB", bytes.div_ceil(1024 * 1024))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Document, Object, Stream};

    /// A PDF with one page per entry of `texts`, each showing that text.
    fn pdf_with_pages(texts: &[&str]) -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Courier",
        });
        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        });
        let mut kids = Vec::new();
        for text in texts {
            let content = Content {
                operations: vec![
                    Operation::new("BT", vec![]),
                    Operation::new("Tf", vec!["F1".into(), 12.into()]),
                    Operation::new("Td", vec![50.into(), 700.into()]),
                    Operation::new("Tj", vec![Object::string_literal(*text)]),
                    Operation::new("ET", vec![]),
                ],
            };
            let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
            let page_id = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => content_id,
            });
            kids.push(page_id.into());
        }
        let count = kids.len() as i64;
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => kids,
                "Count" => count,
                "Resources" => resources_id,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    /// A one-page PDF whose extra object nests `depth` arrays, the shape of
    /// RUSTSEC-2026-0187 that overflowed lopdf 0.34's parser stack.
    fn pdf_with_nested_arrays(depth: usize) -> Vec<u8> {
        let mut body = String::from("%PDF-1.4\n");
        let mut offsets = Vec::new();
        let mut push = |body: &mut String, obj: String| {
            offsets.push(body.len());
            body.push_str(&obj);
        };
        push(
            &mut body,
            "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".into(),
        );
        push(
            &mut body,
            "2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n".into(),
        );
        push(
            &mut body,
            "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 1 1] /Nested 4 0 R >>\nendobj\n"
                .into(),
        );
        push(
            &mut body,
            format!(
                "4 0 obj\n{}{}\nendobj\n",
                "[".repeat(depth),
                "]".repeat(depth)
            ),
        );
        let xref_at = body.len();
        body.push_str(&format!(
            "xref\n0 {}\n0000000000 65535 f \n",
            offsets.len() + 1
        ));
        for offset in &offsets {
            body.push_str(&format!("{offset:010} 00000 n \n"));
        }
        body.push_str(&format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n",
            offsets.len() + 1
        ));
        body.into_bytes()
    }

    #[test]
    fn extracts_each_page_in_order() {
        let pdf = pdf_with_pages(&["first page", "second page"]);
        let (md, pages) = extract_from_bytes(&pdf, &Limits::DEFAULT).unwrap();
        assert_eq!(pages, 2);
        let first = md.find("## Page 1").unwrap();
        let second = md.find("## Page 2").unwrap();
        assert!(first < second, "{md}");
        assert!(
            md.contains("first page") && md.contains("second page"),
            "{md}"
        );
        assert!(!md.contains("*("), "no truncation note expected: {md}");
    }

    #[test]
    fn deeply_nested_arrays_do_not_crash() {
        // The generator makes a valid file: shallow nesting loads normally.
        let (_, pages) = extract_from_bytes(&pdf_with_nested_arrays(10), &Limits::DEFAULT).unwrap();
        assert_eq!(pages, 1);
        // 0.34 aborted the process on this; now it must return, either way.
        let pdf = pdf_with_nested_arrays(100_000);
        let _ = extract_from_bytes(&pdf, &Limits::DEFAULT);
    }

    /// Many pages sharing one small compressed content stream that inflates
    /// to megabytes of text: the amplification from the audit (F02).
    #[test]
    fn shared_compressed_stream_stays_bounded() {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Courier",
        });
        let text = "x".repeat(2 * 1024 * 1024);
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 12.into()]),
                Operation::new("Tj", vec![Object::string_literal(text)]),
                Operation::new("ET", vec![]),
            ],
        };
        let mut stream = Stream::new(dictionary! {}, content.encode().unwrap());
        stream.compress().unwrap();
        let compressed_len = stream.content.len();
        let content_id = doc.add_object(stream);
        let kids: Vec<Object> = (0..200)
            .map(|_| {
                doc.add_object(dictionary! {
                    "Type" => "Page", "Parent" => pages_id, "Contents" => content_id,
                })
                .into()
            })
            .collect();
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => kids, "Count" => 200,
                "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            }),
        );
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);
        let mut pdf = Vec::new();
        doc.save_to(&mut pdf).unwrap();
        assert!(
            compressed_len < 64 * 1024,
            "stream should be tiny: {compressed_len}"
        );

        // 200 pages x 2 MB would be 400 MB of text; the cap stops it at 16 MB.
        let (md, pages) = extract_from_bytes(&pdf, &Limits::DEFAULT).unwrap();
        assert_eq!(pages, 200);
        assert!(md.len() <= MAX_PDF_TEXT_BYTES + 1024, "{}", md.len());
        assert!(md.contains("passed the 16 MB limit"));

        // A per-page limit below the content size refuses the pages instead.
        let limits = Limits {
            stream_bytes: 1024 * 1024,
            pages: 3,
            ..Limits::DEFAULT
        };
        let (md, _) = extract_from_bytes(&pdf, &limits).unwrap();
        assert!(
            md.contains("Couldn't read this page"),
            "{}",
            &md[..md.len().min(300)]
        );
    }

    #[test]
    fn page_cap_truncates_with_a_note() {
        let pdf = pdf_with_pages(&["a", "b", "c"]);
        let limits = Limits {
            pages: 2,
            ..Limits::DEFAULT
        };
        let (md, pages) = extract_from_bytes(&pdf, &limits).unwrap();
        assert_eq!(pages, 3);
        assert!(
            md.contains("## Page 2") && !md.contains("## Page 3"),
            "{md}"
        );
        assert!(md.contains("Only the first 2 of 3 pages"), "{md}");
    }

    #[test]
    fn text_cap_truncates_with_a_note() {
        let pdf = pdf_with_pages(&["aaaaaaaaaa", "bbbbbbbbbb", "cccccccccc"]);
        let limits = Limits {
            text_bytes: 40,
            ..Limits::DEFAULT
        };
        let (md, _) = extract_from_bytes(&pdf, &limits).unwrap();
        assert!(
            md.contains("aaaaaaaaaa") && !md.contains("bbbbbbbbbb"),
            "{md}"
        );
        assert!(md.contains("passed the 1 MB limit"), "{md}");
    }

    #[test]
    fn time_cap_truncates_with_a_note() {
        let pdf = pdf_with_pages(&["a", "b"]);
        let limits = Limits {
            time: Duration::ZERO,
            ..Limits::DEFAULT
        };
        let (md, _) = extract_from_bytes(&pdf, &limits).unwrap();
        assert!(md.contains("Extraction stopped after 0 seconds"), "{md}");
    }

    #[test]
    fn oversized_and_non_regular_files_are_refused() {
        let dir = std::env::temp_dir().join(format!("litemark-pdf-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("big.pdf");
        fs::write(&file, pdf_with_pages(&["x"])).unwrap();
        let limits = Limits {
            file_bytes: 10,
            ..Limits::DEFAULT
        };
        let err = extract_with_limits(&file, &limits).unwrap_err();
        assert!(err.contains("limit for PDFs"), "{err}");
        let err = extract_with_limits(&dir, &Limits::DEFAULT).unwrap_err();
        assert!(err.contains("Not a regular file"), "{err}");
        assert!(extract_text_from_pdf(&file).is_ok());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn garbage_is_an_error_not_a_crash() {
        assert!(extract_from_bytes(b"not a pdf at all", &Limits::DEFAULT).is_err());
    }
}
