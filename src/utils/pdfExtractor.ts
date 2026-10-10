import * as pdfjsLib from 'pdfjs-dist';

// Configure PDF.js worker
if (typeof window !== 'undefined' && 'GlobalWorkerOptions' in pdfjsLib) {
  try {
    (pdfjsLib as any).GlobalWorkerOptions.workerSrc = `https://unpkg.com/pdfjs-dist@${
      (pdfjsLib as any).version || '4.0.379'
    }/build/pdf.worker.min.mjs`;
  } catch (e) {
    // Ignore worker config error
  }
}

export interface PdfExtractionResult {
  markdown: string;
  totalPages: number;
  hasText: boolean;
  pageCount: number;
}

/**
 * Extracts structured text from a PDF file and converts it into editable Markdown.
 */
export async function extractTextFromPdf(file: File | ArrayBuffer): Promise<PdfExtractionResult> {
  const arrayBuffer = file instanceof File ? await file.arrayBuffer() : file;
  const loadingTask = pdfjsLib.getDocument({
    data: new Uint8Array(arrayBuffer),
    useSystemFonts: true,
  });

  const pdf = await loadingTask.promise;
  const totalPages = pdf.numPages;
  const pageSections: string[] = [];
  let totalExtractedLength = 0;

  for (let pageNum = 1; pageNum <= totalPages; pageNum++) {
    const page = await pdf.getPage(pageNum);
    const textContent = await page.getTextContent();

    let lastY: number | null = null;
    let pageString = '';

    for (const item of textContent.items as any[]) {
      if ('str' in item && typeof item.str === 'string') {
        const text = item.str;
        const currentY = item.transform ? item.transform[5] : null;

        if (lastY !== null && currentY !== null && Math.abs(currentY - lastY) > 9) {
          pageString += '\n\n';
        } else if (
          pageString.length > 0 &&
          !pageString.endsWith(' ') &&
          !pageString.endsWith('\n') &&
          text.trim().length > 0
        ) {
          pageString += ' ';
        }

        pageString += text;
        lastY = currentY;
      }
    }

    const trimmedPage = pageString.trim();
    if (trimmedPage) {
      totalExtractedLength += trimmedPage.length;
      pageSections.push(`## Page ${pageNum}\n\n${trimmedPage}`);
    } else {
      pageSections.push(`## Page ${pageNum}\n\n*(No text layer detected on this page)*`);
    }
  }

  const hasText = totalExtractedLength > 10;
  const markdown = pageSections.join('\n\n---\n\n');

  return {
    markdown,
    totalPages,
    hasText,
    pageCount: totalPages,
  };
}
