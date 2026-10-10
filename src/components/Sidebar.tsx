import React, { useState, useRef, useEffect } from 'react';
import {
  Folder,
  FolderOpen,
  FolderPlus,
  FilePlus,
  FileText,
  ChevronRight,
  ChevronDown,
  Trash2,
  Edit2,
  Search,
  X,
  HardDrive,
  MoreVertical,
  Check,
  FolderTree,
} from 'lucide-react';
import { WorkspaceDirectory, WorkspaceFile, WorkspaceFolder, ThemeMode } from '../types';

interface SidebarProps {
  workspace: WorkspaceDirectory;
  activeFileId?: string;
  openFileIds: string[];
  theme: ThemeMode;
  onSelectFile: (file: WorkspaceFile) => void;
  onCreateFile: (folderId: string | null, name: string) => void;
  onCreateFolder: (parentId: string | null, name: string) => void;
  onRenameFile: (fileId: string, newName: string) => void;
  onDeleteFile: (fileId: string) => void;
  onRenameFolder: (folderId: string, newName: string) => void;
  onDeleteFolder: (folderId: string) => void;
  onToggleFolder: (folderId: string) => void;
  onLinkLocalDirectory?: () => void;
  onCloseSidebar: () => void;
}

export const Sidebar: React.FC<SidebarProps> = ({
  workspace,
  activeFileId,
  openFileIds,
  theme,
  onSelectFile,
  onCreateFile,
  onCreateFolder,
  onRenameFile,
  onDeleteFile,
  onRenameFolder,
  onDeleteFolder,
  onToggleFolder,
  onLinkLocalDirectory,
  onCloseSidebar,
}) => {
  const isDark = theme === 'yaru-dark';
  const [searchQuery, setSearchQuery] = useState('');
  const [creatingType, setCreatingType] = useState<'file' | 'folder' | null>(null);
  const [targetFolderId, setTargetFolderId] = useState<string | null>(null);
  const [newItemName, setNewItemName] = useState('');

  const [editingItemId, setEditingItemId] = useState<string | null>(null);
  const [editingItemType, setEditingItemType] = useState<'file' | 'folder' | null>(null);
  const [editNameValue, setEditNameValue] = useState('');

  const [activeMenuId, setActiveMenuId] = useState<string | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (creatingType || editingItemId) {
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [creatingType, editingItemId]);

  useEffect(() => {
    const handleOutsideClick = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setActiveMenuId(null);
      }
    };
    document.addEventListener('mousedown', handleOutsideClick);
    return () => document.removeEventListener('mousedown', handleOutsideClick);
  }, []);

  const handleStartCreate = (type: 'file' | 'folder', folderId: string | null = null) => {
    setCreatingType(type);
    setTargetFolderId(folderId);
    setNewItemName(type === 'file' ? 'New Note.md' : 'New Folder');
  };

  const handleCommitCreate = () => {
    const trimmed = newItemName.trim();
    if (trimmed) {
      if (creatingType === 'file') {
        const withExt = trimmed.includes('.') ? trimmed : `${trimmed}.md`;
        onCreateFile(targetFolderId, withExt);
      } else if (creatingType === 'folder') {
        onCreateFolder(targetFolderId, trimmed);
      }
    }
    setCreatingType(null);
    setNewItemName('');
    setTargetFolderId(null);
  };

  const handleStartRename = (type: 'file' | 'folder', id: string, currentName: string) => {
    setEditingItemId(id);
    setEditingItemType(type);
    setEditNameValue(currentName);
    setActiveMenuId(null);
  };

  const handleCommitRename = () => {
    const trimmed = editNameValue.trim();
    if (trimmed && editingItemId) {
      if (editingItemType === 'file') {
        const withExt = trimmed.includes('.') ? trimmed : `${trimmed}.md`;
        onRenameFile(editingItemId, withExt);
      } else if (editingItemType === 'folder') {
        onRenameFolder(editingItemId, trimmed);
      }
    }
    setEditingItemId(null);
    setEditingItemType(null);
    setEditNameValue('');
  };

  // Filter files based on search
  const filteredFiles = searchQuery.trim()
    ? workspace.files.filter((f) =>
        f.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
        f.content.toLowerCase().includes(searchQuery.toLowerCase())
      )
    : null;

  return (
    <aside
      className={`w-64 sm:w-72 border-r flex flex-col h-full select-none shrink-0 transition-colors z-20 ${
        isDark ? 'bg-[#202020] border-[#333333] text-[#dfdbd2]' : 'bg-[#f6f6f6] border-neutral-300 text-neutral-800'
      }`}
      aria-label="Notes Folder Directory"
    >
      {/* Sidebar Header */}
      <div
        className={`px-3 py-2.5 border-b flex items-center justify-between ${
          isDark ? 'border-[#333333] bg-[#1a1a1a]' : 'border-neutral-200 bg-[#ececec]'
        }`}
      >
        <button
          type="button"
          onClick={onLinkLocalDirectory}
          className="flex items-center gap-2 min-w-0 text-left group/dir cursor-pointer hover:opacity-90 transition-opacity flex-1 mr-1.5"
          title="Click to choose or change notes directory"
        >
          <div className="p-1 rounded bg-[#E95420]/15 text-[#E95420] group-hover/dir:bg-[#E95420]/25 shrink-0 transition-colors">
            <FolderTree className="w-4 h-4" />
          </div>
          <div className="min-w-0 flex-1">
            <h2 className="text-xs font-bold truncate leading-tight group-hover/dir:text-[#E95420] transition-colors">
              {workspace.folderName}
            </h2>
            <div className="text-[10px] text-neutral-400 truncate flex items-center gap-1">
              {workspace.isLinkedToLocalDisk ? (
                <span className="text-emerald-500 font-medium">● Local Disk Linked</span>
              ) : (
                <span className="group-hover/dir:underline">Click to choose directory</span>
              )}
            </div>
          </div>
        </button>

        {/* Action icons */}
        <div className="flex items-center gap-0.5 shrink-0">
          <button
            type="button"
            onClick={() => handleStartCreate('file', null)}
            className={`p-1 rounded transition-colors cursor-pointer ${
              isDark ? 'text-neutral-400 hover:text-white hover:bg-white/10' : 'text-neutral-600 hover:text-black hover:bg-black/10'
            }`}
            title="New Note in root directory"
          >
            <FilePlus className="w-3.5 h-3.5" />
          </button>
          <button
            type="button"
            onClick={() => handleStartCreate('folder', null)}
            className={`p-1 rounded transition-colors cursor-pointer ${
              isDark ? 'text-neutral-400 hover:text-white hover:bg-white/10' : 'text-neutral-600 hover:text-black hover:bg-black/10'
            }`}
            title="New Subfolder"
          >
            <FolderPlus className="w-3.5 h-3.5" />
          </button>
          {onLinkLocalDirectory && (
            <button
              type="button"
              onClick={onLinkLocalDirectory}
              className={`p-1 rounded transition-colors cursor-pointer ${
                isDark ? 'text-neutral-400 hover:text-[#E95420] hover:bg-white/10' : 'text-neutral-600 hover:text-[#E95420] hover:bg-black/10'
              }`}
              title="Choose Big Directory / Open Folder on Computer (Disk)"
            >
              <HardDrive className="w-3.5 h-3.5" />
            </button>
          )}
          <button
            type="button"
            onClick={onCloseSidebar}
            className={`p-1 rounded transition-colors cursor-pointer ml-1 ${
              isDark ? 'text-neutral-400 hover:text-white hover:bg-white/10' : 'text-neutral-600 hover:text-black hover:bg-black/10'
            }`}
            title="Close sidebar"
          >
            <X className="w-3.5 h-3.5" />
          </button>
        </div>
      </div>

      {/* Search Input */}
      <div className={`p-2 border-b ${isDark ? 'border-[#333333]' : 'border-neutral-200'}`}>
        <div className="relative flex items-center">
          <Search className="w-3.5 h-3.5 absolute left-2 text-neutral-400 pointer-events-none" />
          <input
            type="text"
            placeholder="Search notes in directory..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className={`w-full pl-7 pr-6 py-1 text-xs rounded border transition-colors outline-none ${
              isDark
                ? 'bg-[#181818] border-[#383838] text-white placeholder-neutral-500 focus:border-[#E95420]'
                : 'bg-white border-neutral-300 text-neutral-900 placeholder-neutral-400 focus:border-[#E95420]'
            }`}
          />
          {searchQuery && (
            <button
              type="button"
              onClick={() => setSearchQuery('')}
              className="absolute right-1.5 text-neutral-400 hover:text-white p-0.5 rounded cursor-pointer"
            >
              <X className="w-3 h-3" />
            </button>
          )}
        </div>
      </div>

      {/* Directory Content List */}
      <div className="flex-1 overflow-y-auto p-1.5 space-y-0.5 text-xs">
        {/* Inline item creation at root */}
        {creatingType && targetFolderId === null && (
          <div
            className={`flex items-center gap-1.5 px-2 py-1 rounded border mb-1 ${
              isDark ? 'bg-[#2a2a2a] border-[#E95420]' : 'bg-white border-[#E95420]'
            }`}
          >
            {creatingType === 'file' ? (
              <FileText className="w-3.5 h-3.5 text-[#E95420] shrink-0" />
            ) : (
              <Folder className="w-3.5 h-3.5 text-[#E95420] shrink-0" />
            )}
            <input
              ref={inputRef}
              type="text"
              value={newItemName}
              onChange={(e) => setNewItemName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') handleCommitCreate();
                if (e.key === 'Escape') setCreatingType(null);
              }}
              className="flex-1 bg-transparent border-none outline-none text-xs"
              placeholder={creatingType === 'file' ? 'Note name...' : 'Folder name...'}
            />
            <button
              type="button"
              onClick={handleCommitCreate}
              className="p-0.5 text-emerald-400 hover:text-emerald-300 cursor-pointer"
              title="Create"
            >
              <Check className="w-3.5 h-3.5" />
            </button>
            <button
              type="button"
              onClick={() => setCreatingType(null)}
              className="p-0.5 text-neutral-400 hover:text-neutral-200 cursor-pointer"
              title="Cancel"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          </div>
        )}

        {/* Search Results Mode */}
        {filteredFiles !== null ? (
          <div>
            <div className="px-2 py-1 text-[11px] font-semibold text-neutral-400 uppercase tracking-wider">
              Search Results ({filteredFiles.length})
            </div>
            {filteredFiles.length === 0 ? (
              <div className="px-3 py-4 text-center text-xs text-neutral-400">
                No notes found matching "{searchQuery}"
              </div>
            ) : (
              filteredFiles.map((file) => (
                <FileRow
                  key={file.id}
                  file={file}
                  workspace={workspace}
                  isDark={isDark}
                  isActive={file.id === activeFileId}
                  isOpenInTab={openFileIds.includes(file.id)}
                  editingItemId={editingItemId}
                  editNameValue={editNameValue}
                  setEditNameValue={setEditNameValue}
                  inputRef={inputRef}
                  handleCommitRename={handleCommitRename}
                  onSelectFile={onSelectFile}
                  handleStartRename={handleStartRename}
                  onDeleteFile={onDeleteFile}
                  activeMenuId={activeMenuId}
                  setActiveMenuId={setActiveMenuId}
                  menuRef={menuRef}
                />
              ))
            )}
          </div>
        ) : (
          /* Normal Directory Tree Mode */
          <>
            {/* Top Level Folders */}
            {workspace.folders
              .filter((folder) => !folder.parentId)
              .map((folder) => (
                <FolderGroup
                  key={folder.id}
                  folder={folder}
                  workspace={workspace}
                  isDark={isDark}
                  activeFileId={activeFileId}
                  openFileIds={openFileIds}
                  creatingType={creatingType}
                  targetFolderId={targetFolderId}
                  newItemName={newItemName}
                  setNewItemName={setNewItemName}
                  editingItemId={editingItemId}
                  editNameValue={editNameValue}
                  setEditNameValue={setEditNameValue}
                  inputRef={inputRef}
                  handleStartCreate={handleStartCreate}
                  handleCommitCreate={handleCommitCreate}
                  setCreatingType={setCreatingType}
                  handleStartRename={handleStartRename}
                  handleCommitRename={handleCommitRename}
                  onDeleteFolder={onDeleteFolder}
                  onToggleFolder={onToggleFolder}
                  onSelectFile={onSelectFile}
                  onDeleteFile={onDeleteFile}
                  activeMenuId={activeMenuId}
                  setActiveMenuId={setActiveMenuId}
                  menuRef={menuRef}
                />
              ))}

            {/* Root Level Files */}
            {workspace.files
              .filter((file) => !file.folderId)
              .map((file) => (
                <FileRow
                  key={file.id}
                  file={file}
                  workspace={workspace}
                  isDark={isDark}
                  isActive={file.id === activeFileId}
                  isOpenInTab={openFileIds.includes(file.id)}
                  editingItemId={editingItemId}
                  editNameValue={editNameValue}
                  setEditNameValue={setEditNameValue}
                  inputRef={inputRef}
                  handleCommitRename={handleCommitRename}
                  onSelectFile={onSelectFile}
                  handleStartRename={handleStartRename}
                  onDeleteFile={onDeleteFile}
                  activeMenuId={activeMenuId}
                  setActiveMenuId={setActiveMenuId}
                  menuRef={menuRef}
                />
              ))}

            {workspace.files.length === 0 && workspace.folders.length === 0 && (
              <div className="px-3 py-6 text-center text-xs text-neutral-400">
                <p>Notes directory is empty.</p>
                <button
                  type="button"
                  onClick={() => handleStartCreate('file', null)}
                  className="mt-2 text-xs text-[#E95420] hover:underline font-medium cursor-pointer"
                >
                  + Create your first note
                </button>
              </div>
            )}
          </>
        )}
      </div>

      {/* Sidebar Footer info */}
      <div
        className={`px-3 py-2 border-t text-[11px] flex items-center justify-between ${
          isDark ? 'border-[#333333] bg-[#1a1a1a] text-neutral-400' : 'border-neutral-200 bg-[#ececec] text-neutral-600'
        }`}
      >
        <span>
          {workspace.files.length} {workspace.files.length === 1 ? 'note' : 'notes'} • {workspace.folders.length} {workspace.folders.length === 1 ? 'folder' : 'folders'}
        </span>
        {onLinkLocalDirectory && (
          <button
            type="button"
            onClick={onLinkLocalDirectory}
            className="text-[10px] text-[#E95420] hover:underline font-medium cursor-pointer"
          >
            {workspace.isLinkedToLocalDisk ? 'Change Folder' : 'Choose Directory'}
          </button>
        )}
      </div>
    </aside>
  );
};

/* Folder Item & Recursive Children */
interface FolderGroupProps {
  folder: WorkspaceFolder;
  workspace: WorkspaceDirectory;
  isDark: boolean;
  activeFileId?: string;
  openFileIds: string[];
  creatingType: 'file' | 'folder' | null;
  targetFolderId: string | null;
  newItemName: string;
  setNewItemName: (val: string) => void;
  editingItemId: string | null;
  editNameValue: string;
  setEditNameValue: (val: string) => void;
  inputRef: React.RefObject<HTMLInputElement | null>;
  handleStartCreate: (type: 'file' | 'folder', folderId: string | null) => void;
  handleCommitCreate: () => void;
  setCreatingType: (val: any) => void;
  handleStartRename: (type: 'file' | 'folder', id: string, name: string) => void;
  handleCommitRename: () => void;
  onDeleteFolder: (id: string) => void;
  onToggleFolder: (id: string) => void;
  onSelectFile: (file: WorkspaceFile) => void;
  onDeleteFile: (id: string) => void;
  activeMenuId: string | null;
  setActiveMenuId: (id: string | null) => void;
  menuRef: React.RefObject<HTMLDivElement | null>;
}

const FolderGroup: React.FC<FolderGroupProps> = ({
  folder,
  workspace,
  isDark,
  activeFileId,
  openFileIds,
  creatingType,
  targetFolderId,
  newItemName,
  setNewItemName,
  editingItemId,
  editNameValue,
  setEditNameValue,
  inputRef,
  handleStartCreate,
  handleCommitCreate,
  setCreatingType,
  handleStartRename,
  handleCommitRename,
  onDeleteFolder,
  onToggleFolder,
  onSelectFile,
  onDeleteFile,
  activeMenuId,
  setActiveMenuId,
  menuRef,
}) => {
  const isEditing = editingItemId === folder.id;
  const isMenuOpen = activeMenuId === `folder-${folder.id}`;
  const isOpen = folder.isOpen !== false;
  const childFiles = workspace.files.filter((f) => f.folderId === folder.id);

  return (
    <div className="space-y-0.5">
      {/* Folder Header */}
      <div
        className={`group flex items-center justify-between px-2 py-1 rounded cursor-pointer transition-colors relative ${
          isDark ? 'hover:bg-white/5 text-[#dfdbd2]' : 'hover:bg-black/5 text-neutral-800'
        }`}
        onClick={() => onToggleFolder(folder.id)}
      >
        <div className="flex items-center gap-1.5 min-w-0 flex-1">
          <button
            type="button"
            className="p-0.5 text-neutral-400 hover:text-white cursor-pointer"
            onClick={(e) => {
              e.stopPropagation();
              onToggleFolder(folder.id);
            }}
          >
            {isOpen ? <ChevronDown className="w-3.5 h-3.5" /> : <ChevronRight className="w-3.5 h-3.5" />}
          </button>

          {isOpen ? (
            <FolderOpen className="w-3.5 h-3.5 text-[#E95420] shrink-0" />
          ) : (
            <Folder className="w-3.5 h-3.5 text-[#E95420] shrink-0" />
          )}

          {isEditing ? (
            <input
              ref={inputRef}
              type="text"
              value={editNameValue}
              onChange={(e) => setEditNameValue(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') handleCommitRename();
                if (e.key === 'Escape') handleStartRename('folder', '', '');
              }}
              onBlur={handleCommitRename}
              onClick={(e) => e.stopPropagation()}
              className="flex-1 bg-transparent border-b border-[#E95420] outline-none text-xs"
            />
          ) : (
            <span className="font-semibold text-xs truncate">{folder.name}</span>
          )}
        </div>

        {/* Hover / Actions */}
        <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              handleStartCreate('file', folder.id);
            }}
            className="p-1 rounded hover:bg-neutral-500/20 text-neutral-400 hover:text-white cursor-pointer"
            title="New note inside this folder"
          >
            <FilePlus className="w-3 h-3" />
          </button>
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              setActiveMenuId(isMenuOpen ? null : `folder-${folder.id}`);
            }}
            className="p-1 rounded hover:bg-neutral-500/20 text-neutral-400 hover:text-white cursor-pointer"
            title="Folder actions"
          >
            <MoreVertical className="w-3 h-3" />
          </button>
        </div>

        {/* Dropdown Menu */}
        {isMenuOpen && (
          <div
            ref={menuRef}
            onClick={(e) => e.stopPropagation()}
            className={`absolute right-1 top-full mt-1 z-30 w-36 rounded-md shadow-xl border py-1 text-xs ${
              isDark ? 'bg-[#292929] border-[#444] text-[#dfdbd2]' : 'bg-white border-neutral-300 text-neutral-800'
            }`}
          >
            <button
              type="button"
              onClick={() => {
                handleStartCreate('file', folder.id);
                setActiveMenuId(null);
              }}
              className="w-full text-left px-2.5 py-1.5 flex items-center gap-2 hover:bg-[#E95420] hover:text-white cursor-pointer"
            >
              <FilePlus className="w-3.5 h-3.5" />
              <span>Add Note</span>
            </button>
            <button
              type="button"
              onClick={() => handleStartRename('folder', folder.id, folder.name)}
              className="w-full text-left px-2.5 py-1.5 flex items-center gap-2 hover:bg-[#E95420] hover:text-white cursor-pointer"
            >
              <Edit2 className="w-3.5 h-3.5" />
              <span>Rename</span>
            </button>
            <button
              type="button"
              onClick={() => {
                if (confirm(`Delete folder "${folder.name}" and its notes?`)) {
                  onDeleteFolder(folder.id);
                }
                setActiveMenuId(null);
              }}
              className="w-full text-left px-2.5 py-1.5 flex items-center gap-2 hover:bg-red-600 hover:text-white text-red-400 cursor-pointer"
            >
              <Trash2 className="w-3.5 h-3.5" />
              <span>Delete</span>
            </button>
          </div>
        )}
      </div>

      {/* Children files & subfolders when open */}
      {isOpen && (
        <div className="pl-4 space-y-0.5 border-l border-neutral-700/30 ml-2.5">
          {/* Creating inside this folder */}
          {creatingType && targetFolderId === folder.id && (
            <div
              className={`flex items-center gap-1.5 px-2 py-1 rounded border my-1 ${
                isDark ? 'bg-[#2a2a2a] border-[#E95420]' : 'bg-white border-[#E95420]'
              }`}
            >
              <FileText className="w-3.5 h-3.5 text-[#E95420] shrink-0" />
              <input
                ref={inputRef}
                type="text"
                value={newItemName}
                onChange={(e) => setNewItemName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') handleCommitCreate();
                  if (e.key === 'Escape') setCreatingType(null);
                }}
                className="flex-1 bg-transparent border-none outline-none text-xs"
                placeholder="Note name..."
              />
              <button
                type="button"
                onClick={handleCommitCreate}
                className="p-0.5 text-emerald-400 hover:text-emerald-300 cursor-pointer"
              >
                <Check className="w-3 h-3" />
              </button>
              <button
                type="button"
                onClick={() => setCreatingType(null)}
                className="p-0.5 text-neutral-400 hover:text-neutral-200 cursor-pointer"
              >
                <X className="w-3 h-3" />
              </button>
            </div>
          )}

          {childFiles.map((file) => (
            <FileRow
              key={file.id}
              file={file}
              workspace={workspace}
              isDark={isDark}
              isActive={file.id === activeFileId}
              isOpenInTab={openFileIds.includes(file.id)}
              editingItemId={editingItemId}
              editNameValue={editNameValue}
              setEditNameValue={setEditNameValue}
              inputRef={inputRef}
              handleCommitRename={handleCommitRename}
              onSelectFile={onSelectFile}
              handleStartRename={handleStartRename}
              onDeleteFile={onDeleteFile}
              activeMenuId={activeMenuId}
              setActiveMenuId={setActiveMenuId}
              menuRef={menuRef}
            />
          ))}

          {childFiles.length === 0 && !creatingType && (
            <div className="px-2 py-1 text-[11px] text-neutral-500 italic flex items-center gap-1">
              <span>Empty folder.</span>
              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation();
                  handleStartCreate('file', folder.id);
                }}
                className="text-[#E95420] hover:underline cursor-pointer not-italic font-medium"
              >
                + Add note
              </button>
            </div>
          )}
        </div>
      )}
    </div>
  );
};

/* Individual Note / File Row */
interface FileRowProps {
  file: WorkspaceFile;
  workspace: WorkspaceDirectory;
  isDark: boolean;
  isActive: boolean;
  isOpenInTab: boolean;
  editingItemId: string | null;
  editNameValue: string;
  setEditNameValue: (val: string) => void;
  inputRef: React.RefObject<HTMLInputElement | null>;
  handleCommitRename: () => void;
  onSelectFile: (file: WorkspaceFile) => void;
  handleStartRename: (type: 'file' | 'folder', id: string, name: string) => void;
  onDeleteFile: (id: string) => void;
  activeMenuId: string | null;
  setActiveMenuId: (id: string | null) => void;
  menuRef: React.RefObject<HTMLDivElement | null>;
}

const FileRow: React.FC<FileRowProps> = ({
  file,
  workspace,
  isDark,
  isActive,
  isOpenInTab,
  editingItemId,
  editNameValue,
  setEditNameValue,
  inputRef,
  handleCommitRename,
  onSelectFile,
  handleStartRename,
  onDeleteFile,
  activeMenuId,
  setActiveMenuId,
  menuRef,
}) => {
  const isEditing = editingItemId === file.id;
  const isMenuOpen = activeMenuId === `file-${file.id}`;

  return (
    <div
      onClick={() => !isEditing && onSelectFile(file)}
      className={`group flex items-center justify-between px-2 py-1.5 rounded cursor-pointer transition-colors relative ${
        isActive
          ? isDark
            ? 'bg-[#E95420]/20 text-white font-medium border-l-2 border-[#E95420]'
            : 'bg-[#E95420]/15 text-neutral-900 font-medium border-l-2 border-[#E95420]'
          : isDark
          ? 'hover:bg-white/5 text-[#c8c4bc]'
          : 'hover:bg-black/5 text-neutral-700'
      }`}
    >
      <div className="flex items-center gap-1.5 min-w-0 flex-1">
        <FileText
          className={`w-3.5 h-3.5 shrink-0 ${
            isActive ? 'text-[#E95420]' : 'text-neutral-400 group-hover:text-neutral-300'
          }`}
        />
        {isEditing ? (
          <input
            ref={inputRef}
            type="text"
            value={editNameValue}
            onChange={(e) => setEditNameValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') handleCommitRename();
              if (e.key === 'Escape') handleStartRename('file', '', '');
            }}
            onBlur={handleCommitRename}
            onClick={(e) => e.stopPropagation()}
            className="flex-1 bg-transparent border-b border-[#E95420] outline-none text-xs"
          />
        ) : (
          <span className="truncate flex-1">{file.name}</span>
        )}
      </div>

      {/* Right indicators & actions */}
      <div className="flex items-center gap-1 shrink-0">
        {isOpenInTab && !isEditing && (
          <span
            className="w-1.5 h-1.5 rounded-full bg-emerald-500"
            title="Currently open in editor tab"
          />
        )}

        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            setActiveMenuId(isMenuOpen ? null : `file-${file.id}`);
          }}
          className="p-1 rounded opacity-0 group-hover:opacity-100 hover:bg-neutral-500/20 text-neutral-400 hover:text-white cursor-pointer transition-opacity"
          title="Note options"
        >
          <MoreVertical className="w-3 h-3" />
        </button>
      </div>

      {/* Context Menu */}
      {isMenuOpen && (
        <div
          ref={menuRef}
          onClick={(e) => e.stopPropagation()}
          className={`absolute right-1 top-full mt-1 z-30 w-36 rounded-md shadow-xl border py-1 text-xs ${
            isDark ? 'bg-[#292929] border-[#444] text-[#dfdbd2]' : 'bg-white border-neutral-300 text-neutral-800'
          }`}
        >
          <button
            type="button"
            onClick={() => {
              onSelectFile(file);
              setActiveMenuId(null);
            }}
            className="w-full text-left px-2.5 py-1.5 flex items-center gap-2 hover:bg-[#E95420] hover:text-white cursor-pointer"
          >
            <FileText className="w-3.5 h-3.5" />
            <span>Open in Tab</span>
          </button>
          <button
            type="button"
            onClick={() => handleStartRename('file', file.id, file.name)}
            className="w-full text-left px-2.5 py-1.5 flex items-center gap-2 hover:bg-[#E95420] hover:text-white cursor-pointer"
          >
            <Edit2 className="w-3.5 h-3.5" />
            <span>Rename</span>
          </button>
          <button
            type="button"
            onClick={() => {
              if (confirm(`Delete note "${file.name}"?`)) {
                onDeleteFile(file.id);
              }
              setActiveMenuId(null);
            }}
            className="w-full text-left px-2.5 py-1.5 flex items-center gap-2 hover:bg-red-600 hover:text-white text-red-400 cursor-pointer"
          >
            <Trash2 className="w-3.5 h-3.5" />
            <span>Delete</span>
          </button>
        </div>
      )}
    </div>
  );
};
