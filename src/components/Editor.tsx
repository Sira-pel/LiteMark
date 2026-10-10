import React, { useRef, useEffect, useState, useCallback } from 'react';
import { ViewMode, ThemeMode } from '../types';
import { markdownToHtml, htmlToMarkdown, htmlToPlainText, computeStats } from '../utils/markdownParser';

interface EditorProps {
  content: string; // Markdown text
  htmlContent: string;
  onChangeContent: (markdown: string, html: string) => void;
  viewMode: ViewMode;
  theme: ThemeMode;
  showLineNumbers: boolean;
  fontSize: number;
  onCursorChange: (pos: { line: number; col: number }) => void;
  onSaveShortcut: () => void;
  onOpenShortcut: () => void;
  onFindShortcut: () => void;
  onFormatReady: (formatter: {
    format: (cmd: string, val?: string) => void;
    insertTable: () => void;
    insertLink: () => void;
    findInText: (term: string, matchCase: boolean, forward: boolean) => boolean;
    replaceInText: (findTerm: string, replaceTerm: string) => void;
    replaceAllInText: (findTerm: string, replaceTerm: string) => void;
  }) => void;
  onActiveFormatsChange: (formats: {
    bold: boolean;
    italic: boolean;
    underline: boolean;
    strike: boolean;
    h1: boolean;
    h2: boolean;
    h3: boolean;
    ul: boolean;
    ol: boolean;
    quote: boolean;
    code: boolean;
  }) => void;
  onFileDrop: (file: File) => void;
  zoomLevel: number;
  onZoomIn: () => void;
  onZoomOut: () => void;
  onResetZoom: () => void;
}

export const Editor: React.FC<EditorProps> = ({
  content,
  htmlContent,
  onChangeContent,
  viewMode,
  theme,
  showLineNumbers,
  fontSize,
  onCursorChange,
  onSaveShortcut,
  onOpenShortcut,
  onFindShortcut,
  onFormatReady,
  onActiveFormatsChange,
  onFileDrop,
  zoomLevel,
  onZoomIn,
  onZoomOut,
  onResetZoom,
}) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const wysiwygRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const isInternalUpdateRef = useRef(false);
  const [isDragOver, setIsDragOver] = useState(false);
  const [showZoomHUD, setShowZoomHUD] = useState(false);
  const hudTimeoutRef = useRef<any>(null);

  // Show Zoom HUD momentarily when zoom changes
  useEffect(() => {
    setShowZoomHUD(true);
    if (hudTimeoutRef.current) clearTimeout(hudTimeoutRef.current);
    hudTimeoutRef.current = setTimeout(() => setShowZoomHUD(false), 1200);
    return () => {
      if (hudTimeoutRef.current) clearTimeout(hudTimeoutRef.current);
    };
  }, [zoomLevel]);

  // Ctrl + Wheel to zoom in/out smoothly
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;

    const handleWheel = (e: WheelEvent) => {
      if (e.ctrlKey) {
        e.preventDefault();
        if (e.deltaY < 0) {
          onZoomIn();
        } else if (e.deltaY > 0) {
          onZoomOut();
        }
      }
    };

    el.addEventListener('wheel', handleWheel, { passive: false });
    return () => {
      el.removeEventListener('wheel', handleWheel);
    };
  }, [onZoomIn, onZoomOut]);

  const isDark = theme === 'yaru-dark';

  // Synchronize WYSIWYG innerHTML when external content changes (e.g. file opened or tab switched)
  useEffect(() => {
    if (viewMode === 'wysiwyg' && wysiwygRef.current) {
      if (isInternalUpdateRef.current) {
        isInternalUpdateRef.current = false;
        return;
      }
      const targetHtml = htmlContent || markdownToHtml(content);
      if (wysiwygRef.current.innerHTML !== targetHtml) {
        wysiwygRef.current.innerHTML = targetHtml;
      }
    }
  }, [content, htmlContent, viewMode]);

  // Update cursor position and active formatting states
  const updateCursorAndFormatState = useCallback(() => {
    if (viewMode === 'wysiwyg') {
      const selection = window.getSelection();
      if (!selection || selection.rangeCount === 0) return;

      const range = selection.getRangeAt(0);

      // Check active formats at current cursor
      let node: Node | null = range.startContainer;
      let bold = false;
      let italic = false;
      let underline = false;
      let strike = false;
      let h1 = false;
      let h2 = false;
      let h3 = false;
      let ul = false;
      let ol = false;
      let quote = false;
      let code = false;

      while (node && node !== wysiwygRef.current) {
        if (node.nodeType === Node.ELEMENT_NODE) {
          const el = node as HTMLElement;
          const tag = el.tagName.toLowerCase();
          if (tag === 'b' || tag === 'strong') bold = true;
          if (tag === 'i' || tag === 'em') italic = true;
          if (tag === 'u') underline = true;
          if (tag === 's' || tag === 'strike' || tag === 'del') strike = true;
          if (tag === 'h1') h1 = true;
          if (tag === 'h2') h2 = true;
          if (tag === 'h3') h3 = true;
          if (tag === 'ul') ul = true;
          if (tag === 'ol') ol = true;
          if (tag === 'blockquote') quote = true;
          if (tag === 'code') code = true;
        }
        node = node.parentNode;
      }

      onActiveFormatsChange({ bold, italic, underline, strike, h1, h2, h3, ul, ol, quote, code });

      // Approximate Line & Column
      if (wysiwygRef.current) {
        const preRange = range.cloneRange();
        preRange.selectNodeContents(wysiwygRef.current);
        preRange.setEnd(range.startContainer, range.startOffset);
        const textBefore = preRange.toString();
        const lines = textBefore.split('\n');
        onCursorChange({
          line: lines.length,
          col: lines[lines.length - 1].length + 1,
        });
      }
    } else if (textareaRef.current) {
      const pos = textareaRef.current.selectionStart || 0;
      const textBefore = content.substring(0, pos);
      const lines = textBefore.split('\n');
      onCursorChange({
        line: lines.length,
        col: lines[lines.length - 1].length + 1,
      });
    }
  }, [viewMode, content, onActiveFormatsChange, onCursorChange]);

  // Handle Input in WYSIWYG mode
  const handleWysiwygInput = () => {
    if (!wysiwygRef.current) return;
    isInternalUpdateRef.current = true;
    const currentHtml = wysiwygRef.current.innerHTML;
    const newMarkdown = htmlToMarkdown(currentHtml);
    onChangeContent(newMarkdown, currentHtml);
    updateCursorAndFormatState();
  };

  // Handle Input in Raw Source mode
  const handleTextareaChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const newMarkdown = e.target.value;
    const newHtml = markdownToHtml(newMarkdown);
    onChangeContent(newMarkdown, newHtml);
    updateCursorAndFormatState();
  };

  // Formatting operations
  const executeFormat = useCallback((cmd: string, val?: string) => {
    if (viewMode === 'wysiwyg' && wysiwygRef.current) {
      wysiwygRef.current.focus();

      if (cmd === 'heading') {
        const level = val || '1';
        document.execCommand('formatBlock', false, `<h${level}>`);
      } else if (cmd === 'blockquote') {
        document.execCommand('formatBlock', false, '<blockquote>');
      } else if (cmd === 'inlineCode') {
        const selection = window.getSelection();
        if (selection && selection.toString()) {
          const selectedText = selection.toString();
          document.execCommand('insertHTML', false, `<code>${selectedText}</code>`);
        } else {
          document.execCommand('insertHTML', false, '<code>code</code>');
        }
      } else if (cmd === 'codeBlock') {
        const selection = window.getSelection();
        const codeText = selection && selection.toString() ? selection.toString() : '// Enter code here';
        document.execCommand('insertHTML', false, `<pre><code>${codeText}</code></pre><p><br></p>`);
      } else if (cmd === 'checklist') {
        document.execCommand(
          'insertHTML',
          false,
          '<ul><li class="task-list-item"><input type="checkbox" /> <span>Task item</span></li></ul><p><br></p>'
        );
      } else {
        document.execCommand(cmd, false, val);
      }
      handleWysiwygInput();
    } else if (textareaRef.current) {
      // In source code mode, insert markdown syntax markers
      const ta = textareaRef.current;
      const start = ta.selectionStart;
      const end = ta.selectionEnd;
      const selected = content.substring(start, end);

      let replacement = '';
      if (cmd === 'bold') replacement = `**${selected || 'bold text'}**`;
      else if (cmd === 'italic') replacement = `*${selected || 'italic text'}*`;
      else if (cmd === 'underline') replacement = `<u>${selected || 'underlined'}</u>`;
      else if (cmd === 'strikeThrough') replacement = `~~${selected || 'strikethrough'}~~`;
      else if (cmd === 'heading') replacement = `\n# ${selected || 'Heading'}\n`;
      else if (cmd === 'insertUnorderedList') replacement = `\n- ${selected || 'List item'}\n`;
      else if (cmd === 'insertOrderedList') replacement = `\n1. ${selected || 'List item'}\n`;
      else if (cmd === 'blockquote') replacement = `\n> ${selected || 'Quote'}\n`;
      else if (cmd === 'inlineCode') replacement = `\`${selected || 'code'}\``;
      else if (cmd === 'codeBlock') replacement = `\n\`\`\`\n${selected || '// code'}\n\`\`\`\n`;
      else if (cmd === 'insertHorizontalRule') replacement = `\n---\n`;
      else if (cmd === 'checklist') replacement = `\n- [ ] ${selected || 'Task item'}\n`;

      if (replacement) {
        const newText = content.substring(0, start) + replacement + content.substring(end);
        const newHtml = markdownToHtml(newText);
        onChangeContent(newText, newHtml);
      }
    }
  }, [viewMode, content, onChangeContent]);

  // Insert Table
  const insertTable = useCallback(() => {
    if (viewMode === 'wysiwyg' && wysiwygRef.current) {
      wysiwygRef.current.focus();
      const tableHtml = `
        <table>
          <thead>
            <tr><th>Header 1</th><th>Header 2</th><th>Header 3</th></tr>
          </thead>
          <tbody>
            <tr><td>Cell 1</td><td>Cell 2</td><td>Cell 3</td></tr>
            <tr><td>Cell 4</td><td>Cell 5</td><td>Cell 6</td></tr>
          </tbody>
        </table>
        <p><br></p>
      `;
      document.execCommand('insertHTML', false, tableHtml);
      handleWysiwygInput();
    } else {
      const tableMd = `\n| Header 1 | Header 2 | Header 3 |\n| --- | --- | --- |\n| Cell 1 | Cell 2 | Cell 3 |\n| Cell 4 | Cell 5 | Cell 6 |\n\n`;
      if (textareaRef.current) {
        const start = textareaRef.current.selectionStart;
        const newText = content.substring(0, start) + tableMd + content.substring(start);
        onChangeContent(newText, markdownToHtml(newText));
      }
    }
  }, [viewMode, content, onChangeContent]);

  // Insert Link
  const insertLink = useCallback(() => {
    const url = prompt('Enter web link URL (https://...):', 'https://');
    if (!url) return;

    if (viewMode === 'wysiwyg' && wysiwygRef.current) {
      wysiwygRef.current.focus();
      const selection = window.getSelection();
      const text = selection && selection.toString() ? selection.toString() : url;
      document.execCommand('insertHTML', false, `<a href="${url}" target="_blank" rel="noopener">${text}</a>`);
      handleWysiwygInput();
    } else {
      const linkMd = `[Link Title](${url})`;
      if (textareaRef.current) {
        const start = textareaRef.current.selectionStart;
        const newText = content.substring(0, start) + linkMd + content.substring(start);
        onChangeContent(newText, markdownToHtml(newText));
      }
    }
  }, [viewMode, content, onChangeContent]);

  // Search & Replace functionality
  const findInText = useCallback((term: string, matchCase: boolean, forward: boolean): boolean => {
    if (!term) return false;
    if (viewMode === 'wysiwyg') {
      // Use window.find if available
      if ((window as any).find) {
        return (window as any).find(term, matchCase, !forward, true, false, false, false);
      }
      return false;
    } else if (textareaRef.current) {
      const ta = textareaRef.current;
      const haystack = matchCase ? ta.value : ta.value.toLowerCase();
      const needle = matchCase ? term : term.toLowerCase();
      const currentPos = ta.selectionStart;

      let idx = forward ? haystack.indexOf(needle, currentPos + 1) : haystack.lastIndexOf(needle, currentPos - 1);
      if (idx === -1) {
        idx = forward ? haystack.indexOf(needle) : haystack.lastIndexOf(needle);
      }

      if (idx !== -1) {
        ta.focus();
        ta.setSelectionRange(idx, idx + term.length);
        return true;
      }
      return false;
    }
    return false;
  }, [viewMode]);

  const replaceInText = useCallback((findTerm: string, replaceTerm: string) => {
    if (!findTerm) return;
    if (viewMode === 'wysiwyg') {
      const selection = window.getSelection();
      if (selection && selection.toString().toLowerCase() === findTerm.toLowerCase()) {
        document.execCommand('insertText', false, replaceTerm);
        handleWysiwygInput();
      }
    } else if (textareaRef.current) {
      const ta = textareaRef.current;
      const start = ta.selectionStart;
      const end = ta.selectionEnd;
      const selected = ta.value.substring(start, end);
      if (selected.toLowerCase() === findTerm.toLowerCase()) {
        const newText = ta.value.substring(0, start) + replaceTerm + ta.value.substring(end);
        onChangeContent(newText, markdownToHtml(newText));
        setTimeout(() => {
          ta.setSelectionRange(start, start + replaceTerm.length);
        }, 0);
      }
    }
  }, [viewMode, onChangeContent]);

  const replaceAllInText = useCallback((findTerm: string, replaceTerm: string) => {
    if (!findTerm) return;
    const escaped = findTerm.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const regex = new RegExp(escaped, 'g');
    const newMarkdown = content.replace(regex, replaceTerm);
    const newHtml = markdownToHtml(newMarkdown);
    onChangeContent(newMarkdown, newHtml);
  }, [content, onChangeContent]);

  // Expose formatting commands to parent toolbar
  useEffect(() => {
    onFormatReady({
      format: executeFormat,
      insertTable,
      insertLink,
      findInText,
      replaceInText,
      replaceAllInText,
    });
  }, [executeFormat, insertTable, insertLink, findInText, replaceInText, replaceAllInText, onFormatReady]);

  // Keyboard shortcut listeners (Ctrl+B, Ctrl+I, Ctrl+U, Ctrl+S, Ctrl+O, Ctrl+F, Tab)
  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.ctrlKey || e.metaKey) {
      const key = e.key.toLowerCase();
      if (key === 'b') {
        e.preventDefault();
        executeFormat('bold');
      } else if (key === 'i') {
        e.preventDefault();
        executeFormat('italic');
      } else if (key === 'u') {
        e.preventDefault();
        e.stopPropagation();
        executeFormat('underline');
      } else if (key === 's') {
        e.preventDefault();
        onSaveShortcut();
      } else if (key === 'o') {
        e.preventDefault();
        onOpenShortcut();
      } else if (key === 'f') {
        e.preventDefault();
        onFindShortcut();
      } else if (key === '=' || e.key === '+') {
        e.preventDefault();
        onZoomIn();
      } else if (key === '-') {
        e.preventDefault();
        onZoomOut();
      } else if (key === '0') {
        e.preventDefault();
        onResetZoom();
      }
    } else if (e.key === 'Tab') {
      e.preventDefault();
      if (viewMode === 'wysiwyg') {
        document.execCommand('insertHTML', false, '&nbsp;&nbsp;');
        handleWysiwygInput();
      } else if (textareaRef.current) {
        const ta = textareaRef.current;
        const start = ta.selectionStart;
        const end = ta.selectionEnd;
        const newText = content.substring(0, start) + '  ' + content.substring(end);
        onChangeContent(newText, markdownToHtml(newText));
        setTimeout(() => {
          ta.selectionStart = ta.selectionEnd = start + 2;
        }, 0);
      }
    }
  };

  // Drag and drop file support
  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(true);
  };

  const handleDragLeave = () => {
    setIsDragOver(false);
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(false);
    if (e.dataTransfer.files && e.dataTransfer.files.length > 0) {
      onFileDrop(e.dataTransfer.files[0]);
    }
  };

  // Calculate lines for line number gutter
  const lineCount = Math.max(1, content.split(/\r?\n/).length);
  const lineNumbers = Array.from({ length: lineCount }, (_, i) => i + 1);

  return (
    <div
      ref={containerRef}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
      className={`relative flex-1 flex overflow-hidden ${
        isDark ? 'bg-[#1e1e1e] text-[#dfdbd2]' : 'bg-[#ffffff] text-neutral-900'
      }`}
    >
      {/* Floating Zoom HUD Badge */}
      {showZoomHUD && (
        <div className="absolute bottom-4 right-6 z-30 pointer-events-none transition-opacity duration-300">
          <div className="bg-[#2D2D2D]/90 text-white border border-[#444] px-2.5 py-1 rounded-md shadow-lg text-[11px] font-mono tabular-nums backdrop-blur-xs flex items-center gap-1.5">
            <span className="text-neutral-400 font-semibold">Zoom</span>
            <span className="font-bold">{zoomLevel}%</span>
          </div>
        </div>
      )}

      {/* Drag & Drop Overlay */}
      {isDragOver && (
        <div className={`absolute inset-0 z-40 border-2 border-dashed flex items-center justify-center backdrop-blur-xs ${
          isDark
            ? 'bg-black/40 border-neutral-400'
            : 'bg-neutral-900/10 border-neutral-700'
        }`}>
          <div className="bg-[#2D2D2D] text-white px-4 py-2 rounded-lg shadow-lg font-medium text-sm border border-[#444]">
            Drop text or markdown file to open
          </div>
        </div>
      )}

      {/* Line Numbers Gutter */}
      {showLineNumbers && (
        <div
          className={`hidden sm:block w-12 select-none py-6 text-right pr-3 font-mono text-xs overflow-hidden border-r shrink-0 transition-colors ${
            isDark
              ? 'bg-[#181818] border-[#292929] text-neutral-600'
              : 'bg-neutral-100 border-neutral-200 text-neutral-400'
          }`}
          style={{ fontSize: `${fontSize - 2}px` }}
        >
          {lineNumbers.map((num) => (
            <div key={num} className="leading-[1.7] tabular-nums">
              {num}
            </div>
          ))}
        </div>
      )}

      {/* Editor View: WYSIWYG Mode */}
      {viewMode === 'wysiwyg' ? (
        <div className="flex-1 overflow-y-auto p-4 sm:p-6 w-full">
          <div
            ref={wysiwygRef}
            contentEditable
            suppressContentEditableWarning
            onInput={handleWysiwygInput}
            onKeyDown={handleKeyDown}
            onKeyUp={updateCursorAndFormatState}
            onClick={updateCursorAndFormatState}
            data-placeholder="Start typing your markdown or plain text here..."
            className="wysiwyg-content min-h-[80vh] w-full outline-none"
            style={{ fontSize: `${fontSize}px` }}
          />
        </div>
      ) : (
        /* Editor View: Raw Source Code Mode */
        <textarea
          ref={textareaRef}
          value={content}
          onChange={handleTextareaChange}
          onKeyDown={handleKeyDown}
          onKeyUp={updateCursorAndFormatState}
          onClick={updateCursorAndFormatState}
          placeholder="Start typing raw Markdown or Plain Text..."
          spellCheck={false}
          className={`flex-1 w-full h-full p-4 sm:p-6 font-mono outline-none resize-none leading-[1.7] transition-colors ${
            isDark
              ? 'bg-[#1e1e1e] text-[#dfdbd2] placeholder-neutral-600'
              : 'bg-white text-neutral-900 placeholder-neutral-400'
          }`}
          style={{ fontSize: `${fontSize}px` }}
        />
      )}
    </div>
  );
};
