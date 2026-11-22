use super::document::ViewMode;
use crate::services::fs_service::atomic_save;
use crate::theme::ThemeMode;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn default_show_code_files() -> bool {
    true
}

fn default_word_wrap() -> bool {
    true
}

fn default_check_for_updates() -> bool {
    true
}

fn default_base_font_size() -> f32 {
    17.0
}

fn default_zoom_level() -> usize {
    100
}

fn default_show_sidebar() -> bool {
    false
}

fn default_reopen_tabs_on_startup() -> bool {
    true
}

/// Zoom range the zoom in/out commands allow.
pub const ZOOM_RANGE: std::ops::RangeInclusive<usize> = 50..=200;
/// Font sizes the settings menu offers.
pub const FONT_SIZE_RANGE: std::ops::RangeInclusive<f32> = 13.0..=24.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ContentAlignment {
    #[default]
    Centered,
    Left,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorSettings {
    #[serde(default)]
    pub theme: ThemeMode,
    #[serde(default = "default_zoom_level")]
    pub zoom_level: usize,
    #[serde(default)]
    pub view_mode: ViewMode,
    #[serde(default)]
    pub show_line_numbers: bool,
    #[serde(default = "default_show_sidebar")]
    pub show_sidebar: bool,
    #[serde(default)]
    pub zen_mode: bool,
    /// Keep the outline card open at the editor's right edge.
    #[serde(default)]
    pub outline_pinned: bool,
    /// List code files (`.rs`, `.py`, `Makefile`, ...) in the sidebar.
    #[serde(default = "default_show_code_files")]
    pub show_code_files: bool,
    #[serde(default = "default_word_wrap")]
    pub word_wrap: bool,
    #[serde(default = "default_base_font_size")]
    pub base_font_size: f32,
    #[serde(default)]
    pub content_alignment: ContentAlignment,
    /// Workspace folders used before, most recent first (never the current one).
    #[serde(default)]
    pub recent_folders: Vec<PathBuf>,
    /// The workspace folder in use when the app last closed; reopened at
    /// startup while it still exists.
    #[serde(default)]
    pub workspace_folder: Option<PathBuf>,
    /// Look for a newer release on GitHub at startup (at most once a day).
    #[serde(default = "default_check_for_updates")]
    pub check_for_updates: bool,
    /// Unix seconds of the last update check.
    #[serde(default)]
    pub last_update_check: Option<u64>,
    /// Release version whose notice the user closed.
    #[serde(default)]
    pub dismissed_update: Option<String>,
    /// Reopen the notes that were open when the app last closed.
    #[serde(default = "default_reopen_tabs_on_startup")]
    pub reopen_tabs_on_startup: bool,
    /// Files open in tabs, in tab order (untitled tabs and PDFs left out).
    #[serde(default)]
    pub open_tabs: Vec<PathBuf>,
    /// The active tab's file, when it is one of `open_tabs`.
    #[serde(default)]
    pub active_tab: Option<PathBuf>,
    /// Notes opened or switched to, newest first, without duplicates.
    #[serde(default)]
    pub recent_notes: Vec<PathBuf>,
    /// Set when settings.json couldn't be read and couldn't be backed up
    /// either. Saving is refused for the session so the original file is
    /// never overwritten; the text says why.
    #[serde(skip)]
    pub save_blocked: Option<String>,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            theme: ThemeMode::YaruDark,
            zoom_level: 100,
            view_mode: ViewMode::Wysiwyg,
            show_line_numbers: false,
            show_sidebar: false,
            zen_mode: false,
            outline_pinned: false,
            show_code_files: true,
            word_wrap: true,
            base_font_size: 17.0,
            content_alignment: ContentAlignment::Centered,
            recent_folders: Vec::new(),
            workspace_folder: None,
            check_for_updates: true,
            last_update_check: None,
            dismissed_update: None,
            reopen_tabs_on_startup: true,
            open_tabs: Vec::new(),
            active_tab: None,
            recent_notes: Vec::new(),
            save_blocked: None,
        }
    }
}

/// Backups of a damaged settings file kept side by side before giving up.
const MAX_SETTINGS_BACKUPS: usize = 100;

/// Most recent workspace folders kept in the switcher.
pub const MAX_RECENT_FOLDERS: usize = 5;

impl EditorSettings {
    /// Record a workspace switch: the folder just left goes to the front of
    /// the recent list and the one just entered is removed from it.
    pub fn remember_folders(&mut self, left: &std::path::Path, entered: &std::path::Path) {
        self.recent_folders.retain(|p| p != left && p != entered);
        if left != entered {
            self.recent_folders.insert(0, left.to_path_buf());
        }
        self.recent_folders.truncate(MAX_RECENT_FOLDERS);
    }

    /// Drop a folder from the recent list (the folder itself is untouched).
    pub fn forget_folder(&mut self, folder: &std::path::Path) {
        self.recent_folders.retain(|p| p != folder);
    }

    fn config_path() -> Option<PathBuf> {
        let base = dirs::config_dir()?;
        Some(base.join("litemark").join("settings.json"))
    }

    pub fn load() -> Self {
        match Self::config_path() {
            Some(path) => Self::load_from(&path),
            None => Self::default(),
        }
    }

    /// Read settings from `path`. A missing file gives the defaults. A file
    /// that can't be fully read or parsed keeps every field that still
    /// parses, and is copied aside first so the next save can't destroy the
    /// original.
    fn load_from(path: &Path) -> Self {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Self::default(),
            Err(err) => {
                eprintln!("litemark: couldn't read {}: {err}", path.display());
                let backed_up = Self::back_up_unreadable(path).is_some();
                return Self::default().blocked_unless(backed_up, path);
            }
        };
        let mut backed_up = None;
        let content = match String::from_utf8(bytes) {
            Ok(content) => content,
            Err(err) => {
                // Damaged bytes inside one value shouldn't cost the others.
                backed_up = Some(Self::back_up_unreadable(path).is_some());
                String::from_utf8_lossy(err.as_bytes()).into_owned()
            }
        };
        let settings = match serde_json::from_str::<Self>(&content) {
            Ok(settings) => settings,
            Err(_) => {
                if backed_up.is_none() {
                    backed_up = Some(Self::back_up_unreadable(path).is_some());
                }
                Self::salvage(&content)
            }
        };
        settings
            .clamped()
            .blocked_unless(backed_up.unwrap_or(true), path)
    }

    /// Without a backup, block saving for this session: a save would replace
    /// the only copy of the file that couldn't be read.
    fn blocked_unless(mut self, backed_up: bool, path: &Path) -> Self {
        if !backed_up {
            self.save_blocked = Some(format!(
                "Couldn't read or back up {}, so settings won't be saved this session.",
                path.display()
            ));
        }
        self
    }

    /// Pull out-of-range numbers back to what the UI can produce, so a
    /// hand-edited or damaged file can't make text invisible or enormous.
    fn clamped(mut self) -> Self {
        self.zoom_level = self
            .zoom_level
            .clamp(*ZOOM_RANGE.start(), *ZOOM_RANGE.end());
        self.base_font_size = if self.base_font_size.is_finite() {
            self.base_font_size
                .clamp(*FONT_SIZE_RANGE.start(), *FONT_SIZE_RANGE.end())
        } else {
            default_base_font_size()
        };
        self.recent_folders.truncate(MAX_RECENT_FOLDERS);
        super::recent_notes::dedupe_recent(&mut self.recent_notes);
        self
    }

    /// Start from the defaults and apply each stored field that is valid on
    /// its own, so one bad value doesn't reset everything else.
    fn salvage(content: &str) -> Self {
        let mut settings = Self::default();
        let Ok(serde_json::Value::Object(stored)) = serde_json::from_str(content) else {
            return settings;
        };
        for (key, value) in stored {
            let Ok(serde_json::Value::Object(mut merged)) = serde_json::to_value(&settings) else {
                break;
            };
            merged.insert(key, value);
            if let Ok(next) = serde_json::from_value(serde_json::Value::Object(merged)) {
                settings = next;
            }
        }
        settings
    }

    /// Copy `path` to the first free `settings.json.bak`, `.bak.1`, `.bak.2`
    /// and so on, so an earlier backup is never replaced. Returns the copy.
    fn back_up_unreadable(path: &Path) -> Option<PathBuf> {
        let result = (|| {
            let mut source = fs::File::open(path)?;
            for n in 0..MAX_SETTINGS_BACKUPS {
                let mut name = path.as_os_str().to_os_string();
                name.push(if n == 0 {
                    ".bak".to_string()
                } else {
                    format!(".bak.{n}")
                });
                let backup = PathBuf::from(name);
                let mut dest = match fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&backup)
                {
                    Ok(dest) => dest,
                    Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(err) => return Err(err),
                };
                if let Err(err) = io::copy(&mut source, &mut dest) {
                    drop(dest);
                    let _ = fs::remove_file(&backup);
                    return Err(err);
                }
                return Ok(backup);
            }
            Err(io::Error::other("too many settings backups"))
        })();
        match result {
            Ok(backup) => {
                eprintln!(
                    "litemark: {} couldn't be fully read; a copy was kept at {}",
                    path.display(),
                    backup.display()
                );
                Some(backup)
            }
            Err(err) => {
                eprintln!("litemark: couldn't back up {}: {err}", path.display());
                None
            }
        }
    }

    /// Save to the config file. The error names the file, for showing to the user.
    pub fn save(&self) -> Result<(), String> {
        if let Some(reason) = &self.save_blocked {
            return Err(reason.clone());
        }
        let Some(path) = Self::config_path() else {
            return Err("Couldn't save settings: no config folder on this system".into());
        };
        self.save_to(&path)
            .map_err(|err| format!("Couldn't save settings to {}: {err}", path.display()))
    }

    /// Write through a temp file and rename, so a crash mid-write leaves the
    /// previous settings intact instead of a truncated file.
    fn save_to(&self, path: &Path) -> io::Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        atomic_save(path, &json)
    }
}

/// Writes settings on a background thread so `fsync` never blocks the UI.
/// Snapshots sent while a write runs are coalesced: only the newest is saved.
/// After each write the thread sends `()` on the receiver [`Self::spawn`]
/// returns, so the UI can report that write's outcome as soon as it lands.
pub struct SettingsWriter {
    tx: std::sync::mpsc::Sender<WriterMsg>,
    result: std::sync::Arc<std::sync::Mutex<Option<Result<(), String>>>>,
}

enum WriterMsg {
    Save(Box<EditorSettings>),
    Flush(std::sync::mpsc::Sender<()>),
}

impl SettingsWriter {
    pub fn spawn() -> (Self, futures::channel::mpsc::UnboundedReceiver<()>) {
        let (tx, rx) = std::sync::mpsc::channel::<WriterMsg>();
        let (written_tx, written_rx) = futures::channel::mpsc::unbounded::<()>();
        let result = std::sync::Arc::new(std::sync::Mutex::new(None));
        let shared = result.clone();
        let spawned = std::thread::Builder::new()
            .name("settings-writer".into())
            .spawn(move || {
                while let Ok(first) = rx.recv() {
                    let mut latest = None;
                    let mut acks = Vec::new();
                    let mut next = Some(first);
                    while let Some(msg) = next.take().or_else(|| rx.try_recv().ok()) {
                        match msg {
                            WriterMsg::Save(settings) => latest = Some(settings),
                            WriterMsg::Flush(ack) => acks.push(ack),
                        }
                    }
                    if let Some(settings) = latest {
                        if let Ok(mut slot) = shared.lock() {
                            *slot = Some(settings.save());
                        }
                        // Nobody listening (window gone) is fine.
                        let _ = written_tx.unbounded_send(());
                    }
                    for ack in acks {
                        let _ = ack.send(());
                    }
                }
            });
        if spawned.is_err() {
            eprintln!("litemark: could not start the settings writer thread");
        }
        (Self { tx, result }, written_rx)
    }

    /// Queue `settings` for writing.
    pub fn save(&self, settings: &EditorSettings) {
        let _ = self.tx.send(WriterMsg::Save(Box::new(settings.clone())));
    }

    /// The outcome of the latest finished write, once.
    pub fn take_result(&self) -> Option<Result<(), String>> {
        self.result.lock().ok().and_then(|mut slot| slot.take())
    }

    /// Wait until every queued write has reached the disk.
    pub fn flush(&self) {
        let (ack, done) = std::sync::mpsc::channel();
        if self.tx.send(WriterMsg::Flush(ack)).is_ok() {
            let _ = done.recv_timeout(std::time::Duration::from_secs(5));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_writer_wakes_with_each_writes_own_result() {
        // A blocked save fails without touching the disk.
        let blocked = EditorSettings {
            save_blocked: Some("blocked for test".into()),
            ..EditorSettings::default()
        };
        let (writer, mut written) = SettingsWriter::spawn();
        assert!(writer.take_result().is_none());
        writer.save(&blocked);
        // The wake-up comes after the write, carrying that write's outcome.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match written.try_recv() {
                Ok(()) => break,
                Err(_) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(5))
                }
                Err(_) => panic!("writer never woke the UI after a write"),
            }
        }
        assert_eq!(writer.take_result(), Some(Err("blocked for test".into())));
        // Reported once.
        assert_eq!(writer.take_result(), None);
    }

    #[test]
    fn test_content_alignment_serde() {
        let default_settings = EditorSettings::default();
        assert_eq!(
            default_settings.content_alignment,
            ContentAlignment::Centered
        );

        let json = serde_json::to_string(&default_settings).expect("serialize");
        let parsed: EditorSettings = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.content_alignment, ContentAlignment::Centered);

        let custom = r#"{"content_alignment": "Left"}"#;
        let parsed_custom: EditorSettings =
            serde_json::from_str(custom).expect("deserialize custom");
        assert_eq!(parsed_custom.content_alignment, ContentAlignment::Left);
    }

    #[test]
    fn update_checks_default_on_for_old_settings_files() {
        let parsed: EditorSettings = serde_json::from_str("{}").expect("deserialize");
        assert!(parsed.check_for_updates);
        assert_eq!(parsed.last_update_check, None);
        assert_eq!(parsed.dismissed_update, None);
    }

    #[test]
    fn switching_folders_puts_the_old_one_first_without_duplicates() {
        let mut s = EditorSettings::default();
        s.remember_folders(Path::new("/a"), Path::new("/b"));
        s.remember_folders(Path::new("/b"), Path::new("/c"));
        assert_eq!(s.recent_folders, [PathBuf::from("/b"), PathBuf::from("/a")]);
        s.remember_folders(Path::new("/c"), Path::new("/a"));
        assert_eq!(s.recent_folders, [PathBuf::from("/c"), PathBuf::from("/b")]);
    }

    #[test]
    fn recent_folders_are_capped() {
        let mut s = EditorSettings::default();
        for i in 0..10 {
            s.remember_folders(Path::new(&format!("/old{i}")), Path::new("/now"));
        }
        assert_eq!(s.recent_folders.len(), MAX_RECENT_FOLDERS);
        assert_eq!(s.recent_folders[0], PathBuf::from("/old9"));
    }

    #[test]
    fn forgetting_a_recent_folder_removes_only_it() {
        let mut s = EditorSettings::default();
        s.remember_folders(Path::new("/a"), Path::new("/b"));
        s.remember_folders(Path::new("/b"), Path::new("/c"));
        s.forget_folder(Path::new("/a"));
        assert_eq!(s.recent_folders, [PathBuf::from("/b")]);
    }

    #[test]
    fn workspace_folder_survives_a_reload() {
        let s: EditorSettings = serde_json::from_str(r#"{"workspace_folder":"/notes"}"#).unwrap();
        assert_eq!(s.workspace_folder, Some(PathBuf::from("/notes")));
        let none: EditorSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(none.workspace_folder, None);
    }

    fn temp_settings_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("litemark-settings-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = temp_settings_dir("roundtrip");
        let path = dir.join("litemark").join("settings.json");
        let s = EditorSettings {
            zoom_level: 140,
            word_wrap: false,
            ..Default::default()
        };
        s.save_to(&path).unwrap();
        let loaded = EditorSettings::load_from(&path);
        assert_eq!(loaded.zoom_level, 140);
        assert!(!loaded.word_wrap);
        let leftovers: Vec<_> = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("litemark-tmp"))
            .collect();
        assert!(leftovers.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_old_settings_file_loads_with_the_outline_unpinned() {
        let dir = temp_settings_dir("outline-old");
        let path = dir.join("settings.json");
        fs::write(&path, r#"{"zoom_level": 120}"#).unwrap();
        let loaded = EditorSettings::load_from(&path);
        assert_eq!(loaded.zoom_level, 120);
        assert!(!loaded.outline_pinned);
        assert!(!path.with_extension("json.bak").exists());
        fs::write(&path, r#"{"outline_pinned": true}"#).unwrap();
        assert!(EditorSettings::load_from(&path).outline_pinned);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_gives_defaults_without_backup() {
        let dir = temp_settings_dir("missing");
        let path = dir.join("settings.json");
        let loaded = EditorSettings::load_from(&path);
        assert_eq!(loaded.zoom_level, EditorSettings::default().zoom_level);
        assert!(!path.with_extension("json.bak").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn one_bad_field_keeps_the_others_and_backs_up_the_file() {
        let dir = temp_settings_dir("badfield");
        let path = dir.join("settings.json");
        let content = r#"{"theme": "SomeFutureTheme", "zoom_level": 150, "word_wrap": false}"#;
        fs::write(&path, content).unwrap();
        let loaded = EditorSettings::load_from(&path);
        assert_eq!(loaded.zoom_level, 150);
        assert!(!loaded.word_wrap);
        assert_eq!(loaded.theme, EditorSettings::default().theme);
        let backup = path.with_extension("json.bak");
        assert_eq!(fs::read_to_string(backup).unwrap(), content);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn truncated_file_is_backed_up_before_defaults_are_used() {
        let dir = temp_settings_dir("truncated");
        let path = dir.join("settings.json");
        fs::write(&path, r#"{"zoom_level": 150, "word_wr"#).unwrap();
        let loaded = EditorSettings::load_from(&path);
        assert_eq!(loaded.zoom_level, EditorSettings::default().zoom_level);
        assert!(path.with_extension("json.bak").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn bad_utf8_keeps_the_valid_fields_and_backs_up_the_file() {
        let dir = temp_settings_dir("badutf8");
        let path = dir.join("settings.json");
        let mut content = br#"{"zoom_level": 150, "word_wrap": false, "theme": ""#.to_vec();
        content.extend_from_slice(&[0xff, 0xfe]);
        content.extend_from_slice(br#""}"#);
        fs::write(&path, &content).unwrap();
        let loaded = EditorSettings::load_from(&path);
        assert_eq!(loaded.zoom_level, 150);
        assert!(!loaded.word_wrap);
        assert_eq!(fs::read(path.with_extension("json.bak")).unwrap(), content);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_second_damaged_file_does_not_replace_the_first_backup() {
        let dir = temp_settings_dir("twobackups");
        let path = dir.join("settings.json");
        fs::write(&path, "first damage {").unwrap();
        EditorSettings::load_from(&path);
        fs::write(&path, "second damage {").unwrap();
        EditorSettings::load_from(&path);
        assert_eq!(
            fs::read_to_string(dir.join("settings.json.bak")).unwrap(),
            "first damage {"
        );
        assert_eq!(
            fs::read_to_string(dir.join("settings.json.bak.1")).unwrap(),
            "second damage {"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn damaged_file_that_cant_be_backed_up_blocks_saving() {
        let dir = temp_settings_dir("nobackup");
        let path = dir.join("settings.json");
        let damaged = b"{\"zoom_level\": 150, \"theme\": \"\xff\"}";
        fs::write(&path, damaged).unwrap();
        // Every backup name is taken, so the copy can't be made.
        fs::write(dir.join("settings.json.bak"), "old").unwrap();
        for n in 1..MAX_SETTINGS_BACKUPS {
            fs::write(dir.join(format!("settings.json.bak.{n}")), "old").unwrap();
        }
        let loaded = EditorSettings::load_from(&path);
        assert_eq!(loaded.zoom_level, 150, "valid fields still load");
        assert!(loaded.save_blocked.is_some());
        let err = loaded.save().unwrap_err();
        assert!(err.contains("won't be saved this session"), "{err}");
        assert_eq!(fs::read(&path).unwrap(), damaged, "original untouched");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn old_settings_files_load_with_tab_and_recent_defaults() {
        let old = r#"{"theme": "YaruDark", "zoom_level": 120, "word_wrap": false}"#;
        let parsed: EditorSettings = serde_json::from_str(old).expect("deserialize");
        assert_eq!(parsed.zoom_level, 120);
        assert!(parsed.reopen_tabs_on_startup);
        assert!(parsed.open_tabs.is_empty());
        assert_eq!(parsed.active_tab, None);
        assert!(parsed.recent_notes.is_empty());
    }

    #[test]
    fn open_tabs_and_recent_notes_round_trip() {
        let s = EditorSettings {
            reopen_tabs_on_startup: false,
            open_tabs: vec![PathBuf::from("/w/a.md"), PathBuf::from("/w/b.md")],
            active_tab: Some(PathBuf::from("/w/b.md")),
            recent_notes: vec![PathBuf::from("/w/b.md")],
            ..Default::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        let parsed: EditorSettings = serde_json::from_str(&json).unwrap();
        assert!(!parsed.reopen_tabs_on_startup);
        assert_eq!(parsed.open_tabs, s.open_tabs);
        assert_eq!(parsed.active_tab, s.active_tab);
        assert_eq!(parsed.recent_notes, s.recent_notes);
    }

    #[test]
    fn a_hand_edited_recent_list_is_deduped_and_capped_on_load() {
        let dir = temp_settings_dir("recentcap");
        let path = dir.join("settings.json");
        let mut list: Vec<String> = (0..30).map(|i| format!("/n{i}.md")).collect();
        list.insert(1, "/n0.md".into());
        fs::write(
            &path,
            serde_json::json!({ "recent_notes": list }).to_string(),
        )
        .unwrap();
        let loaded = EditorSettings::load_from(&path);
        assert_eq!(loaded.recent_notes.len(), super::super::MAX_RECENT_NOTES);
        assert_eq!(loaded.recent_notes[1], PathBuf::from("/n1.md"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_fields_get_the_real_defaults() {
        let parsed: EditorSettings = serde_json::from_str("{}").expect("deserialize");
        assert_eq!(parsed.zoom_level, 100);
        assert!(!parsed.show_sidebar);
    }

    #[test]
    fn out_of_range_numbers_are_clamped_on_load() {
        let dir = temp_settings_dir("clamp");
        let path = dir.join("settings.json");
        fs::write(
            &path,
            r#"{"zoom_level": 18446744073709551615, "base_font_size": -1e30}"#,
        )
        .unwrap();
        let loaded = EditorSettings::load_from(&path);
        assert_eq!(loaded.zoom_level, *ZOOM_RANGE.end());
        assert_eq!(loaded.base_font_size, *FONT_SIZE_RANGE.start());
        fs::write(&path, r#"{"zoom_level": 0}"#).unwrap();
        assert_eq!(
            EditorSettings::load_from(&path).zoom_level,
            *ZOOM_RANGE.start()
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
