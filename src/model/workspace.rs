use std::path::PathBuf;
use super::document::FileType;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct WorkspaceFile {
    pub id: String,
    pub name: String,
    pub folder_id: Option<String>,
    pub path: PathBuf,
    pub file_type: FileType,
    pub updated_at: u64,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct WorkspaceFolder {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub path: PathBuf,
    pub is_open: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
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
