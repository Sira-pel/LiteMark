/**
 * Ultra-lightweight, zero-dependency Markdown <-> HTML converter
 * Optimized for WYSIWYG editing without visible syntax characters.
 */

export function markdownToHtml(markdown: string): string {
  if (!markdown) return '<p><br></p>';

  const lines = markdown.split(/\r?\n/);
  const htmlParts: string[] = [];

  let inCodeBlock = false;
  let codeBlockContent: string[] = [];
  let inUl = false;
  let inOl = false;
  let inBlockquote = false;
  let quoteContent: string[] = [];
  let inTable = false;
  let tableRows: string[][] = [];

  const closeList = () => {
    if (inUl) {
      htmlParts.push('</ul>');
      inUl = false;
    }
    if (inOl) {
      htmlParts.push('</ol>');
      inOl = false;
    }
  };

  const closeBlockquote = () => {
    if (inBlockquote) {
      const innerHtml = quoteContent.map(line => formatInline(line)).join('<br>');
      htmlParts.push(`<blockquote>${innerHtml}</blockquote>`);
      inBlockquote = false;
      quoteContent = [];
    }
  };

  const closeTable = () => {
    if (inTable && tableRows.length > 0) {
      let tHtml = '<table>';
      const header = tableRows[0];
      tHtml += '<thead><tr>' + header.map(c => `<th>${formatInline(c.trim())}</th>`).join('') + '</tr></thead>';
      if (tableRows.length > 1) {
        tHtml += '<tbody>';
        for (let i = 1; i < tableRows.length; i++) {
          tHtml += '<tr>' + tableRows[i].map(c => `<td>${formatInline(c.trim())}</td>`).join('') + '</tr>';
        }
        tHtml += '</tbody>';
      }
      tHtml += '</table>';
      htmlParts.push(tHtml);
      inTable = false;
      tableRows = [];
    }
  };

  for (let i = 0; i < lines.length; i++) {
    const rawLine = lines[i];
    const trimmed = rawLine.trim();

    // Code block toggle
    if (trimmed.startsWith('```')) {
      if (inCodeBlock) {
        // close code block
        const escaped = escapeHtml(codeBlockContent.join('\n'));
        htmlParts.push(`<pre><code>${escaped}</code></pre>`);
        inCodeBlock = false;
        codeBlockContent = [];
      } else {
        closeList();
        closeBlockquote();
        closeTable();
        inCodeBlock = true;
      }
      continue;
    }

    if (inCodeBlock) {
      codeBlockContent.push(rawLine);
      continue;
    }

    // Horizontal Rule
    if (/^(---|___|\*\*\*)$/.test(trimmed)) {
      closeList();
      closeBlockquote();
      closeTable();
      htmlParts.push('<hr>');
      continue;
    }

    // Headings
    if (/^#{1,6}\s/.test(trimmed)) {
      closeList();
      closeBlockquote();
      closeTable();
      const level = trimmed.match(/^#+/)?.[0].length || 1;
      const text = trimmed.replace(/^#+\s*/, '');
      const tag = level <= 3 ? `h${level}` : 'h3';
      htmlParts.push(`<${tag}>${formatInline(text)}</${tag}>`);
      continue;
    }

    // Blockquote
    if (trimmed.startsWith('>')) {
      closeList();
      closeTable();
      inBlockquote = true;
      quoteContent.push(trimmed.replace(/^>\s?/, ''));
      continue;
    } else if (inBlockquote) {
      closeBlockquote();
    }

    // Markdown Table
    if (trimmed.startsWith('|') && trimmed.endsWith('|')) {
      closeList();
      closeBlockquote();
      // Check if it's separator line like |---|---|
      if (/^\|[\s\-:|]+\|$/.test(trimmed)) {
        continue;
      }
      const cells = trimmed.slice(1, -1).split('|');
      inTable = true;
      tableRows.push(cells);
      continue;
    } else if (inTable) {
      closeTable();
    }

    // Task list items: - [ ] or - [x]
    const taskMatch = trimmed.match(/^[-*]\s+\[([ xX])\]\s+(.*)/);
    if (taskMatch) {
      closeBlockquote();
      closeTable();
      if (!inUl) {
        closeList();
        htmlParts.push('<ul>');
        inUl = true;
      }
      const checked = taskMatch[1].toLowerCase() === 'x';
      const text = taskMatch[2];
      htmlParts.push(
        `<li class="task-list-item"><input type="checkbox" ${checked ? 'checked' : ''} disabled /><span>${formatInline(text)}</span></li>`
      );
      continue;
    }

    // Unordered list: - or *
    const ulMatch = trimmed.match(/^[-*]\s+(.*)/);
    if (ulMatch) {
      closeBlockquote();
      closeTable();
      if (!inUl) {
        closeList();
        htmlParts.push('<ul>');
        inUl = true;
      }
      htmlParts.push(`<li>${formatInline(ulMatch[1])}</li>`);
      continue;
    }

    // Ordered list: 1.
    const olMatch = trimmed.match(/^(\d+)\.\s+(.*)/);
    if (olMatch) {
      closeBlockquote();
      closeTable();
      if (!inOl) {
        closeList();
        htmlParts.push('<ol>');
        inOl = true;
      }
      htmlParts.push(`<li>${formatInline(olMatch[2])}</li>`);
      continue;
    }

    // Blank line
    if (trimmed === '') {
      closeList();
      closeBlockquote();
      closeTable();
      htmlParts.push('<p><br></p>');
      continue;
    }

    // Regular paragraph line
    closeList();
    closeBlockquote();
    closeTable();
    htmlParts.push(`<p>${formatInline(rawLine)}</p>`);
  }

  closeList();
  closeBlockquote();
  closeTable();
  if (inCodeBlock) {
    const escaped = escapeHtml(codeBlockContent.join('\n'));
    htmlParts.push(`<pre><code>${escaped}</code></pre>`);
  }

  return htmlParts.join('\n') || '<p><br></p>';
}

/**
 * Format inline markdown characters: bold, italic, underline, strikethrough, code, links
 */
function formatInline(text: string): string {
  let res = escapeHtml(text);

  // Inline code: `code`
  res = res.replace(/`([^`]+)`/g, '<code>$1</code>');

  // Bold & Italic: ***text*** or ___text___
  res = res.replace(/\*\*\*([^*]+)\*\*\*/g, '<strong><em>$1</em></strong>');

  // Bold: **text** or __text__
  res = res.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
  res = res.replace(/__([^_]+)__/g, '<strong>$1</strong>');

  // Underline: <u>text</u> or <ins>text</ins>
  res = res.replace(/&lt;u&gt;(.*?)&lt;\/u&gt;/gi, '<u>$1</u>');
  res = res.replace(/&lt;ins&gt;(.*?)&lt;\/ins&gt;/gi, '<u>$1</u>');

  // Strikethrough: ~~text~~
  res = res.replace(/~~([^~]+)~~/g, '<s>$1</s>');

  // Italic: *text* or _text_
  res = res.replace(/\*([^*]+)\*/g, '<em>$1</em>');
  res = res.replace(/_([^_]+)_/g, '<em>$1</em>');

  // Links: [text](url)
  res = res.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" target="_blank" rel="noopener noreferrer">$1</a>');

  return res;
}

function escapeHtml(str: string): string {
  return str
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}

/**
 * Reverse converter: Takes HTML from the WYSIWYG contenteditable element and returns clean Markdown.
 */
export function htmlToMarkdown(html: string): string {
  if (!html) return '';

  const parser = new DOMParser();
  const doc = parser.parseFromString(html, 'text/html');

  function nodeToMarkdown(node: Node): string {
    if (node.nodeType === Node.TEXT_NODE) {
      return node.textContent || '';
    }

    if (node.nodeType !== Node.ELEMENT_NODE) {
      return '';
    }

    const el = node as HTMLElement;
    const tag = el.tagName.toLowerCase();

    // Line breaks
    if (tag === 'br') return '\n';

    // Inline elements
    let innerText = Array.from(el.childNodes).map(nodeToMarkdown).join('');

    switch (tag) {
      case 'strong':
      case 'b':
        return innerText.trim() ? `**${innerText.trim()}**` : '';
      case 'em':
      case 'i':
        return innerText.trim() ? `*${innerText.trim()}*` : '';
      case 'u':
      case 'ins':
        return innerText.trim() ? `<u>${innerText.trim()}</u>` : '';
      case 's':
      case 'strike':
      case 'del':
        return innerText.trim() ? `~~${innerText.trim()}~~` : '';
      case 'code':
        if (el.parentElement?.tagName.toLowerCase() === 'pre') {
          return innerText;
        }
        return `\`${innerText}\``;
      case 'pre': {
        const codeEl = el.querySelector('code');
        const codeText = codeEl ? codeEl.textContent || '' : innerText;
        return `\n\`\`\`\n${codeText.trim()}\n\`\`\`\n\n`;
      }
      case 'a': {
        const href = el.getAttribute('href') || '#';
        return `[${innerText}](${href})`;
      }
      case 'h1':
        return `\n# ${innerText.trim()}\n\n`;
      case 'h2':
        return `\n## ${innerText.trim()}\n\n`;
      case 'h3':
        return `\n### ${innerText.trim()}\n\n`;
      case 'h4':
        return `\n#### ${innerText.trim()}\n\n`;
      case 'h5':
        return `\n##### ${innerText.trim()}\n\n`;
      case 'h6':
        return `\n###### ${innerText.trim()}\n\n`;
      case 'p':
        if (innerText.trim() === '') return '\n';
        return `${innerText.trim()}\n\n`;
      case 'blockquote':
        return innerText
          .trim()
          .split('\n')
          .map(l => `> ${l}`)
          .join('\n') + '\n\n';
      case 'ul':
        return innerText + '\n';
      case 'ol':
        return innerText + '\n';
      case 'li': {
        const checkbox = el.querySelector('input[type="checkbox"]');
        if (checkbox) {
          const isChecked = (checkbox as HTMLInputElement).checked;
          const cleanText = innerText.replace(/^[ \t\r\n]+/, '');
          return `- [${isChecked ? 'x' : ' '}] ${cleanText.trim()}\n`;
        }
        const parentTag = el.parentElement?.tagName.toLowerCase();
        if (parentTag === 'ol') {
          const index = Array.from(el.parentElement?.children || []).indexOf(el) + 1;
          return `${index}. ${innerText.trim()}\n`;
        }
        return `- ${innerText.trim()}\n`;
      }
      case 'hr':
        return '\n---\n\n';
      case 'table': {
        let mdTable = '\n';
        const rows = el.querySelectorAll('tr');
        rows.forEach((row, rIdx) => {
          const cells = row.querySelectorAll('th, td');
          const cellTexts = Array.from(cells).map(c => (c.textContent || '').trim());
          mdTable += `| ${cellTexts.join(' | ')} |\n`;
          if (rIdx === 0) {
            mdTable += `| ${cellTexts.map(() => '---').join(' | ')} |\n`;
          }
        });
        return mdTable + '\n';
      }
      case 'div':
        return innerText ? `${innerText}\n` : '\n';
      default:
        return innerText;
    }
  }

  const result = Array.from(doc.body.childNodes)
    .map(nodeToMarkdown)
    .join('')
    .replace(/\n{3,}/g, '\n\n')
    .trim();

  return result;
}

/**
 * Strips formatting to pure plain text
 */
export function htmlToPlainText(html: string): string {
  const parser = new DOMParser();
  const doc = parser.parseFromString(html, 'text/html');
  return (doc.body.textContent || '').trim();
}

/**
 * Calculate document stats: words, chars, lines, reading time
 */
export function computeStats(text: string) {
  const chars = text.length;
  const words = text.trim() ? text.trim().split(/\s+/).filter(Boolean).length : 0;
  const lines = text ? text.split(/\r?\n/).length : 1;
  const readingTimeMin = Math.max(1, Math.ceil(words / 200));

  return { chars, words, lines, readingTimeMin };
}
