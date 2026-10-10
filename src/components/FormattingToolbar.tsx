import React from 'react';
import {
  Bold,
  Italic,
  Underline,
  Strikethrough,
  Heading1,
  Heading2,
  Heading3,
  List,
  ListOrdered,
  CheckSquare,
  Quote,
  Code,
  FileCode,
  Minus,
  Table as TableIcon,
  Link as LinkIcon,
  RotateCcw,
  RotateCw,
  Search,
  RemoveFormatting,
} from 'lucide-react';
import { ViewMode } from '../types';

interface FormattingToolbarProps {
  onFormat: (command: string, value?: string) => void;
  onInsertTable: () => void;
  onInsertLink: () => void;
  onToggleFind: () => void;
  viewMode: ViewMode;
  theme: 'yaru-dark' | 'yaru-light';
  activeFormats: {
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
  };
}

export const FormattingToolbar: React.FC<FormattingToolbarProps> = ({
  onFormat,
  onInsertTable,
  onInsertLink,
  onToggleFind,
  viewMode,
  theme,
  activeFormats,
}) => {
  const isDark = theme === 'yaru-dark';

  const btnClass = (isActive = false) =>
    `p-1.5 sm:p-1 rounded transition-all duration-75 flex items-center justify-center text-xs font-medium cursor-pointer shrink-0 min-w-[34px] sm:min-w-[28px] h-8 sm:h-7 active:scale-90 select-none ${
      isActive
        ? isDark
          ? 'bg-neutral-100 text-neutral-950 font-bold shadow-xs ring-1 ring-white/40'
          : 'bg-neutral-900 text-white font-bold shadow-xs ring-1 ring-black/40'
        : isDark
        ? 'text-[#dfdbd2] hover:bg-[#383838] hover:text-white active:bg-neutral-700 active:text-white'
        : 'text-neutral-700 hover:bg-neutral-200 hover:text-black active:bg-neutral-300 active:text-black'
    }`;

  const dividerClass = isDark ? 'w-px h-5 bg-[#3E3E3E] mx-1 shrink-0' : 'w-px h-5 bg-neutral-300 mx-1 shrink-0';

  return (
    <div
      className={`flex items-center gap-1 sm:gap-0.5 px-2 sm:px-3 py-1 sm:py-1.5 border-b overflow-x-auto select-none transition-colors scrollbar-none ${
        isDark ? 'bg-[#242424] border-[#383838]' : 'bg-[#f4f4f4] border-neutral-300'
      }`}
    >
      {/* Undo / Redo */}
      <button
        type="button"
        onClick={() => onFormat('undo')}
        className={btnClass()}
        title="Undo (Ctrl+Z)"
        aria-label="Undo"
      >
        <RotateCcw className="w-4 h-4" />
      </button>
      <button
        type="button"
        onClick={() => onFormat('redo')}
        className={btnClass()}
        title="Redo (Ctrl+Y)"
        aria-label="Redo"
      >
        <RotateCw className="w-4 h-4" />
      </button>

      <div className={dividerClass} />

      {/* Headings */}
      <button
        type="button"
        onClick={() => onFormat('heading', '1')}
        className={btnClass(activeFormats.h1)}
        title="Heading 1"
        aria-label="Heading 1"
      >
        <Heading1 className="w-4 h-4" />
      </button>
      <button
        type="button"
        onClick={() => onFormat('heading', '2')}
        className={btnClass(activeFormats.h2)}
        title="Heading 2"
        aria-label="Heading 2"
      >
        <Heading2 className="w-4 h-4" />
      </button>
      <button
        type="button"
        onClick={() => onFormat('heading', '3')}
        className={btnClass(activeFormats.h3)}
        title="Heading 3"
        aria-label="Heading 3"
      >
        <Heading3 className="w-4 h-4" />
      </button>

      <div className={dividerClass} />

      {/* Basic Text Formatting */}
      <button
        type="button"
        onClick={() => onFormat('bold')}
        className={btnClass(activeFormats.bold)}
        title="Bold (Ctrl+B)"
        aria-label="Bold"
      >
        <Bold className="w-4 h-4" />
      </button>
      <button
        type="button"
        onClick={() => onFormat('italic')}
        className={btnClass(activeFormats.italic)}
        title="Italic (Ctrl+I)"
        aria-label="Italic"
      >
        <Italic className="w-4 h-4" />
      </button>
      <button
        type="button"
        onClick={() => onFormat('underline')}
        className={btnClass(activeFormats.underline)}
        title="Underline (Ctrl+U)"
        aria-label="Underline"
      >
        <Underline className="w-4 h-4" />
      </button>
      <button
        type="button"
        onClick={() => onFormat('strikeThrough')}
        className={btnClass(activeFormats.strike)}
        title="Strikethrough"
        aria-label="Strikethrough"
      >
        <Strikethrough className="w-4 h-4" />
      </button>

      <div className={dividerClass} />

      {/* Lists & Quotes */}
      <button
        type="button"
        onClick={() => onFormat('insertUnorderedList')}
        className={btnClass(activeFormats.ul)}
        title="Bullet List"
        aria-label="Bullet List"
      >
        <List className="w-4 h-4" />
      </button>
      <button
        type="button"
        onClick={() => onFormat('insertOrderedList')}
        className={btnClass(activeFormats.ol)}
        title="Numbered List"
        aria-label="Numbered List"
      >
        <ListOrdered className="w-4 h-4" />
      </button>
      <button
        type="button"
        onClick={() => onFormat('checklist')}
        className={btnClass()}
        title="Checklist / Task List"
        aria-label="Checklist"
      >
        <CheckSquare className="w-4 h-4" />
      </button>
      <button
        type="button"
        onClick={() => onFormat('blockquote')}
        className={btnClass(activeFormats.quote)}
        title="Quote Block"
        aria-label="Quote Block"
      >
        <Quote className="w-4 h-4" />
      </button>

      <div className={dividerClass} />

      {/* Code */}
      <button
        type="button"
        onClick={() => onFormat('inlineCode')}
        className={btnClass(activeFormats.code)}
        title="Inline Code"
        aria-label="Inline Code"
      >
        <Code className="w-4 h-4" />
      </button>
      <button
        type="button"
        onClick={() => onFormat('codeBlock')}
        className={btnClass()}
        title="Code Block"
        aria-label="Code Block"
      >
        <FileCode className="w-4 h-4" />
      </button>
      <button
        type="button"
        onClick={() => onFormat('insertHorizontalRule')}
        className={btnClass()}
        title="Horizontal Divider"
        aria-label="Horizontal Divider"
      >
        <Minus className="w-4 h-4" />
      </button>

      <div className={dividerClass} />

      {/* Inserts: Table, Link */}
      <button
        type="button"
        onClick={onInsertTable}
        className={btnClass()}
        title="Insert Markdown Table"
        aria-label="Insert Table"
      >
        <TableIcon className="w-4 h-4" />
      </button>
      <button
        type="button"
        onClick={onInsertLink}
        className={btnClass()}
        title="Insert Hyperlink"
        aria-label="Insert Hyperlink"
      >
        <LinkIcon className="w-4 h-4" />
      </button>
      <button
        type="button"
        onClick={() => onFormat('removeFormat')}
        className={btnClass()}
        title="Clear Formatting"
        aria-label="Clear Formatting"
      >
        <RemoveFormatting className="w-4 h-4" />
      </button>

      <div className="flex-1 min-w-[8px]" />

      {/* Find & Replace Search Trigger */}
      <button
        type="button"
        onClick={onToggleFind}
        className={btnClass()}
        title="Find & Replace (Ctrl+F)"
        aria-label="Find & Replace"
      >
        <Search className="w-4 h-4" />
      </button>
    </div>
  );
};
