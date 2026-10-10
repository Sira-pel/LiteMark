export interface DocumentTab {
  id: string;
  title: string;
  content: string; // Markdown / Plaintext string
  fileType: 'markdown' | 'plaintext' | 'pdf';
  isModified: boolean;
  savedHandle?: any; // Native FileSystemFileHandle if available
  lastSavedAt?: Date;
  wordCount: number;
  charCount: number;
  lineCount: number;
  pdfBlobUrl?: string; // Original PDF URL if opened from PDF
  pdfPageCount?: number;
}

export type ThemeMode = 'yaru-dark' | 'yaru-light';
export type ViewMode = 'wysiwyg' | 'source';

export interface EditorSettings {
  theme: ThemeMode;
  fontSize: number;
  fontFamily: 'Ubuntu' | 'Ubuntu Mono';
  wordWrap: boolean;
  showLineNumbers: boolean;
  zenMode: boolean;
  viewMode: ViewMode;
}
