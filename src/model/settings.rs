use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use crate::theme::ThemeMode;
use super::document::ViewMode;

fn default_word_wrap() -> bool {
    true
}

fn default_check_for_updates() -> bool {
    true
}

fn default_base_font_size() -> f32 {
    17.0
}

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
    #[serde(default)]
    pub zoom_level: usize,
    #[serde(default)]
    pub view_mode: ViewMode,
    #[serde(default)]
    pub show_line_numbers: bool,
    #[serde(default)]
    pub show_sidebar: bool,
    #[serde(default)]
    pub zen_mode: bool,
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
        if let Some(path) = Self::config_path() {
            if path.exists() {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(settings) = serde_json::from_str(&content) {
                        return settings;
                    }
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        if let Some(path) = Self::config_path() {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Ok(json) = serde_json::to_string_pretty(self) {
                let _ = fs::write(path, json);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

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
}
