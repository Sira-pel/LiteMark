import React from 'react';
import { X, Download, FileText, ExternalLink } from 'lucide-react';

interface PdfPreviewModalProps {
  isOpen: boolean;
  onClose: () => void;
  title: string;
  pdfUrl?: string;
  theme: 'yaru-dark' | 'yaru-light';
  pageCount?: number;
}

export const PdfPreviewModal: React.FC<PdfPreviewModalProps> = ({
  isOpen,
  onClose,
  title,
  pdfUrl,
  theme,
  pageCount,
}) => {
  if (!isOpen || !pdfUrl) return null;

  const isDark = theme === 'yaru-dark';

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-2 sm:p-4 bg-black/70 backdrop-blur-xs">
      <div
        className={`w-full max-w-5xl h-[90vh] flex flex-col rounded-xl shadow-2xl border overflow-hidden transition-all ${
          isDark
            ? 'bg-[#222222] border-[#3E3E3E] text-[#dfdbd2]'
            : 'bg-white border-neutral-300 text-neutral-800'
        }`}
      >
        {/* Header */}
        <div
          className={`flex items-center justify-between px-4 py-3 border-b shrink-0 ${
            isDark ? 'bg-[#1c1c1c] border-[#333]' : 'bg-[#f5f5f5] border-neutral-300'
          }`}
        >
          <div className="flex items-center gap-2.5 truncate">
            <div className="p-1.5 rounded-lg bg-red-500/20 text-red-500 shrink-0">
              <FileText className="w-4 h-4" />
            </div>
            <div className="truncate">
              <h2 className="font-semibold text-xs sm:text-sm truncate">{title}</h2>
              <p className="text-[11px] text-neutral-400">
                Original PDF Document {pageCount ? `(${pageCount} pages)` : ''}
              </p>
            </div>
          </div>

          <div className="flex items-center gap-2 shrink-0">
            <a
              href={pdfUrl}
              download={title}
              className={`p-1.5 rounded-lg cursor-pointer transition-colors ${
                isDark ? 'hover:bg-[#333] text-neutral-300' : 'hover:bg-neutral-200 text-neutral-700'
              }`}
              title="Download original PDF"
            >
              <Download className="w-4 h-4" />
            </a>
            <a
              href={pdfUrl}
              target="_blank"
              rel="noopener noreferrer"
              className={`p-1.5 rounded-lg cursor-pointer transition-colors ${
                isDark ? 'hover:bg-[#333] text-neutral-300' : 'hover:bg-neutral-200 text-neutral-700'
              }`}
              title="Open in new browser tab"
            >
              <ExternalLink className="w-4 h-4" />
            </a>
            <button
              type="button"
              onClick={onClose}
              className={`p-1.5 rounded-lg cursor-pointer transition-colors ${
                isDark ? 'hover:bg-[#333] text-neutral-400' : 'hover:bg-neutral-200 text-neutral-600'
              }`}
            >
              <X className="w-4 h-4" />
            </button>
          </div>
        </div>

        {/* PDF Frame */}
        <div className="flex-1 w-full h-full bg-[#1e1e1e] relative">
          <iframe
            src={`${pdfUrl}#toolbar=1`}
            title={title}
            className="w-full h-full border-none"
          />
        </div>
      </div>
    </div>
  );
};
