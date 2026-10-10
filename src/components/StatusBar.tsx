import React from 'react';
import { ZoomIn, ZoomOut, RotateCcw } from 'lucide-react';
import { ViewMode } from '../types';

interface StatusBarProps {
  stats: {
    words: number;
    chars: number;
    lines: number;
    readingTimeMin: number;
  };
  cursorPos: {
    line: number;
    col: number;
  };
  fileType: 'markdown' | 'plaintext' | 'pdf';
  viewMode: ViewMode;
  theme: 'yaru-dark' | 'yaru-light';
  isModified: boolean;
  zoomLevel: number; // e.g. 100 for 100%
  onZoomIn: () => void;
  onZoomOut: () => void;
  onResetZoom: () => void;
  onToggleViewMode: () => void;
}

export const StatusBar: React.FC<StatusBarProps> = ({
  stats,
  cursorPos,
  fileType,
  viewMode,
  theme,
  isModified,
  zoomLevel,
  onZoomIn,
  onZoomOut,
  onResetZoom,
  onToggleViewMode,
}) => {
  const isDark = theme === 'yaru-dark';

  return (
    <footer
      className={`h-7 px-2 sm:px-3 border-t flex items-center justify-between text-[11px] sm:text-xs select-none transition-colors shrink-0 ${
        isDark ? 'bg-[#1e1e1e] border-[#333] text-[#8F8B82]' : 'bg-[#e5e5e5] border-neutral-300 text-neutral-600'
      }`}
    >
      {/* Left: Position and document metrics */}
      <div className="flex items-center gap-2 sm:gap-3 tabular-nums font-mono truncate mr-2">
        <span>
          Ln {cursorPos.line}, Col {cursorPos.col}
        </span>
        <span aria-hidden="true" className="opacity-40">·</span>
        <span>
          {stats.words}w
        </span>
        <span aria-hidden="true" className="opacity-40 hidden sm:inline">·</span>
        <span className="hidden sm:inline">
          {stats.lines} lines
        </span>
        <span aria-hidden="true" className="opacity-40 hidden md:inline">·</span>
        <span className="hidden md:inline">
          {stats.chars} chars
        </span>
        <span aria-hidden="true" className="opacity-40 hidden lg:inline">·</span>
        <span className="hidden lg:inline">
          ~{stats.readingTimeMin}m read
        </span>
      </div>

      {/* Right: Zoom controls, Encoding, Format, and View Mode Switcher */}
      <div className="flex items-center gap-1.5 sm:gap-2.5 text-xs shrink-0">
        {/* Zoom Controls */}
        <div
          className={`flex items-center gap-0.5 font-mono tabular-nums text-[11px] px-1 py-0.5 rounded border ${
            isDark ? 'border-[#383838] bg-[#242424]' : 'border-neutral-300 bg-neutral-100'
          }`}
        >
          <button
            type="button"
            onClick={onZoomOut}
            className={`p-0.5 rounded cursor-pointer transition-colors ${
              isDark ? 'hover:text-white hover:bg-[#333]' : 'hover:text-black hover:bg-neutral-200'
            }`}
            title="Zoom Out (Ctrl - or Ctrl + Scroll Down)"
            aria-label="Zoom Out"
          >
            <ZoomOut className="w-3 h-3" />
          </button>

          <button
            type="button"
            onClick={onResetZoom}
            className={`px-1 py-0.2 rounded cursor-pointer transition-colors font-medium text-[10px] active:scale-95 ${
              zoomLevel !== 100
                ? isDark
                  ? 'text-white font-bold underline'
                  : 'text-black font-bold underline'
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
            className={`p-0.5 rounded cursor-pointer transition-colors active:scale-90 ${
              isDark ? 'hover:text-white hover:bg-[#333]' : 'hover:text-black hover:bg-neutral-200'
            }`}
            title="Zoom In (Ctrl + or Ctrl + Scroll Up)"
            aria-label="Zoom In"
          >
            <ZoomIn className="w-3 h-3" />
          </button>
        </div>

        <span aria-hidden="true" className="opacity-40 hidden sm:inline">·</span>
        <span className="hidden sm:inline">UTF-8</span>
        <span aria-hidden="true" className="opacity-40 hidden sm:inline">·</span>
        <span className="hidden sm:inline">LF</span>
        <span aria-hidden="true" className="opacity-40 hidden sm:inline">·</span>

        <button
          type="button"
          onClick={onToggleViewMode}
          title="Toggle between WYSIWYG rich view and raw source code"
          className={`px-2 py-0.5 rounded cursor-pointer transition-all duration-75 text-[11px] sm:text-xs font-medium active:scale-90 ${
            viewMode === 'wysiwyg'
              ? isDark
                ? 'bg-neutral-100 text-neutral-950 font-bold shadow-xs ring-1 ring-white/30'
                : 'bg-neutral-900 text-white font-bold shadow-xs ring-1 ring-black/30'
              : isDark
              ? 'bg-[#333] text-neutral-200 hover:bg-[#444] active:bg-neutral-600'
              : 'bg-neutral-300 text-neutral-800 hover:bg-neutral-400 active:bg-neutral-500'
          }`}
        >
          {viewMode === 'wysiwyg' ? 'WYSIWYG' : 'Raw'}
        </button>
      </div>
    </footer>
  );
};
