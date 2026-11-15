use crate::model::{
    content_hash, file_dialog_block_reason, looks_binary, unique_name, DiskChange, DiskVersion,
    FileType, PortalProbe, WorkspaceDirectory, WorkspaceFile, WorkspaceFolder, CODE_EXTENSIONS,
    TEXT_EXTENSIONS,
};
#[cfg(target_os = "linux")]
use std::ffi::{c_char, CString};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
#[cfg(target_os = "linux")]
use std::os::unix::ffi::OsStrExt;
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
#[cfg(target_os = "linux")]
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(target_os = "linux")]
const AT_FDCWD: i32 = -100;
#[cfg(target_os = "linux")]
const RENAME_NOREPLACE: u32 = 1;
#[cfg(target_os = "linux")]
const ENOSYS: i32 = 38;
#[cfg(target_os = "linux")]
const EINVAL: i32 = 22;

#[cfg(target_os = "linux")]
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

LiteMark is a native Markdown and text editor. Notes start in `~/Documents/Notes`.

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
    let notes = crate::model::workspace::default_notes_dir(&base);
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
    // Path separators, characters Windows refuses in a name (`<>:"|?*`) and
    // control characters all become `_`, so a name typed on one system can
    // be saved and synced to another.
    let replaced: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | '<' | '>' | ':' | '"' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    // Leading dots stay (`.gitignore`, `.env`): dotfiles are listed in the
    // sidebar. Trailing dots and spaces go, since Windows drops them and
    // "notes." would silently become "notes". That also turns "." and ".."
    // into "", which callers reject as an empty name.
    let trimmed = replaced.trim().trim_end_matches(['.', ' ']).to_string();
    avoid_reserved_device_name(trimmed)
}

/// Windows maps `CON`, `NUL`, `COM1` and the like to devices whatever the
/// extension (`con.md` too), so such a note can't be created or opened
/// there. Add `_` after the stem: `CON.md` becomes `CON_.md`.
fn avoid_reserved_device_name(name: String) -> String {
    let stem_end = name.find('.').unwrap_or(name.len());
    if is_reserved_device_name(name[..stem_end].trim_end()) {
        format!("{}_{}", &name[..stem_end], &name[stem_end..])
    } else {
        name
    }
}

fn is_reserved_device_name(stem: &str) -> bool {
    let upper = stem.to_ascii_uppercase();
    if matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) {
        return true;
    }
    let Some(rest) = upper
        .strip_prefix("COM")
        .or_else(|| upper.strip_prefix("LPT"))
    else {
        return false;
    };
    let mut chars = rest.chars();
    matches!(
        (chars.next(), chars.next()),
        (Some('0'..='9' | '\u{b9}' | '\u{b2}' | '\u{b3}'), None)
    )
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

/// Windows cannot open a directory like a file, so there is nothing to sync.
#[cfg(not(unix))]
fn fsync_parent(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
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
#[cfg(target_os = "linux")]
fn noreplace_unsupported(err: &io::Error) -> bool {
    matches!(err.raw_os_error(), Some(ENOSYS) | Some(EINVAL))
}

/// Move `from` to `to` without replacing an existing `to`. A hard link fails
/// with [`io::ErrorKind::AlreadyExists`] when `to` is present. Filesystems
/// without hard links (FAT, exFAT) fall back to an exclusive create and copy.
#[cfg(not(target_os = "linux"))]
fn rename_noreplace(from: &Path, to: &Path) -> io::Result<()> {
    rename_noreplace_portable(from, to)
}

/// On Linux, the fallback when `renameat2(RENAME_NOREPLACE)` is unsupported.
fn rename_noreplace_portable(from: &Path, to: &Path) -> io::Result<()> {
    rename_noreplace_portable_with(from, to, |a, b| fs::hard_link(a, b), |p| fs::remove_file(p))
}

/// [`rename_noreplace_portable`] with the hard link and the removal of the
/// source injected, so tests can force the copy path and a failed removal.
fn rename_noreplace_portable_with(
    from: &Path,
    to: &Path,
    link: impl Fn(&Path, &Path) -> io::Result<()>,
    remove: impl Fn(&Path) -> io::Result<()>,
) -> io::Result<()> {
    // Directories cannot be hard-linked. Check, then rename; a writer that
    // creates `to` in between is the one race this path does not close.
    if fs::symlink_metadata(from)?.is_dir() {
        if fs::symlink_metadata(to).is_ok() {
            return Err(io::Error::from(io::ErrorKind::AlreadyExists));
        }
        return fs::rename(from, to);
    }
    match link(from, to) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => return Err(err),
        Err(_) => copy_noreplace(from, to)?,
    }
    // A move that leaves both names behind is a duplicate, not a move: undo
    // the new name so the note exists once, under its old name.
    if let Err(err) = remove(from) {
        let _ = fs::remove_file(to);
        return Err(err);
    }
    Ok(())
}

/// Copy `from` to a new file `to` (never replacing one), streaming the bytes
/// and keeping the source's permissions. On failure `to` is removed.
fn copy_noreplace(from: &Path, to: &Path) -> io::Result<()> {
    let mut source = fs::File::open(from)?;
    let permissions = source.metadata()?.permissions();
    let mut dest = OpenOptions::new().write(true).create_new(true).open(to)?;
    let copied = (|| {
        io::copy(&mut source, &mut dest)?;
        dest.set_permissions(permissions)?;
        dest.sync_all()
    })();
    drop(dest);
    if copied.is_err() {
        let _ = fs::remove_file(to);
    }
    copied
}

/// `renameat2(RENAME_NOREPLACE)`. Fails with [`io::ErrorKind::AlreadyExists`]
/// when `to` is already present, including across a race with another writer.
/// Filesystems without it (some NFS and FUSE mounts report ENOSYS or EINVAL)
/// use the hard-link fallback, so rename, move and save work there too.
#[cfg(target_os = "linux")]
fn rename_noreplace(from: &Path, to: &Path) -> io::Result<()> {
    rename_noreplace_with(from, to, renameat2_noreplace)
}

/// [`rename_noreplace`] with the `renameat2` call injected, so tests can make
/// it report an unsupported filesystem.
#[cfg(target_os = "linux")]
fn rename_noreplace_with(
    from: &Path,
    to: &Path,
    native: impl Fn(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    match native(from, to) {
        Err(err) if noreplace_unsupported(&err) => rename_noreplace_portable(from, to),
        result => result,
    }
}

#[cfg(target_os = "linux")]
fn renameat2_noreplace(from: &Path, to: &Path) -> io::Result<()> {
    let from_c = CString::new(from.as_os_str().as_bytes())
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
    let to_c = CString::new(to.as_os_str().as_bytes())
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
    // SAFETY: both CStrings outlive the call and are NUL-terminated; the
    // signature matches glibc/musl `renameat2`.
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

/// Suffix of the temporary files `atomic_save` writes next to their target.
const TEMP_SUFFIX: &str = ".litemark-tmp";

/// Write `content` to a new temporary file next to `target`. When `target`
/// exists, the temp file gets its permissions and, on Unix, its owner and
/// group; if the owner or group can't be given away the result is a
/// `PermissionDenied` error, like a directory the user can't create files in.
fn write_temp(target: &Path, content: &str) -> io::Result<PathBuf> {
    let parent = target.parent().unwrap_or(Path::new("."));
    let parent = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };
    fs::create_dir_all(parent)?;

    let original = fs::metadata(target).ok();
    #[cfg(unix)]
    let mode = original
        .as_ref()
        .map(|meta| meta.permissions().mode() & 0o777)
        .unwrap_or(0o644);
    let mut last_err = io::Error::other("couldn't create a temporary file");
    for _ in 0..8 {
        // Fixed length (30 bytes) whatever the target's name, so a name near
        // the 255-byte limit can still be saved.
        let temp_path = parent.join(format!(".{:016x}{TEMP_SUFFIX}", random_suffix()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(mode);
        let opened = options.open(&temp_path);
        let mut file = match opened {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                last_err = err;
                continue;
            }
            Err(err) => return Err(err),
        };
        let write_result = (|| {
            #[cfg(unix)]
            if let Some(meta) = &original {
                match_owner(&file, meta)?;
            }
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

/// Give the temp file the owner and group of the file it will replace, so a
/// note shared in a group folder stays readable by the others.
#[cfg(unix)]
fn match_owner(temp: &fs::File, original: &fs::Metadata) -> io::Result<()> {
    let current = temp.metadata()?;
    if current.uid() == original.uid() && current.gid() == original.gid() {
        return Ok(());
    }
    // Any failure (EPERM for an ordinary user, or a filesystem that doesn't
    // support it) means the owner can't be kept, so it is reported as
    // PermissionDenied and the caller writes in place instead.
    std::os::unix::fs::fchown(temp, Some(original.uid()), Some(original.gid())).map_err(|err| {
        if err.kind() == io::ErrorKind::PermissionDenied {
            err
        } else {
            io::Error::new(io::ErrorKind::PermissionDenied, err)
        }
    })
}

/// Saving by rename would change something about the file besides its
/// text: another name (hard link) shares it and would keep the old text.
fn rename_would_split(meta: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        meta.nlink() > 1
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        false
    }
}

/// Overwrite the open file in place. Not atomic: a crash mid-write can
/// leave a mix of old and new text. Used only when replacing the file would
/// split a hard link, lose its owner, or isn't possible in its folder. The
/// text is written before the length is cut, so the file is never empty
/// while the write runs.
fn write_in_place(mut file: fs::File, content: &str) -> io::Result<()> {
    use std::io::{Seek, SeekFrom};
    file.seek(SeekFrom::Start(0))?;
    file.write_all(content.as_bytes())?;
    file.set_len(content.len() as u64)?;
    file.sync_all()
}

/// Save over an existing file (or create it). Normally the text goes to a
/// temp file that is renamed over the target, so a crash never leaves a
/// half-written note. When that would split a hard link or reset the owner,
/// or the folder doesn't allow new files, the file is overwritten in place
/// instead. A file the user may not write is refused, as a direct write
/// would be, rather than replaced.
pub fn atomic_save(path: &Path, content: &str) -> io::Result<()> {
    let target = resolve_symlink_chain(path)?;
    let existing = match fs::metadata(&target) {
        Ok(meta) => Some(meta),
        Err(err) if err.kind() == io::ErrorKind::NotFound => None,
        Err(err) => return Err(err),
    };
    // Opening for writing checks the real permission (mode bits, ACLs,
    // read-only attribute) without changing the file.
    let in_place = match &existing {
        Some(meta) if meta.is_file() => Some(OpenOptions::new().write(true).open(&target)?),
        _ => None,
    };
    if let (Some(meta), Some(file)) = (&existing, in_place.as_ref()) {
        if rename_would_split(meta) {
            let file = file.try_clone()?;
            return write_in_place(file, content);
        }
    }
    let temp_path = match write_temp(&target, content) {
        Ok(temp_path) => temp_path,
        Err(err) if err.kind() == io::ErrorKind::PermissionDenied => {
            return match in_place {
                Some(file) => write_in_place(file, content),
                None => Err(err),
            };
        }
        Err(err) => return Err(err),
    };
    drop(in_place);
    if let Err(err) = fs::rename(&temp_path, &target) {
        let _ = fs::remove_file(&temp_path);
        return Err(err);
    }
    fsync_parent(&target)?;
    Ok(())
}

/// Record what `path` looks like now, holding `content` (the text just read
/// from it or written to it). `None` when the file can't be inspected.
pub fn disk_version(path: &Path, content: &str) -> Option<DiskVersion> {
    fs::metadata(path).ok()?;
    Some(DiskVersion {
        content_hash: content_hash(content.as_bytes()),
    })
}

/// Has `path` changed since `seen` was recorded? The bytes decide, so a
/// `touch` or a copy that kept the text doesn't count as a change. They are
/// read even when size, time and file id all match: a same-size write within
/// the filesystem's timestamp granularity leaves all three unchanged.
pub fn disk_change(path: &Path, seen: &DiskVersion) -> DiskChange {
    match fs::metadata(path) {
        Ok(_) => {}
        Err(err) if err.kind() == io::ErrorKind::NotFound => return DiskChange::Deleted,
        // Can't tell; let the save itself report the problem.
        Err(_) => return DiskChange::Unchanged,
    }
    match fs::read(path) {
        Ok(bytes) if content_hash(&bytes) == seen.content_hash => DiskChange::Unchanged,
        _ => DiskChange::Changed,
    }
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

/// Largest note we load. Bigger files take seconds to read and lay out on
/// the UI thread, which freezes the window.
pub const MAX_NOTE_BYTES: u64 = 32 * 1024 * 1024;

/// How much of a file `looks_binary` inspects.
const BINARY_SNIFF_BYTES: usize = 8 * 1024;

#[cfg(target_os = "linux")]
const O_NONBLOCK: i32 = 0o4000;
#[cfg(target_os = "macos")]
const O_NONBLOCK: i32 = 0x0004;

/// Why `meta` can't be opened as a note, if it can't.
fn note_open_block(meta: &fs::Metadata) -> Option<String> {
    if meta.is_dir() {
        return Some("it's a folder".into());
    }
    if !meta.is_file() {
        // FIFOs, sockets and devices: reading them can block forever.
        return Some("not a regular file".into());
    }
    if meta.len() > MAX_NOTE_BYTES {
        return Some(format!(
            "file is too large ({} MB, limit {} MB)",
            meta.len() / (1024 * 1024),
            MAX_NOTE_BYTES / (1024 * 1024)
        ));
    }
    None
}

/// Read a note. Failure is an error — callers must not substitute an empty document.
/// Never blocks on a FIFO or device, and refuses files over `MAX_NOTE_BYTES`.
pub fn read_note(path: &Path) -> Result<String, String> {
    use std::io::Read;
    let fail = |err: &dyn std::fmt::Display| format!("Couldn't open {}: {err}", path.display());
    let meta = fs::metadata(path).map_err(|err| fail(&err))?;
    if let Some(reason) = note_open_block(&meta) {
        return Err(fail(&reason));
    }
    // The path may be swapped for a FIFO after the check above, so open
    // without blocking and check the handle we actually got.
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    options.custom_flags(O_NONBLOCK);
    let file = options.open(path).map_err(|err| fail(&err))?;
    let meta = file.metadata().map_err(|err| fail(&err))?;
    if let Some(reason) = note_open_block(&meta) {
        return Err(fail(&reason));
    }
    // Check the head for binary content before reading the rest, so a large
    // binary file costs 8 KiB, not its whole size.
    let mut reader = file.take(MAX_NOTE_BYTES + 1);
    let mut bytes = Vec::with_capacity(BINARY_SNIFF_BYTES);
    (&mut reader)
        .take(BINARY_SNIFF_BYTES as u64)
        .read_to_end(&mut bytes)
        .map_err(|err| fail(&err))?;
    if looks_binary(&bytes) {
        return Err(fail(&"binary file, can't open"));
    }
    bytes.reserve((meta.len() as usize).saturating_sub(bytes.len()));
    // The outer take is +1 so a file that grew past the limit is caught.
    reader.read_to_end(&mut bytes).map_err(|err| fail(&err))?;
    if bytes.len() as u64 > MAX_NOTE_BYTES {
        return Err(fail(&"file is too large"));
    }
    String::from_utf8(bytes).map_err(|err| fail(&err))
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
    // Preserve the original extension unless the new name already ends in a
    // known one. A dot mid-name (e.g. "v1.2 notes") is not an extension, so
    // "n.md" renamed to "v1.2 notes" stays "v1.2 notes.md".
    let keeps_note_ext = |name: &str| {
        let lower = name.to_lowercase();
        // A note must not turn into a read-only PDF tab by being renamed.
        lower.ends_with(".txt")
            || matches!(
                FileType::from_filename(name),
                FileType::Markdown | FileType::Code
            )
    };
    let clean_name = if old_path.is_file() && !keeps_note_ext(&clean_name) {
        if let Some(ext) = old_path.extension().and_then(|e| e.to_str()) {
            format!("{clean_name}.{ext}")
        } else {
            clean_name
        }
    } else {
        clean_name
    };
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

/// Copy a note's content into `dest_dir` under a non-colliding name.
/// The original file is left untouched.
pub fn copy_note_into_workspace(src: &Path, dest_dir: &Path, content: &str) -> io::Result<PathBuf> {
    let name = src
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Untitled.md");
    let target_name = next_available_filename(dest_dir, name);
    safe_create_file(dest_dir, &target_name, content)
}

/// Move a note file into `dest_dir` keeping its name (auto-uniquified on
/// collision). Moving into the note's own folder is a no-op. Returns the
/// new path.
pub fn move_note(src: &Path, dest_dir: &Path) -> io::Result<PathBuf> {
    let name = src
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid path"))?;
    if src.parent() == Some(dest_dir) {
        return Ok(src.to_path_buf());
    }
    let target = dest_dir.join(next_available_filename(dest_dir, name));
    rename_noreplace(src, &target)?;
    fsync_parent(&target)?;
    fsync_parent(src)?;
    Ok(target)
}

/// Move a folder into `dest_dir` keeping its name (auto-uniquified on
/// collision). Moving into the folder's own parent is a no-op. Returns
/// the new path.
pub fn move_folder(src: &Path, dest_dir: &Path) -> io::Result<PathBuf> {
    let name = src
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid path"))?;
    if src.parent() == Some(dest_dir) {
        return Ok(src.to_path_buf());
    }
    let target = dest_dir.join(unique_name(name, &|candidate| {
        dest_dir.join(candidate).exists()
    }));
    rename_noreplace(src, &target)?;
    fsync_parent(&target)?;
    fsync_parent(src)?;
    Ok(target)
}

/// Recursively copy a folder into `dest_dir` under a non-colliding name.
/// The original folder is left untouched. Returns the new path.
pub fn copy_folder(src: &Path, dest_dir: &Path) -> io::Result<PathBuf> {
    let name = src
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid path"))?;
    let target = dest_dir.join(unique_name(name, &|candidate| {
        dest_dir.join(candidate).exists()
    }));
    copy_dir_recursive(src, &target)?;
    fsync_parent(&target)?;
    Ok(target)
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> io::Result<()> {
    // Create the destination first, outside the cleanup's reach: if
    // create_dir fails because something else took the name in between the
    // uniqueness check and here, the cleanup must not delete that real
    // folder — it only ever removes a folder this copy created.
    fs::create_dir(dest)?;
    if let Err(err) = copy_dir_inner(src, dest) {
        // Never leave a half-copied folder behind.
        let _ = fs::remove_dir_all(dest);
        return Err(err);
    }
    Ok(())
}

fn copy_dir_inner(src: &Path, dest: &Path) -> io::Result<()> {
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        // DirEntry::file_type does not follow symlinks: skip links entirely.
        // Copying their targets could pull content from outside the folder,
        // and a broken link would fail the copy halfway through.
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else if file_type.is_file() {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Windows and macOS have native file dialogs that are always available.
#[cfg(not(target_os = "linux"))]
pub fn probe_file_dialog() -> PortalProbe {
    PortalProbe::Ready
}

/// Ask the session bus whether a file dialog can be shown.
#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
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

/// Windows says "Access is denied. (os error 5)" both for real permission
/// problems and when Controlled Folder Access (Ransomware protection) blocks
/// an app it doesn't know from writing to Documents, Desktop and similar
/// folders. The raw message doesn't help someone who hits the second case.
const ACCESS_DENIED_HINT: &str = "Windows may be blocking LiteMark from this folder. \
Allow it under Windows Security > Virus & threat protection > Ransomware protection > \
Allow an app through Controlled folder access, or choose another folder.";

fn is_access_denied(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("access is denied")
        // With the parenthesis, so codes like 53 (network path not found)
        // don't match.
        || message.contains("(os error 5)")
        || message.contains("permission denied")
}

/// The message to show for a failed file operation. On Windows an access
/// denied error gets a hint about Controlled Folder Access; anything else is
/// returned unchanged.
pub fn with_access_denied_hint(message: String) -> String {
    if cfg!(windows) && is_access_denied(&message) && !message.contains(ACCESS_DENIED_HINT) {
        format!("{message} {ACCESS_DENIED_HINT}")
    } else {
        message
    }
}

pub fn file_dialog_error() -> Option<String> {
    file_dialog_block_reason(probe_file_dialog()).map(str::to_string)
}

/// Extensions of common binary formats. Without this every unknown file
/// counts as plain text, so images, video and disk images show up as notes.
/// Short or generic extensions (`a`, `o`, `bin`, `dat`, `db`) are left out:
/// plain-text files use them too, and hiding a note is worse than listing a
/// binary file, which `read_note` refuses to open anyway.
const BINARY_EXTENSIONS: &[&str] = &[
    "7z", "aac", "apk", "app", "avi", "avif", "bmp", "bz2", "class", "deb", "dll", "dmg", "doc",
    "docx", "dylib", "epub", "exe", "flac", "gif", "gz", "heic", "ico", "img", "iso", "jar", "jpeg",
    "jpg", "m4a", "m4v", "mkv", "mov", "mp3", "mp4", "mpg", "msi", "odt", "ogg", "opus", "otf",
    "pkg", "png", "ppt", "pptx", "psd", "pyc", "rar", "rpm", "so", "sqlite", "tar", "tgz", "tif",
    "tiff", "ttf", "vmdk", "wav", "webm", "webp", "woff", "woff2", "xls", "xlsx", "xz", "zip",
    "zst",
];

fn has_binary_extension(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(stem, ext)| {
        !stem.is_empty() && BINARY_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str())
    })
}

/// Folders the scan never descends into: VCS metadata and build output.
const SKIPPED_ENTRIES: &[&str] = &[".git", ".hg", ".svn", ".DS_Store", "node_modules", "target"];

/// Modification time of `path` in Unix seconds, as the sidebar stores it.
pub fn modified_secs(path: &Path) -> Option<u64> {
    let modified = fs::metadata(path).and_then(|meta| meta.modified()).ok()?;
    modified.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs())
}

/// Folders nested deeper than this below the notes folder are not listed.
pub const MAX_SCAN_DEPTH: usize = 64;
/// The scan stops after listing this many folders and notes.
pub const MAX_SCAN_ITEMS: usize = 50_000;

/// Shown when [`WorkspaceDirectory::scan_truncated`] is set. The numbers
/// match [`MAX_SCAN_ITEMS`] and [`MAX_SCAN_DEPTH`] (checked by a test).
pub fn scan_truncated_message() -> String {
    "This folder is too large to list completely. The sidebar shows up to 50,000 items \
     and folders up to 64 levels deep."
        .to_string()
}

/// One listed child of a folder, read and released before the scan goes
/// deeper, so no directory handle stays open across levels.
struct ScanEntry {
    name: String,
    path: PathBuf,
    kind: ScanKind,
}

enum ScanKind {
    Folder,
    File {
        file_type: FileType,
        updated_at: u64,
    },
}

/// The listed children of `dir`, sorted by name. Unreadable folders are
/// empty. Symlinks are never listed: a link back to a parent would loop,
/// and a link to /etc would let rename and trash act outside the notes
/// folder.
fn read_scan_entries(dir: &Path, show_code_files: bool) -> Vec<ScanEntry> {
    let mut entries: Vec<ScanEntry> = listed_entries(dir, show_code_files).collect();
    // Each DirEntry keeps its directory open on Unix; all of them are
    // dropped here, before any child folder is read.
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

/// The children of `dir` the sidebar lists, unsorted, read lazily.
fn listed_entries(dir: &Path, show_code_files: bool) -> impl Iterator<Item = ScanEntry> {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(move |entry| listed_entry(entry, show_code_files))
}

/// `entry` as the sidebar lists it, or `None` if it is left out.
fn listed_entry(entry: fs::DirEntry, show_code_files: bool) -> Option<ScanEntry> {
    let name = entry.file_name().to_string_lossy().to_string();
    // Temp files of an interrupted save are not notes.
    if SKIPPED_ENTRIES.contains(&name.as_str()) || name.ends_with(TEMP_SUFFIX) {
        return None;
    }
    // DirEntry::file_type does not follow symlinks.
    let Ok(entry_type) = entry.file_type() else {
        return None;
    };
    let kind = if entry_type.is_dir() {
        ScanKind::Folder
    } else if entry_type.is_file() {
        let file_type = FileType::from_filename(&name);
        let listed = match file_type {
            FileType::Markdown => true,
            FileType::PlainText => !has_binary_extension(&name),
            FileType::Code => show_code_files,
            FileType::Pdf => false,
        };
        if !listed {
            return None;
        }
        let updated_at = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        ScanKind::File {
            file_type,
            updated_at,
        }
    } else {
        return None;
    };
    Some(ScanEntry {
        name,
        path: entry.path(),
        kind,
    })
}

/// Whether `dir` holds anything the sidebar would list. A folder holding
/// only `.DS_Store`, a PDF or a symlink has nothing to show, so leaving it
/// unread past the depth limit hides nothing.
fn has_entries(dir: &Path, show_code_files: bool) -> bool {
    listed_entries(dir, show_code_files).next().is_some()
}

/// List the notes folder. Dotfiles are listed; only `SKIPPED_ENTRIES` are left out.
/// Code files are listed only when `show_code_files` is set.
///
/// The walk uses its own stack instead of recursion, so a deeply nested
/// folder can't overflow the thread's stack, and holds no directory handle
/// open while it goes deeper. It stops at [`MAX_SCAN_DEPTH`] levels and
/// [`MAX_SCAN_ITEMS`] items and then sets `scan_truncated`. Items come out in
/// the same order as a depth-first walk: each folder, then its contents.
pub fn scan_workspace_dir(dir: &Path, show_code_files: bool) -> WorkspaceDirectory {
    scan_workspace_dir_limited(dir, show_code_files, MAX_SCAN_DEPTH, MAX_SCAN_ITEMS)
}

fn scan_workspace_dir_limited(
    dir: &Path,
    show_code_files: bool,
    max_depth: usize,
    max_items: usize,
) -> WorkspaceDirectory {
    let folder_name = if let Some(name) = dir.file_name().and_then(|f| f.to_str()) {
        format!("📁 {}", name)
    } else {
        "📁 Notes".to_string()
    };

    let mut folders = Vec::new();
    let mut files = Vec::new();
    let mut truncated = false;

    // Per open folder: its remaining children, its id, and how deep they sit
    // (children of the notes folder are at depth 1).
    let mut stack: Vec<(std::vec::IntoIter<ScanEntry>, Option<String>, usize)> =
        vec![(read_scan_entries(dir, show_code_files).into_iter(), None, 1)];
    'walk: while let Some((children, parent_id, depth)) = stack.last_mut() {
        let Some(entry) = children.next() else {
            stack.pop();
            continue;
        };
        if folders.len() + files.len() >= max_items {
            truncated = true;
            break 'walk;
        }
        let parent_id = parent_id.clone();
        let depth = *depth;
        match entry.kind {
            ScanKind::Folder => {
                let folder_id = format!("folder-{}", entry.path.display());
                if depth < max_depth {
                    let children = read_scan_entries(&entry.path, show_code_files);
                    stack.push((children.into_iter(), Some(folder_id.clone()), depth + 1));
                } else if has_entries(&entry.path, show_code_files) {
                    truncated = true;
                }
                folders.push(WorkspaceFolder {
                    id: folder_id,
                    name: entry.name,
                    parent_id,
                    path: entry.path,
                    is_open: true,
                });
            }
            ScanKind::File {
                file_type,
                updated_at,
            } => files.push(WorkspaceFile {
                id: format!("file-{}", entry.path.display()),
                name: entry.name,
                folder_id: parent_id,
                path: entry.path,
                file_type,
                updated_at,
            }),
        }
    }

    WorkspaceDirectory {
        folder_name,
        folder_path: dir.to_path_buf(),
        is_linked_to_local_disk: true,
        folders,
        files,
        scan_truncated: truncated,
    }
}

pub fn pick_open_file() -> Option<PathBuf> {
    let text_and_pdf: Vec<&str> = TEXT_EXTENSIONS.iter().copied().chain(["pdf"]).collect();
    let all_supported: Vec<&str> = text_and_pdf
        .iter()
        .copied()
        .chain(CODE_EXTENSIONS.iter().copied())
        .collect();
    rfd::FileDialog::new()
        .set_title("Open Text, Markdown, Code, or PDF File")
        .add_filter("All Supported", &all_supported)
        .add_filter("Text & Markdown Files", &text_and_pdf)
        .add_filter("Code Files", CODE_EXTENSIONS)
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
    fn binary_extensions_hide_media_but_not_generic_names() {
        for name in ["photo.png", "Clip.MP4", "disk.iso", "notes.sqlite"] {
            assert!(has_binary_extension(name), "{name}");
        }
        // Text files use these too; they stay listed.
        for name in ["data.dat", "lib.a", "main.o", "notes.db", "config.bin", ".png", "README"] {
            assert!(!has_binary_extension(name), "{name}");
        }
    }

    #[test]
    fn test_sanitize_name() {
        // Separators are replaced, so the result is always one path component.
        assert_eq!(sanitize_name("../../etc/passwd"), ".._.._etc_passwd");
        assert_eq!(sanitize_name("My Notes/File.md"), "My Notes_File.md");
        assert_eq!(sanitize_name("a\\b\0c"), "a_b_c");
        // Leading dots are kept (audit F22); trailing dots and spaces are not.
        assert_eq!(sanitize_name(".gitignore"), ".gitignore");
        assert_eq!(sanitize_name(" .hidden"), ".hidden");
        assert_eq!(sanitize_name("..hidden.."), "..hidden");
        assert_eq!(sanitize_name("notes. . "), "notes");
        assert_eq!(sanitize_name("."), "");
        assert_eq!(sanitize_name(".."), "");
        assert_eq!(sanitize_name(" ... "), "");
        assert_eq!(sanitize_name("  "), "");
        // Device names are still caught once trailing dots go.
        assert_eq!(sanitize_name("CON."), "CON_");
    }

    #[test]
    fn sanitize_name_drops_characters_windows_refuses() {
        assert_eq!(sanitize_name(r#"a<b>c:d"e|f?g*h.md"#), "a_b_c_d_e_f_g_h.md");
        assert_eq!(sanitize_name("x\ny.md"), "x_y.md");
        assert_eq!(sanitize_name("tab\there"), "tab_here");
        assert_eq!(sanitize_name("C:\\notes\\a.md"), "C__notes_a.md");
    }

    #[test]
    fn sanitize_name_avoids_windows_device_names() {
        assert_eq!(sanitize_name("CON"), "CON_");
        assert_eq!(sanitize_name("con.md"), "con_.md");
        assert_eq!(sanitize_name("Nul.tar.gz"), "Nul_.tar.gz");
        assert_eq!(sanitize_name("COM1.md"), "COM1_.md");
        assert_eq!(sanitize_name("lpt9"), "lpt9_");
        assert_eq!(sanitize_name("COM\u{b9}.md"), "COM\u{b9}_.md");
        // Only the exact names; longer words are fine.
        assert_eq!(sanitize_name("CONTACTS.md"), "CONTACTS.md");
        assert_eq!(sanitize_name("COM10.md"), "COM10.md");
        assert_eq!(sanitize_name("console.md"), "console.md");
        assert_eq!(sanitize_name("my con.md"), "my con.md");
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

    #[cfg(unix)]
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
    fn safe_rename_preserves_extension_when_omitted() {
        let temp_dir = std::env::temp_dir().join(format!("litemark-rename-ext-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let file = safe_create_file(&temp_dir, "Draft.md", "hello").unwrap();
        let renamed = safe_rename(&file, "FinalDraft").unwrap();
        assert_eq!(renamed.file_name().unwrap(), "FinalDraft.md");
        assert!(renamed.exists());

        // Explicit new extension is respected
        let txt = safe_rename(&renamed, "FinalDraft.txt").unwrap();
        assert_eq!(txt.file_name().unwrap(), "FinalDraft.txt");
        assert!(txt.exists());

        // A dot mid-name is not an extension: "v1.2 notes" keeps ".md"
        let dotted = safe_create_file(&temp_dir, "n.md", "dotted").unwrap();
        let renamed_dotted = safe_rename(&dotted, "v1.2 notes").unwrap();
        assert_eq!(renamed_dotted.file_name().unwrap(), "v1.2 notes.md");

        // An explicit note extension in a dotted name is respected as-is
        let renamed_md = safe_rename(&renamed_dotted, "v1.2.final.md").unwrap();
        assert_eq!(renamed_md.file_name().unwrap(), "v1.2.final.md");

        // Folder rename does NOT add an extension
        let folder = safe_create_folder(&temp_dir, "MyDocs").unwrap();
        let renamed_folder = safe_rename(&folder, "Archive").unwrap();
        assert_eq!(renamed_folder.file_name().unwrap(), "Archive");
        assert!(renamed_folder.is_dir());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    fn nested_tree(root: &Path, levels: usize) -> PathBuf {
        let mut path = root.to_path_buf();
        for _ in 0..levels {
            path.push("d");
        }
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("deepest.md"), "bottom").unwrap();
        path
    }

    /// The old recursive walk overflowed small stacks (1 MiB on the Windows
    /// main thread) on deep trees. The loop needs the same stack at any depth.
    #[test]
    fn deep_folder_tree_scans_on_a_small_stack() {
        let root = std::env::temp_dir().join(format!("litemark-deepscan-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        nested_tree(&root, 300);

        let scan_root = root.clone();
        let (full, capped) = std::thread::Builder::new()
            .stack_size(128 * 1024)
            .spawn(move || {
                (
                    scan_workspace_dir_limited(&scan_root, true, 1_000, MAX_SCAN_ITEMS),
                    scan_workspace_dir(&scan_root, true),
                )
            })
            .unwrap()
            .join()
            .unwrap();

        assert_eq!(full.folders.len(), 300);
        assert_eq!(full.files.len(), 1);
        assert!(!full.scan_truncated);

        assert_eq!(capped.folders.len(), MAX_SCAN_DEPTH);
        assert!(capped.files.is_empty());
        assert!(capped.scan_truncated);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn scan_stops_at_the_item_limit() {
        let root = std::env::temp_dir().join(format!("litemark-scanitems-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        for i in 0..10 {
            fs::write(root.join(format!("note-{i}.md")), "x").unwrap();
        }
        let scan = scan_workspace_dir_limited(&root, true, MAX_SCAN_DEPTH, 4);
        assert_eq!(scan.files.len(), 4);
        assert!(scan.scan_truncated);
        let names: Vec<_> = scan.files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["note-0.md", "note-1.md", "note-2.md", "note-3.md"]);

        let exact = scan_workspace_dir_limited(&root, true, MAX_SCAN_DEPTH, 10);
        assert_eq!(exact.files.len(), 10);
        assert!(!exact.scan_truncated);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn empty_folder_at_the_depth_limit_is_not_a_truncation() {
        let root = std::env::temp_dir().join(format!("litemark-scanedge-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("a").join("b")).unwrap();
        let scan = scan_workspace_dir_limited(&root, true, 2, MAX_SCAN_ITEMS);
        assert_eq!(scan.folders.len(), 2);
        assert!(!scan.scan_truncated);

        // Files the sidebar never lists don't count as hidden items.
        let deep = root.join("a").join("b");
        fs::write(deep.join(".DS_Store"), "x").unwrap();
        fs::write(deep.join("paper.pdf"), "x").unwrap();
        fs::write(deep.join("main.rs"), "x").unwrap();
        let scan = scan_workspace_dir_limited(&root, false, 2, MAX_SCAN_ITEMS);
        assert!(!scan.scan_truncated, "nothing listable below the limit");
        let scan = scan_workspace_dir_limited(&root, true, 2, MAX_SCAN_ITEMS);
        assert!(scan.scan_truncated, "code files are listed when shown");
        fs::remove_file(deep.join("main.rs")).unwrap();

        fs::write(root.join("a").join("b").join("hidden.md"), "x").unwrap();
        let scan = scan_workspace_dir_limited(&root, true, 2, MAX_SCAN_ITEMS);
        assert!(scan.files.is_empty());
        assert!(scan.scan_truncated);
        let _ = fs::remove_dir_all(&root);
    }

    /// Folders come out before their contents, in name order, as the old
    /// recursive walk produced them.
    #[test]
    fn scan_keeps_depth_first_name_order() {
        let root = std::env::temp_dir().join(format!("litemark-scanorder-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("b").join("inner")).unwrap();
        fs::create_dir_all(root.join("a")).unwrap();
        fs::write(root.join("b").join("inner").join("deep.md"), "x").unwrap();
        fs::write(root.join("b").join("note.md"), "x").unwrap();
        fs::write(root.join("a").join("first.md"), "x").unwrap();
        fs::write(root.join("top.md"), "x").unwrap();

        let scan = scan_workspace_dir(&root, true);
        let folders: Vec<_> = scan.folders.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(folders, ["a", "b", "inner"]);
        let files: Vec<_> = scan.files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(files, ["first.md", "deep.md", "note.md", "top.md"]);
        let inner = scan.folders.iter().find(|f| f.name == "inner").unwrap();
        assert_eq!(
            inner.parent_id.as_deref(),
            Some(scan.folders[1].id.as_str())
        );
        let deep = scan.files.iter().find(|f| f.name == "deep.md").unwrap();
        assert_eq!(deep.folder_id.as_deref(), Some(inner.id.as_str()));
        assert!(!scan.scan_truncated);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn truncation_message_names_the_limits() {
        let message = scan_truncated_message();
        assert_eq!(MAX_SCAN_ITEMS, 50_000);
        assert_eq!(MAX_SCAN_DEPTH, 64);
        assert!(message.contains("50,000 items"), "{message}");
        assert!(message.contains("64 levels"), "{message}");
    }

    #[cfg(unix)]
    #[test]
    fn scan_does_not_follow_symlinks() {
        use std::os::unix::fs::symlink;

        let temp_dir = std::env::temp_dir().join(format!("litemark-symlink-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        // A real folder with a note, plus a link back up to the workspace
        // root (would recurse forever if followed).
        let real = safe_create_folder(&temp_dir, "Real").unwrap();
        safe_create_file(&real, "Note.md", "real").unwrap();
        symlink(&temp_dir, temp_dir.join("loop")).unwrap();
        // A link to a directory outside the workspace (would let rename and
        // trash act outside the notes folder if followed).
        symlink("/etc", temp_dir.join("etc-link")).unwrap();
        // A broken link must not crash the scan.
        symlink(temp_dir.join("does-not-exist"), temp_dir.join("broken")).unwrap();

        let scanned = scan_workspace_dir(&temp_dir, true);
        assert_eq!(scanned.folders.len(), 1, "only the real folder is listed");
        assert_eq!(scanned.folders[0].name, "Real");
        assert_eq!(scanned.files.len(), 1, "only the real note is indexed");
        assert_eq!(scanned.files[0].name, "Note.md");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[cfg(unix)]
    #[test]
    fn copy_folder_skips_symlinks_and_cleans_up() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!("litemark-copysym-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        let src = safe_create_folder(&root, "Src").unwrap();
        safe_create_file(&src, "Note.md", "content").unwrap();
        // A broken symlink previously failed the copy halfway, leaving a
        // partial folder behind.
        symlink(src.join("missing-file"), src.join("broken-link")).unwrap();

        let dst_parent = safe_create_folder(&root, "Dst").unwrap();
        let copied = copy_folder(&src, &dst_parent).unwrap();
        assert!(copied.is_dir());
        assert_eq!(fs::read_to_string(copied.join("Note.md")).unwrap(), "content");
        assert!(!copied.join("broken-link").exists(), "symlinks are not copied");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn creating_folder_preserves_existing_notes() {
        let temp_dir =
            std::env::temp_dir().join(format!("litemark-workflow-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let welcome = safe_create_file(&temp_dir, "Welcome.md", "# Welcome").unwrap();
        let nested = safe_create_folder(&temp_dir, "Existing").unwrap();
        let nested_note = safe_create_file(&nested, "Inner.md", "inner").unwrap();

        // Simulate the new-folder workflow: create one more folder, then rescan.
        safe_create_folder(&temp_dir, "New Folder").unwrap();
        let scanned = scan_workspace_dir(&temp_dir, true);

        let file_paths: Vec<_> = scanned.files.iter().map(|f| f.path.clone()).collect();
        assert!(file_paths.contains(&welcome), "existing root note must survive");
        assert!(file_paths.contains(&nested_note), "nested note must survive");
        assert_eq!(scanned.folders.len(), 2, "old and new folders are both listed");

        // Content is untouched.
        assert_eq!(fs::read_to_string(&welcome).unwrap(), "# Welcome");
        assert_eq!(fs::read_to_string(&nested_note).unwrap(), "inner");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn copy_note_into_workspace_handles_collisions() {
        let src_dir = std::env::temp_dir().join(format!("litemark-copy-src-{}", std::process::id()));
        let dst_dir = std::env::temp_dir().join(format!("litemark-copy-dst-{}", std::process::id()));
        let _ = fs::remove_dir_all(&src_dir);
        let _ = fs::remove_dir_all(&dst_dir);
        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&dst_dir).unwrap();

        let src = safe_create_file(&src_dir, "Welcome.md", "current draft").unwrap();
        let copied = copy_note_into_workspace(&src, &dst_dir, "current draft").unwrap();
        assert_eq!(copied.file_name().unwrap(), "Welcome.md");
        assert_eq!(fs::read_to_string(&copied).unwrap(), "current draft");
        assert!(src.exists(), "the original must be left untouched");

        // Second copy picks the next free name.
        let second = copy_note_into_workspace(&src, &dst_dir, "again").unwrap();
        assert_eq!(second.file_name().unwrap(), "Welcome 2.md");
        assert_eq!(fs::read_to_string(&second).unwrap(), "again");

        let _ = fs::remove_dir_all(&src_dir);
        let _ = fs::remove_dir_all(&dst_dir);
    }

    #[test]
    fn move_note_relocates_and_uniquifies() {
        let root =
            std::env::temp_dir().join(format!("litemark-move-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let src_dir = root.join("src");
        let dst_dir = root.join("dst");
        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&dst_dir).unwrap();

        let src = safe_create_file(&src_dir, "Note.md", "data").unwrap();
        let moved = move_note(&src, &dst_dir).unwrap();
        assert_eq!(moved, dst_dir.join("Note.md"));
        assert_eq!(fs::read_to_string(&moved).unwrap(), "data");
        assert!(!src.exists(), "move must not leave the original behind");

        // Same-folder move is a no-op (must NOT create "Note 2.md").
        let again = move_note(&moved, &dst_dir).unwrap();
        assert_eq!(again, moved);

        // Collision picks the next free name instead of clobbering.
        let other = safe_create_file(&src_dir, "Note.md", "other").unwrap();
        let second = move_note(&other, &dst_dir).unwrap();
        assert_eq!(second, dst_dir.join("Note 2.md"));
        assert_eq!(fs::read_to_string(&second).unwrap(), "other");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn move_and_copy_folder_relocate_contents() {
        let root = std::env::temp_dir().join(format!("litemark-fmove-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let src_dir = root.join("src");
        let dst_dir = root.join("dst");
        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&dst_dir).unwrap();

        let folder = safe_create_folder(&src_dir, "Docs").unwrap();
        safe_create_file(&folder, "A.md", "alpha").unwrap();
        let sub = safe_create_folder(&folder, "sub").unwrap();
        safe_create_file(&sub, "B.md", "beta").unwrap();

        // Move the whole folder into dst.
        let moved = move_folder(&folder, &dst_dir).unwrap();
        assert_eq!(moved, dst_dir.join("Docs"));
        assert!(!folder.exists(), "move must not leave the original behind");
        assert_eq!(fs::read_to_string(moved.join("A.md")).unwrap(), "alpha");
        assert_eq!(fs::read_to_string(moved.join("sub/B.md")).unwrap(), "beta");

        // Same-parent move is a no-op (must NOT create "Docs 2").
        assert_eq!(move_folder(&moved, &dst_dir).unwrap(), moved);

        // Copy duplicates the whole tree under a non-colliding name.
        let copied = copy_folder(&moved, &dst_dir).unwrap();
        assert_eq!(copied, dst_dir.join("Docs 2"));
        assert_eq!(fs::read_to_string(copied.join("A.md")).unwrap(), "alpha");
        assert_eq!(fs::read_to_string(copied.join("sub/B.md")).unwrap(), "beta");
        assert!(moved.exists(), "copy must not remove the original");

        let _ = fs::remove_dir_all(&root);
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

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[cfg(unix)]
    #[test]
    fn atomic_save_keeps_hard_links_together() {
        use std::os::unix::fs::MetadataExt;

        let temp_dir =
            std::env::temp_dir().join(format!("litemark-hardlink-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        let a = temp_dir.join("note.md");
        let b = temp_dir.join("same-note-other-name.md");
        fs::write(&a, "a much longer first version").unwrap();
        fs::hard_link(&a, &b).unwrap();
        let ino = fs::metadata(&a).unwrap().ino();

        atomic_save(&a, "v2").unwrap();

        let meta = fs::metadata(&a).unwrap();
        assert_eq!(meta.ino(), ino);
        assert_eq!(meta.nlink(), 2);
        assert_eq!(fs::read_to_string(&a).unwrap(), "v2");
        assert_eq!(fs::read_to_string(&b).unwrap(), "v2");
        let leftovers = fs::read_dir(&temp_dir)
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().contains("litemark-tmp"))
            .count();
        assert_eq!(leftovers, 0);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[cfg(unix)]
    #[test]
    fn atomic_save_still_replaces_a_plain_file() {
        use std::os::unix::fs::MetadataExt;

        let temp_dir =
            std::env::temp_dir().join(format!("litemark-replace-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        let a = temp_dir.join("note.md");
        fs::write(&a, "v1").unwrap();
        let ino = fs::metadata(&a).unwrap().ino();
        atomic_save(&a, "v2").unwrap();
        assert_ne!(
            fs::metadata(&a).unwrap().ino(),
            ino,
            "saved through a temp file"
        );
        assert_eq!(fs::read_to_string(&a).unwrap(), "v2");
        let _ = fs::remove_dir_all(&temp_dir);
    }

    /// Ownership can only be given away with privileges, so this runs as
    /// root (CI containers) and is skipped otherwise.
    #[cfg(unix)]
    #[test]
    fn atomic_save_keeps_owner_and_group_when_allowed() {
        use std::os::unix::fs::MetadataExt;

        let temp_dir = std::env::temp_dir().join(format!("litemark-owner-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        let a = temp_dir.join("shared.md");
        fs::write(&a, "v1").unwrap();
        if std::os::unix::fs::chown(&a, Some(1234), Some(4321)).is_err() {
            let _ = fs::remove_dir_all(&temp_dir);
            return;
        }
        atomic_save(&a, "v2").unwrap();
        let meta = fs::metadata(&a).unwrap();
        assert_eq!((meta.uid(), meta.gid()), (1234, 4321));
        assert_eq!(fs::read_to_string(&a).unwrap(), "v2");
        let _ = fs::remove_dir_all(&temp_dir);
    }

    /// A read-only file is refused rather than replaced. Root may write
    /// anything, so the check is skipped there.
    #[cfg(unix)]
    #[test]
    fn atomic_save_refuses_a_read_only_file() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir =
            std::env::temp_dir().join(format!("litemark-readonly-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        let a = temp_dir.join("locked.md");
        fs::write(&a, "v1").unwrap();
        fs::set_permissions(&a, fs::Permissions::from_mode(0o444)).unwrap();
        if OpenOptions::new().write(true).open(&a).is_ok() {
            let _ = fs::remove_dir_all(&temp_dir);
            return;
        }
        let err = atomic_save(&a, "v2").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(fs::read_to_string(&a).unwrap(), "v1");
        let _ = fs::remove_dir_all(&temp_dir);
    }

    /// A writable file in a folder that doesn't allow new files is written
    /// in place. (As root the folder allows it anyway and the normal path runs.)
    #[cfg(unix)]
    #[test]
    fn atomic_save_writes_in_place_when_the_folder_is_read_only() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = std::env::temp_dir().join(format!("litemark-rodir-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        let dir = temp_dir.join("locked");
        fs::create_dir_all(&dir).unwrap();
        let a = dir.join("note.md");
        fs::write(&a, "a longer first version").unwrap();
        fs::set_permissions(&a, fs::Permissions::from_mode(0o666)).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).unwrap();

        let saved = atomic_save(&a, "v2");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
        saved.unwrap();
        assert_eq!(fs::read_to_string(&a).unwrap(), "v2");
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn disk_change_notices_edits_and_deletes_but_not_touches() {
        let temp_dir =
            std::env::temp_dir().join(format!("litemark-diskchange-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        let a = temp_dir.join("note.md");
        fs::write(&a, "seen").unwrap();
        let seen = disk_version(&a, "seen").unwrap();
        assert_eq!(disk_change(&a, &seen), DiskChange::Unchanged);

        let file = OpenOptions::new().write(true).open(&a).unwrap();
        file.set_modified(UNIX_EPOCH + std::time::Duration::from_secs(1_000_000))
            .unwrap();
        drop(file);
        assert_eq!(disk_change(&a, &seen), DiskChange::Unchanged);

        fs::write(&a, "same length? no").unwrap();
        assert_eq!(disk_change(&a, &seen), DiskChange::Changed);
        fs::write(&a, "SEEN").unwrap();
        assert_eq!(disk_change(&a, &seen), DiskChange::Changed);

        fs::remove_file(&a).unwrap();
        assert_eq!(disk_change(&a, &seen), DiskChange::Deleted);
        assert!(disk_version(&a, "seen").is_none());
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn portable_rename_moves_and_never_replaces() {
        let dir = std::env::temp_dir().join(format!("litemark-portable-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let from = dir.join("from.md");
        let to = dir.join("to.md");
        fs::write(&from, "new").unwrap();

        rename_noreplace_portable(&from, &to).unwrap();
        assert!(!from.exists());
        assert_eq!(fs::read_to_string(&to).unwrap(), "new");

        fs::write(&from, "other").unwrap();
        let err = rename_noreplace_portable(&from, &to).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(&to).unwrap(), "new", "existing file untouched");
        assert!(from.exists(), "source stays when the target is taken");

        let _ = fs::remove_dir_all(&dir);
    }

    fn no_hard_links(_: &Path, _: &Path) -> io::Result<()> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }

    #[test]
    fn portable_rename_copy_keeps_content_and_permissions() {
        let dir = std::env::temp_dir().join(format!("litemark-portable-copy-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let from = dir.join("from.md");
        let to = dir.join("to.md");
        let text = "line\n".repeat(100_000);
        fs::write(&from, &text).unwrap();
        let mut perms = fs::metadata(&from).unwrap().permissions();
        #[cfg(unix)]
        perms.set_mode(0o600);
        #[cfg(not(unix))]
        perms.set_readonly(false);
        fs::set_permissions(&from, perms.clone()).unwrap();

        rename_noreplace_portable_with(&from, &to, no_hard_links, |p| fs::remove_file(p)).unwrap();
        assert!(!from.exists());
        assert_eq!(fs::read_to_string(&to).unwrap(), text);
        #[cfg(unix)]
        assert_eq!(fs::metadata(&to).unwrap().permissions().mode() & 0o777, 0o600);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn portable_rename_undoes_the_new_name_when_the_source_stays() {
        let dir = std::env::temp_dir().join(format!("litemark-portable-undo-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let from = dir.join("from.md");
        let to = dir.join("to.md");
        let stuck = |_: &Path| Err(io::Error::from(io::ErrorKind::PermissionDenied));

        for link_fails in [false, true] {
            fs::write(&from, "note").unwrap();
            let err = if link_fails {
                rename_noreplace_portable_with(&from, &to, no_hard_links, stuck)
            } else {
                rename_noreplace_portable_with(&from, &to, |a, b| fs::hard_link(a, b), stuck)
            }
            .unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
            assert_eq!(fs::read_to_string(&from).unwrap(), "note", "source kept");
            assert!(!to.exists(), "no duplicate left behind (copy path: {link_fails})");
        }

        let _ = fs::remove_dir_all(&dir);
    }

    /// Audit F11: on a filesystem without `RENAME_NOREPLACE` the fallback
    /// still moves files and folders and still refuses to replace.
    #[cfg(target_os = "linux")]
    #[test]
    fn unsupported_renameat2_falls_back_for_files_and_folders() {
        let dir = std::env::temp_dir().join(format!("litemark-nofs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for errno in [ENOSYS, EINVAL] {
            let unsupported = |_: &Path, _: &Path| Err(io::Error::from_raw_os_error(errno));
            let (from, to) = (dir.join("a.md"), dir.join("b.md"));
            fs::write(&from, "note").unwrap();
            rename_noreplace_with(&from, &to, unsupported).unwrap();
            assert!(!from.exists());
            assert_eq!(fs::read_to_string(&to).unwrap(), "note");

            fs::write(&from, "other").unwrap();
            let err = rename_noreplace_with(&from, &to, unsupported).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
            assert_eq!(fs::read_to_string(&to).unwrap(), "note");

            let (src, dest) = (dir.join("folder"), dir.join("moved"));
            fs::create_dir(&src).unwrap();
            fs::write(src.join("inside.md"), "x").unwrap();
            rename_noreplace_with(&src, &dest, unsupported).unwrap();
            assert!(dest.join("inside.md").exists() && !src.exists());

            for path in [&from, &to] {
                let _ = fs::remove_file(path);
            }
            fs::remove_dir_all(&dest).unwrap();
        }
        // Other errors are passed through, not retried.
        let (from, to) = (dir.join("c.md"), dir.join("d.md"));
        fs::write(&from, "c").unwrap();
        let denied = |_: &Path, _: &Path| Err(io::Error::from_raw_os_error(13));
        let err = rename_noreplace_with(&from, &to, denied).unwrap_err();
        assert_eq!(err.raw_os_error(), Some(13));
        assert!(from.exists() && !to.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    /// Audit F22: the temp name no longer grows with the target's name, so
    /// names near the 255-byte limit save, and temp files stay out of the
    /// sidebar.
    #[test]
    fn long_names_save_and_temp_files_are_not_listed() {
        let dir = std::env::temp_dir().join(format!("litemark-longname-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let name = format!("{}.md", "n".repeat(250));
        assert_eq!(name.len(), 253);
        let path = dir.join(&name);
        fs::write(&path, "old").unwrap();
        atomic_save(&path, "new").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        let saved = atomic_save_new(&dir, &format!("{}.md", "m".repeat(250)), "x").unwrap();
        assert_eq!(fs::read_to_string(saved).unwrap(), "x");

        let temp = write_temp(&path, "pending").unwrap();
        let temp_name = temp.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(temp_name.len(), 30, "{temp_name}");
        let listed = scan_workspace_dir(&dir, false);
        assert!(listed
            .files
            .iter()
            .all(|file| !file.name.ends_with(TEMP_SUFFIX)));
        assert_eq!(listed.files.len(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rename_keeps_leading_dots() {
        let dir = std::env::temp_dir().join(format!("litemark-dotname-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("gitignore");
        fs::write(&file, "target/").unwrap();
        let renamed = safe_rename(&file, ".gitignore").unwrap();
        assert_eq!(renamed.file_name().unwrap(), ".gitignore");
        let err = safe_rename(&renamed, "..").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(target_os = "linux")]
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

        #[cfg(unix)]
        {
            let mode = fs::metadata(&written).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o644);
        }

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

    #[test]
    fn nul_bytes_make_a_file_unopenable() {
        let dir = std::env::temp_dir().join(format!("litemark-bin-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let bin = dir.join("blob.txt");
        fs::write(&bin, b"ok\0text").unwrap();
        let err = read_note(&bin).unwrap_err();
        assert!(err.contains("binary file"), "{err}");
        let good = dir.join("main.rs");
        fs::write(&good, "fn main() {}").unwrap();
        assert_eq!(read_note(&good).unwrap(), "fn main() {}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn read_note_does_not_hang_on_a_fifo() {
        let dir = std::env::temp_dir().join(format!("litemark-fifo-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let fifo = dir.join("pipe.md");
        let status = std::process::Command::new("mkfifo").arg(&fifo).status().unwrap();
        assert!(status.success());
        let (tx, rx) = std::sync::mpsc::channel();
        let path = fifo.clone();
        std::thread::spawn(move || {
            let _ = tx.send(read_note(&path));
        });
        let result = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("read_note blocked on a FIFO");
        assert!(result.unwrap_err().contains("not a regular file"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_note_rejects_binary_from_the_head_without_reading_it_all() {
        let dir = std::env::temp_dir().join(format!("litemark-binhead-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let text = dir.join("long.txt");
        let content = "line of text\n".repeat(2000);
        fs::write(&text, &content).unwrap();
        assert_eq!(read_note(&text).unwrap(), content);
        let bin = dir.join("disk.txt");
        let file = fs::File::create(&bin).unwrap();
        file.set_len(MAX_NOTE_BYTES - 1).unwrap();
        drop(file);
        assert!(read_note(&bin).unwrap_err().contains("binary"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sidebar_skips_binary_files_but_keeps_text() {
        let dir = std::env::temp_dir().join(format!("litemark-binlist-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for name in ["backup.iso", "photo.JPG", "notes.txt", "data.csv", "README", ".jpg"] {
            fs::write(dir.join(name), "x").unwrap();
        }
        let mut names: Vec<_> = scan_workspace_dir(&dir, false)
            .files
            .into_iter()
            .map(|f| f.name)
            .collect();
        names.sort();
        assert_eq!(names, [".jpg", "README", "data.csv", "notes.txt"]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_note_refuses_oversized_files() {
        let dir = std::env::temp_dir().join(format!("litemark-big-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let big = dir.join("big.md");
        let file = fs::File::create(&big).unwrap();
        file.set_len(MAX_NOTE_BYTES + 1).unwrap();
        drop(file);
        assert!(read_note(&big).unwrap_err().contains("too large"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rename_swaps_between_known_extensions() {
        let dir = std::env::temp_dir().join(format!("litemark-ren-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let rs = dir.join("a.rs");
        fs::write(&rs, "x").unwrap();
        let py = safe_rename(&rs, "b.py").unwrap();
        assert_eq!(py.file_name().unwrap(), "b.py");
        let rs_md = dir.join("x.md");
        fs::write(&rs_md, "x").unwrap();
        let as_rs = safe_rename(&rs_md, "x.rs").unwrap();
        assert_eq!(as_rs.file_name().unwrap(), "x.rs");
        let plain = safe_rename(&as_rs, "notes").unwrap();
        assert_eq!(plain.file_name().unwrap(), "notes.rs");
        let pdf_name = safe_rename(&plain, "scan.pdf").unwrap();
        assert_eq!(pdf_name.file_name().unwrap(), "scan.pdf.rs");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_lists_dotfiles_and_code_files_follow_the_setting() {
        let temp_dir =
            std::env::temp_dir().join(format!("litemark-scan-code-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join(".git")).unwrap();
        fs::create_dir_all(temp_dir.join("node_modules")).unwrap();
        fs::write(temp_dir.join(".git").join("config.txt"), "x").unwrap();
        fs::write(temp_dir.join("node_modules").join("a.js"), "x").unwrap();
        for name in [
            "Note.md",
            "plain.txt",
            "main.rs",
            "Makefile",
            ".env",
            ".gitignore",
            ".bashrc",
        ] {
            fs::write(temp_dir.join(name), "x").unwrap();
        }

        let names = |show: bool| -> Vec<String> {
            scan_workspace_dir(&temp_dir, show)
                .files
                .into_iter()
                .map(|f| f.name)
                .collect()
        };
        assert_eq!(
            names(true),
            [
                ".bashrc",
                ".env",
                ".gitignore",
                "Makefile",
                "Note.md",
                "main.rs",
                "plain.txt"
            ]
        );
        // `.bashrc` is plain text, so it stays when code files are off.
        assert_eq!(names(false), [".bashrc", "Note.md", "plain.txt"]);
        assert!(scan_workspace_dir(&temp_dir, true).folders.is_empty());
        assert_eq!(FileType::from_filename(".env"), FileType::Code);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn access_denied_is_recognized() {
        assert!(is_access_denied("Couldn't save C:\\a.md: Access is denied. (os error 5)"));
        assert!(is_access_denied("Permission denied (os error 13)"));
        assert!(!is_access_denied("Couldn't save: No such file or directory (os error 2)"));
        assert!(is_access_denied("Couldn't save: (os error 5)"));
        assert!(!is_access_denied("Couldn't open: The network path was not found. (os error 53)"));
        assert!(!is_access_denied("Couldn't open: (os error 50)"));
    }

    #[test]
    fn hint_is_added_on_windows_only_for_access_denied() {
        let denied = "Couldn't save a.md: Access is denied. (os error 5)".to_string();
        let other = "Couldn't save a.md: No such file or directory (os error 2)".to_string();
        assert_eq!(with_access_denied_hint(other.clone()), other);
        let shown = with_access_denied_hint(denied.clone());
        if cfg!(windows) {
            assert!(shown.starts_with(&denied));
            assert!(shown.contains("Controlled folder access"));
            // Showing the same error twice must not stack hints.
            assert_eq!(with_access_denied_hint(shown.clone()), shown);
        } else {
            assert_eq!(shown, denied);
        }
    }

    #[test]
    fn show_code_files_defaults_on_for_old_settings_files() {
        let settings: crate::model::EditorSettings = serde_json::from_str("{}").unwrap();
        assert!(settings.show_code_files);
    }
}
