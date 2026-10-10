/**
 * @license
 * SPDX-License-Identifier: Apache-2.0
 */

import React, { useState, useEffect, useRef, useCallback } from 'react';
import { Minimize2, X } from 'lucide-react';
import { HeaderBar } from './components/HeaderBar';
import { FormattingToolbar } from './components/FormattingToolbar';
import { FindReplaceBar } from './components/FindReplaceBar';
import { Editor } from './components/Editor';
import { StatusBar } from './components/StatusBar';
import { NativeCodeModal } from './components/NativeCodeModal';
import { ShortcutsModal } from './components/ShortcutsModal';
import { Sidebar } from './components/Sidebar';
import { DocumentTab, ThemeMode, ViewMode, WorkspaceDirectory, WorkspaceFile, WorkspaceFolder } from './types';
import { markdownToHtml, htmlToMarkdown, htmlToPlainText, computeStats } from './utils/markdownParser';
import { extractTextFromPdf } from './utils/pdfExtractor';
import { PdfPreviewModal } from './components/PdfPreviewModal';
import { InstallModal } from './components/InstallModal';

const INITIAL_MARKDOWN = `# Welcome to LiteMark

A super lightweight, high-performance **Markdown & Plain Text Editor** designed with dedicated notes folder management and native Linux desktop performance.

## ✨ Key Features
- **WYSIWYG Rich Markdown:** All markdown features work natively without showing raw syntax clutter!
- **Dedicated Notes Folder:** Automatically manages your notes in \`~/Documents/Notes\`.
- **Formatting Included:** Bold, *italic*, <u>underline</u>, and ~~strikethrough~~.
- **Header Levels:** Clean H1, H2, and H3 headers.
- **Lists & Checklists:**
  - [x] Lightweight & low memory usage (~18–20MB RAM)
  - [x] Open & save both .txt and .md files
  - [x] 1-Click install as desktop app or native .deb package
- **Quotes & Code Blocks:**
  > "Simplicity is prerequisite for reliability." — Edsger W. Dijkstra

\`\`\`
// Ultra-fast, zero-bloat native performance
fn main() {
    println!("Hello from LiteMark!");
}
\`\`\`

---

## 🚀 Native Performance (C & Rust with GTK4)
Click the **C / Rust GTK** button in the header bar to inspect and download production-ready **C** and **Rust** GTK4 source code tailored for Ubuntu with **~18MB RAM** footprint!
`;

export default function App() {
  const [tabs, setTabs] = useState<DocumentTab[]>(() => {
    const saved = localStorage.getItem('litemark_tabs') || localStorage.getItem('ubuntu_notepad_tabs');
    if (saved) {
      try {
        const parsed = JSON.parse(saved);
        if (Array.isArray(parsed) && parsed.length > 0) {
          const filtered = parsed.filter(
            (t: any) =>
              t.workspaceFileId !== 'file-roadmap' &&
              t.workspaceFileId !== 'file-journal' &&
              t.workspaceFileId !== 'file-quick-notes'
          );
          if (filtered.length > 0) {
            return filtered;
          }
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
        workspaceFileId: 'file-welcome',
        wordCount: initialStats.words,
        charCount: initialStats.chars,
        lineCount: initialStats.lines,
      },
    ];
  });

  const [activeTabId, setActiveTabId] = useState<string>(() => tabs[0]?.id || 'tab-1');

  // Dedicated Notes Directory Workspace
  const [showSidebar, setShowSidebar] = useState<boolean>(() => {
    const saved = localStorage.getItem('litemark_sidebar');
    return saved !== null ? saved === 'true' : true;
  });

  const [workspace, setWorkspace] = useState<WorkspaceDirectory>(() => {
    const saved = localStorage.getItem('litemark_workspace');
    if (saved) {
      try {
        const parsed = JSON.parse(saved);
        if (parsed && Array.isArray(parsed.files) && Array.isArray(parsed.folders)) {
          // If stored state is from previous demo with Projects & Personal folders, reset to clean default
          const isOldMockDefault =
            parsed.folders.some((f: any) => f.id === 'f-projects' || f.id === 'f-personal') ||
            parsed.files.some(
              (f: any) =>
                f.id === 'file-roadmap' || f.id === 'file-journal' || f.id === 'file-quick-notes'
            );
          if (!isOldMockDefault) {
            return parsed;
          }
        }
      } catch (e) {}
    }
    return {
      folderName: '~/Documents/Notes',
      isLinkedToLocalDisk: false,
      folders: [],
      files: [
        {
          id: 'file-welcome',
          name: 'Welcome.md',
          folderId: null,
          content: INITIAL_MARKDOWN,
          fileType: 'markdown',
          updatedAt: Date.now(),
        },
      ],
    };
  });

  useEffect(() => {
    localStorage.setItem('litemark_sidebar', showSidebar.toString());
  }, [showSidebar]);

  useEffect(() => {
    try {
      localStorage.setItem('litemark_workspace', JSON.stringify(workspace));
    } catch (e) {}
  }, [workspace]);
  const [theme, setTheme] = useState<ThemeMode>(() => {
    return (localStorage.getItem('litemark_theme') as ThemeMode) || (localStorage.getItem('ubuntu_notepad_theme') as ThemeMode) || 'yaru-dark';
  });
  const [viewMode, setViewMode] = useState<ViewMode>('wysiwyg');
  const [zoomLevel, setZoomLevel] = useState<number>(() => {
    const saved = localStorage.getItem('litemark_zoom') || localStorage.getItem('ubuntu_notepad_zoom');
    return saved ? Math.max(50, Math.min(250, Number(saved))) : 100;
  });
  const [showLineNumbers, setShowLineNumbers] = useState(false);
  const [zenMode, setZenMode] = useState(false);
  const [zenNotice, setZenNotice] = useState(false);
  const zenNoticeTimerRef = useRef<any>(null);

  const handleToggleZenMode = useCallback(() => {
    setZenMode((prev) => {
      const next = !prev;
      if (next) {
        setZenNotice(true);
        if (zenNoticeTimerRef.current) clearTimeout(zenNoticeTimerRef.current);
        zenNoticeTimerRef.current = setTimeout(() => {
          setZenNotice(false);
        }, 4000);
      } else {
        setZenNotice(false);
      }
      return next;
    });
  }, []);

  const handleExitZenMode = useCallback(() => {
    setZenMode(false);
    setZenNotice(false);
  }, []);
  const [showFindReplace, setShowFindReplace] = useState(false);
  const [showNativeModal, setShowNativeModal] = useState(false);
  const [showShortcutsModal, setShowShortcutsModal] = useState(false);
  const [showPdfModal, setShowPdfModal] = useState(false);
  const [showInstallModal, setShowInstallModal] = useState(false);
  const [deferredPrompt, setDeferredPrompt] = useState<any>(null);
  const [isStandalone, setIsStandalone] = useState(false);
  const [isExtractingPdf, setIsExtractingPdf] = useState(false);
  const [savedToast, setSavedToast] = useState<string | null>(null);
  const savedToastTimerRef = useRef<any>(null);

  useEffect(() => {
    if (savedToast) {
      if (savedToastTimerRef.current) clearTimeout(savedToastTimerRef.current);
      savedToastTimerRef.current = setTimeout(() => {
        setSavedToast(null);
      }, 3000);
    }
  }, [savedToast]);

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
    localStorage.setItem('litemark_zoom', zoomLevel.toString());
  }, [zoomLevel]);

  // Global keyboard shortcuts (Escape to exit zen mode / close modals, F11 for zen mode)
  useEffect(() => {
    const handleGlobalKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        if (showShortcutsModal || showNativeModal || showPdfModal || showInstallModal || showFindReplace) {
          setShowShortcutsModal(false);
          setShowNativeModal(false);
          setShowPdfModal(false);
          setShowInstallModal(false);
          setShowFindReplace(false);
          return;
        }
        if (zenMode) {
          e.preventDefault();
          handleExitZenMode();
        }
      } else if (e.key === 'F11') {
        e.preventDefault();
        handleToggleZenMode();
      } else if ((e.ctrlKey || e.metaKey) && e.key === '\\') {
        e.preventDefault();
        setShowSidebar((prev) => !prev);
      }
    };

    window.addEventListener('keydown', handleGlobalKeyDown);
    return () => {
      window.removeEventListener('keydown', handleGlobalKeyDown);
    };
  }, [
    zenMode,
    showShortcutsModal,
    showNativeModal,
    showPdfModal,
    showInstallModal,
    showFindReplace,
    handleExitZenMode,
    handleToggleZenMode,
  ]);

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
      localStorage.setItem('litemark_tabs', JSON.stringify(tabs));
    } catch (e) {
      // Storage quota or error
    }
  }, [tabs]);

  // Save theme to local storage
  useEffect(() => {
    localStorage.setItem('litemark_theme', theme);
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

    // Sync with workspace directory note if linked
    const currentTab = tabs.find((t) => t.id === activeTabId);
    if (currentTab?.workspaceFileId) {
      setWorkspace((prev) => ({
        ...prev,
        files: prev.files.map((f) =>
          f.id === currentTab.workspaceFileId
            ? { ...f, content: markdown, updatedAt: Date.now() }
            : f
        ),
      }));
    }
  };

  // Workspace folder directory actions
  const handleOpenFileFromWorkspace = (file: WorkspaceFile) => {
    const existing = tabs.find((t) => t.workspaceFileId === file.id || t.title === file.name);
    if (existing) {
      setActiveTabId(existing.id);
      return;
    }

    const newId = `tab-${Date.now()}`;
    const stats = computeStats(file.content);
    const newTab: DocumentTab = {
      id: newId,
      title: file.name,
      content: file.content,
      fileType: file.name.endsWith('.txt') ? 'plaintext' : 'markdown',
      isModified: false,
      workspaceFileId: file.id,
      savedHandle: file.handle,
      wordCount: stats.words,
      charCount: stats.chars,
      lineCount: stats.lines,
    };
    setTabs((prev) => [...prev, newTab]);
    setActiveTabId(newId);
  };

  const handleCreateFileInWorkspace = (folderId: string | null, name: string) => {
    const newId = `file-${Date.now()}`;
    const initialContent = `# ${name.replace(/\.[^/.]+$/, '')}\n\n`;
    const newFile: WorkspaceFile = {
      id: newId,
      name,
      folderId,
      content: initialContent,
      fileType: name.endsWith('.txt') ? 'plaintext' : 'markdown',
      updatedAt: Date.now(),
    };
    setWorkspace((prev) => ({
      ...prev,
      files: [...prev.files, newFile],
    }));
    handleOpenFileFromWorkspace(newFile);
  };

  const handleCreateFolderInWorkspace = (parentId: string | null, name: string) => {
    const newId = `folder-${Date.now()}`;
    setWorkspace((prev) => ({
      ...prev,
      folders: [...prev.folders, { id: newId, name, parentId, isOpen: true }],
    }));
  };

  const handleRenameWorkspaceFile = (fileId: string, newName: string) => {
    setWorkspace((prev) => ({
      ...prev,
      files: prev.files.map((f) => (f.id === fileId ? { ...f, name: newName } : f)),
    }));
    setTabs((prev) =>
      prev.map((t) => (t.workspaceFileId === fileId ? { ...t, title: newName } : t))
    );
  };

  const handleDeleteWorkspaceFile = (fileId: string) => {
    setWorkspace((prev) => ({
      ...prev,
      files: prev.files.filter((f) => f.id !== fileId),
    }));
    setTabs((prev) => prev.filter((t) => t.workspaceFileId !== fileId || tabs.length <= 1));
  };

  const handleRenameWorkspaceFolder = (folderId: string, newName: string) => {
    setWorkspace((prev) => ({
      ...prev,
      folders: prev.folders.map((f) => (f.id === folderId ? { ...f, name: newName } : f)),
    }));
  };

  const handleDeleteWorkspaceFolder = (folderId: string) => {
    setWorkspace((prev) => ({
      ...prev,
      folders: prev.folders.filter((f) => f.id !== folderId && f.parentId !== folderId),
      files: prev.files.filter((f) => f.folderId !== folderId),
    }));
  };

  const handleToggleFolder = (folderId: string) => {
    setWorkspace((prev) => ({
      ...prev,
      folders: prev.folders.map((f) =>
        f.id === folderId ? { ...f, isOpen: f.isOpen === false ? true : false } : f
      ),
    }));
  };

  const handleLinkLocalDirectory = async () => {
    if ('showDirectoryPicker' in window) {
      try {
        const dirHandle = await (window as any).showDirectoryPicker({
          mode: 'readwrite',
        });
        const loadedFiles: WorkspaceFile[] = [];
        const loadedFolders: WorkspaceFolder[] = [];

        // Recursive directory scanner
        const scanDirectory = async (
          currentDir: any,
          parentId: string | null = null,
          prefix: string = ''
        ) => {
          for await (const [name, handle] of currentDir.entries()) {
            if (handle.kind === 'file') {
              if (
                name.endsWith('.md') ||
                name.endsWith('.txt') ||
                name.endsWith('.text') ||
                name.endsWith('.markdown')
              ) {
                try {
                  const file = await handle.getFile();
                  const text = await file.text();
                  loadedFiles.push({
                    id: `local-file-${prefix}${name}-${Date.now()}`,
                    name,
                    folderId: parentId,
                    content: text,
                    fileType: name.endsWith('.txt') ? 'plaintext' : 'markdown',
                    updatedAt: file.lastModified,
                    handle,
                  });
                } catch (e) {}
              }
            } else if (handle.kind === 'directory') {
              // Ignore hidden directories like .git, node_modules, etc.
              if (!name.startsWith('.') && name !== 'node_modules') {
                const folderId = `local-folder-${prefix}${name}`;
                loadedFolders.push({
                  id: folderId,
                  name,
                  parentId,
                  isOpen: true,
                });
                await scanDirectory(handle, folderId, `${prefix}${name}/`);
              }
            }
          }
        };

        await scanDirectory(dirHandle);

        const newFiles =
          loadedFiles.length > 0
            ? loadedFiles
            : [
                {
                  id: `file-welcome-${Date.now()}`,
                  name: 'Welcome.md',
                  folderId: null,
                  content: INITIAL_MARKDOWN,
                  fileType: 'markdown' as const,
                  updatedAt: Date.now(),
                },
              ];

        const chosenFolderTitle = dirHandle.name ? `📁 ${dirHandle.name}` : '📁 Notes';
        setWorkspace({
          folderName: chosenFolderTitle,
          isLinkedToLocalDisk: true,
          folders: loadedFolders,
          files: newFiles,
        });

        // Set active tab to the first file in the chosen directory
        const activeItem = newFiles[0];
        const stats = computeStats(activeItem.content);
        const newTab: DocumentTab = {
          id: `tab-${activeItem.id}`,
          title: activeItem.name,
          content: activeItem.content,
          fileType: activeItem.fileType,
          isModified: false,
          savedHandle: activeItem.handle,
          workspaceFileId: activeItem.id,
          wordCount: stats.words,
          charCount: stats.chars,
          lineCount: stats.lines,
        };
        setTabs([newTab]);
        setActiveTabId(newTab.id);

        setSavedToast(`Connected to directory: ${dirHandle.name || 'Notes'}`);
        return;
      } catch (err: any) {
        if (err.name === 'AbortError') return;
        console.warn('showDirectoryPicker failed or blocked, falling back to manual path prompt:', err);
      }
    }

    // Fallback: allow user to choose/enter custom directory name/path
    const currentName = workspace.folderName.replace(/^📁\s*/, '');
    const chosenPath = prompt(
      'Choose Notes Directory (enter local folder path or name):',
      currentName || '~/Documents/Notes'
    );
    if (chosenPath && chosenPath.trim()) {
      setWorkspace((prev) => ({
        ...prev,
        folderName: chosenPath.trim(),
      }));
      setSavedToast(`Directory set to ${chosenPath.trim()}`);
    }
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

    // If file belongs to workspace directory
    if (activeTab.workspaceFileId) {
      if (activeTab.savedHandle && activeTab.savedHandle.createWritable) {
        try {
          const writable = await activeTab.savedHandle.createWritable();
          const exportText =
            activeTab.fileType === 'plaintext'
              ? htmlToPlainText(markdownToHtml(activeTab.content))
              : activeTab.content;
          await writable.write(exportText);
          await writable.close();
        } catch (e) {
          console.warn('Could not write to local disk handle', e);
        }
      }

      setWorkspace((prev) => ({
        ...prev,
        files: prev.files.map((f) =>
          f.id === activeTab.workspaceFileId
            ? { ...f, content: activeTab.content, updatedAt: Date.now() }
            : f
        ),
      }));

      setTabs((prev) =>
        prev.map((t) => (t.id === activeTabId ? { ...t, isModified: false } : t))
      );

      setSavedToast(`Saved to ${workspace.folderName}/${activeTab.title}`);
      return;
    }

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
        setSavedToast(`Saved ${activeTab.title}`);
        return;
      } catch (e) {
        // If write fails, fall back to Save As
      }
    }

    // Otherwise automatically store and save into workspace directory
    const newFileId = `file-${Date.now()}`;
    const newFileTitle = activeTab.title.includes('.') ? activeTab.title : `${activeTab.title}.md`;
    const newFile: WorkspaceFile = {
      id: newFileId,
      name: newFileTitle,
      folderId: null,
      content: activeTab.content,
      fileType: activeTab.fileType === 'plaintext' ? 'plaintext' : 'markdown',
      updatedAt: Date.now(),
    };

    setWorkspace((prev) => ({
      ...prev,
      files: [...prev.files, newFile],
    }));

    setTabs((prev) =>
      prev.map((t) =>
        t.id === activeTabId
          ? { ...t, title: newFileTitle, workspaceFileId: newFileId, isModified: false }
          : t
      )
    );

    setSavedToast(`Saved to ${workspace.folderName}/${newFileTitle}`);
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
      {!zenMode ? (
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
          onToggleZenMode={handleToggleZenMode}
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
          isStandalone={isStandalone}
          showSidebar={showSidebar}
          onToggleSidebar={() => setShowSidebar((prev) => !prev)}
        />
      ) : (
        /* Zen Mode Overlay: Floating Exit Button & Notice */
        <>
          <div className="fixed top-3 right-4 z-50 flex items-center gap-2 select-none">
            <button
              type="button"
              onClick={handleExitZenMode}
              className={`group flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs font-medium shadow-xl border backdrop-blur-md transition-all cursor-pointer opacity-80 hover:opacity-100 ${
                theme === 'yaru-dark'
                  ? 'bg-[#292929]/95 text-neutral-200 border-[#444] hover:bg-[#383838] hover:text-white hover:border-[#666]'
                  : 'bg-white/95 text-neutral-800 border-neutral-300 hover:bg-neutral-100 hover:border-neutral-400 shadow-md'
              }`}
              title="Exit Zen Mode (Esc or F11)"
            >
              <Minimize2 className="w-3.5 h-3.5 text-neutral-400 group-hover:text-white" />
              <span>Exit Zen Mode</span>
              <kbd
                className={`font-mono text-[10px] px-1.5 py-0.5 rounded border ${
                  theme === 'yaru-dark'
                    ? 'bg-[#1e1e1e] border-[#4a4a4a] text-neutral-300'
                    : 'bg-neutral-100 border-neutral-300 text-neutral-700'
                }`}
              >
                Esc
              </kbd>
            </button>
          </div>

          {zenNotice && (
            <div
              className={`fixed top-12 left-1/2 -translate-x-1/2 z-50 px-4 py-2 rounded-xl text-xs shadow-2xl border backdrop-blur-md flex items-center gap-3 transition-all ${
                theme === 'yaru-dark'
                  ? 'bg-[#292929]/95 border-[#444] text-[#dfdbd2]'
                  : 'bg-white/95 border-neutral-300 text-neutral-800'
              }`}
            >
              <span>
                🌿 <strong>Zen Mode active</strong> — Press <kbd className="font-mono px-1.5 py-0.5 rounded border border-neutral-500/40 font-semibold text-[11px]">Esc</kbd> or click <strong>Exit Zen Mode</strong> in the top-right corner to go back.
              </span>
              <button
                type="button"
                onClick={() => setZenNotice(false)}
                className="p-1 rounded hover:bg-neutral-500/20 text-neutral-400 hover:text-white cursor-pointer"
                title="Dismiss"
              >
                <X className="w-3.5 h-3.5" />
              </button>
            </div>
          )}
        </>
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

      {/* Main Workspace (Sidebar Folder Directory + Editor) */}
      <div className="flex-1 flex overflow-hidden relative">
        {!zenMode && showSidebar && (
          <Sidebar
            workspace={workspace}
            activeFileId={activeTab?.workspaceFileId}
            openFileIds={tabs.map((t) => t.workspaceFileId).filter(Boolean) as string[]}
            theme={theme}
            onSelectFile={handleOpenFileFromWorkspace}
            onCreateFile={handleCreateFileInWorkspace}
            onCreateFolder={handleCreateFolderInWorkspace}
            onRenameFile={handleRenameWorkspaceFile}
            onDeleteFile={handleDeleteWorkspaceFile}
            onRenameFolder={handleRenameWorkspaceFolder}
            onDeleteFolder={handleDeleteWorkspaceFolder}
            onToggleFolder={handleToggleFolder}
            onLinkLocalDirectory={handleLinkLocalDirectory}
            onCloseSidebar={() => setShowSidebar(false)}
          />
        )}

        <div className="flex-1 flex flex-col min-w-0 h-full overflow-hidden">
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
        </div>
      </div>

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

      {/* Save Notification Toast */}
      {savedToast && (
        <div
          className={`fixed bottom-10 right-6 z-50 px-3.5 py-2 rounded-lg text-xs font-medium shadow-2xl border flex items-center gap-2.5 transition-all pointer-events-none ${
            theme === 'yaru-dark'
              ? 'bg-[#252525]/95 border-[#444] text-[#dfdbd2] shadow-black/50'
              : 'bg-white/95 border-neutral-300 text-neutral-800 shadow-md'
          }`}
        >
          <span className="w-2 h-2 rounded-full bg-emerald-500 shrink-0" />
          <span>{savedToast}</span>
        </div>
      )}
    </div>
  );
}
