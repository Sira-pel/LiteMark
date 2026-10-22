use crate::model::{
    file_dialog_block_reason, unique_name, FileType, PortalProbe, WorkspaceDirectory,
    WorkspaceFile, WorkspaceFolder,
};
use std::ffi::{c_char, CString};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const AT_FDCWD: i32 = -100;
const RENAME_NOREPLACE: u32 = 1;
const ENOSYS: i32 = 38;
const EINVAL: i32 = 22;

unsafe extern "C" {
    fn renameat2(
        olddirfd: i32,
        oldpath: *const c_char,
        newdirfd: i32,
        newpath: *const c_char,
        flags: u32,
    ) -> i32;
}

pub const INITIAL_MARKDOWN: &str = r#"# Welcome to LiteMark

LiteMark is a native Markdown and text editor for Linux. Notes start in `~/Documents/Notes`.

## Formatting

- **Bold**, *italic*, <u>underline</u>, and ~~strikethrough~~
- Headings, lists, and task lists
- [x] Open and save `.md` and `.txt` files
- [x] Switch between the raw view and the WYSIWYG view

> A quote.

```
fn main() {
    println!("Hello from LiteMark!");
}
```
"#;

pub fn get_or_create_notes_dir() -> PathBuf {
    let base = dirs::document_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Documents"));
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

/// Follow a symlink chain so a save replaces the target file and leaves the link in place.
fn resolve_symlink_chain(path: &Path) -> io::Result<PathBuf> {
    let mut current = path.to_path_buf();
    for _ in 0..32 {
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                let link = fs::read_link(&current)?;
                current = if link.is_absolute() {
                    link
                } else {
                    current.parent().unwrap_or(Path::new(".")).join(link)
                };
            }
            _ => return Ok(current),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "symlink chain too deep",
    ))
}

fn fsync_parent(path: &Path) -> io::Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let parent = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };
    let dir = fs::File::open(parent)?;
    dir.sync_all()
}

fn random_suffix() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let tick = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0);
    nanos ^ tick.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (std::process::id() as u64)
}

/// `renameat2(RENAME_NOREPLACE)` is missing on this filesystem.
/// ENOSYS is the usual code; some filesystems return EINVAL instead.
pub(crate) fn noreplace_unsupported(err: &io::Error) -> bool {
    matches!(err.raw_os_error(), Some(ENOSYS) | Some(EINVAL))
}

/// `renameat2(RENAME_NOREPLACE)`. Fails with [`io::ErrorKind::AlreadyExists`]
/// when `to` is already present, including across a race with another writer.
fn rename_noreplace(from: &Path, to: &Path) -> io::Result<()> {
    let from_c = CString::new(from.as_os_str().as_bytes())
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
    let to_c = CString::new(to.as_os_str().as_bytes())
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
    let rc = unsafe {
        renameat2(
            AT_FDCWD,
            from_c.as_ptr(),
            AT_FDCWD,
            to_c.as_ptr(),
            RENAME_NOREPLACE,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn write_temp(target: &Path, content: &str) -> io::Result<PathBuf> {
    let parent = target.parent().unwrap_or(Path::new("."));
    let parent = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };
    fs::create_dir_all(parent)?;

    let original = fs::metadata(target).ok();
    let mode = original
        .as_ref()
        .map(|meta| meta.permissions().mode() & 0o777)
        .unwrap_or(0o644);
    let file_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("tmp");

    let mut last_err = io::Error::other("couldn't create a temporary file");
    for _ in 0..8 {
        let temp_path = parent.join(format!(".{file_name}.{}.litemark-tmp", random_suffix()));
        let opened = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .open(&temp_path);
        let mut file = match opened {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                last_err = err;
                continue;
            }
            Err(err) => return Err(err),
        };
        let write_result = (|| {
            file.write_all(content.as_bytes())?;
            file.sync_all()?;
            if let Some(meta) = &original {
                fs::set_permissions(&temp_path, meta.permissions())?;
            }
            Ok(())
        })();
        drop(file);
        if let Err(err) = write_result {
            let _ = fs::remove_file(&temp_path);
            return Err(err);
        }
        return Ok(temp_path);
    }
    Err(last_err)
}

pub fn atomic_save(path: &Path, content: &str) -> io::Result<()> {
    let target = resolve_symlink_chain(path)?;
    let temp_path = write_temp(&target, content)?;
    if let Err(err) = fs::rename(&temp_path, &target) {
        let _ = fs::remove_file(&temp_path);
        return Err(err);
    }
    fsync_parent(&target)?;
    Ok(())
}

/// Create `dir/desired_name` without replacing an existing file.
/// If the name is taken, or appears between the check and the rename, try the
/// next free name.
pub fn atomic_save_new(dir: &Path, desired_name: &str, content: &str) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let mut blocked = Vec::new();
    for _ in 0..64 {
        let name = unique_name(desired_name, &|candidate| {
            blocked.iter().any(|taken: &String| taken == candidate) || dir.join(candidate).exists()
        });
        let path = dir.join(&name);
        let temp_path = write_temp(&path, content)?;
        match rename_noreplace(&temp_path, &path) {
            Ok(()) => {
                fsync_parent(&path)?;
                return Ok(path);
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                let _ = fs::remove_file(&temp_path);
                blocked.push(name);
            }
            Err(err) if noreplace_unsupported(&err) => {
                // ENOSYS or EINVAL: fall back to a same-directory hard link,
                // which also fails when the destination exists.
                match fs::hard_link(&temp_path, &path) {
                    Ok(()) => {
                        let _ = fs::remove_file(&temp_path);
                        fsync_parent(&path)?;
                        return Ok(path);
                    }
                    Err(link_err) if link_err.kind() == io::ErrorKind::AlreadyExists => {
                        let _ = fs::remove_file(&temp_path);
                        blocked.push(name);
                    }
                    Err(link_err) => {
                        let _ = fs::remove_file(&temp_path);
                        return Err(link_err);
                    }
                }
            }
            Err(err) => {
                let _ = fs::remove_file(&temp_path);
                return Err(err);
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "no free file name",
    ))
}

/// Read a note. Failure is an error — callers must not substitute an empty document.
pub fn read_note(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|err| format!("Couldn't open {}: {err}", path.display()))
}

pub fn move_to_trash(path: &Path) -> Result<(), String> {
    trash::delete(path).map_err(|err| format!("Couldn't move {} to Trash: {err}", path.display()))
}

pub fn safe_create_file(parent: &Path, name: &str, content: &str) -> io::Result<PathBuf> {
    fs::create_dir_all(parent)?;
    let clean_name = sanitize_name(name);
    let filename = if clean_name.is_empty() {
        "Untitled.md".to_string()
    } else if clean_name.contains('.') {
        clean_name
    } else {
        format!("{clean_name}.md")
    };
    let target = parent.join(filename);
    if fs::symlink_metadata(&target).is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "file already exists",
        ));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;
    drop(file);
    fsync_parent(&target)?;
    Ok(target)
}

pub fn safe_create_folder(parent: &Path, name: &str) -> io::Result<PathBuf> {
    let clean_name = sanitize_name(name);
    if clean_name.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "empty name"));
    }
    let target = parent.join(&clean_name);
    fs::create_dir(&target)?;
    Ok(target)
}

pub fn safe_rename(old_path: &Path, new_name: &str) -> io::Result<PathBuf> {
    let clean_name = sanitize_name(new_name);
    if clean_name.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "empty name"));
    }
    let Some(parent) = old_path.parent() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid parent path",
        ));
    };
    let target = parent.join(clean_name);
    if target != old_path && fs::symlink_metadata(&target).is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "destination already exists",
        ));
    }
    if target != old_path {
        rename_noreplace(old_path, &target)?;
        fsync_parent(&target)?;
    }
    Ok(target)
}

pub fn next_available_filename(dir: &Path, desired: &str) -> String {
    let clean = sanitize_name(desired);
    let desired = if clean.is_empty() {
        "Untitled.md".to_string()
    } else {
        clean
    };
    unique_name(&desired, &|name| dir.join(name).exists())
}

/// Ask the session bus whether a file dialog can be shown.
pub fn probe_file_dialog() -> PortalProbe {
    if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_none() {
        let uid = uzers_or_uid();
        let bus = PathBuf::from(format!("/run/user/{uid}/bus"));
        if !bus.exists() {
            return PortalProbe::NoSessionBus;
        }
    }

    let portal_installed = Path::new("/usr/libexec/xdg-desktop-portal").exists()
        || Path::new("/usr/bin/xdg-desktop-portal").exists();

    match Command::new("dbus-send")
        .args([
            "--session",
            "--dest=org.freedesktop.DBus",
            "--print-reply",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus.NameHasOwner",
            "string:org.freedesktop.portal.Desktop",
        ])
        .output()
    {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            if text.contains("boolean true") {
                PortalProbe::Ready
            } else {
                PortalProbe::NotOwned
            }
        }
        Ok(_) => PortalProbe::HelperMissing { portal_installed },
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            PortalProbe::HelperMissing { portal_installed }
        }
        Err(_) => PortalProbe::HelperMissing { portal_installed },
    }
}

fn uzers_or_uid() -> u32 {
    // libc isn't a direct dependency; the session bus path uses the real uid via /proc.
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                let rest = line.strip_prefix("Uid:")?;
                rest.split_whitespace().next()?.parse().ok()
            })
        })
        .unwrap_or(0)
}

pub fn file_dialog_error() -> Option<String> {
    file_dialog_block_reason(probe_file_dialog()).map(str::to_string)
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

                if file_name.starts_with('.')
                    || file_name == "node_modules"
                    || file_name == "target"
                {
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
        .add_filter(
            "Text & Markdown Files",
            &["md", "markdown", "txt", "text", "log", "pdf"],
        )
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

    #[test]
    fn atomic_save_keeps_permissions_and_follows_symlinks() {
        use std::os::unix::fs::{symlink, PermissionsExt};

        let temp_dir = std::env::temp_dir().join(format!("litemark-atomic-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let target = temp_dir.join("real.md");
        fs::write(&target, "old").unwrap();
        let mut perms = fs::metadata(&target).unwrap().permissions();
        perms.set_mode(0o640);
        fs::set_permissions(&target, perms).unwrap();

        let link = temp_dir.join("link.md");
        symlink(&target, &link).unwrap();
        atomic_save(&link, "updated").unwrap();

        assert_eq!(fs::read_to_string(&target).unwrap(), "updated");
        assert!(fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        let mode = fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o640);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn safe_create_and_rename_do_not_overwrite() {
        let temp_dir = std::env::temp_dir().join(format!("litemark-safe-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let created = safe_create_file(&temp_dir, "Note.md", "keep").unwrap();
        assert!(safe_create_file(&temp_dir, "Note.md", "wipe").is_err());
        assert_eq!(fs::read_to_string(&created).unwrap(), "keep");

        fs::write(temp_dir.join("taken.md"), "taken").unwrap();
        let err = safe_rename(&created, "taken.md").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(
            fs::read_to_string(temp_dir.join("taken.md")).unwrap(),
            "taken"
        );
        assert_eq!(fs::read_to_string(&created).unwrap(), "keep");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn unreadable_path_is_an_error_not_an_empty_note() {
        let temp_dir = std::env::temp_dir().join(format!("litemark-read-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        let result = read_note(&temp_dir);
        assert!(
            result.is_err(),
            "a directory must not open as an empty document"
        );
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn next_available_filename_skips_existing_notes() {
        let temp_dir = std::env::temp_dir().join(format!("litemark-names-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        fs::write(temp_dir.join("New Note.md"), "x").unwrap();
        assert_eq!(
            next_available_filename(&temp_dir, "New Note.md"),
            "New Note 2.md"
        );
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn atomic_save_uses_a_random_temp_and_removes_it_on_failure() {
        let temp_dir =
            std::env::temp_dir().join(format!("litemark-tmpfail-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let target = temp_dir.join("blocked.md");
        fs::create_dir(&target).unwrap();
        let err = atomic_save(&target, "nope").unwrap_err();
        assert!(err.kind() != io::ErrorKind::NotFound || err.raw_os_error().is_some());
        let leftovers: Vec<_> = fs::read_dir(&temp_dir)
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().contains("litemark-tmp"))
            .collect();
        assert!(leftovers.is_empty());
        assert!(!temp_dir.join(".blocked.md.litemark-tmp").exists());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn noreplace_falls_back_on_enosys_and_einval() {
        assert!(noreplace_unsupported(&io::Error::from_raw_os_error(38)));
        assert!(noreplace_unsupported(&io::Error::from_raw_os_error(22)));
        assert!(!noreplace_unsupported(&io::Error::from_raw_os_error(17)));
        assert!(!noreplace_unsupported(&io::Error::new(
            io::ErrorKind::AlreadyExists,
            "exists"
        )));
    }

    #[test]
    fn new_path_save_does_not_replace_and_retries_the_next_name() {
        let temp_dir =
            std::env::temp_dir().join(format!("litemark-newpath-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        fs::write(temp_dir.join("Untitled.md"), "keep").unwrap();

        let written = atomic_save_new(&temp_dir, "Untitled.md", "fresh").unwrap();
        assert_eq!(written.file_name().unwrap(), "Untitled 2.md");
        assert_eq!(
            fs::read_to_string(temp_dir.join("Untitled.md")).unwrap(),
            "keep"
        );
        assert_eq!(fs::read_to_string(&written).unwrap(), "fresh");

        let mode = fs::metadata(&written).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn create_folder_does_not_create_missing_parents_or_replace() {
        let temp_dir = std::env::temp_dir().join(format!("litemark-mkdir-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let err = safe_create_folder(&temp_dir.join("missing"), "child").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        assert!(!temp_dir.join("missing").exists());

        let created = safe_create_folder(&temp_dir, "child").unwrap();
        assert!(created.is_dir());
        let again = safe_create_folder(&temp_dir, "child").unwrap_err();
        assert_eq!(again.kind(), io::ErrorKind::AlreadyExists);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
