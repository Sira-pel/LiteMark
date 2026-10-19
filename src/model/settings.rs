use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use crate::theme::ThemeMode;
use super::document::ViewMode;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorSettings {
    pub theme: ThemeMode,
    pub zoom_level: usize,
    pub view_mode: ViewMode,
    pub show_line_numbers: bool,
    pub show_sidebar: bool,
    pub zen_mode: bool,
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
        }
    }
}

impl EditorSettings {
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
