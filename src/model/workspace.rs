use super::document::FileType;
use std::hash::Hasher;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// What a tab's file looked like on disk the last time LiteMark read or
/// wrote it. A save compares it with the file now on disk to notice that
/// another program (a sync client, `git pull`, a second editor) changed or
/// deleted the file in between.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskVersion {
    /// Byte length. A file of another size changed without being read.
    pub len: u64,
    /// Hash of the bytes. A save reads the file and compares, so a `touch`
    /// or a copy that kept the text doesn't count as a change, and a
    /// same-size edit that kept the timestamp still does.
    pub content_hash: u64,
}

impl DiskVersion {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            len: bytes.len() as u64,
            content_hash: content_hash(bytes),
        }
    }

    /// Hash `content` (with a leading UTF-8 BOM when `has_bom` is set) without
    /// allocating a BOM-prefixed copy of the document.
    pub fn from_parts(has_bom: bool, content: &str) -> Self {
        const BOM: &[u8] = "\u{feff}".as_bytes();
        let total_len = content.len() + if has_bom { BOM.len() } else { 0 };
        let mut hasher = ContentHasher::new(total_len as u64);
        if has_bom {
            hasher.update(BOM);
        }
        hasher.update(content.as_bytes());
        Self {
            len: total_len as u64,
            content_hash: hasher.finish(),
        }
    }
}

/// How the file behind a tab compares with what the tab last saw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskChange {
    Unchanged,
    Changed,
    Deleted,
}

/// Stable within one run of the app, which is all a tab needs.
pub fn content_hash(bytes: &[u8]) -> u64 {
    let mut hasher = ContentHasher::new(bytes.len() as u64);
    hasher.update(bytes);
    hasher.finish()
}

/// [`content_hash`] fed in pieces, so a file can be hashed while it is read.
/// The result depends only on the bytes, not on how they were split.
pub struct ContentHasher(std::collections::hash_map::DefaultHasher);

impl ContentHasher {
    pub fn new(len: u64) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        hasher.write_u64(len);
        Self(hasher)
    }

    pub fn update(&mut self, bytes: &[u8]) {
        self.0.write(bytes);
    }

    pub fn finish(&self) -> u64 {
        self.0.finish()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceFile {
    pub id: String,
    pub name: String,
    pub folder_id: Option<Arc<str>>,
    pub path: PathBuf,
    pub file_type: FileType,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceFolder {
    pub id: Arc<str>,
    pub name: String,
    pub parent_id: Option<Arc<str>>,
    pub path: PathBuf,
    pub is_open: bool,
}

/// A value that knows when it may have changed: every mutable access (and
/// every new value) takes a fresh generation, so a cache keyed by
/// [`Tracked::generation`] can't miss a change made anywhere.
#[derive(Debug)]
pub struct Tracked<T> {
    value: T,
    generation: u64,
}

fn next_tracked_generation() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

impl<T> Tracked<T> {
    pub fn generation(&self) -> u64 {
        self.generation
    }
}

impl<T> From<T> for Tracked<T> {
    fn from(value: T) -> Self {
        Self {
            value,
            generation: next_tracked_generation(),
        }
    }
}

impl<T> std::ops::Deref for Tracked<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> std::ops::DerefMut for Tracked<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.generation = next_tracked_generation();
        &mut self.value
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceDirectory {
    pub folder_name: String,
    pub folder_path: PathBuf,
    pub is_linked_to_local_disk: bool,
    pub folders: Vec<WorkspaceFolder>,
    pub files: Vec<WorkspaceFile>,
    /// The scan stopped at its depth or item limit, so some of the folder
    /// isn't listed.
    pub scan_truncated: bool,
}

/// Default notes folder under `base`: an existing `Notes` folder keeps being
/// used so earlier installs don't lose their notes; fresh installs get `LiteMark`.
pub fn default_notes_dir(base: &Path) -> PathBuf {
    let legacy = base.join("Notes");
    if legacy.is_dir() {
        legacy
    } else {
        base.join("LiteMark")
    }
}

impl Default for WorkspaceDirectory {
    fn default() -> Self {
        let home = dirs::document_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Documents"));
        let notes = default_notes_dir(&home);
        let name = notes
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("LiteMark");
        Self {
            folder_name: format!("~/Documents/{name}"),
            folder_path: notes,
            is_linked_to_local_disk: true,
            folders: Vec::new(),
            files: Vec::new(),
            scan_truncated: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::sidebar::{build_sidebar_tree, SidebarCreate, SidebarTreeItem};

    fn make_folder(
        id: &str,
        name: &str,
        parent_id: Option<&str>,
        is_open: bool,
    ) -> WorkspaceFolder {
        WorkspaceFolder {
            id: Arc::from(id),
            name: name.to_string(),
            parent_id: parent_id.map(Arc::from),
            path: PathBuf::from(format!("/tmp/{}", name)),
            is_open,
        }
    }

    fn make_file(id: &str, name: &str, folder_id: Option<&str>) -> WorkspaceFile {
        WorkspaceFile {
            id: id.to_string(),
            name: name.to_string(),
            folder_id: folder_id.map(Arc::from),
            path: PathBuf::from(format!("/tmp/{}", name)),
            file_type: FileType::Markdown,
        }
    }

    /// The tree builder before it was made linear, kept to check the new one.
    fn build_sidebar_tree_reference(
        folders: &[WorkspaceFolder],
        files: &[WorkspaceFile],
        creating: Option<&SidebarCreate>,
    ) -> Vec<SidebarTreeItem> {
        let mut items = Vec::new();
        let folder_ids: std::collections::HashSet<&str> =
            folders.iter().map(|f| f.id.as_ref()).collect();

        fn collect_level(
            parent_id: Option<&str>,
            depth: u16,
            folders: &[WorkspaceFolder],
            files: &[WorkspaceFile],
            folder_ids: &std::collections::HashSet<&str>,
            creating: Option<(Option<&str>, bool)>,
            out: &mut Vec<SidebarTreeItem>,
        ) {
            // The inline create editor sits at the top of the level it targets.
            if let Some((create_parent, is_folder)) = creating {
                if create_parent == parent_id {
                    out.push(SidebarTreeItem::CreateInput { depth, is_folder });
                }
            }
            let mut level_folders: Vec<(u32, &WorkspaceFolder)> = folders
                .iter()
                .enumerate()
                .filter(|(_, f)| {
                    if let Some(pid) = parent_id {
                        f.parent_id.as_deref() == Some(pid)
                    } else {
                        f.parent_id.is_none()
                            || f.parent_id
                                .as_deref()
                                .is_some_and(|p| !folder_ids.contains(p))
                    }
                })
                .map(|(idx, f)| (idx as u32, f))
                .collect();
            level_folders.sort_by_cached_key(|(_, a)| a.name.to_lowercase());

            for (index, folder) in level_folders {
                let is_open = folder.is_open;
                let fid = folder.id.clone();
                out.push(SidebarTreeItem::Folder { index, depth });
                if is_open {
                    collect_level(
                        Some(fid.as_ref()),
                        depth.saturating_add(1),
                        folders,
                        files,
                        folder_ids,
                        creating,
                        out,
                    );
                }
            }

            let mut level_files: Vec<(u32, &WorkspaceFile)> = files
                .iter()
                .enumerate()
                .filter(|(_, f)| {
                    if let Some(pid) = parent_id {
                        f.folder_id.as_deref() == Some(pid)
                    } else {
                        f.folder_id.is_none()
                            || f.folder_id
                                .as_deref()
                                .is_some_and(|p| !folder_ids.contains(p))
                    }
                })
                .map(|(idx, f)| (idx as u32, f))
                .collect();
            level_files.sort_by_cached_key(|(_, a)| a.name.to_lowercase());

            for (index, _) in level_files {
                out.push(SidebarTreeItem::File { index, depth });
            }
        }

        collect_level(
            None,
            0,
            folders,
            files,
            &folder_ids,
            creating.map(|c| (c.parent.as_deref(), c.is_folder)),
            &mut items,
        );
        items
    }

    #[test]
    fn sidebar_tree_matches_the_old_builder_on_random_workspaces() {
        // Small LCG so the cases are the same on every run.
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = |n: usize| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((seed >> 33) as usize) % n
        };
        let names = ["b", "A", "a", "c", "B", "Notes", "notes", "z"];
        for case in 0..300 {
            let folder_count = next(8);
            let mut folders = Vec::new();
            for i in 0..folder_count {
                // Parents only point backwards (a real scan has no cycles);
                // some point at ids that don't exist, which land at the root.
                let parent = match next(4) {
                    0 if i > 0 => Some(format!("d{}", next(i))),
                    1 => Some("missing".to_string()),
                    _ => None,
                };
                folders.push(make_folder(
                    &format!("d{i}"),
                    names[next(names.len())],
                    parent.as_deref(),
                    next(3) != 0,
                ));
            }
            let mut files = Vec::new();
            for i in 0..next(12) {
                let folder = match next(3) {
                    0 if folder_count > 0 => Some(format!("d{}", next(folder_count))),
                    1 => Some("missing".to_string()),
                    _ => None,
                };
                files.push(make_file(
                    &format!("f{i}"),
                    names[next(names.len())],
                    folder.as_deref(),
                ));
            }
            let creating = match next(3) {
                0 => None,
                1 => Some(SidebarCreate {
                    parent: None,
                    is_folder: next(2) == 0,
                }),
                _ => Some(SidebarCreate {
                    parent: Some(format!("d{}", next(folder_count.max(1)))),
                    is_folder: next(2) == 0,
                }),
            };
            assert_eq!(
                build_sidebar_tree(&folders, &files, creating.as_ref()),
                build_sidebar_tree_reference(&folders, &files, creating.as_ref()),
                "case {case}"
            );
        }
    }

    #[test]
    fn default_notes_dir_keeps_an_existing_notes_folder() {
        let base = std::env::temp_dir().join(format!("litemark-default-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        assert_eq!(default_notes_dir(&base), base.join("LiteMark"));
        std::fs::create_dir(base.join("Notes")).unwrap();
        assert_eq!(default_notes_dir(&base), base.join("Notes"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_build_sidebar_tree_empty() {
        let tree = build_sidebar_tree(&[], &[], None);
        assert!(tree.is_empty());
    }

    #[test]
    fn test_build_sidebar_tree_folder_without_files() {
        let folders = vec![make_folder("f1", "New Folder", None, true)];
        let tree = build_sidebar_tree(&folders, &[], None);
        assert_eq!(tree.len(), 1);
        match tree[0] {
            SidebarTreeItem::Folder { index, depth } => {
                assert_eq!(folders[index as usize].name, "New Folder");
                assert_eq!(depth, 0);
            }
            _ => panic!("Expected folder item"),
        }
    }

    #[test]
    fn test_build_sidebar_tree_nested_and_open_close() {
        let root_folder = make_folder("f_root", "Docs", None, true);
        let sub_folder = make_folder("f_sub", "Guides", Some("f_root"), true);
        let sub_file = make_file("doc1", "Setup.md", Some("f_sub"));
        let root_file = make_file("doc2", "Intro.md", Some("f_root"));
        let loose_file = make_file("doc3", "Welcome.md", None);

        // When all are open
        let folders = vec![root_folder.clone(), sub_folder.clone()];
        let files = vec![sub_file.clone(), root_file.clone(), loose_file.clone()];
        let tree = build_sidebar_tree(&folders, &files, None);

        // Order should be:
        // 1. Docs (depth 0)
        // 2. Guides (depth 1)
        // 3. Setup.md (depth 2)
        // 4. Intro.md (depth 1)
        // 5. Welcome.md (depth 0)
        assert_eq!(tree.len(), 5);

        match tree[0] {
            SidebarTreeItem::Folder { index, depth } => {
                assert_eq!(folders[index as usize].id.as_ref(), "f_root");
                assert_eq!(depth, 0);
            }
            _ => panic!("Expected root folder"),
        }
        match tree[1] {
            SidebarTreeItem::Folder { index, depth } => {
                assert_eq!(folders[index as usize].id.as_ref(), "f_sub");
                assert_eq!(depth, 1);
            }
            _ => panic!("Expected sub folder"),
        }
        match tree[2] {
            SidebarTreeItem::File { index, depth } => {
                assert_eq!(files[index as usize].id, "doc1");
                assert_eq!(depth, 2);
            }
            _ => panic!("Expected Setup.md inside Guides"),
        }
        match tree[3] {
            SidebarTreeItem::File { index, depth } => {
                assert_eq!(files[index as usize].id, "doc2");
                assert_eq!(depth, 1);
            }
            _ => panic!("Expected Intro.md inside Docs"),
        }
        match tree[4] {
            SidebarTreeItem::File { index, depth } => {
                assert_eq!(files[index as usize].id, "doc3");
                assert_eq!(depth, 0);
            }
            _ => panic!("Expected Welcome.md at root"),
        }

        // Now test collapsed state: Docs is closed
        let closed_root = make_folder("f_root", "Docs", None, false);
        let folders_closed = vec![closed_root, sub_folder];
        let tree_closed = build_sidebar_tree(&folders_closed, &files, None);

        // Only Docs (depth 0) and Welcome.md (depth 0) should be visible!
        assert_eq!(tree_closed.len(), 2);
        match tree_closed[0] {
            SidebarTreeItem::Folder { index, depth } => {
                assert_eq!(folders_closed[index as usize].id.as_ref(), "f_root");
                assert_eq!(depth, 0);
            }
            _ => panic!("Expected root folder"),
        }
        match tree_closed[1] {
            SidebarTreeItem::File { index, depth } => {
                assert_eq!(files[index as usize].id, "doc3");
                assert_eq!(depth, 0);
            }
            _ => panic!("Expected Welcome.md at root"),
        }
    }

    #[test]
    fn create_input_at_root_sits_at_top_of_tree() {
        let folders = vec![make_folder("f1", "Docs", None, true)];
        let files = vec![make_file("doc1", "Notes.md", None)];
        let creating = SidebarCreate {
            parent: None,
            is_folder: false,
        };
        let tree = build_sidebar_tree(&folders, &files, Some(&creating));
        assert_eq!(tree.len(), 3);
        assert_eq!(
            tree[0],
            SidebarTreeItem::CreateInput {
                depth: 0,
                is_folder: false
            }
        );
        assert!(matches!(&tree[1], SidebarTreeItem::Folder { .. }));
        assert!(matches!(&tree[2], SidebarTreeItem::File { .. }));
    }

    #[test]
    fn create_input_inside_folder_is_first_child_one_level_deeper() {
        let folders = vec![make_folder("f1", "Docs", None, true)];
        let files = vec![make_file("doc1", "Notes.md", None)];
        let creating = SidebarCreate {
            parent: Some("f1".to_string()),
            is_folder: true,
        };
        let tree = build_sidebar_tree(&folders, &files, Some(&creating));
        assert_eq!(tree.len(), 3);
        assert!(matches!(&tree[0], SidebarTreeItem::Folder { .. }));
        assert_eq!(
            tree[1],
            SidebarTreeItem::CreateInput {
                depth: 1,
                is_folder: true
            }
        );
        assert!(matches!(&tree[2], SidebarTreeItem::File { .. }));
    }

    #[test]
    fn create_input_for_closed_folder_is_hidden_until_opened() {
        let folders = vec![make_folder("f1", "Docs", None, false)];
        let creating = SidebarCreate {
            parent: Some("f1".to_string()),
            is_folder: false,
        };
        let tree = build_sidebar_tree(&folders, &[], Some(&creating));
        assert_eq!(tree.len(), 1);
        assert!(matches!(&tree[0], SidebarTreeItem::Folder { .. }));
    }

    #[test]
    fn sidebar_tree_item_stays_compact() {
        assert!(
            std::mem::size_of::<SidebarTreeItem>() <= 12,
            "SidebarTreeItem grew to {} bytes",
            std::mem::size_of::<SidebarTreeItem>()
        );
    }
}
