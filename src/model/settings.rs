use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use crate::theme::ThemeMode;
use super::document::ViewMode;

fn default_word_wrap() -> bool {
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
}

