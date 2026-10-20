use std::path::PathBuf;
use super::document::FileType;

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceFile {
    pub id: String,
    pub name: String,
    pub folder_id: Option<String>,
    pub path: PathBuf,
    pub file_type: FileType,
    pub updated_at: u64,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceFolder {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub path: PathBuf,
    pub is_open: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceDirectory {
    pub folder_name: String,
    pub folder_path: PathBuf,
    pub is_linked_to_local_disk: bool,
    pub folders: Vec<WorkspaceFolder>,
    pub files: Vec<WorkspaceFile>,
}

impl Default for WorkspaceDirectory {
    fn default() -> Self {
        let home = dirs::document_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Documents"));
        let notes = home.join("Notes");
        Self {
            folder_name: "~/Documents/Notes".to_string(),
            folder_path: notes,
            is_linked_to_local_disk: true,
            folders: Vec::new(),
            files: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::sidebar::{build_sidebar_tree, SidebarTreeItem};

    fn make_folder(id: &str, name: &str, parent_id: Option<&str>, is_open: bool) -> WorkspaceFolder {
        WorkspaceFolder {
            id: id.to_string(),
            name: name.to_string(),
            parent_id: parent_id.map(String::from),
            path: PathBuf::from(format!("/tmp/{}", name)),
            is_open,
        }
    }

    fn make_file(id: &str, name: &str, folder_id: Option<&str>) -> WorkspaceFile {
        WorkspaceFile {
            id: id.to_string(),
            name: name.to_string(),
            folder_id: folder_id.map(String::from),
            path: PathBuf::from(format!("/tmp/{}", name)),
            file_type: FileType::Markdown,
            updated_at: 0,
        }
    }

    #[test]
    fn test_build_sidebar_tree_empty() {
        let tree = build_sidebar_tree(&[], &[]);
        assert!(tree.is_empty());
    }

    #[test]
    fn test_build_sidebar_tree_folder_without_files() {
        let folders = vec![make_folder("f1", "New Folder", None, true)];
        let tree = build_sidebar_tree(&folders, &[]);
        assert_eq!(tree.len(), 1);
        match &tree[0] {
            SidebarTreeItem::Folder { folder, depth } => {
                assert_eq!(folder.name, "New Folder");
                assert_eq!(*depth, 0);
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
        let tree = build_sidebar_tree(&folders, &files);

        // Order should be:
        // 1. Docs (depth 0)
        // 2. Guides (depth 1)
        // 3. Setup.md (depth 2)
        // 4. Intro.md (depth 1)
        // 5. Welcome.md (depth 0)
        assert_eq!(tree.len(), 5);

        match &tree[0] {
            SidebarTreeItem::Folder { folder, depth } => {
                assert_eq!(folder.id, "f_root");
                assert_eq!(*depth, 0);
            }
            _ => panic!("Expected root folder"),
        }
        match &tree[1] {
            SidebarTreeItem::Folder { folder, depth } => {
                assert_eq!(folder.id, "f_sub");
                assert_eq!(*depth, 1);
            }
            _ => panic!("Expected sub folder"),
        }
        match &tree[2] {
            SidebarTreeItem::File { file, depth } => {
                assert_eq!(file.id, "doc1");
                assert_eq!(*depth, 2);
            }
            _ => panic!("Expected Setup.md inside Guides"),
        }
        match &tree[3] {
            SidebarTreeItem::File { file, depth } => {
                assert_eq!(file.id, "doc2");
                assert_eq!(*depth, 1);
            }
            _ => panic!("Expected Intro.md inside Docs"),
        }
        match &tree[4] {
            SidebarTreeItem::File { file, depth } => {
                assert_eq!(file.id, "doc3");
                assert_eq!(*depth, 0);
            }
            _ => panic!("Expected Welcome.md at root"),
        }

        // Now test collapsed state: Docs is closed
        let closed_root = make_folder("f_root", "Docs", None, false);
        let folders_closed = vec![closed_root, sub_folder];
        let tree_closed = build_sidebar_tree(&folders_closed, &files);

        // Only Docs (depth 0) and Welcome.md (depth 0) should be visible!
        assert_eq!(tree_closed.len(), 2);
        match &tree_closed[0] {
            SidebarTreeItem::Folder { folder, depth } => {
                assert_eq!(folder.id, "f_root");
                assert_eq!(*depth, 0);
            }
            _ => panic!("Expected root folder"),
        }
        match &tree_closed[1] {
            SidebarTreeItem::File { file, depth } => {
                assert_eq!(file.id, "doc3");
                assert_eq!(*depth, 0);
            }
            _ => panic!("Expected Welcome.md at root"),
        }
    }
}
