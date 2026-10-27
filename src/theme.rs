use gpui::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeMode {
    #[default]
    YaruDark,
    YaruLight,
    System,
}

impl ThemeMode {
    pub fn is_dark(&self) -> bool {
        match self {
            ThemeMode::YaruDark => true,
            ThemeMode::YaruLight => false,
            ThemeMode::System => true,
        }
    }

    pub fn resolved_is_dark(&self, appearance: WindowAppearance) -> bool {
        match self {
            ThemeMode::YaruDark => true,
            ThemeMode::YaruLight => false,
            ThemeMode::System => matches!(appearance, WindowAppearance::Dark | WindowAppearance::VibrantDark),
        }
    }

    #[allow(dead_code)]
    pub fn toggle(&self) -> Self {
        match self {
            ThemeMode::YaruDark => ThemeMode::YaruLight,
            ThemeMode::YaruLight => ThemeMode::YaruDark,
            ThemeMode::System => ThemeMode::YaruDark,
        }
    }
}

pub mod sz {
    use gpui::{px, Pixels};
    pub const TITLE_BAR: Pixels = px(40.);
    pub const TAB_H: Pixels = px(32.);
    pub const PLUS: Pixels = px(28.);
    pub const BTN: Pixels = px(32.);
    pub const TB_TILE_H: Pixels = px(28.); // title-bar tiles, y 6–34
    pub const WC_W: Pixels = px(36.);
    pub const WC_H: Pixels = px(28.);
    pub const TOOLBAR: Pixels = px(40.);
    pub const STATUS: Pixels = px(28.);
    pub const SIDEBAR_W: Pixels = px(304.);
    pub const SIDEBAR_MIN_W: Pixels = px(170.);
    pub const ROW_H: Pixels = px(32.);
    pub const R_SM: Pixels = px(4.);
    pub const R_MD: Pixels = px(6.);
    pub const R_LG: Pixels = px(8.);
    #[allow(dead_code)]
    pub const BODY: f32 = 17.;
    pub const MEASURE_FALLBACK: Pixels = px(672.);
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
    pub selection_bg: Rgba,
    pub close_hover: Rgba,
    pub text_hi: Rgba,
    pub text_strong: Rgba,
    pub text_faint: Rgba,
    pub row_text: Rgba,
    pub row_active: Rgba,
    pub row_hover: Rgba,
    /// Rows in a Shift/Ctrl+A multi-selection: a cool tint that reads apart
    /// from the neutral open-note highlight in both themes.
    pub row_multi: Rgba,
    pub row_multi_border: Rgba,
    pub tile_hover: Rgba,
    pub menu_bg: Rgba,
    pub wc_glyph: Rgba,
    pub h1: Rgba,
    pub h2: Rgba,
    pub code_text: Rgba,
    pub pill_border: Rgba,
    pub pill_text: Rgba,
    pub placeholder: Rgba,
    pub menu_text: Rgba,
    pub menu_shortcut: Rgba,
    pub menu_disabled: Rgba,
    /// Text on a `text_hi`-filled button (the prompt modal's primary choice).
    pub on_primary: Rgba,
    /// Note icon on the open-note row in the sidebar.
    pub row_icon_active: Rgba,
    /// Shortcut text on the highlighted menu row.
    pub menu_shortcut_active: Rgba,
    /// Inset rows inside modals (the shortcuts list).
    pub bg_inset: Rgba,
    /// Fenced code blocks and unchecked task boxes in the editor.
    pub bg_code: Rgba,
    /// Hover fill for header-bar buttons (window controls, new tab, settings).
    pub header_hover: Rgba,
}

impl Theme {
    #[allow(dead_code)]
    pub fn new(mode: ThemeMode) -> Self {
        Self::resolve(mode, WindowAppearance::Dark)
    }

    pub fn resolve(mode: ThemeMode, appearance: WindowAppearance) -> Self {
        if mode.resolved_is_dark(appearance) {
            Self {
                mode,
                bg_editor: rgb(0x1e1e1e),
                bg_header: rgb(0x181818),
                bg_toolbar: rgb(0x242424),
                bg_sidebar: rgb(0x1a1a1a),
                bg_sidebar_header: rgb(0x1a1a1a),
                bg_surface: rgb(0x292929),
                bg_modal: rgb(0x222222),
                bg_modal_header: rgb(0x1c1c1c),
                bg_input: rgb(0x212121),
                bg_hover: rgba(0xffffff0f), // 6%
                bg_active: rgba(0xffffff26),
                border_primary: rgb(0x2e2e2e),
                border_subtle: rgb(0x333333),
                border_surface: rgb(0x383838),
                text_primary: rgb(0xd4d4d4),
                text_muted: rgb(0x9a9a9a),
                selection_bg: rgba(0xffffff2e),
                close_hover: rgb(0xcf2b35),
                text_hi: rgb(0xe6e6e6),
                text_strong: rgb(0xffffff),
                text_faint: rgb(0x7c7c7c),
                row_text: rgb(0xa8a8a8),
                row_active: rgb(0x2c2c2c),
                row_hover: rgb(0x242424),
                row_multi: rgb(0x1f2a36),
                row_multi_border: rgb(0x34506b),
                tile_hover: rgb(0x2a2a2a),
                menu_bg: rgb(0x262626),
                wc_glyph: rgb(0xb0b0b0),
                h1: rgb(0xe8e8e8),
                h2: rgb(0xe2e2e2),
                code_text: rgb(0xdcdcdc),
                pill_border: rgb(0x3a3a3a),
                pill_text: rgb(0xb4b4b4),
                placeholder: rgb(0x8a8a8a),
                menu_text: rgb(0xc8c8c8),
                menu_shortcut: rgb(0x8e8e8e),
                menu_disabled: rgb(0x6e6e6e),
                on_primary: rgb(0x121212),
                row_icon_active: rgb(0xc8c8c8),
                menu_shortcut_active: rgb(0xb4b4b4),
                bg_inset: rgb(0x191919),
                bg_code: rgb(0x292929),
                header_hover: rgb(0x2a2a2a),
            }
        } else {
            Self {
                mode,
                bg_editor: rgb(0xefefef),
                bg_header: rgb(0xdadada),
                bg_toolbar: rgb(0xececec),
                bg_sidebar: rgb(0xe5e5e5),
                bg_sidebar_header: rgb(0xe5e5e5),
                bg_surface: rgb(0xefefef),
                bg_modal: rgb(0xefefef),
                bg_modal_header: rgb(0xe5e5e5),
                bg_input: rgb(0xefefef),
                bg_hover: rgba(0x0000000d),
                bg_active: rgba(0x0000001a),
                border_primary: rgb(0xcacaca),
                border_subtle: rgb(0xd8d8d8),
                border_surface: rgb(0xcacaca),
                text_primary: rgb(0x2e2e2e),
                text_muted: rgb(0x5e5e5e),
                selection_bg: rgba(0x0000001a),
                close_hover: rgb(0xcf2b35),
                text_hi: rgb(0x262626),
                text_strong: rgb(0x2b2b2b),
                text_faint: rgb(0x858585),
                row_text: rgb(0x3d3d3d),
                row_active: rgb(0xd6d6d6),
                row_hover: rgb(0xdedede),
                row_multi: rgb(0xe1edf8),
                row_multi_border: rgb(0xa9c6e4),
                tile_hover: rgb(0xd8d8d8),
                menu_bg: rgb(0xefefef),
                wc_glyph: rgb(0x4a4a4a),
                h1: rgb(0x1f1f1f),
                h2: rgb(0x262626),
                code_text: rgb(0x2e2e2e),
                pill_border: rgb(0xcccccc),
                pill_text: rgb(0x555555),
                placeholder: rgb(0x888888),
                menu_text: rgb(0x383838),
                menu_shortcut: rgb(0x767676),
                menu_disabled: rgb(0xa8a8a8),
                on_primary: rgb(0xf4f4f4),
                row_icon_active: rgb(0x383838),
                menu_shortcut_active: rgb(0x555555),
                bg_inset: rgb(0xe6e6e6),
                bg_code: rgb(0xe4e4e4),
                header_hover: rgb(0xc8c8c8),
            }
        }
    }

    pub const FONT_PROSE: &'static str = "Ubuntu";
    pub const FONT_MONO: &'static str = "Ubuntu Mono";
}

/// Sidebar width for a window: the full width when there is room, shrinking
/// on narrow windows so the editor keeps at least ~55% of the space.
pub fn sidebar_width(window_width: Pixels) -> Pixels {
    (window_width * 0.45).clamp(sz::SIDEBAR_MIN_W, sz::SIDEBAR_W)
}

#[cfg(test)]
mod sidebar_width_tests {
    use super::{sidebar_width, sz};
    use gpui::px;

    #[test]
    fn sidebar_width_shrinks_with_window_within_bounds() {
        assert_eq!(sidebar_width(px(1600.0)), sz::SIDEBAR_W);
        assert_eq!(sidebar_width(px(300.0)), sz::SIDEBAR_MIN_W);
        let mid = sidebar_width(px(480.0));
        assert!(mid > sz::SIDEBAR_MIN_W && mid < sz::SIDEBAR_W);
    }
}
