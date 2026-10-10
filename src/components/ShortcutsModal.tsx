import React from 'react';
import { X, Keyboard } from 'lucide-react';

interface ShortcutsModalProps {
  isOpen: boolean;
  onClose: () => void;
  theme: 'yaru-dark' | 'yaru-light';
}

export const ShortcutsModal: React.FC<ShortcutsModalProps> = ({
  isOpen,
  onClose,
  theme,
}) => {
  if (!isOpen) return null;

  const isDark = theme === 'yaru-dark';

  const shortcuts = [
    { key: 'Ctrl + B', action: 'Toggle Bold' },
    { key: 'Ctrl + I', action: 'Toggle Italic' },
    { key: 'Ctrl + U', action: 'Toggle Underline' },
    { key: 'Ctrl + S', action: 'Save Document' },
    { key: 'Ctrl + O', action: 'Open File (.md, .txt)' },
    { key: 'Ctrl + N', action: 'New Document Tab' },
    { key: 'Ctrl + F', action: 'Find & Replace' },
    { key: 'Ctrl + +', action: 'Zoom In' },
    { key: 'Ctrl + -', action: 'Zoom Out' },
    { key: 'Ctrl + 0', action: 'Reset Zoom (100%)' },
    { key: 'Ctrl + Wheel', action: 'Zoom In / Out' },
    { key: 'Ctrl + Z', action: 'Undo' },
    { key: 'Ctrl + Y', action: 'Redo' },
    { key: 'Tab', action: 'Indent 2 spaces' },
    { key: 'Shift + Tab', action: 'Outdent' },
    { key: 'Escape', action: 'Close modals / Find bar' },
  ];

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-xs">
      <div
        className={`w-full max-w-lg rounded-xl shadow-2xl border overflow-hidden transition-all ${
          isDark
            ? 'bg-[#222222] border-[#3E3E3E] text-[#dfdbd2]'
            : 'bg-white border-neutral-300 text-neutral-800'
        }`}
      >
        <div
          className={`flex items-center justify-between px-5 py-3.5 border-b ${
            isDark ? 'bg-[#1c1c1c] border-[#333]' : 'bg-[#f5f5f5] border-neutral-300'
          }`}
        >
          <div className="flex items-center gap-2">
            <Keyboard className="w-4 h-4 text-neutral-400 dark:text-neutral-300" />
            <h2 className="font-semibold text-sm">Keyboard Shortcuts</h2>
          </div>
          <button
            type="button"
            onClick={onClose}
            className={`p-1.5 rounded-lg cursor-pointer ${
              isDark ? 'hover:bg-[#333] text-neutral-400' : 'hover:bg-neutral-200 text-neutral-600'
            }`}
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="p-5 space-y-2 text-xs max-h-[60vh] overflow-y-auto">
          <div className="grid grid-cols-2 gap-2">
            {shortcuts.map((s) => (
              <div
                key={s.key}
                className={`flex items-center justify-between p-2 rounded border ${
                  isDark
                    ? 'bg-[#191919] border-[#333]'
                    : 'bg-neutral-50 border-neutral-200'
                }`}
              >
                <span className="text-neutral-400">{s.action}</span>
                <kbd
                  className={`font-mono text-[11px] px-1.5 py-0.5 rounded border shadow-xs font-semibold ${
                    isDark
                      ? 'bg-[#2b2b2b] border-[#444] text-neutral-200'
                      : 'bg-white border-neutral-300 text-neutral-900'
                  }`}
                >
                  {s.key}
                </kbd>
              </div>
            ))}
          </div>

          <div
            className={`mt-4 p-3 rounded-lg border text-xs leading-relaxed ${
              isDark
                ? 'bg-[#1a1a1a] border-[#333] text-neutral-400'
                : 'bg-neutral-100 border-neutral-200 text-neutral-600'
            }`}
          >
            <strong className="text-white block mb-1">WYSIWYG Markdown Formatting</strong>
            In WYSIWYG mode, formatting appears visually without markdown characters like <code>**</code> or <code>#</code> cluttering your text. You can toggle between <strong>WYSIWYG View</strong> and <strong>Raw Markdown</strong> anytime using the button in the top right or bottom bar.
          </div>
        </div>

        <div
          className={`flex items-center justify-end px-5 py-3 border-t text-xs ${
            isDark ? 'bg-[#1c1c1c] border-[#333]' : 'bg-[#f5f5f5] border-neutral-200'
          }`}
        >
          <button
            type="button"
            onClick={onClose}
            className={`px-4 py-1.5 rounded-lg transition-all active:scale-95 cursor-pointer font-medium shadow-xs ${
              isDark
                ? 'bg-neutral-100 text-neutral-950 hover:bg-white'
                : 'bg-neutral-900 text-white hover:bg-neutral-800'
            }`}
          >
            Got it
          </button>
        </div>
      </div>
    </div>
  );
};
