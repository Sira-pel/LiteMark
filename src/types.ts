export interface DocumentTab {
  id: string;
  title: string;
  content: string; // Markdown / Plaintext string
  fileType: 'markdown' | 'plaintext' | 'pdf';
  isModified: boolean;
  savedHandle?: any; // Native FileSystemFileHandle if available
  workspaceFileId?: string; // ID of the note in the folder directory
  lastSavedAt?: Date;
  wordCount: number;
  charCount: number;
  lineCount: number;
  pdfBlobUrl?: string; // Original PDF URL if opened from PDF
  pdfPageCount?: number;
}

export interface WorkspaceFile {
  id: string;
  name: string;
  folderId?: string | null; // null or folder ID
  content: string;
  fileType: 'markdown' | 'plaintext';
  updatedAt: number;
  handle?: any; // Native FileSystemFileHandle if loaded from real disk directory
}

export interface WorkspaceFolder {
  id: string;
  name: string;
  parentId?: string | null;
  isOpen?: boolean;
}

export interface WorkspaceDirectory {
  folderName: string; // e.g. "~/Documents/Notes"
  isLinkedToLocalDisk: boolean;
  folders: WorkspaceFolder[];
  files: WorkspaceFile[];
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
