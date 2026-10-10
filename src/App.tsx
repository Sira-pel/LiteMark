/**
 * @license
 * SPDX-License-Identifier: Apache-2.0
 */

import React, { useState, useEffect, useRef, useCallback } from 'react';
import { HeaderBar } from './components/HeaderBar';
import { FormattingToolbar } from './components/FormattingToolbar';
import { FindReplaceBar } from './components/FindReplaceBar';
import { Editor } from './components/Editor';
import { StatusBar } from './components/StatusBar';
import { NativeCodeModal } from './components/NativeCodeModal';
import { ShortcutsModal } from './components/ShortcutsModal';
import { DocumentTab, ThemeMode, ViewMode } from './types';
import { markdownToHtml, htmlToMarkdown, htmlToPlainText, computeStats } from './utils/markdownParser';
import { extractTextFromPdf } from './utils/pdfExtractor';
import { PdfPreviewModal } from './components/PdfPreviewModal';
import { InstallModal } from './components/InstallModal';

const INITIAL_MARKDOWN = `# Welcome to Ubuntu Notepad

A super lightweight, high-performance **Markdown & Plain Text Editor** designed with the authentic Ubuntu GNOME & Yaru aesthetic.

## ✨ Key Features
- **WYSIWYG Rich Markdown:** All markdown features work natively without showing raw syntax clutter!
- **Formatting Included:** Bold, *italic*, <u>underline</u>, and ~~strikethrough~~.
- **Header Levels:** Clean H1, H2, and H3 headers.
- **Lists & Checklists:**
  - [x] Lightweight & low memory usage
  - [x] Open & save both .txt and .md files
  - [ ] Built-in C & Rust GTK4 native desktop code
- **Quotes & Code Blocks:**
  > "Simplicity is prerequisite for reliability." — Edsger W. Dijkstra

\`\`\`
// Ultra-fast, zero-bloat native performance
fn main() {
    println!("Hello from Ubuntu!");
}
\`\`\`

---

## 🚀 Native Performance (C & Rust with GTK4)
Click the **C / Rust GTK** button in the header bar to inspect and download production-ready **C** and **Rust** GTK4 source code tailored for Ubuntu with **~18MB RAM** footprint!
`;

export default function App() {
  const [tabs, setTabs] = useState<DocumentTab[]>(() => {
    const saved = localStorage.getItem('ubuntu_notepad_tabs');
    if (saved) {
      try {
        const parsed = JSON.parse(saved);
        if (Array.isArray(parsed) && parsed.length > 0) {
          return parsed;
        }
      } catch (e) {
        // Fallback
      }
    }
    const initialStats = computeStats(INITIAL_MARKDOWN);
    return [
      {
        id: 'tab-1',
        title: 'Welcome.md',
        content: INITIAL_MARKDOWN,
        fileType: 'markdown',
        isModified: false,
        wordCount: initialStats.words,
        charCount: initialStats.chars,
        lineCount: initialStats.lines,
      },
    ];
  });

  const [activeTabId, setActiveTabId] = useState<string>(() => tabs[0]?.id || 'tab-1');
  const [theme, setTheme] = useState<ThemeMode>(() => {
    return (localStorage.getItem('ubuntu_notepad_theme') as ThemeMode) || 'yaru-dark';
  });
  const [viewMode, setViewMode] = useState<ViewMode>('wysiwyg');
  const [zoomLevel, setZoomLevel] = useState<number>(() => {
    const saved = localStorage.getItem('ubuntu_notepad_zoom');
    return saved ? Math.max(50, Math.min(250, Number(saved))) : 100;
  });
  const [showLineNumbers, setShowLineNumbers] = useState(false);
  const [zenMode, setZenMode] = useState(false);
  const [showFindReplace, setShowFindReplace] = useState(false);
  const [showNativeModal, setShowNativeModal] = useState(false);
  const [showShortcutsModal, setShowShortcutsModal] = useState(false);
  const [showPdfModal, setShowPdfModal] = useState(false);
  const [showInstallModal, setShowInstallModal] = useState(false);
  const [deferredPrompt, setDeferredPrompt] = useState<any>(null);
  const [isStandalone, setIsStandalone] = useState(false);
  const [isExtractingPdf, setIsExtractingPdf] = useState(false);

  useEffect(() => {
    // Check if running in standalone PWA window
    const checkStandalone =
      window.matchMedia('(display-mode: standalone)').matches ||
      (window.navigator as any).standalone === true;
    setIsStandalone(checkStandalone);

    const handleBeforeInstallPrompt = (e: Event) => {
      e.preventDefault();
      setDeferredPrompt(e);
    };

    window.addEventListener('beforeinstallprompt', handleBeforeInstallPrompt);
    return () => {
      window.removeEventListener('beforeinstallprompt', handleBeforeInstallPrompt);
    };
  }, []);

  const handleTriggerPwaInstall = async () => {
    if (deferredPrompt) {
      deferredPrompt.prompt();
      const { outcome } = await deferredPrompt.userChoice;
      if (outcome === 'accepted') {
        setDeferredPrompt(null);
        setShowInstallModal(false);
      }
    }
  };

  const [cursorPos, setCursorPos] = useState({ line: 1, col: 1 });

  const handleZoomIn = useCallback(() => {
    setZoomLevel((prev) => Math.min(250, prev + 10));
  }, []);

  const handleZoomOut = useCallback(() => {
    setZoomLevel((prev) => Math.max(50, prev - 10));
  }, []);

  const handleResetZoom = useCallback(() => {
    setZoomLevel(100);
  }, []);

  useEffect(() => {
    localStorage.setItem('ubuntu_notepad_zoom', zoomLevel.toString());
  }, [zoomLevel]);

  const computedFontSize = Math.round((16 * zoomLevel) / 100);
  const [activeFormats, setActiveFormats] = useState({
    bold: false,
    italic: false,
    underline: false,
    strike: false,
    h1: false,
    h2: false,
    h3: false,
    ul: false,
    ol: false,
    quote: false,
    code: false,
  });

  const fileInputRef = useRef<HTMLInputElement>(null);
  const formatterRef = useRef<{
    format: (cmd: string, val?: string) => void;
    insertTable: () => void;
    insertLink: () => void;
    findInText: (term: string, matchCase: boolean, forward: boolean) => boolean;
    replaceInText: (findTerm: string, replaceTerm: string) => void;
    replaceAllInText: (findTerm: string, replaceTerm: string) => void;
  } | null>(null);

  // Active tab reference
  const activeTab = tabs.find((t) => t.id === activeTabId) || tabs[0];

  // Save tabs to local storage
  useEffect(() => {
    try {
      localStorage.setItem('ubuntu_notepad_tabs', JSON.stringify(tabs));
    } catch (e) {
      // Storage quota or error
    }
  }, [tabs]);

  // Save theme to local storage
  useEffect(() => {
    localStorage.setItem('ubuntu_notepad_theme', theme);
  }, [theme]);

  // Update content of active tab
  const handleContentChange = (markdown: string, html: string) => {
    const stats = computeStats(markdown);
    setTabs((prev) =>
      prev.map((t) =>
        t.id === activeTabId
          ? {
              ...t,
              content: markdown,
              isModified: true,
              wordCount: stats.words,
              charCount: stats.chars,
              lineCount: stats.lines,
            }
          : t
      )
    );
  };

  // Create new tab
  const handleNewTab = () => {
    const newId = `tab-${Date.now()}`;
    const newTitle = `Untitled-${tabs.length + 1}.md`;
    const emptyStats = computeStats('');
    const newTab: DocumentTab = {
      id: newId,
      title: newTitle,
      content: '',
      fileType: 'markdown',
      isModified: false,
      wordCount: emptyStats.words,
      charCount: emptyStats.chars,
      lineCount: emptyStats.lines,
    };
    setTabs((prev) => [...prev, newTab]);
    setActiveTabId(newId);
  };

  // Close tab
  const handleCloseTab = (id: string) => {
    if (tabs.length === 1) {
      // If closing the only tab, clear it
      handleContentChange('', '');
      return;
    }
    const nextTabs = tabs.filter((t) => t.id !== id);
    setTabs(nextTabs);
    if (activeTabId === id) {
      setActiveTabId(nextTabs[nextTabs.length - 1].id);
    }
  };

  // Rename Tab
  const handleRenameTab = (id: string, newTitle: string) => {
    const fileType = newTitle.endsWith('.txt') ? 'plaintext' : 'markdown';
    setTabs((prev) =>
      prev.map((t) => (t.id === id ? { ...t, title: newTitle, fileType } : t))
    );
  };

  // Process file (Markdown, Plain Text, or PDF)
  const handleProcessFile = async (file: File, handle?: any) => {
    const isPdf = file.name.toLowerCase().endsWith('.pdf') || file.type === 'application/pdf';

    if (isPdf) {
      setIsExtractingPdf(true);
      try {
        const result = await extractTextFromPdf(file);
        const pdfBlobUrl = URL.createObjectURL(file);
        const stats = computeStats(result.markdown);
        const newId = `tab-${Date.now()}`;
        const newTab: DocumentTab = {
          id: newId,
          title: file.name,
          content: result.markdown,
          fileType: 'pdf',
          pdfBlobUrl: pdfBlobUrl,
          pdfPageCount: result.totalPages,
          isModified: false,
          savedHandle: handle,
          wordCount: stats.words,
          charCount: stats.chars,
          lineCount: stats.lines,
        };
        setTabs((prev) => [...prev, newTab]);
        setActiveTabId(newId);
      } catch (err) {
        alert('Could not extract text from this PDF file. It might be password-protected or corrupted.');
      } finally {
        setIsExtractingPdf(false);
      }
      return;
    }

    try {
      const text = await file.text();
      loadFileContent(file.name, text, handle);
    } catch (e) {
      alert('Could not read the selected file.');
    }
  };

  // Open file handler (File System Access or fallback input)
  const handleOpenFile = async () => {
    // Try modern File System Access API
    if ('showOpenFilePicker' in window) {
      try {
        const [handle] = await (window as any).showOpenFilePicker({
          types: [
            {
              description: 'Text, Markdown, and PDF Files',
              accept: {
                'text/*': ['.txt', '.md', '.markdown', '.text', '.log'],
                'application/pdf': ['.pdf'],
              },
            },
          ],
          multiple: false,
        });
        const file = await handle.getFile();
        await handleProcessFile(file, handle);
        return;
      } catch (err: any) {
        if (err.name === 'AbortError') return;
      }
    }

    // Fallback to hidden file input
    if (fileInputRef.current) {
      fileInputRef.current.click();
    }
  };

  const handleFileInputChange = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file) {
      await handleProcessFile(file);
    }
    e.target.value = '';
  };

  const loadFileContent = (filename: string, content: string, handle?: any) => {
    const fileType = filename.endsWith('.txt') ? 'plaintext' : 'markdown';
    const stats = computeStats(content);
    const newId = `tab-${Date.now()}`;
    const newTab: DocumentTab = {
      id: newId,
      title: filename,
      content,
      fileType,
      isModified: false,
      savedHandle: handle,
      wordCount: stats.words,
      charCount: stats.chars,
      lineCount: stats.lines,
    };
    setTabs((prev) => [...prev, newTab]);
    setActiveTabId(newId);
  };

  // Save current file
  const handleSaveFile = async () => {
    if (!activeTab) return;

    // If file has existing File System Access handle
    if (activeTab.savedHandle && activeTab.savedHandle.createWritable) {
      try {
        const writable = await activeTab.savedHandle.createWritable();
        const exportText =
          activeTab.fileType === 'plaintext'
            ? htmlToPlainText(markdownToHtml(activeTab.content))
            : activeTab.content;
        await writable.write(exportText);
        await writable.close();
        setTabs((prev) =>
          prev.map((t) => (t.id === activeTabId ? { ...t, isModified: false } : t))
        );
        return;
      } catch (e) {
        // If write fails, fall back to Save As
      }
    }

    // Otherwise standard Save As
    handleSaveAs(activeTab.fileType === 'plaintext' ? 'plaintext' : 'markdown');
  };

  // Save As format
  const handleSaveAs = async (format: 'markdown' | 'plaintext' | 'html') => {
    if (!activeTab) return;

    let contentToSave = '';
    let defaultFilename = activeTab.title;
    let mimeType = 'text/plain;charset=utf-8';

    if (format === 'markdown') {
      contentToSave = activeTab.content;
      if (!defaultFilename.endsWith('.md')) {
        defaultFilename = defaultFilename.replace(/\.[^/.]+$/, '') + '.md';
      }
      mimeType = 'text/markdown;charset=utf-8';
    } else if (format === 'plaintext') {
      contentToSave = htmlToPlainText(markdownToHtml(activeTab.content));
      if (!defaultFilename.endsWith('.txt')) {
        defaultFilename = defaultFilename.replace(/\.[^/.]+$/, '') + '.txt';
      }
      mimeType = 'text/plain;charset=utf-8';
    } else if (format === 'html') {
      contentToSave = `<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <title>${activeTab.title}</title>
  <style>
    body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; line-height: 1.6; max-width: 800px; margin: 40px auto; padding: 0 20px; color: #222; }
    h1, h2, h3 { color: #111; }
    code { background: #f4f4f4; padding: 2px 5px; border-radius: 4px; font-family: monospace; }
    pre { background: #282c34; color: #abb2bf; padding: 16px; border-radius: 6px; overflow-x: auto; }
    blockquote { border-left: 4px solid #888; padding-left: 14px; color: #555; }
    table { border-collapse: collapse; width: 100%; margin: 16px 0; }
    th, td { border: 1px solid #ddd; padding: 8px 12px; text-align: left; }
    th { background: #f8f8f8; }
  </style>
</head>
<body>
  ${markdownToHtml(activeTab.content)}
</body>
</html>`;
      defaultFilename = defaultFilename.replace(/\.[^/.]+$/, '') + '.html';
      mimeType = 'text/html;charset=utf-8';
    }

    // Try modern File System Access API
    if ('showSaveFilePicker' in window) {
      try {
        const handle = await (window as any).showSaveFilePicker({
          suggestedName: defaultFilename,
          types: [
            {
              description: format === 'markdown' ? 'Markdown File' : format === 'plaintext' ? 'Plain Text' : 'HTML Document',
              accept: { [mimeType]: [`.${format === 'markdown' ? 'md' : format === 'plaintext' ? 'txt' : 'html'}`] },
            },
          ],
        });
        const writable = await handle.createWritable();
        await writable.write(contentToSave);
        await writable.close();
        setTabs((prev) =>
          prev.map((t) =>
            t.id === activeTabId
              ? {
                  ...t,
                  title: handle.name || defaultFilename,
                  savedHandle: handle,
                  isModified: false,
                  fileType: format === 'plaintext' ? 'plaintext' : 'markdown',
                }
              : t
          )
        );
        return;
      } catch (err: any) {
        if (err.name === 'AbortError') return;
      }
    }

    // Fallback: Browser Blob Download
    const blob = new Blob([contentToSave], { type: mimeType });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = defaultFilename;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);

    setTabs((prev) =>
      prev.map((t) =>
        t.id === activeTabId
          ? {
              ...t,
              title: defaultFilename,
              isModified: false,
              fileType: format === 'plaintext' ? 'plaintext' : 'markdown',
            }
          : t
      )
    );
  };

  // Print or export to PDF
  const handlePrint = () => {
    window.print();
  };

  // Formatter hook ready
  const handleFormatReady = useCallback((formatter: any) => {
    formatterRef.current = formatter;
  }, []);

  return (
    <div
      className={`h-screen h-[100dvh] flex flex-col overflow-hidden font-sans select-none ${
        theme === 'yaru-dark' ? 'bg-[#1e1e1e] text-[#dfdbd2]' : 'bg-[#ffffff] text-neutral-900'
      }`}
    >
      {/* Hidden File Input Fallback */}
      <input
        ref={fileInputRef}
        type="file"
        accept=".txt,.md,.markdown,.text,.log,.pdf,application/pdf"
        onChange={handleFileInputChange}
        className="hidden"
      />

      {/* PDF Extracting Loading Toast */}
      {isExtractingPdf && (
        <div className="absolute top-14 left-1/2 -translate-x-1/2 z-50 bg-[#2D2D2D] text-white px-4 py-2.5 rounded-lg shadow-xl border border-[#444] text-xs flex items-center gap-2.5 backdrop-blur-md animate-pulse">
          <div className="w-3.5 h-3.5 border-2 border-neutral-300 dark:border-neutral-400 border-t-transparent rounded-full animate-spin" />
          <span>Reading & extracting text from PDF document...</span>
        </div>
      )}

      {/* GNOME Ubuntu Header Bar */}
      {!zenMode && (
        <HeaderBar
          tabs={tabs}
          activeTabId={activeTabId}
          onSelectTab={setActiveTabId}
          onCloseTab={handleCloseTab}
          onNewTab={handleNewTab}
          onOpenFile={handleOpenFile}
          onSaveFile={handleSaveFile}
          onSaveAs={handleSaveAs}
          onPrint={handlePrint}
          theme={theme}
          onToggleTheme={() => setTheme(theme === 'yaru-dark' ? 'yaru-light' : 'yaru-dark')}
          zenMode={zenMode}
          onToggleZenMode={() => setZenMode(!zenMode)}
          viewMode={viewMode}
          onToggleViewMode={() => setViewMode(viewMode === 'wysiwyg' ? 'source' : 'wysiwyg')}
          onOpenNativeModal={() => setShowNativeModal(true)}
          onOpenShortcutsModal={() => setShowShortcutsModal(true)}
          onRenameTab={handleRenameTab}
          zoomLevel={zoomLevel}
          onZoomIn={handleZoomIn}
          onZoomOut={handleZoomOut}
          onResetZoom={handleResetZoom}
          hasPdfUrl={Boolean(activeTab.pdfBlobUrl)}
          onViewPdf={() => setShowPdfModal(true)}
          onOpenInstallModal={() => setShowInstallModal(true)}
          canInstall={Boolean(deferredPrompt) || !isStandalone}
        />
      )}

      {/* Formatting Toolbar */}
      {!zenMode && (
        <FormattingToolbar
          onFormat={(cmd, val) => formatterRef.current?.format(cmd, val)}
          onInsertTable={() => formatterRef.current?.insertTable()}
          onInsertLink={() => formatterRef.current?.insertLink()}
          onToggleFind={() => setShowFindReplace(!showFindReplace)}
          viewMode={viewMode}
          theme={theme}
          activeFormats={activeFormats}
        />
      )}

      {/* Find & Replace Bar Overlay */}
      {showFindReplace && (
        <FindReplaceBar
          onClose={() => setShowFindReplace(false)}
          onFind={(term, matchCase, forward) =>
            formatterRef.current ? formatterRef.current.findInText(term, matchCase, forward) : false
          }
          onReplace={(findTerm, replaceTerm) =>
            formatterRef.current?.replaceInText(findTerm, replaceTerm)
          }
          onReplaceAll={(findTerm, replaceTerm) =>
            formatterRef.current?.replaceAllInText(findTerm, replaceTerm)
          }
          theme={theme}
        />
      )}

      {/* Editor Content Area */}
      <Editor
        key={activeTab.id}
        content={activeTab.content}
        htmlContent={markdownToHtml(activeTab.content)}
        onChangeContent={handleContentChange}
        viewMode={viewMode}
        theme={theme}
        showLineNumbers={showLineNumbers}
        fontSize={computedFontSize}
        zoomLevel={zoomLevel}
        onZoomIn={handleZoomIn}
        onZoomOut={handleZoomOut}
        onResetZoom={handleResetZoom}
        onCursorChange={setCursorPos}
        onSaveShortcut={handleSaveFile}
        onOpenShortcut={handleOpenFile}
        onFindShortcut={() => setShowFindReplace(true)}
        onFormatReady={handleFormatReady}
        onActiveFormatsChange={setActiveFormats}
        onFileDrop={(file) => handleProcessFile(file)}
      />

      {/* Ubuntu Status Bar */}
      {!zenMode && (
        <StatusBar
          stats={{
            words: activeTab.wordCount,
            chars: activeTab.charCount,
            lines: activeTab.lineCount,
            readingTimeMin: Math.max(1, Math.ceil(activeTab.wordCount / 200)),
          }}
          cursorPos={cursorPos}
          fileType={activeTab.fileType}
          viewMode={viewMode}
          theme={theme}
          isModified={activeTab.isModified}
          zoomLevel={zoomLevel}
          onZoomIn={handleZoomIn}
          onZoomOut={handleZoomOut}
          onResetZoom={handleResetZoom}
          onToggleViewMode={() => setViewMode(viewMode === 'wysiwyg' ? 'source' : 'wysiwyg')}
        />
      )}

      {/* Native C & Rust GTK4 Project Dialog */}
      <NativeCodeModal
        isOpen={showNativeModal}
        onClose={() => setShowNativeModal(false)}
        theme={theme}
      />

      {/* Shortcuts Modal */}
      <ShortcutsModal
        isOpen={showShortcutsModal}
        onClose={() => setShowShortcutsModal(false)}
        theme={theme}
      />

      {/* PDF Document Preview Modal */}
      <PdfPreviewModal
        isOpen={showPdfModal}
        onClose={() => setShowPdfModal(false)}
        title={activeTab.title}
        pdfUrl={activeTab.pdfBlobUrl}
        pageCount={activeTab.pdfPageCount}
        theme={theme}
      />

      {/* Seamless Ubuntu Installation Modal */}
      <InstallModal
        isOpen={showInstallModal}
        onClose={() => setShowInstallModal(false)}
        theme={theme}
        deferredPrompt={deferredPrompt}
        onTriggerPwaInstall={handleTriggerPwaInstall}
        isStandalone={isStandalone}
      />
    </div>
  );
}
