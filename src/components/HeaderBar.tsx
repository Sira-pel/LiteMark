import React, { useState, useRef, useEffect } from 'react';
import {
  FileText,
  Plus,
  X,
  FolderOpen,
  Save,
  Download,
  Moon,
  Sun,
  Maximize2,
  Minimize2,
  Terminal,
  HelpCircle,
  FileCode,
  Printer,
  ChevronDown,
  Menu,
  ZoomIn,
  ZoomOut,
} from 'lucide-react';
import { DocumentTab, ThemeMode, ViewMode } from '../types';

interface HeaderBarProps {
  tabs: DocumentTab[];
  activeTabId: string;
  onSelectTab: (id: string) => void;
  onCloseTab: (id: string) => void;
  onNewTab: () => void;
  onOpenFile: () => void;
  onSaveFile: () => void;
  onSaveAs: (format: 'markdown' | 'plaintext' | 'html') => void;
  onPrint: () => void;
  theme: ThemeMode;
  onToggleTheme: () => void;
  zenMode: boolean;
  onToggleZenMode: () => void;
  viewMode: ViewMode;
  onToggleViewMode: () => void;
  onOpenNativeModal: () => void;
  onOpenShortcutsModal: () => void;
  onRenameTab: (id: string, newTitle: string) => void;
  zoomLevel: number;
  onZoomIn: () => void;
  onZoomOut: () => void;
  onResetZoom: () => void;
  hasPdfUrl?: boolean;
  onViewPdf?: () => void;
  onOpenInstallModal?: () => void;
  canInstall?: boolean;
  isStandalone?: boolean;
  showSidebar?: boolean;
  onToggleSidebar?: () => void;
}

export const HeaderBar: React.FC<HeaderBarProps> = ({
  tabs,
  activeTabId,
  onSelectTab,
  onCloseTab,
  onNewTab,
  onOpenFile,
  onSaveFile,
  onSaveAs,
  onPrint,
  theme,
  onToggleTheme,
  zenMode,
  onToggleZenMode,
  viewMode,
  onToggleViewMode,
  onOpenNativeModal,
  onOpenShortcutsModal,
  onRenameTab,
  zoomLevel,
  onZoomIn,
  onZoomOut,
  onResetZoom,
  hasPdfUrl,
  onViewPdf,
  onOpenInstallModal,
  canInstall,
  isStandalone = false,
  showSidebar = false,
  onToggleSidebar,
}) => {
  const [saveMenuOpen, setSaveMenuOpen] = useState(false);
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false);
  const [editingTitleId, setEditingTitleId] = useState<string | null>(null);
  const [editTitleValue, setEditTitleValue] = useState('');
  const menuRef = useRef<HTMLDivElement>(null);
  const mobileMenuRef = useRef<HTMLDivElement>(null);

  const isDark = theme === 'yaru-dark';
  const activeTab = tabs.find((t) => t.id === activeTabId) || tabs[0];

  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setSaveMenuOpen(false);
      }
      if (mobileMenuRef.current && !mobileMenuRef.current.contains(e.target as Node)) {
        setMobileMenuOpen(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  const handleStartRename = (tab: DocumentTab) => {
    setEditingTitleId(tab.id);
    setEditTitleValue(tab.title);
  };

  const handleCommitRename = (tabId: string) => {
    if (editTitleValue.trim()) {
      onRenameTab(tabId, editTitleValue.trim());
    }
    setEditingTitleId(null);
  };

  return (
    <header
      className={`h-10 sm:h-11 border-b flex items-stretch justify-between px-1.5 sm:px-2 select-none transition-colors shrink-0 relative ${
        isDark ? 'bg-[#1b1b1b] border-[#383838] text-[#dfdbd2]' : 'bg-[#e0e0e0] border-neutral-300 text-neutral-800'
      }`}
    >
      {/* Sidebar Toggle Button (Notes Folder Directory - Leftmost) */}
      {onToggleSidebar && (
        <div className="flex items-center my-auto mr-1.5 shrink-0">
          <button
            type="button"
            onClick={onToggleSidebar}
            title={showSidebar ? 'Hide Notes Folder Directory (Ctrl+\\)' : 'Show Notes Folder Directory (Ctrl+\\)'}
            className={`p-1.5 rounded transition-all cursor-pointer flex items-center justify-center shrink-0 ${
              showSidebar
                ? isDark
                  ? 'bg-white/15 text-white border border-white/20 shadow-xs'
                  : 'bg-black/10 text-neutral-900 border border-neutral-300 shadow-xs'
                : isDark
                ? 'text-neutral-400 hover:text-white hover:bg-white/10 border border-transparent'
                : 'text-neutral-600 hover:text-black hover:bg-black/5 border border-transparent'
            }`}
            aria-label="Toggle Notes Directory Sidebar"
          >
            {/* Custom SVG matching exact rounded box + left pill bar from user image */}
            <svg
              viewBox="0 0 20 20"
              className="w-4 h-4"
              fill="none"
              stroke="currentColor"
            >
              <rect x="2.5" y="2.5" width="15" height="15" rx="3.5" strokeWidth="1.8" />
              <rect x="5.2" y="5.2" width="2.4" height="9.6" rx="1.2" fill="currentColor" stroke="none" />
            </svg>
          </button>
        </div>
      )}

      {/* Attached Document Tabs */}
      <div className="flex items-end justify-start gap-1 overflow-x-auto flex-1 min-w-0 mr-2 scrollbar-none h-full">
        {tabs.map((tab) => {
          const isActive = tab.id === activeTabId;
          const isEditing = editingTitleId === tab.id;

          return (
            <div
              key={tab.id}
              onClick={() => onSelectTab(tab.id)}
              onDoubleClick={() => handleStartRename(tab)}
              className={`flex items-center gap-1 sm:gap-1.5 px-2.5 sm:px-3 py-1.5 rounded-t-md text-xs cursor-pointer transition-all duration-75 shrink-0 max-w-[130px] sm:max-w-[180px] active:scale-[0.98] ${
                isActive
                  ? isDark
                    ? 'h-[34px] sm:h-[36px] -mb-px relative z-10 bg-[#242424] text-white font-bold border-t-2 border-t-white border-x border-[#383838] shadow-xs'
                    : 'h-[34px] sm:h-[36px] -mb-px relative z-10 bg-[#f4f4f4] text-neutral-950 font-bold border-t-2 border-t-neutral-950 border-x border-neutral-300 shadow-xs'
                  : isDark
                  ? 'h-[29px] sm:h-[31px] mb-0.5 border-t-2 border-t-transparent border-x border-x-transparent text-[#8F8B82] hover:bg-white/5 hover:text-[#dfdbd2] active:bg-white/10'
                  : 'h-[29px] sm:h-[31px] mb-0.5 border-t-2 border-t-transparent border-x border-x-transparent text-neutral-600 hover:bg-black/5 hover:text-black active:bg-black/10'
              }`}
            >
              <FileText
                className={`w-3.5 h-3.5 shrink-0 ${
                  isActive
                    ? isDark
                      ? 'text-white'
                      : 'text-neutral-900'
                    : 'text-neutral-400'
                }`}
              />

              {isEditing ? (
                <input
                  type="text"
                  value={editTitleValue}
                  onChange={(e) => setEditTitleValue(e.target.value)}
                  onBlur={() => handleCommitRename(tab.id)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') handleCommitRename(tab.id);
                    if (e.key === 'Escape') setEditingTitleId(null);
                  }}
                  autoFocus
                  className="bg-transparent border-b border-neutral-800 dark:border-neutral-200 outline-none text-xs w-20 sm:w-24 text-white"
                />
              ) : (
                <span className="truncate max-w-[70px] sm:max-w-[110px] select-none" title={tab.title}>
                  {tab.title}
                </span>
              )}

              {tab.isModified && (
                <span className="w-1.5 h-1.5 rounded-full bg-neutral-800 dark:bg-neutral-200 shrink-0" title="Modified" />
              )}

              {tabs.length > 1 && (
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    onCloseTab(tab.id);
                  }}
                  className={`p-0.5 rounded hover:bg-neutral-200 dark:hover:bg-neutral-700 hover:text-black dark:hover:text-white cursor-pointer ml-0.5 sm:ml-1 ${
                    isDark ? 'text-neutral-400' : 'text-neutral-500'
                  }`}
                  title="Close tab"
                >
                  <X className="w-3 h-3" />
                </button>
              )}
            </div>
          );
        })}

        <button
          type="button"
          onClick={onNewTab}
          className={`p-1 rounded cursor-pointer transition-colors shrink-0 mb-1 ${
            isDark ? 'hover:bg-[#2e2e2e] text-[#8F8B82] hover:text-white' : 'hover:bg-neutral-300 text-neutral-600'
          }`}
          title="New Document (Ctrl+N)"
        >
          <Plus className="w-3.5 h-3.5" />
        </button>
      </div>

      {/* Zone 3: Actions (Responsive Desktop vs Mobile Drawer) */}
      <div className="flex items-center gap-1 sm:gap-1.5 shrink-0 py-1">
        {/* Quick Save (Always visible) */}
        <button
          type="button"
          onClick={onSaveFile}
          className={`p-1.5 rounded cursor-pointer transition-colors ${
            isDark ? 'hover:bg-[#2e2e2e] text-[#dfdbd2]' : 'hover:bg-neutral-300 text-neutral-800'
          }`}
          title="Save File (Ctrl+S)"
        >
          <Save className="w-4 h-4" />
        </button>

        {/* Desktop-Only Action Buttons (hidden below md) */}
        <div className="hidden md:flex items-center gap-1">
          {/* Open File */}
          <button
            type="button"
            onClick={onOpenFile}
            className={`p-1.5 rounded cursor-pointer transition-colors ${
              isDark ? 'hover:bg-[#2e2e2e] text-[#dfdbd2]' : 'hover:bg-neutral-300 text-neutral-800'
            }`}
            title="Open Text, Markdown or PDF File (Ctrl+O)"
          >
            <FolderOpen className="w-4 h-4" />
          </button>

          {/* Save As Dropdown */}
          <div className="relative" ref={menuRef}>
            <button
              type="button"
              onClick={() => setSaveMenuOpen(!saveMenuOpen)}
              className={`p-1.5 rounded flex items-center gap-1 cursor-pointer transition-colors ${
                isDark ? 'hover:bg-[#2e2e2e] text-[#dfdbd2]' : 'hover:bg-neutral-300 text-neutral-800'
              }`}
              title="Save / Export Options"
            >
              <Download className="w-4 h-4" />
              <ChevronDown className="w-3 h-3 opacity-60" />
            </button>

            {saveMenuOpen && (
              <div
                className={`absolute right-0 top-full mt-1 w-52 py-1 rounded-lg border shadow-xl z-50 text-xs backdrop-blur-md ${
                  isDark ? 'bg-[#292929] border-[#444] text-[#dfdbd2]' : 'bg-white border-neutral-300 text-neutral-800'
                }`}
              >
                <div className="px-3 py-1 font-semibold text-[10px] text-neutral-400 tracking-wider">
                  SAVE AS
                </div>
                <button
                  type="button"
                  onClick={() => {
                    onSaveAs('markdown');
                    setSaveMenuOpen(false);
                  }}
                  className={`w-full text-left px-3 py-1.5 flex items-center gap-2 cursor-pointer transition-colors ${
                    isDark ? 'hover:bg-[#383838]' : 'hover:bg-neutral-100'
                  }`}
                >
                  <FileCode className="w-3.5 h-3.5 text-neutral-400" />
                  <span>Markdown Document (.md)</span>
                </button>
                <button
                  type="button"
                  onClick={() => {
                    onSaveAs('plaintext');
                    setSaveMenuOpen(false);
                  }}
                  className={`w-full text-left px-3 py-1.5 flex items-center gap-2 cursor-pointer transition-colors ${
                    isDark ? 'hover:bg-[#383838]' : 'hover:bg-neutral-100'
                  }`}
                >
                  <FileText className="w-3.5 h-3.5 text-neutral-400" />
                  <span>Plain Text File (.txt)</span>
                </button>
                <div className={`my-1 border-t ${isDark ? 'border-[#383838]' : 'border-neutral-200'}`} />
                <div className="px-3 py-1 font-semibold text-[10px] text-neutral-400 tracking-wider">
                  EXPORT
                </div>
                <button
                  type="button"
                  onClick={() => {
                    onSaveAs('html');
                    setSaveMenuOpen(false);
                  }}
                  className={`w-full text-left px-3 py-1.5 flex items-center gap-2 cursor-pointer transition-colors ${
                    isDark ? 'hover:bg-[#383838]' : 'hover:bg-neutral-100'
                  }`}
                >
                  <FileCode className="w-3.5 h-3.5 text-neutral-400" />
                  <span>Web Document (.html)</span>
                </button>
                <button
                  type="button"
                  onClick={() => {
                    onPrint();
                    setSaveMenuOpen(false);
                  }}
                  className={`w-full text-left px-3 py-1.5 flex items-center gap-2 cursor-pointer transition-colors ${
                    isDark ? 'hover:bg-[#383838]' : 'hover:bg-neutral-100'
                  }`}
                >
                  <Printer className="w-3.5 h-3.5 text-neutral-400" />
                  <span>Print or Export PDF</span>
                </button>
              </div>
            )}
          </div>

          {/* View Original PDF (if tab has PDF source) */}
          {hasPdfUrl && (
            <button
              type="button"
              onClick={onViewPdf}
              className={`px-2 py-1 text-xs rounded flex items-center gap-1.5 border transition-colors cursor-pointer ${
                isDark
                  ? 'bg-neutral-800 text-neutral-200 hover:bg-neutral-700 border-neutral-700'
                  : 'bg-neutral-100 text-neutral-800 hover:bg-neutral-200 border-neutral-300'
              }`}
              title="View original rendered PDF document"
            >
              <FileText className="w-3.5 h-3.5 text-neutral-400" />
              <span className="hidden xl:inline font-medium">Original PDF</span>
            </button>
          )}

          {/* C & Rust Native Code modal button (hidden in native/installed app) */}
          {!isStandalone && (
            <button
              type="button"
              onClick={onOpenNativeModal}
              className={`px-2 py-1 text-xs rounded flex items-center gap-1.5 border transition-colors cursor-pointer ${
                isDark
                  ? 'bg-neutral-800 text-neutral-200 hover:bg-neutral-700 border-neutral-700'
                  : 'bg-neutral-100 text-neutral-800 hover:bg-neutral-200 border-neutral-300'
              }`}
              title="View & Download Native C or Rust GTK4 Source Code for Ubuntu"
            >
              <Terminal className="w-3.5 h-3.5 text-neutral-400" />
              <span className="hidden xl:inline font-mono">C / Rust GTK</span>
            </button>
          )}

          {/* Zoom Controls */}
          <div
            className={`hidden lg:flex items-center gap-0.5 px-1 py-0.5 rounded border text-xs font-mono tabular-nums ${
              isDark ? 'border-[#333] bg-[#242424]' : 'border-neutral-300 bg-neutral-100'
            }`}
          >
            <button
              type="button"
              onClick={onZoomOut}
              className={`p-1 rounded cursor-pointer transition-colors ${
                isDark ? 'hover:bg-[#333]' : 'hover:bg-neutral-200'
              }`}
              title="Zoom Out (Ctrl -)"
              aria-label="Zoom Out"
            >
              <ZoomOut className="w-3.5 h-3.5" />
            </button>
            <button
              type="button"
              onClick={onResetZoom}
              className={`px-1 text-[11px] font-medium cursor-pointer transition-colors ${
                zoomLevel !== 100
                  ? isDark
                    ? 'text-white font-bold'
                    : 'text-neutral-950 font-bold'
                  : ''
              } ${isDark ? 'hover:bg-[#333]' : 'hover:bg-neutral-200'}`}
              title="Reset Zoom to 100% (Ctrl 0)"
              aria-label="Reset Zoom"
            >
              {zoomLevel}%
            </button>
            <button
              type="button"
              onClick={onZoomIn}
              className={`p-1 rounded cursor-pointer transition-colors ${
                isDark ? 'hover:bg-[#333]' : 'hover:bg-neutral-200'
              }`}
              title="Zoom In (Ctrl +)"
              aria-label="Zoom In"
            >
              <ZoomIn className="w-3.5 h-3.5" />
            </button>
          </div>

          {/* Theme Toggle */}
          <button
            type="button"
            onClick={onToggleTheme}
            className={`p-1.5 rounded cursor-pointer transition-colors ${
              isDark ? 'hover:bg-[#2e2e2e] text-[#dfdbd2]' : 'hover:bg-neutral-300 text-neutral-800'
            }`}
            title={isDark ? 'Switch to Yaru Light Theme' : 'Switch to Yaru Dark Theme'}
          >
            {isDark ? <Sun className="w-4 h-4 text-neutral-400" /> : <Moon className="w-4 h-4 text-neutral-700" />}
          </button>

          {/* Zen Mode */}
          <button
            type="button"
            onClick={onToggleZenMode}
            className={`p-1.5 rounded cursor-pointer transition-colors ${
              zenMode
                ? isDark
                  ? 'bg-neutral-100 text-neutral-950 font-semibold'
                  : 'bg-neutral-900 text-white font-semibold'
                : isDark
                ? 'hover:bg-[#2e2e2e] text-[#dfdbd2]'
                : 'hover:bg-neutral-300 text-neutral-800'
            }`}
            title={zenMode ? 'Exit Zen Mode (Esc or F11)' : 'Enter Zen Mode (Distraction-Free) (F11)'}
          >
            {zenMode ? <Minimize2 className="w-4 h-4" /> : <Maximize2 className="w-4 h-4" />}
          </button>

          {/* Shortcuts / Help */}
          <button
            type="button"
            onClick={onOpenShortcutsModal}
            className={`p-1.5 rounded cursor-pointer transition-colors ${
              isDark ? 'hover:bg-[#2e2e2e] text-[#8F8B82] hover:text-white' : 'hover:bg-neutral-300 text-neutral-600'
            }`}
            title="Keyboard Shortcuts & About"
          >
            <HelpCircle className="w-4 h-4" />
          </button>

          {/* Ubuntu GNOME Window Controls (Top Right) */}
          <div className="hidden lg:flex items-center gap-1.5 ml-1 pl-2 border-l border-neutral-300 dark:border-[#383838]" aria-hidden="true">
            <div
              className="w-4 h-4 rounded-full flex items-center justify-center text-[10px] text-neutral-500 hover:text-neutral-900 dark:text-neutral-400 dark:hover:text-white hover:bg-neutral-300 dark:hover:bg-neutral-700 transition-colors cursor-pointer select-none"
              title="Minimize"
            >
              ─
            </div>
            <div
              className="w-4 h-4 rounded-full flex items-center justify-center text-[8px] text-neutral-500 hover:text-neutral-900 dark:text-neutral-400 dark:hover:text-white hover:bg-neutral-300 dark:hover:bg-neutral-700 transition-colors cursor-pointer select-none"
              title="Maximize"
            >
              □
            </div>
            <div
              className="w-4 h-4 rounded-full flex items-center justify-center text-[9px] text-neutral-500 hover:text-neutral-900 dark:text-neutral-400 dark:hover:text-white hover:bg-neutral-300 dark:hover:bg-neutral-700 transition-colors cursor-pointer select-none"
              title="Close"
            >
              ✕
            </div>
          </div>
        </div>

        {/* Mobile Hamburger Menu (visible below md) */}
        <div className="md:hidden relative" ref={mobileMenuRef}>
          <button
            type="button"
            onClick={() => setMobileMenuOpen(!mobileMenuOpen)}
            className={`p-1.5 rounded cursor-pointer transition-colors ${
              isDark ? 'hover:bg-[#2e2e2e] text-[#dfdbd2]' : 'hover:bg-neutral-300 text-neutral-800'
            }`}
            title="More Options"
          >
            <Menu className="w-4 h-4" />
          </button>

          {mobileMenuOpen && (
            <div
              className={`absolute right-0 top-full mt-1 w-64 py-1.5 rounded-lg border shadow-xl z-50 text-xs backdrop-blur-md ${
                isDark ? 'bg-[#252525] border-[#444] text-[#dfdbd2]' : 'bg-white border-neutral-300 text-neutral-800'
              }`}
            >
              <div className="px-3 py-1 font-semibold text-[10px] text-neutral-400 tracking-wider">
                DOCUMENT ACTIONS
              </div>
              <button
                type="button"
                onClick={() => {
                  onOpenFile();
                  setMobileMenuOpen(false);
                }}
                className={`w-full text-left px-3 py-2 flex items-center gap-2.5 cursor-pointer active:scale-98 ${
                  isDark ? 'hover:bg-[#333]' : 'hover:bg-neutral-100'
                }`}
              >
                <FolderOpen className="w-4 h-4 text-neutral-400" />
                <span>Open File (.txt, .md, .pdf)</span>
              </button>

              {hasPdfUrl && (
                <button
                  type="button"
                  onClick={() => {
                    if (onViewPdf) onViewPdf();
                    setMobileMenuOpen(false);
                  }}
                  className={`w-full text-left px-3 py-2 flex items-center gap-2.5 cursor-pointer ${
                    isDark ? 'hover:bg-[#333] text-neutral-200' : 'hover:bg-neutral-100 text-neutral-800'
                  }`}
                >
                  <FileText className="w-4 h-4 text-neutral-400" />
                  <span className="font-medium">View Original PDF</span>
                </button>
              )}
              <button
                type="button"
                onClick={() => {
                  onSaveAs('markdown');
                  setMobileMenuOpen(false);
                }}
                className={`w-full text-left px-3 py-2 flex items-center gap-2.5 cursor-pointer ${
                  isDark ? 'hover:bg-[#333]' : 'hover:bg-neutral-100'
                }`}
              >
                <FileCode className="w-4 h-4 text-neutral-400" />
                <span>Save as Markdown (.md)</span>
              </button>
              <button
                type="button"
                onClick={() => {
                  onSaveAs('plaintext');
                  setMobileMenuOpen(false);
                }}
                className={`w-full text-left px-3 py-2 flex items-center gap-2.5 cursor-pointer ${
                  isDark ? 'hover:bg-[#333]' : 'hover:bg-neutral-100'
                }`}
              >
                <FileText className="w-4 h-4 text-neutral-400" />
                <span>Save as Plain Text (.txt)</span>
              </button>
              <button
                type="button"
                onClick={() => {
                  onSaveAs('html');
                  setMobileMenuOpen(false);
                }}
                className={`w-full text-left px-3 py-2 flex items-center gap-2.5 cursor-pointer ${
                  isDark ? 'hover:bg-[#333]' : 'hover:bg-neutral-100'
                }`}
              >
                <Download className="w-4 h-4 text-neutral-400" />
                <span>Export as HTML</span>
              </button>
              <button
                type="button"
                onClick={() => {
                  onPrint();
                  setMobileMenuOpen(false);
                }}
                className={`w-full text-left px-3 py-2 flex items-center gap-2.5 cursor-pointer ${
                  isDark ? 'hover:bg-[#333]' : 'hover:bg-neutral-100'
                }`}
              >
                <Printer className="w-4 h-4 text-neutral-400" />
                <span>Print / PDF</span>
              </button>

              <div className={`my-1.5 border-t ${isDark ? 'border-[#383838]' : 'border-neutral-200'}`} />
              <div className="px-3 py-1 font-semibold text-[10px] text-neutral-400 tracking-wider">
                APP SETTINGS & TOOLS
              </div>

              {/* Mobile Zoom Level */}
              <div className="flex items-center justify-between px-3 py-2 text-xs">
                <span className="flex items-center gap-2.5">
                  <ZoomIn className="w-4 h-4 text-neutral-400" />
                  <span>Zoom Scale</span>
                </span>
                <div className="flex items-center gap-1 font-mono tabular-nums">
                  <button
                    type="button"
                    onClick={onZoomOut}
                    className={`p-1.5 rounded cursor-pointer ${isDark ? 'hover:bg-[#333]' : 'hover:bg-neutral-200'}`}
                    title="Zoom Out"
                    aria-label="Zoom Out"
                  >
                    <ZoomOut className="w-3.5 h-3.5" />
                  </button>
                  <button
                    type="button"
                    onClick={onResetZoom}
                    className={`px-2 py-0.5 rounded text-[11px] font-medium cursor-pointer ${
                      zoomLevel !== 100
                        ? isDark
                          ? 'text-white font-bold'
                          : 'text-neutral-950 font-bold'
                        : ''
                    } ${isDark ? 'hover:bg-[#333]' : 'hover:bg-neutral-200'}`}
                    title="Reset Zoom"
                  >
                    {zoomLevel}%
                  </button>
                  <button
                    type="button"
                    onClick={onZoomIn}
                    className={`p-1.5 rounded cursor-pointer ${isDark ? 'hover:bg-[#333]' : 'hover:bg-neutral-200'}`}
                    title="Zoom In"
                    aria-label="Zoom In"
                  >
                    <ZoomIn className="w-3.5 h-3.5" />
                  </button>
                </div>
              </div>

              <button
                type="button"
                onClick={() => {
                  onOpenNativeModal();
                  setMobileMenuOpen(false);
                }}
                className={`w-full text-left px-3 py-2 flex items-center gap-2.5 cursor-pointer ${
                  isDark ? 'hover:bg-[#333]' : 'hover:bg-neutral-100'
                }`}
              >
                <Terminal className="w-4 h-4 text-neutral-400" />
                <span>C & Rust GTK4 Source</span>
              </button>

              {onToggleSidebar && (
                <button
                  type="button"
                  onClick={() => {
                    onToggleSidebar();
                    setMobileMenuOpen(false);
                  }}
                  className={`w-full text-left px-3 py-2 flex items-center gap-2.5 cursor-pointer ${
                    isDark ? 'hover:bg-[#333]' : 'hover:bg-neutral-100'
                  }`}
                >
                  <svg
                    viewBox="0 0 20 20"
                    className="w-4 h-4 text-neutral-400"
                    fill="none"
                    stroke="currentColor"
                  >
                    <rect x="2.5" y="2.5" width="15" height="15" rx="3.5" strokeWidth="1.8" />
                    <rect x="5.2" y="5.2" width="2.4" height="9.6" rx="1.2" fill="currentColor" stroke="none" />
                  </svg>
                  <span>{showSidebar ? 'Hide Notes Directory' : 'Show Notes Directory'}</span>
                </button>
              )}

              <button
                type="button"
                onClick={() => {
                  onToggleTheme();
                  setMobileMenuOpen(false);
                }}
                className={`w-full text-left px-3 py-2 flex items-center gap-2.5 cursor-pointer ${
                  isDark ? 'hover:bg-[#333]' : 'hover:bg-neutral-100'
                }`}
              >
                {isDark ? <Sun className="w-4 h-4 text-neutral-300" /> : <Moon className="w-4 h-4 text-neutral-700" />}
                <span>Switch Theme ({isDark ? 'Light' : 'Dark'})</span>
              </button>

              <button
                type="button"
                onClick={() => {
                  onToggleZenMode();
                  setMobileMenuOpen(false);
                }}
                className={`w-full text-left px-3 py-2 flex items-center gap-2.5 cursor-pointer ${
                  isDark ? 'hover:bg-[#333]' : 'hover:bg-neutral-100'
                }`}
              >
                {zenMode ? <Minimize2 className="w-4 h-4" /> : <Maximize2 className="w-4 h-4" />}
                <span>{zenMode ? 'Exit Zen Mode' : 'Zen Distraction-Free'}</span>
              </button>

              <button
                type="button"
                onClick={() => {
                  onOpenShortcutsModal();
                  setMobileMenuOpen(false);
                }}
                className={`w-full text-left px-3 py-2 flex items-center gap-2.5 cursor-pointer ${
                  isDark ? 'hover:bg-[#333]' : 'hover:bg-neutral-100'
                }`}
              >
                <HelpCircle className="w-4 h-4 text-neutral-400" />
                <span>Keyboard Shortcuts & Help</span>
              </button>
            </div>
          )}
        </div>
      </div>
    </header>
  );
};
