use gpui::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeMode {
    #[default]
    YaruDark,
    YaruLight,
}

impl ThemeMode {
    pub fn is_dark(&self) -> bool {
        matches!(self, ThemeMode::YaruDark)
    }

    pub fn toggle(&self) -> Self {
        match self {
            ThemeMode::YaruDark => ThemeMode::YaruLight,
            ThemeMode::YaruLight => ThemeMode::YaruDark,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub mode: ThemeMode,
    pub bg_editor: Rgba,
    pub bg_header: Rgba,
    pub bg_toolbar: Rgba,
    pub bg_sidebar: Rgba,
    pub bg_sidebar_header: Rgba,
    pub bg_surface: Rgba,
    pub bg_modal: Rgba,
    pub bg_modal_header: Rgba,
    pub bg_input: Rgba,
    pub bg_hover: Rgba,
    pub bg_active: Rgba,
    pub border_primary: Rgba,
    pub border_subtle: Rgba,
    pub border_surface: Rgba,
    pub text_primary: Rgba,
    pub text_muted: Rgba,
    pub accent_ubuntu: Rgba,
    pub accent_emerald: Rgba,
    pub accent_danger: Rgba,
    pub selection_bg: Rgba,
}

impl Theme {
    pub fn new(mode: ThemeMode) -> Self {
        if mode.is_dark() {
            Self {
                mode,
                bg_editor: rgb(0x1e1e1e),
                bg_header: rgb(0x1b1b1b),
                bg_toolbar: rgb(0x242424),
                bg_sidebar: rgb(0x202020),
                bg_sidebar_header: rgb(0x1a1a1a),
                bg_surface: rgb(0x292929),
                bg_modal: rgb(0x222222),
                bg_modal_header: rgb(0x1c1c1c),
                bg_input: rgb(0x181818),
                bg_hover: rgba(0xffffff1a), // white/10
                bg_active: rgba(0xffffff26), // white/15
                border_primary: rgb(0x383838),
                border_subtle: rgb(0x333333),
                border_surface: rgb(0x444444),
                text_primary: rgb(0xdfdbd2),
                text_muted: rgb(0x8f8b82),
                accent_ubuntu: rgb(0xe95420),
                accent_emerald: rgb(0x10b981),
                accent_danger: rgb(0xef4444),
                selection_bg: rgba(0x8f8b8259), // rgba(143, 139, 130, 0.35)
            }
        } else {
            Self {
                mode,
                bg_editor: rgb(0xffffff),
                bg_header: rgb(0xe0e0e0),
                bg_toolbar: rgb(0xf4f4f4),
                bg_sidebar: rgb(0xf6f6f6),
                bg_sidebar_header: rgb(0xececec),
                bg_surface: rgb(0xffffff),
                bg_modal: rgb(0xffffff),
                bg_modal_header: rgb(0xf5f5f5),
                bg_input: rgb(0xffffff),
                bg_hover: rgba(0x0000000d), // black/5
                bg_active: rgba(0x0000001a), // black/10
                border_primary: rgb(0xd4d4d4),
                border_subtle: rgb(0xe5e5e5),
                border_surface: rgb(0xd4d4d4),
                text_primary: rgb(0x171717),
                text_muted: rgb(0x525252),
                accent_ubuntu: rgb(0xe95420),
                accent_emerald: rgb(0x10b981),
                accent_danger: rgb(0xdc2626),
                selection_bg: rgba(0x8f8b8259),
            }
        }
    }

    pub const FONT_PROSE: &'static str = "Ubuntu";
    pub const FONT_MONO: &'static str = "Ubuntu Mono";
}
