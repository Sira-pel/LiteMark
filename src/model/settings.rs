use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use crate::services::fs_service::atomic_save;
use crate::theme::ThemeMode;
use super::document::ViewMode;

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
    /// Look for a newer release on GitHub at startup (at most once a day).
    #[serde(default = "default_check_for_updates")]
    pub check_for_updates: bool,
    /// Unix seconds of the last update check.
    #[serde(default)]
    pub last_update_check: Option<u64>,
    /// Release version whose notice the user closed.
    #[serde(default)]
    pub dismissed_update: Option<String>,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            theme: ThemeMode::YaruDark,
            zoom_level: 100,
            view_mode: ViewMode::Wysiwyg,
            show_line_numbers: false,
            show_sidebar: true,
            zen_mode: false,
            show_code_files: true,
            word_wrap: true,
            base_font_size: 17.0,
            content_alignment: ContentAlignment::Centered,
            recent_folders: Vec::new(),
            check_for_updates: true,
            last_update_check: None,
            dismissed_update: None,
        }
    }
}

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
    /// that can't be fully parsed keeps every field that still parses, and
    /// is copied aside first so the next save can't destroy the original.
    fn load_from(path: &Path) -> Self {
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Self::default(),
            Err(err) => {
                eprintln!("litemark: couldn't read {}: {err}", path.display());
                return Self::default();
            }
        };
        let settings = match serde_json::from_str::<Self>(&content) {
            Ok(settings) => settings,
            Err(_) => {
                Self::back_up_unreadable(path);
                Self::salvage(&content)
            }
        };
        settings.clamped()
    }

    /// Pull out-of-range numbers back to what the UI can produce, so a
    /// hand-edited or damaged file can't make text invisible or enormous.
    fn clamped(mut self) -> Self {
        self.zoom_level = self.zoom_level.clamp(*ZOOM_RANGE.start(), *ZOOM_RANGE.end());
        self.base_font_size = if self.base_font_size.is_finite() {
            self.base_font_size
                .clamp(*FONT_SIZE_RANGE.start(), *FONT_SIZE_RANGE.end())
        } else {
            default_base_font_size()
        };
        self.recent_folders.truncate(MAX_RECENT_FOLDERS);
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

    fn back_up_unreadable(path: &Path) {
        let backup = path.with_extension("json.bak");
        match fs::copy(path, &backup) {
            Ok(_) => eprintln!(
                "litemark: {} couldn't be fully read; a copy was kept at {}",
                path.display(),
                backup.display()
            ),
            Err(err) => eprintln!("litemark: couldn't back up {}: {err}", path.display()),
        }
    }

    /// Save to the config file. The error names the file, for showing to the user.
    pub fn save(&self) -> Result<(), String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_alignment_serde() {
        let default_settings = EditorSettings::default();
        assert_eq!(default_settings.content_alignment, ContentAlignment::Centered);

        let json = serde_json::to_string(&default_settings).expect("serialize");
        let parsed: EditorSettings = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.content_alignment, ContentAlignment::Centered);

        let custom = r#"{"content_alignment": "Left"}"#;
        let parsed_custom: EditorSettings = serde_json::from_str(custom).expect("deserialize custom");
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

    fn temp_settings_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("litemark-settings-{tag}-{}", std::process::id()));
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
    fn missing_fields_get_the_real_defaults() {
        let parsed: EditorSettings = serde_json::from_str("{}").expect("deserialize");
        assert_eq!(parsed.zoom_level, 100);
        assert!(parsed.show_sidebar);
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
        assert_eq!(EditorSettings::load_from(&path).zoom_level, *ZOOM_RANGE.start());
        let _ = fs::remove_dir_all(&dir);
    }
}
