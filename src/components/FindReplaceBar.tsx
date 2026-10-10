import React, { useState } from 'react';
import { ChevronUp, ChevronDown, X } from 'lucide-react';

interface FindReplaceBarProps {
  onClose: () => void;
  onFind: (term: string, matchCase: boolean, forward: boolean) => boolean;
  onReplace: (findTerm: string, replaceTerm: string) => void;
  onReplaceAll: (findTerm: string, replaceTerm: string) => void;
  theme: 'yaru-dark' | 'yaru-light';
}

export const FindReplaceBar: React.FC<FindReplaceBarProps> = ({
  onClose,
  onFind,
  onReplace,
  onReplaceAll,
  theme,
}) => {
  const [searchTerm, setSearchTerm] = useState('');
  const [replaceTerm, setReplaceTerm] = useState('');
  const [matchCase, setMatchCase] = useState(false);
  const [notFound, setNotFound] = useState(false);

  const isDark = theme === 'yaru-dark';

  const handleNext = () => {
    if (!searchTerm) return;
    const found = onFind(searchTerm, matchCase, true);
    setNotFound(!found);
  };

  const handlePrev = () => {
    if (!searchTerm) return;
    const found = onFind(searchTerm, matchCase, false);
    setNotFound(!found);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      if (e.shiftKey) {
        handlePrev();
      } else {
        handleNext();
      }
    } else if (e.key === 'Escape') {
      onClose();
    }
  };

  return (
    <div
      className={`flex flex-col sm:flex-row items-stretch sm:items-center gap-2 p-2 sm:px-3 sm:py-2 border-b text-xs transition-colors shadow-md z-30 ${
        isDark ? 'bg-[#292929] border-[#3E3E3E] text-[#dfdbd2]' : 'bg-[#e9e9e9] border-neutral-300 text-neutral-800'
      }`}
    >
      {/* Row 1: Search input + Next/Prev + Case sensitive */}
      <div className="flex items-center gap-1.5 flex-1 min-w-[200px]">
        <input
          type="text"
          value={searchTerm}
          onChange={(e) => {
            setSearchTerm(e.target.value);
            setNotFound(false);
          }}
          onKeyDown={handleKeyDown}
          placeholder="Find in document..."
          autoFocus
          className={`px-2 py-1.5 sm:py-1 rounded border outline-none flex-1 max-w-full sm:max-w-xs transition-colors ${
            notFound
              ? 'border-red-500 bg-red-500/10'
              : isDark
              ? 'bg-[#1e1e1e] border-[#444] text-white focus:border-neutral-400'
              : 'bg-white border-neutral-300 text-black focus:border-neutral-600'
          }`}
        />

        <button
          type="button"
          onClick={handlePrev}
          title="Previous Match (Shift+Enter)"
          className={`p-1.5 sm:p-1 rounded cursor-pointer ${
            isDark ? 'hover:bg-[#383838]' : 'hover:bg-neutral-300'
          }`}
        >
          <ChevronUp className="w-4 h-4" />
        </button>

        <button
          type="button"
          onClick={handleNext}
          title="Next Match (Enter)"
          className={`p-1.5 sm:p-1 rounded cursor-pointer ${
            isDark ? 'hover:bg-[#383838]' : 'hover:bg-neutral-300'
          }`}
        >
          <ChevronDown className="w-4 h-4" />
        </button>

        <label className="flex items-center gap-1 cursor-pointer select-none text-[11px] text-neutral-400 whitespace-nowrap ml-1">
          <input
            type="checkbox"
            checked={matchCase}
            onChange={(e) => setMatchCase(e.target.checked)}
            className="accent-neutral-700 dark:accent-neutral-300"
          />
          <span className="hidden xs:inline">Match Case</span>
          <span className="xs:hidden">Aa</span>
        </label>

        {/* Mobile close button on top row */}
        <button
          type="button"
          onClick={onClose}
          className={`sm:hidden p-1.5 rounded cursor-pointer ml-auto ${
            isDark ? 'hover:bg-[#383838] text-neutral-400' : 'hover:bg-neutral-300 text-neutral-600'
          }`}
          title="Close (Escape)"
        >
          <X className="w-4 h-4" />
        </button>
      </div>

      {/* Row 2 on mobile: Replace input + buttons */}
      <div className="flex items-center gap-1.5 flex-1 min-w-[200px]">
        <input
          type="text"
          value={replaceTerm}
          onChange={(e) => setReplaceTerm(e.target.value)}
          placeholder="Replace with..."
          className={`px-2 py-1.5 sm:py-1 rounded border outline-none flex-1 max-w-full sm:max-w-xs transition-colors ${
            isDark
              ? 'bg-[#1e1e1e] border-[#444] text-white focus:border-neutral-400'
              : 'bg-white border-neutral-300 text-black focus:border-neutral-600'
          }`}
        />

        <button
          type="button"
          onClick={() => onReplace(searchTerm, replaceTerm)}
          disabled={!searchTerm}
          className={`px-2.5 py-1.5 sm:py-1 rounded font-medium cursor-pointer transition-colors whitespace-nowrap ${
            isDark ? 'bg-[#383838] hover:bg-[#454545] text-white' : 'bg-neutral-200 hover:bg-neutral-300 text-neutral-900'
          }`}
        >
          Replace
        </button>

        <button
          type="button"
          onClick={() => onReplaceAll(searchTerm, replaceTerm)}
          disabled={!searchTerm}
          className={`px-2.5 py-1.5 sm:py-1 rounded font-medium cursor-pointer transition-colors whitespace-nowrap shadow-xs ${
            isDark
              ? 'bg-neutral-100 text-neutral-950 hover:bg-white'
              : 'bg-neutral-900 text-white hover:bg-neutral-800'
          }`}
        >
          All
        </button>

        {/* Desktop close button */}
        <button
          type="button"
          onClick={onClose}
          className={`hidden sm:block p-1 rounded cursor-pointer ml-auto ${
            isDark ? 'hover:bg-[#383838] text-neutral-400' : 'hover:bg-neutral-300 text-neutral-600'
          }`}
          title="Close (Escape)"
        >
          <X className="w-4 h-4" />
        </button>
      </div>
    </div>
  );
};
