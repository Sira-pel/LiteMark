use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use crate::model::{FileType, WorkspaceDirectory, WorkspaceFile, WorkspaceFolder};

pub const INITIAL_MARKDOWN: &str = r#"# Welcome to LiteMark

A super lightweight, high-performance **Markdown & Plain Text Editor** designed with dedicated notes folder management and native Linux desktop performance.

## ✨ Key Features
- **WYSIWYG Rich Markdown:** All markdown features work natively without showing raw syntax clutter!
- **Dedicated Notes Folder:** Automatically manages your notes in `~/Documents/Notes`.
- **Formatting Included:** Bold, *italic*, <u>underline</u>, and ~~strikethrough~~.
- **Header Levels:** Clean H1, H2, and H3 headers.
- **Lists & Checklists:**
  - [x] Lightweight & low memory usage (~18–28MB RAM)
  - [x] Open & save both .txt and .md files
  - [x] 1-Click install as desktop app or native .deb package
- **Quotes & Code Blocks:**
  > "Simplicity is prerequisite for reliability." — Edsger W. Dijkstra

```
// Ultra-fast, zero-bloat native performance
fn main() {
    println!("Hello from LiteMark!");
}
```

---

## 🚀 Native Performance (Rust with GPUI)
Running with **GPUI** hardware-accelerated Vulkan pipeline tailored for Ubuntu with **~20MB RAM** footprint!
"#;

pub fn get_or_create_notes_dir() -> PathBuf {
    let base = dirs::document_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Documents"));
    let notes = base.join("Notes");
    let _ = fs::create_dir_all(&notes);
    notes
}

pub fn seed_welcome_note(notes_dir: &Path) {
    let welcome = notes_dir.join("Welcome.md");
    if !welcome.exists() {
        let _ = fs::write(&welcome, INITIAL_MARKDOWN);
    }
}

pub fn sanitize_name(name: &str) -> String {
    name.replace(['/', '\\', '\0'], "_")
        .trim_matches('.')
        .trim()
        .to_string()
}

pub fn atomic_save(path: &Path, content: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        let file_name = path.file_name().and_then(|f| f.to_str()).unwrap_or("tmp");
        let temp_path = parent.join(format!(".{}.litemark-tmp", file_name));

        let mut file = fs::File::create(&temp_path)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        drop(file);

        fs::rename(&temp_path, path)?;
        Ok(())
    } else {
        fs::write(path, content)
    }
}

#[allow(dead_code)]
pub fn safe_create_file(parent: &Path, name: &str, content: &str) -> io::Result<PathBuf> {
    let clean_name = sanitize_name(name);
    let filename = if clean_name.contains('.') {
        clean_name
    } else {
        format!("{}.md", clean_name)
    };
    let target = parent.join(filename);
    atomic_save(&target, content)?;
    Ok(target)
}

#[allow(dead_code)]
pub fn safe_create_folder(parent: &Path, name: &str) -> io::Result<PathBuf> {
    let clean_name = sanitize_name(name);
    let target = parent.join(clean_name);
    fs::create_dir_all(&target)?;
    Ok(target)
}

#[allow(dead_code)]
pub fn safe_rename(old_path: &Path, new_name: &str) -> io::Result<PathBuf> {
    let clean_name = sanitize_name(new_name);
    if let Some(parent) = old_path.parent() {
        let target = parent.join(clean_name);
        fs::rename(old_path, &target)?;
        Ok(target)
    } else {
        Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid parent path"))
    }
}

#[allow(dead_code)]
pub fn safe_delete(path: &Path, is_dir: bool) -> io::Result<()> {
    if is_dir {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

pub fn scan_workspace_dir(dir: &Path) -> WorkspaceDirectory {
    let folder_name = if let Some(name) = dir.file_name().and_then(|f| f.to_str()) {
        format!("📁 {}", name)
    } else {
        "📁 Notes".to_string()
    };

    let mut folders = Vec::new();
    let mut files = Vec::new();

    fn walk(
        current: &Path,
        parent_id: Option<String>,
        folders: &mut Vec<WorkspaceFolder>,
        files: &mut Vec<WorkspaceFile>,
    ) {
        if let Ok(entries) = fs::read_dir(current) {
            let mut entries: Vec<_> = entries.flatten().collect();
            entries.sort_by_key(|e| e.file_name());

            for entry in entries {
                let path = entry.path();
                let file_name = entry.file_name().to_string_lossy().to_string();

                if file_name.starts_with('.') || file_name == "node_modules" || file_name == "target" {
                    continue;
                }

                if path.is_dir() {
                    let folder_id = format!("folder-{}", path.display());
                    folders.push(WorkspaceFolder {
                        id: folder_id.clone(),
                        name: file_name,
                        parent_id: parent_id.clone(),
                        path: path.clone(),
                        is_open: true,
                    });
                    walk(&path, Some(folder_id), folders, files);
                } else if path.is_file() {
                    let file_type = FileType::from_filename(&file_name);
                    if matches!(file_type, FileType::Markdown | FileType::PlainText) {
                        let updated_at = entry
                            .metadata()
                            .and_then(|m| m.modified())
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs())
                            .unwrap_or(0);

                        files.push(WorkspaceFile {
                            id: format!("file-{}", path.display()),
                            name: file_name,
                            folder_id: parent_id.clone(),
                            path,
                            file_type,
                            updated_at,
                        });
                    }
                }
            }
        }
    }

    walk(dir, None, &mut folders, &mut files);

    WorkspaceDirectory {
        folder_name,
        folder_path: dir.to_path_buf(),
        is_linked_to_local_disk: true,
        folders,
        files,
    }
}

pub fn pick_open_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Open Text, Markdown, or PDF File")
        .add_filter("Text & Markdown Files", &["md", "markdown", "txt", "text", "log", "pdf"])
        .pick_file()
}

pub fn pick_save_file(suggested_name: &str, ext: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Save File As")
        .set_file_name(suggested_name)
        .add_filter(ext, &[ext])
        .save_file()
}

pub fn pick_directory() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Choose Notes Folder")
        .pick_folder()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_name() {
        assert_eq!(sanitize_name("../../etc/passwd"), "_.._etc_passwd");
        assert_eq!(sanitize_name("My Notes/File.md"), "My Notes_File.md");
        assert_eq!(sanitize_name("..hidden.."), "hidden");
    }

    #[test]
    fn test_atomic_save_and_read() {
        let temp_dir = std::env::temp_dir().join(format!("litemark-test-{}", std::process::id()));
        fs::create_dir_all(&temp_dir).unwrap();
        let target_file = temp_dir.join("test_note.md");

        let content = "# Test Note\nContent saved atomically.";
        assert!(atomic_save(&target_file, content).is_ok());

        let read_back = fs::read_to_string(&target_file).unwrap();
        assert_eq!(read_back, content);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}

